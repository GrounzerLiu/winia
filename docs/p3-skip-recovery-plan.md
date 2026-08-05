# P3-1 Skip 恢复健壮性——设计文档

> 分支：`compose-core`。目标：结构变化时 Skip 恢复不"张冠李戴"（历史塌缩类 bug 根因）。
> 配套：`handover.md` §3.2、`cleanup-plan.md` P3-1。

---

## 1. 现状盘点（先确认已完成的部分）

**Skip 语义已 Compose 化（scope-research 阶段完成，无需重做）**——`composer.rs:1019`：

```rust
let is_skip = slot_status == SlotStatus::Clean   // 组内 state 无变化（dirty 驱动）
    && params_unchanged                           // 参数相等（ctx.changed 比较）
    && self.prev_nodes.contains_key(&key);        // 有上帧缓存
```

- 参数变化 → Enter（即使 slot clean）✓ 对标 Compose `@Stable` 跳过
- dirty（组内 state 变化）→ Enter ✓ 对标 Compose 组内订阅失效
- 结构变化时的内容语义：Compose 同位置复用 remember 状态，本项目一致（位置复用是特性）

**恢复机制**（`core/materialize.rs` Skip 分支）：
- `prev_node_by_key.remove(&key)` → 恢复缓存节点（modifier/measured_size/约束）
- 子节点按 slot 树结构逐个恢复（本帧组合结构 + 上帧缓存内容）

**残留缺陷（本计划要修）**：Skip 恢复命中条件 = 仅 `prev_nodes.contains_key(&key)`——**未验证子树结构一致性**。if 分支切换/列表项增删后，同位置 slot_key 可能命中**旧内容的缓存**（结构数量不同但位置 key 相同）→ 恢复的 measured_size/modifier 与当前内容不匹配 → 视觉塌缩/错位（历史 bug 家族：nest_demo 塌缩、面板双击消失、modifier 被清空）。

---

## 2. 方案：恢复命中条件升级为"路径相等 + 结构签名相等"

### 2.1 结构签名（children_count，最小侵入）

`Slot.children_count`（composer.rs:462）已存在——**本帧 slot 子树总 slot 数**（start_slot 递归维护）。恢复时对比：

```
Skip 恢复命中条件（新增）：
    prev_nodes.contains_key(&key)
    && cached.children_count == 本帧 slot.children_count
```

- 子树 slot 数量变化（if 分支增删、列表项增删）→ 签名不等 → 放弃恢复 → Enter 重建（dirty=true 重测）→ 结构正确
- 数量相同但内容语义不同（A→B 切换）→ 签名相同仍恢复——但这是 **Compose 语义**（同位置复用 remember/缓存，内容更新靠 state 驱动；A→B 无状态驱动时 Compose 同样显示旧内容）——**不修，符合语义**

### 2.2 实现（CachedNode + materialize）

1. **`CachedNode` 加字段 `children_count: usize`**（node.rs:179）——to_cached 时记录 `self.children.len()`（**物化后 arena 节点 children 数 = 子树 slot 数？**——不等！arena children 是直接子节点数，slot children_count 是子树总 slot 数。修正：**记录 arena 子树总节点数**（DFS 数）或记录直接子数）
   - 用**直接子节点数**更简单且足够：结构变化（增删子树）必然改变某层直接子数。
   - `cached.children_count = self.children.len()`（直接子）
2. **`materialize_node` Skip 分支**（materialize.rs）加验证：
   ```rust
   // 结构签名验证：本帧 slot 树子节点数 vs 缓存节点直接子数
   //（DescNode.children.len()——collect_desc_tree 已建好本帧子树）
   let sig_ok = desc.children.len() == cached_node.children.len();
   if sig_ok { 恢复 } else { 降级 Enter（现有 None 分支） }
   ```
   - 本帧 `desc.children.len()`（组合产物子树）vs 缓存节点 `children.len()`（上帧 arena 子树）——不匹配 → 走现有防御降级路径（重建节点 + dirty）
3. **保留语义**：数量相同时照常恢复（Compose 位置复用语义）

### 2.3 影响面

- `materialize.rs` Skip 分支（~10 行）
- `node.rs` CachedNode 字段 + to_cached（~5 行）
- 无 API 变化、无组合语义变化——纯恢复命中条件收紧

---

## 3. 测试计划

| # | 测试 | 断言 |
|---|------|------|
| T1 | 结构不变 Skip 恢复正常（回归） | 现有 Skip 恢复测试全过（children_count 匹配） |
| T2 | if 分支增删子树 → 签名不等 → Enter 重建 | 帧1 [A,B] 帧2 [A]（B 移除）→ B 位置新节点 Enter + dirty 重测，无缓存错位 |
| T3 | 列表项增删（3→2） | 同 T2，子树根 Enter |
| T4 | 数量相同内容不同（A→B） | 保持恢复（Compose 语义——不强制 Enter） |

**回归**：`cargo test --lib` 167 全绿 + `cargo check --examples` 0 error。

---

## 4. 验收标准

1. T1-T4 全过 + 167 全绿
2. nest_demo 手动验证：Show/hide 切换 + 快速切换无塌缩（WS 树采样）
3. 提交单个、可 revert

---

## 5. 范围外（记录，不实施）

- **树内 diff / desc 比较**（handover §3.2 的另一方向）：当前场景（if/列表）由 children_count 签名覆盖；复杂 diff 收益边际低，YAGNI 不引入
- **显式 `key()` 强制**：用户可用 `ctx.key()` 区分同位置不同语义（Compose 同款）——文档提示即可
- 副作用生命周期（P3-2）独立于本计划

---

## 6. 实施步骤

1. node.rs：CachedNode + `children_count` 字段（to_cached 记录直接子数）
2. materialize.rs：Skip 分支加签名验证（不匹配走现有降级路径）
3. T1-T4 测试（T2/T3 为核心新增）
4. 每提交：`cargo test --lib` + `cargo check --examples`
