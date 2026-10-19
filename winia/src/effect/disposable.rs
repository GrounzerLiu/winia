//! `DisposableEffect`: a synchronous setup/cleanup pair keyed like `LaunchedEffect`.

use crate::composable;
use crate::runtime::composer::ComposeCtx;
use std::sync::Arc;
use std::sync::Mutex;

struct DisposableState<T: PartialEq> {
    prev_key: Option<T>,
    cleanup: Option<Box<dyn FnOnce() + Send>>,
}

/// key 变化时执行同步 setup 和 cleanup。
pub struct DisposableEffect<T: PartialEq + Clone + Send + 'static> {
    key: T,
}

impl<T: PartialEq + Clone + Send + 'static> DisposableEffect<T> {
    pub fn new(key: T) -> Self { Self { key } }

    /// #[composable]：内部 remember/attach_cleanup 从 build 调用点取稳定 base
    #[composable]
    pub fn build<F: FnOnce() + Send + 'static>(
        self,
        ctx: &mut ComposeCtx,
        effect: impl Fn(T) -> F + Send + 'static,
    ) {
        // `remember_handle` for the same reason as `LaunchedEffect`: bookkeeping, never written.
        let state: Arc<Mutex<DisposableState<T>>> =
            ctx.remember_handle(|| Arc::new(Mutex::new(DisposableState { prev_key: None, cleanup: None })));

        let state_clone = Arc::clone(&state);
        let key_changed = {
            let s = state_clone.lock().unwrap();
            s.prev_key.as_ref() != Some(&self.key)
        };

        if key_changed {
            // NOTHING here runs user code while the guard is alive. A panic in the caller's
            // cleanup would poison the mutex, and from then on every `lock().unwrap()` in this
            // effect panics — the instance is dead for the rest of the run. A caller that touches
            // the same state from inside its cleanup would deadlock outright, because `std::sync::
            // Mutex` is not reentrant. Both were reachable: the cleanup and the setup used to be
            // called with `s` still locked.
            let previous = {
                let mut s = state_clone.lock().unwrap();
                s.cleanup.take()
            };
            if let Some(cleanup) = previous {
                cleanup();
            }
            // Setup outside the lock too. `effect` is the caller's code and gets the same treatment
            // for the same reason.
            let next = Box::new(effect(self.key.clone())) as Box<dyn FnOnce() + Send>;
            let mut s = state_clone.lock().unwrap();
            s.cleanup = Some(next);
            s.prev_key = Some(self.key);

            // Registered once per (re)start: the callback lives in this slot's map until the slot
            // is dropped, and it only reads the stable `Arc` captured above.
            let state_for_remove = Arc::clone(&state_clone);
            ctx.remember_on_forget(move || {
                let taken = {
                    let mut s = state_for_remove.lock().unwrap();
                    s.cleanup.take()
                };
                // Outside the lock, same reason as above: this runs during composition teardown and
                // a panic here must not take the whole effect mechanism down with it.
                if let Some(cleanup) = taken {
                    cleanup();
                }
            });
        }
    }
}
