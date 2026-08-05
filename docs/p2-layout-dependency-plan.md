# P2-1 两段式依赖（布局动画统一机制）——实施计划

> 分支：`compose-core`。目标：布局期读动画值 → 只重测不重组，消灭 `force_remeasure` 式旁路。
> 改动范围：`core/state.rs`、`core/composer.rs`、`layout/node.rs`（3 文件）。

---

## 1. 问题（现状链路，已验证）

```
动画值 State 变化（set_no_wake）
  → notify_state_changed_inner → 推入 Composer.pending_states
  → 下帧 compose() 消费 pending：
      slot_deps.get(state_id) → mark_dirty(slot_key)   ← 组合级 dirty
  → 节点 dirty → 下帧重跑用户 composable（重组）
```

**布局期注册混入组合通道**：
- `RECORDING_TARGET`（thread_local 裸指针）→ `recorded_deps: Vec<(u32,u64)>`
- 组合期 `State::get()` 和布局期（measure 中 SizeDynamic 闭包）`State::get()` **写入同一个 recorded_deps**
- compose 末尾 + layout 末尾都 drain 进 `slot_deps`（state_id → HashSet<slot_key>）
- 结果：布局期读动画值 → 动画变化 → **组合级 dirty** → 每帧重跑用户代码

**旁路现状**：`animation-improve` 分支为绕开重组用了 `peek()`（不注册依赖）+ `force_remeasure()`（递归扫祖先链强制重测）——性能/机制双输。

---

## 2. 设计：布局依赖独立通道（对齐 Compose：布局节点订阅 state → remeasure，不 recompose）

### 2.1 数据流

```
布局期 State::get()（measure 内）
  → record_dep 按 IN_LAYOUT 分流
      ├─ 组合期 → recorded_deps（现有）→ slot_deps → mark_dirty（重组）
      └─ 布局期 → layout_recorded（新）→ layout_deps → layout_dirty（只重测）

下帧 compose() 消费 pending：
  slot_deps.get(state_id)   → mark_dirty(k)                    （组合，现有）
  layout_deps.get(state_id) → layout_dirty_keys.insert(k)      （布局，新增，不 mark_dirty）

layout() 测量前：
  layout_dirty_keys → DFS arena 树：命中节点标 node.layout_dirty=true + 祖先链传播

measure_node 常量折叠条件：
  旧：!dirty && cached_constraints == Some(constraints)
  新：!dirty && !layout_dirty && cached_constraints == Some(constraints)
```

### 2.2 三个文件的改动

**A. `core/state.rs`（~15 行）**
- 新增 thread_local `IN_LAYOUT: Cell<bool>`（默认 false）
- `record_dep()` 按 `IN_LAYOUT.get()` 分流到两个目标：
  - 布局期：写入 composer 的 `layout_recorded`（新增第二组 setter：`set_layout_recording_target` / `clear_layout_recording_target`，或一个持有两个指针的结构）
  - 组合期：写入 `recorded_deps`（现有路径不变）
- 实现：把单个 `RECORDING_TARGET` 换成 `RECORDING_TARGETS: (Option<*mut Vec<(u32,u64)>>, Option<*mut Vec<(u32,u64)>>)`（组合/布局各一），`set_recording_target(compose_ptr, layout_ptr)` 一次设置。

**B. `core/composer.rs`（~55 行）**
- Composer 新增字段：
  - `layout_recorded: Vec<(u32, u64)>`（本帧布局期注册）
  - `layout_deps: HashMap<u32, HashSet<u64>>`（state_id → slot_key；上帧布局注册的持久表）
  - `layout_dirty_keys: HashSet<u64>`（本帧 pending 消费收集，layout 应用后清）
- `compose()` pending 消费处：`slot_deps` 分支后追加 `layout_deps` 分支 → 收集到 `self.layout_dirty_keys`（不 mark_dirty）
- `layout()` 开头：`apply_layout_dirty(&arena, &layout_dirty_keys)`：
  - DFS 从 root，node.slot_key ∈ layout_dirty_keys → 标 `node.layout_dirty = true`，**沿祖先链全部标**（父尺寸依赖子——必须重测）
  - 遍历完清空 layout_dirty_keys（下帧重新收集）
- `layout()` 末尾（现有 recorded_deps drain 旁）：
  - `layout_deps` 增量更新：本帧 `layout_recorded` drain 后，**按 slot_key 分组重建**：
    - 本帧注册过的 slot_key → 清旧写新（覆盖）
    - 未注册的 slot_key（常量折叠命中/移除）→ 保留旧项（依赖未变，折叠前提是依赖无 notify）
  - `set_active_slot_key` 已在 measure_node 内（无需改）
- `layout()` 结尾 clear_recording_target 同步清两个指针

