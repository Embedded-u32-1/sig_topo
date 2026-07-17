// Shared helpers for the `sts` (interactive shell) and `stt` (batch replay)
// binaries. Both build a ready-to-run engine the same way -- resolve includes,
// expand instances, register a handler per action -- differing only in (a)
// whether they print a live `[action] <signal>.<id>` line as each action runs
// and (b) which action ids are forced to fail next. Extracting this keeps the
// two binaries from drifting and gives the replay tool the same rollback
// demonstration the shell already has.
//
// This module is `pub` so the binary crates (`src/bin/*.rs`) can build against
// it, but it is intentionally not part of the stable library surface: it is the
// common scaffolding that turns a topology file + a shared fail-set into a
// runnable engine.

use crate::engine::TopologyEngine;
use crate::error::EngineError;
use crate::load_topology;
use crate::schema::TopologySchema;
use crate::trace::TraceEvent;
use serde::Deserialize;
use std::cell::RefCell;
use std::collections::HashSet;
use std::path::Path;
use std::rc::Rc;

/// A batch-replay scenario: an ordered list of events to send.
#[derive(Debug, Deserialize)]
pub struct Scenario {
    pub events: Vec<ScenarioEvent>,
}

/// One event in a scenario. `fail_actions` (optional, defaults to empty) lists
/// action ids that are forced to fail for *this* event only: each named action
/// returns `ActionExecutionError` while this event is dispatched, so the engine
/// rolls the transition back. After the event resolves the names are cleared,
/// so a later event that re-uses the same action is unaffected -- injection is
/// scoped per event, which keeps a scenario readable and replay deterministic.
#[derive(Debug, Deserialize)]
pub struct ScenarioEvent {
    pub signal_id: String,
    pub event: String,
    pub payload: Option<serde_json::Value>,
    #[serde(default)]
    pub fail_actions: Vec<String>,
}

/// Collect every action id referenced by the expanded transitions, deduped.
pub fn collect_action_ids(schema: &TopologySchema) -> HashSet<String> {
    let mut ids = HashSet::new();
    for trans in &schema.transitions {
        ids.extend(trans.actions.all_actions().into_iter().cloned());
    }
    ids
}

/// Register every action id with a handler parameterized by two flags:
///
/// * `fail_set` -- if `Some`, any action whose id is present returns
///   `ActionExecutionError` ("injected failure ..."), which the engine turns
///   into a rolled-back transition. This is the same mechanism the `sts` `fail`
///   command stabs from the REPL.
/// * `record` -- if `true`, the handler prints `[action] <signal>.<id>` as each
///   action runs (the shell's live output). If `false` it stays silent (stt).
pub fn register_actions(
    engine: &mut TopologyEngine,
    ids: &HashSet<String>,
    fail_set: Option<Rc<RefCell<HashSet<String>>>>,
    record: bool,
) {
    for action_id in ids {
        let id = action_id.clone();
        let fail_set = fail_set.as_ref().map(Rc::clone);
        engine.register_action(action_id, move |ctx| {
            if let Some(ref fs) = fail_set {
                if fs.borrow().contains(&id) {
                    return Err(EngineError::ActionExecutionError(format!(
                        "injected failure for action '{}' (set via `fail`)",
                        id
                    )));
                }
            }
            if record {
                println!("[action] {}.{}", ctx.signal_id, id);
            }
            Ok(())
        });
    }
}

/// Resolve includes + expand instances, build the engine, and register every
/// action. `fail_set` is the shared set of action ids forced to fail; `record`
/// toggles the live `[action] ...` stdout line. Exits the process on a bad
/// topology, matching the binaries' fail-fast behaviour.
pub fn load_topology_for_run(
    topology_path: &str,
    fail_set: Option<Rc<RefCell<HashSet<String>>>>,
    record: bool,
) -> TopologyEngine {
    let schema = load_topology(Path::new(topology_path)).unwrap_or_else(|e| {
        eprintln!("Failed to load topology '{}': {}", topology_path, e);
        std::process::exit(1);
    });
    let action_ids = collect_action_ids(&schema);
    let mut engine = TopologyEngine::from_schema(schema).unwrap_or_else(|e| {
        eprintln!("Failed to load topology: {}", e);
        std::process::exit(1)
    });
    register_actions(&mut engine, &action_ids, fail_set, record);
    engine
}

/// One event that failed during `run_scenario`. Replay records these and
/// continues; the caller decides how to report them.
#[derive(Debug)]
pub struct ScenarioError {
    pub signal_id: String,
    pub event: String,
    pub error: EngineError,
    /// The signal's state immediately after the failed event resolved -- i.e.
    /// the state the engine rolled back to. Captured at error time (not after
    /// later events run), so a caller can report the rolled-back state without
    /// it being overwritten by subsequent replay steps.
    pub state_after: String,
}

