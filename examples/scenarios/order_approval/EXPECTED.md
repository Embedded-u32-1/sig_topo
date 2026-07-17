# Order Approval — Scenario

A single-signal topology that models an order moving through a review pipeline. This is the canonical "happy path + guard block + recovery" scenario: a normal transition (all three lifecycle hooks), a guard that blocks an invalid payload, then a guarded success, then a run-down to the terminal state. Adapted from the top-level `examples/order_approval.ddl`.

Path: `examples/scenarios/order_approval/`.

## Signal

| Field | Value |
|-------|-------|
| `id` | `order` |
| `initial_state` | `draft` |
| `states` | `draft`, `submitted`, `approved`, `rejected`, `shipped` |

## Transitions

| from | event | to | guard | actions |
|------|-------|----|-------|---------|
| `draft` | `submit` | `submitted` | — | `on_exit: log_draft_exit`, `on_transition: validate_order_payload`, `on_enter: notify_submitted` |
| `submitted` | `approve` | `approved` | `payload.amount > 0` | `on_transition: reserve_inventory`, `on_enter: notify_customer_approved` |
| `submitted` | `reject` | `rejected` | — | `on_transition: release_hold`, `on_enter: notify_customer_rejected` |
| `approved` | `ship` | `shipped` | — | `on_transition: dispatch_order`, `on_enter: notify_shipped` |

## Teaching points

- **Guard**: `approve` only fires when `payload.amount > 0`. Sending `approve` with `{"amount":0}` is blocked (`GuardBlocked`) and the state stays `submitted`.
- **Recovery**: a guard block leaves the signal in the *source* state, so a later well-formed `approve` (`{"amount":0}` → `{"amount":5000}`) still commits. A block is not a dead end.
- **Normal path** covers all three action hooks (`on_exit`, `on_transition`, `on_enter`) on the first transition, so the `executed_actions` ordering is fully observable.
- **Rollback seam**: `reserve_inventory` is the action a real system would replace with a fallible inventory call. If it returned `Err`, the engine's M19 transaction rollback would revert it and leave the signal in `submitted`.

## Scenario

```json
{
  "expected_final_states": { "order": "shipped" },
  "expected_guard_blocked": [1],
  "events": [
    { "signal_id": "order", "event": "submit" },
    { "signal_id": "order", "event": "approve", "payload": { "amount": 0 } },
    { "signal_id": "order", "event": "approve", "payload": { "amount": 5000 } },
    { "signal_id": "order", "event": "ship" }
  ]
}
```

- Event 0 `submit`: normal transition `draft -> submitted`.
- Event 1 `approve({amount:0})`: **guard blocked** (`payload.amount > 0` is false) → state stays `submitted`. This is the event named in `expected_guard_blocked`.
- Event 2 `approve({amount:5000})`: guard passes → `submitted -> approved`.
- Event 3 `ship`: `approved -> shipped`.
- Final: `order = shipped`.

## Expected key output (via `sts`)

```
sts> state
order: draft
sts> event order submit
[action] order.log_draft_exit
[action] order.validate_order_payload
[action] order.notify_submitted
order -> submitted
  action executed: log_draft_exit
  action executed: validate_order_payload
  action executed: notify_submitted
sts> state
order: submitted
sts> event order approve {"amount":0}
Error: Transition blocked by guard 'payload.amount > 0' for signal 'order' on event 'approve'
State rolled back to 'submitted'
sts> state
order: submitted
sts> event order approve {"amount":5000}
[action] order.reserve_inventory
[action] order.notify_customer_approved
order -> approved
  action executed: reserve_inventory
  action executed: notify_customer_approved
sts> state
order: approved
sts> event order ship
[action] order.dispatch_order
[action] order.notify_shipped
order -> shipped
  action executed: dispatch_order
  action executed: notify_shipped
sts> state
order: shipped
```

The `trace` command then prints the full event/action/state-change log. The blocked `approve({amount:0})` is logged as `EventReceived` with no following `StateChanged`; the `reserve_inventory` rollback seam is not exercised live here (every `sts` action is a `print-and-record` stub that always returns `Ok`) — inject it with `fail reserve_inventory` to observe the live rollback.
