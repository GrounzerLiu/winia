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
    /// Initialisation callback (run once only, to create the main window declaratively).
    pub(crate) init: Option<Box<dyn FnOnce(&mut ComposeCtx)>>,
    /// Whether animations were active on the previous pass (forces a final frame when they stop).
    pub(crate) was_animating: bool,
}

mod handlers;

pub(crate) use handlers::try_selection_key;
mod helpers;
mod windows;
