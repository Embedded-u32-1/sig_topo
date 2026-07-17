# 推进计划

本文件记录「继续踏着想法路线前进」的当前判断、路线与里程碑。作为进度记录持续更新，并随推进提交。

## 当前阶段

项目：`sig_topo` —— 文件驱动的 Rust 状态机引擎（JSON 拓扑 → 解析 → 状态流转 → 动作执行 → 可视化/持久化/追踪），按里程碑演进。
当前阶段：**v0.14 全收口（M38–M40 ✅，195 测试绿，0.4.0）。空闲决策点后，经判断启动 v0.15 新方向：guard 可观测性与调试。M41 起由 agent 逐步实现。**

最近完成的工作（M33）：

- `Cargo.toml` 注册 `sts` bin；`src/bin/sts.rs`（225 行，含 `event` / `state` / `trace` / `help` / `quit` + print-and-record 动作）；`tests/sts_test.rs`（3 个集成测试：正常跃迁 / 失败回滚 / state-trace 读取路径）。
- 编译验证发现 sts.rs 对 engine API 的调用与真实签名**完全一致**，零修改；引擎零改动、未新增依赖。
- `cargo build --bin sts` / `cargo test`(含新增 3 个，所有 suite 0 failed) / `cargo clippy --all-targets` 全绿；实跑会话覆盖全部命令。
- 提交 hash：`3392cc82a04b484cdffdee497b5a876c2d38e885`。

## 路线判断

### 已完成段（v0.1–v0.10 + M25–M27）

按路线图（`doc/roadmap.md`）v0.9 → v0.10 → v0.11 的顺序推进：M21 收口 → M22 示例场景 → M23 级联失败语义 → M25–M27 自定圆整。全部完成。

### 下一段（v0.11 起）：领域描述语言

**判断**：愿景起点文档的核心洞察是「描述文件与业务逻辑分离」。v0.1–v0.10 把"底层引擎"跑通，但"描述文件"仍是面向实现的 JSON（直接暴露 from/event/to/动作 id 等引擎原语），不是面向领域的。**兑现愿景的最后一层，是让"描述"本身升级成领域语言**——用户用业务语义写，编译到 JSON/引擎。

**三条候选**：

| 方向 | 是什么 | 价值 | 代价 |
|------|--------|------|------|
| **A：领域描述语言（DDL）** | 新增受控语法，用业务语义（states / on / go / when / then）描述，编译到 TopologySchema | 兑现愿景最后一层——描述真正可读、可写、面向领域 | 中：设计小语言 + 编译器 + 示例 |
| B：运行时可观测性增强 | DOT 运行时状态高亮（M2 未做）、JSON trace 导出、`snapshot_dot()` | 调试更直观 | 小-中 |
| C：项目成熟度 | public API 加文档注释 + 示例；版本 bump；发布准备 | 收拾体面 | 小 |

**采纳 A 优先**，并把 B 的著名未做项（snapshot_dot）打包进去，C 作为收尾短期 milestone。

**理由**：A 是唯一能让项目从"底层引擎"质变为"可用工具"的一步。愿景起点文档开场白就是"描述语言怎么设计？"——用 JSON 跑通整条链后回过头回答这个问题，是闭环。Snapshot_dot 是 DDL 的天然护航。C 是收尾体力活。

**节奏**：M28 设计+实现 DDL 编译器（含集成测试 + 示例 + doc）→ M29 snapshot_dot 运行时高亮 → M30 项目成熟度收尾（doc-comments + version bump）。

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

### M25：sts 圆整——回滚现场演示 + 命令解析可测化（自定，本轮）

想法起点三大后续事项（事务回滚 ✅、模拟调试 ✅、多语言绑定 M24 低优先级）外，sts 这个「交互式模拟器」还有两处没圆：回滚是「静态缝」而非「可现场演示」、命令解析不可测。本里程碑把 sts 真正补成「无需写 Rust 即可观察整条链路（含回滚）」的工具；全程不改 engine。

- [x] 回滚现场演示：`StsSession` 持 `Rc<RefCell<HashSet<String>>> fail_set`；动作闭包命中集合时返 `ActionExecutionError`，引擎既有 M19 回滚路径点火。`fail <action_id>` 插入（持续失败直到 `reset`）；`reset` 清空。引擎零改动。
- [x] 命令解析可测化：抽 `parse_command(line) -> Result<Command, ParseError>` + `Command` enum（Event/State/Trace/Help/Quit/Fail/Reset/Unknown）；主循环只做 IO + 派发。
- [x] 测试：sts.rs 底部 9 个单元测试（解析路由 + session fail/reset 与闭包共享状态）；tests/sts_fail_command_test.rs 2 个集成测试（fail→可观测回滚；sticky 直到 reset）。总测试 100 个全绿。
- [x] doc/shell.md 新增「在 sts 里现场演示回滚」章节（含实跑 order_approval 的可照抄转录）+ 命令列表补 fail/reset。
- [x] 提交 `b6aac90`：doc/shell.md +79 / src/bin/sts.rs +369/−36 / tests/sts_fail_command_test.rs +204。

观察（留给后续轮次，不阻塞 M25）：

