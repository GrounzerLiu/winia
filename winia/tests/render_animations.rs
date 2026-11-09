//! Render-level coverage for the three animation containers: `AnimatedVisibility`,
//! `AnimatedContent` and `AnimatedSize`.
//!
//! Their unit tests (`components::animated_*`) already pin geometry, specs and listener contracts.
//! What none of them can see is whether the thing is actually DRAWN differently mid-transition —
//! a channel that never reaches the paint layer, or a progress value the graphics layer reads
//! untracked and therefore never re-evaluates, passes every geometry assertion. These tests render
//! real frames and read pixels.
//!
//! Animation is driven the way the unit tests drive it: `update_animations()` advances the engine
//! one step, then the tree is composed and laid out again. The animation registry is process-global,
//! so every test here takes `SERIAL` (the crate's own `TEST_SERIAL` is `pub(crate)`).
//!
//! The page bodies are `#[composable]` functions rather than inline closures: a `remember` inside a
//! plain closure has no stable key, which the composer rejects outright.

use winia::animation::{SlideDirection, TweenSpec, VisibilityTransition, update_animations};
use winia::graphics::{Color, Shape};
use winia::layout::constraints::Constraints;
use winia::modifier::Modifier;
use winia::prelude::*;
use winia::runtime::composer::{ComposeCtx, Composer};
use winia::runtime::state::State;

/// The animation registry is global, so the tests in this file run one at a time.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

/// A short linear tween, so a mid-flight sample is predictable rather than spring-shaped.
///
/// Kept SHORT on purpose: the engine advances by wall-clock time, so a multi-second tween cannot
/// finish inside a test loop that spins in microseconds. 60 ms leaves room for several real steps
/// while still completing almost immediately.
fn linear() -> TweenSpec {
    TweenSpec::new(
        std::time::Duration::from_millis(60),
        winia::animation::interpolator::Linear::new(),
    )
}

fn raster_surface(w: i32, h: i32) -> skia_safe::Surface {
    let info = skia_safe::ImageInfo::new(
        (w, h),
        skia_safe::ColorType::RGBA8888,
        skia_safe::AlphaType::Premul,
        None,
    );
    skia_safe::surfaces::raster(&info, None, None).expect("raster surface")
}

/// Compose + lay out + paint one frame, returning the pixel at `(x, y)`.
fn frame_pixel(
    composer: &mut Composer,
    ui: &impl Fn(&mut ComposeCtx),
    size: (f32, f32),
    at: (i32, i32),
) -> (u8, u8, u8, u8) {
    composer.compose(|ctx| ui(ctx));
    composer.layout(Constraints::new(0.0, size.0, 0.0, size.1));
    let mut surface = raster_surface(size.0 as i32, size.1 as i32);
    surface.canvas().clear(skia_safe::Color::WHITE);
    if let Some(root_idx) = composer.layout_root_idx() {
        let nodes = composer.arena_nodes();
        winia::render::render(nodes, root_idx, surface.canvas());
    }
    let mut pixels = [0u8; 4];
    let info = skia_safe::ImageInfo::new(
        (1, 1),
        skia_safe::ColorType::RGBA8888,
        skia_safe::AlphaType::Premul,
        None,
    );
    let _ = surface.read_pixels(&info, &mut pixels, 4, at);
    (pixels[0], pixels[1], pixels[2], pixels[3])
}

// ── AnimatedVisibility ──

#[composable]
fn visibility_page(ctx: &mut ComposeCtx, visible: State<bool>) {
    AnimatedVisibility::new(visible)
        .enter(VisibilityTransition::fade_in(linear()))
        .exit(VisibilityTransition::fade_out(linear()))
        .build(ctx, |ctx| {
            winia::layout::components::Stack::new()
                .modifier(
                    Modifier::new()
                        .size(120.0, 60.0)
                        .background(Color::from_argb(255, 0, 0, 255), Shape::Rectangle),
                )
                .build(ctx, |_| {});
        });
}

