use super::*;

// ── Hit testing ──

/// Children in PAINT order: tree order, except that a child carrying `Modifier::z_index` moves after
/// the ones with a lower z (stable, so equal z keeps tree order).
///
/// The renderer walks this order to paint and the hit test walks it in reverse, so the topmost sibling
/// both covers and receives the press — Compose's `Modifier.zIndex` contract (it governs drawing order
/// and pointer input order alike). Call it only when [`LayoutNode::children_have_z`] says a child
/// actually sets a z: the common case then keeps iterating `node.children` directly.
pub fn paint_order(nodes: &[LayoutNode], children: &[usize]) -> Vec<usize> {
    let mut order = children.to_vec();
    order.sort_by(|&a, &b| {
        let za = nodes[a].modifier.get_z_index().unwrap_or(0.0);
        let zb = nodes[b].modifier.get_z_index().unwrap_or(0.0);
        za.partial_cmp(&zb).unwrap_or(std::cmp::Ordering::Equal)
    });
    order
}

/// Hit test: returns the chain of node indices from the root down to the leaf (arena flavour).
///
/// The coordinate space is the layout space (scene coordinates). Only scroll offsets are handled —
/// they move the layout; **graphics_layer transforms are not** — a layer changes what is drawn, not
/// what is hit (as in Compose's graphicsLayer: the touch area stays the layout bounds, and a visual
/// overhang or inset after a transform does not change it). The ripple press point's local-coordinate
/// conversion (scene_to_node_local) keeps the same rule.
pub fn hit_test(nodes: &[LayoutNode], root: usize, x: f32, y: f32) -> Vec<usize> {
    let mut path = Vec::new();
    hit_test_recursive(nodes, root, x, y, 0.0, 0.0, &mut path);
    path
}

/// Flight-aware hit test (Phase 3; extended by the overlay pass in Phase 6).
/// The transition layer paints above the main tree, so it is tested first,
/// topmost entry first:
///
/// - **Source** (detached ghost): clicking the ghost == clicking the live
///   target — fraction-mapped into the target's natural rect with a
///   root-anchored path for bubbling fidelity.
/// - **Target/Morph, elevated** (Compose `renderInOverlayDuringTransition`):
///   the endpoint is *still in the tree*, so it routes through its own lerped
///   visual rect with the flight transform inverted, exactly like an in-tree
///   transitioning node — but WITHOUT the ancestor bounds/viewport rejection.
///   That rejection is what makes an element flying outside its container
///   untouchable, and escaping it is the whole point of the overlay pass.
///
/// Everything else falls through to [`hit_test`]. Empty roots (idle) cost one
/// length check — the hot path is untouched.
pub fn hit_test_with_flights(
    nodes: &[LayoutNode],
    root: usize,
    transition_roots: &[usize],
    x: f32,
    y: f32,
) -> Vec<usize> {
    if !transition_roots.is_empty() {
        // Index for parent-chain walks (mid-flight clicks only).
        let mut id_to_idx = std::collections::HashMap::new();
        for (i, n) in nodes.iter().enumerate() {
            id_to_idx.insert(n.id, i);
        }
        // Painted back-to-front, so the LAST entry is on top — test in reverse.
        for &tidx in transition_roots.iter().rev() {
            let Some(node) = nodes.get(tidx) else { continue };
            if node.paint == PaintDisposition::InLayer && node.transition.is_none() {
                // Chrome elevated for the flight: it paints above the flying
                // pair, so it must also be HIT before them — otherwise a tap
                // inside the overlap (a button on a pinned bar the hero slides
                // under) would be routed to the hero. It has no flight
                // transform, so the ordinary subtree walk applies.
                if let Some(path) = hit_through_chrome(nodes, root, &id_to_idx, tidx, x, y) {
                    return path;
                }
                continue;
            }
            let Some(t) = node.transition.as_ref() else { continue };
            let hit = match t.role {
                TransitionRole::Source => hit_through_ghost(nodes, root, &id_to_idx, tidx, x, y),
                TransitionRole::Target | TransitionRole::Morph => {
                    if !t.elevated {
                        continue;
                    }
                    hit_through_elevated(nodes, root, &id_to_idx, tidx, x, y)
                }
            };
            if let Some(path) = hit {
                return path;
            }
        }
    }
    hit_test(nodes, root, x, y)
}

