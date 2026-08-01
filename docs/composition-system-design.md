# 组合系统设计（面向未来）

> 目标：设计一套概念清晰、可扩展、对标 Compose 的组合系统，解决当前
> scope/slot/dirty/Skip/依赖注册多套机制并存、概念混乱的问题。
> 可大改，不拘泥当前框架。

## 一、现状问题清单

当前组合系统经过多轮补丁，积累了多套并行机制，概念混乱：

| 概念 | 现状 | 问题 |
|------|------|------|
| **重组单位** | 节点 slot + content scope 并存 | 粒度不统一：组件级（slot）、闭包级（scope）混用 |
| **依赖注册目标** | `ACTIVE_SLOT_KEY`（节点）+ `SCOPE_STACK`（scope）两套 | 同一个 `State.get()` 在不同位置注册到不同目标，心智负担重 |
| **Skip 判定** | slot 是否 dirty（mark_dirty_path）| 不看参数/输入变化，与 Compose 的"参数相等跳过"语义不同 |
| **缓存机制** | prev_nodes + frame_cache + is_replay_stub + restore_from | 4 种缓存/恢复机制叠加，塌缩 bug 频发（stub 无 policy、路径偏移）|
| **modifier 求值时机** | 构建时（静态）+ 测量时（size Dynamic/闭包）+ 渲染时（background 闭包）| 同一属性多种求值方式 |
| **remember 定位** | 路径哈希编码（FNV-1a）| 与 slot 关系模糊，key 漂移历史问题 |
| **布局树** | LayoutNode 每帧重建（compose 产物）| 与组合树耦合，无法独立缓存 |

**根本症结**：组合（描述"是什么"）与布局（描述"多大/在哪"）耦合，scope 概念是后补的补丁，
导致组合单元（Group/Scope）没有成为一等公民。

## 二、Compose 参考架构

Jetpack Compose 组合系统核心（已确认源码级事实）：

### 1. 组合树（Composition）与 SlotTable
- 每个 `@Composable` 函数调用在组合树中是一个 **Group**
- **SlotTable**：位置化数组，存 Group 元数据、remembered 值、参数值
- Group 按**调用位置**匹配复用（位置 memoization），跨重组持久

### 2. 重组 scope = @Composable 函数调用
- 编译器为每个 `@Composable` 函数生成 `startRestartGroup` → **RecomposeScope**
- 内联函数（Column/Row）用 `startReplaceGroup`（**不产生独立 scope**，属于父 scope）
- **重组单位 = 用户函数**（粒度由用户代码结构决定）

### 3. 依赖追踪（Snapshot）
- 读 MutableState → Snapshot 记录到**当前 scope**
- State 变化 → 精确失效该 scope → Recomposer 调度重跑

### 4. Skip 条件 = 参数相等
- `$composer.changed(param)` 比较参数与上次值
- 相等（且 @Stable/@Immutable）→ **跳过函数体**（保留子树）
- 不等 → 重跑

### 5. remember 存 Group
- remember 值存到当前 Group 的 slot，key 关联
- Group 复用 → remember 保留；Group 移除 → 清除

### 6. 组合与布局分离
- 组合产生 **LayoutNode 描述**（Modifier 链、测量策略）
- Layout 阶段独立测量/放置，**布局节点跨重组复用**（不是每帧重建）

## 三、理想设计

### 核心原则

1. **组合树 = 一等公民**（独立的 Composition Tree，不直接产生布局节点）
2. **重组单位 = Group（组合单元）**——scope 概念内建，非补丁
3. **依赖注册唯一化**——只有"State → Group"一种映射
4. **Skip 语义 Compose 化**——参数相等跳过（@Stable 等价物），而非 dirty 标志
5. **布局树独立缓存**——组合 diff 后增量更新布局节点
6. **modifier 求值统一**——在 Group 内求值，读 State 注册到 Group

### 架构分层

