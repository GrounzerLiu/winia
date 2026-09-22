//! RangeSlider — a slider with two thumbs that pick a range (material3 `RangeSlider`).
//!
//! Same tokens and track geometry as [`crate::ui::slider::Slider`]: both components draw through
//! `slider::draw_track`, so this is the two-thumbed version of that geometry rather than a second
//! reading of the M3 spec.
//! - The segment between the thumbs is the active track (Primary), the segments outside them are
//!   inactive (SecondaryContainer), and a segment that reaches a track end takes that end's
//!   full-round 8 dp corner.
//! - Both thumbs are 4 × 44 capsules (Primary) that halve in width while their own gesture runs,
//!   each with its own [`MutableInteractionSource`] (`interaction_sources`).
//! - With `steps`, the `steps + 2` ticks sit on the same inset axis as the single slider's and take
//!   the active color between the thumbs.
//!
//! Gesture rules (conservative, each one documented against Compose):
//! - A press resolves the NEARER thumb, and that thumb owns the whole gesture (Compose's
//!   `pressToActiveThumb`), so a drag never swaps thumbs halfway; a tie resolves to the end thumb,
//!   matching Compose's strict comparison.
//! - The thumbs cannot cross: the start thumb stops at the end thumb's value and the end thumb at
//!   the start thumb's (Compose 1.3's `coerceAtMost` / `coerceAtLeast` rule).
//! - The track and the two thumbs are three nodes: the root carries the pointer gestures, and each
//!   thumb is its own focusable node with its own key handling. So Tab moves between the thumbs and
//!   the arrow keys move the FOCUSED one — Compose's model (`rangeSliderPressDragModifier` on the
//!   container, one `focusable` per thumb). A press moves focus to the thumb it resolved, which
//!   Compose leaves to the platform.

use crate::composable;
use crate::core::composer::{ComposeCtx, GroupStatus};
use crate::layout::BoxLayout;
use crate::layout::constraints::Constraints;
use crate::layout::node::{LayoutNode, MeasurePolicy, Placement, Point, Size, measure_node};
use crate::modifier::{FocusRequester, KbEvent, Modifier, Shape};
use crate::ui::interaction::MutableInteractionSource;
use crate::ui::slider::{
    SLIDER_ACTIVE_THUMB_WIDTH, SLIDER_THUMB_GAP, SLIDER_THUMB_HEIGHT, SLIDER_THUMB_WIDTH,
    SLIDER_TOUCH_HEIGHT, SLIDER_TRACK_HEIGHT, SliderColors, SliderDefaults, draw_thumb,
    draw_track_body, fraction_from_value, handle_key, snap_value, value_at_x,
};
use crate::ui::theme::WiniaTheme;
use std::sync::Arc;

/// A range value — the winia stand-in for Compose's `ClosedFloatingPointRange<Float>`.
///
/// `start <= end` is normalised when the component builds, not by the type: `RangeSlider::new((0.8,
/// 0.2))` renders as `(0.2, 0.8)` rather than panicking or drawing an inverted track.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RangeValue {
    pub start: f32,
    pub end: f32,
}

impl RangeValue {
    pub fn new(start: f32, end: f32) -> Self {
        Self { start, end }
    }

    /// Distance between the thumbs (0.0 for a collapsed range).
    pub fn span(&self) -> f32 {
        self.end - self.start
    }
}

impl From<(f32, f32)> for RangeValue {
    fn from(v: (f32, f32)) -> Self {
        Self { start: v.0, end: v.1 }
    }
}

/// Which of the two thumbs a gesture owns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeThumb {
    Start,
    End,
}

/// Pixel x of `value`'s thumb centre inside a component of `width`.
///
/// The axis `draw_track` draws on (`corner + (w - 2 × corner) × fraction`), so hit-testing and
/// drawing agree to the pixel — an end value lands on its stop indicator.
pub fn thumb_center_x(value: f32, width: f32, min: f32, max: f32) -> f32 {
    let corner = SLIDER_TRACK_HEIGHT / 2.0;
    corner + (width - 2.0 * corner) * fraction_from_value(value, min, max)
}

/// The thumb a press at `local_x` owns: the nearer of the two.
///
/// A port of Compose's `RangeSliderLogic.compareOffsets` plus the tie-break the press gesture
/// applies to its result (`if (compare != 0) compare < 0 else state.rawOffsetStart > posX`): the
/// nearer thumb wins, and on an exact tie the START thumb wins only if it sits to the RIGHT of the
/// press — i.e. when the press is to its left. For the two degenerate cases that means the start
/// thumb of a collapsed range pressed from its left, and otherwise the end thumb.
pub fn nearest_thumb(local_x: f32, width: f32, value: RangeValue, min: f32, max: f32) -> RangeThumb {
    let start_x = thumb_center_x(value.start, width, min, max);
    let end_x = thumb_center_x(value.end, width, min, max);
    let d_start = (local_x - start_x).abs();
    let d_end = (local_x - end_x).abs();
    if d_start < d_end {
        RangeThumb::Start
    } else if d_end < d_start {
        RangeThumb::End
    } else if start_x > local_x {
        RangeThumb::Start
    } else {
        RangeThumb::End
    }
}

/// Move `thumb` to `new_value`: clamped into `[min, max]`, and clamped against the OTHER thumb so
/// the two cannot cross.
pub fn range_with_moved_thumb(
    value: RangeValue,
    thumb: RangeThumb,
    new_value: f32,
    min: f32,
    max: f32,
) -> RangeValue {
    let v = new_value.clamp(min, max);
    match thumb {
        RangeThumb::Start => RangeValue { start: v.min(value.end), end: value.end },
        RangeThumb::End => RangeValue { start: value.start, end: v.max(value.start) },
    }
}

/// RangeSlider builder (material3 `RangeSlider(value, onValueChange, ...)`) — a controlled
/// component: the caller owns the value and `on_value_change` updates it.
pub struct RangeSlider {
    value: RangeValue,
    on_value_change: Option<Arc<dyn Fn(RangeValue) + Send + Sync>>,
    on_value_change_finished: Option<Arc<dyn Fn() + Send + Sync>>,
    value_range: (f32, f32),
    steps: i32,
    enabled: bool,
    colors: Option<SliderColors>,
    start_source: Option<MutableInteractionSource>,
    end_source: Option<MutableInteractionSource>,
    modifier: Modifier,
}

