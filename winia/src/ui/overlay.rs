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
/// The two endpoints are resolved by [`anchor_slide_origin`].
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
    /// Duration of the ANIMATED part (not counting `delay`).
    pub(crate) duration: std::time::Duration,
    /// How long the overlay holds its start value before the animation begins (Compose's `delayMillis`).
    ///
    /// A plain `TweenSpec` has no delay, so this is honoured by folding the hold into the spec the overlay
    /// layer runs (see `OverlayAnimSpec::animation_spec`) instead of being added to `duration` by callers —
    /// adding it to the duration makes the overlay START MOVING during the delay, which is not what Compose
    /// means by `delayMillis`.
    pub(crate) delay: std::time::Duration,
    pub(crate) interpolator: std::sync::Arc<dyn crate::animation::interpolator::Interpolator>,
    /// Scale around the menu's own centre (what every overlay did before this existed) or around the
    /// point material3's menus grow from — see [`overlay_transform_origin`], which is
    /// `calculateTransformOrigin(anchorBounds, menuBounds)` from `material3/Menu.kt`.
    pub(crate) anchor_pivot: bool,
}

impl OverlayAnimSpec {
    /// The spec the overlay layer runs for this animation, with `delay` expressed as a hold followed by the
    /// real curve — the same construction the SearchBar panel uses (`delayed_tween`).
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

    /// Animation progress (0..=1) -> (scale, alpha, dy, reveal) — called per
    /// frame at render time without State dependencies; t=0 start, t=1 end
    /// (Compose semantics: t is the animation progress); dy is a multiple of the
    /// overlay height (slide_from_y interpolation — render side multiplies by
    /// content height to pixels); reveal is the revealed-height fraction (1.0 =
    /// full height, no clip).
    pub(crate) fn apply(&self, t: f32) -> (f32, f32, f32, f32) {
        let e = self.interpolator.interpolate(t.clamp(0.0, 1.0));
        let scale = self.scale_from + (1.0 - self.scale_from) * e;
        let alpha = if self.fade { e } else { 1.0 };
        let dy = self.slide_from_y * (1.0 - e);
        let reveal = if self.reveal_top { e } else { 1.0 };
        (scale, alpha, dy, reveal)
    }

