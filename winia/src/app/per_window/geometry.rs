use super::*;

/// Compute a node's absolute position in the layout tree (accumulating `position` from the root).
///
/// ⚠ Same space as `hit_test`/`scene_to_node_local`: the accumulation **subtracts ancestor
/// scroll offsets** (a scroll container renders as `canvas.translate(-offset)`, so the scene
/// coordinates hit testing works in are already the scroll-adjusted "scrolled canvas" ones).
/// Previously this summed layout positions only, so inside a scroll container a node's absolute y
/// was polluted by the scroll amount → the paragraph-local coordinate (scene - abs) went negative
/// → skia's nearest glyph was always the first line, and clicking/dragging a multi-line TextField
/// could only land on line one.
pub(crate) fn node_abs_position(nodes: &[LayoutNode], root: usize, id: u64) -> (f32, f32) {
    pub(crate) fn walk(nodes: &[LayoutNode], idx: usize, target: u64, abs_x: f32, abs_y: f32) -> Option<(f32, f32)> {
        let node = &nodes[idx];
        let nx = abs_x + node.position.x;
        let ny = abs_y + node.position.y;
        if node.id == target { return Some((nx, ny)); }
        // Same path as hit testing: a child's coordinate = this node's coordinate - its scroll offset.
        let (dx, dy) = crate::layout::node::scroll_offset_for_node(node);
        for &c in &node.children {
            if let Some(r) = walk(nodes, c, target, nx - dx, ny - dy) { return Some(r); }
        }
        None
    }
    walk(nodes, root, id, 0.0, 0.0).unwrap_or((0.0, 0.0))
}

/// Dispatch a pointer event along the hit_test path (pre: outer→inner, bubble: inner→outer).
pub(crate) fn dispatch_ptr_event(
    nodes: &[LayoutNode],
    root: usize,
    path: &[usize],
    event: &crate::input::PointerEvent,
    scene_pos: (f32, f32),
    captured_id: Option<u64>,
) -> bool {
    // If a node captured the pointer (pressed and not yet released), find it from the root and
    // build its ancestor chain.
    let captured_path: Vec<usize> = if let Some(cid) = captured_id {
        let mut ancestors: Vec<usize> = Vec::new();
        if let Some(nid) = crate::layout::node::find_node_id_by_slot_key(nodes, root, cid).and_then(|nid| crate::layout::node::find_node_by_id(nodes, root, nid)) {
            ancestors.push(nid);
            let mut pid = nodes[nid].parent_id;
            while let Some(id) = pid {
                if let Some(anc) = crate::layout::node::find_node_by_id(nodes, root, id) {
                    ancestors.push(anc);
                    pid = nodes[anc].parent_id;
                } else { break; }
            }
            ancestors.reverse(); // root → ... → captured
            ancestors
        } else {
            path.to_vec()
        }
    } else { path.to_vec() };
    let use_path = &captured_path;
    // Accumulate the path's offset (each node's `position` is relative to its parent).
    // As in `hit_test_recursive`: node i's visual coordinate = Σposition[0..=i]
    // − Σscroll[0..i-1] (the ancestors' scroll offsets; a node's own offset only affects
    // its children, not its own top-left — see `hit_test_recursive` in node.rs).
    let mut abs_x = 0.0f32;
    let mut abs_y = 0.0f32;
    let mut scroll_x = 0.0f32;
    let mut scroll_y = 0.0f32;
    let abs_positions: Vec<(f32, f32)> = use_path.iter().map(|&i| {
        abs_x += nodes[i].position.x;
        abs_y += nodes[i].position.y;
        // This node's coordinate = accumulated position − accumulated ancestor scroll
        // (its own offset is added later).
        let pos = (abs_x - scroll_x, abs_y - scroll_y);
        let (sdx, sdy) = crate::layout::node::scroll_offset_for_node(&nodes[i]);
        scroll_x += sdx;
        scroll_y += sdy;
        pos
    }).collect();

    // on_pre_ptr: outer → inner
    // Dual track (exp/modifier-node): the enum `on_pre_ptr` handlers run first, then the node's own
    // `on_pre` on the same node.
    for (i, &ni) in use_path.iter().enumerate() {
        let local_x = scene_pos.0 - abs_positions[i].0;
        let local_y = scene_pos.1 - abs_positions[i].1;
        let mut ev = event.clone();
        ev.position = (local_x, local_y);
        for el in nodes[ni].modifier.elements() {
            if let crate::modifier::ModifierElement::PointerEvent { on_pre_ptr: Some(handler), .. } = el {
                if handler(&ev) { return true; }
            }
        }
        for p in nodes[ni].modifier.pointer_nodes() {
            if p.on_pre(&ev) { return true; }
        }
    }

    // on_ptr: inner → outer
    for (i, &ni) in use_path.iter().enumerate().rev() {
        let local_x = scene_pos.0 - abs_positions[i].0;
        let local_y = scene_pos.1 - abs_positions[i].1;
        let mut ev = event.clone();
        ev.position = (local_x, local_y);
        for el in nodes[ni].modifier.elements() {
            if let crate::modifier::ModifierElement::PointerEvent { on_ptr: Some(handler), .. } = el {
                if handler(&ev) { return true; }
            }
        }
        for p in nodes[ni].modifier.pointer_nodes() {
            if p.on_event(&ev) { return true; }
        }
    }
    false
}

/// Redraw-request throttle (pure function — unit-tested at the boundaries): a request is allowed
/// only when at least one frame interval has passed since the last one, which keeps WM_PAINT
/// generation at the refresh rate. Deliberately independent of the render throttle
/// (`last_render_time`): when WM_PAINT lands after the request (ε>0), `now-last_render = I-ε < I`
/// would block every timer wake-up and halve the render rate (a 2I interval).
#[allow(dead_code)] // the tests in this file call it
pub(crate) fn should_request_redraw(last_request: std::time::Instant, now: std::time::Instant, interval: std::time::Duration) -> bool {
    now.duration_since(last_request) >= interval
}
