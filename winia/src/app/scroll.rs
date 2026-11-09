//! Scroll: the drag session, nested-scroll dispatch, and applying a delta to the tree.
//!
//! Split out of `app.rs`, which is the winit application; this is what a wheel or a drag does to
//! the node tree once the event has been routed there.

use super::*;

/// A drag-scroll session: a slot key (stable across recomposition), the last position, and velocity
/// samples (used for the fling on release). Both axes are tracked (a LazyRow's horizontal drag and
/// inertia use x); a sample is (t, x, y).
pub(crate) struct DragScroll {
    pub(crate) slot: u64,
    pub(crate) last_x: f32,
    pub(crate) last_y: f32,
    pub(crate) samples: Vec<(std::time::Instant, f32, f32)>,
}

impl DragScroll {
    /// Open a session at `pos`: the deltas that follow are measured from there. The node the
    /// session scrolls is re-resolved by slot key on every move (the layout tree is rebuilt
    /// between frames, so a node index would go stale).
    pub(crate) fn new(slot: u64, pos: (f32, f32)) -> Self {
        Self {
            slot,
            last_x: pos.0,
            last_y: pos.1,
            samples: vec![(std::time::Instant::now(), pos.0, pos.1)],
        }
    }
}

impl DragScroll {
    /// Finger velocity (px/s): the least-squares slope over the last ~200 ms.
    /// Note the x axis is "time since now" (larger means earlier), so the regression slope has the
    /// opposite sign to real time and is negated (measured: a 100 px upward drag read +650 rather
    /// than -650, and the fling went the wrong way).
    fn regression(&self, sel: fn(&(std::time::Instant, f32, f32)) -> f32) -> f32 {
        let now = std::time::Instant::now();
        let cutoff = now - std::time::Duration::from_millis(200);
        let pts: Vec<(f32, f32)> = self
            .samples
            .iter()
            .filter(|(t, _, _)| *t >= cutoff)
            .map(|s| (now.duration_since(s.0).as_secs_f32(), sel(s)))
            .collect();
        if pts.len() < 2 {
            return 0.0;
        }
        let n = pts.len() as f32;
        let sx: f32 = pts.iter().map(|p| p.0).sum();
        let sy: f32 = pts.iter().map(|p| p.1).sum();
        let sxy: f32 = pts.iter().map(|p| p.0 * p.1).sum();
        let sxx: f32 = pts.iter().map(|p| p.0 * p.0).sum();
        let denom = n * sxx - sx * sx;
        if denom.abs() < 1e-6 {
            return 0.0;
        }
        -(n * sxy - sx * sy) / denom
    }
    pub(crate) fn velocity_x(&self) -> f32 {
        self.regression(|s| s.1)
    }
    pub(crate) fn velocity_y(&self) -> f32 {
        self.regression(|s| s.2)
    }
}

/// Shift + wheel semantics: turn a vertical wheel delta into horizontal scrolling (direction kept,
/// vertical component cleared). This is for LazyRow-style horizontal containers; most platforms do
/// not turn Shift+wheel into dx on their own.
pub(crate) fn scroll_delta_with_shift(dx: f32, dy: f32, shift: bool) -> (f32, f32) {
    if shift { (dy, 0.0) } else { (dx, dy) }
}

pub(crate) fn dispatch_nested_scroll_delta(
    nodes: &mut [LayoutNode],
    root: usize,
    target: usize,
    delta: crate::nested_scroll::ScrollDelta,
    source: crate::nested_scroll::NestedScrollSource,
    density: crate::unit::Density,
) -> crate::nested_scroll::ScrollDelta {
    nested_scroll_delta(nodes, root, target, delta, source, density, true)
}

/// The same chain for an event the TARGET itself cannot use (a paging container with no page left):
/// ancestors still get their pre-scroll chance and their post-scroll report, but the target's own
/// offset is left alone. Nudging it is exactly what strands a paged container between two pages, and
/// dropping the event instead would stop a carousel inside a list from scrolling that list at its end.
pub(crate) fn dispatch_nested_scroll_delta_to_ancestors(
    nodes: &mut [LayoutNode],
    root: usize,
    target: usize,
    delta: crate::nested_scroll::ScrollDelta,
    source: crate::nested_scroll::NestedScrollSource,
    density: crate::unit::Density,
) -> crate::nested_scroll::ScrollDelta {
    nested_scroll_delta(nodes, root, target, delta, source, density, false)
}

fn nested_scroll_delta(
    nodes: &mut [LayoutNode],
    root: usize,
    target: usize,
    delta: crate::nested_scroll::ScrollDelta,
    source: crate::nested_scroll::NestedScrollSource,
    density: crate::unit::Density,
    consume_target: bool,
) -> crate::nested_scroll::ScrollDelta {
    fn path_to(nodes: &[LayoutNode], current: usize, target: usize, path: &mut Vec<usize>) -> bool {
        path.push(current);
        if current == target {
            return true;
        }
        for &child in &nodes[current].children {
            if path_to(nodes, child, target, path) {
                return true;
            }
        }
        path.pop();
        false
    }
    let mut path = Vec::new();
    if !path_to(nodes, root, target, &mut path) {
        return crate::nested_scroll::ScrollDelta::ZERO;
    }
    let mut remaining = delta;
    let mut total = crate::nested_scroll::ScrollDelta::ZERO;
    for &idx in &path {
        if let Some(connection) = nodes[idx].modifier.nested_scroll_connection() {
            #[cfg(debug_assertions)]
            if drag_trace_enabled() {
                eprintln!("[pre-scroll] idx={} connection=present", idx);
            }
            let part = connection
                .on_pre_scroll(remaining, source)
                .clamp_to(remaining);
            total = total + part;
            remaining = remaining - part;
        }
    }
    let child_consumed = if consume_target {
        apply_scroll_delta_inner(nodes, target, remaining.x, remaining.y, density, false)
    } else {
        crate::nested_scroll::ScrollDelta::ZERO
    };
    total = total + child_consumed;
    remaining = remaining - child_consumed;
    // post-scroll: ancestors from the inside out (**including target itself**: a TopAppBar-style
    // connection hangs off the scroll container node and relies on on_post_scroll to recolour
    // content_offset; excluding the target would break that, see
    // scaffold_demo/fixture_nested_scroll).
    for &idx in path.iter().rev() {
        if let Some(connection) = nodes[idx].modifier.nested_scroll_connection() {
            let part = connection
                .on_post_scroll(child_consumed, remaining, source)
                .clamp_to(remaining);
            total = total + part;
            remaining = remaining - part;
        }
    }
    total
}