impl RangeSlider {
    /// A range slider for `value` (a [`RangeValue`] or a `(start, end)` tuple).
    pub fn new(value: impl Into<RangeValue>) -> Self {
        Self {
            value: value.into(),
            on_value_change: None,
            on_value_change_finished: None,
            value_range: (0.0, 1.0),
            steps: 0,
            enabled: true,
            colors: None,
            start_source: None,
            end_source: None,
            modifier: Modifier::new(),
        }
    }

    /// Called with the new range on every step of a drag, a tap or a keyboard step.
    pub fn on_value_change(mut self, f: impl Fn(RangeValue) + Send + Sync + 'static) -> Self {
        self.on_value_change = Some(Arc::new(f));
        self
    }

    /// Called when the gesture that changed the value ends (drag end, tap, keyboard KeyUp).
    pub fn on_value_change_finished(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_value_change_finished = Some(Arc::new(f));
        self
    }

    /// Value range (Compose's `valueRange`; winia has no range type, so a tuple).
    pub fn value_range(mut self, min: f32, max: f32) -> Self {
        self.value_range = (min, max);
        self
    }

    /// Discrete steps (> 0 snaps to `steps` evenly spaced values between the ends; 0 = continuous).
    pub fn steps(mut self, steps: i32) -> Self {
        self.steps = steps.max(0);
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn colors(mut self, colors: SliderColors) -> Self {
        self.colors = Some(colors);
        self
    }

    /// Per-thumb interaction sources (Compose's `startInteractionSource` / `endInteractionSource`).
    ///
    /// Each thumb is its own focusable node, so the pair is what carries a thumb's pressed / dragged /
    /// focused state. Passing the SAME source for both thumbs is allowed and means both thumbs share
    /// that state: both halve in width and both rings light while it reports focus.
    pub fn interaction_sources(
        mut self,
        start: MutableInteractionSource,
        end: MutableInteractionSource,
    ) -> Self {
        self.start_source = Some(start);
        self.end_source = Some(end);
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        ctx.changed(&self.value);
        ctx.changed(&self.enabled);
        ctx.changed(&self.steps);
        ctx.changed(&self.value_range);
        ctx.changed(&self.colors);
        let key = ctx.next_key();
        let theme = WiniaTheme::colors();
        let colors = self.colors.unwrap_or_else(|| SliderDefaults::slider_colors(&theme));
        let start_source = self
            .start_source
            .unwrap_or_else(|| ctx.remember(|| MutableInteractionSource::new()).get());
        let end_source = self
            .end_source
            .unwrap_or_else(|| ctx.remember(|| MutableInteractionSource::new()).get());
        let (min, max) = self.value_range;
        let (min, max) = if max > min { (min, max) } else { (min, min + 1.0) };
        let steps = self.steps;
        // As in `Slider`: with `steps` the component only ever SHOWS on-tick values (Compose snaps in
        // `RangeSliderState`'s setters), so a caller-supplied range is snapped as well — not just a
        // dragged one. Snapping the two ends independently can invert the pair, so normalise once
        // more afterwards; without `steps` this is the plain clamp.
        let lo = snap_value(self.value.start.min(self.value.end).clamp(min, max), steps, min, max);
        let hi = snap_value(self.value.start.max(self.value.end).clamp(min, max), steps, min, max);
        let value = RangeValue { start: lo.min(hi), end: lo.max(hi) };
        let enabled = self.enabled;

        // Per-thumb interaction state: each thumb halves in width while its own gesture runs.
        let st_start = start_source.state(enabled);
        let st_end = end_source.state(enabled);
        let start_active = st_start.pressed || st_start.focused || st_start.dragged;
        let end_active = st_end.pressed || st_end.focused || st_end.dragged;

        // The thumb the current (or last) gesture resolved to: set on press, then read by the drag
        // (the keyboard follows FOCUS instead — see the module note).
        let active = ctx.remember(|| RangeThumb::Start);
        // One focus requester per thumb, so a press can hand focus to the thumb it resolved.
        let start_fr = ctx.remember(|| FocusRequester::new()).get();
        let end_fr = ctx.remember(|| FocusRequester::new()).get();
        // Width write-back from the draw node (Backchannel — no recomposition); the gesture
        // callbacks read it for pixel↔value conversion.
        let track_width = ctx.remember_backchannel(|| 0.0f32);
        let tw_press = track_width.clone();
        let tw_tap = track_width.clone();
        let tw_drag_start = track_width.clone();
        let tw_drag = track_width.clone();

        let set_value = self.on_value_change.clone();
        let finished = self.on_value_change_finished.clone();

        // The gestures and the track's body share ONE node — a child of the component's own root, so
        // that node's box is the content box (a caller's `padding` insets it) and the thumbs, placed by
        // the policy inside it, share that very space. The gesture callback's coordinates are local to
        // the node the gesture resolved (`fire_gesture_action` subtracts that node's position), so
        // putting the gestures on the body's node makes the pointer axis and the drawn axis the same
        // one. Two earlier arrangements got this wrong and shifted the pointer axis by the padding
        // (measured: pressing a drawn thumb jumped it by 6.3% of the range with `padding(16)`): the
        // body on the component root with the thumbs as children (children are inset by the padding,
        // the root's own draw box is not), and once the body's node was the root with the gestures on
        // an inner node. The padding band is inert here, as Compose's `modifier.padding(16)` leaves a
        // Slider's own pointer input untouched.
        let mut m = Modifier::new().fill_max_width().then(self.modifier);

        let mut gestures = Modifier::new();
        if enabled {
            // Press: resolve the nearer thumb, remember it for the gesture, hand it focus (so the
            // arrow keys that follow move THIS thumb), jump it to the pressed position (the same
            // immediate jump the single slider does) and emit the press on the thumb's source.
            let src_s = start_source.clone();
            let src_e = end_source.clone();
            let fr_s = start_fr.clone();
            let fr_e = end_fr.clone();
            let v_press = set_value.clone();
            let a_press = active.clone();
            gestures = gestures.on_press(move |pos| {
                let thumb = nearest_thumb(pos.0, tw_press.get(), value, min, max);
                a_press.set(thumb);
                match thumb {
                    RangeThumb::Start => {
                        src_s.emit_press_at(pos);
                        fr_s.request_focus();
                    }
                    RangeThumb::End => {
                        src_e.emit_press_at(pos);
                        fr_e.request_focus();
                    }
                }
                if let Some(cb) = &v_press {
                    let nv = value_at_x(pos.0, tw_press.get(), min, max, steps);
                    cb(range_with_moved_thumb(value, thumb, nv, min, max));
                }
            });

            // Tap: same resolution, plus release and the finished callback.
            let src_s = start_source.clone();
            let src_e = end_source.clone();
            let v_tap = set_value.clone();
            let f_tap = finished.clone();
            let a_tap = active.clone();
            gestures = gestures.on_tap(move |pos| {
                let thumb = nearest_thumb(pos.0, tw_tap.get(), value, min, max);
                a_tap.set(thumb);
                match thumb {
                    RangeThumb::Start => src_s.emit_release(),
                    RangeThumb::End => src_e.emit_release(),
                }
                if let Some(cb) = &v_tap {
                    let nv = value_at_x(pos.0, tw_tap.get(), min, max, steps);
                    cb(range_with_moved_thumb(value, thumb, nv, min, max));
                }
                if let Some(cb) = &f_tap { cb(); }
            });

            // Drag: the thumb resolved at press follows the pointer in ABSOLUTE position (Compose
            // `draggable` + `offsetToValue`), clamped by the other thumb.
            let src_s = start_source.clone();
            let src_e = end_source.clone();
            let fr_s = start_fr.clone();
            let fr_e = end_fr.clone();
            let v_ds = set_value.clone();
            let a_ds = active.clone();
            gestures = gestures.on_drag_start(move |pos| {
                let thumb = nearest_thumb(pos.0, tw_drag_start.get(), value, min, max);
                a_ds.set(thumb);
                match thumb {
                    RangeThumb::Start => {
                        src_s.emit_drag_start();
                        fr_s.request_focus();
                    }
                    RangeThumb::End => {
                        src_e.emit_drag_start();
                        fr_e.request_focus();
                    }
                }
                if let Some(cb) = &v_ds {
                    let nv = value_at_x(pos.0, tw_drag_start.get(), min, max, steps);
                    cb(range_with_moved_thumb(value, thumb, nv, min, max));
                }
            });
            let v_dm = set_value.clone();
            let a_dm = active.clone();
            gestures = gestures.on_drag(move |pos, _delta| {
                if let Some(cb) = &v_dm {
                    let nv = value_at_x(pos.0, tw_drag.get(), min, max, steps);
                    cb(range_with_moved_thumb(value, a_dm.get(), nv, min, max));
                }
            });
            let src_s = start_source.clone();
            let src_e = end_source.clone();
            let f_de = finished.clone();
            let a_de = active.clone();
            gestures = gestures.on_drag_end(move || {
                match a_de.get() {
                    RangeThumb::Start => src_s.emit_drag_end(),
                    RangeThumb::End => src_e.emit_drag_end(),
                }
                src_s.emit_release();
                src_e.emit_release();
                if let Some(cb) = &f_de { cb(); }
            });
            let src_s = start_source.clone();
            let src_e = end_source.clone();
            let a_dc = active.clone();
            gestures = gestures.on_drag_cancel(move || {
                match a_dc.get() {
                    RangeThumb::Start => src_s.emit_drag_end(),
                    RangeThumb::End => src_e.emit_drag_end(),
                }
                src_s.emit_release();
                src_e.emit_release();
            });
        }
        // The root only fills the line and carries the caller's modifier; the track is its single
        // child (see the note on `m` above).
        match ctx.start_restartable_group(key, m, BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                let track_key = ctx.next_key();
                let track_mod = Modifier::new()
                    .fill_max_width()
                    .height(SLIDER_TOUCH_HEIGHT)
                    .draw_node(RangeSliderTrackNode {
                        track_width: track_width.clone(),
                        colors,
                        enabled,
                        value,
                        min,
                        max,
                        steps,
                    })
                    .then(gestures);
                match ctx.start_restartable_group(track_key, track_mod, RangeSliderLayoutPolicy { value, min, max }) {
                    GroupStatus::Skip => {}
                    GroupStatus::Enter => {
                        // The two thumbs, children of the track: each is its own focusable node with
                        // its own key handling, so Tab moves between them and the arrow keys move the
                        // FOCUSED thumb; each draws its own capsule and focus ring above the body.
                        for thumb in [RangeThumb::Start, RangeThumb::End] {
                            let is_start = thumb == RangeThumb::Start;
                            let source = if is_start { start_source.clone() } else { end_source.clone() };
                            let requester = if is_start { start_fr.clone() } else { end_fr.clone() };
                            let thumb_active = if is_start { start_active } else { end_active };
                            let mut tm = Modifier::new()
                                .size(SLIDER_THUMB_WIDTH, SLIDER_THUMB_HEIGHT)
                                // The ring is self-drawn around the capsule: the framework's ring
                                // follows the node's own background/border shape, which a bare
                                // capsule node has none of.
                                .no_focus_ring()
                                .draw_node(RangeThumbNode {
                                    interaction: source.clone(),
                                    colors,
                                    enabled,
                                    active: thumb_active,
                                });
                            if enabled {
                                let v_key = set_value.clone();
                                let f_key = finished.clone();
                                tm = tm
                                    .focusable_with_source(&source)
                                    .focus_requester(&requester)
                                    .on_key_event(move |ke: &KbEvent| {
                                        let current = if is_start { value.start } else { value.end };
                                        let cb = v_key.as_ref().map(|cb| {
                                            let cb = cb.clone();
                                            Arc::new(move |nv: f32| {
                                                cb(range_with_moved_thumb(value, thumb, nv, min, max))
                                            }) as Arc<dyn Fn(f32) + Send + Sync>
                                        });
                                        handle_key(ke, current, min, max, steps, &cb, &f_key)
                                    });
                            }
                            let thumb_key = ctx.next_key();
                            ctx.start_leaf(thumb_key, tm);
                            ctx.end_node();
                        }
                    }
                }
                ctx.end_restartable_group();
            }
        }
        ctx.end_restartable_group();
    }
}

