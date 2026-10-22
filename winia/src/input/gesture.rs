//! Gesture state machine — mirrors Compose `detectTapGestures` / `detectDragGestures`.
//!
//! Pure logic (no winit/skia dependency): app.rs feeds pointer down/move/up events in, the state
//! machine outputs a `GestureAction` (tap / double-tap / long-press / drag family), and app.rs runs
//! the matching Modifier callback.
//!
//! Semantics (aligned with Compose):
//! - **touch slop**: after the press, only movement past the threshold counts as a drag (and the
//!   tap is cancelled)
//! - **double tap**: two taps less than 300ms apart and less than 50px apart → double-tap
//! - **deferred tap** (Compose semantics): when a node registers onDoubleTap, onTap is delayed
//!   until the double-tap window (300ms) closes; a second press on the same node inside the window
//!   cancels the first tap (only DoubleTap is sent); on timeout, or on a press of another node,
//!   the Tap is sent late. `PendingTap` carries that state.
//! - **long press**: a down held for 500ms without passing the slop fires **at its deadline**
//!   (matching Compose `detectTapGestures`: `onLongPress` arrives during the press, not on
//!   release). The event loop drives it by polling `poll_long_press`, and `WaitUntil` makes it wake
//!   at that moment even when idle; releasing only ends the gesture — it sends neither a second
//!   long press nor a tap.
//! - **drag capture**: once a drag starts, events follow the gesture's node (they keep arriving
//!   even when the pointer leaves the component)
//! - drag delta = current position - previous position

use std::time::Instant;

/// Touch slop threshold (logical pixels) — past it the gesture counts as a drag and the tap is cancelled.
pub(crate) const TOUCH_SLOP: f32 = 8.0;
/// Double-tap time window (milliseconds).
pub(crate) const DOUBLE_TAP_TIMEOUT_MS: u128 = 300;
/// Double-tap position tolerance (logical pixels).
pub(crate) const DOUBLE_TAP_SLOP: f32 = 50.0;
/// Long-press duration (milliseconds).
pub(crate) const LONG_PRESS_TIMEOUT_MS: u128 = 500;
/// The same timeout as a `Duration`, which is what the deadline arithmetic wants.
pub(crate) const LONG_PRESS_TIMEOUT: std::time::Duration =
    std::time::Duration::from_millis(LONG_PRESS_TIMEOUT_MS as u64);

/// Which way a gesture (or the scroll container it might belong to) moves.
///
/// A press can be claimed by two owners at once — an inner drag component (a swipeable row, a
/// slider) and the scroll container it sits in — and the finger's dominant axis decides which
/// one keeps the gesture. Compose arbitrates in the same place: its drag detectors each wait for
/// the touch slop along their own orientation, so the direction the finger moves first decides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScrollAxis {
    Horizontal,
    Vertical,
}

impl ScrollAxis {
    /// Dominant axis of a displacement from the press point, or `None` while it is still
    /// undecided: below the touch slop (nothing has started yet) or on an exact diagonal (either
    /// could win — wait for another move instead of guessing).
    pub(crate) fn classify(dx: f32, dy: f32) -> Option<Self> {
        let (ax, ay) = (dx.abs(), dy.abs());
        if ax.max(ay) < TOUCH_SLOP {
            None
        } else if ax > ay {
            Some(Self::Horizontal)
        } else if ay > ax {
            Some(Self::Vertical)
        } else {
            None
        }
    }
}

/// The action a gesture state machine outputs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum GestureAction {
    /// Press (fires immediately on down).
    Press((f32, f32)),
    /// Tap (up, without moving past the slop and before the long-press threshold).
    Tap((f32, f32)),
    /// Double tap (the second tap landed inside the time/position window).
    DoubleTap((f32, f32)),
    /// Long press (down held past the threshold without moving).
    LongPress((f32, f32)),
    /// Drag start (the first move past the slop).
    DragStart((f32, f32)),
    /// Drag move (current position, delta).
    DragMove((f32, f32), (f32, f32)),
    /// Drag end (up).
    DragEnd,
    /// Drag cancelled (cancel — the system interrupted before up).
    #[allow(dead_code)] // reserved: the winit side does not deliver the cancel event yet
    DragCancel,
    /// No action (the event was consumed but produces no callback).
    None,
}

