# Winia v2

基于 **winit + skia-safe** 的声明式跨平台 GUI 框架，架构对标 Jetpack Compose，纯 Rust 实现。

> ⚠️ 处于早期重构阶段，尚未可用。

## 快速开始

```rust
use winia::prelude::*;

fn counter(ctx: &mut ComposeCtx) {
    let count = ctx.remember(|| 0i32);
    Column::new()
        .modifier(Modifier::fill_max_size().padding(16.0))
        .build(ctx, |ctx| {
            Text::new(format!("Count: {}", count.get())).build(ctx);
            Button::new()
                .on_click(move || count.update(|v| *v += 1))
                .build(ctx, |ctx| { Text::new("+1").build(ctx); });
        });
}

fn main() {
    run_app(counter).title("Counter").size(400, 300);
}
```

## 文档

- [架构设计文档](docs/architecture.md) — 整体分层、核心系统设计、API 设计、开发路线图

## Workspace 结构

| Crate | 说明 |
|-------|------|
| `winia` | UI 框架核心库（本 crate） |
| `skiwin` | Skia 渲染后端（Vulkan/GL/CPU） |
| `proc-macro` | 过程宏（`#[item]`、`define_props!`） |
| `material_color_utilities` | Material Design 颜色工具（Dart 移植） |

## 开发状态

- [x] Phase 1: State 系统 + Composition 引擎骨架
- [ ] Phase 2: Modifier 链
- [ ] Phase 3: Layout 布局
- [ ] Phase 4: 基础组件
- [ ] Phase 5: 输入事件
- [ ] Phase 6: 更多组件 + 主题 + 动画
- [ ] Phase 7: 清理与稳定

## 许可证

Apache-2.0
