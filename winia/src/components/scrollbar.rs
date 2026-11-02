//! The scrollbar - Compose Multiplatform's desktop `VerticalScrollbar` / `HorizontalScrollbar` plus
//! M3's `Modifier.nonInteractiveScrollbar` (`Scrollbar.kt`, checked in the browser; the geometry
//! formula follows the `drawScrollbar` gist).
//!
//! Shape: a standalone component (not a modifier), placed next to the scroll container (side by side
//! in a `Row`, or overlaying it on the right inside a `Stack`).
//! Inputs: `ScrollState { offset/fling_limit }` plus its own constraints (the viewport).
//! Three quantities: the offset (read), the viewport (its own constraint height/width) and the
//! content extent (`fling_limit + viewport`).
//!
//! - Thumb geometry (as in M3): `thumb = clamp(track * viewport/content, min, track*0.9)`,
//!   `thumb_offset = (scroll/max) * (track - thumb)`; nothing is drawn while content <= viewport.
//! - Dragging the thumb: `on_drag` -> `offset.set(drag_pos / (track - thumb) * max)`. A drag START
//!   takes over (cancels the list's fling, marks scrolling, and release clears it - otherwise the
//!   fling's `update_animations` write-back keeps the thumb from moving; after release the bar stays
//!   while the pointer still hovers it).
//! - Visibility: `always_show` / scrolling / hover / dragging / scroll pulse, plus a fade animation
//!   (M3: 400 ms delay + 250 ms tween - `push_animatable(fade_state)` drives it, the draw peeks it
//!   and multiplies the alpha, and `fade` stays out of the key so nothing recomposes).
//!   A wheel or programmatic scroll never sets `is_scroll_in_progress` (dispatch only cancels and
//!   sets false), so scrolling is detected from the OFFSET CHANGE instead: every change restarts the
//!   effect (aborting the sleep) -> a continuous scroll stays visible, fading out 400 ms after it
//!   stops. WARNING: a bool latch does not work (set true and never cleared -> always visible).
//!
//! Scope of v1: no RTL mirroring built in (a vertical bar sits where the caller puts it - the last
//! child of a `Row` follows the direction), and a lazy list needs the `LazyScrollbar` wrappers below.

use crate::composable;
use crate::runtime::composer::{ComposeCtx, GroupStatus};
use crate::runtime::state::State;
use crate::layout::BoxLayout;
use crate::modifier::{Modifier};
use crate::graphics::Color;
use crate::input::interaction::MutableInteractionSource;
use crate::theme::WiniaTheme;

/// Default thickness (M3 `Thickness = 4dp`; desktop CMP often uses 8 dp with padding - winia uses 6).
pub const SCROLLBAR_THICKNESS: f32 = 6.0;
/// Default minimum thumb length (M3 `ThumbMinLength = 24dp`).
pub const SCROLLBAR_THUMB_MIN_LENGTH: f32 = 24.0;
/// Maximum thumb length as a fraction of the track (M3 `ThumbMaxLengthFraction = 0.9`).
pub const SCROLLBAR_THUMB_MAX_FRACTION: f32 = 0.9;
/// Default thumb colour (M3 outline at 70% - `NonInteractiveScrollbarDefaults.thumbColor`).
pub fn scrollbar_thumb_color(theme: &crate::theme::ThemeColors) -> Color {
    let o = theme.outline;
    Color::from_argb(((o.a as f32) * 0.7) as u8, o.r, o.g, o.b)
}

/// Pure geometry: given viewport/content/scroll, compute (thumb_offset, thumb_len, visible).
/// Decoupled from drawing and the components so it can be unit tested. `track = viewport - 2*inset`.
pub(crate) fn scrollbar_geometry(
    viewport: f32,
    content: f32,
    scroll: f32,
    thumb_min: f32,
    thumb_max_fraction: f32,
    inset: f32,
) -> Option<(f32, f32)> {
        // Non-finite input is rejected outright (the offset is a public `State`, so a caller can set
        // NaN; NaN inside clamp or a Skia rect is undefined behaviour) - draw nothing.
    if !viewport.is_finite() || !content.is_finite() || !scroll.is_finite() {
        return None;
    }
    if viewport <= 0.0 || content <= viewport {
        return None;
    }
    let track = (viewport - 2.0 * inset).max(0.0);
    if track <= 0.0 || track < thumb_min {
        return None;
    }
    let max_thumb = track * thumb_max_fraction;
        // P0-1: when max_thumb < thumb_min the clamp(min > max) panics outright (guaranteed on a small
        // track or a large thumb_min) - a track that cannot hold the minimum thumb draws nothing.
    if max_thumb < thumb_min {
        return None;
    }
    let thumb = (track * viewport / content).clamp(thumb_min, max_thumb);
    let max_offset = content - viewport;
    let thumb_offset = if max_offset > 0.0 {
        (scroll.clamp(0.0, max_offset) / max_offset) * (track - thumb)
    } else {
        0.0
    };
    Some((thumb_offset, thumb))
}

