use crate::app;
use crate::prelude::*;
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

/// 全局已创建窗口 ID 集合。每次 compose 检查：窗口被 × 关闭后，
/// `CREATED` 中 id 被移除 → 下次 `opened` 重新设为 false → 重建新窗。
static CREATED: LazyLock<Mutex<HashSet<u64>>> = LazyLock::new(|| Mutex::new(HashSet::new()));
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

    /// 构建窗口。每次 compose 都检查 CREATED，确保 × 关闭后能重建。
    pub fn build(self, _ctx: &mut ComposeCtx, content: impl Fn(&mut ComposeCtx) + Send + 'static) {
        let window_id = _ctx.remember(|| 0u64);
        // 首次 compose 或窗口已被 × 关闭 → 分配新 id 并创建
        let wid = window_id.get();
        if wid == 0 || !CREATED.lock().unwrap().contains(&wid) {
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            window_id.set(id);
            let mut created = CREATED.lock().unwrap();
            if created.insert(id) {
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
                })), wrapped);
            }
        }
    }
}