/// The range track's drawing node: the segments, ticks and stop indicators — no thumb, because the
/// two thumbs are nodes of their own (`RangeThumbNode`), each focusable.
///
/// A named type (visible in the debug tree, precise `Skip` through `node_key`) that owns the width
/// write-back channel, so the gesture callbacks' pixel↔value conversion needs no recomposition.
#[derive(Debug)]
pub(crate) struct RangeSliderTrackNode {
    /// Track width write-back (read by the tap/drag pixel↔value conversion).
    pub(crate) track_width: crate::core::state::Backchannel<f32>,
    pub(crate) colors: SliderColors,
    pub(crate) enabled: bool,
    pub(crate) value: RangeValue,
    pub(crate) min: f32,
    pub(crate) max: f32,
    pub(crate) steps: i32,
}

impl crate::modifier::DrawNode for RangeSliderTrackNode {
    fn draw(&self, canvas: &skia_safe::Canvas, rect: skia_safe::Rect) {
        self.track_width.set(rect.width());
        // `single_sided = false`: both outer segments are inactive track, and `thumb_values` keeps
        // the body from drawing a tick or stop a thumb will cover (the thumbs are drawn above this
        // node, in their own nodes).
        draw_track_body(
            canvas,
            rect,
            &self.colors,
            self.enabled,
            self.value.start,
            self.value.end,
            self.min,
            self.max,
            self.steps,
            false,
            &[self.value.start, self.value.end],
        );
    }

