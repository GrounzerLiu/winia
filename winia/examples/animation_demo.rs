//! Animation demo — the whole animation system on one page.
//!
//! Seven groups, each answering a different question about how motion gets its value:
//!
//! - **A. Every-frame targets** — `animate_float_as_state` / `animate_dp_as_state` /
//!   `animate_int_as_state` / `animate_value_as_state`: hand the engine the value it should be at and
//!   it moves there. Spring against tween, `with_visibility_threshold`, `with_done`.
//! - **B. State machines** — `update_transition`: one `Transition` over an enum drives several values,
//!   each animation can pick its spec from the PAIR of states (`transitionSpec`), and a child
//!   transition derives its own target from the parent.
//! - **C. Loops** — `remember_infinite_transition`: reverse/restart pulses, the cyclic curve,
//!   `with_start_offset` (a loop that begins part-way in) and a reverse loop carrying a curve
//!   (`with_base`), whose return leg is that curve running backwards.
//! - **D. Explicit timelines** — `KeyframesOf`: absolute milliseconds, values in the animated type's
//!   units, easing per keyframe.
//! - **E. Impulse and control** — `push_decay` / `push_decay_vector` (fling, scalar and diagonal),
//!   `SnapSpec` (hold, then jump), `RepeatableSpec` (n cycles, optionally fast-forwarded), the
//!   `Animatable` handle (`animate_to` / `snap_to` / `stop` / `is_running` / `target`), a value range
//!   (`set_bounds`) and a queued `animate_sequence` — both reporting WHY they ended (`MotionEnd`).
//! - **F. Containers** — `AnimatedSize` (content size changes animate the box), `Crossfade`,
//!   `AnimatedContent`. `AnimatedVisibility` has its own demo (`animated_visibility_demo`).
//! - **G. Scheme and clock** — the theme's `MotionScheme` (the same move on standard and expressive
//!   motion, whose springs the components ask the theme for) and `TimedAnimation`, which answers what a
//!   spec is worth at any time — the plot at the bottom is drawn by sampling it, nothing running.
//!
//! Each group is its own `#[composable]` function, which is also why an animation inside one group only
//! re-runs that group: winia's invalidation is scoped to the function that read the state, the way a
//! Compose `@Composable` does it.
//!
//! Run: `cargo run -p winia --example animation_demo --features debug-server` (the feature is what lets
//! the debug server read stdin).

use letclone::clone;
use std::sync::Arc;
use std::time::Duration;
use winia::animation::interpolator::{CubicBezier, EaseInOutCubic, EaseOutCubic, Linear};
use winia::animation::{
    self, AnimationSpec, InfiniteRepeatableSpec, KeyframesOf, RepeatableSpec, SnapSpec, SpringSpec,
    StartOffsetType, TimedAnimation, TweenSpec, interpolator,
};
use winia::graphics::GraphicsLayerParams;
use winia::layout::Alignment;
use winia::prelude::*;

// Shared example chrome: top app bar with the settings sheet (theme mode + layout direction).
#[path = "common/settings.rs"]
mod settings;

/// The page's shared toggle: most target-driven animations follow it, the compound ones have their own
/// controls.
#[composable]
fn animation_demo(ctx: &mut ComposeCtx) {
    let clicked = ctx.remember(|| false);
    let scroll_y = ctx.remember(|| ScrollState::new()).get();

    Column::new()
        .modifier(Modifier::new().padding(16.0).fill_max_size())
        .build(ctx, |ctx| {
            Row::new()
                .modifier(Modifier::new().fill_max_width())
                .alignment(Alignment::Center)
                .build(ctx, |ctx| {
                    Column::new()
                        .modifier(Modifier::new().layout_weight(1.0))
                        .build(ctx, |_| {});
                    Button::new()
                        .on_click({
                            clone!(clicked);
                            move || clicked.update(|v| *v = !*v)
                        })
                        .build(ctx, |ctx| {
                            Text::new(if clicked.get() { "Reset" } else { "Animate!" })
                                .font_size(16.0)
                                .build(ctx);
                        });
                });

            Column::new()
                .modifier(
                    Modifier::new()
                        .padding_vertical(8.0)
                        .fill_max_size()
                        .vertical_scroll(scroll_y),
                )
                .build(ctx, |ctx| {
                    intro(ctx);
                    group_a(ctx, &clicked);
                    group_b(ctx);
                    group_c(ctx);
                    group_d(ctx, &clicked);
                    group_e(ctx);
                    group_f(ctx, &clicked);
                    group_g(ctx, &clicked);
                });
        });
}

/// One consistent heading + caption pair, so the groups read as a page rather than as loose widgets.
#[composable]
fn heading(ctx: &mut ComposeCtx, title: &str, caption: &str) {
    Text::new(title)
        .font_size(15.0)
        .modifier(Modifier::new().padding_top(14.0).padding_bottom(2.0))
        .build(ctx);
    Text::new(caption)
        .font_size(11.0)
        .color(WiniaTheme::colors().on_surface_variant)
        .build(ctx);
}

