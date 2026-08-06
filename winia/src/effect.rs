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

/// 挂载一个组合点移除时的清理回调（P2-5 样板合并——
/// LaunchedEffect/DisposableEffect 共用：隐式 leaf + on_remove + end 配对）。
fn attach_cleanup(ctx: &mut ComposeCtx, cleanup: impl FnOnce() + Send + 'static) {
    let key = ctx.next_key();
    ctx.start_leaf_with_remove(key, crate::modifier::Modifier::new(), Box::new(cleanup));
    ctx.end_node();
}

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
        attach_cleanup(ctx, move || {
            let mut s = state_for_remove.lock().unwrap();
            if let Some(h) = s.abort_handle.take() {
                h.abort();
            }
        });
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
        attach_cleanup(ctx, move || {
            let mut s = state_for_remove.lock().unwrap();
            if let Some(cleanup) = s.cleanup.take() {
                cleanup();
            }
        });
    }
}

// ═══════════════════════════════════════════════════════════
// Stream → State 桥接
// ═══════════════════════════════════════════════════════════

/// 将 watch::Receiver 直接转为 State（不经过 WatchStream，send 始终可靠）
pub fn observe_watch<T: Clone + Send + Sync + PartialEq + 'static>(
    ctx: &mut ComposeCtx,
    rx: tokio::sync::watch::Receiver<T>,
    initial: T,
) -> State<T> {
    let rx = std::sync::Arc::new(parking_lot::Mutex::new(rx));
    let state: State<T> = ctx.remember(|| State::new(initial.clone())).get();
    let s = state.clone();
    let started: State<bool> = ctx.remember(|| false);
    if !started.get() {
        started.set(true);
        let scope = remember_coroutine_scope(ctx);
        scope.spawn(async move {
            loop {
                let changed = rx.lock().changed().await;
                if changed.is_err() { break; }
                let v = rx.lock().borrow_and_update().clone();
                s.set(v);
            }
        });
    }
    state
}

// ═══════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════
// 帧时钟（P3-12）——对标 Compose withFrameNanos
// ═══════════════════════════════════════════════════════════

use std::sync::LazyLock;
use tokio::sync::broadcast;

/// 帧时钟发送端——渲染循环（RedrawRequested 处理器）每帧调用 [`frame_tick`]。
/// broadcast 队列化每个帧时间戳：订阅者收到**下一帧**（无 watch 的跳过语义）。
/// static 持有 receiver 防 send 因无订阅者而失败。
static FRAME_CLOCK: LazyLock<(broadcast::Sender<u64>, broadcast::Receiver<u64>)> = LazyLock::new(|| {
    let (tx, rx) = broadcast::channel(16);
    (tx, rx)
});

static FRAME_EPOCH: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

/// 单调时钟纳秒（Instant 基准——墙钟 SystemTime 可能回拨，导致 dt 为负/巨大）
fn monotonic_nanos() -> u64 {
    let epoch = *FRAME_EPOCH.get_or_init(std::time::Instant::now);
    epoch.elapsed().as_nanos() as u64
}

/// 帧循环每帧调用（app.rs RedrawRequested 内注入）——向所有等待者广播帧时间戳。
/// 无副作用失败：无订阅者时 send 返回 Err，忽略（帧时钟只是辅助驱动）。
pub fn frame_tick() {
    let _ = FRAME_CLOCK.0.send(monotonic_nanos());
}

/// 等待下一帧并返回帧时间戳（纳秒，UNIX epoch 基准）——
/// 自定义动画的帧驱动入口（对标 Compose `withFrameNanos`）。
///
/// 用法（LaunchedEffect / CoroutineScope 内）：
/// ```ignore
/// scope.spawn(async move {
///     let mut last = 0u64;
///     loop {
///         let now = winia::effect::with_frame_nanos().await;
///         let dt = (now - last) as f32 / 1e9;
///         last = now;
///         // 按 dt 推进自定义状态……
///     }
/// });
/// ```
///
/// 注意：帧时钟由渲染循环驱动——窗口未渲染（最小化/无 request_redraw）时
/// 不会返回。需要"时间驱动"而非"帧驱动"请用 tokio sleep。
pub fn with_frame_nanos() -> impl std::future::Future<Output = u64> + Send {
    // 非 async fn：subscribe 在**调用时**执行（async fn 在首次 poll 才执行——
    // 会错过调用与 await 之间 frame_tick 发出的帧）
    let mut rx = FRAME_CLOCK.0.subscribe();
    async move {
        loop {
            match rx.recv().await {
                Ok(v) => return v,
                // 订阅晚于缓冲区淘汰（长时间无渲染后恢复）——跳过丢失帧继续等
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return 0, // 发送端消亡——兜底
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// frame_tick 广播后，with_frame_nanos 返回新时间戳（非初始 0）
    #[test]
    fn frame_clock_ticks_and_waits() {
        let rt = tokio::runtime::Builder::new_current_thread().enable_time().build().unwrap();
        rt.block_on(async {
            let fut = with_frame_nanos();
            frame_tick();
            let v = fut.await;
            assert!(v > 0, "帧时间戳必须 > 0（实际 {v}）");
            // 第二次 tick 返回更新的时间戳
            let fut2 = with_frame_nanos();
            std::thread::sleep(std::time::Duration::from_millis(2));
            frame_tick();
            let v2 = fut2.await;
            assert!(v2 > v, "第二次 tick 时间戳必须递增（{v} -> {v2}）");
        });
    }
}
