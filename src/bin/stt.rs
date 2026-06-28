use serde::Deserialize;
use signal_topology::schema::TopologySchema;
use signal_topology::trace::TraceEvent;
use signal_topology::TopologyEngine;
use std::{env, fs, process};

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
    if args.len() != 3 {
        eprintln!("Usage: stt <topology.json> <scenario.json>");
        process::exit(1);
    }

    let topology_path = &args[1];
    let scenario_path = &args[2];

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

    let scenario_json = fs::read_to_string(scenario_path).unwrap_or_else(|e| {
        eprintln!("Failed to read scenario '{}': {}", scenario_path, e);
        process::exit(1);
    });

    let scenario: Scenario = serde_json::from_str(&scenario_json).unwrap_or_else(|e| {
        eprintln!("Failed to parse scenario JSON: {}", e);
        process::exit(1);
    });

    for action_id in &action_ids {
        engine.register_action(action_id, |_| Ok(()));
    }

    for ev in &scenario.events {
        if let Err(e) = engine.send_event(&ev.signal_id, &ev.event, ev.payload.clone()) {
            eprintln!(
                "Error sending event {}.{}: {}",
                ev.signal_id, ev.event, e
            );
            process::exit(1);
        }
    }

    for event in engine.traces() {
        println!("{}", format_trace(event));
    }
}

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
    }
}
