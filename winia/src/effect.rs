//! Effect API — 类似 Jetpack Compose 的 LaunchedEffect / DisposableEffect。
//!
//! - LaunchedEffect: key 变化时启动异步任务，离开组合时自动取消
//! - DisposableEffect: key 变化时执行同步副作用，离开组合时清理
//! - remember_coroutine_scope: 获取组合生命周期绑定的协程作用域
//! - observe_watch: 将 watch channel 转为组合生命周期绑定的 State
//!
//! 依赖 tokio 运行时（已作为项目依赖）。
//!
//! # Timing: these run DURING composition, not after it
//!
//! Compose runs `DisposableEffect`'s setup/cleanup and starts `LaunchedEffect`'s coroutine after the
//! composition is applied. Here both happen inline in the compose pass: `DisposableEffect`'s setup and
//! cleanup are plain synchronous calls from `build`, and `LaunchedEffect` spawns its task from the same
//! place. Two consequences a caller has to know about:
//!
//! - A setup/cleanup that reads layout, window size or a `State` another branch of the same
//!   composition is still writing sees a HALF-BUILT tree, not the frame that is about to be shown.
//! - Writing a `State` from a setup marks its readers dirty while they may still be composing in this
//!   same pass.
//!
//! Moving effects after the apply phase needs a commit hook the composer does not have yet, so this is
//! documented rather than changed: treat an effect body as "runs somewhere inside the frame", and keep
//! it to work that does not depend on the frame being finished.
//!
//! # Lock discipline
//!
//! User code (the effect body and the cleanup a caller returns) is NEVER invoked while one of this
//! module's `Mutex` guards is alive. A panic in it would poison the guard, and every later
//! `lock().unwrap()` in that effect would panic — the instance would be dead for the rest of the run.
//! A caller that re-enters the same state from its cleanup would deadlock outright, `std::sync::Mutex`
//! being non-reentrant. Every site below therefore takes what it needs under the lock and calls out
//! after releasing it.

use crate::composable;
use crate::runtime::composer::ComposeCtx;
use crate::runtime::state::State;
use std::sync::Arc;
use std::sync::Mutex;
use tokio::runtime::Handle;
use tokio::task::JoinHandle;

// ═══════════════════════════════════════════════════════════
// remember_coroutine_scope
// ═══════════════════════════════════════════════════════════

/// Attach a cleanup to the composition point being built: it runs when that slot is dropped.
///
/// This used to materialize an implicit LEAF NODE and hang the callback on its `on_remove`, because
/// the slot table had no other place to put one. The node was a real child of the enclosing
/// container, which cost more than a wasted allocation: an effect inside `Column(spacing = 10)`
/// pushed its next sibling down by one spacing slot (measured: y 29 -> 39), so a `LaunchedEffect`
/// between two rows opened a gap that nothing in the layout asked for. `remember_on_forget` keeps
/// the callback in the slot itself — the same place Compose keeps a `RememberObserver` — and
/// materializes nothing.
///
/// Still `#[composable]`: its scope slot is the thing that dies with the call site, so a branch
/// that stops composing takes the callback with it.
#[composable]
fn attach_cleanup(ctx: &mut ComposeCtx, cleanup: impl FnOnce() + Send + 'static) {
    on_forget(ctx, cleanup);
}

