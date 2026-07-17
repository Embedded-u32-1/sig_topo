//! M41: end-to-end test for the `sts` `why` command.
//!
//! The command's visible behavior is a pure function of the engine's trace log
//! (`run::format_why`), so this test drives a real guard-bearing scenario to
//! produce `ReactionGuardEvaluated` events and asserts the formatted report.
//! It mirrors how a user would use the REPL: load a guarded topology, send an
//! event that fires a reaction, then "why" that reaction to read the guard's
//! trace. The `sts` bin itself is not invoked (its `main()` is untestable IO);
//! the focus is the guard-trace --> report path the REPL's `why` prints.

use signal_topology::run::{collect_action_ids, format_why, register_actions};
use signal_topology::TopologyEngine;
use std::cell::RefCell;
use std::collections::HashSet;
use std::path::Path;
use std::rc::Rc;

/// Compile the M38 guard_template `.ddl` into a ready-to-run engine. Every
/// action id is registered with a no-op handler (the scenario never injects a
/// failure); `record` is off because the action lines are irrelevant here — we
/// read the *guard* trace, not the action trace.
fn build_guard_template_engine(ddl_path: &Path) -> TopologyEngine {
    let src = std::fs::read_to_string(ddl_path).expect("guard_template.ddl should be readable");
    let schema = signal_topology::ddl::compile(&src).expect("guard_template.ddl should compile");
    let action_ids = collect_action_ids(&schema);
    let mut engine = TopologyEngine::from_schema(schema).expect("guard_template schema should build");
    register_actions(&mut engine, &action_ids, Some(Rc::new(RefCell::new(HashSet::new()))), false);
    engine
}

/// Guard-template scenario (M38): one top-level `guard allow_alloc` referenced
/// by two reactions. Drive `order` through `pending -> approved` twice — once
/// with `auto: true` (reaction fires) and once with `auto: false` (reaction
/// skipped) — then `why order approved inventory allocate` should report two
/// evaluations, one fired and one skipped, both showing the guard expression.
#[test]
fn why_reaction_shows_guard_trace_with_fired_and_skipped() {
    let mut engine = build_guard_template_engine(Path::new(
        "examples/scenarios/guard_template/guard_template.ddl",
    ));

    // First approve with auto: true — both reactions (inventory, audit) fire.
    engine
        .send_event("order", "approve", Some(serde_json::json!({"auto": true})))
        .expect("approve with auto=true should commit");
    assert_eq!(engine.get_state("order").unwrap(), "approved");
    assert_eq!(engine.get_state("inventory").unwrap(), "allocated");

    // Reset back to pending so we can re-approve with auto: false.
    engine
        .send_event("order", "reset", None)
        .expect("reset should commit");
    assert_eq!(engine.get_state("order").unwrap(), "pending");

    // Second approve with auto: false — the guard blocks both reactions. The
    // base transition still commits (order is now approved again), but neither
    // reaction fires. `inventory` stays whatever it was (a skipped reaction
    // does not move it) — it is still `allocated` from the first approve.
    engine
        .send_event("order", "approve", Some(serde_json::json!({"auto": false})))
        .expect("approve with auto=false should still commit the base transition");
    assert_eq!(engine.get_state("order").unwrap(), "approved");
    assert_eq!(engine.get_state("inventory").unwrap(), "allocated");

    // The `why` report for the inventory reaction must surface the guard
    // expression and one true + one false result.
    let report = format_why(engine.traces(), "order", "approved", "inventory", "allocate");
    assert!(
        report.contains("Guard evaluations for reaction order.approved -> inventory.allocate:"),
        "report should name the reaction, got:\n{}",
        report
    );
    assert!(
        report.contains("guard=`payload.auto == true`"),
        "report should show the guard expression, got:\n{}",
        report
    );
    assert!(
        report.contains("result=true"),
        "report should record the fired (true) evaluation, got:\n{}",
        report
    );
    assert!(
        report.contains("result=false"),
        "report should record the skipped (false) evaluation, got:\n{}",
        report
    );
    assert!(
        report.contains("2 evaluation(s); 1 fired, 1 skipped."),
        "report should summarize counts, got:\n{}",
        report
    );

    // The sibling audit reaction fired once (auto true) and was skipped once
    // (auto false) — a guard both reactions share.
    let audit_report = format_why(engine.traces(), "order", "approved", "audit", "note");
    assert!(
        audit_report.contains("2 evaluation(s); 1 fired, 1 skipped."),
        "audit reaction shares the guard, expected 2 evals, got:\n{}",
        audit_report
    );
}

/// A reaction that was never evaluated (its `from_state` was never reached)
/// produces a friendly "not found" message rather than an empty table. Here
/// `order` never reaches `shipped`, so the reaction `order.shipped -> ...`
/// has no guard trace at all.
#[test]
fn why_unevaluated_reaction_says_not_found() {
    let engine = build_guard_template_engine(Path::new(
        "examples/scenarios/guard_template/guard_template.ddl",
    ));

    let report = format_why(engine.traces(), "order", "shipped", "inventory", "allocate");
    assert_eq!(
        report,
        "No guard evaluation found for that reaction (from_state may not have been reached yet)."
    );
}
