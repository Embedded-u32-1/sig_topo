//! M47: reaction compensation (cross-signal rollback).
//!
//! A reaction may carry an `on_fail` action id. When the reaction's cascade
//! fails, the engine runs that action (with the failure message carried in
//! `ActionContext.failure`) *before* propagating the error upward — best-effort
//! compensation that never masks the original cascade error.

use std::cell::RefCell;
use std::rc::Rc;

use signal_topology::{EngineError, TopologyEngine, TraceEvent};

fn engine_from_json(json: &str) -> TopologyEngine {
    TopologyEngine::from_json(json).expect("topology should load")
}

/// Collect the compensation action ids that fired, in trace order.
fn compensated(events: &[TraceEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            TraceEvent::ReactionCompensated { action_id, .. } => Some(action_id.clone()),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// 1. basic: a single reaction whose cascade fails fires its on_fail hook, and
//    the original error still propagates.
// ---------------------------------------------------------------------------
#[test]
fn on_fail_fires_when_cascade_fails() {
    let json = r#"{
        "version": "0.1",
        "signals": [
            {"id": "A", "initial_state": "a0", "states": ["a0", "a1"]},
            {"id": "B", "initial_state": "b0", "states": ["b0", "b1"]}
        ],
        "transitions": [
            {"signal_id": "A", "from": "a0", "event": "go", "to": "a1"},
            {"signal_id": "B", "from": "b0", "event": "react", "to": "b1",
             "actions": {"on_enter": ["boom"]}}
        ],
        "reactions": [
            {
                "from_signal": "A", "from_state": "a1",
                "to_signal": "B", "event": "react",
                "on_fail": "compensate"
            }
        ]
    }"#;

    let mut engine = engine_from_json(json);
    let recorded: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let r = recorded.clone();
    engine.register_action("boom", |_| {
        Err(EngineError::ActionExecutionError("boom".to_string()))
    });
    engine.register_action("compensate", move |ctx| {
        // The failure context is available to the compensation action.
        r.borrow_mut()
            .push(format!("{}|{:?}", ctx.event, ctx.failure));
        Ok(())
    });

    let result = engine.send_event("A", "go", None);
    assert!(result.is_err(), "cascade failure must still propagate");

    // B rolled back; A kept its committed state.
    assert_eq!(engine.get_state("A").unwrap(), "a1");
    assert_eq!(engine.get_state("B").unwrap(), "b0", "failing target rolled back");

    // The compensation hook fired exactly once, and saw the failure message.
    assert_eq!(compensated(engine.traces()), vec!["compensate".to_string()]);
    assert_eq!(
        recorded.borrow().clone(),
        vec!["react|Some(\"Action execution error: boom\")".to_string()]
    );
}

// ---------------------------------------------------------------------------
// 2. fork scenario: a fork branch's cascade fails → its on_fail fires. The
//    failure aborts dispatch, so the sibling branch that was still pending and
//    the joining reaction never run.
// ---------------------------------------------------------------------------
#[test]
fn fork_branch_failure_triggers_on_fail() {
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
                "join_group": "parallel",
                "on_fail": "compB"
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
    let flag: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));
    let f = flag.clone();
    engine.register_action("boom", |_| {
        Err(EngineError::ActionExecutionError("boom".to_string()))
    });
    engine.register_action("compB", move |_| {
        *f.borrow_mut() = true;
        Ok(())
    });

    assert!(
        engine.send_event("A", "go", None).is_err(),
        "fork-branch failure must propagate"
    );
    assert!(*flag.borrow(), "compensation hook for the failed branch must fire");

    // The failing branch rolled back; the main transition committed; the join
    // reaction (which depends on the incomplete group) never fired.
    assert_eq!(engine.get_state("A").unwrap(), "a1");
    assert_eq!(engine.get_state("B").unwrap(), "b0");
    assert_eq!(engine.get_state("D").unwrap(), "d0", "join never unblocked");
}

