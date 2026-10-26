use super::*;

// ── Wheel paging ──

/// Where one wheel delta came from, which decides how much travel a page step costs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WheelKind {
    /// A discrete notch (`LineDelta`) — a mouse wheel. One notch is one deliberate page step.
    Notch,
    /// A continuous high-resolution delta (`PixelDelta`, and the fractional lines some drivers send):
    /// a precision touchpad delivers many of these per gesture, so they have to accumulate first.
    Pixel,
}

/// One wheel event, normalized to logical px per axis (the `LineDelta`/`PixelDelta` conversion lives in
/// `window_event`), plus where it came from.
#[derive(Clone, Copy, Debug)]
pub(crate) struct WheelDelta {
    pub(crate) dx: f32,
    pub(crate) dy: f32,
    pub(crate) kind: WheelKind,
}

/// The kind a `LineDelta` reports.
///
/// A whole number of lines is a discrete notch. A FRACTIONAL one (some drivers and high-resolution
/// wheels report half-line steps) is a continuous stream and has to bank travel like a touchpad -
/// paging on every such event is the touchpad bug in another costume.
pub(crate) fn wheel_kind_for_lines(x: f32, y: f32) -> WheelKind {
    if x.fract() == 0.0 && y.fract() == 0.0 {
        WheelKind::Notch
    } else {
        WheelKind::Pixel
    }
}

/// What a PAGED container did with one wheel event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WheelPage {
    /// Not a paging container on this wheel's axis: the caller pixel-scrolls it.
    NotPaged,
    /// A paging container whose travel gate is still banking the gesture: the event is swallowed, not
    /// pixel-scrolled, because a page either turns or nothing moves.
    Held,
    /// It started a page step.
    Stepped,
    /// A paging container with no page left in that direction.
    ///
    /// Deliberately NOT a pixel scroll. The pixel path cancels the animation in flight and moves the
    /// offset by the delta, which leaves the container BETWEEN two pages with nothing left to snap it
    /// back — it sits there until the next notch happens to re-derive a page target. That is the
    /// "stuck half-way between two pages during a fast scroll" report, and the page either turns or
    /// nothing moves is the whole contract of a paged container.
    AtEnd,
}

/// What one wheel event did to one scroll target.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct WheelOutcome {
    /// A paging container claimed the event — it stepped, or its gate is holding the step. The caller
    /// reports the wheel as handled even though nothing pixel-scrolled.
    pub(crate) paged: bool,
    /// The delta pixel scrolling actually consumed (zero whenever `paged`).
    pub(crate) consumed: crate::nested_scroll::ScrollDelta,
}

impl WheelOutcome {
    /// Whether the container did anything at all with the event.
    pub(crate) fn handled(&self) -> bool {
        self.paged || self.consumed.x != 0.0 || self.consumed.y != 0.0
    }
}

/// Wheel-session gate for PAGED containers, one per window (the field lives on `PerWindow`).
///
/// A precision touchpad reports `PixelDelta` as many 1-3 px events at up to ~120 Hz. Paging once per
/// event — which the ungated path did — ran a Pager forward a dozen pages for a single flick, and no
/// delta cut-off can fix that: the problem is per-event paging, not the magnitude. So the events of one
/// gesture bank travel, and a page is armed only when a real notch's worth of it has accumulated (see
/// `PIXEL_PAGE_TRAVEL`). A gap between events, a reversal, or a different container starts over. Plain,
/// non-paging containers never reach this type: they keep scrolling by pixels, one event at a time.
#[derive(Debug, Default)]
pub(crate) struct WheelPager {
    /// Which container and axis the banked travel belongs to: (tree space, node id, vertical axis).
    pub(crate) key: Option<(u64, u64, bool)>,
    /// Signed travel banked toward the next page step, in logical px.
    pub(crate) travel: f32,
    /// When the previous wheel event arrived (`None` before the first one).
    pub(crate) last: Option<std::time::Instant>,
}

impl WheelPager {
    /// A gap at least this long ends the gesture; banked travel does not survive it.
    pub(crate) const SESSION_GAP: std::time::Duration = std::time::Duration::from_millis(400);
    /// Logical px of continuous travel one page step costs: three 20 px lines, i.e. one conventional
    /// three-line mouse notch (the `LineDelta` conversion in `window_event` is 20 px per line). A 3 px
    /// touchpad event therefore needs ~20 siblings before it may turn a page, while the same notch
    /// delivered as pixels still pages on its own.
    pub(crate) const PIXEL_PAGE_TRAVEL: f32 = 60.0;

