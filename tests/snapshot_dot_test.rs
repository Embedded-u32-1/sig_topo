use signal_topology::TopologyEngine;

// A tiny two-signal topology with no actions, so transitions commit without
// any registered handlers. Keeps the tests focused on rendering, not action
// lifecycle.
const TOPOLOGY: &str = r#"{
  "version": "0.1",
  "signals": [
    { "id": "order", "initial_state": "draft", "states": ["draft", "submitted", "shipped"] },
    { "id": "payment", "initial_state": "pending", "states": ["pending", "paid", "refunded"] }
  ],
  "transitions": [
    { "signal_id": "order", "from": "draft", "event": "submit", "to": "submitted" },
    { "signal_id": "order", "from": "submitted", "event": "ship", "to": "shipped" },
    { "signal_id": "payment", "from": "pending", "event": "charge", "to": "paid" },
    { "signal_id": "payment", "from": "paid", "event": "refund", "to": "refunded" }
  ]
}"#;

fn build_engine() -> TopologyEngine {
    TopologyEngine::from_json(TOPOLOGY).expect("topology should parse and validate")
}

#[test]
fn snapshot_dot_highlights_initial_state_on_fresh_engine() {
    let engine = build_engine();
    let dot = engine.snapshot_dot();

    // On a fresh engine, current == initial for every signal, so the runtime
    // highlight (lightgreen) wins over the static lightblue marker.
    assert!(dot.contains(
        "n_order_draft [label=\"draft\" style=filled fillcolor=lightgreen penwidth=2]"
    ));
    assert!(dot.contains(
        "n_payment_pending [label=\"pending\" style=filled fillcolor=lightgreen penwidth=2]"
    ));
    assert!(!dot.contains("lightblue"));
}

#[test]
fn snapshot_dot_highlight_follows_transitions() {
    let mut engine = build_engine();

    // Move `order` to 'submitted'. Now its current state is highlighted
    // lightgreen while the initial 'draft' falls back to lightblue.
    engine
        .send_event("order", "submit", None)
        .expect("submit should commit");
    assert_eq!(engine.get_state("order").unwrap(), "submitted");

    let dot = engine.snapshot_dot();
    assert!(dot.contains(
        "n_order_submitted [label=\"submitted\" style=filled fillcolor=lightgreen penwidth=2]"
    ));
    // Initial state, now different from current, shows the static marker.
    assert!(dot
        .contains("n_order_draft [label=\"draft\" style=filled fillcolor=lightblue]"));
    // A state that is neither current nor initial stays unhighlighted.
    assert!(dot.contains("n_order_shipped [label=\"shipped\"]"));
    assert!(!dot.contains("n_order_shipped [label=\"shipped\" style="));
}

#[test]
fn snapshot_dot_is_per_signal_and_updates_after_each_move() {
    let mut engine = build_engine();

    // Advance both signals one step.
    engine.send_event("order", "submit", None).unwrap();
    engine.send_event("payment", "charge", None).unwrap();

    let dot = engine.snapshot_dot();
    assert!(dot.contains(
        "n_order_submitted [label=\"submitted\" style=filled fillcolor=lightgreen penwidth=2]"
    ));
    assert!(dot.contains(
        "n_payment_paid [label=\"paid\" style=filled fillcolor=lightgreen penwidth=2]"
    ));
    // Both initial states are now static-only.
    assert!(dot.contains("n_order_draft [label=\"draft\" style=filled fillcolor=lightblue]"));
    assert!(dot
        .contains("n_payment_pending [label=\"pending\" style=filled fillcolor=lightblue]"));

    // Advance `order` again; the highlight should move to 'shipped' while
    // `payment` stays on 'paid'.
    engine.send_event("order", "ship", None).unwrap();
    let dot = engine.snapshot_dot();
    assert!(dot.contains(
        "n_order_shipped [label=\"shipped\" style=filled fillcolor=lightgreen penwidth=2]"
    ));
    assert!(dot.contains(
        "n_payment_paid [label=\"paid\" style=filled fillcolor=lightgreen penwidth=2]"
    ));
    // 'submitted' is now neither current nor initial -> no highlight.
    assert!(dot.contains("n_order_submitted [label=\"submitted\"]"));
    assert!(!dot.contains("n_order_submitted [label=\"submitted\" style="));
}

#[test]
fn snapshot_dot_is_valid_digraph() {
    let engine = build_engine();
    let dot = engine.snapshot_dot();

    assert!(dot.starts_with("digraph Topology {"));
    assert!(dot.ends_with("}\n"));
    assert!(dot.contains("subgraph cluster_order"));
    assert!(dot.contains("subgraph cluster_payment"));
}
