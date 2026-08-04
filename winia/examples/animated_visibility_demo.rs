//! AnimatedVisibility demo——出现/消失动画。
//!
//! 运行：`cargo run -p winia --example animated_visibility_demo --features debug-server`
//!
//! 展示：
//! 1. 纯淡入淡出（fade_in/fade_out）
//! 2. 滑入滑出（expand_in/shrink_out——20px 位移）
//! 3. 自定义过渡（弹性 enter）
//! 4. 快速切换（exit 中途 re-enter 应重定向动画不闪断）

use winia::animation::AnimationSpec;
use winia::core::composer::ComposeCtx;
use winia::core::state::State;
use winia::modifier::{Color, Modifier, Shape};
use winia::prelude::{
    AnimatedVisibility, Button, Column, Row, Text, WiniaTheme, Window,
    expand_in, fade_in, fade_out, shrink_out,
};
use winia::app;

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();

    app::run_app(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(420.0, 560.0)
                .title("AnimatedVisibility Demo")
                .build(ctx, |ctx| demo(ctx));
        });
    });
}

#[winia::composable]
fn demo(ctx: &mut ComposeCtx) {
    // 三个独立开关
    let v1 = ctx.remember(|| true);
    let v2 = ctx.remember(|| true);
    let v3 = ctx.remember(|| false);

    Column::new()
        .modifier(Modifier::new().padding(16.0).fill_max_size())
        .build(ctx, |ctx| {
            Text::new("AnimatedVisibility Demo")
                .font_size(18.0)
                .bold()
                .modifier(Modifier::new().padding(12.0))
                .build(ctx);

            // ── 1. 淡入淡出 ──
            toggle_row(ctx, "fade_in / fade_out", &v1);
            AnimatedVisibility::new(v1.clone())
                .build(ctx, |ctx| {
                    panel(ctx, "1. Fade — 淡入淡出面板（默认 300ms）");
                });

            // ── 2. 滑入滑出 ──
            toggle_row(ctx, "expand_in / shrink_out", &v2);
            AnimatedVisibility::new(v2.clone())
                .enter(expand_in())
                .exit(shrink_out())
                .build(ctx, |ctx| {
                    panel(ctx, "2. Expand — 20px 位移滑入滑出");
                });

            // ── 3. 自定义弹性进入 ──
            toggle_row(ctx, "custom spring enter", &v3);
            AnimatedVisibility::new(v3.clone())
                .enter(EnterTransitionSpring())
                .exit(fade_out())
                .build(ctx, |ctx| {
                    panel(ctx, "3. Spring — 弹性进入（bouncy）");
                });
        });
}

fn EnterTransitionSpring() -> winia::prelude::EnterTransition {
    winia::prelude::EnterTransition::new(
        AnimationSpec::Spring(winia::animation::SpringSpec::bouncy()),
        30.0,
    )
}

/// 一行：标签 + Toggle 按钮
#[winia::composable]
fn toggle_row(ctx: &mut ComposeCtx, label: &'static str, v: &State<bool>) {
    Row::new()
        .modifier(Modifier::new().fill_max_width().padding_vertical(6.0))
        .build(ctx, |ctx| {
            Text::new(label)
                .font_size(13.0)
                .color(Color::from_argb(220, 140, 140, 140))
                .modifier(Modifier::new().layout_weight(1.0))
                .build(ctx);
            Button::new()
                .on_click({
                    let v = v.clone();
                    move || { v.update(|x| *x = !*x); }
                })
                .build(ctx, |ctx| {
                    Text::new(if v.get() { "Hide" } else { "Show" })
                        .font_size(13.0)
                        .build(ctx);
                });
        });
}

/// 面板（内容示例）
#[winia::composable]
fn panel(ctx: &mut ComposeCtx, text: &'static str) {
    Column::new()
        .modifier(Modifier::new()
            .fill_max_width()
            .padding(14.0)
            .background(Color::from_argb(40, 100, 200, 255), Shape::rounded(8.0)))
        .build(ctx, |ctx| {
            Text::new(text)
                .font_size(14.0)
                .color(Color::from_argb(240, 255, 255, 255))
                .build(ctx);
        });
}
