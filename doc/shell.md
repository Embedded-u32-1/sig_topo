# sts — Interactive Shell

`sts` (signal-topology-shell) loads a topology JSON and drops you into a REPL for driving signals through transitions by hand. It is the manual, exploratory counterpart to the batch runner `stt` and the visualizer `stv`.

## Install / Prepare

Needs a Rust toolchain (edition 2021). `sts` is built together with the other binaries — no separate dependency:

```bash
cargo build
```

## Start

```bash
cargo run --bin sts -- <topology.json>
```

`sts` uses `load_topology`, so multi-file topologies with `includes`, `components`, and `instances` are resolved and expanded exactly as `stv` / `stt` do. Every action the expanded transitions reference is auto-registered with a print-and-record handler, so the whole chain is observable without writing Rust.

On start it prints a banner and waits at the `sts>` prompt:

```
sts (signal-topology-shell). Topology loaded from 'examples/order_approval.json'.
Type 'help' for commands. Type 'quit' to exit.
sts>
```

## Commands

```
Commands:
  event <signal> <event> [json payload]  send an event to a signal
  state                                   list all signal states
  trace                                   print the trace log
  help                                    show this help
  quit / exit                             leave the shell
```

### `event <signal> <event> [json payload]`

Sends an event to a signal. The payload is the remainder of the line after the event name, parsed as JSON if present; compact or spaced JSON is handled because `sts` slices it from the raw line rather than rejoining `parts`:

```
sts> event order approve {"amount":5000}
[action] order.reserve_inventory
[action] order.notify_customer_approved
order -> approved
  action executed: reserve_inventory
  action executed: notify_customer_approved
```

- `[action] <signal>.<id>` — printed live by the print-and-record handler as each action runs.
- `<signal> -> <to>` — the resulting state.
- `  action executed: <id>` — the action summary, after the transition resolves.

If the transition is blocked by a guard, or the action fails, `sts` prints the error and the rolled-back state:

```
sts> event order approve {"amount":0}
Error: Transition blocked by guard 'payload.amount > 0 and payload.amount <= 100000' for signal 'order' on event 'approve'
State rolled back to 'submitted'
```

A `Transition not found` error (no matching transition for the current state, e.g. sending `approve` when the signal is already `approved`) is reported the same way.

### `state`

Lists every signal and its current state, sorted by id:

```
sts> state
order: submitted
```

When multiple signals exist there is one `<id>: <state>` line per signal.

### `trace`

Prints the full trace log in the same layout produced by `stt`: one `EventReceived` / `ActionStarted` / `ActionSucceeded` / `ActionFailed` / `StateChanged` / `Rollbacked` line per entry, with a monotonic timestamp. A guard-blocked event still logs its `EventReceived` line but no `StateChange` follows it — useful for confirming the event arrived before it was rejected.

```
sts> trace
[1784231128980] EventReceived order.submit payload=None
[1784231128980] ActionStarted order.log_draft_exit
...
```

With no events yet, `trace` prints `(no trace events)`.

### `help` / `quit` / `exit`

`help` reprints the command list; `quit` or `exit` (and end-of-input / Ctrl-D) leave the shell.

## Debugging workflow

1. Load the topology, run `state` to confirm the initial state.
2. Send an event, then `state` again to see where it landed.
3. If the result is wrong, run `trace` and read the event/action/state-change chain:
   - an `EventReceived` with no `StateChange` means the guard blocked it. Re-check the guard expression and the payload;
   - an `ActionFailed` followed by a `Rollbacked` is the M21 transaction rollback — the state returned to what it was before the failed transition.
4. Guard expressions read `payload.<field>` (see [Guards](guards.md)). Missing fields evaluate to `null`, which is falsy, so `payload.emergency == true` blocks cleanly when the field is absent.

## sts vs stt

| | `sts` | `stt` |
|---|---|---|
| Input | interactive REPL, one event at a time | a `scenario.json` file of events, run in batch |
| Output | per-command result + live action lines; trace on demand | one trace dump after all events run |
| Use case | exploring, debugging, teaching | regression runs, reproducible replay (see [Tracing](tracing.md)) |

Both build the engine the same way (resolve includes, expand instances, register a handler per action), so a flow verified interactively in `sts` can be captured into a `scenario.json` and replayed with `stt`.

## End-to-end demo

The following transcript runs verbatim against [examples/order_approval.json](../examples/order_approval.json). Copy the input lines after the prompt:

```bash
cargo run --bin sts -- examples/order_approval.json
```

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
sts> event order approve {"amount":5000}
[action] order.reserve_inventory
[action] order.notify_customer_approved
order -> approved
  action executed: reserve_inventory
  action executed: notify_customer_approved
sts> event order ship
[action] order.dispatch_order
[action] order.notify_shipped
order -> shipped
sts> trace
[...](trace follows)
sts> quit
```

A second walk-through with `gate_flow.json` — covering the `*` wildcard reset and multi-action `on-transition` — lives in [examples/gate_flow.md](../examples/gate_flow.md).
