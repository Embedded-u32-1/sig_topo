use signal_topology::run::{load_topology_for_run, run_scenario, Scenario};
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use std::{env, fs, path::Path, process};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: stp save <topology.json> <scenario.json> <state.json>");
        eprintln!("       stp reload <topology.json> <new_topology.json> <state.json>");
        process::exit(1);
    }

    match args[1].as_str() {
        "save" => {
            if args.len() != 5 {
                eprintln!("Usage: stp save <topology.json> <scenario.json> <state.json>");
                process::exit(1);
            }
            cmd_save(&args[2], &args[3], &args[4]);
        }
        "reload" => {
            if args.len() != 5 {
                eprintln!("Usage: stp reload <topology.json> <new_topology.json> <state.json>");
                process::exit(1);
            }
            cmd_reload(&args[2], &args[3], &args[4]);
        }
        _ => {
            eprintln!("Unknown subcommand: {}", args[1]);
            eprintln!("Usage: stp save <topology.json> <scenario.json> <state.json>");
            eprintln!("       stp reload <topology.json> <new_topology.json> <state.json>");
            process::exit(1);
        }
    }
}

fn cmd_save(topology_path: &str, scenario_path: &str, state_path: &str) {
    // Shared fail set: each scenario event may name `fail_actions` to force those
    // actions to fail for that event alone (injection is scoped per event and
    // cleared afterwards). stp is a batch persist tool, so `record = false`.
    let fail_set = Rc::new(RefCell::new(HashSet::new()));
    let mut engine = load_topology_for_run(topology_path, Some(Rc::clone(&fail_set)), false);

    let scenario_json = fs::read_to_string(scenario_path).unwrap_or_else(|e| {
        eprintln!("Failed to read scenario '{}': {}", scenario_path, e);
        process::exit(1);
    });
    let scenario: Scenario = serde_json::from_str(&scenario_json).unwrap_or_else(|e| {
        eprintln!("Failed to parse scenario JSON: {}", e);
        process::exit(1);
    });

    // Replay records every failure (with the rolled-back state) and continues
    // with the next event instead of stopping, mirroring stt's record-and-
    // continue behaviour -- a replay transcript is most useful when it shows
    // the failure in context with the events that follow.
    for err in run_scenario(&mut engine, &scenario, &fail_set) {
        eprintln!(
            "Error sending event {}.{}: {}",
            err.signal_id, err.event, err.error
        );
        eprintln!("State rolled back to '{}'", err.state_after);
    }

    if let Err(e) = engine.save_state(Path::new(state_path)) {
        eprintln!("Failed to save state '{}': {}", state_path, e);
        process::exit(1);
    }
    println!("State saved to {}", state_path);
}

fn cmd_reload(topology_path: &str, new_topology_path: &str, state_path: &str) {
    let mut engine = load_topology_for_run(topology_path, None, false);

    if let Err(e) = engine.load_state(Path::new(state_path)) {
        eprintln!("Failed to load state '{}': {}", state_path, e);
        process::exit(1);
    }

    let new_topology_json = fs::read_to_string(new_topology_path).unwrap_or_else(|e| {
        eprintln!("Failed to read new topology '{}': {}", new_topology_path, e);
        process::exit(1);
    });

    if let Err(e) = engine.reload_topology(&new_topology_json) {
        eprintln!("Failed to reload topology: {}", e);
        process::exit(1);
    }

    if let Err(e) = engine.save_state(Path::new(state_path)) {
        eprintln!("Failed to save state '{}': {}", state_path, e);
        process::exit(1);
    }
    println!("Topology reloaded and state saved to {}", state_path);
}
