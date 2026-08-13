# Tooltip 组件

> 分支：`tooltips`
> 参考：M3 specs（m3.material.io/components/tooltips/specs）+ Jetpack Compose material3 `Tooltip.kt`

## 1. API

```rust
// Plain tooltip（文本显示在锚点上方）
Tooltip::new("This is a plain tooltip")
    .build(ctx, |ctx| { /* 锚点内容（Button 等） */ });

// 自定义内容（Rich tooltip——标题/正文/按钮由调用方构造）
Tooltip::new("")
    .content(|ctx: &mut ComposeCtx| { /* rich 内容 */ })
    .visible(show_state.clone())      // 外部可见性控制（与 hover 合并）
    .no_hover()                        // 禁用 hover 触发（仅外部控制）
    .build(ctx, |ctx| { /* 锚点 */ });

// 通用 setter
    .position(PopupPosition)   // 默认 TopCenter（锚点上方居中）
    .offset(x, y)              // 默认 (0, 8)——与锚点 8dp 间距
```

- 触发：默认 **hover**（锚点挂 hoverable，进入显示/离开隐藏）；`.visible(State)` 外部
  控制合并（任一 true 显示）；`.no_hover()` 禁用 hover 仅外部控制。
- 结构：`TooltipBox`（Compose 同名）——锚点 content 挂主树 + tooltip 浮层经
  overlay 机制定位在锚点上方。

## 2. 结构与默认值（M3 specs）

**Plain tooltip**：
| 项 | 值 |
|---|---|
| 容器 | `inverse_surface` + 8dp 圆角 + 8dp padding + 180dp 宽 |
| 文字 | `inverse_on_surface` + 14sp + 4 行 |
| 定位 | 锚点上方居中（`TopCenter`）+ 8dp 间距 |

**Rich tooltip**（调用方经 `.content()` 构造）：
| 项 | 值 |
|---|---|
| 容器 | `surface_container` + 4dp 圆角 + 16dp padding |
| 文字 | 标题 onSurface 14sp / 正文 onSurfaceVariant 12sp |
| 结构 | 标题 + 正文 + 最多 2 按钮（M3：subhead/supporting/action） |

## 3. 实现细节

- **结构**（对齐 DropdownMenu）：锚点容器（挂 hoverable 交互源）→ `ctx.open_overlay`
  注册浮层 → overlay 独立 composer 组合 tooltip 内容 + 主树锚点定位。
- **触发**：锚点 `hoverable(&interaction)` → `is_hovered()`（`get()` 注册依赖，
  hover 进出触发重组）→ `show = hover && external` → `record_overlay_active` +
  `open_overlay`（show=false 记录 false → sync 删除，对齐 Popup/Dialog 参数化）。
- **overlay 内容 key**：⚠ 内容在**独立 composer** 组合，闭包不在 `#[composable]`
  注入内——`ctx.next_key()` 直接调用会 panic「无法获得稳定 key」。必须用
  `ctx.key(0, |ctx| ...)` 包裹（固定 key 稳定复用）。
- **嵌套 hover 支持（app.rs 修复）**：`update_hover` 原只发射**最内层** hoverable——
  Tooltip 锚点容器挂 hoverable 时，内部 Button（clickable 也挂 hoverable）抢走
  事件 → 外层收不到 Enter → tooltip 不显示。改为**路径上所有 hoverable** 独立
  Enter/Exit（`hovered_slots` HashSet 集合，对齐 Compose 每个 hoverable 独立收事件）。
- **定位**：复用 PopupPosition 锚点定位（TopCenter = 锚点上方居中），offset 8dp。

## 4. 已知限制 / 待改进

- **出现/消失动画**：Compose tooltip 有淡入/淡出 transition；当前即时显示/隐藏
  （可后续接 AnimatedVisibility overlay 内容）。
- **positionProvider 自适应**：Compose 按锚点位置自动翻转（上方空间不足翻下方）；
  当前固定 TopCenter（`position` 可手动换）。
- **focusable**：Compose TooltipBox 有 focusable 参数（键盘聚焦显示）；当前无。
- **全局互斥**：Compose `MutatorMutex` 保证同时只显示一个 tooltip；当前无。
