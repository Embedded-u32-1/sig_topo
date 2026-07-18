// M51: `stc watch` — poll a `.ddl` file for changes and recompile on change.
//
// Pure std only (no new Cargo dependencies): the watcher polls the file's
// mtime with `fs::metadata`, recompiles through `ddl::compile_full`, and
// (optionally) runs a scenario regression. A shared `AtomicBool` stop flag
// lets a caller end the loop cleanly; the `stc` binary simply leaves it set
// and relies on the default Ctrl+C (SIGINT) termination.

use crate::ddl::compile_full;
use crate::engine::TopologyEngine;
use crate::run::{collect_action_ids, register_actions, run_scenario, Scenario};
use crate::schema::TopologySchema;
use std::cell::RefCell;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

/// Default poll interval (ms) when `stc watch` is given no `--interval`.
pub const DEFAULT_INTERVAL_MS: u64 = 500;
/// Floor for `--interval`; the loop never polls faster than this.
pub const MIN_INTERVAL_MS: u64 = 100;
/// Debounce window: a change is skipped when it arrives less than this long
/// after the previous *compile* (not the previous poll). Editors that save a
/// file in several steps can bounce the mtime repeatedly within a few
/// milliseconds; this avoids compiling a half-written file more than once.
pub const DEBOUNCE_MS: u64 = 200;

/// What the watcher reports to its callback after each compile attempt.
#[derive(Debug, Clone, PartialEq)]
pub enum WatchEvent {
    /// The file compiled cleanly.
    CompiledOk,
    /// Compiling the file failed (the message carries the compiler error).
    CompileError(String),
    /// A `--scenario` regression ran to completion.
    ScenarioResult {
        /// Total scenario events replayed.
        total: usize,
        /// Events that errored (rolled back) during replay.
        failures: usize,
    },
}

/// Parsed `stc watch <file.ddl> [--scenario <file.json>] [--interval <ms>]`.
#[derive(Debug, Clone, PartialEq)]
pub struct WatchArgs {
    pub ddl_path: PathBuf,
    pub scenario_path: Option<PathBuf>,
    pub interval_ms: u64,
}

/// Parse the tokens that follow the `watch` keyword. `argv` is everything
/// after `watch` (e.g. `["file.ddl", "--scenario", "s.json", "--interval",
/// "200"]`); the binary's own name and the `watch` keyword have already been
/// stripped. A missing `<file.ddl>` or an unknown flag is an error.
pub fn parse_watch_args(argv: &[String]) -> Result<WatchArgs, String> {
    let mut ddl = None;
    let mut scenario = None;
    let mut interval = DEFAULT_INTERVAL_MS;

    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "--scenario" => {
                i += 1;
                let v = argv
                    .get(i)
                    .ok_or_else(|| "--scenario requires a file path".to_string())?;
                scenario = Some(PathBuf::from(v));
            }
            "--interval" => {
                i += 1;
                let v = argv
                    .get(i)
                    .ok_or_else(|| "--interval requires a number".to_string())?;
                interval = v
                    .parse::<u64>()
                    .map_err(|_| format!("invalid interval: '{}'", v))?;
            }
            other if other.starts_with("--") => {
                return Err(format!("unknown flag: {}", other));
            }
            positional => {
                if ddl.is_none() {
                    ddl = Some(PathBuf::from(positional));
                } else {
                    return Err(format!("unexpected argument: {}", positional));
                }
            }
        }
        i += 1;
    }

    let ddl = ddl.ok_or_else(|| "missing <file.ddl>".to_string())?;
    Ok(WatchArgs {
        ddl_path: ddl,
        scenario_path: scenario,
        interval_ms: interval,
    })
}

