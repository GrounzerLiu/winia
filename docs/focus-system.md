# Winia 焦点系统

> 对标 Jetpack Compose 的 Focus 系统，支持 Tab 键切换、点击聚焦、FocusRequester 代码请求焦点。

---

## 一、架构概览

```
┌──────────────────────────────────────────────────────┐
│                app.rs (AppState)                      │
│  focused_id: Option<u64>  ─── 持久焦点，跨 compose   │
├──────────────────────────────────────────────────────┤
│              layout/node.rs                           │
│  focus_next() / focus_node() / focus_by_id()         │
│  collect_focusable() / get_focus_id()                │
│  clear_focus() / set_focus_by_ptr()                  │
├──────────────────────────────────────────────────────┤
│              modifier.rs                              │
│  FocusRequester { id: u64 }                           │
│  Modifier::focusable() / Modifier::focus_requester() │
│  ModifierElement::Focusable / FocusRequesterId        │
├──────────────────────────────────────────────────────┤
│              render.rs                                │
│  渲染焦点环：蓝色 2px 描边 (node.focused)            │
└──────────────────────────────────────────────────────┘
```

---

## 二、Modifier 层

### 2.1 两个 Modifier 元素

| Modifier 元素 | 说明 |
|--------------|------|
| `ModifierElement::Focusable` | 标记节点可通过 Tab 键获得焦点 |
| `ModifierElement::FocusRequesterId { id }` | 关联 FocusRequester，支持代码请求焦点 |

### 2.2 用法

```rust
let button_focus = FocusRequester::new();

Button::new()
    .modifier(Modifier::new()
        .focusable()                       // ← Tab 键可聚焦
        .focus_requester(button_focus)     // ← 关联 FocusRequester
    )
```

**注意**：`FocusRequester` 必须用 `ctx.remember` 保持跨 compose：

```rust
fn my_ui(ctx: &mut ComposeCtx) {
    let fr = ctx.remember(|| FocusRequester::new()).get();
    // ...
}
```

否则每次 compose 创建新 ID，旧的 `request_focus()` 失效。

---

## 三、FocusRequester

### 3.1 定义

```rust
#[derive(Debug, Clone)]
pub struct FocusRequester { id: u64 }

impl FocusRequester {
    pub fn new() -> Self;          // 全局唯一 ID
    pub fn request_focus(&self);   // 请求焦点
}
```

### 3.2 工作原理

```
request_focus()
  │
  ├─→ debug::queue_event(DebugEvent::RequestFocus { id })
  │        (通过 debug-server feature 的事件队列)
  │
  └─→ EventLoopProxy.wake_up() → request_redraw()
          │
          └─→ RedrawRequested:
                1. compose (重建 LayoutNode 树)
                2. 恢复持久焦点: focus_by_id(root, focused_id)
                3. layout + render
                4. 处理 RequestFocus 事件:
                   focus_by_id(root, id) → 设置 focused = true
                   focused_id = Some(id)  // 持久化
                5. request_redraw (下一帧生效)
```

### 3.3 依赖

`request_focus()` 仅在 `debug-server` feature 启用时生效。禁用时为空操作。

---

## 四、焦点遍历

### 4.1 Tab 键顺序

深度优先遍历 LayoutNode 树，收集带 `ModifierElement::Focusable` 的节点，按 Tab 键顺序切换。

```
collect_focusable(root, list)
  ├─→ 根节点有 Focusable? → push
  └─→ 递归子节点
```

### 4.2 核心函数

| 函数 | 说明 |
|------|------|
| `focus_next(root)` | 移动到下一个 Focusable 节点，回到第一个 |
| `focus_node(root, target)` | 指定节点获得焦点 |
| `focus_by_id(root, id)` | 通过 FocusRequester ID 聚焦 |
| `collect_focusable(root, list)` | 收集所有 Focusable 节点 |
| `get_focus_id(root)` | 获取当前焦点节点的 FocusRequester ID |
| `clear_focus(root)` | 清除整棵树的焦点标记 |

---

## 五、焦点持久化

**问题**：每次 `compose` 重建 LayoutNode 树，`focused: bool` 丢失。

**方案**：`AppState.focused_id: Option<u64>` 跨 compose 保持焦点 ID。

### 5.1 流程

```
compose() → 重建树
  │
  ├─→ if let Some(fid) = focused_id:
  │      focus_by_id(root, fid)  // 恢复焦点
  │
  ├─→ layout + render
  │
  └─→ 处理事件:
        RequestFocus { id } → focused_id = Some(id)
        Tab / FocusNext       → focused_id = get_focus_id(root)
```

### 5.2 焦点清除

- `FocusRequester::request_focus(id)` → `focused_id = Some(id)`
- `focus_next()` (Tab) → `focused_id = get_focus_id(root)`
- `focus_node()` (点击聚焦) → 目前不更新 focused_id（点击在 compose 前处理，不影响）

---

## 六、渲染

`render.rs` 中在背景和文字之后绘制焦点环：

```rust
if node.focused {
    // 蓝色 2px 描边
    paint.set_color4f(Color4f::new(0.3, 0.6, 1.0, 0.8), None);
    paint.set_style(Stroke);
    paint.set_stroke_width(2.0);
    canvas.draw_rect(rect, &paint);
}
```

---

## 七、完整示例

```rust
fn my_ui(ctx: &mut ComposeCtx) {
    let count = ctx.remember(|| 0i32);
    let btn1 = ctx.remember(|| FocusRequester::new()).get();
    let btn2 = ctx.remember(|| FocusRequester::new()).get();
    let b1c = btn1.clone(); let b2c = btn2.clone();

    Column::new().build(ctx, |ctx| {
        Button::new()
            .on_click({
                let c = count.clone();
                move || { c.update(|v| *v += 1); b2c.request_focus(); }
            })
            .modifier(Modifier::new()
                .size(200.0, 40.0)
                .background(BLUE)
                .focusable()          // ← Tab 键可到达
                .focus_requester(btn1) // ← 代码可 request_focus
            )
            .build(ctx, |ctx| { Text::new("Btn1").build(ctx); });

        Button::new()
            .on_click(move || { b1c.request_focus(); })
            .modifier(Modifier::new()
                .size(200.0, 40.0)
                .background(GREEN)
                .focusable()
                .focus_requester(btn2)
            )
            .build(ctx, |ctx| { Text::new("Btn2").build(ctx); });
    });
}
```

**交互**：
- 按 `Tab` — 焦点在 Btn1 ↔ Btn2 间切换
- 点 Btn1 — 计数 +1，焦点自动跳到 Btn2
- 点 Btn2 — 焦点跳到 Btn1

---

## 八、当前限制

| 限制 | 说明 |
|------|------|
| request_focus 依赖 debug-server | 非 debug 模式为空操作，后续可独立为 feature |
| 无 Shift+Tab 反向遍历 | 焦点链只支持正向 |
| 焦点样式不可定制 | 硬编码蓝色 2px |
| 无焦点变化回调 | 无 `onFocusChanged` 等价物 |
| 无焦点组 (FocusGroup) | 不支持限定焦点范围 |
