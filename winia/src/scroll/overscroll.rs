//! Overscroll — the rubber-band at the end of a scroll container.
//!
//! Compose 1.8 moved overscroll from the platform to the composable: `Modifier.overscroll(effect)`
//! on a scrollable container, with the effect applying the visual pull and absorbing the input the
//! container cannot use. winia has no platform overscroll, so the effect is a plain value:
//!
//! - `apply_scroll_delta` feeds the leftover (the delta the container could not consume because it is
//!   already at the end) into [`OverscrollEffect::pull`];
//! - the renderer translates the content by [`OverscrollEffect::stretch`] along the scroll axis;
//! - every pull re-arms a short delayed tween back to 0, which is the release rubber-band.
//!
//! Deliberately not Compose's Android stretch (no content scaling) and not its per-platform glow:
//! the content SHIFTS past the edge, which leaves the gap the pull came from. A container whose
//! `scroll_reverse` is set skips the effect (the offset is mirrored there, and a signed pull would
//! point the wrong way).

use crate::runtime::state::State;

/// The direction a container scrolls in — Compose's `Orientation` (foundation.gestures).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    Vertical,
    Horizontal,
}

/// The default asymptote: the content never moves further than this past an edge.
const DEFAULT_MAX_PULL: f32 = 120.0;
/// How long the rubber-band waits before returning (a new pull re-arms it).
const RETURN_DELAY_MS: u64 = 60;
/// The return motion's duration.
const RETURN_MS: u64 = 220;

/// Compose's `OverscrollEffect`: hoist it with `remember` and pass it to
/// [`Modifier::overscroll`](crate::modifier::Modifier::overscroll).
///
/// The content SHIFTS past the edge (it is never scaled), and the shift is DAMPED: the first pixels
/// of travel move the content one-to-one, and every further pixel buys less, approaching
/// `max_pull` without reaching it. That is what makes the edge feel like rubber instead of a wall
/// with a hard stop - and it is why the accumulator below is RAW travel (unbounded) while
/// [`Self::stretch`] is the damped answer.
#[derive(Clone, Debug)]
pub struct OverscrollEffect {
    /// Signed finger travel past the edge, in logical pixels, BEFORE damping. Positive = past the
    /// END. This is what the return animation drives back to 0.
    raw: State<f32>,
    /// The asymptote: the content never travels further than this past an edge.
    max_pull: f32,
    /// The raw travel that gets 63% of the way to `max_pull`. Smaller = stiffer.
    reach: f32,
}

impl OverscrollEffect {
    pub fn new() -> Self {
        Self {
            raw: State::new(0.0),
            max_pull: DEFAULT_MAX_PULL,
            reach: DEFAULT_MAX_PULL,
        }
    }

    /// How far the content may travel past an edge at all (`max_pull`). Default 120.
    ///
    /// This is an ASYMPTOTE, not a clamp: no amount of dragging reaches it, which is the point -
    /// a hard stop is what makes a rubber band feel like a wall.
    pub fn max_pull(mut self, pixels: f32) -> Self {
        self.max_pull = pixels.max(1.0);
        self.reach = self.reach.min(self.max_pull);
        self
    }

    /// How STIFF the band is: the raw travel that gets 63% of the way to `max_pull`, in pixels.
    /// Defaults to `max_pull` (a gentle band); a smaller value saturates sooner.
    pub fn reach(mut self, pixels: f32) -> Self {
        self.reach = pixels.max(1.0);
        self
    }

    /// The damped shift, in logical pixels (what the renderer subtracts from the scroll offset).
    /// Positive = past the end.
    pub fn stretch(&self) -> f32 {
        self.curve(self.raw.peek())
    }

    /// `max_pull * (1 - e^(-|raw| / reach))`, with the sign of `raw`: one-to-one at the origin
    /// (slope `max_pull / reach`), flattening towards the asymptote.
    fn curve(&self, raw: f32) -> f32 {
        if raw == 0.0 {
            return 0.0;
        }
        let magnitude = raw.abs();
        let damped = self.max_pull * (1.0 - (-magnitude / self.reach).exp());
        if raw < 0.0 { -damped } else { damped }
    }

