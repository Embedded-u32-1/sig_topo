# Fail Rollback — Scenario

A single-signal topology that demonstrates the engine's M19 transactional rollback. An action is forced to fail at *replay* time via the scenario's `fail_actions` field — the topology and engine are unchanged — and the engine rolls the transition back, recording `ActionFailed` + `Rollbacked` in the trace. A later re-run of the same event, without the injected failure, commits normally. This is the canonical "the same transition that once rolled back can still succeed" story.

Path: `examples/scenarios/fail_rollback/`.

## Signal

| Field | Value |
|-------|-------|
| `id` | `order` |
| `initial_state` | `draft` |
| `states` | `draft`, `submitted`, `approved`, `shipped` |

## Transitions

| from | event | to | guard | actions |
|------|-------|----|-------|---------|
| `draft` | `submit` | `submitted` | — | `on_transition: validate`, `on_enter: notify_submitted` |
| `submitted` | `approve` | `approved` | `payload.amount > 0` | `on_transition: reserve_inventory`, `on_enter: notify_approved` |
| `approved` | `ship` | `shipped` | — | `on_transition: dispatch`, `on_enter: notify_shipped` |

## Teaching points

- **Injected failure is scoped per event**: `reserve_inventory` is forced to fail *only* for event 1 (`fail_actions: ["reserve_inventory"]`) by inserting its id into a shared fail-set before the event runs and clearing it after. The topology and action handlers are untouched.
- **Rollback semantics** (event 1): `reserve_inventory` returns `Err`, so `approve` does not commit. The engine reverts `order` to the source state (`submitted`), records an `ActionFailed` for `reserve_inventory` plus a `Rollbacked order: approved -> submitted` trace event, and returns `ActionExecutionError`.
- **Re-run commits** (event 2): the same `approve` event, sent again *without* `fail_actions`, now runs `reserve_inventory` + `notify_approved` and commits `submitted -> approved`. A rolled-back transition is not doomed — it can succeed once the failure cause is gone.
- **`fail_actions` vs `expected_guard_blocked`**: an injected *action* failure raises `ActionExecutionError`, distinct from a *guard* block (`GuardBlocked`). This scenario's `expected_guard_blocked` is empty; its error comes from `fail_actions`.

## Scenario

```json
{
  "expected_final_states": { "order": "shipped" },
  "expected_guard_blocked": [],
  "events": [
    { "signal_id": "order", "event": "submit" },
    {
      "signal_id": "order",
      "event": "approve",
      "payload": { "amount": 5000 },
      "fail_actions": ["reserve_inventory"]
    },
    { "signal_id": "order", "event": "approve", "payload": { "amount": 5000 } },
    { "signal_id": "order", "event": "ship" }
  ]
}
```

- Event 0 `submit`: commits `draft -> submitted`.
- Event 1 `approve({amount:5000})` with `fail_actions: ["reserve_inventory"]`: **action failure** → `ActionExecutionError`, order rolls back to `submitted` (trace: `ActionFailed reserve_inventory` + `Rollbacked order: approved -> submitted`).
- Event 2 `approve({amount:5000})` (no injection): commits `submitted -> approved`.
- Event 3 `ship`: commits `approved -> shipped`.
- Final: `order = shipped`.

`expected_guard_blocked` is empty because no transition is guard-blocked here; the only error is the injected action failure, which surfaces as `ActionExecutionError` and is recorded by the replay rather than asserted as a guard block.

## Expected key output (via `stt` / `sts`)

```
sts> event order submit
[action] order.validate
[action] order.notify_submitted
order -> submitted
  action executed: validate
  action executed: notify_submitted
sts> fail reserve_inventory            # simulates the scenario's fail_actions
sts> event order approve {"amount":5000}
[action] order.reserve_inventory       # <- injects failure here
Error: Action execution failed for 'reserve_inventory': injected failure ...
State rolled back to 'submitted'
sts> reset                            # clears the fail-set
sts> event order approve {"amount":5000}
[action] order.reserve_inventory
[action] order.notify_approved
order -> approved
  action executed: reserve_inventory
  action executed: notify_approved
sts> event order ship
[action] order.dispatch
[action] order.notify_shipped
order -> shipped
  action executed: dispatch
  action executed: notify_shipped
sts> state
order: shipped
```

The `trace` after event 1 records, in order: `EventReceived order.approve`, `ActionStarted order.reserve_inventory`, `ActionFailed order.reserve_inventory error=...`, `Rollbacked order: approved -> submitted` — and critically **no** `StateChanged` for that attempt. After event 2 it records the full successful lifecycle plus `StateChanged order: submitted -> approved`. That contrast (no `StateChanged` on the failing attempt, one on the re-run) is the observable proof that rollback is real and recovery is possible.
