# Winia v2

基于 **winit + skia-safe** 的声明式跨平台 GUI 框架，架构对标 Jetpack Compose，纯 Rust 实现。

声明式组合（`#[composable]` 函数 + 内容闭包）、链式 Modifier、增量重组、自研布局/动画引擎，渲染走 Skia（Vulkan/GL/CPU），无外部 UI 框架依赖。

> 默认分支：`v2`（合并主线）。项目仍在演进中，但核心功能已可用且有测试保障。

## 快速开始

```rust
use winia::prelude::*;

#[composable]
fn counter(ctx: &mut ComposeCtx) {
    let count = ctx.remember(|| 0i32);
    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .build(ctx, |ctx| {
            Text::new(format!("Count: {}", count.get())).build(ctx);
            Button::new()
                .on_click(move || count.update(|v| *v += 1))
                .build(ctx, |ctx| { Text::new("+1").build(ctx); });
        });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(400.0, 500.0)
                .title("Counter")
                .build(ctx, |ctx| { counter(ctx); });
        });
    });
}
```

## 功能

- **组件**：Text / RichText / TextField / Button / Column / Row / Stack / SelectionContainer / Window（多窗口）/ Popup / Dialog / DropdownMenu / AnimatedVisibility / AnimatedContent / AnimatedSize / Crossfade
- **Modifier**：布局（size / padding / offset / align / aspectRatio / requiredSize / 权重 / 滚动）、绘制（background / border / clip / shadow / blur / backdropBlur）、图形层（alpha / scale / rotate / rotationX/Y / cameraDistance / shadowElevation / transformOrigin / clip）、交互（clickable / focusable / hoverable / tap / double-tap / long-press / drag / key / pointer）、水波纹 `ripple`
- **交互源**：`MutableInteractionSource` + `ComponentState`（press / focus / hover / drag），Button 状态取色与阴影
- **动画**：Tween / Spring / Keyframes / Repeatable / Decay + `animate_*AsState` 族 + AnimatedVisibility / AnimatedContent / AnimatedSize / Crossfade
- **渲染**：Skia（Vulkan/GL/CPU）、GraphicsLayer 2D/3D 透视、HiDPI、两阶段背景模糊
- **调试**：`debug-server` 特性——WebSocket 调试通道（模拟点击/滚动/截图/读布局树）

## 运行与测试

```bash
cargo run -p winia --example counter          # 计数器 + 滚动 + 多窗口示例
cargo test -p winia --lib                     # 275 个单元/布局/渲染测试
cargo test --test ui_test --features debug-server  # UI 集成测试（真实窗口）
```

更多示例见 `winia/examples/`（动画、组件、手势、GraphicsLayer、交互、Overlay、TextField、选区等 21 个）。

## 文档

- [接手文档](docs/handover.md) — 架构现状、已知问题、调试指南（新读者先看这里）
- [组件/Modifier 差距分析](docs/component-gap-analysis.md) — 对标 Compose 的实现清单与剩余缺口
- [Button 组件文档](docs/button.md) — Button API、实现细节与未实现项
- [动画差距分析](docs/animation-gap-analysis.md) — 动画系统全景与实现状态
- [架构设计文档](docs/architecture.md) — 早期设计稿（部分过时，以接手文档为准）
- [UI 测试框架](docs/ui-testing.md) — 集成测试的用法与坑

## Workspace 结构

| Crate | 说明 |
|-------|------|
| `winia` | UI 框架核心库（组合/布局/渲染/动画/组件） |
| `winia-macros` | 过程宏（`#[composable]`、`#[app_root]`、`run_app!`） |
| `skiwin` | Skia 渲染后端（Vulkan/GL/CPU） |

## 开发状态

- [x] 组合引擎：增量重组（Skip）、两段式依赖注册、稳定的 key 机制
- [x] 布局：三阶段测量 + 常量折叠 + RTL + 滚动
- [x] 渲染：Modifier 链 + GraphicsLayer（2D/3D/阴影）+ 背景模糊
- [x] 组件与交互：见上方功能清单（含 InteractionSource 与水波纹）
- [x] 动画系统：值/颜色/尺寸动画 + 容器动画
- [ ] 远期：LazyColumn、TextField label/内置容器视觉、其余 Compose 对齐项（见差距分析）

## 许可证

Apache-2.0
