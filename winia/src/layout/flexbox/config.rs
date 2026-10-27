//! FlexBox's vocabulary: the directions, wrap modes, alignment values and the two config types
//! (Compose's FlexBox.kt:1335-1835 and :2215-2253).

use crate::layout::Constraints;

// ── Vocabulary ──

/// Which way the main axis runs, and whether it runs backwards — Compose's FlexDirection
/// (FlexBox.kt:1335-1390).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexDirection {
    Row,
    RowReverse,
    Column,
    ColumnReverse,

}

impl FlexDirection {
    /// Whether the main axis is horizontal.
    pub const fn is_horizontal(self) -> bool {
        matches!(self, FlexDirection::Row | FlexDirection::RowReverse)
    }

    /// Whether items are laid out from the main-END towards the main-start.
    pub const fn is_reverse(self) -> bool {
        matches!(self, FlexDirection::RowReverse | FlexDirection::ColumnReverse)
    }
}

/// Whether items break onto new lines, and which way the lines stack — Compose's FlexWrap
/// (FlexBox.kt:1391-1438).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexWrap {
    NoWrap,
    Wrap,
    WrapReverse,
}

impl FlexWrap {
    pub(super) const fn is_enabled(self) -> bool {
        !matches!(self, FlexWrap::NoWrap)
    }

    pub(super) const fn is_cross_reverse(self) -> bool {
        matches!(self, FlexWrap::WrapReverse)
    }
}

/// How the leftover main-axis space is distributed inside a line — Compose's FlexJustifyContent
/// (FlexBox.kt:1632-1697).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexJustifyContent {
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

/// How an item sits inside its line's cross extent — Compose's FlexAlignItems
/// (FlexBox.kt:1439-1492).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexAlignItems {
    Start,
    End,
    Center,
    Stretch,
    Baseline,
}

/// A single item's cross-axis override — Compose's FlexAlignSelf (FlexBox.kt:1493-1558).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexAlignSelf {
    Auto,
    Start,
    End,
    Center,
    Stretch,
    Baseline,
}

/// How the lines themselves are distributed in the cross axis — Compose's FlexAlignContent
/// (FlexBox.kt:1559-1631). It has no SpaceEvenly: Compose's value set stops at SpaceAround.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexAlignContent {
    Start,
    End,
    Center,
    Stretch,
    SpaceBetween,
    SpaceAround,
}

/// An item's starting main-axis size — Compose's FlexBasis (FlexBox.kt:1698-1790).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FlexBasis {
    /// The item's max-content size (its size measured with an unbounded main axis).
    Auto,
    /// A fixed size.
    Dp(f32),
    /// A fraction of the container's main axis.
    Percent(f32),
}

// ── Per-item configuration ──

/// What a child of a FlexBox declares about itself — Compose's FlexConfig (FlexBox.kt:2215-2253).
///
/// The defaults are CSS's: grow 0, shrink 1, basis Auto, alignSelf Auto, order 0.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlexItemConfig {
    pub grow: f32,
    pub shrink: f32,
    pub basis: FlexBasis,
    pub align_self: FlexAlignSelf,
    pub order: i32,
}

impl FlexItemConfig {
    pub const fn new() -> Self {
        FlexItemConfig {
            grow: 0.0,
            shrink: 1.0,
            basis: FlexBasis::Auto,
            align_self: FlexAlignSelf::Auto,
            order: 0,
        }
    }

    /// How much of the positive free space this item takes (CSS flex-grow).
    pub const fn grow(mut self, grow: f32) -> Self {
        self.grow = grow;
        self
    }

    /// This item's share of a negative free space (CSS flex-shrink), weighted by its basis.
    pub const fn shrink(mut self, shrink: f32) -> Self {
        self.shrink = shrink;
        self
    }

    /// Where the item's main size starts from.
    pub const fn basis(mut self, basis: FlexBasis) -> Self {
        self.basis = basis;
        self
    }

