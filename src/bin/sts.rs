use signal_topology::run::{format_trace, load_topology_for_run};
use signal_topology::TopologyEngine;
use std::cell::RefCell;
use std::collections::HashSet;
use std::io::{self, Write};
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
            Ok(Command::DotExt) => cmd_dot_ext(&session),
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
}

impl StsSession {
    /// Resolve includes + expand instances, build the engine, and register
    /// every action with a print-and-record handler that *also* consults the
    /// shared `fail_set`: if the action's id is in the set, it returns an
    /// `ActionExecutionError` so the engine rolls the transition back.
    fn new(topology_path: &str) -> Self {
        let fail_set = Rc::new(RefCell::new(HashSet::new()));
        let engine = load_topology_for_run(topology_path, Some(Rc::clone(&fail_set)), true);
        Self { engine, fail_set }
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
    DotExt,
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
        Some("dot-ext") => Ok(Command::DotExt),
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
fn cmd_dot_ext(session: &StsSession) {
    print!("{}", session.engine.snapshot_dot_extended());
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
    println!("  dot-ext                                 print extended DOT with guard-eval result edges");
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
        assert_eq!(parse_command("dot-ext").unwrap(), Command::DotExt);
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
