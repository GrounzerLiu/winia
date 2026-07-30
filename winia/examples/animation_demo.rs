//! 动画系统演示
//!
//! 展示 animate_float_as_state / updateTransition / Spring / Tween / Bouncy

use winia::prelude::*;
use winia::animation::{AnimationSpec, SpringSpec, TweenSpec, Transition};
use winia::app;

fn animation_demo(ctx: &mut ComposeCtx) {
    let clicked = ctx.remember(|| false);

    Column::new()
        .modifier(Modifier::new().padding(16.0).fill_max_size())
        .build(ctx, |ctx| {
            // ── Title ──
            Text::new("Animation Demo")
                .font_size(22.0)
                .modifier(Modifier::new().padding(0.0).padding_vertical(8.0))
                .build(ctx);

            // ── 1. animate_float_as_state ──
            Text::new("1. animate_float_as_state (Spring Bouncy)")
                .font_size(14.0)
                .color(Color::from_argb(200, 100, 100, 100))
                .build(ctx);

            let scale = ctx.animate_float_as_state(
                if clicked.get() { 1.5 } else { 1.0 },
                AnimationSpec::Spring(SpringSpec::bouncy()),
            );
            Text::new(format!("Scale: {:.2}", scale.get()))
                .font_size(12.0)
                .modifier(Modifier::new().padding(4.0).padding_vertical(2.0))
                .build(ctx);

            // ── 2. Tween animation ──
            Text::new("2. Tween (300ms linear)")
                .font_size(14.0)
                .color(Color::from_argb(200, 100, 100, 100))
                .build(ctx);

            let alpha = ctx.animate_float_as_state(
                if clicked.get() { 0.2 } else { 1.0 },
                AnimationSpec::Tween(TweenSpec {
                    duration: Duration::from_millis(300),
                    interpolator: winia::animation::interpolator::linear,
                }),
            );
            Text::new(format!("Alpha: {:.2}", alpha.get()))
                .font_size(12.0)
                .modifier(Modifier::new().padding(4.0).padding_vertical(2.0))
                .build(ctx);

            // ── 3. updateTransition ──
            Text::new("3. updateTransition (multi-property)")
                .font_size(14.0)
                .color(Color::from_argb(200, 100, 100, 100))
                .build(ctx);

            let page = if clicked.get() { 1u8 } else { 0u8 };
            let mut t = ctx.update_transition(
                page,
                AnimationSpec::Spring(SpringSpec::default()),
                "page",
            );
            let tx = t.animate_float(
                ctx,
                |p| if *p == 0 { 0.0 } else { 100.0 },
                "tx",
            );
            let ty = t.animate_float(
                ctx,
                |p| if *p == 0 { 0.0 } else { 50.0 },
                "ty",
            );
            Text::new(format!("Page {}: offset=({:.0},{:.0})", page, tx.get(), ty.get()))
                .font_size(12.0)
                .modifier(Modifier::new().padding(4.0).padding_vertical(2.0))
                .build(ctx);

            // ── Toggle button ──
            Button::new()
                .on_click({ let c = clicked.clone(); move || { c.update(|v| *v = !*v); } })
                .modifier(Modifier::new().padding(8.0))
                .build(ctx, |ctx| {
                    Text::new(if clicked.get() { "Reset" } else { "Animate!" })
                        .font_size(16.0)
                        .build(ctx);
                });
        });
}

fn main() {
    app::run_app(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(400.0, 500.0)
                .title("Animation Demo")
                .build(ctx, |ctx| animation_demo(ctx));
        });
    });
}
