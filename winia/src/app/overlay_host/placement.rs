use super::*;

pub(crate) fn layout_overlays(pw: &mut PerWindow) {
    // material3's `matchAnchorWidth` FORCES a menu's width to its anchor's (`minWidth = maxWidth =
    // menuWidth` in `exposedDropdownSize`), so those widths are needed at MEASURE time — before this pass
    // reaches the positioning code. Resolved up front into owned values: the loop below holds
    // `&mut pw.overlay.layers` and mutates `pw.composer`, so no borrow of the arena can stay alive across it.
    let anchor_widths: Vec<f32> = {
        let nodes = pw.composer.arena_nodes();
        let root = pw.composer.layout_root_idx();
        pw.overlay.layers
            .iter()
            .map(|ov| {
                if !ov.match_anchor_width {
                    return 0.0;
                }
                let rect = ov
                    .anchor_slot
                    .and_then(|slot| root.and_then(|r| overlay_anchor_rect(nodes, r, slot)));
                rect.map(|(_, _, w, _)| w).unwrap_or(0.0)
            })
            .collect()
    };
    for (overlay_index, ov) in pw.overlay.layers.iter_mut().enumerate() {
        // Closing: still needs recompose/layout to drive the BottomSheet slide (offset animation)
        // — a static Dialog's recomposition has no side effects; the freeze applies to interaction only
        // (`hit_overlay` already skips closing ones). The previous `if closing { continue; }` left hide()'s
        // offset animation unlaid-out, so the panel stuck at 401 and only faded — visually "a fade, not a slide".
        if ov.closing {
            // Still recompose/layout, but do not handle input (`hit_overlay` already filters out closing ones).
        }
        // ⚠ Replay the CompositionLocal snapshot (theme/direction/typography captured by the main tree) —
        // the overlay's own Composer recomposes after the `provides` stack has popped, so it needs the snapshot to inherit them.
        let snap = ov.local_snapshot.clone();
        // Same same-frame contract as the main tree (a measure whose window cannot fill the viewport it
        // is measured in asks for one more compose). Consuming the request HERE, right after this
        // overlay's own layout, is what scopes it to this composer: an overlay that asks nothing leaves
        // the flag clear for the next one, so nothing is consumed on another tree's behalf.
        for _ in 0..8 {
            crate::runtime::composition_local::with_snapshot(&snap, || {
                ov.composer.recompose(|ctx| (ov.content)(ctx));
            });
            // An overlay that fits itself around its anchor also keeps material3's
            // `MenuVerticalMargin` (48dp) clear of the top and bottom window edges, so a menu that ends
            // up as tall as the space allows still reads as a panel floating over the page instead of a
            // full-bleed column. The margin is a MEASURE constraint here and a placement clamp below:
            // without it a 30-item menu took the whole window height and sat flush against both edges.
            let constraints = if ov.fit_around_anchor {
                // material3's `matchAnchorWidth` FORCES the width (`minWidth = maxWidth = menuWidth` in
                // `exposedDropdownSize`), so the anchor's rect is needed at measure time — before the
                // positioning pass — and the anchor lives in the main tree, already laid out.
                let anchor_width = anchor_widths[overlay_index];
                let margin = crate::components::dropdown_menu::MENU_VERTICAL_MARGIN;
                crate::layout::Constraints::new(
                    anchor_width,
                    if ov.match_anchor_width { anchor_width } else { pw.width },
                    margin,
                    (pw.height - margin * 2.0).max(0.0),
                )
            } else {
                crate::layout::Constraints::new(0.0, pw.width, 0.0, pw.height)
            };
            ov.composer.layout(constraints);
            if !crate::runtime::composer::take_compose_after_layout() {
                break;
            }
        }
    }
    // Positioning (needs the main tree's anchor position — computed before draw).
    let nodes = pw.composer.arena_nodes();
    let root = pw.composer.layout_root_idx();
    let (w, h) = (pw.width, pw.height);
    for ov in &mut pw.overlay.layers {
        let size = ov.composer.layout_root()
            .map(|r| (r.measured_size.width, r.measured_size.height))
            .unwrap_or((0.0, 0.0));
        // Anchor position (main tree).
        let anchor = ov.anchor_slot.and_then(|s| root.and_then(|r| {
            crate::layout::node::find_node_id_by_slot_key(nodes, r, s)
        })).and_then(|nid| root.map(|r| {
            node_abs_position(nodes, r, nid)
        }));
        let anchor_size = ov.anchor_slot.and_then(|s| root.and_then(|r| {
            crate::layout::node::find_node_id_by_slot_key(nodes, r, s)
        })).and_then(|nid| root.and_then(|r| {
            crate::layout::node::find_node_by_id(nodes, r, nid).map(|i| nodes[i].measured_size)
        }));
        let anchored = anchor.is_some() && anchor_size.is_some();
        let (ax, ay, aw, ah) = match (anchor, anchor_size) {
            (Some((x, y)), Some(s)) => (x, y, s.width, s.height),
            _ => (0.0, 0.0, 0.0, 0.0),
        };
        // The direction a `*Start`/`*End` position resolves against: the ANCHOR node's own resolved
        // direction when there is one (the same field the anchored popup padding reads just below), the
        // root's otherwise. Reading it off the node rather than off the ambient composition local is what
        // makes it correct at placement time — placement runs after composition, and the local is not a
        // placement-time concept.
        let direction = {
            // An overlay may know its own direction from COMPOSE time, where the composition local is still
            // valid — placement runs after composition, which is why the node rule below cannot recover it.
            let from_node = ov.direction.or_else(|| {
                let anchor_node = ov
                    .anchor_slot
                    .and_then(|s| root.and_then(|r| crate::layout::node::find_node_id_by_slot_key(nodes, r, s)));
                anchor_node
                    .and_then(|nid| root.and_then(|r| crate::layout::node::find_node_by_id(nodes, r, nid)))
                    .map(|idx| nodes[idx].layout_direction)
                    .or_else(|| root.map(|r| nodes[r].layout_direction))
            });
            from_node.unwrap_or(crate::layout::direction::current())
        };
        use crate::overlay::PopupPosition as P;
        // `fit_around_anchor` candidate walk (material3's `MenuPosition.kt` lists, in
        // `dropdown_menu::dropdown_menu_position`) when there is an anchor, plain anchored placement
        // otherwise, and window alignment when there is no anchor at all — the three cases the arithmetic
        // has always had, now spelled once in `overlay::resolve_popup_position` for everything but the walk.
        let pos = if anchored && ov.fit_around_anchor {
            // material3's candidate lists: below the anchor, above it, centred on its top edge, then pinned
            // to the nearer window edge — each taken only if the menu fits inside `MenuVerticalMargin` (48dp).
            // winia's placement used to do the first alone, which is why a long menu hung off the bottom edge
            // with its last rows unreachable.
            crate::components::dropdown_menu::dropdown_menu_position((ax, ay, aw, ah), size, (w, h), direction)
        } else {
            crate::overlay::resolve_popup_position(
                ov.position,
                direction,
                if anchored { Some((ax, ay, aw, ah)) } else { None },
                size,
                (w, h),
            )
        };
        // Anchor slide: the panel starts at the anchor's corner and ends at the window's, both axes moving
        // together — Compose's `(lerp(collapsedBounds.left, offsetX, progress),
        // lerp(collapsedBounds.top, offsetY, progress))` with both offsets 0. Only this pass knows the
        // anchor's coordinates, so the interpolation happens here; the caller supplies the progress reader.
        let pos = if let (Some(slide), true) = (&ov.anchor_slide, anchored) {
            let p = slide.progress();
            (
                crate::overlay::anchor_slide_lerp(ax, p),
                crate::overlay::anchor_slide_lerp(ay, p),
            )
        } else {
            pos
        };
        ov.screen_pos = (pos.0 + ov.offset.0, pos.1 + ov.offset.1);
        // The anchor rect, for the render's transform origin (see `OverlayWindow::anchor_rect`).
        ov.anchor_rect = anchored.then_some((ax, ay, aw, ah));
        // Flight coordinate frame (Phase 4 Tier1): overlay canvas renders
        // translated by screen_pos — visuals store window-minus-origin.
        ov.composer.screen_origin = ov.screen_pos;
    }
    // Shared-element Tier0 polls (mirrors the main-tree post-layout poll —
    // overlay composers drive their own flights; Tier1 spans composers and
    // is driven by cross-poll in the main flow below).
    for ov in &mut pw.overlay.layers {
        ov.composer.poll_shared_flights();
    }
    // Overlay focus restore across recompositions (mirrors the main-tree
    // focused_slot_key restore in recompose_layout_render): arena node ids are
    // rebuilt, slot keys are stable.
    for ov in &mut pw.overlay.layers {
        let Some(slot) = ov.focused_slot_key else { continue; };
        let Some(r) = ov.composer.layout_root_idx() else { continue; };
        let nodes = ov.composer.arena_nodes_mut();
        if let Some(new_id) = crate::layout::node::find_node_id_by_slot_key(nodes, r, slot) {
            crate::layout::node::clear_focus(nodes, r);
            crate::layout::node::set_focus_by_id(nodes, r, new_id);
            ov.focused_id = Some(new_id);
        } else {
            ov.focused_id = None;
            ov.focused_slot_key = None;
        }
    }
}
