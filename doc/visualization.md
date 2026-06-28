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
