//! M44: fork/join workflow engine semantics.
//!
//! A transition can fan out to several reactions in parallel (fork); a later
//! reaction can wait until an entire fork group has completed (join). These
//! tests pin down that contract: a fork group's members all fire, and a joining
//! reaction fires only after the group completes — never before.

use signal_topology::{EngineError, TopologyEngine, TraceEvent};

fn engine_from_json(json: &str) -> TopologyEngine {
    TopologyEngine::from_json(json).expect("topology should load")
}

/// Collect the `StateChanged` events in trace order, as `(signal, to)`.
fn state_changed(events: &[TraceEvent]) -> Vec<(String, String)> {
    events
        .iter()
        .filter_map(|e| match e {
            TraceEvent::StateChanged { signal_id, to, .. } => {
                Some((signal_id.clone(), to.clone()))
            }
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// 1. fork: a transition fires B and C reactions in parallel — both execute.
// ---------------------------------------------------------------------------
#[test]
fn fork_fires_all_members() {
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
                "from_signal": "A", "from_state": "a1",
                "to_signal": "B", "event": "react",
                "join_group": "parallel"
            },
            {
                "from_signal": "A", "from_state": "a1",
                "to_signal": "C", "event": "react",
                "join_group": "parallel"
            }
        ]
    }"#;

    let mut engine = engine_from_json(json);
    engine.send_event("A", "go", None).expect("main transition commits");

    assert_eq!(engine.get_state("A").unwrap(), "a1");
    assert_eq!(engine.get_state("B").unwrap(), "b1", "fork member B must fire");
    assert_eq!(engine.get_state("C").unwrap(), "c1", "fork member C must fire");
}

// ---------------------------------------------------------------------------
// 2. join: the downstream reaction fires only after the fork group completes.
// ---------------------------------------------------------------------------
#[test]
fn join_waits_for_group_then_fires() {
    let json = r#"{
        "version": "0.1",
        "signals": [
            {"id": "A", "initial_state": "a0", "states": ["a0", "a1"]},
            {"id": "B", "initial_state": "b0", "states": ["b0", "b1"]},
            {"id": "C", "initial_state": "c0", "states": ["c0", "c1"]},
            {"id": "D", "initial_state": "d0", "states": ["d0", "d1"]}
        ],
        "transitions": [
            {"signal_id": "A", "from": "a0", "event": "go", "to": "a1"},
            {"signal_id": "B", "from": "b0", "event": "react", "to": "b1"},
            {"signal_id": "C", "from": "c0", "event": "react", "to": "c1"},
            {"signal_id": "D", "from": "d0", "event": "react", "to": "d1"}
        ],
        "reactions": [
            {
                "from_signal": "A", "from_state": "a1",
                "to_signal": "B", "event": "react",
                "join_group": "parallel"
            },
            {
                "from_signal": "A", "from_state": "a1",
                "to_signal": "C", "event": "react",
                "join_group": "parallel"
            },
            {
                "from_signal": "A", "from_state": "a1",
                "to_signal": "D", "event": "react",
                "requires": ["parallel"]
            }
        ]
    }"#;

    let mut engine = engine_from_json(json);
    engine.send_event("A", "go", None).expect("main transition commits");

    assert_eq!(engine.get_state("D").unwrap(), "d1", "join reaction D must fire");

    // D's state change must come after both B and C have changed — the join
    // reaction is held back until the parallel group completed.
    let changed = state_changed(engine.traces());
    let pos = |sig: &str| changed.iter().position(|(s, _)| s == sig);
    let b_at = pos("B").expect("B must have changed");
    let c_at = pos("C").expect("C must have changed");
    let d_at = pos("D").expect("D must have changed");
    assert!(d_at > b_at, "D must fire after B");
    assert!(d_at > c_at, "D must fire after C");
}

// ---------------------------------------------------------------------------
// 3. mixed: an ungrouped reaction still fires immediately, a fork group fires,
//    and a join waits for the group — all in one transition.
// ---------------------------------------------------------------------------
#[test]
fn mixed_ungrouped_fork_and_join() {
    let json = r#"{
        "version": "0.1",
        "signals": [
            {"id": "A", "initial_state": "a0", "states": ["a0", "a1"]},
            {"id": "B", "initial_state": "b0", "states": ["b0", "b1"]},
            {"id": "C", "initial_state": "c0", "states": ["c0", "c1"]},
            {"id": "D", "initial_state": "d0", "states": ["d0", "d1"]},
            {"id": "E", "initial_state": "e0", "states": ["e0", "e1"]}
        ],
        "transitions": [
            {"signal_id": "A", "from": "a0", "event": "go", "to": "a1"},
            {"signal_id": "B", "from": "b0", "event": "react", "to": "b1"},
            {"signal_id": "C", "from": "c0", "event": "react", "to": "c1"},
            {"signal_id": "D", "from": "d0", "event": "react", "to": "d1"},
            {"signal_id": "E", "from": "e0", "event": "react", "to": "e1"}
        ],
        "reactions": [
            {
                "from_signal": "A", "from_state": "a1",
                "to_signal": "E", "event": "react"
            },
            {
                "from_signal": "A", "from_state": "a1",
                "to_signal": "B", "event": "react",
                "join_group": "parallel"
            },
            {
                "from_signal": "A", "from_state": "a1",
                "to_signal": "C", "event": "react",
                "join_group": "parallel"
            },
            {
                "from_signal": "A", "from_state": "a1",
                "to_signal": "D", "event": "react",
                "requires": ["parallel"]
            }
        ]
    }"#;

    let mut engine = engine_from_json(json);
    engine.send_event("A", "go", None).expect("main transition commits");

    assert_eq!(engine.get_state("E").unwrap(), "e1", "ungrouped reaction fires");
    assert_eq!(engine.get_state("B").unwrap(), "b1");
    assert_eq!(engine.get_state("C").unwrap(), "c1");
    assert_eq!(engine.get_state("D").unwrap(), "d1", "join reaction fires");

    // E (no group, no dependency) must fire before the join reaction D.
    let changed = state_changed(engine.traces());
    let pos = |sig: &str| changed.iter().position(|(s, _)| s == sig).unwrap();
    assert!(pos("E") < pos("D"), "ungrouped E must fire before joined D");
}

