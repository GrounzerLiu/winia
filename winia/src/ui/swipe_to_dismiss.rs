//! `SwipeToDismissBox` — a row that can be swiped horizontally to dismiss it.
//!
//! Models Compose Material 3 `SwipeToDismissBox`: the content translates with the finger, a
//! caller-supplied background is revealed underneath, and on release the row either settles back
//! or leaves in the swipe direction. `on_dismiss` tells the caller which way the row left, and the
//! caller removes the item.
//!
//! Differences from the Compose component (all deliberate, see `docs/swipe-to-dismiss.md`):
//! - `snap_to` / `reset` / `dismiss` are synchronous — they register an animation and return,
//!   where Compose's are suspend functions the caller awaits.
//! - The anchors are physical rather than direction-relative: `StartToEnd` is `+width` and
//!   `EndToStart` is `-width` under both layout directions. Compose's implementation is
//!   offset-driven and does the same (it never inspects the layout direction), so a direction-aware
//!   flip here would be a new rule of winia's rather than an alignment.
//! - No semantics / accessibility actions: the semantics layer is deferred (see
//!   `docs/semantics-gap.md`).
//!
//! A row inside a scrolling list shares the axis arbitration with the rest of the framework: a
//! vertical drag belongs to the list, a horizontal one to the row (see `app::gesture_move`).

use crate::composable;
use crate::core::composer::ComposeCtx;
use crate::core::state::State;
use crate::layout::Alignment;
use crate::modifier::Modifier;
use crate::ui::anchored_draggable::{AnchoredDraggableState, DraggableAnchors};
use crate::ui::layout_components::Stack;
use std::sync::Arc;

/// Distance (logical px) a drag must pass for the release to land on the dismiss anchor — Compose
/// `SwipeToDismissBoxDefaults.positionalThreshold` (56.dp). It is a DISTANCE, not a fraction: at
/// the default half-the-remaining-distance rule a row would have to travel a fraction of its own
/// width, so a wide row (a tablet's) would need a pointlessly long drag.
pub const SWIPE_DISMISS_POSITIONAL_THRESHOLD: f32 = 56.0;

/// Fling speed (logical px/s) above which a release dismisses regardless of how far it travelled —
/// Compose's private `DismissVelocityThreshold` (125.dp/s).
pub const SWIPE_DISMISS_VELOCITY_THRESHOLD: f32 = 125.0;

/// How far a gesture must travel sideways (logical px) before this row's drag claims it: the
/// framework's touch slop, used the way Compose's horizontal drag detector uses it.
const GESTURE_LOCK_SLOP: f32 = crate::input::gesture::TOUCH_SLOP;

/// How close to the dismiss anchor counts as "arrived" for `on_dismiss`.
///
/// Not zero: the callback makes the caller rebuild its list, and that rebuild is composed in a LATER
/// pass (the write happens while the tree is composing), so firing exactly on the anchor paints one
/// frame of the row with its content already slid out — a fully revealed background flash (reported by
/// eye in the demo). 4 px is the tail of the settle tween: `EaseOutCubic`'s last 4 px of a width-sized
/// travel take roughly 20 % of the 250 ms duration (~3 frames at 60 Hz), which is enough for the
/// rebuild to land on the frame the content clears the row instead of after it.
const ARRIVAL_EPSILON: f32 = 4.0;

/// The row's own orientation lock (see the drag callbacks in [`SwipeToDismissBox::build`]):
/// accumulated travel since the drag started, plus whether the horizontal side has won. Once won the
/// lock is kept, exactly like a drag detector that has taken the touch slop.
#[derive(Default)]
struct Travel {
    dx: f32,
    dy: f32,
    locked: bool,
}

impl Travel {
    /// Accumulate one delta and answer whether the row owns the gesture.
    fn accumulate(&mut self, dx: f32, dy: f32) -> bool {
        self.dx += dx;
        self.dy += dy;
        if !self.locked && self.dx.abs() > self.dy.abs() && self.dx.abs() >= GESTURE_LOCK_SLOP {
            self.locked = true;
        }
        self.locked
    }
}

