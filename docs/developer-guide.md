# Winia v2 开发者指南

> ⚠ 部分内容过时（项目结构/依赖版本/调试通道以当前代码为准）。最新接手与调试指南见 `docs/handover.md`。
> 让后来者快速上手的实操文档。假设读者已有 Rust 基础，不假设 GUI 框架经验。
>
> 适用版本: 0.2.0 · 最后更新: 2026-07-21

---

## 一、一分钟了解项目结构

```
D:\Projects\winia\
├── winia/            ← UI 框架核心
│   ├── src/
│   │   ├── lib.rs        # 公开 re-export
│   │   ├── app.rs        # 入口 + 事件循环
│   │   ├── core/
│   │   │   ├── composer.rs  # Composer + SlotTable + ComposeCtx
│   │   │   └── state.rs     # State<T> 响应式值
│   │   ├── modifier.rs   # Modifier 链 + FocusRequester + ScrollState
│   │   ├── layout.rs + layout/*.rs  # 布局引擎 + 3 种布局策略
│   │   ├── ui.rs + ui/    # Window / Button / Text / Column / Row / Stack
│   │   ├── render.rs     # Skia 渲染管线
│   │   ├── animation.rs  # 动画系统
│   │   ├── input.rs + input/focus.rs  # 焦点管理
│   │   └── debug.rs      # DevTools HTTP 服务器（可选 feature）
│   └── examples/counter.rs  # 主要示例
│
├── skiwin/           ← Skia 渲染后端（Vulkan/GL/CPU）
└── docs/             ← 文档
```

**核心依赖**:
- `winit 0.31` — 窗口创建和事件循环
- `skia-safe 0.99` — Skia 图形 API（Rust 绑定）
- `softbuffer 0.4` — CPU 回退渲染

---

## 二、核心概念

### 2.1 什么是 Composable？

Composable 就是一个**接收 `&mut ComposeCtx` 的普通 Rust 函数**，没有特殊标记：

```rust
fn counter_ui(ctx: &mut ComposeCtx) {
    let count = ctx.remember(|| 0i32);
    Text::new(format!("Count: {}", count.get())).build(ctx);
}
```

- Composable 在每次状态变化时**重新执行**（重组）
- 通过 `ctx.remember()` **持久化**状态跨重组
- 组件用 Builder + `.build(ctx)` 终结

### 2.2 三阶段渲染管线

每帧按顺序执行：

```
compose()  →  layout()  →  render()
   ↑              │            │
   └── State.set() 触发 dirty   │
                                ▼
                          Skia Canvas
```

1. **Compose** — 执行 user composable 函数，构建 SlotTable + LayoutNode 树
2. **Layout** — 递归测量/放置 LayoutNode（`measure_node`）
3. **Render** — 遍历 LayoutNode 树，用 Skia Canvas 绘制

### 2.3 State — 响应式状态

```rust
// 创建（必须通过 ctx.remember 持久化）
let count = ctx.remember(|| 0i32);

// 读 — 自动追踪依赖
let v = count.get();

// 写 — 触发 dirty，下一帧重组
count.set(42);
count.update(|v| *v += 1);
```

**关键机制**:
- `State::get()` 通过 thread-local 记录"谁读了这个值"
- `State::set()` 设全局 dirty 标志 + 记录变化 state id
- `Composer::compose()` 消费 dirty → 标记对应 slot → 重组时只执行脏 slot

---

## 三、渲染管线详解

### 3.1 完整的帧流程

从 winit 事件开始：

```
winit: RedrawRequested
  │
  │  app.rs L181
  ▼
pw.composer.compose(|ctx| (pw.content)(ctx))
  │  composer.rs L331
  │  1. slot_table.reset() + clear layout_nodes
  │  2. consume PENDING_STATES → mark dirty slots
  │  3. execute content(ctx) — user composable 运行
  │  4. slot_table.truncate() — 清理未访问的 slot
  ▼
pw.composer.layout(Constraints::new(0, w, 0, h))
  │  composer.rs L381
  │  measure_node(root, constraints)
  │  → 遍历 modifier → 递归 children → 设置 measured_size + position
  ▼
sw.draw(|surface| {
    canvas.clear(WHITE);
    canvas.scale(sf, sf);          // HiDPI 缩放
    render::render(root, canvas);   // 两阶段渲染
})
  │  render.rs L34
  │  Phase 1: 背景 → 边框 → 文本 → 焦点环 → 子节点
  │  Phase 2: backdrop blur (snapshot + blur + blit)
  ▼
检查 DevTools 事件 / 动画 tick / pending close
```

