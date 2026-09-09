//! 水波纹模式演示（两层模型）
//!
//! ripple 由两层组成：
//! - 背景层（hover/focus 状态层）：bounded 时裁剪到组件形状内；
//!   unbounded 时是以组件对角线为直径的圆（半径 = 对角线/2，节点中心）。
//! - 前景层（按压波纹）：永远是圆，半径 = 组件对角线（从按压点扩散）。
//!
//! 悬停看背景层（bounded 裁剪 / unbounded 整圆），点击看前景层。

use letclone::clone;
use winia::core::composer::GroupStatus;
use winia::layout::{Alignment, BoxLayout};
use winia::prelude::*;

fn section_title(ctx: &mut ComposeCtx, text: &str) {
    Text::new(text)
        .font_size(14.0)
        .color(Color::from_argb(255, 90, 90, 90))
        .modifier(Modifier::new().padding_top(14.0).padding_bottom(6.0))
        .build(ctx);
}

#[composable]
fn ripple_row(
    ctx: &mut ComposeCtx,
    label: &str,
    w: f32,
    h: f32,
    shape: Shape,
    bounded: bool,
    clicks: &State<i32>,
) {
    let src = ctx.remember(|| MutableInteractionSource::new()).get();
    let theme = WiniaTheme::colors();
    let m = Modifier::new()
        .size(w, h)
        .background(theme.surface_container_highest, shape)
        .clickable_with_source(&src, { clone!(clicks); move || clicks.update(|v| *v += 1) })
        .ripple(&src, theme.on_surface, bounded);
    Row::new()
        .modifier(Modifier::new().padding_vertical(6.0))
        .build(ctx, |ctx| {
            Text::new(label)
                .font_size(13.0)
                .modifier(Modifier::new().width(260.0).padding_top(12.0))
                .build(ctx);
            let key = ctx.next_key();
            match ctx.start_restartable_group(
                key,
                m,
                BoxLayout::new().alignment(Alignment::Center),
            ) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {}
            }
            ctx.set_current_node_focus_color(theme.primary);
            ctx.end_restartable_group();
        });
}

#[composable]
fn ripple_demo(ctx: &mut ComposeCtx) {
    let scroll_y = ctx.remember(|| ScrollState::new()).get();
    let clicks = ctx.remember(|| 0i32);

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0).vertical_scroll(scroll_y))
        .build(ctx, |ctx| {
            Text::new("Ripple 两层模型演示")
                .font_size(20.0)
                .build(ctx);
            Text::new("悬停看背景层；点击看前景层（前景裁剪到背景的范围和形状）")
                .font_size(13.0)
                .color(Color::from_argb(255, 100, 100, 100))
                .modifier(Modifier::new().padding_top(4.0))
                .build(ctx);
            Text::new(format!("点击次数：{}", clicks.get()))
                .font_size(13.0)
                .color(Color::from_argb(255, 100, 100, 100))
                .build(ctx);

            section_title(ctx, "bounded（背景裁剪到形状内）");
            ripple_row(ctx, "胶囊 120×40（对角≈126）", 120.0, 40.0, Shape::pill(), true, &clicks);
            ripple_row(ctx, "圆形 48（对角≈68）", 48.0, 48.0, Shape::Circle, true, &clicks);
            ripple_row(ctx, "圆角 8 方 64（对角≈91）", 64.0, 64.0, Shape::rounded(8.0), true, &clicks);

            section_title(ctx, "unbounded（背景圆直径 = 对角线）");
            Text::new("背景：节点中心、直径=对角线的圆；前景：半径=对角线的圆，裁剪到背景圆内")
                .font_size(12.0)
                .color(Color::from_argb(255, 100, 100, 100))
                .modifier(Modifier::new().padding_bottom(4.0))
                .build(ctx);
            ripple_row(ctx, "小圆 24（对角≈34，Switch 拇指场景）", 24.0, 24.0, Shape::Circle, false, &clicks);
            ripple_row(ctx, "圆形 48（对角≈68）", 48.0, 48.0, Shape::Circle, false, &clicks);
            ripple_row(ctx, "圆角 8 方 40（对角≈57）", 40.0, 40.0, Shape::rounded(8.0), false, &clicks);
            ripple_row(ctx, "方 32（对角≈45）", 32.0, 32.0, Shape::Rectangle, false, &clicks);

            Text::new("")
                .modifier(Modifier::new().height(40.0))
                .build(ctx);
        });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::light(ctx, |ctx| {
            Window::new()
                .size(560.0, 660.0)
                .title("Ripple Demo")
                .build(ctx, ripple_demo);
        });
    });
}
