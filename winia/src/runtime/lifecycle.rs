//! Per-Composer lifecycle state for declarative `Window` nodes.
//!
//! It lives here rather than beside the `Window` builder because the *holder* is the Composer: one
//! window's compose pass must not consume another's pending close request, and the only object that
//! can keep them apart is the one that runs the pass. `app/window.rs` sets and reads it.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

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
