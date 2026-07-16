# 推进计划

本文件记录「继续踏着想法路线前进」的当前判断、路线与里程碑。作为进度记录持续更新，并随推进提交。

## 当前阶段

项目：`sig_topo` —— 文件驱动的 Rust 状态机引擎（JSON 拓扑 → 解析 → 状态流转 → 动作执行 → 可视化/持久化/追踪），按里程碑演进。
当前阶段：**v0.9 M21 已收口（commit 3392cc8），进入 M22（示例场景 + `doc/shell.md`）**。

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

### M22：示例场景 + `doc/shell.md`

- [ ] `examples/` 新增演示场景（订单审批、门控流程），可直接被 `sts` 加载跑通。
- [ ] `doc/shell.md`：安装 / 命令列表 / 调试流程 / 与 `stt` 的区别 / 逐步演示。
- [ ] `README.md` 补一段"交互式模拟"指引指向 `sts`。

### M23：v0.10 级联失败语义文档化 + 测试

- 明确：主转移成功 + reaction 触发 → 某一级 cascade 失败 → 已 committed 的上层状态保留。
- 测试 + 文档（`doc/transaction.md` 补充或独立 `doc/cascade-transaction.md`）。

### M24：v0.11 WASM 多语言绑定（低优先级，依赖团队跨平台需求再排期）

- 可行性调研：`cargo build --target wasm32-unknown-unknown` 验证零依赖是否友好。
- `wasm-bindgen` 封装 + 浏览器极简 demo。

## 实现与审核分工

- 具体实现与端到端审核：委托子代理（Agent）。
- 本进程负责：路线判断、计划记录、提交计划文档、按代理反馈把新事实更新到本计划。