pub(crate) fn dispatch_nested_scroll_fling(
    nodes: &mut [LayoutNode],
    root: usize,
    target: usize,
    velocity: crate::nested_scroll::ScrollVelocity,
) -> crate::nested_scroll::ScrollVelocity {
    fn path_to(nodes: &[LayoutNode], current: usize, target: usize, path: &mut Vec<usize>) -> bool {
        path.push(current);
        if current == target {
            return true;
        }
        for &child in &nodes[current].children {
            if path_to(nodes, child, target, path) {
                return true;
            }
        }
        path.pop();
        false
    }
    let mut path = Vec::new();
    if !path_to(nodes, root, target, &mut path) {
        return velocity;
    }

    let mut remaining = velocity;
    let mut consumed = crate::nested_scroll::ScrollVelocity::default();
    // pre-fling: the ancestors (including target) consume some velocity first, in forward path
    // order; the target's own connection takes part in pre too (Compose parity).
    for &idx in &path {
        if let Some(connection) = nodes[idx].modifier.nested_scroll_connection() {
            let part = connection.on_pre_fling(remaining).clamp_to(remaining);
            consumed.x += part.x;
            consumed.y += part.y;
            remaining.x -= part.x;
            remaining.y -= part.y;
        }
    }
    // child fling: the remaining velocity goes to the target scroll node; on hitting a boundary the
    // instantaneous remainder goes to the post-fling chain. That chain is the path in reverse
    // (**including target itself**: a TopAppBar-style connection hangs off the scroll container node
    // and relies on on_post_fling to spring back or reset; excluding the target would break that,
    // see scaffold_demo/fixture_nested_scroll).
    let child_velocity = remaining;
    // What the child actually consumed is the start velocity minus the remainder at the boundary,
    // computed inside the boundary callback (that is the instantaneous remainder on impact).
    // Do NOT pass the start velocity: on_post_fling's consumed_by_child means "how much the child
    // actually consumed", and a TopAppBar uses it to size the spring-back (review C1).
    let post_connections: Vec<std::sync::Arc<dyn crate::nested_scroll::NestedScrollConnection>> =
        path.iter()
            .rev()
            .filter_map(|&idx| nodes[idx].modifier.nested_scroll_connection())
            .collect();
    let child_started = {
        let node = &nodes[target];
        // The 50 px/s floor below is a DECAY floor — it stops a release that is really just the tail
        // of a drag from banking momentum. A container that settles must ignore it: a paging list's
        // whole motion is the snap spring, and a caller's own fling behaviour may want the release too,
        // so a slow drag past halfway that is let go with the finger nearly still has to still advance
        // a page (`wants_fling` covers both). Skipping the fling there left the list resting between two
        // pages for good, because nothing else ever snaps it back. Compose has no such floor —
        // `Scrollable.kt:857-881` calls `performFling` on every release.
        if let Some(ss) = node.modifier.vertical_scroll_state() {
            if child_velocity.y.abs() >= 50.0 || ss.wants_fling() {
                let post_connections = post_connections.clone();
                // A fling that dies at the end bends the content instead of just stopping (Compose
                // pipes the whole fling through the effect: Scrollable.kt:717). Reverse containers
                // skip it - their offsets are mirrored, so a signed pull would point the wrong way.
                let effect = if node.scroll_reverse {
                    None
                } else {
                    node.modifier.overscroll_effect().cloned()
                };
                ss.fling_with_boundary(child_velocity.y, move |remaining_velocity| {
                    if let Some(effect) = effect.as_ref() {
                        effect.pull_for_velocity(remaining_velocity);
                    }
                    // child consumption = start minus the boundary remainder (all of it when the remainder is 0).
                    let consumed_by_child = crate::nested_scroll::ScrollVelocity {
                        x: 0.0,
                        y: child_velocity.y - remaining_velocity,
                    };
                    let mut available = crate::nested_scroll::ScrollVelocity {
                        x: 0.0,
                        y: remaining_velocity,
                    };
                    for connection in &post_connections {
                        let part = connection.on_post_fling(consumed_by_child, available);
                        available.y -= crate::nested_scroll::ScrollVelocity { x: 0.0, y: part.y }
                            .clamp_to(available)
                            .y;
                    }
                });
                true
            } else {
                ss.is_scroll_in_progress.set(false);
                false
            }
        } else if let Some(ss) = node.modifier.horizontal_scroll_state() {
            if child_velocity.x.abs() >= 50.0 || ss.wants_fling() {
                let post_connections = post_connections.clone();
                // Note reverse (RTL) scrolling: the fling velocity must be mirrored just like the
                // gesture delta (apply_scroll_delta already mirrors the delta, so mirroring here keeps
                // the two consistent).
                let fling_vx = if node.scroll_reverse {
                    -child_velocity.x
                } else {
                    child_velocity.x
                };
                let effect = if node.scroll_reverse {
                    None
                } else {
                    node.modifier.overscroll_effect().cloned()
                };
                ss.fling_with_boundary(fling_vx, move |remaining_velocity| {
                    if let Some(effect) = effect.as_ref() {
                        effect.pull_for_velocity(remaining_velocity);
                    }
                    // child consumption = start minus the boundary remainder.
                    let consumed_by_child = crate::nested_scroll::ScrollVelocity {
                        x: child_velocity.x - remaining_velocity,
                        y: 0.0,
                    };
                    let mut available = crate::nested_scroll::ScrollVelocity {
                        x: remaining_velocity,
                        y: 0.0,
                    };
                    for connection in &post_connections {
                        let part = connection.on_post_fling(consumed_by_child, available);
                        available.x -= crate::nested_scroll::ScrollVelocity { x: part.x, y: 0.0 }
                            .clamp_to(available)
                            .x;
                    }
                });
                true
            } else {
                ss.is_scroll_in_progress.set(false);
                false
            }
        } else {
            false
        }
    };
    if child_started {
        consumed.x += child_velocity.x;
        consumed.y += child_velocity.y;
    }
    consumed
}

pub(crate) fn find_scroll_target(
    nodes: &[LayoutNode],
    idx: usize,
    dx: f32,
    dy: f32,
) -> Option<usize> {
    for &child in nodes[idx].children.iter().rev() {
        if let Some(target) = find_scroll_target(nodes, child, dx, dy) {
            return Some(target);
        }
    }
    if (dy != 0.0 && nodes[idx].modifier.vertical_scroll_state().is_some())
        || (dx != 0.0 && nodes[idx].modifier.horizontal_scroll_state().is_some())
    {
        Some(idx)
    } else {
        None
    }
}

#[allow(dead_code)] // the tests in this file call it
pub(crate) fn apply_scroll_delta(
    nodes: &mut [LayoutNode],
    idx: usize,
    dx: f32,
    dy: f32,
    density: crate::unit::Density,
) -> crate::nested_scroll::ScrollDelta {
    apply_scroll_delta_inner(nodes, idx, dx, dy, density, true)
}

