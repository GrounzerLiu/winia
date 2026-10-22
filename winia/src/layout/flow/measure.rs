//! The flow measurement algorithm shared by FlowRow and FlowColumn.

use crate::layout::flex::FlexAxis;
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
    main_spacing: f32,
    cross_spacing: f32,
    max_items_in_main_axis: usize,
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

    let max_main = A::main_max(constraints);
    // An unbounded main axis never runs out of room, so it degenerates to a single Row/Column
    // line (as Compose does). max_items is orthogonal to available space, though, so it still
    // forces a wrap when unbounded (review P1: can_wrap must not disable the count branch).
    let can_wrap = max_main.is_finite();

    // Measure each item loosely; v1 has no weight, so natural sizes are the whole story.
    let mut child_sizes: Vec<Size> = vec![Size::ZERO; n];
    for (i, &c) in children.iter().enumerate() {
        let cc = A::build_phase1(constraints, f32::MAX);
        let (size, _) = measure_node(nodes, policies, c, cc);
        child_sizes[i] = size;
    }

    // Wrapping (Compose getWrapInfo.shouldWrapItem): never at the start of a line, always past
    // max_items, and when the remaining room cannot fit the item. lines holds (start, end) per line.
    let mut lines: Vec<(usize, usize)> = Vec::new();
    let mut line_start = 0usize;
    let mut line_main = 0.0f32; // main-axis space used by the current line, spacing included
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

    // Line sizes: the main axis is the content width (clamped by the constraints), the cross axis
    // is the tallest item in the line.
    struct LineInfo {
        main: f32,
        cross: f32,
    }
    let mut line_infos: Vec<LineInfo> = Vec::with_capacity(lines.len());
    let mut content_cross = 0.0f32;
    for &(s, e) in &lines {
        let mut main = 0.0f32;
        let mut cross = 0.0f32;
        for i in s..e {
            let sz = child_sizes[i];
            if i > s {
                main += main_spacing;
            }
            main += A::main_size(sz);
            cross = cross.max(A::cross_size(sz));
        }
        // The line's main size is the raw content width, NOT clamped: under fill, clamping would
        // lift the content to the container width with the min bound, leaving remaining at 0 and
        // degrading Center/End/Space* to Start. The container is clamped separately; an item wider
        // than the line overflows visually, as in Compose.
        line_infos.push(LineInfo { main, cross });
        content_cross += cross;
    }
    let total_cross_spacing = cross_spacing * (lines.len() as f32 - 1.0).max(0.0);
    content_cross += total_cross_spacing;

    // Container size.
    let container_main = A::constrain_main(
        constraints,
        line_infos.iter().map(|l| l.main).fold(0.0f32, f32::max),
    );
    let container_cross = A::constrain_cross(constraints, content_cross);

    // Placement.
    let mut placements: Vec<Placement> = vec![
        Placement {
            size: Size::ZERO,
            position: Offset::new(0.0, 0.0),
        };
        n
    ];
    let mut cross_offset = 0.0f32;
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
            let child_cross = if align == Alignment::Stretch {
                info.cross
            } else {
                child_cross_full
            };
            let cross_in_line = match align {
                Alignment::Start => 0.0,
                Alignment::End => info.cross - child_cross,
                Alignment::Center => (info.cross - child_cross) / 2.0,
                Alignment::Stretch => 0.0,
            };
            placements[i] = Placement {
                size: A::size(child_main, child_cross),
                position: A::point(main_offset, cross_offset + cross_in_line),
            };
            main_offset += child_main + eff_spacing;
        }
        cross_offset += info.cross + cross_spacing;
    }

    // RTL mirroring (the same final pass flex uses: container width - x - w).
    if direction == LayoutDirection::Rtl {
        let container_x = A::rtl_container_width(
            A::main_size(A::size(container_main, container_cross)),
            container_cross,
        );
        for p in &mut placements {
            p.position.x = container_x - p.position.x - p.size.width;
        }
    }

    (
        A::size(container_main, container_cross),
        placements,
    )
}
