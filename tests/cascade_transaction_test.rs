//! M23 (v0.10): cascade-failure transaction semantics.
//!
//! The single-signal rollback introduced in M19 (see `transaction_test.rs`)
//! is *per-signal*. When a parent commits `StateChanged` and then fans out to
//! children via reactions, a failure at any cascade depth rolls back only the
//! failing signal — already-committed ancestors (and even committed sibling
//! reactions) are retained. Full cascade transaction semantics (rolling the
//! parent back when a descendant fails) are explicitly out of scope; the
//! business layer compensates via guards. These tests pin down the "committed
//! upper layers retained" contract so it cannot drift.

use signal_topology::{EngineError, TopologyEngine, TraceEvent};

/// Three-level cascade A -> B -> C where the deepest child C fails. The
/// contract: every signal that committed before the failure keeps its
/// committed state; only C rolls back to its source.
///
/// Asserts the four M23 requirements together:
///   1. the main transition committed and triggered a cascade that later failed;
///   2. an error propagates back to the caller;
///   3. the parent (and intermediate) states remain at their committed values;
///   4. the trace records the failure (ActionFailed) at the failing level.
#[test]
fn test_deep_cascade_failure_retains_all_committed_ancestors() {
    let json = r#"{
        "version": "0.1",
        "signals": [
            {"id": "A", "initial_state": "a0", "states": ["a0", "a1"]},
            {"id": "B", "initial_state": "b0", "states": ["b0", "b1"]},
            {"id": "C", "initial_state": "c0", "states": ["c0", "c1"]}
        ],
        "transitions": [
            {"signal_id": "A", "from": "a0", "event": "go", "to": "a1"},
            {"signal_id": "B", "from": "b0", "event": "process", "to": "b1"},
            {
                "signal_id": "C",
                "from": "c0",
                "event": "process",
                "to": "c1",
                "actions": { "on_enter": ["boom"] }
            }
        ],
        "reactions": [
            {"from_signal": "A", "from_state": "a1", "to_signal": "B", "event": "process"},
            {"from_signal": "B", "from_state": "b1", "to_signal": "C", "event": "process"}
        ]
    }"#;

    let mut engine = TopologyEngine::from_json(json).expect("Should load topology");
    // Only the deepest action fails; everything else is inert.
    engine.register_action("boom", |_| {
        Err(EngineError::ActionExecutionError(
            "deep child boom".to_string(),
        ))
    });

    // (2) the whole send returns an error once the deep cascade fails.
    let result = engine.send_event("A", "go", None);
    assert!(result.is_err(), "cascade failure must propagate as an error");

    // (3) committed ancestors retained — the core M23 contract.
    assert_eq!(engine.get_state("A").unwrap(), "a1", "parent A must retain committed state");
    assert_eq!(
        engine.get_state("B").unwrap(),
        "b1",
        "intermediate B must retain committed state"
    );
    assert_eq!(engine.get_state("C").unwrap(), "c0", "only the failing child C rolls back");

    let traces = engine.traces();

    // A and B committed and emitted StateChanged...
    assert!(traces.iter().any(|e| matches!(
        e,
        TraceEvent::StateChanged { signal_id, from, to, .. }
            if signal_id == "A" && from == "a0" && to == "a1"
    )));
    assert!(traces.iter().any(|e| matches!(
        e,
        TraceEvent::StateChanged { signal_id, from, to, .. }
            if signal_id == "B" && from == "b0" && to == "b1"
    )));
    // ...C did not.
    assert!(traces.iter().all(|e| !matches!(
        e,
        TraceEvent::StateChanged { signal_id, .. } if signal_id == "C"
    )));

    // (4) the failure is observable: ActionFailed + Rollbacked on C.
    assert!(traces.iter().any(|e| matches!(
        e,
        TraceEvent::ActionFailed { signal_id, action_id, error, .. }
            if signal_id == "C" && action_id == "boom" && error == "deep child boom"
    )));
    assert!(traces.iter().any(|e| matches!(
        e,
        TraceEvent::Rollbacked { signal_id, from, to, .. }
            if signal_id == "C" && from == "c1" && to == "c0"
    )));
    // Ancestors must not have rolled back.
    assert!(traces.iter().all(|e| !matches!(
        e,
        TraceEvent::Rollbacked { signal_id, .. } if signal_id == "A" || signal_id == "B"
    )));
}

/// Branching cascade: a parent fans out to two children, one of which fails.
/// The parent and the *committed sibling* both stay at their committed states;
/// only the failing child rolls back. This pins down that the reaction loop
/// does not undo work that already committed just because a later reaction
/// failed.
#[test]
fn test_branching_cascade_retains_parent_and_committed_sibling() {
    let json = r#"{
        "version": "0.1",
        "signals": [
            {"id": "A", "initial_state": "a0", "states": ["a0", "a1"]},
            {"id": "B", "initial_state": "b0", "states": ["b0", "b1"]},
            {"id": "D", "initial_state": "d0", "states": ["d0", "d1"]}
        ],
        "transitions": [
            {"signal_id": "A", "from": "a0", "event": "go", "to": "a1"},
            {"signal_id": "B", "from": "b0", "event": "process", "to": "b1"},
            {
                "signal_id": "D",
                "from": "d0",
                "event": "process",
                "to": "d1",
                "actions": { "on_enter": ["boom"] }
            }
        ],
        "reactions": [
            {"from_signal": "A", "from_state": "a1", "to_signal": "B", "event": "process"},
            {"from_signal": "A", "from_state": "a1", "to_signal": "D", "event": "process"}
        ]
    }"#;

    let mut engine = TopologyEngine::from_json(json).expect("Should load topology");
    engine.register_action("boom", |_| {
        Err(EngineError::ActionExecutionError("sibling boom".to_string()))
    });

    let result = engine.send_event("A", "go", None);
    assert!(result.is_err());

    assert_eq!(engine.get_state("A").unwrap(), "a1", "parent retained");
    assert_eq!(engine.get_state("B").unwrap(), "b1", "committed sibling retained");
    assert_eq!(engine.get_state("D").unwrap(), "d0", "failing sibling rolled back");

    let traces = engine.traces();
    // Sibling B committed even though sibling D failed later.
    assert!(traces.iter().any(|e| matches!(
        e,
        TraceEvent::StateChanged { signal_id, from, to, .. }
            if signal_id == "B" && from == "b0" && to == "b1"
    )));
    // Only D rolled back.
    assert!(traces.iter().any(|e| matches!(
        e,
        TraceEvent::Rollbacked { signal_id, from, to, .. }
            if signal_id == "D" && from == "d1" && to == "d0"
    )));
    assert!(traces.iter().all(|e| !matches!(
        e,
        TraceEvent::Rollbacked { signal_id, .. } if signal_id == "A" || signal_id == "B"
    )));
}
