# 组合 Scope 可行性研究

> 研究分支：`scope-research`（基于 `text-field` @ `68ea380`）
> 目标：让 `.size(alpha.get() * 200.0 + 50.0, 30.0)` **不用闭包**也能动画平滑（对标 Compose）

## 一、问题本质

闭包方案（`size(move || alpha.get() * 200.0 + 50.0)`）之所以必要，是因为两个架构缺口：

| 缺口 | 现状 | 影响 |
|------|------|------|
| 依赖注册目标 | `State.get()` 注册到**当前 start 的节点**（ACTIVE_SLOT_KEY） | `Column::new().modifier(size(alpha.get()*200+50))` 的求值发生在 build **之前**，注册到**上一个节点**（错位） |
| Skip 判定 | 只看 slot 是否 dirty | box slot clean → Skip（stub 用旧 modifier）→ 组合代码不重跑 → 新值丢失 |

**Compose 解法**：组合 scope（每个 @Composable 调用是 scope），读 State 注册到 scope，scope 失效 → **重新执行 scope 的组合代码** → 表达式重算 → 新 modifier。

## 二、原型设计（最小可行）

### 核心概念

**组合 scope（ComposableScope）**：Slot 树中的一个节点（`is_scope: bool`），**不产生 LayoutNode**，只作为：
- **依赖注册目标**：scope 内（组件外）的 `State::get()` 注册到最内层 scope
- **失效传播单位**：scope 依赖的 State 变化 → scope 子树全部强制 Enter

### 三个改动点

**1. Slot 树 scope 节点**（`composer.rs`）
- `Slot::is_scope: bool`
- `SlotTable::start_scope(key)` / `end_scope()`（复用 start_slot/end_slot，标记 is_scope）
- `find_slot_mut` + `mark_dirty_subtree`：scope 失效 → 子树全标 dirty

**2. 依赖注册目标**（`state.rs` + `composer.rs`）
- thread-local `SCOPE_STACK: Vec<u64>`
- `register_dependency` → `with_active_scope`：栈非空 → 栈顶 scope key；否则 → ACTIVE_SLOT_KEY（组件内）

**3. 失效传播**（`composer.rs`）
- `mark_dirty(key)`：若 key 是 scope → `mark_dirty_subtree`（子树全 Enter，不 Skip）

### 用法（demo 第 2 节）

```rust
ctx.start_scope();                       // 进入 scope
let w = alpha.get() * 200.0 + 50.0;      // 注册到 scope（组件外）
let c = Color::from_argb((alpha.get()*255.0) as u8, 76, 175, 80);
Column::new()
    .modifier(Modifier::new().size(w, 30.0).background(c, ...))  // 静态值，不用闭包
    .build(ctx, ...);
ctx.end_scope();                         // 退出 scope
```

**机制**：alpha 变化 → mark_dirty(scope) → scope 子树全 Enter → 组合代码重跑 → `w`/`c` 重算 → Column 重建（modifier 用新值）。

## 三、验证结果

- ✅ **不用闭包**：`size(w, 30.0)` 直接静态值
- ✅ **动画平滑**：点击后 0.2s box 宽 180（alpha 0.2→0.9 中间值）、Alpha 0.65
- ✅ **依赖到位**：scope 内 `alpha.get()` 注册到 scope（非上一个节点）
- ✅ **失效传播**：scope 子树强制 Enter，组合代码重跑
- ✅ 114 测试全过、28/28 布局正常

## 四、代价与局限

### 粒度
- **scope 越大，重组范围越大**（scope 失效 → 其内所有组件重建）
- 当前 demo 每节一个 scope → alpha 变化只重建第 2 节（粒度 OK）
- 若一个 scope 包整个页面 → 类似"整树重组"（粒度差）
- **建议**：scope 尽量小（每节/每组件一个）

### API 形态
- `start_scope()` / `end_scope()` 是方法调用（**不是闭包**），但需要用户手动配对
- 未来可用宏（`scope! { ... }`）或标记函数自动管理，消除手动配对

### 与现有机制的关系
- 组件内（build 后）的 `State.get()` 仍注册到组件（ACTIVE_SLOT_KEY）——**不变**
- scope 只影响"组件外"的读取（modifier 构造、局部表达式）
- `remember` 在 scope 内存到 scope slot（Slot 树的 remembered）——**跨重组保留**

### 已知边界
- scope 内**多个组件共享** alpha 依赖 → 一次 alpha 变化重建整个 scope（合理，scope 是语义单元）
- scope 嵌套：SCOPE_STACK 支持任意深度（最内层生效）
- scope 与 restartable group（Column/Row）正交：scope 是"代码单元"，group 是"节点单元"

## 五、结论与建议

**可行性：✅ 成立**。组合 scope 用 ~120 行改动实现了"不用闭包 + 动画平滑"，机制（依赖注册到 scope + 失效子树重跑）与 Compose 一致。

**后续工作（若采纳）**：
1. **宏封装**：`scope!(ctx, { ... })` 消除手动 start/end 配对（防漏配）
2. **每节独立 scope**：demo 全面改造，验证多 scope 并行
3. **scope 与懒加载**：scope 失效时只重跑代码，LayoutNode 复用缓存（当前是重建）
4. **性能度量**：scope 粒度 vs 重组开销的权衡数据

**不建议**：做完整 Compose 级 scope（编译器标记 + 自动代码生成）——Rust 无 @Composable 等价物，宏/显式 scope 已能达到等效 API 体验。
