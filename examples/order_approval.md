# Order Approval（订单审批）

A single-signal topology that models an order moving through a review pipeline. It demonstrates normal transitions, a `guard` evaluated against the event payload, and a transition (`reserve_inventory`) named to stand in for a real action that could fail — so it doubles as the M21 rollback seam.

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
| `submitted` | `approve` | `approved` | `payload.amount > 0 and payload.amount <= 100000` | `on_transition: reserve_inventory`, `on_enter: notify_customer_approved` |
| `submitted` | `reject` | `rejected` | — | `on_transition: release_hold`, `on_enter: notify_customer_rejected` |
| `approved` | `ship` | `shipped` | — | `on_transition: dispatch_order`, `on_enter: notify_shipped` |

## Special points

- **Guard**: `approve` only fires when `payload.amount` is in `(0, 100000]`. Sending `approve` with `{"amount":0}` is blocked and the state stays `submitted`.
- **Rollback seam**: `reserve_inventory` is the action a real system would replace with an inventory call that can fail (out-of-stock, timeout). If it returned `Err`, the M21 transaction rollback would revert any already-run `on_transition` actions and leave the signal in `submitted`. In `sts` every action is a print-and-record stub that always returns `Ok`, so the failure path is not exercised live — swap the handler to observe it.
- **Normal path** covers all three action hooks (`on_exit`, `on_transition`, `on_enter`) on the first transition, so the `print-and-record` output is fully observable.

## Demo steps

Run the shell:

```bash
cargo run --bin sts -- examples/order_approval.json
```

Then type:

```
state
event order submit
state
event order approve {"amount":0}
state
event order approve {"amount":5000}
state
event order ship
state
trace
quit
```

## Expected key output

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
Error: Transition blocked by guard 'payload.amount > 0 and payload.amount <= 100000' for signal 'order' on event 'approve'
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

The `trace` command then prints the full event/action/state-change log in `stt` format, including the blocked `approve` (logged as `EventReceived` with no following `StateChanged`).
