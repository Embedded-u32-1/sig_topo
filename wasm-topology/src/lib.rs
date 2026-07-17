//! M35: wasm-bindgen surface exposing `signal-topology`'s `TopologyEngine` to
//! the browser / Node / extension hosts.
//!
//! The engine layer (`signal_topology`) is untouched -- every function here is a
//! thin translation over the existing safe API, mirroring the approach of the
//! C-ABI layer (`signal_topology::ffi`). Two things make this layer necessary:
//!
//! 1. `TopologyEngine::transitions` is `pub(crate)`, so a language-binding crate
//!    that lives outside `signal_topology` cannot scan for action ids on its own.
//!    `TopologyEngine::action_ids()` (added in M35) exposes just enough to
//!    pre-register every action as a no-op, the same trick `ffi.rs` uses.
//! 2. The engine's public types (`TransitionResult`, `TraceEvent`) have no
//!    `serde::Serialize` derive, so the result / trace JSON is hand-rolled here
//!    to match the representation already produced by the FFI surface.

use signal_topology::TopologyEngine;
use serde_json::Value;
use wasm_bindgen::prelude::*;

/// A `TopologyEngine` wrapped for consumption from JavaScript.
///
/// Construct with `new(topology_json)`, then drive it with `send_event` /
/// `get_state` / `get_traces` / `snapshot_dot`. The wrapper owns the engine; let
/// it drop when you are done (the JS GC / wasm bindgen teardown handle that).
#[wasm_bindgen]
pub struct WasmEngine {
    inner: TopologyEngine,
}

// ---------------------------------------------------------------------------
// Pure helpers (no wasm-bindgen). Kept separate so the whole engine-driving
// logic is unit-testable on the host with a plain `#[test]` -- no browser or
// headless wasm required. (The non-wasm host cannot construct or read a
// `JsValue`; that only works on wasm32, so the actual wasm methods are just
// thin one-liners over `send_event_impl` below.)
// ---------------------------------------------------------------------------

/// Serialize a successful `TransitionResult` to JSON, matching the layout of
/// `signal_topology::ffi::engine_send_event`.
fn ok_json(result: &signal_topology::TransitionResult) -> String {
    serde_json::json!({"ok": {
        "signal_id": result.signal_id,
        "from": result.from,
        "to": result.to,
        "executed_actions": result.executed_actions,
    }})
    .to_string()
}

