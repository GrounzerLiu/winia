//! `LazyColumn` / `LazyRow` — Compose foundation's lazy lists, on one axis-generic core.
//!
//! Both are the SAME type over a different axis marker: `LazyColumn = LazyList<VerticalAxis>` and
//! `LazyRow = LazyList<HorizontalAxis>` (`list.rs`), with the axis differences confined to
//! [`LazyAxis`]. Everything below therefore holds for either axis.
//!
//! Core mechanisms, aligned with Compose's lazy architecture:
//!
//! - **Anchor scroll model**: `LazyListState.first_visible_index + first_visible_offset`, not a raw
//!   pixel offset — the scroll position is "which item is first visible, and by how much" (Compose
//!   `LazyListScrollPosition`).
//! - **Stable keys**: `items()` takes a `key` factory (a data id, or any stable value). After items
//!   are inserted or removed at the head of the list, the key finds the original first visible item
//!   again so the viewport does not jump (Compose's
//!   `updateScrollPositionIfTheFirstItemWasMoved`).
//! - **Interval content**: `item`/`items` register as an `IntervalList` (count + key factory +
//!   content closure); a global index resolves to a (section, local index) pair — Compose's
//!   `LazyListIntervalContent`.
//! - **Lazy measurement**: composition estimates the visible range `[start, end]` from the height
//!   cache and registers only those items; measurement places them for real and writes the measured
//!   sizes back into the cache (uncached items are priced with the default estimate, and a large
//!   enough deviation asks for another compose).
//!
//! ⚠ Difference from Compose: winia's composition is imperative (`build` registers nodes directly)
//! and has no `LazyLayout` measure-time composition. The model here is two-phase — estimate during
//! compose, correct during measure (`docs/lazy-column.md` §2.9).

use crate::runtime::composer::{ComposeCtx, GroupStatus};
use crate::composable;
use crate::layout::BoxLayout;
use crate::layout::constraints::Constraints;
use crate::modifier::Modifier;
use std::marker::PhantomData;
use std::sync::Arc;

/// The size an unmeasured item is priced at when it first enters the viewport; the measured size is
/// written back to the cache and used from then on.
pub const LAZY_ITEM_ESTIMATED_HEIGHT: f32 = 48.0;
/// How many items beyond the viewport stay registered on each side (Compose's
/// `beyondViewportPageCount`) — prefetching them keeps scrolling from constantly re-registering.
pub const LAZY_BEYOND_BOUNDS: usize = 4;

/// The lazy list's main axis. Sealed, like [`Axis`](super::axis::Axis): the two markers it carries are
/// the whole set.
///
/// The questions both container families ask — main and cross extents, `size`, `point` — are on
/// [`Axis`](super::axis::Axis); this adds what only a lazy list needs.
pub trait LazyAxis: super::axis::Axis {
    /// A child's constraints: the cross axis inherits the parent's max, the main axis is unbounded
    /// (wrap content).
    #[doc(hidden)]
    fn child_constraints(c: Constraints) -> Constraints;
    /// Replace the cross-axis max (`contentPadding` shrinks the item's usable width).
    #[doc(hidden)]
    fn with_cross_max(c: Constraints, v: f32) -> Constraints;
    /// Pin the main axis's min AND max to `v` — what `fillParentMaxWidth` / `fillParentMaxHeight` do.
    ///
    /// Off by default: the unbounded main axis is what lets an item wrap to its content (see
    /// [`Self::child_constraints`]). A PAGED list wants the opposite — each item exactly fills the
    /// viewport, which is what makes a snap position an item boundary.
    #[doc(hidden)]
    fn with_main_exact(c: Constraints, v: f32) -> Constraints;
    /// The scroll modifier for this axis — `vertical_scroll` or `horizontal_scroll`.
    #[doc(hidden)]
    fn scroll(state: crate::modifier::ScrollState) -> Modifier;
    /// Whether this axis scrolls vertically — the direction an item's placement animates in.
    #[doc(hidden)]
    const VERTICAL: bool;
}

impl LazyAxis for crate::layout::axis::VerticalAxis {
    const VERTICAL: bool = true;
    fn child_constraints(c: Constraints) -> Constraints {
        Constraints { min_width: 0.0, max_width: c.max_width, min_height: 0.0, max_height: f32::MAX }
    }
    fn with_cross_max(c: Constraints, v: f32) -> Constraints {
        Constraints { min_width: 0.0, max_width: v, min_height: 0.0, max_height: c.max_height }
    }
    fn with_main_exact(c: Constraints, v: f32) -> Constraints {
        Constraints { min_width: c.min_width, max_width: c.max_width, min_height: v, max_height: v }
    }
    fn scroll(state: crate::modifier::ScrollState) -> Modifier {
        Modifier::new().vertical_scroll(state)
    }
}

impl LazyAxis for crate::layout::axis::HorizontalAxis {
    const VERTICAL: bool = false;
    fn child_constraints(c: Constraints) -> Constraints {
        Constraints { min_width: 0.0, max_width: f32::MAX, min_height: 0.0, max_height: c.max_height }
    }
    fn with_cross_max(c: Constraints, v: f32) -> Constraints {
        Constraints { min_width: 0.0, max_width: c.max_width, min_height: 0.0, max_height: v }
    }
    fn with_main_exact(c: Constraints, v: f32) -> Constraints {
        Constraints { min_width: v, max_width: v, min_height: c.min_height, max_height: c.max_height }
    }
    fn scroll(state: crate::modifier::ScrollState) -> Modifier {
        Modifier::new().horizontal_scroll(state)
    }
}


mod interval;
mod list;
mod policy;
mod state;

pub(crate) use interval::*;
pub(crate) use list::*;
pub(crate) use policy::*;

pub use list::{ItemHeightCache, LazyColumn, LazyList, LazyRow};
pub use state::LazyListState;

#[cfg(test)]
mod tests;