/// Root→`idx` ancestor chain including `root` (bubbling fidelity), or `None`
/// when the chain is broken (stale arena). `idx` itself is NOT included.
pub(crate) fn ancestor_prefix(
    nodes: &[LayoutNode],
    id_to_idx: &std::collections::HashMap<u64, usize>,
    root: usize,
    idx: usize,
) -> Option<Vec<usize>> {
    let mut prefix: Vec<usize> = Vec::new();
    let mut cur = idx;
    let mut ok = idx == root;
    while let Some(pid) = nodes[cur].parent_id {
        let Some(&pidx) = id_to_idx.get(&pid) else { break };
        if pidx == root {
            prefix.push(root);
            ok = true;
            break;
        }
        prefix.push(pidx);
        cur = pidx;
    }
    if !ok {
        return None;
    }
    prefix.reverse();
    Some(prefix)
}

/// Elevated chrome (Compose `renderInSharedTransitionScopeOverlay`):
/// an ordinary subtree that happens to paint from the layer, at its own
/// position and with no transform — so the normal recursion applies, entered
/// at the node's absolute origin with the root→node prefix for bubbling.
pub(crate) fn hit_through_chrome(
    nodes: &[LayoutNode],
    root: usize,
    id_to_idx: &std::collections::HashMap<u64, usize>,
    idx: usize,
    x: f32,
    y: f32,
) -> Option<Vec<usize>> {
    let node = nodes.get(idx)?;
    if node.paint != PaintDisposition::InLayer {
        return None;
    }
    let tb = abs_rect_upward(nodes, id_to_idx, idx);
    let mut path = ancestor_prefix(nodes, id_to_idx, root, idx)?;
    if hit_test_recursive(
        nodes,
        idx,
        x,
        y,
        tb.x - node.position.x,
        tb.y - node.position.y,
        &mut path,
    ) {
        Some(path)
    } else {
        None
    }
}

/// Detached source ghost → live target routing (Phase 3).
pub(crate) fn hit_through_ghost(
    nodes: &[LayoutNode],
    root: usize,
    id_to_idx: &std::collections::HashMap<u64, usize>,
    tidx: usize,
    x: f32,
    y: f32,
) -> Option<Vec<usize>> {
    let t = nodes.get(tidx)?.transition.as_ref()?;
    let l = t.lerped();
    if l.width <= 0.0 || l.height <= 0.0 {
        return None;
    }
    if x < l.x || x > l.x + l.width || y < l.y || y > l.y + l.height {
        return None;
    }
    let target_slot = t.link_slot?;
    // NOTE (Tier1 limitation): the target may live in a PEER composer's arena
    // (overlay) — invisible to this single-arena search, so cross-composer
    // ghosts paint but ignore taps. The live target itself stays directly
    // hittable in its own composer.
    let target_idx = find_idx_by_slot(nodes, root, target_slot)?;
    // Fraction-map ghost → target natural rect, then descend the live target
    // subtree directly: the target node itself is hit by construction
    // (fractions clamped into its natural rect), and re-entering
    // hit_test_recursive on it would invert the flight transform twice
    // (remap is single-application per level).
    let fx = ((x - l.x) / l.width).clamp(0.0, 1.0);
    let fy = ((y - l.y) / l.height).clamp(0.0, 1.0);
    let tb = abs_rect_upward(nodes, id_to_idx, target_idx);
    // Scrolled-container targets: mapped point outside the visible viewport
    // misses (mirrors the viewport clamp in recursion).
    {
        let tn = &nodes[target_idx];
        // Content box when the target has no explicit scroll viewport.
        let cb = tn.content_box();
        let vw = if tn.scroll_viewport_width > 0.0 {
            tn.scroll_viewport_width
        } else {
            cb.width
        };
        let vh = if tn.scroll_viewport_height > 0.0 {
            tn.scroll_viewport_height
        } else {
            cb.height
        };
        let tx0 = tb.x + fx * tb.width;
        let ty0 = tb.y + fy * tb.height;
        if tx0 < tb.x || tx0 > tb.x + vw || ty0 < tb.y || ty0 > tb.y + vh {
            return None;
        }
    }
    let tx = tb.x + fx * tb.width;
    let ty = tb.y + fy * tb.height;
    // Ancestor prefix root→target, target excluded (pushed below). Broken
    // chains fall through to the main hit test.
    let mut path = ancestor_prefix(nodes, id_to_idx, root, target_idx)?;
    path.push(target_idx);
    // Child basis in layout space (scroll-corrected, like recursion).
    let (sdx, sdy) = scroll_offset_for_node(&nodes[target_idx]);
    let (cpx, cpy) = (tb.x - sdx, tb.y - sdy);
    for &c in nodes[target_idx].children.iter().rev() {
        if hit_test_recursive(nodes, c, tx, ty, cpx, cpy, &mut path) {
            break;
        }
    }
    Some(path)
}

