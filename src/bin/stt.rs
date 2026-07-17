use signal_topology::run::{format_trace, load_topology_for_run, run_scenario, Scenario};
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use std::{env, fs, process};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: stt <topology.json> <scenario.json>");
        process::exit(1);
    }

    let topology_path = &args[1];
    let scenario_path = &args[2];

    // A shared, mutable set of action ids forced to fail. Each scenario event
    // may name `fail_actions` to inject into it for that event alone; the
    // handlers read the set the same way the `sts` shell's `fail` command does.
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

    // Replay records every failure (with the rolled-back state, mirroring the
    // shell's `event` error output) and continues with the next event instead
    // of stopping -- a replay transcript is most useful when it shows the
    // `ActionFailed` + `Rollbacked` in context with the events that follow.
    for err in run_scenario(&mut engine, &scenario, &fail_set) {
        eprintln!("Error sending event {}.{}: {}", err.signal_id, err.event, err.error);
        eprintln!("State rolled back to '{}'", err.state_after);
    }

    for event in engine.traces() {
        println!("{}", format_trace(event));
    }
}