    /// Exit animation progress (1 -> 0 reverse) -> (scale, alpha, dy, reveal) —
    /// t=1 fully shown, t=0 hidden: scale 1 -> scale_from, alpha 1 -> 0, dy 0 ->
    /// slide_from_y, reveal 1 -> 0. Same formula as `apply()` — driving progress
    /// from 1 to 0 naturally reverses it.
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
    /// Stable id (generated via `remember` inside the component — reused across
    /// frames to match the dedicated Composer).
    pub(crate) id: u64,
    /// Anchor node slot key (None = window-aligned).
    pub(crate) anchor_slot: Option<u64>,
    /// Position relative to the anchor / window.
    pub(crate) position: PopupPosition,
    /// Offset after positioning (logical pixels).
    pub(crate) offset: (f32, f32),
    /// Grow the panel out of its anchor: both axes run `lerp(anchor.<axis>, 0, progress)`, so the panel
    /// starts at the anchor's corner and ends at the window's (see [`AnchorSlide`]). The anchor's
    /// coordinates are known only to the layout pass, so the interpolation happens there and the caller
    /// supplies just the progress reader (read per frame with no recomposition cost).
    pub(crate) anchor_slide: Option<AnchorSlide>,
    /// Modal (Dialog): draws a scrim and captures outside clicks for dismiss.
    pub(crate) modal: bool,
    /// Whether this overlay owns the KEYBOARD while it is open: Tab and the arrow keys move focus
    /// inside its own arena instead of the window's main tree, and the page behind it is out of reach
    /// until it closes. The modal components (`Dialog`, `AlertDialog`, `ModalBottomSheet`) set it.
    ///
    /// Independent of [`Self::modal`] on purpose: `modal` is about the scrim and outside clicks, this
    /// is about focus. A popup that merely floats over the page (a menu, a tooltip) leaves it false —
    /// such an overlay takes the keyboard only once something inside it was clicked, the behaviour it
    /// has always had.
    ///
    /// The topmost declaring overlay wins (the ones below are covered by it), and one that declares
    /// itself a focus scope but has nothing focusable is skipped: Tab keeps working instead of being
    /// swallowed by a layer that has nowhere to put focus.
    pub(crate) focus_scope: bool,
    /// Whether an outside click triggers `on_dismiss_request` (non-modal Popup
    /// default true).
    pub(crate) dismiss_on_outside: bool,
    /// When overlay content is hit, **pass through to the main tree** without
    /// consuming the event — used by Tooltip: when a tooltip covers its anchor,
    /// clicking the anchor must still work (otherwise the tooltip blocks the
    /// button and cannot be dismissed).
    pub(crate) click_passthrough: bool,
    /// Outside-click callback.
    pub(crate) on_dismiss: Option<Arc<dyn Fn() + Send + Sync>>,
    /// Fit this overlay INSIDE the window around its anchor, the way material3's
    /// `DropdownMenuPositionProvider` places a menu: below the anchor if it fits, else above it, else
    /// pinned against the window edge (and the same three candidates horizontally).
    ///
    /// Off by default, so every overlay that predates it keeps its exact placement; a long menu opts in,
    /// because without it a menu taller than the space below its anchor hangs off the window edge with
    /// its bottom rows unreachable (measured: container at y=238 and 520 tall in a 520px window, bottom
    /// at 758 — `dropdown_menu_a_long_menu_fits_the_window_and_scrolls_to_its_end`).
    pub(crate) fit_around_anchor: bool,
    /// Enter animation spec (None = instant appear — Popup/DropdownMenu default;
    /// Some = container-layer frame-driven animation — Dialog default).
    pub(crate) enter_anim: Option<OverlayAnimSpec>,
    /// Exit animation spec (None = instant disappear; Some = reverse playback on
    /// close — Dialog defaults to the symmetric counterpart of enter; mirrors
    /// Compose `AnimatedVisibility(exit = ...)`).
    pub(crate) exit_anim: Option<OverlayAnimSpec>,
    /// Overlay content (independent composition unit).
    pub(crate) content: Box<dyn Fn(&mut crate::core::composer::ComposeCtx)>,
    /// CompositionLocal snapshot captured at registration time (inside the main
    /// tree's `provides`) — replayed when the overlay's dedicated Composer
    /// recomposes, so `WiniaTheme::colors()` etc. inherit the main tree's theme.
    /// Filled automatically by [`crate::core::composer::ComposeCtx::open_overlay`].
    pub(crate) local_snapshot: crate::core::composition_local::LocalSnapshot,
}

