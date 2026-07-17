# 信号拓扑图发展路线图

## 当前状态

已完成 v0.1 MVP → v0.6，+v0.7 M16（共 16 个里程碑，58 个测试全部通过）：

- **v0.1 MVP**：JSON 拓扑格式、Rust 引擎、静态校验、状态流转、生命周期动作绑定
- **v0.2 可视化**（M1–M3）：DOT 导出、`stv` 查看工具、文档
- **v0.3 可观测性**（M4–M6）：Trace 事件模型、引擎内嵌收集、`stt` 回放工具
- **v0.4 守卫表达式**（M7–M9）：语法设计、词法/解析/求值器、引擎集成
- **v0.5 持久化与热重载**（M10–M12）：状态快照、`save`/`load`、`reload_topology`、`stp` 工具
- **v0.6 级联信号**（M13–M15）：`ReactionDef`、受控级联执行、深度限制
- **v0.7 模块化导入**（M16 ✅、M17 ✅、M18 ✅）：`ComponentDef`/`InstanceDef`、`expand()` 参数化组件展开、`load_topology` 跨文件导入（循环检测 + 合并并集）、三 CLI 统一接入、示例库、`doc/composition.md`、`README.md`、`src/compose.rs`。验收通过（71 测试绿，house.json 端到端级联跑通）。
- **M16 review 的 2 个 Major 已修复**（commit 965684c）：M1 subst 改为从左到右单次扫描（确定性 + 不二次解释）；M2 MissingBinding 透传真实组件名；附 3 个回归测试。

原始 MVP 排除清单中，守卫、级联、持久化、模块化导入均已完成；**事务回滚**：v0.8 M19 ✅（延迟状态提交 + 回滚）、M20 ✅（transaction_test + doc/transaction.md）；**交互式模拟**：v0.9 M21 ✅（sts REPL，commit 3392cc8）、M22 ✅（示例场景 + doc/shell.md，commit f79acdd）；**级联事务扩展**：v0.10 M23 ✅（级联失败语义 + 测试 + doc/transaction.md，commit eb3e910）；**sts 圆整 / stt 失败注入 / run:: 统一**：自定 M25 ✅ M26 ✅ M27 ✅（REPL 现场回滚 + 命令可测化 + stt 失败注入 + 共享 helpers）。**下一步**：v0.11 已全收口（M28 DDL ✅ M29 snapshot_dot ✅ M30 成熟度 ✅，138 测试绿，version 0.2.0）。v0.12 已全收口：**多语言绑定（C-ABI）** M31 ✅（共享库 + 头文件 + Python/C demo）+ **DDL 高阶语义 reaction guard** M32 ✅ + **示例/场景库** M33 ✅（160 测试绿）。v0.13 已全收口：**DDL 表达力补全** M34 ✅（`from *` 通配直写 + 多动作 hook + reaction 静态 payload）+ **WASM 绑定** M35 ✅（`wasm-bindgen` + 浏览器 demo）+ **stc --check 校验工具** M36 ✅，177 测试绿，version 0.3.0。M24 WASM 因 C-ABI 更克制而让位（后改道 M35 落地）。路线图从这里继续。

## 下阶段目标

### 下阶段目标：领域描述语言（v0.11，M28–M30）

愿景起点的核心洞察是「描述文件与业务逻辑分离」。v0.1–v0.10 跑通了底层引擎，但”描述文件”仍是面向实现的 JSON。本阶段让”描述”升级为面向领域的小语言（DDL），并补完路线图遗留的运行时 DOT 高亮、项目成熟度收尾。

#### 里程碑 M28：DDL 编译器 + `stc` CLI

- `signal <id> { states: [...] initial: ... on <event> from <src> -> <tgt> [when <guard>] { ... } }` 语法。
- `reaction { when <sig> enters <state> -> <tgt_sig> <event> [when <guard>] }` 级联。
- 新增 `src/ddl/{lexer,parser,codegen,mod}.rs` + `compile(src) -> Result<TopologySchema, EngineError>`。
- 新增 `src/bin/stc.rs`：`stc <in.ddl> [out.json]`。
- 示例 `examples/order_approval.ddl` + `doc/ddl.md`（语法→JSON 映射表）。
- 测试：lexer/parser/codegen 单测 + `ddl_test.rs` 集成（.ddl → engine 端到端对拍 JSON）。
- 验收：编译产物语义等价于同场景 JSON；错误带行列号；现有 100 测试零回归。

