# Execution Tracing

The signal topology engine records an append-only execution trace for every event it processes. The trace is stored inside `TopologyEngine` and does not affect runtime behavior.

## Trace event model

Trace events are defined in `src/trace.rs` as the `TraceEvent` enum:

- `EventReceived { signal_id, event, timestamp_ms, payload }` — an event was sent to a signal. The optional JSON payload is serialized as a compact string.
- `ActionStarted { signal_id, action_id, timestamp_ms }` — an action bound to the transition is about to run.
- `ActionSucceeded { signal_id, action_id, timestamp_ms }` — the action completed successfully.
- `ActionFailed { signal_id, action_id, timestamp_ms, error }` — the action returned an error. The original error message is preserved.
- `StateChanged { signal_id, from, to, timestamp_ms }` — the signal state was updated.

Traces are stored in `TraceLog`, an append-only log that supports filtering by signal (`for_signal`) and by start timestamp (`since`).

## Programmatic access

`TopologyEngine` collects traces automatically during `send_event`:

```rust
use signal_topology::TopologyEngine;

let mut engine = TopologyEngine::from_json(json).unwrap();
engine.register_action("my_action", |_| Ok(()));
engine.send_event("task_status", "start", None).unwrap();

for event in engine.traces() {
    println!("{:?}", event);
}

// Filter by signal
let task_traces = engine.traces_for("task_status");

// Clear the log
engine.clear_traces();
```

## CLI trace replay

The `stt` (signal-topology-trace) binary replays a scenario JSON file against a topology and prints a human-readable timeline.

```bash
cargo run --bin stt -- tests/topology.json scenario.json
```

Scenario format:

```json
{
  "events": [
    { "signal_id": "task_status", "event": "start", "payload": null },
    { "signal_id": "task_status", "event": "finish", "payload": { "result": "ok" } }
  ]
}
```

`stt` registers no-op actions for every action id found in the topology, sends each event in order, then prints the trace. It exits with status 1 if the topology or scenario cannot be loaded or if any event fails.

### Example output

```text
[1719993600000] EventReceived task_status.start payload=None
[1719993600001] ActionStarted task_status.log_idle_leave
[1719993600002] ActionSucceeded task_status.log_idle_leave
[1719993600003] StateChanged task_status: idle -> running
[1719993600004] ActionStarted task_status.init_task_resource
[1719993600005] ActionSucceeded task_status.init_task_resource
[1719993600006] ActionStarted task_status.start_task_execution
[1719993600007] ActionSucceeded task_status.start_task_execution
```

## Troubleshooting with traces

- If `ActionFailed` appears before `StateChanged`, the failing action ran during `on_exit`; the state had not changed yet.
- If `ActionFailed` appears after `StateChanged`, the failing action ran during `on_transition` or `on_enter`; the state change is not rolled back.
- Use `traces_for(signal_id)` to isolate the timeline for a single signal.
