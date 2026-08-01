# 组合系统重构 Check List

> 用途：给后来者或压缩上下文后的实现者——照此逐步实现重构路线。
> 分支：`scope-research`（基于 `text-field` @ `68ea380` + scope 原型 + 设计文档）
> 用户决策（2026-08）：派生值 ✅ ｜ `#[composable]` 属性宏 ✅ ｜ 布局树独立缓存 ✅ ｜ 组件 API **保持 builder + content 闭包**
> 用户偏好：**尽量减少闭包和宏**；`Column::new().build(ctx, |ctx| {...})` 可接受；事件回调闭包天然可接受。

---

## 阶段 0：当前状态（已完成，勿重做）

**组合 scope 原型已实现并验证**：
- `Slot::is_scope` + `SlotTable::start_scope/end_scope/find_slot_mut/mark_dirty_subtree`（`winia/src/core/composer.rs`）
- `SCOPE_STACK` thread-local + `with_active_scope`（scope 栈顶优先，否则 ACTIVE_SLOT_KEY）（`composer.rs` / `state.rs`）
- `Composer::start_scope/end_scope`、`ComposeCtx::start_scope/end_scope`（`composer.rs`）
- `register_dependency` 用 `with_active_scope`（`state.rs`）
- **content 闭包自动 scope**：`Column::build`/`Row::build` 内部 `ctx.start_scope()` ... `ctx.end_scope()`（`ui/layout_components.rs`）
- 验证：demo 第 2 节 `let w = alpha.get() * 200.0 + 50.0; .size(w, 30.0)` 不用闭包、动画平滑（188px 中间值）、注册数 5s≈13、114 测试过

**当前已知问题**（后续阶段注意）：
- `modifier_fn` 字段还在 `Column`（`ui/layout_components.rs`）——派生值/属性宏方案下可删
- `Column` 的 `set_current_node_modifier`（`composer.rs`）——modifier_fn 专用，可删
- content scope 与组件 start 的顺序：`start_scope` 在 `start_restartable_group` **之前**（content 内表达式注册到 content scope）

---

## 阶段 2：派生值（泛型 DerivedValue<T>）

**目标**：`.size(&alpha * 200.0 + 50.0, 30.0)` 内联表达式（非闭包、非宏、非 `let w` 中间变量）；
泛型化支持 Color/Dp/Offset 等任意类型派生。

### 步骤

**1. `winia/src/core/state.rs` 末尾加泛型 `DerivedValue<T>`**：
```rust
#[derive(Clone)]
pub struct DerivedValue<T>(pub(crate) Arc<dyn Fn() -> T + Send + Sync>);

impl<T> DerivedValue<T> {
    pub fn new(f: impl Fn() -> T + Send + Sync + 'static) -> Self { DerivedValue(Arc::new(f)) }
    pub fn get(&self) -> T { (self.0)() }
}

/// f32 派生别名（运算符返回类型）
pub type DerivedFloat = DerivedValue<f32>;

// 运算符（macro 批量，Output = DerivedValue<f32>）：
// impl Add/Sub/Mul/Div<f32> for DerivedFloat / &DerivedFloat / &State<f32>
// 常数在左：impl Mul<&State<f32>> for f32
// 每个 impl 内部：闭包持 State clone（State 是 Arc 句柄，clone 廉价），move || s.get() $op rhs
```

**2. `winia/src/modifier.rs` `SizeValue` 加 `From`**：
```rust
impl From<crate::core::state::DerivedValue<f32>> for SizeValue { /* Dynamic(Arc::new(move || d.get())) */ }
impl From<&crate::core::state::DerivedValue<f32>> for SizeValue { /* clone 后同上 */ }
```

**3. （可选）`BackgroundColor` 加 `From<DerivedValue<Color>>`**（颜色派生）：
```rust
impl From<DerivedValue<Color>> for BackgroundColor { /* color_fn = move || d.get() */ }
impl From<&DerivedValue<Color>> for BackgroundColor { /* clone 后同上 */ }
// 用法：.background(&pulse_color, Shape::Circle) 或 DerivedValue::new(|| pulse_color.get())
```

**4. demo 第 2 节改内联**：
```rust
.size(&alpha * 200.0 + 50.0, 30.0)   // 替换 let w = alpha.get() * 200.0 + 50.0
```
（`background` 颜色可用 `let c` 或颜色派生）

### 验证
- `cargo test --lib` 全过
- demo 第 2 节动画平滑（WS：点击后 0.2s box 宽约 180-230 中间值）
- 注册数仍低（<20/5s）

### 已知坑
- `&alpha * 200.0` 中 alpha 是局部 `State<f32>`，借用后可被后续使用（DerivedValue move 了 clone）
- 运算符 impl 可能与其他类型冲突（`f32 * &State` 与 `&State * f32` 方向不同，需都实现）
- `SizeValue` 的 `resolved_size` 已支持 `SizeValue::Dynamic(f) => Some(f())`——派生值走 Dynamic 分支即可（`modifier.rs` ~775 行）
- 泛型 `Arc<dyn Fn() -> T>` 的 Clone：手动 impl（Arc 共享）或 derive（Arc<T:?Sized> 是 Clone）——derive 即可

---

## 阶段 3：`#[composable]` 属性宏

**目标**：`#[composable]` 标记函数 = Group（函数级 scope），无 content 闭包、无显式 scope

### 步骤

**1. 新建 proc-macro crate**（workspace）：
- `winia-macros/Cargo.toml`：
```toml
[lib]
proc-macro = true
[dependencies]
syn = { version = "2", features = ["full"] }
quote = "1"
```
- 根 `Cargo.toml` workspace members 加 `winia-macros`
- `winia/Cargo.toml` 加 `winia-macros = { path = "../winia-macros" }`

