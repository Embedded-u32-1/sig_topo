use signal_topology::export::{render_dot_to_svg, SvgOutcome};
use signal_topology::run::{format_trace, load_topology_for_run};
use signal_topology::TopologyEngine;
use std::cell::RefCell;
use std::collections::HashSet;
use std::io::{self, Write};
use std::path::Path;
use std::rc::Rc;
use std::{env, process};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: sts <topology.json>");
        process::exit(1);
    }

    let topology_path = &args[1];
    let mut session = StsSession::new(topology_path);

    println!(
        "sts (signal-topology-shell). Topology loaded from '{}'.",
        topology_path
    );
    println!("Type 'help' for commands. Type 'quit' to exit.");

    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut line = String::new();

    loop {
        print!("sts> ");
        stdout.flush().unwrap_or(());
        line.clear();
        match stdin.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(e) => {
                eprintln!("Read error: {}", e);
                break;
            }
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match parse_command(line) {
            Ok(Command::Event { signal, event, payload }) => {
                cmd_event(&mut session, &signal, &event, payload);
            }
            Ok(Command::State) => cmd_state(&session),
            Ok(Command::Trace) => cmd_trace(&session),
            Ok(Command::Help) => cmd_help(),
            Ok(Command::Quit) => break,
            Ok(Command::Fail { action_id }) => session.fail(&action_id),
            Ok(Command::Reset) => session.reset(),
            Ok(Command::Dot) => cmd_dot(&session),
            Ok(Command::DotExt { svg }) => cmd_dot_ext(&session, svg),
            Ok(Command::Why {
                from_signal,
                from_state,
                to_signal,
                event,
            }) => cmd_why(&session, &from_signal, &from_state, &to_signal, &event),
            Ok(Command::Unknown(_)) => println!("Unknown command. Type 'help'."),
            Err(ParseError::EventArgs) => {
                println!("Usage: event <signal> <event> [json payload]");
            }
            Err(ParseError::FailActionId) => println!("Usage: fail <action_id>"),
            Err(ParseError::WhyArgs) => {
                println!("Usage: why <from_signal> <from_state> <to_signal> <event>");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Session: engine + a shared, mutable set of action ids forced to fail next run.
// ---------------------------------------------------------------------------

/// Bundles the engine with the shared "force this action to fail" set that both
/// the registered action closures and the REPL dispatch manipulate. The set is
/// the only state `sts` keeps beyond the engine; every other behavior flows
/// from `engine` + `fail_set`.
struct StsSession {
    engine: TopologyEngine,
    fail_set: Rc<RefCell<HashSet<String>>>,
    // The topology file the session was built from, kept so `dot-ext` can name
    // its `<stem>_guarded.svg` next to the source file (mirroring `stv`).
    topology_path: String,
}

impl StsSession {
    /// Resolve includes + expand instances, build the engine, and register
    /// every action with a print-and-record handler that *also* consults the
    /// shared `fail_set`: if the action's id is in the set, it returns an
    /// `ActionExecutionError` so the engine rolls the transition back.
    fn new(topology_path: &str) -> Self {
        let fail_set = Rc::new(RefCell::new(HashSet::new()));
        let engine = load_topology_for_run(topology_path, Some(Rc::clone(&fail_set)), true);
        Self {
            engine,
            fail_set,
            topology_path: topology_path.to_string(),
        }
    }

    /// Mark `action_id` to fail on its next (and every subsequent) execution
    /// until the set is cleared with `reset`. Live rollback demo: the user
    /// watches a transition succeed, runs `fail <action>`, re-sends the event,
    /// and sees the engine return `Error` + roll the state back.
    fn fail(&mut self, action_id: &str) {
        self.fail_set.borrow_mut().insert(action_id.to_string());
        println!("will fail next: {}", action_id);
    }

    /// Clear every forced-failure marker. Actions registered by this session
    /// resume succeeding until `fail` is used again.
    fn reset(&mut self) {
        self.fail_set.borrow_mut().clear();
        println!("fail set cleared");
    }
}

// ---------------------------------------------------------------------------
// Command parsing (pure, unit-tested below).
// ---------------------------------------------------------------------------

/// A parsed REPL command. `Command` carries everything the dispatch loop needs;
/// parsing stdin into it is a pure function so it can be unit-tested without IO.
#[derive(Debug, PartialEq)]
enum Command {
    Event {
        signal: String,
        event: String,
        payload: Option<String>,
    },
    State,
    Trace,
    Help,
    Quit,
    Fail {
        action_id: String,
    },
    Reset,
    Dot,
    DotExt {
        // When true, render the SVG but suppress the stdout DOT dump — the
        // "explicit SVG" mode of `dot-ext --svg`.
        svg: bool,
    },
    Why {
        from_signal: String,
        from_state: String,
        to_signal: String,
        event: String,
    },
    Unknown(String),
}

/// Errors `parse_command` can report. Kept small and explicit so the dispatch
/// loop maps each variant to the right user-facing message.
#[derive(Debug, PartialEq)]
enum ParseError {
    /// `event` was given without the required `<signal> <event>`.
    EventArgs,
    /// `fail` was given without the required `<action_id>`.
    FailActionId,
    /// `why` was given without the required `<from> <state> <to> <event>`.
    WhyArgs,
}

/// Parse a raw REPL line into a `Command`. Pure: no IO, no engine, no globals.
///
/// `event <signal> <event> [json payload]` keeps the JSON intact by slicing the
/// remainder of the line off the `event <signal> <event>` prefix rather than
/// rejoining `split_whitespace` tokens — so compact and spaced JSON both work.
fn parse_command(line: &str) -> Result<Command, ParseError> {
    let line = line.trim();
    let parts: Vec<&str> = line.split_whitespace().collect();
    match parts.first().copied() {
        None => Ok(Command::Unknown(String::new())),
        Some("event") => {
            if parts.len() < 3 {
                return Err(ParseError::EventArgs);
            }
            let signal = parts[1];
            let event = parts[2];
            let prefix = format!("event {} {}", signal, event);
            let payload = line
                .strip_prefix(&prefix)
                .map(str::trim_start)
                .filter(|s| !s.is_empty())
                .map(String::from);
            Ok(Command::Event {
                signal: signal.to_string(),
                event: event.to_string(),
                payload,
            })
        }
        Some("state") => Ok(Command::State),
        Some("trace") => Ok(Command::Trace),
        Some("help") => Ok(Command::Help),
        Some("quit") | Some("exit") => Ok(Command::Quit),
        Some("fail") => match parts.get(1) {
            Some(id) => Ok(Command::Fail {
                action_id: id.to_string(),
            }),
            None => Err(ParseError::FailActionId),
        },
        Some("reset") => Ok(Command::Reset),
        Some("dot") => Ok(Command::Dot),
        Some("dot-ext") => {
            // `dot-ext` accepts an optional `--svg` flag: render the SVG and
            // skip the stdout DOT dump. Any other trailing token is ignored,
            // matching the lenient parsing of the rest of the REPL.
            let svg = parts.get(1) == Some(&"--svg");
            Ok(Command::DotExt { svg })
        }
        Some("why") => {
            if parts.len() < 5 {
                return Err(ParseError::WhyArgs);
            }
            Ok(Command::Why {
                from_signal: parts[1].to_string(),
                from_state: parts[2].to_string(),
                to_signal: parts[3].to_string(),
                event: parts[4].to_string(),
            })
        }
        Some(other) => Ok(Command::Unknown(other.to_string())),
    }
}

// ---------------------------------------------------------------------------
// Dispatch helpers. Each does engine IO + user-facing output; parsing has
// already been done by `parse_command`.
// ---------------------------------------------------------------------------

/// `event <signal> <e> [json payload]` — send an event. The payload (if any) is
/// the remainder of the line after the event name, parsed as JSON.
fn cmd_event(session: &mut StsSession, signal: &str, event: &str, payload_arg: Option<String>) {
    let parsed_payload = match payload_arg {
        None => None,
        Some(text) => match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(v) => Some(v),
            Err(e) => {
                println!("Invalid JSON payload: {}", e);
                return;
            }
        },
    };

    match session.engine.send_event(signal, event, parsed_payload) {
        Ok(result) => {
            println!("{} -> {}", result.signal_id, result.to);
            for action in &result.executed_actions {
                println!("  action executed: {}", action);
            }
        }
        Err(e) => {
            println!("Error: {}", e);
            if let Ok(state) = session.engine.get_state(signal) {
                println!("State rolled back to '{}'", state);
            }
        }
    }
}

/// `state` — list every signal and its current state.
fn cmd_state(session: &StsSession) {
    let mut ids = session.engine.signal_ids();
    ids.sort();
    for id in ids {
        if let Ok(state) = session.engine.get_state(id) {
            println!("{}: {}", id, state);
        }
    }
}

/// `trace` — print the full trace log in stt format.
fn cmd_trace(session: &StsSession) {
    let events = session.engine.traces();
    if events.is_empty() {
        println!("(no trace events)");
        return;
    }
    for event in events {
        println!("{}", format_trace(event));
    }
}

/// `dot` — print the topology as Graphviz DOT with every signal's *current*
/// state highlighted lightgreen. Paste the output into Graphviz (or `dot`
/// directly) to render it. The highlight follows the engine: send an event
/// and run `dot` again to see the live node move.
fn cmd_dot(session: &StsSession) {
    print!("{}", session.engine.snapshot_dot());
}

/// `dot-ext` — print the extended DOT: live-state highlighting plus
/// cross-signal reaction edges colored by their guard-evaluation result. A
/// green solid edge fired, a gray dashed edge was guard-blocked, a red dashed
/// edge's guard errored, and a black dashed edge was never evaluated this run.
/// This is the visual companion to `why`: `why` tells you the guard result for
/// one reaction as text, `dot-ext` shows every reaction at a glance.
///
/// After printing the DOT to stdout this also renders an SVG through the
/// system `dot` (when it is on PATH): `<topology_stem>_guarded.svg` next to
/// the topology file, and prints its path. The SVG is printed to stdout as
/// well (pipe-friendly), but `dot-ext --svg` renders only the SVG — the DOT
/// dump is suppressed for a clean redirect. With no Graphviz the command
/// falls back to the plain DOT and suggests installing it, exactly as before.
fn cmd_dot_ext(session: &StsSession, svg_only: bool) {
    // Lock stdout once and run the shared, testable implementation against it
    // so the REPL prints exactly what `dot_ext` writes.
    let mut stdout = io::stdout();
    dot_ext(
        &session.engine.snapshot_dot_extended(),
        svg_only,
        &mut stdout,
        &session.topology_path,
    )
    .expect("writing DOT and status to stdout should succeed");
}

/// Emit the extended guard DOT (unless `svg_only`) and render the guarded SVG
/// for the topology at `topology_path`. All output — DOT, status line, and
/// the failure note — goes to `out`, so the function is unit-testable: a test
/// can pass a `Vec<u8>` and assert on exactly what `dot-ext` prints. The SVG
/// itself is written next to the topology as a side effect of rendering.
///
/// Mirror of the `format_why` / `cmd_why` split: the testable core lives here,
/// the thin REPL wrapper is `cmd_dot_ext`.
fn dot_ext(
    dot: &str,
    svg_only: bool,
    out: &mut impl Write,
    topology_path: &str,
) -> io::Result<()> {
    if !svg_only {
        write!(out, "{}", dot)?;
    }

    let svg_path = guarded_svg_path(topology_path);
    match render_dot_to_svg(dot, &svg_path) {
        SvgOutcome::Generated => writeln!(out, "Generated {}", svg_path.display()),
        SvgOutcome::GraphvizNotInstalled => writeln!(
            out,
            "Graphviz 'dot' not found in PATH. Install Graphviz to generate '{}'.",
            svg_path.display()
        ),
        SvgOutcome::Failed(msg) => writeln!(
            out,
            "{} SVG was not generated for '{}'.",
            msg,
            svg_path.display()
        ),
    }
}

/// The path `dot-ext` writes its SVG to: next to the topology file, named
/// `<topology_stem>_guarded.svg`. Falls back to the working directory with a
/// `topology_guarded.svg` name when the path has no usable stem or parent.
fn guarded_svg_path(topology_path: &str) -> std::path::PathBuf {
    let path = Path::new(topology_path);
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("topology");
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    parent.join(format!("{}_guarded.svg", stem))
}

/// `why <from> <state> <to> <event>` — print every `ReactionGuardEvaluated` trace
/// event for the reaction `from.state -> to.event`, so a user can see why it
/// fired (`result=true`), was skipped (`result=false`), or was skipped because
/// the guard failed to evaluate (`result=error: ...`). The engine records one
/// such event per reaction dispatch (M38); if none is found for the requested
/// reaction, the from_state was likely never reached (so the reaction was never
/// evaluated) and we say so.
fn cmd_why(
    session: &StsSession,
    from_signal: &str,
    from_state: &str,
    to_signal: &str,
    event: &str,
) {
    print!("{}", signal_topology::run::format_why(
        session.engine.traces(),
        from_signal,
        from_state,
        to_signal,
        event,
    ));
}

/// `help` — describe the available commands.
fn cmd_help() {
    println!("Commands:");
    println!("  event <signal> <event> [json payload]  send an event to a signal");
    println!("  state                                   list all signal states");
    println!("  trace                                   print the trace log");
    println!("  dot                                     print runtime-highlighted DOT");
    println!("  dot-ext [--svg]                         print extended DOT with guard-eval result edges (+ render SVG)");
    println!("  fail <action_id>                        force that action to fail");
    println!("  reset                                   clear forced-failure set");
    println!("  why <from> <state> <to> <event>         print guard evaluation trace for a reaction");
    println!("  help                                    show this help");
    println!("  quit / exit                             leave the shell");
}

// ---------------------------------------------------------------------------
// Unit tests for the pure command-parsing layer.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use signal_topology::EngineError;

    #[test]
    fn parse_event_without_payload() {
        let cmd = parse_command("event order submit").unwrap();
        assert_eq!(
            cmd,
            Command::Event {
                signal: "order".to_string(),
                event: "submit".to_string(),
                payload: None,
            }
        );
    }

    #[test]
    fn parse_event_with_compact_json_payload() {
        let cmd = parse_command(r#"event order approve {"amount":5000}"#).unwrap();
        assert_eq!(
            cmd,
            Command::Event {
                signal: "order".to_string(),
                event: "approve".to_string(),
                payload: Some(r#"{"amount":5000}"#.to_string()),
            }
        );
    }

    #[test]
    fn parse_event_with_spaced_json_payload() {
        let cmd = parse_command(r#"event order approve { "amount" : 5000 }"#).unwrap();
        assert_eq!(
            cmd,
            Command::Event {
                signal: "order".to_string(),
                event: "approve".to_string(),
                payload: Some(r#"{ "amount" : 5000 }"#.to_string()),
            }
        );
    }

    #[test]
    fn parse_event_missing_args_is_error() {
        assert_eq!(parse_command("event"), Err(ParseError::EventArgs));
        assert_eq!(
            parse_command("event order"),
            Err(ParseError::EventArgs)
        );
    }

    #[test]
    fn parse_state_trace_help_quit() {
        assert_eq!(parse_command("state").unwrap(), Command::State);
        assert_eq!(parse_command("trace").unwrap(), Command::Trace);
        assert_eq!(parse_command("help").unwrap(), Command::Help);
        assert_eq!(parse_command("quit").unwrap(), Command::Quit);
        assert_eq!(parse_command("exit").unwrap(), Command::Quit);
    }

    #[test]
    fn parse_fail_and_reset() {
        assert_eq!(
            parse_command("fail reserve_inventory").unwrap(),
            Command::Fail {
                action_id: "reserve_inventory".to_string(),
            }
        );
        assert_eq!(
            parse_command("fail").unwrap_err(),
            ParseError::FailActionId
        );
        assert_eq!(parse_command("reset").unwrap(), Command::Reset);
    }

    #[test]
    fn parse_dot_ext_command() {
        assert_eq!(
            parse_command("dot-ext").unwrap(),
            Command::DotExt { svg: false }
        );
    }

    /// `dot-ext --svg` is parsed as the SVG-only variant (no stdout DOT dump).
    #[test]
    fn parse_dot_ext_svg_flag() {
        assert_eq!(
            parse_command("dot-ext --svg").unwrap(),
            Command::DotExt { svg: true }
        );
    }

    /// `guarded_svg_path` mirrors `stv`'s layout: `<stem>_guarded.svg` next to
    /// the topology. Proves the SVG lands beside the source, not in the cwd.
    #[test]
    fn guarded_svg_path_is_next_to_topology_with_guarded_stem() {
        assert_eq!(
            guarded_svg_path("examples/order_approval.json"),
            std::path::PathBuf::from("examples/order_approval_guarded.svg")
        );
        assert_eq!(
            guarded_svg_path("a/b/topology.json"),
            std::path::PathBuf::from("a/b/topology_guarded.svg")
        );
    }

    /// With no usable stem or parent, the SVG falls back to a `topology`
    /// name in the working directory (parent `.`) rather than panicking.
    #[test]
    fn guarded_svg_path_falls_back_when_path_is_bare() {
        assert_eq!(
            guarded_svg_path(""),
            std::path::PathBuf::from("./topology_guarded.svg")
        );
    }

    /// `dot_ext` writes the extended DOT to the given writer before prompting
    /// SVG rendering — proving the command's stdout is the guard-colored DOT,
    /// and that it is suppressed in `--svg` mode.
    #[test]
    fn dot_ext_writes_guard_dot_to_writer_and_suppresses_in_svg_mode() {
        // Capture `dot_ext`'s stdout in a buffer via the writer it accepts.
        let session = StsSession::new("examples/order_approval.json");
        // There are no reactions in this topology, so the extended DOT is just
        // the live-state-highlighted view — enough to pin down the writer path.
        let dot = session.engine.snapshot_dot_extended();
        assert!(
            dot.contains("digraph Topology"),
            "extended DOT should open a digraph"
        );

        let mut out = Vec::new();
        dot_ext(&dot, false, &mut out, &session.topology_path).expect("write succeeds");
        let printed = String::from_utf8(out).expect("utf-8");
        assert!(
            printed.contains("digraph Topology"),
            "default `dot-ext` should print the DOT; got:\n{}",
            printed
        );

        // `--svg` mode: the DOT dump is suppressed; only the status line remains.
        let mut svg_out = Vec::new();
        dot_ext(&dot, true, &mut svg_out, &session.topology_path).expect("write succeeds");
        let svg_printed = String::from_utf8(svg_out).expect("utf-8");
        assert!(
            !svg_printed.contains("digraph Topology"),
            "`dot-ext --svg` should suppress the DOT dump; got:\n{}",
            svg_printed
        );
    }

    #[test]
    fn parse_why_four_args() {
        let cmd = parse_command("why order approved inventory allocate").unwrap();
        assert_eq!(
            cmd,
            Command::Why {
                from_signal: "order".to_string(),
                from_state: "approved".to_string(),
                to_signal: "inventory".to_string(),
                event: "allocate".to_string(),
            }
        );
    }

    #[test]
    fn parse_why_missing_args_is_error() {
        assert_eq!(parse_command("why"), Err(ParseError::WhyArgs));
        assert_eq!(
            parse_command("why order approved inventory"),
            Err(ParseError::WhyArgs)
        );
    }

    #[test]
    fn parse_unknown_command() {
        assert_eq!(
            parse_command("bogus").unwrap(),
            Command::Unknown("bogus".to_string())
        );
        assert_eq!(
            parse_command("bogus arg").unwrap(),
            Command::Unknown("bogus".to_string())
        );
    }

    /// End-to-end through the real registration path: loading the example
    /// topology builds a session whose every action succeeds until `fail`
    /// marks one. The example topology is resolved relative to the crate root
    /// by `load_topology`, which is how the shell itself loads it.
    #[test]
    fn session_new_registers_all_actions() {
        let session = StsSession::new("examples/order_approval.json");
        assert_eq!(session.engine.get_state("order").unwrap(), "draft");
        let mut ids = session.engine.signal_ids();
        ids.sort();
        assert_eq!(ids, vec!["order"]);
    }

    /// `fail` mutates the shared set that the registered closures read; until
    /// `reset` clears it, the marked action keeps failing. Proves fail_set is
    /// wired end-to-end without IO.
    #[test]
    fn session_fail_and_reset_share_state_with_closures() {
        let mut session = StsSession::new("examples/order_approval.json");

        // Succeeds first.
        session
            .engine
            .send_event("order", "submit", None)
            .expect("submit should succeed");
        assert_eq!(session.engine.get_state("order").unwrap(), "submitted");

        // Force the on_transition action of `approve` to fail, then re-send.
        session.fail("reserve_inventory");
        assert!(session.fail_set.borrow().contains("reserve_inventory"));

        let err = session
            .engine
            .send_event(
                "order",
                "approve",
                Some(serde_json::json!({"amount": 5000})),
            )
            .expect_err("forced action must make send_event fail");
        assert!(matches!(err, EngineError::ActionExecutionError(_)));

        // Rolled back to the source state.
        assert_eq!(session.engine.get_state("order").unwrap(), "submitted");

        // The forced failure is sticky: a second attempt also rolls back.
        let second_err = session
            .engine
            .send_event(
                "order",
                "approve",
                Some(serde_json::json!({"amount": 5000})),
            )
            .expect_err("forced action keeps failing until reset");
        assert!(matches!(
            second_err,
            EngineError::ActionExecutionError(_)
        ));

        // After reset the same event commits.
        session.reset();
        assert!(session.fail_set.borrow().is_empty());
        session
            .engine
            .send_event(
                "order",
                "approve",
                Some(serde_json::json!({"amount": 5000})),
            )
            .expect("after reset approve should succeed");
        assert_eq!(session.engine.get_state("order").unwrap(), "approved");
    }
}