    /// A fling that dies at a boundary keeps a little of its speed as a bend: 40 px/s of raw travel
    /// per pixel, damped by the same curve. Compose hands the whole fling to the effect
    /// (`overscroll.applyToFling`, `foundation/gestures/Scrollable.kt:717`); winia flings through its
    /// own decay, so the effect is given the velocity that was left when the decay hit the limit.
    pub(crate) const PULL_PER_VELOCITY: f32 = 1.0 / 40.0;

    /// Bend for a fling that hit a boundary with `remaining_velocity` (VALUE space, the same sign the
    /// decay runs on: positive moves the offset toward the end).
    pub(crate) fn pull_for_velocity(&self, remaining_velocity: f32) -> f32 {
        self.pull(remaining_velocity * Self::PULL_PER_VELOCITY)
    }

    /// Absorb `leftover` (the delta the container could not consume) and return it: the effect takes
    /// the WHOLE gesture, so nothing falls through to an ancestor - the damping is in the displayed
    /// shift, not in how much of the gesture is accounted for.
    pub(crate) fn pull(&self, leftover: f32) -> f32 {
        if leftover == 0.0 {
            return 0.0;
        }
        // Take the accumulator over from any return already in flight, add the travel and re-arm.
        // Each pull restarts the delay, so a gesture in progress keeps its tension.
        crate::animation::cancel_animation_by_id(self.raw.state_id());
        self.raw.update(|v| *v += leftover);
        self.arm_return();
        leftover
    }

    fn arm_return(&self) {
        let spec = crate::animation::AnimationSpec::Tween(crate::animation::TweenSpec {
            duration: std::time::Duration::from_millis(RETURN_MS),
            delay: std::time::Duration::from_millis(RETURN_DELAY_MS),
            interpolator: std::sync::Arc::new(crate::animation::interpolator::EaseOutCubic::new()),
        });
        crate::animation::push_animatable(self.raw.clone(), 0.0, spec);
    }

    /// Identity for `element_param_eq` (two different handles are different parameters).
    pub(crate) fn same(&self, other: &Self) -> bool {
        self.raw.state_id() == other.raw.state_id()
    }
}
/// Compose's `OverscrollFactory`: what a scroll container asks for the effect it should use when the
/// caller did not hand it one (`foundation/Overscroll.kt:360-366`).
pub type OverscrollFactory = std::sync::Arc<dyn Fn() -> OverscrollEffect + Send + Sync>;

/// A stable identity for a factory value (a fat pointer cannot be cast to `u64` directly).
fn factory_identity(factory: &OverscrollFactory) -> u64 {
    std::sync::Arc::as_ptr(factory) as *const () as u64
}

/// The ONE platform factory. It has to be a single static value: a factory identity is a pointer, and
/// `CompositionLocal::current()` calls its default closure on every read - a default that built a new
/// `Arc` each time gave every scroll state a NEW effect on every build (measured: the same slot's
/// effect state id changed every frame, 213/228/230/232, and a fling's pull landed on an instance the
/// next build had already thrown away, so it never rendered).
static PLATFORM_FACTORY: std::sync::LazyLock<OverscrollFactory> =
    std::sync::LazyLock::new(|| std::sync::Arc::new(OverscrollEffect::new));

fn platform_factory() -> OverscrollFactory {
    std::sync::Arc::clone(&PLATFORM_FACTORY)
}

/// Compose's `LocalOverscrollFactory`, and its default alike: the value is `Some(platform factory)`,
/// so a scroll container overscrolls unless something turns it off - `provides(None)` here is
/// Compose's `LocalOverscrollFactory provides null`.
static LOCAL_OVERSCROLL_FACTORY: std::sync::LazyLock<
    crate::runtime::composition_local::CompositionLocal<Option<OverscrollFactory>>,
