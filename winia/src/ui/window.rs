use crate::app;
use crate::prelude::*;
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

/// 全局已创建窗口 ID 集合。每次 compose 检查：窗口被 × 关闭后，
/// `CREATED` 中 id 被移除 → 下次 rebuild 时重新创建新窗。
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

    pub fn build(self, ctx: &mut ComposeCtx, content: impl Fn(&mut ComposeCtx) + Send + 'static) {
        // created_id 存储在父 slot 中（不受 if 分支 slot 回收影响）
        let created_id = ctx.remember(|| 0u64);
        // 上次 compose 时窗口是否存活
        let was_alive = ctx.remember(|| false);

        let wid = created_id.get();
        let is_alive = wid != 0 && CREATED.lock().unwrap().contains(&wid);

        if !is_alive {
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

        // 检测 alive → !alive 转换，关闭 OS 窗口
        let was = was_alive.get();
        if was && !is_alive && wid != 0 {
            CREATED.lock().unwrap().remove(&wid);
            app::close_window_by_id(wid);
        }
        was_alive.set(is_alive);
    }
}