/// Where a swipe-to-dismiss row is parked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwipeToDismissBoxValue {
    /// At rest, on top of its background.
    Settled,
    /// Dismissed towards the END edge (the right in LTR) — parked at `+width`.
    StartToEnd,
    /// Dismissed towards the START edge (the left in LTR) — parked at `-width`.
    EndToStart,
}

impl PartialOrd for SwipeToDismissBoxValue {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SwipeToDismissBoxValue {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use SwipeToDismissBoxValue::*;
        // Ordering by the anchor the value sits on (`-width < 0 < +width`): `DraggableAnchors`
        // keeps its values in this order, and every lookup in it goes through the positions.
        let rank = |value| match value {
            EndToStart => 0,
            Settled => 1,
            StartToEnd => 2,
        };
        rank(*self).cmp(&rank(*other))
    }
}

/// State of a [`SwipeToDismissBox`] — three anchors over [`AnchoredDraggableState`].
#[derive(Clone)]
pub struct SwipeToDismissBoxState {
    anchored: AnchoredDraggableState<SwipeToDismissBoxValue>,
    /// The latch for `on_dismiss`: the dismissed value already reported, if any.
    ///
    /// It lives in the STATE rather than in the box's composition group, so it follows a caller-owned
    /// state across a rebuild: a row whose group is re-created while the state stays parked (a keyed
    /// list rebuilt under it, the state kept in the caller's own map) must not report the same
    /// dismissal twice.
    reported: State<Option<SwipeToDismissBoxValue>>,
}

impl SwipeToDismissBoxState {
    /// State parked at `initial_value` (normally `Settled`).
    pub fn new(initial_value: SwipeToDismissBoxValue) -> Self {
        let mut anchored = AnchoredDraggableState::new(initial_value);
        anchored.set_velocity_threshold_dp(SWIPE_DISMISS_VELOCITY_THRESHOLD);
        Self { anchored, reported: State::new(None) }
    }

    /// The dismissed value already reported to `on_dismiss`, if any.
    fn reported(&self) -> Option<SwipeToDismissBoxValue> {
        self.reported.get()
    }

    /// Record that a dismissal has been reported.
    fn set_reported(&self, value: Option<SwipeToDismissBoxValue>) {
        self.reported.set(value);
    }

    /// The anchor the offset sits nearest to (Compose `currentValue`).
    pub fn current_value(&self) -> SwipeToDismissBoxValue {
        self.anchored.current_value()
    }

    /// Where the row is heading: the drag target while a finger is down, else the nearest anchor
    /// (Compose `targetValue`).
    pub fn target_value(&self) -> SwipeToDismissBoxValue {
        self.anchored.target_value()
    }

    /// The anchor the row settled on (Compose `settledValue`).
    pub fn settled_value(&self) -> SwipeToDismissBoxValue {
        self.anchored.settled_value()
    }

    /// Current offset in px — `NaN` until the first layout (Compose `requireOffset`).
    pub fn offset(&self) -> f32 {
        self.anchored.offset()
    }

    /// The offset `State` the content's placement follows. A layout dependency: the slide then
    /// costs no recomposition.
    pub fn offset_state(&self) -> State<f32> {
        self.anchored.offset_state()
    }

    /// Which way the row is currently going — the sign of the offset, `Settled` at rest
    /// (Compose `dismissDirection`).
    pub fn dismiss_direction(&self) -> SwipeToDismissBoxValue {
        let offset = self.anchored.offset();
        if offset > 0.0 {
            SwipeToDismissBoxValue::StartToEnd
        } else if offset < 0.0 {
            SwipeToDismissBoxValue::EndToStart
        } else {
            // Also the `NaN` case (no layout yet): comparisons against `NaN` are both false.
            SwipeToDismissBoxValue::Settled
        }
    }

