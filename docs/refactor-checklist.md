# 组合系统重构 Check List

> 用途：给后来者或压缩上下文后的实现者——照此逐步实现重构路线。
> 分支：`scope-research`（基于 `text-field` @ `68ea380` + scope 原型 + 设计文档）
> 用户决策（2026-08）：派生值 ✅ ｜ `#[composable]` 属性宏 ✅ ｜ 布局树独立缓存 ✅ ｜ 组件 API **保持 builder + content 闭包**
> 用户偏好：**尽量减少闭包和宏**；`Column::new().build(ctx, |ctx| {...})` 可接受；事件回调闭包天然可接受。

---

## 阶段 0：当前状态（已完成，勿重做）

**组合 scope 原型已实现并验证（含 review 修复 80e15cb）**：
- `Slot::is_scope` + `SlotTable::start_scope/end_scope/find_slot_mut/mark_dirty_subtree/current_child_is_scope`（`winia/src/core/composer.rs`）
- `SCOPE_STACK` + `NODE_DEPTH` thread-local + `with_active_scope`：
  **组件 build 内（NODE_DEPTH>0）→ 当前节点 key（组件级失效粒度）；组件外（表达式/局部变量）→ 最内层 scope key**（`composer.rs` / `state.rs`）
- `Composer::start_scope/end_scope`、`ComposeCtx::start_scope/end_scope`（`composer.rs`）
- `register_dependency` 用 `with_active_scope`（`state.rs`）
- **content 闭包自动 scope**：`Column::build`/`Row::build`/`Stack::build`/`Button::build` 内部 `ctx.start_scope()` ... `ctx.end_scope()`（`ui/layout_components.rs` / `ui/button.rs`）
- **replay 修复**：`replay_clean_subtree` 跳过 scope slot（`is_scope` 不建 LayoutNode，只递归重放子，子挂到当前父节点）——消除幽灵 stub/双重挂载
- **is_scope 重置**：`start_node` 复用 scope slot 时 `set_current_scope(false)`（同路径类型切换）
- 验证：demo 第 2 节 `let w = alpha.get() * 200.0 + 50.0; .size(w, 30.0)` 不用闭包、动画平滑（188px/Alpha 0.66 中间值）、注册数 5s≈13、**117 测试过**（含 3 个 scope 测试：依赖失效/scope-group 配对/节点优先级）

**当前已知问题**（后续阶段已解决——记录留档）：
- ~~`modifier_fn` 字段还在 `Column`~~——派生值方案下已删
- ~~`Column` 的 `set_current_node_modifier`~~——modifier_fn 专用，已删
- ~~content scope 与组件 start 的顺序~~——Part6（9ecd2e1）已统一：`start_restartable_group` push 组件 scope（容器=scope），移除组件自动 content scope
- ~~scope 是 slot 树中 group 的父（slot 树与 LayoutNode 树结构不一致的脆弱点）~~——Part6 后容器即 scope（slot 树与 LayoutNode 树结构一致），脆弱点消除

---

## 阶段 2：派生值（泛型 DerivedValue<T>）—— ✅ 已完成（fa477f3）

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

## 阶段 3：`#[composable]` 属性宏 —— ✅ 已完成（1813329）

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
- 提前 return 处理（v1 限制：无提前 return 或需处理）——**已被 RAII guard 方案自然解决**（见下）
- proc-macro crate 必须独立于 lib crate（workspace 新 crate）
- syn/quote 版本与 workspace 兼容

### 超出原规划的扩展（后续提交）

- **语句级 key 注入**：`start_scope_keyed(签名哈希)` + 每条语句 `enter_stmt(id)`（RAII `StmtGuard`——thread_local STMT_STACK——return/break/continue/panic 提前退出自动 pop 防泄漏）+ 表达式语句 guard 块包 / let init guard 块包 / 尾表达式跳过包裹（值语义）
- **content 闭包注入**：单参数名为 `ctx` 的闭包视为 content（约定）——注入其体（嵌套递归）；Call/MethodCall 参数遍历（build 的闭包在方法调用参数位）
- **`ctx.key(id, |ctx|)`** 显式 key API（key_override_stack——列表重排/子树移动场景）
- **签名哈希**（含参数类型——跨模块同名碰撞缓解）
- **8 个宏展开级防回归测试**（winia-macros：content 闭包/if 分支/let init/嵌套闭包/match 臂/for 体/let-else diverge/尾表达式）
- 关键修复：`inject_stmt_ids` 的"先注入后 match"统一注入点（防回归测试守护）；`start_scope_keyed` 独立实现避免双重 push（scope=0 跨函数 key 碰撞串位 bug）

---

## 阶段 4：布局树独立缓存 —— ✅ 全部完成（Part1-6）

