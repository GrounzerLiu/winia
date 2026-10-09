//! Top-level overlays — Popup / Dialog / DropdownMenu (mirrors Compose).
//!
//! Mechanism: overlay content **does not participate in main-tree layout** — it
//! is registered as an `OverlayDesc` during composition and materialized /
//! laid out / rendered by `app.rs` with a **dedicated Composer** (rendered
//! after the main tree = on top); pointer hit-testing prefers overlays
//! (topmost first) and an outside click triggers `on_dismiss_request`.
//!
//! Current limitations (v1):
//! - Overlay content only supports clickable (Button / menu items) — gestures /
//!   text selection will follow.
//! - Single-level popups (nesting will follow).

pub mod anchored_draggable;
use std::sync::Arc;
use crate::composable;

/// Where a panel anchored to a node starts, and how it gets to its final place.
///
/// Compose's `FullScreenSearchBarLayout` places its container at
/// `(lerp(collapsedBounds.left, offsetX, progress), lerp(collapsedBounds.top, offsetY, progress))`: the panel
/// grows out of the anchor's corner and ends at the window's corner. Both axes move together, so this is one
/// value rather than a flag per axis.
///
/// The anchor's coordinates are known only to the layout pass, so the caller supplies the progress reader
/// and the layout pass does the interpolation.
///
/// The two endpoints are resolved by `anchor_slide_origin` in the layout pass.
#[derive(Clone)]
pub struct AnchorSlide(pub std::sync::Arc<dyn Fn() -> f32 + Send + Sync>);

impl AnchorSlide {
    /// Create from a progress source (0 = at the anchor, 1 = at the window corner).
    pub fn new(progress: std::sync::Arc<dyn Fn() -> f32 + Send + Sync>) -> Self {
        Self(progress)
    }

    /// Progress, clamped to the animatable range.
    pub fn progress(&self) -> f32 {
        (self.0)().clamp(0.0, 1.0)
    }
}

/// Interpolate from the anchor's corner to the window's corner for one axis — Compose's
/// `lerp(collapsedBounds.<axis>, 0, progress)`.
pub fn anchor_slide_lerp(anchor_axis: f32, progress: f32) -> f32 {
    anchor_axis * (1.0 - progress.clamp(0.0, 1.0))
}

/// The point an anchored overlay scales out of, as fractions of its own box — material3's
/// `calculateTransformOrigin(anchorBounds, menuBounds)` (`material3/Menu.kt`), which is what makes a
/// dropdown menu look like it grows out of the control that opened it instead of out of its centre.
///
/// M3's branches, verbatim: a menu entirely beside the anchor pins that axis to its near edge (`0` when
/// the menu starts past the anchor's end, `1` when it ends before the anchor starts); a menu that
/// overlaps the anchor on an axis uses the middle of the two boxes' intersection; a zero-sized menu
/// falls back to `0`.
pub fn overlay_transform_origin(
    anchor: (f32, f32, f32, f32),
    menu: (f32, f32, f32, f32),
) -> (f32, f32) {
    let (ax, ay, aw, ah) = anchor;
    let (mx, my, mw, mh) = menu;
    let pivot_x = if mx >= ax + aw {
        0.0
    } else if mx + mw <= ax {
        1.0
    } else if mw == 0.0 {
        0.0
    } else {
        let intersection_center = (ax.max(mx) + (ax + aw).min(mx + mw)) / 2.0;
        (intersection_center - mx) / mw
    };
    let pivot_y = if my >= ay + ah {
        0.0
    } else if my + mh <= ay {
        1.0
    } else if mh == 0.0 {
        0.0
    } else {
        let intersection_center = (ay.max(my) + (ay + ah).min(my + mh)) / 2.0;
        (intersection_center - my) / mh
    };
    (pivot_x, pivot_y)
}