// ---------------------------------------------------------------------------
// 3. compensation chain: an inner cascade fails → its on_fail fires; the error
//    bubbles up and makes the outer reaction's cascade fail → its on_fail fires
//    too. Both hooks run, bottom-up.
// ---------------------------------------------------------------------------
#[test]
fn compensation_chain_fires_bottom_up() {
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
            {"signal_id": "C", "from": "c0", "event": "react", "to": "c1",
             "actions": {"on_enter": ["boom"]}}
        ],
        "reactions": [
            {
                "from_signal": "A", "from_state": "a1",
                "to_signal": "B", "event": "react",
                "on_fail": "compOuter"
            },
            {
                "from_signal": "B", "from_state": "b1",
                "to_signal": "C", "event": "react",
                "on_fail": "compInner"
            }
        ]
    }"#;

    let mut engine = engine_from_json(json);
    engine.register_action("boom", |_| {
        Err(EngineError::ActionExecutionError("boom".to_string()))
    });
    engine.register_action("compInner", |_| Ok(()));
    engine.register_action("compOuter", |_| Ok(()));

    assert!(
        engine.send_event("A", "go", None).is_err(),
        "chained failure must propagate"
    );

    // Inner compensation (C) fires first, then the outer (B).
    assert_eq!(
        compensated(engine.traces()),
        vec!["compInner".to_string(), "compOuter".to_string()],
        "compensation must run bottom-up"
    );
}

// ---------------------------------------------------------------------------
// 4. backward compatibility: a reaction with no on_fail propagates the cascade
//    error exactly as before — no hook, no compensation event.
// ---------------------------------------------------------------------------
#[test]
fn no_on_fail_preserves_legacy_behavior() {
    let json = r#"{
        "version": "0.1",
        "signals": [
            {"id": "A", "initial_state": "a0", "states": ["a0", "a1"]},
            {"id": "B", "initial_state": "b0", "states": ["b0", "b1"]}
        ],
        "transitions": [
            {"signal_id": "A", "from": "a0", "event": "go", "to": "a1"},
            {"signal_id": "B", "from": "b0", "event": "react", "to": "b1",
             "actions": {"on_enter": ["boom"]}}
        ],
        "reactions": [
            {
                "from_signal": "A", "from_state": "a1",
                "to_signal": "B", "event": "react"
            }
        ]
    }"#;

    let mut engine = engine_from_json(json);
    engine.register_action("boom", |_| {
        Err(EngineError::ActionExecutionError("boom".to_string()))
    });

    assert!(engine.send_event("A", "go", None).is_err());
    assert!(
        compensated(engine.traces()).is_empty(),
        "no on_fail → no compensation event"
    );
}

// ---------------------------------------------------------------------------
// 5. a failing compensation hook must NOT mask the original cascade error: the
//    hook still records its attempt (ActionFailed) and the cascade Err
//    propagates.
// ---------------------------------------------------------------------------
#[test]
fn failing_hook_does_not_mask_original_error() {
    let json = r#"{
        "version": "0.1",
        "signals": [
            {"id": "A", "initial_state": "a0", "states": ["a0", "a1"]},
            {"id": "B", "initial_state": "b0", "states": ["b0", "b1"]}
        ],
        "transitions": [
            {"signal_id": "A", "from": "a0", "event": "go", "to": "a1"},
            {"signal_id": "B", "from": "b0", "event": "react", "to": "b1",
             "actions": {"on_enter": ["boom"]}}
        ],
        "reactions": [
            {
                "from_signal": "A", "from_state": "a1",
                "to_signal": "B", "event": "react",
                "on_fail": "compBoom"
            }
        ]
    }"#;

    let mut engine = engine_from_json(json);
    engine.register_action("boom", |_| {
        Err(EngineError::ActionExecutionError("boom".to_string()))
    });
    engine.register_action("compBoom", |_| {
        Err(EngineError::ActionExecutionError("comp-boom".to_string()))
    });

    let result = engine.send_event("A", "go", None);
    assert!(result.is_err());
    // The propagated error is the original cascade error, not the hook's.
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("boom"),
        "original cascade error must propagate, got: {}",
        msg
    );

    // The hook ran (ReactionCompensated) even though it then failed.
    assert_eq!(compensated(engine.traces()), vec!["compBoom".to_string()]);
}