1. `order_approval.json` 的 `approve` 只能从 `submitted` 触发，无法在同一信号上「成功→fail→再 approve」原样复现；转录据实写成「submit→fail→approve 回滚→reset→approve 成功」。若要更顺手的教学流，可给示例拓扑补一个逆向转移（示例改动，不影响引擎/测试）。
2. fail_set 是进程内 Rc<RefCell>；未来若要在 stt scenario 层声明「某步某动作失败」，可把它抽到 schema fixture 层——超出 M25，且触及测试/engine 边界，先不动。

### M26：stt 失败注入 + sts/stt 共享 helpers 抽出 lib（自定，本轮）

想法起点"模拟调试"要完整：sts(交互式) 现已能现场演示回滚(M25)，但 stt(scenario 回放) 还不行——其 `load_topology_for_run` 注册 no-op 处理器，scenario 回放永远成功、无回滚可观察。同时 sts/stt 各有一份 `load_topology_for_run` + `format_trace`（重复）。

- [x] 共享 helpers 抽出：新建 `src/run.rs`（pub mod run），抽出 `Scenario`/`ScenarioEvent`(含可选 `fail_actions`)、`collect_action_ids`、`register_actions(engine, ids, fail_set, record)`、`load_topology_for_run(path, fail_set, record)`、`run_scenario(engine, scenario, fail_set)`、`format_trace`。sts.rs 净删 113 行、stt.rs 净删约 120 行重复。
- [x] stt 失败注入：scenario 每个事件可选 `fail_actions: Vec<String>`（#[serde(default)]，零破坏）；per-event 作用域（事件前注入 fail_set、事件后清除）；失败语义改为「记录+继续」(stderr 报告回滚态 + 继续下一事件)，与 sts "回滚+等下一命令" 一致。复用 M25 同套 fail_set 机制。引擎零改动。
- [x] 测试：src/run.rs 单元测试 3 个（去重/format/空集）；tests/stt_fail_scenario_test.rs 集成测试 3 个（成功回归 / 注入失败回滚并继续 / per-event 作用域）；fixture scenario 2 个（scenario_success.json / scenario_fail_inject.json）。总测试从 94 → 100，全绿。
- [x] doc：doc/shell.md 新增「stt scenario format + failure injection」节。
- [x] 提交 `ed16f15`：8 files / +578 / −223。

观察（留给后续轮次，不阻塞 M26）：

1. run_scenario 失败报告委托给调用方（stt 打 stderr），无返回码/非零退出。若 stt 被 CI 用作回归工具，可考虑"有任何注入失败时 exit(1)"——但 guard-blocked 等预期失败不应算，语义需再定义。
2. fail_actions 若列出不存在的 action id 会报 ActionNotFound 而非 ActionExecutionError，行为不同；可在 stt 加 fixture 校验（防 typo），本轮未做。
3. src/run.rs 是 pub mod 暴露为库表面，文档注释已说明是 bin 共享脚手架非稳定 API；未来可收敛为 pub(crate)。
4. 实跑中发现并修复 bug：错误态应在该事件发生时立即捕获存入 ScenarioError，否则会被后续事件覆盖。

### M27：收口 stp 接入 run:: + 修 README/run 文档（自定，本轮）

健康扫描发现 M26 收口后的"最后一公里"：stp 是唯一仍自持重复加载逻辑的 bin（本地 `load_topology_for_run` 108 行 + `Scenario` 不含 `fail_actions`），且 README 写「71 tests」实际 100、`run` 模块与 `transaction.md` 链接缺失。

- [x] stp.rs 切到 run::：删出自持 Scenario/ScenarioEvent/load_topology_for_run（130→96 行，净减 34 行）；用 run::load_topology_for_run + run_scenario；save 子命令回放改用 run::run_scenario（record=false，错误报告到 stderr 但不中断）。
- [x] lib.rs 给 `pub mod run;` 加 doc comment（bin 共享脚手架、非稳定 API）。
- [x] README：测试数 71 → 100；Modules 表补 run 行；doc 导航补 transaction.md 与 doc/run.md 链接。
- [x] 验证：四 bin 全绿；cargo test 100 passed；clippy 零警告；stp save 与 stp reload 实跑通过（order_approval → shipped，reload 后保持 shipped）。
- [x] doc/run.md 新建（50 行）：exports 表 + fail_actions 语义 + 与 sts/stt/stp 关系。
- [x] 提交 `2efeff2`：4 files / +78 / −53。

观察（留给后续轮次，不阻塞 M27）：

1. stp 的 fail_actions 是新增额外能力，但 stp 回放后不打印 trace（只 save_state）；要看注入失败的具体 action/rollback 行需额外调 format_trace——当前未做。
2. 健康扫描候选 B（补 engine/schema/error 单元测试 + 错误路径 fixture）与候选 C（版本 bump 0.1.0 → 0.2.0 / 发布准备）仍未做，可作为下轮方向。

### M28：v0.11 领域描述语言（DDL）编译器 ✅（commit c67429b）

**目标**：让"描述文件"从面向实现的 JSON 升级为面向领域的小语言；用户用业务语义写 `.ddl`，编译到 `TopologySchema`，喂给既有引擎/工具链。引擎零改动。**已完成。**

**语言设计（初版）**——面向业务语义、受控、左到右单次扫描：

```ddl
// 单信号
signal order {
    states: [draft, submitted, approved, rejected, shipped]
    initial: draft

    on submit from draft -> submitted {
        on_exit: log_draft_exit
        on_transition: validate_order_payload
        on_enter: notify_submitted
    }

    on approve from submitted -> approved
        when payload.amount > 0 and payload.amount <= 100000 {
        on_transition: reserve_inventory
        on_enter: notify_customer_approved
    }
}

// 跨信号级联（reaction）
reaction {
    when order enters approved -> order_fulfill begin {}
}
```

