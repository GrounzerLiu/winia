# Winia v2 — 架构设计文档

> ⚠ 早期设计稿，部分内容过时。权威现状见 `docs/handover.md`；组件/Modifier 对齐清单见 `docs/component-gap-analysis.md`；本文"设计原则现状对照"章节已随 compose-core 更新。
> 版本: 0.2.0  
> 状态: 设计阶段  
> 目标: 基于 winit + skia-safe 的声明式跨平台 GUI 框架，架构对标 Jetpack Compose

---

## 一、项目哲学

### 为什么重构？

Winia v1 (D:\winia) 是一个功能可用的原型，但存在以下结构性问题：

| 问题 | 影响 |
|------|------|
| `ItemData` 25+ 字段，职责混乱 | 布局/状态/动画/事件全耦合在一个 struct |
| `ItemEvent` 22 个字段，1100+ 行 | 事件系统无法扩展，`impl_noop!` 宏难以调试 |
| `Shared<T>` 用 PhantomData 区分读写 | 类型别名爆炸（SharedF32, SharedBool...），tokio 强依赖 |
| 缺少组合引擎层 | 状态变化直接触发全量重算，无批处理/依赖追踪 |
| 布局与绘制耦合 | Flex 布局 60000 行，测量/布局/绘制逻辑交错 |
| `unwrap()`/`panic!()` 泛滥 | 渲染路径 crash = 整个窗口消失 |

### 设计原则

1. **声明式优先** — 用户描述 UI 是什么，框架处理如何渲染
2. **分层清晰** — 每层有明确的职责边界，可独立测试
3. **纯 Rust** — 不使用宏 DSL，Builder + 闭包实现声明式 API
4. **零魔法** — 不依赖 proc-macro 做语法变换（现有 proc-macro 仅用于属性生成）
5. **增量更新** — 状态变化只触发受影响的 composable 重组
6. **错误可恢复** — 崩溃边界，渲染失败不影响窗口存活

### 设计原则现状对照（2026-08，compose-core 分支）

> 上述原则是**设计意图**。演进后部分偏离，如实记录，避免后来者按过时原则推断现状。

| 原则 | 现状 | 说明 |
|------|------|------|
| 1 声明式优先 | ✅ 落实 | Builder + 闭包 + `ComposeCtx`，组合/物化/布局三层分离 |
| 2 分层清晰 | ⚠️ 部分 | 物化器已拆出（`core/materialize.rs`）；`app.rs`（事件+焦点+选择+IME）与 `composer.rs`（组合+依赖调度）仍偏大 |
| 3 纯 Rust / 不用宏 DSL | ❌ 已偏离 | `#[composable]` 过程宏（`winia-macros`）是组合 API 的核心——嵌套 composable 靠它注入语句上下文；"宏最小化"为当前口径 |
| 4 零魔法 | ❌ 已偏离 | 宏 + thread_local 依赖追踪（`DEP_BUFFER`/`DEP_MODE`——P2-3 已去裸指针，数据缓冲方案）；依赖注册是隐式桥接 |
| 5 增量更新 | ✅ 落实 | slot 级 dirty + Skip 子树恢复 + 两段式依赖（组合依赖→重组 / 布局依赖→只重测不重组） |
| 6 错误可恢复 | ❌ 未落实 | 渲染/测量路径 panic 直接崩窗口，无崩溃边界（cleanup-plan P3-3 待办） |

---

## 二、整体分层

```
┌──────────────────────────────────────────────────────────┐
│                    App / Platform                         │
│  run_app(), winit 集成, 窗口生命周期, 事件循环              │
├──────────────────────────────────────────────────────────┤
│                    UI Components                          │
│  Button, Text, TextField, Slider, Checkbox, Switch, ...   │
│  纯 composable 函数，不包含布局逻辑                         │
├──────────────────────────────────────────────────────────┤
│                    Modifier System                        │
│  size, padding, background, border, clip, clickable, ...  │
│  不可变链式调用，解耦外观/行为/布局                          │
├──────────────────────────────────────────────────────────┤
│                    Layout System                          │
│  Column, Row, Box, LazyColumn, 自定义 MeasurePolicy        │
│  标准三阶段: Constraints → Measure → Place → Draw          │
├──────────────────────────────────────────────────────────┤
│                 Composition Engine                        │
│  SlotTable / Composer / Recomposer / ComposeCtx            │
│  增量更新、生命周期、状态依赖追踪、重组调度                   │
├──────────────────────────────────────────────────────────┤
│                   Snapshot State                          │
│  State<T> — 可观察值容器，读自动追踪依赖，写触发重组         │
│  derived() — 派生状态                                      │
│  remember() — 在组合中持久化状态                            │
├──────────────────────────────────────────────────────────┤
│              Render / Platform                            │
│  skiwin (Skia + Vulkan/GL/CPU 渲染后端)                    │
│  material_color_utilities (Material Design 颜色工具)        │
└──────────────────────────────────────────────────────────┘
```

