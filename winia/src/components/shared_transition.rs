//! Shared element transition — matching, flight engine and the transition
//! layer.
//!
//! Public API lives on [`SharedTransitionScope`] / [`SharedTransitionLayout`]
//! and the two marker methods (`Modifier::shared_element`,
//! `Modifier::shared_bounds`, plus `render_in_shared_transition_scope_overlay`
//! for chrome). The coordinator (the `impl Composer` blocks below) runs at the
//! compose tail, after layout, and once more per frame before render:
//! match endpoints, drive progress, write per-frame visuals, and decide which
//! nodes the layer paints. See `docs/shared-element-transition.md` for the
//! architecture and `docs/shared-element-usage.md` for the guide.
//!
//! Layer map (see the design doc):
//! - Matching: [`SharedTransitionScope`], [`SharedContentState`], live maps.
//! - Flight engine: [`Flight`] state machine — `on_event` is pure and tested.
//! - Content providers: Tier 0 (same composer) / Tier 1 (main ↔ overlay);
//!   Tier 2 bitmap deliberately unbuilt.
//!
//! Motion note: the spec runs on the flight's four EDGES (`FlightEdges`, window pixels), which is
//! Compose's shape — its `BoundsTransform` returns a `FiniteAnimationSpec<Rect>` and the flight runs it
//! through `Rect.VectorConverter` (`animation/SharedTransitionScope.kt:265-268`, `:1323`, `:1534`), so
//! each edge is its own spring with `Rect.VisibilityThreshold` (one pixel each,
//! `VisibilityThresholds.kt:39-42`) and each inherits its own velocity when the endpoints change
//! mid-flight. A flight also carries a scalar 0→1 CLOCK on the same spec, which is what winia's content
//! crossfade rides (Compose runs enter/exit transitions on their own specs instead); the clock's
//! threshold stays in progress units — see `BoundsTransform::clock_spec`.
//!
//! `SharedBounds` itself keeps `supports_spring() == false`: it is the public rect type the renderer and
//! the tests pass around, while the animated value is the four-edge `FlightEdges`.
//!
//! The default shaping is Compose's spring — see [`BoundsTransform::default`].

// The flight machinery lives a layer down (`crate::transition`): the runtime stores flights,
// the layout pass writes their visuals and the renderer reads them, so none of it may live in
// this component. The composables and the `Modifier` builders stay here.
use crate::transition::*;
use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, Mutex};

use crate::animation::{
    push_animatable, AnimatableValue, AnimationSpec, SpringSpec,
};
// Only the tests build a tween by hand now that the default flight shaping is Compose's spring.
#[cfg(test)]
use crate::animation::TweenSpec;
use crate::runtime::composer::Composer;
use crate::runtime::composer::ComposeCtx;
use crate::runtime::composition_local::CompositionLocal;
use crate::runtime::state::State;
use crate::layout::node::{
    LayoutNode, PaintDisposition,
};
use crate::modifier::{Modifier, ModifierElement};
use crate::components::animated_visibility::VisibilityTransition;

/// Re-exported here (not from a child module) because both the modifiers and their tests
/// name these types through the shared-transition vocabulary.
pub use crate::graphics::{ContentScale, ImageAlignment};

mod flight;
mod modifiers;
mod state;

pub(crate) use flight::*;
pub(crate) use state::*;

pub use state::{
    current_nav_scene, current_shared_scope, nav_scene_is_prev, nav_scene_visibility,
    with_nav_scene, NavSceneInfo, SharedContentState, SharedTransitionDefaults,
    SharedTransitionLayout, SharedTransitionScope,
};

#[cfg(test)]
mod tier0_tests;