### 3.2 Layout 如何工作

核心函数：`measure_node`（`layout/column.rs L178`）

```
输入: LayoutNode + Constraints
                  │
  遍历 modifier.elements() 处理:
  ┌── Size:      constraints.tighten_width/height(value)
  │── Padding:   constraints.offset(-pad*2) + 记录 pad_x/pad_y
  │── FillMax:   constraints.min_width = constraints.max_width
  └── Scroll:    给子节点无限约束
                  │
  ↓ 有 measure_policy? ──→ policy.measure(children, inner)
        │                      ↓
        │               policy.place(children, placements)
        │                      ↓
        │               添加 padding offset
        │                      ↓
        └──→ 设置 node.measured_size
```

**Column 布局策略**:
1. 子节点约束: `max_width = inner_max`，`max_height = remaining`
2. 第一次遍历测量每个子节点
3. 计算主轴总高度 → spacing
4. 第二次遍历设置 placement (x, y)

### 3.3 Render 如何绘制

```
render(root, canvas)
  │
  └─ render_pass1: 递归遍历 LayoutNode 树
       │
       ├── 取出 scroll offset → canvas.translate(0, -dy)
       ├── 处理 Background → draw_background()
       ├── 处理 Border → draw_border()
       ├── 记录 TextContent → 绘制文字
       ├── 记录 Blur → saveLayer + 模糊 paint
       ├── 记录 BackdropBlur → 收集 region
       ├── 绘制焦点环 → draw_focus() 蓝色边框
       ├── scroll clip → canvas.clipRect
       └── 递归子节点
            │
  └─ render_backdrop_blur: 单次 snapshot + 逐 region blur
       │
       ├── 计算所有模糊区域的并集 bounds
       ├── surface.image_snapshot_with_bounds(bounds)
       ├── 对每个 region:
       │     draw_image_rect(blurred) + 递归子节点
       └── restore canvas
```

---

## 四、事件处理

### 4.1 事件流

```
winit WindowEvent
    │
    ├── PointerButton ──→ hit_test → 找 Clickable → on_click()
    │                          │
    │                          └── count.update() → set_global_dirty()
    │                              request_redraw() → ControlFlow::Poll
    │                              sync focused_id
    │
    ├── KeyboardInput(Tab) ──→ focus_next() → request_redraw() + Poll
    │
    ├── MouseWheel ──→ apply_scroll_delta() → request_redraw()
    │
    ├── RedrawRequested ──→ compose → layout → render → DevTools
    │
    ├── CloseRequested ──→ on_close() → remove → exit if last
    │
    ├── ScaleFactorChanged ──→ pw.scale_factor = new
    │
    └── SurfaceResized ──→ pw.width/height = new
```

### 4.2 DevTools 事件（debug-server feature）

DevTools 事件通过 HTTP 接口注入：

```
HTTP /click?x=100&y=50
  │
  └→ simulate_native_click() → QUEUED_EVENTS.push
     → wake() → proxy_wake_up → request_redraw()
     → RedrawRequested → take_queued_events()
     → hit_test(root, x/sf, y/sf) → 找 Clickable → on_click()
```

### 4.3 hit_test 算法

```rust
fn hit_test_recursive(node, x, y, parent_x, parent_y, path) {
    let nx = parent_x + node.position.x;
    let ny = parent_y + node.position.y;
    // 排除超出边界的点
    if x < nx || x > nx + nw || y < ny || y > ny + nh { return false; }

    path.push(node);

    // 计算 scroll 偏移
    let (scroll_dx, scroll_dy) = scroll_offset_for_node(node);

    // 深度优先：检查子节点
    for child in &node.children {
        if hit_test_recursive(child, x, y, nx - scroll_dx, ny - scroll_dy, path) {
            return true;  // ← 找到第一个匹配就返回！
        }
    }
    true
}
```

**关键陷阱**：hit_test 找到第一个子节点就返回 `true`，不再检查同级兄弟。所以点击在按钮**边界**上可能命中前一个兄弟。使用鼠标实际点击（按钮中央）不会遇到此问题。

---

## 五、状态更新与重组

### 5.1 完整更新链

