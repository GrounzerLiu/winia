//! Animation demo - one card per kind of animation the framework supports.
//!
//! Cards: tween, spring (with a mid-flight retarget), the four animate*AsState value types,
//! keyframes, keyframesWithSpline (side by side with keyframes), snap and repeatable,
//! infinite transitions (restart and reverse), the imperative Animatable (animateTo, sequence,
//! decay, stop), updateTransition (three states, several values), AnimatedVisibility, Crossfade
//! and AnimatedSize.
//!
//! Run with: cargo run -p winia --example animation_demo --features debug-server

use std::time::Duration;
use winia::animation::{
    AnimationSpec, DecaySpec, InfiniteRepeatableSpec, KeyframesOf, KeyframesWithSplineOf,
    RepeatMode, RepeatableSpec, SnapSpec, SpringSpec, TweenSpec, VisibilityTransition,
};
use winia::modifier::DrawNode;
use winia::prelude::*;

#[path = "common/settings.rs"]
mod settings;

const CARD_BG: Color = Color {
    r: 0x2A,
    g: 0x27,
    b: 0x2E,
    a: 255,
};
const TRACK_BG: Color = Color {
    r: 0x1E,
    g: 0x1C,
    b: 0x22,
    a: 255,
};
const MUTED: Color = Color {
    r: 0x9E,
    g: 0x9A,
    b: 0xA6,
    a: 255,
};
const ACCENT: Color = Color {
    r: 0xD0,
    g: 0xBC,
    b: 0xFF,
    a: 255,
};
const WARM: Color = Color {
    r: 0xFF,
    g: 0xB4,
    b: 0xAB,
    a: 255,
};
const GREEN: Color = Color {
    r: 0xA6,
    g: 0xE3,
    b: 0xA1,
    a: 255,
};

/// One titled card with a caption.
#[composable]
fn card(ctx: &mut ComposeCtx, title: &str, caption: &str, body: impl FnOnce(&mut ComposeCtx)) {
    Column::new()
        .spacing(8.0)
        .modifier(
            Modifier::new()
                .fill_max_width()
                .padding(12.0)
                .background(CARD_BG, Shape::rounded(12.0)),
        )
        .build(ctx, |ctx| {
            Text::new(title).font_size(12.0).color(ACCENT).build(ctx);
            body(ctx);
            Text::new(caption).font_size(10.0).color(MUTED).build(ctx);
        });
}

/// A dark strip that motion is visible on.
#[composable]
fn track(ctx: &mut ComposeCtx, height: f32, body: impl FnOnce(&mut ComposeCtx)) {
    Row::new()
        .modifier(
            Modifier::new()
                .fill_max_width()
                .height(height)
                .background(TRACK_BG, Shape::rounded(8.0))
                .padding_sides(8.0, 8.0, 8.0, 8.0),
        )
        .build(ctx, |scope| body(scope));
}

/// A round dot of `size` px.
#[composable]
fn dot(ctx: &mut ComposeCtx, color: Color, size: f32) {
    Spacer::new()
        .modifier(
            Modifier::new()
                .size(size, size)
                .background(color, Shape::rounded(size / 2.0)),
        )
        .build(ctx);
}

fn small_button(ctx: &mut ComposeCtx, label: &str, on_click: impl Fn() + Send + Sync + 'static) {
    Button::new()
        .on_click(on_click)
        .build(ctx, |ctx| Text::new(label).font_size(11.0).build(ctx));
}

// 1. Tween: the default spec is Compose's tween() - 300 ms, FastOutSlowIn.
#[composable]
fn tween_card(ctx: &mut ComposeCtx) {
    let right = ctx.remember(|| State::new(false)).get();
    let x = ctx.animate_float_as_state(
        if right.get() { 560.0 } else { 0.0 },
        AnimationSpec::Tween(TweenSpec::default()),
    );
    card(
        ctx,
        "1. animateFloatAsState + tween (300 ms, FastOutSlowIn)",
        "A plain tween: the dot slides to the other end and stops.",
        |ctx| {
            track(ctx, 48.0, |ctx| {
                Spacer::new()
                    .modifier(Modifier::new().width(x.get()))
                    .build(ctx);
                dot(ctx, ACCENT, 24.0);
            });
            small_button(ctx, "toggle tween", move || right.set(!right.peek()));
        },
    );
}

