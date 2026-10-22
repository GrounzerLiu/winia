use super::*;

impl AppState {
    /// Consume events injected by DevTools (clicks, keys, scrolls, drags — UI tests and WS debugging).
    /// Called from two places, window_event (RedrawRequested) and new_events (as a fallback): with several
    /// windows the main one may be in the background, and then RedrawRequested never arrives and the events
    /// sit in the queue.
    /// Perform the actions a platform bridge (or anything else off the UI thread) queued for this
    /// window. Invoke runs the node's real click callback; Focus moves the ring to the node.
    pub(crate) fn consume_ui_actions(&mut self, window_id: WindowId) {
        let wid = window_id.into_raw() as u64;
        let actions = crate::semantics::take_actions(wid);
        if actions.is_empty() {
            return;
        }
        let Some(pw) = self.windows.get_mut(&window_id) else { return };
        for action in actions {
            match action {
                crate::semantics::UiAction::Invoke(node_id) => {
                    // The arena is borrowed immutably and the callback only sets state, which is what
                    // a real click does too.
                    let nodes = pw.composer.arena_nodes();
                    if !crate::semantics::click_node_by_id(nodes, node_id) {
                        log::debug!("[accessibility] invoke: node {node_id} has no click action");
                    }
                }
                crate::semantics::UiAction::Focus(node_id) => {
                    if !pw.focus_by_node_id(node_id) {
                        log::debug!("[accessibility] focus: node {node_id} cannot take focus");
                    }
                }
            }
        }
        if let Some(ref sw) = pw.skia_window {
            sw.request_redraw();
        }
        debug::wake();
    }