// ---------------------------------------------------------------------------
// 4. join with a missing fork group simply never fires that reaction (it is
//    held forever) — it must not crash or block the rest of the cascade.
// ---------------------------------------------------------------------------
#[test]
fn join_to_undefined_group_is_held_without_crashing() {
    let json = r#"{
        "version": "0.1",
        "signals": [
            {"id": "A", "initial_state": "a0", "states": ["a0", "a1"]},
            {"id": "D", "initial_state": "d0", "states": ["d0", "d1"]}
        ],
        "transitions": [
            {"signal_id": "A", "from": "a0", "event": "go", "to": "a1"},
            {"signal_id": "D", "from": "d0", "event": "react", "to": "d1"}
        ],
        "reactions": [
            {
                "from_signal": "A", "from_state": "a1",
                "to_signal": "D", "event": "react",
                "requires": ["does_not_exist"]
            }
        ]
    }"#;

    let mut engine = engine_from_json(json);
    // The main transition commits; the join reaction is simply never unblocked.
    engine.send_event("A", "go", None).expect("main transition commits");
    assert_eq!(engine.get_state("A").unwrap(), "a1");
    assert_eq!(
        engine.get_state("D").unwrap(),
        "d0",
        "join to an undefined group must hold the reaction (D stays put)"
    );
}

// ---------------------------------------------------------------------------
// 5. fork/join is backward compatible: reactions with empty `requires` and no
//    `join_group` behave exactly like the pre-M44 serial cascade.
// ---------------------------------------------------------------------------
#[test]
fn backward_compat_serial_cascade_unchanged() {
    let json = r#"{
        "version": "0.1",
        "signals": [
            {"id": "A", "initial_state": "a0", "states": ["a0", "a1"]},
            {"id": "B", "initial_state": "b0", "states": ["b0", "b1"]},
            {"id": "C", "initial_state": "c0", "states": ["c0", "c1"]},
            {"id": "D", "initial_state": "d0", "states": ["d0", "d1"]}
        ],
        "transitions": [
            {"signal_id": "A", "from": "a0", "event": "go", "to": "a1"},
            {"signal_id": "B", "from": "b0", "event": "react", "to": "b1"},
            {"signal_id": "C", "from": "c0", "event": "react", "to": "c1"},
            {"signal_id": "D", "from": "d0", "event": "react", "to": "d1"}
        ],
        "reactions": [
            {"from_signal": "A", "from_state": "a1", "to_signal": "B", "event": "react"},
            {"from_signal": "B", "from_state": "b1", "to_signal": "C", "event": "react"},
            {"from_signal": "C", "from_state": "c1", "to_signal": "D", "event": "react"}
        ]
    }"#;

    let mut engine = engine_from_json(json);
    engine.send_event("A", "go", None).expect("chain commits");
    assert_eq!(engine.get_state("D").unwrap(), "d1");
}

// ---------------------------------------------------------------------------
// 6. a fork member whose cascade fails: the group still completes only after
//    the member is processed (failure aborts dispatch, as in the pre-M44
//    contract, and never unblocks a join that depends on it).
// ---------------------------------------------------------------------------
#[test]
fn fork_member_failure_propagates() {
    let json = r#"{
        "version": "0.1",
        "signals": [
            {"id": "A", "initial_state": "a0", "states": ["a0", "a1"]},
            {"id": "B", "initial_state": "b0", "states": ["b0", "b1"]},
            {"id": "C", "initial_state": "c0", "states": ["c0", "c1"]},
            {"id": "D", "initial_state": "d0", "states": ["d0", "d1"]}
        ],
        "transitions": [
            {"signal_id": "A", "from": "a0", "event": "go", "to": "a1"},
            {"signal_id": "B", "from": "b0", "event": "react", "to": "b1",
             "actions": {"on_enter": ["boom"]}},
            {"signal_id": "C", "from": "c0", "event": "react", "to": "c1"},
            {"signal_id": "D", "from": "d0", "event": "react", "to": "d1"}
        ],
        "reactions": [
            {
                "from_signal": "A", "from_state": "a1",
                "to_signal": "B", "event": "react",
                "join_group": "parallel"
            },
            {
                "from_signal": "A", "from_state": "a1",
                "to_signal": "C", "event": "react",
                "join_group": "parallel"
            },
            {
                "from_signal": "A", "from_state": "a1",
                "to_signal": "D", "event": "react",
                "requires": ["parallel"]
            }
        ]
    }"#;

    let mut engine = engine_from_json(json);
    engine.register_action("boom", |_| {
        Err(EngineError::ActionExecutionError("boom".to_string()))
    });

    let result = engine.send_event("A", "go", None);
    assert!(result.is_err(), "fork-member failure must propagate");

    // The failing sibling B rolls back; the still-committed main A keeps its
    // state; the never-unblocked join reaction D never fires.
    assert_eq!(engine.get_state("A").unwrap(), "a1", "committed main retained");
    assert_eq!(engine.get_state("B").unwrap(), "b0", "failing fork member rolled back");
    assert_eq!(engine.get_state("D").unwrap(), "d0", "join dependent on incomplete group never fires");
}