#### 里程碑 M29：运行时 DOT 高亮（路线图 M2 未做项）

- `export/dot.rs` 新增 `to_dot_with_state(schema, states)`。
- `TopologyEngine::snapshot_dot(&self) -> String`。
- `stv --live <topology> <state.json>` 叠加状态 / 或 `sts` 内 `dot` 命令。
- 测试 + `doc/visualization.md` 补节。

#### 里程碑 M30：项目成熟度收尾

- pub API 加 `///` 文档注释 + 示例。
- `Cargo.toml` `0.1.0` → `0.2.0`。
- README 补 DDL 段 + 五 CLI 表。
- `cargo doc` 无 warning / `cargo test` 全绿 / `cargo clippy` 零警告。

### 1. 拓扑图可视化查看（MVP+）

让用户无需阅读 JSON 即可直观看到信号、状态与转移关系。仅做”查看”，不参与运行时逻辑。

方案：将 TopologySchema 导出为 Graphviz DOT 格式。理由：

- 无额外 Rust 依赖，保持项目轻量
- 可直接用系统已安装的 `dot` 生成 SVG/PNG/PDF
- 状态机图天然适合 DOT 描述

#### 里程碑 M1：DOT 导出 + CLI 查看工具

- 新增 `src/export/dot.rs`：提供 `to_dot(schema: &TopologySchema) -> String`
  - 每个信号画成一个子图（cluster），内部节点为状态
  - 转移画成带事件标签的有向边
  - 初始状态用特殊填充色或虚线边框标识
  - 动作绑定以工具提示/边标签形式可选展示（MVP 可简化）
- 新增 `src/bin/stv.rs`（signal-topology-viewer CLI）：
  - 读取 JSON 拓扑文件
  - 输出 `.dot` 文件
  - 若系统有 `dot`，自动调用生成 `.svg`
- 新增示例：`examples/topology.json`（沿用 tests/topology.json）
- 新增单元/集成测试：导出内容包含关键节点、边、标签
- 文档：`doc/visualization.md`，说明如何安装 Graphviz 与查看

#### 里程碑 M2：运行时状态快照导出（可选增强）

- 在 DOT 图中叠加当前状态高亮
- 新增 `Engine::snapshot_dot()` 或类似 API
- 用于运行时调试：一眼看出每个信号当前处在哪个状态

#### 里程碑 M3：文档与示例完善

- README.md 快速开始
- 可视化最佳实践
- 示例 JSON 库（任务状态、订单状态、门控流程）

## 下下阶段目标：动作可观测性（v0.3）

MVP 的动作函数返回 `Result<(), EngineError>`，调用者无法知道动作内部具体发生了什么。为了让拓扑图真正可调试、可追踪，下一步引入动作执行事件流：

方案：在 `TopologyEngine` 内部维护一个只追加的 Trace 日志，记录每一次状态转移、动作开始/成功/失败、事件投递。提供只读导出 API，不影响运行时逻辑。

#### 里程碑 M4：Trace 事件模型

- 新增 `src/trace.rs`：
  - `TraceEvent` 枚举：`EventReceived`、`ActionStarted`、`ActionSucceeded`、`ActionFailed`、`StateChanged`
  - `TraceLog` 结构体：只追加 Vec，支持按信号过滤、按时间范围截取
- 扩展 `EngineError` 保留原始错误信息用于 Trace

#### 里程碑 M5：引擎内嵌 Trace 收集

- `TopologyEngine` 内部持有一个 `TraceLog`
- `send_event` 执行过程中自动追加 Trace 事件
- 新增 API：
  - `engine.traces()` 返回全部 Trace
  - `engine.traces_for(signal_id)` 按信号过滤
  - `engine.clear_traces()` 清空日志
- 动作执行失败时，Trace 中保留错误信息，状态回滚或不回滚按当前行为（不回滚）

#### 里程碑 M6：Trace 可视化增强