/// Elevated live endpoint (Target/Morph painted by the transition layer):
/// same remap contract as [`hit_test_recursive`], minus ancestor rejection.
pub(crate) fn hit_through_elevated(
    nodes: &[LayoutNode],
    root: usize,
    id_to_idx: &std::collections::HashMap<u64, usize>,
    idx: usize,
    x: f32,
    y: f32,
) -> Option<Vec<usize>> {
    let node = &nodes[idx];
    let t = node.transition.as_ref()?;
    let l = t.lerped();
    if l.width <= 0.0 || l.height <= 0.0 {
        return None;
    }
    // Absolute position in the same frame the layer paints in.
    let tb = abs_rect_upward(nodes, id_to_idx, idx);
    let (w, h) = {
        // CONTENT box: the flight scales (or re-lays-out) the content into the
        // lerped rect, so the inverse mapping must use that box — the
        // placeholder size the parent was told lives in `measured_size`.
        // A RE-MEASURED end is painted 1:1 on the lerped rect (render skips the
        // scale there, because the content box trails `lerped()` by one poll),
        // so its inverse is the identity.
        if t.remeasure {
            (l.width, l.height)
        } else {
            let cb = node.content_box();
            (cb.width, cb.height)
        }
    };
    let (lx, ly) = t.remap_hit(x, y, tb.x, tb.y, w, h)?;
    // The endpoint's OWN viewport clamp still applies — only ancestors are
    // escaped (a scrollable hero must not take input outside its viewport).
    let mut nw = w;
    let mut nh = h;
    if node.scroll_viewport_width > 0.0 {
        nw = node.scroll_viewport_width;
    }
    if node.scroll_viewport_height > 0.0 {
        nh = node.scroll_viewport_height;
    }
    if lx < tb.x || lx > tb.x + nw || ly < tb.y || ly > tb.y + nh {
        return None;
    }
    let mut path = ancestor_prefix(nodes, id_to_idx, root, idx)?;
    path.push(idx);
    // Child basis: same convention as hit_test_recursive (parent minus its own
    // scroll translate), so descendants resolve identically in-tree and in the
    // layer.
    let (sdx, sdy) = scroll_offset_for_node(node);
    let (cpx, cpy) = (tb.x - sdx, tb.y - sdy);
    for &c in node.children.iter().rev() {
        if hit_test_recursive(nodes, c, lx, ly, cpx, cpy, &mut path) {
            break;
        }
    }
    Some(path)
}

