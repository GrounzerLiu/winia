//! The flow measurement algorithm shared by FlowRow and FlowColumn.

use crate::layout::flex::FlexAxis;
use super::FlowOverflow;
use crate::layout::node::{compute_spacing, measure_node};
use crate::layout::{
    Alignment, Arrangement, Constraints, LayoutDirection, LayoutNode, MeasurePolicy, Placement,
};
use crate::unit::{Offset, Size};

/// Parameterised flow measurement; FlowRow (horizontal main axis) and FlowColumn (vertical) both
/// delegate here.
pub(crate) fn measure_flow<A: FlexAxis>(
    arrangement: Arrangement,
    alignment: Alignment,
    cross_arrangement: Arrangement,
    main_spacing: f32,
    cross_spacing: f32,
    max_items_in_main_axis: usize,
    max_lines: usize,
    overflow: FlowOverflow,
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

    if max_lines == 0 || max_items_in_main_axis == 0 {
        return (
            Size::ZERO,
            vec![Placement { size: Size::ZERO, position: Offset::new(0.0, 0.0) }; n],
        );
    }

    let max_main = A::main_max(constraints);
    // An unbounded main axis never runs out of room, so it degenerates to a single Row/Column
    // line (as Compose does). max_items is orthogonal to available space, though, so it still
    // forces a wrap when unbounded (review P1: can_wrap must not disable the count branch).
    let can_wrap = max_main < f32::MAX;

    // Measure each item with the parent's main-axis budget (Compose does the same: its
    // `measureConstraints` keeps `mainAxisMax` and only resets `mainAxisMin` to 0). Measuring
    // with an infinite main axis would let a wrapping child report its unwrapped width and then
    // paint outside the flow container.
    let main_limit = A::main_max(constraints).min(f32::MAX);
    let mut child_sizes: Vec<Size> = vec![Size::ZERO; n];
    for (i, &c) in children.iter().enumerate() {
        let cc = A::build_phase1(constraints, main_limit);
        let (size, _) = measure_node(nodes, policies, c, cc);
        child_sizes[i] = size;
    }

    // Wrapping (Compose getWrapInfo.shouldWrapItem): never at the start of a line, always past
    // max_items, and when the remaining room cannot fit the item. lines holds (start, end) per line.
    let mut lines: Vec<(usize, usize)> = Vec::new();
    let mut line_start = 0usize;
    let mut line_main = 0.0f32; // main-axis space used by the current line, spacing included
    // max_items == 0 is the empty flow above; every remaining case has a positive cap.
    let max_items = max_items_in_main_axis.max(1);
    for i in 0..n {
        let item_main = A::main_size(child_sizes[i]);
        let index_in_line = i - line_start;
        let need = if index_in_line == 0 { item_main } else { main_spacing + item_main };
        // P1 fix: the count branch is independent of can_wrap (max_items is orthogonal to room);
        // the room branch still needs can_wrap, since unbounded means infinite room.
        let should_wrap = index_in_line >= max_items
            || (can_wrap && index_in_line > 0 && line_main + need > max_main);
        if should_wrap {
            lines.push((line_start, i));
            line_start = i;
            line_main = item_main;
        } else {
            line_main += need;
        }
    }
    lines.push((line_start, n));
    // Compose's Visible overflow disables the maxLines check (FlowLayoutBuildingBlocks.kt:84-90);
    // Clip and the indicator modes do cap the line count.
    if overflow != FlowOverflow::Visible && max_lines < lines.len() {
        lines.truncate(max_lines);
    }

    // Compose's `FlowRowScope` inherits `RowScope.weight`: a weighted item shares the line's free
    // main-axis space instead of taking its natural size. The initial measurement above is the
    // intrinsic-ish size used for wrapping; after the lines are known, re-measure each weighted
    // child with its share. The line target is `max(natural, constraints.min)`, which is the same
    // line layout size used by arrangement below: a wrap-content flow has no free space, a fill
    // flow fills its minimum (usually its fixed width).
    let weights: Vec<Option<f32>> = children
        .iter()
        .map(|&c| nodes[c].modifier.get_layout_weight().filter(|w| *w > 0.0))
        .collect();
    let fills: Vec<bool> = children
        .iter()
        .map(|&c| nodes[c].modifier.get_layout_weight_fill().unwrap_or(true))
        .collect();
    let main_max = A::main_max(constraints);
    let bounded_main = main_max < f32::MAX;
    for &(s, e) in &lines {
        let mut total_weight = 0.0f32;
        let mut natural_main = 0.0f32;
        let mut fixed_main = 0.0f32;
        let mut line_cross_natural = 0.0f32;
        for i in s..e {
            if i > s {
                natural_main += main_spacing;
            }
            natural_main += A::main_size(child_sizes[i]);
            line_cross_natural = line_cross_natural.max(A::cross_size(child_sizes[i]));
            match weights[i] {
                Some(w) => total_weight += w,
                None => fixed_main += A::main_size(child_sizes[i]),
            }
        }
        if total_weight <= 0.0 {
            continue;
        }
        let spacing_total = main_spacing * (e - s - 1) as f32;
        // Compose uses the bounded main-axis MAXIMUM as the weighting target
        // (RowColumnMeasurePolicy.kt:157-165): a wrap-content flow with a finite parent still
        // gives its weighted child the free width. An unbounded axis keeps the natural size (the
        // friendlier side of Compose's mainAxisMin fallback).
        let target_main = if bounded_main { main_max } else { natural_main };
        let available = (target_main - fixed_main - spacing_total).max(0.0);
        for i in s..e {
            let Some(w) = weights[i] else { continue };
            let allocated = available * w / total_weight;
            let child = children[i];
            let fill_fraction = if A::HORIZONTAL {
                nodes[child].modifier.get_fill_max_row_height()
            } else {
                nodes[child].modifier.get_fill_max_column_width()
            };
            let cc = if let Some(fraction) = fill_fraction {
                A::build_phase2_with_cross(
                    allocated,
                    fills[i],
                    line_cross_natural * fraction,
                )
            } else {
                A::build_phase2(constraints, allocated, fills[i], false)
            };
            let (size, _) = measure_node(nodes, policies, child, cc);
            child_sizes[i] = size;
        }
    }

    // Line sizes. The main axis is the raw content width, NOT clamped: under fill, clamping would
    // lift the content to the container width with the min bound, leaving remaining at 0 and
    // degrading Center/End/Space* to Start. The container is clamped separately; an item wider
    // than the line overflows visually, as in Compose. The cross axis is the tallest item that
    // actually fits.
    //
    // Compose's default overflow is Clip: items are added to the line one at a time, and the first
    // item whose cross size exceeds the remaining cross space stops the container
    // (FlowLayoutBuildingBlocks willOverflowCrossAxis). The check must be per ITEM, not per line:
    // a line of heights [20, 20, 40] in 35px keeps the two 20s and drops the 40. The sentinel for
    // "unbounded" in winia is f32::MAX as well as infinity (see flex::build_phase1), hence the
    // `< f32::MAX` test rather than `is_finite`.
    struct LineInfo {
        main: f32,
        cross: f32,
        line_before: f32,
    }
    let max_cross = A::cross_max(constraints);
    let bounded_cross = overflow == FlowOverflow::Clip && max_cross < f32::MAX;
    let mut visible_lines: Vec<(usize, usize)> = Vec::with_capacity(lines.len());
    let mut line_infos: Vec<LineInfo> = Vec::with_capacity(lines.len());
    let mut used_cross = 0.0f32;
    for (li, &(s, e)) in lines.iter().enumerate() {
        let gap = if li == 0 { 0.0 } else { cross_spacing };
        let mut line_cross = 0.0f32;
        let mut end = s;
        let mut truncated = false;
        for i in s..e {
            let next_cross = line_cross.max(A::cross_size(child_sizes[i]));
            if bounded_cross && used_cross + gap + next_cross > max_cross {
                truncated = true;
                break;
            }
            line_cross = next_cross;
            end = i + 1;
        }
        if end == s {
            break;
        }
        // FlowRowScope.fillMaxRowHeight / FlowColumnScope.fillMaxColumnWidth: the child is
        // measured against a cross size that is a fraction of the line's natural cross extent.
        // The line cross itself was just computed from the natural sizes, so the fill cannot grow
        // the line; it can change the child's main size, which is recomputed below. A weighted
        // child keeps the allocation from the weight pass (Compose's weight branch likewise does
        // not apply the fill fraction).
        for i in s..end {
            let child = children[i];
            if nodes[child]
                .modifier
                .get_layout_weight()
                .is_some_and(|w| w > 0.0)
            {
                continue;
            }
            let fraction = if A::HORIZONTAL {
                nodes[child].modifier.get_fill_max_row_height()
            } else {
                nodes[child].modifier.get_fill_max_column_width()
            };
            let Some(fraction) = fraction else { continue };
            let target_cross = line_cross * fraction;
            let base = A::build_phase1(constraints, main_limit);
            let cc = A::with_fixed_cross(&base, target_cross);
            let (size, _) = measure_node(nodes, policies, child, cc);
            child_sizes[i] = size;
        }
        let mut main = 0.0f32;
        let mut line_before = 0.0f32;
        let mut line_after = 0.0f32;
        for i in s..end {
            if i > s {
                main += main_spacing;
            }
            main += A::main_size(child_sizes[i]);
            let child = children[i];
            if let Some(line) = nodes[child].modifier.get_align_by() {
                let position = nodes[child].alignment_line(line);
                let child_cross = A::cross_size(child_sizes[i]);
                // An unspecified line contributes 0 above and 0 below, as flex.rs and Compose's
                // RowColumnMeasurePolicy do; treating it as the whole child would inflate a line
                // next to a non-text Icon that merely carries align_by_baseline.
                line_before = line_before.max(position.unwrap_or(0.0));
                line_after = line_after.max(child_cross - position.unwrap_or(child_cross));
            }
        }
        // A baseline-aligned child contributes its line's distance from its own edge and the
        // content below it; the line must be tall enough for the line plus that tail, exactly as
        // the flex containers do (RowColumnMeasurePolicy.kt:228-251).
        let line_cross = line_cross.max(line_before + line_after);
        visible_lines.push((s, end));
        line_infos.push(LineInfo { main, cross: line_cross, line_before });
        used_cross += gap + line_cross;
        if truncated {
            break;
        }
    }
    lines = visible_lines;

    let mut content_cross = 0.0f32;
    for (i, info) in line_infos.iter().enumerate() {
        if i > 0 {
            content_cross += cross_spacing;
        }
        content_cross += info.cross;
    }

    // Container size.
    let container_main = A::constrain_main(
        constraints,
        line_infos.iter().map(|l| l.main).fold(0.0f32, f32::max),
    );
    let container_cross = A::constrain_cross(constraints, content_cross);
    let remaining_cross = (container_cross - content_cross).max(0.0);
    let (cross_extra, cross_leading) = compute_spacing(
        cross_arrangement,
        remaining_cross,
        line_infos.len().saturating_sub(1),
    );

    // Placement.
    let mut placements: Vec<Placement> = vec![
        Placement {
            size: Size::ZERO,
            position: Offset::new(0.0, 0.0),
        };
        n
    ];
    let mut cross_offset = cross_leading;
    for (li, &(s, e)) in lines.iter().enumerate() {
        let info = &line_infos[li];
        let count = e - s;
        // Remaining room in the line. spaceBetween and friends fill the container; for a plain
        // line that is exactly its content, remaining is 0. Compose bases in-line arrangement on
        // the line's layout size = max(content, constraints.min); winia simplifies that to the
        // container's main axis (fill semantics), so Start/Center/End align inside the container.
        // The difference is invisible under wrap-content when the line is as wide as its content.
        // To keep wrap-content intuitive, the line's layout size is max(content, min bound):
        let row_layout_main = info.main.max(A::constrain_main(constraints, 0.0));
        // Concretely: a fill parent (min == max) gives a container-wide line with alignment inside
        // it; a wrap parent (min == 0) gives a content-wide line. constrain_main(constraints, 0.0)
        // is the min bound (the container width under fill, 0 under wrap), so row_layout_main =
        // max(content, min) is right in both cases.
        let remaining = (row_layout_main - info.main).max(0.0);
        let gap_count = if count > 1 { count - 1 } else { 0 };
        let (spacing_extra, leading) = compute_spacing(arrangement, remaining, gap_count);
        let eff_spacing = main_spacing + spacing_extra;

        let mut main_offset = leading;
        for i in s..e {
            let sz = child_sizes[i];
            let child_main = A::main_size(sz);
            let child_cross_full = A::cross_size(sz);
            // Cross-axis alignment inside the line; align_self overrides the container alignment,
            // reusing the flex semantics.
            let align = nodes[children[i]]
                .modifier
                .get_align_self()
                .unwrap_or(alignment);
            let line = nodes[children[i]].modifier.get_align_by();
            let child_cross = if align == Alignment::Stretch && line.is_none() {
                info.cross
            } else {
                child_cross_full
            };
            let cross_in_line = match line {
                Some(line) => {
                    info.line_before - nodes[children[i]].alignment_line(line).unwrap_or(0.0)
                }
                None => match align {
                    Alignment::Start => 0.0,
                    Alignment::End => info.cross - child_cross,
                    Alignment::Center => (info.cross - child_cross) / 2.0,
                    Alignment::Stretch => 0.0,
                },
            };
            placements[i] = Placement {
                size: A::size(child_main, child_cross),
                position: A::point(main_offset, cross_offset + cross_in_line),
            };
            main_offset += child_main + eff_spacing;
        }
        cross_offset += info.cross + cross_spacing + cross_extra;
    }

    // RTL mirroring (the same final pass flex uses: container width - x - w). Mirror exactly the
    // items that were placed; a legitimate zero-size child still needs mirroring, while an omitted
    // child must stay at (0, 0) so its placement does not depend on the layout direction.
    if direction == LayoutDirection::Rtl {
        let container_x = A::rtl_container_width(
            A::main_size(A::size(container_main, container_cross)),
            container_cross,
        );
        let mut placed = vec![false; n];
        for &(s, e) in &lines {
            for i in s..e {
                placed[i] = true;
            }
        }
        for (i, p) in placements.iter_mut().enumerate() {
            if placed[i] {
                p.position.x = container_x - p.position.x - p.size.width;
            }
        }
    }

    (
        A::size(container_main, container_cross),
        placements,
    )
}
