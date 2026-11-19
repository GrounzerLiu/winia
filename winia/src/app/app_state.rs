use super::*;

// -- PlatformState --

/// Platform handle shared by every window in this app run.
///
/// AndroidApp is cloned into each composer when a window is created. It is intentionally owned by
/// the run state instead of a process-wide static: android_main can be entered again after the
/// Activity is recreated, and the old AndroidApp must not survive that transition.
#[derive(Default)]
pub(crate) struct PlatformState {
    #[cfg(target_os = "android")]
    pub(crate) android_app: Option<winit::platform::android::activity::AndroidApp>,
}

// -- AppState --

pub(crate) struct AppState {
    /// Platform handles, including the AndroidApp used by Android UI code.
    pub(crate) platform: PlatformState,
    /// Windows that have been created.
    pub(crate) windows: HashMap<WindowId, PerWindow>,
    /// Saveable values of windows that have closed, keyed by `created_id`: a window recreated with
    /// the same id picks its state back up.
    pub(crate) saveable: HashMap<u64, HashMap<String, crate::runtime::saveable::SavedValue>>,
    /// Windows waiting to be created (queued by the Window composable).
    pub(crate) pending_content: Vec<PendingWindow>,
    /// Parent window id (for the `is_parent` check — not derived from HashMap order).
    pub(crate) parent_window_id: Option<WindowId>,
    /// The application root closure. It is re-run when root State changes, so declarative windows
    /// behind a State flag can appear and disappear after startup.
    pub(crate) init: Option<Box<dyn Fn(&mut ComposeCtx)>>,
    /// The persistent root composition. Dropping it after the first frame made dynamic root
    /// declarations impossible.
    pub(crate) root: Option<crate::runtime::composer::Composer>,
    /// Whether animations were active on the previous pass (forces a final frame when they stop).
    pub(crate) was_animating: bool,
}

mod handlers;

#[cfg(test)]
mod back_routing_tests;

pub(crate) use handlers::try_selection_key;
mod helpers;
mod windows;
