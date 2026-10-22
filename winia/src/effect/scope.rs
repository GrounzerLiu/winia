//! The composition-bound coroutine scope.

use crate::composable;
use crate::runtime::composer::ComposeCtx;
use std::sync::Arc;
use std::sync::Mutex;
use tokio::runtime::Handle;
use tokio::task::JoinHandle;

/// A coroutine scope bound to a composition point: its tasks are cancelled when the point goes
/// away.
///
/// Tasks live in `Arc<ScopeState>`. Cancellation is driven by the composition, not by the
/// refcount: the owner registers `on_forget` (`hook_scope_teardown`), so the tasks die when the
/// slot does even if a clone is still alive — which is the point, because a task that captured the
/// scope keeps the `Arc` alive by itself. `ScopeState::drop` still aborts as a last resort, but it
/// can no longer be the only mechanism.
pub struct CoroutineScope {
    state: Arc<ScopeState>,
    /// The runtime every task of this scope runs on. Visible to the sibling effect modules, which
    /// spawn through it directly and manage the task's lifetime themselves.
    pub(super) rt: Handle,
}

struct ScopeState {
    handles: Mutex<Vec<JoinHandle<()>>>,
    /// Set once this scope's composition point has registered its teardown. The registration has to
    /// happen once per scope, not once per frame: `remember_coroutine_scope` runs on every
    /// recomposition of its call site.
    forget_hooked: std::sync::atomic::AtomicBool,
}

impl ScopeState {
    /// Abort every task this scope spawned. Idempotent — the list is drained.
    fn abort_all(&self) {
        let handles: Vec<JoinHandle<()>> = {
            let mut h = self.handles.lock().unwrap_or_else(|e| e.into_inner());
            std::mem::take(&mut *h)
        };
        for handle in handles {
            handle.abort();
        }
    }
}

impl Drop for ScopeState {
    fn drop(&mut self) {
        self.abort_all();
    }
}

impl Clone for CoroutineScope {
    fn clone(&self) -> Self { Self { state: Arc::clone(&self.state), rt: self.rt.clone() } }
}

impl CoroutineScope {
    /// A scope bound to the tokio runtime the composition is running on.
    pub(super) fn from_runtime() -> Self {
        let rt = Handle::try_current().expect(
            "remember_coroutine_scope requires an active tokio runtime. \
             `run_app` creates one internally; tests driving `Composer` directly must enter a runtime first."
        );
        CoroutineScope {
            state: Arc::new(ScopeState {
                handles: Mutex::new(Vec::new()),
                forget_hooked: std::sync::atomic::AtomicBool::new(false),
            }),
            rt,
        }
    }

    /// Spawn a coroutine; it is aborted automatically when the composable leaves the tree.
    pub fn spawn(&self, fut: impl std::future::Future<Output = ()> + Send + 'static) {
        let handle = self.rt.spawn(fut);
        let mut h = self.state.handles.lock().unwrap();
        h.retain(|jh| !jh.is_finished());
        h.push(handle);
    }

    /// Abort every task spawned through this scope without dropping the scope itself.
    ///
    /// Used on a key change: the body is cancelled and restarted, and the tasks it spawned belong
    /// to that cancelled body, the way a Compose coroutine's children die with it.
    pub(super) fn abort_spawned(&self) {
        self.state.abort_all();
    }
}

/// The coroutine scope bound to the current composition's lifetime.
///
/// Recompose-safe: `remember` returns the same scope for the same composition position, and
/// `ScopeState::drop` aborts every unfinished coroutine when the point is removed.
///
/// `#[composable]`: its internal `remember` takes a stable base from the call site's statement.
#[composable]
pub fn remember_coroutine_scope(ctx: &mut ComposeCtx) -> CoroutineScope {
    // `remember_handle`: the slot holds the scope itself, not a `State` wrapping it. There is
    // nothing to invalidate here, and a `State` would cost a `StateInner` plus a `StateSignal`
    // per effect instance for a value that is never written.
    let scope = ctx.remember_handle(CoroutineScope::from_runtime);

    hook_scope_teardown(ctx, &scope);
    scope
}

/// Register this composition point's teardown for `scope`: the tasks it spawned are aborted when the
/// point goes away, not when the last clone is dropped. Refcount teardown is a promise the caller can
/// accidentally break — keeping one clone alive (a struct field, or a task that captured the scope)
/// keeps every task the scope owns running past the point that owned it.
///
/// Idempotent per scope (`forget_hooked`), so the effects that remember a scope directly can call it
/// on every recomposition. Every place that creates a scope for a composition point has to call it;
/// `LaunchedEffect` did not, which is how a `scope.spawn` inside an effect body outlived its slot.
pub(super) fn hook_scope_teardown(ctx: &mut ComposeCtx, scope: &CoroutineScope) {
    if scope
        .state
        .forget_hooked
        .swap(true, std::sync::atomic::Ordering::Relaxed)
    {
        return;
    }
    let state = Arc::clone(&scope.state);
    ctx.remember_on_forget(move || state.abort_all());
}