/// Replay a scenario against a ready-to-run engine. `fail_actions` on each event
/// are scoped: those ids are forced to fail for that event alone, then cleared.
/// On error the offending event is recorded and replay continues with the next
/// one -- the engine has already rolled that signal back, so subsequent events
/// still run. This mirrors the shell's "roll back + wait for next command"
/// semantics and lets a replay transcript show `ActionFailed` + `Rollbacked` in
/// context with the events that follow. Returns the list of events that errored.
pub fn run_scenario(
    engine: &mut TopologyEngine,
    scenario: &Scenario,
    fail_set: &Rc<RefCell<HashSet<String>>>,
) -> Vec<ScenarioError> {
    let mut errors = Vec::new();
    for ev in &scenario.events {
        for action_id in &ev.fail_actions {
            fail_set.borrow_mut().insert(action_id.clone());
        }
        if let Err(error) = engine.send_event(&ev.signal_id, &ev.event, ev.payload.clone()) {
            // Capture the state right now, while the rollback is still in
            // effect; a later event could move the signal elsewhere.
            let state_after = engine
                .get_state(&ev.signal_id)
                .map(|s| s.to_string())
                .unwrap_or_default();
            errors.push(ScenarioError {
                signal_id: ev.signal_id.clone(),
                event: ev.event.clone(),
                error,
                state_after,
            });
        }
        for action_id in &ev.fail_actions {
            fail_set.borrow_mut().remove(action_id);
        }
    }
    errors
}

/// Format a single trace event, matching the layout produced by sts and stt:
/// one `EventReceived` / `ActionStarted` / `ActionSucceeded` / `ActionFailed` /
/// `StateChanged` / `Rollbacked` line with a monotonic timestamp.
pub fn format_trace(event: &TraceEvent) -> String {
    match event {
        TraceEvent::EventReceived {
            signal_id,
            event,
            timestamp_ms,
            payload,
        } => format!(
            "[{}] EventReceived {}.{} payload={}",
            timestamp_ms,
            signal_id,
            event,
            payload.as_deref().unwrap_or("None")
        ),
        TraceEvent::ActionStarted {
            signal_id,
            action_id,
            timestamp_ms,
        } => format!("[{}] ActionStarted {}.{}", timestamp_ms, signal_id, action_id),
        TraceEvent::ActionSucceeded {
            signal_id,
            action_id,
            timestamp_ms,
        } => format!("[{}] ActionSucceeded {}.{}", timestamp_ms, signal_id, action_id),
        TraceEvent::ActionFailed {
            signal_id,
            action_id,
            timestamp_ms,
            error,
        } => format!(
            "[{}] ActionFailed {}.{} error={}",
            timestamp_ms, signal_id, action_id, error
        ),
        TraceEvent::StateChanged {
            signal_id,
            from,
            to,
            timestamp_ms,
        } => format!("[{}] StateChanged {}: {} -> {}", timestamp_ms, signal_id, from, to),
        TraceEvent::Rollbacked {
            signal_id,
            from,
            to,
            timestamp_ms,
        } => format!("[{}] Rollbacked {}: {} -> {}", timestamp_ms, signal_id, from, to),
    }
}

// ---------------------------------------------------------------------------
// Unit tests for the pure helpers (collect_action_ids, format_trace).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{ActionBinding, TopologySchema, TransitionDef};

    fn binding(on_exit: &[&str], on_transition: &[&str], on_enter: &[&str]) -> ActionBinding {
        ActionBinding {
            on_exit: on_exit.iter().map(|s| s.to_string()).collect(),
            on_transition: on_transition.iter().map(|s| s.to_string()).collect(),
            on_enter: on_enter.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn dummy_transition(signal: &str, actions: ActionBinding) -> TransitionDef {
        TransitionDef {
            signal_id: signal.to_string(),
            from: "a".to_string(),
            event: "go".to_string(),
            to: "b".to_string(),
            actions,
            guard: None,
        }
    }

    #[test]
    fn collect_action_ids_dedupes_across_transitions() {
        let schema = TopologySchema {
            version: "0.1".to_string(),
            signals: Vec::new(),
            transitions: vec![
                dummy_transition("s1", binding(&["x"], &["y"], &[])),
                // "x" appears again in a second transition plus a new "z".
                dummy_transition("s2", binding(&["x"], &[], &["z"])),
            ],
            reactions: Vec::new(),
            components: None,
            instances: Vec::new(),
            includes: Vec::new(),
        };

        let ids = collect_action_ids(&schema);
        assert_eq!(ids.len(), 3);
        assert!(ids.contains("x"));
        assert!(ids.contains("y"));
        assert!(ids.contains("z"));
    }

    #[test]
    fn collect_action_ids_empty_without_transitions() {
        let schema = TopologySchema {
            version: "0.1".to_string(),
            signals: Vec::new(),
            transitions: Vec::new(),
            reactions: Vec::new(),
            components: None,
            instances: Vec::new(),
            includes: Vec::new(),
        };
        assert!(collect_action_ids(&schema).is_empty());
    }

    #[test]
    fn format_trace_matches_stt_layout() {
        let line = format_trace(&TraceEvent::EventReceived {
            signal_id: "order".to_string(),
            event: "submit".to_string(),
            timestamp_ms: 1000,
            payload: None,
        });
        assert_eq!(line, "[1000] EventReceived order.submit payload=None");

        let line = format_trace(&TraceEvent::ActionFailed {
            signal_id: "order".to_string(),
            action_id: "reserve_inventory".to_string(),
            timestamp_ms: 1001,
            error: "boom".to_string(),
        });
        assert_eq!(line, "[1001] ActionFailed order.reserve_inventory error=boom");

        let line = format_trace(&TraceEvent::Rollbacked {
            signal_id: "order".to_string(),
            from: "approved".to_string(),
            to: "submitted".to_string(),
            timestamp_ms: 1002,
        });
        assert_eq!(line, "[1002] Rollbacked order: approved -> submitted");
    }
}
