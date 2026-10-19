//! The composition-bound coroutine scope.

use crate::composable;
use crate::runtime::composer::ComposeCtx;
use std::sync::Arc;
use std::sync::Mutex;
use tokio::runtime::Handle;
use tokio::task::JoinHandle;

/// 组合生命周期绑定的协程作用域——dispose 时自动取消所有未完成任务。
///
/// 内部用 `Arc<ScopeState>` 管理任务列表。当最后一个 `CoroutineScope` clone
/// 被 drop 时（即组合点被移除），`ScopeState::drop` abort 所有未完成任务。
/// 重组时 `remember` 返回同一个 scope，不会触发 drop。
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

    /// 启动一个协程。当此 composable 离开组合树时，任务会被自动 abort。
    pub fn spawn(&self, fut: impl std::future::Future<Output = ()> + Send + 'static) {
        let handle = self.rt.spawn(fut);
        let mut h = self.state.handles.lock().unwrap();
        h.retain(|jh| !jh.is_finished());
        h.push(handle);
    }
}

/// 获取当前组合生命周期绑定的协程作用域。
/// 重组安全——remember 保证同一组合位置返回同一个 scope。
/// 当组合点被移除时，ScopeState::drop 自动 abort 所有未完成协程。
/// #[composable]：内部 remember 从调用点语句取稳定 base
#[composable]
pub fn remember_coroutine_scope(ctx: &mut ComposeCtx) -> CoroutineScope {
    // `remember_handle`: the slot holds the scope itself, not a `State` wrapping it. There is
    // nothing to invalidate here, and a `State` would cost a `StateInner` plus a `StateSignal`
    // per effect instance for a value that is never written.
    let scope = ctx.remember_handle(CoroutineScope::from_runtime);

    // Cancel on the composition point going away, not on the last clone being dropped. Refcount
    // teardown was the whole contract before, and it is a promise the caller can accidentally
    // break: keeping one clone alive (a struct field, a task spawned on the runtime directly) keeps
    // every task this scope owns running past the point that owned it.
    if !scope
        .state
        .forget_hooked
        .swap(true, std::sync::atomic::Ordering::Relaxed)
    {
        let state = Arc::clone(&scope.state);
        ctx.remember_on_forget(move || state.abort_all());
    }

    scope
}
