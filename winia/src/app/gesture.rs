//! Gesture routing: turning a pointer path into `GestureAction`s and firing them.
//!
//! Split out of `app.rs`. The arena is the overlay that owns the keyboard or the pointer; a gesture
//! that starts on it is routed there rather than to the page below.

use super::*;

/// 手势动作 → 节点回调（坐标转组件本地——对标 Compose onTap 的本地 offset）。
/// `scene_pos` 是事件位置；tap/drag 系列动作自带 `down_pos`——坐标用动作
/// 携带的位置（slop 内移动后 up 位置与按下位置不同，用 up 会偏）。
/// 返回是否消费（有回调执行）。
pub(crate) fn fire_gesture_action(
    nodes: &[crate::layout::node::LayoutNode],
    root: usize,
    slot_key: u64,
    action: crate::input::gesture::GestureAction,
) -> bool {
    // ⚠ 用 slot_key 解析节点（跨重组稳定）——node_id 在重组后会变
    let Some(nid) = crate::layout::node::find_node_id_by_slot_key(nodes, root, slot_key) else {
        return false;
    };
    let Some(idx) = crate::layout::node::find_node_by_id(nodes, root, nid) else {
        return false;
    };
    let (ax, ay) = node_abs_position(nodes, root, nid);
    use crate::input::gesture::GestureAction as G;
    use crate::modifier::ModifierElement as E;
    let mut fired = false;
    for el in nodes[idx].modifier.elements() {
        match (el, action) {
            (E::TapOnPress { cb }, G::Press(p)) => { (cb)((p.0 - ax, p.1 - ay)); fired = true; }
            (E::TapOnTap { cb }, G::Tap(p)) => { (cb)((p.0 - ax, p.1 - ay)); fired = true; }
            (E::TapOnDoubleTap { cb }, G::DoubleTap(p)) => {
                (cb)((p.0 - ax, p.1 - ay));
                fired = true;
            }
            (E::TapOnLongPress { cb }, G::LongPress(p)) => { (cb)((p.0 - ax, p.1 - ay)); fired = true; }
            (E::DragOnStart { cb }, G::DragStart(p)) => { (cb)((p.0 - ax, p.1 - ay)); fired = true; }
            (E::DragOnMove { cb }, G::DragMove(p, delta)) => {
                (cb)((p.0 - ax, p.1 - ay), delta);
                fired = true;
            }
            (E::DragOnEnd { cb }, G::DragEnd) => {
                (cb)(); fired = true;
            }
            (E::DragOnCancel { cb }, G::DragCancel) => { (cb)(); fired = true; }
            _ => {}
        }
    }
    fired
}

/// 命中路径上最内层滚动容器下标（rev 第一个 vertical/horizontal scroll state）
pub(crate) fn path_scroll_idx(nodes: &[LayoutNode], path: &[usize]) -> Option<usize> {
    path.iter().rev().find(|&&n| {
        nodes[n].modifier.vertical_scroll_state().is_some()
            || nodes[n].modifier.horizontal_scroll_state().is_some()
    }).copied()
}

/// 命中路径上最内层拖拽手势组件下标（has_drag_gesture）
pub(crate) fn path_drag_idx(nodes: &[LayoutNode], path: &[usize]) -> Option<usize> {
    path.iter().rev().find(|&&n| nodes[n].modifier.has_drag_gesture()).copied()
}

/// 「内容滚动优先」核心判定（对齐 Compose）：内层可拖组件（slider/switch 在 scroll
/// 内部）优先于滚动；drag 是 scroll 祖先（如 BottomSheet 面板 on_drag）→ 滚动优先。
/// 三处共享：handle_pointer_down 的 child_drag、gesture_down 目标选择、overlay_down。
pub(crate) fn inner_component_drag(drag: Option<usize>, scroll: Option<usize>) -> bool {
    match (drag, scroll) {
        (Some(d), Some(s)) => d > s, // drag 下标更大 = 更靠叶 = 组件在 scroll 内部
        (Some(_), None) => true,     // 无滚动容器：独立的拖拽组件（不降级）
        _ => false,
    }
}