```
┌─────────────────────────────────────┐
│ 组合层 Composition Tree              │
│  Group(key, params, remembered,      │
│        children, dirty)              │
│  依赖: State → Group                 │
└──────────────┬──────────────────────┘
               │ 组合产物（节点描述）
┌──────────────▼──────────────────────┐
│ 布局层 Layout Tree                   │
│  LayoutNode(缓存，跨重组复用)         │
│  测量/放置/绘制                       │
└─────────────────────────────────────┘
```

### 1. Group（组合单元）设计

```rust
struct Group {
    key: u64,              // 调用位置（路径哈希，稳定）
    params: Vec<Value>,    // 参数（用于相等性跳过）
    remembered: HashMap<u64, Box<dyn Any>>,
    children: Vec<Group>,
    dirty: bool,
}
```

**Group 产生方式**：
- **组件 build**（Column/Text/自定义组件）= Group
- **组合函数**（用户自定义 `fn foo(ctx)`）= Group（需显式或宏标记）
- **content 闭包** = Group（当前方案，粒度=闭包）

### 2. 依赖追踪（唯一机制）

```rust
// 唯一的注册目标：当前最内层 Group
thread_local! { static CURRENT_GROUP: ... }  // Group 栈
pub fn register_dependency(state_id: u32) {
    let group = CURRENT_GROUP.top();
    group.deps.insert(state_id);
}
// State 变化 → group.deps 中的 group 失效
```

- **删除** ACTIVE_SLOT_KEY 与 SCOPE_STACK 双轨，统一为"当前 Group"
- 组件内读取 → 注册到组件 Group；组件外（表达式）→ 注册到所在 content/函数 Group

### 3. 重组（Group 失效重跑）

```
State 变化 → 失效其依赖的所有 Group → 调度
重组时：
  对失效 Group：重跑其组合代码
    子 Group：按 key/位置匹配
      参数相等 → Skip（保留子树，不重跑）
      参数不等 → 重跑
```

### 4. 参数相等性（@Stable 等价物）

```rust
trait Stable: PartialEq {}  // 或 derive
// 组件参数实现 Stable → 可跳过
// 动态值（State/闭包）→ 不可比较 → 默认重跑
```

- **静态参数**（String、Color、f32）→ PartialEq → 可跳过
- **动态参数**（&State、派生值）→ 标记"非稳定" → 变化即重跑（依赖已追踪）

### 5. modifier 求值（统一）

```rust
// modifier 在 Group 内求值（组合时）
let group = ctx.current_group();
let m = Modifier::new().size(count.get() * 2, 24);  // count.get() → 注册到 group
node.modifier = m;
// count 变 → group 失效 → 重跑 → modifier 重算 → 应用
```

**不再需要**：size Dynamic/闭包/background 渲染时求值等**多个延迟机制**——
组合重跑天然重算（Compose 语义）。

### 6. 布局树独立

```rust
// 组合 diff 后增量更新
fn apply_composition(root_group: &Group, layout_root: &mut LayoutNode) {
    // 按 key/位置匹配，复用 LayoutNode（不重建）
    // 更新 modifier/测量策略
    // 增删节点
}
```

- 布局节点**跨重组复用**（缓存 measured_size、paragraph 等）
- 组合只描述，布局只执行

### 7. remember 存 Group

```rust
fn remember<T>(ctx, key, init) -> State<T> {
    ctx.current_group().remembered.get_or_init(key, init)
}
```

## 四、Rust 实现路径

### 约束
- 无 `@Composable` 编译器标记 → 需显式/宏产生 Group
- 无自动参数比较 → 需显式 `Stable` 标记或默认策略

### 三种 API 形态（渐进）

**A. builder + content 闭包（**当前方向，已实现**）**
```rust
let w = alpha.get() * 200.0 + 50.0;   // 表达式直接写（无闭包）
let c = Color::from_argb((alpha.get() * 255.0) as u8, 76, 175, 80);
Column::new()
    .modifier(Modifier::new()
        .size(w, 30.0)          // 静态值（content scope 重跑时重算）
        .background(c, Shape::rounded(6.0)))
    .build(ctx, |ctx| {          // content 闭包（用户可接受的唯一闭包形式）
        Text::new(format!("Alpha: {:.2}", alpha.get()))...;
    });
```
- **content 闭包自动是 Group（scope）**：content 内表达式注册到本 scope，
  State 变化 → scope 失效 → content 重跑 → `w`/`c` 重算
