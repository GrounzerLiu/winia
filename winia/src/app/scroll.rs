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
        let pts: Vec<(f32, f32)> = self.samples
            .iter()
            .filter(|(t, _, _)| *t >= cutoff)
            .map(|s| (now.duration_since(s.0).as_secs_f32(), sel(s)))
            .collect();
        if pts.len() < 2 { return 0.0; }
        let n = pts.len() as f32;
        let sx: f32 = pts.iter().map(|p| p.0).sum();
        let sy: f32 = pts.iter().map(|p| p.1).sum();
        let sxy: f32 = pts.iter().map(|p| p.0 * p.1).sum();
        let sxx: f32 = pts.iter().map(|p| p.0 * p.0).sum();
        let denom = n * sxx - sx * sx;
        if denom.abs() < 1e-6 { return 0.0; }
        -(n * sxy - sx * sy) / denom
    }
    pub(crate) fn velocity_x(&self) -> f32 { self.regression(|s| s.1) }
    pub(crate) fn velocity_y(&self) -> f32 { self.regression(|s| s.2) }
}

/// Shift + wheel semantics: turn a vertical wheel delta into horizontal scrolling (direction kept,
/// vertical component cleared). This is for LazyRow-style horizontal containers; most platforms do
/// not turn Shift+wheel into dx on their own.
pub(crate) fn scroll_delta_with_shift(dx: f32, dy: f32, shift: bool) -> (f32, f32) {
    if shift {
        (dy, 0.0)
    } else {
        (dx, dy)
    }
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
        if current == target { return true; }
        for &child in &nodes[current].children {
            if path_to(nodes, child, target, path) { return true; }
        }
        path.pop();
        false
    }
    let mut path = Vec::new();
    if !path_to(nodes, root, target, &mut path) { return crate::nested_scroll::ScrollDelta::ZERO; }
    let mut remaining = delta;
    let mut total = crate::nested_scroll::ScrollDelta::ZERO;
    for &idx in &path {
        if let Some(connection) = nodes[idx].modifier.nested_scroll_connection() {
            #[cfg(debug_assertions)]
            if drag_trace_enabled() {
                eprintln!("[pre-scroll] idx={} connection=present", idx);
            }
            let part = connection.on_pre_scroll(remaining, source).clamp_to(remaining);
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
            let part = connection.on_post_scroll(child_consumed, remaining, source).clamp_to(remaining);
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
        if current == target { return true; }
        for &child in &nodes[current].children {
            if path_to(nodes, child, target, path) { return true; }
        }
        path.pop();
        false
    }
    let mut path = Vec::new();
    if !path_to(nodes, root, target, &mut path) { return velocity; }

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
        path.iter().rev()
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
                ss.fling_with_boundary(child_velocity.y, move |remaining_velocity| {
                    // child consumption = start minus the boundary remainder (all of it when the remainder is 0).
                    let consumed_by_child = crate::nested_scroll::ScrollVelocity {
                        x: 0.0,
                        y: child_velocity.y - remaining_velocity,
                    };
                    let mut available = crate::nested_scroll::ScrollVelocity { x: 0.0, y: remaining_velocity };
                    for connection in &post_connections {
                        let part = connection.on_post_fling(consumed_by_child, available);
                        available.y -= crate::nested_scroll::ScrollVelocity { x: 0.0, y: part.y }.clamp_to(available).y;
                    }
                });
                true
            } else { ss.is_scroll_in_progress.set(false); false }
        } else if let Some(ss) = node.modifier.horizontal_scroll_state() {
            if child_velocity.x.abs() >= 50.0 || ss.wants_fling() {
                let post_connections = post_connections.clone();
                // Note reverse (RTL) scrolling: the fling velocity must be mirrored just like the
                // gesture delta (apply_scroll_delta already mirrors the delta, so mirroring here keeps
                // the two consistent).
                let fling_vx = if node.scroll_reverse { -child_velocity.x } else { child_velocity.x };
                ss.fling_with_boundary(fling_vx, move |remaining_velocity| {
                    // child consumption = start minus the boundary remainder.
                    let consumed_by_child = crate::nested_scroll::ScrollVelocity {
                        x: child_velocity.x - remaining_velocity,
                        y: 0.0,
                    };
                    let mut available = crate::nested_scroll::ScrollVelocity { x: remaining_velocity, y: 0.0 };
                    for connection in &post_connections {
                        let part = connection.on_post_fling(consumed_by_child, available);
                        available.x -= crate::nested_scroll::ScrollVelocity { x: part.x, y: 0.0 }.clamp_to(available).x;
                    }
                });
                true
            } else { ss.is_scroll_in_progress.set(false); false }
        } else { false }
    };
    if child_started {
        consumed.x += child_velocity.x;
        consumed.y += child_velocity.y;
    }
    consumed
}