pub(crate) fn hit_test_recursive(
    nodes: &[LayoutNode],
    idx: usize,
    x: f32,
    y: f32,
    parent_x: f32,
    parent_y: f32,
    path: &mut Vec<usize>,
) -> bool {
    let node = &nodes[idx];
    // A `Placeholder` is a marked copy of a key a flight already owns: it is NOT painted (the flight draws
    // the animated rect), so it must not be hit either — otherwise an invisible duplicate elsewhere on the
    // screen still receives taps and fires its handlers. The whole subtree goes with it; a shared element
    // nested inside stays reachable through the flight's own hit pass, which tests the flying ends first.
    if node.paint == PaintDisposition::Placeholder {
        return false;
    }
    // `Modifier.visible(false)`: not painted, so not hit either — the whole subtree goes with it,
    // exactly like the placeholder case above.
    if !node.modifier.is_visible() {
        return false;
    }
    let nx = parent_x + node.position.x;
    let ny = parent_y + node.position.y;
    let mut nw = node.content_box().width;
    let mut nh = node.content_box().height;
    // A scroll container is hit over its VISIBLE viewport: measured_size is the full content height,
    // and using it would let scrolled-out content swallow clicks and wheel events meant for the
    // siblings that follow.
    if node.scroll_viewport_height > 0.0 {
        nh = node.scroll_viewport_height;
    }
    if node.scroll_viewport_width > 0.0 {
        nw = node.scroll_viewport_width;
    }

    // Flight remap (Phase 3): transitioning endpoints test their lerped
    // visual rect; hits descend in layout space (children keep layout
    // positions — the flight transform is inverted here). Visual miss passes
    // through (v1 skip behavior).
    let (x, y) = match node.transition.as_ref() {
        Some(t) => {
            // The inverse flight transform maps through the box the flight
            // PAINTED: a re-measured end is drawn 1:1 on the lerped rect (render
            // skips the scale), everything else through its content box. Using
            // the reported placeholder size would misplace child descent
            // whenever the two differ (Compose `PlaceHolderSize`).
            let (mw, mh) = if t.remeasure {
                let l = t.lerped();
                (l.width, l.height)
            } else {
                let cb = node.content_box();
                (cb.width, cb.height)
            };
            match t.remap_hit(x, y, nx, ny, mw, mh) {
                Some(p) => p,
                None => return false,
            }
        }
        None => (x, y),
    };

    // Is the point inside the node?
    if x < nx || x > nx + nw || y < ny || y > ny + nh {
        return false;
    }

    path.push(idx);

    // The scroll offset (the render pass translates by -offset)
    let (scroll_dx, scroll_dy) = scroll_offset_for_node(node);

    // Child coordinates = parent coordinates + the scroll offset
    let child_px = nx - scroll_dx;
    let child_py = ny - scroll_dy;

    // Depth first: children before the node itself (a child paints above its parent).
    // The siblings are walked in REVERSE: painting follows PAINT order (later paints higher), so the
    // hit test has to prefer the later one (z-order semantics). Walking forwards made an upper sibling
    // (a rectangle over a full-screen image, say) unreachable — the image underneath kept the hit.
    // PAINT order equals tree order unless a child sets `Modifier::z_index` (`paint_order`; a higher z
    // paints later and is hit first, as in Compose), so the walk goes through paint_order in reverse,
    // which is the old behaviour whenever no z is used.
    let ordered;
    let order: &[usize] = if node.children_have_z {
        ordered = paint_order(nodes, &node.children);
        &ordered
    } else {
        &node.children
    };
    for &c in order.iter().rev() {
        // Shared-element flight (Phase 3): live endpoints (Target/Morph)
        // descend via remap_hit above, so their subtrees stay hittable
        // mid-flight. Only Source-role visuals are skipped — detached
        // retained sources are unreachable from the root walk anyway, and a
        // Source visual must never take input (its ghost routes to the live
        // target through the transition-roots prefix instead).
        if nodes[c].transition.as_ref().is_some_and(|t| t.role == TransitionRole::Source) {
            continue;
        }
        if hit_test_recursive(nodes, c, x, y, child_px, child_py, path) {
            return true;
        }
    }

    // No child matched, so the walk stops at this node
    true
}

/// Scene coordinates to the target node's local coordinates (relative to its top-left corner).
///
/// Takes the same path `hit_test_recursive` does: accumulate `position` from the root and subtract
/// the ancestors' scroll offsets (the render pass translates a scroll container by -offset). The
/// ripple stores its press point this way, matching Compose's `PressInteraction.Press.pressPosition`
/// (the draw pass adds the layout origin back, so the ripple follows the node through scrolling).
/// graphics_layer transforms are not undone — same as the hit test, which does not invert them
/// either.
///
/// `node_abs_position` (app.rs) and `hit_test` use the same space: a change to one has to change the
/// other (multi-line TextField click positioning depends on the two agreeing).
/// A scroll container's offset (converts scene coordinates into content coordinates).
///
/// RTL (scroll_reverse): the render pass translates by `off = content_w - viewport_w - offset`
/// (render.rs:746-748 mirrors it), so the hit test must use the same off — otherwise clicks land in
/// the wrong place (measured: a scrolled RTL tab row reacted off-target).
pub(crate) fn scroll_offset_for_node(node: &LayoutNode) -> (f32, f32) {
    let mut dx = 0.0;
    let mut dy = 0.0;
    if let Some(state) = node.modifier.vertical_scroll_state() {
        // `peek`: this is a pure geometry read (hit test, absolute-rect arithmetic, position
        // callbacks) — it must never subscribe its caller to the offset.
        dy += state.offset.peek();
    }
    if let Some(state) = node.modifier.horizontal_scroll_state() {
        dx += if node.scroll_reverse {
            // RTL: the offset's meaning is mirrored — the content translation is total - viewport -
            // offset (content_w is 0 before the first write-back, which degrades to -offset clamped at
            // 0, matching what the render pass does on that frame)
            let content_w = if node.scroll_content_width > 0.0 { node.scroll_content_width } else { 0.0 };
            (content_w - node.scroll_viewport_width - state.offset.peek()).max(0.0)
        } else {
            state.offset.peek()
        };
    }
    (dx, dy)
}

