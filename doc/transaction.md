# 事务语义（v0.8，M19 — 延迟状态提交）

状态机引擎在 v0.8 引入了**单信号事务回滚**：一个转移的全部生命周期动作被当作一个原子单元。任一动作失败，`signal.current` 恢复到源状态，引擎返回错误，状态不发生跃迁。

## 生命周期动作的新执行顺序

`send_event_internal` 现在按以下顺序执行一个转移：

1. 匹配转移 + 求值 `guard`（失败 → 返回，状态不变，同现状）
2. 执行 `on_exit` 动作（失败 → 返回，状态不变，同现状）
3. 保存 `old_state = signal.current`
4. **临时** `signal.current = to_state`（让后续动作读到目标状态）
5. 执行 `on_transition` 动作；任一失败 → `signal.current = old_state`，推 `Rollbacked`，返回错误
6. 执行 `on_enter` 动作；任一失败 → `signal.current = old_state`，推 `Rollbacked`，返回错误
7. **全部成功**后才推 `TraceEvent::StateChanged`

`ActionContext.to_state` 在动作执行前就已被构建为转移目标，因此在不同生命周期阶段都指向目标状态、是稳定的。本改动只推迟状态提交的*副作用*（`signal.current` 提交与 `StateChanged` / `Rollbacked` 记录），不改变 `ctx` 的语义。

## 回滚的可观测性

回滚时只恢复 `signal.current`。动作的外部副作用（打印、IO、写入外部系统）**不可逆**——这是业务动作的固有限制。为便于调试，trace 保留两类事件：

- `ActionFailed`：哪个动作、为何失败（在 `run_action` 内部推入，所有失败都会出现）
- `Rollbacked { signal_id, from, to }`：表示本次回滚。`from` 是曾被临时进入后放弃的目标状态，`to` 是恢复回的源状态，即「从 `from` 回滚到 `to`」。

因此回滚发生后，用户会看到 `ActionFailed` + `Rollbacked`，但**看不到对应 `StateChanged`**。这是判断一次失败是否触发回滚的判据。

## 与级联（reaction）的交互

级联反应在生命周期动作**全部成功并提交 `StateChanged` 之后**才被派发。所以：

- 父转移成功 → 子级联转移各自独立事务。
- 某一层级联失败 → 该层回滚，但**已提交的上层状态不回滚**。

这是「单信号原子」语义。级联场景的全事务语义（上层随下层失败回滚）被有意推迟到 v0.10（路线图 M23），由业务层在 reaction 上用 guard 做补偿，现阶段不做自动回滚。

## 调试指引

1. 状态未变 + 返回 `Err`：看 trace 里最近的 `ActionFailed` 与 `Rollbacked`，定位失败动作与回滚方向。
2. 状态变了一个中间态：在 M19 之前是 bug（状态先提交、动作后执行），M19 后应不再出现。如仍出现，检查逻辑是否绕过了 `send_event_internal`（注意 `reload_topology` 直接设 `current`，与事务语义无关）。
3. 需要观测全链路顺序：`stt <topology.json> <scenario.json>` 会用 `format_trace` 逐条格式化，`Rollbacked` 以 `Rollbacked <signal>: <from> -> <to>` 形式输出。

## 实现边界

- 仅改 `src/engine.rs`（`send_event_internal`）、`src/trace.rs`（新增 `Rollbacked` 变体）、`src/bin/stt.rs`（`format_trace` 新增 arm）。
- `compose` / `schema` / `guard` / `persist` / `export` 逻辑不变。
- `from_json` / `reload_topology` 入口不变。
- `EngineError` 未新增变体——`ActionExecutionError` 已够用。
