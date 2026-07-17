# Signal Cascades

The signal topology engine supports controlled cascades through **reactions**. A reaction declares that when a signal reaches a specific state, the engine should automatically send an event to another signal.

## Reaction Semantics

A reaction is defined in the topology JSON under the `reactions` array:

```json
{
  "from_signal": "task_status",
  "from_state": "running",
  "to_signal": "ui_indicator",
  "event": "set_busy",
  "payload": { "color": "blue" }
}
```

| Field         | Meaning                                                            |
|---------------|--------------------------------------------------------------------|
| `from_signal` | Signal whose transition result triggers the reaction.              |
| `from_state`  | State that must be entered to trigger. Use `"*"` for any state.    |
| `to_signal`   | Signal that receives the cascaded event.                           |
| `event`       | Event name sent to `to_signal`.                                    |
| `payload`     | Optional static JSON payload forwarded with the cascaded event.    |

### Lifecycle

1. An external event is delivered to a signal via `TopologyEngine::send_event`.
2. The signal transitions to a new state.
3. After the transition completes (including all actions), the engine scans all reactions.
4. Matching reactions are executed by recursively sending events to their `to_signal`.
5. Each cascaded event is recorded in the trace as a normal `EventReceived` entry.

Reactions are evaluated in definition order. The original `send_event` call returns the `TransitionResult` of the top-level transition only; cascaded transitions are side effects.

## Reaction Guards (M32)

A reaction may carry a `when <guard>` clause that gates the cascade. At
cascade time the engine evaluates the guard against the **source event's
payload** — the payload of the `send_event` call that triggered the
transition the reaction reacts to — mirroring how a transition guard reads
its own event's payload. The reaction's static `payload` (the derived event's
payload delivered to the target signal) is a separate value and is *not* what
the guard reads.

```json
{
  "from_signal": "order",
  "from_state": "approved",
  "to_signal": "inventory",
  "event": "allocate",
  "payload": { "sku": "widget" },
  "guard": "payload.auto == true"
}
```

Semantics:

- guard is absent, or evaluates to `true` → the cascade fires.
- guard evaluates to `false` → that reaction is skipped. The main transition
  has already committed, and the remaining reactions/fireflies are untouched.
- guard fails to evaluate (syntax error, etc.) → that reaction is skipped, not
  an error. A single misbehaving guard never breaks the whole cascade chain.

Use a reaction gate when the *decision to cascade* should depend on the data
that caused the transition (e.g. only auto-allocate inventory for orders with
`auto: true`). Use a transition guard on the *target* when the target signal's
own entry should be conditional.

This is configured in DDL as `reaction { when <sig> enters <state> -> <tgt> <ev>
[when <guard>] }` and in JSON via the `reactions[].guard` field. Engines before
M32 defaulted the field to `None` (`#[serde(default)]`), so legacy topology
files cascade unconditionally with no change.

## Cascade Depth Limit

To prevent runaway recursion and stack overflow, the engine enforces a maximum cascade depth. The default limit is `8`. Use `TopologyEngine::set_max_cascade_depth` to configure it.

When a cascade would exceed the limit, `EngineError::CascadeDepthExceeded` is returned. Any state changes from successful cascade levels before the failure remain applied.

## Error Semantics

- `CascadeDepthExceeded`: a recursive cascade exceeded `max_cascade_depth`.
- `ReactionSignalNotFound`: validation failed because a reaction referenced a signal not declared in `signals`.
- Other cascade failures (`TransitionNotFound`, `GuardBlocked`, action errors, etc.) propagate immediately. The original transition's result and state are **not** rolled back.

## Example Topology

```json
{
  "version": "0.1",
  "signals": [
    {
      "id": "task_status",
      "initial_state": "idle",
      "states": ["idle", "running", "success"]
    },
    {
      "id": "ui_indicator",
      "initial_state": "ready",
      "states": ["ready", "busy", "done"]
    }
  ],
  "transitions": [
    {
      "signal_id": "task_status",
      "from": "idle",
      "event": "start",
      "to": "running"
    },
    {
      "signal_id": "task_status",
      "from": "running",
      "event": "finish",
      "to": "success"
    },
    {
      "signal_id": "ui_indicator",
      "from": "ready",
      "event": "set_busy",
      "to": "busy"
    },
    {
      "signal_id": "ui_indicator",
      "from": "busy",
      "event": "set_done",
      "to": "done"
    }
  ],
  "reactions": [
    {
      "from_signal": "task_status",
      "from_state": "running",
      "to_signal": "ui_indicator",
      "event": "set_busy",
      "payload": { "color": "blue" }
    },
    {
      "from_signal": "task_status",
      "from_state": "success",
      "to_signal": "ui_indicator",
      "event": "set_done"
    }
  ]
}
```

Sending `start` to `task_status` transitions it to `running`, which then sends `set_busy` to `ui_indicator` with the static payload `{ "color": "blue" }`.

## Visualization

Cross-signal reactions are intentionally omitted from DOT export to keep diagrams focused on explicit transitions.