**C. `layout/node.rs`（~10 行）**
- `LayoutNode` 新增 `pub(crate) layout_dirty: bool`（构造默认 false；restore_from 时重置 false）
- `measure_node` 常量折叠条件追加 `!nodes[idx].layout_dirty`
- measure 成功路径：`nodes[idx].layout_dirty = false`（重测后清除）
- 每帧 layout 末尾（collect 后）统一清所有节点 layout_dirty（下帧由 apply_layout_dirty 重新标记）——或依赖 apply 时覆盖；选择**apply 前清一次**更稳：layout() 开头先 DFS 清全树 layout_dirty，再 apply 标新值

### 2.3 依赖生命周期（关键正确性论证）

| 场景 | 行为 |
|------|------|
| 节点 A 本帧常量折叠命中（不 measure） | layout_recorded 无 A → layout_deps 保留 A 的旧项 ✓（折叠前提=依赖无 notify） |
| A 的布局依赖 State 变化 | notify → pending → layout_dirty(A) + 祖先 → 下帧 A 重测 → 重新注册 ✓ |
| A 被移除/重建 | 新节点 measure 时覆盖旧 slot_key 项 ✓ |
| 同一 State 组合+布局双依赖 | notify 同时触发 mark_dirty + layout_dirty（互不排斥）✓ |
| 动画 set_no_wake 推进 | notify（不 wake）→ pending → layout_dirty → 下帧重测 ✓（动画帧驱动已由 update_animations 保证） |

---

## 3. 测试计划（新增 4 个）

| # | 测试 | 断言 |
|---|------|------|
| T1 | 记录分流 | 组合期 get() → slot_deps 有、layout_deps 无；布局期（调 measure_node）get() → 反向 |
| T2 | layout_dirty 传播 | 子节点 slot_key 入 layout_dirty_keys → 子与父均 layout_dirty=true |
| T3 | 动画尺寸只重测不重组 | SizeDynamic 依赖动画 State：动画推进 N 帧 → compose 次数不变（组合计数）、measure 次数增加、尺寸更新 |
| T4 | 常量折叠保留依赖 | A 连续两帧折叠（无 notify）→ layout_deps 中 A 的旧项仍在；notify 后 A 重测 |

**回归**：`cargo test --lib` 161 全绿 + `cargo check --examples` 0 error。

---

## 4. 验收标准

1. T1-T4 全过 + 原 161 测试全绿
2. examples 全编译
3. 手工验证（WS 调试）：写临时 demo——Button 点击驱动动画 State，节点 size 随动画变化，树采样确认位置渐变（无跳变），组合计数不增长
4. `animation-improve` 分支的 ShrinkPolicy 思路可在新机制上重做（后续任务，不在本计划内）

---

## 5. 风险与回滚

- **风险**：layout_deps 增量保留逻辑出错 → 动画尺寸不更新（视觉无变化）或过度重测（性能退化）。缓解：T1/T3/T4 直接覆盖。
- **风险**：RECORDING_TARGET 从单指针改双指针 → 现有组合依赖路径受影响。缓解：组合路径代码不动，只加分支；全量测试兜底。
- **回滚**：单个提交，`git revert` 即可。改动集中在 3 文件，无跨模块牵连。

---

## 7. 妥协点追踪表（实施时必须写进代码注释，避免后来者误以为是精确实现）

| # | 妥协点 | 后续去向 | 状态 |
|---|--------|---------|------|
| 1 | thread_local 魔法（IN_LAYOUT + 双指针 RECORDING_TARGET）——现有裸指针方案的扩展，非显式 measure scope | **P2-3 依赖注册收敛**：slot_deps/layout_deps 统一依赖管理器 + 评估去裸指针 | 已立项 |
| 2 | 祖先全链 layout_dirty 是保守超集（Compose 是精确传播） | **无立项**：布局动画场景父必然依赖子尺寸，实际等价；等深树高频布局动画的性能证据再精确化（YAGNI） | 接受降级 |
| 3 | composer.rs 新增 4 字段/方法，SRP 继续恶化 | **P2-2 物化器拆分**：desc→arena 物化拆出；依赖表归属（独立 dependency.rs 或随物化器）P2-3 定 | 已立项 |
| 4 | layout_deps 死 key 残留（永久移除的节点旧项不清理） | **P2-1 顺手清**：每帧 diff 时用 prev_node_by_key 已知的本帧移除 slot_key 集合清理（约 5 行） | 本计划实施 |

---

## 8. 实施步骤（提交拆分）

1. **提交 1**：state.rs 双指针分流 + composer.rs 字段与 setter（编译通过，行为不变——layout_deps 未接线）
2. **提交 2**：composer.rs pending 分流 + layout_dirty_keys 收集 + apply_layout_dirty（含祖先传播）
3. **提交 3**：node.rs layout_dirty 字段 + 常量折叠条件 + 每帧清理
4. **提交 4**：T1-T4 测试
5. 每提交：`cargo test --lib` + `cargo check --examples`