/// A node position callback — `Modifier::on_globally_positioned` / `on_placed`.
pub(crate) type PositionCallback = std::sync::Arc<dyn Fn(GlobalPosition) + Send + Sync>;

/// Walk the tree in place order and collect the position callbacks that must fire, as
/// `(on_placed, on_globally_positioned)`.
///
/// Both are CHANGE-gated: winia has no separate placement pass (placement happens inside measure and
/// a folded node is not re-placed), so "the box moved or resized" is the observable event. The
/// absolute box includes every ancestor's scroll offset, because that is what the renderer's
/// `translate(-offset)` produces on screen — the same arithmetic as `abs_rect_upward`.
pub(crate) fn collect_position_callbacks(
    nodes: &mut [LayoutNode],
    root: usize,
    origin: (f32, f32),
) -> (Vec<(PositionCallback, GlobalPosition)>, Vec<(PositionCallback, GlobalPosition)>) {
    let mut placed = Vec::new();
    let mut global = Vec::new();
    // The composer origin is the walk's starting point, so the box stored by the change gate is
    // already in WINDOW space: an overlay whose layer origin changes counts as a move and re-fires
    // the callback (adding the origin after the gate would leave a static layer reporting local
    // coordinates forever).
    collect_position_callbacks_at(nodes, root, origin.0, origin.1, &mut placed, &mut global);
    (placed, global)
}

pub(crate) fn collect_position_callbacks_at(
    nodes: &mut [LayoutNode],
    idx: usize,
    x: f32,
    y: f32,
    placed: &mut Vec<(PositionCallback, GlobalPosition)>,
    global: &mut Vec<(PositionCallback, GlobalPosition)>,
) {
    let position = GlobalPosition::new(
        x,
        y,
        nodes[idx].measured_size.width,
        nodes[idx].measured_size.height,
    );
    let rect = position.rect();
    let has_callbacks = nodes[idx]
        .modifier
        .elements()
        .iter()
        .any(|el| matches!(
            el,
            crate::modifier::ModifierElement::OnPlaced { .. }
                | crate::modifier::ModifierElement::OnGloballyPositioned { .. }
        ));
    if has_callbacks {
        if nodes[idx].last_global_position != Some(rect) {
            nodes[idx].last_global_position = Some(rect);
            for el in nodes[idx].modifier.elements() {
                match el {
                    crate::modifier::ModifierElement::OnPlaced { callback } => {
                        placed.push((callback.clone(), position));
                    }
                    crate::modifier::ModifierElement::OnGloballyPositioned { callback } => {
                        global.push((callback.clone(), position));
                    }
                    _ => {}
                }
            }
        }
    } else {
        // A node WITHOUT callbacks forgets its last box, so a callback attached later fires on its
        // first placement even when the box never moved.
        nodes[idx].last_global_position = None;
    }
    let (sx, sy) = scroll_offset_for_node(&nodes[idx]);
    let children = nodes[idx].children.clone();
    for child in children {
        let (px, py) = (nodes[child].position.x, nodes[child].position.y);
        collect_position_callbacks_at(nodes, child, x + px - sx, y + py - sy, placed, global);
    }
}

/// Scene coordinates to the target node's local coordinates (relative to its top-left corner).
pub(crate) fn scene_to_node_local(
    nodes: &[LayoutNode],
    path: &[usize],
    target: usize,
    x: f32,
    y: f32,
) -> (f32, f32) {
    let mut ox = 0.0;
    let mut oy = 0.0;
    for &i in path {
        let node = &nodes[i];
        ox += node.position.x;
        oy += node.position.y;
        if i == target {
            break;
        }
        let (dx, dy) = scroll_offset_for_node(node);
        ox -= dx;
        oy -= dy;
    }
    (x - ox, y - oy)
}


