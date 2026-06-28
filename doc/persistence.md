# State Persistence and Hot-Reload

The signal topology engine supports lightweight state persistence and hot-reload. This allows a process to save its current signal states to a JSON file, restore them later, and load a new topology description without restarting.

## State Snapshot Format

A state snapshot is a JSON object with a single `states` field that maps signal IDs to their current state names:

```json
{
  "states": {
    "task_status": "running",
    "payment": "processed"
  }
}
```

The `StateSnapshot` struct is defined in `src/persist.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSnapshot {
    pub states: HashMap<String, String>,
}
```

## Saving and Loading State

`TopologyEngine` provides two methods for persistence:

```rust
pub fn save_state(&self, path: &Path) -> Result<(), EngineError>
pub fn load_state(&mut self, path: &Path) -> Result<(), EngineError>
```

Example:

```rust
use signal_topology::TopologyEngine;
use std::path::Path;

let mut engine = TopologyEngine::from_json(topology_json)?;
engine.send_event("task_status", "start", None)?;

engine.save_state(Path::new("state.json"))?;

let mut engine2 = TopologyEngine::from_json(topology_json)?;
engine2.load_state(Path::new("state.json"))?;
assert_eq!(engine2.get_state("task_status")?, "running");
```

### Validation

`load_state` validates the snapshot against the current topology:

- Every signal in the snapshot must exist in the current topology, otherwise `PersistenceError` is returned.
- Every restored state must be listed in the signal's `states`, otherwise `StateNotFound { signal, state }` is returned.

## Hot-Reloading Topology

`TopologyEngine::reload_topology` loads a new topology JSON while preserving existing signal states:

```rust
pub fn reload_topology(&mut self, json_str: &str) -> Result<(), EngineError>
```

Semantics:

- The new topology is parsed and validated. Any error returns `ReloadError`.
- Signals present in both the old and new topologies keep their current state.
- Signals only in the new topology are initialized to their `initial_state`.
- Signals removed in the new topology are dropped; their states are lost.
- Traces are kept for observability continuity.

Example:

```rust
engine.send_event("task_status", "start", None)?;
engine.reload_topology(new_topology_json)?;
// task_status remains "running" if it still exists in the new topology
```

## CLI Usage

The `stp` (signal-topology-persist) binary provides two subcommands.

### `stp save`

Load a topology, run a scenario, and persist the resulting state:

```bash
cargo run --bin stp -- save topology.json scenario.json state.json
```

All actions referenced by the topology are registered as no-ops, then the scenario events are sent and the final state is written to `state.json`.

### `stp reload`

Load a topology, restore a saved state, reload a new topology, and write the merged state back:

```bash
cargo run --bin stp -- reload topology.json new_topology.json state.json
```

This is useful for demonstrating hot-reload: existing signals keep their states, new signals get their initial states, and removed signals are dropped.

## Error Types

Two new error variants support persistence and reload:

```rust
#[error("Reload error: {0}")]
ReloadError(String),

#[error("Persistence error: {0}")]
PersistenceError(String),
```

- `ReloadError` wraps parse or validation failures during `reload_topology`.
- `PersistenceError` wraps file I/O, JSON parse errors, and unknown signals during `load_state`.
