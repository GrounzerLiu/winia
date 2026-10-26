//! Flex layout — the logic Column and Row share.
//!
//! Column (vertical main axis) and Row (horizontal main axis) measure with the same algorithm; they
//! differ only in how a main/cross coordinate maps onto x/y. The FlexAxis trait turns that difference
//! into a compile-time type parameter, which removes ~140 duplicated lines.

use super::constraints::Constraints;
use crate::unit::Size;
use super::node::*;

// ── FlexAxis trait ──

/// The main/cross-axis mapping. Every method is an associated function, so monomorphisation makes
/// it free at run time.
///
/// The questions both container families ask live in [`Axis`]; this adds what only the flex
/// algorithm needs.
pub(crate) trait FlexAxis: super::axis::Axis {
    /// Whether this axis is horizontal; used where a generic flow helper must choose the row vs
    /// column fill element without a second boolean parameter.
    const HORIZONTAL: bool;
    // ── Building constraints ──
    fn constrain_main(c: &Constraints, v: f32) -> f32;
    fn constrain_cross(c: &Constraints, v: f32) -> f32;

    /// Builds Constraints for a phase-1 child: the cross axis keeps the parent's full range, the main
    /// axis gets min = 0 and max = whatever is left.
    fn build_phase1(c: &Constraints, main_remaining: f32) -> Constraints;
    /// Builds Constraints for a phase-2 (weighted) child: the main axis is fixed to `allocated`, the
    /// cross axis keeps the parent's max with a min that Stretch decides.
    ///
    /// `fill_main` is Compose's `LayoutWeightParentData.fill`: the share is an exact main-axis size
    /// when true and only a MAXIMUM when false — `createConstraints(mainAxisMin = if
    /// (parentData.fill) childMainAxisSize else 0, mainAxisMax = childMainAxisSize,
    /// isPrioritizing = true)` (`RowColumnMeasurePolicy.kt:195-207`).
    ///
    /// Two things in that call are deliberately not modelled, both checked against the source:
    ///
    /// - `isPrioritizing = true` selects `Constraints.fitPrioritizingWidth/Height` instead of a plain
    ///   `Constraints(...)`, and that function is about BIT PACKING, not layout: Compose's
    ///   `Constraints` is a value class over a `Long` with 18 bits for its larger dimension and 13 for
    ///   the smaller (262,143 and 8,191), so the main axis is granted the large budget and the cross
    ///   axis is clamped to what is left ("The width is granted as much space as it needs or caps the
    ///   size to 18 bits. The height is given the remaining space" — `ui-unit` `Constraints.kt:275-313`).
    ///   winia's `Constraints` is four `f32`s with no packing, so there is no budget to prioritise and
    ///   the flag has nothing to mean. It is not a semantics difference to align.
    /// - Compose quantises a weighted child's share to integer pixels and hands the rounding error to
    ///   the earliest children — `weightUnitSpace = remainingToTarget / totalWeight`, then
    ///   `childMainAxisSize = max(0, (weightUnitSpace * weight).fastRoundToInt() + remainderUnit)` with
    ///   `remainderUnit = remainder.sign` decremented per child (`:169-193`). winia allocates
    ///   `remaining * weight / total_weight` as an `f32` and never rounds, so its children fill the
    ///   axis exactly in logical pixels where Compose fills it exactly in device pixels. Matching the
    ///   rounding would mean moving the whole lay-out to integers, against winia's float model.
    fn build_phase2(c: &Constraints, allocated: f32, fill_main: bool, stretch_cross: bool) -> Constraints;

    /// A child measured to fill a flow line's cross extent: fix the cross axis to `cross`, leave
    /// the main axis loose within the parent's budget.
    fn with_fixed_cross(c: &Constraints, cross: f32) -> Constraints;

    /// A weighted child's share with the cross axis fixed to the fill fraction.
    fn build_phase2_with_cross(
        allocated: f32,
        fill_main: bool,
        cross: f32,
    ) -> Constraints;

    // ── Building sizes ──
    // `size` / `point` are on `Axis`.

