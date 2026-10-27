//! FlexBox — CSS flexbox, matching Compose's FlexBox / FlexBoxConfig (foundation-layout 1.13).
//!
//! Compose's FlexBox (FlexBox.kt:133) is a CSS-flex layout: a main axis with grow / shrink / basis,
//! line breaking (FlexWrap), justify-content / align-items / align-self / align-content and a
//! per-item order. It is a DIFFERENT algorithm from Row/Column, which follow Compose's
//! RowColumnMeasurePolicy (weights, no wrapping, no basis), so it lives in its own module instead of
//! being another arm of flex.rs.
//!
//! Where each phase comes from:
//! - items and their basis: createFlexItem (FlexBox.kt:354-406)
//! - line breaking: buildFlexLines (FlexBox.kt:417-496)
//! - flexible lengths: resolveFlexibleLengths (FlexBox.kt:529-676)
//! - line cross sizes: calculateLineCrossAxisSize (FlexBox.kt:916-968)
//! - align-content stretch: applyAlignContentStretch (FlexBox.kt:677-713)
//! - measuring items: measureFlexItems (FlexBox.kt:714-770), measureItem (FlexBox.kt:969-1010)
//! - placing: calculateLineCrossPositions (FlexBox.kt:771-810), positionItemsOnMainAxis
//!   (FlexBox.kt:811-871), calculateItemCrossPosition (FlexBox.kt:872-915), placeFlexItems
//!   (FlexBox.kt:313-353)
//! - container size: measureFlexBox (FlexBox.kt:192-311)
//!
//! What is deliberately not modelled:
//! - Compose resolves the config at MEASURE time through a scope that exposes the incoming
//!   constraints and the density, so a config can be responsive. winia cannot hand a Rust closure a
//!   borrowed scope the way a Kotlin fun interface does, so FlexBox::config_for takes a
//!   FlexBoxConfigInput (constraints + density) BY VALUE — the same information, one copy per
//!   measure.
//! - There is no FlexBoxScope: flex(..) is a plain Modifier, so a config placed on a child of
//!   another container is silently ignored (the same convention layout_weight and align_self follow).
//! - Modifier.weight is not mapped to grow. Compose maps a main-axis fill fraction onto grow when
//!   the grow factor is unset (FlexBox.kt:366-376); winia's weight means "share the leftover", which
//!   is not CSS grow, so mapping it would silently change what weight means inside a FlexBox. Write
//!   grow explicitly instead.
//! - Baseline alignment uses each child's first baseline (AlignmentLine::FIRST_BASELINE) unless the
//!   child sets a line with Modifier::align_by_baseline.
//!
//! Unlike most containers this one is NOT restartable-free: every child reads its flex config from
//! its own modifier chain, so the container declares only its own parameters and the children declare
//! theirs.

use super::container::build_container;
use super::node::intrinsic_size_of;
use super::node::{measure_node, IntrinsicQuery, LayoutNode, MeasurePolicy, Placement};
use super::Constraints;
use crate::modifier::Modifier;
use crate::runtime::composer::ComposeCtx;
use crate::unit::{Offset, Size};
use std::sync::Arc;

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
    const fn is_enabled(self) -> bool {
        !matches!(self, FlexWrap::NoWrap)
    }

    const fn is_cross_reverse(self) -> bool {
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
    fn main_gap(&self, horizontal: bool) -> f32 {
        if horizontal { self.column_gap } else { self.row_gap }
    }

    /// The gap between two lines: the other axis of main_gap.
    fn cross_gap(&self, horizontal: bool) -> f32 {
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

// ── Axis helpers ──

fn main_max(c: &Constraints, horizontal: bool) -> f32 {
    if horizontal { c.max_width } else { c.max_height }
}

fn main_min(c: &Constraints, horizontal: bool) -> f32 {
    if horizontal { c.min_width } else { c.min_height }
}

fn cross_max(c: &Constraints, horizontal: bool) -> f32 {
    if horizontal { c.max_height } else { c.max_width }
}

fn cross_min(c: &Constraints, horizontal: bool) -> f32 {
    if horizontal { c.min_height } else { c.min_width }
}

fn main_of(horizontal: bool, size: Size) -> f32 {
    if horizontal { size.width } else { size.height }
}

fn cross_of(horizontal: bool, size: Size) -> f32 {
    if horizontal { size.height } else { size.width }
}

fn make_size(horizontal: bool, main: f32, cross: f32) -> Size {
    if horizontal { Size::new(main, cross) } else { Size::new(cross, main) }
}

/// A child measured with an unbounded main axis and the container's cross bound: CSS max-content
/// along the main axis. This is what FlexBasis::Auto means (FlexBox.kt:388-395).
fn content_constraints(horizontal: bool, cross_limit: f32) -> Constraints {
    if horizontal {
        Constraints::new(0.0, f32::MAX, 0.0, cross_limit)
    } else {
        Constraints::new(0.0, cross_limit, 0.0, f32::MAX)
    }
}

/// A child measured with an exact main size and a loose cross axis.
fn item_constraints(horizontal: bool, main: f32, cross_limit: f32) -> Constraints {
    if horizontal {
        Constraints::new(main, main, 0.0, cross_limit)
    } else {
        Constraints::new(0.0, cross_limit, main, main)
    }
}

/// A child measured with an exact main size AND an exact cross size (CSS stretch).
fn stretched_constraints(horizontal: bool, main: f32, cross: f32) -> Constraints {
    if horizontal {
        Constraints::new(main, main, cross, cross)
    } else {
        Constraints::new(cross, cross, main, main)
    }
}

// ── Items and lines ──

/// One child's resolved flex state.
#[derive(Debug, Clone)]
struct FlexItem {
    /// The child's index in the arena (composition order), kept so placements can be written back
    /// after order has reordered the item list.
    child: usize,
    /// The child's POSITION in the children slice; placements are indexed by it.
    slot: usize,
    order: i32,
    grow: f32,
    shrink: f32,
    align_self: FlexAlignSelf,
    /// CSS flex base size: the basis resolved against the container.
    base: f32,
    /// CSS hypothetical main size: max(base, min-content).
    hypothetical: f32,
    /// min-content along the main axis.
    min_main: f32,
    target: f32,
    /// The main size the child actually measured at (it can clamp itself below its share).
    measured_main: f32,
    frozen: bool,
    cross: f32,
    /// The item's distance from its own top to its baseline.
    baseline: f32,
    /// Where the item ends up, in the container's frame before the RTL mirror.
    main_position: f32,
    cross_position: f32,
}

/// One line of items.
#[derive(Debug, Clone)]
struct FlexLine {
    start: usize,
    end: usize,
    main: f32,
    cross: f32,
    cross_start: f32,
    max_above_baseline: f32,
}

impl FlexLine {
    fn len(&self) -> usize {
        self.end - self.start
    }
}

// ── The policy ──

/// The measure policy behind FlexBox.
///
/// Debug is written by hand: the responsive config is a closure, which has no Debug of its own.
pub struct FlexBoxLayout {
    pub(crate) config: FlexBoxConfig,
    pub(crate) resolver: Option<Arc<dyn Fn(&FlexBoxConfigInput) -> FlexBoxConfig>>,
    pub(crate) direction: crate::layout::LayoutDirection,
}

impl std::fmt::Debug for FlexBoxLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FlexBoxLayout")
            .field("config", &self.config)
            .field("responsive", &self.resolver.is_some())
            .field("direction", &self.direction)
            .finish()
    }
}

