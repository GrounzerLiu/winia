//! Where a selection's two handles sit.
//!
//! A handle belongs to the SELECTION, not to a text node: the start may be in one run and the end in
//! another, so the points are computed by walking the arena once per end (the run's slot key names
//! the node) and asking that run's paragraph where the caret for the offset is.

use crate::layout::node::{find_node_by_id, find_node_id_by_slot_key, LayoutNode};
use crate::text::selection::SelectionRegistrar;
use crate::text::TextLayout;

/// A handle's scene position: the point sits just BELOW the line, which is where the Compose handles
/// hang (the circle is drawn centred there).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct HandlePoint {
    pub x: f32,
    pub y: f32,
    /// The line height at that caret, so the caller can offset the circle by its radius.
    pub line_height: f32,
}

/// Both handles of a selection, `[start, end]`, or None when there is no selection or its runs left
/// the tree.
pub(crate) fn selection_handle_points(
    nodes: &[LayoutNode],
    root: usize,
    reg: &SelectionRegistrar,
) -> Option<[HandlePoint; 2]> {
    let (start, end) = reg.selection_ends()?;
    let start_point = point_for(nodes, root, start)?;
    let end_point = point_for(nodes, root, end)?;
    Some([start_point, end_point])
}

fn point_for(nodes: &[LayoutNode], root: usize, end: (u64, usize)) -> Option<HandlePoint> {
    let (slot_key, offset) = end;
    let id = find_node_id_by_slot_key(nodes, root, slot_key)?;
    let idx = find_node_by_id(nodes, root, id)?;
    let (abs_x, abs_y) = crate::app::node_abs_position(nodes, root, id);
    let borrow = nodes[idx].cached_paragraph.try_borrow().ok()?;
    let para = borrow.as_ref()?;
    let layout = TextLayout::new(para);
    // The registrar speaks the run's content bytes; a truncated paragraph answers in display bytes.
    let display = para.display_offset(offset.min(para.content_len()));
    let (cx, cy, ch) = layout.get_cursor_position(display)?;
    Some(HandlePoint {
        x: abs_x + cx,
        y: abs_y + cy + ch,
        line_height: ch,
    })
}
