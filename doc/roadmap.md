# 信号拓扑图发展路线图

## 当前状态

已完成 v0.1 MVP：

- JSON 拓扑描述格式
- Rust 引擎解析、静态校验、状态流转
- 事件驱动的状态转移
- 生命周期动作绑定（on_exit / on_transition / on_enter）
- 5 个集成测试全部通过

## 下阶段目标

### 1. 拓扑图可视化查看（MVP+）

让用户无需阅读 JSON 即可直观看到信号、状态与转移关系。仅做“查看”，不参与运行时逻辑。

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
