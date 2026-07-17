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
            Ok(Command::Unknown(_)) => println!("Unknown command. Type 'help'."),
            Err(ParseError::MissingEventArgs) => {
                println!("Usage: event <signal> <event> [json payload]");
            }
            Err(ParseError::MissingFailActionId) => println!("Usage: fail <action_id>"),
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
    Unknown(String),
}

/// Errors `parse_command` can report. Kept small and explicit so the dispatch
/// loop maps each variant to the right user-facing message.
#[derive(Debug, PartialEq)]
enum ParseError {
    MissingEventArgs,
    MissingFailActionId,
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
                return Err(ParseError::MissingEventArgs);
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
            None => Err(ParseError::MissingFailActionId),
        },
        Some("reset") => Ok(Command::Reset),
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

/// `help` — describe the available commands.
fn cmd_help() {
    println!("Commands:");
    println!("  event <signal> <event> [json payload]  send an event to a signal");
    println!("  state                                   list all signal states");
    println!("  trace                                   print the trace log");
    println!("  fail <action_id>                        force that action to fail");
    println!("  reset                                   clear forced-failure set");
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
        assert_eq!(parse_command("event"), Err(ParseError::MissingEventArgs));
        assert_eq!(
            parse_command("event order"),
            Err(ParseError::MissingEventArgs)
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
            ParseError::MissingFailActionId
        );
        assert_eq!(parse_command("reset").unwrap(), Command::Reset);
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