/// The implementation of `apply_scroll_delta`. With `recursive=true`, a node that consumed nothing
/// itself (already at its boundary) falls back to recursing into its children (the old behaviour, the
/// fallback semantics of wheel/drag that lets an inner scroll consume). With `recursive=false` it
/// scrolls **the target only**: `dispatch_nested_scroll_delta`'s explicit-target semantics mean a
/// target that cannot scroll leaves the delta to the post chain's ancestor connections, and must
/// **not** quietly scroll a child (otherwise "drag down while the outer list is at its top" scrolled
/// the inner list, the user-reported bug).
fn apply_scroll_delta_inner(
    nodes: &mut [LayoutNode],
    idx: usize,
    dx: f32,
    dy: f32,
    density: crate::unit::Density,
    recursive: bool,
) -> crate::nested_scroll::ScrollDelta {
    let mut consumed = crate::nested_scroll::ScrollDelta::ZERO;
    {
        let node = &nodes[idx];
        #[cfg(debug_assertions)]
        if drag_trace_enabled() {
            eprintln!(
                "[apply-scroll] idx={} dy={} vp_h={} content_h={} has_vscroll={}",
                idx,
                dy,
                node.scroll_viewport_height,
                node.scroll_content_height,
                node.modifier.vertical_scroll_state().is_some()
            );
        }
        if dy != 0.0
            && let Some(state) = node.modifier.vertical_scroll_state()
        {
            // Manual input takes over: cancel the running fling and clear the in-progress flag (the drag path sets it again).
            crate::animation::cancel_animation(&state.offset);
            state.is_scroll_in_progress.set(false);
            let current = state.offset.get();
            #[cfg(debug_assertions)]
            if drag_trace_enabled() {
                eprintln!("[apply-scroll] -> state.offset before apply = {}", current);
            }
            // Scroll limit = total content height - visible height.
            // The visible size is the node's OWN measured height — what the user sees. It used to prefer
            // `scroll_viewport_height`, which is the padding-DEDUCTED measure constraint: for a menu
            // (a 112x424 container holding 20 items with 8dp of padding) that gave a range of
            // 976 - 408 = 568 where the end is 976 - 424 = 552, so the content scrolled 16px past its end
            // and the last item sat in dead space (24px with its own padding). Falls back to the viewport
            // while the node has not been measured yet, and to 0 (no movement) when neither is known.
            let visible_h = if node.measured_size.height > 0.0 {
                node.measured_size.height
            } else if node.scroll_viewport_height > 0.0 {
                node.scroll_viewport_height
            } else {
                node.modifier
                    .fixed_size()
                    .and_then(|(_, h)| {
                        use crate::layout::Dimension;
                        match h {
                            Dimension::Fixed(h) | Dimension::Dp(crate::unit::Dp(h)) => Some(h),
                            Dimension::Px(p) => Some(p.to_logical(density)),
                            _ => None,
                        }
                    })
                    .unwrap_or(0.0)
            };
            // Total content height: a lazy list (scroll_content_height > 0) uses its real content
            // height; otherwise the node's own height (an ordinary scroll container).
            let content_h = if node.scroll_content_height > 0.0 {
                node.scroll_content_height
            } else {
                node.measured_size.height
            };
            let max_offset = (content_h - visible_h).max(0.0);
            // `reverseScrolling` / `reverseLayout`: the render translate is mirrored (offset 0 is
            // the content's end), so the gesture delta is mirrored with it - a drag must still
            // follow the finger. The horizontal branch below has always done this; the vertical one
            // did not, which left plain reversed containers (and any reversed list) scrolling
            // against the drag.
            let raw = if node.scroll_reverse {
                current + dy
            } else {
                current - dy
            };
            let new = raw.clamp(0.0, max_offset);
            state.offset.set(new);
            consumed.y = current - new;
            // Overscroll: the delta past the edge is absorbed by the effect instead of falling
            // through to an ancestor. `reverse` mirrors the offset, so a signed pull would point
            // the wrong way — those containers skip it.
            let leftover = raw - new;
            if leftover != 0.0
                && !node.scroll_reverse
                && let Some(effect) = node.modifier.overscroll_effect()
            {
                // `pull` returns the absorbed amount in OFFSET space; `consumed` is in DELTA
                // space, and the two point opposite ways (`consumed = current - new`,
                // `leftover = raw - new`). Adding it would tell the nested-scroll ancestors the
                // gesture went the other way, twice as far.
                consumed.y -= effect.pull(leftover);
            }
        }
        // Horizontal scrolling (LazyRow / horizontal scroll container), symmetric to vertical:
        // a positive dx moves the content left.
        if dx != 0.0
            && let Some(state) = node.modifier.horizontal_scroll_state()
        {
            crate::animation::cancel_animation(&state.offset);
            state.is_scroll_in_progress.set(false);
            let current = state.offset.get();
            // Same as the vertical case above: the container's own measured width is what the range ends
            // against, with the viewport only as a not-measured-yet fallback.
            let visible_w = if node.measured_size.width > 0.0 {
                node.measured_size.width
            } else if node.scroll_viewport_width > 0.0 {
                node.scroll_viewport_width
            } else {
                node.modifier
                    .fixed_size()
                    .and_then(|(w, _)| {
                        use crate::layout::Dimension;
                        match w {
                            Dimension::Fixed(w) | Dimension::Dp(crate::unit::Dp(w)) => Some(w),
                            Dimension::Px(p) => Some(p.to_logical(density)),
                            _ => None,
                        }
                    })
                    .unwrap_or(0.0)
            };
            let content_w = if node.scroll_content_width > 0.0 {
                node.scroll_content_width
            } else {
                node.measured_size.width
            };
            let max_offset = (content_w - visible_w).max(0.0);
            // Note reverse (RTL) scrolling: the render-side offset semantics are mirrored (offset 0
            // is the end of the content), so the gesture delta must be mirrored too, or dragging goes
            // the wrong way (a user-measured bug). The vertical case above is the same.
            let raw = if node.scroll_reverse {
                current + dx
            } else {
                current - dx
            };
            let new = raw.clamp(0.0, max_offset);
            state.offset.set(new);
            consumed.x = current - new;
            let leftover = raw - new;
            if leftover != 0.0
                && !node.scroll_reverse
                && let Some(effect) = node.modifier.overscroll_effect()
            {
                consumed.x -= effect.pull(leftover);
            }
        }
    }
    if consumed.x != 0.0 || consumed.y != 0.0 {
        return consumed;
    }
    // Nothing consumed here: in recursive mode fall back to the children (the old fallback
    // semantics); the non-recursive dispatch path does not recurse, and leaves a stuck target to the
    // post chain.
    if !recursive {
        return consumed;
    }
    // Children (clone the indices first, so the recursion does not fight nodes' mutable borrow).
    let children: Vec<usize> = nodes[idx].children.clone();
    for c in children {
        let child = apply_scroll_delta_inner(nodes, c, dx, dy, density, true);
        if child.x != 0.0 || child.y != 0.0 {
            return child;
        }
    }
    consumed
}

