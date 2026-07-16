//! M22 example scenarios, elevated to end-to-end tests so the `EXPECTED`
//! transcripts in `examples/order_approval.md` and `examples/gate_flow.md`
//! cannot drift from the engine. These drive the same call path the `sts` REPL
//! uses at runtime (`load_topology` -> expand -> `from_schema` ->
//! `register_action` -> `send_event`) and assert the key milestones: normal
//! transitions, guard blocks, the `*` wildcard, and the full ordered
//! `executed_actions` the shell prints.

use signal_topology::{load_topology, TopologyEngine};
use std::collections::HashSet;
use std::path::Path;

/// Helper mirroring `sts::load_topology_for_run`: resolve includes + expand
/// instances, then register every action with an always-`Ok` handler (the
/// shell's print-and-record stub). Returns a ready-to-run engine.
fn load_engine_for_run(topology_path: &str) -> TopologyEngine {
    let schema = load_topology(Path::new(topology_path)).expect("should load topology");

    let mut action_ids = HashSet::new();
    for trans in &schema.transitions {
        action_ids.extend(trans.actions.all_actions().into_iter().cloned());
    }

    let mut engine = TopologyEngine::from_schema(schema).expect("should build engine");
    for action_id in &action_ids {
        engine.register_action(action_id, |_| Ok(()));
    }
    engine
}

// ---------------------------------------------------------------------------
// order_approval.md — normal transition (all three hooks), guard block, then
// a guarded success and a final transition through to the end state.
// ---------------------------------------------------------------------------

#[test]
fn test_order_approval_scenario() {
    let mut engine = load_engine_for_run("examples/order_approval.json");

    // Initial state.
    assert_eq!(engine.get_state("order").unwrap(), "draft");

    // 1) Normal transition exercises on_exit + on_transition + on_enter in
    //    order — the full three-hook chain.
    let r = engine.send_event("order", "submit", None).expect("submit should succeed");
    assert_eq!(r.from, "draft");
    assert_eq!(r.to, "submitted");
    assert_eq!(
        r.executed_actions,
        vec!["log_draft_exit", "validate_order_payload", "notify_submitted"]
    );
    assert_eq!(engine.get_state("order").unwrap(), "submitted");

    // 2) Guard block: amount == 0 is outside (0, 100000] -> blocked, state held.
    let err = engine
        .send_event("order", "approve", Some(serde_json::json!({"amount": 0})))
        .expect_err("approve with amount 0 must be guard-blocked");
    assert!(matches!(err, signal_topology::EngineError::GuardBlocked { .. }));
    assert_eq!(engine.get_state("order").unwrap(), "submitted");

    // 3) Guard passes: amount == 5000 -> approved.
    let r = engine
        .send_event("order", "approve", Some(serde_json::json!({"amount": 5000})))
        .expect("approve with amount 5000 should succeed");
    assert_eq!(r.from, "submitted");
    assert_eq!(r.to, "approved");
    assert_eq!(r.executed_actions, vec!["reserve_inventory", "notify_customer_approved"]);
    assert_eq!(engine.get_state("order").unwrap(), "approved");

    // 4) Run it down to the terminal state.
    let r = engine.send_event("order", "ship", None).expect("ship should succeed");
    assert_eq!(r.from, "approved");
    assert_eq!(r.to, "shipped");
    assert_eq!(r.executed_actions, vec!["dispatch_order", "notify_shipped"]);
    assert_eq!(engine.get_state("order").unwrap(), "shipped");
}

// ---------------------------------------------------------------------------
// gate_flow.md — normal transition, guard block, guarded success, then the `*`
// wildcard funneling both fault and a non-fault state back to `closed` (the
// latter producing a `closed -> closed` StateChanged that proves `*` is live).
// ---------------------------------------------------------------------------

#[test]
fn test_gate_flow_scenario() {
    let mut engine = load_engine_for_run("examples/gate_flow.json");

    // Initial state.
    assert_eq!(engine.get_state("gate").unwrap(), "closed");

    // 1) Normal open transition.
    let r = engine.send_event("gate", "open", None).expect("open should succeed");
    assert_eq!(r.from, "closed");
    assert_eq!(r.to, "open");
    assert_eq!(r.executed_actions, vec!["activate_motor", "log_gate_open"]);
    assert_eq!(engine.get_state("gate").unwrap(), "open");

    // 2) Guard block: emergency == false blocks the fault transition.
    let err = engine
        .send_event("gate", "fault", Some(serde_json::json!({"emergency": false})))
        .expect_err("fault with emergency=false must be guard-blocked");
    assert!(matches!(err, signal_topology::EngineError::GuardBlocked { .. }));
    assert_eq!(engine.get_state("gate").unwrap(), "open");

    // 3) Guard passes: emergency == true -> fault, multi-action on_transition.
    let r = engine
        .send_event("gate", "fault", Some(serde_json::json!({"emergency": true})))
        .expect("fault with emergency=true should succeed");
    assert_eq!(r.from, "open");
    assert_eq!(r.to, "fault");
    assert_eq!(
        r.executed_actions,
        vec!["engage_brake", "raise_alarm", "log_fault"]
    );
    assert_eq!(engine.get_state("gate").unwrap(), "fault");

    // 4) Wildcard `*` reset from fault -> closed.
    let r = engine.send_event("gate", "reset", None).expect("reset should succeed");
    assert_eq!(r.from, "fault");
    assert_eq!(r.to, "closed");
    assert_eq!(r.executed_actions, vec!["clear_fault_safely", "log_reset"]);
    assert_eq!(engine.get_state("gate").unwrap(), "closed");

    // 5) Cycle open -> close to exercise the close transition.
    let r = engine.send_event("gate", "open", None).expect("open should succeed");
    assert_eq!(r.to, "open");
    let r = engine.send_event("gate", "close", None).expect("close should succeed");
    assert_eq!(r.from, "open");
    assert_eq!(r.to, "closed");
    assert_eq!(r.executed_actions, vec!["deactivate_motor", "log_gate_closed"]);
    assert_eq!(engine.get_state("gate").unwrap(), "closed");

    // 6) Wildcard `*` reset *from closed* -> closed. This self-loop produces a
    //    `StateChanged gate: closed -> closed`, which is the proof that `*`
    //    matches the current state rather than being a no-op.
    let r = engine.send_event("gate", "reset", None).expect("reset from closed should succeed");
    assert_eq!(r.from, "closed");
    assert_eq!(r.to, "closed");
    assert_eq!(r.executed_actions, vec!["clear_fault_safely", "log_reset"]);

    let traces = engine.traces();
    assert!(traces.iter().any(|e| matches!(
        e,
        signal_topology::TraceEvent::StateChanged { signal_id, from, to, .. }
            if signal_id == "gate" && from == "closed" && to == "closed"
    )), "wildcard reset from closed must emit a closed -> closed StateChanged");
}
