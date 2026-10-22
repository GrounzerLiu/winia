//! Top-level overlays — Popup / Dialog / DropdownMenu (mirrors Compose).
//!
//! Mechanism: overlay content **does not participate in main-tree layout** — it
//! is registered as an `OverlayDesc` during composition and materialized /
//! laid out / rendered by `app.rs` with a **dedicated Composer** (rendered
//! after the main tree = on top); pointer hit-testing prefers overlays
//! (topmost first) and an outside click triggers `on_dismiss_request`.
//!
//! Current limitations (v1):
//! - Overlay content only supports clickable (Button / menu items) — gestures /
//!   text selection will follow.
//! - Single-level popups (nesting will follow).

pub mod anchored_draggable;
use std::sync::Arc;
use crate::composable;

/// Where a panel anchored to a node starts, and how it gets to its final place.
///
/// Compose's `FullScreenSearchBarLayout` places its container at
/// `(lerp(collapsedBounds.left, offsetX, progress), lerp(collapsedBounds.top, offsetY, progress))`: the panel
/// grows out of the anchor's corner and ends at the window's corner. Both axes move together, so this is one
/// value rather than a flag per axis.
///
/// The anchor's coordinates are known only to the layout pass, so the caller supplies the progress reader
/// and the layout pass does the interpolation.
///
/// The two endpoints are resolved by `anchor_slide_origin` in the layout pass.

mod anim;
mod desc;
mod dialog;
mod placement;
mod popup;

pub(crate) use desc::*;
pub(crate) use placement::*;

pub use anim::{anchor_slide_lerp, overlay_reveal_clip_height, overlay_transform_origin, AnchorSlide, OverlayAnimSpec};
pub use desc::OverlayDesc;
pub use dialog::Dialog;
pub use placement::PopupPosition;
pub use popup::Popup;

#[cfg(test)]
mod tests;
