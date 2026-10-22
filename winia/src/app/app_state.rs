use super::*;

// -- AppState --

pub(crate) struct AppState {
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
mod helpers;
mod windows;