pub(crate) fn find_scroll_target(nodes: &[LayoutNode], idx: usize, dx: f32, dy: f32) -> Option<usize> {
    for &child in nodes[idx].children.iter().rev() {
        if let Some(target) = find_scroll_target(nodes, child, dx, dy) { return Some(target); }
    }
    if (dy != 0.0 && nodes[idx].modifier.vertical_scroll_state().is_some())
        || (dx != 0.0 && nodes[idx].modifier.horizontal_scroll_state().is_some()) {
        Some(idx)
    } else { None }
}

#[allow(dead_code)] // the tests in this file call it
pub(crate) fn apply_scroll_delta(nodes: &mut [LayoutNode], idx: usize, dx: f32, dy: f32, density: crate::unit::Density) -> crate::nested_scroll::ScrollDelta {
    apply_scroll_delta_inner(nodes, idx, dx, dy, density, true)
}

/// The implementation of `apply_scroll_delta`. With `recursive=true`, a node that consumed nothing
/// itself (already at its boundary) falls back to recursing into its children (the old behaviour, the
/// fallback semantics of wheel/drag that lets an inner scroll consume). With `recursive=false` it
/// scrolls **the target only**: `dispatch_nested_scroll_delta`'s explicit-target semantics mean a
/// target that cannot scroll leaves the delta to the post chain's ancestor connections, and must
/// **not** quietly scroll a child (otherwise "drag down while the outer list is at its top" scrolled
/// the inner list, the user-reported bug).
fn apply_scroll_delta_inner(nodes: &mut [LayoutNode], idx: usize, dx: f32, dy: f32, density: crate::unit::Density, recursive: bool) -> crate::nested_scroll::ScrollDelta {
    let mut consumed = crate::nested_scroll::ScrollDelta::ZERO;
    {
        let node = &nodes[idx];
        #[cfg(debug_assertions)]
        if drag_trace_enabled() {
            eprintln!("[apply-scroll] idx={} dy={} vp_h={} content_h={} has_vscroll={}",
                idx, dy, node.scroll_viewport_height, node.scroll_content_height,
                node.modifier.vertical_scroll_state().is_some());
        }
        if dy != 0.0 {
            if let Some(state) = node.modifier.vertical_scroll_state() {
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
                node.modifier.fixed_size()
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
            let raw = current - dy;
            let new = raw.clamp(0.0, max_offset);
            state.offset.set(new);
            consumed.y = current - new;
            // Overscroll: the delta past the edge is absorbed by the effect instead of falling
            // through to an ancestor. `reverse` mirrors the offset, so a signed pull would point
            // the wrong way — those containers skip it.
            let leftover = raw - new;
            if leftover != 0.0 && !node.scroll_reverse {
                if let Some(effect) = node.modifier.overscroll_effect() {
                    // `pull` returns the absorbed amount in OFFSET space; `consumed` is in DELTA
                    // space, and the two point opposite ways (`consumed = current - new`,
                    // `leftover = raw - new`). Adding it would tell the nested-scroll ancestors the
                    // gesture went the other way, twice as far.
                    consumed.y -= effect.pull(leftover);
                }
            }
            }
        }
        // Horizontal scrolling (LazyRow / horizontal scroll container), symmetric to vertical:
        // a positive dx moves the content left.
        if dx != 0.0 {
        if let Some(state) = node.modifier.horizontal_scroll_state() {
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
                node.modifier.fixed_size()
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
            if leftover != 0.0 && !node.scroll_reverse {
                if let Some(effect) = node.modifier.overscroll_effect() {
                    consumed.x -= effect.pull(leftover);
                }
            }
        }
        }
    }
    if consumed.x != 0.0 || consumed.y != 0.0 { return consumed; }
    // Nothing consumed here: in recursive mode fall back to the children (the old fallback
    // semantics); the non-recursive dispatch path does not recurse, and leaves a stuck target to the
    // post chain.
    if !recursive { return consumed; }
    // Children (clone the indices first, so the recursion does not fight nodes' mutable borrow).
    let children: Vec<usize> = nodes[idx].children.clone();
    for c in children {
        let child = apply_scroll_delta_inner(nodes, c, dx, dy, density, true);
        if child.x != 0.0 || child.y != 0.0 { return child; }
    }
    consumed
}