关键语法点：
- `signal <id> { ... }`：声明信号 + 状态集 + 初始态。
- `on <event> from <src> -> <tgt> [when <guard>] { <lifecycle> }`：一条转换（守卫可选）。
- `reaction { when <sig> enters <state> -> <tgt_sig> <event> [when <guard>] }`：跨信号派生事件（映射到 ReactionDef）。
- 注释 `//` 到行尾。
- 守卫表达式复用既有 guard 语法（`payload.x > 0 and ...`）。

**实现位点**：
- 新增 `src/ddl/` 模块（`lexer.rs` / `parser.rs` / `codegen.rs` / `mod.rs`）：
  - `lexer.rs`：词法（IDENT / 关键字 / `[` `]` `{` `}` `->` `:` `,` / 字符串 / 数字 / 注释）。
  - `parser.rs`：递归下降 → `DdlDoc` AST（`SignalDecl` / `TransDecl` / `ReactionDecl`）。
  - `codegen.rs`：`DdlDoc -> TopologySchema`（复用既有 `schema.rs` 类型）。
  - `mod.rs`：`pub fn compile(src: &str) -> Result<TopologySchema, EngineError>`。
- 新增 `src/bin/stc.rs`（signal-topology-compiler CLI）：`stc <input.ddl> [output.json]`，读 ddl → 写扁平 JSON。
- `Cargo.toml` 注册 `stc` bin。
- 示例：`examples/order_approval.ddl`（与已有 `order_approval.json` 同语义，对照验证）。
- 文档：`doc/ddl.md`（语法参考 + 编译到 JSON 的映射表 + 示例）。

**测试**：
- `src/ddl/` 内单元测试：lexer 关键字/边界注释；parser 合法/非法（缺 `->`、未知关键字、重复信号）；codegen 映射正确（on_exit/on_transition/on_enter 顺序、guard 透传）。
- `tests/ddl_test.rs` 集成测试：`order_approval.ddl` 编译后喂 `TopologyEngine`，跑 `submit/approve/ship` 终态 `shipped`（与既有 fixture 对拍）；guard 阻断；reaction 级联端到端。
- 现有 100 测试不回归。

**验收标准**：
1. `cargo run --bin stc -- examples/order_approval.ddl /tmp/oa.json` 生成等价于 `order_approval.json` 的 JSON（语义对拍：同事件序列同终态）。
2. 编译产物喂 `TopologyEngine` 端到端跑通既有场景。
3. 语法错误给出带行列号的可定位错误（不 panic）。
4. 现有 100 测试零回归。
5. `doc/ddl.md` 覆盖全部语法。

### M29：运行时状态高亮 DOT（snapshot_dot，路线图 M2 未做项）✅（commit 1280a72）

**目标**：`stv` 当前只能画拓扑骨架；补一个"当前各信号处在哪"的运行时视图。

- `export/dot.rs` 新增 `to_dot_with_state(schema, &HashMap<String,String>) -> String`：当前状态节点高亮（如填充绿色/加粗边框）。
- `TopologyEngine` 新增 `pub fn snapshot_dot(&self) -> String`（复用 `to_dot_with_state`）。
- `stv` 新增用法：`stv --live <topology> <state.json>` 叠加状态；或 `sts` 内命令 `dot` 直接打印。
- 测试：`to_dot_with_state` 初始态 vs 非初始态节点属性不同；`engine.snapshot_dot()` 包含当前态高亮。
- 文档：`doc/visualization.md` 补"运行时高亮"节。

### M30：项目成熟度收尾（doc-comments + version bump）✅（commit b15bcfb）

**目标**：把 already-complete 的工程收拾体面。

- `lib.rs` 及各 pub 模块加 `///` 文档注释 + 示例（cargo doc 可读）。
- `Cargo.toml` version `0.1.0` → `0.2.0`（语义：从 MVP 到完整引擎 + DDL）。
- `README.md` 补"领域描述语言"段 + 五 CLI 表（stv/stt/stp/sts/stc）。
- `cargo doc` 无 warning；`cargo test` 全绿；`cargo clippy --all-targets` 零警告。

### M24：v0.11 WASM 多语言绑定（低优先级，依赖团队跨平台需求再排期，暂不排）

- 可行性调研：`cargo build --target wasm32-unknown-unknown` 验证零依赖是否友好。
- `wasm-bindgen` 封装 + 浏览器极简 demo。

## 实现与审核分工

- 具体实现与端到端审核：委托子代理（Agent）。
- 本进程负责：路线判断、计划记录、提交计划文档、按代理反馈把新事实更新到本计划。
- 当前：M38 ✅ M39 ✅；下一步委托 M40（收口：version 0.4.0 + doc-comments 复核）。

### M28 收口观察（留给后续轮次，不阻塞 M28）

1. reaction 守卫当前不被引擎支持，DDL 编译器已明确报错并指引替代方案（transition guard）。若未来要支持，需扩展 `ReactionDef` 加 guard 字段 + 级联匹配求值——引擎层改动，超出 M28 范围。
2. 动作块 `{ }` 可选（agent 偏离决策），无动作的转换无需空 `{}`，合理降噪。
3. AST 类型本地化（parser 自有 DdlDoc 等），与 serde 解耦；codegen 做 1:1 映射。
4. `TopologySchema` 仅 Deserialize，故 stc.rs 手写字段→serde_json::Value 映射。