```
User clicks button
    │
    ▼
on_click() 执行
    │  ├── count.set(10) → notify()
    │  │     ├── notify_state_changed(state_id)
    │  │     ├── set_global_dirty()
    │  │     └── 遍历 subscribers 调用 callback
    │  │
    │  └── btn2.request_focus() → 直接修改 LayoutNode.focused
    │
    ▼
request_redraw() → 切 ControlFlow::Poll
    │
    ▼
RedrawRequested 触发
    │
    ▼
Composer::compose()
    ├── consume_global_dirty() → take_pending_states()
    ├── 用 state_id → slot_deps → mark_dirty(key)
    ├── 执行 user composable
    │   ├── 脏 slot → 重新执行 composable
    │   └── clean slot → 跳过
    └── 记录新依赖到 slot_deps
    │
    ▼
focus_by_id(root, pw.focused_id) 恢复焦点
    │
    ▼
layout() + render()
```

### 5.2 增量重组机制

```
State::get()
    │
    ├── register_dependency(state_id)
    │       →
    │   thread_local ACTIVE_SLOT_KEY → 记录 (state_id, slot_key)
    │       →
    │   RECORDED_DEPS.push((state_id, slot_key))
    │
State::set(value)
    │
    ├── notify()
    │   ├── notify_state_changed(state_id) → PENDING_STATES.push
    │   └── set_global_dirty()
    │
Composer::compose()
    ├── take_global_dirty() → 有变化?
    ├── take_pending_states() → 遍历 state_id
    ├── slot_deps.get(state_id) → 取 slot_key 列表
    ├── mark_dirty(slot_key) → 标记脏 slot
    ├── 执行 user composable:
    │     start_node(key):
    │       slot.dirty? → 重新执行
    │     clean → 跳过
    └── take_recorded_deps() → 更新 slot_deps
```

### 5.3 remember 的持久化陷阱

`remember` 的 key 由 `remember_counter` 自动递增生成。如果同一 composable 在不同分支中的 remember 次数不同：

```rust
// ❌ 每次 show_alt 变化，这个 remember 的 key 偏移
fn my_composable(ctx: &mut ComposeCtx) {
    let show_window = ctx.remember(|| false);
    if show_alt.get() {
        ctx.remember(|| "alt");  // 消耗 key
    }
    // 这里 remember 的 key 在 show_alt=true/false 时不同！
    let created_id = ctx.remember(|| 0u64);
}
```

**修复**: `remember_at_key(key, init)` 使用固定 key：

```rust
let created_id = ctx.remember_at_key(u64::MAX, || 0u64);
```

---

## 六、窗口管理

### 6.1 声明式窗口

```rust
let show_window = ctx.remember(|| false);

// 窗口由 if 条件控制生命周期
if show_window.get() {
    Window::new()
        .size(250.0, 180.0)
        .title("Sub Window")
        .on_close({ let s = show_window.clone(); move || { s.set(false); } })
        .build(ctx, |ctx| {
            Text::new("Hello from sub window").build(ctx);
        });
}
```

### 6.2 窗口关闭机制

```
if show_window.get() 变为 false
    │
    ▼
Window::build 不执行 → 对应的 slot 被 truncate
    │
    ▼
on_remove 回调触发 → PENDING_REMOVE_ID = window_id
    │
    ▼
compose 后 → has_pending_close() 返回 true
    │
    ▼
APP_PROXY.wake_up() → proxy_wake_up
    │
    ▼
process_detached() → 找到对应窗口 → 执行 on_close → 移除
```

### 6.3 多窗口

- 主窗口由 `run_app` 在 `can_create_surfaces` 中创建
- 子窗口通过 `Window::build` → `open_window_with_close` → `GLOBAL_PENDING` → `process_pending_windows` 创建
- `parent_window_id` 标识主窗口（用于 DevTools 事件路由和关闭行为）
- `PerWindow.created_id` 将声明式 Window id 和 OS WindowId 关联

---

## 七、焦点系统

### 7.1 工作方式

```
Tab 键 → focus_next(root) → 查找下一个 Focusable 节点
  → 设置 LayoutNode.focused = true
  → 更新 pw.focused_id（跨 compose 持久化）
  → request_redraw()
  → render() 遇到 focused == true → draw_focus() 画蓝色边框

鼠标点击 → hit_test → 匹配 Clickable → on_click()
  → 如果有 FocusRequester.request_focus() → 修改 LayoutNode.focused
  → 同步 pw.focused_id = get_focus_id(root)
  → request_redraw() + ControlFlow::Poll
```

