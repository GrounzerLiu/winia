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
//! - The component takes focus as a whole (ONE focusable node) and the ring wraps the thumb the last
//!   gesture resolved to — the same thumb the arrow keys move. Compose focuses the two thumbs
//!   separately (Tab between them); this is the one intentional difference here.

use crate::composable;
use crate::core::composer::{ComposeCtx, GroupStatus};
use crate::layout::BoxLayout;
use crate::modifier::{KbEvent, Modifier};
use crate::ui::interaction::MutableInteractionSource;
use crate::ui::slider::{
    SLIDER_TOUCH_HEIGHT, SLIDER_TRACK_HEIGHT, SliderColors, SliderDefaults, TrackThumb, draw_track,
    fraction_from_value, handle_key, snap_value, value_at_x,
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
/// press — which for the two degenerate cases means the start thumb of a collapsed range pressed
/// from its right, and otherwise the end thumb.
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
    /// The start one is also the component's focus source: the component focuses as a whole.
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

        // The thumb the current (or last) gesture resolved to: set on press, then read by the drag,
        // the focus ring and the keyboard. Kept across gestures, so the ring stays put between them.
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

        let mut m = Modifier::new()
            .fill_max_width()
            .min_height(SLIDER_TOUCH_HEIGHT)
            // The focus ring is self-drawn around a thumb capsule (as in `Slider`), not by the
            // framework around the component rect.
            .no_focus_ring()
            .draw_node(RangeSliderTrackNode {
                track_width: track_width.clone(),
                start_interaction: start_source.clone(),
                colors,
                enabled,
                value,
                min,
                max,
                steps,
                start_active,
                end_active,
                active_thumb: active.get(),
            });

        if enabled {
            // Press: resolve the nearer thumb, remember it for the gesture, jump it to the pressed
            // position (the same immediate jump the single slider does) and emit the press on the
            // source of the thumb that got it.
            let src_s = start_source.clone();
            let src_e = end_source.clone();
            let v_press = set_value.clone();
            let a_press = active.clone();
            m = m.on_press(move |pos| {
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
            m = m.on_tap(move |pos| {
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
            let v_ds = set_value.clone();
            let a_ds = active.clone();
            m = m.on_drag_start(move |pos| {
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
            m = m.on_drag(move |pos, _delta| {
                if let Some(cb) = &v_dm {
                    let nv = value_at_x(pos.0, tw_drag.get(), min, max, steps);
                    cb(range_with_moved_thumb(value, a_dm.get(), nv, min, max));
                }
            });
            let src_s = start_source.clone();
            let src_e = end_source.clone();
            let f_de = finished.clone();
            let a_de = active.clone();
            m = m.on_drag_end(move || {
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
            m = m.on_drag_cancel(move || {
                match a_dc.get() {
                    RangeThumb::Start => src_s.emit_drag_end(),
                    RangeThumb::End => src_e.emit_drag_end(),
                }
                src_s.emit_release();
                src_e.emit_release();
            });
        }

        if enabled {
            // One focusable node (the start source is the focus source; the ring follows `active`),
            // and the arrow keys move the active thumb — see the module note on this difference
            // from Compose's per-thumb focus.
            let v_key = set_value.clone();
            let f_key = finished.clone();
            let a_key = active.clone();
            m = m
                .focusable_with_source(&start_source)
                .on_key_event(move |ke: &KbEvent| {
                    let thumb = a_key.get();
                    let current = match thumb {
                        RangeThumb::Start => value.start,
                        RangeThumb::End => value.end,
                    };
                    let cb = v_key.as_ref().map(|cb| {
                        let cb = cb.clone();
                        Arc::new(move |nv: f32| {
                            cb(range_with_moved_thumb(value, thumb, nv, min, max))
                        }) as Arc<dyn Fn(f32) + Send + Sync>
                    });
                    handle_key(ke, current, min, max, steps, &cb, &f_key)
                });
        }
        m = m.then(self.modifier);

        match ctx.start_restartable_group(
            key,
            m,
            BoxLayout::new().alignment(crate::layout::Alignment::Center),
        ) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {}
        }
        ctx.set_current_node_focus_color(theme.primary);
        ctx.end_restartable_group();
    }
}

/// The range track's drawing node — the two-thumb counterpart of
/// [`crate::ui::slider::SliderTrackNode`], and the same shape: a named type (visible in the debug
/// tree, precise `Skip` through `node_key`) that owns the width write-back channel, so pixel↔value
/// conversion needs no recomposition.
///
/// `start_interaction` is the component's focus source (see the module note); the ring wraps
/// `active_thumb` when that source reports focus.
#[derive(Debug)]
pub(crate) struct RangeSliderTrackNode {
    /// Track width write-back (read by the tap/drag pixel↔value conversion).
    pub(crate) track_width: crate::core::state::Backchannel<f32>,
    pub(crate) start_interaction: MutableInteractionSource,
    pub(crate) colors: SliderColors,
    pub(crate) enabled: bool,
    pub(crate) value: RangeValue,
    pub(crate) min: f32,
    pub(crate) max: f32,
    pub(crate) steps: i32,
    pub(crate) start_active: bool,
    pub(crate) end_active: bool,
    pub(crate) active_thumb: RangeThumb,
}

impl crate::modifier::DrawNode for RangeSliderTrackNode {
    fn draw(&self, canvas: &skia_safe::Canvas, rect: skia_safe::Rect) {
        self.track_width.set(rect.width());
        // Render-time peek, as in `SliderTrackNode`: the focus state reaches the ring through the
        // redraw channel (a focused thumb also rebuilds through `interaction.state`), so reading a
        // dependency here would be too late to register anyway.
        let focused = self.start_interaction.is_focused_value();
        let focus_alpha = self.start_interaction.focus_indicator_alpha_value();
        let thumbs = [
            TrackThumb {
                value: self.value.start,
                active: self.start_active,
                focused: focused && self.active_thumb == RangeThumb::Start,
            },
            TrackThumb {
                value: self.value.end,
                active: self.end_active,
                focused: focused && self.active_thumb == RangeThumb::End,
            },
        ];
        draw_track(
            canvas, rect, &self.colors, self.enabled, &thumbs, self.min, self.max, self.steps, false,
            focus_alpha,
        );
    }

    fn node_key(&self) -> String {
        // Static visual params only: both values, the range, the step count, per-thumb active
        // state, which thumb the ring would wrap, the colors, the enable flag and the focus source
        // identity. `track_width` (write-back) and `focus_alpha` (per-frame animation) stay out,
        // exactly as in `SliderTrackNode`.
        format!(
            "rangeslidertrack:{:?}:{}:{}:{}:{}:{}:{}:{}:{}:{:?}:{}",
            self.colors,
            self.enabled,
            self.value.start.to_bits(),
            self.value.end.to_bits(),
            self.min.to_bits(),
            self.max.to_bits(),
            self.steps,
            self.start_active,
            self.end_active,
            self.active_thumb,
            self.start_interaction.source_id(),
        )
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
    }

    #[test]
    fn a_press_on_the_far_side_moves_the_other_thumb() {
        let got = Arc::new(AtomicI32::new(-1));
        let (press, _drag, _end) = callbacks(RangeValue::new(0.25, 0.75), 0, got.clone());
        // x = 240 is nearer the end thumb → the START thumb keeps 0.25 and the end thumb jumps.
        press((240.0, 24.0));
        assert_eq!(got.load(Ordering::Relaxed), 250, "the start thumb value is unchanged");
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
        let src = MutableInteractionSource::new();
        let node = |v: RangeValue, min: f32, max: f32, steps: i32, sa: bool, ea: bool, t: RangeThumb| {
            RangeSliderTrackNode {
                track_width: crate::core::state::Backchannel::new(0.0),
                start_interaction: src.clone(),
                colors,
                enabled: true,
                value: v,
                min,
                max,
                steps,
                start_active: sa,
                end_active: ea,
                active_thumb: t,
            }
        };
        let base = node(RangeValue::new(0.25, 0.75), 0.0, 1.0, 0, false, false, RangeThumb::Start);
        let base_key = base.node_key();
        assert_eq!(base_key, node(RangeValue::new(0.25, 0.75), 0.0, 1.0, 0, false, false, RangeThumb::Start).node_key(), "same params → same key (Skip holds)");
        assert_ne!(base_key, node(RangeValue::new(0.2, 0.75), 0.0, 1.0, 0, false, false, RangeThumb::Start).node_key(), "start value");
        assert_ne!(base_key, node(RangeValue::new(0.25, 0.7), 0.0, 1.0, 0, false, false, RangeThumb::Start).node_key(), "end value");
        assert_ne!(base_key, node(RangeValue::new(0.25, 0.75), 0.0, 2.0, 0, false, false, RangeThumb::Start).node_key(), "max");
        assert_ne!(base_key, node(RangeValue::new(0.25, 0.75), -1.0, 1.0, 0, false, false, RangeThumb::Start).node_key(), "min");
        assert_ne!(base_key, node(RangeValue::new(0.25, 0.75), 0.0, 1.0, 4, false, false, RangeThumb::Start).node_key(), "steps");
        assert_ne!(base_key, node(RangeValue::new(0.25, 0.75), 0.0, 1.0, 0, true, false, RangeThumb::Start).node_key(), "start thumb active");
        assert_ne!(base_key, node(RangeValue::new(0.25, 0.75), 0.0, 1.0, 0, false, true, RangeThumb::Start).node_key(), "end thumb active");
        assert_ne!(base_key, node(RangeValue::new(0.25, 0.75), 0.0, 1.0, 0, false, false, RangeThumb::End).node_key(), "which thumb the ring wraps");
        let mut other = node(RangeValue::new(0.25, 0.75), 0.0, 1.0, 0, false, false, RangeThumb::Start);
        other.enabled = false;
        assert_ne!(base_key, other.node_key(), "enabled");
    }
}
