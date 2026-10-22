use super::*;

// ═══════════════ Dialog ═══════════════

/// Modal dialog (mirrors Compose `Dialog`) — centered with a scrim; clicking
/// the scrim triggers `on_dismiss_request`.
pub struct Dialog {
    pub(crate) visible: bool,
    pub(crate) on_dismiss: Option<Arc<dyn Fn() + Send + Sync>>,
    pub(crate) dismiss_on_outside: bool,
    pub(crate) enter_anim: Option<OverlayAnimSpec>,
    pub(crate) exit_anim: Option<OverlayAnimSpec>,
    pub(crate) position: PopupPosition,
    pub(crate) offset: (f32, f32),
    pub(crate) anchor_slot: Option<u64>,
    pub(crate) anchor_slide: Option<AnchorSlide>,
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
            direction: None,
            anchor_slot: self.anchor_slot,
            position: self.position,
            offset: self.offset,
            anchor_slide: self.anchor_slide,
            modal: true,
            // A dialog owns the keyboard while it is up (see `OverlayDesc::focus_scope`).
            focus_scope: true,
            dismiss_on_outside: self.dismiss_on_outside,
            dismiss_on_back_press: true,
            dismiss_defers_to_content: false,
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

