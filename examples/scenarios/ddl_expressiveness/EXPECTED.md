# DDL Expressiveness — Scenario

The M34 DDL-side language features that make a `.ddl` file read like a domain
document rather than JSON. A single scenario exercises all three together:
(1) the wildcard `on ev from * -> state`, (2) multi-action lifecycle hooks, and
(3) the reaction static payload `with { ... }`. Together they show the `.ddl`
source as a self-contained domain story.

Path: `examples/scenarios/ddl_expressiveness/`.

## Signals

| id       | initial_state | states                 |
|----------|---------------|------------------------|
| `machine`| `idle`        | `idle`, `running`, `fault` |
| `audit`  | `quiet`       | `quiet`, `noted`       |

## Transitions

The wildcard `on reset from * -> idle` lowers to one `reset` transition per
source state (the rows marked `*`):

| signal   | from     | event   | to     | guard                      | actions                                   |
|----------|----------|---------|--------|----------------------------|-------------------------------------------|
| `machine`| `idle`   | `start` | `running` | —                       | `on_transition: warm_up, calibrate`, `on_enter: log_run` |
| `machine`| `running`| `stop`  | `idle` | —                          | `on_enter: log_stop`                      |
| `machine`| `running`| `fault` | `fault`| `payload.emergency == true`| `on_transition: engage_brake`, `on_enter: log_fault` |
| `machine`| `*`      | `reset` | `idle` | —                          | `on_transition: clear_fault_safely`, `on_enter: log_reset` |
| `audit`  | `quiet`  | `note`  | `noted`| —                          | —                                         |

The `*` wildcard lowers to three concrete arms: `idle -> idle`,
`running -> idle`, `fault -> idle`.

## Reactions

| from_signal | from_state | to_signal | event | static payload          |
|-------------|------------|-----------|-------|--------------------------|
| `machine`   | `idle`     | `audit`   | `note`| `{ "origin": "reset" }` |

The reaction's `with { ... }` block is the derived `note` event's payload
(M34). It guards nothing and always fires when `machine` enters `idle`.

## Teaching points

- **Wildcard `from *` (M34)**: a single DDL line `on reset from * -> idle`
  lowers to one transition per source state. The `fault -> idle` arm is what
  the scenario drives; the lowered schema also contains the `idle -> idle`
  self-loop that proves `*` matches the current state (cf. `gate_flow`).
- **Multi-action hooks (M34)**: `on_transition: warm_up, calibrate` runs the
  actions in declaration order — `warm_up` then `calibrate` — before
  `on_enter: log_run`. Phases always run `on_exit` → `on_transition` →
  `on_enter`; within a phase, declaration order.
- **Transition guard (M28/M34)**: `fault` carries the guard
  `payload.emergency == true`. Sending `fault` with `emergency: false` is
  rejected with `GuardBlocked` and the state stays `running`. The same event
  with `emergency: true` commits.
- **Reaction static payload (M34)**: `with { "origin": "reset" }` rides on the
  derived `note` event to `audit`. The guard side (the source event's payload)
  and the static payload are distinct: no guard here, so it always fires; the
  payload goes to the target.

## Scenario

```json
{
  "expected_final_states": {
    "machine": "idle",
    "audit": "noted"
  },
  "expected_guard_blocked": [1],
  "events": [
    { "signal_id": "machine", "event": "start" },
    { "signal_id": "machine", "event": "fault", "payload": { "emergency": false } },
    { "signal_id": "machine", "event": "fault", "payload": { "emergency": true } },
    { "signal_id": "machine", "event": "reset" }
  ]
}
```

- **Event 0** `start`: `machine idle -> running`. Multi-action hook runs in
  order: `warm_up`, `calibrate`, then `log_run`. The reaction watches
  `machine enters idle`, so it does **not** match.
  State: `machine=running, audit=quiet`.
- **Event 1** `fault({emergency:false})`: transition guard `emergency == true`
  is **false** → **GuardBlocked**, state stays `running`. This is the event in
  `expected_guard_blocked`. (A guard block is not a cascade; no reaction runs.)
  State: `machine=running, audit=quiet`.
- **Event 2** `fault({emergency:true})`: guard **true** → `machine running ->
  fault`; `engage_brake` then `log_fault`. Reaction does not match.
  State: `machine=fault, audit=quiet`.
- **Event 3** `reset`: wildcard matches `fault` → `machine fault -> idle`;
  `clear_fault_safely` then `log_reset`. The reaction matches (`machine enters
  idle`) and fires the derived `note` event carrying the static payload
  `{ "origin": "reset" }` → `audit quiet -> noted`.
  State: `machine=idle, audit=noted`.

Final: `machine = idle`, `audit = noted`.

## Expected key output (via `sts`)

```
sts> state
machine: idle
audit: quiet
sts> event machine start
[action] machine.warm_up
[action] machine.calibrate
[action] machine.log_run
machine -> running
  action executed: warm_up
  action executed: calibrate
  action executed: log_run
sts> state
machine: running
audit: quiet
sts> event machine fault {"emergency":false}
Error: Transition blocked by guard 'payload.emergency == true' for signal 'machine' on event 'fault'
State rolled back to 'running'
sts> state
machine: running
audit: quiet
sts> event machine fault {"emergency":true}
[action] machine.engage_brake
[action] machine.log_fault
machine -> fault
  action executed: engage_brake
  action executed: log_fault
sts> state
machine: fault
audit: quiet
sts> event machine reset
[action] machine.clear_fault_safely
[action] machine.log_reset
machine -> idle
  action executed: clear_fault_safely
  action executed: log_reset
sts> state
machine: idle
audit: noted
```

The `start` trace shows the multi-action hook order (`warm_up` before
`calibrate` before `log_run`); the `fault({emergency:false})` trace shows the
guard block; and the final `reset` trace shows `machine fault -> idle` (the
wildcard arm) followed by the cascaded `audit quiet -> noted` carrying the
static payload — all three M34 features in one walk-through.
