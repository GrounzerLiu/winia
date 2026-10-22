use super::*;

// ── AppState ──

pub(crate) struct AppState {
    /// 已创建的窗口
    pub(crate) windows: HashMap<WindowId, PerWindow>,
    /// Saveable values of windows that have closed, keyed by `created_id`: a window recreated with
    /// the same id picks its state back up.
    pub(crate) saveable: HashMap<u64, HashMap<String, crate::runtime::saveable::SavedValue>>,
    /// 待创建的窗口（由 Window composable 排队）
    pub(crate) pending_content: Vec<PendingWindow>,
    /// 父窗口 ID（用于 is_parent 判断，不依赖 HashMap 顺序）
    pub(crate) parent_window_id: Option<WindowId>,
    /// 初始化回调（仅首次调用，用于声明式创建主窗口）
    pub(crate) init: Option<Box<dyn FnOnce(&mut ComposeCtx)>>,
    /// 上轮动画是否活跃（停止时强制终帧渲染）
    pub(crate) was_animating: bool,
}


mod handlers;
mod helpers;
mod windows;