#[cfg(test)]
mod nested_scroll_chain_tests {
    use super::{dispatch_nested_scroll_delta};
    use crate::layout::node::LayoutNode;
    use crate::unit::{Offset, Size};
    use crate::modifier::{Modifier, ScrollState};
    use crate::nested_scroll::{NestedScrollConnection, NestedScrollSource, ScrollDelta, ScrollVelocity};

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
        fn with_global(name: &str, log: std::sync::Arc<std::sync::Mutex<Vec<String>>>, global: std::sync::Arc<std::sync::Mutex<Vec<String>>>) -> Self {
            Self { name: name.to_string(), log, global_log: Some(global), order: std::sync::atomic::AtomicUsize::new(0) }
        }
    }
    impl NestedScrollConnection for Recorder {
        fn on_pre_scroll(&self, available: ScrollDelta, _: NestedScrollSource) -> ScrollDelta {
            let n = self.order.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.log.lock().unwrap().push(format!("pre-{}-{}", self.name, n));
            if let Some(g) = &self.global_log {
                g.lock().unwrap().push(format!("pre-{}", self.name));
            }
            ScrollDelta::new(available.x / 2.0, available.y / 2.0)
        }
        fn on_post_scroll(&self, _: ScrollDelta, available: ScrollDelta, _: NestedScrollSource) -> ScrollDelta {
            let n = self.order.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.log.lock().unwrap().push(format!("post-{}-{}", self.name, n));
            if let Some(g) = &self.global_log {
                g.lock().unwrap().push(format!("post-{}", self.name));
            }
            ScrollDelta::new(available.x, available.y)
        }
        fn on_pre_fling(&self, _: ScrollVelocity) -> ScrollVelocity { ScrollVelocity::default() }
        fn on_post_fling(&self, _: ScrollVelocity, _: ScrollVelocity) -> ScrollVelocity { ScrollVelocity::default() }
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
        assert_eq!(effect.stretch(), 10.0, "and it became the rubber-band pull");
        assert_eq!(
            mailbox.lock().unwrap().y, 0.0,
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
            LayoutNode::leaf(Modifier::new().vertical_scroll(scroll.clone()).nested_scroll(conn_t).size(100.0, 100.0)),
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
            &mut nodes, 0, 2,
            ScrollDelta::new(0.0, -20.0),
            NestedScrollSource::Wheel,
            density,
        );

        // R: one pre (eats 10) plus one post (after the child consumes, eats the rest); R is the last post.
        assert_eq!(*r_log.lock().unwrap(), vec!["pre-R-0", "post-R-1"], "R runs pre then post");
        // M: one pre plus one post, before T and R (reverse order M -> T -> R).
        assert_eq!(*m_log.lock().unwrap(), vec!["pre-M-0", "post-M-1"], "M runs pre then post");
        // T: pre plus post (a TopAppBar-style connection hangs off the target, so post must be
        // called; this was the key regression before the fix, where excluding the target broke the
        // content_offset recolour and the spring-back).
        assert_eq!(*t_log.lock().unwrap(), vec!["pre-T-0", "post-T-1"], "T takes part in post too (a TopAppBar relies on it)");

        // consumed should be negative (consumed in the negative delta direction), and the child has
        // actually scrolled, so offset > 0.
        assert!(consumed.y < 0.0, "consumption should follow the delta direction, got {:?}", consumed);
        assert!(scroll.offset.get() > 0.0, "the child should actually scroll (pre only eats half), got {}", scroll.offset.get());

        // Cross-node global order: pre forward R -> M -> T, post reverse T -> M -> R (including target T).
        let g = global.lock().unwrap().clone();
        assert_eq!(g, vec!["pre-R", "pre-M", "pre-T", "post-T", "post-M", "post-R"],
            "the global order should be pre-R -> M -> T -> post-T -> M -> R (target T takes part in post), got {g:?}");
    }
}

#[cfg(test)]
mod release_velocity_floor_tests {
    use super::{dispatch_nested_scroll_fling};
    use crate::layout::node::LayoutNode;
    use crate::unit::{Offset, Size};
    use crate::fling::SnapInfo;
    use crate::modifier::{Modifier, ScrollState};
    use crate::nested_scroll::ScrollVelocity;

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
        SnapInfo { step: STEP, min_fling_velocity: 400.0, ..Default::default() }
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
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
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
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
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
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();

        let (mut nodes, scroll) = tree(None, STEP * 0.6);
        dispatch_nested_scroll_fling(&mut nodes, 0, 1, ScrollVelocity { x: 0.0, y: GENTLE });

        assert!(
            !crate::animation::has_animation_for_state(scroll.offset.state_id()),
            "without a snap spec a {GENTLE} px/s release must start nothing — the 50 px/s floor still \
             guards the decay, which is what it was written for"
        );
        assert_eq!(scroll.offset.get(), STEP * 0.6, "and the offset is untouched");

        // The floor is a floor, not a wall: the same container released fast does fling.
        dispatch_nested_scroll_fling(&mut nodes, 0, 1, ScrollVelocity { x: 0.0, y: 900.0 });
        assert!(
            crate::animation::has_animation_for_state(scroll.offset.state_id()),
            "a fast release on an ordinary container still flings"
        );
    }
}