**2. 属性宏实现**（`winia-macros/src/lib.rs`）：
```rust
#[proc_macro_attribute]
pub fn composable(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // 解析 fn 签名 + 块体
    // 变换：
    //   1. 函数体开头插入: { let _g = ctx.start_group();  （压 CURRENT_GROUP）
    //   2. 函数体结尾（最后表达式/return 前）插入: ctx.end_group();
    //   注意：函数须有第一个参数 `ctx: &mut ComposeCtx`
    // 输出：变换后的 fn
}
```
- 关键：函数体是 `Block`，在 `{` 后插 `let _g = ...;`，在 `}` 前插 `ctx.end_group();`
- 若函数有 `return` 提前退出，需处理（简化：要求无提前 return，或所有 return 前插 end_group——v1 只支持末尾）

**3. composer 已有 `start_scope/end_scope`**（阶段 0 实现）——宏注入调用它们即可：
```rust
// 展开后：
#[composable]
fn section2(ctx: &mut ComposeCtx, alpha: &State<f32>) {
    let __scope = ctx.start_scope();   // 宏注入
    let w = alpha.get() * 200.0 + 50.0;
    Column::new().modifier(Modifier::new().size(w, 30.0)).build(ctx, |ctx| { ... });
    ctx.end_scope();                   // 宏注入
}
```

**4. demo 用 `#[composable]` 拆函数**：
```rust
#[composable]
fn section2(ctx: &mut ComposeCtx, alpha: &State<f32>) { ... }
// 调用：section2(ctx, &alpha);  （普通函数调用，无闭包无宏调用语法）
```

### 验证
- `cargo test --lib` + demo 编译
- `#[composable]` 函数内表达式（alpha.get() * 200 + 50）注册到函数 Group——alpha 变 → 函数重跑 → 平滑
- 粒度：函数级（函数内多 content 共享函数 Group）

### 已知坑
- 属性宏**不能变换调用点**——ctx 必须显式传（已定：接受）
- 提前 return 处理（v1 限制：无提前 return 或需处理）
- proc-macro crate 必须独立于 lib crate（workspace 新 crate）
- syn/quote 版本与 workspace 兼容

---

## 阶段 4：布局树独立缓存

**目标**：组合 diff → 增量更新 LayoutNode，跨重组复用（省测量/paragraph 重建）

### 步骤

**1. 布局树与组合树分离**：
- 当前：`compose()` 每帧 `layout_nodes.clear()` 全重建
- 改为：`layout_nodes` 持久化，`apply_composition(group_tree, layout_root)` 按 key/路径匹配增量更新
- 复用：key 相同 → 更新 modifier/测量策略，保留 measured_size/cached_paragraph

**2. `apply_composition` 核心**：
```rust
fn apply_composition(composer: &mut Composer) {
    // 组合产物（LayoutNode 描述或 Group 树）→ 现有 LayoutNode 树
    // 按 slot key/路径匹配：
    //   - 匹配且参数未变 → 复用（保留测量）
    //   - 匹配但 dirty → 更新 modifier，重测
    //   - 无匹配 → 新建；旧的无匹配 → 移除（on_remove）
}
```

**3. 测量复用**：
- `measure_node` 的常量折叠已有（`!dirty && cached_constraints == Some`）——布局树复用后自动生效
- `prev_nodes`/`frame_cache`/`is_replay_stub` 三套缓存**可合并简化**（布局树复用后不需要 stub 重建）

**4. 清理**：
- 删除 `is_replay_stub`/`frame_cache`（如果布局树复用替代了它们）
- 保留 `prev_nodes`（或改为 LayoutNode 树内缓存）

### 验证
- 动画 demo 各节正常 + 动画平滑
- 注册数低（重组局部化）
- 114+ 测试过

### 已知坑
- 组合产物（节点描述）与 LayoutNode 的映射设计——需要稳定的 key/路径
- `on_remove` 回调在节点移除时正确触发（Window 生命周期）
- 增量更新的边界：结构变化（插删节点）时子树重建

---

## 阶段 5：参数相等性跳过（Stable trait）

**目标**：Compose 式 Skip——参数相等则跳过函数体重跑

### 步骤
1. `trait Stable: PartialEq {}`（或 derive 宏）
2. 组件参数实现 Stable → 相等时子 Group 不重跑
3. 动态值（&State/DerivedFloat/闭包）→ 非 Stable → 依赖追踪决定
4. `start_restartable_group` 的 Skip 判定：**slot clean && 参数相等**（当前只有 clean）

### 验证
- 参数未变时子树 Skip（不重建）
- 参数变化时重跑

---

## 通用验证（每阶段后）

```bash
cd /d/Projects/winia/winia
cargo test --lib                      # 114+ 全过
cargo build -p winia --example animation_demo --features debug-server
# WS 验证（demo 启动后）：
#   ws://localhost:9998 → 't' 看树（28/28 非0、Reset 切换）
#   'c 350 40' 点 Animate → 0.2s 后 't' → box 宽应为动画中间值（平滑非跳变）
# 注册数：5s 内 <20（无重组风暴）
```

## 里程碑检查

- [ ] 阶段 2：派生值（`.size(&alpha * 200.0 + 50.0, 30.0)`）
- [ ] 阶段 3：`#[composable]` 属性宏（函数 = Group）
- [ ] 阶段 4：布局树独立缓存（LayoutNode 复用）
- [ ] 阶段 5：参数相等跳过（Stable trait）
- [ ] 文档最终更新（设计 + 使用指南）