### 各层职责

| 层 | 核心文件 | 职责 |
|----|---------|------|
| State | `core/state.rs` | 响应式值容器，依赖追踪，变化通知 |
| Composition | `core/composer.rs` | 组合树管理，SlotTable，重组调度 |
| Modifier | `modifier/mod.rs` | 链式修饰符抽象，Layout/Draw/Input 三类 |
| Layout | `layout/*.rs` | Constraints，MeasurePolicy trait，Column/Row/Box/LazyColumn |
| Components | `ui/*.rs` | 具体的 composable 函数（Button, Text 等） |
| Render | `render/*.rs` | Skia 绘制桥接，脏区域追踪 |
| Input | `input/*.rs` | 事件枚举，命中测试，手势识别，焦点管理 |
| Animation | `animation/*.rs` | `animateFloatAsState`, `AnimatedVisibility`, 过渡 |
| Theme | `theme/*.rs` | Material Theme，Colors，Typography，Shapes |
| App | `app/*.rs` | `run_app()` 入口，winit 窗口封装 |

---

## 三、核心系统设计

### 3.1 State 响应式系统

**类比**: Compose 的 `MutableState<T>` + `derivedStateOf`

```
用户代码                     框架内部
─────────                    ────────
ctx.remember(|| 0)     →    SlotTable 存储 State<T>
state.get()             →    thread-local 追踪依赖
state.set(1)            →    通知 Composer 标记 dirty
                          →  下一帧批量重组
```

**关键决策**:
- 使用 `thread_local!` 而非显式传参来追踪依赖——让 `State::get()` 在 composable 函数中自动生效
- 使用 `PartialEq` 去重——`set()` 值未变化时跳过通知
- `Clone` 廉价（Arc clone），可安全地在闭包间传递
- 不再区分 Source/Derived 类型参数，统一为 `State<T>`
- 不依赖 tokio（异步功能作为 optional feature）

**API**:

```rust
// 创建
let count = ctx.remember(|| 0i32);         // 组合内状态
let name = State::new(String::new());       // 外部状态，可传入 composable

// 读写
let v = count.get();                        // 读，自动追踪依赖
count.set(10);                              // 写，PartialEq 去重
count.update(|v| *v += 1);                  // 原地更新，始终通知

// 派生
let doubled = count.derive(|v| v * 2);      // 自动追踪 count 的变化
```

### 3.2 Composition 引擎

**类比**: Compose 的 Composer + SlotTable

**核心数据结构**: SlotTable（槽位表）

```
组合过程:
  Composer.compose(|ctx| {
      ctx.start_node(key=1)          // 写入 SlotTable[0] → Group(key=1)
          ctx.remember(|| "hello")   //    在 SlotTable[0] 中存状态
          ctx.start_node(key=2)      //    写入 SlotTable[1] → Group(key=2)
              ctx.remember(|| 42)    //        在 SlotTable[1] 中存状态
          ctx.end_node()             //    current++
      ctx.end_node()                 // current++
  })
```

**重组算法**（简化版）:

1. `State.set()` → 通知 `Composer.request_recomposition()`
2. Recomposer 批量收集 → 下一帧执行
3. `Composer.recompose()` → 从根开始重新遍历
4. SlotTable.reset() → 指针归零
5. 重新执行 composable 函数，复用 key 匹配的 Slot
6. 多余 Slot 被 truncate() 清理
7. 只对变化的 LayoutNode 执行 `measure()` + `place()` + `draw()`

**待优化**: 当前原型使用 `Vec<Slot>` + 索引指针。后续迁移到 **Gap Buffer** 实现 O(1) 插入/删除。

### 3.3 Modifier 系统

**类比**: Compose 的 `Modifier` 链

**核心抽象**:

```rust
pub trait ModifierNode {
    /// 在测量约束上施加限制（LayoutModifier）
    fn measure(&self, constraints: Constraints, next: &dyn MeasureFn) -> Size;
    
    /// 调整子节点位置（LayoutModifier）
    fn place(&self, position: Point, next: &dyn PlaceFn);
    
    /// 在绘制前后插入逻辑（DrawModifier）
    fn draw(&self, canvas: &Canvas, bounds: Rect, next: &dyn DrawFn);

    /// 处理输入事件（PointerInputModifier）
    fn on_pointer_event(&self, event: &PointerEvent) -> bool;
}
```

