# Guard Compensation — Scenario

The M47 reaction **compensation** feature combined with a **guard**: a
reaction's `on_fail: <action_id>` hook is a compensation action that runs when
the cascade it triggers fails. A reaction *guard* (`when <expr>`) first decides
whether the cascade is attempted at all — so the compensation only ever runs
for a reaction that was *eligible* to fire and whose downstream step then
failed. The guard and the compensation together express "only attempt this side
effect for auto-orders, and undo the bookkeeping if it cannot complete."

Path: `examples/scenarios/guard_compensation/`.

## Signals

| id         | initial_state | states                  |
|------------|---------------|-------------------------|
| `order`    | `pending`     | `pending`, `paid`       |
| `inventory`| `ok`          | `ok`, `allocated`       |

## Transitions

| signal     | from      | event     | to         | actions            |
|------------|-----------|-----------|------------|--------------------|
| `order`    | `pending` | `pay`     | `paid`     |                    |
| `order`    | `paid`    | `reset`   | `pending`  |                    |
| `inventory`| `ok`      | `allocate`| `allocated`| `on_transition: commit_stock` |

## Reactions

| from_signal | from_state | to_signal   | event     | guard                | on_fail          |
|-------------|------------|-------------|-----------|----------------------|------------------|
| `order`     | `paid`     | `inventory` | `allocate`| `payload.auto == true` | `release_holding` |

## Teaching points

- **Guard gates the cascade** (M32/M47): the reaction fires only when
  `payload.auto == true`. With `auto: false` the reaction is silently skipped —
  the main transition still commits and no compensation can run, because no
  cascade was ever attempted.
- **Compensation (M47)**: `on_fail: release_holding` runs when the cascade
  fails — i.e. when the derived `allocate` transition's `commit_stock` action
  is injected to fail, so `inventory` rolls back and the cascade error
  propagates. The compensation runs *before* that error propagates upward,
  with the failure message carried in `ActionContext.failure`.
- **Best-effort, never masking**: the compensation's own completion or failure
  never masks the original cascade error, which is still returned
  (`ActionExecutionError`). Compensation is a rollback hook, not a catch.
- **Cascade order**: the reaction guard is the "should we attempt it" gate and
  `on_fail` is the "what if it fails" hook — two distinct concerns on one
  reaction.

## Scenario

```json
{
  "expected_final_states": {
    "order": "paid",
    "inventory": "allocated"
  },
  "expected_guard_blocked": [],
  "events": [
    { "signal_id": "order", "event": "pay", "payload": { "auto": false } },
    { "signal_id": "order", "event": "reset" },
    { "signal_id": "order", "event": "pay", "payload": { "auto": true }, "fail_actions": ["commit_stock"] },
    { "signal_id": "order", "event": "reset" },
    { "signal_id": "order", "event": "pay", "payload": { "auto": true } }
  ]
}
```

- **Event 0** `pay({auto:false})`: main transition commits `order pending ->
  paid`. Reaction guard `auto == true` is **false** → reaction **skipped**
  (silent, not an error). `inventory` is untouched.
  State: `order=paid, inventory=ok`.
- **Event 1** `reset`: `order paid -> pending` (returns to pending for the next
  attempt; no reaction watches `reset`).
  State: `order=pending, inventory=ok`.
- **Event 2** `pay({auto:true})` with `fail_actions: ["commit_stock"]`: main
  transition commits `order pending -> paid`. Guard is **true** → cascade fires
  `inventory allocate`, but `commit_stock` fails → `inventory` rolls back to
  `ok` and the cascade error propagates → `release_holding` **runs as
  compensation** before the error surfaces as `ActionExecutionError`.
  State: `order=paid, inventory=ok`.
  This is the M47 compensation point: the side effect was attempted (guard was
  true) and undone when its downstream step failed.
- **Event 3** `reset`: `order paid -> pending`.
  State: `order=pending, inventory=ok`.
- **Event 4** `pay({auto:true})`: guard **true** → cascade fires, this time
  `commit_stock` succeeds → `inventory ok -> allocated`. No compensation runs
  (the cascade did not fail). A previously compensated attempt is not doomed.
  State: `order=paid, inventory=allocated`.

Final: `order = paid`, `inventory = allocated`.

`expected_guard_blocked` is empty because the reaction guard's false case is a
silent *skip*, not a `GuardBlocked`; the only error is the injected action
failure at event 2 (`ActionExecutionError`), driven by `fail_actions`.

## Expected key output (via `sts`)

```
sts> state
order: pending
inventory: ok
sts> event order pay {"auto":false}
order -> paid
sts> state
order: paid
inventory: ok
sts> event order reset
order -> pending
sts> fail commit_stock
sts> event order pay {"auto":true}
order -> paid
[action error] inventory.commit_stock      # injected failure
[compensation] release_holding             # <- on_fail hook ran
Error: Action execution failed for 'commit_stock': ...
State rolled back to 'ok'
sts> reset
sts> event order reset
order -> pending
sts> event order pay {"auto":true}
order -> paid
sts> state
order: paid
inventory: allocated
```

The trace at event 2 records `inventory.commit_stock` `ActionFailed` +
`inventory` `Rollbacked allocated -> ok`, followed by the `release_holding`
compensation (`ReactionCompensated`) — and only *then* the cascade error. Event
4 records a clean `inventory ok -> allocated` with no `ReactionCompensated`,
proving the compensation fires only on a genuine cascade failure.
