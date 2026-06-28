use signal_topology::{EngineError, TopologyEngine};
use std::fs;
use std::path::PathBuf;

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

fn temp_state_file(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("sig_topo_test_{}_{}", name, std::process::id()));
    path
}

#[test]
fn test_save_and_load_state_roundtrip() {
    let json = load_topology();
    let mut engine = TopologyEngine::from_json(&json).expect("Should load valid topology");
    register_noop_actions(&mut engine);

    engine
        .send_event("task_status", "start", None)
        .expect("Transition should succeed");
    assert_eq!(engine.get_state("task_status").unwrap(), "running");

    let state_path = temp_state_file("roundtrip");
    engine.save_state(&state_path).expect("Should save state");

    let mut engine2 = TopologyEngine::from_json(&json).expect("Should load valid topology");
    engine2.load_state(&state_path).expect("Should load state");

    assert_eq!(engine2.get_state("task_status").unwrap(), "running");

    let _ = fs::remove_file(&state_path);
}

#[test]
fn test_load_state_rejects_invalid_state() {
    let json = load_topology();
    let mut engine = TopologyEngine::from_json(&json).expect("Should load valid topology");
    register_noop_actions(&mut engine);

    engine
        .send_event("task_status", "start", None)
        .expect("Transition should succeed");

    let state_path = temp_state_file("invalid_state");
    engine.save_state(&state_path).expect("Should save state");

    let corrupted = r#"{"states":{"task_status":"nonexistent_state"}}"#;
    fs::write(&state_path, corrupted).expect("Should write corrupted state");

    let result = engine.load_state(&state_path);
    assert!(matches!(
        result,
        Err(EngineError::StateNotFound { signal, state })
            if signal == "task_status" && state == "nonexistent_state"
    ));

    let _ = fs::remove_file(&state_path);
}

#[test]
fn test_load_state_rejects_unknown_signal() {
    let json = load_topology();
    let mut engine = TopologyEngine::from_json(&json).expect("Should load valid topology");

    let state_path = temp_state_file("unknown_signal");
    let corrupted = r#"{"states":{"unknown_signal":"idle"}}"#;
    fs::write(&state_path, corrupted).expect("Should write corrupted state");

    let result = engine.load_state(&state_path);
    assert!(matches!(result, Err(EngineError::PersistenceError(_))));

    let _ = fs::remove_file(&state_path);
}

#[test]
fn test_reload_topology_keeps_existing_signal_states() {
    let json = load_topology();
    let mut engine = TopologyEngine::from_json(&json).expect("Should load valid topology");
    register_noop_actions(&mut engine);

    engine
        .send_event("task_status", "start", None)
        .expect("Transition should succeed");
    assert_eq!(engine.get_state("task_status").unwrap(), "running");

    engine.reload_topology(&json).expect("Should reload same topology");
    assert_eq!(engine.get_state("task_status").unwrap(), "running");
}

#[test]
fn test_reload_topology_adds_new_signals_with_initial_state() {
    let json = load_topology();
    let mut engine = TopologyEngine::from_json(&json).expect("Should load valid topology");
    register_noop_actions(&mut engine);

    engine
        .send_event("task_status", "start", None)
        .expect("Transition should succeed");

    let new_json = r#"{
        "version": "0.1",
        "signals": [
            {"id": "task_status", "initial_state": "idle", "states": ["idle", "running", "success", "failed"]},
            {"id": "new_signal", "initial_state": "init", "states": ["init", "done"]}
        ],
        "transitions": []
    }"#;

    engine
        .reload_topology(new_json)
        .expect("Should reload topology with new signal");

    assert_eq!(engine.get_state("task_status").unwrap(), "running");
    assert_eq!(engine.get_state("new_signal").unwrap(), "init");
}

#[test]
fn test_reload_topology_drops_removed_signals() {
    let json = r#"{
        "version": "0.1",
        "signals": [
            {"id": "s1", "initial_state": "a", "states": ["a", "b"]},
            {"id": "s2", "initial_state": "x", "states": ["x", "y"]}
        ],
        "transitions": []
    }"#;
    let mut engine = TopologyEngine::from_json(json).expect("Should load valid topology");

    let new_json = r#"{
        "version": "0.1",
        "signals": [
            {"id": "s1", "initial_state": "a", "states": ["a", "b"]}
        ],
        "transitions": []
    }"#;

    engine
        .reload_topology(new_json)
        .expect("Should reload topology without s2");

    assert!(engine.get_state("s2").is_err());
    assert!(engine.signal_ids().iter().all(|id| *id != "s2"));
}

#[test]
fn test_reload_topology_rejects_invalid_topology() {
    let json = load_topology();
    let mut engine = TopologyEngine::from_json(&json).expect("Should load valid topology");

    let invalid_json = r#"{"version":"0.1","signals":[{"id":"s1","initial_state":"invalid","states":["a"]}],"transitions":[]}"#;
    let result = engine.reload_topology(invalid_json);
    assert!(matches!(result, Err(EngineError::ReloadError(_))));
}

#[test]
fn test_save_and_load_empty_state() {
    let json = r#"{
        "version": "0.1",
        "signals": [],
        "transitions": []
    }"#;
    let engine = TopologyEngine::from_json(json).expect("Should load valid topology");

    let state_path = temp_state_file("empty_state");
    engine.save_state(&state_path).expect("Should save empty state");

    let saved = fs::read_to_string(&state_path).expect("Should read saved state");
    assert_eq!(saved, "{\n  \"states\": {}\n}");

    let mut engine2 = TopologyEngine::from_json(json).expect("Should load valid topology");
    engine2.load_state(&state_path).expect("Should load empty state");
    assert!(engine2.signal_ids().is_empty());

    let _ = fs::remove_file(&state_path);
}

#[test]
fn test_reload_empty_topology() {
    let json = r#"{
        "version": "0.1",
        "signals": [
            {"id": "s1", "initial_state": "a", "states": ["a", "b"]}
        ],
        "transitions": []
    }"#;
    let mut engine = TopologyEngine::from_json(json).expect("Should load valid topology");
    assert_eq!(engine.get_state("s1").unwrap(), "a");

    let empty_json = r#"{
        "version": "0.1",
        "signals": [],
        "transitions": []
    }"#;

    engine
        .reload_topology(empty_json)
        .expect("Should reload empty topology");

    assert!(engine.get_state("s1").is_err());
    assert!(engine.signal_ids().is_empty());
}