#[composable]
fn intro(ctx: &mut ComposeCtx) {
    Text::new("The animation system, by the question each API answers")
        .font_size(16.0)
        .build(ctx);
    Text::new("Press Animate! above for the target-driven groups; the rest carry their own controls.")
        .font_size(11.0)
        .color(WiniaTheme::colors().on_surface_variant)
        .build(ctx);
}

// ─────────────────────────────────────────────────────────────────────────────
// A. Every-frame targets
// ─────────────────────────────────────────────────────────────────────────────

#[composable]
fn group_a(ctx: &mut ComposeCtx, clicked: &winia::runtime::state::State<bool>) {
    heading(
        ctx,
        "A. Every-frame targets",
        "You say where the value should be; the engine gets it there. Same target, different engines.",
    );

    // Spring against tween on the same move: the spring is soft at both ends and overshoots nothing
    // (damping 1.0), the tween is linear and arrives exactly on time.
    let spring = ctx.animate_float_as_state(
        if clicked.get() { 200.0 } else { 40.0 },
        AnimationSpec::Spring(SpringSpec {
            stiffness: SpringSpec::STIFFNESS_LOW,
            ..SpringSpec::default()
        }),
    );
    let tween = ctx.animate_float_as_state(
        if clicked.get() { 200.0 } else { 40.0 },
        AnimationSpec::Tween(TweenSpec::new(Duration::from_millis(500), Linear::new())),
    );
    bar(ctx, &spring, 1.0, 18.0, Color::from_argb(255, 63, 81, 181), "spring");
    bar(ctx, &tween, 1.0, 18.0, Color::from_argb(255, 0, 150, 136), "tween 500ms linear");

    // A looser visibility threshold is Compose's `visibilityThreshold` argument: the spring counts
    // itself arrived sooner, so the bar snaps the last stretch.
    let loose = ctx.animate_float_as_state(
        if clicked.get() { 200.0 } else { 40.0 },
        AnimationSpec::Spring(SpringSpec {
            stiffness: SpringSpec::STIFFNESS_LOW,
            ..SpringSpec::default()
        })
        .with_visibility_threshold(40.0),
    );
    bar(
        ctx,
        &loose,
        1.0,
        14.0,
        Color::from_argb(255, 255, 152, 0),
        "spring, threshold 40",
    );

    // Colour, animated in Oklab with a spring per channel (Compose's Color.VectorConverter).
    let colour = ctx.animate_color_as_state(
        if clicked.get() {
            Color::from_argb(255, 76, 175, 80)
        } else {
            Color::from_argb(255, 233, 30, 99)
        },
        AnimationSpec::Spring(SpringSpec::default()),
    );
    swatch(ctx, &colour, 24.0, "colour spring (Oklab)");

    // Int and Dp: same machinery, different value types.
    let count = animation::animate_int_as_state(
        ctx,
        if clicked.get() { 100 } else { 0 },
        TweenSpec::new(Duration::from_millis(400), EaseOutCubic::new()),
    );
    Text::new(format!("animate_int_as_state: {}", count.get()))
        .font_size(12.0)
        .color(WiniaTheme::colors().on_surface_variant)
        .build(ctx);
    let width = ctx.animate_dp_as_state(
        if clicked.get() { 30.dp() } else { 200.dp() },
        AnimationSpec::Spring(SpringSpec::default()),
    );
    Column::new()
        .modifier(
            Modifier::new()
                .size(&width, 12.0)
                .background(Color::from_argb(255, 96, 125, 139), Shape::rounded(3.0)),
        )
        .build(ctx, |_| {});

    // A vector value: `Offset` springs on each axis independently, exactly as Compose's converter does.
    let offset = animation::animate_value_as_state(
        ctx,
        if clicked.get() {
            Offset::new(120.0, 16.0)
        } else {
            Offset::new(0.0, 0.0)
        },
        AnimationSpec::Spring(SpringSpec::default()),
    );
    Column::new()
        .modifier(Modifier::new().size(200.0, 40.0))
        .build(ctx, |ctx| {
            let o = offset.get();
            Column::new()
                .modifier(
                    Modifier::new()
                        .offset(o.x, o.y)
                        .size(24.0, 16.0)
                        .background(Color::from_argb(255, 156, 39, 176), Shape::rounded(3.0)),
                )
                .build(ctx, |_| {});
        });

    // Compose's `finishedListener`, on the slot-owned state so nothing can drive it twice.
    let done = ctx.remember(|| false);
    let with_done = ctx.animate_float_as_state_with_done(
        if clicked.get() { 1.0 } else { 0.2 },
        AnimationSpec::Spring(SpringSpec {
            damping_ratio: SpringSpec::DAMPING_RATIO_HIGH_BOUNCY,
            stiffness: SpringSpec::STIFFNESS_HIGH,
            ..SpringSpec::default()
        }),
        {
            clone!(done);
            move || done.set(true)
        },
    );
    bar(
        ctx,
        &with_done,
        200.0,
        14.0,
        Color::from_argb(255, 121, 85, 72),
        if done.get() {
            "with_done: callback fired"
        } else {
            "with_done: running"
        },
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// B. State machines
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Debug)]
enum Phase {
    Idle,
    Loading,
    Done,
}

