use signal_topology::export::{render_dot_to_svg, to_dot, to_dot_with_state, SvgOutcome};
use signal_topology::load_topology;
use signal_topology::persist::StateSnapshot;
use std::env;
use std::fs;
use std::path::Path;

/// Render `dot` to a `.dot` file at `dot_path` and, if Graphviz is on PATH,
/// render an SVG next to it (same stem, `.svg` extension). Prints the paths
/// produced.
///
/// The SVG step funnels through the shared `render_dot_to_svg` helper (piping
/// the in-memory DOT to `dot -Tsvg` via stdin) so the availability check +
/// `dot` invocation stay in sync with `sts`.
fn render_dot(dot: String, dot_path: &Path) {
    fs::write(dot_path, &dot).unwrap_or_else(|e| {
        eprintln!("Failed to write '{}': {}", dot_path.display(), e);
        std::process::exit(1);
    });
    println!("Generated {}", dot_path.display());

    let svg_path = dot_path.with_extension("svg");
    match render_dot_to_svg(&dot, &svg_path) {
        SvgOutcome::Generated => {
            println!("Generated {}", svg_path.display());
        }
        SvgOutcome::GraphvizNotInstalled => {
            println!(
                "Graphviz 'dot' not found in PATH. Install Graphviz to generate '{}'.",
                svg_path.display()
            );
        }
        SvgOutcome::Failed(msg) => {
            eprintln!("{} SVG was not generated for '{}'.", msg, svg_path.display());
        }
    }
}

/// `stv <topology.json>` — render the structural skeleton (initial states
/// highlighted lightblue).
fn skeleton_mode(input_path: &Path) {
    // load_topology resolves `includes` (relative to the file's parent
    // directory, with cycle detection) and expands parameterized
    // `instances`, returning a fully flat TopologySchema ready to render.
    let schema = load_topology(input_path).unwrap_or_else(|e| {
        eprintln!("Failed to load topology '{}': {}", input_path.display(), e);
        std::process::exit(1);
    });

    let input_stem = input_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("topology");
    let parent = input_path.parent().unwrap_or_else(|| Path::new("."));

    render_dot(to_dot(&schema), &parent.join(format!("{}.dot", input_stem)));
}

/// `stv --live <topology.json> <state.json>` — render the topology with each
/// signal's current state (from the snapshot) highlighted lightgreen. The
/// snapshot uses the same `StateSnapshot` format `stp` writes: `{"states":
/// {"<signal>": "<state>"}}`.
fn live_mode(input_path: &Path, state_path: &Path) {
    let schema = load_topology(input_path).unwrap_or_else(|e| {
        eprintln!("Failed to load topology '{}': {}", input_path.display(), e);
        std::process::exit(1);
    });

    let snapshot: StateSnapshot = {
        let text = fs::read_to_string(state_path).unwrap_or_else(|e| {
            eprintln!("Failed to read state '{}': {}", state_path.display(), e);
            std::process::exit(1);
        });
        serde_json::from_str(&text).unwrap_or_else(|e| {
            eprintln!("Failed to parse state '{}': {}", state_path.display(), e);
            std::process::exit(1);
        })
    };

    let input_stem = input_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("topology");
    let parent = input_path.parent().unwrap_or_else(|| Path::new("."));

    // `<stem>.dot` is reserved for the skeleton; the live view writes
    // `<stem>_live.dot` so the two can coexist.
    render_dot(
        to_dot_with_state(&schema, &snapshot.states),
        &parent.join(format!("{}_live.dot", input_stem)),
    );
}

fn main() {
    let args: Vec<String> = env::args().collect();
    match args.as_slice() {
        [_, topology] => skeleton_mode(Path::new(topology)),
        [_, live, topology, state] if live == "--live" => {
            live_mode(Path::new(topology), Path::new(state))
        }
        _ => {
            eprintln!("Usage: stv <topology.json>");
            eprintln!("       stv --live <topology.json> <state.json>");
            std::process::exit(1);
        }
    }
}
