//! M31: C-ABI (FFI) shared-library surface.
//!
//! A thin, unsafe wrapper that lets C / Python / Node (via FFI) drive the
//! engine without writing Rust: load a JSON topology, send events, read state
//! and traces. The engine layer (`crate::engine`) is untouched -- every function
//! here is a pointer-and-string translation over the existing safe API.
//!
//! ## Memory ownership
//!
//! - Every `*mut c_char` returned by `engine_send_event` / `engine_get_state` /
//!   `engine_get_traces` is heap-allocated by Rust and **must** be released by
//!   the caller with `engine_free_str`. Failing to do so leaks memory.
//! - The `*mut TopologyEngine` from `engine_new` **must** be released with
//!   `engine_free`. Passing it to any function after `engine_free` is undefined
//!   behaviour.
//! - Input pointers (`topology_json`, `event_json`, `signal_id`) are borrowed
//!   only for the duration of the call; the engine never takes ownership of
//!   them.

use crate::engine::TopologyEngine;
use crate::trace::TraceEvent;
use serde::Deserialize;
use serde_json::Value;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::ptr;

/// The event payload sent to `engine_send_event`, deserialized from the
/// `event_json` argument. `payload` is optional and may be any JSON value.
#[derive(Deserialize)]
struct EventInput {
    signal_id: String,
    event: String,
    #[serde(default)]
    payload: Option<Value>,
}

/// Read a borrowed C string into a Rust `String`, or `None` if the pointer is
/// null or the bytes are not valid UTF-8.
unsafe fn c_str_to_string(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    CStr::from_ptr(ptr).to_str().ok().map(str::to_owned)
}

/// Leak a Rust string into a heap-allocated null-terminated C string. Returns
/// null if the string somehow contains an embedded null byte (impossible for
/// serde_json output, but handled defensively to avoid panicking across FFI).
fn leak_string(s: String) -> *mut c_char {
    match CString::new(s) {
        Ok(cs) => cs.into_raw(),
        Err(_) => ptr::null_mut(),
    }
}

/// Build a `{"error": <msg>}` JSON string and leak it for the caller.
fn err_json(msg: &str) -> *mut c_char {
    leak_string(serde_json::json!({"error": msg}).to_string())
}

/// Serialize a `serde_json::Value` to a leaked C string.
fn ok_json(value: &Value) -> *mut c_char {
    leak_string(value.to_string())
}