    fn node_key(&self) -> String {
        // Static visual params only: both values, the range, the step count and the colors.
        // `track_width` is a write-back channel and stays out, exactly as in `SliderTrackNode`.
        format!(
            "rangeslidertrack:{:?}:{}:{}:{}:{}:{}:{}",
            self.colors,
            self.enabled,
            self.value.start.to_bits(),
            self.value.end.to_bits(),
            self.min.to_bits(),
            self.max.to_bits(),
            self.steps,
        )
    }
}

/// One thumb: its capsule and, while this thumb holds focus, its ring.
///
/// The node's rect is the RESTING capsule box (4 × 44) whatever the gesture state, so the ring's
/// geometry stays put while a drag narrows the capsule inside it — the same rule the single slider's
/// thumb follows.
#[derive(Debug)]
pub(crate) struct RangeThumbNode {
    /// This thumb's own interaction source: its focused/dragged state drives the ring and the
    /// halved capsule width.
    pub(crate) interaction: MutableInteractionSource,
    pub(crate) colors: SliderColors,
    pub(crate) enabled: bool,
    /// The thumb's own gesture is running (press / drag / focus), which halves its width.
    pub(crate) active: bool,
}

impl crate::modifier::DrawNode for RangeThumbNode {
    fn draw(&self, canvas: &skia_safe::Canvas, rect: skia_safe::Rect) {
        // Render-time peek, as in `SliderTrackNode`: focus reaches the ring through the redraw
        // channel (a focus change also rebuilds through `interaction.state`), so reading a dependency
        // here would be too late to register anyway.
        let focused = self.interaction.is_focused_value();
        let focus_alpha = self.interaction.focus_indicator_alpha_value();
        draw_thumb(
            canvas,
            rect.left + rect.width() / 2.0,
            rect.top + rect.height() / 2.0,
            &self.colors,
            self.enabled,
            self.active,
            focused,
            focus_alpha,
        );
    }

    fn node_key(&self) -> String {
        // Static visual params only; the focus state arrives through the source (and the redraw
        // channel), so it is not part of the key — as in `SliderTrackNode`.
        format!(
            "rangethumb:{:?}:{}:{}:{}",
            self.colors,
            self.enabled,
            self.active,
            self.interaction.source_id(),
        )
    }
}

/// Places the two thumbs on their value positions inside the node that also draws the track body.
///
/// The thumb x comes from [`thumb_center_x`] — the same axis the track draws and `nearest_thumb`
/// hit-tests on, in the SAME coordinate space (the gesture callback's `pos` is local to this node) —
/// so drawing, placement and pointer resolution cannot drift apart, a caller's padding included.
#[derive(Debug)]
struct RangeSliderLayoutPolicy {
    value: RangeValue,
    min: f32,
    max: f32,
}

impl MeasurePolicy for RangeSliderLayoutPolicy {
    fn measure(
        &self,
        nodes: &mut Vec<LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        let row_w = constraints.max_width;
        let height = SLIDER_TOUCH_HEIGHT.min(constraints.max_height);
        let cy = height / 2.0;
        let mut placements = Vec::with_capacity(children.len());
        for (i, v) in [self.value.start, self.value.end].iter().enumerate() {
            let (thumb_size, _) = measure_node(
                nodes,
                policies,
                children[i],
                Constraints::new(SLIDER_THUMB_WIDTH, SLIDER_THUMB_WIDTH, SLIDER_THUMB_HEIGHT, SLIDER_THUMB_HEIGHT),
            );
            let x = thumb_center_x(*v, row_w, self.min, self.max) - SLIDER_THUMB_WIDTH / 2.0;
            placements.push(Placement {
                size: thumb_size,
                position: Point::new(x, cy - SLIDER_THUMB_HEIGHT / 2.0),
            });
        }
        (Size::new(row_w, height), placements)
    }