    /// How far the row has travelled towards dismissal: 0 at `Settled`, 1 at the anchor of the
    /// direction the offset is currently in.
    ///
    /// Compose's `progress` is documented as the fraction from `currentValue` to `targetValue`, but
    /// its arithmetic divides by zero whenever the two agree (which is most of the time, and always
    /// once the row is parked at a dismiss anchor). winia reports the displacement instead — that is
    /// what a reveal panel driven by this number wants (a background that fades in as the row is
    /// dragged out), and it is well defined in every state.
    pub fn progress(&self) -> f32 {
        let direction = self.dismiss_direction();
        if direction == SwipeToDismissBoxValue::Settled {
            return 0.0;
        }
        self.anchored
            .progress(&SwipeToDismissBoxValue::Settled, &direction)
    }

    /// Position of an anchor in px without registering a dependency (`NaN` when that direction is
    /// disabled) — for arrival checks and paint-time reads.
    pub fn position_of(&self, value: SwipeToDismissBoxValue) -> f32 {
        self.anchored.peek_position_of(&value)
    }

    /// Is the settle animation still running?
    pub fn is_animation_running(&self) -> bool {
        self.anchored.is_animation_running()
    }

    /// Jump to a value with no animation (Compose `snapTo`).
    pub fn snap_to(&self, value: SwipeToDismissBoxValue) {
        self.anchored.snap_to(value);
    }

    /// Animate back to `Settled` (Compose `reset`).
    pub fn reset(&self) {
        self.anchored.animate_to(SwipeToDismissBoxValue::Settled);
    }

    /// Animate towards a dismiss anchor (Compose `dismiss`). `Settled` is a no-op — that is `reset`'s
    /// job — and so is a direction that is switched off: it has no anchor, and animating to a value
    /// without a position would leave the state saying "dismissed" while the row never moved and its
    /// gestures stayed gated off.
    pub fn dismiss(&self, direction: SwipeToDismissBoxValue) {
        if direction == SwipeToDismissBoxValue::Settled {
            return;
        }
        if self.position_of(direction).is_nan() {
            return;
        }
        self.anchored.animate_to(direction);
    }

    /// Feed a horizontal drag delta (from `Modifier::on_drag`).
    pub fn drag_delta(&self, dx: f32) {
        self.anchored.drag_delta(dx);
    }

    /// Settle a release: the fling velocity decides first, the 56 px positional threshold below it.
    pub fn settle(&self) -> SwipeToDismissBoxValue {
        self.settle_with_velocity(self.anchored.last_velocity())
    }

    /// [`Self::settle`] with a velocity the caller measured itself (px/s).
    pub fn settle_with_velocity(&self, velocity: f32) -> SwipeToDismissBoxValue {
        self.anchored
            .settle_with_velocity(velocity, Some(SWIPE_DISMISS_POSITIONAL_THRESHOLD))
    }

    /// The underlying anchored-draggable state (advanced use: custom thresholds).
    pub fn anchored_draggable(&self) -> &AnchoredDraggableState<SwipeToDismissBoxValue> {
        &self.anchored
    }

    /// Rebuild the anchors for a row `width` px wide. A disabled direction gets no anchor at all,
    /// so the row cannot be parked there (`AnchoredDraggableState` clamps to the anchors it has).
    fn update_anchors(&self, width: f32, allow_start: bool, allow_end: bool) {
        let mut anchors = vec![(SwipeToDismissBoxValue::Settled, 0.0)];
        if allow_start {
            anchors.push((SwipeToDismissBoxValue::StartToEnd, width));
        }
        if allow_end {
            anchors.push((SwipeToDismissBoxValue::EndToStart, -width));
        }
        self.anchored.update_anchors(DraggableAnchors::new(anchors));

        // Re-park a settled row exactly on its anchor when the width it is measured against changes
        // (a resize, or the very first layout). A finger or a running tween owns the offset while it
        // lasts — the same rule the drawer and the sheet follow.
        if self.anchored.is_dragging() || self.anchored.is_animation_running() {
            return;
        }
        let parked = self.anchored.settled_value();
        let pos = self.anchored.peek_position_of(&parked);
        if pos.is_nan() {
            // The direction the row is parked in was just disabled: fall back to `Settled`
            // rather than leaving it parked at an offset no anchor explains.
            self.anchored.snap_to(SwipeToDismissBoxValue::Settled);
            return;
        }
        let offset = self.anchored.offset();
        if !offset.is_nan() && (offset - pos).abs() > 0.5 {
            self.anchored.offset_state().set(pos);
        }
    }
}

