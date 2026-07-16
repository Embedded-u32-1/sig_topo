# Composition: Components, Instances & Imports

Large topologies are hard to reuse. v0.7 adds three composition mechanisms so a sub-topology can be defined once and used many times:

- **`components`** — named, parameterized component definitions declared inline in a schema.
- **`instances`** — instantiate a component with concrete bindings, expanding into real signals/transitions/reactions.
- **`includes`** — import signals/transitions/reactions from another JSON file by relative path.

All three are resolved by `load_topology` *before* the engine is built, so the engine always consumes a flat, fully expanded topology.

## Syntax

### Components (parameterized sub-topologies)

A component is declared under `components` (a map of name → `ComponentDef`):

```json
{
  "components": {
    "lockable": {
      "params": ["name"],
      "signals": [
        { "id": "${name}", "initial_state": "unlocked", "states": ["locked", "unlocked"] }
      ],
      "transitions": [
        { "signal_id": "${name}", "from": "unlocked", "event": "lock", "to": "locked" },
        { "signal_id": "${name}", "from": "locked", "event": "unlock", "to": "unlocked" }
      ]
    }
  }
}
```

| Field         | Meaning                                                              |
|---------------|----------------------------------------------------------------------|
| `params`      | Ordered list of parameter names referenced as `${param}` in fields.  |
| `signals`     | Signal definitions; string fields may contain `${param}`.            |
| `transitions` | Transition definitions; string fields may contain `${param}`.        |
| `reactions`   | Optional reaction definitions; string fields may contain `${param}`. |

Every string field supports `${param}` substitution (id, state names, events, actions, guard expression). Substitution is a single left-to-right pass and never re-scans a substituted value, so a value containing `${other}` is not reinterpreted.

### Instances (instantiation)

An instance is declared under `instances` (a list of `InstanceDef`):

```json
{
  "instances": [
    { "component": "lockable", "bindings": { "name": "door" } },
    { "component": "lockable", "bindings": { "name": "window" } }
  ]
}
```

| Field       | Meaning                                                     |
|-------------|-------------------------------------------------------------|
| `component` | Name of the component to instantiate (must exist under `components`). |
| `bindings`  | Map of every declared `param` → concrete value.             |

Each instance expands into its component's signals/transitions/reactions with every `${param}` replaced by its bound value. Two instances of the same component produce independent signals (above: `door` and `window`).

### Includes (cross-file import)

`includes` is a list of relative file paths resolved against the **including file's parent directory**:

```json
{
  "includes": ["lockable.json"]
}
```

The referenced file is read and its `signals`, `transitions`, `reactions`, and `components` are merged into the importing schema. It is a **verbatim union-merge**: the included file's signal ids are preserved unchanged — there is no renaming or scoping.

```bash
# Directory layout
examples/components/
├── lockable.json   # reusable sub-topology (signal "gate")
├── house.json      # instances door/window + includes lockable.json
└── breaker.json    # includes lockable.json
```

## Execution Semantics

`load_topology(path)` performs the full pipeline:

1. **Read + parse** the top-level file into a `TopologySchema`.
2. **Resolve includes** recursively. Relative paths are resolved against each including file's parent directory. Canonicalized absolute paths drive cycle detection — revisiting any file raises `IncludeCycle`.
3. **Union-merge** recursively included schemas (signals/transitions/reactions appended; duplicate signal ids across files raise `DuplicateSignalAfterExpand`; same-named `components` are overwritten, last-wins, no error).
4. **Expand** every `instance` into flat signals/transitions/reactions via `${param}` substitution, then enforce unique signal ids across the whole result (`DuplicateSignalAfterExpand` on collision). The returned schema is fully flat: `components`/`instances`/`includes` are empty/absent.

The result is a flat `TopologySchema` consumed directly by `TopologyEngine::from_schema` (which calls `expand` again as a no-op).

```
house.json ──► load_topology ──► [read] ──► [resolve includes: lockable.json]
                                               │
                                               ▼
                                         [union-merge gate]
                                               │
                                               ▼
                                         [expand instances: door, window]
                                               │
                                               ▼
                                         flat schema → TopologyEngine
```

## CLI Usage

All three binaries now load topologies through `load_topology`, so includes and instances are parsed everywhere:

```bash
# Render a composed topology to DOT/SVG
cargo run --bin stv -- examples/components/house.json

# Run a scenario (reactions across includes/instances fire at runtime)
cargo run --bin stt -- examples/components/house.json scenario.json

# Persist state resolved from the composed topology
cargo run --bin stp -- save examples/components/house.json scenario.json state.json
```

## Example: `house.json`

`examples/components/house.json` demonstrates all three mechanisms together:

- A `lockable` **component** (param `name`).
- Two **instances** (`door`, `window`) → signals `door`, `window`.
- An **include** of `lockable.json` → signal `gate`.
- A top-level `controller` signal with a **reaction**: when `door` reaches `locked`, send `notify` to `controller`.

```bash
cargo run --bin stv -- examples/components/house.json
```

produces a DOT diagram with four subgraphs (`controller`, `gate`, `door`, `window`), and running a scenario that locks `door` cascades to `controller` becoming alerted.

## Error Troubleshooting

| Error | When it appears |
|-------|-----------------|
| `ComponentNotFound` | An `instance` references a component name not declared in `components`. |
| `MissingBinding` | An instance is missing a binding for one of the component's declared `params`. |
| `InvalidParamRef` | After substitution, a `${param}` remains whose name is not in the component's `params` list. |
| `DuplicateSignalAfterExpand` | Two signals share an id after expansion — either two instances/components collide, or a top-level/included signal reuses an id. |
| `IncludeNotFound` | An `includes` path does not point to a readable, valid JSON topology file. |
| `IncludeCycle` | File A includes file B (transitively) that includes file A; detected via canonicalized absolute path. |

All composition errors surface as `EngineError`; the CLI binaries print the message and exit non-zero.

## Limitations

- **Engine layer unchanged.** Composition is a preprocessing step. The engine receives and runs a flat schema exactly as before.
- **No new dependencies.** All composition logic lives in `src/compose.rs` using only `std` + `serde_json`.
- **Includes are resolved once at load time.** A relative path is fixed when the file is loaded. The runtime `reload_topology` API expects an already-flat schema string — to change imports, reload with a fresh flat topology.
- **No scoping/renaming on include.** The included file's signal ids are preserved verbatim and must be globally unique after merge.