#[composable]
fn group_b(ctx: &mut ComposeCtx) {
    heading(
        ctx,
        "B. State machines (update_transition)",
        "One transition over the phase drives width, colour and slide. The outbound leg is quick, the return is slow — a per-pair transitionSpec.",
    );
    let phase = ctx.remember(|| Phase::Idle);
    Row::new().build(ctx, |ctx| {
        for (label, next) in [
            ("Idle", Phase::Idle),
            ("Loading", Phase::Loading),
            ("Done", Phase::Done),
        ] {
            Button::new()
                .on_click({
                    clone!(phase);
                    move || phase.set(next)
                })
                .build(ctx, |ctx| {
                    Text::new(label).font_size(11.0).build(ctx);
                });
        }
    });

    let current = phase.get();
    let quick = AnimationSpec::Tween(TweenSpec::new(Duration::from_millis(180), EaseOutCubic::new()));
    let slow = AnimationSpec::Tween(TweenSpec::new(Duration::from_millis(900), EaseOutCubic::new()));
    let mut transition = ctx.update_transition(current, quick.clone(), "phase");

    // Per-pair specs: leaving Idle is the quick leg, everything else eases back slowly. Compose's
    // `transitionSpec = { initial, target -> … }` sees the same pair.
    let spec_for = {
        let quick = quick.clone();
        let slow = slow.clone();
        move |from: &Phase, _to: &Phase| {
            if *from == Phase::Idle {
                quick.clone()
            } else {
                slow.clone()
            }
        }
    };
    let width = transition.animate_float_with_spec(
        ctx,
        |p| match p {
            Phase::Idle => 80.0,
            Phase::Loading => 180.0,
            Phase::Done => 260.0,
        },
        "width",
        spec_for.clone(),
    );
    let tint = transition.animate_color_with_spec(
        ctx,
        |p| match p {
            Phase::Idle => Color::from_argb(255, 96, 125, 139),
            Phase::Loading => Color::from_argb(255, 255, 152, 0),
            Phase::Done => Color::from_argb(255, 76, 175, 80),
        },
        "tint",
        spec_for,
    );
    let slide = transition.animate_float(
        ctx,
        |p| match p {
            Phase::Idle => 0.0,
            Phase::Loading => 20.0,
            Phase::Done => 40.0,
        },
        "slide",
    );
    Column::new()
        .modifier(Modifier::new().height(56.0).fill_max_width())
        .build(ctx, |ctx| {
            let y = slide.get();
            let w = width.get();
            let c = tint.get();
            Column::new()
                .modifier(
                    Modifier::new()
                        .offset(0.0, y)
                        .size(w, 40.0)
                        .background(c, Shape::rounded(6.0))
                        .padding(4.0),
                )
                .build(ctx, |ctx| {
                    Text::new(format!(
                        "{current:?}  {w:.0}px  from {:?}",
                        transition.previous()
                    ))
                    .font_size(11.0)
                    .color(Color::WHITE)
                    .build(ctx);
                });
        });

    // A child transition derives its own target from the parent's, so it follows along.
    let mut child = transition.create_child_transition(|p| matches!(p, Phase::Done));
    let dot = child.animate_float(ctx, |d| if *d { 1.0 } else { 0.2 }, "dot");
    Row::new()
        .modifier(Modifier::new().height(20.0))
        .alignment(Alignment::Center)
        .build(ctx, |ctx| {
            Column::new()
                .modifier(
                    Modifier::new()
                        .size(16.0 * dot.get(), 16.0)
                        .background(Color::from_argb(255, 63, 81, 181), Shape::Circle)
                        .graphics_layer(|| GraphicsLayerParams::default()),
                )
                .build(ctx, |_| {});
            Text::new("child transition: on when the phase is Done")
                .font_size(11.0)
                .color(WiniaTheme::colors().on_surface_variant)
                .build(ctx);
        });
}

// ─────────────────────────────────────────────────────────────────────────────
// C. Loops
// ─────────────────────────────────────────────────────────────────────────────

