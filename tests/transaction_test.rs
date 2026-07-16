use signal_topology::{EngineError, TopologyEngine, TraceEvent};
use std::fs;

fn load_topology() -> String {
    fs::read_to_string("tests/topology.json").expect("Failed to read topology.json")
}

/// All lifecycle actions succeed: state commits, `StateChanged` is present, no
/// `Rollbacked`.
#[test]
fn test_transaction_full_success_commits_state() {
    let mut engine = TopologyEngine::from_json(&load_topology()).expect("Should load topology");
    for action_id in [
        "log_idle_leave",
        "init_task_resource",
        "start_task_execution",
    ] {
        engine.register_action(action_id, |_| Ok(()));
    }

    let result = engine.send_event("task_status", "start", None);
    assert!(result.is_ok());
    let result = result.unwrap();
    assert_eq!(result.from, "idle");
    assert_eq!(result.to, "running");
    assert_eq!(engine.get_state("task_status").unwrap(), "running");

    let traces = engine.traces();
    assert!(traces.iter().any(|e| matches!(
        e,
        TraceEvent::StateChanged {
            signal_id,
            from,
            to,
            ..
        } if signal_id == "task_status" && from == "idle" && to == "running"
    )));
    assert!(traces.iter().all(|e| !matches!(
        e,
        TraceEvent::Rollbacked { signal_id, .. } if signal_id == "task_status"
    )));
}

/// `on_transition` action fails: state rolls back to source, `StateChanged` is
/// absent, `ActionFailed` + `Rollbacked` are present for observability.
#[test]
fn test_transaction_on_transition_failure_rolls_back() {
    let mut engine = TopologyEngine::from_json(&load_topology()).expect("Should load topology");
    engine.register_action("log_idle_leave", |_| Ok(()));
    engine.register_action("init_task_resource", |_| {
        Err(EngineError::ActionExecutionError(
            "resource init failed".to_string(),
        ))
    });
    engine.register_action("start_task_execution", |_| Ok(()));

    let result = engine.send_event("task_status", "start", None);
    assert!(result.is_err());
    assert_eq!(engine.get_state("task_status").unwrap(), "idle");

    let traces = engine.traces();
    assert!(traces.iter().all(|e| !matches!(
        e,
        TraceEvent::StateChanged { signal_id, .. } if signal_id == "task_status"
    )));
    assert!(traces.iter().any(|e| matches!(
        e,
        TraceEvent::ActionFailed {
            signal_id,
            action_id,
            error,
            ..
        } if signal_id == "task_status" && action_id == "init_task_resource" && error == "resource init failed"
    )));
    assert!(traces.iter().any(|e| matches!(
        e,
        TraceEvent::Rollbacked {
            signal_id,
            from,
            to,
            ..
        } if signal_id == "task_status" && from == "running" && to == "idle"
    )));
}

/// `on_enter` action fails: state still rolls back to source, even though the
/// `on_transition` action in front of it succeeded.
#[test]
fn test_transaction_on_enter_failure_rolls_back() {
    let mut engine = TopologyEngine::from_json(&load_topology()).expect("Should load topology");
    engine.register_action("log_idle_leave", |_| Ok(()));
    engine.register_action("init_task_resource", |_| Ok(()));
    engine.register_action("start_task_execution", |_| {
        Err(EngineError::ActionExecutionError(
            "task execution failed".to_string(),
        ))
    });

    let result = engine.send_event("task_status", "start", None);
    assert!(result.is_err());
    assert_eq!(engine.get_state("task_status").unwrap(), "idle");

    let traces = engine.traces();
    // on_transition action is recorded as succeeded...
    assert!(traces.iter().any(|e| matches!(
        e,
        TraceEvent::ActionSucceeded { signal_id, action_id, .. }
            if signal_id == "task_status" && action_id == "init_task_resource"
    )));
    // ...but the state still rolled back because on_enter failed.
    assert!(traces.iter().all(|e| !matches!(
        e,
        TraceEvent::StateChanged { signal_id, .. } if signal_id == "task_status"
    )));
    assert!(traces.iter().any(|e| matches!(
        e,
        TraceEvent::ActionFailed {
            signal_id,
            action_id,
            error,
            ..
        } if signal_id == "task_status" && action_id == "start_task_execution" && error == "task execution failed"
    )));
    assert!(traces.iter().any(|e| matches!(
        e,
        TraceEvent::Rollbacked {
            signal_id,
            from,
            to,
            ..
        } if signal_id == "task_status" && from == "running" && to == "idle"
    )));
}

/// A transition with no lifecycle actions is trivially atomic: it commits.
#[test]
fn test_transaction_actionless_transition_commits() {
    let json = r#"{
        "version": "0.1",
        "signals": [
            {"id": "s", "initial_state": "a", "states": ["a", "b"]}
        ],
        "transitions": [
            {"signal_id": "s", "from": "a", "event": "go", "to": "b"}
        ]
    }"#;
    let mut engine = TopologyEngine::from_json(json).expect("Should load topology");

    let result = engine.send_event("s", "go", None);
    assert!(result.is_ok());
    assert_eq!(engine.get_state("s").unwrap(), "b");

    let traces = engine.traces();
    assert!(traces.iter().any(|e| matches!(
        e,
        TraceEvent::StateChanged { signal_id, from, to, .. }
            if signal_id == "s" && from == "a" && to == "b"
    )));
    assert!(traces.iter().all(|e| !matches!(
        e,
        TraceEvent::Rollbacked { signal_id, .. } if signal_id == "s"
    )));
}

/// Multi-level cascade: parent commits and reaction drives the child; if the
/// child's action fails the child rolls back while the parent (already
/// committed) stays. This documents per-signal atomicity — full cascade
/// transaction semantics are deferred (see roadmap v0.10 M23).
#[test]
fn test_transaction_cascade_child_failure_rolls_back_only_child() {
    let json = r#"{
        "version": "0.1",
        "signals": [
            {"id": "A", "initial_state": "a0", "states": ["a0", "a1"]},
            {"id": "B", "initial_state": "b0", "states": ["b0", "b1"]}
        ],
        "transitions": [
            {"signal_id": "A", "from": "a0", "event": "go", "to": "a1"},
            {
                "signal_id": "B",
                "from": "b0",
                "event": "process",
                "to": "b1",
                "actions": { "on_enter": ["boom"] }
            }
        ],
        "reactions": [
            {"from_signal": "A", "from_state": "a1", "to_signal": "B", "event": "process"}
        ]
    }"#;
    let mut engine = TopologyEngine::from_json(json).expect("Should load topology");
    engine.register_action("boom", |_| {
        Err(EngineError::ActionExecutionError("child boom".to_string()))
    });

    // Cascade child fails -> the whole send returns Err.
    let result = engine.send_event("A", "go", None);
    assert!(result.is_err());

    // Parent A already committed; only child B rolled back to b0.
    assert_eq!(engine.get_state("A").unwrap(), "a1");
    assert_eq!(engine.get_state("B").unwrap(), "b0");

    let traces = engine.traces();
    assert!(traces.iter().any(|e| matches!(
        e,
        TraceEvent::StateChanged { signal_id, from, to, .. }
            if signal_id == "A" && from == "a0" && to == "a1"
    )));
    assert!(traces.iter().all(|e| !matches!(
        e,
        TraceEvent::StateChanged { signal_id, .. } if signal_id == "B"
    )));
    assert!(traces.iter().any(|e| matches!(
        e,
        TraceEvent::Rollbacked { signal_id, from, to, .. }
            if signal_id == "B" && from == "b1" && to == "b0"
    )));
}
