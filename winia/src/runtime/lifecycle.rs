//! Per-Composer lifecycle state for declarative `Window` nodes.
//!
//! It lives here rather than beside the `Window` builder because the *holder* is the Composer: one
//! window's compose pass must not consume another's pending close request, and the only object that
//! can keep them apart is the one that runs the pass. `app/window.rs` sets and reads it.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Per-Composer lifecycle state for declarative Window nodes.
///
/// A composer can hold several `Window` declarations, so "rebuilt" is a set of ids: one declaration
/// being rebuilt must not hide another declaration's removal.
#[derive(Clone, Default)]
pub(crate) struct LifecycleState {
    rebuilt: Arc<Mutex<HashSet<u64>>>,
    pending_remove_id: Arc<AtomicU64>,
}

impl LifecycleState {
    pub(crate) fn reset_for_compose(&self) {
        self.rebuilt.lock().unwrap_or_else(|e| e.into_inner()).clear();
        self.pending_remove_id.store(0, Ordering::Release);
    }

    pub(crate) fn reset_pending_remove(&self) {
        self.pending_remove_id.store(0, Ordering::Release);
    }

    pub(crate) fn mark_rebuilt(&self, id: u64) {
        if id != 0 {
            self.rebuilt
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(id);
        }
    }

    pub(crate) fn active_ids(&self) -> HashSet<u64> {
        self.rebuilt.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub(crate) fn set_pending_remove(&self, id: u64) {
        if id != 0 {
            self.pending_remove_id.store(id, Ordering::Release);
        }
    }

    pub(crate) fn pending_close_id(&self) -> Option<u64> {
        let id = self.pending_remove_id.load(Ordering::Acquire);
        let rebuilt = self.rebuilt.lock().unwrap_or_else(|e| e.into_inner());
        (id != 0 && !rebuilt.contains(&id)).then_some(id)
    }
}
