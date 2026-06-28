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

    assert!(dot.contains("n_task_status_idle [label=\"idle\" style=filled fillcolor=lightblue]"));
    assert!(dot.contains("n_task_status_running [label=\"running\"]"));
    assert!(dot.contains("n_task_status_success [label=\"success\"]"));
    assert!(dot.contains("n_task_status_failed [label=\"failed\"]"));
}

#[test]
fn test_dot_contains_expected_edges() {
    let schema = load_schema();
    let dot = to_dot(&schema);

    assert!(dot.contains("n_task_status_idle -> n_task_status_running"));
    assert!(dot.contains("n_task_status_running -> n_task_status_success"));
    assert!(dot.contains("n_task_status_running -> n_task_status_failed"));
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
    assert!(dot.contains("n_task_status_running -> n_task_status_idle"));
    assert!(dot.contains("n_task_status_success -> n_task_status_idle"));
    assert!(dot.contains("n_task_status_failed -> n_task_status_idle"));

    // 'idle -> idle' should not be emitted for the wildcard reset transition
    assert!(!dot.contains("n_task_status_idle -> n_task_status_idle"));
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

#[test]
fn test_dot_empty_topology_renders_valid_digraph() {
    let schema: TopologySchema = serde_json::from_str(r#"{"version":"0.1","signals":[],"transitions":[]}"#).unwrap();
    let dot = to_dot(&schema);

    assert!(dot.starts_with("digraph Topology {"));
    assert!(dot.ends_with("}\n"));
}

#[test]
fn test_dot_special_characters_in_ids_are_escaped_and_sanitized() {
    let json = r#"{
      "version": "0.1",
      "signals": [
        {
          "id": "sig-with\"quote",
          "initial_state": "st\\back",
          "states": ["st\\back", "state-with-dash"]
        }
      ],
      "transitions": [
        {
          "signal_id": "sig-with\"quote",
          "from": "st\\back",
          "event": "ev\"ent",
          "to": "state-with-dash"
        }
      ]
    }"#;
    let schema: TopologySchema = serde_json::from_str(json).unwrap();
    let dot = to_dot(&schema);

    assert!(dot.contains("subgraph cluster_sig_with_quote"));
    assert!(dot.contains("n_sig_with_quote_st_back [label=\"st\\\\back\" style=filled fillcolor=lightblue]"));
    assert!(dot.contains("n_sig_with_quote_state_with_dash [label=\"state-with-dash\"]"));
    assert!(dot.contains("n_sig_with_quote_st_back -> n_sig_with_quote_state_with_dash"));
    assert!(dot.contains("label=\"ev\\\"ent\""));
}

#[test]
fn test_dot_transition_with_no_actions_has_event_only_label() {
    let json = r#"{
      "version": "0.1",
      "signals": [
        {
          "id": "s",
          "initial_state": "a",
          "states": ["a", "b"]
        }
      ],
      "transitions": [
        {
          "signal_id": "s",
          "from": "a",
          "event": "go",
          "to": "b"
        }
      ]
    }"#;
    let schema: TopologySchema = serde_json::from_str(json).unwrap();
    let dot = to_dot(&schema);

    assert!(dot.contains("label=\"go\""));
}

#[test]
fn test_dot_numeric_signal_id_produces_valid_node_ids() {
    let json = r#"{
      "version": "0.1",
      "signals": [
        {
          "id": "123sig",
          "initial_state": "1a",
          "states": ["1a", "2b"]
        }
      ],
      "transitions": [
        {
          "signal_id": "123sig",
          "from": "*",
          "event": "tick",
          "to": "2b"
        }
      ]
    }"#;
    let schema: TopologySchema = serde_json::from_str(json).unwrap();
    let dot = to_dot(&schema);

    assert!(dot.contains("n_123sig_1a [label=\"1a\" style=filled fillcolor=lightblue]"));
    assert!(dot.contains("n_123sig_2b [label=\"2b\"]"));
    assert!(dot.contains("n_123sig_1a -> n_123sig_2b"));
}
