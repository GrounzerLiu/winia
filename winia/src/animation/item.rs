//! The animation contract for keyed layout items.
//!
//! A layout container sees an item's new slot but not where the item was painted a frame ago.
//! `ItemAnimationSpec` is the data a child attaches with `Modifier::animate_item`; the container's
//! `ItemAnimationHost` owns the per-key states. Keeping the spec on the item means a lazy list and a
//! Flow share one marker and one runtime instead of growing a second switch on each container.

use crate::animation::{AnimationSpec, RepeatMode, SpringSpec};

/// Placement and fade curves for one item.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct ItemAnimationSpec {
    /// How the paint offset travels from the old slot to the new one. `None` disables movement
    /// (useful for an item that only fades). `Decay` is unsupported; finite `Repeatable` is allowed.
    pub(crate) placement: Option<AnimationSpec>,
    /// Fade-in applied when the key first appears after the host's first layout. `Decay` is unsupported.
    pub(crate) fade_in: Option<AnimationSpec>,
    /// Fade-out curve for a removed key. The host captures the item's last rendered frame and the
    /// exit layer consumes this curve after the item leaves the layout tree. `Decay` is unsupported;
    /// the channel needs a defined end edge so the exit picture can be dropped.
    pub(crate) fade_out: Option<AnimationSpec>,
}

impl Default for ItemAnimationSpec {
    fn default() -> Self {
        Self {
            placement: Some(AnimationSpec::Spring(SpringSpec::default())),
            fade_in: None,
            fade_out: None,
        }
    }
}

fn usable_channel(spec: &AnimationSpec) -> bool {
    match spec {
        // Decay has no target value, so it cannot drive an item channel to a defined end. A finite
        // Repeatable is allowed only when its final iteration ends at the target: Restart always
        // does, Reverse does only for an odd iteration count.
        AnimationSpec::Decay(_) => false,
        AnimationSpec::Repeatable(repeat) => {
            repeat.mode == RepeatMode::Restart || repeat.iterations % 2 == 1
        }
        AnimationSpec::Spring(spring) => spring.threshold > 0.0,
        _ => true,
    }
}

impl ItemAnimationSpec {
    /// True when every configured channel can be driven by the item host.
    pub(crate) fn is_valid(&self) -> bool {
        self.placement.as_ref().is_none_or(usable_channel)
            && self.fade_in.as_ref().is_none_or(usable_channel)
            && self.fade_out.as_ref().is_none_or(usable_channel)
    }

    /// Drop unsupported channels while keeping the valid ones. Public fields let a caller bypass the
    /// builder assertions; the layout should not lose a valid placement because one fade is unsupported.
    pub(crate) fn sanitized(&self) -> Self {
        Self {
            placement: self.placement.clone().filter(usable_channel),
            fade_in: self.fade_in.clone().filter(usable_channel),
            fade_out: self.fade_out.clone().filter(usable_channel),
        }
    }

    /// The default placement spring with no fades.
    pub fn new() -> Self { Self::default() }

    /// The configured placement curve, if any.
    pub fn placement_spec(&self) -> Option<&AnimationSpec> { self.placement.as_ref() }
    /// The configured fade-in curve, if any.
    pub fn fade_in_spec(&self) -> Option<&AnimationSpec> { self.fade_in.as_ref() }
    /// The configured fade-out curve, if any.
    pub fn fade_out_spec(&self) -> Option<&AnimationSpec> { self.fade_out.as_ref() }

    /// Sets the placement curve.
    pub fn placement(mut self, spec: impl Into<AnimationSpec>) -> Self {
        let spec = spec.into();
        assert!(usable_channel(&spec), "item placement must settle (got {spec:?})");
        self.placement = Some(spec);
        self
    }

    /// Disables movement while keeping any fade channels.
    pub fn without_placement(mut self) -> Self {
        self.placement = None;
        self
    }

    /// Sets the fade-in curve.
    pub fn fade_in(mut self, spec: impl Into<AnimationSpec>) -> Self {
        let spec = spec.into();
        assert!(usable_channel(&spec), "item fade_in must settle (got {spec:?})");
        self.fade_in = Some(spec);
        self
    }

    /// Sets the fade-out curve recorded for the exit layer.
    pub fn fade_out(mut self, spec: impl Into<AnimationSpec>) -> Self {
        let spec = spec.into();
        assert!(usable_channel(&spec), "item fade_out must settle (got {spec:?})");
        self.fade_out = Some(spec);
        self
    }

    /// True when the item has no channel at all, so a layout can skip its bookkeeping.
    pub fn is_inert(&self) -> bool {
        self.placement.is_none() && self.fade_in.is_none() && self.fade_out.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animation::{RepeatMode, RepeatableSpec, TweenSpec};

    #[test]
    #[should_panic(expected = "item fade_out must settle")]
    fn a_decaying_item_fade_is_rejected() {
        let _ = ItemAnimationSpec::new().fade_out(crate::animation::DecaySpec::new(4.2, 0.1));
    }

    #[test]
    fn a_finite_repeatable_item_fade_is_allowed() {
        let spec = ItemAnimationSpec::new().fade_out(AnimationSpec::Repeatable(
            RepeatableSpec::new(2, RepeatMode::Restart, TweenSpec::default()),
        ));
        assert!(spec.is_valid());
    }

    #[test]
    #[should_panic(expected = "item fade_out must settle")]
    fn an_even_reverse_repeatable_does_not_reach_the_target() {
        let _ = ItemAnimationSpec::new().fade_out(AnimationSpec::Repeatable(
            RepeatableSpec::new(2, RepeatMode::Reverse, TweenSpec::default()),
        ));
    }
}