    // ── RTL ──
    /// The container width used when mirroring for RTL: a horizontal main axis uses the row's own
    /// measured width, a vertical one uses the cross-axis width.
    fn rtl_container_width(measured_main: f32, cross_size: f32) -> f32;
}

// ── Vertical main axis (Column) ──

impl FlexAxis for super::axis::VerticalAxis {
    const HORIZONTAL: bool = false;
    #[inline] fn constrain_main(c: &Constraints, v: f32) -> f32 { c.constrain_height(v) }
    #[inline] fn constrain_cross(c: &Constraints, v: f32) -> f32 { c.constrain_width(v) }

    #[inline]
    fn build_phase1(c: &Constraints, main_remaining: f32) -> Constraints {
        Constraints {
            // Loosen the cross-axis minimum, as Compose's Column does: a child must not inherit the
            // parent's tight width, or a fill_max_size parent would force every child to fill it.
            min_width: 0.0,
            max_width: c.max_width,
            min_height: 0.0,
            max_height: main_remaining.max(0.0),
        }
    }

    #[inline]
    fn build_phase2(c: &Constraints, allocated: f32, fill_main: bool, stretch_cross: bool) -> Constraints {
        Constraints {
            min_width: if stretch_cross { c.min_width } else { 0.0 },
            max_width: c.max_width,
            min_height: if fill_main { allocated } else { 0.0 },
            max_height: allocated,
        }
    }

    #[inline]
    fn with_fixed_cross(c: &Constraints, cross: f32) -> Constraints {
        Constraints {
            min_width: cross,
            max_width: cross,
            min_height: 0.0,
            max_height: c.max_height,
        }
    }

    #[inline]
    fn build_phase2_with_cross(
        allocated: f32,
        fill_main: bool,
        cross: f32,
    ) -> Constraints {
        Constraints {
            min_width: cross,
            max_width: cross,
            min_height: if fill_main { allocated } else { 0.0 },
            max_height: allocated,
        }
    }

    #[inline]
    fn rtl_container_width(_measured_main: f32, cross_size: f32) -> f32 {
        cross_size
    }
}

// ── Horizontal main axis (Row) ──

impl FlexAxis for super::axis::HorizontalAxis {
    const HORIZONTAL: bool = true;
    #[inline] fn constrain_main(c: &Constraints, v: f32) -> f32 { c.constrain_width(v) }
    #[inline] fn constrain_cross(c: &Constraints, v: f32) -> f32 { c.constrain_height(v) }

    #[inline]
    fn build_phase1(c: &Constraints, main_remaining: f32) -> Constraints {
        Constraints {
            min_width: 0.0,
            max_width: main_remaining.max(0.0),
            // Loosen the cross-axis minimum, as Compose's Row does: a child must not inherit the
            // parent's tight height.
            min_height: 0.0,
            max_height: c.max_height,
        }
    }

    #[inline]
    fn build_phase2(c: &Constraints, allocated: f32, fill_main: bool, stretch_cross: bool) -> Constraints {
        Constraints {
            min_width: if fill_main { allocated } else { 0.0 },
            max_width: allocated,
            min_height: if stretch_cross { c.min_height } else { 0.0 },
            max_height: c.max_height,
        }
    }

    #[inline]
    fn with_fixed_cross(c: &Constraints, cross: f32) -> Constraints {
        Constraints {
            min_width: 0.0,
            max_width: c.max_width,
            min_height: cross,
            max_height: cross,
        }
    }

    #[inline]
    fn build_phase2_with_cross(
        allocated: f32,
        fill_main: bool,
        cross: f32,
    ) -> Constraints {
        Constraints {
            min_width: if fill_main { allocated } else { 0.0 },
            max_width: allocated,
            min_height: cross,
            max_height: cross,
        }
    }

    #[inline]
    fn rtl_container_width(measured_main: f32, _cross_size: f32) -> f32 {
        measured_main
    }
}

// ── The parameterised measure ──