/// Pure geometry: the scroll offset for a thumb dragged to `pos` inside the track.
pub(crate) fn scrollbar_offset_for_thumb_pos(
    pos: f32,
    track: f32,
    thumb: f32,
    max_offset: f32,
) -> f32 {
    if !pos.is_finite() || !track.is_finite() || !thumb.is_finite() || !max_offset.is_finite() {
        return 0.0;
    }
    let travel = (track - thumb).max(1.0);
    (pos / travel * max_offset).clamp(0.0, max_offset.max(0.0))
}

// ── ScrollbarNode ──

/// The scrollbar's paint node (a rounded thumb plus an optional track).
///
/// Fade: M3's 400 ms delay + 250 ms tween. `fade: State` is stored on the node and `draw` peeks it
/// to multiply the alpha (no recomposition - the animation engine requests a redraw per frame, the
/// same convention as Slider's `focus_alpha`). It must NOT be snapshotted to an f32 during build
/// (peek registers no dependency -> a fade change never recomposes -> the node keeps the old value
/// -> always-visible never shows and the bar vanishes on release; measured).
#[derive(Debug)]
pub(crate) struct ScrollbarNode {
    pub(crate) vertical: bool,
    pub(crate) thumb_color: Color,
    pub(crate) track_color: Color,
    pub(crate) thickness: f32,
    pub(crate) inset: f32,
        /// Render-time geometry (computed during build from the constraints and the States; draw only paints).
    pub(crate) thumb_offset: f32,
    pub(crate) thumb_len: f32,
        /// The geometry is valid (content > viewport and the track can hold the thumb). Fade is gated separately.
    pub(crate) has_thumb: bool,
        /// The fade alpha State (peeked during draw; not part of the key).
    pub(crate) fade: State<f32>,
}

impl crate::modifier::DrawNode for ScrollbarNode {
    fn draw(&self, canvas: &skia_safe::Canvas, rect: skia_safe::Rect) {
                // The fade State is peeked during draw (no recomposition - the animation engine requests a
                // redraw per frame; snapshotting it to an f32 during build registers no dependency, so the
                // fade would never recompose; measured).
        let fade = self.fade.peek();
        if !self.has_thumb || fade <= 0.01 {
            return;
        }
                // Fade alpha multiply (M3 fade semantics).
        let mul = fade.clamp(0.0, 1.0);
        let thumb_a = ((self.thumb_color.a as f32) * mul) as u8;
        let track_a = ((self.track_color.a as f32) * mul) as u8;
        let mut paint = skia_safe::Paint::default();
        paint.set_anti_alias(true);
                // Track (full length, skipped while transparent).
        if track_a != 0 {
            paint.set_color(skia_safe::Color::from_argb(
                track_a,
                self.track_color.r,
                self.track_color.g,
                self.track_color.b,
            ));
            if self.vertical {
                canvas.draw_rect(
                    skia_safe::Rect::new(
                        rect.left,
                        rect.top + self.inset,
                        rect.left + self.thickness,
                        rect.bottom - self.inset,
                    ),
                    &paint,
                );
            } else {
                canvas.draw_rect(
                    skia_safe::Rect::new(
                        rect.left + self.inset,
                        rect.top,
                        rect.right - self.inset,
                        rect.top + self.thickness,
                    ),
                    &paint,
                );
            }
        }
                // Thumb (rounded, radius = thickness/2).
        paint.set_color(skia_safe::Color::from_argb(
            thumb_a,
            self.thumb_color.r,
            self.thumb_color.g,
            self.thumb_color.b,
        ));
        let r = self.thickness / 2.0;
        if self.vertical {
            let thumb_rect = skia_safe::Rect::new(
                rect.left,
                rect.top + self.inset + self.thumb_offset,
                rect.left + self.thickness,
                rect.top + self.inset + self.thumb_offset + self.thumb_len,
            );
            canvas.draw_round_rect(thumb_rect, r, r, &paint);
        } else {
            let thumb_rect = skia_safe::Rect::new(
                rect.left + self.inset + self.thumb_offset,
                rect.top,
                rect.left + self.inset + self.thumb_offset + self.thumb_len,
                rect.top + self.thickness,
            );
            canvas.draw_round_rect(thumb_rect, r, r, &paint);
        }
    }
    fn node_key(&self) -> String {
                // The fade State is NOT part of the key (a per-frame animation value peeked during draw;
                // in the key it would re-enter every frame. The write-back channel mirrors Slider's
                // `focus_alpha`, and the engine's request_redraw drives the drawing).
        format!(
            "scrollbar:{}:{:?}:{:?}:{}:{}:{}:{}:{}",
            self.vertical,
            self.thumb_color,
            self.track_color,
            self.thickness.to_bits(),
            self.inset.to_bits(),
            self.thumb_offset.to_bits(),
            self.thumb_len.to_bits(),
            self.has_thumb,
        )
    }
}