impl FlexBoxLayout {
    /// A layout with a fixed config; the builder adds the responsive form.
    pub fn new(config: FlexBoxConfig) -> Self {
        FlexBoxLayout {
            config,
            resolver: None,
            direction: crate::layout::LayoutDirection::Ltr,
        }
    }

    fn resolve(&self, constraints: Constraints) -> FlexBoxConfig {
        match &self.resolver {
            Some(resolver) => resolver(&FlexBoxConfigInput {
                constraints,
                density: crate::runtime::density::current_density().density,
            }),
            None => self.config,
        }
    }

    /// CSS align-items as the item-level align-self it corresponds to (FlexBox.kt:872-915).
    fn effective_self(&self, item: &FlexItem, config: &FlexBoxConfig) -> FlexAlignSelf {
        match item.align_self {
            FlexAlignSelf::Auto => match config.align_items {
                FlexAlignItems::Start => FlexAlignSelf::Start,
                FlexAlignItems::End => FlexAlignSelf::End,
                FlexAlignItems::Center => FlexAlignSelf::Center,
                FlexAlignItems::Stretch => FlexAlignSelf::Stretch,
                FlexAlignItems::Baseline => FlexAlignSelf::Baseline,
            },
            other => other,
        }
    }
}

impl MeasurePolicy for FlexBoxLayout {
    fn measure(
        &self,
        nodes: &mut Vec<LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        let config = self.resolve(constraints);
        let horizontal = config.direction.is_horizontal();
        let reverse_main = config.direction.is_reverse();
        let cross_reverse = config.wrap.is_cross_reverse();
        let main_gap = config.main_gap(horizontal);
        let cross_gap = config.cross_gap(horizontal);
        let main_limit = main_max(&constraints, horizontal);
        let cross_limit = cross_max(&constraints, horizontal);
        let cross_floor = cross_min(&constraints, horizontal);

        if children.is_empty() {
            return (
                make_size(horizontal, main_min(&constraints, horizontal), cross_floor),
                Vec::new(),
            );
        }

        // ── Phase 1: each item's basis, grow/shrink and order (createFlexItem, :354-406) ──
        let mut items: Vec<FlexItem> = Vec::with_capacity(children.len());
        for (slot, &child) in children.iter().enumerate() {
            let declared = nodes[child].modifier.get_flex_item().unwrap_or_default();
            // max-content along the main axis: measured once, used by Auto AND by a percentage basis
            // on an unbounded axis — both are Compose's own fallbacks (:379-395).
            let probe = measure_node(
                nodes,
                policies,
                child,
                content_constraints(horizontal, cross_limit),
            )
            .0;
            let max_content = main_of(horizontal, probe);
            let base = match declared.basis {
                FlexBasis::Dp(value) => value.max(0.0),
                FlexBasis::Percent(fraction) => {
                    // A NaN fraction (a computed 0.0/0.0) falls back to max-content, as Compose's
                    // isNaN check does (:386-394), instead of collapsing the item to zero.
                    if fraction.is_nan() || main_limit >= f32::MAX {
                        max_content
                    } else {
                        (main_limit * fraction.max(0.0)).max(0.0)
                    }
                }
                FlexBasis::Auto => max_content,
            };
            let min_main = intrinsic_size_of(
                nodes,
                policies,
                child,
                if horizontal {
                    IntrinsicQuery::MinWidth
                } else {
                    IntrinsicQuery::MinHeight
                },
                cross_limit,
            );
            // Compose turns a main-axis fill into growth when the item declares no grow factor
            // (FlexBox.kt:365-377). grow 0.0 is treated as "not declared", which is what CSS grow 0
            // effectively is.
            let grow = if declared.grow > 0.0 {
                declared.grow
            } else {
                nodes[child]
                    .modifier
                    .get_main_axis_fill(horizontal)
                    .unwrap_or(0.0)
            };
            items.push(FlexItem {
                child,
                slot,
                order: declared.order,
                grow: grow.max(0.0),
                shrink: declared.shrink.max(0.0),
                align_self: declared.align_self,
                base,
                hypothetical: base.max(min_main),
                min_main,
                target: base.max(min_main),
                measured_main: base.max(min_main),
                frozen: false,
                cross: 0.0,
                baseline: 0.0,
                main_position: 0.0,
                cross_position: 0.0,
            });
        }

        // CSS order: a stable sort, so equal orders keep composition order.
        if items.iter().any(|item| item.order != 0) {
            items.sort_by_key(|item| item.order);
        }

        // ── Phase 2: break the items into lines (buildFlexLines, :417-496) ──
        let mut lines: Vec<FlexLine> = Vec::new();
        let mut line_start = 0usize;
        // Compose accumulates the hypothetical main size WITH a trailing gap and subtracts it when
        // the line is closed (:451-496).
        let mut accumulated = 0.0f32;
        for index in 0..items.len() {
            let hypothetical = items[index].hypothetical;
            let would_overflow = accumulated + hypothetical > main_limit;
            let too_many = index - line_start >= config.max_items_in_each_line;
            if config.wrap.is_enabled() && index > line_start && (too_many || would_overflow) {
                let main = resolve_line(&mut items[line_start..index], main_limit, main_gap);
                lines.push(FlexLine {
                    start: line_start,
                    end: index,
                    main,
                    cross: 0.0,
                    cross_start: 0.0,
                    max_above_baseline: 0.0,
                });
                line_start = index;
                accumulated = hypothetical + main_gap;
            } else {
                accumulated += hypothetical + main_gap;
            }
        }
        let main = resolve_line(&mut items[line_start..], main_limit, main_gap);
        lines.push(FlexLine {
            start: line_start,
            end: items.len(),
            main,
            cross: 0.0,
            cross_start: 0.0,
            max_above_baseline: 0.0,
        });

        // ── Phase 3a: measure every item at a tight main size (measureFlexItems, :714-770) ──
        //
        // Each line's items are clamped to the cross space LEFT for that line, as Compose does with
        // its running remainingCrossAxisSize (:723, :763-765, :984-1006).
        let mut remaining_cross = cross_limit;
        for line in &mut lines {
            let line_limit = remaining_cross.min(cross_limit);
            let mut line_cross = 0.0f32;
            let mut above = 0.0f32;
            let mut below = 0.0f32;
            let mut aligns_by_baseline = false;
            for index in line.start..line.end {
                let child = items[index].child;
                let baseline_line = nodes[child]
                    .modifier
                    .get_align_by()
                    .unwrap_or(crate::layout::node::AlignmentLine::FIRST_BASELINE);
                let aligns = matches!(
                    self.effective_self(&items[index], &config),
                    FlexAlignSelf::Baseline
                );
                let size = measure_node(
                    nodes,
                    policies,
                    child,
                    item_constraints(horizontal, items[index].target, line_limit),
                )
                .0;
                items[index].cross = cross_of(horizontal, size);
                items[index].measured_main = main_of(horizontal, size);
                if aligns {
                    aligns_by_baseline = true;
                    // A child that publishes no line hangs its whole height BELOW the baseline —
                    // Compose falls back to the placeable's height (:2124-2146).
                    let baseline = nodes[child]
                        .alignment_line(baseline_line)
                        .unwrap_or(items[index].cross);
                    items[index].baseline = baseline;
                    above = above.max(baseline);
                    below = below.max(items[index].cross - baseline);
                } else {
                    line_cross = line_cross.max(items[index].cross);
                }
            }
            line.cross = if aligns_by_baseline {
                line_cross.max(above + below)
            } else {
                line_cross
            };
            line.max_above_baseline = above;
            // A single line (NoWrap) takes the incoming cross minimum, not just its tallest item
            // (:226-232).
            if !config.wrap.is_enabled() {
                line.cross = line.cross.max(cross_floor);
            }
            remaining_cross = (line_limit - line.cross - cross_gap).max(0.0);
        }

        // ── Phase 3b: align-content: stretch grows the LINES first (:246-254) ──
        //
        // It has to run before the stretched items are measured, or a stretched item fills the
        // line's INTRINSIC extent and leaves the grown part of the line empty.
        let total_gap_cross = cross_gap * (lines.len().saturating_sub(1) as f32);
        let mut lines_cross: f32 = lines.iter().map(|line| line.cross).sum::<f32>();
        if config.wrap.is_enabled()
            && config.align_content == FlexAlignContent::Stretch
            && cross_floor < f32::MAX
            && lines_cross + total_gap_cross < cross_floor
        {
            let extra = (cross_floor - lines_cross - total_gap_cross) / lines.len() as f32;
            for line in &mut lines {
                line.cross += extra;
            }
            lines_cross = lines.iter().map(|line| line.cross).sum();
        }

        // ── Phase 3c: a stretched item fills its line's (final) cross extent (:714-770) ──
        for line in &lines {
            for index in line.start..line.end {
                if matches!(
                    self.effective_self(&items[index], &config),
                    FlexAlignSelf::Stretch
                ) {
                    let child = items[index].child;
                    let cross = line.cross.min(cross_limit);
                    let size = measure_node(
                        nodes,
                        policies,
                        child,
                        stretched_constraints(horizontal, items[index].target, cross),
                    )
                    .0;
                    items[index].cross = cross_of(horizontal, size);
                }
            }
        }

        // ── Phase 4: align-content — distribute the lines in the cross axis (:771-810) ──
        //
        // free = the space left over the lines PLUS the gaps between them; the gap must be subtracted
        // exactly once (totalLinesCrossSize in Compose does not include it, :779-780).
        let total_cross = lines_cross + total_gap_cross;
        if config.wrap.is_enabled() {
            let free = (cross_floor.max(total_cross) - total_cross).max(0.0);
            let space_between = match config.align_content {
                FlexAlignContent::SpaceAround => free / lines.len() as f32,
                FlexAlignContent::SpaceBetween => {
                    if lines.len() > 1 {
                        free / (lines.len() - 1) as f32
                    } else {
                        0.0
                    }
                }
                _ => 0.0,
            };
            let mut position = match config.align_content {
                FlexAlignContent::End => {
                    if cross_reverse {
                        0.0
                    } else {
                        free
                    }
                }
                FlexAlignContent::Center => free / 2.0,
                FlexAlignContent::SpaceAround => space_between / 2.0,
                _ => {
                    if cross_reverse {
                        free
                    } else {
                        0.0
                    }
                }
            };
            let order: Vec<usize> = if cross_reverse {
                (0..lines.len()).rev().collect()
            } else {
                (0..lines.len()).collect()
            };
            for index in order {
                lines[index].cross_start = position;
                position += lines[index].cross + space_between + cross_gap;
            }
        }

        // ── Phase 5: the container's own size (:192-311) ──
        let content_cross = lines
            .iter()
            .map(|line| line.cross_start + line.cross)
            .fold(0.0f32, f32::max);
        let content_main = lines.iter().map(|line| line.main).fold(0.0f32, f32::max);
        let floor_main = main_min(&constraints, horizontal);
        let container_main = content_main
            .max(floor_main)
            .clamp(floor_main, main_max(&constraints, horizontal));
        let container_cross = content_cross
            .max(cross_floor)
            .clamp(cross_floor, cross_limit);

        // ── Phase 6: place the items (:811-871, :872-915, :313-353) ──
        for line in &lines {
            let remaining = container_main - line.main;
            let count = line.len();
            let space_between = match config.justify_content {
                FlexJustifyContent::SpaceAround => {
                    if count > 0 {
                        remaining / count as f32
                    } else {
                        0.0
                    }
                }
                FlexJustifyContent::SpaceBetween => {
                    if count > 1 {
                        remaining / (count - 1) as f32
                    } else {
                        0.0
                    }
                }
                FlexJustifyContent::SpaceEvenly => remaining / (count + 1) as f32,
                _ => 0.0,
            };
            let mut main_position = match config.justify_content {
                FlexJustifyContent::End => {
                    if reverse_main {
                        0.0
                    } else {
                        remaining
                    }
                }
                FlexJustifyContent::Center => remaining / 2.0,
                FlexJustifyContent::SpaceAround => space_between / 2.0,
                FlexJustifyContent::SpaceEvenly => space_between,
                FlexJustifyContent::SpaceBetween => {
                    if count == 1 && reverse_main {
                        remaining
                    } else {
                        0.0
                    }
                }
                FlexJustifyContent::Start => {
                    if reverse_main {
                        remaining
                    } else {
                        0.0
                    }
                }
            };
            let order: Vec<usize> = if reverse_main {
                (line.start..line.end).rev().collect()
            } else {
                (line.start..line.end).collect()
            };
            for index in order {
                items[index].main_position = main_position;
                main_position += items[index].target + space_between + main_gap;
                let cross_offset = match self.effective_self(&items[index], &config) {
                    FlexAlignSelf::Start | FlexAlignSelf::Stretch | FlexAlignSelf::Auto => 0.0,
                    FlexAlignSelf::End => line.cross - items[index].cross,
                    FlexAlignSelf::Center => (line.cross - items[index].cross) / 2.0,
                    FlexAlignSelf::Baseline => line.max_above_baseline - items[index].baseline,
                };
                items[index].cross_position = line.cross_start + cross_offset;
            }
        }

        // placeRelative mirrors only the x coordinate under RTL (placeFlexItems, :313-353). The
        // placement list is indexed by the child's position in composition order, so each item
        // carries its slot.
        let rtl = self.direction == crate::layout::LayoutDirection::Rtl;
        let container_size = make_size(horizontal, container_main, container_cross);
        let mut placements: Vec<Placement> = vec![
            Placement {
                size: Size::new(0.0, 0.0),
                position: Offset::new(0.0, 0.0),
            };
            children.len()
        ];
        for item in &items {
            let child_size = make_size(horizontal, item.measured_main, item.cross);
            let mut x = if horizontal {
                item.main_position
            } else {
                item.cross_position
            };
            let y = if horizontal {
                item.cross_position
            } else {
                item.main_position
            };
            if rtl {
                x = container_size.width - x - child_size.width;
            }
            placements[item.slot] = Placement {
                size: child_size,
                position: Offset::new(x, y),
            };
        }
        (container_size, placements)
    }

