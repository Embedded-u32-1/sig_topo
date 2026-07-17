//! M33: the `examples/scenarios/` regression + teaching library.
//!
//! Every subdirectory of `examples/scenarios/` is a self-contained scenario:
//! a `.ddl` topology, a `.scenario.json` replay, and an `EXPECTED.md`
//! transcript. This test discovers them automatically, compiles each `.ddl`
//! through `ddl::compile`, builds a ready-to-run engine (every action
//! registered with a shared fail-set so `fail_actions` injection works), then
//! replays the scenario event by event and asserts:
//!
//! 1. Events whose index is listed in `expected_guard_blocked` raise
//!    `EngineError::GuardBlocked`; events that name `fail_actions` raise
//!    `EngineError::ActionExecutionError` (the injected failure); every other
//!    event returns `Ok`.
//! 2. After the whole scenario resolves, each signal's current state matches
//!    `expected_final_states`.
//!
//! Because the scenarios are discovered by directory walk, adding a new one is
//! purely additive: drop a new `<name>/` directory in with its three files and
//! it is picked up and run here with no test-code change. That is what makes
//! this a *library* of scenarios rather than a set of isolated one-off examples.

use serde::Deserialize;
use signal_topology::run::{collect_action_ids, register_actions, ScenarioEvent};
use signal_topology::{EngineError, TopologyEngine};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::rc::Rc;

/// The extended on-disk scenario format. `events` reuses the shared
/// `run::ScenarioEvent` (so `fail_actions` injection works unchanged); the two
/// metadata fields are what the test asserts against.
#[derive(Debug, Deserialize)]
struct ScenarioFixture {
    expected_final_states: HashMap<String, String>,
    #[serde(default)]
    expected_guard_blocked: Vec<usize>,
    events: Vec<ScenarioEvent>,
}

/// Compile a `.ddl` file into a ready-to-run engine. Every action id is
/// registered with a handler parameterized by a shared `fail_set`: any action
/// whose id is present returns `ActionExecutionError`, which is exactly the
/// mechanism `run::run_scenario` uses to inject a per-event failure. Passing
/// the same `fail_set` the test mutates per event lets the scenario's
/// `fail_actions` field drive a real rollback.
fn build_engine_from_ddl(ddl_path: &Path, fail_set: Rc<RefCell<HashSet<String>>>) -> TopologyEngine {
    let src = fs::read_to_string(ddl_path).expect("scenario .ddl should be readable");
    let schema = signal_topology::ddl::compile(&src).expect("scenario .ddl should compile");
    let action_ids = collect_action_ids(&schema);
    let mut engine = TopologyEngine::from_schema(schema).expect("scenario schema should build");
    register_actions(&mut engine, &action_ids, Some(fail_set), false);
    engine
}

/// Discover and run every scenario under `examples/scenarios/`. Each scenario
/// is replayed against a fresh engine; per-event outcomes are asserted against
/// `expected_guard_blocked` / `fail_actions`, then the final states are
/// asserted against `expected_final_states`.
#[test]
fn all_scenario_dirs_pass() {
    let scenarios_dir = Path::new("examples/scenarios");
    let mut entries: Vec<_> = fs::read_dir(scenarios_dir)
        .expect("examples/scenarios/ should exist")
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .collect();
    entries.sort_by_key(|e| e.file_name());

    assert!(
        entries.len() >= 4,
        "expected at least 4 scenarios, found {}",
        entries.len()
    );

    for entry in &entries {
        let name = entry.file_name().to_string_lossy().to_string();
        let dir = entry.path();
        let ddl_path = dir.join(format!("{}.ddl", name));
        let scenario_path = dir.join(format!("{}.scenario.json", name));
        let expected_md_path = dir.join("EXPECTED.md");

        assert!(ddl_path.exists(), "[{name}] missing {name}.ddl");
        assert!(
            scenario_path.exists(),
            "[{name}] missing {name}.scenario.json"
        );
        assert!(expected_md_path.exists(), "[{name}] missing EXPECTED.md");

        let fail_set = Rc::new(RefCell::new(HashSet::new()));
        let mut engine = build_engine_from_ddl(&ddl_path, Rc::clone(&fail_set));

        let fixture_json =
            fs::read_to_string(&scenario_path).expect("scenario .scenario.json should be readable");
        let fixture: ScenarioFixture =
            serde_json::from_str(&fixture_json).expect("scenario .scenario.json should parse");

        for (i, ev) in fixture.events.iter().enumerate() {
            // Inject this event's `fail_actions` for the duration of the event,
            // mirroring `run::run_scenario`'s per-event scoping.
            for action_id in &ev.fail_actions {
                fail_set.borrow_mut().insert(action_id.clone());
            }

            let result = engine.send_event(&ev.signal_id, &ev.event, ev.payload.clone());

            for action_id in &ev.fail_actions {
                fail_set.borrow_mut().remove(action_id);
            }

            if fixture.expected_guard_blocked.contains(&i) {
                assert!(
                    matches!(result, Err(EngineError::GuardBlocked { .. })),
                    "[{name}] event {i} ({}.{}) expected GuardBlocked, got {:?}",
                    ev.signal_id,
                    ev.event,
                    result
                );
            } else if !ev.fail_actions.is_empty() {
                // An injected action failure must surface as ActionExecutionError
                // (and the engine must have rolled the signal back).
                assert!(
                    matches!(result, Err(EngineError::ActionExecutionError(_))),
                    "[{name}] event {i} ({}.{}) expected ActionExecutionError (injected), got {:?}",
                    ev.signal_id,
                    ev.event,
                    result
                );
            } else {
                assert!(
                    result.is_ok(),
                    "[{name}] event {i} ({}.{}) expected Ok, got {:?}",
                    ev.signal_id,
                    ev.event,
                    result
                );
            }
        }

        for (signal_id, expected_state) in &fixture.expected_final_states {
            assert_eq!(
                engine.get_state(signal_id).unwrap(),
                expected_state.as_str(),
                "[{name}] final state of {signal_id}"
            );
        }
    }
}