    /// Banks one wheel event aimed at `key` and reports whether that event earns a page step, and which
    /// way (`Some(true)` = forward, the sense `ScrollState::snap_page` takes).
    ///
    /// `amount` is the signed delta along the axis the container pages on. A notch always earns its
    /// step; a pixel delta earns one only once the banked travel reaches `PIXEL_PAGE_TRAVEL`. At most one
    /// step per event and nothing carries over, so an oversized delta (an accelerated flick, a scale
    /// change between two events) cannot cascade several pages out of a single event.
    pub(crate) fn page_step(
        &mut self,
        key: (u64, u64, bool),
        amount: f32,
        kind: WheelKind,
        now: std::time::Instant,
    ) -> Option<bool> {
        let stale = self
            .last
            .map_or(true, |t| now.saturating_duration_since(t) >= Self::SESSION_GAP);
        // A new container, the other axis, a fresh gesture or a reversal all mean the banked travel
        // belongs to a motion the user has abandoned: half a notch one way and half a notch back is a
        // scrub, not a page step.
        if stale || self.key != Some(key) || self.travel * amount < 0.0 {
            self.travel = 0.0;
            self.key = Some(key);
        }
        self.last = Some(now);
        if kind == WheelKind::Notch {
            // Discrete intent: page at once, and drop any pixel travel banked before the notch.
            self.travel = 0.0;
            return Some(amount < 0.0);
        }
        self.travel += amount;
        if self.travel.abs() < Self::PIXEL_PAGE_TRAVEL {
            return None;
        }
        self.travel = 0.0;
        Some(amount < 0.0)
    }
}

/// Wheel-session namespaces: the main tree and every overlay layer bank their travel separately, so a
/// half-notch over a menu cannot carry into the list behind it.
pub(crate) const MAIN_TREE_WHEEL_SPACE: u64 = 0;
pub(crate) const OVERLAY_WHEEL_SPACE_BASE: u64 = 1;

/// The scroll node one wheel event should act on, from a hit path (root to leaf, innermost last).
///
/// The rule is the main tree's, and it is shared so the overlay layers and the injected debug wheel
/// cannot drift from it: a node takes the wheel when it has the axis the delta moves, or — for a purely
/// vertical wheel — when it has a HORIZONTAL state that PAGES.
pub(crate) fn wheel_scroll_target(nodes: &[LayoutNode], path: &[usize], dx: f32, dy: f32) -> Option<usize> {
    // A HORIZONTAL PAGER still takes a vertical wheel. Compose does not do this: its wheel logic takes
    // the DOMINANT axis and returns 0 when that is not the container's axis
    // (NonTouchScrollingLogic.kt:255-264), so a plain wheel over a HorizontalPager does nothing there.
    // winia deviates for containers that PAGE - and only those, so a vertical ancestor keeps the wheel
    // a plain horizontal row would have stolen.
    path.iter()
        .rev()
        .find(|&&idx| {
            if nodes[idx].modifier.is_scroll_input_disabled() {
                return false;
            }
            (dy != 0.0 && nodes[idx].modifier.vertical_scroll_state().is_some())
                || (dx != 0.0 && nodes[idx].modifier.horizontal_scroll_state().is_some())
                || (dy != 0.0
                    && dx == 0.0
                    && nodes[idx].modifier.vertical_scroll_state().is_none()
                    && nodes[idx].modifier.horizontal_scroll_state().is_some_and(|s| s.snaps()))
        })
        .copied()
}

/// One wheel event on a PAGED container moves one page (`ScrollState::snap_page`).
///
/// The axis is chosen by which scroll state the NODE has, not by which delta arrived: a precision
/// trackpad can deliver a diagonal pixel delta to a vertical pager, and picking by delta then finds no
/// horizontal state and silently pixel-scrolls it. A horizontal-only container still takes a vertical
/// wheel (winia's deliberate deviation - Compose uses the dominant axis and ignores the rest, which
/// leaves a HorizontalPager unmovable by a plain wheel).
///
/// `space` namespaces `pager` per tree, so two arenas cannot share one gesture's banked travel.
pub(crate) fn wheel_page_step(
    nodes: &[LayoutNode],
    target: usize,
    space: u64,
    wheel: WheelDelta,
    pager: &mut WheelPager,
    now: std::time::Instant,
) -> WheelPage {
    let node = &nodes[target];
    let horizontal = node.modifier.horizontal_scroll_state();
    let vertical = node.modifier.vertical_scroll_state();
    let (state, vertical_axis, amount) = match (horizontal, vertical) {
        (Some(h), Some(v)) => {
            if wheel.dx.abs() > wheel.dy.abs() {
                (h, false, wheel.dx)
            } else {
                (v, true, wheel.dy)
            }
        }
        (Some(h), None) => (h, false, if wheel.dx != 0.0 { wheel.dx } else { wheel.dy }),
        (None, Some(v)) => (v, true, if wheel.dy != 0.0 { wheel.dy } else { wheel.dx }),
        (None, None) => return WheelPage::NotPaged,
    };
    if !state.snaps() {
        return WheelPage::NotPaged;
    }
    // `reverseLayout` mirrors the direction (the pixel and fling paths do the same).
    let reverse = node.modifier.is_lazy_scroll_reverse()
        || node.modifier.is_horizontal_scroll_reverse();
    match pager.page_step((space, node.id, vertical_axis), amount, wheel.kind, now) {
        Some(forward) if state.snap_page(forward, reverse) => WheelPage::Stepped,
        // No page left in that direction: swallowed (and the bar pulses), never nudged.
        Some(_) => WheelPage::AtEnd,
        None => WheelPage::Held,
    }
}