> = std::sync::LazyLock::new(|| {
    crate::runtime::composition_local::CompositionLocal::new(|| Some(platform_factory()))
});

/// Compose's `rememberOverscrollEffect()`: the effect this composition's scroll containers should
/// feed, or `None` when overscroll is turned off for the subtree.
#[crate::composable]
pub fn remember_overscroll_effect(
    ctx: &mut crate::runtime::composer::ComposeCtx,
) -> Option<OverscrollEffect> {
    let factory = LOCAL_OVERSCROLL_FACTORY.current()?;
    // Keyed by the factory INSTANCE: replacing the factory has to build a new effect, exactly as
    // Compose re-remembers when `LocalOverscrollFactory` changes (`Overscroll.kt:342-350`).
    let key = factory_identity(&factory) ^ 0x4F56_4552_5343_524F;
    let effect: crate::runtime::state::State<OverscrollEffect> =
        ctx.remember_at_key(key, move || factory());
    Some(effect.get())
}

/// The effect a scroll container should carry RIGHT NOW, resolved from the ambient factory.
///
/// This is what `Modifier::vertical_scroll` / `horizontal_scroll` attach, which is why a hand-built
/// `Column + vertical_scroll(state)` bounces with no setup: the builder runs inside the composing
/// body, so the thread-local provider stack already holds this subtree's value. A modifier built
/// OUTSIDE composition (a cached one, a test fixture) sees the default factory and gets an effect too -
/// the same thing `CompositionLocal::current()` means everywhere else.
///
/// The instance is cached on the scroll state per factory, so rebuilding hands back the SAME effect;
/// a fresh one would drop a pull in flight.
pub(crate) fn ambient_effect(state: &crate::modifier::ScrollState) -> Option<OverscrollEffect> {
    let factory = LOCAL_OVERSCROLL_FACTORY.current()?;
    let id = factory_identity(&factory);
    Some(state.overscroll_for(id, move || factory()))
}

/// Remember a factory across rebuilds, the way Compose's `rememberPlatformOverscrollFactory` does.
///
/// Building `Arc::new(..)` inline and handing it to [`with_overscroll_factory`] makes a NEW identity
/// every rebuild: the scroll states then mint a new effect each frame (a pull in flight is lost) and
/// the provider dirties its subtree every frame because the identity changed. Measured before this
/// helper existed: two rebuilds left two cache entries.
#[crate::composable]
pub fn remember_overscroll_factory(
    ctx: &mut crate::runtime::composer::ComposeCtx,
    create: impl Fn() -> OverscrollEffect + Send + Sync + 'static,
) -> OverscrollFactory {
    let factory: crate::runtime::state::State<OverscrollFactory> =
        ctx.remember(move || std::sync::Arc::new(create) as OverscrollFactory);
    factory.get()
}

/// Provide a factory for a subtree - `None` turns overscroll off for everything inside.
///
/// The `ctx.changed` + dirty marking are load-bearing: a provider that only changes the local leaves
/// already-Skipped children reading the old value (the same trap `DisableSelection` documents).
#[crate::composable]
pub fn with_overscroll_factory(
    ctx: &mut crate::runtime::composer::ComposeCtx,
    factory: Option<OverscrollFactory>,
    content: impl FnOnce(&mut crate::runtime::composer::ComposeCtx),
) {
    let identity = factory.as_ref().map(factory_identity);
    if ctx.changed(&identity) {
        ctx.mark_subtree_dirty();
    }
    LOCAL_OVERSCROLL_FACTORY.provides(factory, || content(ctx));
}

/// Turn the overscroll off for a subtree, the same way `LocalOverscrollFactory provides null` does.
#[crate::composable]
pub fn disable_overscroll(
    ctx: &mut crate::runtime::composer::ComposeCtx,
    content: impl FnOnce(&mut crate::runtime::composer::ComposeCtx),
) {
    with_overscroll_factory(ctx, None, content);
}

