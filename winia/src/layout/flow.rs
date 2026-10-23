//! Flow layout — Compose foundation's `FlowRow` / `FlowColumn` (`FlowLayout.kt` and
//! `FlowLayoutBuildingBlocks.kt`; see `docs/flow-layout.md`).
//!
//! A line wraps once the main axis is full (`getWrapInfo` semantics): never before the first
//! item, after `max_items`, or when the remaining main-axis room cannot fit the next item. A
//! line's height is its tallest item that fits; items align on the cross axis by `alignment`;
//! the remaining main-axis space is distributed by `arrangement` (through `compute_spacing`);
//! `cross_arrangement` distributes the free cross space and `cross_spacing` adds a fixed gap
//! between lines. A child may fill a fraction of its line's cross extent
//! (`fill_max_row_height` / `fill_max_column_width`); weighted children keep their weight
//! allocation and are cross-filled as well. Items past the cross-axis maximum follow Compose's default `Clip` overflow: they
//! are measured but not placed.
//!
//! v1 scope (differences from Compose, documented in `docs/flow-layout.md`):
//! - weight is supported within a line; line breaking still uses the child's natural size
//! - `max_lines` and the `Clip` / `Visible` overflow modes exist; the indicator variants
//!   (`ExpandIndicator` / `ExpandOrCollapseIndicator`) do not
//! - intrinsic queries are answered from the children's intrinsic sizes, using the same line
//!   and max-items/max-lines rules as measurement

/// What happens to items that do not fit the cross-axis maximum, and whether `max_lines` applies
/// (Compose's `FlowRowOverflow`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FlowOverflow {
    /// Stop placing items once the cross-axis budget is exhausted; `max_lines` caps the rows
    /// (Compose's default).
    #[default]
    Clip,
    /// Place every line even when it extends past the cross-axis maximum; `max_lines` is ignored
    /// (Compose's `Visible`).
    Visible,
}

mod column;
mod intrinsic;
mod measure;
mod row;

pub use column::{FlowColumn, FlowColumnLayout};
pub use row::{FlowRow, FlowRowLayout};

#[cfg(test)]
mod tests;