    /// Runs on every event batch from new_events, so an injected event is never lost.
    pub(crate) fn consume_debug_events(&mut self, window_id: WindowId) {
        let Some(pw) = self.windows.get_mut(&window_id) else { return };
        let mut handled = false;
        for evt in debug::take_queued_events(window_id.into_raw() as u64) {
            match evt {
                debug::DebugEvent::Click { x, y } => {
                    // Overlay first (mirroring the real pointer path, handle_pointer_down):
                    // a hit on an overlay is consumed there (it never reaches the main tree); a click
                    // outside dismisses it.
                    // NOTE: Click is a synthesized single event (no separate down/up) — once the
                    // overlay is hit the click has to run immediately (the real path records on down
                    // and fires on up; here we record on down and exec straight away, otherwise an
                    // overlay button could never be clicked).
                    let click_kind = pw.input.last_kind.clone();
                    if overlay_down(pw, (x, y), click_kind) {
                        // Overlay hit: run the click now (synthesized single event — record on
                        // down and fire on up immediately; the real path fires on PointerUp).
                        exec_overlay_click(pw);
                        // The synthetic click is down + up in one event, so it has to close the
                        // gesture the down opened, exactly as the real up path does
                        // (`exec_overlay_click` → `gesture_up`): otherwise a tap in a popup would
                        // leave its tracker (and any deferred tap) behind, and `on_tap` would never
                        // fire for a harness click.
                        gesture_up(pw, (x, y));
                        // Release the overlay's press (a synthesized event has no real up — otherwise
                        // the ripple/pressed state stays stuck until the overlay closes).
                        release_pressed_interaction(pw);
                        if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                        handled = true;
                        continue;
                    }
                    // Read-only phase: hit_test + click detection (the arena borrow ends at the block's end).
                    let (fid, sk, _path_len, _click_handled) = {
                        let arena = pw.composer.arena_nodes();
                        let root_idx = pw.composer.layout_root_idx();
                        if let Some(r) = root_idx {
                            let path = hit_test_with_flights(arena, r, pw.composer.transition_roots(), x, y);
                            // Debug click focus: the deepest focusable node on the path
                            // (after the text-field-v2 containerisation focus/interaction live on the
                            // container — a child of an empty field is 0 px wide and cannot be hit; the
                            // old logic only looked at ime_callback and no longer works); also matches
                            // nodes that declare IME (the old leaf semantics).
                            // NOTE: deliberately BROADER than the real overlay path
                            // (`overlay_down`, which focuses only a field that holds an input):
                            // a synthesized click has to be able to put focus anywhere, because
                            // that is how a following `k <char>` finds a target. Do not narrow
                            // this to match — see `clicking_an_overlay_button_does_not_steal_focus`.
                            let (fid, sk) = path.iter().rev().find_map(|&i| {
                                let has_focusable = arena[i].modifier.elements().iter()
                                    .any(|el| matches!(el, crate::modifier::ModifierElement::Focusable { .. }));
                                let wants_ime = arena[i].ime_callback.borrow().is_some();
                                (has_focusable || wants_ime).then_some((arena[i].id, arena[i].slot_key))
                            }).unwrap_or((0, 0));
                            let path_len = path.len();
                            // Click detection (P2-1: reuses fire_click_along_path, with the same
                            // semantics as the main tree and overlays, so an inline copy cannot drift).
                            let click_handled = fire_click_along_path(arena, &path);
                            if click_handled {
                                handled = true;
                            }
                            (fid, sk, path_len, click_handled)
                        } else { (0, 0, 0, false) }
                    };
                    if fid != 0 {
                        if let Some(r) = pw.composer.layout_root_idx() {
                            let nodes = pw.composer.arena_nodes_mut();
                            crate::layout::node::clear_focus(nodes, r);
                            crate::layout::node::set_focus_by_id(nodes, r, fid);
                        }
                        pw.focused_id = Some(fid);
                        pw.focused_slot_key = Some(sk);
                        if let Some(ref sw) = pw.skia_window { set_ime_enabled(&sw, true); }
                    }
                    debug_log!("[debug-click] pos=({:.0},{:.0}) path_len={} sf={}", x, y, _path_len, pw.scale_factor);
                    debug_log!("[debug-click] handled={} pos=({:.0},{:.0})", _click_handled, x, y);
                }
                debug::DebugEvent::Key { key } => {
                    if key == "Escape" {
                        // The same helper the real key path calls — see `PerWindow::escape_key`.
                        pw.escape_key();
                        if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    } else if key == "Tab" {
                        // The SAME helper the real key path calls: this branch used to keep its own
                        // copy of the traversal rule, which silently went stale the moment the rule
                        // changed (it still walked the main tree while an overlay owned the keyboard,
                        // and a UI test driving Tab over the debug server would have measured that
                        // copy instead of the framework). Shift is not expressible in `k <key>`.
                        pw.tab_move_focus(false);
                    } else if let Some(k) = parse_debug_key(&key) {
                        // Any key: reuse the real keyboard dispatch path (Preview/Bubble/activation) —
                        // so WS can simulate character input, deletion and arrow keys. Modifiers default
                        // to none (Ctrl and other combinations are not supported yet — add modifier
                        // fields to KbEvent if needed).
                        let ke = crate::input::KbEvent {
                            key: k,
                            event_type: crate::input::KbEventType::KeyDown,
                            is_alt_pressed: pw.modifiers.alt_key(),
                            is_ctrl_pressed: pw.modifiers.control_key(),
                            is_shift_pressed: pw.modifiers.shift_key(),
                            is_meta_pressed: pw.modifiers.meta_key(),
                            repeat: false,
                        };
                        if !dispatch_key_to_overlay(pw, &ke) && !focus_scope_is_open(pw) {
                            // Same gate as the real key path: a focus-scope overlay owns the keyboard,
                            // so a key it does not consume does not reach the page behind it either.
                            dispatch_key_to_focus(pw, &ke);
                        }
                    }
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    handled = true;
                }
                debug::DebugEvent::FocusNext => {
                    // The real traversal rule, not a copy: this arm used to walk the main tree only.
                    pw.tab_move_focus(false);
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    handled = true;
                }
                debug::DebugEvent::RequestFocus { id } => {
                    if let Some(r) = pw.composer.layout_root_idx() {
                        let nodes = pw.composer.arena_nodes_mut();
                        if crate::layout::node::focus_by_id(nodes, r, id) {
                            pw.focused_id = crate::layout::node::get_focus_id(nodes, r);
                            pw.focused_slot_key = pw.focused_id.and_then(|fid| crate::layout::node::find_node_by_id(nodes, r, fid).map(|idx| nodes[idx].slot_key));
                            pw.apply_ime_for_focus(pw.focused_id);
                            handled = true;
                        }
                    }
                    if !handled {
                        // Main tree missed — try overlays topmost-first (same
                        // single-focus clearing as the frame request path).
                        for i in (0..pw.overlay.layers.len()).rev() {
                            let hit = {
                                let ov = &mut pw.overlay.layers[i];
                                if ov.closing {
                                    false
                                } else if let Some(r) = ov.composer.layout_root_idx() {
                                    crate::layout::node::focus_by_id(ov.composer.arena_nodes_mut(), r, id)
                                } else {
                                    false
                                }
                            };
                            if hit {
                                let (fid, fslot, want) = {
                                    let ov = &pw.overlay.layers[i];
                                    let nodes = ov.composer.arena_nodes();
                                    let r = ov.composer.layout_root_idx();
                                    let fid = r.and_then(|r| crate::layout::node::get_focus_id(nodes, r));
                                    let (fslot, want) = fid.and_then(|f| {
                                        r.and_then(|r| crate::layout::node::find_node_by_id(nodes, r, f))
                                    }).map(|found| {
                                        (nodes[found].slot_key, node_or_descendant_wants_ime(nodes, found))
                                    }).unwrap_or((0, false));
                                    (fid, fslot, want)
                                };
                                pw.overlay.layers[i].focused_id = fid;
                                pw.overlay.layers[i].focused_slot_key = fid.map(|_| fslot);
                                if pw.focused_id.is_some() {
                                    if let Some(r) = pw.composer.layout_root_idx() {
                                        crate::layout::node::clear_focus(pw.composer.arena_nodes_mut(), r);
                                    }
                                    pw.focused_id = None;
                                    pw.focused_slot_key = None;
                                }
                                for (j, other) in pw.overlay.layers.iter_mut().enumerate() {
                                    if j != i && other.focused_id.is_some() {
                                        if let Some(r) = other.composer.layout_root_idx() {
                                            crate::layout::node::clear_focus(other.composer.arena_nodes_mut(), r);
                                        }
                                        other.focused_id = None;
                                        other.focused_slot_key = None;
                                    }
                                }
                                if let Some(ref sw) = pw.skia_window {
                                    set_ime_enabled(&sw, want);
                                }
                                handled = true;
                                break;
                            }
                        }
                    }
                }
                debug::DebugEvent::PointerDown { x, y } => {
                    // Simulate a pointer down: shares the core with the real PointerButton Down
                    // (with_focus=false — the debug path does no cursor/focus work).
                    pw.input.last_pos = Some((x, y));
                    let modifiers = pw.modifiers;
                    handle_pointer_down(pw, (x, y), pw.input.last_kind.clone(), &modifiers, false);
                    handled = true;
                }
                debug::DebugEvent::PointerMove { x, y } => {
                    // Simulate a drag selection: shares the core with the real PointerMoved (including the x_off alignment offset).
                    pw.input.last_pos = Some((x, y));
                    let modifiers = pw.modifiers;
                    handle_pointer_move(pw, (x, y), pw.input.last_kind.clone(), &modifiers);
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                }
                debug::DebugEvent::PointerUp { x, y } => {
                    // Simulate a release: first run the real Up click detection (exercising the real
                    // chain). As on the real path (the PointerButton Up arm), a State set inside
                    // on_click must request_redraw — otherwise it relies on the asynchronous wake_up
                    // chain (occasionally lost → the user sees the count not refresh).
                    if detect_click(pw, (x, y)) {
                        if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    }
                    // Execute the overlay click (same as the real path).
                    if exec_overlay_click(pw) {
                        if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    }
                    // End the overlay drag (same as the real path).
                    if overlay_drag_up(pw) {
                        if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    }
                    // Gesture up detection (same as the real path).
                    if gesture_up(pw, (x, y)) {
                        if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    }
                    // End the drag scroll: fast enough → fling (same as the real path).
                    drag_scroll_up(pw);
                    // Release the pressed interaction (same as the real path).
                    release_pressed_interaction(pw);
                    // Notify a selection change, then clean up.
                    if let Some(slot) = pw.input.down_slot {
                        let nodes = pw.composer.arena_nodes();
                        if let Some(r) = pw.composer.layout_root_idx() {
                            if let Some(nid) = crate::layout::node::find_node_id_by_slot_key(nodes, r, slot) {
                                if let Some(idx) = crate::layout::node::find_node_by_id(nodes, r, nid) {
                                    if let Some(reg) = nodes[idx].registrar.borrow().as_ref() {
                                        reg.fire_on_change();
                                    }
                                }
                            }
                        }
                    }
                    pw.input.down_slot = None;
                    pw.input.down = None;
                    handled = true;
                }
                debug::DebugEvent::Scroll { dx, dy } => {
                    // An injected wheel follows the same precedence as a real one: an overlay under the
                    // pointer first (a test moves the pointer with `m x y`), then the main tree's blind
                    // search — which is what makes a scrollable menu testable at all, since the blind
                    // search only ever sees the main tree's arena.
                    let overlay_target = pw.input.last_pos.and_then(|(px, py)| {
                        let (i, (lx, ly)) = hit_overlay(pw, (px, py))?;
                        let ov = &pw.overlay.layers[i];
                        let r = ov.composer.layout_root_idx()?;
                        let nodes = ov.composer.arena_nodes();
                        let path = hit_test_with_flights(nodes, r, ov.composer.transition_roots(), lx, ly);
                        wheel_scroll_target(nodes, &path, dx, dy).map(|t| (i, t, r))
                    });
                    // An injected wheel is one notch — `s` in the debug protocol — so it pages at once
                    // instead of banking travel the way a touchpad stream does.
                    let wheel = WheelDelta { dx, dy, kind: WheelKind::Notch };
                    let now = std::time::Instant::now();
                    let density = crate::unit::Density::from_density(pw.scale_factor as f32);
                    if let Some((i, target, r)) = overlay_target {
                        let ov = &mut pw.overlay.layers[i];
                        let outcome = apply_wheel_to_target(
                            ov.composer.arena_nodes_mut(),
                            r,
                            target,
                            OVERLAY_WHEEL_SPACE_BASE + i as u64,
                            wheel,
                            &mut pw.wheel,
                            density,
                            now,
                        );
                        handled = outcome.handled();
                    } else if let Some(r) = pw.composer.layout_root_idx() {
                        let nodes = pw.composer.arena_nodes();
                        // An injected wheel follows the REAL one when the test moved the pointer:
                        // hit-test at that position and take the first axis-matching node on the path
                        // (including a horizontal-only container, which takes the vertical wheel).
                        // Without the pointer it falls back to the blind search, which is what a test
                        // that never moves the mouse has to use.
                        let by_pointer = pw.input.last_pos.and_then(|(px, py)| {
                            let path = crate::layout::node::hit_test_with_flights(nodes, r, pw.composer.transition_roots(), px, py);
                            wheel_scroll_target(nodes, &path, dx, dy)
                        });
                        if let Some(target) = by_pointer.or_else(|| find_scroll_target(nodes, r, dx, dy)) {
                            // Same helper as the real wheel, so the two paths cannot drift again.
                            let outcome = apply_wheel_to_target(
                                pw.composer.arena_nodes_mut(),
                                r,
                                target,
                                MAIN_TREE_WHEEL_SPACE,
                                wheel,
                                &mut pw.wheel,
                                density,
                                now,
                            );
                            handled = outcome.handled();
                        }
                    }
                }
                debug::DebugEvent::Resize { w, h } => {
                    // Real resize: request_inner_size → WM_SIZE → SurfaceResized
                    // comes back through the event path naturally — exactly the same route as dragging or
                    // maximising (both the pw fields and the size State are updated by the event handler,
                    // so this does not fight surface_size self-healing).
                    if let Some(ref sw) = pw.skia_window {
                        let _ = sw.request_surface_size(
                            winit::dpi::LogicalSize::new(w as f64, h as f64).into(),
                        );
                    }
                    handled = true;
                }
                _ => {}
            }
        }
        if handled { if let Some(ref sw) = pw.skia_window { sw.request_redraw(); } }
        if debug::has_pending() { if let Some(ref sw) = pw.skia_window { sw.request_redraw(); } }
    }
}

