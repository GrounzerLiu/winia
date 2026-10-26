//! Gesture routing: turning a pointer path into `GestureAction`s and firing them.
//!
//! Split out of `app.rs`. The arena is the overlay that owns the keyboard or the pointer; a gesture
//! that starts on it is routed there rather than to the page below.

use super::*;

/// Gesture action → node callback (coordinates converted to component-local — mirrors Compose onTap's local offset).
/// `scene_pos` is the event position; the tap/drag action family carries its own `down_pos` — the coordinates
/// come from the position the action carries (after moving within the slop, the up position differs from the down
/// position, and using up would be off).
/// Returns whether it was consumed (a callback ran).
/// What the pointer is doing: the press that may become a click, the gesture the press opened, and
/// the sessions that gesture started.
///
/// These were fourteen fields on `PerWindow`, every one of them widened to `pub(crate)` so this
/// module could read it. They move together — the tracker, the node and slot it belongs to, the
/// arena it lives in, the axis it locked onto, the scroll ancestor it may hand itself to, and the
/// taps waiting out a double-tap window. The window keeps the frame; the pointer keeps this.
pub(crate) struct GestureState {
    /// Pointer down state (Compose-style click detection).
    pub(crate) down: Option<PtrDownState>,
    /// Most recent PointerKind (Move events inherit it from the last Down).
    pub(crate) last_kind: crate::input::PointerKind,
    /// Scene coordinates of the last PointerMoved (logical pixels) — used to hit the scroll target for
    /// MouseWheel (§3.7: winit 0.31 MouseWheel events carry no cursor position, so the pointer position
    /// must be recorded; cleared by the caller on PointerLeft).
    pub(crate) last_pos: Option<(f32, f32)>,
    /// Innermost node ID at Down (subsequent Move/Up go to this node first rather than to hit_test).
    pub(crate) down_slot: Option<u64>,
    /// Gesture state machine (a single active gesture — created on down, destroyed on up/cancel).
    pub(crate) tracker: Option<crate::input::gesture::GestureTracker>,
    /// Gesture callback routing: the gesture node id.
    pub(crate) node: Option<u64>,
    /// Double-tap context (the previous tap's node/time/position — carried across gestures;
    /// bound to a node — gestures on different nodes do not share the double-tap count).
    pub(crate) tap_ctx: Option<(u64, std::time::Instant, (f32, f32))>,
    /// slot_key of the gesture node (stable across recomposition — node_id changes, so find_node_by_id fails).
    pub(crate) slot: Option<u64>,
    /// Arena of the node the current gesture belongs to: `None` = the main tree, `Some(overlay id)`
    /// = that popup's layer. The id (not the index) is stored because an overlay can be removed
    /// while a gesture — or a deferred tap — is still in flight.
    pub(crate) arena: Option<u64>,
    /// Dominant axis the current gesture locked onto: decided once, on the first move that is
    /// decisive, and kept for the rest of the gesture (see `gesture_move`). `None` while undecided.
    pub(crate) axis: Option<crate::input::gesture::ScrollAxis>,
    /// Scroll ancestor recorded for the current gesture when the press landed on an inner drag
    /// component (a swipeable row inside a list): if the finger turns out to move along the
    /// scroll's axis, the gesture is handed to that node and the component's drag is cancelled.
    /// `None` for a press with no scroll on its hit path.
    pub(crate) scroll_slot: Option<u64>,
    /// The arena's screen origin, FROZEN when the gesture started. The gesture measures displacement
    /// against it, so an overlay that MOVES under the finger must not add its own motion: the expanded
    /// `SearchBar` slides for `SEARCH_BAR_EXPAND_MS` and is pressable while it moves, and reading the
    /// live origin turned a stationary finger into a slop-exceeding drag that cancelled the tap
    /// (`a_tap_survives_its_own_popup_moving`).
    pub(crate) arena_origin: (f32, f32),
    /// Drag-scroll session (the press landed on a scroll container: content follows the pointer, release flings by velocity).
    pub(crate) drag_scroll: Option<DragScroll>,
    /// Deferred tap list (when the node registers on_double_tap — Compose semantics: onTap is deferred until
    /// the double-tap window ends; a second down on the same node inside the window → cancel; on timeout → fire;
    /// pendings for different nodes are independent — quick successive taps on several gesture nodes each fire at their own deadline).
    pub(crate) pending_taps: Vec<crate::input::gesture::PendingTap>,
}

