//! `SwipeToDismissBox` — a row that can be swiped horizontally to dismiss it.
//!
//! Ported from Material 3 / Compose (`SwipeToDismissBox.kt`; the plan is in
//! `docs/swipe-to-dismiss-plan.md`). The state is a thin vocabulary over
//! [`AnchoredDraggableState`], which already provides the whole machine — anchors, drag deltas, a settle
//! with a positional threshold and a fling velocity, progress and a value-change veto.
//!
//! ```ignore
//! let state = ctx.remember(|| SwipeToDismissBoxState::new(SwipeToDismissBoxValue::Settled)).get();
//! SwipeToDismissBox::new(state.clone())
//!     .background(|ctx| { /* an "archive" panel, revealed as the row slides */ })
//!     .on_dismiss(move |direction| { /* remove the row */ })
//!     .build(ctx, |ctx| { /* the row itself */ });
//! ```

use std::sync::Arc;

use crate::composable;
use crate::core::composer::ComposeCtx;
use crate::core::state::State;
use crate::layout::LayoutDirection;
use crate::modifier::Modifier;
use crate::ui::anchored_draggable::{AnchoredDraggableState, DraggableAnchors};
use crate::ui::layout_components::Stack;
use crate::ui::theme::WiniaTheme;

/// The directions in which a [`SwipeToDismissBox`] can be dismissed (Compose's
/// `SwipeToDismissBoxValue`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SwipeToDismissBoxValue {
    /// Dismissed by swiping in the reading direction (to the right under LTR).
    StartToEnd,
    /// Dismissed by swiping against the reading direction (to the left under LTR).
    EndToStart,
    /// Not dismissed — the resting position.
    Settled,
}

/// The anchors are stored in value order and searched positionally, so the ordering has to follow the
/// POSITION of each value (`EndToStart` is the most negative offset, `Settled` is zero) rather than the
/// declaration order — the same reason `SheetValue` implements `Ord` by hand.
impl Ord for SwipeToDismissBoxValue {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let order = |v: &SwipeToDismissBoxValue| match v {
            SwipeToDismissBoxValue::EndToStart => 0,
            SwipeToDismissBoxValue::Settled => 1,
            SwipeToDismissBoxValue::StartToEnd => 2,
        };
        order(self).cmp(&order(other))
    }
}

impl PartialOrd for SwipeToDismissBoxValue {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Defaults for [`SwipeToDismissBox`], from Compose's `SwipeToDismissBoxDefaults`.
pub struct SwipeToDismissBoxDefaults;

impl SwipeToDismissBoxDefaults {
    /// Distance the drag has to pass for a release to land on the dismiss anchor: 56 dp
    /// (`SwipeToDismissBoxDefaults.positionalThreshold`). A distance, not a fraction.
    ///
    /// One dp is one LOGICAL px in this framework, so the token is used as it is (the sheet's
    /// `sheet_panel_geometry` carries the same note; `Dp::to_px` would ask for the density-scaled value).
    pub const POSITIONAL_THRESHOLD_DP: f32 = 56.0;
    /// Fling velocity that dismisses regardless of distance: 125 dp/s (`DismissVelocityThreshold`, and the
    /// same number `AnchoredDraggableState` defaults to).
    pub const DISMISS_VELOCITY_THRESHOLD_DP: f32 = 125.0;
}

/// State of a [`SwipeToDismissBox`] (Compose's `SwipeToDismissBoxState`).
#[derive(Clone)]
pub struct SwipeToDismissBoxState {
    anchored: AnchoredDraggableState<SwipeToDismissBoxValue>,
    /// Whether the start→end direction may dismiss at all — it decides which anchors exist, so a
    /// disabled direction is unreachable rather than merely ignored.
    enable_start_to_end: State<bool>,
    enable_end_to_start: State<bool>,
    /// Mirror the anchors: under RTL the READING direction is to the left, so `StartToEnd` moves toward a
    /// negative x.
    rtl: State<bool>,
}

impl SwipeToDismissBoxState {
    /// A state resting at `initial_value` (usually `Settled`).
    pub fn new(initial_value: SwipeToDismissBoxValue) -> Self {
        let mut anchored = AnchoredDraggableState::new(initial_value);
        anchored.set_velocity_threshold_dp(SwipeToDismissBoxDefaults::DISMISS_VELOCITY_THRESHOLD_DP);
        Self {
            anchored,
            enable_start_to_end: State::new(true),
            enable_end_to_start: State::new(true),
            rtl: State::new(false),
        }
    }

