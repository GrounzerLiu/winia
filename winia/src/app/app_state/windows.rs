use super::*;

impl AppState {
    /// 消费 DevTools 注入事件（点击/按键/滚动/拖拽等——UI 测试 + WS 调试）。
    /// 在 window_event（RedrawRequested）与 new_events（兜底）两处调用：
    /// 多窗口下主窗口可能在后台——RedrawRequested 不来时事件卡队列，
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

    /// new_events 每轮事件批次必然执行——保证注入事件不丢失。
    pub(crate) fn consume_debug_events(&mut self, window_id: WindowId) {
        let Some(pw) = self.windows.get_mut(&window_id) else { return };
        let mut handled = false;
        for evt in debug::take_queued_events(window_id.into_raw() as u64) {
            match evt {
                debug::DebugEvent::Click { x, y } => {
                    // overlay 优先（对齐真实指针路径 handle_pointer_down）：
                    // 命中 overlay → 消费（不进主树）；外部点击 → dismiss。
                    // ⚠ Click 是合成单事件（非 down/up 分离）——overlay 命中后
                    // 必须立即执行点击（真实路径 down 记录 + up 触发；这里
                    // down 记录后直接 exec，否则 overlay 按钮永远点不动）
                    let click_kind = pw.input.last_kind.clone();
                    if overlay_down(pw, (x, y), click_kind) {
                        // 命中 overlay：立即执行点击（合成单事件——down 记录 +
                        // 立即 up 触发；真实路径由 PointerUp 事件触发）
                        exec_overlay_click(pw);
                        // The synthetic click is down + up in one event, so it has to close the
                        // gesture the down opened, exactly as the real up path does
                        // (`exec_overlay_click` → `gesture_up`): otherwise a tap in a popup would
                        // leave its tracker (and any deferred tap) behind, and `on_tap` would never
                        // fire for a harness click.
                        gesture_up(pw, (x, y));
                        // 释放 overlay 的 press（合成事件无真实 up——否则按下
                        // 波纹/按压态卡死到 overlay 关闭）
                        release_pressed_interaction(pw);
                        if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                        handled = true;
                        continue;
                    }
                    // 只读阶段：hit_test + click 检测（arena 借用在块尾结束）
                    let (fid, sk, _path_len, _click_handled) = {
                        let arena = pw.composer.arena_nodes();
                        let root_idx = pw.composer.layout_root_idx();
                        if let Some(r) = root_idx {
                            let path = hit_test_with_flights(arena, r, pw.composer.transition_roots(), x, y);
                            // debug 点击聚焦：path 中最深的 focusable 节点
                            // （text-field-v2 容器化后焦点/交互在容器——空字段
                            // 输入子节点 0 宽点不中；旧逻辑只看 ime_callback
                            // 已失效）；兼容声明 IME 的节点（旧 leaf 语义）
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
                            // Click 检测（P2-1：复用 fire_click_along_path，
                            // 与主树/overlay 同语义，防内联重复漂移）
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
                        // 任意按键：复用真实键盘派发路径（Preview/Bubble/激活）——
                        // WS 可模拟字符输入/删除/方向键。修饰键默认无（Ctrl 等
                        // 组合暂不支持——如需可扩展 KbEvent 修饰字段）
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
                    // 模拟指针按下：与真实 PointerButton Down 共用核心
                    // （with_focus=false——调试路径不做光标/聚焦）
                    pw.input.last_pos = Some((x, y));
                    let modifiers = pw.modifiers;
                    handle_pointer_down(pw, (x, y), pw.input.last_kind.clone(), &modifiers, false);
                    handled = true;
                }
                debug::DebugEvent::PointerMove { x, y } => {
                    // 模拟拖动选择：与真实 PointerMoved 共用核心（含 x_off 对齐偏移）
                    pw.input.last_pos = Some((x, y));
                    let modifiers = pw.modifiers;
                    handle_pointer_move(pw, (x, y), pw.input.last_kind.clone(), &modifiers);
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                }
                debug::DebugEvent::PointerUp { x, y } => {
                    // 模拟释放：先走真实 Up 的 click 检测（验证真实链路）
                    // 与真实路径（PointerButton Up 分支）一致：on_click 内
                    // State set 后必须 request_redraw——否则依赖 wake_up 异步
                    // 链（偶发丢失 → 用户看到 count 不刷新）
                    if detect_click(pw, (x, y)) {
                        if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    }
                    // overlay 点击执行（与真实路径一致）
                    if exec_overlay_click(pw) {
                        if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    }
                    // overlay 拖拽结束（与真实路径一致）
                    if overlay_drag_up(pw) {
                        if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    }
                    // 手势 up 判定（与真实路径一致）
                    if gesture_up(pw, (x, y)) {
                        if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    }
                    // 拖拽滚动结束：速度足够 → fling（与真实路径一致）
                    drag_scroll_up(pw);
                    // 释放按下交互（与真实路径一致）
                    release_pressed_interaction(pw);
                    // 通知选区变化 + 清理
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
                    // 真实 resize：request_inner_size → WM_SIZE → SurfaceResized
                    // 事件自然回流——与拖拽/最大化完全同通路（pw 字段与尺寸 State
                    // 均由事件处理器统一更新，避免与 surface_size 自愈互相打架）
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