// 2. Spring: bouncy, and a retarget mid-flight keeps the velocity.
#[composable]
fn spring_card(ctx: &mut ComposeCtx) {
    let target = ctx.remember(|| State::new(0.0f32)).get();
    let x = ctx.animate_float_as_state(target.get(), AnimationSpec::Spring(SpringSpec::bouncy()));
    card(
        ctx,
        "2. spring (bouncy) + mid-flight retarget",
        "The dot overshoots; pressing retarget while it moves hands the velocity to the new spring.",
        |ctx| {
            track(ctx, 56.0, |ctx| {
                Spacer::new()
                    .modifier(Modifier::new().width(x.get().max(0.0)))
                    .build(ctx);
                dot(ctx, WARM, 24.0);
            });
            Row::new().spacing(8.0).build(ctx, |ctx| {
                small_button(ctx, "to start", {
                    let t = target.clone();
                    move || t.set(0.0)
                });
                small_button(ctx, "to end", {
                    let t = target.clone();
                    move || t.set(520.0)
                });
                small_button(ctx, "retarget mid-flight", {
                    let t = target.clone();
                    move || t.set(if t.peek() == 0.0 { 300.0 } else { 120.0 })
                });
            });
        },
    );
}

// 3. The four value types: colour, size, offset and dp all animate from one bool.
#[composable]
fn values_card(ctx: &mut ComposeCtx) {
    let on = ctx.remember(|| State::new(false)).get();
    let color = ctx.animate_color_as_state(
        if on.get() { GREEN } else { WARM },
        AnimationSpec::Tween(TweenSpec::default()),
    );
    let size = ctx.animate_dp_as_state(
        if on.get() { 56.0.dp() } else { 28.0.dp() },
        AnimationSpec::Spring(SpringSpec::default()),
    );
    let shift = ctx.animate_offset_as_state(
        winia::unit::Offset::new(if on.get() { 160.0 } else { 0.0 }, 0.0),
        AnimationSpec::Tween(TweenSpec::default()),
    );
    let pad = ctx.animate_float_as_state(
        if on.get() { 24.0 } else { 0.0 },
        AnimationSpec::Tween(TweenSpec::default()),
    );
    card(
        ctx,
        "3. animateColorAsState / animateDpAsState / animateOffsetAsState",
        "One toggle drives four different value types with their own specs.",
        |ctx| {
            track(ctx, 80.0, |ctx| {
                Spacer::new()
                    .modifier(
                        Modifier::new()
                            .width(shift.get().x.max(0.0))
                            .height(pad.get() + 1.0),
                    )
                    .build(ctx);
                dot(ctx, color.get(), size.get().value());
            });
            small_button(ctx, "toggle values", move || on.set(!on.peek()));
        },
    );
}

// 4 + 5. Keyframes vs keyframesWithSpline over the same keyframes.
#[composable]
fn keyframes_card(ctx: &mut ComposeCtx) {
    let run = ctx.remember(|| State::new(0)).get();
    // A keyframe band: 0 -> 420 px, up to 520 at 350 ms, back to 200 at 700 ms, end at 420.
    let kf = KeyframesOf::between(0.0, 420.0)
        .at(0, 0.0)
        .at(350, 520.0)
        .at(700, 200.0)
        .at(1000, 420.0)
        .build();
    let sp = KeyframesWithSplineOf::between(0.0, 420.0)
        .at(0, 0.0)
        .at(350, 520.0)
        .at(700, 200.0)
        .at(1000, 420.0)
        .build();
    let kx = ctx.animate_float_as_state(
        if run.get() % 2 == 1 { 420.0 } else { 0.0 },
        AnimationSpec::Keyframes(kf),
    );
    let sx = ctx.animate_float_as_state(
        if run.get() % 2 == 1 { 420.0 } else { 0.0 },
        AnimationSpec::KeyframesWithSpline(sp),
    );
    card(
        ctx,
        "4. keyframes and keyframesWithSpline",
        "Same keyframes: the top dot moves linearly between them, the bottom one follows the monotone cubic spline through them.",
        |ctx| {
            for (label, x) in [("keyframes", kx.clone()), ("spline", sx.clone())] {
                Row::new().spacing(8.0).build(ctx, |ctx| {
                    Text::new(label)
                        .font_size(10.0)
                        .color(MUTED)
                        .modifier(Modifier::new().width(64.0))
                        .build(ctx);
                    Spacer::new()
                        .modifier(Modifier::new().width(x.get().max(0.0)))
                        .build(ctx);
                    dot(ctx, if label == "keyframes" { ACCENT } else { WARM }, 18.0);
                });
            }
            small_button(ctx, "play keyframes", move || run.set(run.peek() + 1));
        },
    );
}