impl Default for GestureState {
    fn default() -> Self {
        Self {
            down: None,
            last_kind: crate::input::PointerKind::Mouse {
                button: crate::input::PointerButton::Primary,
            },
            last_pos: None,
            down_slot: None,
            tracker: None,
            node: None,
            tap_ctx: None,
            slot: None,
            arena: None,
            axis: None,
            scroll_slot: None,
            arena_origin: (0.0, 0.0),
            drag_scroll: None,
            pending_taps: Vec::new(),
        }
    }
}

/// Intermediate state of Compose-style click detection.
pub(crate) struct PtrDownState {
    pub(crate) node_id: u64,
    pub(crate) position: (f32, f32),
    pub(crate) time: std::time::Instant,
    /// Start character offset of the text selection (recorded on Down).
    pub(crate) selection_anchor: Option<usize>,
    /// SelectionContainer registrar at Down time (so a drag across containers does not switch offset spaces).
    pub(crate) anchor_registrar: Option<crate::text::selection::SelectionRegistrar>,
}

pub(crate) fn fire_gesture_action(
    nodes: &[crate::layout::node::LayoutNode],
    root: usize,
    slot_key: u64,
    action: crate::input::gesture::GestureAction,
) -> bool {
    // ⚠ Resolve the node by slot_key (stable across recomposition) — node_id changes after recomposition.
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

/// Index of the innermost scroll container on the hit path (the first vertical/horizontal scroll state from rev).
pub(crate) fn path_scroll_idx(nodes: &[LayoutNode], path: &[usize]) -> Option<usize> {
    path.iter().rev().find(|&&n| {
        // A container whose input is disabled (a list with `user_scroll(false)`) keeps clipping and
        // programmatic scrolling but must not swallow the gesture: it falls through to its parent.
        !nodes[n].modifier.is_scroll_input_disabled()
            && (nodes[n].modifier.vertical_scroll_state().is_some()
                || nodes[n].modifier.horizontal_scroll_state().is_some())
    }).copied()
}

/// Index of the innermost drag-gesture component on the hit path (has_drag_gesture).
pub(crate) fn path_drag_idx(nodes: &[LayoutNode], path: &[usize]) -> Option<usize> {
    path.iter().rev().find(|&&n| nodes[n].modifier.has_drag_gesture()).copied()
}

/// The core "content scrolling wins" decision (matching Compose): an inner draggable component (a slider/switch inside
/// a scroll) wins over scrolling; when the drag is an ancestor of the scroll (e.g. the BottomSheet panel's on_drag) → scrolling wins.
/// Shared by three call sites: handle_pointer_down's child_drag, gesture_down's target choice, and overlay_down.
pub(crate) fn inner_component_drag(drag: Option<usize>, scroll: Option<usize>) -> bool {
    match (drag, scroll) {
        (Some(d), Some(s)) => d > s, // larger drag index = closer to the leaf = the component is inside the scroll
        (Some(_), None) => true,     // no scroll container: a standalone draggable component (do not downgrade)
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
fn gesture_arena_overlay(pw: &PerWindow, arena: Option<u64>) -> Option<usize> {
    let id = arena?;
    pw.overlay.layers.iter().position(|o| o.id == id)
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
fn gesture_arena_pos(pw: &PerWindow, scene_pos: (f32, f32)) -> Option<(f32, f32)> {
    match pw.input.arena {
        None => Some(scene_pos),
        Some(id) => {
            // The arena must still exist (its composer is where the action lands)...
            pw.overlay.layers.iter().find(|o| o.id == id)?;
            // ...but the conversion uses the origin frozen at press time, not the live one: see
            // `gesture_arena_origin`.
            Some((
                scene_pos.0 - pw.input.arena_origin.0,
                scene_pos.1 - pw.input.arena_origin.1,
            ))
        }
    }
}

/// Fire a gesture action at `slot` inside `arena` — the arena the gesture target belongs to, passed
/// explicitly because the callers clear `pw.input.arena` (the gesture is over) before dispatching
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
        Some(id) => match pw.overlay.layers.iter().position(|o| o.id == id) {
            Some(i) => {
                let ov = &pw.overlay.layers[i];
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
        Some(id) => match pw.overlay.layers.iter().position(|o| o.id == id) {
            Some(i) => {
                let ov = &pw.overlay.layers[i];
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
    for t in std::mem::take(&mut pw.input.pending_taps) {
        use crate::input::gesture::PendingTapAction as A;
        match crate::input::gesture::pending_tap_on_down(&t, now, node_id) {
            A::Fire => pw.fire_pending_tap(t),
            A::Cancel => {}
            A::Keep => kept.push(t),
        }
    }
    pw.input.pending_taps = kept;
}

/// Pointer-down gesture entry point: hit test to find the innermost gesture node → create the tracker (capture semantics —
/// later move/up are routed by gesture_node, so the pointer still reaches it after leaving the component) → fire on_press immediately.
pub(crate) fn gesture_down(pw: &mut PerWindow, scene_pos: (f32, f32)) {
    // Resolve the gesture target first (the borrow is released as soon as the block ends — `pw` is borrowed mutably below for the pending tap).
    let hit = {
        let nodes = pw.composer.arena_nodes();
        let Some(r) = pw.composer.layout_root_idx() else { return; };
        let path = hit_test_with_flights(nodes, r, pw.composer.transition_roots(), scene_pos.0, scene_pos.1);
        press_gesture_target(nodes, &path)
    };
    let Some((node_id, slot, has_drag)) = hit else {
        #[cfg(debug_assertions)]
        if drag_trace_enabled() {
            eprintln!("[gesture-down] no gesture target at {scene_pos:?}");
        }
        return;
    };
    #[cfg(debug_assertions)]
    if drag_trace_enabled() {
        eprintln!("[gesture-down] target id={node_id} slot={slot} has_drag={has_drag} at {scene_pos:?}");
    }

    process_pending_taps_on_down(pw, node_id);

    // The double-tap context is isolated per node (Compose per-pointerInput semantics) — different nodes do not share it.
    let ctx = pw.input.tap_ctx.take()
        .filter(|(n, _, _)| *n == node_id)
        .map(|(_, t, p)| (t, p));
    pw.input.tracker = Some(crate::input::gesture::GestureTracker::new(node_id, scene_pos, has_drag, ctx));
    pw.input.node = Some(node_id);
    pw.input.slot = Some(slot);
    pw.input.arena = None; // the main tree
    pw.input.axis = None;
    pw.input.scroll_slot = None;
    pw.input.arena_origin = (0.0, 0.0);
    // Fire on_press immediately (local coordinates).
    let nodes = pw.composer.arena_nodes();
    let Some(r) = pw.composer.layout_root_idx() else { return; };
    fire_gesture_action(nodes, r, slot, crate::input::gesture::GestureAction::Press(scene_pos));
}

/// Pointer-move gesture entry point: route whenever a tracker exists (capture — independent of hit testing).
///
/// A press can be claimed by two owners at once — an inner drag component (a swipeable row, a
/// slider) and the scroll container it sits in — and the finger's dominant axis decides which one
/// keeps the gesture (Compose arbitrates in the same place: each drag detector waits for the touch
/// slop along its own orientation, so the direction the finger moves first decides the owner).
/// The decision is taken ONCE and holds for the rest of the gesture: handing over mid-gesture
/// would mean replaying the deltas the component already consumed.
pub(crate) fn gesture_move(pw: &mut PerWindow, scene_pos: (f32, f32)) -> bool {
    let Some(slot) = pw.input.slot else { return false; };
    let Some(local) = gesture_arena_pos(pw, scene_pos) else {
        end_gesture(pw); // the target's overlay vanished mid-drag
        return false;
    };
    let mut axis_undecided = false;
    if pw.input.axis.is_none() {
        if let Some((scroll_slot, down)) = pw
            .input
            .scroll_slot
            .zip(pw.input.tracker.as_ref().map(|t| t.down_position()))
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
                    pw.input.axis = Some(axis);
                    if axis == ScrollAxis::Vertical {
                        // The scroll ancestor owns it: cancel the component's drag, so no
                        // `on_drag_*` callback fires for a gesture it did not get, and open a
                        // scroll session on the ancestor node — the rest of the gesture (and its
                        // fling) is the ordinary drag-scroll path from here on. A target with no drag
                        // of its own has nothing to cancel; the call is then a no-op.
                        if let Some(t) = pw.input.tracker.as_mut() {
                            t.cancel_drag_for_arbitration();
                        }
                        pw.input.drag_scroll = Some(DragScroll::new(scroll_slot, scene_pos));
                    }
                }
            }
        }
    }
    if pw.input.axis == Some(crate::input::gesture::ScrollAxis::Vertical) {
        return false; // the scroll session owns the rest of the gesture
    }
    let allow_drag = !axis_undecided
        && (pw.input.scroll_slot.is_none()
            || pw.input.axis == Some(crate::input::gesture::ScrollAxis::Horizontal));
    #[cfg(debug_assertions)]
    if drag_trace_enabled() {
        eprintln!("[gesture-move] local={local:?} axis_undecided={axis_undecided} scroll_slot={:?} allow_drag={allow_drag}", pw.input.scroll_slot);
    }
    let action = {
        let Some(t) = pw.input.tracker.as_mut() else { return false; };
        t.on_move(local, allow_drag)
    };
    #[cfg(debug_assertions)]
    if drag_trace_enabled() {
        eprintln!("[gesture-move] action={action:?}");
    }
    if action == crate::input::gesture::GestureAction::None {
        return false;
    }
    let arena = pw.input.arena;
    fire_in_gesture_arena(pw, arena, slot, action)
}

/// Drop the gesture state (tracker + target) without firing anything.
pub(crate) fn end_gesture(pw: &mut PerWindow) {
    pw.input.tracker = None;
    pw.input.node = None;
    pw.input.slot = None;
    pw.input.arena = None;
    pw.input.axis = None;
    pw.input.scroll_slot = None;
    pw.input.arena_origin = (0.0, 0.0);
}

/// Drag-scroll end: enough velocity → inertial fling (content velocity = -finger velocity — flicking the finger up
/// keeps the content moving up, i.e. the offset grows). Not enough velocity → just clear the scrolling marker.
pub(crate) fn drag_scroll_up(pw: &mut PerWindow) {
    let Some(ds) = pw.input.drag_scroll.take() else { eprintln!("[DBG-DS] up but no drag_scroll"); return };
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
    // Finger velocity → scroll velocity (content velocity = -finger velocity), through the nested scroll pre/post fling chain.
    let velocity = crate::nested_scroll::ScrollVelocity { x: -vx, y: -vy };
    let _ = dispatch_nested_scroll_fling(pw.composer.arena_nodes_mut(), root, idx, velocity);
}

/// Pointer-up gesture entry point: decide on up (tap/double-tap/long-press/drag-end) → destroy the tracker.
pub(crate) fn gesture_up(pw: &mut PerWindow, _scene_pos: (f32, f32)) -> bool {
    let Some(slot) = pw.input.slot else { return false; };
    let Some(gid) = pw.input.node else { return false; };
    // The tracker recorded positions in the target's arena, so its action carries arena-local
    // coordinates already; the arena itself decides where the action is dispatched — captured here
    // because `end_gesture` clears it before the dispatch below.
    let arena = pw.input.arena;
    if arena.is_some() && gesture_arena_overlay(pw, arena).is_none() {
        end_gesture(pw); // the target's overlay vanished mid-gesture
        return false;
    }
    let action = {
        let Some(mut t) = pw.input.tracker.take() else { return false; };
        // ⚠ on_up must run first (the Tap branch records last_tap) and only then read tap_context —
        // in the reverse order the double-tap context is always None (ctx is read before the up decision).
        let action = t.on_up();
        pw.input.tap_ctx = t.tap_context().map(|(t, p)| (gid, t, p));
        action
    };
    end_gesture(pw);
    if action == crate::input::gesture::GestureAction::None {
        return false;
    }
    // ⚠ Compose detectTapGestures semantics: when the node registers onDoubleTap, onTap is deferred
    // until the double-tap window ends — a second up on the same node inside the window → fire DoubleTap only (the first tap
    // was already cancelled on the second down); on timeout/pressing another node → fire the deferred Tap (fire_pending_tap).
    if let crate::input::gesture::GestureAction::Tap(pos) = action {
        if slot_has_double_tap(pw, arena, slot) {
            pw.input.pending_taps.push(crate::input::gesture::PendingTap::new(slot, gid, pos, arena));
            return false;
        }
        return fire_in_gesture_arena(pw, arena, slot, action);
    }
    if matches!(action, crate::input::gesture::GestureAction::DoubleTap(_)) {
        // Double-tap hit: the first tap's pending should already have been cancelled on the second down — defensively clear leftovers for this node.
        pw.input.pending_taps.retain(|t| t.node_id != gid);
    }
    fire_in_gesture_arena(pw, arena, slot, action)
}