/// The parameterised flex measure. Column's and Row's `MeasurePolicy::measure` delegate straight here.
pub(crate) fn measure_flex<A: FlexAxis>(
    arrangement: Arrangement,
    alignment: Alignment,
    spacing: f32,
    direction: LayoutDirection,
    nodes: &mut Vec<LayoutNode>,
    policies: &[Box<dyn MeasurePolicy>],
    children: &[usize],
    constraints: &Constraints,
) -> (Size, Vec<Placement>) {
    let n = children.len();
    if n == 0 {
        return (
            Size::new(
                constraints.constrain_width(0.0),
                constraints.constrain_height(0.0),
            ),
            Vec::new(),
        );
    }

    // ── Per-child attributes ──
    let weights: Vec<Option<f32>> = children
        .iter()
        .map(|&c| {
            nodes[c]
                .modifier
                .get_layout_weight()
                // A non-positive weight is NOT a weighted child. Compose rejects `weight <= 0` at
                // the modifier (`Row.kt:383-384`, `Column.kt:366-367`) and its measure pass only
                // treats `weight > 0f` as weighted (`RowColumnMeasurePolicy.kt:113-116`). Treating
                // `Some(0.0)` as weighted measured the child with a fixed zero slot, so it vanished
                // instead of measuring like any other child.
                .filter(|w| *w > 0.0)
                // Compose coerces `+inf` to `Float.MAX_VALUE` (`Row.kt:388`, after the guard at
                // `:384`). An infinite weight would make the share `remaining * inf / inf` — NaN —
                // and hand the child NaN constraints, which `f32::clamp` panics on.
                .map(|w| w.min(f32::MAX))
        })
        .collect();
    // Compose's `LayoutWeightParentData.fill`. A node with a weight but no explicit flag fills,
    // which is what `Modifier::layout_weight` has always meant here.
    let fills: Vec<bool> = children.iter()
        .map(|&c| nodes[c].modifier.get_layout_weight_fill().unwrap_or(true))
        .collect();
    let aligns: Vec<Alignment> = children.iter()
        .map(|&c| nodes[c].modifier.get_align_self().unwrap_or(alignment))
        .collect();

    // ── Phase 1: the children without a weight ──
    let mut child_sizes: Vec<Size> = vec![Size::ZERO; n];
    // Main-axis size a WEIGHTED child is placed at. The parent allocates that share
    // (Compose measures a weighted child with fixed main constraints and keeps the
    // slot), so it must NOT come from the child's measured/returned size: a flight
    // layout override replaces that return with the placeholder size the parent was
    // told, which would otherwise let a weighted hero shrink to its natural width
    // mid-flight while a trailing sibling slides over it.
    let mut allocated_main: Vec<Option<f32>> = vec![None; n];
    let mut total_fixed_main: f32 = 0.0;
    // The gaps charged to the non-weighted children so far, each capped at what the axis had left
    // when it was charged — Compose's `fixedSpace` accumulation. `last_gap` is Compose's
    // `spaceAfterLastNoWeight`: when there is no weighted child to follow it, the trailing gap is
    // given back (`RowColumnMeasurePolicy.kt:152-155`), or a two-child container would carry one
    // gap too many.
    let mut charged_spacing: f32 = 0.0;
    let mut last_gap: f32 = 0.0;
    let mut max_cross: f32 = 0.0;
    let mut total_weight: f32 = 0.0;

    for (i, &c) in children.iter().enumerate() {
        if let Some(w) = weights[i] {
            total_weight += w;
            continue;
        }
        // Every preceding NON-WEIGHTED child charges its gap, whatever it measured — Compose adds
        // `spaceAfterLastNoWeight` after each of them (`RowColumnMeasurePolicy.kt:143-145`), an empty
        // one included: `min(arrangementSpacing, (remaining - placeable).coerceAtLeast(0))`. The
        // `min` matters on a full axis: a 60 dp gap after a 50 dp child in a 100 dp column is
        // charged 50, not 60, which is what keeps the container at 100 instead of inflating it to
        // 110 once the size is no longer clamped to the maximum.
        //
        // Weighted children cannot be counted here — they have not been measured yet — and their
        // spacing is charged to the weighted allocation instead. Compose does the same: its
        // `fixedSpace` only ever sees non-weighted children, and the weighted pool subtracts
        // `arrangementSpacingTotal` for them separately (`:163-165`).
        let main_remaining = A::main_max(constraints) - total_fixed_main - charged_spacing;

        let cc = A::build_phase1(constraints, main_remaining);
        let (size, _) = measure_node(nodes, policies, c, cc);
        let size_main = A::main_size(size);
        total_fixed_main += size_main;
        max_cross = max_cross.max(A::cross_size(size));
        child_sizes[i] = size;
        last_gap = spacing.min((main_remaining - size_main).max(0.0));
        charged_spacing += last_gap;
    }

    // What is left over
    //
    // A main axis is unbounded when it carries EITHER sentinel: `f32::INFINITY`
    // (`Constraints::UNBOUNDED`) or `f32::MAX` (what a scroll container writes for its content —
    // `layout/node/measure/engine.rs:560` for the vertical axis, `:582` for the horizontal one).
    // `is_finite()` is true for `f32::MAX`, so the old test called a scrolling parent bounded and
    // handed the weighted children `f32::MAX` as their share: measured, a weighted child in a
    // `vertical_scroll` Column came out 3.4e38 tall and dragged the column with it.
    //
    // The pool is Compose's (`RowColumnMeasurePolicy.kt:157-165`):
    //   targetSpace = if (mainAxisMax != Infinity) mainAxisMax else mainAxisMin
    //   remainingToTarget = (targetSpace - fixedSpace - arrangementSpacingTotal).coerceAtLeast(0)
    // `mainAxisMin` is not always 0 — a `heightIn(min = 200)` column inside a scroller has min 200
    // and max `f32::MAX`, and Compose gives the weighted children that 200 while a hard-coded 0
    // would give them nothing.
    let bounded_main = A::main_max(constraints) < f32::MAX;
    let target_main = if bounded_main {
        A::main_max(constraints)
    } else {
        A::main_min(constraints)
    };
    // Gaps among the weighted children; the gaps next to a non-weighted one are already in
    // `charged_spacing` (`RowColumnMeasurePolicy.kt:163`).
    let weighted_count = weights.iter().filter(|w| w.is_some()).count();
    if weighted_count == 0 {
        charged_spacing -= last_gap;
    }
    let arrangement_spacing_total = spacing * weighted_count.saturating_sub(1) as f32;
    let fixed_space = total_fixed_main + charged_spacing;
    let remaining = (target_main - fixed_space - arrangement_spacing_total).max(0.0);

    // ── Phase 2: the weighted children ──
    for (i, &c) in children.iter().enumerate() {
        if let Some(w) = weights[i] {
            // `remaining * (w / total_weight)`, not `remaining * w / total_weight`: two finite
            // weights near `f32::MAX` can overflow the product to infinity.
            let allocated = if total_weight > 0.0 { remaining * (w / total_weight) } else { 0.0 };
            // The allocation only wins when the parent actually HAD space to allocate, i.e. a
            // positive share. A zero share means the axis gave the weighted children nothing — an
            // unbounded axis with `min == 0`, or a minimum already consumed by fixed children — and
            // then the child keeps whatever it measured (0, unless it ignores its constraints),
            // which is also what Compose does with `mainAxisMin == 0`.
            //
            // `fill = false` also falls back to the child's own measurement, and that is the whole
            // point of the flag: Compose measures such a child with its share as the MAXIMUM and
            // then adds the size the child asked for to `weightedSpace`, so a short child leaves the
            // container short instead of stretching it to the share.
            allocated_main[i] = if fills[i] && total_weight > 0.0 && allocated > 0.0 {
                Some(allocated)
            } else {
                None
            };
            let stretch_cross = alignment == Alignment::Stretch || aligns[i] == Alignment::Stretch;
            let cc = A::build_phase2(constraints, allocated, fills[i], stretch_cross);
            let (size, _) = measure_node(nodes, policies, c, cc);
            max_cross = max_cross.max(A::cross_size(size));
            child_sizes[i] = size;
        }
    }

    // ── Phase 3: the size and the layout ──
    //
    // Compose's `fixedSpace + weightedSpace`:
    //   fixedSpace    = the non-weighted children plus their capped gaps (`charged_spacing`)
    //   weightedSpace = the weighted children's measured sizes plus the gaps among them, clamped to
    //                   what the axis has left (`RowColumnMeasurePolicy.kt:217-220`)
    // Summing `spacing * (n - 1)` instead (what this did) inflates an over-full container: 50 + 40
    // with a 60 dp gap in a 100 dp column reported 110, where Compose caps the gap at 50 and
    // reports 100. The upper clamp was hiding it until the size stopped being coerced.
    let weighted_space = {
        let measured: f32 = allocated_main
            .iter()
            .enumerate()
            .filter(|(i, _)| weights[*i].is_some())
            .map(|(i, alloc)| alloc.unwrap_or_else(|| A::main_size(child_sizes[i])))
            .sum();
        let cap = if bounded_main {
            (A::main_max(constraints) - fixed_space).max(0.0)
        } else {
            f32::MAX
        };
        (measured + arrangement_spacing_total).clamp(0.0, cap)
    };
    let total_content_main = fixed_space + weighted_space;

    // The arrangement spreads the space the container ACTUALLY ends up with, so the container's main size
    // is resolved first and the leftover is derived from it. Two things make that subtle:
    //
    //  - The unbounded sentinels are `f32::INFINITY` and `f32::MAX` (a scroll container writes the
    //    latter); both are spelled out where `remaining` is decided. Getting that wrong once put
    //    `Arrangement::Center`'s child at 1.7e38 — measured on a menu item's label
    //    (`pos:[12,170141173319319264429905852091742258462720]`).
    //  - A container that carries a MINIMUM (a 48dp menu item whose label is 20px) really does have space
    //    to distribute, which only shows up once the minimum is used to floor the content size.
    //
    // The size is the CONTENT size, floored by the incoming minimum — never the maximum the parent offers.
    // Compose resolves it the same way: `mainAxisLayoutSize = max((fixedSpace + weightedSpace)
    // .fastCoerceAtLeast(0), mainAxisMin)` (`RowColumnMeasurePolicy.kt:252`), where `fixedSpace` only
    // accumulates the children measured and `mainAxisMax` is never consulted. A spreading arrangement on a
    // container with no explicit size therefore hugs its content and has no leftover to spread, which is
    // why `Row(horizontalArrangement = SpaceBetween)` needs `fillMaxWidth()` to push children apart.
    //
    // winia used to grow such a container to the maximum the parent offered
    // (`total_content_main + (main_max - total_content_main).max(0.0)`) — a deliberate convenience, "how
    // the demos spread a bar across their container". It was not worth the divergence: it made
    // `SpaceBetween` behave unlike Compose everywhere, and it silently defeated the date picker dialog's
    // `weight(1f, fill = false)` collapse by holding the dialog at its 568 dp cap. Removing it cost
    // nothing — every call site that wanted the spreading had already said so with an explicit
    // `fill_max_width` (`layout_demo`'s rows through their shared `arr_pad`, the date picker's nine
    // header/weekday/grid rows, `swipe_to_dismiss_demo`'s label row), which is the Compose idiom anyway.
    let measured_main = total_content_main.max(A::main_min(constraints));
    let remaining_main = (measured_main - total_content_main).max(0.0);
    let gap_count = if n > 1 { n - 1 } else { 0 };
    let (spacing_extra, leading_space) = compute_spacing(arrangement, remaining_main, gap_count);
    let effective_spacing = spacing + spacing_extra;

    // ── The final cross-axis size ──
    //
    // Compose's alignment-line pass (`RowColumnMeasurePolicy.kt:228-251`) runs before the size is
    // taken: a child aligned by one of its lines contributes that line's distance from its own top
    // (`beforeCrossAxisAlignmentLine`) and everything below it (`afterCrossAxisAlignmentLine`), and
    // the cross axis must be at least the two added together so a line placed at `before` still has
    // room for the tallest child under it. `line_before` is where every line-aligned child's line
    // lands; children without a line ignore it.
    let mut line_before = 0.0f32;
    let mut line_after = 0.0f32;
    for (i, &c) in children.iter().enumerate() {
        let Some(line) = nodes[c].modifier.get_align_by() else { continue };
        let child_cross = A::cross_size(child_sizes[i]);
        let position = nodes[c].alignment_line(line);
        line_before = line_before.max(position.unwrap_or(0.0));
        // An unspecified line is treated as the whole child hanging below the line, as Compose does.
        line_after = line_after.max(child_cross - position.unwrap_or(child_cross));
    }

    // `Alignment::Stretch` is winia's own cross-axis value (Compose spells it `fillMaxSize`), so
    // it has no Compose rule of its own — but it still has to respect the same floor and the same
    // alignment lines, or it becomes the one arm where an overflowing child is reported smaller
    // than it is.
    let cross_size = if alignment == Alignment::Stretch && A::cross_max(constraints) < f32::MAX {
        A::cross_max(constraints)
            .max(max_cross)
            .max(line_before + line_after)
            .max(A::cross_min(constraints))
    } else {
        // Compose: `maxOf(crossAxisSpace, crossAxisMin, beforeCrossAxisAlignmentLine +
        // afterCrossAxisAlignmentLine)` — the cross axis is not coerced into `crossAxisMax` either
        // (`RowColumnMeasurePolicy.kt:253-258`).
        max_cross
            .max(line_before + line_after)
            .max(A::cross_min(constraints))
    };

    // Place the children
    let mut placements = Vec::with_capacity(n);
    let mut main_offset = leading_space;

    for (i, child_size) in child_sizes.iter().enumerate() {
        let align = aligns[i];
        let line = nodes[children[i]].modifier.get_align_by();
        // A line-aligned child is placed by its line, not by an edge, so it keeps its own cross size
        // even under a stretching parent — Compose's `getCrossAxisPosition` asks the line first and
        // only falls back to the parent's alignment (`Row.kt:216-231`).
        let child_cross = if align == Alignment::Stretch && line.is_none() {
            cross_size
        } else {
            A::cross_size(*child_size)
        };
        // A weighted child keeps its allocated slot (see `allocated_main`).
        let child_main = allocated_main[i].unwrap_or_else(|| A::main_size(*child_size));
        let cross_offset = match line {
            // `beforeCrossAxisAlignmentLine - alignmentLinePosition`: every line-aligned child's
            // line lands on the same cross-axis position.
            Some(line) => {
                line_before - nodes[children[i]].alignment_line(line).unwrap_or(0.0)
            }
            None => match align {
                Alignment::Start => 0.0,
                Alignment::End => cross_size - child_cross,
                Alignment::Center => (cross_size - child_cross) / 2.0,
                Alignment::Stretch => 0.0,
            },
        };
        placements.push(Placement {
            size: A::size(child_main, child_cross),
            position: A::point(main_offset, cross_offset),
        });
        main_offset += child_main + effective_spacing;
    }

    // ── Mirroring for RTL ──
    if direction == LayoutDirection::Rtl {
        let container_x = A::rtl_container_width(measured_main, cross_size);
        for p in &mut placements {
            p.position.x = container_x - p.position.x - p.size.width;
        }
    }

    (A::size(measured_main, cross_size), placements)
}