/// Applies one wheel event to `target` in the arena rooted at `root_idx`.
///
/// A paging container takes one page step; anything else pixel-scrolls through the nested-scroll chain,
/// with a horizontal-only target reading the wheel's dy as dx. Shared by the main tree, the overlay
/// layers and the injected debug wheel, so the three paths cannot drift apart again.
///
/// Nothing here settles the container between events: a snapping container settles once the wheel (or
/// the injected gesture) stops, not after every event, which is both what a real mouse does and what a
/// UI test has to see.
pub(crate) fn apply_wheel_to_target(
    nodes: &mut [LayoutNode],
    root_idx: usize,
    target: usize,
    space: u64,
    wheel: WheelDelta,
    pager: &mut WheelPager,
    density: crate::unit::Density,
    now: std::time::Instant,
) -> WheelOutcome {
    match wheel_page_step(nodes, target, space, wheel, pager, now) {
        WheelPage::Stepped | WheelPage::Held => {
            return WheelOutcome { paged: true, consumed: Default::default() }
        }
        WheelPage::AtEnd => {
            // The notch hit the end of the pages. The paged container must not be nudged - a pixel
            // scroll cancels the settle in flight and strands it between two pages - but the event is
            // not swallowed either: an ancestor still gets its pre-scroll chance, so a carousel inside
            // a list does not stop scrolling that list at its last page.
            let consumed = crate::app::scroll::dispatch_nested_scroll_delta_to_ancestors(
                nodes,
                root_idx,
                target,
                crate::nested_scroll::ScrollDelta::new(wheel.dx, wheel.dy),
                crate::nested_scroll::NestedScrollSource::Wheel,
                density,
            );
            let (pulse_dx, pulse_dy) = wheel_axes_for(&nodes[target], wheel.dx, wheel.dy);
            pulse_refused_wheel(&nodes[target], pulse_dx, pulse_dy, consumed);
            return WheelOutcome { paged: false, consumed };
        }
        WheelPage::NotPaged => {}
    }
    // A horizontal-only target takes the wheel's dy as dx (see `wheel_scroll_target`).
    let (dx, dy) = wheel_axes_for(&nodes[target], wheel.dx, wheel.dy);
    let consumed = dispatch_nested_scroll_delta(
        nodes,
        root_idx,
        target,
        crate::nested_scroll::ScrollDelta::new(dx, dy),
        crate::nested_scroll::NestedScrollSource::Wheel,
        density,
    );
    pulse_refused_wheel(&nodes[target], dx, dy, consumed);
    WheelOutcome { paged: false, consumed }
}

/// The axes a wheel event acts on for `target`: a horizontal-only container reads the wheel's dy as
/// dx (a plain vertical wheel has to move a horizontal list, see `wheel_scroll_target`).
pub(crate) fn wheel_axes_for(node: &LayoutNode, dx: f32, dy: f32) -> (f32, f32) {
    let only_horizontal = node.modifier.vertical_scroll_state().is_none()
        && node.modifier.horizontal_scroll_state().is_some();
    if only_horizontal && dy != 0.0 {
        (dy, 0.0)
    } else {
        (dx, dy)
    }
}

/// Pulse the scrollbar of an axis whose wheel event moved nothing: that bump is what tells the user
/// the wheel arrived at all (a fade driven by the offset cannot see it). Decided per axis — a diagonal
/// scroll at the top edge consumes x while y is stuck, and a joint condition would lose the vertical
/// pulse.
pub(crate) fn pulse_refused_wheel(
    node: &LayoutNode,
    dx: f32,
    dy: f32,
    consumed: crate::nested_scroll::ScrollDelta,
) {
    if dy != 0.0 && consumed.y == 0.0 {
        if let Some(ss) = node.modifier.vertical_scroll_state() {
            ss.scroll_pulse.update(|v| *v = v.wrapping_add(1));
        }
    }
    if dx != 0.0 && consumed.x == 0.0 {
        if let Some(ss) = node.modifier.horizontal_scroll_state() {
            ss.scroll_pulse.update(|v| *v = v.wrapping_add(1));
        }
    }
}

