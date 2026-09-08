//! Card 样式演示——不同变体/状态/交互/形状（对标 material3 Card 一族）
//!
//! 展示：
//! - CardStyle: Filled / Elevated / Outlined 变体
//! - 纯展示卡片（无 on_click）与可点击卡片（on_click + 波纹）
//! - enabled / disabled 状态
//! - 自定义 shape / colors / border
//! - Elevated 阴影（hover/按下升高、移出回落）

use letclone::clone;
use winia::prelude::*;

fn section_title(ctx: &mut ComposeCtx, text: &str) {
    Text::new(text)
        .font_size(14.0)
        .color(Color::from_argb(255, 90, 90, 90))
        .modifier(Modifier::new().padding_top(14.0).padding_bottom(6.0))
        .build(ctx);
}

#[composable]
fn card_demo(ctx: &mut ComposeCtx) {
    let scroll_y = ctx.remember(|| ScrollState::new()).get();
    let clicks = ctx.remember(|| 0i32);

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0).vertical_scroll(scroll_y))
        .spacing(12.0)
        .build(ctx, |ctx| {
            Text::new("Card 样式演示（material3 对齐）")
                .font_size(20.0)
                .build(ctx);

            // ── 变体（纯展示，无 on_click）──
            section_title(ctx, "变体（纯展示）");
            Card::new().build(ctx, |ctx| {
                Text::new("Filled Card（SurfaceContainerHighest 容器）")
                    .modifier(Modifier::new().padding(16.0))
                    .build(ctx);
            });
            Card::elevated().build(ctx, |ctx| {
                Text::new("Elevated Card（SurfaceContainerLow + 阴影）")
                    .modifier(Modifier::new().padding(16.0))
                    .build(ctx);
            });
            Card::outlined().build(ctx, |ctx| {
                Text::new("Outlined Card（Surface 容器 + 1dp outline 边框）")
                    .modifier(Modifier::new().padding(16.0))
                    .build(ctx);
            });

            // ── 可点击（on_click + 波纹）──
            section_title(ctx, "可点击（hover/按下阴影升高 + 波纹）");
            Card::elevated()
                .on_click({ clone!(clicks); move || clicks.update(|v| *v += 1) })
                .build(ctx, |ctx| {
                    Row::new()
                        .modifier(Modifier::new().padding(16.0))
                        .build(ctx, |ctx| {
                            Text::new("点击次数：")
                                .font_size(14.0)
                                .build(ctx);
                            Text::new(format!("{}", clicks.get()))
                                .font_size(14.0)
                                .build(ctx);
                        });
                });

            // ── 禁用状态 ──
            section_title(ctx, "禁用状态");
            Card::outlined()
                .enabled(false)
                .on_click({ clone!(clicks); move || clicks.update(|v| *v += 1) })
                .build(ctx, |ctx| {
                    Text::new("Disabled Card（内容 @38%，点击无响应）")
                        .modifier(Modifier::new().padding(16.0))
                        .build(ctx);
                });

            // ── 自定义 shape / colors / border ──
            section_title(ctx, "自定义 shape / colors / border");
            Card::filled()
                .shape(Shape::rounded(4.0))
                .colors(CardColors::new(
                    Color::from_argb(255, 232, 222, 248),
                    Color::from_argb(255, 33, 33, 33),
                    Color::from_argb(255, 232, 222, 248),
                    Color::from_argb(255, 120, 120, 120),
                ))
                .build(ctx, |ctx| {
                    Row::new()
                        .modifier(Modifier::new().padding(16.0))
                        .build(ctx, |ctx| {
                            Text::new("自定义颜色 + 4dp 圆角")
                                .font_size(14.0)
                                .build(ctx);
                        });
                });
            Card::outlined()
                .border(CardBorder::new(2.0, Color::from_argb(255, 103, 80, 164)))
                .on_click({ clone!(clicks); move || clicks.update(|v| *v += 1) })
                .build(ctx, |ctx| {
                    Text::new("自定义 2px 边框（点击可交互）")
                        .modifier(Modifier::new().padding(16.0))
                        .build(ctx);
                });
        });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::light(ctx, |ctx| {
            Window::new()
                .size(480.0, 640.0)
                .title("Card Demo")
                .build(ctx, |ctx| { card_demo(ctx); });
        });
    });
}
