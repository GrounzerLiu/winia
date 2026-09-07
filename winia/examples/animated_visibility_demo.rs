//! AnimatedVisibility demo — content enter/exit animation (cf. Compose AnimatedVisibility)
//!
//! Run: cargo run -p winia --example animated_visibility_demo
//!
//! Five panels showing different transition combos:
//! - A: fade_in + expand_in (fade + vertical expand)
//! - B: slide_in(Right) + scale_in (slide from right + scale)
//! - C: fade_out + shrink_out (fade + shrink — followers move up)
//! - D: expand_in_h (horizontal expand — fixed 300px bar grows rightward)
//! - E: slide Fraction(1.0) (full-width slide-in — Compose initialOffsetX equivalent)

use winia::prelude::*;
use winia::animation::{SpringSpec, TweenSpec};

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        Window::new()
            .size(420.0, 900.0)
            .title("AnimatedVisibility Demo")
            .build(ctx, |ctx| {
                panel_a(ctx);
                panel_b(ctx);
                panel_c(ctx);
                panel_d(ctx);
                panel_e(ctx);
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
            // Fixed anchor outside the panel: always visible — moves up when C collapses.
            Text::new("── fixed anchor (moves up when panel C shrinks) ──").font_size(12.0).build(ctx);
        });
}

/// Panel D: horizontal expand only (new transition).
/// Enter: a fixed 300px purple bar grows rightward from the start edge.
/// Exit: shrinks back toward the start edge. No slide — pure width animation.
#[composable]
fn panel_d(ctx: &mut ComposeCtx) {
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
                    Text::new("D: horizontal expand (toggle)").font_size(12.0).build(ctx);
                });
            AnimatedVisibility::new(show.clone())
                .enter(VisibilityTransition::expand_in_h(TweenSpec::default()))
                .exit(VisibilityTransition::shrink_out_h(TweenSpec::default()))
                .build(ctx, |ctx| {
                    Text::new("Panel D — watch me grow rightward")
                        .font_size(14.0)
                        .color(Color::WHITE)
                        .modifier(
                            Modifier::new()
                                .size(300.0, 44.0)
                                .background(Color::from_argb(255, 156, 39, 176), Shape::rounded(8.0)),
                        )
                        .build(ctx);
                });
        });
}

/// Panel E: full-width slide only (new SlideOffset::Fraction).
/// Enter: content slides in from the right edge by its full width
/// (Fraction(1.0) — the Compose `initialOffsetX = { fullWidth }` equivalent).
/// No expand — pure translation + fade.
#[composable]
fn panel_e(ctx: &mut ComposeCtx) {
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
                    Text::new("E: full-width slide (toggle)").font_size(12.0).build(ctx);
                });
            AnimatedVisibility::new(show.clone())
                .enter(
                    VisibilityTransition::slide_in_offset(
                        SlideDirection::Right,
                        SlideOffset::Fraction(1.0),
                        TweenSpec::default(),
                    )
                    .with_fade(),
                )
                .exit(
                    VisibilityTransition::slide_out_offset(
                        SlideDirection::Right,
                        SlideOffset::Fraction(1.0),
                        TweenSpec::default(),
                    )
                    .with_fade(),
                )
                .build(ctx, |ctx| {
                    Text::new("Panel E — full-width slide-in")
                        .modifier(Modifier::new().background(Color::from_argb(255, 0, 150, 136), Shape::rounded(8.0)))
                        .build(ctx);
                });
        });
}