### M30 收口观察（留给后续轮次，不阻塞 M30）

1. lib.rs doctest 使用 `tests/topology.json` 端到端演示，是该 crate 第一个 doctest；若未来 fixture 路径/信号名变化需同步，注意"文档即测试"的双面性。
2. README 测试数现在反映真实总数（含 doctest，138）；每次新增 doctest 后记得同步，避免漂移。
3. agent 初写 README 测试数为 137（pre-M30 基线）；我据实修正为 138。

## v0.11 完成后的下一步方向判断

v0.11（M28 DDL + M29 snapshot_dot + M30 成熟度）全收口。项目现状：五 CLI（stv/stt/stp/sts/stc）、138 测试、engine/guard/compose/persist/trace/ddl/export 全模块 doc-commented、version 0.2.0。

候选后续方向（暂不排期，待下次"踏着想法路线"指令时采纳或另指）：

| 方向 | 说明 |
|------|------|
| D：多语言绑定 | 原路线图 M24（WASM/C-ABI），让非 Rust 业务代码驱动引擎。依赖跨平台需求。 |
| E：DDL 生态扩展 | LSP/高亮、`stc --watch` 监视重编、DDL → 多目标（JSON / DOT / 文档）一行出。 |
| F：运行时增强 | reaction 守卫（ReactionDef 加 guard 字段，DDL 编译器已预留）、跨信号分布式事务。 |
| G：示例/场景库 | 把 DDL 示例扩成"场景即测试"的回归库（订单审批 / 门控 / 任务调度 / 故障保护）。 |

M31 收口后的调整：C-ABI 已落地；M32（DDL reaction guard）+ M33（场景库）顺次推进。**v0.12 已全收口。**

## v0.12 回顾：多语言绑定（C-ABI）+ DDL 高阶语义 + 场景库

### 路线判断

愿景起点"描述文件与业务逻辑分离"的下一公里生态是**让非 Rust 也能驱动引擎**。候选 D/E/F/G 中，**D 是唯一能把项目从"Rust 库"升格为"可被任何语言消费的运行时"的一步**。采用 **C-ABI 优先于 WASM**：纯 `extern "C"` + 头文件，保持零新增依赖（wasm-bindgen 重且需 JS 胶水，违背克制原则）。

**节奏**：M31 C-ABI 共享库（engine FFI + 头文件 + 跨语言 demo）→ M32 DDL 高阶语义（reaction guard + enter/leave 事件）→ M33 示例/场景库。

### M31：v0.12 C-ABI 共享库 ✅（commit 9f54f80）

**目标**：让 C / C++ / Python / Node（via FFI）都能加载 .JSON 拓扑、投递事件、读状态/trace。lib 本身零依赖暴露；跨语言 demo 用系统自带工具（python3 ctypes / gcc）验证。

**实现位点**：
- 新增 `src/ffi.rs`：`#[no_mangle] pub extern "C"` 函数：
  - `engine_new(topology_json: *const c_char) -> *mut TopologyEngine`
  - `engine_send_event(engine: *mut TopologyEngine, event_json: *const c_char) -> *mut c_char`（事件 JSON `{signal_id,event,payload}` → 结果 JSON；调用方用 `engine_free_str` 释放）
  - `engine_get_state(engine: *mut TopologyEngine, signal_id: *const c_char) -> *mut c_char`
  - `engine_get_traces(engine: *mut TopologyEngine) -> *mut c_char`
  - `engine_free(engine: *mut TopologyEngine)` / `engine_free_str(s: *mut c_char)`
  - 所有函数内部 `unsafe` 解指针；返回 JSON 用 `CString::into_raw` 泄漏。
- 新增 `include/signal_topology.h`：手写 C 头文件（函数声明 + 使用说明 + 内存规则）。
- `Cargo.toml`：`[lib]` 加 `crate-type = ["cdylib", "staticlib"]`（保留 rlib）。
- `examples/ffi/`：
  - `test.c`：加载 `.so`，跑 order_approval → 断言终态 shipped。
  - `test.py`：python3 ctypes 加载 `.so`，跑同一场景。
  - `run.sh`：一键编译 + 跑 C demo + 跑 Python demo。
- 文档：`doc/ffi.md`（函数签名、内存所有权规则、跨语言 demo 步骤）。

**测试**：
- `tests/ffi_test.rs`：Rust 侧调 FFI（engine_new/send_event/get_state/free），跑 order_approval → 终态 shipped。
- C demo 与 Python demo 实跑验证（写 `examples/ffi/run.sh`）。
- 现有 138 测试零回归。

**验收标准**：
1. `cargo build` 生成 `.so`/`.a`（`target/debug/libsignal_topology.so` 存在）。
2. `cargo test` 全绿（FFI 端到端 + 138 旧测试零回归）。
3. `cargo clippy --all-targets` 零警告。
4. `examples/ffi/test.c` 编译运行后断言终态 shipped（实跑验证）。
5. `examples/ffi/test.py` 运行后断言终态 shipped（实跑验证）。
6. `doc/ffi.md` 覆盖函数签名 + 内存所有权规则 + demo 步骤。