// -- Shared build logic (P2-7: the vertical and horizontal builds were line-for-line duplicates and
// a fix would land in one only - the horizontal viewport peek->get miss, which left the bar
// invisible on the first frame, is the measured case). The common function keeps a FIXED remember
// order (call order is slot order): hover_src -> last_scroll -> last_pulse -> fade_state -> effect
// (one slot) -> viewport_state -> grab_offset, so all four components (Vertical/Horizontal/
// Lazy/HorizontalLazy) call it in the same order and never drift a slot. The two lazy builds take
// one extra `scrolling_state` remember before calling it, but each component has its own scope
// (`#[composable]` source-hash isolated), so the slots cannot interfere.
// The axis differences are parameterized by ScrollbarAxis (four places): the size element, the
// on_size_changed axis, the drag's local axis, and ScrollbarNode.vertical.
#[derive(Clone, Copy)]
enum ScrollbarAxis {
    Vertical,
    Horizontal,
}

/// The scrollbar axis (every difference between the vertical and horizontal builds).
#[derive(Clone, Copy)]
struct ScrollbarConfig {
    thickness: f32,
    thumb_color: Option<Color>,
    track_color: Color,
    thumb_min_length: f32,
    always_show: bool,
    inset: f32,
}

/// Shared configuration (each component's appearance parameters - used for the changed registration;
/// copied into closures so no borrow escapes).
fn scrollbar_build_shared(
    ctx: &mut ComposeCtx,
    scroll_state_src: &crate::modifier::ScrollState,
    cfg: ScrollbarConfig,
    axis: ScrollbarAxis,
    scroll_reverse: Option<bool>,
) -> Modifier {
    let vertical = matches!(axis, ScrollbarAxis::Vertical);
    let theme = WiniaTheme::colors();
    let thumb_color = cfg
        .thumb_color
        .unwrap_or_else(|| scrollbar_thumb_color(&theme));
    /// Shared build body: read the three quantities -> fade pulse/effect -> viewport -> assemble the
    /// Modifier -> return. The changed registrations stay in each build's head (they belong to `Self`).
    /// `scroll_reverse`: Some for a reversed horizontal container (`is_horizontal_scroll_reversed`'s
    /// value) - the render mirrors the offset semantics (offset 0 = the content's end), and the
    /// scrollbar geometry and drag use the same mirrored coordinate `visual = max - scroll`; None for
    /// vertical. `None` and `Some(false)` are equivalent (both use the raw offset); the `Option` shape
    /// exists so the accessor's return value can be passed straight through.
    let scroll_raw = scroll_state_src.offset.get();
    let limit = scroll_state_src.fling_limit.get();
    let scrolling = scroll_state_src.is_scroll_in_progress.get();
        // The three quantities: the offset (read, registers a dependency) and fling_limit (read,
        // registers a dependency); the viewport comes from its own constraint (with fill_max_height it
        // is the parent's height, which is why the bar must sit level with the scroll container).
        // It MUST be a build-time `get` (registers a composition dependency); a draw-time peek does not.
    let scroll = match scroll_reverse {
        Some(true) => (limit - scroll_raw).clamp(0.0, limit.max(0.0)),
        _ => scroll_raw,
    };
        // P2-3: with reverse the offset semantics are mirrored - the scrollbar works in visual
        // coordinates (visual 0 = the content's end = the bar at the start; visual = max - scroll).
        // Vertical has no reverse notion, so None uses the raw offset.
    let hover_src: MutableInteractionSource =
        ctx.remember(|| MutableInteractionSource::new()).get();
    let hovered = hover_src.is_hovered();
    let dragged = hover_src.is_dragged();
        // Visible while hovered or dragged too (a hidden bar cannot be grabbed) - the interaction source
        // is remembered and held.
    let last_scroll = ctx.remember_backchannel(|| scroll);
    let scroll_active = last_scroll.peek() != scroll;
    if scroll_active {
        last_scroll.set(scroll);
    }
    let last_pulse = ctx.remember_backchannel(|| scroll_state_src.scroll_pulse.get());
    let pulse_active = last_pulse.peek() != scroll_state_src.scroll_pulse.get();
    if pulse_active {
        last_pulse.set(scroll_state_src.scroll_pulse.get());
    }
    let fade_target = if cfg.always_show || scrolling || hovered || dragged
        || scroll_active || pulse_active
    {
        1.0f32
    } else {
        0.0f32
    };
        // Fade: the target alpha (always_show / scrolling / hover / dragging / scroll pulse -> 1, else
        // 0) driven by a 250 ms tween (M3 ThumbFadeDurationMillis; the 400 ms delay lives in the
        // LaunchedEffect). Peeked at draw time (no recomposition) - fade_state stays out of node_key.
        //
        // WARNING: for a wheel or programmatic scroll `scrolling` is always false (dispatch cancels and
        // sets false; only a drag sets it true), so scrolling is detected from the offset and pulse
        // CHANGES instead: `last_scroll` remembers the previous frame's offset, and a different value
        // means a scroll is running (written through a Backchannel, so no recomposition - this frame's
        // build carries on); `scroll_pulse` is bumped by dispatch when a wheel event HIT but consumed
        // nothing (still scrolling at the top or bottom, offset unchanged, P1-3). Either change lights
        // the fade, which is the feedback for "the end was reached" (same as M3/CMP).
        // WARNING: a bool latch does not work (true once scrolled, never cleared -> always visible).
        // WARNING: the key MUST contain `scroll`: a bool condition merges consecutive scrolls - the
        // 250 ms tween reaches 1, the key then stops changing while the scroll continues, and the
        // following 400 ms sleep fades the bar out. `scroll` changes every frame -> the effect restarts
        // every frame -> the sleep is aborted -> the bar stays.
        // (pulse fires once at a boundary and is NOT in the key - a single pulse in the key would
        // restart the effect twice on the same frame as `scroll`; its `get` already registers the
        // dependency that drives the lit frame.)
    let fade_state: State<f32> = ctx.remember(|| fade_target);
    let keep_visible = cfg.always_show || scrolling || hovered || dragged;
    crate::effect::LaunchedEffect::new((fade_target, scrolling, hovered, dragged, scroll)).build(
        ctx,
        {
            let fade_state = fade_state.clone();
            move |scope| {
                let fade_state = fade_state.clone();
                async move {
                    if fade_target > 0.5 {
                            // Leaving the interaction (hover out and the scroll stops) fades out after 400 ms (M3
                            // ThumbFadeDelayMillis): one LaunchedEffect owns "show -> wait -> hide". The key holds every
                            // input, so any change aborts the sleep and restarts it (a continuous scroll restarts every
                            // frame -> stays visible; once it stops there is no recomposition and no restart, so the task
                            // from the last scrolling frame sleeps the full 400 ms -> hides, exactly 400 ms after the stop).
                            // On waking it looks only at the static hold conditions (always_show / scrolling / hover /
                            // drag):
                            // - any true -> stay (a resting hover must not hide, since nothing would recompose it back);
                            // - none -> tween to 0 and hide.
                            // WARNING: the static snapshot inside the task is the latest one - an unaborted sleep proves
                            // none of the key inputs changed (except the scroll pulse, which only survives together with a
                            // new `scroll` that would have aborted this task, so on waking it has always decayed).
                            // WARNING: a "has scrolled" latch does not work (never cleared -> always visible): the pulse
                            // plus the snapshot is the whole semantics.
                        crate::animation::push_animatable(
                            fade_state.clone(),
                            1.0,
                            crate::animation::AnimationSpec::Tween(
                                crate::animation::TweenSpec::new(
                                    std::time::Duration::from_millis(250),
                                    crate::animation::interpolator::Linear::new(),
                                ),
                            ),
                        );
                    }
                                            // Show: tween to 1 (playing from the current value, so no flash).
                    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
                    if !keep_visible {
                        crate::animation::push_animatable(
                            fade_state,
                            0.0,
                            crate::animation::AnimationSpec::Tween(
                                crate::animation::TweenSpec::new(
                                    std::time::Duration::from_millis(250),
                                    crate::animation::interpolator::Linear::new(),
                                ),
                            ),
                        );
                    }
                    drop(scope);
                }
            }
        },
    );
                        // Hold: a scroll/hover/drag change during the delay changes the key, aborts the
                        // sleep, and keeps the bar up.
    let viewport_state: State<f32> = ctx.remember(|| 0.0f32);
        // The viewport is not known yet (first frame, constraints not settled) -> fall back to the
        // remembered previous value; nothing is drawn on the first frame.
    let viewport = viewport_state.get();
    let content = limit + viewport;

    let (thumb_offset, thumb_len, has_thumb) = match scrollbar_geometry(
        viewport,
        content,
        scroll,
        cfg.thumb_min_length,
        SCROLLBAR_THUMB_MAX_FRACTION,
        cfg.inset,
    ) {
        Some((off, len)) => (off, len, true),
        _ => (0.0, 0.0, false),
    };

        // The viewport comes from measure, so read the previous frame's value here. v1: the scrollbar
        // fills max height and the parent decides its constraint, so build cannot see it. It is taken
        // from layout instead: an on_size_changed callback writes viewport_state (the same idea as
        // NavTransition capturing its displacement baseline). 0 on the first frame -> nothing drawn,
        // recomposed once the write-back arrives.
        // WARNING P0-2: it MUST be a `get` (registers a composition dependency), because only `set`'s
        // notify drives a recomposition; `peek` has no subscriber, so the write-back would never
        // recompose and the geometry would stay stale after the first frame or a resize (the manual
        // double-build tests hid this; the horizontal bar once kept a peek and never showed on the
        // first frame - measured). `set` dedups by PartialEq and on_size_changed dedups per element,
        // so a stable size stops the cycle.
    let track = (viewport - 2.0 * cfg.inset).max(0.0);
    let max_offset = (content - viewport).max(0.0);
    let drag_thumb_len = thumb_len;
        // Drag the thumb -> derive the offset (track/thumb/max are build-time values captured by the
        // closure).
    let grab_offset = ctx.remember_backchannel(|| 0.0f32);
    let drag_inset = cfg.inset;
    let drag_cb = {
        let scroll_state_src = scroll_state_src.clone();
        let grab_offset = grab_offset.clone();
                // Under reverse the visual -> raw write-back has to be mirrored (visual = max - raw).
        let reverse_write = matches!(scroll_reverse, Some(true));
        let write_max = max_offset;
        move |pos: (f32, f32)| {
                        // `pos` is in the thumb bar's local coordinates (inset included); subtracting the inset
                        // gives the position inside the track.
            let pos_in_track = if vertical { pos.1 } else { pos.0 } - drag_inset;
            let target = pos_in_track - grab_offset.peek();
            let off_visual = scrollbar_offset_for_thumb_pos(target, track, drag_thumb_len, max_offset);
            let off = if reverse_write {
                (write_max - off_visual).clamp(0.0, write_max.max(0.0))
            } else {
                off_visual
            };
            scroll_state_src.offset.set(off);
        }
    };
    let (hover_src_clone, scroll_state_clone) =
        (hover_src.clone(), scroll_state_src.clone());
    let (hover_src_clone2, scroll_state_clone2) =
        (hover_src.clone(), scroll_state_src.clone());
        // P1-2: `drag_cancel` runs exactly the same cleanup as `end` (a lost window or a system
        // interruption never sends `end`) - otherwise is_scroll_in_progress/dragged stay set and the
        // fade never hides.
    let m = if vertical {
        Modifier::new()
            .width(cfg.thickness)
            .fill_max_height()
    } else {
        Modifier::new()
            .height(cfg.thickness)
            .fill_max_width()
    }
    .hoverable(&hover_src)
    .on_size_changed(move |w, h| {
        if vertical {
            let _ = w;
            viewport_state.set(h);
        } else {
            viewport_state.set(w);
        }
    })
    .draw_node(ScrollbarNode {
        vertical,
        thumb_color,
        track_color: cfg.track_color,
        thickness: cfg.thickness,
        inset: cfg.inset,
        thumb_offset,
        thumb_len,
        has_thumb,
        fade: fade_state.clone(),
    })
    .on_drag_start({
        let hover_src = hover_src_clone.clone();
        let scroll_state = scroll_state_clone.clone();
        let grab_offset = grab_offset.clone();
        move |pos: (f32, f32)| {
            hover_src.emit_drag_start();
                        // Take over: cancel the list's fling (otherwise the next frame's update_animations writes
                        // the offset back and the thumb cannot move); mark scrolling (cleared on release).
            scroll_state.cancel_fling();
            scroll_state.is_scroll_in_progress.set(true);
                        // Grab offset = press point inside the track minus the thumb's top at that moment
                        // (captured from the build values). Clamped to [0, thumb]: a press outside the thumb
                        // (on the empty track) puts grab at the thumb edge, so the first frame does not jump
                        // ("grab the edge you pressed and follow from there").
            let pos_in_track = if vertical { pos.1 } else { pos.0 } - drag_inset;
            grab_offset.set((pos_in_track - thumb_offset).clamp(0.0, drag_thumb_len.max(0.0)));
        }
    })
    .on_drag_end(move || {
        hover_src_clone2.emit_drag_end();
        scroll_state_clone2.is_scroll_in_progress.set(false);
    })
    .on_drag_cancel({
        let hover_src = hover_src.clone();
        let scroll_state_src = scroll_state_src.clone();
        move || {
            hover_src.emit_drag_end();
            scroll_state_src.is_scroll_in_progress.set(false);
        }
    })
    .on_drag(move |pos, _delta| drag_cb(pos));
    m
}