    /// The current state value (the anchor the offset is closest to).
    pub fn current_value(&self) -> SwipeToDismissBoxValue {
        self.anchored.current_value()
    }

    /// The value being animated or dragged toward.
    pub fn target_value(&self) -> SwipeToDismissBoxValue {
        self.anchored.target_value()
    }

    /// The value the box has SETTLED at — it stays `Settled` for the whole of a dismissal animation.
    pub fn settled_value(&self) -> SwipeToDismissBoxValue {
        self.anchored.settled_value()
    }

    /// Fraction of the way from the settled value to the target, `0..=1` — for a background that reacts to
    /// the drag.
    pub fn progress(&self) -> f32 {
        let from = self.anchored.settled_value();
        let to = self.anchored.target_value();
        if from == to {
            return 1.0;
        }
        self.anchored.progress(&from, &to)
    }

    /// Which way the box is going (or has been dismissed) — `Settled` while it is at rest.
    pub fn dismiss_direction(&self) -> SwipeToDismissBoxValue {
        let off = self.anchored.offset();
        if off.is_nan() || off == 0.0 {
            SwipeToDismissBoxValue::Settled
        } else if off > 0.0 {
            SwipeToDismissBoxValue::StartToEnd
        } else {
            SwipeToDismissBoxValue::EndToStart
        }
    }

    /// Current offset in logical px (positive = toward the end of the reading direction).
    pub fn offset(&self) -> f32 {
        self.anchored.offset()
    }

    /// The offset as a `State`, for a layout offset that follows it without recomposing.
    pub fn offset_state(&self) -> State<f32> {
        self.anchored.offset_state()
    }

    /// Whether a gesture currently owns the offset.
    pub fn is_dragging(&self) -> bool {
        self.anchored.is_dragging()
    }

    /// The offset this value's anchor sits at (`NaN` when that direction has no anchor).
    pub fn position_of(&self, value: SwipeToDismissBoxValue) -> f32 {
        self.anchored.position_of(&value)
    }

    /// Feed a drag delta (the x component of a pointer move).
    pub fn drag_delta(&self, dx: f32) {
        self.anchored.drag_delta(dx);
    }

    /// Settle after a drag: the fling velocity decides if it is past the threshold, the 56 dp positional
    /// threshold otherwise. Returns the value it settled toward.
    pub fn settle(&self) -> SwipeToDismissBoxValue {
        self.settle_with_velocity(self.anchored.last_velocity())
    }

    /// [`Self::settle`] with an explicit velocity (Compose's `settle(velocity)`), for a caller that tracks
    /// the pointer itself — and for tests, which have no gesture timings to measure.
    pub fn settle_with_velocity(&self, velocity: f32) -> SwipeToDismissBoxValue {
        self.anchored
            .settle_with_velocity(velocity, Some(SwipeToDismissBoxDefaults::POSITIONAL_THRESHOLD_DP))
    }

    /// Animate back to `Settled`.
    pub fn reset(&self) {
        self.anchored.animate_to(SwipeToDismissBoxValue::Settled);
    }

    /// Animate to a dismissed direction.
    pub fn dismiss(&self, direction: SwipeToDismissBoxValue) {
        self.anchored.animate_to(direction);
    }

