//! The animation contract for keyed layout items.
//!
//! A layout container sees an item's new slot but not where the item was painted a frame ago.
//! `ItemAnimationSpec` is the data a child attaches with `Modifier::animate_item`; the container's
//! `ItemAnimationHost` owns the per-key states. Keeping the spec on the item means a lazy list and a
//! Flow share one marker and one runtime instead of growing a second switch on each container.

use crate::animation::{AnimationSpec, SpringSpec};

/// Placement and fade curves for one item.
#[derive(Clone, Debug)]
pub struct ItemAnimationSpec {
    /// How the paint offset travels from the old slot to the new one. `None` disables movement
    /// (useful for an item that only fades).
    pub placement: Option<AnimationSpec>,
    /// Fade-in applied when the key first appears after the host's first layout.
    pub fade_in: Option<AnimationSpec>,
    /// Fade-out curve for a removed key. The host captures the item's last rendered frame and the
    /// exit layer consumes this curve after the item leaves the layout tree.
    pub fade_out: Option<AnimationSpec>,
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

impl ItemAnimationSpec {
    /// The default placement spring with no fades.
    pub fn new() -> Self { Self::default() }

    /// Sets the placement curve.
    pub fn placement(mut self, spec: impl Into<AnimationSpec>) -> Self {
        self.placement = Some(spec.into());
        self
    }

    /// Disables movement while keeping any fade channels.
    pub fn without_placement(mut self) -> Self {
        self.placement = None;
        self
    }

    /// Sets the fade-in curve.
    pub fn fade_in(mut self, spec: impl Into<AnimationSpec>) -> Self {
        self.fade_in = Some(spec.into());
        self
    }

    /// Sets the fade-out curve recorded for the exit layer.
    pub fn fade_out(mut self, spec: impl Into<AnimationSpec>) -> Self {
        self.fade_out = Some(spec.into());
        self
    }

    /// True when the item has no channel at all, so a layout can skip its bookkeeping.
    pub fn is_inert(&self) -> bool {
        self.placement.is_none() && self.fade_in.is_none() && self.fade_out.is_none()
    }
}
