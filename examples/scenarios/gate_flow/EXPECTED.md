# Gate Flow — Scenario

A single-signal topology for a physical gate/door. It is deliberately different from the order scenario: it uses a `*` wildcard transition for an emergency reset and a `guard` on the fault event. Together they show the engine blocking a guarded transition and then matching on *any* source state. Adapted from the top-level `examples/gate_flow.json`.

Path: `examples/scenarios/gate_flow/`.

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
| `open` | `fault` | `fault` | `payload.emergency == true` | `on_transition: engage_brake, engage_backup_brake`, `on_enter: log_fault` |
| `closed` | `reset` | `closed` | — | `on_transition: clear_fault_safely`, `on_enter: log_reset` |
| `open` | `reset` | `closed` | — | `on_transition: clear_fault_safely`, `on_enter: log_reset` |
| `fault` | `reset` | `closed` | — | `on_transition: clear_fault_safely`, `on_enter: log_reset` |
| `fault` | `repair` | `closed` | — | `on_transition: run_diagnostics`, `on_enter: log_repair` |

## Teaching points

- **Guard**: `fault` only fires when `payload.emergency == true`. Sending `fault` with `{"emergency":false}` is blocked (`GuardBlocked`) and the state stays `open`.
- **Wildcard `*`**: the DDL writes the reset directly as the single wildcard `on reset from * -> closed` (M34); the compiler lowers it to one `reset` transition per source state, behaviorally identical to the JSON form. A `reset` from any state funnels back to `closed` (the explicit `fault -> repair -> closed` path is separate).
- **Wildcard proves it is live**: the `closed -> closed` reset arm emits a `StateChanged gate: closed -> closed`. This self-loop is the proof that the wildcard matches the *current* state rather than acting as a no-op.

## Scenario

```json
{
  "expected_final_states": { "gate": "closed" },
  "expected_guard_blocked": [1],
  "events": [
    { "signal_id": "gate", "event": "open" },
    { "signal_id": "gate", "event": "fault", "payload": { "emergency": false } },
    { "signal_id": "gate", "event": "fault", "payload": { "emergency": true } },
    { "signal_id": "gate", "event": "reset" },
    { "signal_id": "gate", "event": "open" },
    { "signal_id": "gate", "event": "close" },
    { "signal_id": "gate", "event": "reset" }
  ]
}
```

- Event 0 `open`: `closed -> open`.
- Event 1 `fault({emergency:false})`: **guard blocked** (`payload.emergency == true` is false) → state stays `open`. This is the event named in `expected_guard_blocked`.
- Event 2 `fault({emergency:true})`: guard passes → `open -> fault`.
- Event 3 `reset`: wildcard matches `fault` → `fault -> closed`.
- Event 4 `open`: `closed -> open`.
- Event 5 `close`: `open -> closed`.
- Event 6 `reset`: wildcard matches `closed` → `closed -> closed` (the self-loop that proves `*` is live).
- Final: `gate = closed`.

## Expected key output (via `sts`)

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
[action] gate.engage_backup_brake
[action] gate.log_fault
gate -> fault
  action executed: engage_brake
  action executed: engage_backup_brake
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

The final `reset` from `closed` produces a `StateChanged gate: closed -> closed` in the trace. This is the canonical proof that the wildcard matches the current state rather than acting as a no-op — a subtlety that is easy to get wrong in hand-written test drivers.
