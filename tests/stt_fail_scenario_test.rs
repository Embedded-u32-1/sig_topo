//! M26: stt failure-injection scenario replay.
//!
//! `stt` used to register no-op handlers, so a scenario replay could never roll
//! back. After the shared-helper refactor (`src/run.rs`), stt builds its engine
//! through the same `load_topology_for_run` the shell uses, and each scenario
//! event may name `fail_actions` to inject a failure for that event alone.
//!
//! These tests drive the real shared path (`load_topology_for_run` +
//! `run_scenario`) and assert the observable contract: a successful replay
//! commits every transition; an injected failure logs `ActionFailed` +
//! `Rollbacked`, rolls the signal back to its source state, and replay
//! *continues* with the next event (record-and-continue, mirroring the shell's
//! "roll back + wait for next command" semantics).

use signal_topology::run::{load_topology_for_run, run_scenario, Scenario};
use signal_topology::{EngineError, TraceEvent};
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

/// Parse a scenario JSON file and replay it through a freshly built engine,
/// returning the engine so a test can inspect state + trace. Mirrors what
/// `src/bin/stt.rs::main` does, minus the IO around it.
fn replay(scenario_path: &str, topology_path: &str) -> signal_topology::TopologyEngine {
    let fail_set = Rc::new(RefCell::new(HashSet::new()));
    let mut engine = load_topology_for_run(topology_path, Some(Rc::clone(&fail_set)), false);

    let json = std::fs::read_to_string(scenario_path).expect("scenario file readable");
    let scenario: Scenario = serde_json::from_str(&json).expect("scenario JSON parses");

    let errors = run_scenario(&mut engine, &scenario, &fail_set);
    // The replay path records and continues; surface any error as a panic only
    // when a test does not explicitly assert it (the success test below).
    assert!(
        errors.is_empty(),
        "unexpected replay errors: {:?}",
        errors
    );
    engine
}

/// Regression: a scenario with no `fail_actions` commits every transition, just
/// like the pre-M26 replay. Asserts the full draft -> submitted -> approved ->
/// shipped chain and that the trace records a `StateChanged` for each with no
/// `ActionFailed` / `Rollbacked` anywhere.
#[test]
fn successful_scenario_commits_every_transition() {
    let engine = replay(
        "tests/fixtures/scenario_success.json",
        "examples/order_approval.json",
    );

    assert_eq!(engine.get_state("order").unwrap(), "shipped");

    let traces = engine.traces();
    assert!(traces.iter().any(|e| matches!(
        e,
        TraceEvent::StateChanged { signal_id, from, to, .. }
            if signal_id == "order" && from == "draft" && to == "submitted"
    )));
    assert!(traces.iter().any(|e| matches!(
        e,
        TraceEvent::StateChanged { signal_id, from, to, .. }
            if signal_id == "order" && from == "submitted" && to == "approved"
    )));
    assert!(traces.iter().any(|e| matches!(
        e,
        TraceEvent::StateChanged { signal_id, from, to, .. }
            if signal_id == "order" && from == "approved" && to == "shipped"
    )));

    assert!(
        traces.iter().all(|e| !matches!(e, TraceEvent::ActionFailed { .. })),
        "a clean scenario must not log any ActionFailed"
    );
    assert!(
        traces.iter().all(|e| !matches!(e, TraceEvent::Rollbacked { .. })),
        "a clean scenario must not log any Rollbacked"
    );
}

