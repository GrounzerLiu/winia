use super::*;

/// overlay 命中测试——返回 (overlay 索引, 本地坐标)——从最上层（最后一个）往下
pub(crate) fn hit_overlay(pw: &PerWindow, scene_pos: (f32, f32)) -> Option<(usize, (f32, f32))> {
    for i in (0..pw.overlay.layers.len()).rev() {
        let ov = &pw.overlay.layers[i];
        // 关闭中（退出动画播放）：不响应交互——命中视同穿透（下层/主树）
        if ov.closing {
            continue;
        }
        let local = (scene_pos.0 - ov.screen_pos.0, scene_pos.1 - ov.screen_pos.1);
        if let Some(r) = ov.composer.layout_root_idx() {
            let nodes = ov.composer.arena_nodes();
            if !hit_test_with_flights(nodes, r, ov.composer.transition_roots(), local.0, local.1).is_empty() {
                return Some((i, local));
            }
        }
    }
    None
}
/// overlay 渲染（主树之后——上层；模态先画遮罩）

/// overlay 拖拽结束（含嵌套滚动 fling）。overlay 内可滚列表的 fling 走 overlay Composer。
pub(crate) fn overlay_drag_up(pw: &mut PerWindow) -> bool {
    let mut handled = false;
    if let Some(ds) = pw.overlay.drag_scroll.take() {
        // 复用主树 drag_scroll_up 逻辑：样本估速 → overlay 的 dispatch_nested_scroll_fling
        //（此前为空壳——注释"由 overlay 内 NestedScrollConnection on_post_fling 兜底"未接通，
        // 导致松手后列表不 fling、sheet 不 settle → "松手没有动画"）
        let (vx, vy) = (ds.velocity_x(), ds.velocity_y());
        // target 反查：ds.slot 是 down 时 overlay composer 的 scroll 节点 slot_key
        let target: Option<usize> = (|| {
            let ov_idx = pw.overlay.layers.len().checked_sub(1).unwrap_or(0);
            let ov = pw.overlay.layers.get(ov_idx)?;
            let r = ov.composer.layout_root_idx()?;
            let nodes = ov.composer.arena_nodes();
            let id = crate::layout::node::find_node_id_by_slot_key(nodes, r, ds.slot)?;
            crate::layout::node::find_node_by_id(nodes, r, id)
        })();
        #[cfg(debug_assertions)]
        if drag_trace_enabled() {
            eprintln!("[overlay-drag-up] slot={:?} target={:?} v=({},{})", ds.slot, target, vx, vy);
        }
        let Some(idx) = target else { handled = true; return handled; };
        let Some(ov) = pw.overlay.layers.last_mut() else { handled = true; return handled; };
        let Some(r) = ov.composer.layout_root_idx() else { handled = true; return handled; };
        // 手指速度 → 滚动速度（内容速度 = -手指速度），走 nested scroll pre/post fling 链
        let velocity = crate::nested_scroll::ScrollVelocity { x: -vx, y: -vy };
        let _ = dispatch_nested_scroll_fling(ov.composer.arena_nodes_mut(), r, idx, velocity);
        handled = true;
    }
    let Some((idx, slot, _down)) = pw.overlay.drag.take() else { return handled; };
    let started = pw.overlay.drag_started;
    pw.overlay.drag_started = false;
    pw.overlay.drag_last = None;
    if !started {
        return handled;
    }
    if let Some(ov) = pw.overlay.layers.get(idx) {
        let nodes = ov.composer.arena_nodes();
        if let Some(r) = ov.composer.layout_root_idx() {
            fire_gesture_action(nodes, r, slot,
                crate::input::gesture::GestureAction::DragEnd);
        }
    }
    true
}
pub(crate) fn exec_overlay_click(pw: &mut PerWindow) -> bool {
    let Some((idx, local, _nid)) = pw.overlay.click.take() else { return false; };
    let Some(ov) = pw.overlay.layers.get(idx) else { return false; };
    // 关闭中（down 后外部 dismiss 等）→ 不 fire click（避免对已关闭的
    // overlay 误触发——极窄边界但语义正确）
    if ov.closing {
        return false;
    }
    let Some(r) = ov.composer.layout_root_idx() else { return false; };
    let nodes = ov.composer.arena_nodes();
    let path = hit_test_with_flights(nodes, r, ov.composer.transition_roots(), local.0, local.1);
    // 沿路径找 clickable（最内层优先）
    let r = fire_click_along_path(nodes, &path);
    r
}
/// 指针按下：先测 overlay（最上层）——命中 → 记录点击目标；外部 → dismiss
pub(crate) fn overlay_down(pw: &mut PerWindow, scene_pos: (f32, f32), kind: crate::input::PointerKind) -> bool {
    if pw.overlay.layers.is_empty() {
        return false;
    }
    if let Some((i, local)) = hit_overlay(pw, scene_pos) {
        // 命中 overlay 内容——记录点击目标（v1：仅 clickable——up 时执行）
        // ⚠ click_passthrough（Tooltip）：命中浮层但**放行主树**——浮层盖住
        // 锚点（锚点上方 tooltip 与锚点本身重叠）时点击锚点仍生效（否则
        // tooltip 挡住锚点按钮 → 外部 visible 控制关不了）
        let ov = &mut pw.overlay.layers[i];
        if ov.click_passthrough {
            return false;
        }
        // overlay：按 drag 相对 scroll 的深度判定（对齐主树 child_drag「最内层手势优先」：
        // app.rs:2922-2926）——内层可拖组件（slider/switch）优先于滚动；外层面板 on_drag 让位
        // 于内层滚动（列表滚动优先，到边后由 SheetNested.on_post_scroll 折叠 sheet）。
        // path 下标从根到叶递增；rev() 找到的第一个命中即最内层（下标最大）。
        // - drag 更深（组件在 scroll 内部，didx_last > scroll_idx 且更内层）→ 拖拽组件优先
        // - 否则若有 scroll → 滚动优先（列表滚；面板 on_drag 让位）
        // - 否则若有 drag → 面板 on_drag 拖 sheet（背景/文字/空白）
        {
            let nodes = ov.composer.arena_nodes();
            if let Some(r) = ov.composer.layout_root_idx() {
                let path = hit_test_with_flights(nodes, r, ov.composer.transition_roots(), local.0, local.1);
                let scroll_idx = path_scroll_idx(nodes, &path);
                let drag_idx = path_drag_idx(nodes, &path);
                let inner_comp_drag = inner_component_drag(drag_idx, scroll_idx);
                // The axis arbitration in `gesture_move` is main-tree only: an overlay drag runs
                // through `overlay_drag` (its own session, which fires the callbacks itself), not
                // through the tracker, so there is nothing to hand over here.
                pw.input.axis = None;
                pw.input.scroll_slot = None;
                if inner_comp_drag {
                    if let Some(didx) = drag_idx {
                        pw.overlay.drag = Some((i, nodes[didx].slot_key, scene_pos));
                        pw.overlay.drag_origin = ov.screen_pos;
                        pw.overlay.drag_started = false;
                        pw.overlay.drag_last = None;
                        pw.overlay.drag_scroll = None;
                    }
                } else if let Some(t) = scroll_idx {
                    // 列表区（面板 on_drag 让位）：内容滚动优先
                    pw.overlay.drag = None;
                    pw.overlay.drag_started = false;
                    pw.overlay.drag_last = None;
                    pw.overlay.drag_scroll = Some(DragScroll::new(nodes[t].slot_key, scene_pos));
                } else if let Some(didx) = drag_idx {
                    // 非滚动区：fallback 到面板 on_drag（背景/文字拖 sheet）
                    pw.overlay.drag = Some((i, nodes[didx].slot_key, scene_pos));
                    pw.overlay.drag_origin = ov.screen_pos;
                    pw.overlay.drag_started = false;
                    pw.overlay.drag_last = None;
                    pw.overlay.drag_scroll = None;
                } else {
                    pw.overlay.drag = None;
                    pw.overlay.drag_started = false;
                    pw.overlay.drag_last = None;
                    pw.overlay.drag_scroll = None;
                }
            }
        }
        // 发射 Press（按下波纹——对标 Compose PressInteraction.Press；
        // ripple 渲染已支持 overlay，缺的只是事件触发）
        {
            // ⚠ 找不到 clickable_interaction 时**跳过 Press 但不阻断**——
            // 否则普通 clickable（无 interaction source，如 ModalBottomSheet
            // Scrim 层）的点击永远到不了 overlay_click → 点击丢失。
            let nodes = ov.composer.arena_nodes();
            if let Some(r) = ov.composer.layout_root_idx() {
                let path = hit_test_with_flights(nodes, r, ov.composer.transition_roots(), local.0, local.1);
                if let Some(&idx) = path.iter().rev().find(|&&i| {
                    nodes[i].modifier.clickable_interaction().is_some()
                        || nodes[i].modifier.node_click_interaction().is_some()
                }) {
                    if let Some(src) = nodes[idx]
                        .modifier
                        .clickable_interaction()
                        .cloned()
                        .or_else(|| nodes[idx].modifier.node_click_interaction())
                    {
                        // 波纹中心 = 节点本地坐标（overlay 内无滚动/变换——直接换算）
                        let local_press = crate::layout::node::scene_to_node_local(nodes, &path, idx, local.0, local.1);
                        src.emit_press_at(local_press);
                        ov.pressed_interaction = Some((nodes[idx].slot_key, src.clone()));
                    }
                }
            }
        }
        // Press gesture — the same dispatch the main tree does from `gesture_down`. The overlay
        // path used to fire only `on_click` (on up, through `fire_click_along_path`), so a
        // component whose reaction lives on `on_press` was dead inside a popup: a popup
        // `TextField` focuses through `on_press → FocusRequester::request_focus`, which is why
        // this handler used to need a focus rule of its own. With the gesture dispatched, popup
        // content reacts like main-tree content and that rule is gone (the focus lands one frame
        // later, through the ordinary focus-request queue — see `take_focus_requests`).
        //
        // The tracker goes with it, so the rest of the tap family (`on_tap` / `on_double_tap` /
        // `on_long_press`) fires in a popup too; `gesture_move` / `gesture_up` route their actions
        // back into this arena (`pw.input.arena`). A target WITH drag gestures is tracked as well:
        // the overlay drag session for that same node is stood down below, because both dispatchers
        // would otherwise fire its `on_drag_*` callbacks for one gesture.
        {
            // Immutable scope: resolve the target and fire the Press, then leave the overlay borrow
            // before the tracker bookkeeping below needs `pw` mutably.
            let target = {
                let ov = &pw.overlay.layers[i];
                let nodes = ov.composer.arena_nodes();
                match ov.composer.layout_root_idx() {
                    None => None,
                    Some(r) => {
                        let path = hit_test_with_flights(nodes, r, ov.composer.transition_roots(), local.0, local.1);
                        press_gesture_target(nodes, &path).map(|(nid, slot, has_drag)| {
                            fire_gesture_action(nodes, r, slot, crate::input::gesture::GestureAction::Press(local));
                            (nid, slot, has_drag, ov.id)
                        })
                    }
                }
            };
            if let Some((nid, slot, has_drag, ov_id)) = target {
                if has_drag {
                    // The tracker owns this node's drag as well as its tap family: the arbitration
                    // above gave the same node to `pw.overlay.drag`, and both dispatchers would fire
                    // `on_drag_start` / `on_drag` / `on_drag_end` for one gesture. Stand the overlay
                    // session down instead — the tracker routes the drag through
                    // `fire_in_gesture_arena` into this same arena, so nothing is lost, and the node
                    // gains its `on_tap` / `on_double_tap` / `on_long_press` (the tap family of a
                    // popup drag target used to be unreachable, `Slider` included).
                    if pw.overlay.drag.map(|(idx, key, _)| idx == i && key == slot).unwrap_or(false) {
                        pw.overlay.drag = None;
                        pw.overlay.drag_started = false;
                        pw.overlay.drag_last = None;
                    }
                    // A scroll session belongs to a DIFFERENT node (the arbitration picks scroll when
                    // the drag is its ancestor); leave it alone.
                }
                // Same bookkeeping as the main tree for both kinds of target: a deferred tap on this
                // node is due, or this press is its double-tap candidate.
                process_pending_taps_on_down(pw, nid);
                let ctx = pw.input.tap_ctx.take()
                    .filter(|(n, _, _)| *n == nid)
                    .map(|(_, t, p)| (t, p));
                pw.input.tracker = Some(crate::input::gesture::GestureTracker::new(nid, local, has_drag, ctx));
                pw.input.node = Some(nid);
                pw.input.slot = Some(slot);
                pw.input.arena = Some(ov_id);
                pw.input.axis = None;
                pw.input.scroll_slot = None;
                pw.input.arena_origin = pw
                    .overlay
                    .layers
                    .iter()
                    .find(|o| o.id == ov_id)
                    .map(|o| o.screen_pos)
                    .unwrap_or((0.0, 0.0));
            }
        }
        // Overlay pointer dispatch + caret placement — the main-tree sequence from
        // `handle_pointer_down`: Down goes to the overlay arena for its `on_ptr` handlers, and a
        // tap places the text caret from the grapheme anchor (tap-to-place).
        //
        // Focus is deliberately NOT set here. A component that wants the keyboard asks for it,
        // and the press gesture above just gave it that chance (the popup `TextField` container
        // does `on_press → FocusRequester::request_focus`), so a tap on a popup button leaves the
        // field beside it alone — the rule Compose's `Clickable` follows by never calling
        // `requestFocus` for a click. This handler used to focus the deepest focusable node on the
        // hit path (the debug-click rule, see `consume_debug_events`), which stole the keyboard
        // from a field as soon as any button in the same popup was tapped. Drag-select inside
        // overlay inputs is v1-out.
        {
            let ptr_ev = crate::input::PointerEvent {
                event_type: crate::input::PointerEventType::Down,
                position: (0.0, 0.0),
                scene_position: local,
                kind: kind.clone(),
                is_alt_pressed: pw.modifiers.alt_key(),
                is_ctrl_pressed: pw.modifiers.control_key(),
                is_shift_pressed: pw.modifiers.shift_key(),
                is_meta_pressed: pw.modifiers.meta_key(),
            };
            // Read phase (immutable): hit path + caret anchor.
            // (Root is guaranteed by hit_overlay above; the None arm only
            // satisfies the compiler and still falls through to click recording.)
            let (path, caret): (Vec<usize>, Option<(usize, usize)>) = match {
                let ov = &pw.overlay.layers[i];
                let nodes = ov.composer.arena_nodes();
                // Owned roots: the scrutinee borrow ends here, but the arm
                // below needs them (and `ov` there is the outer `&mut` binding).
                let troots: Vec<usize> = ov.composer.transition_roots().to_vec();
                match ov.composer.layout_root_idx() {
                    None => None,
                    Some(r) => Some((nodes, r, troots)),
                }
            } {
                None => (Vec::new(), None),
                Some((nodes, r, troots)) => {
                let path = hit_test_with_flights(nodes, r, &troots, local.0, local.1);
                let caret = (|| {
                    let &innermost = path.last()?;
                    let anchor_node = find_anchor_text_node(nodes, &path, innermost)?;
                    let borrow = nodes[anchor_node].cached_paragraph.try_borrow().ok()?;
                    let para = borrow.as_ref()?;
                    let (ax, ay) = node_abs_position(nodes, r, nodes[anchor_node].id);
                    let (pad_s, pad_t, pad_e, _) = nodes[anchor_node].modifier.get_padding_sides();
                    let pad_x = if nodes[anchor_node].layout_direction == crate::layout::LayoutDirection::Rtl { pad_e } else { pad_s };
                    let tl = crate::text::TextLayout::new(para, 0);
                    let hit = tl.get_closest_grapheme_cluster_cluster_at(skia_safe::Point::new(local.0 - ax - pad_x, local.1 - ay - pad_t));
                    let edit = crate::text::field::offset_mapping_for_node(nodes, r, anchor_node)
                        .map(|m| m.transformed_to_original(hit))
                        .unwrap_or(hit);
                    Some((anchor_node, edit))
                })();
                (path, caret)
                }
            };
            // Pointer dispatch (immutable arena borrow, ends immediately).
            if !path.is_empty() {
                let ov = &pw.overlay.layers[i];
                if let Some(r) = ov.composer.layout_root_idx() {
                    let nodes = ov.composer.arena_nodes();
                    dispatch_ptr_event(nodes, r, &path, &ptr_ev, local, None);
                }
            }
            if let Some((aidx, aoff)) = caret {
                let nodes = pw.overlay.layers[i].composer.arena_nodes();
                if let Some(n) = nodes.get(aidx) {
                    n.cursor_index.set(aoff);
                    if let Some(cb) = n.cursor_callback.borrow_mut().as_mut() {
                        cb(aoff);
                    }
                }
            }
        }
        // (ov borrow ended at the Press block above; re-index here.)
        let nid = pw.overlay.layers[i].composer.layout_root_idx().and_then(|r| {
            let nodes = pw.overlay.layers[i].composer.arena_nodes();
            hit_test_with_flights(nodes, r, pw.overlay.layers[i].composer.transition_roots(), local.0, local.1).last().map(|&idx| nodes[idx].id)
        });
        pw.overlay.click = Some((i, local, nid.unwrap_or(0)));
        return true; // 事件消费——不进主树
    }
    // Outside press: an overlay that dismisses on an outside press closes; an overlay that does
    // not still CONSUMES the press when it is modal (its scrim blocks what is behind it).
    //
    // ⚠ `dismiss_on_outside == false` has to be honoured for a modal overlay too: `modal` alone
    // says "blocks the content behind", not "closes on any press". Compose's
    // `DialogProperties(dismissOnClickOutside = false)` and `PopupProperties.dismissOnClickOutside`
    // keep the overlay open, and callers pass the flag for exactly that — Nav3's
    // `dialogProperties` (`nav.rs`) and `AlertDialog`'s own builder. Treating `modal` as
    // "dismiss" made those flags silent no-ops.
    // ⚠ 同步移除（不等 recompose）——否则残留 overlay 会吞掉关闭后
    // 紧接着的点击（用户"点两次才打开"）且多渲染一帧（视觉闪烁）
    for i in (0..pw.overlay.layers.len()).rev() {
        // A closing overlay is on its way out: it must not be dismissed again and must not swallow
        // the press — `hit_overlay` treats it as transparent for the same reason. Without this a
        // dialog that was just closed (Escape, a button, an outside press) eats the next press for
        // the length of its exit animation, which is the "click twice to open" complaint.
        if pw.overlay.layers[i].closing {
            continue;
        }
        let dismiss = pw.overlay.layers[i].dismiss_on_outside;
        if !dismiss && !pw.overlay.layers[i].modal {
            continue;
        }
        let passthrough = pw.overlay.layers[i].click_passthrough;
        let id = pw.overlay.layers[i].id;
        if dismiss {
            // A content that owns its exit (a bottom sheet) gets the request only: it animates itself out
            // and flips the caller's `visible`, and that removes the overlay. Everything else starts the
            // host's exit now (idempotent with a component that flips its own state during composition:
            // the `closing` flag stops the second path).
            if pw.overlay.layers[i].dismiss_defers_to_content {
                if let Some(cb) = pw.overlay.layers[i].on_dismiss.clone() {
                    cb();
                }
            } else {
                begin_overlay_close(pw, id);
            }
        }
        // ⚠ Tooltip（passthrough）：dismiss 后**放行主树**——点击不消费
        // （否则点按钮第一次只关 tooltip、按钮收不到——需点两次）
        if passthrough {
            continue;
        }
        return true;
    }
    false
}
/// overlay 内 hover 更新（overlay composer + 本地坐标）
pub(crate) fn overlay_update_hover(ov: &mut OverlayWindow, local: (f32, f32)) {
    let hit: Vec<(u64, crate::input::interaction::MutableInteractionSource)> = {
        let nodes = ov.composer.arena_nodes();
        let Some(r) = ov.composer.layout_root_idx() else { return; };
        let path = hit_test_with_flights(nodes, r, ov.composer.transition_roots(), local.0, local.1);
        path.iter()
            .filter(|&&i| nodes[i].modifier.has_hoverable())
            .filter_map(|&i| nodes[i].modifier.hoverable_interaction().map(|s| (nodes[i].slot_key, s.clone())))
            .collect()
    };
    let hit_slots: std::collections::HashSet<u64> = hit.iter().map(|(s, _)| *s).collect();
    for (slot, src) in &hit {
        if ov.hovered_slots.insert(*slot) {
            src.emit_hover_enter();
        }
    }
    let gone: Vec<u64> = ov.hovered_slots.iter().copied().filter(|s| !hit_slots.contains(s)).collect();
    for slot in gone {
        ov.hovered_slots.remove(&slot);
        overlay_exit_hover_at(ov, slot);
    }
}
/// 对 overlay 内指定 slot 的节点补发 Hover Exit
pub(crate) fn overlay_exit_hover_at(ov: &mut OverlayWindow, slot: u64) {
    let nodes = ov.composer.arena_nodes();
    let Some(r) = ov.composer.layout_root_idx() else { return; };
    if let Some(nid) = crate::layout::node::find_node_id_by_slot_key(nodes, r, slot) {
        if let Some(idx) = crate::layout::node::find_node_by_id(nodes, r, nid) {
            if let Some(src) = nodes[idx].modifier.hoverable_interaction() {
                src.emit_hover_exit();
            }
        }
    }
}