/// Register `cleanup` for the composition point being built here: it runs when that point is
/// removed, and never on a plain recomposition.
///
/// The `#[composable]` is load-bearing — it gives the call its own scope slot, which is what dies
/// when the branch stops composing. [`ComposeCtx::remember_on_forget`] is the same hook without
/// that per-call slot: calling the method directly attaches the callback to whatever slot is
/// current, so a component that stops using a resource while staying composed keeps the callback.
/// Prefer this function unless the callback belongs to the whole enclosing slot.
///
/// No layout node is created — see `remember_on_forget`'s doc for the bug that used to.
#[composable]
pub fn on_forget(ctx: &mut ComposeCtx, cleanup: impl FnOnce() + Send + 'static) {
    ctx.remember_on_forget(cleanup);
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
/// #[composable]：内部 remember 从调用点语句取稳定 base
#[composable]
pub fn remember_coroutine_scope(ctx: &mut ComposeCtx) -> CoroutineScope {
    // `.peek()`: the slot holds a handle, and nothing ever writes it. A tracked read would only
    // register a dependency edge that can never fire (AGENTS.md, "remember and State handles").
    ctx.remember(|| {
        let rt = Handle::try_current().expect(
            "remember_coroutine_scope requires an active tokio runtime. \
             `run_app` creates one internally; tests driving `Composer` directly must enter a runtime first."
        );
        CoroutineScope { state: Arc::new(ScopeState { handles: Mutex::new(Vec::new()) }), rt }
    }).peek()
}

// ═══════════════════════════════════════════════════════════
// LaunchedEffect
// ═══════════════════════════════════════════════════════════

/// 内部状态
struct LaunchedEffectState<T: PartialEq> {
    prev_key: Option<T>,
    /// The RUNNING task, not just its abort handle: replacing the effect has to wait for the old
    /// body to actually stop. `AbortHandle::abort` only asks — it returns immediately, so the old
    /// body can still be mid-await when the new one starts, and the two then race for whatever
    /// shared state they touch. Compose's `LaunchedEffect` joins for that reason.
    task: Option<tokio::task::JoinHandle<()>>,
}

/// key 变化时在协程中执行 block，离开组合时自动取消。
pub struct LaunchedEffect<T: PartialEq + Clone + Send + 'static> {
    key: T,
}

impl<T: PartialEq + Clone + Send + 'static> LaunchedEffect<T> {
    /// 组合生命周期绑定的协程作用域——dispose 时自动取消所有未完成任务。
    pub fn new(key: T) -> Self { Self { key } }

    /// #[composable]：内部 remember/attach_cleanup 从 build 调用点取稳定 base
    #[composable]
    pub fn build<F: std::future::Future<Output = ()> + Send + 'static>(
        self,
        ctx: &mut ComposeCtx,
        block: impl FnOnce(CoroutineScope) -> F + Send + 'static,
    ) {
        let scope = remember_coroutine_scope(ctx);

        let state: crate::runtime::state::State<Arc<Mutex<LaunchedEffectState<T>>>> =
            ctx.remember(|| Arc::new(Mutex::new(LaunchedEffectState { prev_key: None, task: None })));

        // `.peek()`: this slot holds the effect's bookkeeping, never a value the framework writes,
        // so a tracked read would only add a dependency edge that can never fire.
        let state_clone = Arc::clone(&state.peek());
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
        }

        // on_remove 时取消
        let state_for_remove = Arc::clone(&state_clone);
        attach_cleanup(ctx, move || {
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

    /// #[composable]：内部 remember/attach_cleanup 从 build 调用点取稳定 base
    #[composable]
    pub fn build<F: FnOnce() + Send + 'static>(
        self,
        ctx: &mut ComposeCtx,
        effect: impl Fn(T) -> F + Send + Sync + 'static,
    ) {
        let state: crate::runtime::state::State<Arc<Mutex<DisposableState<T>>>> =
            ctx.remember(|| Arc::new(Mutex::new(DisposableState { prev_key: None, cleanup: None })));

        // `.peek()`: same reason as LaunchedEffect above — the slot is bookkeeping, not state.
        let state_clone = Arc::clone(&state.peek());
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
        }

        // on_remove 时执行最终清理
        let state_for_remove = Arc::clone(&state_clone);
        attach_cleanup(ctx, move || {
            let taken = {
                let mut s = state_for_remove.lock().unwrap();
                s.cleanup.take()
            };
            // Outside the lock, same reason as above: this runs during composition teardown and a
            // panic here must not take the whole effect mechanism down with it.
            if let Some(cleanup) = taken {
                cleanup();
            }
        });
    }
}

// ═══════════════════════════════════════════════════════════
// Stream → State bridging
// ═══════════════════════════════════════════════════════════

/// What `observe_watch` remembers across recompositions: the receiver it is currently subscribed to
/// and the task following it.
///
/// The task is a `JoinHandle`, not an `AbortHandle`, for the same reason
/// `LaunchedEffectState::task` is one: `abort` only asks, so a replacement that started at once
/// would overlap the old task until it unwinds — and both of them write the same `State`.
struct WatchSubscription<T> {
    rx: Option<tokio::sync::watch::Receiver<T>>,
    task: Option<tokio::task::JoinHandle<()>>,
}

