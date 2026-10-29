use super::*;

impl ApplicationHandler for AppState {
        fn new_events(&mut self, event_loop: &dyn ActiveEventLoop, _cause: StartCause) {
        // Wait + request_redraw drives the animation itself (avoids dropping frames when racing between Poll and Wait).
        event_loop.set_control_flow(ControlFlow::Wait);
        // Advance the animations on every pass (independent of windows, so multi-window and child-window animations advance correctly).
        // Animations (ripples and state-layer transitions register in the global animation list too — they drive redraws automatically).
        let animating = crate::animation::update_animations();
        if animating {
            // Animations active: WaitUntil wakes on a timer (aligned to the refresh rate) so every
            // frame is woken (no freeze), while request throttling (at least a frame interval since
            // the last render) limits WM_PAINT production — fixing the 55k/s self-driven spin.
            let interval = self.windows.values().map(|pw| pw.clock.frame_interval).min()
                .unwrap_or(std::time::Duration::from_millis(16));
            event_loop.set_control_flow(ControlFlow::WaitUntil(std::time::Instant::now() + interval));
            let now = std::time::Instant::now();
            for pw in self.windows.values_mut() {
                if now.duration_since(pw.clock.last_request_time) >= pw.clock.frame_interval {
                    pw.clock.last_request_time = now;
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                }
            }
        } else if self.was_animating {
            // Animations just stopped: force a final frame (the request from the last set may have been skipped).
            for pw in self.windows.values_mut() {
                if let Some(ref sw) = pw.skia_window {
                    pw.clock.force_redraw = true; // Only set for a renderable window (no None left behind).
                    sw.request_redraw();
                }
            }
        }
        // Fallback: request a redraw when a pending state exists (a State changed and awaits
        // recomposition — a click or an async callback) or a render is owed (force_redraw — an
        // update skipped by frame throttling). A throttled pass is not dropped: WaitUntil retries
        // at the next available moment, so a one-off update does not wait for an external event.
        let now = std::time::Instant::now();
        let mut retry_deadline: Option<std::time::Instant> = None;
        for pw in self.windows.values_mut() {
            // An owed render aligns on last_render (force_redraw is set when frame throttling skips one).
            if pw.clock.force_redraw {
                if now.duration_since(pw.clock.last_render_time) >= pw.clock.frame_interval {
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                } else {
                    let d = pw.clock.last_render_time + pw.clock.frame_interval;
                    retry_deadline = Some(match retry_deadline { Some(e) => e.min(d), None => d });
                }
            }
            // Pending states throttle on last_request (at most once per frame while an animation keeps them pending).
            if pw.composer.has_pending_states() {
                if now.duration_since(pw.clock.last_request_time) >= pw.clock.frame_interval {
                    pw.clock.last_request_time = now;
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                } else {
                    let d = pw.clock.last_request_time + pw.clock.frame_interval;
                    retry_deadline = Some(match retry_deadline { Some(e) => e.min(d), None => d });
                }
            }
        }
        if let Some(d) = retry_deadline {
            event_loop.set_control_flow(ControlFlow::WaitUntil(d));
        }
        // Fire a deferred tap when its timeout expires (Compose detectTapGestures: with an
        // onDoubleTap, onTap fires after a delay) — fired at once when overdue, otherwise a WaitUntil
        // timer wakes us (an idle window still fires it after the double-tap window closes, with no
        // input event needed).
        let now = std::time::Instant::now();
        let mut next_tap_deadline: Option<std::time::Instant> = None;
        for pw in self.windows.values_mut() {
            let mut kept = Vec::new();
            for t in std::mem::take(&mut pw.input.pending_taps) {
                if t.deadline <= now {
                    pw.fire_pending_tap(t);
                } else {
                    let d = t.deadline;
                    kept.push(t);
                    next_tap_deadline = Some(match next_tap_deadline {
                        Some(e) => e.min(d),
                        None => d,
                    });
                }
            }
            pw.input.pending_taps = kept;
        }
        if let Some(d) = next_tap_deadline {
            event_loop.set_control_flow(ControlFlow::WaitUntil(d));
        }
        // Long press fires AT its deadline, while the finger is still down (Compose
        // `detectTapGestures`: `onLongPress` arrives during the press, not on release). The tracker
        // is polled here, and the `WaitUntil` below is what wakes an idle window at that moment.
        //
        // Nothing is duplicated for it: the gesture, its slot and its arena are already on the
        // window, so the deadline only has to be remembered until it passes.
        let mut next_long_press: Option<std::time::Instant> = None;
        for pw in self.windows.values_mut() {
            let fired = pw.input.tracker.as_mut().and_then(|tracker| tracker.poll_long_press(now));
            if let Some(action) = fired {
                let (slot, arena) = (pw.input.slot, pw.input.arena);
                if let Some(slot) = slot {
                    let fired_callback = fire_in_gesture_arena(pw, arena, slot, action);
                    if fired_callback {
                        if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    }
                }
            }
            if let Some(deadline) = pw.input.tracker.as_ref().and_then(|tracker| tracker.long_press_deadline()) {
                next_long_press = Some(match next_long_press {
                    Some(existing) => existing.min(deadline),
                    None => deadline,
                });
            }
        }
        if let Some(d) = next_long_press {
            event_loop.set_control_flow(ControlFlow::WaitUntil(d));
        }
        // Fallback consumption of DevTools events: only walk windows with queued events, so an event
        // batch does not spin through every window; legacy target 0 only ever matches the parent.
        let targets = debug::queued_event_targets();
        if !targets.is_empty() {
            let windows: Vec<WindowId> = self.windows.keys().copied().collect();
            for wid in windows {
                if targets.contains(&(wid.into_raw() as u64)) {
                    self.consume_debug_events(wid);
                }
            }
        }
        // Actions the accessibility bridge queued: UIA calls a provider on its own schedule, so the
        // provider only records what was asked for and this — the frame loop — performs it, running
        // the same callback a real click or Tab would.
        if crate::semantics::has_actions() {
            let windows: Vec<WindowId> = self.windows.keys().copied().collect();
            for wid in windows {
                self.consume_ui_actions(wid);
            }
        }
        self.was_animating = animating;
    }

    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        AppState::process_pending_windows(self, event_loop);
    }

    fn resumed(&mut self, _event_loop: &dyn ActiveEventLoop) {}

    fn proxy_wake_up(&mut self, event_loop: &dyn ActiveEventLoop) {
                // If the debug server asked to shut down, exit the event loop.
        if debug::is_shutdown() {
            event_loop.exit();
            return;
        }
        AppState::process_pending_windows(self, event_loop);
        // Consume pending closes (pushed by on_remove; the end of compose consumes them too).
        crate::app::window::Window::process_detached(
            &mut self.windows,
            event_loop,
            &|| debug::force_shutdown(),
            &mut self.saveable,
        );
        // Redraw requests raised from OUTSIDE the window they concern: a `Window` node publishing a new
        // theme intent runs in its DECLARING tree, which leaves the window's own composer with nothing
        // pending — without this its frame would never be scheduled and the new theme would sit unseen.
        for created_id in take_redraw_requests() {
            for pw in self.windows.values() {
                if pw.created_id == Some(created_id) {
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                }
            }
        }
        // Wake the windows when the debug tool has pending requests (screenshots and simulated events need RedrawRequested).
        if debug::has_pending() {
            for pw in self.windows.values() {
                if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
            }
        }
        // The mode moved (from an application or a background thread): every window has to look at it, and
        // a window whose content an application composed itself has no dependency to be woken by.
        if crate::theme::take_theme_redraw_all() {
            for pw in self.windows.values() {
                if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
            }
        }
        // An async State change wakes the event loop and then needs request_redraw (throttled to at
        // most one per frame interval — an animation notifies every frame and would otherwise storm).
        let now = std::time::Instant::now();
        let mut retry_deadline: Option<std::time::Instant> = None;
        for pw in self.windows.values_mut() {
            if pw.composer.has_pending_states() {
                if now.duration_since(pw.clock.last_request_time) >= pw.clock.frame_interval {
                    pw.clock.last_request_time = now;
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                } else {
                    // A throttled pass is not dropped: retry when due (a one-off update does not wait for an external event).
                    let d = pw.clock.last_request_time + pw.clock.frame_interval;
                    retry_deadline = Some(match retry_deadline { Some(e) => e.min(d), None => d });
                }
            }
        }
        if let Some(d) = retry_deadline {
            event_loop.set_control_flow(ControlFlow::WaitUntil(d));
        }
    }

    fn window_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let is_parent = self.parent_window_id.map(|p| p == window_id).unwrap_or(false);
        let closing_last = self.windows.len() == 1;
        // winit reports the system theme per WINDOW (winit-win32 raises it from `WM_SETTINGCHANGE`), while
        // the change itself is process-wide: record it, and schedule the frame of every window — a window
        // that is not redrawn never notices (each one re-resolves against the epoch in its own frame).
        if let WindowEvent::ThemeChanged(theme) = &event {
            crate::theme::note_platform_theme(matches!(theme, winit::window::Theme::Dark));
            for pw in self.windows.values_mut() {
                if let Some(ref sw) = pw.skia_window {
                    sw.request_redraw();
                }
            }
            return;
        }
        let Some(pw) = self.windows.get_mut(&window_id) else { return };

        match event {
            WindowEvent::MouseWheel { delta, .. } => {
                let mut wheel = match delta {
                    // LineDelta's 20 is logical px per line, matching the scroll offsets below. One
                    // notch is discrete, so it pages as one (`WheelKind::Notch`).
                    winit::event::MouseScrollDelta::LineDelta(x, y) => WheelDelta {
                        dx: x * 20.0,
                        dy: y * 20.0,
                        kind: wheel_kind_for_lines(x, y),
                    },
                    // `PixelDelta` is a PHYSICAL position (winit), and everything downstream is
                    // logical, so it converts like the pointer positions do. Without this a
                    // touchpad scroll moved `density` times too far on a HiDPI display. Only
                    // platforms that emit PixelDelta (macOS/Linux touchpads) reached it — the
                    // win32 backend sends LineDelta — so the conversion is unverified here.
                    //
                    // Its events are continuous, so `WheelPager` decides when they add up to a page.
                    winit::event::MouseScrollDelta::PixelDelta(p) => {
                        let lp = p.to_logical::<f32>(pw.scale_factor);
                        WheelDelta { dx: lp.x, dy: lp.y, kind: WheelKind::Pixel }
                    }
                    // `MouseScrollDelta` is `#[non_exhaustive]` in winit 0.31 (it was marked so in
                    // 0.31.0-beta.3), so a future variant must not break this match: an unknown
                    // delta scrolls nothing rather than being guessed at.
                    _ => WheelDelta { dx: 0.0, dy: 0.0, kind: WheelKind::Notch },
                };
                // Shift + vertical wheel → horizontal scroll (so horizontal containers such as LazyRow
                // work; most systems do not translate Shift+wheel into dx, so it is done explicitly here).
                let (dx, dy) = scroll_delta_with_shift(wheel.dx, wheel.dy, pw.modifiers.shift_key());
                wheel.dx = dx;
                wheel.dy = dy;
                if dx != 0.0 || dy != 0.0 {
                    // Overlays float ABOVE the main tree and own separate arenas, so a wheel over one has
                    // to scroll ITS container — a menu's list — rather than whatever the main tree has
                    // under the same point. This is the routing pointer events already use (`hit_overlay`),
                    // and without it a scrollable menu could not be scrolled at all: the main-tree search
                    // below cannot see into another arena (measured: the last item stayed at y=920 in a
                    // 520px window no matter how much the wheel moved).
                    let overlay_target = pw.input.last_pos.and_then(|(px, py)| {
                        let (i, (lx, ly)) = hit_overlay(pw, (px, py))?;
                        let ov = &pw.overlay.layers[i];
                        let r = ov.composer.layout_root_idx()?;
                        let nodes = ov.composer.arena_nodes();
                        let path = hit_test_with_flights(nodes, r, ov.composer.transition_roots(), lx, ly);
                        // The overlay's own arena, the main tree's rule: a menu's list pages exactly
                        // like a pager in the main tree, and a horizontal pager inside the menu takes
                        // the vertical wheel too.
                        wheel_scroll_target(nodes, &path, wheel.dx, wheel.dy).map(|t| (i, t, r))
                    });
                    let now = std::time::Instant::now();
                    let density = crate::runtime::density::density_with_system_font_scale(pw.scale_factor as f32);
                    if let Some((i, target, r)) = overlay_target {
                        let ov = &mut pw.overlay.layers[i];
                        apply_wheel_to_target(
                            ov.composer.arena_nodes_mut(),
                            r,
                            target,
                            OVERLAY_WHEEL_SPACE_BASE + i as u64,
                            wheel,
                            &mut pw.wheel,
                            density,
                            now,
                        );
                    } else if let Some(root_idx) = pw.composer.layout_root_idx() {
                        let nodes = pw.composer.arena_nodes();
                        // §3.7 fix: hit-test at the mouse position first, then find the scroll target on the
                        // hit path — of two side-by-side scroll areas only the hovered one scrolls (the old
                        // find_scroll_target blind-searched children in reverse order and could pick wrong).
                        // Uses the last PointerMoved position (winit 0.31 MouseWheel carries no cursor position).
                        // NOTE: the real wheel has **no** blind-search fallback — with the mouse over no scroll
                        // node nothing scrolls (otherwise wheeling over blank space moved a list or
                        // Shift+horizontal-scrolled: a user-reported bug). `None` (no movement since startup,
                        // or the pointer left the window) does not scroll either; the blind search belongs to
                        // DebugEvent::Scroll only (injected test events have no coordinates — see consume_debug_events).
                        let target = pw.input.last_pos.and_then(|(px, py)| {
                            let path = crate::layout::node::hit_test_with_flights(nodes, root_idx, pw.composer.transition_roots(), px, py);
                            // The hit path runs root → leaf; find the first axis-matching scroll node from the inside out.
                            wheel_scroll_target(nodes, &path, wheel.dx, wheel.dy)
                        });
                        if let Some(target) = target {
                            // Paging, pixel scrolling and the edge pulse all live in the shared
                            // helper; see `wheel_page_step` and `apply_wheel_to_target`.
                            apply_wheel_to_target(
                                pw.composer.arena_nodes_mut(),
                                root_idx,
                                target,
                                MAIN_TREE_WHEEL_SPACE,
                                wheel,
                                &mut pw.wheel,
                                density,
                                now,
                            );
                        }
                    }
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                }
            }
            WindowEvent::CloseRequested => {
                if let Some(ref mut cb) = pw.on_close { cb(); }
                // Snapshot saveable state while the composition is still alive to provide it.
                if let Some(key) = pw.saveable_key {
                    if let Some(registry) = pw.composer.saveable_registry() {
                        let saved = registry.perform_save();
                        if !saved.is_empty() {
                            crate::runtime::saveable::persist::save_saved(key, &saved);
                            self.saveable.insert(key, saved);
                        }
                    }
                }
                if let Some(ref sw) = pw.skia_window {
                    crate::accessibility::uninstall(&**sw, window_id.into_raw() as u64);
                }
                self.windows.remove(&window_id);
                // The window is gone, so drop its frame clock too (otherwise the registry grows by one entry per window opened).
                crate::effect::forget_window(window_id.into_raw() as u64);
                // Clean up the debug tree entry (nothing renders after close — a leftover would mislead UI tests).
                debug::remove_tree(window_id.into_raw() as u64);
                // Ask the other windows to redraw (state may have changed).
                for pw in self.windows.values() {
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                }
                if closing_last || is_parent {
                    debug::force_shutdown();
                    event_loop.exit();
                }
            }
            WindowEvent::Destroyed => {
                if let Some(cid) = pw.created_id { crate::app::window::CREATED.lock().unwrap().remove(&cid); }
                if let Some(ref sw) = pw.skia_window {
                    crate::accessibility::uninstall(&**sw, window_id.into_raw() as u64);
                }
                self.windows.remove(&window_id);
                debug::remove_tree(window_id.into_raw() as u64);
                for pw in self.windows.values() {
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                }
                if self.windows.is_empty() {
                    event_loop.exit();
                }
            }
            // Moving across screens: Moved fires at a high rate while dragging — debounced inside with 300 ms.
            WindowEvent::Moved { .. } => {
                pw.refresh_frame_interval();
            }
            WindowEvent::ThemeChanged(_) => {
                // Handled before the per-window lookup — see the top of this function: the report is
                // process-wide, so one window's event has to schedule every window's frame.
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                pw.scale_factor = scale_factor;
                // Switching monitors may change the scale — refresh the frame interval too (debounced inside the method).
                pw.refresh_frame_interval();
                // Phase 4.1: a scale change must trigger recomposition — TextUnit::Px/Dimension::Px
                // are converted with current_density() and written into the modifier during composition,
                // so without a re-run of build the px values keep the old density (text and sizes would
                // not follow DPI scaling). Full recomposition — DPI changes are rare, the cost is fine.
                pw.composer.request_recomposition(0);
                if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
            }
            WindowEvent::PointerButton { position, state, button, .. } => {
                let lp = position.to_logical::<f32>(pw.scale_factor);
                let scene_pos = (lp.x, lp.y);
                // Down/Up refresh the pointer position too (wheeling right after a click without moving
                // would otherwise use a stale last_pointer_pos and hit the wrong target).
                pw.input.last_pos = Some(scene_pos);
                let event_type = if state.is_pressed() {
                    crate::input::PointerEventType::Down
                } else {
                    crate::input::PointerEventType::Up
                };
                if state.is_pressed() {
                    // -- Down: the pointer-down core (shared, so the real and Debug paths cannot diverge) --
                    let modifiers = pw.modifiers;
                    handle_pointer_down(pw, scene_pos, crate::input::PointerKind::from_button_source(&button), &modifiers, true);
                }
                // -- Up: Compose-style click detection (on release only — Down keeps
                // pointer_down_state for drag selection; running it unconditionally would take the
                // state right after Down, so a drag could not select) --
                // NOTE: this must sit OUTSIDE `if state.is_pressed()` (the Down block): an Up event
                // (Released) never enters the Down block, so click detection nested inside it never
                // ran for Up (which once made every real click a no-op while [sel-up-clean] outside
                // the block printed fine).
                if !state.is_pressed() {
                    // Request a redraw explicitly: a State set inside on_click goes through the async
                    // wake_up chain, and without the pending check as a fallback the refresh was missed.
                    if detect_click(pw, scene_pos) {
                        if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    }
                    // Execute the overlay click (recorded when down hit an overlay — fired on up; the
                    // main-tree detect_click short-circuits on down, records no pointer_down_state and idles).
                    if exec_overlay_click(pw) {
                        if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    }
                    // End the overlay drag (up).
                    if overlay_drag_up(pw) {
                        if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    }
                    // Gesture up detection (tap/double-tap/long-press/drag-end).
                    if gesture_up(pw, scene_pos) {
                        if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    }
                    // End the drag scroll: fast enough → fling (inertial scrolling).
                    drag_scroll_up(pw);
                    // Release the pressed interaction (Compose's Release semantics — the clickable pressed state ends).
                    release_pressed_interaction(pw);
                }
                // -- Pointer event dispatch (on Up, dispatch before clearing the capture) --
                let nodes = pw.composer.arena_nodes();
                if let Some(r) = pw.composer.layout_root_idx() {
                    let path = hit_test_with_flights(nodes, r, pw.composer.transition_roots(), scene_pos.0, scene_pos.1);
                    let ptr_ev = crate::input::PointerEvent {
                        event_type,
                        position: (0.0, 0.0),
                        scene_position: scene_pos,
                        kind: crate::input::PointerKind::from_button_source(&button),
                        is_alt_pressed: pw.modifiers.alt_key(),
                        is_ctrl_pressed: pw.modifiers.control_key(),
                        is_shift_pressed: pw.modifiers.shift_key(),
                        is_meta_pressed: pw.modifiers.meta_key(),
                    };
                    pw.input.last_kind = ptr_ev.kind.clone();
                    dispatch_ptr_event(nodes, r, &path, &ptr_ev, scene_pos, pw.input.down_slot);
                }
                // After Up, clear the capture and notify a selection change.
                if !state.is_pressed() {
                    // The Compose way: take the registrar by the dragged node's slot_key and fire directly.
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
                    // Defensive: take may not have run (an edge outside the click-detection branch) — clear the state anyway.
                    pw.input.down = None;
                }
                if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                event_loop.set_control_flow(ControlFlow::Poll);
                let nodes = pw.composer.arena_nodes();
                if let Some(r) = pw.composer.layout_root_idx() {
                    let fid = crate::layout::node::get_focus_id(nodes, r);
                    let slot = fid.and_then(|id| crate::layout::node::find_node_by_id(nodes, r, id).map(|idx| nodes[idx].slot_key));
                    pw.focused_id = fid;
                    pw.focused_slot_key = slot;
                }
                if let Some(ref proxy) = *APP_PROXY.lock().unwrap() { let _ = proxy.wake_up(); }
            }
            WindowEvent::PointerMoved { position, .. } => {
                let lp = position.to_logical::<f32>(pw.scale_factor);
                let scene_pos = (lp.x, lp.y);
                // Record the last pointer position (used by MouseWheel to find its scroll target — §3.7;
                // winit 0.31 MouseWheel events carry no cursor position).
                pw.input.last_pos = Some(scene_pos);
                // The pointer-move core (shared, so the real and Debug paths cannot diverge; the Debug
                // path used to lack the x_off alignment offset, misplacing Center/Right text selection).
                let modifiers = pw.modifiers;
                let consumed = handle_pointer_move(pw, scene_pos, pw.input.last_kind.clone(), &modifiers);
                // Request a redraw when the move was consumed (on_pointer_event may update State) or a
                // drag selection is in progress; an unconsumed hover move does not wake the event loop.
                if consumed || pw.input.down.is_some() {
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                }
            }
            // Pointer left the window: clear every hover (send Exit to each hoverable) and clear the
            // pointer position (otherwise the wheel still uses the old coordinates and scrolls a
            // container the mouse is no longer over — a user-reported bug).
            WindowEvent::PointerLeft { .. } => {
                let olds: Vec<u64> = pw.hovered_slots.drain().collect();
                for old in olds {
                    exit_hover_at(pw, old);
                }
                pw.input.last_pos = None;
                update_cursor(pw, crate::input::PointerIcon::DEFAULT);
            }
            WindowEvent::ModifiersChanged(m) => {
                pw.modifiers = m.state();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let event_type = if event.state.is_pressed() {
                    crate::input::KbEventType::KeyDown
                } else {
                    crate::input::KbEventType::KeyUp
                };
                let ke = crate::input::KbEvent {
                    key: event.logical_key.clone(),
                    event_type,
                    is_alt_pressed: pw.modifiers.alt_key(),
                    is_ctrl_pressed: pw.modifiers.control_key(),
                    is_shift_pressed: pw.modifiers.shift_key(),
                    is_meta_pressed: pw.modifiers.meta_key(),
                    repeat: event.repeat,
                };
                let mut consumed = false;
                if event.state.is_pressed() && matches!(&event.logical_key, Key::Named(NamedKey::Escape)) {
                    consumed = pw.escape_key();
                }
                if event.state.is_pressed() && matches!(&event.logical_key, Key::Named(NamedKey::Tab)) {
                    pw.tab_move_focus(pw.modifiers.shift_key());
                    consumed = true;
                }
                if !consumed {
                    consumed = dispatch_key_to_overlay(pw, &ke);
                }
                if !consumed {
                    // The page is out of reach while a focus-scope overlay is up: it keeps its focused
                    // node (and its ring comes back when the overlay closes), but keys it would have
                    // handled — typing into a field behind the scrim — stop here instead.
                    if !focus_scope_is_open(pw) {
                        consumed = dispatch_key_to_focus(pw, &ke);
                    }
                }
                // Arrow-key focus navigation (mirroring Compose Desktop's arrow-key navigation) —
                // picks the focusable node in the target direction's half-plane with the smallest
                // "directional distance + perpendicular offset x 2".
                if !consumed && event.state.is_pressed() && !event.repeat {
                    let dir = match &event.logical_key {
                        Key::Named(NamedKey::ArrowRight) => Some(FocusDir::Right),
                        Key::Named(NamedKey::ArrowLeft) => Some(FocusDir::Left),
                        Key::Named(NamedKey::ArrowDown) => Some(FocusDir::Down),
                        Key::Named(NamedKey::ArrowUp) => Some(FocusDir::Up),
                        _ => None,
                    };
                    if let Some(dir) = dir {
                        if pw.focus_move_directional(dir) {
                            consumed = true;
                        }
                    }
                }
                if consumed {
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    event_loop.set_control_flow(ControlFlow::Poll);
                }
            }
            WindowEvent::Ime(ime) => {
                use winit::event::Ime;
                match ime {
                    Ime::Preedit(text, cursor) => {
                        // Focused overlay first (independent composer — main-tree
                        // lookup can never reach overlay nodes), then main tree.
                        // NOTE TextField containerisation: focus is on the container and ime_callback is
                        // on the input leaf — looking only at the focused node means Preedit never
                        // arrives (the pre-edit is not shown).
                        // Find the first ime_callback downwards from the focused node.
                        let overlay_target: Option<(usize, usize)> = (0..pw.overlay.layers.len()).rev().find_map(|i| {
                            let ov = &pw.overlay.layers[i];
                            if ov.closing { return None; }
                            let fid = ov.focused_id?;
                            let nodes = ov.composer.arena_nodes();
                            let r = ov.composer.layout_root_idx()?;
                            let idx = crate::layout::node::find_node_by_id(nodes, r, fid)?;
                            let ime_idx = find_descendant_ime_callback(nodes, idx)?;
                            Some((i, ime_idx))
                        });
                        if let Some((i, ime_idx)) = overlay_target {
                            let nodes = pw.overlay.layers[i].composer.arena_nodes();
                            if let Some(cb) = nodes[ime_idx].ime_callback.borrow_mut().as_mut() {
                                cb(&text, cursor);
                            }
                        } else if let Some(fid) = pw.focused_id {
                            let nodes = pw.composer.arena_nodes();
                            if let Some(r) = pw.composer.layout_root_idx() {
                                if let Some(idx) = crate::layout::node::find_node_by_id(nodes, r, fid) {
                                    if let Some(ime_idx) = find_descendant_ime_callback(nodes, idx) {
                                        if let Some(cb) = nodes[ime_idx].ime_callback.borrow_mut().as_mut() {
                                            cb(&text, cursor);
                                        }
                                    }
                                }
                            }
                        }
                        if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    }
                    Ime::Commit(text) => {
                        // IME committed text — dispatched to the focused node's on_key_event as Characters.
                        // Focused overlay first, then main tree (same routing rule).
                        let overlay_target: Option<usize> = (0..pw.overlay.layers.len()).rev().find(|&i| {
                            let ov = &pw.overlay.layers[i];
                            if ov.closing { return false; }
                            ov.focused_id.and_then(|fid| {
                                ov.composer.layout_root_idx().and_then(|r| {
                                    crate::layout::node::find_node_by_id(ov.composer.arena_nodes(), r, fid)
                                })
                            }).is_some()
                        });
                        if let Some(i) = overlay_target {
                            // Send character by character (overlay arena — bubble-only, mirroring main).
                            for ch in text.chars() {
                                let s = ch.to_string();
                                let ke = crate::input::KbEvent {
                                    key: winit::keyboard::Key::Character(s.clone().into()),
                                    event_type: crate::input::KbEventType::KeyDown,
                                    is_alt_pressed: false, is_ctrl_pressed: false,
                                    is_shift_pressed: false, is_meta_pressed: false,
                                    repeat: false,
                                };
                                let ov = &pw.overlay.layers[i];
                                if let Some(fid) = ov.focused_id {
                                    let nodes = ov.composer.arena_nodes();
                                    if let Some(r) = ov.composer.layout_root_idx() {
                                        let mut path: Vec<usize> = Vec::new();
                                        if let Some(idx) = crate::layout::node::find_node_by_id(nodes, r, fid) {
                                            path.push(idx);
                                            let mut pid = nodes[idx].parent_id;
                                            while let Some(id) = pid {
                                                if let Some(anc) = crate::layout::node::find_node_by_id(nodes, r, id) {
                                                    path.push(anc); pid = nodes[anc].parent_id;
                                                } else { break; }
                                            }
                                            path.reverse();
                                        }
                                        for &ni in path.iter().rev() {
                                            let mut consumed = false;
                                            for el in nodes[ni].modifier.elements().iter().rev() {
                                                if let crate::modifier::ModifierElement::KbEvent { on_key: Some(handler), .. } = el {
                                                    if handler(&ke) { consumed = true; break; }
                                                }
                                            }
                                            if consumed { break; }
                                        }
                                    }
                                }
                            }
                            if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                        } else if let Some(fid) = pw.focused_id {
                            let nodes = pw.composer.arena_nodes();
                            if let Some(r) = pw.composer.layout_root_idx() {
                                // Send character by character.
                                for ch in text.chars() {
                                    let s = ch.to_string();
                                    let ke = crate::input::KbEvent {
                                        key: winit::keyboard::Key::Character(s.clone().into()),
                                        event_type: crate::input::KbEventType::KeyDown,
                                        is_alt_pressed: false, is_ctrl_pressed: false,
                                        is_shift_pressed: false, is_meta_pressed: false,
                                        repeat: false,
                                    };
                                    let mut path: Vec<usize> = Vec::new();
                                    if let Some(idx) = crate::layout::node::find_node_by_id(nodes, r, fid) {
                                        path.push(idx);
                                        let mut pid = nodes[idx].parent_id;
                                        while let Some(id) = pid {
                                            if let Some(anc) = crate::layout::node::find_node_by_id(nodes, r, id) {
                                                path.push(anc); pid = nodes[anc].parent_id;
                                            } else { break; }
                                        }
                                        path.reverse();
                                    }
                                    for &ni in path.iter().rev() {
                                        let mut consumed = false;
                                        for el in nodes[ni].modifier.elements().iter().rev() {
                                            if let crate::modifier::ModifierElement::KbEvent { on_key: Some(handler), .. } = el {
                                                if handler(&ke) { consumed = true; break; }
                                            }
                                        }
                                        if consumed { break; }
                                    }
                                }
                                if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                            }
                        }
                    }
                    Ime::Enabled | Ime::Disabled => {}
                    _ => {}
                }
            }
            WindowEvent::SurfaceResized(s) => {
                // Adopt the event size directly. The old version ignored changes above 50% (to guard
                // against stale winit #2094 events) — but maximising always changes more than 50%, so
                // the whole maximise path was swallowed (size and layout stuck on the old values).
                // Stale events are now healed by the per-frame inner_size calibration in RedrawRequested.
                let l = s.to_logical::<f32>(pw.scale_factor);
                pw.width = l.width; pw.height = l.height;
                if let Some(ref mut sw) = pw.skia_window { sw.resize(); }
                // No force_redraw here: while dragging, SurfaceResized arrives at mouse rate (~125 Hz),
                // and rendering unconditionally would bypass frame throttling (measured 3.6 ms/frame).
                // Frame throttling's "render owed" mechanism (it sets force_redraw when it skips) keeps
                // updates from being lost, while request_redraw keeps the chain alive (continuous
                // WM_PAINT plus self-driving while the OS drags, frame gap >= 16 ms).
                // The size State's set() notifies dependents (slots that read window_size(), such as
                // NavigationSuiteScaffold) as dirty, so incremental recomposition switches their form.
                if let Some(s) = pw.window_size_state.borrow().as_ref() {
                    s.set((l.width, l.height));
                }
                if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
            }
            WindowEvent::RedrawRequested => {
                // Frame clock tick: the global one (with_frame_nanos, any window's frame) and this
                // window's own (ctx.frame_clock(), woken only by its own frames).
                crate::effect::frame_tick();
                crate::effect::FrameClock::for_window(window_id.into_raw() as u64).tick();
                // Size self-healing calibration: trust winit's actual surface size. Stale events of
                // the winit #2094 kind (an old size event can arrive late when maximising) are
                // corrected on the next frame — a deviation above 0.5 logical pixels updates pw and
                // notifies the size State (so the suite's form refreshes too).
                if let Some(ref sw) = pw.skia_window {
                    let phys = sw.surface_size();
                    let l = phys.to_logical::<f32>(pw.scale_factor);
                    if (l.width - pw.width).abs() > 0.5 || (l.height - pw.height).abs() > 0.5 {
                        pw.width = l.width;
                        pw.height = l.height;
                        if let Some(s) = pw.window_size_state.borrow().as_ref() {
                            s.set((l.width, l.height));
                        }
                    }
                }
                // Advance animations (redundant with new_events — a Windows drag-resize runs a modal
                // loop, so new_events/AboutToWait are blocked and never fire; the WM_PAINT-driven
                // RedrawRequested is the only event that keeps arriving during a drag, so ticking here
                // is what makes animations move while dragging). Ticking in both places is harmless:
                // Animatable advances by the real dt.
                crate::animation::update_animations();
                // Consume focus requests (handled before compose so none are lost).
                let _focus_window = pw.composer.focus_window(window_id.into_raw() as u64);
                for id in pw.composer.take_focus_requests() {
                    // Main tree first; on miss try overlays topmost-first (overlay
                    // inputs request focus via on_press exactly like main-tree ones,
                    // but the old code only resolved requests against the main arena,
                    // silently dropping every overlay focus request).
                    let mut main_hit = false;
                    // While a focus scope owns the keyboard the page is out of reach, and that has to
                    // hold for a focus REQUEST too — not only for keys. A component asking for focus
                    // behind a scrim would take it (and turn its IME back on) while the modal is up.
                    if !focus_scope_is_open(pw) {
                        if let Some(r) = pw.composer.layout_root_idx() {
                            let nodes = pw.composer.arena_nodes_mut();
                            if crate::layout::node::focus_by_id(nodes, r, id) {
                                pw.focused_id = crate::layout::node::get_focus_id(nodes, r);
                                main_hit = true;
                            }
                        }
                    }
                    if main_hit {
                        // Single-focus invariant: main focus clears overlay foci.
                        for ov in pw.overlay.layers.iter_mut() {
                            if ov.focused_id.is_some() {
                                if let Some(r) = ov.composer.layout_root_idx() {
                                    crate::layout::node::clear_focus(ov.composer.arena_nodes_mut(), r);
                                }
                                ov.focused_id = None;
                                ov.focused_slot_key = None;
                            }
                        }
                    } else {
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
                                    let fid = ov.composer.layout_root_idx()
                                        .and_then(|r| crate::layout::node::get_focus_id(nodes, r));
                                    let (fslot, want) = fid.and_then(|f| {
                                        crate::layout::node::find_node_by_id(nodes, ov.composer.layout_root_idx()?, f)
                                    }).map(|found| {
                                        (nodes[found].slot_key, node_or_descendant_wants_ime(nodes, found))
                                    }).unwrap_or((0, false));
                                    (fid, fslot, want)
                                };
                                pw.overlay.layers[i].focused_id = fid;
                                pw.overlay.layers[i].focused_slot_key = fid.map(|_| fslot);
                                // Single-focus invariant: overlay focus clears main
                                // focus and sibling overlay foci.
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
                                break;
                            }
                        }
                    }
                    // Look up slot_key separately (avoids a borrow conflict with root).
                    if let Some(fid) = pw.focused_id {
                        let nodes = pw.composer.arena_nodes();
                        if let Some(r) = pw.composer.layout_root_idx() {
                            if let Some(found) = crate::layout::node::find_node_by_id(nodes, r, fid) {
                                pw.focused_slot_key = Some(nodes[found].slot_key);
                                // IME is switched per component declaration: text components (TextField)
                                // declare the IME requirement during composition via
                                // set_current_node_ime_callback; the framework only forwards mechanically
                                // and does not inspect component types. A focusable such as Button that
                                // declares no ime_callback does not turn the IME on when focused.
                                // NOTE TextField containerisation: focus is on the container and
                                // ime_callback on the input leaf — so search the subtree downwards
                                // (node_or_descendant_wants_ime).
                                let wants_ime = node_or_descendant_wants_ime(nodes, found);
                                if let Some(ref sw) = pw.skia_window {
                                    set_ime_enabled(&sw, wants_ime);
                                }
                            }
                        }
                    }
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                }
                // Incremental recomposition → layout → render.
                // Frame-rate limit: on Windows acquire_next_image does not block on vsync (measured
                // 1-2 ms), so without throttling a ~1300 fps render storm follows (the present fence
                // waits only for the GPU submission, not for the display refresh). Skip when less than
                // 16 ms (~60 fps) has passed since the last render — animation values are picked up
                // fresh on the next render (no dropped frames).
                let now = std::time::Instant::now();
                if pw.clock.render_disabled {
                    // Crash boundary (P3-3): stop updating after repeated panics — keep the last picture.
                    return;
                }
                if !pw.clock.force_redraw && now.duration_since(pw.clock.last_render_time) < pw.clock.frame_interval {
                    // Frame throttling hit: do not drop this update — record a render owed (force_redraw),
                    // and the next available frame re-sends request_redraw from new_events. The original
                    // implementation skipped outright, so a one-off state change (a deferred double-tap
                    // tap) only refreshed on the next external event.
                    pw.clock.force_redraw = true;
                    event_loop.set_control_flow(ControlFlow::WaitUntil(pw.clock.last_render_time + pw.clock.frame_interval));
                } else {
                pw.clock.last_render_time = now;
                // Crash boundary (P3-3): a panic in compose/layout/draw (user content code, a skia
                // exception) does not crash the window — it is caught, the frame is skipped (the
                // previous picture stays) and the next frame retries normally. The consecutive-panic
                // counter prevents a storm: past the threshold it prints an error and stops updating.
                // The render path has no unsafe (since P2-3) — using self after catch_unwind is only
                // logically inconsistent, not memory-unsafe; slots/arenas are rebuilt from the root
                // every frame, so a half-updated state left by a panic heals on the next frame.
                let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let wid = window_id.into_raw() as u64;
                    // Built only when something consumes it (the debug server or the accessibility
                    // bridge): it is a full tree walk per frame, and `publishing_enabled` is a const,
                    // so a plain build never pays for it.
                    let mut main_nodes = Vec::new();
                    pw.recompose_layout_render(window_id, |nodes, root_idx, _surface| {
                        debug::update_tree(wid, &debug::build_tree_json(nodes, root_idx));
                        if crate::semantics::publishing_enabled() {
                            main_nodes = crate::semantics::semantics_tree(nodes, root_idx);
                        }
                    });
                    // The arenas of the overlays' own Composers go into the debug tree too (modal/popup
                    // become observable; replaced wholesale every frame — an entry disappears once the
                    // overlay closes). z order = pw.overlay.layers stack order, matching the draw order
                    // of render_overlays.
                    let ov_trees: Vec<(u64, (f32, f32), String)> = pw.overlay.layers.iter().filter_map(|ov| {
                        ov.composer.layout_root_idx()
                            .map(|r| (ov.id, ov.screen_pos, debug::build_tree_json(ov.composer.arena_nodes(), r)))
                    }).collect();
                    debug::set_overlay_trees(wid, ov_trees);
                    if crate::semantics::publishing_enabled() {
                        // The overlays' arenas are walked too, so a modal's contents are in the tree a
                        // screen reader descends. Each keeps its screen origin: its tree is in the
                        // overlay's own coordinates.
                        let overlays = pw.overlay.layers.iter().filter_map(|ov| {
                            ov.composer.layout_root_idx().map(|r| crate::semantics::OverlaySemantics {
                                id: ov.id,
                                origin: ov.screen_pos,
                                nodes: crate::semantics::semantics_tree(ov.composer.arena_nodes(), r),
                            })
                        }).collect();
                        // The two numbers a platform bridge needs to turn logical bounds into
                        // physical ones. Where the window sits on screen is the bridge's own business
                        // — it has the handle, and the answer changes whenever the user moves it.
                        let snapshot = crate::semantics::WindowSemantics {
                            main: main_nodes,
                            overlays,
                            window_size: (pw.width, pw.height),
                            scale_factor: pw.scale_factor as f32,
                        };
                        // Tell the platform bridge what changed since the last frame — it raises the
                        // structure/focus notifications its clients listen for, and does nothing at
                        // all when no client is listening (feature `accessibility`; a no-op otherwise).
                        crate::accessibility::notify(wid, &snapshot);
                        crate::semantics::publish(wid, snapshot);
                    }
                }));
                match panic_result {
                    Ok(()) => {
                        pw.clock.force_redraw = false; // Clear the forced frame only after a successful render (kept if it panicked).
                        pw.clock.consecutive_panics = 0;
                        // Animation self-driving fallback: an animation registered during a render relies
                        // on wake_up to start the next round, but on Windows EventLoopProxy::wake_up from
                        // inside event handling is occasionally lost (a known winit race) → the animation
                        // freezes at its start value until the next external event (a mouse move) converges
                        // it in one large dt. An explicit request_redraw keeps animation frames coming
                        // without depending on that possibly-lost wake-up.
                        if crate::animation::is_animating() {
                            if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                        }
                    }
                    Err(e) => {
                        pw.clock.consecutive_panics += 1;
                        let msg = if let Some(s) = e.downcast_ref::<&str>() { (*s).to_string() }
                                  else if let Some(s) = e.downcast_ref::<String>() { s.clone() }
                                  else { "unknown panic".to_string() };
                        eprintln!("[render-panic] consecutive panic #{} (frame skipped, previous picture kept): {}", pw.clock.consecutive_panics, msg);
                        if pw.clock.consecutive_panics >= 30 {
                            eprintln!("[render-panic] 30 consecutive panics — stopping rendering for this window (avoiding a panic storm)");
                            pw.clock.render_disabled = true;
                        }
                        // force_redraw stays true — the next frame tries again (unless rendering stopped).
                    }
                }
                // Update the IME caret area (so the candidate window follows the caret).
                if let Some(ref sw) = pw.skia_window {
                    if let Some(fid) = pw.focused_id {
                        let nodes = pw.composer.arena_nodes();
                        if let Some(r) = pw.composer.layout_root_idx() {
                            // NOTE TextField containerisation: focus is on the container and
                            // paragraph/cursor on the input leaf — so search downwards for the
                            // ime_callback node (find_descendant_ime_callback); looking only at the
                            // focused node leaves the container without a paragraph, so the candidate
                            // area never updates.
                            let pidx = crate::layout::node::find_node_by_id(nodes, r, fid)
                                .and_then(|idx| find_descendant_ime_callback(nodes, idx));
                            if let Some(pidx) = pidx {
                                if let Ok(borrow) = nodes[pidx].cached_paragraph.try_borrow() {
                                    if let Some(p) = borrow.as_ref() {
                                        let abs = node_abs_position(nodes, r, nodes[pidx].id);
                                        // Content-box offset (on the render side content_x = node origin +
                                        // padding — the IME area must match, otherwise the candidate
                                        // window is shifted by the whole padding).
                                        let (pad_s, pad_t, pad_e, _) = nodes[pidx].modifier.get_padding_sides();
                                        let pad_x = if nodes[pidx].layout_direction == crate::layout::LayoutDirection::Rtl { pad_e } else { pad_s };
                                        // Empty text: no glyph to locate — put the IME area at the content
                                        // origin (line height approximated; matches the render side's empty cursor).
                                        let empty = nodes[pidx].modifier.content_len() == 0;
                                        // The caret index is the edit offset — map it to the display offset
                                        // (TextFieldVisual sits on the container — search upwards).
                                        let caret_idx = crate::text::field::offset_mapping_for_node(nodes, r, pidx)
                                            .map(|m| m.original_to_transformed(nodes[pidx].cursor_index.get()))
                                            .unwrap_or_else(|| nodes[pidx].cursor_index.get());
                                        let (mut cx, cy, ch) = if empty {
                                            (0.0, 0.0, 20.0)
                                        } else {
                                            let tl = crate::text::TextLayout::new(p);
                                            match tl.get_cursor_position(caret_idx) {
                                                Some(v) => v,
                                                None => (0.0, 0.0, 20.0),
                                            }
                                        };
                                        // The IME window follows the same horizontal mapping the
                                        // renderer uses for the caret (a geometric transform widens
                                        // the drawn run, not the laid-out one).
                                        if let Some(t) = crate::layout::node::text_transform(&nodes[pidx]) {
                                            cx = t.map_x(cx, cy);
                                        }
                                        // A line-height style moves the glyphs inside their box.
                                        let line_dy = crate::layout::node::line_geometry(&nodes[pidx], p).paint_dy;
                                        let align = nodes[pidx].modifier.align().unwrap_or(crate::text::TextAlign::Left);
                                        // CONTENT box: the painter uses it, so the IME
                                        // candidate window must too — under a flight layout
                                        // override `measured_size` is the size the PARENT was
                                        // told (review R3-F6).
                                        let node_w = nodes[pidx].content_box().width;
                                        let intrinsic_w = p.max_intrinsic_width();
                                        // NOTE Empty text: the render side draws the caret at the content
                                        // origin (left-aligned, not offset by align) — the IME area must
                                        // match, otherwise with Center/Right alignment the candidate window
                                        // for empty input sits in the middle of the container instead of at the caret.
                                        let x = if empty {
                                            (abs.0 + pad_x) as f64
                                        } else {
                                            let x_off = match align {
                                                crate::text::TextAlign::Left
                                                | crate::text::TextAlign::Justify
                                                | crate::text::TextAlign::Start
                                                | crate::text::TextAlign::End => abs.0,
                                                crate::text::TextAlign::Center => abs.0 + (node_w - intrinsic_w).max(0.0) / 2.0,
                                                crate::text::TextAlign::Right => abs.0 + (node_w - intrinsic_w).max(0.0),
                                            };
                                            (x_off + pad_x + cx) as f64
                                        };
                                        let y = (abs.1 + pad_t + cy + line_dy) as f64;
                                        let _ = sw.request_ime_update(
                                            winit::window::ImeRequest::Update(
                                                winit::window::ImeRequestData::default()
                                                    .with_cursor_area(
                                                        winit::dpi::Position::Logical(winit::dpi::LogicalPosition::new(x, y)),
                                                        winit::dpi::Size::Logical(winit::dpi::LogicalSize::new(2.0, ch as f64)),
                                                    )
                                            )
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                } // end frame-rate limit else (render + IME sync)
                // Check whether compose left a window pending close.
                if pw.composer.pending_window_close_id().is_some() {
                    if let Some(ref proxy) = *APP_PROXY.lock().unwrap() { let _ = proxy.wake_up(); }
                }
                // Consume DevTools events (parent window only) — new_events calls this as a fallback too (see below).
                if is_parent {
                    self.consume_debug_events(window_id);
                }
            }
            _ => {}
        }
    }
}

