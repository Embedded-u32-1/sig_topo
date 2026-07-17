# Task Cascade — Scenario

A two-signal topology with two reactions — one unguarded, one guarded. In a single walk-through it demonstrates the three headline facts about cross-signal cascades: (1) a reaction fires a derived event on another signal, (2) a guarded reaction is *selectively* skipped when its guard is false, and (3) the main transition commits regardless of whether any reaction fires or is skipped.

Path: `examples/scenarios/task_cascade/`.

## Signals

| Field | Value |
|-------|-------|
| `id` | `task` |
| `initial_state` | `idle` |
| `states` | `idle`, `running`, `done` |

| Field | Value |
|-------|-------|
| `id` | `ui` |
| `initial_state` | `ready` |
| `states` | `ready`, `busy`, `done` |

## Transitions

| signal | from | event | to | actions |
|--------|------|-------|----|---------|
| `task` | `idle` | `start` | `running` | `on_transition: begin_work` |
| `task` | `running` | `finish` | `done` | `on_transition: complete_work` |
| `ui` | `ready` | `show_busy` | `busy` | `on_transition: render_busy` |
| `ui` | `busy` | `show_done` | `done` | `on_transition: render_done` |

## Reactions

| when | guard | derived event |
|------|-------|---------------|
| `task` enters `running` | — | `ui show_busy` |
| `task` enters `done` | `payload.record == true` | `ui show_done` |

## Teaching points

- **Cascade trigger** (event 0): `task start` commits `idle -> running`; reaction A fires and delivers `ui show_busy`, moving `ui` `ready -> busy`. The main transition's commit is what *causes* the derived event.
- **Guard selective skip** (event 1): `task finish` carries `record: false`, so reaction B's guard `payload.record == true` is false. Reaction B is skipped — `ui` stays `busy` — while the main transition still commits `task running -> done`.
- **Main transition always commits**: reaction B being skipped is *not* an error. `send_event` returns `Ok` and `task` reaches `done`. A reaction guard that is false behaves like "this cascade does not apply", not like a failure (see `doc/cascades.md` / `doc/transaction.md`).
- **Guard reads the source payload**: the guard is evaluated against the payload of the `send_event` call that triggered the main transition (`{"record": false}`), not against any payload on the derived event. This mirrors how a transition guard reads its own event's payload.

## Scenario

```json
{
  "expected_final_states": { "task": "done", "ui": "busy" },
  "expected_guard_blocked": [],
  "events": [
    { "signal_id": "task", "event": "start" },
    { "signal_id": "task", "event": "finish", "payload": { "record": false } }
  ]
}
```

- Event 0 `task start`: commits `task idle -> running`; reaction A fires → `ui ready -> busy`.
- Event 1 `task finish({record:false})`: commits `task running -> done`; reaction B guard is false → cascade skipped, `ui` stays `busy`.
- Final: `task = done`, `ui = busy`.

`expected_guard_blocked` is empty: reaction-guard skips are silent, so no `GuardBlocked` is raised. To observe the guarded reaction *flying*, replay event 1 with `{"record": true}` — `ui` would then reach `done` while `task` still reaches `done`.

## Expected trace highlights (via `sts trace` / `stt`)

After event 0, the trace records (in order):
- `EventReceived task.start`
- `ActionStarted/ActionSucceeded task.begin_work`
- `StateChanged task: idle -> running`
- `EventReceived ui.show_busy` (the derived event)
- `ActionStarted/ActionSucceeded ui.render_busy`
- `StateChanged ui: ready -> busy`

After event 1:
- `EventReceived task.finish`
- `ActionStarted/ActionSucceeded task.complete_work`
- `StateChanged task: running -> done`
- (no `EventReceived` for `ui.show_done` — reaction B was skipped)

`ui` never receives a derived event during event 1, which is the observable proof that false-guarded reactions are dropped without touching the main transition.
