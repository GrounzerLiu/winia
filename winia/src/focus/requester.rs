use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};

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

/// Drain the queued focus requests for the current window (called from `app`'s
/// `RedrawRequested`).
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

/// A focus requester: call `request_focus()` to give the associated component focus.
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

    /// Request focus. Works with or without the `debug-server` feature; the request is applied
    /// on the next frame's `RedrawRequested`.
    pub fn request_focus(&self) {
        FOCUS_REQUESTS.lock().unwrap().push(FocusRequest { window_id: self.window_id, id: self.id });
        // Also go through the debug channel, preserving the old behaviour.
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
