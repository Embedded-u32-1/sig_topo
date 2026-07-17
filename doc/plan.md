# 推进计划

本文件记录「继续踏着想法路线前进」的当前判断、路线与里程碑。作为进度记录持续更新，并随推进提交。

## 当前阶段

项目：`sig_topo` —— 文件驱动的 Rust 状态机引擎（JSON 拓扑 → 解析 → 状态流转 → 动作执行 → 可视化/持久化/追踪），按里程碑演进。
当前阶段：**v0.12 M31 ✅（C-ABI 共享库，commit 9f54f80，150 测试绿）；下一步 M32（DDL 高阶语义 reaction guard）。**

最近完成的工作（M31）：

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
- 当前：M31 ✅；下一步委托 M32（DDL 高阶语义 reaction guard）。

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

M31 收口后的调整：C-ABI 已落地；M32（DDL reaction guard）+ M33（场景库）顺次推进。

## v0.12 下一段：多语言绑定（C-ABI）+ DDL 高阶语义 + 场景库

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

### M32：DDL 高阶语义（reaction guard）—— 下一步

- 引擎层：`ReactionDef` 新增 `guard: Option<String>`；`send_event_internal` 派发 reaction 前求值 guard。
- DDL：`reaction { when <sig> enters <state> [when <guard>] -> <tgt_sig> <event> }` 的守卫不再报错，编译到 `ReactionDef.guard`。
- 测试：reaction guard 通过/阻断；级联端到端。
- 文档：`doc/ddl.md` + `doc/cascades.md` 补节。

### M33：示例/场景库（回归 + 教学）

- `examples/scenarios/`：订单审批 / 门控 / 任务调度 / 故障保护，每场景含 `.ddl` + `.scenario.json` + EXPECTED.md。
- `tests/scenarios_test.rs`：自动遍历 `examples/scenarios/`，每场景跑一遍、断言终态与关键 trace 节点。
- 文档：`doc/scenarios.md` 场景索引 + 业务说明。