impl Default for SwipeToDismissBoxState {
    fn default() -> Self {
        Self::new(SwipeToDismissBoxValue::Settled)
    }
}

/// A swipeable row: `background` is revealed as the `content` translates, and a release past the
/// threshold dismisses it.
///
/// The caller removes the item once `on_dismiss` fires — the callback runs when the row has
/// ARRIVED at the dismiss anchor (the slide is over), not when the settle starts.
pub struct SwipeToDismissBox {
    state: Option<SwipeToDismissBoxState>,
    background: Option<Box<dyn Fn(&mut ComposeCtx) + Send + Sync>>,
    modifier: Modifier,
    allow_start: bool,
    allow_end: bool,
    gestures_enabled: bool,
    on_dismiss: Option<Arc<dyn Fn(SwipeToDismissBoxValue) + Send + Sync>>,
}

impl SwipeToDismissBox {
    pub fn new() -> Self {
        Self {
            state: None,
            background: None,
            modifier: Modifier::new(),
            allow_start: true,
            allow_end: true,
            gestures_enabled: true,
            on_dismiss: None,
        }
    }

    /// Use a caller-owned state (Compose `state`); without it the box remembers its own.
    pub fn state(mut self, state: SwipeToDismissBoxState) -> Self {
        self.state = Some(state);
        self
    }

