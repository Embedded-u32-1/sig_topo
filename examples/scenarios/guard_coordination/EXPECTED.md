# Guard Coordination — Scenario

The M38/M39 "guard template / reuse" feature: a single top-level
`guard <id> { <expr> }` declaration whose expression is shared by *multiple*
reactions via `when <id>`. The compiler inlines the guard's expression into
every reaction that references it, so the reactions behave identically — they
**cannot diverge**. This scenario demonstrates that guarantee across three
signals: a shared guard gates inventory reservation *and* audit clearing
together.

Path: `examples/scenarios/guard_coordination/`.

## Guard declaration

| id          | expr                    |
|-------------|-------------------------|
| `canreserve` | `payload.amount <= 100` |

## Signals

| id         | initial_state | states                        |
|------------|---------------|-------------------------------|
| `order`    | `pending`     | `pending`, `paid`, `cancelled`|
| `inventory`| `ok`          | `ok`, `reserved`, `low`       |
| `audit`    | `flagged`     | `clean`, `flagged`            |

## Transitions

| signal     | from      | event    | to         | actions            |
|------------|-----------|----------|------------|--------------------|
| `order`    | `pending` | `pay`    | `paid`     |                    |
| `order`    | `paid`    | `reset`  | `pending`  |                    |
| `order`    | `paid`    | `cancel` | `cancelled`| `verify_cancel`    |
| `inventory`| `ok`      | `reserve`| `reserved` |                    |
| `inventory`| `reserved`| `release`| `ok`       |                    |
| `inventory`| `reserved`| `deduct` | `low`      |                    |
| `audit`    | `clean`   | `mark`   | `flagged`  |                    |
| `audit`    | `flagged` | `clear`  | `clean`    |                    |

## Reactions

The first two reactions share the guard id `canreserve`; the third is unguarded
and always runs on reset.

| from_signal | from_state | to_signal   | event    | guard          | guarded? |
|-------------|------------|-------------|----------|----------------|----------|
| `order`     | `paid`     | `inventory` | `reserve`| `canreserve`   | yes      |
| `order`     | `paid`     | `audit`     | `clear`  | `canreserve`   | yes      |
| `order`     | `pending`  | `audit`     | `mark`   | *(none)*       | no       |

## Teaching points

- **Guard template (M38)**: `guard canreserve { payload.amount <= 100 }` is a
  named, reusable guard. Reactions reference it via `when canreserve` rather
  than rewriting the expression.
- **Inline expansion**: the compiler inlines the guard's expression, so both
  guarded reactions end up with the identical guard text
  `payload.amount <= 100` — exactly as if it had been written out twice.
- **Shared guard => consistent behavior (M39)**: because both reactions share
  the *same* guard, they move in lockstep. When the guard is true both fire;
  when false both skip. Inventory reservation and audit clearing can never end
  up in inconsistent states (reserved-but-flagged, or unreserved-but-clean) —
  the guard is single-source-of-truth.
- **Trace (M38 part B)**: each guarded reaction emits a
  `ReactionGuardEvaluated` trace event with `result` `"true"` (fired),
  `"false"` (skipped), or `"error: <msg>"`. A shared guard shows identical
  `result` values across the reactions that share it.
- **Unguarded reaction**: the reset reaction (`audit mark`) has no guard and
  always fires, showing the contrast — not every reaction needs a guard.

## Scenario

```json
{
  "expected_final_states": {
    "order": "paid",
    "inventory": "reserved",
    "audit": "flagged"
  },
  "expected_guard_blocked": [],
  "events": [
    { "signal_id": "order", "event": "pay", "payload": { "amount": 50 } },
    { "signal_id": "order", "event": "reset" },
    { "signal_id": "order", "event": "pay", "payload": { "amount": 200 } },
    { "signal_id": "order", "event": "cancel", "fail_actions": [ "verify_cancel" ] }
  ]
}
```

- **Event 0** `pay({amount:50})`: guard `50 <= 100` is **true** → both guarded
  reactions fire. `order -> paid`, `inventory ok -> reserved`,
  `audit flagged -> clean`.
  State: `order=paid, inventory=reserved, audit=clean`.
- **Event 1** `reset`: `order paid -> pending`. The unguarded reset reaction
  fires: `audit clean -> flagged`. (The guarded reactions list
  `from_state: paid`, so none match.)
  State: `order=pending, inventory=reserved, audit=flagged`.
- **Event 2** `pay({amount:200})`: guard `200 <= 100` is **false** → both guarded
  reactions **skip** (`ReactionGuardEvaluated result="false"` for each), but the
  main transition still commits: `order -> paid`. `inventory` and `audit` are
  untouched.
  State: `order=paid, inventory=reserved, audit=flagged`.
  This is the M39 consistency point: the shared guard skipped *both* reactions
  together, so neither inventory nor audit moved.
- **Event 3** `cancel` with `verify_cancel` forced to fail: the `cancel`
  transition's `on_transition` action fails, so the engine **rolls back** the
  order to `paid` (and emits a `Rollbacked` trace). No reaction fires on a
  rolled-back transition.
  State: `order=paid, inventory=reserved, audit=flagged`.

Final: `order = paid`, `inventory = reserved`, `audit = flagged`.

## Expected key output (via `sts`)

```
sts> state
order: pending
inventory: ok
audit: flagged
sts> event order pay {"amount":50}
order -> paid
sts> state
order: paid
inventory: reserved
audit: clean
sts> event order reset {}
order -> pending
sts> state
order: pending
inventory: reserved
audit: flagged
sts> event order pay {"amount":200}
order -> paid
sts> state
order: paid
inventory: reserved
audit: flagged
sts> event order cancel {}
order -> paid
sts> state
order: paid
inventory: reserved
audit: flagged
```

The trace from the `pay({amount:200})` event shows two
`ReactionGuardEvaluated` lines with `result=false` — the M39 signal that both
shared-guard reactions were intentionally skipped together, keeping inventory
and audit consistent.