### M32：DDL 高阶语义（reaction guard）✅（commit a7cd356）

- 引擎层：`ReactionDef` 新增 `guard: Option<String>`；`send_event_internal` 派发 reaction 前求值 guard。
- DDL：`reaction { when <sig> enters <state> [when <guard>] -> <tgt_sig> <event> }` 的守卫不再报错，编译到 `ReactionDef.guard`。
- 测试：reaction guard 通过/阻断；级联端到端。
- 文档：`doc/ddl.md` + `doc/cascades.md` 补节。

### M33：示例/场景库（回归 + 教学）✅（commit 274f04d）

- `examples/scenarios/`：订单审批 / 门控 / 任务调度 / 故障保护，每场景含 `.ddl` + `.scenario.json` + EXPECTED.md。
- `tests/scenarios_test.rs`：自动遍历 `examples/scenarios/`，每场景跑一遍、断言终态与关键 trace 节点。
- 文档：`doc/scenarios.md` 场景索引 + 业务说明。

### M33 收口观察（留给后续轮次，不阻塞 M33）

1. **发现 DDL 编译器预存限制**：`from *` 通配符在 DDL 中不能直写（词法把 `*` 分成 `Mul` token，parser 的 `from != "*"` 检查实际是死代码）。当前 workaround：展开为逐源态转移（gate_flow 含 closed→closed 自环）。
2. **DDL 每 lifecycle hook 只支持一个动作**（不支持逗号分隔多动作）。已记入 `doc/scenarios.md` 限制节。
3. scenarios_test 自动发现机制：新增场景零代码改动。

## v0.12 完成后的下一步方向判断

v0.12（M31 C-ABI + M32 reaction guard + M33 场景库）全收口。项目现状：五 CLI + C-ABI .so/.a、160 测试、DDL 编译器 + reaction guard、snapshot_dot、场景库自动回归、version 0.2.0。

候选后续方向（暂不排期，待下一步指令）：

| 方向 | 说明 |
|------|------|
| H：WASM 绑定 | 原 M24，浏览器/Node 驱动 engine（C-ABI 已打通，WASM 是延伸）。 |
| I：DDL 表达力补全 | 直写 `from *` 通配、逗号分隔多动作 hook、reaction 静态 payload 模板。 |
| J：生态工具 | `stc --watch` 监视重编、DDL 多目标一行出（JSON/DOT/文档）。 |
| K：分布式事务 | 跨信号全有或全无、reaction 补偿。 |

本次不自动推进；等待下一步指令（已采纳 I 优先 → v0.13，见下）。

## v0.13 下一段：DDL 表达力补全 + WASM + 校验工具

### 路线判断

M33 发现两个具体 DDL 编译器限制——（1）`from *` 通配符不能直写（parser `from != "*"` check 是死代码 + lexer 把 `*` 分成 Mul token）；（2）每 lifecycle hook 只支持一个动作。它们是"描述文件与业务逻辑分离"愿景的绊脚石——用户遇到限制必须退回 engine-thinking 手动展开。候选 H/I/J/K 中，**I（DDL 表达力补全）是唯一让 DDL 从"可用的 workaround"升格为"顺手的领域语言"的一步**。

**节奏**：M34 通配 `from *` 展开 + 多动作 hook + reaction 静态 payload → M35 WASM 绑定 → M36 DDL 校验工具（`stc --check`）→ M37 收口。

### M34：v0.13 DDL 表达力补全 ✅（commit 097d12b）

**目标**：让 DDL 通顺表达当前不得不用 workaround 的语义。

**具体改进**：

**A. `from *` 通配符直写（parser + codegen）**
- parser：`parse_transition` 的 `from` 位置允许 `Mul` token（`*`），捕获为 wildcard。
- codegen：`from = *` 时展开为"从每个状态出一条到 `to` 的转移"（含 `to→to` 自环）。参考 `export/dot.rs` 对 `*` 的处理。
- 结果：`gate_flow.ddl` 的 3 条 reset 展开可简化回 `on reset from * -> closed`。

**B. 多动作 hook（parser + codegen）**
- parser：`on_exit` / `on_transition` / `on_enter` 允许逗号分隔多 action id。
- codegen：透传到 `Vec<String>`。
- 结果：`gate_flow.ddl` 的多动作 on_transition 可直写。

**C. reaction 静态 payload（parser + codegen）**
- parser：`reaction { when <sig> enters <state> [when <guard>] -> <tgt_sig> <event> [with { ... }] }` 可选 `with { JSON }` 静态 payload。
- codegen：透传到 `ReactionDef.payload`。
- 结果：reaction guard 可基于自身静态 payload 求值（结合 M32 的 parent_payload）。

**实现位点**：
- `src/ddl/parser.rs`：修 `parse_transition`（Mul wildcard）、`parse_actions`（逗号分隔多 action）、`parse_reaction`（可选 with payload）。
- `src/ddl/codegen.rs`：`emit_transition` 处理 wildcard 展开、`emit_reaction` 透传 payload。
- `src/ddl/lexer.rs`：确认 Mul token 在 from 位置能被识别（byte_pos）。
- 示例更新：`examples/scenarios/gate_flow/gate_flow.ddl` 用直写 + 多动作简化；`tests/fixtures/reaction_guard.ddl` 加 `with` payload 用例。
- 文档：`doc/ddl.md` 更新语法参考。