/// `AnimatedVisibility` fades its content out: mid-exit the sampled pixel is neither the content
/// colour nor the background. That is the only observable that distinguishes a real fade from an
/// instant removal, and it is what a graphics-layer channel that never re-evaluated would fail.
#[test]
fn animated_visibility_fades_rather_than_vanishing() {
    let _serial = serial();
    const SIZE: (f32, f32) = (120.0, 60.0);
    const SAMPLE: (i32, i32) = (60, 30);

    let holder: std::cell::RefCell<Option<State<bool>>> = std::cell::RefCell::new(None);
    let ui = |ctx: &mut ComposeCtx| {
        let v: State<bool> = ctx.remember_at_key(1, || true);
        *holder.borrow_mut() = Some(v.clone());
        visibility_page(ctx, v);
    };

    let mut composer = Composer::new();
    let full = frame_pixel(&mut composer, &ui, SIZE, SAMPLE);
    assert!(
        full.2 > 200 && full.0 < 60,
        "the content starts fully opaque blue: {full:?}"
    );

    holder.borrow().as_ref().unwrap().set(false);
    frame_pixel(&mut composer, &ui, SIZE, SAMPLE);

    let mut saw_intermediate = false;
    let mut frames = 0;
    // Layout/paint FIRST: the animation is registered from the measure pass.
    for _ in 0..2_000 {
        let px = frame_pixel(&mut composer, &ui, SIZE, SAMPLE);
        frames += 1;
        // Blended towards white: red has risen off 0 while blue is still dominant.
        if px.0 > 20 && px.0 < 235 && px.2 > 60 {
            saw_intermediate = true;
            break;
        }
        if !update_animations() {
            break;
        }
    }
    assert!(
        frames > 1,
        "a 600 ms exit has to take more than one engine step, took {frames}"
    );
    assert!(
        saw_intermediate,
        "the exit must pass through partially-transparent frames, not jump to the background"
    );
}

// ── AnimatedSize ──

#[composable]
fn size_page(ctx: &mut ComposeCtx, height: State<f32>) {
    AnimatedSize::new(linear()).build(ctx, |ctx| {
        winia::layout::components::Stack::new()
            .modifier(
                Modifier::new()
                    .size(100.0, height.get())
                    .background(Color::from_argb(255, 0, 0, 255), Shape::Rectangle),
            )
            .build(ctx, |_| {});
    });
}

/// `AnimatedSize` animates the BOX: mid-transition the container is shorter than its target.
#[test]
fn animated_size_grows_through_intermediate_heights() {
    let _serial = serial();
    const W: f32 = 100.0;
    const TALL: f32 = 200.0;
    const START: f32 = 40.0;

    let holder: std::cell::RefCell<Option<State<f32>>> = std::cell::RefCell::new(None);
    let ui = |ctx: &mut ComposeCtx| {
        let h: State<f32> = ctx.remember_at_key(2, || START);
        *holder.borrow_mut() = Some(h.clone());
        size_page(ctx, h);
    };

    let mut composer = Composer::new();
    let top = frame_pixel(&mut composer, &ui, (W, TALL), (50, 10));
    assert!(
        top.2 > 200 && top.0 < 60,
        "the box starts {START} tall: {top:?}"
    );

    holder.borrow().as_ref().unwrap().set(TALL);
    frame_pixel(&mut composer, &ui, (W, TALL), (50, 10));

    let mut heights = Vec::new();
    // The unit test's advance order, copied exactly: step the engine, then re-compose + lay out.
    // Re-composing re-enters the group and re-runs the measure pass, which is where `AnimatedSize`
    // reads the animated value and pushes the next step; a loop that only re-lays-out leaves the
    // group Clean and the animation drains without ever re-registering.
    let recompose = |composer: &mut Composer| {
        composer.compose(|ctx| ui(ctx));
        composer.layout(Constraints::new(0.0, W, 0.0, TALL));
    };
    for _ in 0..10_000 {
        if !update_animations() {
            break;
        }
        recompose(&mut composer);
        heights.push(
            composer
                .layout_root_idx()
                .map(|r| composer.arena_nodes()[r].measured_size.height)
                .unwrap_or(-1.0),
        );
    }

    assert!(
        heights.iter().any(|h| *h > START + 5.0 && *h < TALL - 5.0),
        "the container must pass through intermediate heights, got {} frames: {:?}",
        heights.len(),
        &heights[..heights.len().min(8)]
    );
    let last = *heights.last().expect("frames");
    assert!(
        (last - TALL).abs() < 1.0,
        "and settle at the target height, got {last}"
    );
}

// ── AnimatedContent ──