// -- Lazy scrollbar adaptation --
//
// LazyColumn/LazyRow use LazyListState (an anchor model: pixel offset plus a derived
// first_visible), while Vertical/HorizontalScrollbar take a ScrollState (the pixel model).
// LazyListState.offset and ScrollState.offset mean the same pixels (lazy scroll input goes through
// the vertical_scroll channel and the offset hangs off the same driver), so adapting one means
// turning the LazyListState channels into ScrollState handles: offset (read dependency) +
// fling_limit (read dependency) + is_scroll_in_progress. The lazy side has no such field - the
// lazy build remembers an `is_scrolling` (lazy_column.rs:643) but outside code only gets the
// LazyListState, not that flag. So only offset/fling_limit/pulse are converted, scrolling stays
// false: a wheel or programmatic scroll never sets it anyway (dispatch only cancels and sets
// false), the fade is lit by the offset change plus the pulse, and dragging the thumb sets the
// scrollbar's own assembled state through on_drag_start (the scrollbar assembles its own
// ScrollState, which is why dragging shows correctly). `pulse` is cloned too (wired up in
// lazy_column.rs).
//
// Usage:
pub struct LazyScrollbar {
    state: crate::layout::lazy::LazyListState,
    modifier: Modifier,
    thickness: f32,
    thumb_color: Option<Color>,
    track_color: Color,
    thumb_min_length: f32,
    always_show: bool,
    scroll_reverse: Option<bool>,
}