    fn place(&self, nodes: &mut Vec<LayoutNode>, children: &[usize], placements: &[Placement]) {
        for (index, child) in children.iter().enumerate() {
            if let Some(placement) = placements.get(index) {
                nodes[*child].position = placement.position;
                nodes[*child].measured_size = placement.size;
            }
        }
    }
}

/// CSS flexible-length resolution for one line (resolveFlexibleLengths, FlexBox.kt:529-676).
///
/// Returns the line's main size: the sum of the resolved targets plus the gaps.
fn resolve_line(items: &mut [FlexItem], container_main: f32, gap: f32) -> f32 {
    let count = items.len();
    if count == 0 {
        return 0.0;
    }
    let total_gap = if count > 1 {
        (count as f32 - 1.0) * gap
    } else {
        0.0
    };
    // An unbounded main axis has nothing to distribute: every item stays at its hypothetical size.
    if container_main >= f32::MAX {
        let mut sum = total_gap;
        for item in items.iter_mut() {
            item.target = item.hypothetical;
            sum += item.target;
        }
        return sum;
    }

    let hypothetical_line: f32 =
        items.iter().map(|item| item.hypothetical).sum::<f32>() + total_gap;
    let growing = hypothetical_line < container_main;

    let mut sum_frozen = 0.0f32;
    let mut sum_base = 0.0f32;
    let mut sum_factors = 0.0f32;
    for item in items.iter_mut() {
        let factor = if growing { item.grow } else { item.shrink };
        // Compose freezes an item that has no factor at all, and — when shrinking — one whose basis
        // is smaller than its hypothetical size (:556-570, the CSS min-content rule).
        let freezes = factor <= 0.0 || (!growing && item.base < item.hypothetical);
        if freezes {
            item.target = item.hypothetical;
            item.frozen = true;
            sum_frozen += item.target;
        } else {
            item.frozen = false;
            sum_base += item.base;
            sum_factors += factor;
        }
    }
    let initial_free = container_main - total_gap - sum_frozen - sum_base;

    if growing {
        let sum_sizes = total_gap + sum_frozen + sum_base;
        let mut free = container_main - sum_sizes;
        // CSS 9.7 step 4.b: with less than one total grow the items may not take more than their
        // share of the INITIAL free space (:585-600).
        if sum_factors < 1.0 {
            let fractional = initial_free * sum_factors;
            if fractional.abs() < free.abs() {
                free = fractional;
            }
        }
        let mut cap_left = Some(free.max(0.0));
        // CSS 9.7 steps 4.d/4.e: grow, then FREEZE any item whose share would put it below its
        // min-content size and hand the rest to the others. Clamping without redistributing made the
        // line wider than its container (measured: bases 50+50 with one min-content of 200 gave
        // 200 + 230 for a 300 dp line).
        let mut rounds = 0usize;
        loop {
            rounds += 1;
            let unfrozen: Vec<usize> = (0..items.len()).filter(|i| !items[*i].frozen).collect();
            let grow_total: f32 = unfrozen.iter().map(|i| items[*i].grow).sum();
            let frozen_targets: f32 = items
                .iter()
                .filter(|item| item.frozen)
                .map(|item| item.target)
                .sum();
            let base_total: f32 = unfrozen.iter().map(|i| items[*i].base).sum();
            let remaining = (container_main - total_gap - frozen_targets - base_total).max(0.0);
            // The sum-of-factors cap (4.b) only limits the space handed out BEFORE any item freezes:
            // once an item is pinned at its min-content size that size wins over the cap.
            let space = match cap_left {
                Some(cap) => remaining.min(cap),
                None => remaining,
            };
            if grow_total <= 0.0 || space <= 0.0 || rounds > items.len() + 1 {
                for &index in &unfrozen {
                    items[index].target = items[index].base;
                }
                break;
            }
            let mut froze = false;
            for &index in &unfrozen {
                let share = items[index].grow / grow_total;
                let candidate = items[index].base + space * share;
                if candidate < items[index].min_main {
                    items[index].target = items[index].min_main;
                    items[index].frozen = true;
                    froze = true;
                } else {
                    items[index].target = candidate;
                }
            }
            if !froze {
                break;
            }
            cap_left = None;
        }
    } else {
        // CSS 9.7 step 4.c: scale the shrink factors by the basis, then re-run while items hit their
        // min-content floor (:612-676).
        let mut needs_redistribution = true;
        let mut loops = 0usize;
        while needs_redistribution && loops < count {
            needs_redistribution = false;
            loops += 1;
            let mut frozen_targets = 0.0f32;
            let mut unfrozen_base = 0.0f32;
            let mut scaled_total = 0.0f32;
            for item in items.iter() {
                if item.frozen {
                    frozen_targets += item.target;
                } else {
                    unfrozen_base += item.base;
                    scaled_total += item.shrink * item.base;
                }
            }
            let sum_sizes = total_gap + frozen_targets + unfrozen_base;
            let mut free = container_main - sum_sizes;
            if sum_factors < 1.0 {
                let fractional = initial_free * sum_factors;
                if fractional.abs() < free.abs() {
                    free = fractional;
                }
            }
            let mut current_free = free.abs();
            let mut current_scaled = scaled_total;
            for item in items.iter_mut().filter(|item| !item.frozen) {
                let scaled = item.shrink * item.base;
                let share = if current_scaled > 0.0 {
                    scaled / current_scaled
                } else {
                    0.0
                };
                let take = current_free * share;
                current_free -= take;
                current_scaled -= scaled;
                let target = item.base - take;
                if target < item.min_main {
                    item.target = item.min_main;
                    item.frozen = true;
                    needs_redistribution = true;
                    // The frozen item no longer takes part: its factor leaves the pool, or the
                    // "less than one total factor" rule keeps counting it (:660).
                    sum_factors = (sum_factors - item.shrink).max(0.0);
                } else {
                    item.target = target;
                }
            }
        }
    }

    total_gap + items.iter().map(|item| item.target).sum::<f32>()
}