    fn place(&self, nodes: &mut Vec<LayoutNode>, children: &[usize], placements: &[Placement]) {
        for (index, &child) in children.iter().enumerate() {
            if let Some(p) = placements.get(index) {
                nodes[child].position = p.position;
                nodes[child].measured_size = p.size;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::composer::Composer;
    use crate::modifier::{DrawNode, ModifierElement};
    use crate::ui::theme::ThemeColors;
    use std::sync::atomic::{AtomicI32, Ordering};

    // ── Pure logic ──

    #[test]
    fn nearest_thumb_picks_the_nearer_thumb() {
        let v = RangeValue::new(0.25, 0.75);
        // 300-wide track: thumb centres 79 and 221 (the inset axis 8 + 284 × f)
        assert_eq!(nearest_thumb(60.0, 300.0, v, 0.0, 1.0), RangeThumb::Start);
        assert_eq!(nearest_thumb(240.0, 300.0, v, 0.0, 1.0), RangeThumb::End);
        // Exactly between the two (150) is a tie, and the tie goes to the start thumb only when it
        // sits right of the press — it does not, so the end thumb takes it.
        assert_eq!(nearest_thumb(150.0, 300.0, v, 0.0, 1.0), RangeThumb::End);
        // A collapsed range ties everywhere: pressed from its left the start thumb takes it,
        // pressed from its right the end thumb does.
        let collapsed = RangeValue::new(0.5, 0.5);
        let c = thumb_center_x(0.5, 300.0, 0.0, 1.0);
        assert_eq!(nearest_thumb(c - 0.5, 300.0, collapsed, 0.0, 1.0), RangeThumb::Start);
        assert_eq!(nearest_thumb(c + 0.5, 300.0, collapsed, 0.0, 1.0), RangeThumb::End);
    }

    #[test]
    fn a_thumb_cannot_cross_the_other() {
        let v = RangeValue::new(0.2, 0.5);
        assert_eq!(
            range_with_moved_thumb(v, RangeThumb::Start, 0.9, 0.0, 1.0),
            RangeValue::new(0.5, 0.5),
            "the start thumb stops at the end thumb"
        );
        assert_eq!(
            range_with_moved_thumb(v, RangeThumb::End, 0.1, 0.0, 1.0),
            RangeValue::new(0.2, 0.2),
            "the end thumb stops at the start thumb"
        );
        assert_eq!(
            range_with_moved_thumb(v, RangeThumb::Start, -3.0, 0.0, 1.0),
            RangeValue::new(0.0, 0.5),
            "below the range clamps to min"
        );
        assert_eq!(
            range_with_moved_thumb(v, RangeThumb::End, 7.0, 0.0, 1.0),
            RangeValue::new(0.2, 1.0),
            "above the range clamps to max"
        );
    }

    #[test]
    fn thumb_center_matches_the_drawing_axis() {
        // Both ends land on the track's stop indicators, the middle is symmetric.
        assert_eq!(thumb_center_x(0.0, 300.0, 0.0, 1.0), 8.0);
        assert_eq!(thumb_center_x(1.0, 300.0, 0.0, 1.0), 292.0);
        assert_eq!(thumb_center_x(0.5, 300.0, 0.0, 1.0), 150.0);
        // A non-unit range maps the same way; values outside clamp.
        assert_eq!(thumb_center_x(50.0, 200.0, 0.0, 100.0), 100.0);
        assert_eq!(thumb_center_x(-10.0, 200.0, 0.0, 100.0), 8.0);
    }

    // ── Pixel tests ──

    fn render_range_px(build: impl FnOnce(&mut ComposeCtx)) -> (Vec<[u8; 4]>, usize) {
        use skia_safe::{Color as SkColor, surfaces};
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = Composer::new();
        let scene = |ctx: &mut ComposeCtx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| build(ctx));
        };
        composer.compose(scene);
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
        let mut surface = surfaces::raster_n32_premul((300, 300)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(SkColor::WHITE);
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        crate::render::render(nodes, root, canvas);
        let pm = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        (px.to_vec(), 300)
    }

    fn at(px: &[[u8; 4]], w: usize, x: f32, y: f32) -> (i32, i32, i32) {
        let p = px[(y as usize) * w + (x as usize)];
        (p[2] as i32, p[1] as i32, p[0] as i32) // BGRA → RGB
    }

    fn close(a: (i32, i32, i32), b: (i32, i32, i32)) -> bool {
        (a.0 - b.0).abs() <= 8 && (a.1 - b.1).abs() <= 8 && (a.2 - b.2).abs() <= 8
    }

    fn rgb(c: crate::modifier::Color) -> (i32, i32, i32) {
        (c.r as i32, c.g as i32, c.b as i32)
    }

    #[test]
    fn range_slider_renders_two_thumbs_and_the_active_middle() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let (px, w) = render_range_px(|ctx| {
            RangeSlider::new((0.25, 0.75))
                .value_range(0.0, 1.0)
                .on_value_change(|_| {})
                .build(ctx);
        });
        let prim = rgb(theme.primary);
        let sec = rgb(theme.secondary_container);
        let white = (255, 255, 255);
        // Track centre line y = 24; thumb centres 8 + 284 × 0.25 = 79 and 221.
        assert!(close(at(&px, w, 40.0, 24.0), sec), "left of the range is inactive track (got {:?})", at(&px, w, 40.0, 24.0));
        assert!(close(at(&px, w, 150.0, 24.0), prim), "between the thumbs is active track (got {:?})", at(&px, w, 150.0, 24.0));
        assert!(close(at(&px, w, 280.0, 24.0), sec), "right of the range is inactive track (got {:?})", at(&px, w, 280.0, 24.0));
        // Two thumbs, one per value.
        assert!(close(at(&px, w, 79.0, 24.0), prim), "start thumb is primary (got {:?})", at(&px, w, 79.0, 24.0));
        assert!(close(at(&px, w, 221.0, 24.0), prim), "end thumb is primary (got {:?})", at(&px, w, 221.0, 24.0));
        // The gaps between a thumb and the track stay clear (gap = thumb half + 6 dp = 8).
        assert!(close(at(&px, w, 72.0, 24.0), white), "gap left of the start thumb is clear (got {:?})", at(&px, w, 72.0, 24.0));
        assert!(close(at(&px, w, 228.0, 24.0), white), "gap right of the end thumb is clear (got {:?})", at(&px, w, 228.0, 24.0));
    }

    /// Thumbs at the ends of the range behave exactly like a single slider's thumb: the fill stops
    /// one gap short of the thumb, so the thumb reads as the end of the track and the last pixels
    /// stay clear. (An earlier version filled to the track's end for a range at an end, which stuck
    /// out past the thumb — reported as wrong against the single slider's look.)
    #[test]
    fn a_range_at_an_end_leaves_the_end_clear() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let prim = rgb(theme.primary);
        let white = (255, 255, 255);
        let (px, w) = render_range_px(|ctx| {
            RangeSlider::new((0.0, 1.0)).value_range(0.0, 1.0).on_value_change(|_| {}).build(ctx);
        });
        // Thumbs at 8 and 292, the fill between 16 and 284: clear at both ends, no stop indicator
        // behind a thumb that sits on the end.
        assert!(close(at(&px, w, 3.0, 24.0), white), "nothing is drawn at the very left (got {:?})", at(&px, w, 3.0, 24.0));
        assert!(close(at(&px, w, 8.0, 24.0), prim), "the start thumb sits corner px in (got {:?})", at(&px, w, 8.0, 24.0));
        assert!(close(at(&px, w, 20.0, 24.0), prim), "the fill starts one gap past the thumb (got {:?})", at(&px, w, 20.0, 24.0));
        assert!(close(at(&px, w, 150.0, 24.0), prim), "the whole span is active (got {:?})", at(&px, w, 150.0, 24.0));
        assert!(close(at(&px, w, 280.0, 24.0), prim), "the fill ends one gap before the end thumb (got {:?})", at(&px, w, 280.0, 24.0));
        assert!(close(at(&px, w, 292.0, 24.0), prim), "the end thumb (got {:?})", at(&px, w, 292.0, 24.0));
        assert!(close(at(&px, w, 297.0, 24.0), white), "nothing is drawn at the very right (got {:?})", at(&px, w, 297.0, 24.0));

        // The same at one end only: (0.0, 0.5) leaves the left clear and an inactive track right of
        // the end thumb.
        let sec = rgb(theme.secondary_container);
        let (px2, w2) = render_range_px(|ctx| {
            RangeSlider::new((0.0, 0.5)).value_range(0.0, 1.0).on_value_change(|_| {}).build(ctx);
        });
        assert!(close(at(&px2, w2, 3.0, 24.0), white), "clear left end (got {:?})", at(&px2, w2, 3.0, 24.0));
        assert!(close(at(&px2, w2, 20.0, 24.0), prim), "active from one gap past the thumb (got {:?})", at(&px2, w2, 20.0, 24.0));
        assert!(close(at(&px2, w2, 200.0, 24.0), sec), "inactive right of the end thumb (got {:?})", at(&px2, w2, 200.0, 24.0));
    }