### 7.2 FocusRequester

```rust
let btn1 = ctx.remember(|| FocusRequester::new()).get();
let btn2 = ctx.remember(|| FocusRequester::new()).get();

Button::new()
    .modifier(Modifier::new().focusable().focus_requester(&btn1))
    .on_click({ let b = btn2.clone(); move || { b.request_focus(); } })
    .build(ctx, |ctx| { Text::new("Button 1").build(ctx); });
```

- `FocusRequester` 是可 Clone 的（内部是 Arc<AtomicU64>）
- `&FocusRequester` 实现了 `Into<FocusRequester>`，所以 `focus_requester(&btn)` 不消耗所有权
- `request_focus()` 通过 thread-local `SLOT_FOCUS` 找到当前 composable 的 slot → 在 LayoutNode 上设置焦点

---

## 八、如何添加新组件

以添加一个 `Divider` 组件为例：

### 8.1 定义组件

```rust
// ui/divider.rs
pub struct Divider {
    thickness: f32,
    color: Color,
}

impl Divider {
    pub fn new() -> Self { ... }
    pub fn thickness(mut self, v: f32) -> Self { self.thickness = v; self }
    pub fn color(mut self, c: Color) -> Self { self.color = c; self }

    pub fn build(self, ctx: &mut ComposeCtx) {
        let key = ctx.next_key();
        let modifier = Modifier::new()
            .fill_max_width()
            .height(self.thickness)
            .background(self.color, Shape::Rectangle)
            .clickable(|| {});  // 需要空 clickable 吗？不必须
        ctx.start_leaf(key, modifier);
        ctx.end_node();
    }
}
```

### 8.2 注册到模块

```rust
// ui.rs 或 lib.rs
pub mod divider;
pub use divider::Divider;
```

### 8.3 使用

```rust
Divider::new().thickness(2.0).color(Color::GRAY).build(ctx);
```

### 8.4 测试

```rust
#[test]
fn test_divider_builder() {
    let mut composer = Composer::new();
    composer.compose(|ctx| {
        Divider::new().thickness(2.0).color(Color::GRAY).build(ctx);
    });
    let root = composer.layout_root().unwrap();
    // 验证 root 的 modifier 包含 size/background
    assert!(...);
}
```

---

## 九、调试技巧

### 9.1 启用 DevTools

```bash
cargo run -p winia --example counter --features debug-server
```

打开 `http://localhost:9999/tree` 查看组件树。

### 9.2 模拟点击

```bash
# physical 坐标（会被除以 scale_factor）
curl "http://localhost:9999/click?x=150&y=350"
```

### 9.3 查看 stderr

框架在关键路径有 `eprintln!` 输出（发行版中移除）。可以临时加：

```rust
eprintln!("[tag] value={}", value);
```

### 9.4 常见问题

| 症状 | 可能原因 |
|------|---------|
| 点击无反应 | 坐标不对（参考 tree 的 pos）或 hit_test 边界问题 |
| 窗口不关闭 | `has_pending_close()` + `wake_up()` 链路断 |
| 焦点环不显示 | `pw.focused_id` 未同步或 ControlFlow::Wait |
| 状态不持久化 | remember key 偏移 → 用 `remember_at_key` 固定 key |
| 布局错位 | padding 约束 + `tighten_width` clamp 导致大小不对 |

---

## 十、关键代码速查

| 功能 | 文件 | 行号 |
|------|------|------|
| 事件循环入口 | `app.rs` | `run_app` L379 |
| 渲染管线 | `app.rs` | `RedrawRequested` L181 |
| Composer::compose | `core/composer.rs` | L331 |
| measure_node | `layout/column.rs` | L178 |
| hit_test | `layout/node.rs` | L180 |
| State::get/set | `core/state.rs` | L98 / L108 |
| render | `render.rs` | L34 |
| backdrop blur | `render.rs` | L147 |
| Window::build | `ui/window.rs` | L91 |
| process_detached | `ui/window.rs` | L65 |
| modifier 链 | `modifier.rs` | L164 |
| FocusRequester | `modifier.rs` | L462 |
| ScrollState | `modifier.rs` | L433 |
| animation tick | `animation.rs` | L130 |
| devtools click | `debug.rs` | L160 |