#[composable]
fn group_c(ctx: &mut ComposeCtx) {
    heading(
        ctx,
        "C. Loops (remember_infinite_transition)",
        "Five loops off one scope: reverse, restart with a curve, keyframes, one that starts part-way in (FastForward), and a reverse loop whose return leg runs that same curve backwards.",
    );
    let mut infinite = ctx.remember_infinite_transition();
    let pulse = infinite.animate_float(
        ctx,
        0.35,
        1.0,
        InfiniteRepeatableSpec::reverse(Duration::from_millis(600)),
    );
    let sweep = infinite.animate_float(
        ctx,
        0.0,
        1.0,
        InfiniteRepeatableSpec::restart_tween(
            Duration::from_millis(1400),
            TweenSpec::new(Duration::from_millis(1400), EaseOutCubic::new()),
        ),
    );
    let steps = infinite.animate_float(
        ctx,
        0.0,
        1.0,
        InfiniteRepeatableSpec::restart_keyframes(
            Duration::from_millis(1200),
            vec![
                (0.0, 0.0, Arc::new(Linear::new()) as Arc<dyn interpolator::Interpolator>),
                (0.5, 1.0, Arc::new(EaseOutCubic::new()) as Arc<dyn interpolator::Interpolator>),
                (1.0, 0.0, Arc::new(Linear::new()) as Arc<dyn interpolator::Interpolator>),
            ],
        ),
    );
    let ahead = infinite.animate_float(
        ctx,
        0.0,
        1.0,
        InfiniteRepeatableSpec::restart(Duration::from_millis(1200))
            .with_start_offset(Duration::from_millis(900), StartOffsetType::FastForward),
    );
    // The reverse shape carrying a CURVE: `Reverse` mirrors the iteration time, so the return leg is
    // `fast_out_slow_in` running backwards rather than a linear trip back (`with_base`).
    let curved = infinite.animate_float(
        ctx,
        0.0,
        1.0,
        InfiniteRepeatableSpec::reverse(Duration::from_millis(1000)).with_base(TweenSpec::new(
            Duration::from_millis(1000),
            Arc::new(interpolator::fast_out_slow_in()) as Arc<dyn interpolator::Interpolator>,
        )),
    );

    Row::new()
        .modifier(Modifier::new().height(44.0))
        .alignment(Alignment::Center)
        .build(ctx, |ctx| {
            // The values of an infinite transition are `Visual`s updated at PAINT time (no
            // recomposition), so everything here reads them through closures the renderer calls per
            // frame — that is what makes the loops free of rebuilds.
            Column::new()
                .modifier(
                    Modifier::new()
                        .size(34.0, 34.0)
                        .graphics_layer({
                            clone!(pulse);
                            move || GraphicsLayerParams {
                                alpha: pulse.peek(),
                                ..Default::default()
                            }
                        })
                        .background(Color::from_argb(255, 33, 150, 243), Shape::Circle),
                )
                .build(ctx, |_| {});
            Column::new()
                .modifier(
                    Modifier::new()
                        .size(120.0, 10.0)
                        .background(Color::from_argb(60, 0, 0, 0), Shape::rounded(4.0)),
                )
                .build(ctx, |ctx| {
                    Column::new()
                        .modifier(
                            Modifier::new()
                                .size(14.0, 10.0)
                                .graphics_layer({
                                    clone!(sweep);
                                    move || GraphicsLayerParams {
                                        translation_x: sweep.peek() * 106.0,
                                        ..Default::default()
                                    }
                                })
                                .background(Color::from_argb(255, 0, 150, 136), Shape::rounded(4.0)),
                        )
                        .build(ctx, |_| {});
                });
            Column::new()
                .modifier(
                    Modifier::new()
                        .size(60.0, 32.0)
                        .graphics_layer({
                            clone!(steps);
                            move || GraphicsLayerParams {
                                scale_x: 1.0,
                                scale_y: steps.peek().max(0.05),
                                ..Default::default()
                            }
                        })
                        .background(Color::from_argb(255, 255, 152, 0), Shape::rounded(3.0)),
                )
                .build(ctx, |_| {});
            Column::new()
                .modifier(
                    Modifier::new()
                        .size(124.0, 6.0)
                        .background(Color::from_argb(60, 0, 0, 0), Shape::rounded(3.0)),
                )
                .build(ctx, |ctx| {
                    Column::new()
                        .modifier(
                            Modifier::new()
                                .size(12.0, 6.0)
                                .graphics_layer({
                                    clone!(ahead);
                                    move || GraphicsLayerParams {
                                        translation_x: ahead.peek() * 112.0,
                                        ..Default::default()
                                    }
                                })
                                .background(Color::from_argb(255, 233, 30, 99), Shape::rounded(3.0)),
                        )
                        .build(ctx, |_| {});
                });
            Column::new()
                .modifier(
                    Modifier::new()
                        .size(70.0, 12.0)
                        .graphics_layer({
                            clone!(curved);
                            move || GraphicsLayerParams {
                                scale_x: 0.12 + curved.peek() * 0.88,
                                ..Default::default()
                            }
                        })
                        .background(Color::from_argb(255, 156, 39, 176), Shape::rounded(3.0)),
                )
                .build(ctx, |_| {});
        });
}

// ─────────────────────────────────────────────────────────────────────────────
// D. Explicit timelines
// ─────────────────────────────────────────────────────────────────────────────