**链式语义**（从左到右 = 从外到内）:

```rust
Modifier::size(100, 100)     // ① 最外层: 限制盒子 100×100
    .padding(10)              // ② 往内: 留 10px 边距
    .background(Color::RED, RoundedCornerShape(8))  // ③ 最内层: 红底圆角
    .clickable(on_click)      // ④ 点击区域 = ② 的 inner bounds
// 内容绘制在 ③ 之上
```

**三类 Modifier**:

| 类型 | 示例 | 影响阶段 |
|------|------|---------|
| LayoutModifier | size, padding, margin, fillMaxWidth | Measure + Place |
| DrawModifier | background, border, clip, shadow | Draw |
| PointerInputModifier | clickable, focusable, scrollable | Input |

### 3.4 Layout 系统

**三阶段**: Constraints → Measure → Place → Draw

```
1. Constraints: 父节点告诉子节点可用空间范围 (min/max width/height)
2. Measure:    子节点根据 Constraints 计算所需尺寸
3. Place:      父节点为子节点分配位置 (x, y)
4. Draw:       按顺序绘制
```

**核心类型**:

```rust
#[derive(Copy, Clone)]
pub struct Constraints {
    pub min_width: f32,
    pub max_width: f32,
    pub min_height: f32,
    pub max_height: f32,
}

pub trait MeasurePolicy {
    fn measure(&self, children: &[LayoutNode], constraints: Constraints) 
        -> (Size, Vec<Placement>);  // 返回自身尺寸 + 每个子节点的位置
}
```

**布局原语**:

| 原语 | Compose 对应 | 功能 |
|------|-------------|------|
| Column | Column | 垂直排列子元素 |
| Row | Row | 水平排列子元素 |
| Box | Box | 层叠子元素（Z 轴） |
| LazyColumn | LazyColumn | 虚拟滚动列表（仅渲染可见项） |
| 自定义 | Layout composable | 实现 MeasurePolicy trait |

### 3.5 事件 / 输入系统

**分层事件处理**:

```
winit WindowEvent
    │
    ▼
Event Adapter  ──→  内部 UiEvent 枚举
    │
    ▼
Hit Test  ──→  确定事件目标（按 LayoutNode 的 bounds 碰撞检测）
    │
    ▼
Event Dispatch  ──→  冒泡/捕获传播
    │
    ▼
Gesture Detector  ──→  识别手势（单击/双击/长按/滑动/拖拽）
    │
    ▼
Focus Manager  ──→  Tab 键焦点链
```

**统一事件枚举**（替代 v1 的 22 个字段）:

```rust
pub enum UiEvent {
    Pointer(PointerEvent),     // 合并 click_input + pointer_button + pointer_moved + cursor_move
    Key(KeyEvent),             // keyboard_input
    Focus(FocusEvent),         // focus_changed
    Scroll(ScrollEvent),       // mouse_wheel
    Ime(ImeEvent),             // ime_input
}
```

### 3.6 用户 API（最终形态）

```rust
use winia::prelude::*;

fn counter(ctx: &mut ComposeCtx) {
    let count = ctx.remember(|| 0i32);

    Column::new()
        .modifier(Modifier::fill_max_size().padding(16.0))
        .arrangement(Arrangement::Center)
        .alignment(Alignment::CenterHorizontally)
        .build(ctx, |ctx| {
            // 标题
            Text::new(format!("Count: {}", count.get()))
                .style(TextStyle::headline_large())
                .build(ctx);

            Spacer::vertical(16.0).build(ctx);

            // 按钮
            Button::new()
                .on_click({ let count = count.clone(); move || count.update(|v| *v += 1) })
                .build(ctx, |ctx| {
                    Text::new("Increment").build(ctx);
                });

            // 条件渲染
            if count.get() > 0 {
                Spacer::vertical(8.0).build(ctx);
                OutlinedButton::new()
                    .on_click(move || count.set(0))
                    .build(ctx, |ctx| {
                        Text::new("Reset").build(ctx);
                    });
            }
        });
}

fn main() {
    run_app(counter)
        .title("Counter")
        .size(400, 300);
}
```

### 3.7 与 Jetpack Compose 的对照