/// Collect every action id referenced by the engine's transitions, deduped and
/// sorted for deterministic registration.
fn collect_action_ids(engine: &TopologyEngine) -> Vec<String> {
    let mut ids: Vec<String> = engine
        .transitions
        .iter()
        .flat_map(|t| t.actions.all_actions().into_iter().cloned())
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

/// Hand-write a `TraceEvent` to a JSON value. `TraceEvent` deliberately has no
/// `serde::Serialize` derive (see `src/trace.rs`), so the FFI layer builds its
/// own representation rather than touching the existing type.
fn trace_to_value(e: &TraceEvent) -> Value {
    let mut v = serde_json::json!({
        "signal_id": e.signal_id(),
        "timestamp_ms": e.timestamp_ms(),
    });
    let obj = v.as_object_mut().expect("json! object is always an Object");
    match e {
        TraceEvent::EventReceived {
            event, payload, ..
        } => {
            obj.insert("kind".into(), Value::from("EventReceived"));
            obj.insert("event".into(), Value::from(event.clone()));
            obj.insert(
                "payload".into(),
                payload.clone().map(Value::from).unwrap_or(Value::Null),
            );
        }
        TraceEvent::ActionStarted { action_id, .. } => {
            obj.insert("kind".into(), Value::from("ActionStarted"));
            obj.insert("action_id".into(), Value::from(action_id.clone()));
        }
        TraceEvent::ActionSucceeded { action_id, .. } => {
            obj.insert("kind".into(), Value::from("ActionSucceeded"));
            obj.insert("action_id".into(), Value::from(action_id.clone()));
        }
        TraceEvent::ActionFailed {
            action_id, error, ..
        } => {
            obj.insert("kind".into(), Value::from("ActionFailed"));
            obj.insert("action_id".into(), Value::from(action_id.clone()));
            obj.insert("error".into(), Value::from(error.clone()));
        }
        TraceEvent::StateChanged { from, to, .. } => {
            obj.insert("kind".into(), Value::from("StateChanged"));
            obj.insert("from".into(), Value::from(from.clone()));
            obj.insert("to".into(), Value::from(to.clone()));
        }
        TraceEvent::Rollbacked { from, to, .. } => {
            obj.insert("kind".into(), Value::from("Rollbacked"));
            obj.insert("from".into(), Value::from(from.clone()));
            obj.insert("to".into(), Value::from(to.clone()));
        }
        TraceEvent::ReactionGuardEvaluated {
            reaction_from_signal,
            reaction_from_state,
            reaction_to_signal,
            reaction_event,
            guard,
            result,
            ..
        } => {
            obj.insert("kind".into(), Value::from("ReactionGuardEvaluated"));
            obj.insert("from_signal".into(), Value::from(reaction_from_signal.clone()));
            obj.insert("from_state".into(), Value::from(reaction_from_state.clone()));
            obj.insert("to_signal".into(), Value::from(reaction_to_signal.clone()));
            obj.insert("event".into(), Value::from(reaction_event.clone()));
            obj.insert("guard".into(), Value::from(guard.clone()));
            obj.insert("result".into(), Value::from(result.clone()));
        }
    }
    v
}

/// Create an engine from a JSON topology, returning an opaque pointer.
///
/// On success the caller owns the returned pointer and must free it with
/// `engine_free`. On a parse or validation error, returns null -- the caller
/// must check for this.
///
/// Every action referenced by the topology is automatically registered as a
/// no-op, so the engine is fully runnable across the language boundary without
/// the caller needing to supply action implementations. (The FFI surface has no
/// way to register Rust closures, so this is the only way to drive topologies
/// that bind actions.)
///
/// # Safety
///
/// `topology_json` must be a valid, null-terminated C string (or null, in
/// which case this returns null). It is only borrowed for the duration of the
/// call.
#[no_mangle]
pub unsafe extern "C" fn engine_new(topology_json: *const c_char) -> *mut TopologyEngine {
    let json = match unsafe { c_str_to_string(topology_json) } {
        Some(s) => s,
        None => return ptr::null_mut(),
    };

    let mut engine = match TopologyEngine::from_json(&json) {
        Ok(e) => e,
        Err(_) => return ptr::null_mut(),
    };

    for action_id in collect_action_ids(&engine) {
        let id = action_id.clone();
        engine.register_action(&id, |_| Ok(()));
    }

    Box::into_raw(Box::new(engine))
}

/// Send an event to a signal. `event_json` must be a JSON object with
/// `signal_id` and `event` string fields and an optional `payload`:
///
/// ```json
/// {"signal_id": "order", "event": "submit"}
/// {"signal_id": "order", "event": "approve", "payload": {"amount": 5000}}
/// ```
///
/// Returns a JSON string the caller must free with `engine_free_str`:
/// `{"ok": {"signal_id":..., "from":..., "to":..., "executed_actions":[...]}}`
/// on success, or `{"error": "..."}` on failure (unknown signal / event,
/// guard block, action error, ...).
///
/// # Safety
///
/// `engine` must be a valid pointer returned by `engine_new` that has not yet
/// been freed. `event_json` must be a valid, null-terminated C string. Both are
/// only borrowed for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn engine_send_event(
    engine: *mut TopologyEngine,
    event_json: *const c_char,
) -> *mut c_char {
    let engine = match unsafe { engine.as_mut() } {
        Some(e) => e,
        None => return err_json("null engine pointer"),
    };

    let json = match unsafe { c_str_to_string(event_json) } {
        Some(s) => s,
        None => return err_json("null event_json pointer"),
    };

    let event: EventInput = match serde_json::from_str(&json) {
        Ok(e) => e,
        Err(e) => return err_json(&format!("invalid event JSON: {}", e)),
    };

    match engine.send_event(&event.signal_id, &event.event, event.payload) {
        Ok(result) => {
            let value = serde_json::json!({"ok": {
                "signal_id": result.signal_id,
                "from": result.from,
                "to": result.to,
                "executed_actions": result.executed_actions,
            }});
            ok_json(&value)
        }
        Err(e) => err_json(&e.to_string()),
    }
}

