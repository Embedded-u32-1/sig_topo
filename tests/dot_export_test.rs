use signal_topology::export::to_dot;
use signal_topology::schema::TopologySchema;
use std::fs;

fn load_schema() -> TopologySchema {
    let json = fs::read_to_string("tests/topology.json").expect("Failed to read topology.json");
    serde_json::from_str(&json).expect("Failed to parse topology.json")
}

#[test]
fn test_dot_contains_state_nodes() {
    let schema = load_schema();
    let dot = to_dot(&schema);

    assert!(dot.contains("task_status_idle [label=\"idle\" style=filled fillcolor=lightblue]"));
    assert!(dot.contains("task_status_running [label=\"running\"]"));
    assert!(dot.contains("task_status_success [label=\"success\"]"));
    assert!(dot.contains("task_status_failed [label=\"failed\"]"));
}

#[test]
fn test_dot_contains_expected_edges() {
    let schema = load_schema();
    let dot = to_dot(&schema);

    assert!(dot.contains("task_status_idle -> task_status_running"));
    assert!(dot.contains("task_status_running -> task_status_success"));
    assert!(dot.contains("task_status_running -> task_status_failed"));
}

#[test]
fn test_dot_edge_labels_include_events_and_actions() {
    let schema = load_schema();
    let dot = to_dot(&schema);

    assert!(
        dot.contains("label=\"start [log_idle_leave, init_task_resource, start_task_execution]\"")
    );
    assert!(dot.contains("label=\"finish [notify_task_success]\""));
    assert!(dot.contains("label=\"error [record_task_error]\""));
}

#[test]
fn test_dot_wildcard_transition_expands_to_all_states() {
    let schema = load_schema();
    let dot = to_dot(&schema);

    // Wildcard '*' should produce edges from every state except the target ('idle')
    assert!(dot.contains("task_status_running -> task_status_idle"));
    assert!(dot.contains("task_status_success -> task_status_idle"));
    assert!(dot.contains("task_status_failed -> task_status_idle"));

    // 'idle -> idle' should not be emitted for the wildcard reset transition
    assert!(!dot.contains("task_status_idle -> task_status_idle"));
}

#[test]
fn test_dot_initial_state_has_visual_distinction() {
    let schema = load_schema();
    let dot = to_dot(&schema);

    assert!(dot.contains("fillcolor=lightblue"));
}

#[test]
fn test_dot_is_valid_digraph() {
    let schema = load_schema();
    let dot = to_dot(&schema);

    assert!(dot.starts_with("digraph Topology {"));
    assert!(dot.ends_with("}\n"));
    assert!(dot.contains("subgraph cluster_task_status"));
}