impl Default for OverscrollEffect {
    fn default() -> Self {
        Self::new()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::node::LayoutNode;
    // (tests below use these)
    use crate::modifier::{Modifier, ScrollState};
    use crate::unit::{Density, Size};

    fn container(effect: &OverscrollEffect, offset: f32) -> (Vec<LayoutNode>, ScrollState) {
        let state = ScrollState::new();
        state.offset.set(offset);
        let mut node = LayoutNode::leaf(
            Modifier::new()
                .size(100.0, 100.0)
                .vertical_scroll(state.clone())
                .overscroll(effect),
        );
        node.measured_size = Size::new(100.0, 100.0);
        node.scroll_content_height = 300.0;
        (vec![node], state)
    }

    /// The band is DAMPED: the first pixels are one-to-one, every further pixel buys less, and the
    /// cap is an asymptote rather than a clamp.
    #[test]
    fn a_pull_starts_one_to_one_and_then_fades_towards_the_cap() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        crate::animation::clear_all_animations();
        let effect = OverscrollEffect::new();
        // pull() returns what it TOOK (the whole gesture), stretch() what it SHOWED.
        assert_eq!(effect.pull(10.0), 10.0, "the gesture is fully consumed");
        assert!(
            (effect.stretch() - 9.6).abs() < 0.3,
            "and moves the content almost 1:1, got {}",
            effect.stretch()
        );

        effect.pull(10.0);
        let second = effect.stretch() - 9.6;
        assert!(
            second < 9.0,
            "the second 10 px buys less than the first: {second}"
        );

        // A huge pull approaches the asymptote: at raw 240 it is still visibly short of the cap,
        // and only f32 rounding makes a 5 000 px pull read as exactly 120.
        effect.pull(220.0);
        assert!(
            (effect.stretch() - 103.7).abs() < 1.5,
            "raw 240 -> ~104, got {}",
            effect.stretch()
        );
        effect.pull(5_000.0);
        assert!(
            effect.stretch() <= 120.0,
            "the cap is an asymptote, got {}",
            effect.stretch()
        );

        // The other direction is symmetric.
        let back = OverscrollEffect::new();
        back.pull(-10.0);
        assert!(
            (back.stretch() + 9.6).abs() < 0.3,
            "mirrored, got {}",
            back.stretch()
        );
        crate::animation::clear_all_animations();
    }