/// Press-gesture target on a hit path, as `(node_id, slot_key, has_drag)`: the innermost
/// node that can receive a press.
///
/// A drag gesture that is an ANCESTOR of a scroll container is skipped, so the panel
/// `on_drag` of a bottom sheet does not build a drag tracker when a press lands in the list
/// inside it — the two systems would run at once and the sheet would follow the finger before
/// the list had scrolled to its end. An inner gesture component (a slider or a switch inside
/// the scroll) still wins, because drag deeper than scroll means the component. This is
/// Compose's "content scrolling wins" rule, and the main tree (`gesture_down`) and the overlay
/// path (`overlay_down`) share it here so a popup behaves like the rest of the app.
pub(crate) fn press_gesture_target(
    nodes: &[crate::layout::node::LayoutNode],
    path: &[usize],
) -> Option<(u64, u64, bool)> {
    let scroll_idx = path_scroll_idx(nodes, path);
    let gid = path.iter().rev()
        .find(|&&i| {
            if !nodes[i].modifier.has_gesture() { return false; }
            if !nodes[i].modifier.has_drag_gesture() { return true; } // non-drag gestures (tap) are never skipped
            inner_component_drag(Some(i), scroll_idx)
        })
        .copied()?;
    let n = &nodes[gid];
    Some((n.id, n.slot_key, n.modifier.has_drag_gesture()))
}

/// Overlay index of a gesture arena — `None` for the main tree, and for a target whose overlay has
/// since been removed (the gesture then has nowhere to land).
///
/// A `closing` overlay is deliberately still resolved: unlike a press (`hit_overlay` treats a fading
/// overlay as transparent) the finger is already down and its nodes are still composed, so the
/// gesture completes into the popup and dies with it when `finish_closing_overlays` removes it.
pub(crate) fn gesture_arena_overlay(pw: &PerWindow, arena: Option<u64>) -> Option<usize> {
    let id = arena?;
    pw.overlays.iter().position(|o| o.id == id)
}

/// Convert a window position into the gesture target's arena space: unchanged for the main tree,
/// LAYER-LOCAL inside a popup — a gesture callback receives coordinates local to the arena it was
/// resolved in (`fire_gesture_action` subtracts the node's position there), so the tracker has to
/// measure and report in the same space. `None` once the target's overlay is gone.
///
/// ⚠ The conversion uses the overlay's CURRENT `screen_pos`, so an overlay that MOVES during a
/// gesture injects its own motion into the measured position — enough to cross the 8 px tap slop and
/// cancel the tap family. The expanded `SearchBar` is exactly that: an anchored panel sliding to the
/// window corner over `SEARCH_BAR_EXPAND_MS`, pressable while it moves (`anchor_slide`). Nothing
/// shipped is visibly affected (its content is a `TextField` whose `on_press` already fired plus
/// `clickable` rows that use `on_click`), but a component that wants a tap to survive its own
/// overlay's motion would need the arena origin frozen at press time.
pub(crate) fn gesture_arena_pos(pw: &PerWindow, scene_pos: (f32, f32)) -> Option<(f32, f32)> {
    match pw.gesture_arena {
        None => Some(scene_pos),
        Some(id) => {
            // The arena must still exist (its composer is where the action lands)...
            pw.overlays.iter().find(|o| o.id == id)?;
            // ...but the conversion uses the origin frozen at press time, not the live one: see
            // `gesture_arena_origin`.
            Some((
                scene_pos.0 - pw.gesture_arena_origin.0,
                scene_pos.1 - pw.gesture_arena_origin.1,
            ))
        }
    }
}