**测试**：
- `src/ddl/` 单测：wildcard 展开条数正确（N 状态 → N 转移含自环）；多动作 hook 解析顺序正确；reaction payload 透传。
- `tests/ddl_test.rs`：gate_flow 直写 `from *` 后端到端跑通 + 自环证明；多 action 按序 executed_actions；reaction with-payload guard 求值基于 payload。
- 现有 160 测试零回归 + clippy 零警告。

**验收标准**：
1. `on reset from * -> closed` 直编译通过，行为等价于逐态展开。
2. 多动作 hook executed_actions 顺序与写序一致。
3. reaction `with { "x": 1 }` 静态 payload 出现在 ReactionDef.payload。
4. 160 测试零回归 + clippy 零警告。
5. `doc/ddl.md` 更新覆盖新语法。

### M34 收口观察（留给后续轮次，不阻塞 M34）

1. **顺手修了 DDL lexer 双引号字符串预存缺陷**：原 lexer 只处理单引号 `'...'`，JSON payload 必须用双引号，会触发 `unexpected character '"'` 错误。修复后两引号共用 `TokenKind::String`。
2. **wildcard AST 选 `from: String = "*"`**（推荐方案），与 schema/引擎/JSON 完全对齐，无需改 schema.rs。
3. **reaction payload 选 raw-text 方案**（parser 存原始文本，codegen 调 serde_json::from_str），编译期可发现非法 JSON。
4. gate_flow.ddl 的 3 条 reset 简化为 1 条 `on reset from * -> closed` + 多动作 fault hook；scenarios_test 自动回归通过。

### M35：WASM 绑定 ✅（commit 939a9a0）

- `wasm-bindgen` 封装 `TopologyEngine`（本里程碑唯一新增依赖）。
- 浏览器极简 demo：`<textarea>` 编辑 DDL + 跑 `sts` 式交互。
- 前置调研：`cargo build --target wasm32-unknown-unknown` 验证。
- 验收：浏览器/Node 加载 order_approval → 终态 shipped。

### M36：DDL 校验工具（`stc --check`）✅（commit 2258dd6）

- `stc --check <file.ddl>`：编译 + warn on suspicious patterns（* 导致自环、从未触发的状态、无入边终态）。
- `stc` 默认仍编译；`--check` 额外输出 warning。
- 验收：gate_flow.ddl 上 `--check` 报告自环 warning。

### M37：收口 ✅（v0.13 全收口）

- 全模块 doc-comments 复核（M34 触及的 parser/codegen）：补 `parser.rs` AST 类型（`DdlDoc`/`SignalDecl`/`TransDecl`/`DdlActionBinding`/`ReactionDecl` 及其 pub 字段）+ `lexer.rs` `Token` 结构体及其 pub 字段；其余模块（check/engine/schema/lib/stc）已全覆盖，零补充。
- version 0.2.0 → 0.3.0（主 crate + wasm-topology 子 crate）。
- README / roadmap / plan 同步（测试数 138 → 177；补 `check` Modules 行 + WASM demo 段 + WASM/Check doc 导航；roadmap 收口 v0.12 + v0.13 并指向 I2/J/K 候选）。
- `cargo test` 177 全绿 + `cargo clippy --all-targets` 零警告 + `RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps` 零 warning。

## v0.14 下一段：reaction guard 跨信号协调语义深化

### 路线判断

**判断**：愿景"描述文件与业务逻辑分离"的下一层是**让"描述"真正能表达跨信号业务规则**。M32 已让 reaction guard 能基于 payload 求值，但 guard 是单条静态条件。在 H/I/J/K 候选中，**K（reaction 跨信号协调语义深化）是唯一让引擎从"信号各自为政"升格为"信号间能表达业务协调"的一步**——这是"描述文件"兑现"业务协调"的关键。

**节奏**：M38 reaction guard 复合语义（guard 模板/重用 + guard 求值 trace + guard 嵌套组合）→ M39 guard demo 场景 + stc --check 增强（guard-pattern lint）→ M40 收口。

### M38：v0.14 reaction guard 复合语义 ✅（commit 99d2ce1）

**目标**：让 reaction guard 表达更丰富的业务协调逻辑，并让 guard 求值过程可观测。

**具体改进**：

**A. guard 模板 / 重用（parser + codegen）**
- 当前 `reaction { ... when <expr> }` 的 expr 每次都要完整写，跨反应无法复用。
- 新增顶级 `guard <id> { <expr> }` 声明（顶层，与 signal/reaction 同级）；reaction 中可用 `when <guard_id>` 引用。
- parser：加 `parse_guard_decl`（顶级 `guard <id> { ... }`）；reaction 的 `when` 后接受 identifier（引用）或字面表达式。
- codegen：顶级 guard 表达式展开到每个引用它的 reaction 中（inline 展开）。
- 结果：多个 reaction 共享同一 guard 条件时只需写一次。

**B. guard 求值 trace（engine）**
- M29/M30 已实现 action/transition trace，但 reaction guard 求值目前"无声无息"（false/error 只 skip、不记录）。
- 在 `send_event_internal` 的 reaction 派发段加 trace 事件：`ReactionGuardEvaluated { signal_id, reaction, guard, result, timestamp_ms }`（result = Ok(true)/Ok(false)/Err）。
- 扩展 `TraceEvent` 枚举（`src/trace.rs`）+ 对应 `format_trace`（`src/run.rs`）+ snapshot 导出。
- 结果：用户能在 trace 里看到"为什么这条 reaction 没触发"。

