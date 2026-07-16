# Gate Flow（门控流程）

A single-signal topology for a physical gate/door. It is kept deliberately different from the order scenario: it uses a `*` wildcard transition for an emergency reset, a `guard` on the fault event, and multi-action `on_transition` hooks. Together they show the engine matching on any source state and running several actions in one transition.

## Signal

| Field | Value |
|-------|-------|
| `id` | `gate` |
| `initial_state` | `closed` |
| `states` | `closed`, `open`, `fault` |

## Transitions

| from | event | to | guard | actions |
|------|-------|----|-------|---------|
| `closed` | `open` | `open` | — | `on_transition: activate_motor`, `on_enter: log_gate_open` |
| `open` | `close` | `closed` | — | `on_transition: deactivate_motor`, `on_enter: log_gate_closed` |
| `open` | `fault` | `fault` | `payload.emergency == true` | `on_transition: engage_brake, raise_alarm`, `on_enter: log_fault` |
| `*` | `reset` | `closed` | — | `on_transition: clear_fault_safely`, `on_enter: log_reset` |
| `fault` | `repair` | `closed` | — | `on_transition: run_diagnostics, clear_fault_safely`, `on_enter: log_repair` |

## Special points

- **Wildcard `*`**: the `reset` transition matches the signal regardless of its current state. A `reset` from `fault` lands in `closed` via the wildcard (the explicit `fault -> repair -> closed` path is separate), and a `reset` from any other state also funnels back to `closed`.
- **Guard**: `fault` only fires when `payload.emergency == true`. Sending `fault` with `{"emergency":false}` is blocked and the state stays `open`.
- **Multi-action**: `engage_brake` and `raise_alarm` run together on the fault transition, so the order and grouping of actions is observable in the trace.

## Demo steps

Run the shell:

```bash
cargo run --bin sts -- examples/gate_flow.json
```

Then type:

```
state
event gate open
state
event gate fault {"emergency":false}
state
event gate fault {"emergency":true}
state
event gate reset {}
state
event gate open
state
event gate close
state
event gate reset {}
state
trace
quit
```

## Expected key output

```
sts> state
gate: closed
sts> event gate open
[action] gate.activate_motor
[action] gate.log_gate_open
gate -> open
  action executed: activate_motor
  action executed: log_gate_open
sts> state
gate: open
sts> event gate fault {"emergency":false}
Error: Transition blocked by guard 'payload.emergency == true' for signal 'gate' on event 'fault'
State rolled back to 'open'
sts> state
gate: open
sts> event gate fault {"emergency":true}
[action] gate.engage_brake
[action] gate.raise_alarm
[action] gate.log_fault
gate -> fault
  action executed: engage_brake
  action executed: raise_alarm
  action executed: log_fault
sts> state
gate: fault
sts> event gate reset {}
[action] gate.clear_fault_safely
[action] gate.log_reset
gate -> closed
  action executed: clear_fault_safely
  action executed: log_reset
sts> state
gate: closed
sts> event gate open
[action] gate.activate_motor
[action] gate.log_gate_open
gate -> open
  action executed: activate_motor
  action executed: log_gate_open
sts> state
gate: open
sts> event gate close
[action] gate.deactivate_motor
[action] gate.log_gate_closed
gate -> closed
  action executed: deactivate_motor
  action executed: log_gate_closed
sts> state
gate: closed
sts> event gate reset {}
[action] gate.clear_fault_safely
[action] gate.log_reset
gate -> closed
  action executed: clear_fault_safely
  action executed: log_reset
sts> state
gate: closed
```

After the final `reset` from `closed`, `trace` shows a `StateChanged gate: closed -> closed`: this is the wildcard matching the current state and funneling it to the same place, which confirms `*` is live rather than a no-op.
