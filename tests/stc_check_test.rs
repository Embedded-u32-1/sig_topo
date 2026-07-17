//! M36: end-to-end tests for `stc --check`.
//!
//! These run the actual `stc` binary the way a user would: `cargo build
//! --bin stc` once, then invoke the compiled binary with `--check` on real
//! `.ddl` files and assert on its stderr. Running the prebuilt binary
//! directly (rather than through `cargo run`) keeps each test fast and avoids
//! nested-cargo target-directory lock contention during `cargo test`.
//!
//! The scenarios covered:
//! - `gate_flow.ddl` carries a wildcard `from * -> closed`, which the compiler
//!   lowers to a `closed -> closed` self-loop; `--check` must report a self-loop
//!   warning while still writing valid JSON.
//! - `order_approval.ddl` is a clean linear-ish flow with no self-loops and no
//!   unreachable states; `--check` must report no warnings.
//!
//! Both cases also verify the JSON output is still emitted (warnings are
//! non-blocking) and parses as a topology.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Locate the compiled `stc` binary, building it first if necessary. The
/// binary lives in the workspace `target/debug/`, alongside the crate under
/// test — found by walking up from `CARGO_MANIFEST_DIR`.
fn stc_binary() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let candidate = manifest_dir.join("target/debug/stc");
    if candidate.exists() {
        return candidate;
    }
    // Build once so the binary is present for the whole test process.
    let status = Command::new(env!("CARGO"))
        .args(["build", "--bin", "stc"])
        .status()
        .expect("`cargo build --bin stc` should run");
    assert!(status.success(), "`cargo build --bin stc` failed");
    assert!(candidate.exists(), "built stc binary not found at {candidate:?}");
    candidate
}

/// Run `stc --check` on `relative_ddl` (relative to the crate root), writing
/// JSON to `out_json` (created under the target dir so it is outside the
/// source tree). Returns the captured (stdout, stderr); stderr carries the
/// warnings.
fn run_stc_check(relative_ddl: &str, out_json: &Path) -> (String, String) {
    let stc = stc_binary();
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let ddl_path = manifest_dir.join(relative_ddl);
    assert!(ddl_path.exists(), "DDL fixture missing: {ddl_path:?}");

    let output = Command::new(&stc)
        .args(["--check", ddl_path.to_str().unwrap(), out_json.to_str().unwrap()])
        .output()
        .expect("stc should run");
    // Warnings are non-blocking: exit code stays 0 regardless.
    assert!(
        output.status.success(),
        "stc --check should exit 0 even with warnings, got {:?}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

#[test]
fn gate_flow_wildcard_reports_self_loop() {
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/stc_check_gate_flow.json");
    let (stdout, stderr) =
        run_stc_check("examples/scenarios/gate_flow/gate_flow.ddl", &out);

    // The wildcard `from * -> closed` lowers to a closed -> closed self-loop,
    // which --check must surface.
    assert!(
        stderr.contains("self-loop"),
        "expected a self-loop warning on stderr, got:\n{stderr}"
    );
    assert!(
        stderr.contains("closed -> closed"),
        "expected the closed self-loop to be named, got:\n{stderr}"
    );
    assert!(
        stderr.contains("warning(s) found"),
        "expected a warning summary, got:\n{stderr}"
    );

    // Non-blocking: JSON is still written and parses.
    assert!(
        stdout.contains("Compiled"),
        "expected a `Compiled ...` confirmation on stdout, got:\n{stdout}"
    );
    let json = std::fs::read_to_string(&out).expect("JSON output should exist");
    assert!(
        serde_json::from_str::<serde_json::Value>(&json).is_ok(),
        "output should be valid JSON"
    );
    assert!(
        json.contains("gate"),
        "compiled JSON should still describe the gate signal"
    );
}

#[test]
fn order_approval_reports_no_warnings() {
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/stc_check_order_approval.json");
    let (stdout, stderr) = run_stc_check("examples/order_approval.ddl", &out);

    // Clean flow: no self-loops, no unreachable states.
    assert!(
        !stderr.contains("self-loop"),
        "order_approval should have no self-loop, got:\n{stderr}"
    );
    assert!(
        !stderr.contains("unreachable-state"),
        "order_approval should have no unreachable state, got:\n{stderr}"
    );
    assert!(
        stderr.contains("No warnings found"),
        "expected a `No warnings found` note, got:\n{stderr}"
    );

    // JSON still written and valid.
    assert!(stdout.contains("Compiled"));
    let json = std::fs::read_to_string(&out).expect("JSON output should exist");
    assert!(
        serde_json::from_str::<serde_json::Value>(&json).is_ok(),
        "output should be valid JSON"
    );
}

// M39: `stc --check` runs AST-level guard lints in addition to the schema-level
// self-loop / unreachable-state checks. These two tests exercise the new
// `UnusedGuardTemplate` and `DuplicateGuardCondition` lints on inline DDL.

/// Write `ddl` to a temp file under the crate's `target/` dir (outside the
/// source tree) and run `stc --check` on it, returning the captured
/// `(stdout, stderr)`. The JSON output path is derived from `out_name`.
fn run_stc_check_ddl(ddl: &str, out_name: &str) -> (String, String) {
    let out = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!("stc_check_{out_name}.json"));
    let ddl_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!("stc_check_{out_name}.ddl"));
    std::fs::write(&ddl_path, ddl).expect("temp ddl should be writable");

    let (stdout, stderr) = run_stc_check(ddl_path.to_str().unwrap(), &out);
    // Leave the temp files for inspection; they live under target/.

    (stdout, stderr)
}

#[test]
fn stc_check_reports_unused_guard_template() {
    // `lonely` is declared at the top level but never referenced by a reaction;
    // `used` is referenced. Only `lonely` should be flagged.
    let ddl = r#"
guard used {
    payload.amount <= 100
}
guard lonely {
    payload.x == 1
}
signal order {
    states: [pending, paid]
    initial: pending
    on pay from pending -> paid
}
reaction {
    when order enters paid -> order pay when used
}
"#;
    let (_stdout, stderr) = run_stc_check_ddl(ddl, "unused_guard");

    assert!(
        stderr.contains("unused-guard-template"),
        "expected an UnusedGuardTemplate warning on stderr, got:\n{stderr}"
    );
    assert!(
        stderr.contains("lonely"),
        "expected the unused guard to be named, got:\n{stderr}"
    );
    // The used guard must NOT be reported as unused.
    assert!(
        !stderr.contains("unused guard 'used'"),
        "`used` is referenced and must not be flagged, got:\n{stderr}"
    );
}

#[test]
fn stc_check_reports_duplicate_guard_condition() {
    // `g1` and `g2` declare identical expressions; both should be reported.
    let ddl = r#"
guard g1 {
    payload.amount <= 100
}
guard g2 {
    payload.amount <= 100
}
signal order {
    states: [pending, paid]
    initial: pending
    on pay from pending -> paid
}
"#;
    let (_stdout, stderr) = run_stc_check_ddl(ddl, "duplicate_guard");

    assert!(
        stderr.contains("duplicate-guard-condition"),
        "expected a DuplicateGuardCondition warning on stderr, got:\n{stderr}"
    );
    assert!(
        stderr.contains("g1") && stderr.contains("g2"),
        "expected both duplicate guards to be named, got:\n{stderr}"
    );
}