/// Gesture tracking state — created on every down, destroyed on up/cancel.
pub(crate) struct GestureTracker {
    /// The gesture node's id (the routing target for callbacks). The tracked node supplies it at
    /// construction, so this is who the tracker represents; nothing reads it yet, but keeping it
    /// avoids discarding information the caller already handed over.
    #[allow(dead_code)]
    pub(crate) node_id: u64,
    /// Press position.
    down_pos: (f32, f32),
    /// Previous move position.
    last_pos: (f32, f32),
    /// Press time.
    down_time: Instant,
    /// Whether the touch slop has been passed (the tap is cancelled).
    slop_passed: bool,
    /// Whether the drag has started (on_drag_start fires only once).
    dragging: bool,
    /// Whether this node declared a drag callback (decides whether crossing the slop starts a drag or silently cancels the tap).
    has_drag: bool,
    /// Double-tap state: time/position of the previous tap.
    last_tap_time: Option<Instant>,
    last_tap_pos: Option<(f32, f32)>,
    /// Has this gesture's long press already fired? It fires AT the deadline now, so the release must
    /// neither fire it again nor fall through to a tap — the press was consumed.
    long_press_fired: bool,
}

/// A deferred tap (when the node registered `on_double_tap` — Compose `detectTapGestures` semantics).
/// Once on_up decides Tap it is not fired immediately but parked on the PerWindow; a second down on
/// the same node inside the double-tap window cancels it (nothing is sent late); a timeout sends it
/// late; a press on another node does not affect it (it stays until the deadline and the event loop
/// sends it — Compose's deferred onTaps for separate pointerInputs are independent of each other).
#[derive(Clone)]
pub(crate) struct PendingTap {
    /// Gesture node slot_key (stable across recomposition — the node is resolved from the current layout tree when the tap is sent late).
    pub(crate) slot_key: u64,
    /// Gesture node id (same-node check — whether a second down inside the window cancels the first tap).
    pub(crate) node_id: u64,
    /// Scene coordinates of the first tap (converted to component-local coordinates when sent late).
    pub(crate) pos: (f32, f32),
    /// Arena the tap was recorded in: `None` = the main tree, `Some(overlay id)` = that popup's
    /// layer. The tap has to be re-fired there, and `pos` is layer-local for a popup.
    pub(crate) overlay_id: Option<u64>,
    /// The deadline to send it by (now + DOUBLE_TAP_TIMEOUT_MS).
    pub(crate) deadline: std::time::Instant,
}

impl PendingTap {
    pub(crate) fn new(slot_key: u64, node_id: u64, pos: (f32, f32), overlay_id: Option<u64>) -> Self {
        Self {
            slot_key,
            node_id,
            pos,
            overlay_id,
            deadline: std::time::Instant::now()
                + std::time::Duration::from_millis(DOUBLE_TAP_TIMEOUT_MS as u64),
        }
    }
}

/// How a new down disposes of a single pending tap (pure logic — consumed by app.rs, easy to unit-test).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PendingTapAction {
    /// Already timed out: send the first tap late.
    Fire,
    /// A second press on the same node inside the double-tap window: cancel (wait for up to decide the double tap, sending only DoubleTap).
    Cancel,
    /// A press on another node: keep (wait for the deadline to send it late — neither early nor cancelled).
    Keep,
}

/// How a single pending tap is disposed of when a "new down arrives".
pub(crate) fn pending_tap_on_down(
    t: &PendingTap,
    now: std::time::Instant,
    down_node: u64,
) -> PendingTapAction {
    if t.deadline <= now {
        PendingTapAction::Fire
    } else if t.node_id == down_node {
        PendingTapAction::Cancel
    } else {
        PendingTapAction::Keep
    }
}

impl GestureTracker {
    pub(crate) fn new(node_id: u64, pos: (f32, f32), has_drag: bool, last_tap: Option<(Instant, (f32, f32))>) -> Self {
        Self {
            node_id,
            down_pos: pos,
            last_pos: pos,
            down_time: Instant::now(),
            slop_passed: false,
            dragging: false,
            has_drag,
            last_tap_time: last_tap.map(|(t, _)| t),
            last_tap_pos: last_tap.map(|(_, p)| p),
            long_press_fired: false,
        }
    }