impl LazyScrollbar {
    pub fn new(state: crate::layout::lazy::LazyListState) -> Self {
        Self {
            state,
            modifier: Modifier::new(),
            thickness: SCROLLBAR_THICKNESS,
            thumb_color: None,
            track_color: Color::TRANSPARENT,
            thumb_min_length: SCROLLBAR_THUMB_MIN_LENGTH,
            always_show: false,
            scroll_reverse: None,
        }
    }

    pub fn modifier(mut self, m: Modifier) -> Self { self.modifier = self.modifier.then(m); self }
    pub fn thickness(mut self, t: f32) -> Self { self.thickness = t; self }
    pub fn thumb_color(mut self, c: Color) -> Self { self.thumb_color = Some(c); self }
    pub fn track_color(mut self, c: Color) -> Self { self.track_color = c; self }
    pub fn thumb_min_length(mut self, l: f32) -> Self { self.thumb_min_length = l; self }
    pub fn always_show(mut self, v: bool) -> Self { self.always_show = v; self }
    /// A lazy list's vertical scrollbar (CMP `VerticalScrollbar` adapted to LazyListState).
    /// `scroll_reverse`: a reversed lazy list (`reverse_layout(true)`) - the render mirrors the offset
    /// semantics (offset 0 = the content's end) and the scrollbar geometry and drag use the same
    /// mirrored coordinate `visual = max - scroll`. `None` and `Some(false)` are equivalent.
    pub fn scroll_reverse(mut self, v: Option<bool>) -> Self { self.scroll_reverse = v; self }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        ctx.changed(&self.thickness);
        ctx.changed(&self.thumb_color);
        ctx.changed(&self.track_color);
        ctx.changed(&self.thumb_min_length);
        ctx.changed(&self.always_show);
        ctx.changed(&self.scroll_reverse);
        ctx.changed(&self.state.offset.state_id());
        let key = ctx.next_key();
        let cfg = ScrollbarConfig {
            thickness: self.thickness,
            thumb_color: self.thumb_color,
            track_color: self.track_color,
            thumb_min_length: self.thumb_min_length,
            always_show: self.always_show,
            inset: 2.0,
        };
            /// A reversed lazy list (`reverse_layout(true)` - the same value the list has).
        let scrolling_state: State<bool> = ctx.remember(|| false);
        let assembled = crate::modifier::ScrollState {
            last_read_offset: std::sync::Arc::new(std::sync::Mutex::new(0.0)),
            overscroll_cache: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            offset: self.state.offset.clone(),
            is_scroll_in_progress: scrolling_state.clone(),
            fling_limit: self.state.fling_limit.clone(),
            scroll_pulse: self.state.scroll_pulse.clone(),
            // The scrollbar drives its OWN drag, not the list's: it reads the list's offset but writes
            // its dragging state here, so it must not pick up the list's paging snap.
            snap: crate::runtime::state::Backchannel::new(None),
            step_goal: std::sync::Arc::new(std::sync::Mutex::new(None)),
            fling_behavior: None,
        };
        let m = scrollbar_build_shared(ctx, &assembled, cfg, ScrollbarAxis::Vertical, self.scroll_reverse)
            .then(self.modifier);
        match ctx.start_restartable_group(key, m, BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {}
        }
        ctx.end_restartable_group();
    }
}

        // Assemble the ScrollState: offset/fling_limit/pulse are cloned (the same States are stable
        // across frames); is_scroll_in_progress is a remembered false (the lazy side has no such
        // channel; the fade relies on the change plus the pulse, and a thumb drag sets this
        // assembled state through the scrollbar's own on_drag_start - note that does not write back
        // to the LazyListState, but the fade reads the assembled scrolling, which is valid within
        // the same frame; release clears the same assembled state, so nothing is left behind).