// ── The builder ──

/// A CSS-flex container — Compose's FlexBox (FlexBox.kt:133).
///
/// FlexItemConfig travels on each child's modifier:
///
///     FlexBox::new()
///         .config(FlexBoxConfig::new().wrap(FlexWrap::Wrap).gap(8.0))
///         .build(ctx, |ctx| {
///             Stack::new()
///                 .modifier(Modifier::new().flex(FlexItemConfig::new().grow(1.0)))
///                 .build(ctx, |_| {});
///         });
#[derive(Default)]
pub struct FlexBox {
    config: FlexBoxConfig,
    resolver: Option<Arc<dyn Fn(&FlexBoxConfigInput) -> FlexBoxConfig>>,
    modifier: Modifier,
}

impl FlexBox {
    pub fn new() -> Self {
        FlexBox {
            config: FlexBoxConfig::new(),
            resolver: None,
            modifier: Modifier::new(),
        }
    }

    /// A fixed configuration.
    pub fn config(mut self, config: FlexBoxConfig) -> Self {
        self.config = config;
        self.resolver = None;
        self
    }

    /// A configuration resolved at MEASURE time from the incoming constraints and the density —
    /// Compose's FlexBoxConfig lambda (FlexBox.kt:1836-2030).
    ///
    /// Read everything the resolver depends on INSIDE the closure (a `State::get()` there registers a
    /// LAYOUT dependency, so a write re-measures). A value captured from the enclosing composition is
    /// frozen once the container Skips: the Skip decision only compares the declared parameters, and
    /// a new closure is not one of them — Compose sidesteps this with `rememberUpdatedState`, which
    /// has no equivalent here.
    ///
    /// Calling config(..) after config_for(..) drops the resolver; the last call wins.
    pub fn config_for(
        mut self,
        resolver: impl Fn(&FlexBoxConfigInput) -> FlexBoxConfig + 'static,
    ) -> Self {
        self.resolver = Some(Arc::new(resolver));
        self
    }