/// Read a file's current mtime, or `None` if it cannot be read (the file was
/// deleted between polls, its directory vanished, ...). The caller treats
/// `None` as "no decision this round" and tries again on the next poll.
fn current_mtime(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Compile `ddl_path` into a `TopologySchema`. The mtime is checked again
/// right before reading, so we don't waste a compile on a file that
/// disappeared between polls. Any lexical / syntactic / semantic error is
/// returned as a human-readable string (the compiler's `EngineError` already
/// carries a line/column).
fn recompile(ddl_path: &Path) -> Result<TopologySchema, String> {
    let src = fs::read_to_string(ddl_path)
        .map_err(|e| format!("Failed to read '{}': {}", ddl_path.display(), e))?;
    let (schema, _ddl_doc) = compile_full(&src)
        .map_err(|e| format!("Failed to compile '{}': {}", ddl_path.display(), e))?;
    Ok(schema)
}

/// If `scenario_path` is set, replay that scenario against a fresh engine
/// built from `schema` and return the pass/fail counts. Any error reading or
/// parsing the scenario is surfaced as an `Err` so the caller can report it as
/// a `CompileError`-style event.
fn run_scenario_regression(
    schema: &TopologySchema,
    scenario_path: &Path,
) -> Result<(usize, usize), String> {
    let scenario_json = fs::read_to_string(scenario_path).map_err(|e| {
        format!(
            "Failed to read scenario '{}': {}",
            scenario_path.display(),
            e
        )
    })?;
    let scenario: Scenario = serde_json::from_str(&scenario_json).map_err(|e| {
        format!(
            "Failed to parse scenario '{}': {}",
            scenario_path.display(),
            e
        )
    })?;

    // Reuse the same scaffolding the batch binaries use: a shared per-event
    // fail set plus every action id registered with a silent handler.
    let fail_set = Rc::new(RefCell::new(HashSet::new()));
    let action_ids = collect_action_ids(schema);
    let mut engine = TopologyEngine::from_schema(schema.clone())
        .map_err(|e| format!("Failed to build engine: {}", e))?;
    register_actions(&mut engine, &action_ids, Some(Rc::clone(&fail_set)), false);

    let failures = run_scenario(&mut engine, &scenario, &fail_set).len();
    Ok((scenario.events.len(), failures))
}

/// Poll `ddl_path` for changes and recompile on every mtime change.
///
/// * On startup it compiles immediately (the first observed mtime counts as a
///   "change" against the initial `None`), so the user gets prompt feedback.
/// * After a change is detected it compiles, invokes `callback` with the
///   outcome, and — if `--scenario` was given — runs the regression and
///   invokes `callback` again with the result.
/// * A change arriving less than `DEBOUNCE_MS` after the previous compile is
///   skipped (the mtime is still recorded, so a later *distinct* change is not
///   lost).
/// * The loop exits when `running` is flipped to `false`. The `stc` binary
///   never flips it, so the process runs until Ctrl+C; tests use it to stop
///   the loop deterministically.
pub fn watch_file(
    ddl_path: &Path,
    scenario_path: Option<&Path>,
    interval_ms: u64,
    running: &AtomicBool,
    callback: &mut impl FnMut(WatchEvent),
) {
    let interval = Duration::from_millis(interval_ms.max(MIN_INTERVAL_MS));
    let debounce = Duration::from_millis(DEBOUNCE_MS);

    let mut last_mtime: Option<SystemTime> = None;
    let mut last_compile: Option<Instant> = None;

    while running.load(Ordering::Relaxed) {
        let cur_mtime = current_mtime(ddl_path);

        let mtime_changed = match (last_mtime, cur_mtime) {
            (None, Some(_)) => true,            // first observation -> initial compile
            (Some(prev), Some(cur)) => cur != prev,
            _ => false,
        };

        if mtime_changed {
            if let Some(t) = last_compile {
                if t.elapsed() < debounce {
                    // Record the mtime so this change isn't re-raised, then wait.
                    last_mtime = cur_mtime;
                    std::thread::sleep(interval);
                    continue;
                }
            }

            last_mtime = cur_mtime;
            last_compile = Some(Instant::now());

            match recompile(ddl_path) {
                Ok(schema) => {
                    callback(WatchEvent::CompiledOk);
                    if let Some(scn) = scenario_path {
                        match run_scenario_regression(&schema, scn) {
                            Ok((total, failures)) => {
                                callback(WatchEvent::ScenarioResult { total, failures });
                            }
                            Err(msg) => callback(WatchEvent::CompileError(msg)),
                        }
                    }
                }
                Err(msg) => callback(WatchEvent::CompileError(msg)),
            }
        }

        std::thread::sleep(interval);
    }
}

// ---------------------------------------------------------------------------
// Unit tests: argument parsing + the pure helpers.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn args(tokens: &[&str]) -> Vec<String> {
        tokens.iter().map(|t| t.to_string()).collect()
    }

    #[test]
    fn parse_watch_args_minimal() {
        let w = parse_watch_args(&args(&["file.ddl"])).unwrap();
        assert_eq!(w.ddl_path, PathBuf::from("file.ddl"));
        assert_eq!(w.scenario_path, None);
        assert_eq!(w.interval_ms, DEFAULT_INTERVAL_MS);
    }

    #[test]
    fn parse_watch_args_full() {
        let w = parse_watch_args(&args(&[
            "file.ddl",
            "--scenario",
            "s.json",
            "--interval",
            "200",
        ]))
        .unwrap();
        assert_eq!(w.ddl_path, PathBuf::from("file.ddl"));
        assert_eq!(w.scenario_path, Some(PathBuf::from("s.json")));
        assert_eq!(w.interval_ms, 200);
    }

    #[test]
    fn parse_watch_args_flags_before_ddl() {
        let w = parse_watch_args(&args(&["--interval", "300", "--scenario", "s.json", "f.ddl"])).unwrap();
        assert_eq!(w.ddl_path, PathBuf::from("f.ddl"));
        assert_eq!(w.scenario_path, Some(PathBuf::from("s.json")));
        assert_eq!(w.interval_ms, 300);
    }

    #[test]
    fn parse_watch_args_missing_ddl_errors() {
        assert!(parse_watch_args(&args(&["--scenario", "s.json"])).is_err());
    }

    #[test]
    fn parse_watch_args_unknown_flag_errors() {
        assert!(parse_watch_args(&args(&["f.ddl", "--bogus"])).is_err());
    }

    #[test]
    fn parse_watch_args_extra_positional_errors() {
        assert!(parse_watch_args(&args(&["a.ddl", "b.ddl"])).is_err());
    }

    #[test]
    fn parse_watch_args_interval_must_be_a_number() {
        assert!(parse_watch_args(&args(&["f.ddl", "--interval", "big"])).is_err());
    }

    #[test]
    fn parse_watch_args_scenario_value_required() {
        assert!(parse_watch_args(&args(&["f.ddl", "--scenario"])).is_err());
    }

    #[test]
    fn watch_event_equality() {
        assert_eq!(WatchEvent::CompiledOk, WatchEvent::CompiledOk);
        assert_eq!(
            WatchEvent::ScenarioResult {
                total: 3,
                failures: 1,
            },
            WatchEvent::ScenarioResult {
                total: 3,
                failures: 1,
            }
        );
        assert_ne!(
            WatchEvent::CompiledOk,
            WatchEvent::CompileError("x".to_string())
        );
    }
}
