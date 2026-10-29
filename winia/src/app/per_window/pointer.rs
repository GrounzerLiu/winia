use super::*;

/// Fire the first on_click walking the hit path from inside out; returns whether one fired.
/// Shared by main-tree clicks and overlay clicks (no duplicated implementation).
/// Dual track (exp/modifier-node): the enum Clickable wins (the old behaviour), and with none
/// present it falls back to an open ClickNode (third-party custom clicks need no core change).
pub(crate) fn fire_click_along_path(nodes: &[crate::layout::node::LayoutNode], path: &[usize]) -> bool {
    for &i in path.iter().rev() {
        if let Some(cb) = nodes[i].modifier.on_click() {
            (cb)();
            return true;
        }
        if let Some(node_cb) = nodes[i].modifier.node_click() {
            node_cb.on_click();
            return true;
        }
    }
    false
}


/// Press: emit Press on the interaction source bound to the innermost clickable on the hit path
/// (Compose clickable's `PressInteraction.Press`).
pub(crate) fn press_interaction_down(pw: &mut PerWindow, path: &[usize], scene_pos: (f32, f32)) {
    let (slot, idx, src) = {
        let nodes = pw.composer.arena_nodes();
        let Some(&idx) = path.iter().rev().find(|&&i| {
            nodes[i].modifier.clickable_interaction().is_some()
                || nodes[i].modifier.node_click_interaction().is_some()
        }) else {
            return;
        };
        let src = match nodes[idx]
            .modifier
            .clickable_interaction()
            .cloned()
            .or_else(|| nodes[idx].modifier.node_click_interaction())
        {
            Some(s) => s,
            None => return,
        };
        (nodes[idx].slot_key, idx, src)
    };
    // Scene coordinates → node-local coordinates (Compose's `PressInteraction.Press.pressPosition`).
    // The ripple centre must be stored locally: drawing adds the layout origin back, so the ripple
    // follows the node through scrolling/transforms. Stored in scene coordinates it would detach
    // from the button once a scroll or animation happens after the press.
    let local = {
        let nodes = pw.composer.arena_nodes();
        let clickable_local =
            crate::layout::node::scene_to_node_local(nodes, path, idx, scene_pos.0, scene_pos.1);
        // When the Ripple and the clickable are different nodes (Switch's Handle container, say),
        // the press coordinates have to be converted into the Ripple node's local space, or the
        // ripple anchor is offset. The local-coordinate difference equals the absolute-position
        // difference of the two nodes (equivalent to the scene conversion when nothing scrolls or
        // transforms).
        let ripple_adjust = pw.composer.layout_root_idx().and_then(|r| {
            // Prefer locating it within the hit path (avoids a misplaced anchor when one source is
            // reused); Switch's ripple hangs on a Handle child node, which is not on the path when
            // the track's blank area is clicked — hence the whole-tree fallback. Known limitation:
            // when one source is reused by several components the whole-tree search may pick the
            // wrong node (every current Switch uses its own source, so it does not bite).
            let ridx = path
                .iter()
                .copied()
                .find(|&i| {
                    nodes[i]
                        .modifier
                        .ripple_interaction()
                        .map(|s| s == &src)
                        .unwrap_or(false)
                })
                .or_else(|| {
                    nodes.iter().position(|n| {
                        n.modifier
                            .ripple_interaction()
                            .map(|s| s == &src)
                            .unwrap_or(false)
                    })
                })?;
            if ridx == idx {
                return None;
            }
            let (ax, ay) = node_abs_position(nodes, r, nodes[idx].id);
            let (rx, ry) = node_abs_position(nodes, r, nodes[ridx].id);
            Some((ax - rx, ay - ry))
        });
        match ripple_adjust {
            Some((dx, dy)) => (clickable_local.0 + dx, clickable_local.1 + dy),
            None => clickable_local,
        }
    };
    src.emit_press_at(local);
    pw.pressed_interaction = Some((slot, src));
}

/// Release or cancel the press interaction (on Up or when the pointer leaves the slop) —
/// Compose's `PressInteraction.Release`/`Cancel`.
pub(crate) fn release_pressed_interaction(pw: &mut PerWindow) {
    if let Some((_, src)) = pw.pressed_interaction.take() {
        src.emit_release();
    }
    // Release the overlay press interactions too (an overlay has its own composer, hence its own
    // pressed state).
    for ov in &mut pw.overlay.layers {
        if let Some((_, src)) = ov.pressed_interaction.take() {
            src.emit_release();
        }
    }
}