| Compose | Winia v2 | 差异 |
|---------|---------|------|
| `MutableState<T>` | `State<T>` | 概念一致，实现用 RwLock 而非 snapshot |
| `remember { }` | `ctx.remember(|| ...)` | 需要 ctx 参数（Rust 无隐式 receiver） |
| `derivedStateOf { }` | `state.derive(|| ...)` | 同步语义 |
| `Modifier.xxx()` | `Modifier::xxx()` → `.modifier(m)` | 链式 API |
| `Column { }` | `Column::new().build(ctx, \|ctx\| { })` | 需要 `.build()` 终结器 |
| `LaunchedEffect` | `ctx.launch_effect(key, ...)` | 基于 async/await |
| `@Composable` 函数 | 普通 Rust 函数，接收 `&mut ComposeCtx` | 无特殊标记 |
| Snapshot 系统 | 简化版（单线程，无 MVCC） | 无需并发快照隔离 |

### 3.8 ScrollState — 滚动控制

**类比**: Compose 的 `ScrollState`

`ScrollState` 是 Winia 中可滚动容器的状态句柄。它存储当前滚动偏移并提供编程滚动能力。

```rust
/// 创建 ScrollState（须在 composable 中用 remember 保持）
let scroll_state = ctx.remember(|| ScrollState::new()).get();

/// 应用于可滚动容器
Column::new()
    .modifier(Modifier::new()
        .size(200.0, 150.0)
        .vertical_scroll(scroll_state))  // ← 绑定 ScrollState
    .build(ctx, |ctx| {
        for i in 0..30 {
            Text::new(format!("Line {}", i)).build(ctx);
        }
    });
```

**核心字段**:
| 字段 | 类型 | 说明 |
|------|------|------|
| `offset` | `State<f32>` | 当前滚动偏移量（像素），可读写 |
| `is_scroll_in_progress` | `State<bool>` | 是否正在滚动 |

**方法**:
- `ScrollState::new()` — 创建偏移为 0 的滚动状态
- `scroll_to(value, max_offset)` — 立即跳到指定位置（自动 clamp 到 `[0, max_offset]`）

**注意事项**:
- `ScrollState` 不是 `State`，它是内部包含 `State<f32>` 的容器。需要 `ctx.remember().get()` 获得克隆（廉价 Arc clone）。
- 滚动偏移由 `MouseWheel` 事件自动更新，不需要手动管理。
- 渲染时自动应用 `canvas.translate(0, -offset)` 进行视口平移。
- 命中测试 (`hit_test`) 自动加上 scroll offset，保证点击坐标正确映射。

### 3.9 remember 与 remember_at_key — 状态持久化

**类比**: Compose 的 `remember { mutableStateOf(...) }`

`remember` 是 Winia 中跨组合（compose）持久化状态的核心原语。每次调用 `compose()` 时，slot 系统会匹配 key，返回上一次的同一个实例。

```rust
// 基本用法
let count = ctx.remember(|| 0i32);

// 状态管理
let show_window = ctx.remember(|| false);
let scroll_state = ctx.remember(|| ScrollState::new()).get();
let focus_req = ctx.remember(|| FocusRequester::new()).get();
```

**内部机制**:
1. `remember` 为每个调用分配递增的 `remember_counter` 作为 slot key
2. 在 SlotTable 中查找 key → 存在则返回已有的 `State<T>`
3. 不存在则执行 `init` 创建新 `State<T>` 并存入 slot
4. slot 跨 compose 保持，直到对应的 composable 被移除

**跨分支持久化陷阱**:

`remember` 的 key 由 `remember_counter` 决定。如果同一个 composable 在不同分支中（如 `if`/`else`）的 `remember` 调用次数不同，后续 compose 中 key 会偏移，导致状态丢失。

```rust
// ❌ 问题代码：show_alt 分支改变 remember_counter
let show_window = ctx.remember(|| false);
if show_alt.get() {
    ctx.remember(|| "alt");  // ← 消耗了一个 key
} else {
    // else 分支没有 remember，key 少了一个
    Button::new()...build(ctx, |ctx| { ... });  // ← Button 内部有 remember
}
// 这里 ctx.remember(|| 0u64) 的 key 取决于 show_alt →
// 结果：每次 show_alt 变化时，这个 remember 得到不同的 key，
// 导致旧的 State 找不到，值重置为 0！
```

**解决方案：`remember_at_key`**

```rust
// ✅ 固定 key 存储，不受分支影响
let created_id = ctx.remember_at_key(u64::MAX, || 0u64);
```

