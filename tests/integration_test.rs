use signal_topology::{EngineError, TopologyEngine};
use std::fs;

fn load_topology() -> String {
    fs::read_to_string("tests/topology.json").expect("Failed to read topology.json")
}

#[test]
fn test_valid_topology_loads_and_initial_state_is_correct() {
    let json = load_topology();
    let engine = TopologyEngine::from_json(&json).expect("Should load valid topology");
    assert_eq!(engine.get_state("task_status").unwrap(), "idle");
}

#[test]
fn test_event_triggers_transition_and_actions_execute_in_order() {
    let json = load_topology();
    let mut engine = TopologyEngine::from_json(&json).expect("Should load valid topology");
    
    engine.register_action("log_idle_leave", |_| Ok(()));
    engine.register_action("init_task_resource", |_| Ok(()));
    engine.register_action("start_task_execution", |_| Ok(()));
    engine.register_action("notify_task_success", |_| Ok(()));
    engine.register_action("record_task_error", |_| Ok(()));
    engine.register_action("clear_task_data", |_| Ok(()));
    
    let result = engine.send_event("task_status", "start", None).expect("Transition should succeed");
    assert_eq!(result.from, "idle");
    assert_eq!(result.to, "running");
    assert_eq!(result.executed_actions, vec!["log_idle_leave", "init_task_resource", "start_task_execution"]);
    assert_eq!(engine.get_state("task_status").unwrap(), "running");
    
    let result = engine.send_event("task_status", "finish", None).expect("Transition should succeed");
    assert_eq!(result.from, "running");
    assert_eq!(result.to, "success");
    assert_eq!(result.executed_actions, vec!["notify_task_success"]);
    assert_eq!(engine.get_state("task_status").unwrap(), "success");
}

#[test]
fn test_unmatched_event_returns_error_and_state_unchanged() {
    let json = load_topology();
    let mut engine = TopologyEngine::from_json(&json).expect("Should load valid topology");
    
    let result = engine.send_event("task_status", "unknown_event", None);
    assert!(matches!(result, Err(EngineError::TransitionNotFound { .. })));
    assert_eq!(engine.get_state("task_status").unwrap(), "idle");
}

#[test]
fn test_invalid_topology_fails_at_load() {
    // Test: duplicate signal id
    let json = r#"{"version":"0.1","signals":[{"id":"s1","initial_state":"a","states":["a"]},{"id":"s1","initial_state":"a","states":["a"]}],"transitions":[]}"#;
    assert!(matches!(TopologyEngine::from_json(json), Err(EngineError::ValidationError(_))));
    
    // Test: invalid initial state
    let json = r#"{"version":"0.1","signals":[{"id":"s1","initial_state":"invalid","states":["a"]}],"transitions":[]}"#;
    assert!(matches!(TopologyEngine::from_json(json), Err(EngineError::ValidationError(_))));
    
    // Test: unknown transition signal AND unknown transition state
    let json = r#"{"version":"0.1","signals":[{"id":"s1","initial_state":"a","states":["a"]}],"transitions":[{"signal_id":"unknown","from":"a","event":"e","to":"unknown","actions":{}}]}"#;
    assert!(matches!(TopologyEngine::from_json(json), Err(EngineError::ValidationError(_))));
}

#[test]
fn test_json_only_rule_change_works_without_code_change() {
    let json = load_topology();
    let mut engine = TopologyEngine::from_json(&json).expect("Should load valid topology");
    
    engine.register_action("log_idle_leave", |_| Ok(()));
    engine.register_action("init_task_resource", |_| Ok(()));
    engine.register_action("start_task_execution", |_| Ok(()));
    engine.register_action("notify_task_success", |_| Ok(()));
    engine.register_action("record_task_error", |_| Ok(()));
    engine.register_action("clear_task_data", |_| Ok(()));
    
    let result = engine.send_event("task_status", "start", None).expect("Transition should succeed");
    assert_eq!(result.to, "running");
    
    let result = engine.send_event("task_status", "reset", None).expect("Transition should succeed");
    assert_eq!(result.from, "running");
    assert_eq!(result.to, "idle");
    assert_eq!(result.executed_actions, vec!["clear_task_data"]);
}