    #[test]
    fn range_steps_color_ticks_inside_the_range() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let (px, w) = render_range_px(|ctx| {
            RangeSlider::new((0.0, 0.4)).value_range(0.0, 1.0).steps(4).on_value_change(|_| {}).build(ctx);
        });
        // Slider colors cross the two tick colors for contrast: on the primary active track the
        // ticks are secondary_container, on the secondary inactive track they are primary.
        let prim = rgb(theme.primary);
        let sec = rgb(theme.secondary_container);
        // Ticks at 8 + 284 × f for f = 0, .2, .4, .6, .8, 1 → 8, 64.8, 121.6, 178.4, 235.2, 292.
        // The ones on the thumbs (8, 121.6) are skipped, so the first visible tick is 64.8: inside
        // the range → the active tick color.
        assert!(close(at(&px, w, 64.8, 24.0), sec), "a tick inside the range takes the active tick color (got {:?})", at(&px, w, 64.8, 24.0));
        assert!(close(at(&px, w, 178.4, 24.0), prim), "a tick outside the range takes the inactive tick color (got {:?})", at(&px, w, 178.4, 24.0));
        assert!(close(at(&px, w, 235.2, 24.0), prim), "and so does the next one (got {:?})", at(&px, w, 235.2, 24.0));
    }

    #[test]
    fn disabled_range_uses_the_disabled_palette() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let (px, w) = render_range_px(|ctx| {
            RangeSlider::new((0.25, 0.75)).value_range(0.0, 1.0).enabled(false).build(ctx);
        });
        // The disabled colors carry an alpha, so what the surface shows is that color composited
        // over the white background — the composited value is what this pins.
        let over_white = |c: crate::modifier::Color, a: f32| {
            (
                (c.r as f32 * a + 255.0 * (1.0 - a)) as i32,
                (c.g as f32 * a + 255.0 * (1.0 - a)) as i32,
                (c.b as f32 * a + 255.0 * (1.0 - a)) as i32,
            )
        };
        let thumb_disabled = over_white(theme.on_surface, 0.38);
        let inactive_disabled = over_white(theme.on_surface, 0.12);
        assert!(close(at(&px, w, 79.0, 24.0), thumb_disabled), "a disabled thumb is onSurface @ 38% (got {:?}, expected {thumb_disabled:?})", at(&px, w, 79.0, 24.0));
        // The disabled ACTIVE track carries the same 38% alpha as the thumb; the inactive one the
        // 12% of `DisabledInactiveTrackAlpha`.
        assert!(close(at(&px, w, 280.0, 24.0), inactive_disabled), "a disabled inactive track is onSurface @ 12% (got {:?}, expected {inactive_disabled:?})", at(&px, w, 280.0, 24.0));
    }

    // ── Gesture callbacks ──

    /// Build a range slider and hand back its press / drag callbacks, laid out 300 × 300.
    type PressCb = Arc<dyn Fn((f32, f32)) + Send + Sync>;
    type DragCb = Arc<dyn Fn((f32, f32), (f32, f32)) + Send + Sync>;
    type DragEndCb = Arc<dyn Fn() + Send + Sync>;

    #[allow(clippy::type_complexity)]
    fn callbacks(
        value: RangeValue,
        steps: i32,
        got: Arc<AtomicI32>,
    ) -> (PressCb, DragCb, DragEndCb) {
        use skia_safe::surfaces;
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = Composer::new();
        let scene = |ctx: &mut ComposeCtx| {
            let g = got.clone();
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                RangeSlider::new(value)
                    .value_range(0.0, 1.0)
                    .steps(steps)
                    .on_value_change(move |v: RangeValue| {
                        g.store((v.start * 1000.0) as i32, Ordering::Relaxed);
                    })
                    .build(ctx);
            });
        };
        composer.compose(scene);
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
        // Render once so the draw node writes the track width back (300) — the gesture callbacks
        // convert pixels to values through it.
        let mut surface = surfaces::raster_n32_premul((300, 300)).unwrap();
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        crate::render::render(nodes, root, surface.canvas());

        let mut press: Option<PressCb> = None;
        let mut drag: Option<DragCb> = None;
        let mut drag_end: Option<DragEndCb> = None;
        for n in composer.arena_nodes() {
            for el in n.modifier.elements() {
                match el {
                    ModifierElement::TapOnPress { cb } => press = Some(cb.clone()),
                    ModifierElement::DragOnMove { cb } => drag = Some(cb.clone()),
                    ModifierElement::DragOnEnd { cb } => drag_end = Some(cb.clone()),
                    _ => {}
                }
            }
        }
        (
            press.expect("a range slider has a press callback"),
            drag.expect("a range slider has a drag callback"),
            drag_end.expect("a range slider has a drag-end callback"),
        )
    }

    #[test]
    fn pressing_jumps_only_the_nearer_thumb() {
        let got = Arc::new(AtomicI32::new(-1));
        let (press, drag, _end) = callbacks(RangeValue::new(0.25, 0.75), 0, got.clone());
        // x = 60 is nearer the start thumb (79) than the end thumb (221): it jumps to
        // (60 - 8) / 284 = 0.183, the end thumb stays.
        press((60.0, 24.0));
        assert_eq!(got.load(Ordering::Relaxed), 183, "the nearer (start) thumb jumped, the end one stayed");
        let _ = drag;
        let _ = crate::modifier::take_focus_requests();
    }

    #[test]
    fn a_drag_keeps_the_thumb_it_resolved_at_press() {
        let got = Arc::new(AtomicI32::new(-1));
        let (press, drag, end) = callbacks(RangeValue::new(0.25, 0.75), 0, got.clone());
        // Press on the end thumb, then drag to x = 280 → end = (280 - 8) / 284 = 0.958.
        press((240.0, 24.0));
        drag((280.0, 24.0), (40.0, 0.0));
        assert_eq!(got.load(Ordering::Relaxed), 250, "the start thumb stayed at 0.25");
        // Dragging far past the other thumb keeps the END thumb: it clamps at the start thumb's
        // value instead of jumping to the far side (which a re-resolved thumb would do).
        drag((10.0, 24.0), (-270.0, 0.0));
        assert_eq!(got.load(Ordering::Relaxed), 250, "the end thumb clamped at the start thumb (0.25)");
        end();
        // A press queues a focus request in a PROCESS-global queue (`FocusRequester` is
        // window-agnostic in a windowless test), which would otherwise leak into the modifier
        // module's own queue assertions and make them order-dependent.
        let _ = crate::modifier::take_focus_requests();
    }

    #[test]
    fn a_press_on_the_far_side_moves_the_other_thumb() {
        let got = Arc::new(AtomicI32::new(-1));
        let (press, _drag, _end) = callbacks(RangeValue::new(0.25, 0.75), 0, got.clone());
        // x = 240 is nearer the end thumb → the START thumb keeps 0.25 and the end thumb jumps.
        press((240.0, 24.0));
        assert_eq!(got.load(Ordering::Relaxed), 250, "the start thumb value is unchanged");
        let _ = crate::modifier::take_focus_requests();
    }

    /// Both ends snap to ticks, a caller-supplied range included (see `Slider`'s note): with
    /// steps = 4 over 0..1 the ticks are 0, .2, .4, .6, .8, 1, so (0.3, 0.31) collapses onto the 0.4
    /// tick — thumb centres 8 + 284 × f — and (0.3, 0.7) becomes (0.4, 0.8).
    #[test]
    fn discrete_range_snaps_both_ends() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let prim = rgb(theme.primary);
        let sec = rgb(theme.secondary_container);
        let white = (255, 255, 255);

        let (px, w) = render_range_px(|ctx| {
            RangeSlider::new((0.3, 0.31)).value_range(0.0, 1.0).steps(4).on_value_change(|_| {}).build(ctx);
        });
        assert!(close(at(&px, w, 121.6, 24.0), prim), "the collapsed range sits on the 0.4 tick (got {:?})", at(&px, w, 121.6, 24.0));
        // Left of it the track is inactive — an unsnapped (0.3, 0.31) would have put two thumbs here
        // and left 110 in the clear.
        assert!(close(at(&px, w, 110.0, 24.0), sec), "inactive left of the snapped thumb (got {:?})", at(&px, w, 110.0, 24.0));
        assert!(close(at(&px, w, 150.0, 24.0), sec), "inactive right of the snapped thumb (got {:?})", at(&px, w, 150.0, 24.0));

        let (px2, w2) = render_range_px(|ctx| {
            RangeSlider::new((0.3, 0.7)).value_range(0.0, 1.0).steps(4).on_value_change(|_| {}).build(ctx);
        });
        assert!(close(at(&px2, w2, 121.6, 24.0), prim), "start snapped to 0.4 (got {:?})", at(&px2, w2, 121.6, 24.0));
        assert!(close(at(&px2, w2, 235.2, 24.0), prim), "end snapped to 0.8 (got {:?})", at(&px2, w2, 235.2, 24.0));
        assert!(close(at(&px2, w2, 150.0, 24.0), prim), "the snapped span is the active one (got {:?})", at(&px2, w2, 150.0, 24.0));
        assert!(close(at(&px2, w2, 228.0, 24.0), white), "the gap before the snapped end thumb is clear (got {:?})", at(&px2, w2, 228.0, 24.0));
    }

    #[test]
    fn range_slider_track_node_key_covers_all_visual_params() {
        let colors = SliderDefaults::slider_colors(&ThemeColors::light_from_seed(0x6750A4));
        let node = |v: RangeValue, min: f32, max: f32, steps: i32| RangeSliderTrackNode {
            track_width: crate::core::state::Backchannel::new(0.0),
            colors,
            enabled: true,
            value: v,
            min,
            max,
            steps,
        };
        let base = node(RangeValue::new(0.25, 0.75), 0.0, 1.0, 0);
        let base_key = base.node_key();
        assert_eq!(
            base_key,
            node(RangeValue::new(0.25, 0.75), 0.0, 1.0, 0).node_key(),
            "same params → same key (Skip holds)"
        );
        assert_ne!(base_key, node(RangeValue::new(0.2, 0.75), 0.0, 1.0, 0).node_key(), "start value");
        assert_ne!(base_key, node(RangeValue::new(0.25, 0.7), 0.0, 1.0, 0).node_key(), "end value");
        assert_ne!(base_key, node(RangeValue::new(0.25, 0.75), 0.0, 2.0, 0).node_key(), "max");
        assert_ne!(base_key, node(RangeValue::new(0.25, 0.75), -1.0, 1.0, 0).node_key(), "min");
        assert_ne!(base_key, node(RangeValue::new(0.25, 0.75), 0.0, 1.0, 4).node_key(), "steps");
        let mut other = node(RangeValue::new(0.25, 0.75), 0.0, 1.0, 0);
        other.enabled = false;
        assert_ne!(base_key, other.node_key(), "enabled");
    }

    /// A thumb's key must cover what it draws (its colors, the halved width) and its own source —
    /// the ring comes from that source, so two thumbs with different sources must not collide.
    #[test]
    fn range_thumb_node_key_covers_all_visual_params() {
        let colors = SliderDefaults::slider_colors(&ThemeColors::light_from_seed(0x6750A4));
        let src = MutableInteractionSource::new();
        let node = |active: bool, enabled: bool, src: MutableInteractionSource| RangeThumbNode {
            interaction: src,
            colors,
            enabled,
            active,
        };
        let base_key = node(false, true, src.clone()).node_key();
        assert_eq!(base_key, node(false, true, src.clone()).node_key(), "same params → same key");
        assert_ne!(base_key, node(true, true, src.clone()).node_key(), "halved width while active");
        assert_ne!(base_key, node(false, false, src.clone()).node_key(), "enabled");
        assert_ne!(
            base_key,
            node(false, true, MutableInteractionSource::new()).node_key(),
            "the source drives the ring, so it must be part of the key"
        );
        let mut recolored = node(false, true, src.clone());
        recolored.colors.thumb_color = crate::modifier::Color::from_argb(255, 1, 2, 3);
        assert_ne!(base_key, recolored.node_key(), "colors");
    }

    /// The nesting the component builds: the root fills the line, its single child is the track (the
    /// node with the gestures and the body), and the track's children are the two thumbs.
    fn range_nodes(composer: &Composer) -> (usize, usize, [usize; 2]) {
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        assert_eq!(nodes[root].children.len(), 1, "the root's single child is the track");
        let track = nodes[root].children[0];
        let thumbs = nodes[track].children.clone();
        assert_eq!(thumbs.len(), 2, "the track's children are the two thumbs");
        (root, track, [thumbs[0], thumbs[1]])
    }

    /// Both thumbs sit on [`thumb_center_x`] — the axis the drawing and the hit test both use, in the
    /// track's own space (which is also the space the gesture callbacks report).
    #[test]
    fn both_thumbs_are_placed_on_the_value_axis() {
        let v = RangeValue::new(0.25, 0.75);
        let mut composer = Composer::new();
        let theme = ThemeColors::light_from_seed(0x6750A4);
        composer.compose(|ctx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                RangeSlider::new(v).value_range(0.0, 1.0).on_value_change(|_| {}).build(ctx);
            });
        });
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
        let (_root, track, thumbs) = range_nodes(&composer);
        let nodes = composer.arena_nodes();
        assert_eq!(nodes[track].measured_size.width, 300.0, "the track fills the line");
        assert_eq!(nodes[track].measured_size.height, SLIDER_TOUCH_HEIGHT);

        for (i, value) in [v.start, v.end].iter().enumerate() {
            let t = &nodes[thumbs[i]];
            assert_eq!(t.measured_size.width, SLIDER_THUMB_WIDTH, "thumb {} keeps its resting width", i);
            assert_eq!(t.measured_size.height, SLIDER_THUMB_HEIGHT);
            let expected = thumb_center_x(*value, 300.0, 0.0, 1.0) - SLIDER_THUMB_WIDTH / 2.0;
            assert!(
                (t.position.x - expected).abs() < 0.01,
                "thumb {i} at x={} but the axis says {expected}",
                t.position.x
            );
            assert!((t.position.y - (SLIDER_TOUCH_HEIGHT - SLIDER_THUMB_HEIGHT) / 2.0).abs() < 0.01);
        }
    }

    /// A caller's `padding` must NOT shift the pointer↔value axis.
    ///
    /// The track is a child of the component root, so a padding insets it while the gestures live on
    /// the track itself: the pointer axis and the drawn axis are the same box, and the padding band is
    /// inert — what Compose's `modifier.padding(16)` does to a Slider too. Two earlier arrangements
    /// got this wrong (the body on the root with the thumbs as children; then the gestures on the root
    /// while the body/thumbs were children) and this test measured the difference: pressing a drawn
    /// thumb jumped it by the padding, 0.313 and then 0.278 where the thumb sat at 0.25.
    #[test]
    fn a_caller_padding_does_not_shift_the_pointer_axis() {
        use skia_safe::surfaces;
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let got = Arc::new(AtomicI32::new(-1));
        let mut composer = Composer::new();
        let scene = |ctx: &mut ComposeCtx| {
            let g = got.clone();
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                RangeSlider::new(RangeValue::new(0.25, 0.75))
                    .value_range(0.0, 1.0)
                    .on_value_change(move |v: RangeValue| {
                        g.store((v.start * 1000.0) as i32, Ordering::Relaxed);
                    })
                    .modifier(Modifier::new().padding(16.0))
                    .build(ctx);
            });
        };
        composer.compose(scene);
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
        let mut surface = surfaces::raster_n32_premul((300, 300)).unwrap();
        let root = composer.layout_root_idx().expect("root");
        crate::render::render(composer.arena_nodes(), root, surface.canvas());

        // The drawn start thumb, in the space the gesture callback receives (the track's own box).
        let (_root, track, thumbs) = range_nodes(&composer);
        let nodes = composer.arena_nodes();
        assert_eq!(nodes[track].measured_size.width, 268.0, "the padding insets the track");
        let thumb_x = nodes[thumbs[0]].position.x + SLIDER_THUMB_WIDTH / 2.0;
        let mut press: Option<PressCb> = None;
        for n in composer.arena_nodes() {
            for el in n.modifier.elements() {
                if let ModifierElement::TapOnPress { cb } = el {
                    press = Some(cb.clone());
                }
            }
        }
        press.expect("a range slider has a press callback")((thumb_x, 24.0));
        assert_eq!(
            got.load(Ordering::Relaxed),
            250,
            "pressing the drawn thumb (x={thumb_x}) must not move it — padding shifted the axis"
        );
        let _ = crate::modifier::take_focus_requests();
    }

    /// Each thumb carries its own focusable node and its own key handling: two focus stops, and a
    /// pointer press hands focus to the thumb it resolved (Compose's model, one `focusable` per
    /// thumb). Without this the keyboard would have a single target and Tab could not switch.
    #[test]
    fn each_thumb_is_its_own_focus_target_with_its_own_keys() {
        let mut composer = Composer::new();
        let theme = ThemeColors::light_from_seed(0x6750A4);
        composer.compose(|ctx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                RangeSlider::new((0.25, 0.75)).value_range(0.0, 1.0).on_value_change(|_| {}).build(ctx);
            });
        });
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
        let (root, track, thumbs) = range_nodes(&composer);
        let nodes = composer.arena_nodes();
        let focusable_thumbs = thumbs
            .iter()
            .filter(|&&c| {
                let els = nodes[c].modifier.elements();
                els.iter().any(|el| matches!(el, ModifierElement::Focusable { .. }))
                    && els.iter().any(|el| matches!(el, ModifierElement::KbEvent { on_key: Some(_), .. }))
                    && els.iter().any(|el| matches!(el, ModifierElement::FocusRequesterId { .. }))
            })
            .count();
        assert_eq!(focusable_thumbs, 2, "both thumbs are focusable, carry keys and a focus requester");
        for (name, idx) in [("root", root), ("track", track)] {
            let focusable = nodes[idx]
                .modifier
                .elements()
                .iter()
                .any(|el| matches!(el, ModifierElement::Focusable { .. }));
            assert!(!focusable, "the {name} holds the gestures and the body, not a third focus stop");
        }
    }
}