**Part1（2059720）**：`prev_nodes`/`frame_cache` 改以 `slot_key` 为键（原 slot path 含 scope 层 vs LayoutNode path 无 scope → 键 miss → is_skip 恒 false 全 Enter）；新增 `restore_layout`；`test_is_skip_after_clean_frame`。
**Part2（2df4474）**：measure 期动态尺寸依赖修复（kf/dp 冻结）+ `State::set_no_wake`（wake 自旋）。
**Part3（c5fc856）**：demo 粒度优化（注册数 3.3x）。
**Part4（1cd7e1c）**：**arena 索引树重构**——`LayoutNode.children: Vec<LayoutNode>` → `Vec<usize>` + `NodeArena`（nodes/free/root/policies 池）+ `MeasurePolicy` trait arena 化（nodes/policies/children 索引拆分借用）+ measure_node/hit_test/focus 系列全部 arena 化 + render/app/debug 调用点（fleet 4 并行任务 + 手动修复）。
**Part5（a68c180）**：**节点复用**——compose 保留上帧树 + `prev_node_by_key` + start_node/start_restartable_group 按 slot_key 复用槽位 + compose 末尾回收未复用（`free_node_skip` 跳过已复用——修复复用节点被 free 递归进本帧树成环的栈溢出；根因：start_restartable_group 复用分支漏记 reused_nodes）+ visited 防环——`test_arena_reuse_stabilizes` 验证 10 帧 arena 16→16 **零增长**（对象级复用 100%）。
**Part6（9ecd2e1）**：**content scope 死代码修复（架构级）**——统一依赖注册目标 = 最内层 scope（对标 Compose RestartGroup/ReplaceGroup）：`with_active_scope` 移除 NODE_DEPTH 优先（content 闭包内 NODE_DEPTH 恒≥1 → scope 栈收不到依赖）→ 始终 scope 栈顶、空则回退 ACTIVE_SLOT_KEY；`start_restartable_group` push 组件 scope / end pop（容器=scope：组件内 Text 读取注册最近容器——对标 ReplaceGroup 内联）；移除组件自动 content scope（与容器 scope 合并）；删 NODE_DEPTH 死代码。

**遗留（后续）**：
- vsync：swapchain 已用 PresentMode::Fifo，但动画期间每显示帧多次重组（事件循环 Wait + request_redraw 的 Windows 行为疑点）——渲染层性能优化，非正确性
- 组合树与布局树仍耦合（设计文档"组合树一等公民"——Slot 树 + arena 树并存——远期）

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

## 阶段 5：参数相等性跳过（Stable trait）—— ✅ 已完成（877e4e7）

**实现**：`ParamValue` trait（`Box<dyn Any>` 无法通用 PartialEq，trait object 桥接 `eq_any`）+ `Slot.params`/`Composer.pending_params` + `ComposeCtx::changed<T: PartialEq + Clone>`（对标 Compose `$composer.changed(param)` 参数序列比较——start 组件前按序调用，与上帧 slot.params 比较）+ `start_restartable_group` 的 is_skip 加参数相等条件（slot clean **且参数全等**才 Skip；参数变化即使 clean 也 Enter）。

**用法**（#[composable] 组件内）：
```rust
#[composable]
fn card(ctx, title: &str) {
    let _changed = ctx.changed(&title.to_string());  // 参数声明（start group 前）
    Column::new()...build(ctx, ...);  // title 未变 + slot clean → Skip（content 不重跑）
}
```

**测试**：`test_changed_param_comparison`（机制：首次 true/同参 false/变化 true/回退 true）+ `test_param_equal_skip_integration`（参数未变 Skip、参数变化 Enter）——121 测试全过。

**已知边界**：
- 动态值（`&State`/DerivedValue/闭包）不可比较——依赖追踪决定（State 变化 → slot dirty → Enter）
- Text 等 leaf 组件不参与（内联 ReplaceGroup 语义——随父 scope 重跑，对标 Compose）
- 手动 `ctx.changed` 声明（Rust 无编译器自动生成——Compose 的编译器等价物由用户显式调用）

---

## 阶段 6：组合/布局完整分离 —— 进行中（composition-separation 分支）

**目标**（设计文档"组合树 = 一等公民"）：compose 阶段只构建组合树（Slot 树——持有节点描述）；
布局阶段（materialize）从组合树物化 LayoutNode（arena）——两棵树完全分离。
当前耦合点：`start_node` 同时做组合侧（start_slot）和布局侧（直接建/复用 arena 节点）——
把布局侧移出组合阶段。

### 步骤

**1. Slot 加节点描述字段**：
```rust
struct Slot {
    // ...现有（key/remembered/children/dirty/children_count/is_scope/params）
    desc: Option<NodeDesc>,   // 节点描述（组合产物）——is_scope 或纯组合 Slot 为 None
}
struct NodeDesc {
    key: u64,
    modifier: Modifier,
    policy: Option<Box<dyn MeasurePolicy>>,
    on_remove: Option<Box<dyn FnOnce() + Send>>,
}
```

**2. start_node/end_node 改造（组合侧只写描述）**：
- `start_node`：start_slot + 写 `desc`（modifier/policy/on_remove）到当前 Slot——不碰 arena
- `end_node`：end_slot——不收集 frame_cache（物化期处理）
- `replay_clean_subtree`（Skip）：改——clean 子树物化期恢复缓存（组合期只标记/跳过）
- GROUP_STACK push/pop 保留（组合侧——依赖注册不变）

