use crate::app;
use crate::prelude::*;
use std::cell::Cell;
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

thread_local! {
    /// Window::build 成功后设置，compose 末尾检测
    static WINDOW_REBUILT: Cell<bool> = const { Cell::new(false) };
    /// on_remove 推入的待关闭窗口 id
    static PENDING_REMOVE_ID: Cell<u64> = const { Cell::new(0) };
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
        let wid = PENDING_REMOVE_ID.get();
        if wid == 0 { return; }
        PENDING_REMOVE_ID.set(0);
        let rebuilt = WINDOW_REBUILT.get();
        WINDOW_REBUILT.set(false);
        if rebuilt { return; } // Window::build 被调用了 → 不关闭

        // Window::build 没被调用 → 关闭
        if !CREATED.lock().unwrap().contains(&wid) { return; }
        CREATED.lock().unwrap().remove(&wid);
        let to_close: Vec<winit::window::WindowId> = windows.iter()
            .filter(|(_, pw)| pw.created_id() == Some(wid))
            .map(|(wid2, _)| *wid2)
            .collect();
        for w in to_close {
            if let Some(mut pw) = windows.remove(&w) {
                if let Some(ref mut cb) = pw.on_close { cb(); }
                for pw2 in windows.values() {
                    if let Some(ref sw) = pw2.skia_window { sw.request_redraw(); }
                }
                if windows.is_empty() { force_shutdown(); event_loop.exit(); }
            }
        }
    }

    pub fn build(self, ctx: &mut ComposeCtx, content: impl Fn(&mut ComposeCtx) + Send + 'static) {
        let created_id = ctx.remember(|| 0u64);

        // on_remove 设置待关闭 id，compose 末尾检测
        let cid = created_id.clone();
        let key = ctx.next_key();
        ctx.start_leaf_with_remove(key, Modifier::new(), Box::new(move || {
            let wid = cid.get();
            if wid != 0 && CREATED.lock().unwrap().contains(&wid) {
                PENDING_REMOVE_ID.with(|p| p.set(wid));
            }
        }));

        let wid = created_id.get();
        let need_new = wid == 0 || !CREATED.lock().unwrap().contains(&wid);

        if need_new {
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            created_id.set(id);
            CREATED.lock().unwrap().insert(id);

            let w = self.state.width;
            let h = self.state.height;
            let mut on_close = self.on_close;
            let id_close = id;
            let wrapped: Option<Box<dyn FnMut() + Send>> = Some(Box::new(move || {
                CREATED.lock().unwrap().remove(&id_close);
                if let Some(ref mut f) = on_close { f(); }
            }));
            app::open_window_with_close(w, h, Some(Box::new(move |ctx| {
                Column::new().modifier(Modifier::new().padding(8.0)).build(ctx, |ctx| {
                    content(ctx);
                });
            })), wrapped, Some(id));
        }

        ctx.end_node();

        // end_node 后：标记 Window::build 已被调用
        WINDOW_REBUILT.with(|r| r.set(true));
    }
}
