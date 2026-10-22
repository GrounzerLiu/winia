//! The shared-transition machinery the toolkit drives.
//!
//! A flight is stored on the runtime's composer (`shared_flights`), its visual is written onto the
//! `LayoutNode` by the layout pass and read by the renderer, and the Modifier chain carries its
//! configuration — so none of this can live in the component that composes it. The component keeps
//! the composables and the `Modifier` builders and imports what is below from here.

use crate::animation::visibility::VisibilityTransition;
use crate::animation::{AnimationSpec, KeyframesSpec, SpringSpec, TweenSpec};
use crate::layout::node::{
    scroll_offset_for_node, LayoutNode,
};
use crate::graphics::{ContentScale, ImageAlignment};
use crate::modifier::{Modifier, ModifierElement};
use crate::graphics::{Shape};
use crate::runtime::state::State;
use crate::unit::Size;
use std::collections::HashMap;

/// Flight rect in window-logical pixels: origin + size.
///
/// `lerp` deliberately does NOT clamp `t`: overshoot (`t > 1`, `t < 0`) flies
/// past the endpoint, which is how spring progress renders (Phase 2).

mod flight;
mod helpers;
mod types;

pub(crate) use flight::*;
pub(crate) use helpers::*;
pub(crate) use types::*;

pub use types::{BoundsTransform, FlightEdges, OverlayClip, PathMotion, PlaceHolderSize, ResizeMode, SharedBounds, SharedKind};

#[cfg(test)]
mod tests;