    /// The panel revealed underneath the content (Compose `backgroundContent`). Optional here,
    /// where Compose requires it: a box with nothing to reveal (a plain delete row) would
    /// otherwise have to pass an empty closure.
    pub fn background(mut self, content: impl Fn(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        self.background = Some(Box::new(content));
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = modifier;
        self
    }

    /// Allow a dismiss towards the END edge (Compose `enableDismissFromStartToEnd`, default true).
    pub fn enable_dismiss_from_start_to_end(mut self, enabled: bool) -> Self {
        self.allow_start = enabled;
        self
    }

    /// Allow a dismiss towards the START edge (Compose `enableDismissFromEndToStart`, default true).
    pub fn enable_dismiss_from_end_to_start(mut self, enabled: bool) -> Self {
        self.allow_end = enabled;
        self
    }

    /// Whether the row reacts to drags at all (Compose `gesturesEnabled`, default true).
    pub fn gestures_enabled(mut self, enabled: bool) -> Self {
        self.gestures_enabled = enabled;
        self
    }

    /// Called once when the row has settled in a dismissed direction, with that direction.
    pub fn on_dismiss(
        mut self,
        callback: impl Fn(SwipeToDismissBoxValue) + Send + Sync + 'static,
    ) -> Self {
        self.on_dismiss = Some(Arc::new(callback));
        self
    }

    /// `#[composable]` so the state reads here (the settle-arrival check in particular) re-enter this
    /// component instead of the caller's whole subtree: the arrival check samples the offset while the
    /// slide runs, which would otherwise recompose everything around the row.
    ///
    /// The state is remembered here when the caller does not pass one, and that works in a keyed list
    /// (measured: dismiss a row, scroll the list, scroll back, dismiss the row now on top — both land).
    /// Two things it does not do, both measured:
    ///
    /// - **A rebuild that is not keyed** (a plain `for` loop over the data) hands the arriving item the
    ///   remembered state of the list POSITION the dismissed row left, so the row comes up parked at the
    ///   dismiss anchor. Key the list, or pass a state you own.
    /// - **A reorder does not carry state to the moved item**: keyed `LazyColumn` items are keyed by
    ///   (position, item key), so an item that changes position gets a fresh state. Keep the state in
    ///   your own data (keyed by the item) if it has to follow the item when the list is reordered.
    ///
    /// The fixture `fixture_swipe_dismiss` and the demo pass a state they created in the item's own
    /// scope, which is also what a caller who reads `progress()` or drives `dismiss(v)` needs.
    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx, content: impl Fn(&mut ComposeCtx) + Send + Sync + 'static) {
        let SwipeToDismissBox { state, background, modifier, allow_start, allow_end, gestures_enabled, on_dismiss } = self;
        // Declare the scalar parameters: a slot compares these to decide whether this component has to
        // re-run at all (`ctx.changed` is how the framework's builders do it), and the flags below are
        // plain values a caller can flip without any other dependency changing.
        ctx.changed(&allow_start);
        ctx.changed(&allow_end);
        ctx.changed(&gestures_enabled);
        let holder = ctx.remember(|| SwipeToDismissBoxState::default());
        let state = state.unwrap_or_else(|| holder.get());
        // A state that has not been measured yet has NO offset (`AnchoredDraggableState` starts at
        // NaN and the first layout seeds it from the measured width). The content's placement reads
        // that offset, and NaN places it nowhere — the row paints its background for that frame. It is
        // visible whenever a row's state is re-created mid-list: measured, removing a row re-creates
        // the state of every row below it (their lazy items are re-keyed) and each of them flashed its
        // revealed panel for a frame. Seed the offset with the current value's anchor — 0 for a row
        // parked at `Settled`, which is what a fresh state is.
        if state.offset().is_nan() {
            let parked = state.position_of(state.settled_value());
            state.offset_state().set(if parked.is_nan() { 0.0 } else { parked });
        }
        let settled = state.settled_value();

        // `fired` is the once-per-dismissal latch. It lives in the STATE (see the field), and it is a
        // plain state read (not a peek) so that the guard below is re-evaluated when it changes.
        if settled == SwipeToDismissBoxValue::Settled {
            state.set_reported(None);
        } else {
            // `offset()` is a TRACKED read: it makes the arrival of the settle animation
            // observable here, which is what lets `on_dismiss` fire after the slide rather than at
            // its start (the caller removes the row, and removing it early would cut the slide
            // short). The read happens only while the row is dismissed, so a drag — where nothing
            // is dismissed — never recomposes the row per frame.
            let target = state.position_of(settled);
            // ⚠ The offset is read OUTSIDE any gate, and that order is load-bearing: the tracked read
            // is what keeps this group re-running while the settle advances, so the check below is
            // re-evaluated as the row moves.
            let offset = state.offset();
            if !target.is_nan() {
                let arrived = !offset.is_nan() && (offset - target).abs() <= ARRIVAL_EPSILON;
                // Arrival is judged by the DISTANCE, not by `is_animation_running()`: the offset
                // sitting on the anchor is the fact the caller cares about, and it is the one signal
                // that cannot get stuck. Measured: the animation table can keep reporting a finished
                // tween as running (a row parked exactly on its anchor with `is_animation_running()`
                // still true), which under an animation-flag gate left the row dismissed on screen
                // while `on_dismiss` never fired.
                if arrived {
                    if state.reported() != Some(settled) {
                        state.set_reported(Some(settled));
                        if let Some(callback) = &on_dismiss {
                            callback(settled);
                        }
                    }
                } else if !state.is_animation_running() {
                    // Not there yet and nothing animating: the anchor moved under a row that had
                    // already settled (a resize while the tween ran, and `update_anchors` leaves a
                    // running animation alone). Re-park so the arrival above can be satisfied.
                    state.offset_state().set(target);
                }
            }
        }

        let mut host = Modifier::new().on_size_changed({
            let state = state.clone();
            move |width, _| state.update_anchors(width, allow_start, allow_end)
        });
        // The callbacks stay attached for the row's whole life and gate on the state inside instead
        // of being attached only while the row is settled. Compose gates with
        // `enabled = gesturesEnabled && settledValue == Settled`, and the callback check is that same
        // rule — with a modifier chain whose SHAPE does not change from one composition to the next
        // (a shape that changes is a hazard for a row rebuilt into a new slot).
        if gestures_enabled {
            let drag_state = state.clone();
            let end_state = state.clone();
            // The half of the arbitration a component can do on its own: a row with no scroll ancestor
            // (or one inside an overlay, where the framework's arbitration is off) must not be dismissed
            // by the lateral drift of a mostly-vertical swipe. Deltas are ignored until the gesture has
            // travelled further sideways than vertically past the touch slop, and it stays locked once
            // it has — Compose's horizontal detector wins the slop the same way.
            let travel = Arc::new(std::sync::Mutex::new(Travel::default()));
            let travel_reset = Arc::clone(&travel);
            let travel_move = Arc::clone(&travel);
            host = host
                .on_drag_start(move |_pos| {
                    if let Ok(mut t) = travel_reset.lock() {
                        *t = Travel::default();
                    }
                })
                .on_drag(move |_pos, (dx, dy)| {
                    // A dismissed row must not be dragged back into view.
                    if drag_state.settled_value() != SwipeToDismissBoxValue::Settled {
                        return;
                    }
                    let locked = match travel_move.lock() {
                        Ok(mut t) => t.accumulate(dx, dy),
                        Err(_) => false,
                    };
                    if locked {
                        drag_state.drag_delta(dx);
                    }
                })
                .on_drag_end(move || {
                    if end_state.settled_value() == SwipeToDismissBoxValue::Settled {
                        end_state.settle();
                    }
                });
        }
        let host = host.then(modifier);

        Stack::new().modifier(host).alignment(Alignment::Start).build(ctx, |ctx| {
            if let Some(background) = background {
                background(ctx);
            }
            // The content is moved by a LAYOUT offset rather than a graphics translation: the
            // placement participates in hit testing, so the content is clickable where it is drawn.
            // `absolute_offset`, not `offset`: the displacement is a physical distance, and a plain
            // offset would mirror its x under RTL.
            Stack::new()
                .modifier(Modifier::new().absolute_offset(state.offset_state(), 0.0))
                .build(ctx, |ctx| content(ctx));
        });
    }
}

