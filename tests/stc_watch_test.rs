//! M51: `stc watch` integration tests.
//!
//! Drive `watch_file` from a background thread against a temp `.ddl` file we
//! rewrite by hand, forcing an mtime change the watcher must detect and
//! recompile. The stop flag ends the loop deterministically so the test
//! finishes instead of waiting for Ctrl+C.

use signal_topology::watch::{watch_file, WatchEvent};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// A minimal, valid `.ddl` source.
const DDL_V1: &str = r#"
signal order {
    states: [draft, submitted, approved]
    initial: draft
    on submit from draft -> submitted
    on approve from submitted -> approved
}
"#;

/// A second, distinct source that still compiles — adds a `rejected` state and
/// a transition to it, so a fresh compile produces a different topology.
const DDL_V2: &str = r#"
signal order {
    states: [draft, submitted, approved, rejected]
    initial: draft
    on submit from draft -> submitted
    on approve from submitted -> approved
    on reject from submitted -> rejected
}
"#;

/// A temp directory that is removed when the guard drops.
fn temp_dir() -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("stc_watch_test_{}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).expect("create temp dir");
    p
}

/// Spawn the watcher; return the stop flag + a shared, collected list of events.
fn start_watch(
    ddl_path: PathBuf,
    events: Arc<Mutex<Vec<WatchEvent>>>,
    interval_ms: u64,
) -> (Arc<AtomicBool>, thread::JoinHandle<()>) {
    let running = Arc::new(AtomicBool::new(true));
    let r = Arc::clone(&running);
    let handle = thread::spawn(move || {
        let mut callback = |event: WatchEvent| {
            events.lock().unwrap().push(event);
        };
        watch_file(&ddl_path, None, interval_ms, &r, &mut callback);
    });
    (running, handle)
}

/// Wait until `pred` is true for the collected events, or the timeout elapses.
/// Returns whatever the collector holds at the end for assertion.
fn wait_for(
    events: &Arc<Mutex<Vec<WatchEvent>>>,
    timeout_ms: u64,
    pred: impl Fn(&[WatchEvent]) -> bool,
) -> Vec<WatchEvent> {
    let deadline = std::time::Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        {
            let guard = events.lock().unwrap();
            if pred(&guard) {
                return guard.clone();
            }
        }
        if std::time::Instant::now() >= deadline {
            return events.lock().unwrap().clone();
        }
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn watch_recompiles_on_file_change() {
    let dir = temp_dir();
    let ddl = dir.join("order.ddl");
    fs::write(&ddl, DDL_V1).expect("write ddl v1");

    // Short poll (the minimum) so the test runs fast; the watcher clamps to
    // MIN_INTERVAL_MS internally.
    let events = Arc::new(Mutex::new(Vec::new()));
    let (running, handle) = start_watch(ddl.clone(), Arc::clone(&events), 100);

    // Initial compile should fire immediately on the first observed mtime.
    let _ = wait_for(&events, 3000, |e| {
        e.iter().any(|x| matches!(x, WatchEvent::CompiledOk))
    });

    // Rewrite the file. The mtime must change; fs::write truncates + writes,
    // so a fresh mtime is guaranteed. Give the OS a tick in case mtime
    // resolution is coarse.
    thread::sleep(Duration::from_millis(1100));
    fs::write(&ddl, DDL_V2).expect("write ddl v2");

    let got = wait_for(&events, 5000, |e| {
        // At least two successful compiles: the initial one + the change.
        e.iter()
            .filter(|x| matches!(x, WatchEvent::CompiledOk))
            .count()
            >= 2
    });

    running.store(false, Ordering::Relaxed);
    handle.join().expect("watcher thread exits");

    let ok_count = got
        .iter()
        .filter(|x| matches!(x, WatchEvent::CompiledOk))
        .count();
    assert!(
        ok_count >= 2,
        "expected at least 2 successful compiles (initial + change), got {ok_count}; events={:?}",
        got
    );
    // No compile errors along the way.
    assert!(
        !got.iter().any(|x| matches!(x, WatchEvent::CompileError(_))),
        "unexpected compile errors: {:?}",
        got
    );
}

#[test]
fn watch_reports_compile_error_on_broken_file() {
    let dir = temp_dir();
    let ddl = dir.join("broken.ddl");
    fs::write(&ddl, DDL_V1).expect("write valid ddl");

    let events = Arc::new(Mutex::new(Vec::new()));
    let (running, handle) = start_watch(ddl.clone(), Arc::clone(&events), 100);

    // Ensure the initial (valid) compile landed.
    let _ = wait_for(&events, 3000, |e| {
        e.iter().any(|x| matches!(x, WatchEvent::CompiledOk))
    });

    // Write something that does NOT compile. Same coarse-mtime caveat.
    thread::sleep(Duration::from_millis(1100));
    fs::write(&ddl, "signal order { this is not valid ddl").expect("write broken ddl");

    let got = wait_for(&events, 5000, |e| {
        e.iter().any(|x| matches!(x, WatchEvent::CompileError(_)))
    });

    running.store(false, Ordering::Relaxed);
    handle.join().expect("watcher thread exits");

    assert!(
        got.iter().any(|x| matches!(x, WatchEvent::CompileError(_))),
        "expected a CompileError after writing broken source, got: {:?}",
        got
    );
}

#[test]
fn watch_runs_scenario_regression_when_given() {
    let dir = temp_dir();
    let ddl = dir.join("order.ddl");
    fs::write(&ddl, DDL_V1).expect("write ddl v1");

    // A scenario that just submits and approves — both valid under DDL_V1.
    let scenario = dir.join("order.scenario.json");
    fs::write(
        &scenario,
        r#"{ "events": [
            { "signal_id": "order", "event": "submit" },
            { "signal_id": "order", "event": "approve" }
        ] }"#,
    )
    .expect("write scenario");

    let events = Arc::new(Mutex::new(Vec::new()));
    let running = Arc::new(AtomicBool::new(true));
    let r = Arc::clone(&running);
    let events2 = Arc::clone(&events);
    let handle = thread::spawn(move || {
        let mut callback = |event: WatchEvent| {
            events2.lock().unwrap().push(event);
        };
        watch_file(&ddl, Some(&scenario), 100, &r, &mut callback);
    });

    let got = wait_for(&events, 5000, |e| {
        e.iter()
            .any(|x| matches!(x, WatchEvent::ScenarioResult { failures: 0, .. }))
    });

    running.store(false, Ordering::Relaxed);
    handle.join().expect("watcher thread exits");

    assert!(
        got.iter()
            .any(|x| matches!(x, WatchEvent::ScenarioResult { total: 2, failures: 0 })),
        "expected a passing 2-event scenario run, got: {:?}",
        got
    );
}
