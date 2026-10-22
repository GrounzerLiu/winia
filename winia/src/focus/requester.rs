use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};

// ── FocusRequester ──

static NEXT_FOCUS_ID: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static CURRENT_FOCUS_WINDOW: RefCell<u64> = const { RefCell::new(0) };
}

static FOCUS_REQUESTS: std::sync::Mutex<Vec<FocusRequest>> = std::sync::Mutex::new(Vec::new());

#[derive(Debug, Clone)]
struct FocusRequest {
    window_id: u64,
    id: u64,
}

pub(crate) struct FocusWindowGuard {
    prev: u64,
}

impl Drop for FocusWindowGuard {
    fn drop(&mut self) {
        CURRENT_FOCUS_WINDOW.with(|c| c.replace(self.prev));
    }
}

pub(crate) fn focus_window(window_id: u64) -> FocusWindowGuard {
    let prev = CURRENT_FOCUS_WINDOW.with(|c| c.replace(window_id));
    FocusWindowGuard { prev }
}

fn current_focus_window() -> u64 {
    CURRENT_FOCUS_WINDOW.with(|c| *c.borrow())
}

/// 消费所有排队的焦点请求（供 app.rs RedrawRequested 调用）
pub(crate) fn take_focus_requests() -> Vec<u64> {
    let current = current_focus_window();
    let mut req = FOCUS_REQUESTS.lock().unwrap();
    let mut drained = Vec::new();
    let mut kept = Vec::new();
    for item in req.drain(..) {
        if item.window_id == 0 || item.window_id == current {
            drained.push(item.id);
        } else {
            kept.push(item);
        }
    }
    *req = kept;
    drained
}

/// 焦点请求器——可在代码中调用 request_focus() 让关联组件获得焦点
#[derive(Debug, Clone)]
pub struct FocusRequester {
    pub(crate) id: u64,
    pub(crate) window_id: u64,
}

impl FocusRequester {
    pub fn new() -> Self {
        FocusRequester { id: NEXT_FOCUS_ID.fetch_add(1, Ordering::Relaxed), window_id: current_focus_window() }
    }

    pub fn id(&self) -> u64 { self.id }

    /// 请求焦点。无论是否启用 debug-server，都生效。
    /// 焦点将在下一帧 RedrawRequested 时应用。
    pub fn request_focus(&self) {
        FOCUS_REQUESTS.lock().unwrap().push(FocusRequest { window_id: self.window_id, id: self.id });
        // 同时走 debug 通道（兼容旧行为）
        #[cfg(feature = "debug-server")]
        crate::debug::queue_event(crate::debug::DebugEvent::RequestFocus { id: self.id });
    }
}

impl Default for FocusRequester {
    fn default() -> Self { Self::new() }
}

impl From<&FocusRequester> for FocusRequester {
    fn from(fr: &FocusRequester) -> Self { fr.clone() }
}
