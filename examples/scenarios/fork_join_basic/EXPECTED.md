# Fork-Join Basic — Scenario

The M44 fork/join feature: a single transition fans out to *two* parallel
reactions via a `fork { }` block, and a `join fork0 { }` reaction waits until
every fork member has completed before it fires. This is the canonical
"payment and inventory happen in parallel; shipping waits for both" workflow
pattern — the minimal fork/join that the pre-M44 linear cascade (A→B→C) could
not express.

Path: `examples/scenarios/fork_join_basic/`.

## Signals

| id         | initial_state | states                 |
|------------|---------------|------------------------|
| `order`    | `pending`     | `pending`, `paid`      |
| `inventory`| `ok`          | `ok`, `reserved`       |
| `audit`    | `flagged`     | `flagged`, `clean`     |
| `shipment` | `pending`     | `pending`, `ready`     |

## Transitions

| signal     | from      | event     | to        |
|------------|-----------|-----------|-----------|
| `order`    | `pending` | `pay`     | `paid`    |
| `inventory`| `ok`      | `reserve` | `reserved`|
| `audit`    | `flagged` | `clear`   | `clean`   |
| `shipment` | `pending` | `dispatch`| `ready`   |

## Reactions

The first two reactions are grouped in `fork { }` (auto-named `fork0`); the
third is held in `join fork0 { }` until the group completes.

| from_signal | from_state | to_signal   | event     | fork group | requires |
|-------------|------------|-------------|-----------|------------|----------|
| `order`     | `paid`     | `inventory` | `reserve` | `fork0`    | —        |
| `order`     | `paid`     | `audit`     | `clear`   | `fork0`    | —        |
| `order`     | `paid`     | `shipment`  | `dispatch`| —          | `fork0`  |

## Teaching points

- **Fork (M44)**: `fork { ... }` assigns every member reaction the same
  auto-generated `join_group` (`fork0`, `fork1`, …) in source order. All
  members fire on the triggering transition — here paying the order fans out to
  reserve inventory *and* clear audit, each as its own per-signal-atomic
  cascade.
- **Join (M44)**: `join fork0 { ... }` gives each member reaction a `requires`
  on the named group. Those reactions are held back until every `fork0` member
  has fired. `shipment` therefore becomes `ready` only after *both*
  `inventory` and `audit` have changed — the join is the synchronization bar.
- **Ordering guarantee**: a trace shows `StateChanged` for `inventory` and
  `audit` *before* `shipment` leaves `pending`. That ordering is the observable
  proof that the join waited for the group rather than firing immediately.
- **Backward compatibility**: a reaction with no `join_group` and empty
  `requires` behaves exactly like the pre-M44 cascade, so existing topologies
  are unchanged.

## Scenario

```json
{
  "expected_final_states": {
    "order": "paid",
    "inventory": "reserved",
    "audit": "clean",
    "shipment": "ready"
  },
  "expected_guard_blocked": [],
  "events": [
    { "signal_id": "order", "event": "pay" }
  ]
}
```

- Event 0 `pay`: `order pending -> paid`. The main transition commits, then the
  fork fires both members in source order — `inventory ok -> reserved`,
  `audit flagged -> clean` — and once the group is complete the join releases
  `shipment pending -> ready`.
- Final: `order = paid`, `inventory = reserved`, `audit = clean`,
  `shipment = ready`.

## Expected key output (via `sts`)

```
sts> state
order: pending
inventory: ok
audit: flagged
shipment: pending
sts> event order pay
order -> paid
sts> state
order: paid
inventory: reserved
audit: clean
shipment: ready
```

The trace for the `pay` event records, in order:
`StateChanged order: pending -> paid`,
`StateChanged inventory: ok -> reserved`,
`StateChanged audit: flagged -> clean`, and
`StateChanged shipment: pending -> ready` — the last of the four
proving the join held `shipment` until both fork branches had completed.
