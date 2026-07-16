use signal_topology::load_topology;
use signal_topology::trace::TraceEvent;
use signal_topology::TopologyEngine;
use std::collections::HashSet;
use std::io::{self, Write};
use std::{env, process};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: sts <topology.json>");
        process::exit(1);
    }

    let topology_path = &args[1];
    let mut engine = load_topology_for_run(topology_path);

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
        let parts: Vec<&str> = line.split_whitespace().collect();
        match parts[0] {
            "event" => cmd_event(&mut engine, &parts, line),
            "state" => cmd_state(&engine),
            "trace" => cmd_trace(&engine),
            "help" => cmd_help(),
            "quit" | "exit" => break,
            _ => println!("Unknown command. Type 'help'."),
        }
    }
}

/// `event <signal> <e> [json payload]` — send an event. The payload is the
/// remainder of the line after the event name; if present it is parsed as JSON.
fn cmd_event(engine: &mut TopologyEngine, parts: &[&str], raw_line: &str) {
    if parts.len() < 3 {
        println!("Usage: event <signal> <event> [json payload]");
        return;
    }
    let signal = parts[1];
    let event = parts[2];

    // Payload is everything after `event <signal> <event> `. Locate it by slicing
    // the original line so compact or spaced JSON stays intact.
    let prefix = format!("event {} {}", signal, event);
    let payload = raw_line
        .strip_prefix(&prefix)
        .map(str::trim_start)
        .filter(|s| !s.is_empty());

    let parsed_payload = match payload {
        None => None,
        Some(text) => match serde_json::from_str::<serde_json::Value>(text) {
            Ok(v) => Some(v),
            Err(e) => {
                println!("Invalid JSON payload: {}", e);
                return;
            }
        },
    };

    match engine.send_event(signal, event, parsed_payload) {
        Ok(result) => {
            println!("{} -> {}", result.signal_id, result.to);
            for action in &result.executed_actions {
                println!("  action executed: {}", action);
            }
        }
        Err(e) => {
            println!("Error: {}", e);
            if let Ok(state) = engine.get_state(signal) {
                println!("State rolled back to '{}'", state);
            }
        }
    }
}

/// `state` — list every signal and its current state.
fn cmd_state(engine: &TopologyEngine) {
    let mut ids = engine.signal_ids();
    ids.sort();
    for id in ids {
        if let Ok(state) = engine.get_state(id) {
            println!("{}: {}", id, state);
        }
    }
}

/// `trace` — print the full trace log in stt format.
fn cmd_trace(engine: &TopologyEngine) {
    let events = engine.traces();
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
    println!("  help                                    show this help");
    println!("  quit / exit                             leave the shell");
}

/// Load a topology file into a ready-to-run engine. Resolves `includes` and
/// expands `instances` via `load_topology`, collects action ids from the
/// *expanded* transitions (so component-defined actions are registered too),
/// builds the engine, and registers every action with a print-and-record
/// handler so the whole chain is observable without writing Rust.
fn load_topology_for_run(topology_path: &str) -> TopologyEngine {
    let schema = load_topology(std::path::Path::new(topology_path)).unwrap_or_else(|e| {
        eprintln!("Failed to load topology '{}': {}", topology_path, e);
        process::exit(1);
    });

    let mut action_ids = HashSet::new();
    for trans in &schema.transitions {
        action_ids.extend(trans.actions.all_actions().into_iter().cloned());
    }

    let mut engine = TopologyEngine::from_schema(schema).unwrap_or_else(|e| {
        eprintln!("Failed to load topology: {}", e);
        process::exit(1);
    });

    for action_id in &action_ids {
        let id = action_id.clone();
        engine.register_action(action_id, move |ctx| {
            println!("[action] {}.{}", ctx.signal_id, id);
            Ok(())
        });
    }

    engine
}

/// Format a single trace event, matching the layout produced by stt.
fn format_trace(event: &TraceEvent) -> String {
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
        } => format!(
            "[{}] ActionStarted {}.{}",
            timestamp_ms, signal_id, action_id
        ),
        TraceEvent::ActionSucceeded {
            signal_id,
            action_id,
            timestamp_ms,
        } => format!(
            "[{}] ActionSucceeded {}.{}",
            timestamp_ms, signal_id, action_id
        ),
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
        } => format!(
            "[{}] StateChanged {}: {} -> {}",
            timestamp_ms, signal_id, from, to
        ),
        TraceEvent::Rollbacked {
            signal_id,
            from,
            to,
            timestamp_ms,
        } => format!(
            "[{}] Rollbacked {}: {} -> {}",
            timestamp_ms, signal_id, from, to
        ),
    }
}