    /// Jump to a value without animating.
    pub fn snap_to(&self, value: SwipeToDismissBoxValue) {
        self.anchored.snap_to(value);
    }

    /// Veto a pending state change (Compose's deprecated `confirmValueChange`, kept as the lower-level
    /// hook rather than a constructor argument).
    pub fn set_confirm_value_change(&mut self, f: impl Fn(SwipeToDismissBoxValue) -> bool + Send + Sync + 'static) {
        self.anchored.set_confirm_value_change(move |v: &SwipeToDismissBoxValue| f(*v));
    }

    /// The velocity threshold in dp/s above which a release dismisses regardless of distance.
    pub fn set_velocity_threshold_dp(&mut self, dp: f32) {
        self.anchored.set_velocity_threshold_dp(dp);
    }

    /// Recompute the anchors for a box `width` px wide: `Settled` at 0, then one anchor per ENABLED
    /// direction at exactly one width away (so a dismissed row rests fully off-screen).
    pub fn update_anchors(&self, width: f32) {
        if width <= 0.0 || width.is_nan() {
            return;
        }
        let start_to_end = if self.rtl.get() { -width } else { width };
        let end_to_start = -start_to_end;
        let mut pairs = vec![(SwipeToDismissBoxValue::Settled, 0.0)];
        if self.enable_start_to_end.get() {
            pairs.push((SwipeToDismissBoxValue::StartToEnd, start_to_end));
        }
        if self.enable_end_to_start.get() {
            pairs.push((SwipeToDismissBoxValue::EndToStart, end_to_start));
        }
        self.anchored.update_anchors(DraggableAnchors::new(pairs));
    }

    fn set_enable_start_to_end(&self, v: bool) {
        self.enable_start_to_end.set(v);
    }

    fn set_enable_end_to_start(&self, v: bool) {
        self.enable_end_to_start.set(v);
    }

    fn set_rtl(&self, v: bool) {
        self.rtl.set(v);
    }
}

/// A row that can be swiped horizontally to dismiss it (Compose's `SwipeToDismissBox`).
pub struct SwipeToDismissBox {
    state: SwipeToDismissBoxState,
    background: Option<Box<dyn Fn(&mut ComposeCtx) + Send + Sync>>,
    modifier: Modifier,
    on_dismiss: Option<Arc<dyn Fn(SwipeToDismissBoxValue) + Send + Sync>>,
    enable_start_to_end: bool,
    enable_end_to_start: bool,
    gestures_enabled: bool,
}

impl SwipeToDismissBox {
    /// A box over `state`.
    pub fn new(state: SwipeToDismissBoxState) -> Self {
        Self {
            state,
            background: None,
            modifier: Modifier::new(),
            on_dismiss: None,
            enable_start_to_end: true,
            enable_end_to_start: true,
            gestures_enabled: true,
        }
    }

    /// What the row reveals as it slides: laid out behind the content, filling the box, and NOT moved by
    /// the drag (Compose's `backgroundContent`).
    pub fn background(mut self, background: impl Fn(&mut ComposeCtx) + Send + Sync + 'static) -> Self {
        self.background = Some(Box::new(background));
        self
    }

    /// Called once the box has settled in a dismissed direction, with that direction.
    pub fn on_dismiss(mut self, f: impl Fn(SwipeToDismissBoxValue) + Send + Sync + 'static) -> Self {
        self.on_dismiss = Some(Arc::new(f));
        self
    }

    /// Whether swiping toward the end of the reading direction dismisses (default true).
    pub fn enable_dismiss_from_start_to_end(mut self, v: bool) -> Self {
        self.enable_start_to_end = v;
        self
    }

    /// Whether swiping against the reading direction dismisses (default true).
    pub fn enable_dismiss_from_end_to_start(mut self, v: bool) -> Self {
        self.enable_end_to_start = v;
        self
    }

    /// Whether a drag does anything at all (default true).
    pub fn gestures_enabled(mut self, v: bool) -> Self {
        self.gestures_enabled = v;
        self
    }

