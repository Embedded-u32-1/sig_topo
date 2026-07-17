# Guard Template — Scenario

The M38 "guard template / reuse" feature: a single top-level
`guard <id> { <expr> }` declaration whose expression is shared by multiple
reactions via `when <id>`. The compiler inlines the guard's expression into
every reaction that references it, so the reactions behave identically and the
expression is written only once.

Path: `examples/scenarios/guard_template/`.

## Guard declaration

| id          | expr                    |
|-------------|-------------------------|
| `allow_alloc` | `payload.auto == true` |

## Signals

| id         | initial_state | states              |
|------------|---------------|---------------------|
| `order`    | `pending`     | `pending`, `approved` |
| `inventory`| `idle`        | `idle`, `allocated` |
| `audit`    | `idle`        | `idle`, `noted`     |

## Transitions

| signal   | from      | event    | to        |
|----------|-----------|----------|-----------|
| `order`  | `pending` | `approve`| `approved`|
| `order`  | `approved`| `reset`  | `pending` |
| `inventory`| `idle` | `allocate`| `allocated`|
| `audit`  | `idle`    | `note`   | `noted`   |

## Reactions

Both reactions reference `when allow_alloc`, so both share the guard
`payload.auto == true`.

| from_signal | from_state | to_signal   | event    | guard          |
|-------------|------------|-------------|----------|----------------|
| `order`     | `approved` | `inventory` | `allocate` | `allow_alloc` |
| `order`     | `approved` | `audit`     | `note`   | `allow_alloc` |

## Teaching points

- **Guard template (M38)**: `guard allow_alloc { payload.auto == true }` is a
  named, reusable guard. Reactions reference it via `when allow_alloc` rather
  than rewriting the expression.
- **Inline expansion**: the compiler inlines the guard's expression, so both
  reactions end up with the guard text `payload.auto == true` — identical to
  writing it literally in each reaction.
- **Shared behavior**: when the guard is true both reactions fire; when false
  both skip. The main transition commits either way.
- **Trace (M38 part B)**: each reaction guard evaluation emits a
  `ReactionGuardEvaluated` trace event with `result` `"true"`, `"false"`, or
  `"error: <msg>"`, so a trace shows *why* a reaction fired or was skipped.

## Scenario

```json
{
  "expected_final_states": {
    "order": "approved",
    "inventory": "allocated",
    "audit": "noted"
  },
  "expected_guard_blocked": [],
  "events": [
    { "signal_id": "order", "event": "approve", "payload": { "auto": true } },
    { "signal_id": "order", "event": "reset" },
    { "signal_id": "order", "event": "approve", "payload": { "auto": false } }
  ]
}
```

- Event 0 `approve({auto:true})`: guard is true → both reactions fire.
  `order -> approved`, `inventory -> allocated`, `audit -> noted`.
- Event 1 `reset`: `order -> pending` (reactions list `from_state: approved`,
  so no reaction matches).
- Event 2 `approve({auto:false})`: guard is false → both reactions **skip**
  (`ReactionGuardEvaluated result="false"`), but the main transition still
  commits: `order -> approved`. The state of `inventory` and `audit` from
  event 0 is retained.
- Final: `order = approved`, `inventory = allocated`, `audit = noted`.

## Expected key output (via `sts`)

```
sts> state
order: pending
inventory: idle
audit: idle
sts> event order approve {"auto":true}
order -> approved
sts> state
order: approved
inventory: allocated
audit: noted
sts> event order reset {}
order -> pending
sts> state
order: pending
inventory: allocated
audit: noted
sts> event order approve {"auto":false}
order -> approved
sts> state
order: approved
inventory: allocated
audit: noted
```

The trace from the final `approve({auto:false})` shows two
`ReactionGuardEvaluated` lines with `result=false` — the M38 signal that both
shared-guard reactions were intentionally skipped.
