//! sts (signal-topology-shell) integration tests.
//!
//! These exercise the same engine call path that `src/bin/sts.rs` uses at
//! runtime: `load_topology` -> collect action ids from expanded transitions ->
//! `TopologyEngine::from_schema` -> `register_action` -> `send_event` /
//! `get_state` / `signal_ids` / `traces`. They do not invoke the `main()` so
//! the shell's IO layer stays out of the way; the focus is on semantics that
//! the REPL's `event` / `state` / `trace` commands depend on.

use signal_topology::{load_topology, EngineError, TopologyEngine, TraceEvent};
use std::collections::HashSet;
use std::path::Path;

/// Helper mirroring `sts::load_topology_for_run`: resolve includes + expand
/// instances, then register every (print-and-record) action with the shell's
/// always-`Ok` handler.
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

/// `event <signal> <event>` drives a normal transition. Mirrors what the
/// REPL's `event` command does on success: `send_event` returns `Ok` and the
/// result records the target `to` state.
#[test]
fn test_sts_event_command_drives_normal_transition() {
    let mut engine = load_engine_for_run("tests/topology.json");

    // `task_status` starts at `idle` (topology.json).
    assert_eq!(engine.get_state("task_status").unwrap(), "idle");

    let result = engine.send_event("task_status", "start", None);
    assert!(result.is_ok(), "send_event should succeed");
    let result = result.unwrap();
    assert_eq!(result.signal_id, "task_status");
    assert_eq!(result.from, "idle");
    assert_eq!(result.to, "running");

    // The shell's `event` path also echoes `executed_actions`; here all three
    // lifecycle actions were registered and must be recorded.
    assert_eq!(
        result.executed_actions,
        vec!["log_idle_leave", "init_task_resource", "start_task_execution"]
    );
    assert_eq!(engine.get_state("task_status").unwrap(), "running");

    // Durable trace side effect the REPL's `trace` command prints.
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
}

/// A lifecycle action returning `Err` triggers rollback: the signal returns to
/// the source state and the trace records `ActionFailed` + `Rollbacked`. This
/// is the failure path the REPL's `event` command reports ("Error: ..." then
/// "State rolled back to '<source>'").
#[test]
fn test_sts_event_command_rolls_back_on_action_failure() {
    // Reuse load_topology + expand like the shell, then override the
    // `on_transition` action with one that fails — exactly the scenario sts
    // surfaces on the error path.
    let schema = load_topology(Path::new("tests/topology.json")).expect("should load topology");
    let mut engine = TopologyEngine::from_schema(schema).expect("should build engine");
    engine.register_action("log_idle_leave", |_| Ok(()));
    engine.register_action("init_task_resource", |_| {
        Err(EngineError::ActionExecutionError(
            "resource init failed".to_string(),
        ))
    });
    engine.register_action("start_task_execution", |_| Ok(()));

    let result = engine.send_event("task_status", "start", None);
    assert!(result.is_err(), "failing action must make send_event fail");

    // Rolled back to the source state — the shell prints this on failure.
    assert_eq!(engine.get_state("task_status").unwrap(), "idle");

    let traces = engine.traces();
    // No committed StateChanged when rollback happens.
    assert!(traces.iter().all(|e| !matches!(
        e,
        TraceEvent::StateChanged { signal_id, .. } if signal_id == "task_status"
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
    assert!(traces.iter().any(|e| matches!(
        e,
        TraceEvent::Rollbacked {
            signal_id,
            from,
            to,
            ..
        } if signal_id == "task_status" && from == "running" && to == "idle"
    )));
}

/// `state` lists every signal_id -> current state, and `trace` returns the
/// ordered event log. Mirrors the REPL's `state` and `trace` read paths.
#[test]
fn test_sts_state_and_trace_read_paths() {
    let mut engine = load_engine_for_run("tests/topology.json");

    // `state` read path before any event: single signal at its initial state.
    let mut ids = engine.signal_ids();
    ids.sort();
    assert_eq!(ids, vec!["task_status"]);
    assert_eq!(engine.get_state("task_status").unwrap(), "idle");

    // Drive a transition so the trace log is non-empty.
    engine
        .send_event("task_status", "start", None)
        .expect("start should succeed");
    assert_eq!(engine.get_state("task_status").unwrap(), "running");

    // `state` read path after the transition reflects the new state.
    assert_eq!(engine.get_state("task_status").unwrap(), "running");

    // `trace` read path returns an ordered, non-empty log including the event
    // receipt and the committed state change.
    let traces = engine.traces();
    assert!(!traces.is_empty());
    assert!(traces.iter().any(|e| matches!(
        e,
        TraceEvent::EventReceived { signal_id, event, .. }
            if signal_id == "task_status" && event == "start"
    )));
    assert!(traces.iter().any(|e| matches!(
        e,
        TraceEvent::StateChanged { signal_id, from, to, .. }
            if signal_id == "task_status" && from == "idle" && to == "running"
    )));
}
