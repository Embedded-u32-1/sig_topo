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

## 技术边界（保持克制）

- 不引入 Web 前端/编辑器（避免复杂度爆炸）
- 可视化只读，不写回 JSON
- 不改动核心引擎运行时行为
- 优先使用文本格式与系统工具，不绑定特定图像库

## 验收标准

1. `cargo run --bin stv -- examples/topology.json` 能生成 `topology.dot`
2. 在有 Graphviz 环境下能自动生成 `topology.svg`
3. 生成的图包含所有信号、所有状态、所有转移
4. 新增测试覆盖 DOT 导出关键内容
5. 现有 5 个集成测试保持通过
