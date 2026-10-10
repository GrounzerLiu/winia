//! AnimatedVisibility demo — content enter/exit animation (cf. Compose AnimatedVisibility)
//!
//! Run: cargo run -p winia --example animated_visibility_demo
//!
//! Ten panels, each showing one way to drive the component:
//! - A: fade_in + expand_in — which is also what `AnimatedVisibility::new(v)` alone does, since every
//!   Compose overload defaults to `fadeIn() + expand*()`
//! - B: slide_in(Right) + scale_in (slide from the right + scale)
//! - C: fade_out + shrink_out (fade + shrink — the fixed anchor below moves up)
//! - D: horizontal expand, both anchors (a fixed-width bar growing rightward from Start and a second
//!   one growing leftward from End)
//! - E: slide Fraction(1.0) (full-width slide-in — Compose's `initialOffsetX = { it }`)
//! - F: PER-CHANNEL specs — `fade_in(delayed tween) + expand_in(quick spring)`: the box is already at
//!   full height while the fade is still waiting (each channel is its own animatable, as in Compose)
//! - G: `build_with_scope` — the content reads `VisibilityScope` (Compose's `AnimatedVisibilityScope`)
//!   and moves a badge with the transition's own progress, at paint time
//! - H: `animate_on_appear()` beside the default — content composed already visible does NOT animate
//!   (the left box opts into the first-appearance enter, the right box shows the rule)
//! - I: `VisibilityTransition::empty()` on both sides (Compose's `EnterTransition.None` +
//!   `ExitTransition.None` — an instant switch, nothing animates)
//! - J: a CHILD with its own enter/exit — `VisibilityScope::animate_enter_exit`, Compose's
//!   `Modifier.animateEnterExit(enter, exit)`: one visibility target, two clocks (the container fades in
//!   120 ms, the child in 1200)

use letclone::clone;
use std::time::Duration;
use winia::animation::interpolator::{EaseOutCubic, Linear};
use winia::animation::{SpringSpec, TweenSpec};
use winia::graphics::GraphicsLayerParams;
use winia::prelude::*;

// Shared example chrome: top app bar with the settings sheet (theme mode + layout direction).
#[path = "common/settings.rs"]
mod settings;

fn main() {
    winia::run_app!(|ctx| {
        // Follow the system theme: with no theme node the window is pinned to the light default.
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(420.0, 1240.0)
                .title("AnimatedVisibility Demo")
                .build(ctx, |ctx| {
                    // Each panel keeps its own caption button — the top bar carries the window's
                    // title, and the cases are labelled individually. The chrome's content slot
                    // takes ONE child (it is a Scaffold slot), so the panels go into a Column; the
                    // window's root slot used to stack them by itself.
                    settings::shell("AnimatedVisibility Demo", ctx, |ctx| {
                        Column::new()
                            .modifier(Modifier::new().fill_max_size())
                            .build(ctx, |ctx| {
                                panel_a(ctx);
                                panel_b(ctx);
                                panel_c(ctx);
                                panel_d(ctx);
                                panel_e(ctx);
                                panel_f(ctx);
                                panel_g(ctx);
                                panel_h(ctx);
                                panel_i(ctx);
                                panel_j(ctx);
                            });
                    });
                });
        });
    });
}

/// Panel A: fade + expand (fade in/out plus a vertical expand/shrink). This is also what
/// `AnimatedVisibility::new(v)` does without `.enter`/`.exit`.
#[composable]
fn panel_a(ctx: &mut ComposeCtx) {
    let show = ctx.remember(|| true);
    Column::new()
        .modifier(Modifier::new().padding(16.0))
        .build(ctx, |ctx| {
            Button::new()
                .on_click({
                    clone!(show);
                    move || show.update(|v| *v = !*v)
                })
                .build(ctx, |ctx| {
                    Text::new("A: fade + expand (toggle)").font_size(12.0).build(ctx);
                });
            AnimatedVisibility::new(show.clone())
                .enter(VisibilityTransition::fade_in(TweenSpec::default()).with_expand())
                .exit(VisibilityTransition::fade_out(TweenSpec::default()).with_expand())
                .build(ctx, |ctx| {
                    Text::new("Panel A — fade + expand content")
                        .modifier(Modifier::new().background(Color::from_argb(255, 76, 175, 80), Shape::rounded(8.0)))
                        .build(ctx);
                });
        });
}

