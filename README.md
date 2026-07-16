# signal-topology

A file-driven Rust state-machine engine: describe a system as a JSON topology of signals, transitions, reactions and guards, then run scenarios, persist state, trace events, and export diagrams.

## Quick Start

```bash
cargo build

# Render a topology to DOT (and SVG if Graphviz is installed)
cargo run --bin stv -- examples/cascade_topology.json

# Run a scenario and print the trace
cargo run --bin stt -- examples/cascade_topology.json scenario.json

# Persist / restore engine state
cargo run --bin stp -- save examples/cascade_topology.json scenario.json state.json
cargo run --bin stp -- reload examples/cascade_topology.json new_topology.json state.json

# Multi-file topologies (components / instances / includes)
cargo run --bin stv -- examples/components/house.json
```

`house.json` demonstrates composition: parameterized components, instances, and cross-file includes are all resolved before the engine runs. See [doc/composition.md](doc/composition.md).

# Interactive simulation

```bash
cargo run --bin sts -- examples/order_approval.json
```

`sts` (signal-topology-shell) loads a topology and drops you into a REPL where you can send events, inspect state, and print the trace log one step at a time. Walk-throughs: [examples/order_approval.md](examples/order_approval.md), [examples/gate_flow.md](examples/gate_flow.md). Full command reference: [doc/shell.md](doc/shell.md).

## Modules

| Module      | Purpose |
|-------------|---------|
| `schema`    | Topology JSON types (`SignalDef`, `TransitionDef`, `ReactionDef`, `TopologySchema`) and component/instance definitions. |
| `engine`    | `TopologyEngine`: validate a topology and drive signals through transitions with action callbacks and cascade depth limiting. |
| `compose`   | `load_topology` / `expand` / `from_path`: resolve includes (with cycle detection), expand parameterized instances, and build flat topologies. |
| `guard`     | Expression guards evaluated against signal state to allow or block a transition. |
| `trace`     | Ordered log of `TraceEvent`s (`EventReceived`, `StateChanged`, action lifecycle) produced while running. |
| `persist`   | `save_state` / `load_state` / `reload_topology`: snapshot and restore engine state to/from JSON. |
| `export`    | `to_dot`: render a topology to Graphviz DOT for visualization. |

## Documentation

- [Visualization](doc/visualization.md) — rendering topologies to DOT/SVG with `stv`.
- [Signal Cascades](doc/cascades.md) — reactions and the cascade depth limit.
- [Guards](doc/guards.md) — transition guard expressions.
- [Persistence](doc/persistence.md) — saving, restoring, and reloading state with `stp`.
- [Tracing](doc/tracing.md) — the trace log and event lifecycle.
- [Composition](doc/composition.md) — components, instances, and includes (v0.7).
- [Shell](doc/shell.md) — the `sts` interactive REPL (commands, debugging, end-to-end demo).
- [Roadmap](doc/roadmap.md) — milestone history and upcoming direction.

## Tests

```bash
cargo test
```

71 tests across unit, integration, and CLI test files. All should pass with zero failures before merging.

## License

Licensed under [MIT](LICENSE).