/// One key press of keyboard scrolling: arrows move a line, Page keys a viewport, Home/End jump to
/// an end. Returns whether it scrolled anything, so the caller can fall through to focus navigation
/// when nothing in the focused node's ancestry can move.
///
/// Compose's `Scrollable` handles PageUp/PageDown only, one viewport per press
/// (`foundation/gestures/Scrollable.kt:428-471`); arrow keys there move FOCUS and the focused item is
/// brought into view. winia has no bring-into-view, so the arrows scroll instead - a desktop list that
/// only reacts to the wheel is the one gap a first-time user hits.
///
/// The step walks the focused node's ancestry from the inside out and stops at the first container
/// that can still move in that direction, so a nested list scrolls itself before its parent - the same
/// order a wheel takes.
pub(crate) fn scroll_by_key(
    nodes: &mut [LayoutNode],
    root: usize,
    start_idx: usize,
    key: &winit::keyboard::Key,
) -> bool {
    use winit::keyboard::{Key, NamedKey};
    /// One arrow press, in logical pixels (Windows scrolls three 16 px lines per notch).
    const LINE_STEP: f32 = 48.0;
    let Some(path) = node_path(nodes, root, start_idx) else {
        return false;
    };
    let density = crate::runtime::density::current_density();
    // The sign is the wheel's: a NEGATIVE vertical delta scrolls DOWN (offset grows).
    let step = match key {
        Key::Named(NamedKey::ArrowDown) => (0.0, -LINE_STEP),
        Key::Named(NamedKey::ArrowUp) => (0.0, LINE_STEP),
        Key::Named(NamedKey::ArrowLeft) => (LINE_STEP, 0.0),
        Key::Named(NamedKey::ArrowRight) => (-LINE_STEP, 0.0),
        Key::Named(NamedKey::Home) => (0.0, f32::MAX / 4.0),
        Key::Named(NamedKey::End) => (0.0, f32::MIN / 4.0),
        Key::Named(NamedKey::PageUp) | Key::Named(NamedKey::PageDown) => {
            let down = matches!(key, Key::Named(NamedKey::PageDown));
            // A page is the viewport of the container that will take it, so the step cannot be
            // resolved before the container is known; a placeholder is replaced per candidate.
            (0.0, if down { -1.0 } else { 1.0 })
        }
        _ => return false,
    };
    let is_page = matches!(
        key,
        Key::Named(NamedKey::PageUp) | Key::Named(NamedKey::PageDown)
    );
    for &idx in path.iter().rev() {
        let (max_offset, current, visible) = scroll_range(&nodes[idx]);
        let (mut dx, mut dy) = step;
        if is_page {
            let extent = if dy != 0.0 { visible.1 } else { visible.0 };
            if extent <= 0.0 {
                continue;
            }
            dy = if dy < 0.0 { -extent } else { extent };
        }
        if dx == 0.0 && dy == 0.0 {
            continue;
        }
        // Only the container that can still move in this direction takes the key.
        if dy != 0.0 {
            if nodes[idx].modifier.vertical_scroll_state().is_none() {
                continue;
            }
            if dy < 0.0 && current >= max_offset - 0.5 {
                continue;
            }
            if dy > 0.0 && current <= 0.5 {
                continue;
            }
        } else {
            if nodes[idx].modifier.horizontal_scroll_state().is_none() {
                continue;
            }
            if dx < 0.0 && current >= max_offset - 0.5 {
                continue;
            }
            if dx > 0.0 && current <= 0.5 {
                continue;
            }
        }
        // A key step stops at the edge: clamp it to the room that is left, so the spill cannot
        // feed the overscroll effect. Compose's key handling clamps too - only a POINTER gesture
        // bends the content; measured before this: ArrowDown 20 px from the end left the offset at
        // the end AND a 24.97 px stretch.
        let has_vertical = nodes[idx].modifier.vertical_scroll_state().is_some();
        let has_horizontal = nodes[idx].modifier.horizontal_scroll_state().is_some();
        let forward = (max_offset - current).max(0.0);
        let backward = current.max(0.0);
        if dy != 0.0 && has_vertical {
            dy = if dy < 0.0 {
                dy.max(-forward)
            } else {
                dy.min(backward)
            };
        } else if dx != 0.0 && has_horizontal && !has_vertical {
            // `scroll_range` reports the VERTICAL axis when a node has both, so a dual-axis node
            // keeps its unclamped step rather than clamping against the wrong numbers.
            dx = if dx < 0.0 {
                dx.max(-forward)
            } else {
                dx.min(backward)
            };
        }
        let consumed = dispatch_nested_scroll_delta(
            nodes,
            root,
            idx,
            crate::nested_scroll::ScrollDelta::new(dx, dy),
            crate::nested_scroll::NestedScrollSource::Drag,
            density,
        );
        return consumed.x != 0.0 || consumed.y != 0.0;
    }
    false
}

/// The node indices from `root` down to `target`, or `None` when `target` is not under `root`.
fn node_path(nodes: &[LayoutNode], root: usize, target: usize) -> Option<Vec<usize>> {
    fn walk(nodes: &[LayoutNode], current: usize, target: usize, path: &mut Vec<usize>) -> bool {
        path.push(current);
        if current == target {
            return true;
        }
        for &child in &nodes[current].children {
            if walk(nodes, child, target, path) {
                return true;
            }
        }
        path.pop();
        false
    }
    let mut path = Vec::new();
    walk(nodes, root, target, &mut path).then_some(path)
}

/// The window-level entry: the focused node takes the key, else the innermost node under the last
/// pointer position (a list nobody tabbed into is still what the last click aimed at). Both the real
/// key path and the debug `k` path call THIS, so a debug-driven arrow and a real one cannot diverge.
pub(crate) fn scroll_by_key_in_window(
    pw: &mut crate::app::per_window::PerWindow,
    key: &winit::keyboard::Key,
) -> bool {
    let Some(root) = pw.composer.layout_root_idx() else {
        return false;
    };
    // An INDEX, not an id: a node without one has id 0, and resolving 0 found the root - the arrow
    // then scrolled the page instead of the card the pointer was in (measured: two ArrowDown presses
    // moved the card 0 px in one run and 120 in another, depending on what the press had focused).
    let start_idx = pw
        .focused_id
        .and_then(|id| {
            let nodes = pw.composer.arena_nodes();
            crate::layout::node::path_to_id(nodes, root, id).and_then(|path| path.last().copied())
        })
        .or_else(|| {
            let pos = pw.input.last_pos?;
            let nodes = pw.composer.arena_nodes();
            crate::layout::node::hit_test(nodes, root, pos.0, pos.1)
                .last()
                .copied()
        });
    let Some(start_idx) = start_idx else {
        return false;
    };
    let nodes = pw.composer.arena_nodes_mut();
    scroll_by_key(nodes, root, start_idx, key)
}

