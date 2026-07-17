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

// A four-signal topology: driving `A.go` commits A to `a1` and triggers three
// reactions keyed on `A` entering `a1`. Their three guards evaluate to
// `true`, `false`, and `error` against the same payload, so the extended DOT
// exercises every guard-result color on distinct edges in one snapshot.
const GUARDED_CASCADE: &str = r#"{
  "version": "0.1",
  "signals": [
    { "id": "A", "initial_state": "a0", "states": ["a0", "a1"] },
    { "id": "B", "initial_state": "b0", "states": ["b0", "b1"] },
    { "id": "C", "initial_state": "c0", "states": ["c0", "c1"] },
    { "id": "D", "initial_state": "d0", "states": ["d0", "d1"] }
  ],
  "transitions": [
    { "signal_id": "A", "from": "a0", "event": "go", "to": "a1" },
    { "signal_id": "B", "from": "b0", "event": "react", "to": "b1" },
    { "signal_id": "C", "from": "c0", "event": "react", "to": "c1" },
    { "signal_id": "D", "from": "d0", "event": "react", "to": "d1" }
  ],
  "reactions": [
    { "from_signal": "A", "from_state": "a1", "to_signal": "B", "event": "react",
      "guard": "payload.enable == true" },
    { "from_signal": "A", "from_state": "a1", "to_signal": "C", "event": "react",
      "guard": "payload.enable == false" },
    { "from_signal": "A", "from_state": "a1", "to_signal": "D", "event": "react",
      "guard": "payload.x + \"s\"" }
  ]
}"#;

#[test]
fn snapshot_dot_extended_includes_guard_edges() {
    let mut engine =
        TopologyEngine::from_json(GUARDED_CASCADE).expect("topology should parse and validate");

    // `enable == true` -> B's reaction fires (guard result `true`);
    // `enable == false` -> C's reaction is blocked (guard result `false`);
    // `payload.x + "s"` -> x is absent => Null; Null + "s" errors (result
    // `error: ...`). The main transition still commits.
    engine
        .send_event("A", "go", Some(serde_json::json!({ "enable": true })))
        .expect("A.go should commit");
    assert_eq!(engine.get_state("A").unwrap(), "a1");

    let dot = engine.snapshot_dot_extended();

    // guard=true  -> solid green.
    assert!(
        dot.contains(
            "n_A_a1 -> n_B_b0 [label=\"react [guard: true]\" color=green style=solid]"
        ),
        "guard=true edge should be solid green; got:\n{}",
        dot
    );
    // guard=false -> dashed gray.
    assert!(
        dot.contains(
            "n_A_a1 -> n_C_c0 [label=\"react [guard: false]\" color=gray style=dashed]"
        ),
        "guard=false edge should be dashed gray; got:\n{}",
        dot
    );
    // guard=error -> dashed red. The exact error text is an implementation detail
    // of the guard evaluator, so assert the edge id and color rather than the
    // message.
    assert!(
        dot.contains("n_A_a1 -> n_D_d0") && dot.contains("color=red style=dashed"),
        "guard=error edge should be dashed red; got:\n{}",
        dot
    );

    // Live state highlight is preserved on top of the reaction edges.
    assert!(dot.contains(
        "n_A_a1 [label=\"a1\" style=filled fillcolor=lightgreen penwidth=2]"
    ));
    assert!(dot.starts_with("digraph Topology {"));
    assert!(dot.ends_with("}\n"));
}