    /// Extra modifier for the box.
    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx, content: impl Fn(&mut ComposeCtx) + Send + Sync + 'static) {
        let state = self.state;
        let gestures = self.gestures_enabled;

        // Which directions may dismiss and the ambient reading direction decide the ANCHORS, so they have
        // to be in place before the layout callback recomputes them.
        state.set_enable_start_to_end(self.enable_start_to_end);
        state.set_enable_end_to_start(self.enable_end_to_start);
        state.set_rtl(WiniaTheme::direction() == LayoutDirection::Rtl);

        // The dismissal callback fires once per dismissal, when the box is finally off-screen.
        let fired: State<bool> = ctx.remember(|| false);
        let settled_now = state.settled_value();
        if settled_now == SwipeToDismissBoxValue::Settled {
            fired.set(false);
        } else if !fired.get() {
            // `settled_value` flips when the dismissal is REQUESTED, so wait for the offset to reach the
            // anchor — the same geometry check the bottom sheet needs (see docs/bottom-sheet.md): the
            // tracked read of the offset is what re-runs this while the row slides out.
            let target = state.position_of(settled_now);
            let off = state.offset();
            if !off.is_nan() && !target.is_nan() && (off - target).abs() <= 0.5 {
                fired.set(true);
                if let Some(cb) = &self.on_dismiss {
                    (cb)(settled_now);
                }
            }
        }

        let st_anchors = state.clone();
        let st_drag = state.clone();
        let st_end = state.clone();

        let mut box_mod = self.modifier.on_size_changed(move |w, _h| {
            // The anchors are one width away, so they are only known once the box is measured.
            st_anchors.update_anchors(w);
        });
        if gestures {
            box_mod = box_mod
                .on_drag(move |_pos, (dx, _dy)| {
                    // Compose gates the gesture the same way: once the box has settled in a dismissed
                    // direction it cannot be dragged back (the caller removes the row).
                    if st_drag.settled_value() == SwipeToDismissBoxValue::Settled {
                        st_drag.drag_delta(dx);
                    }
                })
                .on_drag_end(move || {
                    if st_end.settled_value() != SwipeToDismissBoxValue::Settled {
                        return;
                    }
                    // Fling velocity if any, the 56 dp positional threshold otherwise.
                    st_end.settle();
                });
        }