    /// The knobs: a smaller `reach` saturates sooner, a smaller `max_pull` caps lower.
    #[test]
    fn reach_and_cap_shape_the_band() {
        let stiff = OverscrollEffect::new().reach(20.0);
        stiff.pull(20.0);
        assert!(
            (stiff.stretch() - 120.0 * (1.0 - (-1.0f32).exp())).abs() < 0.5,
            "reach 20 at raw 20 is 63% of the cap, got {}",
            stiff.stretch()
        );
        let soft = OverscrollEffect::new();
        soft.pull(20.0);
        assert!(
            stiff.stretch() > soft.stretch(),
            "the stiffer band moves further for the same drag"
        );

        let small = OverscrollEffect::new().max_pull(40.0);
        small.pull(5_000.0);
        assert!(
            small.stretch() <= 40.0,
            "the cap bounds it, got {}",
            small.stretch()
        );
        crate::animation::clear_all_animations();
    }
    #[test]
    fn the_delta_past_the_end_is_absorbed_by_the_effect() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        crate::animation::clear_all_animations();
        let effect = OverscrollEffect::new();
        let (mut nodes, state) = container(&effect, 200.0); // 300 content - 100 viewport
        let consumed = crate::app::scroll::apply_scroll_delta(
            &mut nodes,
            0,
            0.0,
            -10.0,
            Density::from_density(1.0),
        );
        assert_eq!(state.offset.peek(), 200.0, "the offset stays clamped");
        assert!(
            (effect.stretch() - 9.6).abs() < 0.3,
            "the leftover is the (damped) pull, got {}",
            effect.stretch()
        );
        assert_eq!(
            consumed.y, -10.0,
            "the gesture is consumed in the DELTA's direction, so no ancestor sees it"
        );
        crate::animation::clear_all_animations();
    }

    #[test]
    fn the_delta_past_the_start_pulls_the_other_way() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        crate::animation::clear_all_animations();
        let effect = OverscrollEffect::new();
        let (mut nodes, state) = container(&effect, 0.0);
        let consumed = crate::app::scroll::apply_scroll_delta(
            &mut nodes,
            0,
            0.0,
            10.0,
            Density::from_density(1.0),
        );
        assert_eq!(state.offset.peek(), 0.0);
        assert!(
            (effect.stretch() + 9.6).abs() < 0.3,
            "got {}",
            effect.stretch()
        );
        assert_eq!(
            consumed.y, 10.0,
            "the pull is consumed in the gesture's direction"
        );
        crate::animation::clear_all_animations();
    }

    /// A live gesture keeps its tension: every pull restarts the 60 ms delay, so the band does not
    /// start shrinking under a finger that never left the screen.
    #[test]
    fn a_live_gesture_keeps_its_tension() {
        let (_guard, clock) = crate::animation::tests::serial_clock();
        crate::animation::clear_all_animations();
        let effect = OverscrollEffect::new();
        effect.pull(60.0);
        let held = effect.stretch();
        clock.advance(std::time::Duration::from_millis(50));
        crate::animation::update_animations();
        assert!(
            (effect.stretch() - held).abs() < 0.01,
            "the 60 ms delay has not elapsed yet"
        );

        effect.pull(5.0);
        clock.advance(std::time::Duration::from_millis(50));
        crate::animation::update_animations();
        assert!(
            effect.stretch() > held,
            "the re-armed delay keeps the pull alive ({})",
            effect.stretch()
        );

        clock.advance(std::time::Duration::from_millis(400));
        crate::animation::update_animations();
        assert!(
            effect.stretch() < 1.0,
            "after the gesture stops the return runs, got {}",
            effect.stretch()
        );
        crate::animation::clear_all_animations();
    }
    /// The whole chain, in pixels: the render has to READ the pull. This is the test that would
    /// have caught the per-build effect identity - the pull was applied, the effect was real, and
    /// the frame drew nothing.
    #[test]
    fn a_pull_shifts_the_painted_content() {
        fn frame(pull: f32) -> (u8, u8, u8) {
            let effect = OverscrollEffect::new();
            if pull != 0.0 {
                effect.pull(pull);
            }
            let scroll = crate::modifier::ScrollState::new();
            let s = scroll.clone();
            let mut composer = crate::runtime::composer::Composer::new();
            composer.compose(crate::compose!(|ctx| {
                crate::layout::Column::new()
                    .modifier(
                        Modifier::new()
                            .size(100.0, 100.0)
                            .vertical_scroll(s.clone())
                            .overscroll(&effect),
                    )
                    .build(ctx, |ctx| {
                        for _ in 0..3 {
                            crate::layout::Column::new()
                                .modifier(Modifier::new().size(100.0, 40.0).background(
                                    crate::graphics::Color::RED,
                                    crate::graphics::Shape::Rectangle,
                                ))
                                .build(ctx, |_| {});
                        }
                    });
            }));
            composer.layout(crate::layout::Constraints::new(0.0, 100.0, 0.0, 100.0));
            let mut surface = skia_safe::surfaces::raster_n32_premul((100, 100)).unwrap();
            surface.canvas().clear(skia_safe::Color::WHITE);
            let idx = composer.layout_root_idx().unwrap();
            crate::render::render(composer.arena_nodes(), idx, surface.canvas());
            let mut px = [0u8; 4];
            let info = skia_safe::ImageInfo::new(
                (1, 1),
                skia_safe::ColorType::RGBA8888,
                skia_safe::AlphaType::Premul,
                None,
            );
            surface.read_pixels(&info, &mut px, 4, (50, 95));
            crate::animation::clear_all_animations();
            (px[0], px[1], px[2])
        }
        let still = frame(0.0);
        let pulled = frame(40.0);
        assert_eq!(
            still,
            (255, 0, 0),
            "the content fills the bottom of the viewport at rest"
        );
        assert_eq!(
            pulled,
            (255, 255, 255),
            "and a pull lifts it away, leaving the background"
        );
    }

    /// A fling that dies at the end bends the content past the END (a positive pull), damped by the
    /// same curve: a slow fling is nearly proportional, a hard one approaches the cap.
    #[test]
    fn a_fling_bend_follows_velocity_and_fades() {
        let effect = OverscrollEffect::new();
        effect.pull_for_velocity(2_000.0);
        let slow = effect.stretch();
        assert!(
            (slow - 120.0 * (1.0 - (-(2000.0 / 40.0) / 120.0f32).exp())).abs() < 0.5,
            "2000 px/s -> raw 50 damped, got {slow}"
        );

        let hard = OverscrollEffect::new();
        hard.pull_for_velocity(100_000.0);
        assert!(
            hard.stretch() <= 120.0,
            "even a huge fling stops at the cap, got {}",
            hard.stretch()
        );

        let back = OverscrollEffect::new();
        back.pull_for_velocity(-2_000.0);
        assert!(
            (back.stretch() + slow).abs() < 0.5,
            "the sign follows the velocity"
        );
    }
    /// The fling path really reaches the effect: a decay that hits the end leaves a bend.
    /// A hand-built `Column + vertical_scroll` gets the effect from the ambient factory, exactly as a
    /// LazyColumn does - the deviation this used to have is gone.
    #[test]
    fn a_plain_scroll_column_takes_the_ambient_effect() {
        fn compose(disable: bool) -> (bool, Option<u64>) {
            let mut composer = crate::runtime::composer::Composer::new();
            let scroll = ScrollState::new();
            let s = scroll.clone();
            composer.compose(crate::compose!(|ctx| {
                let body = |ctx: &mut crate::runtime::composer::ComposeCtx| {
                    crate::layout::Column::new()
                        .modifier(
                            Modifier::new()
                                .size(100.0, 100.0)
                                .vertical_scroll(s.clone()),
                        )
                        .build(ctx, |ctx| {
                            crate::components::Text::new("row").build(ctx);
                        });
                };
                if disable {
                    disable_overscroll(ctx, body);
                } else {
                    body(ctx);
                }
            }));
            let found = composer.arena_nodes().iter().find_map(|n| {
                n.modifier
                    .overscroll_effect()
                    .map(|e| e.stretch().to_bits() as u64)
            });
            (found.is_some(), found)
        }

        let (present, _) = compose(false);
        assert!(present, "the builder attaches the ambient effect");
        let (present, _) = compose(true);
        assert!(!present, "and `disable_overscroll` keeps it off");
    }

    /// A factory built inline would be a new identity every rebuild; the remember helper keeps one.
    #[test]
    fn a_remembered_factory_is_one_identity_across_rebuilds() {
        let mut composer = crate::runtime::composer::Composer::new();
        let scroll = ScrollState::new();
        let mut ids = Vec::new();
        for _ in 0..2 {
            let sc = scroll.clone();
            composer.compose(crate::compose!(|ctx| {
                let factory = remember_overscroll_factory(ctx, || OverscrollEffect::new());
                ids.push(std::sync::Arc::as_ptr(&factory) as *const () as u64);
                with_overscroll_factory(ctx, Some(factory), |ctx| {
                    crate::layout::Column::new()
                        .modifier(
                            crate::modifier::Modifier::new()
                                .size(100.0, 100.0)
                                .vertical_scroll(sc.clone()),
                        )
                        .build(ctx, |ctx| {
                            crate::components::Text::new("x").build(ctx);
                        });
                });
            }));
        }
        assert_eq!(ids[0], ids[1], "the same factory identity both times");
        assert_eq!(
            scroll.overscroll_cache.lock().unwrap().len(),
            1,
            "and one cached effect"
        );
    }

    /// The default factory is ONE value. `CompositionLocal::current()` calls its default closure on
    /// every read, so a default that built a new `Arc` each time gave every scroll state a new effect
    /// on every build - and a fling's pull landed on an instance the next build had thrown away.
    #[test]
    fn the_default_factory_keeps_one_identity() {
        let first = LOCAL_OVERSCROLL_FACTORY
            .current()
            .expect("a default factory");
        // Churn the allocator between the reads: a per-read Arc would land somewhere else.
        let junk: Vec<_> = (0..16)
            .map(|i| std::sync::Arc::new(vec![i as u8; 64]))
            .collect();
        let second = LOCAL_OVERSCROLL_FACTORY
            .current()
            .expect("a default factory");
        assert_eq!(
            factory_identity(&first),
            factory_identity(&second),
            "one static value"
        );
        drop(junk);
    }

    /// Rebuilding must hand back the SAME effect: a fresh one would drop a pull in flight. The pull
    /// is taken ONCE and must still be there after a rebuild (a per-build effect reports 0).
    #[test]
    fn the_effect_is_stable_across_rebuilds() {
        let mut composer = crate::runtime::composer::Composer::new();
        let scroll = ScrollState::new();
        let s = scroll.clone();
        let mut stretches = Vec::new();
        for build in 0..2 {
            composer.compose(crate::compose!(|ctx| {
                crate::layout::Column::new()
                    .modifier(
                        Modifier::new()
                            .size(100.0, 100.0)
                            .vertical_scroll(s.clone()),
                    )
                    .build(ctx, |ctx| {
                        crate::components::Text::new("row").build(ctx);
                    });
            }));
            let effect = composer
                .arena_nodes()
                .iter()
                .find_map(|n| n.modifier.overscroll_effect().cloned())
                .expect("an effect");
            if build == 0 {
                effect.pull(10.0);
            }
            stretches.push(effect.stretch());
            // Same interference a real build has: an unstable factory would mint a different
            // effect here and lose the pull.
            let junk: Vec<_> = (0..16)
                .map(|i| std::sync::Arc::new(vec![i as u8; 64]))
                .collect();
            drop(junk);
        }
        assert!(
            (stretches[0] - 9.6).abs() < 0.3,
            "the first build pulled, got {}",
            stretches[0]
        );
        assert_eq!(
            stretches[0].to_bits(),
            stretches[1].to_bits(),
            "the second build reused the effect, pull included"
        );
        crate::animation::clear_all_animations();
    }

    #[test]
    fn a_fling_into_the_end_leaves_a_bend() {
        let effect = OverscrollEffect::new();
        let scroll = ScrollState::new();
        scroll.offset.set(100.0);
        scroll.fling_limit.set(100.0);
        let mut nodes = vec![
            LayoutNode::leaf(Modifier::new().size(100.0, 100.0)),
            LayoutNode::leaf(
                Modifier::new()
                    .vertical_scroll(scroll.clone())
                    .overscroll(&effect)
                    .size(100.0, 100.0),
            ),
        ];
        nodes[0].measured_size = Size::new(100.0, 100.0);
        nodes[1].measured_size = Size::new(100.0, 100.0);
        nodes[1].scroll_viewport_height = 100.0;
        nodes[1].scroll_content_height = 200.0;
        nodes[0].children.push(1);

        crate::app::scroll::dispatch_nested_scroll_fling(
            &mut nodes,
            0,
            1,
            crate::nested_scroll::ScrollVelocity { x: 0.0, y: 1500.0 },
        );
        // The decay clamps on its first tick; give the animation one.
        crate::animation::update_animations();
        assert!(
            effect.stretch() > 0.0,
            "the fling bent the content: got {}",
            effect.stretch()
        );
        crate::animation::clear_all_animations();
    }
}