/// Fire a gesture action at `slot` inside `arena` — the arena the gesture target belongs to, passed
/// explicitly because the callers clear `pw.gesture_arena` (the gesture is over) before dispatching
/// its last action. The main tree and each overlay have their own composer, so an action is routed
/// together with the arena it was produced for — the same split `press_gesture_target` follows on
/// the way in. `None` means the MAIN TREE; an arena that no longer resolves (the popup is gone) is
/// NOT the main tree — the action is dropped instead, which is the whole point of carrying the arena.
pub(crate) fn fire_in_gesture_arena(
    pw: &mut PerWindow,
    arena: Option<u64>,
    slot: u64,
    action: crate::input::gesture::GestureAction,
) -> bool {
    match arena {
        None => {
            let nodes = pw.composer.arena_nodes();
            let Some(r) = pw.composer.layout_root_idx() else { return false; };
            fire_gesture_action(nodes, r, slot, action)
        }
        Some(id) => match pw.overlays.iter().position(|o| o.id == id) {
            Some(i) => {
                let ov = &pw.overlays[i];
                let nodes = ov.composer.arena_nodes();
                let Some(r) = ov.composer.layout_root_idx() else { return false; };
                fire_gesture_action(nodes, r, slot, action)
            }
            None => false,
        },
    }
}

/// Is `on_double_tap` registered on `slot`, in `arena`? The answer decides whether a tap is deferred
/// to the double-tap window (see `gesture_up`).
///
/// `None` means the main tree only, matching `fire_in_gesture_arena`: an arena that no longer
/// resolves answers `false` rather than consulting the main tree (the caller's vanished-arena guard
/// has already ended the gesture; the tap that follows is dropped on dispatch).
pub(crate) fn slot_has_double_tap(pw: &PerWindow, arena: Option<u64>, slot: u64) -> bool {
    let (nodes, r) = match arena {
        None => (pw.composer.arena_nodes(), pw.composer.layout_root_idx()),
        Some(id) => match pw.overlays.iter().position(|o| o.id == id) {
            Some(i) => {
                let ov = &pw.overlays[i];
                (ov.composer.arena_nodes(), ov.composer.layout_root_idx())
            }
            None => return false,
        },
    };
    let Some(r) = r else { return false; };
    crate::layout::node::find_node_id_by_slot_key(nodes, r, slot)
        .and_then(|nid| crate::layout::node::find_node_by_id(nodes, r, nid))
        .map(|idx| nodes[idx].modifier.has_double_tap())
        .unwrap_or(false)
}

/// Pending-tap bookkeeping for a new press on `node_id` (Compose double-tap semantics, rules in
/// `pending_tap_on_down`): a due tap is fired into the arena it was recorded in, a window hit on the
/// same node cancels it (the up decides double-tap), anything else keeps waiting for its deadline.
pub(crate) fn process_pending_taps_on_down(pw: &mut PerWindow, node_id: u64) {
    let now = std::time::Instant::now();
    let mut kept = Vec::new();
    for t in std::mem::take(&mut pw.pending_taps) {
        use crate::input::gesture::PendingTapAction as A;
        match crate::input::gesture::pending_tap_on_down(&t, now, node_id) {
            A::Fire => pw.fire_pending_tap(t),
            A::Cancel => {}
            A::Keep => kept.push(t),
        }
    }
    pw.pending_taps = kept;
}

/// 指针按下手势入口：hit test 找最内层手势节点 → 创建 tracker（capture 语义——
/// 后续 move/up 由 gesture_node 路由，指针移出组件仍接收）→ on_press 立即触发。
pub(crate) fn gesture_down(pw: &mut PerWindow, scene_pos: (f32, f32)) {
    // 先解析手势目标（借用结束即释放——后面要可变借用 pw 处理 pending tap）
    let hit = {
        let nodes = pw.composer.arena_nodes();
        let Some(r) = pw.composer.layout_root_idx() else { return; };
        let path = hit_test_with_flights(nodes, r, pw.composer.transition_roots(), scene_pos.0, scene_pos.1);
        press_gesture_target(nodes, &path)
    };
    let Some((node_id, slot, has_drag)) = hit else {
        return;
    };

    process_pending_taps_on_down(pw, node_id);

    // 双击上下文按节点隔离（Compose per-pointerInput 语义）——不同节点不共享
    let ctx = pw.gesture_tap_ctx.take()
        .filter(|(n, _, _)| *n == node_id)
        .map(|(_, t, p)| (t, p));
    pw.gesture = Some(crate::input::gesture::GestureTracker::new(node_id, scene_pos, has_drag, ctx));
    pw.gesture_node = Some(node_id);
    pw.gesture_slot = Some(slot);
    pw.gesture_arena = None; // the main tree
    pw.gesture_axis = None;
    pw.gesture_scroll_slot = None;
    pw.gesture_arena_origin = (0.0, 0.0);
    // on_press 立即触发（本地坐标）
    let nodes = pw.composer.arena_nodes();
    let Some(r) = pw.composer.layout_root_idx() else { return; };
    fire_gesture_action(nodes, r, slot, crate::input::gesture::GestureAction::Press(scene_pos));
}

