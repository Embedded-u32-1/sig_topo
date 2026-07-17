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

// ---------------------------------------------------------------------------
// M38 part B: every reaction guard evaluation is recorded in the trace as a
// `ReactionGuardEvaluated` event, with result "true" / "false" / "error: ...".
// ---------------------------------------------------------------------------

/// Collect the `ReactionGuardEvaluated` events from an engine's trace.
fn guard_eval_events(engine: &TopologyEngine) -> Vec<TraceEvent> {
    engine
        .traces()
        .iter()
        .filter(|e| matches!(e, TraceEvent::ReactionGuardEvaluated { .. }))
        .cloned()
        .collect()
}

#[test]
fn reaction_guard_evaluated_true_is_traced() {
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
                "from_signal": "A", "from_state": "a1",
                "to_signal": "B", "event": "react",
                "guard": "payload.enable == true"
            }
        ]
    }"#;

    let mut engine = engine_from_json(json);
    engine
        .send_event("A", "go", Some(serde_json::json!({"enable": true})))
        .unwrap();

    let evals = guard_eval_events(&engine);
    assert_eq!(evals.len(), 1, "a single guarded reaction yields one eval event");
    match &evals[0] {
        TraceEvent::ReactionGuardEvaluated {
            reaction_from_signal,
            reaction_from_state,
            reaction_to_signal,
            reaction_event,
            guard,
            result,
            ..
        } => {
            assert_eq!(reaction_from_signal, "A");
            assert_eq!(reaction_from_state, "a1");
            assert_eq!(reaction_to_signal, "B");
            assert_eq!(reaction_event, "react");
            assert_eq!(guard, "payload.enable == true");
            assert_eq!(result, "true");
        }
        other => panic!("expected ReactionGuardEvaluated, got {:?}", other),
    }
}

#[test]
fn reaction_guard_evaluated_false_is_traced() {
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
                "from_signal": "A", "from_state": "a1",
                "to_signal": "B", "event": "react",
                "guard": "payload.enable == true"
            }
        ]
    }"#;

    let mut engine = engine_from_json(json);
    engine
        .send_event("A", "go", Some(serde_json::json!({"enable": false})))
        .unwrap();

    let evals = guard_eval_events(&engine);
    assert_eq!(evals.len(), 1);
    match &evals[0] {
        TraceEvent::ReactionGuardEvaluated { result, .. } => {
            assert_eq!(result, "false", "guard=false must be recorded as \"false\"");
        }
        other => panic!("expected ReactionGuardEvaluated, got {:?}", other),
    }

    // The cascade was skipped: B stays at b0.
    assert_eq!(engine.get_state("B").unwrap(), "b0");
}

#[test]
fn reaction_guard_evaluated_error_is_traced() {
    // `payload.x + "s"` adds an integer-shaped null to a string -> the Add
    // arm errors ("Cannot perform arithmetic"). The guard evaluation must be
    // recorded as "error: <msg>" and the reaction skipped.
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
                "from_signal": "A", "from_state": "a1",
                "to_signal": "B", "event": "react",
                "guard": "payload.x + \"s\""
            }
        ]
    }"#;

    let mut engine = engine_from_json(json);
    // payload.x is absent -> Null; Null + "s" is an arithmetic error.
    engine
        .send_event("A", "go", Some(serde_json::json!({"x": null})))
        .unwrap();

    let evals = guard_eval_events(&engine);
    assert_eq!(evals.len(), 1);
    match &evals[0] {
        TraceEvent::ReactionGuardEvaluated { result, .. } => {
            assert!(
                result.starts_with("error:"),
                "expected \"error: ...\", got {:?}",
                result
            );
        }
        other => panic!("expected ReactionGuardEvaluated, got {:?}", other),
    }

    // Reaction skipped on eval error, main transition still commits.
    assert_eq!(engine.get_state("A").unwrap(), "a1");
    assert_eq!(engine.get_state("B").unwrap(), "b0");
}

// ---------------------------------------------------------------------------
// M38 part A end-to-end: two reactions sharing one guard id behave identically
// and expand to the same guard text.
// ---------------------------------------------------------------------------
use signal_topology::ddl::compile;

#[test]
fn two_reactions_share_guard_id_behave_identically() {
    let ddl = r#"
guard allow_alloc {
    payload.auto == true
}

signal order {
    states: [pending, approved]
    initial: pending
    on approve from pending -> approved
}
signal inventory {
    states: [idle, allocated]
    initial: idle
    on allocate from idle -> allocated
}
signal audit {
    states: [idle, noted]
    initial: idle
    on note from idle -> noted
}

// Two reactions reference the same guard id.
reaction {
    when order enters approved -> inventory allocate when allow_alloc
}
reaction {
    when order enters approved -> audit note when allow_alloc
}
"#;

    let schema = compile(ddl).expect("ddl should compile");
    let r0 = &schema.reactions[0];
    let r1 = &schema.reactions[1];

    // Both expanded to the identical guard text (the inlined expression).
    assert_eq!(r0.guard, r1.guard);
    assert_eq!(r0.guard, Some("payload.auto == true".to_string()));

    let mut engine = TopologyEngine::from_schema(schema).unwrap();

    // enable=true → both reactions fire.
    engine
        .send_event("order", "approve", Some(serde_json::json!({"auto": true})))
        .unwrap();
    assert_eq!(engine.get_state("inventory").unwrap(), "allocated");
    assert_eq!(engine.get_state("audit").unwrap(), "noted");

    let evals = guard_eval_events(&engine);
    assert_eq!(evals.len(), 2, "two guarded reactions → two eval events");
    for e in evals {
        match e {
            TraceEvent::ReactionGuardEvaluated { result, .. } => assert_eq!(result, "true"),
            other => panic!("expected ReactionGuardEvaluated, got {:?}", other),
        }
    }
}

#[test]
fn two_reactions_share_guard_id_skipped_when_false() {
    let ddl = r#"
guard allow_alloc {
    payload.auto == true
}

signal order {
    states: [pending, approved]
    initial: pending
    on approve from pending -> approved
}
signal inventory {
    states: [idle, allocated]
    initial: idle
    on allocate from idle -> allocated
}
signal audit {
    states: [idle, noted]
    initial: idle
    on note from idle -> noted
}

reaction {
    when order enters approved -> inventory allocate when allow_alloc
}
reaction {
    when order enters approved -> audit note when allow_alloc
}
"#;

    let mut engine = TopologyEngine::from_schema(compile(ddl).unwrap()).unwrap();

    // enable=false → both reactions skipped (shared guard), but order still commits.
    engine
        .send_event("order", "approve", Some(serde_json::json!({"auto": false})))
        .unwrap();
    assert_eq!(engine.get_state("order").unwrap(), "approved");
    assert_eq!(engine.get_state("inventory").unwrap(), "idle");
    assert_eq!(engine.get_state("audit").unwrap(), "idle");

    let evals = guard_eval_events(&engine);
    assert_eq!(evals.len(), 2);
    for e in evals {
        match e {
            TraceEvent::ReactionGuardEvaluated { result, .. } => assert_eq!(result, "false"),
            other => panic!("expected ReactionGuardEvaluated, got {:?}", other),
        }
    }
}