**3. 新增 materialize()（layout 开头调用）**：
```rust
fn materialize(&mut self) {
    // 遍历 Slot 树（组合树）——对每个节点 Slot（desc.is_some()）：
    //   - key 匹配 prev_node_by_key → 复用 arena 节点（更新 modifier/policy/on_remove；
    //     dirty 按 Slot.dirty；clean → restore_layout 缓存）
    //   - 不匹配 → 新建
    //   - is_scope / 纯组合 Slot → 跳过（不物化）
    // 建树（children 顺序 = Slot 树顺序——递归物化）
    // 回收未复用节点（free——on_remove 触发）
    // 更新 prev_node_by_key / prev_nodes
}
```

**4. layout()：先 materialize() 再 measure_node + place**

**5. 清理**：start_node/end_node 的 arena 操作、frame_cache（如果物化替代）、
`replay_clean_subtree` 的 stub 机制（物化期统一恢复）

### 验证
- 154 测试全过（arena 相关测试调整）
- 动画 demo 各节正常 + 动画平滑（WS 验证）
- 注册数低（重组局部化不变）
- 组合树/布局树结构一致（is_scope 跳过物化的正确性）

### 已知坑
- Slot 树与 arena 树的结构一致性（is_scope Slot 不物化——子挂到最近物化父）
- on_remove 时机（物化期触发——Window 生命周期）
- GROUP_STACK 依赖注册（组合侧不变——节点 key 在组合期已知）
- prev_node_by_key 的 key 匹配（Slot.key 与 arena 节点 slot_key）
- 文本内容变化检测（modifier_text_content_differs——物化期处理 clean 但内容变）

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

- [x] 阶段 2：派生值（`.size(&alpha * 200.0 + 50.0, 30.0)`）——fa477f3 完成
- [x] 阶段 3：`#[composable]` 属性宏（函数 = Group）——1813329 完成
- [x] 阶段 4：布局树独立缓存（LayoutNode 复用）——Part1-6 全部完成（c64f335 free 池测试）
- [x] 阶段 5：参数相等跳过（Stable trait）——877e4e7（changed 机制）+ c53b719（容器组件自动参数暂存）+ 07db7d7（布局层 dirty 修复）
- [x] 文档最终更新（设计 + 使用指南）——composition-system-design.md 第七节（2026-08 现状评估）

---

## 多做的（超出原 checklist 规划）

### 设计文档目标①：依赖注册唯一化（GROUP_STACK——40e9441）
`ACTIVE_SLOT_KEY` + `SCOPE_STACK` 双轨合并为 `GROUP_STACK`（thread_local 统一栈——scope/容器/节点共用）——`State.get()` 注册到栈顶最内层 Group（单一注册目标）；start_node push / end_node pop 配对（组件内读取失效目标 = 节点）；测量阶段栈空回退 ACTIVE_SLOT_KEY。checklist 未规划（只有阶段 2-5），实际完成。

### 交互修复链（大量 bug 修复——非 checklist 内容）
- **真实点击**：Up click 检测嵌套在 Down 块内永不执行（9031ed6）+ slot_key 跨重组匹配（f1edb49）
- **拖动选择**：跨容器 clamp（935638c）、输出文本不可选（09bb4df/871f44b）、compute_selection 纯函数共享（6c51532）、RichText 自选修复（6febd6f——try_current 统一）
- **Window key 加盐**（034a415——结构变化误复用旧槽幽灵节点）+ 多 Window remember 独立（b376167）
- **nest_demo 串位**（6c81f91——start_scope_keyed 双重 push None 覆盖 Some → scope=0 跨函数 key 碰撞）
- **窗口列表消失**（键盘/焦点/列表稳定性相关修复）

### 阶段 4/5 的补全（checklist 未记）
- 阶段 4 Part6（9ecd2e1）：content scope 死代码修复（容器=scope 统一——with_active_scope 移除 NODE_DEPTH 优先）
- 阶段 5 容器组件自动参数暂存（c53b719——checklist 只有手动 changed）+ 布局层 dirty 修复（07db7d7——参数变化 Enter 时置 dirty 防常量折叠返回旧值）

### 测试与工具
- 149 测试全过（winia）+ 8 全过（winia-macros）
- debug_log! 宏（debug-server feature 门控——用户构建零日志）
- nest_demo（多种嵌套演示——嵌套函数/多级 if/match/for/while/深层闭包/结构切换）

### 明确不做（收益边际）
- **组合树与布局树完整分离**（apply_composition 两棵树）——实际收益已通过 arena 复用 + 折叠 + key 匹配达成（与 Compose 组合产物模式一致）——设计文档标注远期
- **阶段 4 的三套缓存合并**（prev_nodes/frame_cache/is_replay_stub）——依赖完整分离——当前机制仍需要
