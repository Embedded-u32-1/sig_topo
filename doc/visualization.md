# Visualization

This project can export a signal topology schema to [Graphviz DOT](https://graphviz.org/doc/info/lang.html) format for visualization.

## Installing Graphviz

On Debian/Ubuntu:

```bash
sudo apt-get update
sudo apt-get install graphviz
```

On macOS with Homebrew:

```bash
brew install graphviz
```

On Windows with Chocolatey:

```bash
choco install graphviz
```

## Using the CLI viewer

The `stv` binary reads a topology JSON file and produces a DOT file next to it. If `dot` is available on your PATH, it also renders an SVG automatically.

```bash
cargo run --bin stv -- tests/topology.json
```

## Output files

Running the command above will create:

- `tests/topology.dot` - the Graphviz DOT source.
- `tests/topology.svg` - an SVG rendering of the topology (if Graphviz is installed).

Open `tests/topology.svg` in any web browser to view the diagram.

## Programmatic export

You can also call the export function directly from Rust:

```rust
use signal_topology::export::to_dot;
use signal_topology::schema::TopologySchema;

let schema: TopologySchema = serde_json::from_str(json)?;
let dot = to_dot(&schema);
```

The generated diagram uses:

- One subgraph per signal, labeled with the signal id.
- One node per state, named `n_<signal_id>_<state>`.
- A filled light-blue style for the initial state node.
- Directed edges labeled with event names and associated action ids.
- Wildcard (`*`) transitions expanded into edges from every state to the target state.

## Runtime state highlighting

`skeleton` diagrams (above) only show *structure* — which states exist and how they connect. To see **where every signal is right now**, the engine can render a DOT with the current state of each signal highlighted live.

### `stv --live`

```bash
stv --live <topology.json> <state.json>
```

`<state.json>` uses the same `StateSnapshot` format `stp` writes — a `{"states": {"<signal>": "<state>"}}` object (see `doc/persistence.md`). The output is written next to the topology as `<stem>_live.dot` (and `<stem>_live.svg` if Graphviz is installed), so it does not clobber the skeleton `<stem>.dot`.

### `dot` command in `sts`

Inside the `sts` shell, the `dot` command prints the runtime-highlighted DOT for the live engine:

```
sts> dot
digraph Topology {
...
}
```

Send an event and run `dot` again — the highlight follows the engine. The output is plain DOT, so you can pipe it straight to Graphviz:

```
sts> dot | dot -Tsvg > /tmp/live.svg
```

### `TopologyEngine::snapshot_dot`

Programmatically, `engine.snapshot_dot()` returns the same runtime-highlighted DOT as a `String`. It reconstructs a minimal schema from the engine's runtime state, so it works without holding the original `TopologySchema`.

### Highlight strategy

Per state node, first match wins:

- **Current state** (the signal's present value) → `style=filled fillcolor=lightgreen penwidth=2`. The runtime highlight always wins, so the node a signal is sitting on reads as "live".
- **Initial state** (only when it differs from current) → `style=filled fillcolor=lightblue`, the static "started here" marker.
- **Everything else** → no extra attributes.

So when current ≠ initial you see both cues at once (lightblue = "started here", lightgreen = "is here now"); when they coincide, lightgreen wins. On a fresh engine, where current == initial for every signal, only lightgreen appears. This is implemented by `to_dot_with_state(schema, states)` in `src/export/dot.rs`; passing an empty `states` map reproduces the plain skeleton exactly.

## Extended DOT with guard-eval result

The views above render *states* and *transitions*. To see the cross-signal **reaction** wiring and, crucially, whether each reaction's guard let it fire, the engine can render an *extended* DOT that overlays reaction edges colored by their guard-evaluation result.

### `dot-ext` command in `sts`

Inside the `sts` shell, `dot-ext` prints the extended view:

```
sts> dot-ext
digraph Topology {
...
  n_order_approved -> n_inventory_idle [label="allocate [guard: true]" color=green style=solid];
  n_order_approved -> n_audit_idle [label="note [guard: false]" color=gray style=dashed];
}
```

Like `dot`, it highlights each signal's current state. In addition it draws one dashed arrow per reaction, from the watched state of `from_signal` to an anchor node in `to_signal`'s cluster, colored by the guard result recorded in the trace:

- **solid green** — guard evaluated to `true`; the reaction fired.
- **dashed gray** — guard evaluated to `false`; the cascade was blocked (the main transition still committed).
- **dashed red** — guard failed to evaluate (`error: <msg>`); the reaction was skipped.
- **dashed black** — `not evaluated`; the reaction's guard was never evaluated this run (the watched state was not reached, or the reaction has no guard).

Reaction edges are the visual companion to `why`: `why` reports one reaction's guard trace as text, while `dot-ext` shows every reaction at a glance.

### `TopologyEngine::snapshot_dot_extended`

Programmatically, `engine.snapshot_dot_extended()` returns the same extended DOT as a `String`. It reconstructs the schema from runtime state and keys each reaction's color off the most recent `ReactionGuardEvaluated` trace event, so a reaction absent from the trace renders dashed-black `not evaluated`.

### How colors are decided

Implemented by `to_dot_extended(schema, states, guard_info)` in `src/export/dot.rs`. `guard_info` maps each reaction `(from_signal, from_state, to_signal, event)` to its guard-result string (`"true"`, `"false"`, or `"error: <msg>"`); a reaction missing from the map is drawn dashed black. The reaction's tail anchors on the watched `from_state` node (or, for a `*` wildcard reaction, the signal's current state), and its head anchors on the target signal's first state — a reaction delivers an *event* to a signal rather than targeting a particular state, so the first state is a stable in-cluster landing point.
