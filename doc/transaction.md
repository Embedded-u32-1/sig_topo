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

## 级联失败与已 committed 上层（v0.10，M23）

在 M19 的单信号回滚之上，M23 把「派生级联某级失败时，已 committed 上层状态保留」固化为可测试契约。因为 reaction 派发（`src/engine.rs` 的 `send_event_internal` 末尾）发生在父信号 `StateChanged` **之后**，且用 `?` 向上传播错误，所以：

- 父转移先提交自己的 `StateChanged`，再依次触发匹配的 reaction；
- 任一子级联失败时，只回滚**失败的那个信号**（它自己的生命周期动作分支），把错误沿反应链向上递给调用方；
- **已提交的上层（包括父、祖先、以及先完成提交的反应兄弟）一律保留**，不会被失败的后辈拖回。

这就是「逐信号原子」：每一级在自己的生命周期动作边界内原子，跨信号不构成一个大事务。

### 受测场景（`tests/cascade_transaction_test.rs`）

三级链 `A -> B -> C`，`C` 的 `on_enter` 动作返回 `Err`：

1. `A` 跃迁提交 `a0 -> a1`，触发 `B`；
2. `B` 跃迁提交 `b0 -> b1`，触发 `C`；
3. `C` 临时进入 `c1` 后 `on_enter` 失败，回滚到 `c0`，把 `ActionExecutionError` 向上抛；
4. 调用方收到 `Err`；此时 `A == a1`、`B == b1`、`C == c0`。

分支拓扑：父 `A` 分叉到兄弟 `B` 与 `D`，`D` 失败。则 `A` 与**先提交完成的兄弟 `B`** 都保留（`a1`、`b1`），只有 `D` 回滚（`d0`）。这说明 reaction 循环不会因某个兄弟失败而撤销其它已提交的兄弟。

trace 上能观察到 `C`/`D` 的 `ActionFailed` + `Rollbacked`，但 `A`/`B` **没有** `Rollbacked`，可作为该契约的判据（参见「回滚的可观测性」）。

### 边界与已知限制

- 中途失败的级联会把已成功的那半段作为「既有状态」留下来（如上面的 `B`/`D` 兄弟拓扑）。业务层若期望全有或全无，需在父上用 guard 预检，或把补偿动作放进后续转移——引擎本身不做分布式回滚。
- 同一信号上多个匹配 reaction 按注册顺序派发；首个失败的 `?` 会中断剩余 reaction（短路），因此反应注册顺序在「部分兄弟可能失败」的场景下是有意义的。
- 动作的外部副作用（打印、IO、写外部系统）不可逆，即便该信号后来回滚。这与 M19 一致，级联下每层各自承担。

## 调试指引

1. 状态未变 + 返回 `Err`：看 trace 里最近的 `ActionFailed` 与 `Rollbacked`，定位失败动作与回滚方向。
2. 状态变了一个中间态：在 M19 之前是 bug（状态先提交、动作后执行），M19 后应不再出现。如仍出现，检查逻辑是否绕过了 `send_event_internal`（注意 `reload_topology` 直接设 `current`，与事务语义无关）。
3. 需要观测全链路顺序：`stt <topology.json> <scenario.json>` 会用 `format_trace` 逐条格式化，`Rollbacked` 以 `Rollbacked <signal>: <from> -> <to>` 形式输出。

## 实现边界

- 仅改 `src/engine.rs`（`send_event_internal`）、`src/trace.rs`（新增 `Rollbacked` 变体）、`src/bin/stt.rs`（`format_trace` 新增 arm）。
- `compose` / `schema` / `guard` / `persist` / `export` 逻辑不变。
- `from_json` / `reload_topology` 入口不变。
- `EngineError` 未新增变体——`ActionExecutionError` 已够用。
