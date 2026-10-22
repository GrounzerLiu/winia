//! RangeSlider — a slider with two thumbs that pick a range (material3 `RangeSlider`).
//!
//! Same tokens and track geometry as [`crate::components::slider::Slider`]: both components draw through
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
//! - Each thumb is its own focusable node with its own key handling, so Tab moves between the thumbs
//!   and the arrow keys move the FOCUSED one — Compose's model (`rangeSliderPressDragModifier` on the
//!   container, one `focusable` per thumb). A press does NOT move focus: a click must not take the
//!   keyboard from wherever it was (the rule `clicking_an_overlay_button_does_not_steal_focus` states,
//!   and what the plain [`crate::components::slider::Slider`] does too), so the keyboard reaches a thumb
//!   through Tab alone.

use crate::composable;
use crate::runtime::composer::{ComposeCtx, GroupStatus};
use crate::layout::BoxLayout;
use crate::layout::constraints::Constraints;
use crate::layout::node::{LayoutNode, MeasurePolicy, Placement, measure_node};
use crate::unit::{Offset, Size};
use crate::modifier::{Modifier};
use crate::input::{KbEvent};
use crate::input::interaction::MutableInteractionSource;
use crate::components::slider::{
    SLIDER_THUMB_HEIGHT, SLIDER_THUMB_WIDTH,
    SLIDER_TOUCH_HEIGHT, SLIDER_TRACK_HEIGHT, SliderColors, SliderDefaults, draw_thumb,
    draw_track_body, fraction_from_value, handle_key, snap_value, value_at_x,
};
use crate::theme::WiniaTheme;
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
        let m = Modifier::new().fill_max_width().then(self.modifier);

        let mut gestures = Modifier::new();
        if enabled {
            // Press: resolve the nearer thumb, remember it for the gesture, jump it to the pressed
            // position (the same immediate jump the single slider does) and emit the press on the
            // thumb's source. Focus is NOT touched: a click must not take the keyboard from wherever
            // it was (the rule `clicking_an_overlay_button_does_not_steal_focus` states, and what the
            // plain `Slider` does too) — the keyboard reaches a thumb through Tab.
            let src_s = start_source.clone();
            let src_e = end_source.clone();
            let v_press = set_value.clone();
            let a_press = active.clone();
            gestures = gestures.on_press(move |pos| {
                let thumb = nearest_thumb(pos.0, tw_press.get(), value, min, max);
                a_press.set(thumb);
                match thumb {
                    RangeThumb::Start => src_s.emit_press_at(pos),
                    RangeThumb::End => src_e.emit_press_at(pos),
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
            // `draggable` + `offsetToValue`), clamped by the other thumb. Focus is left alone here
            // too (see the press handler).
            let src_s = start_source.clone();
            let src_e = end_source.clone();
            let v_ds = set_value.clone();
            let a_ds = active.clone();
            gestures = gestures.on_drag_start(move |pos| {
                let thumb = nearest_thumb(pos.0, tw_drag_start.get(), value, min, max);
                a_ds.set(thumb);
                match thumb {
                    RangeThumb::Start => src_s.emit_drag_start(),
                    RangeThumb::End => src_e.emit_drag_start(),
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
    pub(crate) track_width: crate::runtime::state::Backchannel<f32>,
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
                position: Offset::new(x, cy - SLIDER_THUMB_HEIGHT / 2.0),
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
mod tests;
