//! M28: DDL end-to-end integration tests.
//!
//! Compiles `.ddl` fixtures through `ddl::compile`, feeds the resulting
//! `TopologySchema` to the unmodified `TopologyEngine`, and asserts the same
//! behaviour as the equivalent JSON fixtures. This pins down the "DDL is a
//! front-end to the engine, engine layer untouched" contract.

use signal_topology::{EngineError, TopologyEngine};
use std::fs;

/// Compile a DDL source string straight into a runnable engine.
fn engine_from_ddl(src: &str) -> TopologyEngine {
    let schema = signal_topology::ddl::compile(src).expect("DDL should compile");
    TopologyEngine::from_schema(schema).expect("schema should build an engine")
}

#[test]
fn order_approval_ddl_end_to_end_reaches_shipped() {
    let src = fs::read_to_string("examples/order_approval.ddl")
        .expect("order_approval.ddl should exist");
    let mut engine = engine_from_ddl(&src);

    for action in [
        "log_draft_exit",
        "validate_order_payload",
        "notify_submitted",
        "reserve_inventory",
        "notify_customer_approved",
        "dispatch_order",
        "notify_shipped",
    ] {
        engine.register_action(action, |_| Ok(()));
    }

    engine
        .send_event("order", "submit", None)
        .expect("submit should succeed");
    assert_eq!(engine.get_state("order").unwrap(), "submitted");

    engine
        .send_event(
            "order",
            "approve",
            Some(serde_json::json!({"amount": 5000})),
        )
        .expect("approve with amount=5000 should pass the guard");
    assert_eq!(engine.get_state("order").unwrap(), "approved");

    engine
        .send_event("order", "ship", None)
        .expect("ship should succeed");
    assert_eq!(engine.get_state("order").unwrap(), "shipped");
}

#[test]
fn order_approval_ddl_guard_blocks_invalid_amount() {
    let src = fs::read_to_string("examples/order_approval.ddl")
        .expect("order_approval.ddl should exist");
    let mut engine = engine_from_ddl(&src);
    // Register every action `submit` runs, so the transition completes and the
    // only thing under test is the `approve` guard.
    for action in [
        "log_draft_exit",
        "validate_order_payload",
        "notify_submitted",
        "reserve_inventory",
    ] {
        engine.register_action(action, |_| Ok(()));
    }

    engine
        .send_event("order", "submit", None)
        .expect("submit should succeed");

    // amount == 0 violates `payload.amount > 0` -> guard blocks, state unchanged.
    let result = engine.send_event(
        "order",
        "approve",
        Some(serde_json::json!({"amount": 0})),
    );
    assert!(
        matches!(result, Err(EngineError::GuardBlocked { .. })),
        "approve(amount=0) should be guard-blocked, got {:?}",
        result
    );
    assert_eq!(
        engine.get_state("order").unwrap(),
        "submitted",
        "state must not change on guard block"
    );

    // amount above the cap also violates the guard.
    let result = engine.send_event(
        "order",
        "approve",
        Some(serde_json::json!({"amount": 100_001})),
    );
    assert!(matches!(result, Err(EngineError::GuardBlocked { .. })));
}

#[test]
fn reaction_cascade_end_to_end() {
    // When `order` enters `approved`, a reaction kicks `inventory` into
    // motion. Verifies the DDL `reaction { ... }` maps to a working cascade.
    let src = r#"
signal order {
    states: [submitted, approved]
    initial: submitted

    on approve from submitted -> approved
}

signal inventory {
    states: [idle, allocating]
    initial: idle

    on allocate from idle -> allocating
}

reaction {
    when order enters approved -> inventory allocate
}
"#;

    let mut engine = engine_from_ddl(src);
    engine.register_action("noop", |_| Ok(()));

    engine
        .send_event("order", "approve", None)
        .expect("approve should succeed and trigger cascade");

    assert_eq!(engine.get_state("order").unwrap(), "approved");
    assert_eq!(
        engine.get_state("inventory").unwrap(),
        "allocating",
        "reaction should have cascaded into inventory"
    );
}

#[test]
fn reaction_guard_is_rejected_at_compile_time() {
    // The engine's `ReactionDef` carries no guard field and cascade matching
    // does not evaluate one, so a reaction guard cannot be enforced. The
    // compiler rejects it (rather than silently dropping it) and points the
    // user at transition guards instead.
    let src = r#"
signal order {
    states: [submitted, approved]
    initial: submitted

    on approve from submitted -> approved
}

signal inventory {
    states: [idle, allocating]
    initial: idle

    on allocate from idle -> allocating
}

reaction {
    when order enters approved -> inventory allocate when payload.auto
}
"#;

    let err = signal_topology::ddl::compile(src).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("reaction guards are not supported"),
        "expected a clear unsupported-guard error, got: {}",
        msg
    );
}

#[test]
fn syntax_error_reports_line_number() {
    let src = r#"
signal s {
    states: [a, b]
    initial: a

    on go from a b
}
"#;
    let err = signal_topology::ddl::compile(src).unwrap_err();
    let msg = err.to_string();
    // The missing `->` is on line 6 of the source.
    assert!(
        msg.contains("line 6"),
        "error should point at line 6, got: {}",
        msg
    );
}

#[test]
fn duplicate_signal_reports_error() {
    let src = r#"
signal dup {
    states: [a]
    initial: a
}
signal dup {
    states: [b]
    initial: b
}
"#;
    let err = signal_topology::ddl::compile(src).unwrap_err();
    assert!(
        err.to_string().contains("duplicate signal"),
        "got: {}",
        err
    );
}
