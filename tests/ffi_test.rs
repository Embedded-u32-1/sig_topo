//! M31: C-ABI end-to-end integration tests.
//!
//! Exercise the `extern "C"` surface exactly as a foreign caller would: through
//! opaque pointers and JSON strings, no access to crate internals. This pins
//! down the "the FFI is a faithful, runnable wrapper over the engine" contract
//! from the outside.

use signal_topology::ffi;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::ptr;

/// The order_approval topology -- single signal "order", draft -> shipped.
const ORDER_APPROVAL: &str = include_str!("../examples/order_approval.json");

/// Safety helper: read a returned `char *` into a Rust `String` and free it.
/// Returns `None` if the pointer is null.
unsafe fn read_and_free(ptr: *mut c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    let s = CStr::from_ptr(ptr).to_str().ok()?.to_owned();
    ffi::engine_free_str(ptr);
    Some(s)
}

/// Build an engine over ORDER_APPROVAL and register every referenced action as
/// a no-op -- engine_new does this automatically, so the returned engine is
/// immediately runnable.
unsafe fn new_engine() -> *mut signal_topology::TopologyEngine {
    let json = CString::new(ORDER_APPROVAL).unwrap();
    let engine = ffi::engine_new(json.as_ptr());
    assert!(
        !engine.is_null(),
        "engine_new should succeed for a valid topology"
    );
    engine
}

#[test]
fn ffi_engine_round_trip_reaches_shipped() {
    unsafe {
        let engine = new_engine();

        let submit = CString::new(r#"{"signal_id":"order","event":"submit"}"#).unwrap();
        let r = read_and_free(ffi::engine_send_event(engine, submit.as_ptr())).unwrap();
        assert!(r.contains(r#""to":"submitted""#), "submit result: {}", r);

        let approve = CString::new(
            r#"{"signal_id":"order","event":"approve","payload":{"amount":5000}}"#,
        )
        .unwrap();
        let r = read_and_free(ffi::engine_send_event(engine, approve.as_ptr())).unwrap();
        assert!(r.contains(r#""to":"approved""#), "approve result: {}", r);

        let ship = CString::new(r#"{"signal_id":"order","event":"ship"}"#).unwrap();
        let r = read_and_free(ffi::engine_send_event(engine, ship.as_ptr())).unwrap();
        assert!(r.contains(r#""to":"shipped""#), "ship result: {}", r);

        let id = CString::new("order").unwrap();
        let state = read_and_free(ffi::engine_get_state(engine, id.as_ptr())).unwrap();
        assert!(
            state.contains(r#""shipped""#),
            "final state should be shipped, got: {}",
            state
        );

        ffi::engine_free(engine);
    }
}

#[test]
fn ffi_engine_get_traces_returns_json_array() {
    unsafe {
        let engine = new_engine();

        let submit = CString::new(r#"{"signal_id":"order","event":"submit"}"#).unwrap();
        let _ = read_and_free(ffi::engine_send_event(engine, submit.as_ptr()));

        let traces = read_and_free(ffi::engine_get_traces(engine)).unwrap();
        let parsed: serde_json::Value =
            serde_json::from_str(&traces).expect("traces should be valid JSON");
        assert!(
            parsed.is_array(),
            "traces should be a JSON array, got: {}",
            traces
        );
        let arr = parsed.as_array().unwrap();
        assert!(
            !arr.is_empty(),
            "a submit transition should produce trace events"
        );
        for e in arr {
            assert!(
                e.get("kind").is_some(),
                "every trace event should carry a kind field, got: {}",
                e
            );
        }

        ffi::engine_free(engine);
    }
}

#[test]
fn ffi_engine_new_returns_null_on_bad_json() {
    unsafe {
        let bad = CString::new("this is not json").unwrap();
        let engine = ffi::engine_new(bad.as_ptr());
        assert!(
            engine.is_null(),
            "engine_new must return null on invalid JSON"
        );
    }
}

#[test]
fn ffi_engine_send_event_returns_json_error_on_unknown_event() {
    unsafe {
        let engine = new_engine();
        // "order" has no transition for "defenestrate" from any state.
        let evt = CString::new(r#"{"signal_id":"order","event":"defenestrate"}"#).unwrap();
        let r = read_and_free(ffi::engine_send_event(engine, evt.as_ptr())).unwrap();
        assert!(
            r.contains("error"),
            "an unknown event should yield an error JSON, got: {}",
            r
        );
        ffi::engine_free(engine);
    }
}

#[test]
fn ffi_engine_get_state_returns_error_on_unknown_signal() {
    unsafe {
        let engine = new_engine();
        let id = CString::new("nonexistent_signal").unwrap();
        let r = read_and_free(ffi::engine_get_state(engine, id.as_ptr())).unwrap();
        assert!(
            r.contains("error"),
            "an unknown signal id should yield an error JSON, got: {}",
            r
        );
        ffi::engine_free(engine);
    }
}

#[test]
fn ffi_engine_guard_block_returns_error_json() {
    unsafe {
        let engine = new_engine();
        // Reach "submitted" first.
        let submit = CString::new(r#"{"signal_id":"order","event":"submit"}"#).unwrap();
        let _ = read_and_free(ffi::engine_send_event(engine, submit.as_ptr()));

        // amount == 0 violates `payload.amount > 0` -> guard blocks the approve.
        let approve =
            CString::new(r#"{"signal_id":"order","event":"approve","payload":{"amount":0}}"#)
                .unwrap();
        let r = read_and_free(ffi::engine_send_event(engine, approve.as_ptr())).unwrap();
        assert!(
            r.contains("error"),
            "a guard-blocked transition should yield an error JSON, got: {}",
            r
        );

        // And the state must be unchanged.
        let id = CString::new("order").unwrap();
        let state = read_and_free(ffi::engine_get_state(engine, id.as_ptr())).unwrap();
        assert!(
            state.contains(r#""submitted""#),
            "guard block should leave state as submitted, got: {}",
            state
        );

        ffi::engine_free(engine);
    }
}

#[test]
fn ffi_null_engine_pointers_return_errors() {
    unsafe {
        let evt = CString::new(r#"{"signal_id":"order","event":"submit"}"#).unwrap();
        let r =
            read_and_free(ffi::engine_send_event(ptr::null_mut(), evt.as_ptr())).unwrap();
        assert!(
            r.contains("error"),
            "null engine should error on send_event, got: {}",
            r
        );

        let id = CString::new("order").unwrap();
        let r = read_and_free(ffi::engine_get_state(ptr::null_mut(), id.as_ptr())).unwrap();
        assert!(
            r.contains("error"),
            "null engine should error on get_state, got: {}",
            r
        );

        let r = read_and_free(ffi::engine_get_traces(ptr::null_mut())).unwrap();
        assert!(
            r.contains("error"),
            "null engine should error on get_traces, got: {}",
            r
        );
    }
}