/// Query the current state of a signal. Returns a JSON string the caller must
/// free with `engine_free_str`: `{"state": "..."}` on success or
/// `{"error": "..."}` if the signal is unknown.
///
/// # Safety
///
/// `engine` must be a valid pointer returned by `engine_new` that has not yet
/// been freed. `signal_id` must be a valid, null-terminated C string. Both are
/// only borrowed for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn engine_get_state(
    engine: *mut TopologyEngine,
    signal_id: *const c_char,
) -> *mut c_char {
    let engine = match unsafe { engine.as_ref() } {
        Some(e) => e,
        None => return err_json("null engine pointer"),
    };

    let id = match unsafe { c_str_to_string(signal_id) } {
        Some(s) => s,
        None => return err_json("null signal_id pointer"),
    };

    match engine.get_state(&id) {
        Ok(state) => ok_json(&serde_json::json!({"state": state})),
        Err(e) => err_json(&e.to_string()),
    }
}

/// Return every recorded trace event as a JSON array string the caller must
/// free with `engine_free_str`. Each element carries `signal_id`,
/// `timestamp_ms`, a `kind` discriminator, and kind-specific fields.
///
/// # Safety
///
/// `engine` must be a valid pointer returned by `engine_new` that has not yet
/// been freed. It is only borrowed for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn engine_get_traces(engine: *mut TopologyEngine) -> *mut c_char {
    let engine = match unsafe { engine.as_ref() } {
        Some(e) => e,
        None => return err_json("null engine pointer"),
    };

    let events: Vec<Value> = engine.traces().iter().map(trace_to_value).collect();
    ok_json(&Value::from(events))
}

/// Free an engine created by `engine_new`. Passing a null pointer is a no-op.
/// Passing a pointer that was already freed or that was not returned by
/// `engine_new` is undefined behaviour.
///
/// # Safety
///
/// `engine` must be either null or a valid pointer returned by `engine_new`
/// that has not yet been freed (or a prior `engine_free` call on it).
#[no_mangle]
pub unsafe extern "C" fn engine_free(engine: *mut TopologyEngine) {
    if engine.is_null() {
        return;
    }
    unsafe {
        drop(Box::from_raw(engine));
    }
}