/// The height an entering/closing overlay is clipped to, or `None` for "no clip".
///
/// Two render-time rules, both easy to get wrong:
/// - a slide (`dy != 0`) clips to the FULL height, because the translation is what moves the panel;
/// - a reveal clips to `height * reveal`, and once `reveal` reaches 1 there is NO clip at all.
///
/// "No clip" must stay distinguishable from "clip to zero height": the first is a settled overlay, the
/// second is an invisible one. Reporting the sentinel as `0` made a fully open docked dropdown read as
/// empty in the render trace (measured: `painted_h=0` while `layout_h=280`).
pub fn overlay_reveal_clip_height(height: f32, dy: f32, reveal: f32) -> Option<f32> {
    if dy != 0.0 {
        Some(height)
    } else if reveal < 1.0 {
        Some((height * reveal).max(0.0))
    } else {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupPosition {
    TopLeft,
    TopCenter,
    TopRight,
    Center,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

/// Overlay **enter animation** spec (mirrors Compose content-layer
/// `AnimatedVisibility` `enter = scaleIn + fadeIn` — Compose Dialog itself has no
/// built-in animation; material2's fixed scale+fade lives in the content layer;
/// Winia pushes the animation down to the overlay container layer, driven
/// uniformly per frame without depending on content-layer animation components).
///
/// - `scale_from`: starting scale (default 0.8 — Compose `scaleIn(initialScale=0.8f)`)
/// - `fade`: whether to fade in (default true — Compose `fadeIn()`)
/// - `slide_from_y`: starting vertical offset as a multiple of overlay height;
///   negative = slide in from above — e.g. docked dropdown
///   `slideIn(initialOffset = { IntOffset(0, -it.height / 2) })`;
///   default 0.0 = no offset
/// - `reveal_top`: reveal from the top (clip height 0 → full — approximates the
///   fullscreen search bounds-morph expand; true shared-element morph needs
///   anchor geometry, out of scope for this mechanism; default false)
/// - `duration`: duration (default 200ms)
/// - `interpolator`: easing curve (default EaseOutCubic — Compose `easeOut`)
///
/// `None` (`OverlayDesc.enter_anim / exit_anim = None`) = no animation (instant
/// appear / disappear — default for menu-like popups).
#[derive(Clone)]
pub struct OverlayAnimSpec {
    pub(crate) scale_from: f32,
    pub(crate) fade: bool,
    pub(crate) slide_from_y: f32,
    pub(crate) reveal_top: bool,
    pub(crate) duration: std::time::Duration,
    pub(crate) delay: std::time::Duration,
    pub(crate) interpolator: std::sync::Arc<dyn crate::animation::interpolator::Interpolator>,
    pub(crate) anchor_pivot: bool,
}

impl OverlayAnimSpec {
    pub(crate) fn animation_spec(&self) -> crate::animation::AnimationSpec {
        let delay_ms = self.delay.as_millis() as u64;
        if delay_ms == 0 {
            return crate::animation::AnimationSpec::Tween(crate::animation::TweenSpec::new(
                self.duration,
                self.interpolator.clone(),
            ));
        }
        let duration_ms = self.duration.as_millis() as u64;
        let total = (duration_ms + delay_ms) as f32;
        let held = delay_ms as f32 / total;
        // Dense sampling with LINEAR segments: a curved segment interpolator replays the curve once per
        // segment (the sawtooth measured on the SearchBar expansion).
        let steps = 96usize;
        let mut frames: Vec<(f32, f32)> = Vec::with_capacity(steps + 2);
        frames.push((0.0, 0.0));
        frames.push((held, 0.0));
        for i in 1..=steps {
            let t = i as f32 / steps as f32;
            frames.push((held + (1.0 - held) * t, self.interpolator.interpolate(t)));
        }
        crate::animation::AnimationSpec::Keyframes(crate::animation::KeyframesSpec::new(
            std::time::Duration::from_millis(duration_ms + delay_ms),
            frames,
        ))
    }
    /// Default enter animation (scale 0.8 -> 1 + fade, 200ms EaseOutCubic — the
    /// classic material2 Dialog open effect).
    pub fn default_enter() -> Self {
        Self {
            scale_from: 0.8,
            fade: true,
            slide_from_y: 0.0,
            reveal_top: false,
            duration: std::time::Duration::from_millis(200),
            delay: std::time::Duration::ZERO,
            interpolator: std::sync::Arc::new(crate::animation::interpolator::EaseOutCubic::new()),
            anchor_pivot: false,
        }
    }

    /// Default exit animation (scale 1 -> 0.8 + fade out, 200ms EaseInCubic —
    /// reverse of enter; mirrors Compose `AnimatedVisibility(exit = scaleOut + fadeOut)`).
    pub fn default_exit() -> Self {
        Self {
            scale_from: 0.8,
            fade: true,
            slide_from_y: 0.0,
            reveal_top: false,
            duration: std::time::Duration::from_millis(200),
            delay: std::time::Duration::ZERO,
            interpolator: std::sync::Arc::new(crate::animation::interpolator::EaseInCubic::new()),
            anchor_pivot: false,
        }
    }

    /// Scale only (no fade).
    pub fn scale_only(from: f32, duration: std::time::Duration) -> Self {
        Self { scale_from: from, fade: false, slide_from_y: 0.0, reveal_top: false, duration, delay: std::time::Duration::ZERO, interpolator: std::sync::Arc::new(crate::animation::interpolator::EaseOutCubic::new()), anchor_pivot: false }
    }

    /// Fade only.
    pub fn fade_only(duration: std::time::Duration) -> Self {
        Self { scale_from: 1.0, fade: true, slide_from_y: 0.0, reveal_top: false, duration, delay: std::time::Duration::ZERO, interpolator: std::sync::Arc::new(crate::animation::interpolator::EaseOutCubic::new()), anchor_pivot: false }
    }

    /// Dropdown slide + fade (slide_in y=-height/2 with fade — mirrors docked
    /// dropdown `slideIn(-height/2) + fadeIn`).
    pub fn slide_down(duration: std::time::Duration) -> Self {
        Self { scale_from: 1.0, fade: true, slide_from_y: -0.5, reveal_top: false, duration, delay: std::time::Duration::ZERO, interpolator: std::sync::Arc::new(crate::animation::interpolator::EaseOutCubic::new()), anchor_pivot: false }
    }

    /// Hold the start value for `delay` before the animation runs (Compose's `delayMillis`). See the field
    /// doc: the overlay layer turns this into a hold-then-curve spec, so it must NOT be folded into
    /// `duration` by the caller.
    pub fn delay(mut self, d: std::time::Duration) -> Self {
        self.delay = d;
        self
    }

    /// Custom easing curve.
    pub fn with_interpolator(mut self, interp: impl crate::animation::interpolator::Interpolator + 'static) -> Self {
        self.interpolator = std::sync::Arc::new(interp);
        self
    }

    /// Starting scale (close to 0 = dialog grows from center; 1.0 = no scale).
    pub fn scale_from(mut self, v: f32) -> Self {
        self.scale_from = v;
        self
    }

    /// Starting vertical offset as a multiple of overlay height (-0.5 = slide
    /// in from half height above).
    pub fn slide_from_y(mut self, v: f32) -> Self {
        self.slide_from_y = v;
        self
    }

    /// Reveal from top (clip height 0 -> full).
    pub fn reveal_top(mut self, v: bool) -> Self {
        self.reveal_top = v;
        self
    }

    /// Fade toggle.
    pub fn fade(mut self, v: bool) -> Self {
        self.fade = v;
        self
    }

    /// Duration.
    pub fn duration(mut self, d: std::time::Duration) -> Self {
        self.duration = d;
        self
    }

    /// Expand reveal + fade (approximates the fullscreen search bounds-morph
    /// expand — true shared-element morph needs anchor geometry; 300-400ms
    /// with EaseOutCubic lands crisply).
    pub fn expand_fade(duration: std::time::Duration) -> Self {
        Self { scale_from: 1.0, fade: true, slide_from_y: 0.0, reveal_top: true, duration, delay: std::time::Duration::ZERO, interpolator: std::sync::Arc::new(crate::animation::interpolator::EaseOutCubic::new()), anchor_pivot: false }
    }

    pub(crate) fn apply(&self, t: f32) -> (f32, f32, f32, f32) {
        let e = self.interpolator.interpolate(t.clamp(0.0, 1.0));
        let scale = self.scale_from + (1.0 - self.scale_from) * e;
        let alpha = if self.fade { e } else { 1.0 };
        let dy = self.slide_from_y * (1.0 - e);
        let reveal = if self.reveal_top { e } else { 1.0 };
        (scale, alpha, dy, reveal)
    }

    pub(crate) fn apply_exit(&self, t: f32) -> (f32, f32, f32, f32) {
        self.apply(t)
    }
}

impl Default for OverlayAnimSpec {
    fn default() -> Self { Self::default_enter() }
}

/// Overlay descriptor — registered during composition (e.g. `Popup::build`
/// calls `ctx.open_overlay` internally).
pub struct OverlayDesc {
    pub(crate) id: u64,
    pub(crate) anchor_slot: Option<u64>,
    pub(crate) position: PopupPosition,
    pub(crate) offset: (f32, f32),
    pub(crate) anchor_slide: Option<AnchorSlide>,
    pub(crate) modal: bool,
    pub(crate) focus_scope: bool,
    pub(crate) dismiss_on_outside: bool,
    pub(crate) dismiss_on_back_press: bool,
    pub(crate) click_passthrough: bool,
    pub(crate) on_dismiss: Option<Arc<dyn Fn() + Send + Sync>>,
    pub(crate) fit_around_anchor: bool,
    pub(crate) match_anchor_width: bool,
    pub(crate) enter_anim: Option<OverlayAnimSpec>,
    pub(crate) exit_anim: Option<OverlayAnimSpec>,
    pub(crate) content: Box<dyn Fn(&mut crate::runtime::composer::ComposeCtx)>,
    pub(crate) local_snapshot: crate::runtime::composition_local::LocalSnapshot,
}

pub(crate) fn next_overlay_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

// ═══════════════ Popup ═══════════════

/// Non-modal popup (mirrors Compose `Popup`) — positioned relative to an anchor
/// or the window; an outside click triggers `on_dismiss_request`. The anchor is
/// the previous sibling at the call site (e.g. the trigger button — popup
/// content follows it); falls back to window alignment when there is no sibling.
///
/// ```ignore
/// Popup::new()
///     .position(PopupPosition::BottomLeft)
///     .on_dismiss_request(|| show.set(false))
///     .build(ctx, |ctx| { /* 弹出内容 */ });
/// ```
pub struct Popup {
    visible: bool,
    position: PopupPosition,
    offset: (f32, f32),
    on_dismiss: Option<Arc<dyn Fn() + Send + Sync>>,
    dismiss_on_outside: bool,
    anchor_slot: Option<u64>,
    enter_anim: Option<OverlayAnimSpec>,
    exit_anim: Option<OverlayAnimSpec>,
}

impl Popup {
    /// `visible` is parameterized (mirrors `DropdownMenu::new(expanded)`): `build`
    /// **always executes** and records the active state — `sync_overlays` deletes
    /// the overlay on `active=false` (explicit close) vs retaining it when there
    /// is no record this frame (owner Skipped). Wrapping the call site in `if`
    /// (so `build` does not execute) makes Skip vs explicit close
    /// indistinguishable at the slot layer and breaks deletion/retention.
    pub fn new(visible: bool) -> Self {
        Self {
            visible,
            position: PopupPosition::BottomLeft,
            offset: (0.0, 4.0),
            on_dismiss: None,
            dismiss_on_outside: true,
            anchor_slot: None,
            enter_anim: None,
            exit_anim: None,
        }
    }

    pub fn position(mut self, p: PopupPosition) -> Self {
        self.position = p;
        self
    }

    pub fn offset(mut self, x: f32, y: f32) -> Self {
        self.offset = (x, y);
        self
    }

    pub fn on_dismiss_request(mut self, cb: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_dismiss = Some(Arc::new(cb));
        self
    }

    /// Whether a press OUTSIDE the popup dismisses it — `true` by default, like Compose's
    /// `PopupProperties.dismissOnClickOutside`.
    ///
    /// With `true` that press is consumed by the popup (it closes instead of reaching whatever it
    /// landed on), so a popup that should stay open while the rest of the window is used — a
    /// palette, a floating panel — passes `false` and the press falls through.
    pub fn dismiss_on_outside(mut self, v: bool) -> Self {
        self.dismiss_on_outside = v;
        self
    }

    /// Enter animation (default None = instant appear for menu-like semantics;
    /// dropdowns use `slide_down`).
    pub fn enter_animation(mut self, anim: Option<OverlayAnimSpec>) -> Self {
        self.enter_anim = anim;
        self
    }

    /// Exit animation (default None = instant disappear).
    pub fn exit_animation(mut self, anim: Option<OverlayAnimSpec>) -> Self {
        self.exit_anim = anim;
        self
    }

    /// Explicit anchor slot override. Needed because `#[composable]` pushes a
    /// fresh scope at `build` entry, so the default `prev_sibling_slot_key()`
    /// capture inside `build` always sees an empty scope (None) and the popup
    /// falls back to window alignment. Callers that need anchoring must capture
    /// `ctx.prev_sibling_slot_key()` in their own scope and pass it here:
    /// ```ignore
    /// Surface::new().build(ctx, |ctx| { /* anchor */ });
    /// let anchor = ctx.prev_sibling_slot_key();
    /// Popup::new(true).anchor_slot(anchor).build(ctx, |ctx| { /* ... */ });
    /// ```
    pub fn anchor_slot(mut self, slot: Option<u64>) -> Self {
        self.anchor_slot = slot;
        self
    }

    /// `#[composable]`: `remember` (overlay id) is keyed from the call site.
    /// `build` always executes (even when `visible=false`) — records
    /// `active=false` for `sync` to delete.
    #[composable]
    pub fn build(self, ctx: &mut crate::runtime::composer::ComposeCtx, content: impl Fn(&mut crate::runtime::composer::ComposeCtx) + 'static) {
        let id = ctx.remember(|| next_overlay_id());
        ctx.record_overlay_active(id.get(), self.visible);
        if !self.visible {
            return; // Closed: do not register an overlay — `sync` deletes on `active=false`.
        }
        // Explicit anchor wins; otherwise fall back to prev-sibling capture
        // (None inside build scope — kept for non-composable callers).
        let anchor = self.anchor_slot.or_else(|| ctx.prev_sibling_slot_key());
        ctx.open_overlay(crate::overlay::OverlayDesc {
            id: id.get(),
            // Anchor = caller-supplied (see `anchor_slot`); default = last sibling
            // in the current scope — always None inside build's own scope ->
            // window alignment.
            anchor_slot: anchor,
            position: self.position,
            offset: self.offset,
            anchor_slide: None,
            modal: false,
            // A popup takes the keyboard only once something inside it was clicked (`keyboard_scope`
            // keeps it there while it lasts); it does not grab focus the moment it floats up.
            focus_scope: false,
            dismiss_on_outside: self.dismiss_on_outside,
            dismiss_on_back_press: true,
            click_passthrough: false,
            // A popup keeps winia's historic placement (below the anchor, no fitting).
            fit_around_anchor: false,
            match_anchor_width: false,
            on_dismiss: self.on_dismiss,
            enter_anim: self.enter_anim,
            exit_anim: self.exit_anim,
            content: Box::new(content),
            local_snapshot: Vec::new(),
        });
    }
}

impl Default for Popup { fn default() -> Self { Self::new(false) } }

// ═══════════════ Dialog ═══════════════

/// Modal dialog (mirrors Compose `Dialog`) — centered with a scrim; clicking
/// the scrim triggers `on_dismiss_request`.
pub struct Dialog {
    visible: bool,
    on_dismiss: Option<Arc<dyn Fn() + Send + Sync>>,
    dismiss_on_outside: bool,
    enter_anim: Option<OverlayAnimSpec>,
    exit_anim: Option<OverlayAnimSpec>,
    position: PopupPosition,
    offset: (f32, f32),
    anchor_slot: Option<u64>,
    anchor_slide: Option<AnchorSlide>,
}

impl Dialog {
    /// `visible` is parameterized (same as `Popup`): `build` always executes and
    /// records active — `sync` deletes on `active=false` (explicit close) vs no
    /// record (owner Skipped).
    pub fn new(visible: bool) -> Self {
        Self {
            visible,
            on_dismiss: None,
            dismiss_on_outside: true,
            // Default enter/exit (scale 0.8 -> 1 + fade — classic material2 Dialog;
            // exit plays in reverse).
            enter_anim: Some(OverlayAnimSpec::default_enter()),
            exit_anim: Some(OverlayAnimSpec::default_exit()),            position: PopupPosition::Center,
            offset: (0.0, 0.0),
            anchor_slot: None,
            anchor_slide: None,
        }
    }

    /// Where the panel sits (default `Center`, the classic modal Dialog). A component that grows out of an
    /// anchor passes `TopLeft` plus [`Self::offset`] so the panel starts AT the anchor instead of the middle
    /// of the window — `Center` positions the overlay's own current size, so a small panel mid-animation
    /// lands in the middle of the screen.
    pub fn position(mut self, p: PopupPosition) -> Self {
        self.position = p;
        self
    }

    /// Pixel offset applied after [`Self::position`].
    pub fn offset(mut self, x: f32, y: f32) -> Self {
        self.offset = (x, y);
        self
    }

    /// Anchor the panel to a node of the MAIN tree (its measured position and size), like `Popup` does.
    /// Together with `position(TopLeft)` this is how a panel starts AT a bar instead of at the window's
    /// corner.
    pub fn anchor_slot(mut self, slot: Option<u64>) -> Self {
        self.anchor_slot = slot;
        self
    }
    /// Grow the panel out of its anchor: both axes run `lerp(anchor.<axis>, 0, progress)` as `progress`
    /// goes 0 -> 1, so the panel starts at the anchor's corner and ends at the window's (see [`AnchorSlide`]).
    pub fn anchor_slide(mut self, slide: AnchorSlide) -> Self {
        self.anchor_slide = Some(slide);
        self
    }

    pub fn on_dismiss_request(mut self, cb: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_dismiss = Some(Arc::new(cb));
        self
    }

    /// Whether clicking the scrim dismisses (default true — Compose Dialog
    /// `dismissOnClickOutside=true`).
    pub fn dismiss_on_outside(mut self, v: bool) -> Self {
        self.dismiss_on_outside = v;
        self
    }

    /// Custom **enter** animation (default scale 0.8 -> 1 + fade 200ms
    /// EaseOutCubic). Pass `None` for instant appear. Mirrors Compose
    /// content-layer `AnimatedVisibility(enter = scaleIn(...) + fadeIn(...))` —
    /// Winia pushes the animation down to the overlay container layer.
    pub fn enter_animation(mut self, anim: Option<OverlayAnimSpec>) -> Self {
        self.enter_anim = anim;
        self
    }

    /// Custom **exit** animation (default is the reverse of enter: scale 1 ->
    /// 0.8 + fade out 200ms EaseInCubic). Pass `None` for instant disappear.
    /// Mirrors Compose `AnimatedVisibility(exit = scaleOut(...) + fadeOut(...))`.
    pub fn exit_animation(mut self, anim: Option<OverlayAnimSpec>) -> Self {
        self.exit_anim = anim;
        self
    }

    /// Disable enter/exit animations (instant appear / disappear).
    pub fn no_animation(self) -> Self {
        self.enter_animation(None).exit_animation(None)
    }

    /// `#[composable]`: `remember` (overlay id) is keyed from the call site.
    /// `build` always executes (even when `visible=false`) — records
    /// `active=false` for `sync` to delete.
    #[composable]
    pub fn build(self, ctx: &mut crate::runtime::composer::ComposeCtx, content: impl Fn(&mut crate::runtime::composer::ComposeCtx) + 'static) {
        let id = ctx.remember(|| next_overlay_id());
        ctx.record_overlay_active(id.get(), self.visible);
        if !self.visible {
            return; // Closed: do not register an overlay — `sync` deletes on `active=false`.
        }
        ctx.open_overlay(crate::overlay::OverlayDesc {
            id: id.get(),
            anchor_slot: self.anchor_slot,
            position: self.position,
            offset: self.offset,
            anchor_slide: self.anchor_slide,
            modal: true,
            // A dialog owns the keyboard while it is up (see `OverlayDesc::focus_scope`).
            focus_scope: true,
            dismiss_on_outside: self.dismiss_on_outside,
            dismiss_on_back_press: true,
            click_passthrough: false,
            // A dialog is centred (`PopupPosition::Center`), so there is nothing to fit around.
            fit_around_anchor: false,
            match_anchor_width: false,
            on_dismiss: self.on_dismiss,
            enter_anim: self.enter_anim,
            exit_anim: self.exit_anim,
            content: Box::new(content),
            local_snapshot: Vec::new(),
        });
    }
}

impl Default for Dialog { fn default() -> Self { Self::new(false) } }

// ═══════════════ DropdownMenu ═══════════════

#[cfg(test)]
mod tests {
    use super::*;

    /// A menu item must carry the ripple AND the "no focus ring" marker.
    ///
    /// material3's item is `clickable(..., indication = ripple(true))` and marks focus with a state layer,
    /// not with an outline. winia draws a focus ring around any focused node by default (`render.rs`, gated
    /// on `ModifierElement::NoFocusRing`), which across a menu reads as a divider between rows — so the item
    /// opts out, and the highlight it keeps comes from the ripple element's state layer (`hover + focus`).
    ///
    /// Teeth: dropping `no_focus_ring()` from the item fails the second assertion. That one is structural,
    /// not pixel-based, on purpose: the ring is a ~1px band just OUTSIDE the item's rect (measured:
    /// physical x=23 against the item's edge at 24 is `(197,193,199)` while everything inside is the
    /// `(217,211,219)` state layer), so a pixel probe of it is a rounding artefact away from lying.

    /// The reveal clip height must keep "settled, no clip" distinct from "clipped to zero height".
    ///
    /// Teeth: returning `Some(0.0)` instead of `None` at `reveal == 1.0` (the mistake this function was
    /// extracted from — a fully open docked dropdown reported `painted_h=0` in the render trace while its
    /// layout height was 280) fails the settled case below.
    #[test]
    fn reveal_clip_height_distinguishes_no_clip_from_zero() {
        // Settled: fully revealed and not sliding -> NO clip.
        assert_eq!(
            overlay_reveal_clip_height(280.0, 0.0, 1.0),
            None,
            "a settled overlay is not clipped at all (0 here would read as an invisible panel)"
        );
        // Revealing: proportional height.
        assert_eq!(overlay_reveal_clip_height(280.0, 0.0, 0.5), Some(140.0));
        // Just started: effectively zero height, but it is a real clip.
        assert_eq!(overlay_reveal_clip_height(280.0, 0.0, 0.0), Some(0.0));
        // Sliding: the translation moves the panel, so the clip is the FULL height regardless of reveal.
        assert_eq!(
            overlay_reveal_clip_height(280.0, -0.5, 0.2),
            Some(280.0),
            "a slide clips to its full height (the offset, not a reveal, is what animates)"
        );
        // Overshoot (a spring past 1.0) must not produce a clip taller than the panel.
        assert_eq!(overlay_reveal_clip_height(280.0, 0.0, 1.2), None);
    }

    /// A panel that grows out of an anchor runs `lerp(anchor, 0, progress)` on BOTH axes, so it starts at the
    /// anchor's corner and reaches the window's corner exactly when the animation finishes.
    ///
    /// Teeth: dropping the second axis (the first version slid only Y) or using the wrong endpoint fails the
    /// assertions below — the earlier Y-only version left a permanent gap equal to the anchor's own top.
    #[test]
    fn anchor_slide_interpolates_both_axes_from_the_anchor_to_the_window_corner() {
        // An anchor 240 wide at x=40, and 56 tall at y=69 (the demo's bar).
        let (ax, ay) = (40.0f32, 69.0f32);

        // progress 0: exactly at the anchor.
        assert_eq!(anchor_slide_lerp(ax, 0.0), ax, "starts at the anchor's x");
        assert_eq!(anchor_slide_lerp(ay, 0.0), ay, "starts at the anchor's y");

        // progress 1: at the window corner, on BOTH axes.
        assert_eq!(anchor_slide_lerp(ax, 1.0), 0.0, "ends at the window's left edge");
        assert_eq!(anchor_slide_lerp(ay, 1.0), 0.0, "ends at the window's top edge");

        // Half way on each axis, and the Y axis really does move (the earlier Y-only version is what this
        // catches: it must not stay at the anchor's top).
        assert_eq!(anchor_slide_lerp(ax, 0.5), ax / 2.0, "half way in x");
        assert_eq!(anchor_slide_lerp(ay, 0.5), ay / 2.0, "half way in y");

        // Out-of-range progress is clamped, so a spring overshoot cannot push the panel past the window.
        assert_eq!(anchor_slide_lerp(ay, 1.5), 0.0, "overshoot clamps to the window edge");
        assert_eq!(anchor_slide_lerp(ay, -0.5), ay, "undershoot clamps to the anchor");
    }

    /// `AnchorSlide` exposes the clamped progress the layout pass interpolates with.
    #[test]
    fn anchor_slide_clamps_its_progress() {
        let over = AnchorSlide::new(std::sync::Arc::new(|| 1.4));
        assert_eq!(over.progress(), 1.0);
        let under = AnchorSlide::new(std::sync::Arc::new(|| -0.2));
        assert_eq!(under.progress(), 0.0);
        let mid = AnchorSlide::new(std::sync::Arc::new(|| 0.25));
        assert_eq!(mid.progress(), 0.25);
    }

}