- 扩展 DOT 导出，支持按 Trace 高亮“最近走过的边”
- 新增 CLI 子命令或工具，输出 Trace 为文本时间线
- 文档：如何根据 Trace 排查状态流转问题

## 技术边界（保持克制）

- 不引入 Web 前端/编辑器（避免复杂度爆炸）
- 可视化只读，不写回 JSON
- 不改动核心引擎运行时行为
- 优先使用文本格式与系统工具，不绑定特定图像库
- Trace 是运行时调试工具，不做持久化、不做分布式追踪

## 下下下阶段目标：条件转移与守卫表达式（v0.4）

MVP 中转移只按 `(from, event)` 匹配。实际业务中常需要“只有在满足某条件时才允许转移”，例如：余额充足才能扣款、权限校验通过才能进入审批。下一步引入轻量级守卫条件：

方案：在转移规则中增加 `guard` 字段，用一段受限表达式描述条件。引擎在匹配到转移后、执行动作前求值守卫；守卫失败时返回专门的错误，状态不变。

#### 里程碑 M7：守卫表达式语法设计

- 在 `TransitionDef` 中新增可选 `guard: Option<String>`
- 表达式语法保持极简：
  - 支持 `payload.field` 读取事件 payload
  - 支持字面量：整数、浮点、字符串、布尔值
  - 支持比较：`==`, `!=`, `<`, `<=`, `>`, `>=`
  - 支持逻辑组合：`and`, `or`, `not`
  - 支持算术：`+`, `-`, `*`, `/`
- 示例：`"payload.amount > 0 and payload.currency == 'USD'"`

#### 里程碑 M8：守卫求值引擎

- 新增 `src/guard/` 模块：
  - 词法分析器（tokenizer）
  - 递归下降解析器（parser）生成 AST
  - 求值器（evaluator），输入表达式字符串与 `ActionContext`，输出 `bool`
- 错误处理：语法错误返回 `EngineError::GuardEvaluationError`
- 不使用外部表达式库，保持零额外依赖

#### 里程碑 M9：引擎集成与测试

- `send_event` 在匹配转移后、执行 `on_exit` 前求值 guard
- 守卫失败返回 `EngineError::GuardBlocked { signal, event, guard }`，状态不变
- 新增测试覆盖：
  - 守卫通过，正常转移
  - 守卫失败，状态不变，返回 GuardBlocked
  - payload 字段缺失/null 的求值行为
  - 复杂逻辑表达式
- 新增文档 `doc/guards.md`：守卫表达式语法、示例、调试

## 验收标准

1. `cargo run --bin stv -- examples/topology.json` 能生成 `topology.dot`
2. 在有 Graphviz 环境下能自动生成 `topology.svg`
3. 生成的图包含所有信号、所有状态、所有转移
4. 新增测试覆盖 DOT 导出关键内容
5. 现有 5 个集成测试保持通过

## 下下下阶段目标：状态持久化与热重载（v0.5）

随着拓扑变复杂，运行时需要能够在进程重启后恢复状态，并在不重启的情况下加载新的拓扑描述。这一步引入轻量级文件持久化和热重载机制，保持简单、无外部依赖。

方案：

- 状态持久化：引擎将所有信号的当前状态序列化为 JSON 文件，提供 `save_state` / `load_state` API。
- 热重载：在保留当前状态的前提下，加载新的拓扑描述并重新校验；若校验通过则替换 schema 和 transitions，状态保持不变。
- 限制：热重载时如果新拓扑删除某个信号，该信号状态被丢弃；如果新增信号，使用新拓扑的 initial_state。

#### 里程碑 M10：状态快照序列化

- 新增 `src/persist.rs`：
  - `StateSnapshot` 结构体：`HashMap<String, String>` 表示 signal_id -> current_state
  - `TopologyEngine::save_state(&self, path: &Path) -> Result<(), EngineError>`：将当前状态写入 JSON 文件
  - `TopologyEngine::load_state(&mut self, path: &Path) -> Result<(), EngineError>`：从 JSON 文件恢复状态，并校验每个状态对应当前拓扑中的合法状态
- 错误处理：文件 IO 错误、JSON 解析错误、非法状态恢复错误