/// Panel B: slide + scale (slide in from the right, scale up). Neither builder carries a SIZE change,
/// so the layout never animates: the space this panel takes appears at its full height on the frame the
/// content arrives and collapses on the frame it is removed, which is why the panels below jump instead
/// of travelling. Compose behaves the same way — its KDoc says only an expanding/shrinking transition
/// animates the parent and siblings (`AnimatedVisibility.kt:157-158`) and that the content is removed
/// from the tree once the exit finishes (`:172-173`). Panel C is the contrast.
#[composable]
fn panel_b(ctx: &mut ComposeCtx) {
    let show = ctx.remember(|| false);
    Column::new()
        .modifier(Modifier::new().padding(16.0))
        .build(ctx, |ctx| {
            Button::new()
                .on_click({
                    clone!(show);
                    move || show.update(|v| *v = !*v)
                })
                .build(ctx, |ctx| {
                    Text::new("B: slide + scale (toggle) - no size change, so the panels below jump").font_size(12.0).build(ctx);
                });
            AnimatedVisibility::new(show.clone())
                .enter(
                    VisibilityTransition::slide_in(SlideDirection::Right, SpringSpec::default())
                        .with_scale(),
                )
                .exit(VisibilityTransition::slide_out(SlideDirection::Right, TweenSpec::default()))
                .build(ctx, |ctx| {
                    Text::new("Panel B — slide + scale content")
                        .modifier(Modifier::new().background(Color::from_argb(255, 33, 150, 243), Shape::rounded(8.0)))
                        .build(ctx);
                });
        });
}

/// Panel C: fade + shrink (the fixed anchor under it moves up as the panel collapses).
#[composable]
fn panel_c(ctx: &mut ComposeCtx) {
    let show = ctx.remember(|| true);
    Column::new()
        .modifier(Modifier::new().padding(16.0))
        .build(ctx, |ctx| {
            Button::new()
                .on_click({
                    clone!(show);
                    move || show.update(|v| *v = !*v)
                })
                .build(ctx, |ctx| {
                    Text::new("C: fade + shrink (toggle)").font_size(12.0).build(ctx);
                });
            AnimatedVisibility::new(show.clone())
                .enter(VisibilityTransition::expand_in(TweenSpec::default()))
                .exit(VisibilityTransition::fade_out(TweenSpec::default()).with_expand())
                .build(ctx, |ctx| {
                    Column::new().build(ctx, |ctx| {
                        Text::new("Panel C — fade + shrink content")
                            .modifier(Modifier::new().background(Color::from_argb(255, 255, 152, 0), Shape::rounded(8.0)))
                            .build(ctx);
                        // 内容下方锚点：C 收缩时此文本应跟随上移（布局动画验证）
                        Text::new("(the anchor below — it follows the shrink upward)").font_size(12.0).build(ctx);
                    });
                });
            // Fixed anchor outside the panel: always visible — moves up when C collapses.
            Text::new("── fixed anchor (moves up when panel C shrinks) ──").font_size(12.0).build(ctx);
        });
}

