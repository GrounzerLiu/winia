//! Flow layout — Compose foundation's `FlowRow` / `FlowColumn` (`FlowLayout.kt` and
//! `FlowLayoutBuildingBlocks.kt`; see `docs/flow-layout.md`).
//!
//! A line wraps once the main axis is full (`getWrapInfo` semantics): never before the first
//! item, after `max_items`, or when the remaining main-axis room cannot fit the next item. A
//! line's height is its tallest item that fits; items align on the cross axis by `alignment`;
//! the remaining main-axis space is distributed by `arrangement` (through `compute_spacing`);
//! and `spacing` separates the lines. Items past the cross-axis maximum follow Compose's default
//! `Clip` overflow: they are measured but not placed.
//!
//! v1 scope (differences from Compose, documented in `docs/flow-layout.md`):
//! - no weight (Compose re-distributes the line's remaining space in a second measure pass)
//! - no `maxLines` or overflow-indicator API (`ExpandIndicator` / `ExpandOrCollapseIndicator`);
//!   the default Clip cross-axis cut is implemented
//! - no intrinsic overrides (the generic `MeasurePolicy` approximation is used)

mod column;
mod measure;
mod row;

pub use column::{FlowColumn, FlowColumnLayout};
pub use row::{FlowRow, FlowRowLayout};

#[cfg(test)]
mod tests;
