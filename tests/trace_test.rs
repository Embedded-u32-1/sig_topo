use signal_topology::{EngineError, TopologyEngine, TraceEvent};
use std::fs;

fn load_topology() -> String {
    fs::read_to_string("tests/topology.json").expect("Failed to read topology.json")
}

fn register_noop_actions(engine: &mut TopologyEngine) {
    for action_id in [
        "log_idle_leave",
        "init_task_resource",
        "start_task_execution",
        "notify_task_success",
        "record_task_error",
        "clear_task_data",
    ] {
        engine.register_action(action_id, |_| Ok(()));
    }
}

#[test]
fn test_trace_records_event_received_and_state_changed() {
    let json = load_topology();
    let mut engine = TopologyEngine::from_json(&json).expect("Should load valid topology");
    register_noop_actions(&mut engine);

    engine
        .send_event("task_status", "start", None)
        .expect("Transition should succeed");

    let traces = engine.traces();
    assert!(traces.iter().any(|e| matches!(
        e,
        TraceEvent::EventReceived {
            signal_id,
            event,
            payload,
            ..
        } if signal_id == "task_status" && event == "start" && payload.is_none()
    )));
    assert!(traces.iter().any(|e| matches!(
        e,
        TraceEvent::StateChanged {
            signal_id,
            from,
            to,
            ..
        } if signal_id == "task_status" && from == "idle" && to == "running"
    )));
}

#[test]
fn test_trace_records_action_lifecycle() {
    let json = load_topology();
    let mut engine = TopologyEngine::from_json(&json).expect("Should load valid topology");
    register_noop_actions(&mut engine);

    engine
        .send_event("task_status", "start", None)
        .expect("Transition should succeed");

    let traces = engine.traces();
    for action_id in ["log_idle_leave", "init_task_resource", "start_task_execution"] {
        assert!(traces.iter().any(|e| matches!(
            e,
            TraceEvent::ActionStarted {
                signal_id,
                action_id: id,
                ..
            } if signal_id == "task_status" && id == action_id
        )));
        assert!(traces.iter().any(|e| matches!(
            e,
            TraceEvent::ActionSucceeded {
                signal_id,
                action_id: id,
                ..
            } if signal_id == "task_status" && id == action_id
        )));
    }
}

#[test]
fn test_trace_records_action_failure() {
    let json = load_topology();
    let mut engine = TopologyEngine::from_json(&json).expect("Should load valid topology");

    engine.register_action("log_idle_leave", |_| Ok(()));
    engine.register_action("init_task_resource", |_| {
        Err(EngineError::ActionExecutionError("resource init failed".to_string()))
    });
    engine.register_action("start_task_execution", |_| Ok(()));

    let result = engine.send_event("task_status", "start", None);
    assert!(result.is_err());

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
    assert!(traces.iter().any(|e| matches!(
        e,
        TraceEvent::ActionFailed {
            signal_id,
            action_id,
            error,
            ..
        } if signal_id == "task_status" && action_id == "init_task_resource" && error == "resource init failed"
    )));
    assert_eq!(engine.get_state("task_status").unwrap(), "running");
}

#[test]
fn test_trace_filter_by_signal() {
    let json = r#"{
        "version": "0.1",
        "signals": [
            {"id": "s1", "initial_state": "a", "states": ["a", "b"]},
            {"id": "s2", "initial_state": "x", "states": ["x", "y"]}
        ],
        "transitions": [
            {"signal_id": "s1", "from": "a", "event": "go", "to": "b"},
            {"signal_id": "s2", "from": "x", "event": "go", "to": "y"}
        ]
    }"#;
    let mut engine = TopologyEngine::from_json(json).expect("Should load valid topology");

    engine.send_event("s1", "go", None).unwrap();
    engine.send_event("s2", "go", None).unwrap();

    let s1_traces = engine.traces_for("s1");
    assert!(s1_traces.iter().all(|e| e.signal_id() == "s1"));
    assert!(s1_traces
        .iter()
        .any(|e| matches!(e, TraceEvent::StateChanged { signal_id, .. } if signal_id == "s1")));
    assert!(!s1_traces
        .iter()
        .any(|e| matches!(e, TraceEvent::StateChanged { signal_id, .. } if signal_id == "s2")));
}

#[test]
fn test_trace_clear() {
    let json = load_topology();
    let mut engine = TopologyEngine::from_json(&json).expect("Should load valid topology");
    register_noop_actions(&mut engine);

    engine
        .send_event("task_status", "start", None)
        .expect("Transition should succeed");
    assert!(!engine.traces().is_empty());

    engine.clear_traces();
    assert!(engine.traces().is_empty());
}
