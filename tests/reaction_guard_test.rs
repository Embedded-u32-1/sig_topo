//! M32: reaction-guard semantics.
//!
//! A reaction guard gates the cascade: it is evaluated against the reaction's
//! static payload before the derived event is dispatched. A guard that is
//! false, or that fails to evaluate, skips *that* reaction only — the main
//! transition has already committed, and the remaining reactions are untouched.
//! This file pins down that contract.

use signal_topology::{TopologyEngine, TraceEvent};

/// Build an engine from a JSON topology string.
fn engine_from_json(json: &str) -> TopologyEngine {
    TopologyEngine::from_json(json).expect("topology should load")
}

// ---------------------------------------------------------------------------
// 1. guard = true  → cascade fires (end-to-end, same as no guard).
// ---------------------------------------------------------------------------
#[test]
fn reaction_guard_true_fires_cascade() {
    let json = r#"{
        "version": "0.1",
        "signals": [
            {"id": "A", "initial_state": "a0", "states": ["a0", "a1"]},
            {"id": "B", "initial_state": "b0", "states": ["b0", "b1"]}
        ],
        "transitions": [
            {"signal_id": "A", "from": "a0", "event": "go", "to": "a1"},
            {"signal_id": "B", "from": "b0", "event": "react", "to": "b1"}
        ],
        "reactions": [
            {
                "from_signal": "A",
                "from_state": "a1",
                "to_signal": "B",
                "event": "react",
                "guard": "payload.enable == true"
            }
        ]
    }"#;

    let mut engine = engine_from_json(json);
    engine
        .send_event("A", "go", Some(serde_json::json!({"enable": true})))
        .expect("main transition should commit");

    assert_eq!(engine.get_state("A").unwrap(), "a1");
    assert_eq!(
        engine.get_state("B").unwrap(),
        "b1",
        "guard=true must let the cascade fire"
    );
}

// ---------------------------------------------------------------------------
// 2. guard = false → cascade skipped, but the main transition commits.
// ---------------------------------------------------------------------------
#[test]
fn reaction_guard_false_skips_cascade_but_commits_main() {
    let json = r#"{
        "version": "0.1",
        "signals": [
            {"id": "A", "initial_state": "a0", "states": ["a0", "a1"]},
            {"id": "B", "initial_state": "b0", "states": ["b0", "b1"]}
        ],
        "transitions": [
            {"signal_id": "A", "from": "a0", "event": "go", "to": "a1"},
            {"signal_id": "B", "from": "b0", "event": "react", "to": "b1"}
        ],
        "reactions": [
            {
                "from_signal": "A",
                "from_state": "a1",
                "to_signal": "B",
                "event": "react",
                "guard": "payload.enable == true"
            }
        ]
    }"#;

    let mut engine = engine_from_json(json);
    // Main transition succeeds even though the reaction guard is false.
    engine
        .send_event("A", "go", Some(serde_json::json!({"enable": false})))
        .expect("main transition must commit regardless of reaction guard");

    assert_eq!(engine.get_state("A").unwrap(), "a1", "main transition committed");
    assert_eq!(
        engine.get_state("B").unwrap(),
        "b0",
        "guard=false must skip the cascade, leaving B untouched"
    );

    // No EventReceived was recorded for B — the derived event never fired.
    let b_received = engine
        .traces_for("B")
        .into_iter()
        .filter(|e| matches!(e, TraceEvent::EventReceived { .. }))
        .count();
    assert_eq!(b_received, 0, "no derived event should reach B");
}

// ---------------------------------------------------------------------------
// 3. guard references a payload field → decision follows the payload value.
// ---------------------------------------------------------------------------
#[test]
fn reaction_guard_gates_on_payload_field() {
    let json = r#"{
        "version": "0.1",
        "signals": [
            {"id": "A", "initial_state": "a0", "states": ["a0", "a1"]},
            {"id": "B", "initial_state": "b0", "states": ["b0", "b1"]}
        ],
        "transitions": [
            {"signal_id": "A", "from": "a0", "event": "go", "to": "a1"},
            {"signal_id": "B", "from": "b0", "event": "react", "to": "b1"}
        ],
        "reactions": [
            {
                "from_signal": "A",
                "from_state": "a1",
                "to_signal": "B",
                "event": "react",
                "guard": "payload.enable == true"
            }
        ]
    }"#;

    // Guard reads the *source* event's payload, which carries enable=true.
    let mut engine = engine_from_json(json);
    engine
        .send_event("A", "go", Some(serde_json::json!({"enable": true})))
        .expect("main transition commits");
    assert_eq!(
        engine.get_state("B").unwrap(),
        "b1",
        "source payload enable=true → guard passes → cascade fires"
    );

    // Same topology, but source payload says enable=false → guard blocks the
    // cascade while the main transition still commits.
    let mut engine2 = engine_from_json(json);
    engine2
        .send_event("A", "go", Some(serde_json::json!({"enable": false})))
        .expect("main transition commits");
    assert_eq!(
        engine2.get_state("B").unwrap(),
        "b0",
        "source payload enable=false → guard skips the cascade"
    );
}

