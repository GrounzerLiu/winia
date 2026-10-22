use super::*;

/// Overlay descriptor — registered during composition (e.g. `Popup::build`
/// calls `ctx.open_overlay` internally).
pub struct OverlayDesc {
    pub(crate) id: u64,
    pub(crate) anchor_slot: Option<u64>,
    /// The direction the `*Start`/`*End` positions resolve against. `None` keeps the existing rule: the
    /// anchor node's own resolved direction when there is one, else the root node's. A component that knows
    /// its direction at COMPOSE time — where the composition local is still valid — can pass it here.
    /// Placement runs after composition, which is exactly why the local cannot be consulted there.
    pub(crate) direction: Option<crate::layout::LayoutDirection>,
    pub(crate) position: PopupPosition,
    pub(crate) offset: (f32, f32),
    pub(crate) anchor_slide: Option<AnchorSlide>,
    pub(crate) modal: bool,
    pub(crate) focus_scope: bool,
    pub(crate) dismiss_on_outside: bool,
    pub(crate) dismiss_on_back_press: bool,
    /// Whether the CONTENT owns its own exit — Compose's `onDismissRequest` contract, which is always the
    /// caller's to complete. A dismissal request then calls `on_dismiss` and leaves the overlay open: the
    /// component animates itself out and stops composing the overlay, which is what removes it. winia's
    /// host otherwise starts its own exit fade next to the handler, which faded a bottom sheet out while it
    /// was still sliding down. `false` for everything else (the door-out animation stays the host's).
    pub(crate) dismiss_defers_to_content: bool,
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