`remember_at_key(key, init)` 使用指定的固定 key 存储状态，而非自动递增的 `remember_counter`。适合需要跨分支稳定持久化的关键值，如窗口句柄、组件 ID 等。

**何时使用**:
| 场景 | 使用 |
|------|------|
| 普通状态（计数、开关） | `ctx.remember(|| ...)` |
| 跨分支稳定持久化 | `ctx.remember_at_key(fixed_key, || ...)` |
| 引用外部 State | 直接传入 `State::new(...)` |


---

## 四、模块目录结构

```
winia/
├── Cargo.toml
├── src/
│   ├── lib.rs                    # 公开 re-export
│   │
│   ├── core/                     # 运行时核心
│   │   ├── mod.rs
│   │   ├── state.rs              # State<T>, derive(), 依赖追踪
│   │   └── composer.rs           # ComposeCtx, Composer, SlotTable
│   │
│   ├── modifier/                 # 修饰符系统
│   │   ├── mod.rs                # Modifier 链, ModifierNode trait
│   │   ├── layout_modifier.rs    # size, padding, margin, fillMax...
│   │   ├── draw_modifier.rs      # background, border, clip, shadow
│   │   └── input_modifier.rs     # clickable, focusable, scrollable
│   │
│   ├── layout/                   # 布局系统
│   │   ├── mod.rs
│   │   ├── constraints.rs        # Constraints
│   │   ├── node.rs               # LayoutNode
│   │   ├── policy.rs             # MeasurePolicy trait
│   │   ├── column.rs             # Column
│   │   ├── row.rs                # Row
│   │   ├── box.rs                # Box
│   │   └── lazy_column.rs        # LazyColumn（虚拟滚动）
│   │
│   ├── ui/                       # UI 组件 (composable 函数)
│   │   ├── mod.rs
│   │   ├── text.rs               # Text
│   │   ├── button.rs             # Button, OutlinedButton, TextButton
│   │   ├── text_field.rs         # TextField
│   │   ├── checkbox.rs           # Checkbox（v2 新增）
│   │   ├── switch.rs             # Switch（v2 新增）
│   │   ├── slider.rs             # Slider
│   │   ├── icon.rs               # Icon
│   │   ├── image.rs              # Image
│   │   ├── divider.rs            # Divider
│   │   ├── progress.rs           # LinearProgress, CircularProgress
│   │   └── scaffold.rs           # Scaffold（v2 新增）
│   │
│   ├── input/                    # 输入系统
│   │   ├── mod.rs
│   │   ├── event.rs              # UiEvent 枚举
│   │   ├── hit_test.rs           # 命中测试
│   │   ├── gesture.rs            # GestureDetector trait
│   │   └── focus.rs              # FocusManager
│   │
│   ├── render/                   # 渲染
│   │   ├── mod.rs
│   │   ├── skia.rs               # Skia 绘制管道
│   │   └── damage.rs             # 脏区域追踪
│   │
│   ├── theme/                    # 主题
│   │   ├── mod.rs
│   │   ├── colors.rs             # 基于 material_color_utilities
│   │   ├── typography.rs         # TypeScale
│   │   └── shapes.rs             # Shape 定义
│   │
│   ├── animation/                # 动画
│   │   ├── mod.rs
│   │   ├── core.rs               # animateFloatAsState, Animation<T>
│   │   └── transition.rs         # AnimatedVisibility, 进入/退出过渡
│   │
│   └── app/                      # 应用壳
│       ├── mod.rs
│       ├── window.rs             # winit Window 封装
│       └── runtime.rs            # run_app() 入口
```

---

### 3.8 增量重组（v2 最新）

**类比**：Compose 的 Positional Memoization

```
State:count.set(1) → notify_state_changed
  → Composer.compose() 消费 PENDING_STATES
  → 查 slot_deps[state_id] → 标记 SlotTable.dirty_keys
  → 重组时 start_slot 检查 dirty → clean 跳过 composable 执行
```

**三层脏追踪**：

| 层 | 位置 | 说明 |
|----|------|------|
| 全局脏标志 | `State::notify → set_global_dirty` | AtomicBool，快速检查 |
| State→Slot 依赖 | `get() → registrar → record_dep` | 知道哪个 slot 读了哪个 state |
| Slot 跳过 | `SlotTable::start_slot` 检查 `dirty_keys` | clean 分支不执行 |

### 3.9 焦点系统（v2 最新）