/// Free a string returned by `engine_send_event` / `engine_get_state` /
/// `engine_get_traces`. Passing a null pointer is a no-op.
///
/// # Safety
///
/// `s` must be either null or a valid pointer returned by one of the functions
/// above that has not yet been freed.
#[no_mangle]
pub unsafe extern "C" fn engine_free_str(s: *mut c_char) {
    if s.is_null() {
        return;
    }
    unsafe {
        drop(CString::from_raw(s));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::raw::c_char;

    const ORDER_APPROVAL: &str = include_str!("../examples/order_approval.json");

    /// Safety helper: read a returned C string into a Rust `String` and free
    /// it. Returns `None` if the pointer is null.
    unsafe fn read_and_free(ptr: *mut c_char) -> Option<String> {
        if ptr.is_null() {
            return None;
        }
        let s = CStr::from_ptr(ptr).to_str().ok()?.to_owned();
        engine_free_str(ptr);
        Some(s)
    }

    unsafe fn new_engine() -> *mut TopologyEngine {
        let json = CString::new(ORDER_APPROVAL).unwrap();
        let engine = engine_new(json.as_ptr());
        assert!(!engine.is_null(), "engine_new should succeed for valid topology");
        engine
    }

    #[test]
    fn unit_engine_round_trip_reaches_shipped() {
        unsafe {
            let engine = new_engine();

            let submit = CString::new(r#"{"signal_id":"order","event":"submit"}"#).unwrap();
            let r = read_and_free(engine_send_event(engine, submit.as_ptr())).unwrap();
            assert!(r.contains(r#""to":"submitted""#), "submit result: {}", r);

            let approve = CString::new(
                r#"{"signal_id":"order","event":"approve","payload":{"amount":5000}}"#,
            )
            .unwrap();
            let r = read_and_free(engine_send_event(engine, approve.as_ptr())).unwrap();
            assert!(r.contains(r#""to":"approved""#), "approve result: {}", r);

            let ship = CString::new(r#"{"signal_id":"order","event":"ship"}"#).unwrap();
            let r = read_and_free(engine_send_event(engine, ship.as_ptr())).unwrap();
            assert!(r.contains(r#""to":"shipped""#), "ship result: {}", r);

            let id = CString::new("order").unwrap();
            let state = read_and_free(engine_get_state(engine, id.as_ptr())).unwrap();
            assert!(
                state.contains(r#""shipped""#),
                "final state should be shipped, got: {}",
                state
            );

            engine_free(engine);
        }
    }

    #[test]
    fn unit_get_traces_returns_json_array() {
        unsafe {
            let engine = new_engine();
            let submit = CString::new(r#"{"signal_id":"order","event":"submit"}"#).unwrap();
            let _ = read_and_free(engine_send_event(engine, submit.as_ptr()));

            let traces = read_and_free(engine_get_traces(engine)).unwrap();
            let parsed: Value = serde_json::from_str(&traces).expect("traces should be valid JSON");
            assert!(parsed.is_array(), "traces should be a JSON array");
            assert!(!parsed.as_array().unwrap().is_empty(), "submit should produce trace events");

            let first = &parsed.as_array().unwrap()[0];
            assert!(
                first.get("kind").is_some(),
                "trace events should carry a kind field"
            );

            engine_free(engine);
        }
    }

    #[test]
    fn unit_new_returns_null_on_bad_json() {
        unsafe {
            let bad = CString::new("this is not json").unwrap();
            let engine = engine_new(bad.as_ptr());
            assert!(
                engine.is_null(),
                "engine_new should return null on invalid JSON"
            );
        }
    }

    #[test]
    fn unit_send_event_returns_error_on_unknown_event() {
        unsafe {
            let engine = new_engine();
            let unknown = CString::new(r#"{"signal_id":"order","event":"does_not_exist"}"#).unwrap();
            let r = read_and_free(engine_send_event(engine, unknown.as_ptr())).unwrap();
            assert!(
                r.contains("error"),
                "unknown event should yield an error JSON, got: {}",
                r
            );
            engine_free(engine);
        }
    }

    #[test]
    fn unit_null_pointer_inputs_return_errors() {
        unsafe {
            let null_c: *const c_char = ptr::null();
            // null engine + null event_json
            let r = read_and_free(engine_send_event(ptr::null_mut(), null_c)).unwrap();
            assert!(r.contains("error"), "null engine should error, got: {}", r);
            // null event_json with a valid engine
            let engine = new_engine();
            let r = read_and_free(engine_send_event(engine, null_c)).unwrap();
            assert!(
                r.contains("error"),
                "null event_json should error, got: {}",
                r
            );
            // null signal_id
            let r = read_and_free(engine_get_state(engine, null_c)).unwrap();
            assert!(
                r.contains("error"),
                "null signal_id should error, got: {}",
                r
            );
            // null engine for get_state / get_traces
            let r = read_and_free(engine_get_state(ptr::null_mut(), null_c)).unwrap();
            assert!(r.contains("error"), "null engine should error, got: {}", r);
            let r = read_and_free(engine_get_traces(ptr::null_mut())).unwrap();
            assert!(r.contains("error"), "null engine should error, got: {}", r);

            engine_free(engine);
        }
    }
}