/// Parse the `event_json` argument into `(signal_id, event, payload)`.
///
/// `payload` is `None` when absent or explicitly `null`; otherwise it is
/// forwarded verbatim (any JSON value) so transition guards can read it.
fn parse_event_json(event_json: &str) -> Result<(String, String, Option<Value>), String> {
    let parsed: Value = serde_json::from_str(event_json)
        .map_err(|e| format!("invalid event JSON: {e}"))?;
    let signal_id = parsed
        .get("signal_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let event = parsed
        .get("event")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let payload = match parsed.get("payload") {
        Some(v) if !v.is_null() => Some(v.clone()),
        _ => None,
    };
    Ok((signal_id, event, payload))
}

/// Register a no-op for every action id referenced by the engine's topology,
/// so action-bearing topologies run without the host supplying action code.
fn register_all_noops(engine: &mut TopologyEngine) {
    for id in engine.action_ids() {
        let id = id.clone();
        engine.register_action(&id, |_| Ok(()));
    }
}

/// The engine-driving core of `send_event`, returning a plain `Result<String,
/// String>`. Fully host-testable because it never touches a `JsValue`; the
/// `#[wasm_bindgen] send_event` is a one-line wrapper that only converts the
/// error string into a thrown exception.
fn send_event_impl(engine: &mut TopologyEngine, event_json: &str) -> Result<String, String> {
    let (signal_id, event, payload) = parse_event_json(event_json)?;
    match engine.send_event(&signal_id, &event, payload) {
        Ok(r) => Ok(ok_json(&r)),
        Err(e) => Err(e.to_string()),
    }
}

// ---------------------------------------------------------------------------
// wasm-bindgen surface. The non-wasm host can build the crate but cannot run
// these methods (see `describe` panics above), so they are exercised by the
// wasm32-targeted `#[wasm_bindgen_test]` in the `wasm_tests` module.
// ---------------------------------------------------------------------------

#[wasm_bindgen]
impl WasmEngine {
    /// Create an engine from a JSON topology.
    ///
    /// Returns the engine on success. On a parse or validation error, throws a
    /// JS exception carrying the engine error message (wasm-bindgen maps the
    /// `Err(JsValue)` into a thrown exception).
    ///
    /// Every action referenced by the topology is auto-registered as a no-op,
    /// so the engine is fully runnable from JS with no further setup -- exactly
    /// like `ffi::engine_new`.
    #[wasm_bindgen(constructor)]
    pub fn new(topology_json: &str) -> Result<WasmEngine, JsValue> {
        let mut inner = TopologyEngine::from_json(topology_json)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        register_all_noops(&mut inner);
        Ok(WasmEngine { inner })
    }

    /// Send an event to a signal. `event_json` must be a JSON object with
    /// `signal_id` (string) and `event` (string) and an optional `payload`:
    ///
    /// ```json
    /// {"signal_id": "order", "event": "submit"}
    /// {"signal_id": "order", "event": "approve", "payload": {"amount": 5000}}
    /// ```
    ///
    /// Returns a JSON string on success, throws a JS exception on any failure
    /// (unknown signal / event, guard block, action error):
    ///
    /// ```json
    /// {"ok": {"signal_id":"order","from":"draft","to":"submitted","executed_actions":[...]}}
    /// ```
    pub fn send_event(&mut self, event_json: &str) -> Result<String, JsValue> {
        send_event_impl(&mut self.inner, event_json).map_err(|e| JsValue::from_str(&e))
    }

    /// Return the current state of `signal_id`, or `null` if the signal is
    /// unknown (`Option<String>` maps to JS `string | null`).
    pub fn get_state(&self, signal_id: &str) -> Option<String> {
        self.inner.get_state(signal_id).ok().map(String::from)
    }

    /// Return every recorded trace event as a JSON array string. Each element
    /// carries `signal_id`, `timestamp_ms`, a `kind` discriminator, and
    /// kind-specific fields -- the same shape as `ffi::engine_get_traces`.
    pub fn get_traces(&self) -> String {
        self.inner.traces_json()
    }

    /// Render the topology as Graphviz DOT with each signal's *current* state
    /// highlighted lightgreen (see `signal_topology::snapshot_dot`).
    pub fn snapshot_dot(&self) -> String {
        self.inner.snapshot_dot()
    }
}

// ---------------------------------------------------------------------------
// Host-runnable unit tests (no wasm needed).
//
// These exercise the same engine-driving logic the wasm methods use (via the
// pure `send_event_impl` helper), so the project's `cargo test` stays green
// even without a browser / wasm-pack.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const ORDER_APPROVAL: &str = include_str!("../../examples/order_approval.json");

    fn engine() -> TopologyEngine {
        let mut e = TopologyEngine::from_json(ORDER_APPROVAL).expect("fixture parses");
        register_all_noops(&mut e);
        e
    }

    #[test]
    fn host_engine_reaches_shipped() {
        let mut e = engine();
        let submit = send_event_impl(&mut e, r#"{"signal_id":"order","event":"submit"}"#).unwrap();
        assert_eq!(
            submit,
            serde_json::json!({"ok":{"signal_id":"order","from":"draft","to":"submitted",
                "executed_actions":["log_draft_exit","validate_order_payload","notify_submitted"]}})
                .to_string()
        );
        let approve = send_event_impl(
            &mut e,
            r#"{"signal_id":"order","event":"approve","payload":{"amount":5000}}"#,
        )
        .unwrap();
        assert!(approve.contains(r#""to":"approved""#), "approve: {approve}");
        let ship = send_event_impl(&mut e, r#"{"signal_id":"order","event":"ship"}"#).unwrap();
        assert!(ship.contains(r#""to":"shipped""#), "ship: {ship}");
        assert_eq!(e.get_state("order").unwrap(), "shipped");
    }

    #[test]
    fn host_snapshot_dot_includes_highlight() {
        let mut e = engine();
        // Advance so the current state is non-initial; the renderer highlights
        // the "live" state lightgreen regardless, so the marker must appear.
        send_event_impl(&mut e, r#"{"signal_id":"order","event":"submit"}"#).unwrap();
        let dot = e.snapshot_dot();
        assert!(
            dot.contains("lightgreen"),
            "snapshot_dot should highlight the live state lightgreen:\n{dot}"
        );
        assert!(dot.contains("submitted"), "dot should mention submitted:\n{dot}");
    }

    #[test]
    fn host_get_traces_is_json_array() {
        let mut e = engine();
        send_event_impl(&mut e, r#"{"signal_id":"order","event":"submit"}"#).unwrap();
        let json = e.traces_json();
        let parsed: Value = serde_json::from_str(&json).expect("traces must be valid JSON");
        assert!(parsed.is_array(), "traces should be a JSON array");
        assert!(!parsed.as_array().unwrap().is_empty(), "submit emits traces");
        assert_eq!(parsed[0]["kind"], "EventReceived");
    }

    #[test]
    fn host_send_event_rejects_bad_json() {
        let mut e = engine();
        let err = send_event_impl(&mut e, "this is not json").unwrap_err();
        assert!(
            err.contains("invalid event JSON"),
            "expected an invalid-JSON message, got: {err}"
        );
    }

    #[test]
    fn host_send_event_unknown_signal_errors() {
        let mut e = engine();
        let err = send_event_impl(&mut e, r#"{"signal_id":"nope","event":"submit"}"#).unwrap_err();
        assert!(
            err.contains("Signal not found"),
            "expected a Signal-not-found message, got: {err}"
        );
    }

    #[test]
    fn host_parse_event_json_payload_handling() {
        let (sid, ev, p) =
            parse_event_json(r#"{"signal_id":"a","event":"go","payload":{"x":1}}"#).unwrap();
        assert_eq!(sid, "a");
        assert_eq!(ev, "go");
        assert_eq!(p, Some(serde_json::json!({"x":1})));

        let (_, _, p) = parse_event_json(r#"{"signal_id":"a","event":"go"}"#).unwrap();
        assert_eq!(p, None, "absent payload -> None");

        let (_, _, p) = parse_event_json(r#"{"signal_id":"a","event":"go","payload":null}"#).unwrap();
        assert_eq!(p, None, "explicit null payload -> None");

        assert!(parse_event_json("not json").is_err());
    }
}

// ---------------------------------------------------------------------------
// wasm-targeted tests. These only compile (and run) on wasm32; when the host
/// runs `cargo test` they are compiled out. Run them in a browser / Node with:
//
//     wasm-pack test --headless --chrome   # or --firefox, or --node
//
// They mirror the host tests above against the real wasm glue.
// ---------------------------------------------------------------------------

#[cfg(all(test, target_arch = "wasm32"))]
mod wasm_tests {
    use super::*;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    const ORDER_APPROVAL: &str = include_str!("../../examples/order_approval.json");

    #[wasm_bindgen_test]
    fn wasm_engine_reaches_shipped() {
        let mut e = WasmEngine::new(ORDER_APPROVAL).expect("load");
        let s = e
            .send_event(r#"{"signal_id":"order","event":"submit"}"#)
            .unwrap();
        assert!(s.contains("submitted"), "submit: {s}");
        let a = e
            .send_event(r#"{"signal_id":"order","event":"approve","payload":{"amount":5000}}"#)
            .unwrap();
        assert!(a.contains("approved"), "approve: {a}");
        let sh = e
            .send_event(r#"{"signal_id":"order","event":"ship"}"#)
            .unwrap();
        assert!(sh.contains("shipped"), "ship: {sh}");
        assert_eq!(e.get_state("order"), Some("shipped".to_string()));
    }

    #[wasm_bindgen_test]
    fn wasm_snapshot_dot_includes_highlight() {
        let mut e = WasmEngine::new(ORDER_APPROVAL).expect("load");
        e.send_event(r#"{"signal_id":"order","event":"submit"}"#)
            .unwrap();
        let dot = e.snapshot_dot();
        assert!(
            dot.contains("lightgreen"),
            "dot should highlight live state"
        );
    }
}