pub struct HorizontalLazyScrollbar {
    state: crate::layout::lazy::LazyListState,
    modifier: Modifier,
    thickness: f32,
    thumb_color: Option<Color>,
    track_color: Color,
    thumb_min_length: f32,
    always_show: bool,
    scroll_reverse: Option<bool>,
}

impl HorizontalLazyScrollbar {
    pub fn new(state: crate::layout::lazy::LazyListState) -> Self {
        Self {
            state,
            modifier: Modifier::new(),
            thickness: SCROLLBAR_THICKNESS,
            thumb_color: None,
            track_color: Color::TRANSPARENT,
            thumb_min_length: SCROLLBAR_THUMB_MIN_LENGTH,
            always_show: false,
            scroll_reverse: None,
        }
    }

    pub fn modifier(mut self, m: Modifier) -> Self { self.modifier = self.modifier.then(m); self }
    pub fn thickness(mut self, t: f32) -> Self { self.thickness = t; self }
    pub fn thumb_color(mut self, c: Color) -> Self { self.thumb_color = Some(c); self }
    pub fn track_color(mut self, c: Color) -> Self { self.track_color = c; self }
    pub fn thumb_min_length(mut self, l: f32) -> Self { self.thumb_min_length = l; self }
    pub fn always_show(mut self, v: bool) -> Self { self.always_show = v; self }
    /// A lazy row's horizontal scrollbar (CMP `HorizontalScrollbar` adapted to LazyListState, used by
    /// `LazyRow`). The assembly semantics are identical to `LazyScrollbar`, only the axis differs;
    /// `scroll_reverse` has the same meaning (pass `Some(true)` for a reversed horizontal lazy list).
    /// Usage:
    /// ```ignore
    /// let list_state = ctx.remember(|| LazyListState::new()).get();
    /// LazyRow::new()
    ///     .state(list_state.clone())
    ///     .items_plain(30, content)
    ///     .build(ctx);
    /// HorizontalLazyScrollbar::new(list_state).build(ctx);
    /// ```
    pub fn scroll_reverse(mut self, v: Option<bool>) -> Self { self.scroll_reverse = v; self }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        ctx.changed(&self.thickness);
        ctx.changed(&self.thumb_color);
        ctx.changed(&self.track_color);
        ctx.changed(&self.thumb_min_length);
        ctx.changed(&self.always_show);
        ctx.changed(&self.scroll_reverse);
        ctx.changed(&self.state.offset.state_id());
        let key = ctx.next_key();
        let cfg = ScrollbarConfig {
            thickness: self.thickness,
            thumb_color: self.thumb_color,
            track_color: self.track_color,
            thumb_min_length: self.thumb_min_length,
            always_show: self.always_show,
            inset: 2.0,
        };
            /// A reversed horizontal lazy list (the same value the list has).
        let scrolling_state: State<bool> = ctx.remember(|| false);
        let assembled = crate::modifier::ScrollState {
            last_read_offset: std::sync::Arc::new(std::sync::Mutex::new(0.0)),
            overscroll_cache: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            offset: self.state.offset.clone(),
            is_scroll_in_progress: scrolling_state.clone(),
            fling_limit: self.state.fling_limit.clone(),
            scroll_pulse: self.state.scroll_pulse.clone(),
            // As above: the scrollbar's own drag must not inherit the list's paging snap.
            snap: crate::runtime::state::Backchannel::new(None),
            step_goal: std::sync::Arc::new(std::sync::Mutex::new(None)),
            fling_behavior: None,
        };
        let m = scrollbar_build_shared(ctx, &assembled, cfg, ScrollbarAxis::Horizontal, self.scroll_reverse)
            .then(self.modifier);
        match ctx.start_restartable_group(key, m, BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {}
        }
        ctx.end_restartable_group();
    }
}