#[composable]
fn group_d(ctx: &mut ComposeCtx, clicked: &winia::runtime::state::State<bool>) {
    heading(
        ctx,
        "D. Explicit timelines (KeyframesOf)",
        "Compose's keyframe shape: milliseconds and values in the animated type's units, with easing per keyframe. This one overshoots to 260 on a 40 → 190 flight.",
    );
    let spec = KeyframesOf::between(40.0, 190.0)
        .at(0, 40.0)
        .at(220, 260.0)
        .at_with(420, 150.0, Arc::new(CubicBezier::new(0.4, 0.0, 0.2, 1.0)))
        .at(600, 190.0)
        .build();
    let width = ctx.animate_float_as_state(
        if clicked.get() { 190.0 } else { 40.0 },
        AnimationSpec::Keyframes(spec),
    );
    bar(
        ctx,
        &width,
        1.0,
        22.0,
        Color::from_argb(255, 0, 150, 136),
        "keyframes: 40 → 260 → 150 → 190 over 600ms",
    );

    // Behind it, the same timeline drawn as ticks so the shape is visible while it moves.
    let ticks = ctx.animate_float_as_state(
        if clicked.get() { 1.0 } else { 0.0 },
        AnimationSpec::Keyframes(
            KeyframesOf::between(0.0, 1.0)
                .at_with(0, 0.0, Arc::new(EaseOutCubic::new()))
                .at_with(300, 1.0, Arc::new(EaseOutCubic::new()))
                .build(),
        ),
    );
    Row::new()
        .modifier(Modifier::new().height(10.0).fill_max_width())
        .build(ctx, |ctx| {
            Column::new()
                .modifier(
                    Modifier::new()
                        .size(200.0 * ticks.get(), 8.0)
                        .background(Color::from_argb(255, 121, 85, 72), Shape::rounded(2.0)),
                )
                .build(ctx, |_| {});
        });
}

// ─────────────────────────────────────────────────────────────────────────────
// E. Impulse and control
// ─────────────────────────────────────────────────────────────────────────────