/// 指针移动手势入口：tracker 存在即路由（capture——不依赖 hit test）。
///
/// A press can be claimed by two owners at once — an inner drag component (a swipeable row, a
/// slider) and the scroll container it sits in — and the finger's dominant axis decides which one
/// keeps the gesture (Compose arbitrates in the same place: each drag detector waits for the touch
/// slop along its own orientation, so the direction the finger moves first decides the owner).
/// The decision is taken ONCE and holds for the rest of the gesture: handing over mid-gesture
/// would mean replaying the deltas the component already consumed.
pub(crate) fn gesture_move(pw: &mut PerWindow, scene_pos: (f32, f32)) -> bool {
    let Some(slot) = pw.gesture_slot else { return false; };
    let Some(local) = gesture_arena_pos(pw, scene_pos) else {
        end_gesture(pw); // the target's overlay vanished mid-drag
        return false;
    };
    let mut axis_undecided = false;
    if pw.gesture_axis.is_none() {
        if let Some((scroll_slot, down)) = pw
            .gesture_scroll_slot
            .zip(pw.gesture.as_ref().map(|t| t.down_position()))
        {
            // The scroll is armed whenever the press passed over an inner drag component — the
            // target of the press may still be a plain tap node inside it (a `TextField` inside a
            // `Slider`, say), and that case needs the arbitration just as much: without it a vertical
            // drag over such a press scrolls nothing and reaches nothing.
            use crate::input::gesture::ScrollAxis;
            match ScrollAxis::classify(local.0 - down.0, local.1 - down.1) {
                // Undecided (below the slop, or an exact diagonal): neither owner starts YET. The move
                // still reaches the tracker with the drag withheld, so crossing the touch slop keeps
                // cancelling the tap family exactly as it did before the arbitration existed.
                None => axis_undecided = true,
                Some(axis) => {
                    pw.gesture_axis = Some(axis);
                    if axis == ScrollAxis::Vertical {
                        // The scroll ancestor owns it: cancel the component's drag, so no
                        // `on_drag_*` callback fires for a gesture it did not get, and open a
                        // scroll session on the ancestor node — the rest of the gesture (and its
                        // fling) is the ordinary drag-scroll path from here on. A target with no drag
                        // of its own has nothing to cancel; the call is then a no-op.
                        if let Some(t) = pw.gesture.as_mut() {
                            t.cancel_drag_for_arbitration();
                        }
                        pw.drag_scroll = Some(DragScroll::new(scroll_slot, scene_pos));
                    }
                }
            }
        }
    }
    if pw.gesture_axis == Some(crate::input::gesture::ScrollAxis::Vertical) {
        return false; // the scroll session owns the rest of the gesture
    }
    let allow_drag = !axis_undecided
        && (pw.gesture_scroll_slot.is_none()
            || pw.gesture_axis == Some(crate::input::gesture::ScrollAxis::Horizontal));
    let action = {
        let Some(t) = pw.gesture.as_mut() else { return false; };
        t.on_move(local, allow_drag)
    };
    if action == crate::input::gesture::GestureAction::None {
        return false;
    }
    let arena = pw.gesture_arena;
    fire_in_gesture_arena(pw, arena, slot, action)
}