// ── Intrinsic sizes (Compose's IntrinsicMeasureBlocks) ──
//
// Row/Column is the only Compose container that overrides all four intrinsic queries instead of
// falling back to the MeasurePolicy default: `weight` needs the same arithmetic as the measure phases
// (RowColumnImpl.kt:371-452). The two functions below are that port; row.rs and column.rs pass the
// query surface in, following IntrinsicMeasureBlocks' table.
// （RowColumnImpl.kt:261-369）。

/// Compose's `intrinsicMainAxisSize` (RowColumnImpl.kt:371-394).
///
/// A child without a weight contributes its own main-axis intrinsic size; a weighted child HAS none —
/// the parent hands it a share — so the container prices the weighted set instead: the largest weight
/// unit (that child's main size divided by its weight) times the total weight. This is why a
/// `weight(1f)` label inside a Column still reports a finite intrinsic height: without the arithmetic
/// the label echoes the constraints back and the container reports the maximum rather than the content
/// width (the two-pass dropdown measurement walked into exactly that; see docs/dropdown-menu.md 4.2).
pub(crate) fn flex_intrinsic_main(
    ctx: &mut IntrinsicCtx<'_>,
    children: &[usize],
    main_query: IntrinsicQuery,
    cross_axis_available: f32,
    spacing: f32,
) -> f32 {
    if children.is_empty() {
        return 0.0;
    }
    let mut fixed_space = 0.0f32;
    let mut weight_unit_space = 0.0f32;
    let mut total_weight = 0.0f32;
    for &c in children {
        let weight = ctx.child_weight(c);
        let size = ctx.child_intrinsic(c, main_query, cross_axis_available);
        if weight > 0.0 {
            total_weight += weight;
            weight_unit_space = weight_unit_space.max(size / weight);
        } else {
            fixed_space += size;
        }
    }
    weight_unit_space * total_weight + fixed_space + spacing * (children.len() as f32 - 1.0).max(0.0)
}

