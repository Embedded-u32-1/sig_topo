# Scenarios — M33

A library of self-contained **scenario = regression test + teaching walk-through** fixtures. Each scenario lives in its own directory under `examples/scenarios/` with three files:

```
examples/scenarios/<name>/
├── <name>.ddl            # topology (Domain Description Language)
├── <name>.scenario.json  # replay + assertions
└── EXPECTED.md           # annotated walk-through ("EXPECTED transcript")
```

The test `tests/scenarios_test.rs` **discovers every subdirectory automatically** — there is no central registry. Drop a new `<name>/` directory in with its three files and it is picked up and run with no test-code change. The test:

1. Compiles `<name>.ddl` through `ddl::compile`.
2. Builds a ready-to-run engine (every action registered against a shared fail-set, so `fail_actions` injection works).
3. Replays `<name>.scenario.json` event by event, asserting:
   - events listed in `expected_guard_blocked` raise `GuardBlocked`;
   - events naming `fail_actions` raise `ActionExecutionError` (injected failure);
   - all other events return `Ok`;
4. Asserts every signal's final state matches `expected_final_states`.

Because the assertions travel with the fixture, the `EXPECTED.md` transcripts in this directory can never silently drift from the engine — the test is the regression guard and the markdown is the teaching material, for the price of one.

## Running

```bash
# Run every scenario (the full regression + teaching library)
cargo test --test scenarios_test

# Run a single scenario by name substring
cargo test --test scenarios_test <name>
```

## Scenario index

| Scenario | Files | Teaching points |
|----------|-------|-----------------|
| [`order_approval`](examples/scenarios/order_approval/EXPECTED.md) | [`order_approval.ddl`](examples/scenarios/order_approval/order_approval.ddl) · [`order_approval.scenario.json`](examples/scenarios/order_approval/order_approval.scenario.json) · [`EXPECTED.md`](examples/scenarios/order_approval/EXPECTED.md) | Happy path + guard block + recovery. A guarded `approve` blocks on `{"amount":0}` (`GuardBlocked`), then a later well-formed `approve({"amount":5000})` commits — a block is not a dead end. All three lifecycle hooks on the first transition. |
| [`gate_flow`](examples/scenarios/gate_flow/EXPECTED.md) | [`gate_flow.ddl`](examples/scenarios/gate_flow/gate_flow.ddl) · [`gate_flow.scenario.json`](examples/scenarios/gate_flow/gate_flow.scenario.json) · [`EXPECTED.md`](examples/scenarios/gate_flow/EXPECTED.md) | Wildcard reset + emergency guard + the "wildcard is live" proof. A `reset` from *any* state funnels to `closed`; the `closed -> closed` self-loop proves the `*` wildcard matches the current state rather than being a no-op. Guard blocks `fault` when `emergency` is false. |
| [`task_cascade`](examples/scenarios/task_cascade/EXPECTED.md) | [`task_cascade.ddl`](examples/scenarios/task_cascade/task_cascade.ddl) · [`task_cascade.scenario.json`](examples/scenarios/task_cascade/task_cascade.scenario.json) · [`EXPECTED.md`](examples/scenarios/task_cascade/EXPECTED.md) | Cross-signal cascade, the three headline facts in one walk-through: a reaction fires a derived event on another signal; a guarded reaction is *selectively* skipped when its guard is false (and that skip is silent, not an error); the main transition commits regardless. |
| [`fail_rollback`](examples/scenarios/fail_rollback/EXPECTED.md) | [`fail_rollback.ddl`](examples/scenarios/fail_rollback/fail_rollback.ddl) · [`fail_rollback.scenario.json`](examples/scenarios/fail_rollback/fail_rollback.scenario.json) · [`EXPECTED.md`](examples/scenarios/fail_rollback/EXPECTED.md) | Action failure + rollback + recovery. `reserve_inventory` is injected to fail for one event (`fail_actions`), the engine records `ActionFailed` + `Rollbacked` and reverts the signal; the same event re-run *without* the injection commits — a rolled-back transition is not doomed. |

## `.scenario.json` format (extended)

The shared `run::ScenarioEvent` is reused for `events` (so `fail_actions` injection works unchanged). Two metadata fields carry the assertions:

```json
{
  "expected_final_states": { "order": "shipped" },
  "expected_guard_blocked": [1],
  "events": [
    { "signal_id": "order", "event": "submit" },
    {
      "signal_id": "order",
      "event": "approve",
      "payload": { "amount": 0 },
      "fail_actions": ["reserve_inventory"]
    }
  ]
}
```

| Field | Meaning |
|-------|---------|
| `expected_final_states` | `signal_id -> state` the engine must hold after the whole scenario resolves. |
| `expected_guard_blocked` | Zero-based indices of events expected to raise `GuardBlocked`. Optional; defaults to `[]`. |
| `events` | Ordered replay. Each `ScenarioEvent` has `signal_id`, `event`, optional `payload`, and optional `fail_actions` (action ids forced to fail for that event only — per-event scoped, mirroring `run::run_scenario`). |

An event that both is listed in `expected_guard_blocked` (a *transition guard* block) and names `fail_actions` (an *injected action* failure) is a contradiction — keep the two disjoint. Use `expected_guard_blocked` for `GuardBlocked`, `fail_actions` for `ActionExecutionError`, and neither for plain happy-path events.

## Adding a scenario

1. `mkdir examples/scenarios/<name>`.
2. Write `<name>.ddl` — the topology. Keep it minimal and focused on one or two teaching points; start from an existing scenario's structure.
3. Write `<name>.scenario.json` — the replay. Set `expected_final_states` and, if you exercise a guard block, list its event index in `expected_guard_blocked`; if you inject a failure, name it in the event's `fail_actions`.
4. Write `EXPECTED.md` — the walk-through in the style of the others: a signal/transition table, the teaching points, the literal scenario JSON, and the expected `sts`/`stt` transcript. The test does **not** read this file, so its job is purely human teaching — but `cargo test --test scenarios_test` is what keeps its claims honest.
5. Run `cargo test --test scenarios_test <name>` and confirm green. No registration anywhere else is needed.

## Notes & known limitations

- **DDL wildcard**: the DDL compiler (v0.12) binds exactly one source state per transition, so the JSON wildcard `from *` is written in DDL as one explicit transition per source state (see `gate_flow.ddl`). The engine behavior is identical; the `closed -> closed` self-loop is the proof.
- **DDL actions**: exactly one action per lifecycle hook is supported, so a multi-action `on_transition: a, b` from the JSON fixtures is expressed as separate transitions/hooks (see `gate_flow.ddl`).
- **Reaction payloads**: DDL does not yet emit reaction payloads (deferred in M28), so scenario guards that need to read `payload.*` must be driven from the *source* event's payload (as `task_cascade` does).