    /// When this gesture's long press is due, or `None` when it can no longer happen: it already
    /// fired, the finger moved past the touch slop, or a drag took the gesture.
    ///
    /// The event loop registers this with `WaitUntil`, so the long press arrives at its deadline even
    /// when the window is idle and no further input is coming.
    pub(crate) fn long_press_deadline(&self) -> Option<Instant> {
        if self.long_press_fired || self.slop_passed || self.dragging {
            return None;
        }
        Some(self.down_time + LONG_PRESS_TIMEOUT)
    }

    /// Fire the long press if its deadline has passed — what makes `on_long_press` reach the callback
    /// while the finger is still down, instead of on release. Returns the action at most once per
    /// gesture; the caller dispatches it through the gesture's arena, like every other gesture action.
    pub(crate) fn poll_long_press(&mut self, now: Instant) -> Option<GestureAction> {
        let deadline = self.long_press_deadline()?;
        if now < deadline {
            return None;
        }
        self.long_press_fired = true;
        Some(GestureAction::LongPress(self.down_pos))
    }

    /// Pointer moved — returns an action (the drag family, or None).
    ///
    /// `allow_drag` gates *starting* the drag only: an inner component whose axis is still being
    /// arbitrated (see `ScrollAxis`) cancels the tap as usual once the touch slop is crossed, but
    /// must not fire `on_drag_start` yet — the gesture may be about to be handed to the enclosing
    /// scroll. A drag that was held back can still start on a later move (the finger may cross the
    /// slop along a diagonal that decides nothing); once it has started, this flag is ignored, and
    /// a drag cancelled by [`Self::cancel_drag_for_arbitration`] never re-arms.
    pub(crate) fn on_move(&mut self, pos: (f32, f32), allow_drag: bool) -> GestureAction {
        let delta = (pos.0 - self.last_pos.0, pos.1 - self.last_pos.1);
        self.last_pos = pos;
        if !self.slop_passed {
            let dx = pos.0 - self.down_pos.0;
            let dy = pos.1 - self.down_pos.1;
            if dx * dx + dy * dy < TOUCH_SLOP * TOUCH_SLOP {
                return GestureAction::None;
            }
            self.slop_passed = true;
        }
        if self.dragging {
            return GestureAction::DragMove(pos, delta);
        }
        if self.has_drag && allow_drag {
            self.dragging = true;
            return GestureAction::DragStart(pos);
        }
        GestureAction::None
    }

    /// Pointer released — returns an action (tap family / drag-end / None).
    pub(crate) fn on_up(&mut self) -> GestureAction {
        if self.dragging {
            return GestureAction::DragEnd;
        }
        if self.long_press_fired {
            // The press was consumed by a long press that already fired at its deadline: no second
            // long press, and no tap either.
            return GestureAction::None;
        }
        if self.slop_passed {
            // Moved, but never dragged (no drag callback) — silently.
            return GestureAction::None;
        }
        // Did not move: long press, tap, or double tap.
        let held = self.down_time.elapsed().as_millis();
        if held >= LONG_PRESS_TIMEOUT_MS {
            // The deadline passed but the sweep has not run yet (the loop was busy, or the release and
            // the deadline arrived in the same batch). Fire it here so the press is not simply lost —
            // `long_press_fired` keeps it to one action per gesture either way.
            self.long_press_fired = true;
            return GestureAction::LongPress(self.down_pos);
        }
        // Double-tap detection: the previous tap was inside the time window and close enough.
        if let (Some(lt), Some(lp)) = (self.last_tap_time, self.last_tap_pos) {
            let dt = Instant::now().duration_since(lt).as_millis();
            let dx = self.down_pos.0 - lp.0;
            let dy = self.down_pos.1 - lp.1;
            if dt < DOUBLE_TAP_TIMEOUT_MS && dx * dx + dy * dy <= DOUBLE_TAP_SLOP * DOUBLE_TAP_SLOP {
                // Double tap hit — clear the record (a third tap starts counting afresh).
                self.last_tap_time = None;
                self.last_tap_pos = None;
                return GestureAction::DoubleTap(self.down_pos);
            }
        }
        // Record this tap (for the next double-tap decision).
        self.last_tap_time = Some(Instant::now());
        self.last_tap_pos = Some(self.down_pos);
        GestureAction::Tap(self.down_pos)
    }

