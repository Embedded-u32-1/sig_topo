//! Integration test for the sts live-rollback demo (`fail <action>`).
//!
//! `src/bin/sts.rs` keeps its registration helper (`load_topology_for_run`)
//! inside the binary crate, so it is not reachable from `tests/`. This file
//! therefore mirrors that helper — load -> expand -> register every action with
//! a handler that consults a shared `fail_set` — exactly as the shell does, and
//! then exercises the same engine call path the REPL's `event` command drives.
//!
//! The point is to assert the *observable* contract of the `fail` command from
//! the user's seat: marking an action makes the next `send_event` fail with
//! `ActionExecutionError`, rolls the state back to the source, logs
//! `ActionFailed` + `Rollbacked` and logs no `StateChanged`; the marker is
//! sticky until a `reset` clears it. This is the sts-specific increment over
//! `tests/transaction_test.rs`, which injects failure by overriding a single
//! hardcoded action rather than via the shared fail-set the REPL manipulates.

use signal_topology::{load_topology, EngineError, TopologyEngine, TraceEvent};
use std::cell::RefCell;
use std::collections::HashSet;
use std::path::Path;
use std::rc::Rc;

/// Mirror of `sts::load_topology_for_run`: resolve includes + expand instances,
/// then register every action with a print-and-record handler that *also*
/// consults a shared `fail_set`. Returns the engine plus the shared set so a
/// test can drive `fail` / `reset` the way the REPL dispatch does.
fn load_engine_with_fail_set(
    topology_path: &str,
) -> (TopologyEngine, Rc<RefCell<HashSet<String>>>) {
    let schema = load_topology(Path::new(topology_path)).expect("should load topology");

    let mut action_ids = HashSet::new();
    for trans in &schema.transitions {
        action_ids.extend(trans.actions.all_actions().into_iter().cloned());
    }

    let mut engine = TopologyEngine::from_schema(schema).expect("should build engine");
    let fail_set = Rc::new(RefCell::new(HashSet::new()));

    for action_id in &action_ids {
        let id = action_id.clone();
        let fail_set = Rc::clone(&fail_set);
        engine.register_action(action_id, move |ctx| {
            if fail_set.borrow().contains(&id) {
                return Err(EngineError::ActionExecutionError(format!(
                    "injected failure for action '{}' (set via `fail`)",
                    id
                )));
            }
            println!("[action] {}.{}", ctx.signal_id, id);
            Ok(())
        });
    }

    (engine, fail_set)
}