// ── VerticalScrollbar ──

        // Same assembly semantics as LazyScrollbar (above): scrolling is a remembered false, the
        // fade relies on the pulse, and a drag sets the assembled state without leaving residue.
pub struct VerticalScrollbar {
    scroll: crate::modifier::ScrollState,
    modifier: Modifier,
    thickness: f32,
    thumb_color: Option<Color>,
    track_color: Color,
    thumb_min_length: f32,
    always_show: bool,
}

impl VerticalScrollbar {
    pub fn new(scroll: crate::modifier::ScrollState) -> Self {
        Self {
            scroll,
            modifier: Modifier::new(),
            thickness: SCROLLBAR_THICKNESS,
            thumb_color: None,
            track_color: Color::TRANSPARENT,
            thumb_min_length: SCROLLBAR_THUMB_MIN_LENGTH,
            always_show: false,
        }
    }

    pub fn modifier(mut self, m: Modifier) -> Self { self.modifier = self.modifier.then(m); self }
    pub fn thickness(mut self, t: f32) -> Self { self.thickness = t; self }
    pub fn thumb_color(mut self, c: Color) -> Self { self.thumb_color = Some(c); self }
    pub fn track_color(mut self, c: Color) -> Self { self.track_color = c; self }
    pub fn thumb_min_length(mut self, l: f32) -> Self { self.thumb_min_length = l; self }
    /// The vertical scrollbar (CMP `VerticalScrollbar`).
    ///
    /// Usage: level with the scroll container (inside a `Row`) and sharing its `ScrollState`.
    /// ```ignore
    /// Row::new().build(ctx, |ctx| {
    ///     Column::new().modifier(Modifier::new().fill_max_height().layout_weight(1.0)
    ///         .vertical_scroll(scroll.clone()))
    ///         .items_plain(30, content)
    ///         .build(ctx);
    ///     VerticalScrollbar::new(scroll.clone()).build(ctx);
    /// });
    /// ```
    pub fn always_show(mut self, v: bool) -> Self { self.always_show = v; self }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        ctx.changed(&self.thickness);
        ctx.changed(&self.thumb_color);
        ctx.changed(&self.track_color);
        ctx.changed(&self.thumb_min_length);
        ctx.changed(&self.always_show);
            /// Always visible (by default only during a scroll, like `alwaysShowScrollBar`).
        ctx.changed(&self.scroll.offset.state_id());
        let key = ctx.next_key();
        let cfg = ScrollbarConfig {
            thickness: self.thickness,
            thumb_color: self.thumb_color,
            track_color: self.track_color,
            thumb_min_length: self.thumb_min_length,
            always_show: self.always_show,
            inset: 2.0,
        };
        let m = scrollbar_build_shared(ctx, &self.scroll, cfg, ScrollbarAxis::Vertical, None)
            .then(self.modifier);
        match ctx.start_restartable_group(key, m, BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {}
        }
        ctx.end_restartable_group();
    }
}

