//! The FlexBox measure policy: CSS flexbox on the line model (FlexBox.kt:192-1010). The line
//! breaking, the flexible-length resolution, the cross-axis distribution and the placement all live
//! here; the vocabulary is in config.rs and the intrinsic walk in intrinsic.rs.

use super::config::{
    FlexAlignContent, FlexAlignItems, FlexAlignSelf, FlexBasis, FlexBoxConfig, FlexBoxConfigInput,
    FlexDirection, FlexJustifyContent, FlexWrap,
};
use super::intrinsic;
use crate::layout::node::intrinsic_size_of;
use crate::layout::node::{
    measure_node, IntrinsicCtx, IntrinsicQuery, LayoutNode, MeasurePolicy, Placement,
};
use crate::layout::Constraints;
use crate::unit::{Offset, Size};
use std::sync::Arc;
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
    /// The cross space left for this line when its items were measured — a stretched item is
    /// clamped to it (Compose's `remainingCrossAxisSize`, :984-1006).
    cross_limit: f32,
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
    /// Shared per-item placement/fade runtime. Only items carrying `animate_item` allocate state,
    /// so a flex container without markers pays one Arc clone.
    pub(crate) item_animations: crate::layout::item_animation::ItemAnimationHost,
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
            item_animations: crate::layout::item_animation::ItemAnimationHost::default(),
        }
    }

    /// The per-item animation runtime this container shares with the render pass (the marker is
    /// each child's own `Modifier::animate_item`).
    pub fn item_animations(
        mut self,
        host: crate::layout::item_animation::ItemAnimationHost,
    ) -> Self {
        self.item_animations = host;
        self
    }

    /// The layout direction a hand-built policy places with. FlexBox::build resolves it from the
    /// modifier chain (or the ambient direction) itself, so this is only for a policy used directly.
    pub fn direction(mut self, direction: crate::layout::LayoutDirection) -> Self {
        self.direction = direction;
        self
    }

    pub(super) fn resolve(&self, constraints: Constraints) -> FlexBoxConfig {
        match &self.resolver {
            Some(resolver) => resolver(&FlexBoxConfigInput {
                constraints,
                density: crate::runtime::density::current_density().density,
            }),
            None => self.config,
        }
    }

    /// The MAIN-axis intrinsic of the whole content (FlexBox.kt:1031-1082 for min, :1155-1185 for
    /// max). A wrapping min is the widest single item — every item could be alone on its line; a
    /// wrapping max is the widest line when a `maxItemsInEachLine` cap forces the breaks.
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
                    cross_limit,
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
            cross_limit,
        });

        // ── Phase 3a: measure every item at a tight main size (measureFlexItems, :714-770) ──
        //
        // Each line's items are clamped to the cross space LEFT for that line, as Compose does with
        // its running remainingCrossAxisSize (:723, :763-765, :984-1006).
        let mut remaining_cross = cross_limit;
        for line in &mut lines {
            let line_limit = remaining_cross.min(cross_limit);
            line.cross_limit = line_limit;
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
                    // Compose clamps the line's cross extent to the space left for THIS line, not to
                    // the container's maximum (:984-1006).
                    let cross = line.cross.min(line.cross_limit);
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
            // The distribution is computed in the CROSS-START frame (the same arithmetic for both wrap
            // modes) and then mirrored when the lines stack from the cross-end. Compose instead walks
            // the reversed lines with increasing positions, which for SpaceBetween/SpaceAround on a
            // wrap-reverse container walks the block off the cross extent (three 20 dp lines with
            // 164 dp of free space landed at 164 / 274 / 384 inside a 240 dp box); mirroring the
            // forward distribution is what CSS describes and what keeps the block inside.
            let cross_extent = cross_floor.max(total_cross);
            let mut position = match config.align_content {
                FlexAlignContent::End => free,
                FlexAlignContent::Center => free / 2.0,
                FlexAlignContent::SpaceAround => space_between / 2.0,
                _ => 0.0,
            };
            let mut starts = vec![0.0f32; lines.len()];
            for (index, line) in lines.iter().enumerate() {
                starts[index] = position;
                position += line.cross + space_between + cross_gap;
            }
            for (index, line) in lines.iter_mut().enumerate() {
                line.cross_start = if cross_reverse {
                    cross_extent - starts[index] - line.cross
                } else {
                    starts[index]
                };
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
    // ── Intrinsics (FlexBox.kt:1031-1230) ──
    //
    // Compose overrides all four instead of letting the default approximation measure the content:
    // a wrapping flex measured at the query constraints lays the items out for real and answers the
    // RESOLVED line sizes, which is not what "how narrow can this get" asks. The walk lives in
    // intrinsic.rs.
    fn min_intrinsic_width(&self, ctx: &mut IntrinsicCtx<'_>, children: &[usize], height: f32) -> f32 {
        intrinsic::width(self, ctx, children, height, true)
    }

    fn max_intrinsic_width(&self, ctx: &mut IntrinsicCtx<'_>, children: &[usize], height: f32) -> f32 {
        intrinsic::width(self, ctx, children, height, false)
    }

    fn min_intrinsic_height(&self, ctx: &mut IntrinsicCtx<'_>, children: &[usize], width: f32) -> f32 {
        intrinsic::height(self, ctx, children, width, true)
    }

    fn max_intrinsic_height(&self, ctx: &mut IntrinsicCtx<'_>, children: &[usize], width: f32) -> f32 {
        intrinsic::height(self, ctx, children, width, false)
    }

    fn place(&self, nodes: &mut Vec<LayoutNode>, children: &[usize], placements: &[Placement]) {
        self.item_animations.begin_frame();
        let mut seen = std::collections::HashSet::new();
        for (index, child) in children.iter().enumerate() {
            let Some(placement) = placements.get(index) else {
                continue;
            };
            nodes[*child].position = placement.position;
            nodes[*child].measured_size = placement.size;
            nodes[*child].item_paint = None;
            // The item's identity is its composition key, so a flex container needs no extra API to
            // animate: SlotKey is what a keyed loop (or a plain call site) already produced.
            let key = if nodes[*child].slot_key != 0 {
                nodes[*child].slot_key
            } else {
                nodes[*child].id
            };
            if !seen.insert(key) {
                debug_assert!(false, "FlexBox item key {key} is used by more than one child");
                continue;
            }
            let Some(spec) = nodes[*child].modifier.animate_item_spec() else {
                // The child is live but carries no marker: drop its channels, or finish_frame would
                // treat it as removed and draw a ghost over the live item.
                self.item_animations.retire(key);
                continue;
            };
            let position = nodes[*child].position;
            // Every child is composed each frame (there is no window), so a key unknown to the host
            // IS new to the data.
            self.item_animations
                .place(nodes, *child, key, position, &spec, true);
        }
        self.item_animations.finish_frame(None);
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