/// Bridge a `watch::Receiver` straight into a `State` (no `WatchStream` hop: `send` is always
/// reliable). `#[composable]`: the two remembers and the coroutine scope take a stable base from
/// the call site.
///
/// A REPLACED receiver is re-subscribed. The first version gated the spawn on a one-shot `started`
/// flag, so passing a different channel at the same call site silently kept listening to the old one
/// — the `rx` argument was honoured exactly once and then ignored for the lifetime of the slot.
/// Compose's `produceState(key = rx)` restarts for the same reason.
#[composable]
pub fn observe_watch<T: Clone + Send + Sync + PartialEq + 'static>(
    ctx: &mut ComposeCtx,
    rx: tokio::sync::watch::Receiver<T>,
    initial: T,
) -> State<T> {
    // `remember` already returns a `State<T>`, so the slot holds the value itself. The `State::new`
    // wrapper it replaced only added a layer: the outer handle was written once at init and read once
    // per frame, so it could never invalidate anything the inner one did not.
    let state: State<T> = ctx.remember(|| initial.clone());
    let slot: State<Arc<Mutex<WatchSubscription<T>>>> = ctx.remember(|| {
        Arc::new(Mutex::new(WatchSubscription { rx: None, task: None }))
    });
    let slot_arc = Arc::clone(&slot.peek());

    // `same_channel` is the receiver's identity test: two clones of one channel answer true, two
    // channels answer false. That is the key this effect restarts on.
    let needs_subscribe = {
        let s = slot_arc.lock().unwrap();
        match &s.rx {
            Some(current) => !current.same_channel(&rx),
            None => true,
        }
    };

    if needs_subscribe {
        let scope = remember_coroutine_scope(ctx);
        // Take the old handle out of the slot OUTSIDE the lock, then abort it: the abort itself is
        // not user code, but this file keeps every task-handle call off the shared guard.
        let previous = {
            let mut s = slot_arc.lock().unwrap();
            s.task.take()
        };
        if let Some(old) = &previous {
            old.abort();
        }
        let s_state = state.clone();
        // Keep one clone for the slot's identity check; the task owns the other.
        let rx_keep = rx.clone();
        // The task owns the receiver instead of sharing it behind a lock: `changed()` borrows the
        // receiver for the length of the wait, so a guard alive across that await would make the
        // future `!Send` — and it would be held for the whole, indefinite wait besides.
        //
        // The replacement waits for the aborted task to stop before following the new channel.
        // Without the join the two overlap until the abort unwinds, and the old task can still run
        // the `set` that follows its last `changed()` — writing the PREVIOUS channel's value over
        // the new one. `LaunchedEffect` joins for exactly this reason.
        let handle = scope.rt.spawn(async move {
            if let Some(old) = previous {
                let _ = old.await;
            }
            follow_watch(rx, s_state).await;
        });
        let mut s = slot_arc.lock().unwrap();
        s.rx = Some(rx_keep);
        s.task = Some(handle);
    }

    // Leaving the composition stops the subscription too. The task is spawned on the runtime
    // directly (`rt.spawn`, not `CoroutineScope::spawn`), so `ScopeState::drop` does not know
    // about it, and dropping an `AbortHandle` cancels nothing: without this the task outlived the
    // slot and kept following a channel whose `State` had no readers left.
    let slot_for_remove = Arc::clone(&slot_arc);
    attach_cleanup(ctx, move || {
        let taken = {
            let mut s = slot_for_remove.lock().unwrap();
            s.task.take()
        };
        if let Some(task) = taken {
            task.abort();
        }
    });

    state
}

