# signal-topology

A file-driven Rust state-machine / workflow engine. Describe a system as a **DDL** topology of signals, transitions, reactions and guards, then run scenarios, persist state, trace events, and export diagrams to DOT/SVG/WASM.

**[5-min quick start](doc/getting-started.md)** · [Architecture](doc/architecture.md) · [Changelog](CHANGELOG.md)

## Quick Start

```bash
cargo build

# Write + compile a DDL topology
cargo run --bin stc -- my.ddl my.json

# Drive it interactively (REPL: event / state / trace / why / dot-ext)
cargo run --bin sts -- my.json

# Watch mode (auto-recompile on change + scenario regression)
cargo run --bin stc -- watch my.ddl --interval 500

# Visualize (DOT + SVG via Graphviz)
cargo run --bin stv -- my.json          # static skeleton
cargo run --bin sts -- my.json          # then: dot-ext  -> runtime view with guard colors

# Scenario replay + persist
cargo run --bin stt -- my.json scenario.json
cargo run --bin stp -- save my.json scenario.json state.json

# Browser demo (WASM)
wasm-pack build --target web --out-dir pkg --release -p wasm-topology
python3 -m http.server 8080 && open http://localhost:8080/demo/index.html
```

Full walk-throughs: [order_approval.md](examples/order_approval.md), [gate_flow.md](examples/gate_flow.md). Shell reference: [doc/shell.md](doc/shell.md). DDL reference: [doc/ddl.md](doc/ddl.md).

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
| `run`       | Shared scaffolding for the `sts` / `stt` / `stp` binaries: builds a runnable engine from a topology + fail-set. Not a stable library API. |
| `ddl`       | Domain Description Language compiler: `.ddl` source → `TopologySchema` (JSON model). |
| `guard templates` | Top-level `guard <id> { <expr> }` declarations shared across reactions via `when <id>` references (inlined by the compiler, linted by `stc --check`). |
| `check`     | Semantic checks for `stc --check`: report suspicious patterns (self-loops, unreachable states, unused / duplicate guards) as non-blocking warnings. |

## Documentation

- [Quick start](doc/getting-started.md) — 5-minute walkthrough
- [Architecture](doc/architecture.md) — modules, core concepts, data flow
- [Changelog](CHANGELOG.md) — version history
- [Visualization](doc/visualization.md) — rendering topologies to DOT/SVG with `stv`.
- [Signal Cascades](doc/cascades.md) — reactions and the cascade depth limit.
- [Guards](doc/guards.md) — transition guard expressions.
- [Persistence](doc/persistence.md) — saving, restoring, and reloading state with `stp`.
- [Tracing](doc/tracing.md) — the trace log and event lifecycle.
- [Composition](doc/composition.md) — components, instances, and includes (v0.7).
- [Shell](doc/shell.md) — the `sts` interactive REPL (commands, debugging, end-to-end demo).
- [Transaction](doc/transaction.md) — single-signal transactional rollback semantics (v0.8).
- [Run module](doc/run.md) — shared `sts`/`stt`/`stp` scaffolding (not a stable library API).
- [DDL](doc/ddl.md) — the Domain Description Language: write `.ddl` instead of JSON, compile with `stc`.
- [Check](doc/ddl.md) — linting with `stc --check` (self-loops, unreachable states).
- [Scenarios](doc/scenarios.md) — the `examples/scenarios/` regression + teaching library (each scenario is a self-contained `.ddl` + `.scenario.json` + `EXPECTED.md` walk-through).
- [WASM](doc/wasm.md) — `wasm-bindgen` surface + browser / Node demo.
- [Roadmap](doc/roadmap.md) — milestone history and upcoming direction.

## Tests

```bash
cargo test
```

257 tests across unit, integration, CLI, and doctest files. All should pass with zero failures before merging.

## License

Licensed under [MIT](LICENSE).