/// `(max offset, current offset, visible extent)` for a node - the same numbers
/// `apply_scroll_delta_inner` works with, so a key step and a wheel notch agree about how far the
/// container can go. The axis choice follows the state that exists (vertical first).
fn scroll_range(node: &LayoutNode) -> (f32, f32, (f32, f32)) {
    let vertical = node.modifier.vertical_scroll_state();
    let state = vertical
        .clone()
        .or_else(|| node.modifier.horizontal_scroll_state());
    let Some(state) = state else {
        return (0.0, 0.0, (0.0, 0.0));
    };
    let vertical_axis = vertical.is_some();
    let visible = if vertical_axis {
        if node.measured_size.height > 0.0 {
            node.measured_size.height
        } else {
            node.scroll_viewport_height
        }
    } else if node.measured_size.width > 0.0 {
        node.measured_size.width
    } else {
        node.scroll_viewport_width
    };
    let content = if vertical_axis {
        if node.scroll_content_height > 0.0 {
            node.scroll_content_height
        } else {
            node.measured_size.height
        }
    } else if node.scroll_content_width > 0.0 {
        node.scroll_content_width
    } else {
        node.measured_size.width
    };
    let max = (content - visible).max(0.0);
    (max, state.offset.peek(), (visible, visible))
}
#[cfg(test)]
mod key_scroll_tests {
    use super::scroll_by_key;
    use crate::layout::Column;
    use crate::layout::node::LayoutNode;
    use crate::modifier::{Modifier, ScrollState};
    use crate::unit::Size;
    use winit::keyboard::{Key, NamedKey};

    fn key(named: NamedKey) -> Key {
        Key::Named(named)
    }

    /// root -> scroller(400x100 holding 400 px of content), the scroller is the focused node.
    fn one_list(offset: f32) -> (Vec<LayoutNode>, ScrollState) {
        let scroll = ScrollState::new();
        scroll.offset.set(offset);
        let mut nodes = vec![
            LayoutNode::leaf(Modifier::new().size(400.0, 100.0)),
            LayoutNode::leaf(
                Modifier::new()
                    .vertical_scroll(scroll.clone())
                    .size(400.0, 100.0),
            ),
        ];
        nodes[0].id = 1;
        nodes[1].id = 2;
        nodes[0].measured_size = Size::new(400.0, 100.0);
        nodes[0].children.push(1);
        nodes[1].measured_size = Size::new(400.0, 100.0);
        nodes[1].scroll_viewport_height = 100.0;
        nodes[1].scroll_content_height = 400.0;
        (nodes, scroll)
    }

    #[test]
    fn an_arrow_scrolls_one_line_and_a_page_scrolls_a_viewport() {
        let (mut nodes, scroll) = one_list(0.0);
        assert!(
            scroll_by_key(&mut nodes, 0, 1, &key(NamedKey::ArrowDown)),
            "the arrow scrolled"
        );
        assert_eq!(scroll.offset.peek(), 48.0, "one line down");

        assert!(scroll_by_key(&mut nodes, 0, 1, &key(NamedKey::PageDown)));
        assert_eq!(scroll.offset.peek(), 148.0, "a page is the viewport (100)");

        assert!(scroll_by_key(&mut nodes, 0, 1, &key(NamedKey::ArrowUp)));
        assert_eq!(scroll.offset.peek(), 100.0, "and back up one line");
    }

    /// A key step near the end stops AT the end and does not bend the content: the spill used to go
    /// into the overscroll effect (measured: 24.97 px of stretch from one ArrowDown).
    #[test]
    fn a_key_step_stops_at_the_end_without_bending_the_content() {
        let (mut nodes, scroll) = one_list(280.0);
        scroll.fling_limit.set(300.0);
        let effect = crate::overscroll::OverscrollEffect::new();
        nodes[1].modifier = nodes[1].modifier.clone().overscroll(&effect);
        assert!(scroll_by_key(&mut nodes, 0, 1, &key(NamedKey::ArrowDown)));
        assert_eq!(scroll.offset.peek(), 300.0, "the step stops at the end");
        assert_eq!(
            effect.stretch(),
            0.0,
            "and nothing spills into the rubber band"
        );
        crate::animation::clear_all_animations();
    }

    #[test]
    fn home_and_end_reach_the_ends() {
        let (mut nodes, scroll) = one_list(150.0);
        assert!(scroll_by_key(&mut nodes, 0, 1, &key(NamedKey::End)));
        assert_eq!(
            scroll.offset.peek(),
            300.0,
            "the end is content minus viewport"
        );
        assert!(scroll_by_key(&mut nodes, 0, 1, &key(NamedKey::Home)));
        assert_eq!(scroll.offset.peek(), 0.0);
    }

    #[test]
    fn a_key_with_nothing_left_to_scroll_falls_through() {
        let (mut nodes, scroll) = one_list(300.0);
        assert!(
            !scroll_by_key(&mut nodes, 0, 1, &key(NamedKey::ArrowDown)),
            "at the end the arrow must reach focus navigation"
        );
        assert_eq!(scroll.offset.peek(), 300.0);
        assert!(!scroll_by_key(&mut nodes, 0, 1, &key(NamedKey::End)));
    }

    #[test]
    fn unrelated_keys_and_missing_focus_do_nothing() {
        let (mut nodes, scroll) = one_list(0.0);
        assert!(!scroll_by_key(&mut nodes, 0, 1, &key(NamedKey::Enter)));
        assert!(
            !scroll_by_key(&mut nodes, 0, 0, &key(NamedKey::ArrowDown)),
            "a node outside any scroller"
        );
        assert!(
            !scroll_by_key(&mut nodes, 0, 99, &key(NamedKey::ArrowDown)),
            "an index that is not in the tree"
        );
        assert_eq!(scroll.offset.peek(), 0.0);
    }

    #[test]
    fn an_inner_list_at_its_end_passes_the_key_to_the_outer_one() {
        // root -> outer(0..200) -> inner(0..60, already at its end), focus inside the inner one.
        let outer = ScrollState::new();
        let inner = ScrollState::new();
        inner.offset.set(60.0);
        let mut nodes = vec![
            LayoutNode::leaf(Modifier::new().size(400.0, 100.0)),
            LayoutNode::leaf(
                Modifier::new()
                    .vertical_scroll(outer.clone())
                    .size(400.0, 100.0),
            ),
            LayoutNode::leaf(
                Modifier::new()
                    .vertical_scroll(inner.clone())
                    .size(400.0, 50.0),
            ),
        ];
        nodes[0].id = 1;
        nodes[1].id = 2;
        nodes[2].id = 3;
        nodes[0].measured_size = Size::new(400.0, 100.0);
        nodes[1].measured_size = Size::new(400.0, 100.0);
        nodes[1].scroll_content_height = 300.0;
        nodes[2].measured_size = Size::new(400.0, 50.0);
        nodes[2].scroll_content_height = 110.0;
        nodes[1].children.push(2);
        nodes[0].children.push(1);

        assert!(scroll_by_key(&mut nodes, 0, 2, &key(NamedKey::ArrowDown)));
        assert_eq!(inner.offset.peek(), 60.0, "the inner list is at its end");
        assert_eq!(outer.offset.peek(), 48.0, "so the outer one scrolls");
    }
}
#[cfg(test)]
mod nested_scroll_chain_tests {
    use super::dispatch_nested_scroll_delta;
    use crate::layout::node::LayoutNode;
    use crate::modifier::{Modifier, ScrollState};
    use crate::nested_scroll::{
        NestedScrollConnection, NestedScrollSource, ScrollDelta, ScrollVelocity,
    };
    use crate::unit::{Offset, Size};