// 6. Snap (a step) and a finite repeat.
#[composable]
fn snap_repeat_card(ctx: &mut ComposeCtx) {
    let step = ctx.remember(|| State::new(0i32)).get();
    let x = ctx.animate_int_as_state(
        step.get() * 140,
        AnimationSpec::Snap(SnapSpec::new(Duration::from_millis(120))),
    );
    let pulse = ctx.remember(|| State::new(0u32)).get();
    let p = ctx.animate_float_as_state(
        pulse.get() as f32,
        AnimationSpec::Repeatable(RepeatableSpec::new(
            3,
            RepeatMode::Reverse,
            TweenSpec::new(
                Duration::from_millis(220),
                winia::animation::interpolator::Linear::new(),
            ),
        )),
    );
    card(
        ctx,
        "5. snap(delay) and repeatable(3, reverse)",
        "The upper dot jumps one step per press after 120 ms; the lower bar runs three reverse cycles per press.",
        |ctx| {
            track(ctx, 40.0, |ctx| {
                Spacer::new()
                    .modifier(Modifier::new().width(x.get().max(0) as f32))
                    .build(ctx);
                dot(ctx, ACCENT, 20.0);
            });
            Row::new()
                .modifier(Modifier::new().fill_max_width().height(18.0))
                .spacing(6.0)
                .build(ctx, |ctx| {
                    let width = 120.0 + 360.0 * p.get().clamp(0.0, 1.0);
                    Spacer::new()
                        .modifier(
                            Modifier::new()
                                .width(width)
                                .height(14.0)
                                .background(WARM, Shape::rounded(7.0)),
                        )
                        .build(ctx);
                });
            Row::new().spacing(8.0).build(ctx, |ctx| {
                small_button(ctx, "next step", {
                    let s = step.clone();
                    move || s.set((s.peek() + 1) % 4)
                });
                small_button(ctx, "pulse x3", {
                    let p = pulse.clone();
                    move || p.set(p.peek() + 1)
                });
            });
        },
    );
}

// 7. Infinite transitions: restart and reverse.
#[derive(Debug)]
struct InfiniteDots {
    restart: Visual<f32>,
    reverse: Visual<f32>,
}

impl DrawNode for InfiniteDots {
    fn draw(&self, canvas: &skia_safe::Canvas, rect: skia_safe::Rect) {
        for (value, y, color) in [
            (&self.restart, rect.top() + rect.height() * 0.3, ACCENT),
            (&self.reverse, rect.top() + rect.height() * 0.7, WARM),
        ] {
            let x = rect.left() + value.peek().clamp(0.0, 1.0) * (rect.width() - 20.0) + 10.0;
            let mut paint = skia_safe::Paint::default();
            paint.set_anti_alias(true);
            paint.set_color(skia_safe::Color::from_argb(
                color.a, color.r, color.g, color.b,
            ));
            canvas.draw_circle((x, y), 8.0, &paint);
        }
    }
}