impl Default for SwipeToDismissBox {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The settles push tweens into the process-wide animation table, so these tests hold
    /// `TEST_SERIAL` and clear the table afterwards — AGENTS.md requires the isolation.
    struct AnimGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl AnimGuard {
        fn new() -> Self {
            let lock = crate::animation::tests::TEST_SERIAL
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            crate::animation::clear_all_animations();
            Self { _lock: lock }
        }
    }

    impl Drop for AnimGuard {
        fn drop(&mut self) {
            crate::animation::clear_all_animations();
        }
    }

    fn state() -> SwipeToDismissBoxState {
        SwipeToDismissBoxState::new(SwipeToDismissBoxValue::Settled)
    }

    #[test]
    fn anchors_follow_the_enabled_directions() {
        let _guard = AnimGuard::new();

        let both = state();
        both.update_anchors(300.0, true, true);
        assert_eq!(both.position_of(SwipeToDismissBoxValue::Settled), 0.0);
        assert_eq!(both.position_of(SwipeToDismissBoxValue::StartToEnd), 300.0);
        assert_eq!(both.position_of(SwipeToDismissBoxValue::EndToStart), -300.0);

        let start_only = state();
        start_only.update_anchors(300.0, true, false);
        assert_eq!(start_only.position_of(SwipeToDismissBoxValue::StartToEnd), 300.0);
        assert!(
            start_only.position_of(SwipeToDismissBoxValue::EndToStart).is_nan(),
            "a disabled direction must have no anchor to settle on"
        );

        let end_only = state();
        end_only.update_anchors(300.0, false, true);
        assert!(end_only.position_of(SwipeToDismissBoxValue::StartToEnd).is_nan());
        assert_eq!(end_only.position_of(SwipeToDismissBoxValue::EndToStart), -300.0);

        let neither = state();
        neither.update_anchors(300.0, false, false);
        assert!(neither.position_of(SwipeToDismissBoxValue::StartToEnd).is_nan());
        assert!(neither.position_of(SwipeToDismissBoxValue::EndToStart).is_nan());
        assert_eq!(neither.position_of(SwipeToDismissBoxValue::Settled), 0.0);
    }