    /// The container's own modifier; the caller's chain comes last.
    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        // Declare every parameter that decides the layout, so an unchanged container Skips
        // (docs/component-authoring.md 3.2).
        ctx.changed(&self.config);
        let responsive = self.resolver.is_some();
        ctx.changed(&responsive);
        let direction = self
            .modifier
            .get_layout_direction()
            .unwrap_or(crate::layout::direction::current());
        ctx.changed(&direction);
        build_container(
            ctx,
            self.modifier,
            FlexBoxLayout {
                config: self.config,
                resolver: self.resolver,
                direction,
            },
            content,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modifier::Modifier;
    use crate::unit::Size;

    fn leaf(width: f32, height: f32) -> LayoutNode {
        let mut node = LayoutNode::leaf(Modifier::new().size(width, height));
        node.measured_size = Size::new(width, height);
        node
    }

    fn item_leaf(width: f32, height: f32, config: FlexItemConfig) -> LayoutNode {
        let mut node = LayoutNode::leaf(Modifier::new().size(width, height).flex(config));
        node.measured_size = Size::new(width, height);
        node
    }

    fn run(
        config: FlexBoxConfig,
        nodes: &mut Vec<LayoutNode>,
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        let children: Vec<usize> = (0..nodes.len()).collect();
        let policy = FlexBoxLayout::new(config);
        policy.measure(nodes, &[], &children, constraints)
    }

    fn xs(placements: &[Placement]) -> Vec<f32> {
        placements.iter().map(|p| p.position.x).collect()
    }

    fn ys(placements: &[Placement]) -> Vec<f32> {
        placements.iter().map(|p| p.position.y).collect()
    }

    fn xs_of(placements: &[Placement], indices: &[usize]) -> Vec<f32> {
        indices.iter().map(|i| placements[*i].position.x).collect()
    }

    #[test]
    fn a_row_packs_its_items_at_the_start() {
        let mut nodes = vec![leaf(50.0, 20.0), leaf(50.0, 20.0), leaf(50.0, 20.0)];
        let (size, placements) = run(
            FlexBoxConfig::new(),
            &mut nodes,
            Constraints::new(0.0, 300.0, 0.0, 100.0),
        );
        assert_eq!(size, Size::new(150.0, 20.0), "a row is as wide as its content");
        assert_eq!(xs(&placements), vec![0.0, 50.0, 100.0]);
    }

    #[test]
    fn the_column_gap_separates_the_items() {
        let mut nodes = vec![leaf(50.0, 20.0), leaf(50.0, 20.0), leaf(50.0, 20.0)];
        let (size, placements) = run(
            FlexBoxConfig::new().column_gap(10.0),
            &mut nodes,
            Constraints::new(0.0, 300.0, 0.0, 100.0),
        );
        assert_eq!(size.width, 170.0, "two gaps are part of the line");
        assert_eq!(xs(&placements), vec![0.0, 60.0, 120.0]);
    }

    #[test]
    fn justify_content_distributes_the_leftover() {
        let constraints = Constraints::new(300.0, 300.0, 0.0, 100.0);
        let cases: Vec<(FlexJustifyContent, Vec<f32>)> = vec![
            (FlexJustifyContent::Start, vec![0.0, 50.0, 100.0]),
            (FlexJustifyContent::End, vec![150.0, 200.0, 250.0]),
            (FlexJustifyContent::Center, vec![75.0, 125.0, 175.0]),
            (FlexJustifyContent::SpaceBetween, vec![0.0, 125.0, 250.0]),
            (FlexJustifyContent::SpaceAround, vec![25.0, 125.0, 225.0]),
            (FlexJustifyContent::SpaceEvenly, vec![37.5, 125.0, 212.5]),
        ];
        for (justify, expected) in cases {
            let mut nodes = vec![leaf(50.0, 20.0), leaf(50.0, 20.0), leaf(50.0, 20.0)];
            let (_, placements) = run(
                FlexBoxConfig::new().justify_content(justify),
                &mut nodes,
                constraints,
            );
            assert_eq!(xs(&placements), expected, "justify {justify:?}");
        }
    }

    #[test]
    fn wrap_breaks_the_line_when_the_next_item_does_not_fit() {
        let mut nodes = vec![leaf(100.0, 20.0), leaf(100.0, 20.0), leaf(100.0, 20.0)];
        let (size, placements) = run(
            FlexBoxConfig::new().wrap(FlexWrap::Wrap),
            &mut nodes,
            Constraints::new(0.0, 250.0, 0.0, 200.0),
        );
        // Two 100 dp items fit 250, the third does not: lines of 2 and 1. The container is as wide as
        // the widest line, which is 200 — not the incoming maximum.
        assert_eq!(size, Size::new(200.0, 40.0));
        assert_eq!(xs(&placements), vec![0.0, 100.0, 0.0]);
        assert_eq!(ys(&placements), vec![0.0, 0.0, 20.0], "the third item starts line two");
    }

    #[test]
    fn max_items_in_each_line_breaks_a_line() {
        let mut nodes = vec![leaf(20.0, 20.0), leaf(20.0, 20.0), leaf(20.0, 20.0)];
        let (_, placements) = run(
            FlexBoxConfig::new()
                .wrap(FlexWrap::Wrap)
                .max_items_in_each_line(2),
            &mut nodes,
            Constraints::new(0.0, 300.0, 0.0, 200.0),
        );
        assert_eq!(ys(&placements), vec![0.0, 0.0, 20.0]);
    }

    #[test]
    fn grow_shares_the_free_space() {
        let mut nodes = vec![
            item_leaf(50.0, 20.0, FlexItemConfig::new().grow(1.0)),
            item_leaf(50.0, 20.0, FlexItemConfig::new().grow(1.0)),
        ];
        let (size, placements) = run(
            FlexBoxConfig::new(),
            &mut nodes,
            Constraints::new(300.0, 300.0, 0.0, 100.0),
        );
        assert_eq!(size.width, 300.0);
        assert_eq!(
            placements.iter().map(|p| p.size.width).collect::<Vec<_>>(),
            vec![150.0, 150.0]
        );
    }

    #[test]
    fn shrink_takes_space_proportionally() {
        // The basis is what shrinks: CSS fills the free space by scaling each shrink factor by the
        // item's basis (9.7 step 4.c). A leaf sized 200 dp cannot shrink at all — its MIN-CONTENT size
        // is 200 — so these items declare a larger basis than their content.
        let mut nodes = vec![
            item_leaf(50.0, 20.0, FlexItemConfig::new().basis_dp(200.0)),
            item_leaf(50.0, 20.0, FlexItemConfig::new().basis_dp(200.0)),
        ];
        let (size, placements) = run(
            FlexBoxConfig::new(),
            &mut nodes,
            Constraints::new(300.0, 300.0, 0.0, 100.0),
        );
        assert_eq!(size.width, 300.0);
        assert_eq!(
            placements.iter().map(|p| p.size.width).collect::<Vec<_>>(),
            vec![150.0, 150.0]
        );
    }

    #[test]
    fn a_percentage_basis_uses_the_container_main() {
        let mut nodes = vec![
            item_leaf(10.0, 20.0, FlexItemConfig::new().basis_percent(0.25)),
            item_leaf(10.0, 20.0, FlexItemConfig::new().basis_percent(0.25)),
        ];
        let (_, placements) = run(
            FlexBoxConfig::new(),
            &mut nodes,
            Constraints::new(400.0, 400.0, 0.0, 100.0),
        );
        assert_eq!(
            placements.iter().map(|p| p.size.width).collect::<Vec<_>>(),
            vec![100.0, 100.0]
        );
    }

    #[test]
    fn row_reverse_puts_the_first_item_at_the_end() {
        let mut nodes = vec![leaf(50.0, 20.0), leaf(50.0, 20.0)];
        let (_, placements) = run(
            FlexBoxConfig::new().direction(FlexDirection::RowReverse),
            &mut nodes,
            Constraints::new(100.0, 100.0, 0.0, 100.0),
        );
        assert_eq!(xs(&placements), vec![50.0, 0.0]);
    }

    #[test]
    fn rtl_mirrors_the_main_axis() {
        let mut nodes = vec![leaf(50.0, 20.0), leaf(50.0, 20.0)];
        let children: Vec<usize> = (0..nodes.len()).collect();
        let mut policy = FlexBoxLayout::new(FlexBoxConfig::new());
        policy.direction = crate::layout::LayoutDirection::Rtl;
        let (_, placements) = policy.measure(
            &mut nodes,
            &[],
            &children,
            Constraints::new(100.0, 100.0, 0.0, 100.0),
        );
        assert_eq!(xs(&placements), vec![50.0, 0.0]);
    }

    #[test]
    fn align_items_centre_puts_the_item_in_the_middle_of_the_line() {
        let mut nodes = vec![leaf(20.0, 20.0), leaf(20.0, 60.0)];
        let (size, placements) = run(
            FlexBoxConfig::new().align_items(FlexAlignItems::Center),
            &mut nodes,
            Constraints::new(0.0, 300.0, 0.0, 100.0),
        );
        assert_eq!(size.height, 60.0, "the line is as tall as its tallest item");
        assert_eq!(ys(&placements), vec![20.0, 0.0]);
    }

    #[test]
    fn align_items_stretch_fills_the_line_cross() {
        let mut nodes = vec![leaf(20.0, 20.0), leaf(20.0, 60.0)];
        let (_, placements) = run(
            FlexBoxConfig::new().align_items(FlexAlignItems::Stretch),
            &mut nodes,
            Constraints::new(0.0, 300.0, 0.0, 100.0),
        );
        assert_eq!(placements[0].size.height, 60.0, "stretch takes the line extent");
        assert_eq!(placements[1].size.height, 60.0);
    }

    #[test]
    fn align_self_overrides_align_items() {
        let mut nodes = vec![
            leaf(20.0, 60.0),
            item_leaf(
                20.0,
                20.0,
                FlexItemConfig::new().align_self(FlexAlignSelf::End),
            ),
        ];
        let (_, placements) = run(
            FlexBoxConfig::new().align_items(FlexAlignItems::Start),
            &mut nodes,
            Constraints::new(0.0, 300.0, 0.0, 100.0),
        );
        assert_eq!(ys(&placements), vec![0.0, 40.0], "the second item hugs the line's end");
    }

    #[test]
    fn align_content_space_between_distributes_the_lines() {
        let mut nodes = vec![
            leaf(100.0, 20.0),
            leaf(100.0, 20.0),
            leaf(100.0, 20.0),
        ];
        let (size, placements) = run(
            FlexBoxConfig::new()
                .wrap(FlexWrap::Wrap)
                .align_content(FlexAlignContent::SpaceBetween),
            &mut nodes,
            Constraints::new(0.0, 150.0, 120.0, 120.0),
        );
        assert_eq!(size.height, 120.0, "the cross minimum is taken");
        // Each 100 dp item takes a line of its own (150 dp cannot hold two), so there are three lines
        // of 20: 60 dp of free space over two SpaceBetween gaps puts them at 0, 50 and 100.
        assert_eq!(ys(&placements), vec![0.0, 50.0, 100.0]);
    }

    #[test]
    fn order_reorders_the_items () {
        let mut nodes = vec![
            item_leaf(20.0, 20.0, FlexItemConfig::new().order(2)),
            item_leaf(20.0, 20.0, FlexItemConfig::new().order(0)),
            item_leaf(20.0, 20.0, FlexItemConfig::new().order(1)),
        ];
        let (_, placements) = run(
            FlexBoxConfig::new(),
            &mut nodes,
            Constraints::new(0.0, 300.0, 0.0, 100.0),
        );
        // Items are placed in order 0, 1, 2 -> children 1, 2, 0.
        assert_eq!(xs_of(&placements, &[1, 2, 0]), vec![0.0, 20.0, 40.0]);
    }

    #[test]
    fn a_column_direction_swaps_the_axes() {
        let mut nodes = vec![leaf(50.0, 20.0), leaf(50.0, 20.0)];
        let (size, placements) = run(
            FlexBoxConfig::new().direction(FlexDirection::Column),
            &mut nodes,
            Constraints::new(0.0, 300.0, 0.0, 300.0),
        );
        assert_eq!(size, Size::new(50.0, 40.0));
        assert_eq!(ys(&placements), vec![0.0, 20.0]);
    }

    #[test]
    fn an_empty_flex_box_takes_the_minimum() {
        let mut nodes: Vec<LayoutNode> = Vec::new();
        let (size, placements) = run(
            FlexBoxConfig::new(),
            &mut nodes,
            Constraints::new(120.0, 400.0, 60.0, 400.0),
        );
        assert_eq!(size, Size::new(120.0, 60.0));
        assert!(placements.is_empty());
    }

    #[test]
    fn a_baseline_without_a_published_line_falls_back_to_the_item_height() {
        // A child that publishes no alignment line hangs entirely BELOW the baseline, so its baseline
        // is its own height — Compose's getBaseline fallback (:2124-2146). The two boxes then align
        // their BOTTOMS: line cross 40, the 20 dp box sits at 20.
        let mut nodes = vec![leaf(20.0, 20.0), leaf(20.0, 40.0)];
        let (_, placements) = run(
            FlexBoxConfig::new().align_items(FlexAlignItems::Baseline),
            &mut nodes,
            Constraints::new(0.0, 300.0, 0.0, 100.0),
        );
        assert_eq!(ys(&placements), vec![20.0, 0.0]);
    }

    /// A main-axis fill implies growth when the item declares no grow factor — Compose's
    /// mainAxisFillFraction fallback (FlexBox.kt:365-377).
    #[test]
    fn a_filling_child_grows_into_the_free_space() {
        let mut filling = LayoutNode::leaf(Modifier::new().fill_max_width());
        filling.measured_size = Size::new(0.0, 20.0);
        let mut nodes = vec![filling, leaf(50.0, 20.0)];
        let (size, placements) = run(
            FlexBoxConfig::new(),
            &mut nodes,
            Constraints::new(300.0, 300.0, 0.0, 100.0),
        );
        assert_eq!(size.width, 300.0);
        assert!(
            (placements[0].size.width - 250.0).abs() < 0.5,
            "the filling child takes the space the other one leaves: {:?}",
            placements[0].size
        );
    }

    /// An item cannot grow below its min-content size: CSS 9.7 freezes it there and hands the rest to
    /// the other items. Clamping without redistributing made the line WIDER than its container
    /// (measured: bases 50+50 with one min-content of 200 gave 200 + 230 for a 300 dp line).
    #[test]
    fn a_growing_item_cannot_grow_below_its_min_content() {
        let mut nodes = vec![
            item_leaf(200.0, 20.0, FlexItemConfig::new().basis_dp(50.0).grow(1.0)),
            item_leaf(50.0, 20.0, FlexItemConfig::new().basis_dp(50.0).grow(9.0)),
        ];
        let (size, placements) = run(
            FlexBoxConfig::new(),
            &mut nodes,
            Constraints::new(300.0, 300.0, 0.0, 100.0),
        );
        assert_eq!(size.width, 300.0);
        let widths: Vec<f32> = placements.iter().map(|p| p.size.width).collect();
        assert!(
            (widths[0] - 200.0).abs() < 0.5,
            "the 200 dp item stays at its min-content size: {widths:?}"
        );
        assert!(
            (widths[0] + widths[1] - 300.0).abs() < 0.5,
            "the other item takes exactly the rest, so the line is not wider than the container: {widths:?}"
        );
    }

    /// The cross gap is subtracted ONCE: Compose's totalLinesCrossSize is the sum of the line sizes
    /// and the gap only appears in totalGap (:779-780).
    #[test]
    fn align_content_does_not_subtract_the_cross_gap_twice() {
        let mut nodes = vec![leaf(100.0, 20.0), leaf(100.0, 20.0), leaf(100.0, 20.0)];
        let (size, placements) = run(
            FlexBoxConfig::new()
                .wrap(FlexWrap::Wrap)
                .align_content(FlexAlignContent::SpaceBetween)
                .row_gap(10.0),
            &mut nodes,
            Constraints::new(0.0, 150.0, 120.0, 120.0),
        );
        assert_eq!(size.height, 120.0);
        // free = 120 - 60 - 20 = 40 over two gaps -> 20 each: 0, 50, 100.
        assert_eq!(ys(&placements), vec![0.0, 50.0, 100.0]);
    }

    /// align-content: stretch grows the LINES before the stretched items are measured, so a stretched
    /// item fills the grown line (Compose runs applyAlignContentStretch before measureFlexItems).
    #[test]
    fn align_content_stretch_grows_the_stretched_items() {
        let mut nodes = vec![leaf(200.0, 100.0), leaf(200.0, 100.0)];
        let (size, placements) = run(
            FlexBoxConfig::new()
                .wrap(FlexWrap::Wrap)
                .align_items(FlexAlignItems::Stretch)
                .align_content(FlexAlignContent::Stretch),
            &mut nodes,
            Constraints::new(0.0, 150.0, 300.0, 300.0),
        );
        assert_eq!(size.height, 300.0, "the two lines share the cross minimum");
        let heights: Vec<f32> = placements.iter().map(|p| p.size.height).collect();
        assert!(
            heights.iter().all(|height| (height - 150.0).abs() < 0.5),
            "each stretched item fills its grown line: {heights:?}"
        );
    }

    /// The baseline path with REAL published lines: two texts of different sizes must sit on one
    /// first baseline (Compose's FlexAlignItems.Baseline, FlexBox.kt:916-968).
    #[test]
    fn a_baseline_line_lines_up_the_texts() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(crate::compose!(|ctx| {
            FlexBox::new()
                .config(FlexBoxConfig::new().align_items(FlexAlignItems::Baseline))
                .modifier(Modifier::new().fill_max_width())
                .build(ctx, |ctx| {
                    crate::components::Text::new("small")
                        .font_size(12.0)
                        .build(ctx);
                    crate::components::Text::new("large")
                        .font_size(32.0)
                        .build(ctx);
                });
        }));
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 200.0));
        let root = composer.layout_root().unwrap();
        let texts = root.children.clone();
        assert_eq!(texts.len(), 2, "two texts in the line");
        let nodes = composer.arena_nodes();
        let line = crate::layout::node::AlignmentLine::FIRST_BASELINE;
        let first = nodes[texts[0]].alignment_line(line).expect("small text has a baseline");
        let second = nodes[texts[1]]
            .alignment_line(line)
            .expect("large text has a baseline");
        let first_y = nodes[texts[0]].position.y + first;
        let second_y = nodes[texts[1]].position.y + second;
        assert!(
            (first_y - second_y).abs() < 0.5,
            "both texts sit on one baseline: {first_y} vs {second_y}"
        );
        assert!(
            nodes[texts[1]].position.y < nodes[texts[0]].position.y,
            "the larger text starts higher: {:?} {:?}",
            nodes[texts[0]].position,
            nodes[texts[1]].position
        );
    }}