- **FocusRequester** — `ctx.remember(|| FocusRequester::new()).get()`，`request_focus()` 请求焦点
- **Modifier.focus_requester(&fr)** — 不消耗所有权（`From<&FocusRequester>`）
- **Tab 键遍历** — `focus_next()` 深度优先 + `focus_next` 事件
- **焦点持久化** — `AppState.focused_id: Option<u64>` 跨 compose 保持，compose 后 `focus_by_id` 恢复
- 文档：`docs/handover.md` §2.7（输入与焦点）

### 3.10 动画系统（v2 最新）

- `animate_as_state(initial, target, duration, easing)` → `State<f32>`，每帧自动插值
- `animate_to(state, target, duration, easing, on_finish)` — 改变目标+完成回调
- 5 种缓动 + 全局动画列表 + `tick()` 每帧推进 + 自动 request_redraw

### 3.11 DevTools（v2 最新，可选 feature）

- `debug-server` feature flag，默认不启用
- 本地 HTTP 服务器 `http://localhost:9999`
- `/screenshot` BMP 按需截图，`/tree` 组件树 JSON，`/click?x&y` 模拟点击
- `/shutdown` 优雅退出，`/event?type=...` 通用事件
- EventLoopProxy 唤醒 + ControlFlow::Poll 响应

---

## 五、开发路线图

### Phase 1 — State + ComposeCtx 核心 ✅ 进行中

- [x] `State<T>` 响应式容器（读追踪、写通知、PartialEq 去重）
- [x] `ComposeCtx` + `Composer` 骨架（SlotTable、key 管理）
- [ ] `ctx.remember()` 功能完成
- [ ] 单元测试：State 读写通知、remember 跨重组保持

### Phase 2 — Modifier 链

- [ ] `Modifier` 链式结构
- [ ] `LayoutModifier`: size, padding, margin, fillMaxWidth/Height
- [ ] `DrawModifier`: background, border, clip
- [ ] `PointerInputModifier`: clickable

### Phase 3 — Layout 布局

- [ ] `Constraints` 约束模型
- [ ] `LayoutNode` + `MeasurePolicy`
- [ ] `Column` / `Row` / `Box`
- [ ] 完整的 Measure → Place → Draw 管道

### Phase 4 — 基础组件

- [ ] `Text`
- [ ] `Button`
- [ ] `Column` / `Row` / `Box` 集成
- [ ] 首个可运行示例（Counter）

### Phase 5 — 输入事件

- [ ] `UiEvent` 统一枚举
- [ ] 命中测试
- [ ] 事件分发（冒泡/捕获）
- [ ] 手势检测器

### Phase 6 — 更多组件 + 主题

- [ ] `TextField`, `Slider`, `Checkbox`, `Switch`
- [ ] `Scaffold`, `LazyColumn`
- [ ] Material Theme 完整集成
- [ ] 动画系统

### Phase 7 — 清理与稳定

- [ ] 删除 v1 遗留代码
- [ ] 文档完善
- [ ] 示例项目
- [ ] 性能基准

---

## 六、关键设计决策记录

| 决策 | 选择 | 理由 |
|------|------|------|
| 不用宏 DSL | ✅ | IDEA 难以展开 proc-macro，纯 Rust Builder 可读性足够 |
| Builder 模式做主力 | ✅ | IDE 补全完美，编译错误清晰，零学习成本 |
| thread_local 追踪依赖 | ✅ | 避免在 composable 函数签名中添加额外参数 |
| RwLock 而非 Mutex | ✅ | State 读多写少，RwLock 性能更好 |
| 不需要 Snapshot/MVCC | ✅ | 单线程 UI 无需并发快照隔离，简化实现 |
| SlotTable 用 Vec 起步 | ✅ | 先跑通逻辑，后续升级 Gap Buffer |
| 不依赖 tokio（默认） | ✅ | 减小二进制体积，异步作为 optional feature |

---

## 七、Workspace 结构

```
D:\Projects\winia\          # workspace root
├── Cargo.toml              # [workspace] 配置
├── docs/                   # 本文档目录
├── winia/                  # UI 框架核心库
├── skiwin/                 # Skia 渲染后端（从 v1 复制）
├── proc-macro/             # 过程宏（从 v1 复制）
└── material_color_utilities/  # Material Design 颜色工具（从 v1 复制）
```

### 依赖关系

```
winia ──→ proc-macro (path)
     ├──→ skiwin (path, features: vulkan/gl)
     └──→ skia-safe, parking_lot, log, thiserror

skiwin ──→ skia-safe, winit, softbuffer, ash/vulkano/glutin (optional)
```