/// Hover update: **every hoverable on the path** enters/leaves → emit Hover Enter/Exit
/// (Compose hoverable: each hoverable independently receives Enter/Exit; before the fix only the
/// innermost was emitted, so an outer nested hoverable — a Tooltip anchor container with a Button
/// inside — never got Enter and the Tooltip did not show). A removed node gets its Exit
/// automatically.
/// The cursor a hit path asks for: the deepest `pointer_hover_icon`, unless an ancestor with
/// `override_descendants` wins — Compose Desktop's `PointerHoverIconModifierNode` rule.
pub(crate) fn pointer_icon_for_path(
    nodes: &[LayoutNode],
    path: &[usize],
) -> crate::input::PointerIcon {
    let mut nearest: Option<crate::input::PointerIcon> = None;
    // Leaf first: the innermost icon is the nearest one, and the first override wins over
    // everything below it.
    for &idx in path.iter().rev() {
        for el in nodes[idx].modifier.elements().iter().rev() {
            if let crate::modifier::ModifierElement::PointerHoverIcon { icon, override_descendants } = el {
                if *override_descendants {
                    return *icon;
                }
                if nearest.is_none() {
                    nearest = Some(*icon);
                }
            }
        }
    }
    nearest.unwrap_or_default()
}

/// Apply a cursor to the window, skipping the platform call when it is already the current one.
pub(crate) fn update_cursor(pw: &mut PerWindow, icon: crate::input::PointerIcon) {
    if pw.cursor_icon == icon {
        return;
    }
    pw.cursor_icon = icon;
    if let Some(sw) = pw.skia_window.as_ref() {
        sw.set_cursor(winit::cursor::Cursor::Icon(icon.cursor_icon()));
    }
}

pub(crate) fn update_hover(pw: &mut PerWindow, scene_pos: (f32, f32)) {
    // Overlays first: when the pointer is over overlay content, update only that overlay's hover
    // and clear hover on the main tree and on **every other overlay** (an overlay covers what is
    // below it — matching real pointer stacking, so a lower button keeps no stale hover while
    // overlays are stacked).
    if let Some((i, local)) = hit_overlay(pw, scene_pos) {
        let ov = &mut pw.overlay.layers[i];
        overlay_update_hover(ov, local);
        // Clear all main-tree hover (the pointer is over an overlay, so the main tree is hidden).
        let olds: Vec<u64> = pw.hovered_slots.drain().collect();
        for slot in olds {
            exit_hover_at(pw, slot);
        }
        // Clear hover on the other (lower) overlays.
        for j in 0..pw.overlay.layers.len() {
            if j == i { continue; }
            let olds: Vec<u64> = pw.overlay.layers[j].hovered_slots.drain().collect();
            for slot in olds {
                overlay_exit_hover_at(&mut pw.overlay.layers[j], slot);
            }
        }
        let icon = {
            let ov = &pw.overlay.layers[i];
            match ov.composer.layout_root_idx() {
                Some(root) => {
                    let nodes = ov.composer.arena_nodes();
                    let path = hit_test_with_flights(
                        nodes,
                        root,
                        ov.composer.transition_roots(),
                        local.0,
                        local.1,
                    );
                    pointer_icon_for_path(nodes, &path)
                }
                None => crate::input::PointerIcon::DEFAULT,
            }
        };
        update_cursor(pw, icon);
        return;
    }
    // No overlay hit: update the main tree's hover and clear hover on all overlays (the pointer
    // left them, so their hover state is cleaned up).
    for ov in &mut pw.overlay.layers {
        let olds: Vec<u64> = ov.hovered_slots.drain().collect();
        for slot in olds {
            overlay_exit_hover_at(ov, slot);
        }
    }
    // (slot, interaction) for every hoverable on the current path.
    let (hit, icon): (Vec<(u64, crate::input::interaction::MutableInteractionSource)>, crate::input::PointerIcon) = {
        let nodes = pw.composer.arena_nodes();
        let Some(r) = pw.composer.layout_root_idx() else { return; };
        let path = hit_test_with_flights(nodes, r, pw.composer.transition_roots(), scene_pos.0, scene_pos.1);
        let hit = path.iter()
            .filter(|&&i| nodes[i].modifier.has_hoverable())
            .filter_map(|&i| nodes[i].modifier.hoverable_interaction().map(|s| (nodes[i].slot_key, s.clone())))
            .collect();
        (hit, pointer_icon_for_path(nodes, &path))
    };
    update_cursor(pw, icon);
    let hit_slots: std::collections::HashSet<u64> = hit.iter().map(|(s, _)| *s).collect();
    // Still hovering: skip (Enter already emitted).
    // Newly entered: emit Enter and record.
    for (slot, src) in &hit {
        if pw.hovered_slots.insert(*slot) {
            src.emit_hover_enter();
        }
    }
    // Left: emit Exit and drop the slot.
    let gone: Vec<u64> = pw.hovered_slots.iter().copied().filter(|s| !hit_slots.contains(s)).collect();
    for slot in gone {
        pw.hovered_slots.remove(&slot);
        exit_hover_at(pw, slot);
    }
}


