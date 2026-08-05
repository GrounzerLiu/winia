//! AnimatedVisibility demo — 内容出现/消失动画（对标 Compose AnimatedVisibility）
//!
//! 运行：cargo run -p winia --example animated_visibility_demo
//!
//! 三个面板展示不同过渡组合：
//! - A：fade_in + expand_in（淡入 + 垂直展开）
//! - B：slide_in(Right) + scale_in（右滑 + 缩放）
//! - C：fade_out + shrink_out（淡出 + 收缩——下方锚点跟随上移）

use winia::prelude::*;
use winia::animation::{SpringSpec, TweenSpec};

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        Window::new()
            .size(420.0, 560.0)
            .title("AnimatedVisibility Demo")
            .build(ctx, |ctx| {
                panel_a(ctx);
                panel_b(ctx);
                panel_c(ctx);
            });
    });
}

/// 面板 A：fade + expand（淡入淡出 + 垂直展开/收缩）
#[composable]
fn panel_a(ctx: &mut ComposeCtx) {
    let show = ctx.remember(|| true);
    Column::new()
        .modifier(Modifier::new().padding(16.0))
        .build(ctx, |ctx| {
            Button::new()
                .on_click({
                    let s = show.clone();
                    move || s.update(|v| *v = !*v)
                })
                .build(ctx, |ctx| {
                    Text::new("A: fade + expand (toggle)").font_size(12.0).build(ctx);
                });
            AnimatedVisibility::new(show.clone())
                .enter(VisibilityTransition::fade_in(TweenSpec::default()).with_expand())
                .exit(VisibilityTransition::fade_out(TweenSpec::default()).with_expand())
                .build(ctx, |ctx| {
                    Text::new("Panel A — fade + expand 内容")
                        .modifier(Modifier::new().background(Color::from_argb(255, 76, 175, 80), Shape::rounded(8.0)))
                        .build(ctx);
                });
        });
}

/// 面板 B：slide + scale（右滑进入 + 缩放）
#[composable]
fn panel_b(ctx: &mut ComposeCtx) {
    let show = ctx.remember(|| false);
    Column::new()
        .modifier(Modifier::new().padding(16.0))
        .build(ctx, |ctx| {
            Button::new()
                .on_click({
                    let s = show.clone();
                    move || s.update(|v| *v = !*v)
                })
                .build(ctx, |ctx| {
                    Text::new("B: slide + scale (toggle)").font_size(12.0).build(ctx);
                });
            AnimatedVisibility::new(show.clone())
                .enter(
                    VisibilityTransition::slide_in(SlideDirection::Right, SpringSpec::default())
                        .with_scale(),
                )
                .exit(VisibilityTransition::slide_out(SlideDirection::Right, TweenSpec::default()))
                .build(ctx, |ctx| {
                    Text::new("Panel B — slide + scale 内容")
                        .modifier(Modifier::new().background(Color::from_argb(255, 33, 150, 243), Shape::rounded(8.0)))
                        .build(ctx);
                });
        });
}

/// 面板 C：fade + shrink（淡出 + 收缩——下方内容跟随上移）
#[composable]
fn panel_c(ctx: &mut ComposeCtx) {
    let show = ctx.remember(|| true);
    Column::new()
        .modifier(Modifier::new().padding(16.0))
        .build(ctx, |ctx| {
            Button::new()
                .on_click({
                    let s = show.clone();
                    move || s.update(|v| *v = !*v)
                })
                .build(ctx, |ctx| {
                    Text::new("C: fade + shrink (toggle)").font_size(12.0).build(ctx);
                });
            AnimatedVisibility::new(show.clone())
                .enter(VisibilityTransition::expand_in(TweenSpec::default()))
                .exit(VisibilityTransition::fade_out(TweenSpec::default()).with_expand())
                .build(ctx, |ctx| {
                    Column::new().build(ctx, |ctx| {
                        Text::new("Panel C — fade + shrink 内容")
                            .modifier(Modifier::new().background(Color::from_argb(255, 255, 152, 0), Shape::rounded(8.0)))
                            .build(ctx);
                        // 内容下方锚点：C 收缩时此文本应跟随上移（布局动画验证）
                        Text::new("（下方锚点——收缩时应跟随上移）").font_size(12.0).build(ctx);
                    });
                });
            // 面板外的锚点：始终显示——C 完全消失时此文本上移填位
            Text::new("── 固定锚点（面板 C 收缩时上移） ──").font_size(12.0).build(ctx);
        });
}
