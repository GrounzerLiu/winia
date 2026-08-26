use crate::app;
use crate::composable;
use crate::prelude::*;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

/// Window 占位 leaf / created_id 的 key 盐（黄金比例——与内容节点 next_key 空间隔离）
const WINDOW_KEY_SALT: u64 = 0x9E37_79B9_7F4A_7C15;

/// 子窗口内容包装（#[composable]——框架内部组合点也遵守稳定 key 规则：
/// app.rs 的 process_pending_windows 直接 compose 此闭包——STMT_STACK 空，
/// 无宏覆盖会触发稳定 key panic）
#[composable]
fn sub_window_content(ctx: &mut ComposeCtx, content: &impl Fn(&mut ComposeCtx)) {
    // 内容铺满窗口，不做默认留白（Scaffold 等全屏页面需要 edge-to-edge）。
    Column::new().build(ctx, |ctx| {
        content(ctx);
    });
}

/// Per-Composer lifecycle state for declarative Window nodes.
///
/// Keeping these flags with the Composer prevents one window's compose pass
/// from consuming another window's pending close request.
#[derive(Clone, Default)]
pub(crate) struct LifecycleState {
    rebuilt: Arc<AtomicBool>,
    pending_remove_id: Arc<AtomicU64>,
}

impl LifecycleState {
    pub(crate) fn reset_for_compose(&self) {
        self.rebuilt.store(false, Ordering::Release);
        self.pending_remove_id.store(0, Ordering::Release);
    }

    pub(crate) fn reset_pending_remove(&self) {
        self.pending_remove_id.store(0, Ordering::Release);
    }

    pub(crate) fn mark_rebuilt(&self) {
        self.rebuilt.store(true, Ordering::Release);
    }

    pub(crate) fn set_pending_remove(&self, id: u64) {
        self.pending_remove_id.store(id, Ordering::Release);
    }

    pub(crate) fn pending_close_id(&self) -> Option<u64> {
        let id = self.pending_remove_id.load(Ordering::Acquire);
        (id != 0 && !self.rebuilt.load(Ordering::Acquire)).then_some(id)
    }
}

/// 全局已创建窗口 ID 集合。
pub(crate) static CREATED: LazyLock<Mutex<HashSet<u64>>> = LazyLock::new(|| Mutex::new(HashSet::new()));
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// 声明式窗口 Builder。
pub struct Window {
    state: WindowState,
    on_close: Option<Box<dyn FnMut() + Send>>,
}

pub struct WindowState {
    pub width: f32,
    pub height: f32,
    pub title: String,
}

impl Default for WindowState {
    fn default() -> Self {
        WindowState { width: 300.0, height: 200.0, title: String::new() }
    }
}

impl Window {
    pub fn new() -> Self {
        Window { state: WindowState::default(), on_close: None }
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.state.title = title.into();
        self
    }

    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.state.width = width;
        self.state.height = height;
        self
    }

    pub fn on_close(mut self, f: impl FnMut() + Send + 'static) -> Self {
        self.on_close = Some(Box::new(f));
        self
    }

    /// 在 compose 末尾调用，检测是否有关闭窗口需求
    pub(crate) fn process_detached(windows: &mut std::collections::HashMap<winit::window::WindowId, crate::app::PerWindow>,
                                   event_loop: &dyn winit::event_loop::ActiveEventLoop,
                                   force_shutdown: &dyn Fn()) {
        let pending: Vec<(winit::window::WindowId, u64)> = windows
            .iter()
            .filter_map(|(window_id, pw)| {
                pw.composer.pending_window_close_id().map(|id| (*window_id, id))
            })
            .collect();
        for (owner_window_id, wid) in pending {
            let Some(pw) = windows.get(&owner_window_id) else { continue };
            pw.composer.reset_pending_window_remove();
            Self::close_detached_window(windows, event_loop, force_shutdown, wid);
        }
    }

    fn close_detached_window(windows: &mut std::collections::HashMap<winit::window::WindowId, crate::app::PerWindow>,
                             event_loop: &dyn winit::event_loop::ActiveEventLoop,
                             force_shutdown: &dyn Fn(),
                             wid: u64) {
        // Window::build was not called for the Composer that owns this request.
        if !CREATED.lock().unwrap().contains(&wid) { return; }
        CREATED.lock().unwrap().remove(&wid);
        let to_close: Vec<winit::window::WindowId> = windows.iter()
            .filter(|(_, pw)| pw.created_id() == Some(wid))
            .map(|(wid2, _)| *wid2)
            .collect();
        for w in to_close {
            if let Some(mut pw) = windows.remove(&w) {
                // 清理 debug 树条目（逻辑关闭不走 winit Destroyed——残留会误判）
                crate::debug::remove_tree(w.into_raw() as u64);
                if let Some(ref mut cb) = pw.on_close { cb(); }
                for pw2 in windows.values() {
                    if let Some(ref sw) = pw2.skia_window { sw.request_redraw(); }
                }
                if windows.is_empty() { force_shutdown(); event_loop.exit(); }
            }
        }
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx, content: impl Fn(&mut ComposeCtx) + Send + 'static) {
        // 占位 leaf 与 created_id 共用同一个加盐 key：① 与内容节点 key 空间隔离
        // （结构变化时不误复用旧槽）；② 每 Window 独立（remember_at_key(u64::MAX)
        // 会让多个 Window 共享同一 created_id → 第二个 Window 永不创建）
        let key = ctx.next_key().wrapping_add(WINDOW_KEY_SALT);
        let created_id = ctx.remember_at_key(key, || 0u64);
        let _wid = created_id.get();

        // 创建仅用于 layout + on_remove 的 leaf slot
        // on_remove 中读取 State 最新值（以应对已创建窗口的 id）。
        // The request is stored on this Composer's lifecycle context so a
        // different window cannot consume it.
        let cid = created_id.clone();
        let lifecycle = ctx.window_lifecycle();
        let remove_lifecycle = lifecycle.clone();
        ctx.start_leaf_with_remove(key, Modifier::new(), Box::new(move || {
            let wid = cid.get();
            if wid != 0 && CREATED.lock().unwrap().contains(&wid) {
                remove_lifecycle.set_pending_remove(wid);
            }
        }));

        let wid = created_id.get();
        let need_new = wid == 0 || !CREATED.lock().unwrap().contains(&wid);

        if need_new {
                        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            created_id.set_silent(id); // 静默：窗口创建标记不触发主窗口异常重组
            CREATED.lock().unwrap().insert(id);

            let w = self.state.width;
            let h = self.state.height;
            let mut on_close = self.on_close;
            let id_close = id;
            let wrapped: Option<Box<dyn FnMut() + Send>> = Some(Box::new(move || {
                CREATED.lock().unwrap().remove(&id_close);
                if let Some(ref mut f) = on_close { f(); }
            }));
            let theme_colors = crate::ui::theme::WiniaTheme::colors();
            let theme_for_window = theme_colors.clone();
            app::open_window_with_title(w, h, self.state.title.clone(), Some(Box::new(move |ctx| {
                crate::ui::theme::WiniaTheme::with_theme(theme_colors.clone(), ctx, |ctx| {
                    sub_window_content(ctx, &content);
                });
            })), wrapped, Some(id), Some(theme_for_window));
        }

        ctx.end_node();

        // end_node 后：标记 Window::build 已被调用
        lifecycle.mark_rebuilt();
    }

}