/// Compose's `intrinsicCrossAxisSize` (RowColumnImpl.kt:396-452).
///
/// The cross-axis answer needs each child's MAIN-axis space first: a child's cross size is priced by
/// the main space it will occupy. A child without a weight takes the smaller of its unbounded
/// main-axis intrinsic size and the container's remaining space; a weighted child takes one weight
/// unit each. `main_query` here is the main axis's MAX query — every Compose cross-axis block pairs
/// with `maxIntrinsic*` (RowColumnImpl.kt:281-355), because a child given more space can also grow on
/// the cross axis.
pub(crate) fn flex_intrinsic_cross(
    ctx: &mut IntrinsicCtx<'_>,
    children: &[usize],
    main_query: IntrinsicQuery,
    cross_query: IntrinsicQuery,
    main_axis_available: f32,
    spacing: f32,
) -> f32 {
    if children.is_empty() {
        return 0.0;
    }
    let unbounded = main_axis_available >= f32::MAX;
    let spacing_total = spacing * (children.len() as f32 - 1.0).max(0.0);
    // Compose: `fixedSpace = min((n - 1) * mainAxisSpacing, mainAxisAvailable)`。
    let mut fixed_space = if unbounded {
        spacing_total
    } else {
        spacing_total.min(main_axis_available)
    };
    let mut cross_axis_max = 0.0f32;
    let mut total_weight = 0.0f32;

    for &c in children {
        let weight = ctx.child_weight(c);
        if weight > 0.0 {
            total_weight += weight;
            continue;
        }
        // Ask the child how much main-axis space it wants, never more than what is left.
        let remaining = if unbounded {
            f32::MAX
        } else {
            (main_axis_available - fixed_space).max(0.0)
        };
        let main_axis_space = ctx.child_intrinsic(c, main_query, f32::MAX).min(remaining);
        fixed_space += main_axis_space;
        cross_axis_max = cross_axis_max.max(ctx.child_intrinsic(c, cross_query, main_axis_space));
    }

    // How much main-axis space one weight unit is worth (unbounded: Compose uses Infinity, so the
    // cross axis is asked as unbounded).
    let weight_unit_space = if total_weight == 0.0 {
        0.0
    } else if unbounded {
        f32::MAX
    } else {
        (main_axis_available - fixed_space).max(0.0) / total_weight
    };
    for &c in children {
        let weight = ctx.child_weight(c);
        if weight > 0.0 {
            let main_space = if weight_unit_space >= f32::MAX {
                f32::MAX
            } else {
                weight_unit_space * weight
            };
            cross_axis_max = cross_axis_max.max(ctx.child_intrinsic(c, cross_query, main_space));
        }
    }
    cross_axis_max
}
