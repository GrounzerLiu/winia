//! Effect API — 类似 Jetpack Compose 的 LaunchedEffect / DisposableEffect。
//!
//! - LaunchedEffect: key 变化时启动异步任务，离开组合时自动取消
//! - DisposableEffect: key 变化时执行同步副作用，离开组合时清理
//! - remember_coroutine_scope: 获取组合生命周期绑定的协程作用域
//! - use_stream: 将 Stream 转为组合生命周期绑定的 State
//!
//! 依赖 tokio 运行时（已作为项目依赖）。

use crate::core::composer::ComposeCtx;
use crate::core::state::State;
use std::sync::Arc;
use std::sync::Mutex;
use tokio::runtime::Handle;
use tokio::task::JoinHandle;

// ═══════════════════════════════════════════════════════════
// remember_coroutine_scope
// ═══════════════════════════════════════════════════════════

/// 组合生命周期绑定的协程作用域——dispose 时自动取消所有未完成任务。
///
/// 内部用 `Arc<ScopeState>` 管理任务列表。当最后一个 `CoroutineScope` clone
/// 被 drop 时（即组合点被移除），`ScopeState::drop` abort 所有未完成任务。
/// 重组时 `remember` 返回同一个 scope，不会触发 drop。
pub struct CoroutineScope {
    state: Arc<ScopeState>,
    rt: Handle,
}

struct ScopeState {
    handles: Mutex<Vec<JoinHandle<()>>>,
}

impl Drop for ScopeState {
    fn drop(&mut self) {
        let mut h = self.handles.lock().unwrap();
        for handle in h.drain(..) {
            handle.abort();
        }
    }
}

impl Clone for CoroutineScope {
    fn clone(&self) -> Self { Self { state: Arc::clone(&self.state), rt: self.rt.clone() } }
}

impl CoroutineScope {
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
pub fn remember_coroutine_scope(ctx: &mut ComposeCtx) -> CoroutineScope {
    ctx.remember(|| {
        let rt = Handle::try_current().expect(
            "remember_coroutine_scope requires an active tokio runtime. \
             Start one with `tokio::runtime::Runtime::new()` before calling `run_app`."
        );
        CoroutineScope { state: Arc::new(ScopeState { handles: Mutex::new(Vec::new()) }), rt }
    }).get()
}

// ═══════════════════════════════════════════════════════════
// LaunchedEffect
// ═══════════════════════════════════════════════════════════

/// 内部状态
struct LaunchedEffectState<T: PartialEq> {
    prev_key: Option<T>,
    abort_handle: Option<tokio::task::AbortHandle>,
}

/// key 变化时在协程中执行 block，离开组合时自动取消。
pub struct LaunchedEffect<T: PartialEq + Clone + Send + 'static> {
    key: T,
}

impl<T: PartialEq + Clone + Send + 'static> LaunchedEffect<T> {
    pub fn new(key: T) -> Self { Self { key } }

    /// 创建一个 key 为 `()` 的 LaunchedEffect——只在首次组合时执行一次。
    pub fn unit() -> LaunchedEffect<()> { LaunchedEffect { key: () } }

    pub fn build<F: std::future::Future<Output = ()> + Send + 'static>(
        self,
        ctx: &mut ComposeCtx,
        block: impl FnOnce(CoroutineScope) -> F + Send + 'static,
    ) {
        let scope = remember_coroutine_scope(ctx);

        let state: crate::core::state::State<Arc<Mutex<LaunchedEffectState<T>>>> =
            ctx.remember(|| Arc::new(Mutex::new(LaunchedEffectState { prev_key: None, abort_handle: None })));

        let state_clone = Arc::clone(&state.get());
        let key_changed = {
            let s = state_clone.lock().unwrap();
            s.prev_key.as_ref() != Some(&self.key)
        };

        if key_changed {
            {
                let mut s = state_clone.lock().unwrap();
                if let Some(h) = s.abort_handle.take() {
                    h.abort();
                }
            }

            let fut = block(scope.clone());
            let join_handle = scope.rt.spawn(fut);
            {
                let mut s = state_clone.lock().unwrap();
                s.abort_handle = Some(join_handle.abort_handle());
                s.prev_key = Some(self.key);
            }
        }

        // on_remove 时取消
        let state_for_remove = Arc::clone(&state_clone);
        let key2 = ctx.next_key();
        ctx.start_leaf_with_remove(key2, crate::modifier::Modifier::new(), Box::new(move || {
            let mut s = state_for_remove.lock().unwrap();
            if let Some(h) = s.abort_handle.take() {
                h.abort();
            }
        }));
        ctx.end_node();
    }
}

// ═══════════════════════════════════════════════════════════
// DisposableEffect
// ═══════════════════════════════════════════════════════════

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

    /// 创建一个 key 为 `()` 的 DisposableEffect——只在首次组合时执行 setup，dispose 时 cleanup。
    pub fn unit() -> DisposableEffect<()> { DisposableEffect { key: () } }

    pub fn build<F: FnOnce() + Send + 'static>(
        self,
        ctx: &mut ComposeCtx,
        effect: impl Fn(T) -> F + Send + Sync + 'static,
    ) {
        let state: crate::core::state::State<Arc<Mutex<DisposableState<T>>>> =
            ctx.remember(|| Arc::new(Mutex::new(DisposableState { prev_key: None, cleanup: None })));

        let state_clone = Arc::clone(&state.get());
        let key_changed = {
            let s = state_clone.lock().unwrap();
            s.prev_key.as_ref() != Some(&self.key)
        };

        if key_changed {
            let mut s = state_clone.lock().unwrap();
            // 先执行旧清理
            if let Some(cleanup) = s.cleanup.take() {
                cleanup();
            }
            // 执行新 setup，框架负责 Box 包装
            s.cleanup = Some(Box::new(effect(self.key.clone())));
            s.prev_key = Some(self.key);
        }

        // on_remove 时执行最终清理
        let state_for_remove = Arc::clone(&state_clone);
        let key2 = ctx.next_key();
        ctx.start_leaf_with_remove(key2, crate::modifier::Modifier::new(), Box::new(move || {
            let mut s = state_for_remove.lock().unwrap();
            if let Some(cleanup) = s.cleanup.take() {
                cleanup();
            }
        }));
        ctx.end_node();
    }
}

// ═══════════════════════════════════════════════════════════
// StreamObverse — trait: stream.observe(ctx, initial) → State
// ═══════════════════════════════════════════════════════════

pub trait StreamObverse: futures_util::Stream {
    fn observe(self, ctx: &mut ComposeCtx, initial: Self::Item) -> State<Self::Item>
    where
        Self: Sized + Send + 'static,
        Self::Item: Clone + Send + Sync + PartialEq + 'static;
}

impl<S: futures_util::Stream + Send + 'static> StreamObverse for S {
    fn observe(self, ctx: &mut ComposeCtx, initial: S::Item) -> State<S::Item>
    where
        S::Item: Clone + Send + Sync + PartialEq + 'static,
    {
        let state: State<S::Item> = ctx.remember(|| State::new(initial.clone())).get();
        let s = state.clone();
        let scope = remember_coroutine_scope(ctx);
        scope.spawn(async move {
            use futures_util::StreamExt;
            let mut stream = Box::pin(self);
            while let Some(value) = stream.next().await {
                s.set(value);
            }
        });
        state
    }
}
