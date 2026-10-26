use super::*;

// ── Focus traversal ──

/// Collects the ids of every focusable node in the tree (arena flavour).
pub fn collect_focusable_ids(nodes: &[LayoutNode], root: usize, list: &mut Vec<u64>) {
    // Compose's `Modifier.visible(false)` also clears `focused` (`Visible.kt:111-116`), so a hidden
    // subtree cannot be reached with Tab either.
    if !nodes[root].modifier.is_visible() {
        return;
    }
    if has_focusable_modifier(&nodes[root]) {
        list.push(nodes[root].id);
    }
    let children = nodes[root].children.clone();
    for c in children {
        collect_focusable_ids(nodes, c, list);
    }
}

/// Collects every focusable node together with its VISUAL CENTRE (absolute coordinates, ancestors'
/// scroll offsets included — the same path hit_test takes); arrow-key focus navigation uses it.
pub fn collect_focus_candidates(nodes: &[LayoutNode], root: usize) -> Vec<(u64, f32, f32)> {
    let mut out = Vec::new();
    collect_focus_candidates_rec(nodes, root, 0.0, 0.0, &mut out);
    out
}

pub(crate) fn collect_focus_candidates_rec(
    nodes: &[LayoutNode],
    idx: usize,
    parent_x: f32,
    parent_y: f32,
    out: &mut Vec<(u64, f32, f32)>,
) {
    let node = &nodes[idx];
    // A hidden subtree is not focusable at all (see `collect_focusable_ids`).
    if !node.modifier.is_visible() {
        return;
    }
    let nx = parent_x + node.position.x;
    let ny = parent_y + node.position.y;
    if has_focusable_modifier(node) {
        out.push((
            node.id,
            nx + node.measured_size.width / 2.0,
            ny + node.measured_size.height / 2.0,
        ));
    }
    let (sdx, sdy) = scroll_offset_for_node(node);
    let children = node.children.clone();
    for c in children {
        collect_focus_candidates_rec(nodes, c, nx - sdx, ny - sdy, out);
    }
}

/// Looks a node up by `id` and returns its arena index (immutable).
pub fn find_node_by_id(nodes: &[LayoutNode], root: usize, id: u64) -> Option<usize> {
    if nodes[root].id == id { return Some(root); }
    let children = nodes[root].children.clone();
    for c in children {
        if let Some(n) = find_node_by_id(nodes, c, id) { return Some(n); }
    }
    None
}

pub fn has_focusable_modifier(node: &LayoutNode) -> bool {
    let focusable = node
        .modifier
        .elements()
        .iter()
        .any(|el| matches!(el, crate::modifier::ModifierElement::Focusable { .. }));
    // `focusProperties { canFocus = false }` keeps an otherwise focusable node out of every focus
    // path — search AND a programmatic request (the request is checked in `set_focus_by_id`).
    focusable && node.modifier.focus_can_focus() != Some(false)
}

/// The root-to-node index path for `id`, if the node is in this arena.
pub(crate) fn path_to_id(nodes: &[LayoutNode], root: usize, id: u64) -> Option<Vec<usize>> {
    if nodes[root].id == id {
        return Some(vec![root]);
    }
    for c in nodes[root].children.clone() {
        if let Some(mut path) = path_to_id(nodes, c, id) {
            path.insert(0, root);
            return Some(path);
        }
    }
    None
}

/// The id Tab should land on when it ENTERS a focus group at its boundary.
///
/// Compose's `focusGroup()` remembers the active child: a group entered from the first (or last,
/// when tabbing backwards) focusable restores that memory instead. Moving WITHIN the group is left
/// alone — only the boundary candidate is substituted.
pub(crate) fn restore_focus_group_memory(
    nodes: &[LayoutNode],
    root: usize,
    ids: &[u64],
    candidate: u64,
    backwards: bool,
) -> u64 {
    let Some(path) = path_to_id(nodes, root, candidate) else { return candidate };
    // OUTERMOST group first: Compose enters a group through its ACTIVE child, and every ancestor
    // group records the deepest focused id, so the outer group's memory already points inside any
    // nested group. Consulting only the NEAREST group loses that memory the moment focus moves
    // outside the nested group but stays inside the outer one.
    for &group in path.iter() {
        if !nodes[group].modifier.is_focus_group() {
            continue;
        }
        let first = ids
            .iter()
            .copied()
            .find(|id| path_to_id(nodes, group, *id).is_some());
        let last = ids
            .iter()
            .rev()
            .copied()
            .find(|id| path_to_id(nodes, group, *id).is_some());
        let at_boundary = if backwards { Some(candidate) == last } else { Some(candidate) == first };
        if !at_boundary {
            continue;
        }
        if let Some(memory) = nodes[group].focus_group_memory {
            if ids.contains(&memory) {
                return memory;
            }
        }
    }
    candidate
}

/// Moves focus to the next focusable node and reports whether it moved.
pub fn focus_next(nodes: &mut Vec<LayoutNode>, root: usize) -> bool {
    let ids: Vec<u64> = {
        let mut ids = Vec::new();
        collect_focusable_ids(nodes, root, &mut ids);
        ids
    };
    if ids.is_empty() { return false; }
    let current = ids.iter().position(|id| {
        find_node_by_id(nodes, root, *id).map(|n| nodes[n].focused).unwrap_or(false)
    });
    let next = match current {
        Some(i) if i + 1 < ids.len() => i + 1,
        _ => 0,
    };
    let next_id = restore_focus_group_memory(nodes, root, &ids, ids[next], false);
    clear_focus(nodes, root);
    set_focus_by_id(nodes, root, next_id);
    true
}

