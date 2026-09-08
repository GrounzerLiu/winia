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

/// Popup position (mirrors Compose `PopupPosition` — relative to anchor / window).
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
    pub(crate) interpolator: std::sync::Arc<dyn crate::animation::interpolator::Interpolator>,
}

impl OverlayAnimSpec {
    /// Default enter animation (scale 0.8 -> 1 + fade, 200ms EaseOutCubic — the
    /// classic material2 Dialog open effect).
    pub fn default_enter() -> Self {
        Self {
            scale_from: 0.8,
            fade: true,
            slide_from_y: 0.0,
            reveal_top: false,
            duration: std::time::Duration::from_millis(200),
            interpolator: std::sync::Arc::new(crate::animation::interpolator::EaseOutCubic::new()),
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
            interpolator: std::sync::Arc::new(crate::animation::interpolator::EaseInCubic::new()),
        }
    }

    /// Scale only (no fade).
    pub fn scale_only(from: f32, duration: std::time::Duration) -> Self {
        Self { scale_from: from, fade: false, slide_from_y: 0.0, reveal_top: false, duration, interpolator: std::sync::Arc::new(crate::animation::interpolator::EaseOutCubic::new()) }
    }

    /// Fade only.
    pub fn fade_only(duration: std::time::Duration) -> Self {
        Self { scale_from: 1.0, fade: true, slide_from_y: 0.0, reveal_top: false, duration, interpolator: std::sync::Arc::new(crate::animation::interpolator::EaseOutCubic::new()) }
    }

    /// Dropdown slide + fade (slide_in y=-height/2 with fade — mirrors docked
    /// dropdown `slideIn(-height/2) + fadeIn`).
    pub fn slide_down(duration: std::time::Duration) -> Self {
        Self { scale_from: 1.0, fade: true, slide_from_y: -0.5, reveal_top: false, duration, interpolator: std::sync::Arc::new(crate::animation::interpolator::EaseOutCubic::new()) }
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
        Self { scale_from: 1.0, fade: true, slide_from_y: 0.0, reveal_top: true, duration, interpolator: std::sync::Arc::new(crate::animation::interpolator::EaseOutCubic::new()) }
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
    /// Modal (Dialog): draws a scrim and captures outside clicks for dismiss.
    pub(crate) modal: bool,
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
            modal: false,
            dismiss_on_outside: true,
            click_passthrough: false,
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
            exit_anim: Some(OverlayAnimSpec::default_exit()),
        }
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
            anchor_slot: None,
            position: PopupPosition::Center,
            offset: (0.0, 0.0),
            modal: true,
            dismiss_on_outside: self.dismiss_on_outside,
            click_passthrough: false,
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

/// Dropdown menu (mirrors Compose `DropdownMenu`) — anchored to a trigger
/// container; clicking outside dismisses it.
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
}

impl DropdownMenu {
    pub fn new(expanded: crate::core::state::State<bool>) -> Self {
        Self {
            expanded,
            on_dismiss: None,
        }
    }

    pub fn on_dismiss_request(mut self, cb: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_dismiss = Some(Arc::new(cb));
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

        // `build` always executes (parameterized by `expanded`) — records active
        // for `sync` to delete (mirrors Popup/Dialog's `visible` parameterization:
        // `expanded=false` records `false` -> delete).
        ctx.record_overlay_active(id.get(), expanded);
        if expanded {
            ctx.open_overlay(crate::ui::overlay::OverlayDesc {
                id: id.get(),
                anchor_slot: Some(anchor_slot),
                position: PopupPosition::BottomLeft,
                offset: (0.0, 4.0),
                modal: false,
                dismiss_on_outside: true,
                click_passthrough: false,
                on_dismiss: self.on_dismiss,
                enter_anim: None, // DropdownMenu 默认无进入动画
                exit_anim: None, // DropdownMenu 默认无退出动画
                content: Box::new(menu),
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
}

impl DropdownMenuItem {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            on_click: None,
            enabled: true,
        }
    }

    pub fn on_click(mut self, cb: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_click = Some(Arc::new(cb));
        self
    }

    pub fn enabled(mut self, v: bool) -> Self {
        self.enabled = v;
        self
    }

    /// `#[composable]`: same contract as Popup/Dialog (marks a composition unit).
    #[composable]
    pub fn build(self, ctx: &mut crate::core::composer::ComposeCtx) {
        let modifier = crate::modifier::Modifier::new()
            .size(160.0, 36.0)
            .padding(crate::modifier::SizeValue::Static(crate::modifier::Dimension::Fixed(12.0)))
            .background(
                crate::modifier::Color::from_argb(255, 250, 250, 250),
                crate::modifier::Shape::RoundedRect { corner_radius: 4.0 },
            );
        let on_click = self.on_click;
        let modifier = if self.enabled {
            modifier.clickable(move || {
                if let Some(cb) = &on_click {
                    (cb)();
                }
            })
        } else {
            modifier
        };
        let text = self.text;
        crate::ui::Column::new()
            .modifier(modifier)
            .build(ctx, |ctx| {
                crate::ui::Text::new(text)
                    .font_size(13.0)
                    .color(if self.enabled {
                        crate::modifier::Color::from_argb(255, 60, 60, 60)
                    } else {
                        crate::modifier::Color::from_argb(120, 160, 160, 160)
                    })
                    .build(ctx);
            });
    }
}
