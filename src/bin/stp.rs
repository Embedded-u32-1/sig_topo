use serde::Deserialize;
use signal_topology::schema::TopologySchema;
use signal_topology::TopologyEngine;
use std::{env, fs, path::Path, process};

#[derive(Debug, Deserialize)]
struct Scenario {
    events: Vec<ScenarioEvent>,
}

#[derive(Debug, Deserialize)]
struct ScenarioEvent {
    signal_id: String,
    event: String,
    payload: Option<serde_json::Value>,
}

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
    let mut engine = load_topology(topology_path);

    let scenario_json = fs::read_to_string(scenario_path).unwrap_or_else(|e| {
        eprintln!("Failed to read scenario '{}': {}", scenario_path, e);
        process::exit(1);
    });
    let scenario: Scenario = serde_json::from_str(&scenario_json).unwrap_or_else(|e| {
        eprintln!("Failed to parse scenario JSON: {}", e);
        process::exit(1);
    });

    for ev in &scenario.events {
        if let Err(e) = engine.send_event(&ev.signal_id, &ev.event, ev.payload.clone()) {
            eprintln!(
                "Error sending event {}.{}: {}",
                ev.signal_id, ev.event, e
            );
            process::exit(1);
        }
    }

    if let Err(e) = engine.save_state(Path::new(state_path)) {
        eprintln!("Failed to save state '{}': {}", state_path, e);
        process::exit(1);
    }
    println!("State saved to {}", state_path);
}

fn cmd_reload(topology_path: &str, new_topology_path: &str, state_path: &str) {
    let mut engine = load_topology(topology_path);

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

fn load_topology(topology_path: &str) -> TopologyEngine {
    let topology_json = fs::read_to_string(topology_path).unwrap_or_else(|e| {
        eprintln!("Failed to read topology '{}': {}", topology_path, e);
        process::exit(1);
    });

    let topology_schema: TopologySchema = serde_json::from_str(&topology_json).unwrap_or_else(|e| {
        eprintln!("Failed to parse topology JSON: {}", e);
        process::exit(1);
    });

    let mut action_ids = std::collections::HashSet::new();
    for trans in &topology_schema.transitions {
        action_ids.extend(trans.actions.all_actions().into_iter().cloned());
    }

    let mut engine = TopologyEngine::from_schema(topology_schema).unwrap_or_else(|e| {
        eprintln!("Failed to load topology: {}", e);
        process::exit(1);
    });

    for action_id in &action_ids {
        engine.register_action(action_id, |_| Ok(()));
    }

    engine
}
