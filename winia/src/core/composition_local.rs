//! CompositionLocal — 类似 Jetpack Compose 的隐式数据传递机制。

use std::any::Any;
use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static SLOTS: RefCell<Vec<(u64, Box<dyn Any>)>> = RefCell::new(Vec::new());
}

struct PopGuard {
    id: u64,
    used: bool,
}

impl PopGuard {
    fn new(id: u64) -> Self { Self { id, used: false } }
    fn disarm(&mut self) { self.used = true; }
}

impl Drop for PopGuard {
    fn drop(&mut self) {
        if !self.used {
            SLOTS.with(|s| {
                let mut slots = s.borrow_mut();
                if let Some(pos) = slots.iter().rposition(|(id, _)| *id == self.id) {
                    slots.remove(pos);
                }
            });
        }
    }
}

pub struct CompositionLocal<T: Clone + 'static> {
    id: AtomicU64,
    default: fn() -> T,
}

impl<T: Clone + 'static> CompositionLocal<T> {
    pub const fn new(default: fn() -> T) -> Self {
        Self { id: AtomicU64::new(0), default }
    }

    fn id(&self) -> u64 {
        let id = self.id.load(Ordering::Relaxed);
        if id != 0 { return id; }
        let new_id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        match self.id.compare_exchange(0, new_id, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => new_id,
            Err(existing) => existing,
        }
    }

    pub fn current(&self) -> T {
        let id = self.id();
        SLOTS.with(|s| {
            let slots = s.borrow();
            for (sid, val) in slots.iter().rev() {
                if *sid == id {
                    return val.downcast_ref::<T>().unwrap().clone();
                }
            }
            (self.default)()
        })
    }

    pub fn provides<R>(&self, value: T, f: impl FnOnce() -> R) -> R {
        let id = self.id();
        SLOTS.with(|s| s.borrow_mut().push((id, Box::new(value))));
        let mut guard = PopGuard::new(id);
        let result = f();
        SLOTS.with(|s| {
            let mut slots = s.borrow_mut();
            if let Some(pos) = slots.iter().rposition(|(sid, _)| *sid == id) {
                slots.remove(pos);
            }
        });
        guard.disarm();
        result
    }
}
