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
  fail <action_id>                        force that action to fail (live rollback demo)
  reset                                   clear the forced-failure set
  why <from> <state> <to> <event>         print guard evaluation trace for a reaction
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

### `why <from> <state> <to> <event>`

A reaction fires an event on another signal when its guard is true; when the
guard is false (or fails to evaluate) the reaction is *skipped* silently.
`why` shows the guard-evaluation trace the engine recorded (M38) for one
reaction so you can see *why* it did or did not fire: the guard expression,
the result (`true` / `false` / `error: <msg>`), and a tally over the whole
session. The reaction is identified as `from.state -> to.event` (the
`ReactionGuardEvaluated` trace fields):

```
sts> why order approved inventory allocate
Guard evaluations for reaction order.approved -> inventory.allocate:
[1784321081516] guard=`payload.auto == true` result=true
[1784321081517] guard=`payload.auto == true` result=false
2 evaluation(s); 1 fired, 1 skipped.
```

Use it after an event that *should* have triggered a reaction but didn't: the
false line tells you exactly which guard blocked it. Matching is strict — the
four fields must name a reaction that the engine actually evaluated. If the
reaction was never evaluated (most commonly because `from_state` was never
reached, so the reaction never ran), `why` reports that instead:

```
sts> why order shipped inventory allocate
No guard evaluation found for that reaction (from_state may not have been reached yet).
```

`ReactionGuardEvaluated` records the guard expression and its result, not the
payload that produced the result. To recover that payload, find the
corresponding `EventReceived` line for the same signal in `trace`.

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

## stt scenario format + failure injection

`stt` reads a `scenario.json` that lists events to replay in order:

```json
{
  "events": [
    { "signal_id": "order", "event": "submit" },
    { "signal_id": "order", "event": "approve", "payload": { "amount": 5000 } }
  ]
}
```

Each event is `{ "signal_id", "event", "payload?": json }` -- the same shape the `sts` `event` command sends. Drop the file next to your topology and run:

```bash
cargo run --bin stt -- examples/order_approval.json scenario.json
```

`stt` dumps the full trace log after all events run, identical in layout to the `sts` `trace` command.

### Injecting a failure

An event can optionally name `fail_actions` -- a list of action ids forced to fail *for that event alone*. While the event dispatches, each named action returns an `ActionExecutionError`, so the engine rolls the transition back (exactly like the `sts` `fail` command). After the event resolves the names are cleared, so a later event that re-uses the same action is unaffected.

```json
{
  "events": [
    { "signal_id": "order", "event": "submit" },
    {
      "signal_id": "order",
      "event": "approve",
      "payload": { "amount": 5000 },
      "fail_actions": ["reserve_inventory"]
    },
    { "signal_id": "order", "event": "approve", "payload": { "amount": 5000 } }
  ]
}
```

On a failed event `stt` prints the error and the rolled-back state to stderr and *continues* with the next event -- a replay transcript is most useful when it shows `ActionFailed` + `Rollbacked` in context with the events that follow. The trace above records the failing `approve` as `ActionStarted order.reserve_inventory` -> `ActionFailed ...` -> `Rollbacked order: approved -> submitted`, then the re-try commits to `approved`.

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

## 在 sts 里现场演示回滚

`sts` 能让你「亲眼看到」事务回滚，而无需写 Rust：用 `fail <action>` 把某个动作标记为「持续失败」，再触发它所在的转移，就会在 REPL 里看到 `Error: Action execution error: ...` + `State rolled back to '<源态>'`，`trace` 里则是 `ActionFailed` + `Rollbacked` 而没有 `StateChanged`。

工作原理（不改引擎）：每个动作注册时多读一个共享的 `fail_set`——动作的 id 在集合里就返回 `EngineError::ActionExecutionError`，否则正常打印 + `Ok(())`。引擎本身对待这个错误和对待任何真实动作失败一模一样：试探性地提交目标态后，回滚到源态并记 `Rollbacked`（见 M19 事务回滚）。`fail` 只是让你从 REPL 里控制「哪个动作、在什么时候」失败。

### `fail` 语义

- `fail <action_id>` — 把 `<action_id>` 加入失败集合，回显 `will fail next: <action_id>`。**标记是「持续」的**：加入后该动作每次执行都会失败（方便你反复观察回滚）。
- `reset` — 清空整个失败集合，回显 `fail set cleared`。清空后该动作恢复正常。
- `fail` 不带参数会提示 `Usage: fail <action_id>`。

### 会话转录（实跑）

以下逐字运行自 [examples/order_approval.json](../examples/order_approval.json)。输入行紧跟在 `sts>` 提示符后；输出是实跑的（`trace` 的时间戳每次运行不同，结构一致）。

注意此拓扑里 `approve` 只能从 `submitted` 触发，所以演示从 `submit` 进入 `submitted` 后开始：

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
sts> fail reserve_inventory
will fail next: reserve_inventory
sts> event order approve {"amount":5000}
Error: Action execution error: injected failure for action 'reserve_inventory' (set via `fail`)
State rolled back to 'submitted'
sts> state
order: submitted
sts> trace
[...] EventReceived order.submit payload=None
[...] ActionStarted order.log_draft_exit
[...] ActionSucceeded order.log_draft_exit
[...] ActionStarted order.validate_order_payload
[...] ActionSucceeded order.validate_order_payload
[...] ActionStarted order.notify_submitted
[...] ActionSucceeded order.notify_submitted
[...] StateChanged order: draft -> submitted
[...] EventReceived order.approve payload={"amount":5000}
[...] ActionStarted order.reserve_inventory
[...] ActionFailed order.reserve_inventory error=injected failure for action 'reserve_inventory' (set via `fail`)
[...] Rollbacked order: approved -> submitted
sts> reset
fail set cleared
sts> event order approve {"amount":5000}
[action] order.reserve_inventory
[action] order.notify_customer_approved
order -> approved
  action executed: reserve_inventory
  action executed: notify_customer_approved
sts> state
order: approved
sts> quit
```

逐行看关键点：

1. `fail reserve_inventory` 前，`approve` 会正常走到 `approved`（此处用 `submit` 进入 `submitted`，给 `fail` 之后那次 `approve` 备好起点）。
2. `fail` 之后发 `approve`：`reserve_inventory` 在 `on_transition` 阶段失败，引擎打印 `Error: Action execution error: ...`，随后 `State rolled back to 'submitted'`——信号态回到转移前的 `submitted`。
3. `trace` 里，`approve` 那次是 `ActionStarted order.reserve_inventory` → `ActionFailed ...` → `Rollbacked order: approved -> submitted`，**没有** `StateChanged order: submitted -> approved`——和 M19 事务回滚「`StateChanged` 缺席、`ActionFailed` + `Rollbacked` 到场」完全一致。
4. `reset` 清空失败集合后，同样的 `approve` 再次走到 `approved`。

> **Tip: debugging a reaction that did not fire.** The rollback trace shows
> *why a transition failed* (an `ActionFailed` + `Rollbacked`), but a skipped
> *reaction* leaves no such trace — its guard was simply false. Use `why
> <from> <state> <to> <event>` (see above) to read the guard-evaluation trace
> for that reaction and see exactly which guard blocked it.

这正是 `sts` 的定位：整条链路（含回滚现场 + 反应守卫诊断）都能在 REPL 里观察到，无需写 Rust。