/// Drop the gesture state (tracker + target) without firing anything.
pub(crate) fn end_gesture(pw: &mut PerWindow) {
    pw.gesture = None;
    pw.gesture_node = None;
    pw.gesture_slot = None;
    pw.gesture_arena = None;
    pw.gesture_axis = None;
    pw.gesture_scroll_slot = None;
    pw.gesture_arena_origin = (0.0, 0.0);
}

/// 拖拽滚动结束：速度足够 → 惯性 fling（内容速度 = -手指速度——手指向上甩
/// 内容继续向上 = offset 增大）。速度不足 → 仅结束滚动中标记。
pub(crate) fn drag_scroll_up(pw: &mut PerWindow) {
    let Some(ds) = pw.drag_scroll.take() else { eprintln!("[DBG-DS] up but no drag_scroll"); return };
    let (vx, vy) = (ds.velocity_x(), ds.velocity_y());
    let target: Option<usize> = (|| {
        let nodes = pw.composer.arena_nodes();
        let Some(r) = pw.composer.layout_root_idx() else { return None };
        let id = crate::layout::node::find_node_id_by_slot_key(nodes, r, ds.slot)?;
        crate::layout::node::find_node_by_id(nodes, r, id)
    })();
    #[cfg(debug_assertions)]
    if drag_trace_enabled() {
        eprintln!("[drag-up] slot={:?} target_idx={:?} v=({},{})",
            ds.slot, target, vx, vy);
    }
    let Some(idx) = target else { return };
    let Some(root) = pw.composer.layout_root_idx() else { return };
    // 手指速度 → 滚动速度（内容速度 = -手指速度），并走 nested scroll pre/post fling 链
    let velocity = crate::nested_scroll::ScrollVelocity { x: -vx, y: -vy };
    let _ = dispatch_nested_scroll_fling(pw.composer.arena_nodes_mut(), root, idx, velocity);
}

/// 指针释放手势入口：up 判定（tap/double-tap/long-press/drag-end）→ 销毁 tracker。
pub(crate) fn gesture_up(pw: &mut PerWindow, _scene_pos: (f32, f32)) -> bool {
    let Some(slot) = pw.gesture_slot else { return false; };
    let Some(gid) = pw.gesture_node else { return false; };
    // The tracker recorded positions in the target's arena, so its action carries arena-local
    // coordinates already; the arena itself decides where the action is dispatched — captured here
    // because `end_gesture` clears it before the dispatch below.
    let arena = pw.gesture_arena;
    if arena.is_some() && gesture_arena_overlay(pw, arena).is_none() {
        end_gesture(pw); // the target's overlay vanished mid-gesture
        return false;
    }
    let action = {
        let Some(mut t) = pw.gesture.take() else { return false; };
        // ⚠ 必须先 on_up（Tap 分支记录 last_tap）再取 tap_context——
        // 顺序颠倒则双击上下文恒 None（ctx 在 up 判定前读取）
        let action = t.on_up();
        pw.gesture_tap_ctx = t.tap_context().map(|(t, p)| (gid, t, p));
        action
    };
    end_gesture(pw);
    if action == crate::input::gesture::GestureAction::None {
        return false;
    }
    // ⚠ Compose detectTapGestures 语义：节点注册 onDoubleTap 时，onTap 延迟
    // 到双击窗口结束——窗口内第二次 up 命中双击 → 只发 DoubleTap（第一次 tap
    // 已在第二次 down 时取消）；超时/按下其他节点 → 补发 Tap（fire_pending_tap）
    if let crate::input::gesture::GestureAction::Tap(pos) = action {
        if slot_has_double_tap(pw, arena, slot) {
            pw.pending_taps.push(crate::input::gesture::PendingTap::new(slot, gid, pos, arena));
            return false;
        }
        return fire_in_gesture_arena(pw, arena, slot, action);
    }
    if matches!(action, crate::input::gesture::GestureAction::DoubleTap(_)) {
        // 双击命中：第一次 tap 的 pending 应已在第二次 down 时取消——防御性清理同节点残留
        pw.pending_taps.retain(|t| t.node_id != gid);
    }
    fire_in_gesture_arena(pw, arena, slot, action)
}