    #[test]
    fn a_slow_release_past_the_positional_threshold_dismisses() {
        let _guard = AnimGuard::new();

        // 60 px is past the 56 px threshold but still NEARER to `Settled` than to the anchor one
        // width away (240 px) — with the default half-the-distance rule the row would spring back.
        let dismissed = state();
        dismissed.update_anchors(300.0, true, true);
        dismissed.drag_delta(60.0);
        assert_eq!(
            dismissed.settle_with_velocity(10.0),
            SwipeToDismissBoxValue::StartToEnd,
            "the positional threshold is a distance, not half the row"
        );

        let kept = state();
        kept.update_anchors(300.0, true, true);
        kept.drag_delta(50.0);
        assert_eq!(
            kept.settle_with_velocity(10.0),
            SwipeToDismissBoxValue::Settled,
            "just under the threshold springs back"
        );
    }

    #[test]
    fn a_fling_dismisses_below_the_positional_threshold() {
        let _guard = AnimGuard::new();
        let s = state();
        s.update_anchors(300.0, true, true);
        s.drag_delta(10.0);
        assert_eq!(
            s.settle_with_velocity(400.0),
            SwipeToDismissBoxValue::StartToEnd,
            "past 125 px/s the fling decides, not the distance"
        );
        assert_eq!(
            s.settled_value(),
            SwipeToDismissBoxValue::StartToEnd,
            "the row is parked in the direction it left in"
        );
    }

    #[test]
    fn dismiss_direction_follows_the_offset_sign() {
        let _guard = AnimGuard::new();
        let s = state();
        s.update_anchors(300.0, true, true);
        assert_eq!(
            s.dismiss_direction(),
            SwipeToDismissBoxValue::Settled,
            "a row at rest is going nowhere"
        );
        s.drag_delta(-40.0);
        assert_eq!(s.dismiss_direction(), SwipeToDismissBoxValue::EndToStart);
        s.drag_delta(80.0);
        assert_eq!(s.dismiss_direction(), SwipeToDismissBoxValue::StartToEnd);
    }

    #[test]
    fn progress_follows_the_displacement_towards_the_dismiss_anchor() {
        let _guard = AnimGuard::new();
        let s = state();
        s.update_anchors(300.0, true, true);
        assert_eq!(s.progress(), 0.0, "nothing is dismissed yet");
        s.drag_delta(150.0);
        let p = s.progress();
        assert!(
            (p - 0.5).abs() < 0.01,
            "half way to the dismiss anchor is half the progress, got {p}"
        );
        s.drag_delta(-150.0); // back to rest
        assert_eq!(s.progress(), 0.0, "and it unwinds as the row comes back");
    }

    #[test]
    fn a_resize_keeps_a_settled_row_on_its_anchor() {
        let _guard = AnimGuard::new();
        let s = state();
        s.update_anchors(300.0, true, true);
        s.snap_to(SwipeToDismissBoxValue::EndToStart);
        assert_eq!(s.offset(), -300.0);

        s.update_anchors(400.0, true, true); // the row got wider while it sat there
        assert_eq!(s.offset(), -400.0, "the parked anchor moved with the width");
        assert_eq!(s.settled_value(), SwipeToDismissBoxValue::EndToStart);
    }

    #[test]
    fn disabling_a_direction_parks_a_row_stuck_in_it() {
        let _guard = AnimGuard::new();
        let s = state();
        s.update_anchors(300.0, true, true);
        s.snap_to(SwipeToDismissBoxValue::StartToEnd);

        s.update_anchors(300.0, false, true); // that direction is switched off at runtime
        assert_eq!(s.settled_value(), SwipeToDismissBoxValue::Settled);
        assert_eq!(s.offset(), 0.0, "the row came back instead of hanging off the edge");
    }
}
