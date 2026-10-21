//! Compose foundation's `BringIntoViewRequester` — ask a node's scrolling ancestors to reveal it.
//!
//! The requester is a handle: attach it with `Modifier::bring_into_view_requester`, then call
//! [`BringIntoViewRequester::bring_into_view`] from an event handler. Requests are queued and served
//! by the post-layout pass (`Composer::layout`), where every ancestor's viewport and scroll offset are
//! known.
//!
//! Deliberate scope for this first version:
//! - served by plain `Modifier::vertical_scroll` / `horizontal_scroll` ancestors (the offset is
//!   animated with the default spring);
//! - `reverseLayout` / RTL-mirrored horizontal scrolling is NOT compensated — the request is skipped
//!   there rather than scrolled the wrong way;
//! - a lazy list can only reveal nodes that are composed (an off-screen item has no requester).

use std::sync::{Arc, Mutex};

use crate::layout::node::{scroll_offset_for_node, LayoutNode};
use crate::modifier::ModifierElement;

/// A node-local rectangle: `(x, y, width, height)`.
type LocalRect = (f32, f32, f32, f32);

/// Compose's `BringIntoViewRequester`.
#[derive(Clone)]
pub struct BringIntoViewRequester(Arc<RequesterInner>);

struct RequesterInner {
    /// Requests not served yet; `None` means "the whole node".
    requests: Mutex<Vec<Option<LocalRect>>>,
    /// Bumped on every request so the composing group re-enters and a frame is scheduled.
    tick: crate::runtime::state::State<u64>,
}

impl BringIntoViewRequester {
    pub fn new() -> Self {
        Self(Arc::new(RequesterInner {
            requests: Mutex::new(Vec::new()),
            tick: crate::runtime::state::State::new(0),
        }))
    }

    /// Ask every scrolling ancestor to reveal this node.
    pub fn bring_into_view(&self) {
        self.push(None);
    }

    /// Ask every scrolling ancestor to reveal a rectangle INSIDE this node (node-local coordinates).
    pub fn bring_into_view_rect(&self, x: f32, y: f32, width: f32, height: f32) {
        self.push(Some((x, y, width, height)));
    }

    fn push(&self, rect: Option<LocalRect>) {
        self.0.requests.lock().unwrap().push(rect);
        // Wake the composer: a request has to produce a layout pass even when nothing else changed.
        // The modifier subscribed this state during composition, so this re-enters that group.
        self.0.tick.update(|v| *v += 1);
    }

    /// Composition-time subscription: read the request counter so a later request re-enters.
    pub(crate) fn subscribe(&self) {
        let _ = self.0.tick.get();
    }

    /// Identity, for `element_param_eq` (two different requesters are different parameters).
    pub(crate) fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    fn take_requests(&self) -> Vec<Option<LocalRect>> {
        std::mem::take(&mut *self.0.requests.lock().unwrap())
    }
}

impl Default for BringIntoViewRequester {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for BringIntoViewRequester {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BringIntoViewRequester").finish_non_exhaustive()
    }
}

/// Serve every queued request in the tree, after layout produced final positions.
pub(crate) fn process_bring_into_view(nodes: &mut [LayoutNode], root: usize) {
    let mut ancestors: Vec<(usize, f32, f32)> = Vec::new();
    walk(nodes, root, 0.0, 0.0, &mut ancestors);
}

fn walk(
    nodes: &mut [LayoutNode],
    idx: usize,
    x: f32,
    y: f32,
    ancestors: &mut Vec<(usize, f32, f32)>,
) {
    // Drain this node's requests, then displace them through the ancestors, innermost first.
    let requests: Vec<Option<LocalRect>> = {
        let mut out = Vec::new();
        for el in nodes[idx].modifier.elements() {
            if let ModifierElement::BringIntoViewRequester { requester } = el {
                out.extend(requester.take_requests());
            }
        }
        out
    };
    if !requests.is_empty() {
        let size = nodes[idx].measured_size;
        for request in requests {
            let (rx, ry, rw, rh) = request.unwrap_or((0.0, 0.0, size.width, size.height));
            let mut rect = (x + rx, y + ry, rw, rh);
            for &(anc, ax, ay) in ancestors.iter().rev() {
                rect = reveal_in(nodes, anc, ax, ay, rect);
            }
        }
    }

    let (sx, sy) = scroll_offset_for_node(&nodes[idx]);
    ancestors.push((idx, x, y));
    let children = nodes[idx].children.clone();
    for child in children {
        let (px, py) = (nodes[child].position.x, nodes[child].position.y);
        walk(nodes, child, x + px - sx, y + py - sy, ancestors);
    }
    ancestors.pop();
}