#[composable]
fn infinite_card(ctx: &mut ComposeCtx) {
    let mut inf = ctx.remember_infinite_transition();
    let restart = inf.animate_float(
        ctx,
        0.0,
        1.0,
        InfiniteRepeatableSpec::restart(Duration::from_millis(900)),
    );
    let reverse = inf.animate_float(
        ctx,
        0.0,
        1.0,
        InfiniteRepeatableSpec::reverse(Duration::from_millis(900)),
    );
    card(
        ctx,
        "6. rememberInfiniteTransition (restart / reverse)",
        "Both loops run forever: the top restarts at 0, the bottom mirrors back along the same curve.",
        |ctx| {
            Column::new()
                .modifier(
                    Modifier::new()
                        .fill_max_width()
                        .height(72.0)
                        .background(TRACK_BG, Shape::rounded(8.0))
                        .draw_node(InfiniteDots {
                            restart: restart.clone(),
                            reverse: reverse.clone(),
                        }),
                )
                .build(ctx, |_| {});
        },
    );
}

// 8. The imperative Animatable: animateTo, a sequence, decay and stop.
#[composable]
fn animatable_card(ctx: &mut ComposeCtx) {
    let anim = ctx.remember_animatable(0.0f32);
    let value = anim.as_state().get();
    let velocity = anim.velocity();
    card(
        ctx,
        "7. Animatable (animateTo / sequence / decay / stop)",
        "The low-level handle: targets queue, a fling decays with friction, stop freezes the value.",
        |ctx| {
            track(ctx, 48.0, |ctx| {
                // Read the state INSIDE the track's restartable group: a value read in the card body
                // belongs to the outer scope, so the Row's group skips (its own parameters did not
                // change) and the spacer keeps its old width - the readout moved while the dot stood
                // still (reported as "card 7 does nothing").
                let value = anim.as_state().get();
                Spacer::new()
                    .modifier(Modifier::new().width(value.max(0.0)))
                    .build(ctx);
                dot(ctx, GREEN, 20.0);
            });
            Text::new(format!(
                "value {:.0} px, velocity {:.0} px/s",
                value, velocity
            ))
            .font_size(10.0)
            .color(MUTED)
            .build(ctx);
            Row::new().spacing(8.0).build(ctx, |ctx| {
                small_button(ctx, "to 0", {
                    let a = anim.clone();
                    move || a.animate_to(0.0, AnimationSpec::Spring(SpringSpec::default()))
                });
                small_button(ctx, "to 420", {
                    let a = anim.clone();
                    move || a.animate_to(420.0, AnimationSpec::Spring(SpringSpec::default()))
                });
                small_button(ctx, "sequence", {
                    let a = anim.clone();
                    move || {
                        a.animate_sequence([
                            (420.0, AnimationSpec::Tween(TweenSpec::default())),
                            (0.0, AnimationSpec::Spring(SpringSpec::bouncy())),
                        ])
                    }
                });
                small_button(ctx, "fling", {
                    let a = anim.clone();
                    move || a.animate_decay(900.0, DecaySpec::default())
                });
                small_button(ctx, "stop", {
                    let a = anim.clone();
                    move || {
                        a.stop();
                    }
                });
            });
        },
    );
}

// 9. updateTransition: three states, several values plus a child transition.
#[composable]
fn transition_card(ctx: &mut ComposeCtx) {
    let state = ctx.remember(|| State::new(0i32)).get();
    let spec = AnimationSpec::Spring(SpringSpec {
        stiffness: 700.0,
        damping_ratio: 0.9,
        ..SpringSpec::default()
    });
    let mut transition = ctx.update_transition(state.get(), spec.clone(), "demo");
    let x = transition.animate_float(ctx, |s| *s as f32 * 200.0, "x");
    let color = transition.animate_color(
        ctx,
        |s| match *s {
            0 => ACCENT,
            1 => WARM,
            _ => GREEN,
        },
        "color",
    );
    let size = transition.animate_float(ctx, |s| 20.0 + *s as f32 * 8.0, "size");
    card(
        ctx,
        "8. updateTransition (three states)",
        "One state machine animates position, colour and size together; every presses cycles the state.",
        |ctx| {
            track(ctx, 64.0, |ctx| {
                Spacer::new()
                    .modifier(Modifier::new().width(x.get().max(0.0)))
                    .build(ctx);
                dot(ctx, color.get(), size.get().max(4.0));
            });
            small_button(ctx, "next state", move || state.set((state.peek() + 1) % 3));
        },
    );
}