/// Driving `approve` succeeds, then forcing its on-transition action to fail
/// makes the next `approve` roll back — exactly the REPL's "success -> fail ->
/// re-send -> Error + rollback" story. The action ids (`reserve_inventory`,
/// …) come straight from `examples/order_approval.json`.
#[test]
fn fail_command_triggers_observable_rollback() {
    let (mut engine, _fail_set) = load_engine_with_fail_set("examples/order_approval.json");

    // submit: draft -> submitted (all on_* actions succeed initially).
    let submit = engine.send_event("order", "submit", None);
    assert!(submit.is_ok(), "submit should succeed");
    assert_eq!(engine.get_state("order").unwrap(), "submitted");

    // First approve succeeds: submitted -> approved.
    let first = engine.send_event(
        "order",
        "approve",
        Some(serde_json::json!({"amount": 5000})),
    );
    assert!(first.is_ok(), "first approve should succeed");
    assert_eq!(engine.get_state("order").unwrap(), "approved");
    let traces = engine.traces();
    assert!(traces.iter().any(|e| matches!(
        e,
        TraceEvent::StateChanged {
            signal_id,
            from,
            to,
            ..
        } if signal_id == "order" && from == "submitted" && to == "approved"
    )));

    // Move the signal back into `submitted` so we can re-send `approve`. There
    // is no transition for this in the topology, so drive it via the engine's
    // own reaction-free path: reject is submitted -> rejected; instead we just
    // re-load a fresh engine at `submitted` by sending submit on a new engine.
    // The demo's important claim is about re-sending the same event, so build a
    // clean engine already at `submitted`.
    let (mut engine2, fail_set2) = load_engine_with_fail_set("examples/order_approval.json");
    engine2.send_event("order", "submit", None).unwrap();
    assert_eq!(engine2.get_state("order").unwrap(), "submitted");

    // Mark the on-transition action of `approve` to fail — this is what the
    // REPL's `fail reserve_inventory` does.
    fail_set2.borrow_mut().insert("reserve_inventory".to_string());

    // Re-sending approve now fails and rolls back to `submitted`.
    let rolled_back = engine2.send_event(
        "order",
        "approve",
        Some(serde_json::json!({"amount": 5000})),
    );
    assert!(rolled_back.is_err(), "forced action must make send_event fail");
    let err = rolled_back.unwrap_err();
    assert!(
        matches!(err, EngineError::ActionExecutionError(_)),
        "error must be ActionExecutionError, got {:?}",
        err
    );
    assert_eq!(engine2.get_state("order").unwrap(), "submitted");

    let traces2 = engine2.traces();
    // No committed StateChanged to `approved` for order on the failing attempt
    // (submit's own draft -> submitted StateChanged is, of course, present).
    assert!(traces2.iter().all(|e| !matches!(
        e,
        TraceEvent::StateChanged { signal_id, to, .. }
            if signal_id == "order" && to == "approved"
    )));
    // The failing action is recorded as ActionFailed.
    assert!(traces2.iter().any(|e| matches!(
        e,
        TraceEvent::ActionFailed {
            signal_id,
            action_id,
            ..
        } if signal_id == "order" && action_id == "reserve_inventory"
    )));
    // And a Rollbacked records the tentative -> source rewind.
    assert!(traces2.iter().any(|e| matches!(
        e,
        TraceEvent::Rollbacked {
            signal_id,
            from,
            to,
            ..
        } if signal_id == "order" && from == "approved" && to == "submitted"
    )));
}

/// The forced-failure marker is *sticky*: once set, the action keeps failing
/// across events until `reset` clears the set. This is what lets the user watch
/// the rollback repeatedly, then clear it and see the transition commit again.
#[test]
fn fail_marker_is_sticky_until_reset() {
    let (mut engine, fail_set) = load_engine_with_fail_set("examples/order_approval.json");
    engine.send_event("order", "submit", None).unwrap();
    assert_eq!(engine.get_state("order").unwrap(), "submitted");

    fail_set.borrow_mut().insert("reserve_inventory".to_string());

    // First forced failure.
    assert!(engine
        .send_event(
            "order",
            "approve",
            Some(serde_json::json!({"amount": 5000}))
        )
        .is_err());
    assert_eq!(engine.get_state("order").unwrap(), "submitted");

    // Marker is sticky: a second attempt also fails (no reset in between).
    assert!(engine
        .send_event(
            "order",
            "approve",
            Some(serde_json::json!({"amount": 5000}))
        )
        .is_err());
    assert_eq!(engine.get_state("order").unwrap(), "submitted");

    // After reset the same event commits.
    fail_set.borrow_mut().clear();
    assert!(engine
        .send_event(
            "order",
            "approve",
            Some(serde_json::json!({"amount": 5000}))
        )
        .is_ok());
    assert_eq!(engine.get_state("order").unwrap(), "approved");

    let traces = engine.traces();
    // Two ActionFailed entries (one per failed attempt) and exactly one
    // committed StateChanged (after reset).
    let failed = traces.iter().filter(|e| matches!(
        e,
        TraceEvent::ActionFailed { action_id, .. } if action_id == "reserve_inventory"
    )).count();
    let committed = traces.iter().filter(|e| matches!(
        e,
        TraceEvent::StateChanged { signal_id, from, to, .. }
            if signal_id == "order" && from == "submitted" && to == "approved"
    )).count();
    assert_eq!(failed, 2, "both failed attempts must be recorded");
    assert_eq!(committed, 1, "only the post-reset attempt commits");
}
