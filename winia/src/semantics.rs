//! Semantics: what a node *is*, for a screen reader and for tests.
//!
//! Compose models this as a second tree ("the semantics tree") built from per-node semantics
//! properties, separate from both the composition and the layout tree. winia's version: a
//! [`crate::modifier::Modifier::semantics`] element declares role / name / state on a node, and
//! [`semantics_tree`] resolves those declarations into a tree at query time — no caching, no
//! invalidation, so a caller can ask once per frame (or once per platform request) and get the
//! current answer.
//!
//! Two rules do most of the work:
//!
//! - **A node that claims its subtree absorbs it.** A click target — or a node that asks for
//!   [`SemanticsConfig::merge_descendants`] explicitly — becomes ONE node whose name is the text it
//!   contains, so a button reads as "Submit" instead of as an unnamed button with a text child.
//!   Compose's `clickable` does the same inside foundation, which is why `Button { Text("Submit") }`
//!   is one node there too.
//! - **A node that declares nothing and claims nothing is transparent.** It does not appear at all;
//!   only its descendants do. A `Column` is therefore invisible to a screen reader, as it should be.
//!
//! Deliberate limits of this first slice (see `docs/semantics.md`): one tree rather than Compose's
//! (merged + unmerged) pair, a small property set — role, name, state, and the click action that is
//! already implied by the modifier chain — and no collections, custom actions or live regions.

use crate::layout::node::{scroll_offset_for_node, LayoutNode};
use crate::modifier::ModifierElement;
use crate::selection::ToggleableState;


mod json;
mod registry;
mod tree;
mod types;

pub(crate) use registry::*;
pub(crate) use tree::*;

pub use json::semantics_json;
pub use registry::{forget, has_actions, publish, published, request_action, take_actions, OverlaySemantics, UiAction, WindowSemantics};
pub use tree::{click_node_by_id, semantics_tree};
pub use types::{
    CollectionInfo, CollectionItemInfo, IndexForKeyAction, LiveRegionMode, ScrollToIndexAction,
    SemanticsAction, SemanticsConfig, SemanticsNode, SemanticsRole, SemanticsState,
};

#[cfg(test)]
mod tests;