/// Moves focus to the previous focusable node (Shift+Tab).
pub fn focus_prev(nodes: &mut Vec<LayoutNode>, root: usize) -> bool {
    let ids: Vec<u64> = {
        let mut ids = Vec::new();
        collect_focusable_ids(nodes, root, &mut ids);
        ids
    };
    if ids.is_empty() { return false; }
    let current = ids.iter().position(|id| {
        find_node_by_id(nodes, root, *id).map(|n| nodes[n].focused).unwrap_or(false)
    });
    let prev = match current {
        Some(0) | None => ids.len() - 1,
        Some(i) => i - 1,
    };
    let prev_id = restore_focus_group_memory(nodes, root, &ids, ids[prev], true);
    clear_focus(nodes, root);
    set_focus_by_id(nodes, root, prev_id);
    true
}

/// Deliver `on_focus_changed` callbacks for every node whose focus flag changed.
///
/// Runs after the frame's focus sync (main tree and each overlay), so a change from Tab, an arrow
/// key, a click or the focused node being removed is delivered once, with the final flag.
pub fn dispatch_focus_changed(nodes: &mut [LayoutNode], root: usize) {
    let mut calls: Vec<(std::sync::Arc<dyn Fn(bool) + Send + Sync>, bool)> = Vec::new();
    collect_focus_changes(nodes, root, &mut calls);
    for (callback, focused) in calls {
        callback(focused);
    }
}

pub(crate) fn collect_focus_changes(
    nodes: &mut [LayoutNode],
    idx: usize,
    out: &mut Vec<(std::sync::Arc<dyn Fn(bool) + Send + Sync>, bool)>,
) {
    let focused = nodes[idx].focused;
    if focused != nodes[idx].focus_reported {
        nodes[idx].focus_reported = focused;
        for el in nodes[idx].modifier.elements() {
            if let crate::modifier::ModifierElement::OnFocusChanged { callback } = el {
                out.push((callback.clone(), focused));
            }
        }
    }
    let children = nodes[idx].children.clone();
    for c in children {
        collect_focus_changes(nodes, c, out);
    }
}

pub fn clear_focus(nodes: &mut Vec<LayoutNode>, root: usize) {
    nodes[root].focused = false;
    let children = nodes[root].children.clone();
    for c in children {
        clear_focus(nodes, c);
    }
}

pub fn set_focus_by_id(nodes: &mut Vec<LayoutNode>, root: usize, target_id: u64) -> bool {
    let Some(path) = path_to_id(nodes, root, target_id) else { return false };
    let Some(&target) = path.last() else { return false };
    // `focusProperties { canFocus = false }`: a programmatic request is refused too.
    if nodes[target].modifier.focus_can_focus() == Some(false) {
        return false;
    }
    nodes[target].focused = true;
    // Record the active child of every focus group on the path, so re-entering restores it.
    for &idx in &path[..path.len().saturating_sub(1)] {
        if nodes[idx].modifier.is_focus_group() {
            nodes[idx].focus_group_memory = Some(target_id);
        }
    }
    true
}

/// Focuses the given node on click (`target` is an arena index).
// ── The FocusRequester global registry ──

/// Focus exactly this node id, refusing WITHOUT touching the current focus when the node is
/// missing or declares `focusProperties { canFocus = false }`. Clearing first and discovering the
/// refusal afterwards used to drop the user's focus and leave `focused_id` pointing at a node that
/// was not actually focused.
pub fn focus_node_by_id(nodes: &mut Vec<LayoutNode>, root: usize, target_id: u64) -> bool {
    let allowed = path_to_id(nodes, root, target_id)
        .and_then(|path| path.last().copied())
        .is_some_and(|target| nodes[target].modifier.focus_can_focus() != Some(false));
    if !allowed {
        return false;
    }
    clear_focus(nodes, root);
    set_focus_by_id(nodes, root, target_id)
}

/// Sets focus through a FocusRequester id.
pub fn focus_by_id(nodes: &mut Vec<LayoutNode>, root: usize, focus_requester_id: u64) -> bool {
    match find_node_id_by_focus_requester(nodes, root, focus_requester_id) {
        Some(id) => focus_node_by_id(nodes, root, id),
        None => false,
    }
}

/// Internal: finds a node id by FocusRequesterId (arena flavour).
pub fn find_node_id_by_focus_requester(nodes: &[LayoutNode], root: usize, requester_id: u64) -> Option<u64> {
    if has_focus_id(&nodes[root], requester_id) {
        return Some(nodes[root].id);
    }
    let children = nodes[root].children.clone();
    for c in children {
        if let Some(id) = find_node_id_by_focus_requester(nodes, c, requester_id) {
            return Some(id);
        }
    }
    None
}

/// Finds a node id by slot_key (a slot_key is stable across recomposition).
pub fn find_node_id_by_slot_key(nodes: &[LayoutNode], root: usize, slot_key: u64) -> Option<u64> {
    if nodes[root].slot_key == slot_key { return Some(nodes[root].id); }
    let children = nodes[root].children.clone();
    for c in children {
        if let Some(id) = find_node_id_by_slot_key(nodes, c, slot_key) {
            return Some(id);
        }
    }
    None
}

pub(crate) fn has_focus_id(node: &LayoutNode, id: u64) -> bool {
    node.modifier.elements().iter().any(|el| matches!(el, crate::modifier::ModifierElement::FocusRequesterId { id: fid } if *fid == id))
}

/// Finds the FocusRequesterId of the tree's first focused node (used for persistence).
pub fn get_focus_id(nodes: &[LayoutNode], root: usize) -> Option<u64> {
    if nodes[root].focused {
        return Some(nodes[root].id);
    }
    let children = nodes[root].children.clone();
    for c in children {
        if let Some(id) = get_focus_id(nodes, c) {
            return Some(id);
        }
    }
    None
}