// ── HorizontalScrollbar ──

        // P2-4: the ScrollState handle is part of the identity (swapping the whole state must
        // recompose - otherwise the old offset subscription stays live and the new state has none,
        // and the bar stays bound to the old one).
pub struct HorizontalScrollbar {
    scroll: crate::modifier::ScrollState,
    modifier: Modifier,
    thickness: f32,
    thumb_color: Option<Color>,
    track_color: Color,
    thumb_min_length: f32,
    always_show: bool,
    scroll_reverse: Option<bool>,
}

impl HorizontalScrollbar {
    pub fn new(scroll: crate::modifier::ScrollState) -> Self {
        Self {
            scroll,
            modifier: Modifier::new(),
            thickness: SCROLLBAR_THICKNESS,
            thumb_color: None,
            track_color: Color::TRANSPARENT,
            thumb_min_length: SCROLLBAR_THUMB_MIN_LENGTH,
            always_show: false,
            scroll_reverse: None,
        }
    }

    pub fn modifier(mut self, m: Modifier) -> Self { self.modifier = self.modifier.then(m); self }
    pub fn thickness(mut self, t: f32) -> Self { self.thickness = t; self }
    pub fn thumb_color(mut self, c: Color) -> Self { self.thumb_color = Some(c); self }
    pub fn track_color(mut self, c: Color) -> Self { self.track_color = c; self }
    pub fn thumb_min_length(mut self, l: f32) -> Self { self.thumb_min_length = l; self }
    pub fn always_show(mut self, v: bool) -> Self { self.always_show = v; self }
    /// A reversed horizontal scroll (RTL - the same value the container's
    /// `horizontal_scroll_reverse` has; an RTL container such as `TabRow` passes `Some(is_rtl)`).
    pub fn scroll_reverse(mut self, v: Option<bool>) -> Self { self.scroll_reverse = v; self }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        ctx.changed(&self.thickness);
        ctx.changed(&self.thumb_color);
        ctx.changed(&self.track_color);
        ctx.changed(&self.thumb_min_length);
        ctx.changed(&self.always_show);
        ctx.changed(&self.scroll_reverse);
        // P2-4: the ScrollState handle is part of the identity (swapping the whole state must
        // recompose - otherwise the old offset subscription stays live and the new state has none,
        // and the bar stays bound to the old one).
        ctx.changed(&self.scroll.offset.state_id());
        let key = ctx.next_key();
        let cfg = ScrollbarConfig {
            thickness: self.thickness,
            thumb_color: self.thumb_color,
            track_color: self.track_color,
            thumb_min_length: self.thumb_min_length,
            always_show: self.always_show,
            inset: 2.0,
        };
        let m = scrollbar_build_shared(ctx, &self.scroll, cfg, ScrollbarAxis::Horizontal, self.scroll_reverse)
            .then(self.modifier);
        match ctx.start_restartable_group(key, m, BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {}
        }
        ctx.end_restartable_group();
    }
}


#[cfg(test)]
mod tests;