/// Emit Hover Exit for the node with the given slot (skipped when the node is gone — the hover
/// state cleans itself up).
pub(crate) fn exit_hover_at(pw: &mut PerWindow, slot: u64) {
    let nodes = pw.composer.arena_nodes();
    let Some(r) = pw.composer.layout_root_idx() else { return; };
    if let Some(nid) = crate::layout::node::find_node_id_by_slot_key(nodes, r, slot) {
        if let Some(idx) = crate::layout::node::find_node_by_id(nodes, r, nid) {
            if let Some(src) = nodes[idx].modifier.hoverable_interaction() {
                src.emit_hover_exit();
            }
        }
    }
}

/// Compose-style click detection: Down records (`pointer_down_state`) and Up fires on_click on
/// release. Shared by the real PointerButton Up path and the debug simulation (d/u), so there is
/// no parallel implementation and the behaviour can be verified by simulation.
pub(crate) fn detect_click(pw: &mut PerWindow, scene_pos: (f32, f32)) -> bool {
    const CLICK_SLOP: f32 = 18.0;
    const CLICK_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(500);
    let Some(down) = pw.input.down.take() else {
        return false;
    };
    // slot_key of the node the Down landed on (stable across recomposition: one may happen between
    // Down and Up, e.g. triggered by focus, rebuilding arena nodes → node ids change → matching the
    // old id would always fail; a slot_key is stable by composition position).
    let down_slot = pw.input.down_slot;
    let dx = scene_pos.0 - down.position.0;
    let dy = scene_pos.1 - down.position.1;
    let dist = (dx * dx + dy * dy).sqrt();
    let in_time = down.time.elapsed() < CLICK_TIMEOUT;
    if dist <= CLICK_SLOP && in_time {
        let nodes = pw.composer.arena_nodes();
        if let Some(r) = pw.composer.layout_root_idx() {
            let path = hit_test_with_flights(nodes, r, pw.composer.transition_roots(), scene_pos.0, scene_pos.1);
            let hit = down_slot
                .map(|slot| path.iter().any(|&i| nodes[i].slot_key == slot))
                .unwrap_or_else(|| path.iter().any(|&i| nodes[i].id == down.node_id));
            if hit {
                // Only fire the click on the same node (the first on_click from inside out).
                return fire_click_along_path(nodes, &path);
            }
        }
    }
    false
}

/// Dispatch a key event along the focus path (Preview: root→focused; Bubble: focused→root,
/// matching `onPreviewKeyEvent`/`onKeyEvent`) plus focused-component activation (Enter/Space
/// triggers onClick — only the focused node's own clickable).
///
/// Shared by the real KeyboardInput path and the debug simulation (so the two cannot diverge):
/// framework-level keys such as Escape/Tab are handled by the caller beforehand (focus navigation,
/// clearing focus) and never reach this function.