- **modifier 不用闭包**（构建时值 + scope 重跑），派生值仅可选补充
- 闭包**只出现在 content**（组件内容），事件回调（on_click）天然闭包
- 粒度 = content 闭包（拆小 content 即细粒度）

**B. `#[composable]` 属性宏（**最优形态，推荐**）**
```rust
#[composable]
fn section2(ctx: &mut ComposeCtx, alpha: &State<f32>) {
    let w = alpha.get() * 200.0 + 50.0;      // 表达式直接写
    let c = Color::from_argb((alpha.get()*255.0) as u8, 76, 175, 80);
    Column::new()
        .modifier(Modifier::new().size(w, 30.0).background(c, ...))
        .build(ctx, |ctx| { ... });          // 或函数参数式（无 content 闭包）
}
```
- **proc-macro 变换**：函数开头注入 `ctx.start_group()`、结尾 `ctx.end_group()` → 函数 = Group
- **无 content 闭包**（可配函数参数式组件 API）、**无显式 scope**（宏注入）
- **粒度 = 函数**（对标 @Composable——函数失效 → 函数体重跑）
- ctx 显式传（Rust 属性宏不能变换调用点，无法隐式 ctx）
- 实现：新 proc-macro crate（`syn`/`quote`，~200 行）

**C. 派生值（可选补充，非必须）**
```rust
.size(&alpha * 200.0 + 50.0, 30.0)   // 运算符表达式（State 运算返回延迟表达式）
```
- 替代"content 内先算 let w"的写法，直接内联
- 不是闭包也不是宏；`size` 统一收 `impl Into<SizeValue>`（静态/Dynamic/派生值）

**D. 函数式宏 `compose!`（远期，语法受限）**
```rust
compose! { Column(ctx, vec![Text(ctx, ...), Box(ctx, ...)]) }
```
- 宏解析块 → 组合调用，但宏内 Rust 语法受限，不如属性宏灵活

### 推荐路线
1. **阶段 1（当前分支，已完成）**：A（builder + content 闭包自动 scope）——表达式不用闭包，验证机制
2. **阶段 2（推荐）**：B（`#[composable]` 属性宏）——函数 = Group，无 content 闭包、无显式 scope、粒度=函数
3. **阶段 3（可选）**：C（派生值运算符）——`&alpha * 200.0 + 50.0` 内联替代 `let w`，非闭包非宏
4. **阶段 4**：布局树独立缓存（组合 diff → 增量更新 LayoutNode）
5. **阶段 5**：参数相等性跳过（Stable trait）——真正 Compose 式 Skip
6. **远期（可选）**：D（compose! 函数式宏）——语法受限，仅当声明式块需求高

## 五、关键权衡

| 决策 | 选项 | 权衡 |
|------|------|------|
| 重组单位 | content 闭包（A）vs 用户函数（B）| A 简单粒度粗；B 复杂粒度细（推荐）|
| Skip 判定 | dirty 标志（现状）vs 参数相等（Compose）| 参数相等需 Stable 标记，但语义正确 |
| 布局树 | 每帧重建（现状）vs 独立缓存 | 独立缓存省测量/paragraph，改动大 |
| modifier 求值 | 多机制（现状）vs 组合时统一 | 统一依赖组合重跑，需 Group 架构 |
| 依赖注册 | 双轨（现状）vs 唯一 Group | 唯一化是核心清理，必做 |

## 六、结论

**方向**：以"组合树 = 一等公民 + Group = 重组单位 + 唯一依赖注册"为骨架，
渐进重构（A→B→C），最终达到：
- `count.get() * 2` 这种表达式**直接可用**（不用闭包/scope/动态 size）
- 重组粒度由**用户函数结构**决定（对标 @Composable）
- 布局树跨重组复用（性能）
- 概念单一（Group、依赖、失效）

**当前 scope-research 分支的成果**（content 自动 scope）是阶段 1 的可行验证；
阶段 2（composable_fn 宏）是下一个自然步骤。