**C. guard 组合（parser 语法糖）**
- 当前一条 reaction 只支持一个 guard。加 `when <g1> and <g2>` / `when <g1> or <g2>` 语法（最多二层），编译为嵌套求值。
- **简化方案**：让单个 guard 表达式直接复用既有 guard 语言（and/or/not）—— 其实**不加新语法**就能表达组合（如 `payload.x > 0 and payload.y < 100`）。所以 C 实际不需要新 parser 语法，只需让 guard 表达式语言完整支持 and/or/not（检查既有 guard eval 是否已支持）。若已支持则 C 跳过。

**实现位点**：
- `src/ddl/parser.rs`：顶级 `guard <id> { ... }` 声明；reaction `when` 接受 id ref 或字面 expr。
- `src/ddl/codegen.rs`：顶级 guard 展开 + reaction guard ref 解析。
- `src/ddl/mod.rs`：AST 加 `GuardDecl { id, expr }`。
- `src/trace.rs`：加 `ReactionGuardEvaluated` 变体。
- `src/engine.rs`：reaction 派发段 push trace 事件。
- `src/run.rs`：`format_trace` 处理新变体。
- 示例：`examples/scenarios/` 加 1 个 guard-template 场景（多个 reaction 共享 guard）。
- 文档：`doc/ddl.md` + `doc/cascades.md` 补节。

**测试**：
- `src/ddl/` 单测：guard 声明解析、guard ref 解析、guard 展开正确性。
- engine：guard 求值 trace 捕获（true/false/err 三种情况都 trace 到）。
- 端到端：两个 reaction 共享同一 guard id → 两者行为一致。
- 现有 177 测试零回归 + clippy 零警告。

**验收标准**：
1. 顶级 `guard <id> { <expr> }` 声明 + reaction `when <id>` ref 编译通过、行为等价于内联展开。
2. trace 里能看到 `ReactionGuardEvaluated` 事件（含 result）。
3. 现有 177 测试零回归 + clippy 零警告。
4. `doc/ddl.md` + `doc/cascades.md` 补 guard 模板 + guard trace 节。

### M39：guard demo 场景 + stc --check 增强 ✅（commit 6a83d80）

**目标**：把 guard 新语义变成可触摸的教学示例 + 静态检查。

- 新增 `examples/scenarios/guard_coordination/`：双信号协调场景（如"支付成功 → 库存扣减" + guard 模板控制何时扣减）。
- `tests/scenarios_test.rs` 自动发现覆盖新场景。
- `src/check.rs` 加新 lint：`UnusedGuardTemplate`（声明但从未被 reaction ref 的 guard）、`DuplicateGuardCondition`（多个 guard声明相同 expr）。
- 验收：stc --check guard_coordination.ddl 报告预期 lint。

### M40：收口 ✅（v0.14 全收口）

**目标**：把 v0.14 三档加速（M38/M39/M40）收拾体面；全程不改功能代码行为。

**实际完成项**：

- [x] doc-comments 复核：M38/M39 新增代码在 M38/M39 轮次已全覆盖（`trace.rs` `ReactionGuardEvaluated` 变体 + `signal_id()`/`timestamp_ms()` 分支；`engine.rs` reaction 派生段 M38 trace 注释；`check.rs` `UnusedGuardTemplate`/`DuplicateGuardCondition` Display + `check_ddl` pub fn；`ddl/parser.rs` `ReactionDecl.guard_ref` + `parse_guard_spec`/`parse_guard_decl`；`ddl/mod.rs` `compile_full` + `DdlDoc`/`GuardDecl`/`ReactionDecl` re-export；`bin/stc.rs` `--check` 调用注释）。`RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps` 零 warning。
- [x] version 0.3.0 → 0.4.0（主 crate `Cargo.toml` + 子 crate `wasm-topology/Cargo.toml`；跨信号协调是质的飞跃）。
- [x] README 同步：测试数 177 → 195；Modules 表加 `guard templates` 行（顶级 guard 声明 + reaction when ref）+ `check` 行核对存在；doc 导航含 guard template / guard coordination（`doc/ddl.md` 覆盖）。
- [x] roadmap / plan 同步：当前状态指向 v0.14 全收口；下一步候选 L1-L4。
- [x] `cargo test` 195 全绿 + `cargo clippy --all-targets` 零警告 + `RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps` 零 warning。

**观察（留给后续轮次，不阻塞 M40）**：

1. doc-comments 复核发现 M38/M39 代码在当轮已写齐，本轮零补充；doc 零 warning 已于 M37 起持续保持，本轮再确认。
2. `guard templates` 行在 Modules 表中位于 `ddl` 与 `check` 之间，反映它是 DDL 编译器 + check 的交叉关注点。

## v0.14 完成后的下一步方向判断

v0.14（M38 reaction guard 复合语义 + M39 guard demo + stc --check 增强 + M40 收口）全收口。项目现状：六 CLI（stv/stt/stp/sts/stp/stc）+ C-ABI .so/.a + WASM、195 测试、guard 模板/重用 + guard 求值 trace + guard lint、version 0.4.0。

候选后续方向（暂不排期，待下次「踏着想法路线」指令时采纳或另指）：

