# 推进计划

本文件记录「继续踏着想法路线前进」的当前判断、路线与里程碑。作为进度记录持续更新，并随推进提交。

## 当前阶段

项目：`sig_topo` —— 文件驱动的 Rust 状态机引擎（JSON 拓扑 → 解析 → 状态流转 → 动作执行 → 可视化/持久化/追踪），按里程碑演进。
当前阶段：**v0.9（M21 3392cc8 + M22 f79acdd）与 v0.10 M23（eb3e910）均收口，示例转录已升格为测试，进入空闲决策点**。

最近完成的工作（M21）：

- `Cargo.toml` 注册 `sts` bin；`src/bin/sts.rs`（225 行，含 `event` / `state` / `trace` / `help` / `quit` + print-and-record 动作）；`tests/sts_test.rs`（3 个集成测试：正常跃迁 / 失败回滚 / state-trace 读取路径）。
- 编译验证发现 sts.rs 对 engine API 的调用与真实签名**完全一致**，零修改；引擎零改动、未新增依赖。
- `cargo build --bin sts` / `cargo test`(含新增 3 个，所有 suite 0 failed) / `cargo clippy --all-targets` 全绿；实跑会话覆盖全部命令。
- 提交 hash：`3392cc82a04b484cdffdee497b5a876c2d38e885`。

## 路线判断

按路线图（`doc/roadmap.md`）v0.9 → v0.10 → v0.11 的顺序：

1. **先把 M21 收口（最高 ROI）** —— 代码本体已成型，只是半成品。闭环一个可感知的交互式产品，比跳去新里程碑成本低。
2. **紧接着 M22** —— 示例场景（订单审批、门控流程）配 `sts` 逐步演示 + `doc/shell.md`。它直接依赖 `sts`，顺水推舟。
3. 其后按路线图推进：v0.10 M23（级联失败语义文档化+测试）→ v0.11 M24（WASM 多语言绑定，低优先级、依赖团队跨平台需求再排期）。

选择"M21 收口 → M22"作为当前踏出的两步，而非直接跳到 v0.10。

## 里程碑

### M21：`sts` 收口 ✅（commit 3392cc8）

- [x] 编译验证：sts.rs 对 engine API 的调用与真实签名完全一致，零修改。
- [x] `cargo build --bin sts` / `cargo test` / `cargo clippy --all-targets` 全绿。
- [x] 集成测试 `tests/sts_test.rs`：正常跃迁 / 失败回滚 / state-trace 读取路径。
- [x] `sts` bin 注册 + `src/bin/sts.rs` + 测试一并提交。

观察（留给后续轮次，不阻塞 M21）：

1. sts 的 `main()` 不可测（`process::exit` + stdin 循环）；命令字符串解析（payload 切片、未知命令）未单测——若未来要覆盖，需把 `cmd_event` / `cmd_state` / `cmd_trace` 抽成返回 String/Result 的函数再测。
2. 交互式中"State rolled back to..."打印路径未人肉触发，但底层回滚语义已由 `test_sts_event_command_rolls_back_on_action_failure` 保障。
3. `load_topology_for_run` 仅从 `schema.transitions` 收集动作，reaction 级联目标 transition 的动作属另一路径；对本 fixture 无遗漏，但复杂 cascade 拓扑下可复核一次。偏 M22 示例场景工作。

### M22：示例场景 + `doc/shell.md` ✅（commit f79acdd）

- [x] `examples/order_approval.json` + `.md`：订单审批，5 状态，guard `payload.amount > 0 and <= 100000` + payload，含 reserve_inventory 回滚缝。
- [x] `examples/gate_flow.json` + `.md`：门控流程，3 状态，`*` 通配 reset、emergency guard、多动作 on_transition。
- [x] `doc/shell.md`：安装/命令列表/调试流程/sts vs stt 对比/端到端演示转录。
- [x] README 补"交互式模拟"段 + doc 列表补 Shell 项。
- [x] 字段命名核对沿用 tests/topology.json、tests/cascade_topology.json、doc/guards.md；未发明新字段；未改 src/。

观察（留给后续轮次，不阻塞 M22）：

1. 回滚路径是"静态缝"而非"可现场演示"：sts 自动注册的动作恒 Ok，无法真的触发 Rollbacked trace；要看真回滚需 sts 支持注册失败钩子或 `--fail <action>` 参数——超出 M22 范围。doc/shell.md 与两份 .md 都已如实标注。
2. order_approval / gate_flow 的 EXPECTED 转录目前由文档由人工守住，未升测试；值得在 M23 考虑把它们加进端到端测试（printf event 序列 → 断言 state 字符串）以防文档漂移。
3. gate_flow .md 的 EXPECTED 转录未逐字 diff 终版输出（仅 order 有完整比对贴在报告）；doc/shell.md 演示转录只引用 order。

### M23：v0.10 级联失败语义文档化 + 测试 ✅（commit eb3e910）

- [x] 语义确认：engine 行为符合预期——`signal.current` 在自身生命周期全部成功并写 StateChanged 之后才派发 reaction，子级失败只回滚子级自身，`?` 返回调用方，已 committed 的父/祖先/兄弟信号全部保留。**逐信号原子，跨信号不做分布式回滚**。引擎零改动，未发现 bug。
- [x] 测试 `tests/cascade_transaction_test.rs`：(A) 深链 A→B→C 叶级失败 → A/B 保留、仅 C 回滚；(A补) trace 含 C 的 ActionFailed + Rollbacked，A/B 无 Rollbacked。(B) 分叉 A→B/D 且 D 失败 → 父 A 与先提交兄弟 B 保留，仅 D 回滚。
- [x] 把 M22 示例升格 `tests/sts_scenarios_test.rs`：order_approval（guard 阻断 + 三 hook 有序 executed_actions + 终态 ship）、gate_flow（guard 阻断 + * 通配命中含 closed→closed 自环证明 * 非 no-op）。防文档漂移。
- [x] 文档 `doc/transaction.md` 新增 §「级联失败与已 committed 上层（v0.10，M23）」：语义 + 受测场景 + 边界与已知限制（reaction 短路、已提交半段残留）。

观察（留给后续轮次，不阻塞 M23）：

1. reaction 短路：多个匹配 reaction 按注册顺序派发，首个失败 `?` 跳过后续兄弟；当前是预期行为，注册顺序在部分兄弟可能失败时有意义——业务层应知晓。
2. 全有或全无（跨信号分布式回滚）仍未支持；业务层通过 guard 补偿。这是语义边界而非 bug。
3. M21 reserve_inventory 回滚 seam 在 sts 仍为 stub（恒 Ok），M23 测试用独立 fixture 覆盖同一语义，与 example 路径互不干扰。

### M24：v0.11 WASM 多语言绑定（低优先级，依赖团队跨平台需求再排期）

- 可行性调研：`cargo build --target wasm32-unknown-unknown` 验证零依赖是否友好。
- `wasm-bindgen` 封装 + 浏览器极简 demo。

## 实现与审核分工

- 具体实现与端到端审核：委托子代理（Agent）。
- 本进程负责：路线判断、计划记录、提交计划文档、按代理反馈把新事实更新到本计划。