#[composable]
fn group_e(ctx: &mut ComposeCtx) {
    heading(
        ctx,
        "E. Impulse and control",
        "Motion a caller starts itself: a fling with no target, a delayed jump, a fixed number of cycles, and a value driven by hand.",
    );

    // A fling: no target, it just decays. The velocity is the value's own type, so the diagonal one
    // travels on both axes independently.
    let scalar = ctx.remember_animatable(0.0f32);
    let diagonal = ctx.remember_animatable(Offset::new(0.0, 0.0));
    Row::new().build(ctx, |ctx| {
        Button::new()
            .on_click({
                clone!(scalar);
                move || {
                    scalar.snap_to(0.0);
                    animation::push_decay(
                        scalar.as_state(),
                        900.0,
                        animation::exponential_decay(4.2),
                    );
                }
            })
            .build(ctx, |ctx| {
                Text::new("fling").font_size(11.0).build(ctx);
            });
        Button::new()
            .on_click({
                clone!(diagonal);
                move || {
                    diagonal.snap_to(Offset::new(0.0, 0.0));
                    animation::push_decay_vector(
                        diagonal.as_state(),
                        Offset::new(700.0, -260.0),
                        animation::exponential_decay(4.2),
                    );
                }
            })
            .build(ctx, |ctx| {
                Text::new("diagonal fling").font_size(11.0).build(ctx);
            });
        Button::new()
            .on_click({
                clone!(scalar, diagonal);
                move || {
                    let v = scalar.stop();
                    diagonal.stop();
                    scalar.snap_to(v / 900.0);
                }
            })
            .build(ctx, |ctx| {
                Text::new("stop (keep velocity)").font_size(11.0).build(ctx);
            });
    });
    let scalar_value = scalar.as_state().get();
    Row::new()
        .modifier(Modifier::new().height(20.0).fill_max_width())
        .build(ctx, |ctx| {
            Column::new()
                .modifier(
                    Modifier::new()
                        .offset(scalar_value * 120.0, 0.0)
                        .size(14.0, 14.0)
                        .background(Color::from_argb(255, 63, 81, 181), Shape::Circle),
                )
                .build(ctx, |_| {});
        });
    Row::new()
        .modifier(Modifier::new().height(48.0).fill_max_width())
        .build(ctx, |ctx| {
            let o = diagonal.as_state().get();
            let rest = diagonal
                .is_running()
                .then(|| " (flinging)".to_string())
                .unwrap_or_default();
            Column::new()
                .modifier(
                    Modifier::new()
                        .offset(o.x * 0.16, o.y * 0.12 + 12.0)
                        .size(14.0, 14.0)
                        .background(Color::from_argb(255, 0, 150, 136), Shape::Circle),
                )
                .build(ctx, |_| {});
            Text::new(format!("offset ({:.0}, {:.0}){rest}", o.x, o.y))
                .font_size(11.0)
                .color(WiniaTheme::colors().on_surface_variant)
                .build(ctx);
        });

    // snap(delayMillis): hold the value where it is, then jump. repeatable: a fixed number of cycles,
    // the second one fast-forwarded a quarter of a cycle in.
    let jump = ctx.remember_animatable(0.0f32);
    let cycles = ctx.remember_animatable(0.0f32);
    Row::new().build(ctx, |ctx| {
        Button::new()
            .on_click({
                clone!(jump);
                move || {
                    jump.snap_to(0.0);
                    jump.animate_to(1.0, AnimationSpec::Snap(SnapSpec::new(Duration::from_millis(600))));
                }
            })
            .build(ctx, |ctx| {
                Text::new("snap after 600ms").font_size(11.0).build(ctx);
            });
        Button::new()
            .on_click({
                clone!(cycles);
                move || {
                    cycles.snap_to(0.0);
                    cycles.animate_to(
                        1.0,
                        AnimationSpec::Repeatable(
                            RepeatableSpec::new(
                                3,
                                animation::RepeatMode::Restart,
                                TweenSpec::new(
                                    Duration::from_millis(300),
                                    EaseOutCubic::new(),
                                ),
                            )
                            .with_start_offset(
                                Duration::from_millis(220),
                                StartOffsetType::FastForward,
                            ),
                        ),
                    );
                }
            })
            .build(ctx, |ctx| {
                Text::new("3 cycles, fast-forwarded").font_size(11.0).build(ctx);
            });
    });
    let jump_value = jump.as_state().get();
    let cycles_value = cycles.as_state().get();
    Text::new(format!(
        "snap: {jump_value:.2}   repeatable: {cycles_value:.2}   (both jump back to 0 when restarted)"
    ))
    .font_size(11.0)
    .color(WiniaTheme::colors().on_surface_variant)
    .build(ctx);

    // The Animatable handle: start, jump, and cancel while keeping the velocity. The readout shows what
    // the handle can be asked about — value, target, isRunning, velocity.
    let handle = ctx.remember_animatable(0.0f32);
    let stopped = ctx.remember(|| 0.0f32);
    Row::new().build(ctx, |ctx| {
        Button::new()
            .on_click({
                clone!(handle);
                move || {
                    handle.animate_to(
                        1.0,
                        AnimationSpec::Spring(SpringSpec {
                            stiffness: 120.0,
                            ..SpringSpec::default()
                        }),
                    )
                }
            })
            .build(ctx, |ctx| {
                Text::new("animate_to(1)").font_size(11.0).build(ctx);
            });
        Button::new()
            .on_click({
                clone!(handle);
                move || handle.snap_to(0.0)
            })
            .build(ctx, |ctx| {
                Text::new("snap_to(0)").font_size(11.0).build(ctx);
            });
        Button::new()
            .on_click({
                clone!(handle, stopped);
                move || stopped.set(handle.stop())
            })
            .build(ctx, |ctx| {
                Text::new("stop").font_size(11.0).build(ctx);
            });
    });
    let live = handle.as_state().get();
    let handle_state = handle.as_state();
    bar(
        ctx,
        &handle_state,
        200.0,
        18.0,
        Color::from_argb(255, 0, 121, 107),
        &format!(
            "Animatable: value {live:.3}  target {}  running {}  velocity {:.1}  stopped at {:.1}",
            match handle.target() {
                Some(t) => format!("{t:.2}"),
                None => "idle".to_string(),
            },
            handle.is_running(),
            handle.velocity(),
            stopped.get(),
        ),
    );

    // A value range and a queued sequence, both of which report WHY their motion ended — the thing a plain
    // "finished" callback cannot tell apart: arriving, stopping at a bound, or the engine's overrun guard.
    let bounded = ctx.remember_animatable(0.0f32);
    let queued = ctx.remember_animatable(0.0f32);
    let bounded_end = ctx.remember(|| "not run yet".to_string());
    Row::new().build(ctx, |ctx| {
        Button::new()
            .on_click({
                clone!(bounded, bounded_end);
                move || {
                    bounded.snap_to(0.0);
                    bounded.set_bounds(0.0, 1.0);
                    let report = bounded_end.clone();
                    bounded.animate_to_with_result(
                        5.0,
                        AnimationSpec::Spring(SpringSpec::spatial_default()),
                        move |end| report.set(format!("{:?}", end.reason)),
                    );
                }
            })
            .build(ctx, |ctx| {
                Text::new("to 5.0, bounds (0, 1)").font_size(11.0).build(ctx);
            });
        Button::new()
            .on_click({
                clone!(queued);
                move || {
                    queued.snap_to(0.0);
                    // Three legs in one call: each starts when the one before it ends.
                    queued.animate_sequence([
                        (
                            1.0,
                            AnimationSpec::Tween(TweenSpec::new(
                                Duration::from_millis(260),
                                EaseOutCubic::new(),
                            )),
                        ),
                        (0.35, AnimationSpec::Spring(SpringSpec::spatial_fast())),
                        (
                            0.8,
                            AnimationSpec::Tween(TweenSpec::new(
                                Duration::from_millis(320),
                                EaseInOutCubic::new(),
                            )),
                        ),
                    ]);
                }
            })
            .build(ctx, |ctx| {
                Text::new("sequence: 1 → 0.35 → 0.8").font_size(11.0).build(ctx);
            });
    });
    let bounded_value = bounded.as_state().get();
    let bounded_state = bounded.as_state();
    let queued_value = queued.as_state().get();
    let queued_state = queued.as_state();
    bar(
        ctx,
        &bounded_state,
        200.0,
        16.0,
        Color::from_argb(255, 255, 111, 0),
        &format!("bounded: {bounded_value:.3}  (ended: {})", bounded_end.get()),
    );
    bar(
        ctx,
        &queued_state,
        200.0,
        16.0,
        Color::from_argb(255, 63, 81, 181),
        &format!("sequence: {queued_value:.3}"),
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// F. Containers
// ─────────────────────────────────────────────────────────────────────────────

#[composable]
fn group_f(ctx: &mut ComposeCtx, clicked: &winia::runtime::state::State<bool>) {
    heading(
        ctx,
        "F. Containers",
        "Containers that animate their own layout or their children: AnimatedSize follows the content, Crossfade swaps it, AnimatedContent moves between screens.",
    );

    // AnimatedSize: the content's new size arrives at once, the box animates to it (Compose's
    // `animateContentSize`) — the border travels while the label inside is already its new width.
    AnimatedSize::new(TweenSpec::new(Duration::from_millis(450), EaseOutCubic::new()))
        .content_alignment(ContentAlignment::CENTER_START)
        .modifier(
            Modifier::new()
                .height(40.0)
                .border(1.0, WiniaTheme::colors().outline, Shape::rounded(6.0)),
        )
        .build(ctx, |ctx| {
            Text::new(if clicked.get() { "narrow" } else { "wide content" })
                .font_size(12.0)
                .modifier(Modifier::new().background(
                    Color::from_argb(255, 63, 81, 181),
                    Shape::rounded(4.0),
                ))
                .build(ctx);
        });

    // AnimatedContent: the same idea around a screen switch — the outgoing and the incoming screen
    // cross-fade while the container animates its size between them.
    let screen = ctx.remember(|| 0u32);
    AnimatedContent::new(screen.clone())
        .size_animation(SpringSpec::default())
        .modifier(Modifier::new().height(60.0).fill_max_width())
        .build(ctx, |ctx, s| {
            let (colour, label, height) = match s % 2 {
                0 => (Color::from_argb(255, 96, 125, 139), "screen A", 26.0),
                _ => (Color::from_argb(255, 0, 121, 107), "screen B — taller", 46.0),
            };
            Row::new()
                .modifier(
                    Modifier::new()
                        .height(height)
                        .background(colour, Shape::rounded(4.0))
                        .padding(6.0),
                )
                .alignment(Alignment::Center)
                .build(ctx, |ctx| {
                    Text::new(label).font_size(11.0).color(Color::WHITE).build(ctx);
                });
        });
    Button::new()
        .on_click({
            clone!(screen);
            move || screen.update(|v| *v = (*v + 1) % 2)
        })
        .build(ctx, |ctx| {
            Text::new("Next screen").font_size(11.0).build(ctx);
        });

    // Crossfade: N generations at once, each on its own clock.
    let page = ctx.remember(|| 0u32);
    Crossfade::new(page.clone())
        .animation(TweenSpec::new(Duration::from_millis(420), EaseOutCubic::new()))
        .build(ctx, |ctx, p| {
            let (colour, label) = match p % 3 {
                0 => (Color::from_argb(255, 76, 175, 80), "Page A"),
                1 => (Color::from_argb(255, 255, 152, 0), "Page B"),
                _ => (Color::from_argb(255, 233, 30, 99), "Page C"),
            };
            Row::new()
                .modifier(
                    Modifier::new()
                        .width(200.0)
                        .height(36.0)
                        .background(colour, Shape::rounded(4.0)),
                )
                .alignment(Alignment::Center)
                .build(ctx, |ctx| {
                    Text::new(label).color(Color::WHITE).build(ctx);
                });
        });

    Row::new().build(ctx, |ctx| {
        Button::new()
            .on_click({
                clone!(page);
                move || page.update(|v| *v = (*v + 1) % 3)
            })
            .build(ctx, |ctx| {
                Text::new("Next page").font_size(11.0).build(ctx);
            });
    });
}

// ─────────────────────────────────────────────────────────────────────────────
// G. Scheme and clock
// ─────────────────────────────────────────────────────────────────────────────

#[composable]
fn group_g(ctx: &mut ComposeCtx, clicked: &winia::runtime::state::State<bool>) {
    heading(
        ctx,
        "G. Motion scheme and the clock",
        "The theme decides a component's spring, so the same move reads differently under standard and expressive motion. Below that, TimedAnimation answers what a spec is worth at any time — the plot is sampled from it, with nothing running.",
    );
    let target = if clicked.get() { 1.0 } else { 0.0 };
    // Both bars make the SAME call with the SAME target; only the scheme in force differs. The caption
    // prints the springs so the numbers behind the motion are legible, since `spatial_default()` resolves
    // through the theme rather than naming a spring.
    let standard = ctx.animate_float_as_state(
        target,
        AnimationSpec::Spring(SpringSpec::spatial_default()),
    );
    let reference = SpringSpec::spatial_default();
    bar(
        ctx,
        &standard,
        200.0,
        18.0,
        Color::from_argb(255, 63, 81, 181),
        &format!(
            "standard scheme: spring({:.1}, {:.0})",
            reference.damping_ratio, reference.stiffness
        ),
    );
    WiniaTheme::with_motion_scheme(MotionScheme::expressive(), ctx, |ctx| {
        let expressive = ctx.animate_float_as_state(
            target,
            AnimationSpec::Spring(SpringSpec::spatial_default()),
        );
        let token = SpringSpec::spatial_default();
        bar(
            ctx,
            &expressive,
            200.0,
            18.0,
            Color::from_argb(255, 156, 39, 176),
            &format!(
                "expressive scheme: spring({:.1}, {:.0})",
                token.damping_ratio, token.stiffness
            ),
        );
    });

    // `TimedAnimation` is the engine's own evaluation as a function of time: this plot samples one spring
    // at 32 instants without registering or running anything. Its `duration()` is the estimate a running
    // spring is cut off at, so the plot shows the whole flight.
    let timed = TimedAnimation::target(
        0.0f32,
        1.0f32,
        AnimationSpec::Spring(SpringSpec::spatial_default()),
    );
    let duration = timed.duration();
    Text::new(format!(
        "TimedAnimation: {} ms of spring({:.1}, {:.0}), sampled at 32 times",
        duration.as_millis(),
        SpringSpec::spatial_default().damping_ratio,
        SpringSpec::spatial_default().stiffness
    ))
    .font_size(11.0)
    .color(WiniaTheme::colors().on_surface_variant)
    .build(ctx);
    Row::new()
        .modifier(Modifier::new().height(52.0).fill_max_width())
        .alignment(Alignment::End)
        .build(ctx, |ctx| {
            for i in 0..32 {
                let at = duration.mul_f32(i as f32 / 31.0);
                let value = timed.value_at(at).clamp(0.0, 1.15);
                Column::new()
                    .modifier(
                        Modifier::new()
                            .size(5.0, (value * 44.0).max(1.0))
                            .background(
                                Color::from_argb(255, 0, 150, 136),
                                Shape::rounded(2.0),
                            ),
                    )
                    .build(ctx, |_| {});
            }
        });
    Text::new("the ramp rises, overshoots slightly and settles: that is the 0.9 damping of the token above")
        .font_size(11.0)
        .color(WiniaTheme::colors().on_surface_variant)
        .build(ctx);
}

/// A labelled bar whose width is the animated value: `unit` is how many pixels one unit of the value is
/// worth, so a 0..1 animation and a 0..200 px animation both read on the same 200 px track. This is the
/// cheap way to watch any float animation.
#[composable]
fn bar(
    ctx: &mut ComposeCtx,
    value: &winia::runtime::state::State<f32>,
    unit: f32,
    height: f32,
    colour: Color,
    label: &str,
) {
    Text::new(label)
        .font_size(11.0)
        .color(WiniaTheme::colors().on_surface_variant)
        .build(ctx);
    Column::new()
        .modifier(Modifier::new().height(height).fill_max_width())
        .build(ctx, |ctx| {
            Column::new()
                .modifier(
                    Modifier::new()
                        .size((value.get() * unit).clamp(2.0, 200.0), height)
                        .background(colour, Shape::rounded(3.0)),
                )
                .build(ctx, |_| {});
        });
}

/// A labelled swatch whose fill is the animated colour, read at paint time.
#[composable]
fn swatch(
    ctx: &mut ComposeCtx,
    value: &winia::runtime::state::State<Color>,
    height: f32,
    label: &str,
) {
    let c = value.get();
    Text::new(format!(
        "{label}  #{:02X}{:02X}{:02X}",
        c.r, c.g, c.b
    ))
    .font_size(11.0)
    .color(WiniaTheme::colors().on_surface_variant)
    .build(ctx);
    Column::new()
        .modifier(
            Modifier::new()
                .height(height)
                .fill_max_width()
                .background(c, Shape::rounded(3.0)),
        )
        .build(ctx, |_| {});
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(460.0, 760.0)
                .title("Animation Demo")
                .build(ctx, |ctx| {
                    settings::shell("Animation Demo", ctx, |ctx| animation_demo(ctx));
                });
        });
    });
}