    /// A recorder connection: logs the call order of on_pre_scroll/on_post_scroll. pre consumes half
    /// and post consumes all of available, which makes both the order and the consumed amount easy to
    /// check. The optional `global_log` records the **cross-node** order ("pre-R", "post-T" and so
    /// on); a per-node log can only verify that node's own order and cannot capture
    /// pre-R -> M -> T -> post-T -> M -> R.
    struct Recorder {
        name: String,
        log: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
        global_log: Option<std::sync::Arc<std::sync::Mutex<Vec<String>>>>,
        order: std::sync::atomic::AtomicUsize,
    }
    impl Recorder {
        fn with_global(
            name: &str,
            log: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
            global: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
        ) -> Self {
            Self {
                name: name.to_string(),
                log,
                global_log: Some(global),
                order: std::sync::atomic::AtomicUsize::new(0),
            }
        }
    }
    impl NestedScrollConnection for Recorder {
        fn on_pre_scroll(&self, available: ScrollDelta, _: NestedScrollSource) -> ScrollDelta {
            let n = self.order.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.log
                .lock()
                .unwrap()
                .push(format!("pre-{}-{}", self.name, n));
            if let Some(g) = &self.global_log {
                g.lock().unwrap().push(format!("pre-{}", self.name));
            }
            ScrollDelta::new(available.x / 2.0, available.y / 2.0)
        }
        fn on_post_scroll(
            &self,
            _: ScrollDelta,
            available: ScrollDelta,
            _: NestedScrollSource,
        ) -> ScrollDelta {
            let n = self.order.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.log
                .lock()
                .unwrap()
                .push(format!("post-{}-{}", self.name, n));
            if let Some(g) = &self.global_log {
                g.lock().unwrap().push(format!("post-{}", self.name));
            }
            ScrollDelta::new(available.x, available.y)
        }
        fn on_pre_fling(&self, _: ScrollVelocity) -> ScrollVelocity {
            ScrollVelocity::default()
        }
        fn on_post_fling(&self, _: ScrollVelocity, _: ScrollVelocity) -> ScrollVelocity {
            ScrollVelocity::default()
        }
    }

    #[test]
    fn an_overscrolled_target_reaches_its_ancestor_with_nothing_left() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        crate::animation::clear_all_animations();
        struct PostTap(std::sync::Arc<std::sync::Mutex<ScrollDelta>>);
        impl NestedScrollConnection for PostTap {
            fn on_post_scroll(
                &self,
                _child_consumed: ScrollDelta,
                available: ScrollDelta,
                _source: NestedScrollSource,
            ) -> ScrollDelta {
                *self.0.lock().unwrap() = available;
                ScrollDelta::ZERO
            }
        }
        let mailbox = std::sync::Arc::new(std::sync::Mutex::new(ScrollDelta::ZERO));
        let effect = crate::overscroll::OverscrollEffect::new();
        let scroll = ScrollState::new();
        scroll.offset.set(200.0); // at the end of a 300-content / 100-viewport list
        let mut nodes = vec![
            LayoutNode::leaf(
                Modifier::new()
                    .nested_scroll(PostTap(mailbox.clone()))
                    .size(400.0, 400.0),
            ),
            LayoutNode::leaf(
                Modifier::new()
                    .vertical_scroll(scroll.clone())
                    .overscroll(&effect)
                    .size(100.0, 100.0),
            ),
        ];
        nodes[0].measured_size = Size::new(400.0, 400.0);
        nodes[1].measured_size = Size::new(100.0, 100.0);
        nodes[1].scroll_viewport_height = 100.0;
        nodes[1].scroll_content_height = 300.0;
        nodes[0].children.push(1);

        let consumed = dispatch_nested_scroll_delta(
            &mut nodes,
            0,
            1,
            ScrollDelta::new(0.0, -10.0),
            NestedScrollSource::Drag,
            crate::unit::Density::from_density(1.0),
        );
        assert_eq!(consumed.y, -10.0, "the whole gesture is accounted for");
        assert!(
            (effect.stretch() - 9.6).abs() < 0.3,
            "and it became the (damped) rubber-band pull, got {}",
            effect.stretch()
        );
        assert_eq!(
            mailbox.lock().unwrap().y,
            0.0,
            "the ancestor must not see the absorbed pull (nor its opposite)"
        );
        crate::animation::clear_all_animations();
    }

    #[test]
    fn delta_chain_pre_post_order_includes_target() {
        // Build root(connection R) -> mid(connection M) -> target(scroll + connection T).
        let r_log = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let m_log = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let t_log = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        // A shared global log, to verify the cross-node order pre-R -> M -> T -> post-T -> M -> R.
        let global = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));

        let conn_r = Recorder::with_global("R", r_log.clone(), global.clone());
        let conn_m = Recorder::with_global("M", m_log.clone(), global.clone());
        let conn_t = Recorder::with_global("T", t_log.clone(), global.clone());

        let scroll = ScrollState::new();
        scroll.offset.set(0.0);
        let mut nodes = vec![
            LayoutNode::leaf(Modifier::new().nested_scroll(conn_r).size(200.0, 200.0)),
            LayoutNode::leaf(Modifier::new().nested_scroll(conn_m).size(200.0, 200.0)),
            LayoutNode::leaf(
                Modifier::new()
                    .vertical_scroll(scroll.clone())
                    .nested_scroll(conn_t)
                    .size(100.0, 100.0),
            ),
        ];
        nodes[0].measured_size = Size::new(200.0, 200.0);
        nodes[1].measured_size = Size::new(200.0, 200.0);
        nodes[1].position = Offset::new(0.0, 0.0);
        nodes[2].measured_size = Size::new(100.0, 100.0);
        nodes[2].position = Offset::new(0.0, 100.0);
        nodes[2].scroll_viewport_height = 100.0;
        nodes[2].scroll_content_height = 200.0; // 100 of scrollable range
        nodes[0].children.push(1);
        nodes[1].children.push(2);

        let density = crate::unit::Density::from_density(1.0);
        // Consumption walk-through (dy=-20; a negative delta moves the content up, i.e. raises the
        // offset; each pre eats half):
        //   R pre -10 -> M pre -5 -> T pre -2.5 -> child left -2.5 -> child consumes 2.5
        //   post chain (including T): M post eats all of available -> T post eats all -> R post eats the rest
        let consumed = dispatch_nested_scroll_delta(
            &mut nodes,
            0,
            2,
            ScrollDelta::new(0.0, -20.0),
            NestedScrollSource::Wheel,
            density,
        );

        // R: one pre (eats 10) plus one post (after the child consumes, eats the rest); R is the last post.
        assert_eq!(
            *r_log.lock().unwrap(),
            vec!["pre-R-0", "post-R-1"],
            "R runs pre then post"
        );
        // M: one pre plus one post, before T and R (reverse order M -> T -> R).
        assert_eq!(
            *m_log.lock().unwrap(),
            vec!["pre-M-0", "post-M-1"],
            "M runs pre then post"
        );
        // T: pre plus post (a TopAppBar-style connection hangs off the target, so post must be
        // called; this was the key regression before the fix, where excluding the target broke the
        // content_offset recolour and the spring-back).
        assert_eq!(
            *t_log.lock().unwrap(),
            vec!["pre-T-0", "post-T-1"],
            "T takes part in post too (a TopAppBar relies on it)"
        );

        // consumed should be negative (consumed in the negative delta direction), and the child has
        // actually scrolled, so offset > 0.
        assert!(
            consumed.y < 0.0,
            "consumption should follow the delta direction, got {:?}",
            consumed
        );
        assert!(
            scroll.offset.get() > 0.0,
            "the child should actually scroll (pre only eats half), got {}",
            scroll.offset.get()
        );

        // Cross-node global order: pre forward R -> M -> T, post reverse T -> M -> R (including target T).
        let g = global.lock().unwrap().clone();
        assert_eq!(
            g,
            vec!["pre-R", "pre-M", "pre-T", "post-T", "post-M", "post-R"],
            "the global order should be pre-R -> M -> T -> post-T -> M -> R (target T takes part in post), got {g:?}"
        );
    }
}