/// The core M26 claim: an injected failure rolls the signal back and replay
/// continues. The second event names `reserve_inventory` in `fail_actions`, so
/// its `approve` fails with `ActionExecutionError`, logs `ActionFailed` +
/// `Rollbacked`, and leaves the signal at `submitted`. The third event re-sends
/// `approve` with no injection, which now commits to `approved` -- proving the
/// failure was scoped to one event and that record-and-continue works.
#[test]
fn injected_failure_rolls_back_and_replay_continues() {
    let fail_set = Rc::new(RefCell::new(HashSet::new()));
    let mut engine = load_topology_for_run(
        "examples/order_approval.json",
        Some(Rc::clone(&fail_set)),
        false,
    );

    let json = std::fs::read_to_string("tests/fixtures/scenario_fail_inject.json")
        .expect("scenario file readable");
    let scenario: Scenario = serde_json::from_str(&json).expect("scenario JSON parses");

    let errors = run_scenario(&mut engine, &scenario, &fail_set);

    // Exactly one event errored: the injected `approve`.
    assert_eq!(errors.len(), 1, "only the injected event should error");
    assert_eq!(errors[0].signal_id, "order");
    assert_eq!(errors[0].event, "approve");
    assert!(matches!(errors[0].error, EngineError::ActionExecutionError(_)));

    // The signal rolled back to the source state of the failing transition and
    // stayed there until the (non-injected) re-try committed.
    assert_eq!(engine.get_state("order").unwrap(), "approved");

    let traces = engine.traces();

    // submit committed: draft -> submitted.
    assert!(traces.iter().any(|e| matches!(
        e,
        TraceEvent::StateChanged { signal_id, from, to, .. }
            if signal_id == "order" && from == "draft" && to == "submitted"
    )));

    // The injected failure is recorded as ActionFailed + Rollbacked, and there
    // is NO committed StateChanged to `approved` from the first (failing) attempt.
    assert!(traces.iter().any(|e| matches!(
        e,
        TraceEvent::ActionFailed { signal_id, action_id, .. }
            if signal_id == "order" && action_id == "reserve_inventory"
    )));
    assert!(traces.iter().any(|e| matches!(
        e,
        TraceEvent::Rollbacked { signal_id, from, to, .. }
            if signal_id == "order" && from == "approved" && to == "submitted"
    )));

    // The re-try committed: exactly one submitted -> approved StateChanged.
    let committed = traces.iter().filter(|e| matches!(
        e,
        TraceEvent::StateChanged { signal_id, from, to, .. }
            if signal_id == "order" && from == "submitted" && to == "approved"
    )).count();
    assert_eq!(committed, 1, "only the post-rollback attempt should commit");
}

/// `fail_actions` must be scoped to the event that names them: an action forced
/// to fail in one event must succeed in a later event that does not name it.
/// This is what keeps a scenario readable and replay deterministic.
#[test]
fn fail_actions_are_scoped_per_event() {
    let fail_set = Rc::new(RefCell::new(HashSet::new()));
    let mut engine = load_topology_for_run(
        "examples/order_approval.json",
        Some(Rc::clone(&fail_set)),
        false,
    );

    // First approve injects a failure; second approve does not.
    let scenario = Scenario {
        events: vec![
            signal_topology::run::ScenarioEvent {
                signal_id: "order".to_string(),
                event: "submit".to_string(),
                payload: None,
                fail_actions: vec![],
            },
            signal_topology::run::ScenarioEvent {
                signal_id: "order".to_string(),
                event: "approve".to_string(),
                payload: Some(serde_json::json!({"amount": 5000})),
                fail_actions: vec!["reserve_inventory".to_string()],
            },
            signal_topology::run::ScenarioEvent {
                signal_id: "order".to_string(),
                event: "approve".to_string(),
                payload: Some(serde_json::json!({"amount": 5000})),
                fail_actions: vec![],
            },
        ],
    };

    let errors = run_scenario(&mut engine, &scenario, &fail_set);
    assert_eq!(errors.len(), 1, "only the event naming the injection errors");

    // After the scoped failure clears, the same action runs fine: reserve_inventory
    // appears as ActionSucceeded in the re-try.
    let traces = engine.traces();
    assert!(traces.iter().any(|e| matches!(
        e,
        TraceEvent::ActionSucceeded { signal_id, action_id, .. }
            if signal_id == "order" && action_id == "reserve_inventory"
    )));
    assert_eq!(engine.get_state("order").unwrap(), "approved");
}
