# [P1] 全量重组优化：LayoutNode 复用 + layout 增量 dirty

<!-- 进度表格由 task-keeper 管理，请勿手动编辑，否则会被覆盖 -->

## 实施方案

### 背景
当前 compose() 每次都 layout_nodes.clear() → 无条件 LayoutNode::new()。即使 composable 函数输出完全不变，所有 measured_size 归零，必须全量 re-measure。SlotTable 已有 dirty 标记但 LayoutNode 层完全没利用。

### 核心思路
引入双缓冲：prev_layout_nodes (旧帧) ↔ layout_nodes (新帧)。在 start_node() 中，若 slot clean，从 prev_layout_nodes 克隆节点（保留 measured_size），否则新建。measure_node 入口检查 node.dirty → clean 则跳过重新测量。

### 第 1 步：SlotTable.start_slot() 返回状态
- 当前返回 ()，无信号
- 改为 pub(crate) enum SlotStatus { Clean, New, Dirty }
- start_slot() 返回 SlotStatus：
  - Clean：key 匹配 + slot.dirty == false + dirty_keys 中不存在
  - Dirty：key 匹配 + (slot.dirty || dirty_keys 包含)
  - New：不匹配（新 key 路径）

### 第 2 步：双缓冲
- Composer 加 prev_layout_nodes: Vec<LayoutNode>
- compose() 末尾 prev_layout_nodes = std::mem::take(&mut layout_nodes)
- compose() 开始时不清空 layout_nodes，仅 clear() 重置
- 保留 prev_node_stack 用于复用时的树位置追踪

### 第 3 步：start_node() 复用
- 接收 SlotStatus
- Clean → 从 prev_layout_nodes 取对应索引节点的 clone，用新 modifier 更新
- New/Dirty → LayoutNode::new（当前行为）
- 追踪 prev_index → new_index 映射

### 第 4 步：measure_node dirty 跳过
- LayoutNode 加字段：
  - pub dirty: bool（新节点 = true，复用 clean 节点 = false）
  - pub cached_constraints: Option<Constraints>
- measure_node 入口：若 !node.dirty && constraints == cached → return measured_size
- 测量完成后设置 node.dirty = false; node.cached_constraints = Some(constraints)

### 影响范围
- slot_table.rs: ~5行变更
- composer.rs: ~40行变更
- node.rs: ~10行变更
- 测试: 所有现有测试应继续通过

## 进度







| 步骤 | 状态 | 备注 |
|------|------|------|
| 1. SlotTable 暴露 slot 是否 clean — start_slot 返回 SlotStatus 枚举 | ✅ 完成 | 新增 SlotStatus 枚举 (Clean/Dirty/New)。start_slot() 改为返回 SlotStatus：Clean（key 匹配且无脏标记）、Dirty（key 匹配但有脏标记或新 slot）、New 路径通过 Dirty 变体覆盖。start_node() 调用点暂用 _slot_status 捕获返回值。编译通过，3 已有警告无新增。 |
| 2. Composer 加 prev_layout_nodes 双缓冲 — compose() 结束时 swap 而非 clear | ✅ 完成 | 新增 prev_measured_sizes: HashMap<Vec<usize>, Size> 字段。compose() 中移除 prev_root 保存逻辑，改为 layout() 后调用 collect_measured_sizes() 收集整棵树测量结果。双重缓冲机制：每帧 layout 完成后更新缓存供下帧复用。 |
| 3. start_node() 复用 clean slot 的 LayoutNode — 保留 measured_size | ✅ 完成 | start_node() 中 slot_status = start_slot(key); Clean 且 prev_measured_sizes 命中时：node.measured_size = cached_size; node.dirty = false。SlotTable 暴露 current_path() 供路径匹配。收集函数 collect_measured_sizes() 递归遍历 LayoutNode 树，按 [child_idx, ...] 路径索引。 |
| 4. LayoutNode 加 dirty 标志 + cached_constraints — measure_node 入口跳过 clean | ✅ 完成 | LayoutNode 新增 dirty: bool 和 cached_constraints: Option<Constraints>。所有构造点 (new/container/Default) 初始化为 dirty=true, None。measure_node 入口：!dirty && constraints == cached_constraints 则直接返回 measured_size（跳过重新测量）。测量完成后标记 dirty=false + 缓存约束。修改了 if-else 为 let result = ... 模式统一出口。 |
| 5. 运行测试验证 — 32 测试通过，全量重组行为不变 | ✅ 完成 | cargo test --lib: 32 passed, 0 failed。无新增编译警告。全量重组行为对观察者不变，但内部已新增 clean slot 的 measured_size 缓存复用和 measure_node dirty 跳过机制。 |

- **进度类型**：sequential（顺序）
- **完成进度**：5/5（当前：5/5）