#[cfg(test)]
mod release_velocity_floor_tests {
    use super::dispatch_nested_scroll_fling;
    use crate::fling::SnapInfo;
    use crate::layout::node::LayoutNode;
    use crate::modifier::{Modifier, ScrollState};
    use crate::nested_scroll::ScrollVelocity;
    use crate::unit::{Offset, Size};

    const STEP: f32 = 336.0;
    /// Below the 50 px/s floor the call site uses for the DECAY, and below the snap's own 400 dp/s
    /// threshold too — so the snap that runs here is the "settle on the nearer page" branch, which is
    /// the one a slow drag-and-release needs.
    const GENTLE: f32 = 12.0;

    /// A root with one scrollable child, mid-page so the snap has somewhere to go.
    ///
    /// The offset matters: at an exact page boundary the snap target equals the current offset and the
    /// spring is never registered (`push_animatable_with_velocity_and_done` short-circuits on
    /// `peek() == target`), which would make a `has_animation` assertion vacuous.
    fn tree(snap: Option<SnapInfo>, offset: f32) -> (Vec<LayoutNode>, ScrollState) {
        let scroll = ScrollState::new();
        scroll.offset.set(offset);
        scroll.fling_limit.set(STEP * 9.0);
        scroll.snap.set(snap);
        tree_with(scroll)
    }

    /// The same tree around a caller-configured state (a custom fling behaviour, say).
    fn tree_with(scroll: ScrollState) -> (Vec<LayoutNode>, ScrollState) {
        let mut nodes = vec![
            LayoutNode::leaf(Modifier::new().size(400.0, 600.0)),
            LayoutNode::leaf(
                Modifier::new()
                    .vertical_scroll(scroll.clone())
                    .size(400.0, STEP),
            ),
        ];
        nodes[0].measured_size = Size::new(400.0, 600.0);
        nodes[1].measured_size = Size::new(400.0, STEP);
        nodes[1].position = Offset::new(0.0, 0.0);
        nodes[1].scroll_viewport_height = STEP;
        nodes[1].scroll_content_height = STEP * 10.0;
        nodes[0].children.push(1);
        (nodes, scroll)
    }

    fn snap_spec() -> SnapInfo {
        SnapInfo {
            step: STEP,
            min_fling_velocity: 400.0,
            ..Default::default()
        }
    }

    /// A paged list released below the 50 px/s floor still flings — because its whole motion is the
    /// snap spring, and the floor was written for a decay it does not run.
    ///
    /// This is the half no unit test reached before: `ScrollState::fling` enters `fling_with_boundary`
    /// directly and never crosses this call site, so the existing
    /// `a_paged_list_settles_even_when_it_is_released_at_rest` is green whether or not the bypass here
    /// exists. Without `|| ss.snaps()`, the release below is dropped before any fling starts and the
    /// list comes to rest between two pages with nothing left to snap it back.
    #[test]
    fn a_paged_list_flings_below_the_decay_floor() {
        let _g = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();

        // Mid-page, so the nearer-page snap target is a real move.
        let (mut nodes, scroll) = tree(Some(snap_spec()), STEP * 0.6);
        dispatch_nested_scroll_fling(&mut nodes, 0, 1, ScrollVelocity { x: 0.0, y: GENTLE });

        assert!(
            crate::animation::has_animation_for_state(scroll.offset.state_id()),
            "a paging list released at {GENTLE} px/s must still run its snap spring — the 50 px/s \
             floor is the decay's, and this list has no decay phase"
        );

        // And it is the SNAP that ran, not a decay: one page, on the boundary.
        let mut frames = 0;
        while crate::animation::has_animation_for_state(scroll.offset.state_id()) && frames < 400 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(10));
            frames += 1;
        }
        let end = scroll.offset.get();
        assert!(
            (end / STEP - (end / STEP).round()).abs() < 1e-3,
            "the gentle release settled at {end}, which is not a page boundary — a decay would stop \
             wherever the friction ran out"
        );
        assert!(
            end >= STEP,
            "and it advanced to the nearer page ahead ({end} should be at least one page in)"
        );
    }

    /// The 50 px/s floor is keyed on `wants_fling`, so a container carrying its own fling behaviour
    /// (and no snap spec) is asked about a release it would otherwise never see.
    #[test]
    fn a_custom_strategy_flings_below_the_decay_floor() {
        let _g = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();

        #[derive(Debug)]
        struct LandOnTwoPages;
        impl crate::fling::FlingBehavior for LandOnTwoPages {
            fn plan(&self, _fling: crate::fling::FlingContext) -> crate::fling::FlingPlan {
                crate::fling::FlingPlan::spring(
                    STEP * 2.0,
                    crate::animation::SpringSpec {
                        stiffness: crate::animation::SpringSpec::STIFFNESS_EFFECTS_DEFAULT,
                        ..Default::default()
                    },
                )
            }
        }

        let mut scroll = ScrollState::new();
        scroll.offset.set(STEP * 0.2);
        scroll.fling_limit.set(STEP * 9.0);
        scroll.fling_behavior = Some(std::sync::Arc::new(LandOnTwoPages));
        let (mut nodes, scroll) = tree_with(scroll);
        dispatch_nested_scroll_fling(&mut nodes, 0, 1, ScrollVelocity { x: 0.0, y: GENTLE });

        assert!(
            crate::animation::has_animation_for_state(scroll.offset.state_id()),
            "a container with its own fling behaviour must be asked about a {GENTLE} px/s release"
        );
        crate::animation::clear_all_animations();
    }

    /// The control, and the reason the test above is about the exception rather than about the number:
    /// the SAME velocity on a container with no snap spec is still dropped by the floor, so a plain
    /// scroll view does not bank a fling out of the tail of a slow drag.
    #[test]
    fn an_ordinary_container_keeps_the_decay_floor() {
        let _g = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();

        let (mut nodes, scroll) = tree(None, STEP * 0.6);
        dispatch_nested_scroll_fling(&mut nodes, 0, 1, ScrollVelocity { x: 0.0, y: GENTLE });

        assert!(
            !crate::animation::has_animation_for_state(scroll.offset.state_id()),
            "without a snap spec a {GENTLE} px/s release must start nothing — the 50 px/s floor still \
             guards the decay, which is what it was written for"
        );
        assert_eq!(
            scroll.offset.get(),
            STEP * 0.6,
            "and the offset is untouched"
        );

        // The floor is a floor, not a wall: the same container released fast does fling.
        dispatch_nested_scroll_fling(&mut nodes, 0, 1, ScrollVelocity { x: 0.0, y: 900.0 });
        assert!(
            crate::animation::has_animation_for_state(scroll.offset.state_id()),
            "a fast release on an ordinary container still flings"
        );
    }
}