        let st_offset = state.clone();
        let background = self.background;
        Stack::new()
            .modifier(box_mod)
            .build(ctx, |ctx| {
                if let Some(background) = background {
                    Stack::new()
                        .modifier(Modifier::new().fill_max_size())
                        .build(ctx, |ctx| background(ctx));
                }
                // The content carries the translation, the background does not: `absolute_offset_x`, not
                // `offset` — the value is already direction-aware (the anchors mirror for RTL) and
                // `offset` would mirror it a second time.
                Stack::new()
                    .modifier(Modifier::new()
                        .fill_max_width()
                        .absolute_offset_x(st_offset.offset_state()))
                    .build(ctx, |ctx| content(ctx));
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positional_threshold_matches_compose() {
        assert_eq!(SwipeToDismissBoxDefaults::POSITIONAL_THRESHOLD_DP, 56.0);
        assert_eq!(SwipeToDismissBoxDefaults::DISMISS_VELOCITY_THRESHOLD_DP, 125.0);
    }

    /// The anchors carry one entry per ENABLED direction, each exactly one width away, and `Settled` at
    /// zero — so a row with a direction disabled cannot reach that anchor at all.
    #[test]
    fn anchors_follow_the_enabled_directions() {
        let st = SwipeToDismissBoxState::new(SwipeToDismissBoxValue::Settled);
        st.update_anchors(300.0);
        assert_eq!(st.position_of(SwipeToDismissBoxValue::Settled), 0.0);
        assert_eq!(st.position_of(SwipeToDismissBoxValue::StartToEnd), 300.0);
        assert_eq!(st.position_of(SwipeToDismissBoxValue::EndToStart), -300.0);

        st.set_enable_start_to_end(false);
        st.update_anchors(300.0);
        assert!(st.position_of(SwipeToDismissBoxValue::StartToEnd).is_nan());
        assert_eq!(st.position_of(SwipeToDismissBoxValue::EndToStart), -300.0);
    }

    /// RTL mirrors the anchors: the reading direction is to the left, so `StartToEnd` moves toward a
    /// negative x.
    #[test]
    fn rtl_mirrors_the_anchors() {
        let st = SwipeToDismissBoxState::new(SwipeToDismissBoxValue::Settled);
        st.set_rtl(true);
        st.update_anchors(300.0);
        assert_eq!(st.position_of(SwipeToDismissBoxValue::StartToEnd), -300.0);
        assert_eq!(st.position_of(SwipeToDismissBoxValue::EndToStart), 300.0);
    }

    /// The 56 dp positional threshold decides a release: just under it snaps back, just over it dismisses.
    ///
    /// The velocity is explicit and SMALL — a gesture's velocity comes from pointer timing, which a unit
    /// test has none of, and the two extremes each skip the positional branch: zero velocity settles to the
    /// nearest anchor, and anything past the 125 dp/s fling threshold dismisses by speed alone. The sign
    /// matters too, since the threshold is measured from the anchor the motion started at.
    #[test]
    fn the_positional_threshold_decides_the_settle() {
        let st = SwipeToDismissBoxState::new(SwipeToDismissBoxValue::Settled);
        st.update_anchors(300.0);

        st.drag_delta(-40.0);
        assert_eq!(
            st.settle_with_velocity(-50.0),
            SwipeToDismissBoxValue::Settled,
            "under the threshold: back"
        );

        st.snap_to(SwipeToDismissBoxValue::Settled);
        st.drag_delta(-80.0);
        assert_eq!(
            st.settle_with_velocity(-50.0),
            SwipeToDismissBoxValue::EndToStart,
            "over the threshold: dismissed"
        );
    }

    /// A fling dismisses below the distance threshold, and `dismiss_direction` reports which way it went.
    #[test]
    fn a_fling_dismisses_and_the_direction_is_reported() {
        let st = SwipeToDismissBoxState::new(SwipeToDismissBoxValue::Settled);
        st.update_anchors(300.0);

        // 10 px is far under the 56 dp threshold, but 400 dp/s is over the 125 dp/s fling threshold.
        st.drag_delta(10.0);
        assert_eq!(st.dismiss_direction(), SwipeToDismissBoxValue::StartToEnd);
        assert_eq!(st.settle_with_velocity(400.0), SwipeToDismissBoxValue::StartToEnd);

        st.snap_to(SwipeToDismissBoxValue::Settled);
        st.drag_delta(-10.0);
        assert_eq!(st.settle_with_velocity(-400.0), SwipeToDismissBoxValue::EndToStart);

        st.snap_to(SwipeToDismissBoxValue::Settled);
        assert_eq!(st.dismiss_direction(), SwipeToDismissBoxValue::Settled);
    }

    /// A disabled direction has no anchor, so dragging that way cannot settle there.
    #[test]
    fn a_disabled_direction_cannot_be_reached() {
        let st = SwipeToDismissBoxState::new(SwipeToDismissBoxValue::Settled);
        st.set_enable_start_to_end(false);
        st.update_anchors(300.0);

        st.drag_delta(200.0);
        assert_ne!(
            st.settle_with_velocity(50.0),
            SwipeToDismissBoxValue::StartToEnd,
            "the start→end anchor does not exist"
        );

        st.snap_to(SwipeToDismissBoxValue::Settled);
        st.drag_delta(-80.0);
        assert_eq!(st.settle_with_velocity(-50.0), SwipeToDismissBoxValue::EndToStart);
    }
}