/// Follow `rx` into `state` until the channel closes or the task is aborted.
async fn follow_watch<T>(mut rx: tokio::sync::watch::Receiver<T>, state: State<T>)
where
    T: Clone + Send + Sync + PartialEq + 'static,
{
    // Seed from the channel's CURRENT value, so a re-subscription lands on the new source instead of
    // keeping whatever the previous receiver had left in the State.
    state.set(rx.borrow_and_update().clone());
    loop {
        if rx.changed().await.is_err() {
            break;
        }
        state.set(rx.borrow_and_update().clone());
    }
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
    // max(1)：首次初始化与调用同纳秒时 elapsed=0（测试断言 > 0 会失败）
    (epoch.elapsed().as_nanos() as u64).max(1)
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
    // 同步丢弃订阅时刻已存在的旧帧：broadcast 新 receiver 的游标从当前
    // 最新开始——不丢弃的话 recv 会立即返回"已经发生"的帧而非"下一帧"
    //（并行测试 poll 时机不定——flaky 根因）。
    // ⚠ 必须在这里同步执行：放进 async body 会在首次 poll 才运行——若
    // frame_tick 发生在 subscribe 与 poll 之间，会把"下一帧"误当旧帧丢弃
    // → 死等（曾引入此 bug）。
    let _ = rx.try_recv();
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


    // ── Effects are not layout citizens ──

    /// An effect occupies no layout node, so it does not shift its neighbours.
    ///
    /// This is the regression test for the reason `remember_on_forget` exists. The cleanup used to
    /// ride on an implicit LEAF NODE, which was a real child of the enclosing container: an effect
    /// sitting between two rows of a `Column(spacing = 10)` pushed the second row from y=29 to y=39,
    /// i.e. opened a one-spacing gap that nothing in the layout asked for.
    #[test]
    fn an_effect_adds_no_layout_node_and_no_spacing_slot() {
        let sample = |with_effect: bool| -> (Vec<f32>, usize) {
            let mut composer = crate::runtime::composer::Composer::new();
            composer.compose(|ctx: &mut ComposeCtx| {
                crate::layout::Column::new().spacing(10.0).build(ctx, |ctx| {
                    crate::components::Text::new("first").build(ctx);
                    if with_effect {
                        DisposableEffect::new(()).build(ctx, |_| Box::new(|| {}));
                    }
                    crate::components::Text::new("second").build(ctx);
                });
            });
            composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
            let nodes = composer.arena_nodes();
            let root = composer.layout_root_idx().expect("root");
            let ys = nodes[root].children.iter().map(|&c| nodes[c].position.y).collect();
            (ys, nodes.len())
        };
        let (plain, plain_nodes) = sample(false);
        let (with, with_nodes) = sample(true);
        assert_eq!(
            with, plain,
            "an effect must not move its neighbours: without it {plain:?}, with it {with:?}"
        );
        assert_eq!(
            with_nodes, plain_nodes,
            "and it must not allocate a layout node: {plain_nodes} vs {with_nodes}"
        );
    }


    /// Dropping the whole `Composer` runs the callbacks too.
    ///
    /// This is the path with no explicit collector, and it used to work by accident: the callback
    /// rode on a `LayoutNode`, which has a `Drop` that fires `on_remove`. Moving the callback into
    /// the slot dropped that guarantee — measured on `remember_infinite_transition`: its animation id
    /// stayed in the global table after the composer went away (1 before the drop, still 1 after).
    /// `Slot` has its own `Drop` now, and this pins it.
    #[test]
    fn dropping_the_composer_runs_the_forget_callbacks() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc as StdArc;
        let ran = StdArc::new(AtomicUsize::new(0));
        {
            let mut composer = crate::runtime::composer::Composer::new();
            let r = ran.clone();
            composer.compose(move |ctx: &mut ComposeCtx| {
                let r2 = r.clone();
                DisposableEffect::new(()).build(ctx, move |_| {
                    let r3 = r2.clone();
                    Box::new(move || {
                        r3.fetch_add(1, Ordering::SeqCst);
                    })
                });
            });
            composer.layout(crate::layout::Constraints::new(0.0, 100.0, 0.0, 100.0));
            assert_eq!(ran.load(Ordering::SeqCst), 0, "nothing has been removed yet");
        }
        assert_eq!(
            ran.load(Ordering::SeqCst),
            1,
            "dropping the composer must run the cleanup, not leak it"
        );
    }
    // ── DisposableEffect: the lifecycle it promises ──

    /// Setup runs once for one key, and a key change runs the OLD cleanup before the new setup.
    /// The order matters: a setup that installs a listener and a cleanup that removes it must not
    /// overlap, or the new listener is removed by the old cleanup.
    #[test]
    fn a_key_change_runs_the_old_cleanup_before_the_new_setup() {
        use std::sync::{Arc as StdArc, Mutex as StdMutex};
        let log = StdArc::new(StdMutex::new(Vec::<String>::new()));
        let mut composer = crate::runtime::composer::Composer::new();
        let mut frame = |composer: &mut crate::runtime::composer::Composer, key: u32| {
            let l = log.clone();
            composer.compose(move |ctx: &mut ComposeCtx| {
                let l2 = l.clone();
                DisposableEffect::new(key).build(ctx, move |k| {
                    l.lock().unwrap().push(format!("setup {k}"));
                    // The effect closure is `Fn`, so it may be called again: clone per call.
                    let l3 = l2.clone();
                    Box::new(move || l3.lock().unwrap().push(format!("cleanup {k}")))
                });
            });
            composer.layout(crate::layout::Constraints::new(0.0, 100.0, 0.0, 100.0));
        };
        frame(&mut composer, 1);
        assert_eq!(*log.lock().unwrap(), vec!["setup 1".to_string()], "first frame: setup only");
        frame(&mut composer, 1);
        assert_eq!(
            *log.lock().unwrap(),
            vec!["setup 1".to_string()],
            "the same key must not re-run the effect"
        );
        frame(&mut composer, 2);
        assert_eq!(
            *log.lock().unwrap(),
            vec!["setup 1".to_string(), "cleanup 1".to_string(), "setup 2".to_string()],
            "key change: old cleanup, then new setup"
        );
    }

    /// Leaving the composition runs the last cleanup.
    #[test]
    fn leaving_the_composition_runs_the_cleanup() {
        use std::sync::{Arc as StdArc, Mutex as StdMutex};
        let log = StdArc::new(StdMutex::new(Vec::<String>::new()));
        let mut composer = crate::runtime::composer::Composer::new();
        let show = crate::runtime::state::State::new(true);
        let mut frame = |composer: &mut crate::runtime::composer::Composer| {
            let l = log.clone();
            let s = show.clone();
            composer.compose(move |ctx: &mut ComposeCtx| {
                if s.get() {
                    let l2 = l.clone();
                    DisposableEffect::new(()).build(ctx, move |_| {
                        l.lock().unwrap().push("setup".to_string());
                        let l3 = l2.clone();
                        Box::new(move || l3.lock().unwrap().push("cleanup".to_string()))
                    });
                }
            });
            composer.layout(crate::layout::Constraints::new(0.0, 100.0, 0.0, 100.0));
        };
        frame(&mut composer);
        assert_eq!(*log.lock().unwrap(), vec!["setup".to_string()]);
        show.set(false);
        frame(&mut composer);
        assert_eq!(
            *log.lock().unwrap(),
            vec!["setup".to_string(), "cleanup".to_string()],
            "the branch is gone, so the cleanup must have run"
        );
    }

    /// A PANICKING cleanup must not poison the effect's lock. The cleanup used to run with the
    /// mutex guard alive, so a panic left it poisoned and every later `lock().unwrap()` in that
    /// effect panicked — the instance was dead for the rest of the run. The cleanup is called
    /// outside the guard now, so the next key change still works.
    #[test]
    fn a_panicking_cleanup_does_not_kill_the_effect() {
        use std::sync::{Arc as StdArc, Mutex as StdMutex};
        let setups = StdArc::new(StdMutex::new(0usize));
        let mut composer = crate::runtime::composer::Composer::new();
        let mut frame = |composer: &mut crate::runtime::composer::Composer, key: u32| {
            let n = setups.clone();
            composer.compose(move |ctx: &mut ComposeCtx| {
                let n2 = n.clone();
                DisposableEffect::new(key).build(ctx, move |k| {
                    *n.lock().unwrap() += 1;
                    let cleanup: Box<dyn FnOnce() + Send> = if k == 1 {
                        // The FIRST effect's cleanup panics; the second's is well behaved.
                        Box::new(|| panic!("cleanup blew up"))
                    } else {
                        let _ = n2.clone();
                        Box::new(|| {})
                    };
                    cleanup
                });
            });
            composer.layout(crate::layout::Constraints::new(0.0, 100.0, 0.0, 100.0));
        };
        frame(&mut composer, 1);
        // The key change runs the panicking cleanup. Swallow it: this test is about what the lock
        // looks like afterwards, not about propagating a caller's panic.
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| frame(&mut composer, 2)));
        assert!(panicked.is_err(), "the caller's cleanup really did panic");
        assert_eq!(*setups.lock().unwrap(), 1, "setup 2 had not run yet when the panic escaped");

        // THE POINT: the effect still works. With the old code the cleanup ran under the guard, so
        // this second key change would panic on `lock().unwrap()` with a PoisonError instead of
        // running setup 3.
        frame(&mut composer, 3);
        assert_eq!(
            *setups.lock().unwrap(),
            2,
            "a panicking cleanup must not poison the effect's lock: setup 3 has to run"
        );
    }

    // ── observe_watch: which channel is it actually listening to ──

    /// A REPLACED receiver is re-subscribed. The first version spawned once behind a `started`
    /// flag, so a different channel at the same call site was silently ignored and the state kept
    /// following the old one forever.
    #[test]
    fn observe_watch_follows_a_replaced_receiver() {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let _e = rt.enter();
        let (tx1, rx1) = tokio::sync::watch::channel(1u32);
        let (tx2, rx2) = tokio::sync::watch::channel(2u32);
        let mut composer = crate::runtime::composer::Composer::new();
        let mut compose_with = |composer: &mut crate::runtime::composer::Composer,
                                rx: tokio::sync::watch::Receiver<u32>| {
            composer.compose(move |ctx: &mut ComposeCtx| {
                let v = observe_watch(ctx, rx.clone(), 0);
                let _ = v.get();
            });
            composer.layout(crate::layout::Constraints::new(0.0, 100.0, 0.0, 100.0));
        };
        compose_with(&mut composer, rx1.clone());
        // Same call site, a DIFFERENT channel: this is the replacement.
        compose_with(&mut composer, rx2.clone());
        // Drive the runtime so the spawned subscription tasks reach their first await.
        rt.block_on(async { for _ in 0..50 { tokio::task::yield_now().await; } });
        // Send on both channels; only the CURRENT one may reach the state.
        tx1.send(11).unwrap();
        tx2.send(22).unwrap();
        rt.block_on(async { for _ in 0..50 { tokio::task::yield_now().await; } });
        // Read the slot's value by composing once more with the same receiver. The value has to come
        // back through an AtomicU32: `compose` takes a `move` closure, so a plain `let mut seen`
        // would be captured BY VALUE and the assignment inside would never leave the closure.
        let seen = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let seen_out = seen.clone();
        let r2 = rx2.clone();
        composer.compose(move |ctx: &mut ComposeCtx| {
            let v = observe_watch(ctx, r2.clone(), 0);
            seen_out.store(v.get(), std::sync::atomic::Ordering::SeqCst);
        });
        assert_eq!(
            seen.load(std::sync::atomic::Ordering::SeqCst),
            22,
            "the state must follow the REPLACED receiver (22), not the one it started on (11)"
        );
    }

    // ── LaunchedEffect: replacement order ──

    /// A key change ABORTS the old body (a parked future is dropped, it does not linger) and runs the
    /// replacement exactly once.
    ///
    /// KNOWN GAP, recorded rather than hidden: this does NOT prove the replacement waits for the old
    /// body to stop. The mutation that removes that join — spawning the new body directly — still
    /// passes here, because on a `current_thread` runtime the abort lands before the new task is
    /// polled either way. The join matters for the multi-threaded runtime `run_app` builds, where the
    /// two tasks really do run on different workers, and that interleaving is not reproducible with a
    /// deterministic test on this harness. The ordering is therefore a code-level guarantee with a
    /// comment saying why, not a pinned behaviour.
    #[test]
    fn a_launched_effect_key_change_aborts_the_old_body() {
        use std::sync::{Arc as StdArc, Mutex as StdMutex};
        struct Marker(StdArc<StdMutex<Vec<&'static str>>>, &'static str);
        impl Drop for Marker {
            fn drop(&mut self) { self.0.lock().unwrap().push(self.1); }
        }
        let log = StdArc::new(StdMutex::new(Vec::<&'static str>::new()));
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let _e = rt.enter();
        let mut composer = crate::runtime::composer::Composer::new();
        let mut frame = |composer: &mut crate::runtime::composer::Composer, key: u32| {
            let l = log.clone();
            composer.compose(move |ctx: &mut ComposeCtx| {
                let l2 = l.clone();
                LaunchedEffect::new(key).build(ctx, move |_scope| {
                    let l3 = l2.clone();
                    async move {
                        // Held across an await, so it is dropped only when this future is dropped
                        // (aborted) or completes.
                        let _guard = Marker(l3.clone(), "old dropped");
                        l3.lock().unwrap().push("old start");
                        // Park forever: only the abort ends this body.
                        std::future::pending::<()>().await;
                    }
                });
            });
            composer.layout(crate::layout::Constraints::new(0.0, 100.0, 0.0, 100.0));
        };
        frame(&mut composer, 1);
        rt.block_on(async { for _ in 0..10 { tokio::task::yield_now().await; } });
        assert_eq!(*log.lock().unwrap(), vec!["old start"], "the first body started and parked");

        // Replace it with a body that logs when it starts.
        let l = log.clone();
        composer.compose(move |ctx: &mut ComposeCtx| {
            let l2 = l.clone();
            LaunchedEffect::new(2u32).build(ctx, move |_scope| {
                let l3 = l2.clone();
                async move { l3.lock().unwrap().push("new start"); }
            });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 100.0, 0.0, 100.0));
        rt.block_on(async { for _ in 0..40 { tokio::task::yield_now().await; } });

        let seen = log.lock().unwrap().clone();
        let dropped_at = seen.iter().position(|e| *e == "old dropped");
        let started_at = seen.iter().position(|e| *e == "new start");
        assert!(dropped_at.is_some(), "the aborted body was dropped, not left parked: {seen:?}");
        assert!(started_at.is_some(), "the replacement body ran: {seen:?}");
        assert_eq!(
            seen.iter().filter(|e| **e == "new start").count(),
            1,
            "and it ran exactly once: {seen:?}"
        );
    }
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

    // ── observe_watch: leaving the composition stops the subscription ──

    /// Removing the composition point stops the watch task.
    ///
    /// The task is spawned on the runtime directly (`rt.spawn`, not `CoroutineScope::spawn`), so
    /// `ScopeState::drop` does not know about it, and dropping an `AbortHandle` cancels nothing.
    /// Before the `on_forget` hook was added the task outlived the slot and kept writing the
    /// `State` after the composition point (and every reader of it) was gone.
    #[test]
    fn observe_watch_stops_when_the_composition_point_is_removed() {
        use std::sync::{Arc as StdArc, Mutex as StdMutex};
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let _e = rt.enter();
        let (tx, rx) = tokio::sync::watch::channel(1u32);
        let mut composer = crate::runtime::composer::Composer::new();
        let kept: StdArc<StdMutex<Option<State<u32>>>> = StdArc::new(StdMutex::new(None));

        let k = kept.clone();
        composer.compose(move |ctx: &mut ComposeCtx| {
            let v = observe_watch(ctx, rx.clone(), 0);
            *k.lock().unwrap() = Some(v);
        });
        composer.layout(crate::layout::Constraints::new(0.0, 100.0, 0.0, 100.0));
        rt.block_on(async { for _ in 0..50 { tokio::task::yield_now().await; } });

        let state = kept.lock().unwrap().clone().expect("the effect returned a state");
        assert_eq!(state.peek(), 1, "the subscription seeds the channel's current value");

        // A frame that does not mention the effect: the slot is dropped, so its forget callback
        // runs and has to abort the task.
        composer.compose(|_ctx: &mut ComposeCtx| {});
        composer.layout(crate::layout::Constraints::new(0.0, 100.0, 0.0, 100.0));
        rt.block_on(async { for _ in 0..50 { tokio::task::yield_now().await; } });

        let _ = tx.send(2);
        rt.block_on(async { for _ in 0..50 { tokio::task::yield_now().await; } });
        assert_eq!(
            state.peek(),
            1,
            "a removed composition point must stop listening: the task outlived its slot"
        );
    }
}
