# `run` module

The `run` module (`src/run.rs`) is the **shared scaffolding** for the three
command-line binaries `sts`, `stt` and `stp`. Its single job is to turn a
topology file plus a shared set of "forced-to-fail" action ids into a ready-to-
run `TopologyEngine`, so the binaries don't each repeat that setup.

It is `pub` only so the binary crates (`src/bin/*.rs`) can build against it.
It is **not part of the stable library surface** -- treat it as build support
for the binaries. Binary usage and the per-binary docs take precedence; expect
these APIs to track the binaries' needs rather than a semver guarantee.

## Exports

| Item | Purpose |
|------|---------|
| `Scenario` | A batch-replay scenario: an ordered list of `ScenarioEvent`s to send. |
| `ScenarioEvent` | One event (`signal_id`, `event`, optional `payload`) plus an optional `fail_actions` list. |
| `load_topology_for_run` | Resolve includes, expand instances, build the engine, register a handler per action. Returns a runnable `TopologyEngine`. |
| `run_scenario` | Replay a `Scenario` against the engine, recording each failure and continuing. Returns the list of `ScenarioError`s. |
| `format_trace` | Format one `TraceEvent` to the single-line layout `sts`/`stt` share. |

## `fail_actions` semantics

Each `ScenarioEvent` may carry a `fail_actions: Vec<String>` (optional,
defaults to empty via `#[serde(default)]`). The ids named there are forced to
fail **for that event only**:

1. Before the event is dispatched, the named ids are inserted into the shared
   fail-set, so their action handler returns `ActionExecutionError`.
2. The engine turns that into a rolled-back transition.
3. After the event resolves, the ids are cleared from the set.

So a later event that re-uses the same action id is unaffected -- injection is
scoped per event, which keeps a scenario readable and replay deterministic.
`run_scenario` captures the rolled-back state at error time, so a caller can
report `ActionFailed` + `Rollbacked` without it being overwritten by the events
that follow.

## Relationship to the binaries

- `sts` (shell) -- `record = true`: prints a live `[action] <signal>.<id>` line
  as each action runs; the REPL's `fail` command writes the same fail-set.
- `stt` (replay) -- `record = false`: silent replay that prints the trace and
  any failures after the run.
- `stp` (persist) -- `record = false`: replays a scenario to drive the engine
  into a state, then `save_state` / `load_state` / `reload_topology`.

All three now build their engine through `load_topology_for_run`, so engine
setup, action registration and fail-injection no longer drift between them.