| 方向 | 说明 |
|------|------|
| L1：guard 可视化 | `snapshot_dot` 标出被 guard 阻断的 reaction（guard=false 的 reaction 用虚线/灰色边），让协调失败在图上一目了然。 |
| L2：guard 调试器 | `sts` 内 `why <reaction>` 命令：打印该 reaction 的 guard 求值 trace（`ReactionGuardEvaluated` 事件），定位「为什么没触发」。 |
| L3：完整工作流引擎 | fork / join、sub-topology 组合；把当前线性级联升格为可表达并行分支与汇流的工作流。 |
| L4：stc LSP | 基于 `check_ddl` + `guard_ref` 的补全/诊断 LSP：guard id 自动补全、未实时报错的 unused/duplicate guard。 |

本次不自动推进；等待下一步指令（已采纳 L1+L2 优先 → v0.15，见下）。

## v0.15 下一段：guard 可观测性与调试

### 路线判断

**判断**：M38 已让 guard 求值有 trace，M29 已有 snapshot_dot 运行时高亮，但两者还没连接——用户看到"某 reaction 被跳过"却不能直观看到"为什么被跳过"。在 L1/L2/L3/L4 候选中，**L1+L2 的组合（guard 可视化 + guard 调试器）是把 guard 系统从"可用"升格为"可调试"的最自然一步**，且都建立在已有基础上（guard trace + snapshot_dot + sts REPL）。

**节奏**：M41 guard 调试器（sts `why` 命令 + guard trace 可视化）→ M42 snapshot_dot 增强（标出 guard-blocked reaction）→ M43 收口。

### M41：v0.15 guard 调试器 —— 设计 + 实现（本轮起）

**目标**：让用户在 sts REPL 内定位「为什么这条 reaction 没触发」。

**具体改进**：

**A. sts `why` 命令（`src/bin/sts.rs`）**
- 新增 `why <from_signal> <from_state> <to_signal> <event>` 命令（或更简洁的 `why <reaction_index>`）。
- 实现：从 engine trace 中过滤出匹配的 `ReactionGuardEvaluated` 事件，打印：反应 guard 表达式、求值结果（true/false/error）、当时的 payload 快照。
- 解析：扩展 `Command` enum 加 `Why { from_signal, from_state, to_signal, event }`；`parse_command` 加 `why` 分支；dispatch 加 `cmd_why`。
- 若无匹配 trace（ reaction 从未被评估），友好提示「该 reaction 尚未被评估过（可能 from_state 未到达）」。

**B. guard trace 彩色/可视化输出（`src/run.rs` format_trace 或独立 helper）**
- 在 `sts` 的 `trace` 命令输出中，`ReactionGuardEvaluated` 事件用颜色标记（true=绿 / false=黄 / err=红）—— 仅 tty 输出时启用（用 `atty` 检测或简单 `\x1b[...m` 转义，无需新依赖）。
- 或在 `why` 命令内直接打印"求值上下文"：guard 表达式 + 当时的 payload JSON + 求值路径。

**测试**：
- `src/bin/sts.rs` 单元测试：parse_command("why ...") 路由正确。
- `tests/sts_test.rs` 或新建 `tests/sts_why_test.rs`：端到端跑含 guard 的场景 → 调 `why` 命令 → 断言输出含 ReactionGuardEvaluated 关键信息（guard 表达式 + result）。
- 现有 195 测试零回归。

**验收标准**：
1. sts `why` 命令能打印指定 reaction 的 guard 求值 trace（表达式 + 结果 + payload）。
2. 195 测试零回归 + clippy 零警告。
3. `doc/shell.md` 补 `why` 命令说明 + 示例转录。

### M42：snapshot_dot 增强（标出 guard-blocked reaction）

**目标**：在 DOT 图中可视化 reaction guard 阻断。

**具体改进**：
- `export/dot.rs` 加 `to_dot_with_guard_info(schema, states, guard_info)` 变体：接受每 reaction 的 guard 求值结果映射，在对应的 reaction 边（若显式绘制 reaction 边）或标注中标记结果。
- 当前 `to_dot` 不绘制反应边（README 注明）；本里程碑可选：(a) 新增 mode 绘制 reaction 边（虚线、带 guard result 颜色）或 (b) 在现有 DOT 输出中加注释/label 标注 guard result。
- 推荐 (a)：新增 `to_dot_extended(schema, states, reactions_info)` 显式绘制 reaction 边（颜色：true=实线绿 / false=虚线灰 / err=虚线红）。
- 在 `TopologyEngine` 新增 `pub fn snapshot_dot_extended(&self) -> String`。
- 在 `sts` 加 `dot-ext` 命令调用；或在 `snapshot_dot` 内自动叠加（若 trace 中存在 guard info）。

**测试**：
- `to_dot_extended` 单元测试：guard=true 边为实线绿、guard=false 边为虚线灰。
- 端到端：跑含 guard 场景 → snapshot_dot_extended → 断言输出含颜色属性。

**验收标准**：
1. `snapshot_dot_extended` 绘制 reaction 边并按 guard 结果着色。
2. 195 测试零回归 + clippy 零警告。

### M43：收口

- 全模块 doc-comments 复核（M41/M42 新增代码）。
- version 0.4.0 → 0.5.0（guard 可观测性 + 调试是 guard 系统的闭环）。
- README / roadmap / plan 同步（加 `why` 命令 + snapshot_dot_extended 段）。
- `cargo test` + `cargo clippy` + `cargo doc` 全绿。
