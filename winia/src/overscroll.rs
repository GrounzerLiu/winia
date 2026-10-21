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

/// How far the content may be pulled past the edge, in logical pixels.
const MAX_PULL: f32 = 120.0;
/// How long the rubber-band waits before returning (a new pull re-arms it).
const RETURN_DELAY_MS: u64 = 60;
/// The return motion's duration.
const RETURN_MS: u64 = 220;

/// Compose's `OverscrollEffect`: hoist it with `remember` and pass it to
/// [`Modifier::overscroll`](crate::modifier::Modifier::overscroll).
#[derive(Clone, Debug)]
pub struct OverscrollEffect {
    /// Signed pull along the scroll axis, in logical pixels. Positive = past the END (content is
    /// translated in the scroll direction), negative = past the START.
    value: State<f32>,
}

impl OverscrollEffect {
    pub fn new() -> Self {
        Self {
            value: State::new(0.0),
        }
    }

    /// The current pull, in logical pixels (what the renderer subtracts from the scroll offset).
    pub fn stretch(&self) -> f32 {
        self.value.peek()
    }

    /// Absorb `leftover` (the delta the container could not consume). Returns the signed amount the
    /// effect absorbed, which the caller adds to its consumed delta so the gesture does not fall
    /// through to an ancestor.
    pub(crate) fn pull(&self, leftover: f32) -> f32 {
        if leftover == 0.0 {
            return 0.0;
        }
        let current = self.value.peek();
        let next = (current + leftover).clamp(-MAX_PULL, MAX_PULL);
        let absorbed = next - current;
        if absorbed == 0.0 {
            return 0.0;
        }
        // Take the value over from any return already in flight, then set the new pull and re-arm
        // the return. Each pull restarts the delay, so a gesture in progress keeps its tension.
        crate::animation::cancel_animation_by_id(self.value.state_id());
        self.value.set(next);
        self.arm_return();
        absorbed
    }

    fn arm_return(&self) {
        let spec = crate::animation::AnimationSpec::Tween(crate::animation::TweenSpec {
            duration: std::time::Duration::from_millis(RETURN_MS),
            delay: std::time::Duration::from_millis(RETURN_DELAY_MS),
            interpolator: std::sync::Arc::new(crate::animation::interpolator::EaseOutCubic::new()),
        });
        crate::animation::push_animatable(self.value.clone(), 0.0, spec);
    }

    /// Identity for `element_param_eq` (two different handles are different parameters).
    pub(crate) fn same(&self, other: &Self) -> bool {
        self.value.state_id() == other.value.state_id()
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

    #[test]
    fn pull_absorbs_and_clamps() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        crate::animation::clear_all_animations();
        let effect = OverscrollEffect::new();
        assert_eq!(effect.pull(40.0), 40.0);
        assert_eq!(effect.stretch(), 40.0);
        assert_eq!(effect.pull(1_000.0), MAX_PULL - 40.0);
        assert_eq!(effect.stretch(), MAX_PULL);
        assert_eq!(effect.pull(10.0), 0.0, "at the cap nothing more is absorbed");
        // The negative direction is independent: pulling back first releases the positive pull.
        assert_eq!(effect.pull(-50.0), -50.0);
        assert_eq!(effect.stretch(), MAX_PULL - 50.0);
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
        assert_eq!(effect.stretch(), 10.0, "the leftover is the pull");
        assert!(consumed.y > 0.0, "the gesture is consumed, not passed to an ancestor");
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
        assert_eq!(effect.stretch(), -10.0);
        assert!(consumed.y < 0.0);
        crate::animation::clear_all_animations();
    }
}
