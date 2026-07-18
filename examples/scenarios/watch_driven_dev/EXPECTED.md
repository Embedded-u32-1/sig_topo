# Watch-Driven Development — Scenario

The M51 `stc watch` development loop: a watcher polls a `.ddl` file for changes
and recompiles it on every save; with `--scenario` it also *replays a scenario*
against a fresh engine after each successful compile, so an edit that breaks
the scenario is caught the instant it is saved. The `.ddl` here is the kind of
topology you would iterate on under the watch — a guarded, multi-action
workflow whose guard threshold you tune as the business rule evolves — and the
scenario is the regression contract the watcher guards.

Path: `examples/scenarios/watch_driven_dev/`.

## Signals

| id          | initial_state | states                       |
|-------------|---------------|------------------------------|
| `withdrawal`| `draft`       | `draft`, `submitted`, `approved` |
| `ledger`    | `balanced`    | `balanced`, `debit`          |

## Transitions

| signal       | from        | event    | to         | guard                              | actions                              |
|--------------|-------------|----------|------------|------------------------------------|--------------------------------------|
| `withdrawal`| `draft`     | `submit` | `submitted`| —                                  | `on_transition: validate_request`, `on_enter: log_submitted` |
| `withdrawal`| `submitted` | `approve`| `approved` | `payload.amount > 0 and payload.amount <= 1000` | `on_transition: reserve_funds`, `on_enter: log_approved` |
| `ledger`    | `balanced`  | `debit`  | `debit`   | —                                  | —                                    |

## Reactions

| from_signal | from_state | to_signal | event  | static payload                |
|-------------|------------|-----------|--------|-------------------------------|
| `withdrawal`| `approved` | `ledger`  | `debit`| `{ "origin": "withdrawal" }` |

## Teaching points

- **Watch mode (M51)**: `stc watch w.ddl --scenario w.scenario.json` compiles
  once at startup, then recompiles on every mtime change. A clean compile
  prints `Recompiled OK`; a compile error prints `Recompile failed: ...` with
  line/column and **keeps watching** so the next edit is compiled afresh.
- **Scenario regression on save (M51)**: with `--scenario`, every successful
  compile replays the scenario against a fresh engine, printing
  `Scenario PASS: N event(s)` or `Scenario FAIL: k/N event(s) failed`. An edit
  that regresses the scenario (tightening the guard past a scenario amount,
  renaming an action, breaking the syntax) is detected on save, not in
  production.
- **Debounce (M51)**: edits landing within 200 ms of the previous compile are
  debounced so a half-written file is not compiled.
- **The guard knob**: `payload.amount > 0 and payload.amount <= 1000` is the
  tuning point. Tightening it to `<= 500` makes the "500" scenario arm flip
  from pass to `GuardBlocked`; the watcher reports the scenario failure at the
  next save. The scenario is the contract that keeps the tuning honest.
- **Trace**: a passing `approve` records the multi-action hook
  (`validate_request`→`reserve_funds`→`log_approved`), then the cascaded
  `ledger` `debit` with its static payload.

## Scenario

```json
{
  "expected_final_states": {
    "withdrawal": "approved",
    "ledger": "debit"
  },
  "expected_guard_blocked": [1],
  "events": [
    { "signal_id": "withdrawal", "event": "submit" },
    { "signal_id": "withdrawal", "event": "approve", "payload": { "amount": 5000 } },
    { "signal_id": "withdrawal", "event": "approve", "payload": { "amount": 500 } }
  ]
}
```

- **Event 0** `submit`: `withdrawal draft -> submitted`; `validate_request` then
  `log_submitted`. Reaction watches `approved`, does not match.
  State: `withdrawal=submitted, ledger=balanced`.
- **Event 1** `approve({amount:5000})`: guard `5000 <= 1000` is **false** →
  **GuardBlocked**, state stays `submitted`. This is the event in
  `expected_guard_blocked`. No lifecycle action runs, no cascade.
  State: `withdrawal=submitted, ledger=balanced`.
- **Event 2** `approve({amount:500})`: guard **true** → `withdrawal submitted ->
  approved`; `reserve_funds` then `log_approved`. The reaction fires the
  derived `debit` event with payload `{ "origin": "withdrawal" }` → `ledger
  balanced -> debit`.
  State: `withdrawal=approved, ledger=debit`.

Final: `withdrawal = approved`, `ledger = debit`.

Note how the scenario stays *passing* under `stc watch` while the guard still
permits a 500-amount approval; tightening the guard to `<= 100` would make
event 2 the new guard-block and the watcher would report
`Scenario FAIL: 1/3 event(s) failed` on the next save — the regression caught
live.

## Expected key output (via `stc watch` + `sts`)

Watcher session (the watcher recompiles + replays on every save):

```
$ stc watch withdrawal.ddl --scenario withdrawal.scenario.json --interval 200
Watching 'withdrawal.ddl' every 200ms (Ctrl+C to stop)...
On each successful compile, running scenario 'withdrawal.scenario.json'
Recompiled OK
Scenario PASS: 3 event(s)          # initial save
```

Interactive `sts` transcript for the final passing run:

```
sts> state
withdrawal: draft
ledger: balanced
sts> event withdrawal submit
[action] withdrawal.validate_request
[action] withdrawal.log_submitted
withdrawal -> submitted
  action executed: validate_request
  action executed: log_submitted
sts> state
withdrawal: submitted
ledger: balanced
sts> event withdrawal approve {"amount":5000}
Error: Transition blocked by guard 'payload.amount > 0 and payload.amount <= 1000' for signal 'withdrawal' on event 'approve'
State rolled back to 'submitted'
sts> state
withdrawal: submitted
ledger: balanced
sts> event withdrawal approve {"amount":500}
[action] withdrawal.reserve_funds
[action] withdrawal.log_approved
withdrawal -> approved
  action executed: reserve_funds
  action executed: log_approved
sts> state
withdrawal: approved
ledger: debit
```

The watcher keeps a tight loop: edit the guard, save, and the
`Scenario PASS`/`Scenario FAIL` line tells you immediately whether the saved
edit regressed the contract — which is the whole point of watch-driven
development.