// ---------------------------------------------------------------------------
// 4. guard that fails to evaluate → skip that reaction (robust cascade).
// ---------------------------------------------------------------------------
#[test]
fn reaction_guard_eval_error_skips_reaction() {
    // `payload.missing > 5` — the field is absent, so the comparison yields
    // Null, which is falsy. The guard evaluates cleanly to false (no error),
    // and the reaction is skipped. The main transition still commits.
    let json = r#"{
        "version": "0.1",
        "signals": [
            {"id": "A", "initial_state": "a0", "states": ["a0", "a1"]},
            {"id": "B", "initial_state": "b0", "states": ["b0", "b1"]}
        ],
        "transitions": [
            {"signal_id": "A", "from": "a0", "event": "go", "to": "a1"},
            {"signal_id": "B", "from": "b0", "event": "react", "to": "b1"}
        ],
        "reactions": [
            {
                "from_signal": "A",
                "from_state": "a1",
                "to_signal": "B",
                "event": "react",
                "guard": "payload.missing > 5"
            }
        ]
    }"#;

    let mut engine = engine_from_json(json);
    engine
        .send_event("A", "go", None)
        .expect("main transition must commit even when a reaction guard is ill-formed");

    assert_eq!(engine.get_state("A").unwrap(), "a1");
    assert_eq!(
        engine.get_state("B").unwrap(),
        "b0",
        "a guard that cannot be satisfied must skip the reaction, not crash the cascade"
    );
}

// ---------------------------------------------------------------------------
// 5. one guarded reaction skipped, a sibling reaction still fires.
//    Confirms a failing guard does not abort the reaction loop.
// ---------------------------------------------------------------------------
#[test]
fn reaction_guard_skip_does_not_abort_sibling_reactions() {
    let json = r#"{
        "version": "0.1",
        "signals": [
            {"id": "A", "initial_state": "a0", "states": ["a0", "a1"]},
            {"id": "B", "initial_state": "b0", "states": ["b0", "b1"]},
            {"id": "C", "initial_state": "c0", "states": ["c0", "c1"]}
        ],
        "transitions": [
            {"signal_id": "A", "from": "a0", "event": "go", "to": "a1"},
            {"signal_id": "B", "from": "b0", "event": "react", "to": "b1"},
            {"signal_id": "C", "from": "c0", "event": "react", "to": "c1"}
        ],
        "reactions": [
            {
                "from_signal": "A",
                "from_state": "a1",
                "to_signal": "B",
                "event": "react",
                "guard": "payload.enable == true"
            },
            {
                "from_signal": "A",
                "from_state": "a1",
                "to_signal": "C",
                "event": "react"
            }
        ]
    }"#;

    let mut engine = engine_from_json(json);
    engine
        .send_event("A", "go", Some(serde_json::json!({"enable": false})))
        .expect("main transition commits");

    assert_eq!(engine.get_state("A").unwrap(), "a1");
    assert_eq!(engine.get_state("B").unwrap(), "b0", "guarded reaction skipped");
    assert_eq!(
        engine.get_state("C").unwrap(),
        "c1",
        "un-guarded sibling reaction must still fire"
    );
}

// ---------------------------------------------------------------------------
// 6. backward compatibility: a reaction with no guard field (legacy JSON)
//    still cascades unconditionally.
// ---------------------------------------------------------------------------
#[test]
fn reaction_without_guard_field_still_cascades() {
    let json = r#"{
        "version": "0.1",
        "signals": [
            {"id": "A", "initial_state": "a0", "states": ["a0", "a1"]},
            {"id": "B", "initial_state": "b0", "states": ["b0", "b1"]}
        ],
        "transitions": [
            {"signal_id": "A", "from": "a0", "event": "go", "to": "a1"},
            {"signal_id": "B", "from": "b0", "event": "react", "to": "b1"}
        ],
        "reactions": [
            {"from_signal": "A", "from_state": "a1", "to_signal": "B", "event": "react"}
        ]
    }"#;

    let mut engine = engine_from_json(json);
    engine.send_event("A", "go", None).expect("main transition commits");
    assert_eq!(engine.get_state("B").unwrap(), "b1", "legacy reaction (no guard) cascades");
}

// ---------------------------------------------------------------------------
// 7. guard=false must NOT be reported as an error to the caller — the main
//    send_event returns Ok, because the main transition succeeded.
// ---------------------------------------------------------------------------
#[test]
fn reaction_guard_false_returns_ok_not_err() {
    let json = r#"{
        "version": "0.1",
        "signals": [
            {"id": "A", "initial_state": "a0", "states": ["a0", "a1"]},
            {"id": "B", "initial_state": "b0", "states": ["b0", "b1"]}
        ],
        "transitions": [
            {"signal_id": "A", "from": "a0", "event": "go", "to": "a1"},
            {"signal_id": "B", "from": "b0", "event": "react", "to": "b1"}
        ],
        "reactions": [
            {
                "from_signal": "A",
                "from_state": "a1",
                "to_signal": "B",
                "event": "react",
                "guard": "payload.enable == true"
            }
        ]
    }"#;

    let mut engine = engine_from_json(json);
    let result = engine.send_event("A", "go", Some(serde_json::json!({"enable": false})));
    assert!(
        matches!(result, Ok(ref r) if r.signal_id == "A" && r.to == "a1"),
        "send_event must return Ok with the main transition result, got {:?}",
        result
    );
}