/// Allocate a top-level overlay id (used via `remember` during composition —
/// stable across frames).
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
    /// Whether a press outside the popup dismisses it (default true, like Compose's
    /// `PopupProperties.dismissOnClickOutside`). See [`Popup::dismiss_on_outside`].
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
    pub fn build(self, ctx: &mut crate::core::composer::ComposeCtx, content: impl Fn(&mut crate::core::composer::ComposeCtx) + 'static) {
        let id = ctx.remember(|| next_overlay_id());
        ctx.record_overlay_active(id.get(), self.visible);
        if !self.visible {
            return; // Closed: do not register an overlay — `sync` deletes on `active=false`.
        }
        // Explicit anchor wins; otherwise fall back to prev-sibling capture
        // (None inside build scope — kept for non-composable callers).
        let anchor = self.anchor_slot.or_else(|| ctx.prev_sibling_slot_key());
        ctx.open_overlay(crate::ui::overlay::OverlayDesc {
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
            click_passthrough: false,
            // A popup keeps winia's historic placement (below the anchor, no fitting).
            fit_around_anchor: false,
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
    /// Where the panel sits (`Center` by default — the classic modal Dialog). A component growing out of an
    /// anchor passes `TopLeft` + `offset`, because `Center` positions the overlay's CURRENT size and would
    /// put a small mid-animation panel in the middle of the window.
    position: PopupPosition,
    offset: (f32, f32),
    /// Optional main-tree anchor node (see [`Self::anchor_slot`]).
    anchor_slot: Option<u64>,
    /// See [`OverlayDesc::anchor_slide`].
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
    pub fn build(self, ctx: &mut crate::core::composer::ComposeCtx, content: impl Fn(&mut crate::core::composer::ComposeCtx) + 'static) {
        let id = ctx.remember(|| next_overlay_id());
        ctx.record_overlay_active(id.get(), self.visible);
        if !self.visible {
            return; // Closed: do not register an overlay — `sync` deletes on `active=false`.
        }
        ctx.open_overlay(crate::ui::overlay::OverlayDesc {
            id: id.get(),
            anchor_slot: self.anchor_slot,
            position: self.position,
            offset: self.offset,
            anchor_slide: self.anchor_slide,
            modal: true,
            // A dialog owns the keyboard while it is up (see `OverlayDesc::focus_scope`).
            focus_scope: true,
            dismiss_on_outside: self.dismiss_on_outside,
            click_passthrough: false,
            // A dialog is centred (`PopupPosition::Center`), so there is nothing to fit around.
            fit_around_anchor: false,
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

/// M3 `MenuItemColors`: the foreground roles a menu item resolves by enabled state.
///
/// Mirrors `androidx.compose.material3.MenuItemColors` field for field, and the defaults mirror
/// `ColorScheme.defaultMenuItemColors`: text `onSurface`, icons `onSurfaceVariant`, and the disabled
/// variants are the same roles at `ListItemDisabled*Opacity` (0.38).
#[derive(Clone, PartialEq)]
pub struct MenuItemColors {
    pub text: crate::modifier::Color,
    pub leading_icon: crate::modifier::Color,
    pub trailing_icon: crate::modifier::Color,
    pub disabled_text: crate::modifier::Color,
    pub disabled_leading_icon: crate::modifier::Color,
    pub disabled_trailing_icon: crate::modifier::Color,
}

impl MenuItemColors {
    /// `MenuDefaults.itemColors()` — from the current theme's roles.
    pub fn defaults() -> Self {
        let c = crate::ui::theme::WiniaTheme::colors();
        Self {
            text: c.on_surface,
            leading_icon: c.on_surface_variant,
            trailing_icon: c.on_surface_variant,
            disabled_text: with_alpha_factor(c.on_surface, DISABLED_ALPHA),
            disabled_leading_icon: with_alpha_factor(c.on_surface, DISABLED_ALPHA),
            disabled_trailing_icon: with_alpha_factor(c.on_surface, DISABLED_ALPHA),
        }
    }

    pub fn text_color(&self, enabled: bool) -> crate::modifier::Color {
        if enabled { self.text } else { self.disabled_text }
    }

    pub fn leading_icon_color(&self, enabled: bool) -> crate::modifier::Color {
        if enabled { self.leading_icon } else { self.disabled_leading_icon }
    }

    pub fn trailing_icon_color(&self, enabled: bool) -> crate::modifier::Color {
        if enabled { self.trailing_icon } else { self.disabled_trailing_icon }
    }
}

impl std::fmt::Debug for MenuItemColors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MenuItemColors")
    }
}

/// `ListTokens.ListItemDisabled*Opacity` — the disabled foreground alpha, shared with the navigation
/// components (which keep their own copy of this constant; this is the menu's).
const DISABLED_ALPHA: f32 = 0.38;

fn with_alpha_factor(color: crate::modifier::Color, factor: f32) -> crate::modifier::Color {
    crate::modifier::Color::from_argb(
        ((color.a as f32 * factor).round().min(255.0)) as u8,
        color.r,
        color.g,
        color.b,
    )
}

/// Dropdown menu (mirrors Compose material3 `DropdownMenu`) — anchored to a trigger container;
/// clicking outside dismisses it.
///
/// Every knob mirrors the material3 signature, and so does its default:
///
/// | M3 | winia | default |
/// |---|---|---|
/// | `offset: DpOffset` | [`Self::offset`] | `(0, 0)` |
/// | `shape` | [`Self::shape`] | `MenuTokens.ContainerShape` — CornerExtraSmall (4dp) |
/// | `containerColor` | [`Self::container_color`] | `MenuTokens.ContainerColor` — `surfaceContainer` |
/// | `tonalElevation` | [`Self::tonal_elevation`] | `ElevationTokens.Level0` |
/// | `shadowElevation` | [`Self::shadow_elevation`] | `MenuTokens.ContainerElevation` — Level2 (3dp) |
/// | `border` | [`Self::border`] | `null` |
///
/// ```ignore
/// let expanded = ctx.remember(|| false);
/// DropdownMenu::new(expanded.clone())
///     .build(ctx,
///         |ctx| { Button::new().on_click(|| expanded.set(true)).build(...) },  // 锚点
///         |ctx| {  // 菜单项
///             DropdownMenuItem::new("选项 A").on_click(|| ...).build(ctx);
///         });
/// ```
pub struct DropdownMenu {
    expanded: crate::core::state::State<bool>,
    on_dismiss: Option<Arc<dyn Fn() + Send + Sync>>,
    /// M3 `modifier` — applied to the menu's own container (the surface), so a caller can tag it or
    /// adjust it. Appended outside the internal modifier, like every other component here.
    modifier: crate::modifier::Modifier,
    /// M3 `scrollState` — `rememberScrollState()` when unset. The container scrolls, so a menu longer
    /// than the space it was given stays reachable instead of hanging off the window edge.
    scroll_state: Option<crate::modifier::ScrollState>,
    offset: (f32, f32),
    shape: Option<crate::modifier::Shape>,
    container_color: Option<crate::modifier::Color>,
    tonal_elevation: f32,
    shadow_elevation: Option<f32>,
    border: Option<crate::ui::surface::SurfaceBorder>,
}

impl DropdownMenu {
    pub fn new(expanded: crate::core::state::State<bool>) -> Self {
        Self {
            expanded,
            on_dismiss: None,
            modifier: crate::modifier::Modifier::new(),
            scroll_state: None,
            // M3: `DpOffset(0.dp, 0.dp)`. The drop-down placement itself comes from the anchor
            // (`PopupPosition::BottomLeft`), which is the equivalent of the platform popup's anchoring.
            offset: (0.0, 0.0),
            shape: None,
            container_color: None,
            tonal_elevation: 0.0,
            shadow_elevation: None,
            border: None,
        }
    }

    pub fn on_dismiss_request(mut self, cb: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_dismiss = Some(Arc::new(cb));
        self
    }

    /// M3 `modifier` — applied to the menu's container (its surface). Appended outside the internal
    /// modifier, like every other component here.
    pub fn modifier(mut self, modifier: crate::modifier::Modifier) -> Self {
        self.modifier = modifier;
        self
    }

    /// M3 `scrollState` — the menu's content scrolls through it (`rememberScrollState()` when unset).
    pub fn scroll_state(mut self, state: crate::modifier::ScrollState) -> Self {
        self.scroll_state = Some(state);
        self
    }

    /// M3 `offset: DpOffset` — added to the anchored position (x follows the layout direction there;
    /// winia's anchor is explicit, so x is applied as given).
    pub fn offset(mut self, x: f32, y: f32) -> Self {
        self.offset = (x, y);
        self
    }

    /// M3 `shape` — `MenuDefaults.shape` (CornerExtraSmall, 4dp) when unset.
    pub fn shape(mut self, shape: impl Into<crate::modifier::Shape>) -> Self {
        self.shape = Some(shape.into());
        self
    }

    /// M3 `containerColor` — `MenuDefaults.containerColor` (`surfaceContainer`) when unset.
    pub fn container_color(mut self, color: crate::modifier::Color) -> Self {
        self.container_color = Some(color);
        self
    }

    /// M3 `tonalElevation` — `ElevationTokens.Level0` by default.
    pub fn tonal_elevation(mut self, elevation: f32) -> Self {
        self.tonal_elevation = elevation;
        self
    }

    /// M3 `shadowElevation` — `MenuTokens.ContainerElevation` (Level2, 3dp) when unset.
    pub fn shadow_elevation(mut self, elevation: f32) -> Self {
        self.shadow_elevation = Some(elevation);
        self
    }

    /// M3 `border` — no border by default.
    pub fn border(mut self, border: crate::ui::surface::SurfaceBorder) -> Self {
        self.border = Some(border);
        self
    }

    /// `#[composable]`: `remember` (overlay id) / `next_key` (anchor container)
    /// are keyed from the call site.
    #[composable]
    pub fn build(
        self,
        ctx: &mut crate::core::composer::ComposeCtx,
        anchor: impl FnOnce(&mut crate::core::composer::ComposeCtx),
        menu: impl Fn(&mut crate::core::composer::ComposeCtx) + 'static,
    ) {
        let expanded = self.expanded.get(); // Registers dependency — changes trigger recomposition.
        // Anchor container (regular composition — lives in the main tree; the menu
        // is anchored to its position).
        let anchor_key = ctx.next_key();
        let modifier = crate::modifier::Modifier::new();
        let id = ctx.remember(|| next_overlay_id());
        match ctx.start_restartable_group(anchor_key, modifier, crate::layout::box_layout::BoxLayout::new()) {
            crate::core::composer::GroupStatus::Skip => {}
            crate::core::composer::GroupStatus::Enter => {
                anchor(ctx);
            }
        }
        let anchor_slot = ctx.composer_slot_key(); // Container slot key (anchor).
        ctx.end_restartable_group();

        // M3's default is `rememberScrollState()`, i.e. a state the menu owns across frames. Remembered
        // HERE (not inside the menu's content closure) so it keeps one composition position whether or
        // not the menu is open — a conditional `remember` is what shifts the slots after it.
        let scroll_state = match self.scroll_state.clone() {
            Some(s) => s,
            None => ctx.remember(|| crate::modifier::ScrollState::new()).get(),
        };

        // `build` always executes (parameterized by `expanded`) — records active
        // for `sync` to delete (mirrors Popup/Dialog's `visible` parameterization:
        // `expanded=false` records `false` -> delete).
        ctx.record_overlay_active(id.get(), expanded);
        if expanded {
            let shape = self.shape.unwrap_or(crate::modifier::Shape::RoundedRect { corner_radius: 4.0 });
            let container_color = self
                .container_color
                .unwrap_or_else(|| crate::ui::theme::WiniaTheme::colors().surface_container);
            let shadow_elevation = self.shadow_elevation.unwrap_or(3.0); // ElevationTokens.Level2
            let tonal_elevation = self.tonal_elevation;
            let border = self.border;
            let menu_modifier = self.modifier;
            // material3's menu animation targets: scale 0.8 -> 1 with a fade, growing out of the anchor.
            let mut enter_anim = OverlayAnimSpec::default_enter();
            enter_anim.anchor_pivot = true;
            let mut exit_anim = OverlayAnimSpec::default_exit();
            exit_anim.anchor_pivot = true;
            ctx.open_overlay(crate::ui::overlay::OverlayDesc {
                id: id.get(),
                anchor_slot: Some(anchor_slot),
                position: PopupPosition::BottomLeft,
                offset: self.offset,
                anchor_slide: None,
                modal: false,
                // material3's `DefaultMenuProperties = PopupProperties(focusable = true)`
                // (`androidMain/AndroidMenu.android.kt`): a menu's popup owns the keyboard while it is up,
                // so Tab moves within the menu instead of walking the page behind it — and a focused item
                // activates on Enter/Space through the key dispatcher's focused-node path
                // (`app.rs`: "聚焦组件的键盘激活（对标 Compose clickable）"). Esc dismissal does not
                // depend on this: it was already working, and is pinned by a test either way.
                focus_scope: true,
                dismiss_on_outside: true,
                click_passthrough: false,
                // material3's `DropdownMenuPositionProvider`: a menu fits itself around the anchor, so a
                // menu taller than the space below it ends up above or against the window edge instead of
                // hanging off the bottom.
                fit_around_anchor: true,
                on_dismiss: self.on_dismiss,
                // material3's menu open/close animation (`Menu.kt`'s `DropdownMenuContent`): a transition
                // on `expandedState` driving `graphicsLayer { scaleX/scaleY/alpha }` from
                // `ClosedScaleTarget = 0.8f` / `ClosedAlphaTarget = 0f` to `ExpandedScaleTarget = 1f` /
                // `ExpandedAlphaTarget = 1f`, with `transformOrigin =
                // calculateTransformOrigin(anchorBounds, menuBounds)` — so the menu grows out of its
                // anchor. `OverlayAnimSpec::default_enter/exit` already carries 0.8 + fade, which is
                // exactly those targets; `anchor_pivot` is the transform origin.
                //
                // Not material3's: the duration and curve. M3 reads them from `MotionSchemeKeyTokens.
                // FastSpatial` (scale) and `FastEffects` (alpha), whose values live in the motion scheme
                // and are NOT in the extracted sources here, so this keeps winia's 200ms
                // ease-out-in (Dialog's enters with the same spec) rather than guessing at numbers.
                enter_anim: Some(enter_anim),
                exit_anim: Some(exit_anim),
                // M3's content is `@Composable ColumnScope.() -> Unit`: the items live in a COLUMN that
                // the menu owns, inside the menu's own surface (shape/container/elevation), with
                // `DropdownMenuVerticalPadding` (8dp) above and below. winia has no `ColumnScope`
                // receiver, so the menu wraps the content itself — and the wrapper is load-bearing, not
                // cosmetic: composed as top-level siblings the items collapse to the last one, because a
                // composition's root is a single node (`materialize`: "wrap in a container, or keep
                // emitting siblings, which is its own round"). Measured with three items in a bare
                // composer: arena_len=6 and the root was the LAST item (9x12) — the first two were gone,
                // and the same thing showed up in `overlay_demo` and in the UI fixture's tree.
                content: Box::new(move |ctx| {
                    let mut surface = crate::ui::surface::Surface::new()
                        .shape(shape)
                        .color(container_color)
                        .tonal_elevation(tonal_elevation)
                        .shadow_elevation(shadow_elevation);
                    if let Some(b) = border {
                        surface = surface.border(b);
                    }
                    surface.build(ctx, |ctx| {
                        // M3's chain, in its order: the caller's modifier, then
                        // `padding(vertical = DropdownMenuVerticalPadding)` (8dp, and OUTSIDE the scroll,
                        // so it stays put while the content moves), then `verticalScroll(scrollState)`.
                        // The scroll is what keeps a menu taller than its space reachable: a winia scroll
                        // container measures `min(its content, the viewport it was given)`.
                        let m = menu_modifier
                            .clone()
                            .then(crate::modifier::Modifier::new().padding_vertical(8.0))
                            .then(crate::modifier::Modifier::new().vertical_scroll(scroll_state.clone()));
                        crate::ui::Column::new()
                            .modifier(m)
                            .build(ctx, |ctx| menu(ctx));
                    });
                }),
                local_snapshot: Vec::new(),
            });
        }
    }
}

// ═══════════════ DropdownMenuItem ═══════════════

/// Dropdown menu item — text + click callback.
pub struct DropdownMenuItem {
    text: String,
    on_click: Option<Arc<dyn Fn() + Send + Sync>>,
    enabled: bool,
    /// 调用方 modifier，追加在内部样式**外层**（同 `Button` 约定：可覆盖默认样式；也是测试挂
    /// `test_tag` 的入口）——对齐 M3 `DropdownMenuItem(text, onClick, modifier, …)` 的 modifier。
    modifier: crate::modifier::Modifier,
    /// M3 `colors: MenuItemColors`（未设 = `MenuDefaults.itemColors()`）。
    colors: Option<MenuItemColors>,
    /// M3 `contentPadding`（未设 = 水平 12、垂直 0）。
    content_padding: Option<(f32, f32)>,
    /// M3 `interactionSource: MutableInteractionSource? = null` — the ripple/hover source. `None` means
    /// the item makes and remembers its own.
    interaction_source: Option<crate::ui::interaction::MutableInteractionSource>,
    /// M3 `leadingIcon: @Composable (() -> Unit)? = null` — winia's slot convention is a boxed `FnOnce`,
    /// as in `ListItem::leading_content`.
    leading_icon: Option<Box<dyn FnOnce(&mut crate::core::composer::ComposeCtx) + Send + Sync>>,
    /// M3 `trailingIcon: @Composable (() -> Unit)? = null`.
    trailing_icon: Option<Box<dyn FnOnce(&mut crate::core::composer::ComposeCtx) + Send + Sync>>,
}

impl DropdownMenuItem {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            on_click: None,
            enabled: true,
            modifier: crate::modifier::Modifier::new(),
            colors: None,
            content_padding: None,
            interaction_source: None,
            leading_icon: None,
            trailing_icon: None,
        }
    }

    pub fn modifier(mut self, modifier: crate::modifier::Modifier) -> Self {
        self.modifier = modifier;
        self
    }

    pub fn on_click(mut self, cb: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_click = Some(Arc::new(cb));
        self
    }

    pub fn enabled(mut self, v: bool) -> Self {
        self.enabled = v;
        self
    }

    /// M3 `colors: MenuItemColors` — `MenuDefaults.itemColors()` when unset.
    pub fn colors(mut self, colors: MenuItemColors) -> Self {
        self.colors = Some(colors);
        self
    }

    /// M3 `contentPadding` — `MenuDefaults.DropdownMenuItemContentPadding` (horizontal 12dp, vertical 0)
    /// when unset. Pass `PaddingValues`-style `(horizontal, vertical)`.
    pub fn content_padding(mut self, horizontal: f32, vertical: f32) -> Self {
        self.content_padding = Some((horizontal, vertical));
        self
    }

    /// M3 `interactionSource` — the press/hover source behind the item's ripple. Left unset the item owns
    /// one (material3's `null` default).
    pub fn interaction_source(
        mut self,
        source: crate::ui::interaction::MutableInteractionSource,
    ) -> Self {
        self.interaction_source = Some(source);
        self
    }

    /// M3 `leadingIcon` — drawn in a box at least 24dp wide
    /// (`ListTokens.ListItemLeadingIconSize`), tinted with `MenuItemColors::leading_icon_color`, and the
    /// label starts 12dp after it.
    pub fn leading_icon(
        mut self,
        content: impl FnOnce(&mut crate::core::composer::ComposeCtx) + Send + Sync + 'static,
    ) -> Self {
        self.leading_icon = Some(Box::new(content));
        self
    }

    /// M3 `trailingIcon` — same box and tinting on the other side, with 12dp between the label and it.
    pub fn trailing_icon(
        mut self,
        content: impl FnOnce(&mut crate::core::composer::ComposeCtx) + Send + Sync + 'static,
    ) -> Self {
        self.trailing_icon = Some(Box::new(content));
        self
    }

    /// `#[composable]`: same contract as Popup/Dialog (marks a composition unit).
    #[composable]
    pub fn build(self, ctx: &mut crate::core::composer::ComposeCtx) {
        // M3 geometry (`Menu.kt`): `sizeIn(minWidth 112dp, maxWidth 280dp, minHeight 48dp)` with
        // `padding(contentPadding)` (horizontal 12dp, vertical 0 by default). No per-item background and
        // no per-item corner radius: the MENU's surface paints the container, and the item is a
        // full-width row on top of it.
        let (pad_h, pad_v) = self.content_padding.unwrap_or((12.0, 0.0));
        let modifier = crate::modifier::Modifier::new()
            .min_width(112.0)
            .max_width(280.0)
            .min_height(48.0)
            .padding_horizontal(pad_h)
            .padding_vertical(pad_v);
        let on_click = self.on_click;
        let colors = self.colors.clone().unwrap_or_else(MenuItemColors::defaults);
        let text_color = colors.text_color(self.enabled);
        // M3's item is `clickable(enabled, onClick, interactionSource, indication = ripple(true))`: a
        // ripple bounded to the item, in the content colour, on an interaction source the item owns unless
        // the caller passes one — the same wiring `Button` uses (`clickable_with_source` +
        // `ripple_with_shape`). The shape is a plain rectangle because M3 gives the item no `shape`: the
        // menu's own Surface clips the corners (`surface.rs` applies `clip(shape)`), and the 8dp vertical
        // padding already keeps the top and bottom items clear of them.
        let interaction = self
            .interaction_source
            .unwrap_or_else(|| ctx.remember(|| crate::ui::interaction::MutableInteractionSource::new()).get());
        let modifier = if self.enabled {
            let modifier = modifier.clickable_with_source(&interaction, move || {
                if let Some(cb) = &on_click {
                    (cb)();
                }
            });
            modifier
                .ripple_with_shape(
                    &interaction,
                    text_color,
                    true,
                    crate::modifier::Shape::Rectangle,
                )
                // No focus RING: material3's menu items mark focus with a state layer, not an outline, and
                // winia draws the ring around any focused node by default (`render.rs`). The highlight is
                // still there — it comes from the same element as the ripple above, whose state layer
                // paints `hover + focus` (`render.rs`: "状态层…hover_opacity + focus_opacity").
                .no_focus_ring()
        } else {
            modifier
        };
        let modifier = modifier.then(self.modifier);
        let text = self.text;
        // M3 typography: `ProvideTextStyle(MaterialTheme.typography.labelLarge)`.
        let style = crate::ui::theme::WiniaTheme::typography().label_large;
        // The item is material3's `Row(verticalAlignment = Alignment.CenterVertically)` — winia's
        // `Row::alignment(Alignment::Center)` centres on the cross axis. (Fixing the centring is what
        // removed the "asymmetric padding" a screenshot showed: the label used to sit at the item's TOP,
        // `pos:[12,0]` in a 48-tall row, i.e. 28px below it and 0 above.)
        //
        // The three children and their geometry are material3's `DropdownMenuItemContent`, verbatim:
        //   leadingIcon  Box(defaultMinSize(minWidth = ListItemLeadingIconSize))     — 24dp
        //   text         Box(weight(1f).padding(start = 12dp if leading, end = 12dp if trailing))
        //   trailingIcon Box(defaultMinSize(minWidth = ListItemTrailingIconSize))    — 24dp
        // each icon tinted with its own colour role through `with_content_color`, which is winia's
        // equivalent of material3's `CompositionLocalProvider(LocalContentColor provides …)`.
        //
        // One difference, forced by a missing framework feature: material3's `weight(1f)` is meant to run
        // inside the menu's `width(IntrinsicSize.Max)` column, which makes the menu as wide as its widest
        // item and every item that wide. winia has no intrinsic measurement, and a weighted child fills the
        // CONSTRAINT instead — measured: the menu jumped from 112 to the 280 max as soon as the text was
        // weighted. So the weight is applied only when there IS a trailing icon, the case where material3's
        // stretching is visible (a shortcut hint sits at the menu's right edge, not glued to its label).
        // Without one, the label is content-sized: labels still start at the same offset when every item
        // carries a leading icon, the menu is as wide as its widest item, and the only difference left is
        // that items do not share one width. Both cases are recorded in `docs/dropdown-menu.md`.
        let leading_icon = self.leading_icon;
        let trailing_icon = self.trailing_icon;
        let has_leading = leading_icon.is_some();
        let has_trailing = trailing_icon.is_some();
        let stretch_label = has_trailing;
        let leading_color = colors.leading_icon_color(self.enabled);
        let trailing_color = colors.trailing_icon_color(self.enabled);
        let icon_box = |ctx: &mut crate::core::composer::ComposeCtx,
                        color: crate::modifier::Color,
                        content: Box<dyn FnOnce(&mut crate::core::composer::ComposeCtx) + Send + Sync>| {
            crate::ui::theme::WiniaTheme::with_content_color(color, ctx, |ctx| {
                crate::ui::Column::new()
                    .modifier(crate::modifier::Modifier::new().min_width(24.0))
                    .build(ctx, |ctx| content(ctx));
            });
        };
        crate::ui::Row::new()
            .alignment(crate::layout::node::Alignment::Center)
            .modifier(modifier)
            .build(ctx, |ctx| {
                if let Some(content) = leading_icon {
                    icon_box(ctx, leading_color, content);
                }
                crate::ui::Column::new()
                    .modifier({
                        let mut m = crate::modifier::Modifier::new();
                        if stretch_label {
                            m = m.layout_weight(1.0);
                        }
                        m.padding_sides(
                            if has_leading { 12.0 } else { 0.0 },
                            0.0,
                            if has_trailing { 12.0 } else { 0.0 },
                            0.0,
                        )
                    })
                    .build(ctx, |ctx| {
                        crate::ui::Text::new(text)
                            .style(style)
                            .color(text_color)
                            .build(ctx);
                    });
                if let Some(content) = trailing_icon {
                    icon_box(ctx, trailing_color, content);
                }
            });
    }
}

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
    #[test]
    fn a_menu_item_carries_a_ripple_and_no_focus_ring() {
        let mut composer = crate::core::composer::Composer::new();
        composer.compose(|ctx| {
            DropdownMenuItem::new("A").build(ctx);
        });
        composer.layout(crate::layout::constraints::Constraints::new(0.0, 400.0, 0.0, 600.0));
        let root = composer.layout_root_idx().expect("the item's node");
        let nodes = composer.arena_nodes();
        let els = nodes[root].modifier.elements();
        assert!(
            els.iter().any(|e| matches!(e, crate::modifier::ModifierElement::Clickable { .. })),
            "the item is clickable: {:?}",
            els.len()
        );
        assert!(
            els.iter().any(|e| matches!(e, crate::modifier::ModifierElement::Ripple { .. })),
            "the item carries a ripple (material3's indication)"
        );
        assert!(
            els.iter().any(|e| matches!(e, crate::modifier::ModifierElement::NoFocusRing)),
            "…and no focus ring: material3 marks menu-item focus with a state layer"
        );
    }

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