#[composable]
fn content_page(ctx: &mut ComposeCtx, page: State<u32>) {
    AnimatedContent::new(page)
        .enter(VisibilityTransition::fade_in(linear()).with_slide(SlideDirection::Left))
        .exit(VisibilityTransition::fade_out(linear()))
        .build(ctx, |ctx, value| {
            let color = if value == 0 {
                Color::from_argb(255, 0, 0, 255)
            } else {
                Color::from_argb(255, 255, 0, 0)
            };
            winia::layout::components::Stack::new()
                .modifier(
                    Modifier::new()
                        .size(120.0, 60.0)
                        .background(color, Shape::Rectangle),
                )
                .build(ctx, |_| {});
        });
}

/// `AnimatedContent` keeps the outgoing state on screen while the incoming one fades in, so during
/// the switch a pixel can be a blend of BOTH colours. A swap would only ever produce one of them.
#[test]
fn animated_content_shows_both_generations_mid_switch() {
    let _serial = serial();
    const SIZE: (f32, f32) = (120.0, 60.0);
    const SAMPLE: (i32, i32) = (60, 30);

    let holder: std::cell::RefCell<Option<State<u32>>> = std::cell::RefCell::new(None);
    let ui = |ctx: &mut ComposeCtx| {
        let p: State<u32> = ctx.remember_at_key(3, || 0u32);
        *holder.borrow_mut() = Some(p.clone());
        content_page(ctx, p);
    };

    let mut composer = Composer::new();
    let first = frame_pixel(&mut composer, &ui, SIZE, SAMPLE);
    assert!(
        first.2 > 200 && first.0 < 60,
        "generation 0 paints blue: {first:?}"
    );

    holder.borrow().as_ref().unwrap().set(1);
    frame_pixel(&mut composer, &ui, SIZE, SAMPLE);

    let mut saw_blend = false;
    // Same order as the unit tests: step the engine, then re-compose and paint. Re-composing is
    // what re-enters the group and re-runs the measure pass that pushes the next step.
    for _ in 0..10_000 {
        if !update_animations() {
            break;
        }
        let px = frame_pixel(&mut composer, &ui, SIZE, SAMPLE);
        // Both channels materially present and neither saturated: red over blue or blue over red.
        if px.0 > 25 && px.2 > 25 && px.0 < 240 && px.2 < 240 {
            saw_blend = true;
            break;
        }
    }
    assert!(
        saw_blend,
        "mid-switch the outgoing and incoming generations must be visible together"
    );

    // `break` left the transition mid-flight: drain it before reading the settled frame.
    for _ in 0..10_000 {
        if !update_animations() {
            break;
        }
        frame_pixel(&mut composer, &ui, SIZE, SAMPLE);
    }
    let settled = frame_pixel(&mut composer, &ui, SIZE, SAMPLE);
    assert!(
        settled.0 > 200 && settled.2 < 60,
        "and once the switch finishes only the incoming one is left: {settled:?}"
    );
}

// ── The guard ──

#[composable]
fn short_tween_page(ctx: &mut ComposeCtx, width: State<f32>) {
    AnimatedSize::new(linear()).build(ctx, |ctx| {
        winia::layout::components::Stack::new()
            .modifier(Modifier::new().size(width.get(), 20.0))
            .build(ctx, |_| {});
    });
}

/// Guard for the tests above: if the engine advanced in a single step, every "intermediate" sample
/// would be vacuous. This drives a short tween and counts the engine steps it actually takes.
#[test]
fn a_tween_takes_more_than_one_engine_step() {
    let _serial = serial();

    let holder: std::cell::RefCell<Option<State<f32>>> = std::cell::RefCell::new(None);
    let ui = |ctx: &mut ComposeCtx| {
        let w: State<f32> = ctx.remember_at_key(4, || 20.0);
        *holder.borrow_mut() = Some(w.clone());
        short_tween_page(ctx, w);
    };

    let mut composer = Composer::new();
    frame_pixel(&mut composer, &ui, (100.0, 40.0), (5, 5));
    holder.borrow().as_ref().unwrap().set(90.0);
    frame_pixel(&mut composer, &ui, (100.0, 40.0), (5, 5));

    let mut steps = 0;
    for _ in 0..2_000 {
        if !update_animations() {
            break;
        }
        steps += 1;
        composer.compose(|ctx| ui(ctx));
        composer.layout(Constraints::new(0.0, 100.0, 0.0, 40.0));
    }
    assert!(
        steps > 1,
        "a 300 ms tween must take several engine steps, got {steps}"
    );
}
