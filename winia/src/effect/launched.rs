//! `LaunchedEffect`: run a coroutine for a key, cancel it when the key changes or the point goes away.

use crate::composable;
use crate::effect::scope::CoroutineScope;
use crate::runtime::composer::ComposeCtx;
use std::sync::Arc;
use std::sync::Mutex;
use tokio::task::JoinHandle;

/// Internal state.
struct LaunchedEffectState<T: PartialEq> {
    prev_key: Option<T>,
    /// The RUNNING task, not just its abort handle: replacing the effect has to wait for the old
    /// body to actually stop. `AbortHandle::abort` only asks — it returns immediately, so the old
    /// body can still be mid-await when the new one starts, and the two then race for whatever
    /// shared state they touch. Compose's `LaunchedEffect` joins for that reason.
    task: Option<tokio::task::JoinHandle<()>>,
}

/// Run `block` in a coroutine for one key; cancel it when the key changes or the point leaves.
pub struct LaunchedEffect<T: PartialEq + Clone + Send + 'static> {
    key: T,
}

impl<T: PartialEq + Clone + Send + 'static> LaunchedEffect<T> {
    /// The coroutine scope bound to the composition's lifetime; every unfinished task is
    /// cancelled when the effect is disposed.
    pub fn new(key: T) -> Self { Self { key } }

    /// `#[composable]`: its internal remember/attach_cleanup takes a stable base from the
    /// `build` call site.
    #[composable]
    pub fn build<F: std::future::Future<Output = ()> + Send + 'static>(
        self,
        ctx: &mut ComposeCtx,
        block: impl FnOnce(CoroutineScope) -> F + Send + 'static,
    ) {
        // Inlined instead of calling `remember_coroutine_scope`: that is a `#[composable]`
        // function, so it would add a scope slot per effect per frame purely to get a remember key
        // this `build` already has. The teardown registration is the part that must NOT be skipped —
        // without it a `scope.spawn` in the body outlives the slot (its task holds a scope clone, so
        // the refcount never reaches zero and `ScopeState::drop` never aborts it).
        let scope: CoroutineScope = ctx.remember_handle(CoroutineScope::from_runtime);
        crate::effect::scope::hook_scope_teardown(ctx, &scope);

        // `remember_handle`, not `remember`: nothing ever writes this slot, so the `State` wrapper
        // would be a `StateInner` + `StateSignal` per effect instance with no possible reader.
        let state: Arc<Mutex<LaunchedEffectState<T>>> =
            ctx.remember_handle(|| Arc::new(Mutex::new(LaunchedEffectState { prev_key: None, task: None })));

        let state_clone = Arc::clone(&state);
        let key_changed = {
            let s = state_clone.lock().unwrap();
            s.prev_key.as_ref() != Some(&self.key)
        };

        if key_changed {
            // Take the old task out from under the lock, then abort it OUTSIDE it: the abort itself
            // is not user code, but keeping every call to a task handle off the shared guard is the
            // rule this file follows.
            let previous = {
                let mut s = state_clone.lock().unwrap();
                s.task.take()
            };
            // `abort` takes `&self`, so the handle stays usable for the join below.
            if let Some(old) = &previous {
                old.abort();
            }
            // The body was handed this scope, so anything it spawned through `scope.spawn` belongs
            // to the restart too: a Compose coroutine cancels its children with it.
            scope.abort_spawned();
            // The new body waits for the old one to finish before it runs. Without this the two
            // bodies overlap for as long as the aborted future takes to unwind, which for an effect
            // that mutates shared state is a real interleaving rather than a theoretical one.
            let fut = block(scope.clone());
            let join_handle = scope.rt.spawn(async move {
                if let Some(old) = previous {
                    // An aborted task resolves with `Err(Cancelled)`; either way it has STOPPED by
                    // the time this returns, which is the whole point.
                    let _ = old.await;
                }
                fut.await;
            });
            let mut s = state_clone.lock().unwrap();
            s.task = Some(join_handle);
            s.prev_key = Some(self.key);

            // Registered once per (re)start, not once per frame: `remember_on_forget` keeps the
            // callback in this slot's map, where it survives recomposition until the slot is
            // dropped, and the closure below only reads the stable `Arc` it captured.
            let state_for_remove = Arc::clone(&state_clone);
            ctx.remember_on_forget(move || {
                let taken = {
                    let mut s = state_for_remove.lock().unwrap();
                    s.task.take()
                };
                if let Some(task) = taken {
                    task.abort();
                }
            });
        }
    }
}