#### 里程碑 M11：热重载拓扑

- 新增 `TopologyEngine::reload_topology(&mut self, json_str: &str) -> Result<(), EngineError>`：
  - 解析并校验新拓扑
  - 保留现有信号状态
  - 对新增信号使用 initial_state
  - 对删除信号丢弃状态
  - 替换内部 schema/transitions
- 新增 `EngineError::ReloadError(String)` 用于热重载相关错误

#### 里程碑 M12：CLI 与测试

- 扩展 `stt` 或新增 `stp`（signal-topology-persist）CLI：
  - `stp save <topology.json> <state.json>`：加载拓扑、注册空动作、发送 scenario 后保存状态
  - `stp reload <topology.json> <new_topology.json>`：演示热重载
- 新增测试：
  - 状态保存后能正确恢复
  - 热重载后状态保留/新增/删除行为正确
  - 非法状态文件加载失败
- 文档 `doc/persistence.md`：状态持久化与热重载使用说明

## 下下下下阶段目标：级联信号与派生事件（v0.6）

目前各信号完全独立，但真实业务中一个信号的状态变化经常需要触发另一个信号的事件，例如“订单支付成功”后“库存”应自动扣减。下一步引入受控的级联机制：信号 A 的转移完成后，可向信号 B 投递派生事件。

方案：

- 在拓扑描述中新增 `reactions` 数组，声明源信号在特定状态变化时向目标信号投递的事件。
- 引擎在 `send_event` 主转移完成后，按声明触发派生事件。
- 为避免无限循环，默认限制级联深度，超过则返回错误。

#### 里程碑 M13：级联描述模型

- 在 `TopologySchema` 中新增 `reactions: Vec<ReactionDef>`
- `ReactionDef` 字段：
  - `from_signal`: 源信号 ID
  - `from_state`: 源状态（或 `*` 表示任意）
  - `to_signal`: 目标信号 ID
  - `event`: 向目标信号投递的事件名
  - `payload`: 可选的静态 payload 模板（JSON），支持引用源上下文
- 静态校验：from_signal、to_signal 必须存在

#### 里程碑 M14：引擎级联执行

- `send_event` 完成主转移后，扫描并匹配 reactions
- 对每个匹配 reaction，递归调用 `send_event` 投递派生事件
- 新增配置 `max_cascade_depth: usize`（默认 8）
- 超过深度返回 `EngineError::CascadeDepthExceeded`
- 记录级联过程到 Trace

#### 里程碑 M15：测试、文档与示例

- 新增 `tests/cascade_test.rs`：
  - 单级级联
  - 多级级联
  - 循环级联被深度限制
  - 级联失败不影响已完成的转移
- 文档 `doc/cascades.md`：级联语义、深度限制、调试建议
- 示例：`examples/cascade_topology.json`

## 下下下下下阶段目标：模块化导入与组件复用（v0.7）

目前所有信号、转移都内联在单一 JSON 文件里，规模稍大就难以复用和协作。原 MVP 排除清单里承诺了「模块化导入、组件复用」，这是迄今为止唯一未做的结构性项目。本阶段让拓扑能引用**参数化组件**（可复用的状态机片段），并支持**跨文件导入**，从而把「描述文件与业务逻辑分离」的范式推进到「描述文件之间也可组合」。

方案：

- 新增 `components`（命名、参数化的子拓扑：signals + transitions + reactions）
- 新增 `instances`（以具体绑定实例化组件的地方，支持 `${param}` 模板替换）
- 新增 `includes`（导入其它 JSON 文件，递归展开，做循环检测）
- 引擎本身仍消费「展开后的扁平 schema」——引擎层不变，组合逻辑放在新增的 `src/compose.rs` 加载阶段
- 工具链（stv / stt / stp）改用新的加载器

#### 里程碑 M16：参数化组件展开（in-file）

- Schema 扩展：`components: Option<HashMap<String, ComponentDef>>`、`instances: Vec<InstanceDef>`
  - `ComponentDef`：`params: Vec<String>` + signals / transitions / reactions
  - `InstanceDef`：`component`（组件名）+ `bindings: HashMap<String,String>`（参数 → 具体值）