/// Pointer-down core (shared by the real PointerButton path and the debug simulation, so they
/// cannot diverge).
///
/// One path: hit_test → clear the old selection → grapheme anchor placement → record
/// `PtrDownState` → cursor/auto-focus (real path only, `with_focus`) → dispatch
/// `on_pointer_event`. Returns true when a node was hit and handled.
pub(crate) fn handle_pointer_down(
    pw: &mut PerWindow,
    scene_pos: (f32, f32),
    kind: crate::input::PointerKind,
    modifiers: &winit::keyboard::ModifiersState,
    with_focus: bool,
) -> bool {
    // Top-level popups first: an overlay hit records the click target (the event does not reach the
    // main tree); an outside click dismisses (modal / dismissible).
    if overlay_down(pw, scene_pos, kind.clone()) {
        return true;
    }
    let nodes = pw.composer.arena_nodes();
    let Some(r) = pw.composer.layout_root_idx() else { return false; };
    let path = hit_test_with_flights(nodes, r, pw.composer.transition_roots(), scene_pos.0, scene_pos.1);
    let Some(&innermost) = path.last() else { return false; };

    // Clear the old selection (a new click begins).
    {
        if let Some(reg) = nodes[innermost].registrar.borrow().as_ref().cloned() {
            reg.clear_selection();
        }
    }
    // Grapheme anchor placement.
    // ⚠ With a containerized TextField the paragraph is cached only on the **input leaf**, so a
    // container node has no `cached_paragraph` — clicking the container's blank area hits the
    // container, and the innermost node has no para. `find_anchor_text_node` handles it: search
    // upward for an ancestor with a paragraph (the hit text itself), and when that fails and the
    // hit is a TextField container, search downward for the input leaf in the subtree.
    let anchor_node = find_anchor_text_node(nodes, &path, innermost);
    let anchor = anchor_node.and_then(|ai| {
        let borrow = nodes[ai].cached_paragraph.try_borrow().ok()?;
        borrow.as_ref().map(|para| {
            let (ax, ay) = node_abs_position(nodes, r, nodes[ai].id);
            // The paragraph-local coordinate has to subtract the node's padding (the renderer draws
            // text in the content area = node origin + padding; without the subtraction every click
            // position is off by the padding and the cursor jumps to the wrong place).
            let (pad_s, pad_t, pad_e, _) = nodes[ai].modifier.get_padding_sides();
            let pad_x = if nodes[ai].layout_direction == crate::layout::LayoutDirection::Rtl { pad_e } else { pad_s };
            let tl = crate::text::TextLayout::new(para);
            // A `textGeometricTransform` widens/leans the GLYPH run, and the paragraph is laid out in
            // its own space, so the pointer has to be mapped back into that space before the hit
            // test (`unmap_x` is the inverse of the paint matrix).
            let local = (
                scene_pos.0 - ax - pad_x,
                scene_pos.1 - ay - pad_t,
            );
            let local = match crate::layout::node::text_transform(&nodes[ai]) {
                Some(t) => (t.unmap_x(local.0, local.1), local.1),
                None => local,
            };
            // A `LineHeightStyle` also moves the glyphs inside their box: take the paint offset back
            // out before asking the paragraph which cluster is under the pointer.
            let line_dy = crate::layout::node::line_geometry(&nodes[ai], para).paint_dy;
            let local = (local.0, local.1 - line_dy);
            let hit = tl.get_closest_grapheme_cluster_at(skia_safe::Point::new(local.0, local.1));
            // The result is a display-text offset (the paragraph holds the displayed text), so it
            // goes back through the OffsetMapping to an editing offset (password masking / formatted
            // input).
            // ⚠ `TextFieldVisual` sits on the **container**, so it must be looked up upward
            // (`offset_mapping_for_node`).
            crate::text::field::offset_mapping_for_node(nodes, r, ai)
                .map(|m| m.transformed_to_original(hit))
                .unwrap_or(hit)
        })
    });
    // Use only the anchor node's own registrar (never fall back to the active registrar): a
    // non-selectable node (an unregistered output Text, say) has `node.registrar == None`, and a
    // fallback would pick up a global leftover (Container B's, say) → `anchor_registrar` bound to
    // the wrong container → dragging to B gives `same_reg = true` → mixed offsets → B gets
    // selected.
    let own_reg = anchor_node.and_then(|ai| nodes[ai].registrar.borrow().as_ref().cloned());
    let anchor_global = anchor.and_then(|a| {
        anchor_node.and_then(|ai| {
            own_reg.as_ref()
                .and_then(|reg| reg.segment_info(nodes[ai].slot_key).map(|(off, _)| off + a))
        })
    });
    pw.input.down = Some(PtrDownState {
        node_id: nodes[innermost].id,
        position: scene_pos,
        time: std::time::Instant::now(),
        selection_anchor: anchor_global,
        anchor_registrar: own_reg, // None (a non-selectable node) → no anchor container.
    });
    pw.input.down_slot = Some(nodes[innermost].slot_key);

    if with_focus {
        // Set the TextField cursor position and run the selection callback (real path only).
        // ⚠ Click-to-focus is the component's own decision (TextField requests focus internally) —
        // the framework never auto-focuses (Compose: clicking a clickable/focusable does not request
        // focus; focus comes from Tab navigation or an explicit requestFocus).
        // The cursor is set on the anchor node (the input leaf) — both the cursor drawing and the
        // callback live there.
        if let (Some(a), Some(ai)) = (anchor, anchor_node) {
            nodes[ai].cursor_index.set(a);
            if let Some(cb) = nodes[ai].cursor_callback.borrow_mut().as_mut() {
                cb(a);
            }
        }
    }

    // Press interaction (the clickable's bound source — Compose Press semantics; placed after the
    // `nodes` borrow ends).
    press_interaction_down(pw, &path, scene_pos);

    // Gesture entry (on_press fires immediately; later move/up are routed by the gesture node) —
    // placed after the `with_focus` block (the `nodes` borrow has ended, so it does not conflict
    // with the `pw` mutable borrow).
    gesture_down(pw, scene_pos);

    // Drag-scroll target: from the press point, walk up to the nearest scroll container. Text
    // selection and a component's drag gesture take priority (dragging a selection or a component
    // does not scroll — Compose lets the innermost `pointerInput` consume it).
    pw.input.drag_scroll = (|| {
        let nodes = pw.composer.arena_nodes();
        let Some(r) = pw.composer.layout_root_idx() else { return None };
        let path = hit_test_with_flights(nodes, r, pw.composer.transition_roots(), scene_pos.0, scene_pos.1);
        #[cfg(debug_assertions)]
        if drag_trace_enabled() {
            eprintln!("[drag-down] scene=({},{}) path_len={} path={:?}",
                scene_pos.0, scene_pos.1, path.len(),
                path.iter().map(|&i| (i, nodes[i].slot_key, nodes[i].modifier.vertical_scroll_state().is_some())).collect::<Vec<_>>());
            for &i in &path {
                let n = &nodes[i];
                eprintln!("  [drag-node] idx={} pos=({},{}) size=({},{}) vp_h={} scroll={}",
                    i, n.position.x, n.position.y, n.measured_size.width, n.measured_size.height,
                    n.scroll_viewport_height,
                    n.modifier.vertical_scroll_state().is_some());
            }
        }
        let Some(&_innermost) = path.last() else { return None };
        let selecting = pw.input.down.as_ref()
            .map(|s| s.selection_anchor.is_some())
            .unwrap_or(false);
        // Inner gestures first, but **scrolling outranks an outer panel's drag** (Compose: a
        // BottomSheet panel is draggable as a whole, yet a press inside the list area scrolls the
        // content first — the panel's on_drag is the outer fallback). `child_drag` holds only when
        // the drag node is deeper than the scroll node (an inner component such as a slider/switch);
        // when the drag is the scroll's ancestor (a panel), scrolling wins and child_drag is not
        // granted.
        let drag_idx = path_drag_idx(nodes, &path);
        let scroll_idx = path_scroll_idx(nodes, &path);
        let child_drag = inner_component_drag(drag_idx, scroll_idx);
        // An inner drag component and the scroll it sits in both want this gesture; which one gets it
        // is decided by the finger's dominant axis in `gesture_move`, so record the scroll as the
        // fallback owner here. `None` when the component stands alone (it owns every direction), and
        // `None` while a text selection is in progress: the selection keeps the pointer (it is
        // extended from every move below), so the list must not start scrolling under the finger.
        pw.input.scroll_slot = if child_drag && !selecting {
            scroll_idx.map(|i| nodes[i].slot_key)
        } else {
            None
        };
        if selecting || child_drag {
            None
        } else {
            scroll_idx.map(|i| DragScroll::new(nodes[i].slot_key, scene_pos))
        }
    })();

    // Dispatch on_pointer_event (Down).
    let nodes = pw.composer.arena_nodes();
    let ptr_ev = crate::input::PointerEvent {
        event_type: crate::input::PointerEventType::Down,
        position: (0.0, 0.0),
        scene_position: scene_pos,
        kind,
        is_alt_pressed: modifiers.alt_key(),
        is_ctrl_pressed: modifiers.control_key(),
        is_shift_pressed: modifiers.shift_key(),
        is_meta_pressed: modifiers.meta_key(),
    };
    pw.input.last_kind = ptr_ev.kind.clone();
    dispatch_ptr_event(nodes, r, &path, &ptr_ev, scene_pos, pw.input.down_slot);
    true
}

