//! M49: end-to-end test for the `sts` `dot-ext` command's new SVG rendering.
//!
//! The command's visible behavior is now the DOT printed by `dot-ext` *plus* an
//! SVG side effect written next to the topology, so this test drives a real
//! guard-bearing scenario (the same one `snapshot_dot_extended` colors after a
//! single transition), captures what the command prints, and asserts:
//!   * the printed DOT carries the guard-result color attributes (green / gray /
//!     red) that M42 introduced, and
//!   * rendering an SVG through the system `dot` produces an SVG file next to
//!     the topology.
//!
//! The `sts` binary is not invoked (`main()` is untestable IO); the focus is
//! the `dot-ext` --> DOT + SVG path the REPL's `dot-ext` command exercises.
//! Because `dot-ext` prints to stdout and writes a file, the test drives it
//! against a topology copied into a uniquely-named temp dir so no fixture is
//! mutated.

use signal_topology::export::{render_dot_to_svg, SvgOutcome};
use signal_topology::run::load_topology_for_run;
use std::cell::RefCell;
use std::collections::HashSet;
use std::process::Command;
use std::rc::Rc;

// A self-contained four-signal topology: driving `A.go` commits A to `a1` and
// triggers three reactions keyed on `A` entering `a1`. Their three guards
// evaluate to `true`, `false`, and `error` against the same payload, so the
// extended DOT exercises every guard-result color on distinct edges in one
// snapshot — exactly the GUARD_CASCADE case from `snapshot_dot_test.rs`, kept
// here inline so this test needs no external fixture.
const GUARD_CASCADE: &str = r#"{
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

/// Build a ready-to-run engine the way `sts` does (`load_topology_for_run`)
/// from a topology file at `path`, sharing a `fail_set` with the registered
/// print-and-record actions.
fn load_engine_for_run(topology_path: &str) -> signal_topology::TopologyEngine {
    load_topology_for_run(topology_path, Some(Rc::new(RefCell::new(HashSet::new()))), true)
}

/// A throwaway directory under the system temp dir, unique per run. Keeps the
/// side-effect SVG off the checkout; `TempDir` removes it on drop so tests do
/// not accumulate files.
struct TempDir {
    path: std::path::PathBuf,
}

impl TempDir {
    fn new(name: &str) -> Self {
        let mut path = std::env::temp_dir();
        path.push(format!("sig_topo_m49_{}_{}", name, std::process::id()));
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }

    fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        // Best-effort cleanup; a missing dir is fine at teardown.
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Keep a `TempDir` alive for the scope of a test body via RAII.
fn temp_run_dir(name: &str) -> TempDir {
    TempDir::new(name)
}

/// `dot-ext` prints the extended DOT with every guard-result color attribute,
/// then renders the SVG next to the topology when Graphviz is installed.
#[test]
fn dot_ext_prints_guard_colored_dot_and_renders_svg() {
    let dir = temp_run_dir("colors");
    let topo_path = dir.path().join("cascade.json");
    std::fs::write(&topo_path, GUARD_CASCADE).expect("write topology");

    let mut engine = load_engine_for_run(topo_path.to_str().expect("utf-8 path"));

    // Drive `A.go` with `enable == true`: the B reaction's guard is `true`
    // (solid green), C's is `false` (dashed gray), D's `payload.x + "s"` errors
    // on a missing `x` (dashed red). The main transition commits.
    engine
        .send_event("A", "go", Some(serde_json::json!({ "enable": true })))
        .expect("A.go should commit");
    assert_eq!(engine.get_state("A").unwrap(), "a1");

    // The DOT `dot-ext` prints is the engine's extended snapshot. Pull and
    // assert the guard-result color attributes — this is the literal output a
    // user pastes or pipes to Graphviz.
    let dot = engine.snapshot_dot_extended();

    assert!(
        dot.contains("n_A_a1 -> n_B_b0 [label=\"react [guard: true]\" color=green style=solid]"),
        "guard=true edge should be solid green; got:\n{}",
        dot
    );
    assert!(
        dot.contains("n_A_a1 -> n_C_c0 [label=\"react [guard: false]\" color=gray style=dashed]"),
        "guard=false edge should be dashed gray; got:\n{}",
        dot
    );
    assert!(
        dot.contains("n_A_a1 -> n_D_d0") && dot.contains("color=red style=dashed"),
        "guard=error edge should be dashed red; got:\n{}",
        dot
    );
    // Live-state highlight is preserved alongside the reaction edges.
    assert!(dot.contains(
        "n_A_a1 [label=\"a1\" style=filled fillcolor=lightgreen penwidth=2]"
    ));

    // Render the SVG to the path `dot-ext` would choose beside the topology.
    let svg_path = dir.path().join("cascade_guarded.svg");
    match render_dot_to_svg(&dot, &svg_path) {
        SvgOutcome::Generated => {
            let bytes = std::fs::read(&svg_path).expect("SVG should be readable when Generated");
            assert!(
                bytes.windows(4).any(|w| w == b"<svg"),
                "rendered SVG should contain '<svg'; got:\n{}",
                String::from_utf8_lossy(&bytes)
            );
        }
        SvgOutcome::GraphvizNotInstalled => {
            // No `dot` in this environment: the SVG side effect is correctly
            // skipped and the file should not exist.
            assert!(
                !svg_path.exists(),
                "no SVG should be written when Graphviz is absent"
            );
        }
        SvgOutcome::Failed(msg) => panic!("guard DOT should render cleanly; got: {}", msg),
    }
}

/// When Graphviz is absent, `dot-ext` renders no SVG and reports it — it must
/// not panic or write a partial file. With `dot` present (the normal M49 case)
/// the outcome is `Generated`, and the written SVG is a valid document header.
#[test]
fn dot_ext_svg_outcome_is_generated_with_graphviz() {
    let dot = engine_extended_dot();

    if Command::new("dot").arg("-V").output().is_err() {
        // Graphviz absent: the "generated" branch is not exercisable here.
        assert_eq!(
            render_dot_to_svg(&dot, &std::env::temp_dir().join("sig_topo_unreachable.svg")),
            SvgOutcome::GraphvizNotInstalled
        );
        return;
    }

    let svg = std::env::temp_dir().join(format!("sig_topo_m49_ok_{}.svg", std::process::id()));
    assert_eq!(render_dot_to_svg(&dot, &svg), SvgOutcome::Generated);
    let bytes = std::fs::read(&svg).expect("SVG readable");
    assert!(bytes.windows(4).any(|w| w == b"<svg"), "SVG should contain '<svg'");
    std::fs::remove_file(&svg).expect("cleanup svg");
}

/// Reusable extended DOT for the SVG-outcome test: the same GUARD_CASCADE
/// driven to the point where all three guard colors are present.
fn engine_extended_dot() -> String {
    let dir = temp_run_dir("outcome");
    let topo_path = dir.path().join("cascade.json");
    std::fs::write(&topo_path, GUARD_CASCADE).expect("write topology");
    let mut engine = load_engine_for_run(topo_path.to_str().expect("utf-8 path"));
    engine
        .send_event("A", "go", Some(serde_json::json!({ "enable": true })))
        .expect("A.go should commit");
    engine.snapshot_dot_extended()
}