    /// Cancel (the system interrupted) — returns drag cancel (while dragging) or None.
    /// NOTE: winit does not deliver PointerCanceled/TouchCanceled events yet — this branch is a
    /// reserved API (for a future window losing focus or a touch being cancelled); the state machine
    /// logic is already covered by unit tests.
    #[allow(dead_code)] // reserved: see the doc comment above; the state machine is unit-tested
    pub(crate) fn on_cancel(&mut self) -> GestureAction {
        if self.dragging {
            GestureAction::DragCancel
        } else {
            GestureAction::None
        }
    }

    /// Hand back the double-tap context to the next gesture created.
    pub(crate) fn tap_context(&self) -> Option<(Instant, (f32, f32))> {
        self.last_tap_time.zip(self.last_tap_pos)
    }

    pub(crate) fn down_position(&self) -> (f32, f32) {
        self.down_pos
    }

    /// A tracker that believes the press started at `down_time` — the clock the deadline arithmetic
    /// reads. Test-only: production creates trackers at the real press moment.
    #[cfg(test)]
    pub(crate) fn with_down_time(node_id: u64, pos: (f32, f32), has_drag: bool, down_time: Instant) -> Self {
        let mut tracker = Self::new(node_id, pos, has_drag, None);
        tracker.down_time = down_time;
        tracker
    }

