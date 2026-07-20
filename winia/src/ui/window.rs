use crate::app;
use crate::prelude::*;
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

/// 全局已创建窗口 ID 集合。每次 compose 检查：窗口被 × 关闭后，
/// `CREATED` 中 id 被移除 → 下次 `opened` 重新设为 false → 重建新窗。
pub(crate) static CREATED: LazyLock<Mutex<HashSet<u64>>> = LazyLock::new(|| Mutex::new(HashSet::new()));
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static CLOSE_QUEUE: LazyLock<Mutex<Vec<u64>>> = LazyLock::new(|| Mutex::new(Vec::new()));

/// 处理 app 层待关闭窗口
pub(crate) fn process_close_queue(windows: &mut std::collections::HashMap<winit::window::WindowId, crate::app::PerWindow>,
                                   event_loop: &dyn winit::event_loop::ActiveEventLoop,
                                   force_shutdown: &dyn Fn()) {
    let ids: Vec<u64> = CLOSE_QUEUE.lock().unwrap().drain(..).collect();
    for cid in ids {
        // 移除 CREATED 记录
        CREATED.lock().unwrap().remove(&cid);
        // 查找对应的 WindowId
        let to_close: Vec<winit::window::WindowId> = windows.iter()
            .filter(|(_, pw)| pw.created_id() == Some(cid))
            .map(|(wid, _)| *wid)
            .collect();
        for wid in to_close {
            if let Some(mut pw) = windows.remove(&wid) {
                if let Some(ref mut cb) = pw.on_close { cb(); }
                for pw2 in windows.values() {
                    if let Some(ref sw) = pw2.skia_window { sw.request_redraw(); }
                }
                if windows.is_empty() {
                    force_shutdown();
                    event_loop.exit();
                }
            }
        }
    }
}

/// 当 slot 被回收时自动关闭窗口的守卫。
/// 通过 `ctx.remember()` 存储在 slot 的 remembered 表中。
/// slot 被 truncate → Box 被 drop → State 被 drop → Arc<Mutex<Inner>> Clone 被 drop
/// → 最后一个 Clone 的 Drop 运行 → 推送 CLOSE_QUEUE
#[derive(Clone)]
struct WindowLife {
    inner: Arc<Mutex<WindowLifeInner>>,
}
struct WindowLifeInner {
    created_id: u64,
    done: bool,
}
impl Drop for WindowLifeInner {
    fn drop(&mut self) {
        if !self.done && self.created_id != 0 {
            self.done = true;
            CLOSE_QUEUE.lock().unwrap().push(self.created_id);
            crate::app::wake_impl();
        }
    }
}

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
        let window_id = ctx.remember(|| 0u64);
        // 守卫：与 slot 生命周期绑定，slot 回收时自动关闭窗口
        let guard: State<WindowLife> = ctx.remember(|| WindowLife {
            inner: Arc::new(Mutex::new(WindowLifeInner { created_id: 0, done: false })),
        });

        let wid = window_id.get();
        if wid == 0 || !CREATED.lock().unwrap().contains(&wid) {
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            window_id.set(id);
            // 更新守卫中的 created_id
            guard.get().inner.lock().unwrap().created_id = id;

            let mut created = CREATED.lock().unwrap();
            created.insert(id);
            drop(created);

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
    }
}