// 10. AnimatedVisibility: fade + slide in and out.
#[composable]
fn visibility_card(ctx: &mut ComposeCtx) {
    let visible = ctx.remember(|| State::new(true)).get();
    card(
        ctx,
        "9. AnimatedVisibility (fade + slide)",
        "The chip enters with a fade and a slide from the left, and leaves the same way.",
        |ctx| {
            Row::new()
                .modifier(Modifier::new().fill_max_width().height(56.0))
                .build(ctx, |ctx| {
                    AnimatedVisibility::new(visible.clone())
                        .enter(
                            VisibilityTransition::fade_in(TweenSpec::default())
                                .with_slide(winia::animation::SlideDirection::Left),
                        )
                        .exit(
                            VisibilityTransition::fade_out(TweenSpec::default())
                                .with_slide(winia::animation::SlideDirection::Left),
                        )
                        .build(ctx, |ctx| {
                            Text::new("visible chip")
                                .font_size(12.0)
                                .modifier(
                                    Modifier::new()
                                        .padding_sides(12.0, 8.0, 12.0, 8.0)
                                        .background(WARM, Shape::rounded(14.0)),
                                )
                                .build(ctx);
                        });
                });
            small_button(ctx, "toggle visibility", move || {
                visible.set(!visible.peek())
            });
        },
    );
}

// 11. Crossfade between two labels.
#[composable]
fn crossfade_card(ctx: &mut ComposeCtx) {
    let target = ctx.remember(|| State::new(0i32)).get();
    card(
        ctx,
        "10. Crossfade",
        "The outgoing content fades out while the incoming one fades in over the same 300 ms.",
        |ctx| {
            Crossfade::new(target.clone())
                .animation(TweenSpec::default())
                .build(ctx, |ctx, value| {
                    let (label, color) = match value {
                        0 => ("first", ACCENT),
                        1 => ("second", WARM),
                        _ => ("third", GREEN),
                    };
                    Text::new(label).font_size(16.0).color(color).build(ctx);
                });
            small_button(ctx, "crossfade", move || {
                target.set((target.peek() + 1) % 3)
            });
        },
    );
}

// 12. AnimatedSize: the box follows its content's size.
#[composable]
fn animated_size_card(ctx: &mut ComposeCtx) {
    let big = ctx.remember(|| State::new(false)).get();
    card(
        ctx,
        "11. AnimatedSize",
        "The container animates to the size of whatever it holds; the content jumps, the box eases.",
        |ctx| {
            AnimatedSize::new(TweenSpec::default()).build(ctx, |ctx| {
                Column::new()
                    .modifier(
                        Modifier::new()
                            .size(
                                if big.get() { 260.0 } else { 90.0 },
                                if big.get() { 70.0 } else { 36.0 },
                            )
                            .background(if big.get() { GREEN } else { WARM }, Shape::rounded(10.0)),
                    )
                    .build(ctx, |_| {});
            });
            small_button(ctx, "resize content", move || big.set(!big.peek()));
        },
    );
}

#[composable]
fn animation_demo(ctx: &mut ComposeCtx) {
    tween_card(ctx);
    spring_card(ctx);
    values_card(ctx);
    keyframes_card(ctx);
    snap_repeat_card(ctx);
    infinite_card(ctx);
    animatable_card(ctx);
    transition_card(ctx);
    visibility_card(ctx);
    crossfade_card(ctx);
    animated_size_card(ctx);
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(760.0, 1500.0)
                .title("Animation demo")
                .build(ctx, |ctx| {
                    settings::shell("Animation", ctx, |ctx, scaffold| {
                        let page = ctx.remember(ScrollState::new).get();
                        Column::new()
                            .spacing(12.0)
                            .modifier(
                                Modifier::new()
                                    .fill_max_size()
                                    .padding_sides(
                                        scaffold.start,
                                        scaffold.top,
                                        scaffold.end,
                                        scaffold.bottom,
                                    )
                                    .vertical_scroll(page)
                                    .padding(16.0),
                            )
                            .build(ctx, |scope| animation_demo(scope));
                    });
                });
        });
    });
}
