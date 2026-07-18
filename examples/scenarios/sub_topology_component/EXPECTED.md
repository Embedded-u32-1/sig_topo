# Sub-Topology Component — Scenario

The M45 component / port / instantiation feature: a reusable `component`
bundles a signal topology and exposes one of its states through a `port`. An
`instantiate ... connect { }` block creates a concrete copy and *wires* the
port to a parent-level signal. During expansion the component's internal signal
is renamed into the parent namespace, so a parent reaction can react to the
wired parent signal directly — composition without the parent knowing the
component's internals.

Path: `examples/scenarios/sub_topology_component/`.

## Component + instance

| component | params | port                | wire target |
|-----------|--------|---------------------|-------------|
| `lockable`| (none) | `port out lock.locked as locked` | `door` |

`instantiate lockable as door with {} connect { locked -> door }` after
compilation, the component's `lock` signal is renamed to `door` everywhere —
its signal id, its transitions, and any reaction referencing it. The parent
reacts to `door` directly.

## Signals (after expansion)

| id          | initial_state | states               |
|-------------|---------------|----------------------|
| `controller`| `idle`        | `idle`, `alerted`   |
| `door`      | `unlocked`    | `locked`, `unlocked`|

`door` is the expanded name of the component's internal `lock` signal.

## Transitions (after expansion)

| signal      | from       | event   | to         |
|-------------|------------|---------|------------|
| `controller`| `idle`     | `notify`| `alerted`  |
| `controller`| `alerted`  | `reset` | `idle`     |
| `door`      | `unlocked` | `lock`  | `locked`   |
| `door`      | `locked`   | `unlock`| `unlocked` |

## Reactions (after expansion)

| from_signal | from_state | to_signal    | event   |
|-------------|------------|--------------|---------|
| `door`      | `locked`   | `controller` | `notify`|

The parent reaction names the *wired* signal (`door`), not the component-internal
`lock`. After expansion that is exactly the reaction the engine runs.

## Teaching points

- **Component (M45)**: `component lockable { ... }` defines a reusable bundle.
  Its string fields may use `${param}` placeholders (here there are none) and
  it declares one `port` exposing `lock.locked` aliased as `locked`.
- **Port + wire (M45)**: `port out lock.locked as locked` is the component's
  reaction interface. `connect { locked -> door }` wires that aliased port to
  the parent signal `door`. During expansion the component-internal `lock` is
  renamed to `door` everywhere.
- **Parent reacts to the wire**: the parent reaction reads `when door enters
  locked`. The parent never references the component-internal `lock` name, so
  the component's implementation is fully hidden behind its port.
- **Direction is metadata**: `port out ...` documents that the state change
  flows outward; direction does not alter runtime semantics.

## Scenario

```json
{
  "expected_final_states": {
    "controller": "idle",
    "door": "locked"
  },
  "expected_guard_blocked": [],
  "events": [
    { "signal_id": "door", "event": "lock" },
    { "signal_id": "controller", "event": "reset" }
  ]
}
```

- Event 0 `door lock`: the component's (now renamed) `lock` transition runs —
  `door unlocked -> locked`. The parent reaction fires: `controller idle ->
  alerted`.
  State: `controller = alerted`, `door = locked`.
- Event 1 `controller reset`: `controller alerted -> idle` (no reaction
  matches — the reaction watches `door`, not `controller`).
  State: `controller = idle`, `door = locked`.
- Final: `controller = idle`, `door = locked`.

## Expected key output (via `sts`)

```
sts> state
controller: idle
door: unlocked
sts> event door lock
door -> locked
sts> state
controller: alerted
door: locked
sts> event controller reset
controller -> idle
sts> state
controller: idle
door: locked
```

The `door -> locked` step is the proof that the component's internal `lock`
signal was renamed to `door` during expansion — the parent drives the
sub-topology through the wired port, and the parent reaction cascades off the
exposed state without ever knowing the component's internals.