    /// A fixed basis — Compose basis(Dp).
    pub const fn basis_dp(mut self, size: f32) -> Self {
        self.basis = FlexBasis::Dp(size);
        self
    }

    /// A basis as a fraction of the container's main axis — Compose basis(Float).
    pub const fn basis_percent(mut self, fraction: f32) -> Self {
        self.basis = FlexBasis::Percent(fraction);
        self
    }

    /// Override the container's align-items for this item.
    pub const fn align_self(mut self, align: FlexAlignSelf) -> Self {
        self.align_self = align;
        self
    }

    /// Where this item sorts; a smaller order comes first in its line.
    pub const fn order(mut self, order: i32) -> Self {
        self.order = order;
        self
    }
}

impl Default for FlexItemConfig {
    fn default() -> Self {
        Self::new()
    }
}

// ── Container configuration ──

/// Everything a FlexBox decides about its children — Compose's FlexBoxConfig (FlexBox.kt:1791-1835).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlexBoxConfig {
    pub direction: FlexDirection,
    pub wrap: FlexWrap,
    pub justify_content: FlexJustifyContent,
    pub align_items: FlexAlignItems,
    pub align_content: FlexAlignContent,
    /// The gap between lines (CSS row-gap when the main axis is horizontal).
    pub row_gap: f32,
    /// The gap between items of one line (CSS column-gap when the main axis is horizontal).
    pub column_gap: f32,
    /// Break a line after this many items even when they would fit. Compose's
    /// maxItemsInEachLine (FlexBox.kt:1961), a Compose addition with no CSS counterpart.
    pub max_items_in_each_line: usize,
}

impl FlexBoxConfig {
    pub const fn new() -> Self {
        FlexBoxConfig {
            direction: FlexDirection::Row,
            wrap: FlexWrap::NoWrap,
            justify_content: FlexJustifyContent::Start,
            align_items: FlexAlignItems::Start,
            align_content: FlexAlignContent::Start,
            row_gap: 0.0,
            column_gap: 0.0,
            max_items_in_each_line: usize::MAX,
        }
    }

    pub const fn direction(mut self, direction: FlexDirection) -> Self {
        self.direction = direction;
        self
    }

    pub const fn wrap(mut self, wrap: FlexWrap) -> Self {
        self.wrap = wrap;
        self
    }

    pub const fn justify_content(mut self, justify: FlexJustifyContent) -> Self {
        self.justify_content = justify;
        self
    }

    pub const fn align_items(mut self, align: FlexAlignItems) -> Self {
        self.align_items = align;
        self
    }

    pub const fn align_content(mut self, align: FlexAlignContent) -> Self {
        self.align_content = align;
        self
    }

    pub const fn row_gap(mut self, gap: f32) -> Self {
        self.row_gap = gap;
        self
    }

    pub const fn column_gap(mut self, gap: f32) -> Self {
        self.column_gap = gap;
        self
    }

    /// The same gap on both axes.
    pub const fn gap(self, gap: f32) -> Self {
        self.row_gap(gap).column_gap(gap)
    }

    pub const fn max_items_in_each_line(mut self, max: usize) -> Self {
        self.max_items_in_each_line = max;
        self
    }

    /// The gap between two items of one line: CSS column-gap for a row, row-gap for a column.
    pub(super) fn main_gap(&self, horizontal: bool) -> f32 {
        if horizontal { self.column_gap } else { self.row_gap }
    }

    /// The gap between two lines: the other axis of main_gap.
    pub(super) fn cross_gap(&self, horizontal: bool) -> f32 {
        if horizontal { self.row_gap } else { self.column_gap }
    }
}

impl Default for FlexBoxConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// What FlexBox::config_for receives when the config is resolved: the constraints the container was
/// measured with, and the density in effect. Compose exposes these through the scope a FlexBoxConfig
/// lambda runs in (FlexBox.kt:1836-2030); Rust cannot hand a closure a borrowed scope, so they come
/// by value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlexBoxConfigInput {
    pub constraints: Constraints,
    pub density: f32,
}