    /// Give the drag up after all (the enclosing scroll won the axis arbitration): the gesture keeps
    /// cancelling the tap but stops tracking a drag, so no `on_drag_*` callback — not even a
    /// `DragCancel` — fires for a gesture the component never received. The caller (`app::gesture_move`)
    /// keeps the axis locked for the rest of the gesture, which is what stops a later `on_move` from
    /// starting the drag again; this call itself does not latch anything.
    pub(crate) fn cancel_drag_for_arbitration(&mut self) {
        self.slop_passed = true;
        self.dragging = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tracker(has_drag: bool) -> GestureTracker {
        GestureTracker::new(1, (10.0, 10.0), has_drag, None)
    }

    #[test]
    fn test_tap_basic() {
        let mut t = tracker(false);
        assert_eq!(t.on_up(), GestureAction::Tap((10.0, 10.0)));
    }

    #[test]
    fn test_tap_moved_within_slop() {
        // Moving less than the slop still counts as a tap.
        let mut t = tracker(false);
        assert_eq!(t.on_move((14.0, 12.0), true), GestureAction::None);
        assert_eq!(t.on_up(), GestureAction::Tap((10.0, 10.0)), "moving inside the slop is still a tap");
    }

    #[test]
    fn test_tap_cancelled_by_slop() {
        // Moving past the slop without a drag callback → the tap is cancelled.
        let mut t = tracker(false);
        assert_eq!(t.on_move((30.0, 10.0), true), GestureAction::None);
        assert_eq!(t.on_up(), GestureAction::None, "a tap that moves past the slop is cancelled");
    }

    #[test]
    fn test_drag_sequence() {
        let mut t = tracker(true);
        // Below the slop: no action.
        assert_eq!(t.on_move((15.0, 10.0), true), GestureAction::None);
        // Past the slop: DragStart (first time).
        assert_eq!(t.on_move((30.0, 10.0), true), GestureAction::DragStart((30.0, 10.0)));
        // Afterwards: DragMove(pos, delta).
        assert_eq!(
            t.on_move((50.0, 20.0), true),
            GestureAction::DragMove((50.0, 20.0), (20.0, 10.0))
        );
        assert_eq!(t.on_up(), GestureAction::DragEnd);
    }

    #[test]
    fn test_drag_start_only_once() {
        let mut t = tracker(true);
        t.on_move((30.0, 10.0), true); // slop
        t.on_move((40.0, 10.0), true);
        // The second large move is still a DragMove (start fires only once).
        assert_eq!(
            t.on_move((60.0, 10.0), true),
            GestureAction::DragMove((60.0, 10.0), (20.0, 0.0))
        );
    }

    #[test]
    fn test_double_tap() {
        // First tap.
        let mut t1 = tracker(false);
        assert_eq!(t1.on_up(), GestureAction::Tap((10.0, 10.0)));
        // Second tap (carrying the previous tap's context).
        let mut t2 = GestureTracker::new(1, (12.0, 11.0), false, t1.tap_context());
        assert_eq!(t2.on_up(), GestureAction::DoubleTap((12.0, 11.0)));
    }

    #[test]
    fn test_pending_tap_metadata() {
        let t = PendingTap::new(42, 7, (3.0, 4.0), None);
        assert_eq!(t.slot_key, 42);
        assert_eq!(t.node_id, 7);
        assert_eq!(t.pos, (3.0, 4.0));
        assert_eq!(t.overlay_id, None, "a main-tree tap carries no overlay id");
        assert!(t.deadline > std::time::Instant::now(), "the deadline lies beyond the double-tap window");
    }

    #[test]
    fn test_pending_tap_on_down() {
        use std::time::{Duration, Instant};
        let now = Instant::now();
        // Already timed out → send it late.
        let expired = PendingTap { slot_key: 1, node_id: 7, pos: (0.0, 0.0), overlay_id: None, deadline: now - Duration::from_millis(1) };
        assert_eq!(pending_tap_on_down(&expired, now, 7), PendingTapAction::Fire);
        assert_eq!(pending_tap_on_down(&expired, now, 99), PendingTapAction::Fire, "a timeout does not depend on which node is pressed");
        // A second press on the same node inside the window → cancel.
        let live = PendingTap { slot_key: 1, node_id: 7, pos: (0.0, 0.0), overlay_id: Some(3), deadline: now + Duration::from_millis(100) };
        assert_eq!(pending_tap_on_down(&live, now, 7), PendingTapAction::Cancel);
        // A press on another node inside the window → keep (neither sent early nor cancelled).
        assert_eq!(pending_tap_on_down(&live, now, 99), PendingTapAction::Keep);
        // Boundary: deadline == now → treated as timed out and sent late.
        let boundary = PendingTap { slot_key: 1, node_id: 7, pos: (0.0, 0.0), overlay_id: None, deadline: now };
        assert_eq!(pending_tap_on_down(&boundary, now, 7), PendingTapAction::Fire);
    }

    #[test]
    fn test_double_tap_too_far() {
        let mut t1 = tracker(false);
        t1.on_up();
        // Position difference above 50: not a double tap.
        let mut t2 = GestureTracker::new(1, (80.0, 10.0), false, t1.tap_context());
        assert_eq!(t2.on_up(), GestureAction::Tap((80.0, 10.0)), "too far apart to be a double tap");
    }

    #[test]
    fn test_drag_cancel() {
        let mut t = tracker(true);
        t.on_move((30.0, 10.0), true); // drag start
        assert_eq!(t.on_cancel(), GestureAction::DragCancel);
    }

    #[test]
    fn test_cancel_without_drag_noop() {
        let mut t = tracker(false);
        assert_eq!(t.on_cancel(), GestureAction::None);
    }

    #[test]
    fn a_long_press_fires_at_its_deadline_while_still_held() {
        // The behaviour change this replaced: `on_long_press` used to arrive on RELEASE, so a caller
        // could not react to a hold until the finger came up. Compose fires it at the deadline.
        let start = Instant::now();
        let mut t = GestureTracker::with_down_time(1, (10.0, 10.0), false, start);
        let deadline = start + LONG_PRESS_TIMEOUT;
        assert_eq!(t.long_press_deadline(), Some(deadline), "the deadline is the press + the timeout");

        assert_eq!(t.poll_long_press(start + LONG_PRESS_TIMEOUT / 2), None, "not yet");
        assert_eq!(
            t.poll_long_press(deadline),
            Some(GestureAction::LongPress((10.0, 10.0))),
            "at the deadline, with the finger still down"
        );
        assert_eq!(t.poll_long_press(deadline + LONG_PRESS_TIMEOUT), None, "and only once");
    }

    #[test]
    fn a_fired_long_press_is_not_followed_by_a_tap() {
        let start = Instant::now();
        let mut t = GestureTracker::with_down_time(1, (10.0, 10.0), false, start);
        t.poll_long_press(start + LONG_PRESS_TIMEOUT);
        assert_eq!(
            t.on_up(),
            GestureAction::None,
            "the press was consumed by the long press: no second long press, and no tap"
        );
    }

    #[test]
    fn moving_past_the_slop_cancels_a_pending_long_press() {
        let start = Instant::now();
        let mut t = GestureTracker::with_down_time(1, (10.0, 10.0), false, start);
        t.on_move((40.0, 10.0), true);
        assert_eq!(t.long_press_deadline(), None, "a moving finger is not a hold");
        assert_eq!(t.poll_long_press(start + LONG_PRESS_TIMEOUT * 2), None);
    }

    #[test]
    fn a_dragging_gesture_has_no_long_press() {
        let start = Instant::now();
        let mut t = GestureTracker::with_down_time(1, (10.0, 10.0), true, start);
        t.on_move((40.0, 10.0), true); // drag starts
        assert_eq!(t.long_press_deadline(), None);
        assert_eq!(t.poll_long_press(start + LONG_PRESS_TIMEOUT * 2), None);
    }

    #[test]
    fn a_deadline_that_the_sweep_missed_still_fires_on_release() {
        // The event loop was busy (or the release and the deadline landed in one batch): the release
        // must not silently swallow the press. It fires here, once.
        let start = Instant::now();
        let mut t = GestureTracker::with_down_time(1, (10.0, 10.0), false, start);
        std::thread::sleep(std::time::Duration::from_millis(LONG_PRESS_TIMEOUT_MS as u64 + 20));
        assert_eq!(t.on_up(), GestureAction::LongPress((10.0, 10.0)), "the hold still counts");
        assert!(t.long_press_fired, "and it is recorded, so nothing fires again");
    }

    #[test]
    fn a_quick_release_is_still_a_tap() {
        // The deadline machinery must not turn short presses into holds.
        let start = Instant::now();
        let mut t = GestureTracker::with_down_time(1, (10.0, 10.0), false, start);
        assert_eq!(
            t.on_up(),
            GestureAction::Tap((10.0, 10.0)),
            "a release before the deadline is a tap"
        );
    }

    #[test]
    fn axis_classification_waits_for_a_decisive_move() {
        use ScrollAxis::{Horizontal, Vertical};
        // Below the touch slop nothing has started, so nothing can be decided.
        assert_eq!(ScrollAxis::classify(4.0, 3.0), None);
        assert_eq!(ScrollAxis::classify(0.0, 0.0), None);
        // One axis clearly ahead decides, in either sign.
        assert_eq!(ScrollAxis::classify(-20.0, 6.0), Some(Horizontal));
        assert_eq!(ScrollAxis::classify(6.0, -20.0), Some(Vertical));
        // An exact diagonal is undecided: either owner could take it, so wait for the next move.
        assert_eq!(ScrollAxis::classify(12.0, 12.0), None);
        assert_eq!(ScrollAxis::classify(-9.0, 9.0), None);
    }

    #[test]
    fn a_held_back_drag_can_still_start_on_a_later_move() {
        // The finger crosses the slop on a diagonal that decides nothing (7,5), so the component
        // is not allowed to start yet; the next move is clearly horizontal and the drag begins
        // there — a drag that was merely held back is not cancelled.
        let mut t = tracker(true);
        t.on_move((17.0, 15.0), false);
        assert_eq!(t.on_move((30.0, 15.0), true), GestureAction::DragStart((30.0, 15.0)));
        assert_eq!(
            t.on_move((40.0, 15.0), true),
            GestureAction::DragMove((40.0, 15.0), (10.0, 0.0))
        );
    }

    #[test]
    fn an_arbitration_cancel_does_not_rearm_the_drag() {
        // The scroll won: the component's drag is dead for the rest of the gesture, and the
        // release must not look like the end of a drag the component never began.
        let mut t = tracker(true);
        t.on_move((30.0, 10.0), true);
        t.cancel_drag_for_arbitration();
        assert_eq!(t.on_move((40.0, 10.0), false), GestureAction::None);
        assert_eq!(t.on_up(), GestureAction::None);
    }
}
