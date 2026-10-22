//! Flow layout — Compose foundation's `FlowRow` / `FlowColumn` (`FlowLayout.kt` and
//! `FlowLayoutBuildingBlocks.kt`; see `docs/flow-layout.md`).
//!
//! A line wraps once the main axis is full (`getWrapInfo` semantics): never before the first
//! item, after `max_items`, or when the remaining main-axis room cannot fit the next item. A
//! line's height is its tallest item; items align on the cross axis by `alignment`; the
//! remaining main-axis space is distributed by `arrangement` (through `compute_spacing`); and
//! `spacing` separates the lines.
//!
//! v1 scope (differences from Compose, documented in `docs/flow-layout.md`):
//! - no weight (Compose re-distributes the line's remaining space in a second measure pass)
//! - no maxLines/overflow (contextual API)
//! - no intrinsic (winia has no intrinsic system)

mod column;
mod measure;
mod row;

pub use column::{FlowColumn, FlowColumnLayout};
pub use row::{FlowRow, FlowRowLayout};

#[cfg(test)]
mod tests;