/// The demo made it look like `.padding(16).vertical_scroll(state)` never scrolled while the reverse
/// order did. It was a measurement artifact - the tree reports LAYOUT positions, and a scroll offset
/// is applied when painting, so reading text positions cannot see a scroll at all. Both orders give
/// the same scroll range; this test keeps that true.
#[cfg(test)]
mod modifier_order_tests {
    use crate::layout::Column;
    use crate::layout::Constraints;
    use crate::layout::node::LayoutNode;
    use crate::modifier::{Modifier, ScrollState};
    use crate::runtime::composer::Composer;

    fn run(scroll_first: bool) -> (f32, f32, f32, f32) {
        let mut c = Composer::new();
        let scroll = ScrollState::new();
        let s = scroll.clone();
        c.compose(move |ctx| {
            let mut m = Modifier::new().fill_max_size();
            m = if scroll_first {
                m.vertical_scroll(s.clone()).padding(16.0)
            } else {
                m.padding(16.0).vertical_scroll(s.clone())
            };
            Column::new().modifier(m).build(ctx, |ctx| {
                for i in 0..20 {
                    crate::components::Text::new(format!("row {i}"))
                        .modifier(Modifier::new().height(40.0))
                        .build(ctx);
                }
            });
        });
        c.layout(Constraints::new(0.0, 400.0, 0.0, 300.0));
        let nodes: &[LayoutNode] = c.arena_nodes();
        let node = nodes
            .iter()
            .find(|n| n.modifier.vertical_scroll_state().is_some())
            .expect("a scroll node");
        (
            node.measured_size.width,
            node.measured_size.height,
            node.scroll_content_height,
            node.scroll_viewport_height,
        )
    }

    #[test]
    fn both_modifier_orders_give_the_same_scroll_range() {
        let scroll_first = run(true);
        let pad_first = run(false);
        assert_eq!(
            scroll_first, pad_first,
            "the order of padding and vertical_scroll must not matter"
        );
        // 20 rows of 40 inside a 300-tall viewport, 16 px of padding on each side.
        assert_eq!(scroll_first.2, 832.0, "content height");
        assert_eq!(scroll_first.3, 268.0, "viewport height (300 - 32)");
        assert!(
            scroll_first.2 > scroll_first.3,
            "and the content really overflows"
        );
    }
}
#[cfg(test)]
mod vertical_reverse_tests {
    use super::apply_scroll_delta;
    use crate::runtime::composer::Composer;
    use crate::unit::Density;

    fn container(reverse: bool) -> (Composer, crate::modifier::ScrollState) {
        let scroll = crate::modifier::ScrollState::new();
        let s = scroll.clone();
        let mut composer = Composer::new();
        composer.compose(crate::compose!(|ctx| {
            crate::layout::Column::new()
                .modifier(
                    crate::modifier::Modifier::new()
                        .size(100.0, 100.0)
                        .vertical_scroll(s.clone())
                        .vertical_scroll_reverse(reverse),
                )
                .build(ctx, |ctx| {
                    crate::components::Text::new("content")
                        .modifier(crate::modifier::Modifier::new().size(100.0, 300.0))
                        .build(ctx);
                });
        }));
        composer.layout(crate::layout::Constraints::new(0.0, 100.0, 0.0, 100.0));
        (composer, scroll)
    }

    /// The reverse setters used to assume the scroll element was LAST, but `vertical_scroll` /
    /// `horizontal_scroll` append the ambient `Overscroll` element after it - so the flag was
    /// silently dropped (the tab row's RTL call site was affected).
    #[test]
    fn a_reverse_flag_survives_the_ambient_overscroll_element() {
        let vertical = crate::modifier::Modifier::new()
            .vertical_scroll(crate::modifier::ScrollState::new())
            .vertical_scroll_reverse(true);
        assert_eq!(vertical.is_vertical_scroll_reversed(), Some(true));
        assert!(
            vertical.overscroll_effect().is_some(),
            "the ambient effect is still there"
        );

        let horizontal = crate::modifier::Modifier::new()
            .horizontal_scroll(crate::modifier::ScrollState::new())
            .horizontal_scroll_reverse(true);
        assert_eq!(horizontal.is_horizontal_scroll_reversed(), Some(true));
    }

    /// Compose's `verticalScroll(reverseScrolling = true)`: offset 0 shows the END, and the gesture
    /// delta is mirrored so dragging still follows the finger.
    #[test]
    fn a_reversed_vertical_container_mirrors_the_delta() {
        let (mut forward, fwd) = container(false);
        apply_scroll_delta(
            forward.arena_nodes_mut(),
            0,
            0.0,
            -10.0,
            Density::from_density(1.0),
        );
        assert_eq!(
            fwd.offset.peek(),
            10.0,
            "a normal container moves on a negative delta"
        );

        let (mut reversed, rev) = container(true);
        let root = reversed.layout_root_idx().expect("root");
        assert!(
            reversed.arena_nodes()[root].scroll_reverse,
            "the flag reaches the node during measure"
        );
        apply_scroll_delta(
            reversed.arena_nodes_mut(),
            root,
            0.0,
            10.0,
            Density::from_density(1.0),
        );
        assert_eq!(
            rev.offset.peek(),
            10.0,
            "the reversed one takes the opposite sign"
        );
    }
}