/// Pointer-move core (shared by the real PointerMoved path and the debug simulation, so they
/// cannot diverge).
///
/// One path: drag-selection (18px slop + `x_off` alignment shift + grapheme placement +
/// `compute_selection`) plus `on_pointer_event` dispatch. Returns whether the move was consumed
/// (the selection changed or a handler consumed it).
pub(crate) fn handle_pointer_move(
    pw: &mut PerWindow,
    scene_pos: (f32, f32),
    kind: crate::input::PointerKind,
    modifiers: &winit::keyboard::ModifiersState,
) -> bool {
    // Gesture drive (drag capture: route whenever a tracker exists — the pointer keeps receiving
    // moves even outside the component).
    // ⚠ Must run before the `nodes` borrow (`gesture_move` takes `nodes` and a mutable `pw`
    // itself).
    let mut handled = false;
    if pw.input.node.is_some() && gesture_move(pw, scene_pos) {
        handled = true;
    }
    // Cancel an overlay click past the slop: dragging 18px away after pressing on the overlay
    // cancels the click (matching the main tree's `detect_click` slop rule; the down position is
    // the overlay's `screen_pos` plus the local offset).
    if pw.overlay.click.is_some() {
        let cancel = match pw.overlay.click {
            Some((idx, local, _)) => {
                match pw.overlay.layers.get(idx) {
                    Some(ov) => {
                        let down_screen = (ov.screen_pos.0 + local.0, ov.screen_pos.1 + local.1);
                        let dx = scene_pos.0 - down_screen.0;
                        let dy = scene_pos.1 - down_screen.1;
                        (dx * dx + dy * dy).sqrt() > 18.0
                    }
                    None => false,
                }
            }
            None => false,
        };
        if cancel {
            pw.overlay.click = None;
            release_pressed_interaction(pw);
        }
    }
    if let Some((idx, slot, down_pos)) = pw.overlay.drag {
        let (dx, dy) = (scene_pos.0 - down_pos.0, scene_pos.1 - down_pos.1);
        if !pw.overlay.drag_started {
            if dx * dx + dy * dy >= 8.0 * 8.0 {
                pw.overlay.drag_started = true;
                pw.overlay.drag_last = Some(scene_pos);
                if let Some(ov) = pw.overlay.layers.get(idx) {
                    let nodes = ov.composer.arena_nodes();
                    if let Some(r) = ov.composer.layout_root_idx() {
                        // ⚠ The overlay arena is LAYER-local, and a gesture callback receives
                        // node-local coordinates (`fire_gesture_action` subtracts the node's
                        // position inside the arena it was given). So the absolute position has to
                        // be converted here — passing `scene_pos` shifted it by the popup's screen
                        // origin, which is why dragging a Slider inside a popup landed on the wrong
                        // value (deltas are unaffected: the layer offset cancels in a difference).
                        let local = (
                            scene_pos.0 - pw.overlay.drag_origin.0,
                            scene_pos.1 - pw.overlay.drag_origin.1,
                        );
                        fire_gesture_action(nodes, r, slot,
                            crate::input::gesture::GestureAction::DragStart(local));
                        fire_gesture_action(nodes, r, slot,
                            crate::input::gesture::GestureAction::DragMove(local, (dx, dy)));
                    }
                }
                handled = true;
            }
        } else {
            let last = pw.overlay.drag_last.unwrap_or(down_pos);
            let inc = (scene_pos.0 - last.0, scene_pos.1 - last.1);
            pw.overlay.drag_last = Some(scene_pos);
            if let Some(ov) = pw.overlay.layers.get(idx) {
                let nodes = ov.composer.arena_nodes();
                if let Some(r) = ov.composer.layout_root_idx() {
                    let local = (
                        scene_pos.0 - pw.overlay.drag_origin.0,
                        scene_pos.1 - pw.overlay.drag_origin.1,
                    );
                    fire_gesture_action(nodes, r, slot,
                        crate::input::gesture::GestureAction::DragMove(local, inc));
                }
            }
            handled = true;
        }
    }
    // Drag-scrolling inside an overlay (its own Composer — a LazyColumn inside a BottomSheet).
    // List scrolling goes through the overlay Composer's `dispatch_nested_scroll_delta`, and the
    // Sheet consumes the remaining available delta through `SheetNested` (nested_scroll) on the
    // panel. This branch consumes only the list; whether the Sheet can also be dragged is up to
    // the nested-scroll chain.
    if pw.overlay.drag_scroll.is_some() {
        let target: Option<usize> = (|| {
            let ov_idx = pw.overlay.layers.len().checked_sub(1).unwrap_or(0);
            let slot = pw.overlay.drag_scroll.as_ref().unwrap().slot;
            let ov = pw.overlay.layers.get(ov_idx)?;
            let r = ov.composer.layout_root_idx()?;
            let nodes = ov.composer.arena_nodes();
            let id = crate::layout::node::find_node_id_by_slot_key(nodes, r, slot)?;
            crate::layout::node::find_node_by_id(nodes, r, id)
        })();
        let (dx, dy) = {
            let ds = pw.overlay.drag_scroll.as_mut().unwrap();
            let dx = scene_pos.0 - ds.last_x;
            let dy = scene_pos.1 - ds.last_y;
            ds.last_x = scene_pos.0;
            ds.last_y = scene_pos.1;
            let now = std::time::Instant::now();
            ds.samples.push((now, scene_pos.0, scene_pos.1));
            let cutoff = now - std::time::Duration::from_millis(200);
            ds.samples.retain(|(t, _, _)| *t >= cutoff);
            (dx, dy)
        };
        if let Some(idx) = target {
            let (ax, ay) = {
                let ov_idx = pw.overlay.layers.len().checked_sub(1).unwrap_or(0);
                let ov = &pw.overlay.layers[ov_idx];
                let nodes = ov.composer.arena_nodes();
                if nodes[idx].modifier.vertical_scroll_state().is_some() { (0.0, dy) }
                else if nodes[idx].modifier.horizontal_scroll_state().is_some() { (dx, 0.0) }
                else { (0.0, 0.0) }
            };
            if ax != 0.0 || ay != 0.0 {
                let ov_idx = pw.overlay.layers.len().checked_sub(1).unwrap_or(0);
                if let Some(ov) = pw.overlay.layers.get_mut(ov_idx) {
                    if let Some(root) = ov.composer.layout_root_idx() {
                        let density = crate::unit::Density::from_density(pw.scale_factor as f32);
                        let consumed = dispatch_nested_scroll_delta(ov.composer.arena_nodes_mut(), root, idx, crate::nested_scroll::ScrollDelta::new(ax, ay), crate::nested_scroll::NestedScrollSource::Drag, density);
                        if consumed.x != 0.0 || consumed.y != 0.0 {
                            handled = true;
                        }
                    }
                }
            }
            let ov_idx = pw.overlay.layers.len().checked_sub(1).unwrap_or(0);
            if let Some(ov) = pw.overlay.layers.get(ov_idx) {
                let nodes = ov.composer.arena_nodes();
                if let Some(ss) = nodes[idx].modifier.vertical_scroll_state() { ss.is_scroll_in_progress.set(true); }
                else if let Some(ss) = nodes[idx].modifier.horizontal_scroll_state() { ss.is_scroll_in_progress.set(true); }
            }
        }
    }
    // Drag-scrolling: the content follows the pointer and velocity samples are recorded (for the
    // fling on release). Placed before gesture/text selection — but the press already excluded a
    // component drag gesture and a text selection, so nothing conflicts here.
    if pw.input.drag_scroll.is_some() {
        let target: Option<usize> = (|| {
            let nodes = pw.composer.arena_nodes();
            let Some(r) = pw.composer.layout_root_idx() else { return None };
            let slot = pw.input.drag_scroll.as_ref().unwrap().slot;
            let id = crate::layout::node::find_node_id_by_slot_key(nodes, r, slot)?;
            crate::layout::node::find_node_by_id(nodes, r, id)
        })();
        let (dx, dy) = {
            let ds = pw.input.drag_scroll.as_mut().unwrap();
            let dx = scene_pos.0 - ds.last_x;
            let dy = scene_pos.1 - ds.last_y;
            ds.last_x = scene_pos.0;
            ds.last_y = scene_pos.1;
            let now = std::time::Instant::now();
            ds.samples.push((now, scene_pos.0, scene_pos.1));
            let cutoff = now - std::time::Duration::from_millis(200);
            ds.samples.retain(|(t, _, _)| *t >= cutoff);
            (dx, dy)
        };
        if let Some(idx) = target {
            #[cfg(debug_assertions)]
            if drag_trace_enabled() {
                let nodes = pw.composer.arena_nodes();
                eprintln!("[drag-move] target_idx={} scroll_v={} scroll_h={} slot={:?}",
                    idx,
                    nodes[idx].modifier.vertical_scroll_state().is_some(),
                    nodes[idx].modifier.horizontal_scroll_state().is_some(),
                    pw.input.drag_scroll.as_ref().map(|d| d.slot));
            }
            // Axis-aware: a vertical container takes dy and a horizontal one takes dx
            // (`apply_scroll_delta` picks by the node's axis).
            let (ax, ay) = {
                let nodes = pw.composer.arena_nodes();
                if nodes[idx].modifier.vertical_scroll_state().is_some() {
                    (0.0, dy)
                } else if nodes[idx].modifier.horizontal_scroll_state().is_some() {
                    (dx, 0.0)
                } else {
                    (0.0, 0.0)
                }
            };
            if ax != 0.0 || ay != 0.0 {
                let root = pw.composer.layout_root_idx();
                let consumed = root.map(|root| dispatch_nested_scroll_delta(
                    pw.composer.arena_nodes_mut(),
                    root,
                    idx,
                    crate::nested_scroll::ScrollDelta::new(ax, ay),
                    crate::nested_scroll::NestedScrollSource::Drag,
                    crate::unit::Density::from_density(pw.scale_factor as f32),
                )).unwrap_or(crate::nested_scroll::ScrollDelta::ZERO);
                handled = consumed.x != 0.0 || consumed.y != 0.0;
            }
            // Mark the drag as in progress (`apply_scroll_delta` sets this false when it cancels a
            // fling — this assignment overrides that).
            let nodes = pw.composer.arena_nodes();
            if let Some(ss) = nodes[idx].modifier.vertical_scroll_state() {
                ss.is_scroll_in_progress.set(true);
            } else if let Some(ss) = nodes[idx].modifier.horizontal_scroll_state() {
                ss.is_scroll_in_progress.set(true);
            }
        }
    }
    // Hover update (does its own hit test — it does not depend on the `nodes` borrow below).
    update_hover(pw, scene_pos);
    // Past the slop: cancel the press interaction (Compose: a press moving beyond the touch slop
    // is cancelled).
    if pw.input.down.is_some() {
        let down = pw.input.down.as_ref().unwrap();
        let dx = scene_pos.0 - down.position.0;
        let dy = scene_pos.1 - down.position.1;
        if (dx * dx + dy * dy).sqrt() > 18.0 {
            release_pressed_interaction(pw);
            // Cancel the overlay click too (matching the main tree's `detect_click` slop rule — once
            // dragged past the slop, Up must not fire an overlay click).
            pw.overlay.click = None;
        }
    }
    let nodes = pw.composer.arena_nodes();
    let Some(r) = pw.composer.layout_root_idx() else { return false; };
    let path = hit_test_with_flights(nodes, r, pw.composer.transition_roots(), scene_pos.0, scene_pos.1);

    // Drag-select text.
    if pw.input.down.is_some() {
        if let Some(&innermost) = path.last() {
            let down = pw.input.down.as_ref().unwrap();
            let dx = scene_pos.0 - down.position.0;
            let dy = scene_pos.1 - down.position.1;
            const CLICK_SLOP: f32 = 18.0;
            // Text selection: pressing on selectable text (an anchor exists) → **every move updates
            // the selection** (a single letter is ~7px < CLICK_SLOP, so the slop threshold made it
            // impossible to select one letter or a small range; Compose enters drag-selection on
            // press). Every other case keeps the slop rule (clickable / drag gesture).
            let text_selection = down.selection_anchor.is_some();
            if text_selection || (dx * dx + dy * dy).sqrt() > CLICK_SLOP {
                let (abs_x, abs_y) = node_abs_position(nodes, r, nodes[innermost].id);
                if let Ok(borrow) = nodes[innermost].cached_paragraph.try_borrow() {
                    if let Some(para) = borrow.as_ref() {
                        // Alignment shift (matching the renderer's `x_off`) — drawing uses the content
                        // box, and the selection highlight has to agree (mid-flight `measured_size` is
                        // the size the parent was told, review R3-F6).
                        let node_w = nodes[innermost].content_box().width;
                        let align = nodes[innermost].modifier.align().unwrap_or(crate::text::TextAlign::Left);
                        // The slack rule has to be the PAINT paths' rule, not a copy of the old
                        // one: a WRAPPED paragraph was laid out inside its box, so a negative
                        // slack would push it out of it — but an UNWRAPPED one
                        // (`soft_wrap(false)`, laid out at its intrinsic width) can be WIDER than
                        // the box, and then centring or right-aligning has to shift it the other
                        // way. With the clamped form here, a selection on an overflowing label
                        // was skewed by half the overflow (`render.rs` uses the conditional one
                        // at both of its sites).
                        let soft_wrap = nodes[innermost]
                            .modifier
                            .elements()
                            .iter()
                            .find_map(|el| match el {
                                crate::modifier::ModifierElement::TextContent { soft_wrap, .. } => Some(*soft_wrap),
                                _ => None,
                            })
                            .unwrap_or(true);
                        let slack = if soft_wrap {
                            (node_w - para.max_intrinsic_width()).max(0.0)
                        } else {
                            node_w - para.max_intrinsic_width()
                        };
                        let x_off = match align {
                            crate::text::TextAlign::Center => abs_x + slack / 2.0,
                            crate::text::TextAlign::Right => abs_x + slack,
                            _ => abs_x,
                        };
                        // Paragraph-local coordinate: subtract the node's padding (consistent with the
                        // renderer's content area — otherwise the dragged selection is off by the
                        // padding).
                        let (pad_s, pad_t, pad_e, _) = nodes[innermost].modifier.get_padding_sides();
                        let pad_x = if nodes[innermost].layout_direction == crate::layout::LayoutDirection::Rtl { pad_e } else { pad_s };
                        let tl = crate::text::TextLayout::new(para);
                        {
                            let down = pw.input.down.as_ref().unwrap();
                            // A non-selectable node (not registered with any SelectionContainer) → do not
                            // update the selection (wrapped in `if let` rather than `else return`, because a
                            // return would skip `dispatch_ptr_event`/`request_redraw`).
                            if let Some(reg) = nodes[innermost].registrar.borrow().as_ref().cloned() {
                                let current_index = tl.get_closest_grapheme_cluster_at(
                                    skia_safe::Point::new(scene_pos.0 - x_off - pad_x, scene_pos.1 - abs_y - pad_t));
                                // Display offset → editing offset (password masking / formatted input;
                                // `TextFieldVisual` is on the container, so look upward).
                                let current_index = crate::text::field::offset_mapping_for_node(nodes, r, innermost)
                                    .map(|m| m.transformed_to_original(current_index))
                                    .unwrap_or(current_index);
                                let cur_off = reg.segment_info(nodes[innermost].slot_key).map(|(off, _)| off);
                                if let Some((target, s, e)) = crate::components::selection_container::compute_selection(
                                    down.anchor_registrar.as_ref(), down.selection_anchor,
                                    &reg, cur_off, current_index,
                                    scene_pos.1, down.position.1, abs_y,
                                ) {
                                    // The range is in editing offsets (anchor/current were converted back);
                                    // the registrar's space is display offsets, so convert before writing the
                                    // selection.
                                    let (ts, te) = crate::text::field::offset_mapping_for_node(nodes, r, innermost)
                                        .map(|m| (m.original_to_transformed(s), m.original_to_transformed(e)))
                                        .unwrap_or((s, e));
                                    target.set_selection(ts, te);
                                    handled = true;
                                }
                            } // end if let Some(reg)
                        }
                    }
                }
            }
        }
    }

    // Dispatch on_pointer_event (even a hover Move can update State).
    let nodes = pw.composer.arena_nodes();
    let ptr_ev = crate::input::PointerEvent {
        event_type: crate::input::PointerEventType::Move,
        position: (0.0, 0.0),
        scene_position: scene_pos,
        kind,
        is_alt_pressed: modifiers.alt_key(),
        is_ctrl_pressed: modifiers.control_key(),
        is_shift_pressed: modifiers.shift_key(),
        is_meta_pressed: modifiers.meta_key(),
    };
    pw.input.last_kind = ptr_ev.kind.clone();
    let consumed = dispatch_ptr_event(nodes, r, &path, &ptr_ev, scene_pos, pw.input.down_slot);
    handled || consumed
}