/// Scroll `ancestor` so `rect` (in window coordinates) is inside its viewport. Returns the rect
/// adjusted for the scroll it asked for, so an outer ancestor sees the post-scroll position.
fn reveal_in(
    nodes: &mut [LayoutNode],
    ancestor: usize,
    ax: f32,
    ay: f32,
    rect: LocalRect,
) -> LocalRect {
    let (w, h) = (nodes[ancestor].measured_size.width, nodes[ancestor].measured_size.height);
    let (pad_s, pad_t, pad_e, pad_b) = nodes[ancestor].modifier.get_padding_sides();
    let rtl = nodes[ancestor].layout_direction == crate::layout::LayoutDirection::Rtl;
    let (vx, vy) = (ax + if rtl { pad_e } else { pad_s }, ay + pad_t);
    let (vw, vh) = ((w - pad_s - pad_e).max(0.0), (h - pad_t - pad_b).max(0.0));

    let mut rect = rect;
    if let Some(state) = nodes[ancestor].modifier.vertical_scroll_state() {
        if !nodes[ancestor].scroll_reverse {
            let offset = state.offset.peek();
            let content_top = rect.1 - vy + offset;
            let content_bottom = content_top + rect.3;
            let target = if content_top < offset {
                content_top
            } else if content_bottom > offset + vh {
                content_bottom - vh
            } else {
                offset
            };
            let target = target.clamp(0.0, scroll_limit(nodes, ancestor, vh, false));
            if (target - offset).abs() > 0.5 {
                state.animate_scroll_to(
                    target,
                    scroll_limit(nodes, ancestor, vh, false),
                    crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec::default()),
                );
                rect.1 += offset - target;
            }
        }
    }
    if let Some(state) = nodes[ancestor].modifier.horizontal_scroll_state() {
        if !nodes[ancestor].scroll_reverse {
            let offset = state.offset.peek();
            let content_left = rect.0 - vx + offset;
            let content_right = content_left + rect.2;
            let target = if content_left < offset {
                content_left
            } else if content_right > offset + vw {
                content_right - vw
            } else {
                offset
            };
            let target = target.clamp(0.0, scroll_limit(nodes, ancestor, vw, true));
            if (target - offset).abs() > 0.5 {
                state.animate_scroll_to(
                    target,
                    scroll_limit(nodes, ancestor, vw, true),
                    crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec::default()),
                );
                rect.0 += offset - target;
            }
        }
    }
    rect
}

/// The farthest offset this ancestor can scroll: the measured fling limit, or (before the first
/// measure wrote it) the content size minus the viewport.
fn scroll_limit(nodes: &[LayoutNode], ancestor: usize, viewport: f32, horizontal: bool) -> f32 {
    let limit = if horizontal {
        nodes[ancestor]
            .modifier
            .horizontal_scroll_state()
            .map(|state| state.fling_limit.peek())
    } else {
        nodes[ancestor]
            .modifier
            .vertical_scroll_state()
            .map(|state| state.fling_limit.peek())
    }
    .unwrap_or(f32::MAX);
    if limit.is_finite() {
        limit.max(0.0)
    } else {
        let content = nodes[ancestor]
            .scroll_content_height
            .max(nodes[ancestor].scroll_content_width);
        (content - viewport).max(0.0)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::node::LayoutNode;
    use crate::modifier::{Modifier, ScrollState};
    use crate::unit::{Offset, Size};

    fn fixture() -> (Vec<LayoutNode>, ScrollState, BringIntoViewRequester) {
        let state = ScrollState::new();
        state.fling_limit.set(300.0);
        let requester = BringIntoViewRequester::new();
        let mut container = LayoutNode::new(Modifier::new().vertical_scroll(state.clone()), Some(0));
        container.children = vec![1];
        container.measured_size = Size::new(100.0, 100.0);
        container.scroll_viewport_height = 100.0;
        container.scroll_content_height = 400.0;
        let mut child = LayoutNode::leaf(
            Modifier::new()
                .size(100.0, 100.0)
                .bring_into_view_requester(&requester),
        );
        child.position = Offset::new(0.0, 300.0);
        child.measured_size = Size::new(100.0, 100.0);
        (vec![container, child], state, requester)
    }

    #[test]
    fn a_request_scrolls_the_nearest_vertical_container() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        crate::animation::clear_all_animations();
        let (mut nodes, state, requester) = fixture();

        requester.bring_into_view();
        process_bring_into_view(&mut nodes, 0);
        let target: Option<f32> = crate::animation::animation_target(state.offset.state_id());
        assert_eq!(target, Some(300.0), "the bottom item is revealed at the container's end");
        crate::animation::clear_all_animations();
    }

    #[test]
    fn a_request_is_served_once() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        crate::animation::clear_all_animations();
        let (mut nodes, state, requester) = fixture();

        requester.bring_into_view();
        process_bring_into_view(&mut nodes, 0);
        crate::animation::clear_all_animations();
        // The queue was drained: a second pass starts nothing.
        process_bring_into_view(&mut nodes, 0);
        let target: Option<f32> = crate::animation::animation_target(state.offset.state_id());
        assert_eq!(target, None, "no second request is invented");
    }

    #[test]
    fn a_part_of_the_node_can_be_targeted() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        crate::animation::clear_all_animations();
        let (mut nodes, state, requester) = fixture();
        // Only the top half of the child: its bottom is at content 350, so the viewport shows
        // 250..350 and the node's top half (300..350) is already in view once scrolled by 250.
        requester.bring_into_view_rect(0.0, 0.0, 100.0, 50.0);
        process_bring_into_view(&mut nodes, 0);
        let target: Option<f32> = crate::animation::animation_target(state.offset.state_id());
        assert_eq!(target, Some(250.0));
        crate::animation::clear_all_animations();
    }

    #[test]
    fn the_tick_wakes_the_composer() {
        let requester = BringIntoViewRequester::new();
        let before = requester.0.tick.peek();
        requester.bring_into_view();
        assert_eq!(requester.0.tick.peek(), before + 1);
    }
}
