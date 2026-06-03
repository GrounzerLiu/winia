use std::sync::atomic::{AtomicU32, Ordering};

static PRE_ID: AtomicU32 = AtomicU32::new(0);

/// Generate a unique id (lock-free, thread-safe)
pub fn next_id() -> u32 {
    PRE_ID.fetch_add(1, Ordering::Relaxed)
}