/// Panel D: horizontal expand only, with BOTH anchors. Two fixed-width bars share one toggle:
/// the purple one is anchored at Start (`expand_in_h` / `shrink_out_h`, the default) and grows
/// rightward, the amber one is anchored at End (`with_expand_h_from(ExpandFromH::End)`) and grows
/// leftward — Compose's `expandHorizontally(expandFrom = Alignment.End)`. No slide: pure width.
#[composable]
fn panel_d(ctx: &mut ComposeCtx) {
    let show = ctx.remember(|| false);
    Column::new()
        .modifier(Modifier::new().padding(16.0))
        .build(ctx, |ctx| {
            Button::new()
                .on_click({
                    clone!(show);
                    move || show.update(|v| *v = !*v)
                })
                .build(ctx, |ctx| {
                    Text::new("D: horizontal expand, both anchors (toggle)").font_size(12.0).build(ctx);
                });
            AnimatedVisibility::new(show.clone())
                .enter(VisibilityTransition::expand_in_h(TweenSpec::default()))
                .exit(VisibilityTransition::shrink_out_h(TweenSpec::default()))
                .build(ctx, |ctx| {
                    Text::new("Panel D — grows rightward from Start")
                        .font_size(14.0)
                        .color(Color::WHITE)
                        .modifier(
                            Modifier::new()
                                .size(300.0, 44.0)
                                .background(Color::from_argb(255, 156, 39, 176), Shape::rounded(8.0)),
                        )
                        .build(ctx);
                });
            AnimatedVisibility::new(show.clone())
                .enter(
                    VisibilityTransition::expand_in_h(TweenSpec::default())
                        .with_expand_h_from(ExpandFromH::End),
                )
                .exit(
                    VisibilityTransition::shrink_out_h(TweenSpec::default())
                        .with_expand_h_from(ExpandFromH::End),
                )
                .build(ctx, |ctx| {
                    Text::new("and leftward from End")
                        .font_size(14.0)
                        .color(Color::from_argb(255, 32, 32, 32))
                        .modifier(
                            Modifier::new()
                                .size(220.0, 34.0)
                                .background(Color::from_argb(255, 255, 193, 7), Shape::rounded(8.0)),
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
                    clone!(show);
                    move || show.update(|v| *v = !*v)
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

/// Panel F: PER-CHANNEL specs. Compose gives every channel its own animatable and its own
/// `animationSpec`, so `fadeIn(tween(delayMillis = 250)) + expandIn(spring)` fades late while the box
/// is already at full height. Combine two builders with `+` (a channel added later with a `with_*`
/// setter falls back to the shared spec instead, see `VisibilityTransition::plus`).
#[composable]
fn panel_f(ctx: &mut ComposeCtx) {
    let show = ctx.remember(|| false);
    // A deliberately slow fade that also waits, against a fast spring size change: the box is at full
    // height while the fade has barely started (watch it for ~2 s after the click).
    let slow_fade = TweenSpec::new(Duration::from_millis(1800), Linear::new())
        .delay(Duration::from_millis(400));
    let quick_size = SpringSpec { stiffness: 1400.0, ..SpringSpec::default() };
    Column::new()
        .modifier(Modifier::new().padding(16.0))
        .build(ctx, |ctx| {
            Button::new()
                .on_click({
                    clone!(show);
                    move || show.update(|v| *v = !*v)
                })
                .build(ctx, |ctx| {
                    Text::new("F: per-channel specs (toggle)").font_size(12.0).build(ctx);
                });
            AnimatedVisibility::new(show.clone())
                .enter(
                    VisibilityTransition::fade_in(slow_fade.clone())
                        + VisibilityTransition::expand_in(quick_size.clone()),
                )
                .exit(
                    VisibilityTransition::fade_out(TweenSpec::new(
                        Duration::from_millis(1200),
                        EaseOutCubic::new(),
                    )) + VisibilityTransition::shrink_out(TweenSpec::new(
                        Duration::from_millis(160),
                        EaseOutCubic::new(),
                    )),
                )
                .build(ctx, |ctx| {
                    Text::new("Panel F — full height before the fade starts")
                        .font_size(13.0)
                        .modifier(Modifier::new().background(Color::from_argb(255, 63, 81, 181), Shape::rounded(8.0)))
                        .build(ctx);
                });
        });
}

/// Panel G: a child riding the same clock — `build_with_scope` hands the content a `VisibilityScope`
/// (Compose's `AnimatedVisibilityScope`, whose `transition` exists for exactly this). The badge reads
/// the alpha channel at PAINT time through a graphics layer, so it moves with the fade without
/// re-composing.
#[composable]
fn panel_g(ctx: &mut ComposeCtx) {
    let show = ctx.remember(|| false);
    let spec = TweenSpec::new(Duration::from_millis(1600), EaseOutCubic::new());
    Column::new()
        .modifier(Modifier::new().padding(16.0))
        .build(ctx, |ctx| {
            Button::new()
                .on_click({
                    clone!(show);
                    move || show.update(|v| *v = !*v)
                })
                .build(ctx, |ctx| {
                    Text::new("G: scope-driven child (toggle)").font_size(12.0).build(ctx);
                });
            AnimatedVisibility::new(show.clone())
                .enter(VisibilityTransition::fade_in(spec.clone()).with_expand())
                .exit(VisibilityTransition::fade_out(spec.clone()).with_expand())
                .build_with_scope(ctx, |scope, ctx| {
                    let alpha = scope.alpha.clone();
                    Column::new().build(ctx, |ctx| {
                        Text::new("Panel G — the badge follows the fade")
                            .font_size(13.0)
                            .modifier(Modifier::new().background(Color::from_argb(255, 0, 121, 107), Shape::rounded(8.0)))
                            .build(ctx);
                        Row::new()
                            .modifier(Modifier::new().graphics_layer(move || GraphicsLayerParams {
                                alpha: alpha.peek(),
                                ..GraphicsLayerParams::default()
                            }))
                            .build(ctx, |ctx| {
                                Text::new("·")
                                    .font_size(30.0)
                                    .color(Color::from_argb(255, 255, 87, 34))
                                    .build(ctx);
                            });
                    });
                });
        });
}

/// Panel H: `animate_on_appear()` beside the default rule. Both boxes are visible from the first
/// frame; the left one opts into Compose's first-appearance enter (`MutableTransitionState(false)`
/// with `targetState = true`), the right one shows what a component composed already visible does —
/// nothing, because a transition never animates its initial state.
#[composable]
fn panel_h(ctx: &mut ComposeCtx) {
    let visible = ctx.remember(|| true);
    let spec = TweenSpec::new(Duration::from_millis(2000), EaseOutCubic::new());
    Row::new()
        .modifier(Modifier::new().padding(16.0))
        .build(ctx, |ctx| {
            AnimatedVisibility::new(visible.clone())
                .animate_on_appear()
                .enter(VisibilityTransition::fade_in(spec.clone()).with_expand())
                .exit(VisibilityTransition::fade_out(spec.clone()).with_expand())
                .build(ctx, |ctx| {
                    Text::new("H: animates in\n(animate_on_appear)")
                        .font_size(12.0)
                        .modifier(Modifier::new().background(Color::from_argb(255, 255, 235, 59), Shape::rounded(8.0)))
                        .build(ctx);
                });
            AnimatedVisibility::new(visible.clone())
                .enter(VisibilityTransition::fade_in(spec.clone()).with_expand())
                .exit(VisibilityTransition::fade_out(spec.clone()).with_expand())
                .build(ctx, |ctx| {
                    Text::new("H: already there\n(composed visible)")
                        .font_size(12.0)
                        .modifier(Modifier::new().background(Color::from_argb(255, 158, 158, 158), Shape::rounded(8.0)))
                        .build(ctx);
                });
        });
}

/// Panel J: a CHILD with its own enter/exit — Compose's `Modifier.animateEnterExit(enter, exit)`, through
/// `VisibilityScope::animate_enter_exit`. The container fades over 120 ms and the inner box over 1200 ms, so
/// a quarter of a second in the frame is already opaque while the child has barely started: one visibility
/// target, two clocks.
#[composable]
fn panel_j(ctx: &mut ComposeCtx) {
    let show = ctx.remember(|| false);
    Column::new()
        .modifier(Modifier::new().padding(16.0))
        .build(ctx, |ctx| {
            Button::new()
                .on_click({
                    clone!(show);
                    move || show.update(|v| *v = !*v)
                })
                .build(ctx, |ctx| {
                    Text::new("J: a child runs its own enter (toggle)").font_size(12.0).build(ctx);
                });
            AnimatedVisibility::new(show.clone())
                .enter(VisibilityTransition::fade_in(TweenSpec::new(
                    Duration::from_millis(120),
                    Linear::new(),
                )))
                .exit(VisibilityTransition::fade_out(TweenSpec::new(
                    Duration::from_millis(120),
                    Linear::new(),
                )))
                .build_with_scope(ctx, |scope, ctx| {
                    Column::new()
                        .modifier(
                            Modifier::new()
                                .padding(8.0)
                                .background(Color::from_argb(255, 76, 175, 80), Shape::rounded(8.0)),
                        )
                        .build(ctx, |ctx| {
                            Text::new("container: 120 ms fade").font_size(13.0).build(ctx);
                            // The child's own channels: it fades on its own clock, inside a box that is
                            // already there. `extent` is what a fraction slide would travel over.
                            let layer = scope.animate_enter_exit(
                                ctx,
                                VisibilityTransition::fade_in(TweenSpec::new(
                                    Duration::from_millis(1200),
                                    Linear::new(),
                                )),
                                VisibilityTransition::fade_out(TweenSpec::new(
                                    Duration::from_millis(1200),
                                    Linear::new(),
                                )),
                                (160.0, 24.0),
                            );
                            Row::new()
                                .modifier(layer.padding(6.0))
                                .build(ctx, |ctx| {
                                    Text::new("child: 1200 ms fade of its own").font_size(12.0).build(ctx);
                                });
                        });
                });
        });
}

/// Panel I: an instant switch — `VisibilityTransition::empty()` on both sides, Compose's
/// `EnterTransition.None` + `ExitTransition.None`. Nothing is animated: the content is there on the
/// next frame and the removal leaves no animation running.
#[composable]
fn panel_i(ctx: &mut ComposeCtx) {
    let show = ctx.remember(|| false);
    let none = VisibilityTransition::empty();
    Column::new()
        .modifier(Modifier::new().padding(16.0))
        .build(ctx, |ctx| {
            Button::new()
                .on_click({
                    clone!(show);
                    move || show.update(|v| *v = !*v)
                })
                .build(ctx, |ctx| {
                    Text::new("I: instant switch / None (toggle)").font_size(12.0).build(ctx);
                });
            AnimatedVisibility::new(show.clone())
                .enter(none.clone())
                .exit(none.clone())
                .build(ctx, |ctx| {
                    Text::new("Panel I — no animation at all")
                        .font_size(13.0)
                        .modifier(Modifier::new().background(Color::from_argb(255, 96, 125, 139), Shape::rounded(8.0)))
                        .build(ctx);
                });
        });
}