- 新增 `src/compose.rs`，核心函数 `expand(schema) -> Result<TopologySchema, EngineError>`
  - 按 `instances` 查找组件，对所有字符串字段做 `${param}` 替换，拼接成扁平 signals/transitions/reactions
  - 仅用标准库 + 已有 serde，不引入额外依赖
- 错误变体（加入 `error.rs`）：`ComponentNotFound`、`MissingBinding`、`DuplicateSignalAfterExpand`、`InvalidParamRef`
- 展开后的 schema 无 components/instances，喂给现有引擎——**引擎层零修改**
- 新增 `tests/compose_test.rs`：基本实例化、同一组件多次实例化、参数注入到状态名、缺少绑定报错、展开后信号重复报错
- 文档草案 `doc/composition.md`

#### 里程碑 M17：跨文件导入（includes + 循环检测）

- Schema 扩展：`includes: Vec<String>`（相对主文件路径）
- `src/compose.rs` 增加自由函数 `load_topology(path: &Path) -> Result<TopologySchema, EngineError>`
  - 读主文件 → 递归加载并展开 `includes`（用规范路径的 visited-set 做循环检测）→ 展开本地 instances → 返回扁平 schema
- 合并语义：signals / transitions / reactions / components 取并集；跨文件出现重复 signal id → 显式报错（避免静默覆盖）
- 错误变体：`IncludeNotFound`、`IncludeCycle`
- 对外加载入口：保留 `from_json`（平面用法不变），新增的自由函数供 CLI 工具调用；或提供 `TopologyEngine::from_json_at(path)`
- 新增 `tests/compose_test.rs` 用例：两文件导入、传递导入、循环检测、跨文件重复信号报错、平面旧文件仍可用

#### 里程碑 M18：CLI 集成、示例与文档

- `stv` / `stt` / `stp` 改用新加载器（includes 相对主文件解析）；`stv` 在 DOT 中用子图标签/备注标记组件/实例来源
- 示例库：
  - `examples/components/lockable.json`：「可锁定」组件（locked/unlocked）
  - `examples/components/house.json`：实例化 door、window 两个锁
  - `examples/components/breaker.json`：导入式「故障保护」组件，演示 includes
- 顺带补上一直缺失的 `README.md` 快速开始
- `doc/composition.md` 定稿：组件语法、实例化、导入、错误排查

### 本阶段验收标准

1. `cargo test --test compose_test` 全部通过，且现有 49 个测试不回归
2. 同一组件可多次实例化、互不干扰
3. 跨文件导入含循环时返回 `IncludeCycle` 报错而非死循环
4. 跨文件重复 signal id 返回显式报错
5. `stv examples/components/house.json` 能生成含 door/window 子图的 DOT
6. 平面旧 JSON 仍可用（`from_json` 向后兼容）

## 后续方向（v0.8 → v0.11，已展开为里程碑）

在原始 MVP 排除清单（事务回滚、模拟调试、并发、多语言绑定）中，按「真实正确性缺陷优先、其次可感知性、最后生态扩展」排序。

### v0.8 事务与回滚（正确性缺陷，最高优先级）

**问题**：`send_event`（src/engine.rs:269-276）在 `on_transition`/`on_enter` 动作执行前就 `signal.current = to_state` 并推 `StateChanged`。一旦后续动作失败，信号状态已变但动作只完成了一部分 —— 业务层观察到状态变化却无法回滚动作副作用，引擎处于不一致态。

**方案**：把状态提交推迟到全部生命周期动作成功之后；任一动作失败则恢复到源状态并返回错误。

#### 里程碑 M19：延迟状态提交

- 调整 `send_event_internal` 执行顺序：
  1. 匹配转移 + 求值 guard（失败→返回，状态不变，同现状）
  2. 执行 `on_exit` 动作（失败→返回，状态不变，同现状）
  3. 保存 `old_state = signal.current`
  4. **临时** `signal.current = to_state`
  5. 执行 `on_transition` 动作；若失败→`signal.current = old_state`，返回错误
  6. 执行 `on_enter` 动作；若失败→`signal.current = old_state`，返回错误
  7. **全部成功**后推 `TraceEvent::StateChanged`
