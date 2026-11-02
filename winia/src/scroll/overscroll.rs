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
        if raw == 0.0 { return 0.0; }
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
        assert!((effect.stretch() - 9.6).abs() < 0.3, "and moves the content almost 1:1, got {}", effect.stretch());

        effect.pull(10.0);
        let second = effect.stretch() - 9.6;
        assert!(second < 9.0, "the second 10 px buys less than the first: {second}");

        // A huge pull approaches the asymptote: at raw 240 it is still visibly short of the cap,
        // and only f32 rounding makes a 5 000 px pull read as exactly 120.
        effect.pull(220.0);
        assert!((effect.stretch() - 103.7).abs() < 1.5, "raw 240 -> ~104, got {}", effect.stretch());
        effect.pull(5_000.0);
        assert!(effect.stretch() <= 120.0, "the cap is an asymptote, got {}", effect.stretch());

        // The other direction is symmetric.
        let back = OverscrollEffect::new();
        back.pull(-10.0);
        assert!((back.stretch() + 9.6).abs() < 0.3, "mirrored, got {}", back.stretch());
        crate::animation::clear_all_animations();
    }

    /// The knobs: a smaller `reach` saturates sooner, a smaller `max_pull` caps lower.
    #[test]
    fn reach_and_cap_shape_the_band() {
        let stiff = OverscrollEffect::new().reach(20.0);
        stiff.pull(20.0);
        assert!((stiff.stretch() - 120.0 * (1.0 - (-1.0f32).exp())).abs() < 0.5,
            "reach 20 at raw 20 is 63% of the cap, got {}", stiff.stretch());
        let soft = OverscrollEffect::new();
        soft.pull(20.0);
        assert!(stiff.stretch() > soft.stretch(), "the stiffer band moves further for the same drag");

        let small = OverscrollEffect::new().max_pull(40.0);
        small.pull(5_000.0);
        assert!(small.stretch() <= 40.0, "the cap bounds it, got {}", small.stretch());
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
        assert!((effect.stretch() - 9.6).abs() < 0.3, "the leftover is the (damped) pull, got {}", effect.stretch());
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
        assert!((effect.stretch() + 9.6).abs() < 0.3, "got {}", effect.stretch());
        assert_eq!(consumed.y, 10.0, "the pull is consumed in the gesture's direction");
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
        assert!((effect.stretch() - held).abs() < 0.01, "the 60 ms delay has not elapsed yet");

        effect.pull(5.0);
        clock.advance(std::time::Duration::from_millis(50));
        crate::animation::update_animations();
        assert!(effect.stretch() > held, "the re-armed delay keeps the pull alive ({})", effect.stretch());

        clock.advance(std::time::Duration::from_millis(400));
        crate::animation::update_animations();
        assert!(effect.stretch() < 1.0, "after the gesture stops the return runs, got {}", effect.stretch());
        crate::animation::clear_all_animations();
    }
    /// A fling that dies at the end bends the content past the END (a positive pull), damped by the
    /// same curve: a slow fling is nearly proportional, a hard one approaches the cap.
    #[test]
    fn a_fling_bend_follows_velocity_and_fades() {
        let effect = OverscrollEffect::new();
        effect.pull_for_velocity(2_000.0);
        let slow = effect.stretch();
        assert!((slow - 120.0 * (1.0 - (-(2000.0 / 40.0) / 120.0f32).exp())).abs() < 0.5,
            "2000 px/s -> raw 50 damped, got {slow}");

        let hard = OverscrollEffect::new();
        hard.pull_for_velocity(100_000.0);
        assert!(hard.stretch() <= 120.0, "even a huge fling stops at the cap, got {}", hard.stretch());

        let back = OverscrollEffect::new();
        back.pull_for_velocity(-2_000.0);
        assert!((back.stretch() + slow).abs() < 0.5, "the sign follows the velocity");
    }
    /// The fling path really reaches the effect: a decay that hits the end leaves a bend.
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

        crate::app::scroll::dispatch_nested_scroll_fling(&mut nodes, 0, 1, crate::nested_scroll::ScrollVelocity { x: 0.0, y: 1500.0 });
        // The decay clamps on its first tick; give the animation one.
        crate::animation::update_animations();
        assert!(effect.stretch() > 0.0, "the fling bent the content: got {}", effect.stretch());
        crate::animation::clear_all_animations();
    }}