- 关键：状态回滚只恢复 `signal.current`，动作的外部副作用（IO、日志）不可逆——这是业务动作的固有限制，Trace 里保留 `ActionFailed` 让调试可观测。
- 新增错误变体 `ActionExecutionError` 已存在，无需新增；可考虑加 `Rollbacked { signal, from, to }` TraceEvent 让回滚可观测。

#### 里程碑 M20：测试与回滚可观测性

- 新增 `tests/transaction_test.rs`：
  - 全部动作成功→状态正常跃迁、StateChanged 存在
  - `on_transition` 动作失败→状态回滚到源状态、返回错误、StateChanged 不存在但 ActionFailed 存在
  - `on_enter` 动作失败→同上回滚语义
  - 多级级联下某层失败→该层回滚
- 文档 `doc/transaction.md`：回滚语义、调试（看 Trace 的 ActionFailed / Rollbacked）、与外部副作用的交互说明

### v0.9 交互式模拟（可感知性）

目前「描述文件驱动」的链路需写 Rust 注册动作才能感知。新增一个面向终端用户的交互式模拟器。

#### 里程碑 M21：`sts` (signal-topology-shell) REPL

- 新增 `src/bin/sts.rs`：命令行 `sts <topology.json>`，内部用 `load_topology` + 注册打印动作。
- 命令：
  - `event <signal> <e> [json payload]` — 发事件，输出 StateChanged + 失败时回滚信息
  - `state` — 列出所有信号当前状态
  - `trace` — 打印最近一次 send_event 的 trace
  - `reset` — 清空 trace / 重置到 initial（可选重载拓扑）
  - `help` / `quit`
- 仅用 std（`std::io` 读行），不引入 rustyline 等新依赖；保留一行简易解析即可。
- 把引擎的 ActionFn 注册为一个通用"记录 + 打印"动作，让用户无需写 Rust 即可观察整条链路。

#### 里程碑 M22：文档与示例

- 新增 `examples/` 场景（订单审批、门控流程）配 `sts` 逐步演示。
- 文档 `doc/shell.md`：安装 / 命令列表 / 调试流程 / 与 stt 的区别。

### v0.10 事务语义扩展（级联事务）

在 v0.8 单信号回滚基础上，可选地支持级联场景下更明确的失败语义：当一个派生级联失败时，文档化「已提交的上层状态不回滚」的语义（业务层可选在 reaction 上通过 guard 做补偿）。

#### 里程碑 M23：级联失败语义文档化 + 测试

- 明确测试：主转移成功 + reaction 触发 → 某一级 cascade 失败 → 返回 CascadeDepthExceeded / ActionExecutionError，已 committed 的上层状态保留。
- 文档补充到 `doc/transaction.md` 或独立 `doc/cascade-transaction.md`。

### v0.11 多语言绑定（生态扩展，低优先级）

C-ABI / WASM，让非 Rust 业务代码驱动引擎。依赖团队是否有跨平台需求再排期。

#### 里程碑 M24：WASM 沙箱

- `wasm-bindgen` 封装 `TopologyEngine`，浏览器/Node 可加载。
- 一个在浏览器里用 `<textarea>` 编辑拓扑 + 跑 `sts` 的极简 demo。
- 里程较大，先做可行性调研（`cargo build --target wasm32-unknown-unknown` 验证零依赖是否友好）。

## v0.13 完成后的下一步方向候选

v0.13（M34 DDL 表达力补全 + M35 WASM + M36 stc --check）全收口，177 测试绿，version 0.3.0。候选后续方向（暂不排期，待下一步指令）：

| 方向 | 说明 |
|------|------|
| I2：完整 DDL 表达力 | 补齐 `stc --check` 未覆盖的校验、更丰富的 guard 错误提示；让 DDL 从「可用」升格为「顺手的领域语言」。 |
| J：生态工具 | `stc --watch` 监视重编、DDL 多目标一行出（JSON / DOT 文档）。 |
| K：分布式事务 | 跨信号全有或全无、reaction 补偿（当前为逐信号原子，跨信号不做分布式回滚）。 |

本次不自动推进；等待下一步指令。
