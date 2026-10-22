use super::*;

// ── 依赖注册桥接 ──
///
/// State::get() 没有显式 context，因此当前 recorder 仍通过 TLS 暴露。
/// DependencyFrameGuard 在 compose/layout 入口保存外层 buffer、模式和 queue，
/// 正常 commit 或 panic Drop 都恢复外层 frame，避免嵌套调用覆盖调用方状态。

// ── 依赖记录（两段式：组合依赖 -> slot_deps 重组；布局依赖 -> layout_deps 只重测）──
//
// The active buffers remain thread-local because State::get has no explicit context.
// DependencyFrameStack snapshots them before every compose/layout entry, so nested
// composers get an isolated recorder and a panic discards only the inner frame.

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum DepMode {
    None,
    Compose,
    Layout,
}

pub(crate) struct DependencyFrame {
    id: u64,
    buffer: Vec<(Arc<StateSignal>, u64)>,
    mode: DepMode,
    recorder_queue: Option<Weak<ComposerSubscription>>,
}

/// Owns one active dependency frame. A dropped, uncommitted frame restores the
/// outer recorder and removes subscriptions learned only by the failed frame.
pub(crate) struct DependencyFrameGuard {
    id: u64,
    subscription: Option<Arc<ComposerSubscription>>,
    baseline_signals: std::collections::HashSet<StateId>,
    active: bool,
    committed: bool,
}

static NEXT_DEPENDENCY_FRAME_ID: AtomicU64 = AtomicU64::new(1);

thread_local! {
    /// Current dependency recorder. The matching stack stores the outer frame.
    static DEP_BUFFER: std::cell::RefCell<Vec<(Arc<StateSignal>, u64)>> = const { std::cell::RefCell::new(Vec::new()) };
    static DEP_MODE: std::cell::Cell<DepMode> = const { std::cell::Cell::new(DepMode::None) };
    static RECORDER_QUEUE: std::cell::RefCell<Option<Weak<ComposerSubscription>>> = const { std::cell::RefCell::new(None) };
    static DEPENDENCY_FRAME_STACK: std::cell::RefCell<Vec<DependencyFrame>> = const { std::cell::RefCell::new(Vec::new()) };
}

pub(crate) fn push_dependency_frame(mode: DepMode, recorder_queue: Option<Weak<ComposerSubscription>>) -> DependencyFrameGuard {
    let subscription = recorder_queue.as_ref().and_then(Weak::upgrade);
    let baseline_signals = subscription
        .as_ref()
        .map(|queue| queue.signal_ids())
        .unwrap_or_default();
    let frame_id = NEXT_DEPENDENCY_FRAME_ID.fetch_add(1, Ordering::Relaxed);
    let previous = DependencyFrame {
        id: frame_id,
        buffer: DEP_BUFFER.with(|b| std::mem::take(&mut *b.borrow_mut())),
        mode: DEP_MODE.with(|m| {
            let previous = m.get();
            m.set(mode);
            previous
        }),
        recorder_queue: RECORDER_QUEUE.with(|q| {
            std::mem::replace(&mut *q.borrow_mut(), recorder_queue)
        }),
    };
    DEPENDENCY_FRAME_STACK.with(|frames| frames.borrow_mut().push(previous));
    DependencyFrameGuard {
        id: frame_id,
        subscription,
        baseline_signals,
        active: true,
        committed: false,
    }
}

pub(crate) fn restore_dependency_frame(frame_id: u64) -> bool {
    let previous = DEPENDENCY_FRAME_STACK.with(|frames| {
        let mut frames = frames.borrow_mut();
        if frames.last().map(|frame| frame.id) != Some(frame_id) {
            return None;
        }
        frames.pop()
    });
    let Some(previous) = previous else {
        // Drop must never panic while unwinding. A mismatched guard is a caller
        // bug; leave the active top frame intact instead of corrupting it.
        return false;
    };

    DEP_BUFFER.with(|b| *b.borrow_mut() = previous.buffer);
    DEP_MODE.with(|m| m.set(previous.mode));
    RECORDER_QUEUE.with(|q| *q.borrow_mut() = previous.recorder_queue);
    true
}

impl DependencyFrameGuard {
    /// Commit the current frame and restore the outer recorder. The guard must
    /// remain alive until all State reads for this phase have been consumed.
    pub(crate) fn commit(&mut self) {
        if !self.active {
            return;
        }
        if restore_dependency_frame(self.id) {
            self.active = false;
            self.committed = true;
        }
    }
}

impl Drop for DependencyFrameGuard {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        if restore_dependency_frame(self.id) && !self.committed {
            if let Some(subscription) = &self.subscription {
                subscription.retain_signals(&self.baseline_signals);
            }
        }
        self.active = false;
    }
}


/// Composer composition entry: subscribe reads to this Composer's queue.
pub(crate) fn begin_compose_deps_with_queue(queue: Weak<ComposerSubscription>) -> DependencyFrameGuard {
    push_dependency_frame(DepMode::Compose, Some(queue))
}


/// Begin layout dependency recording for a live Composer subscription.
pub(crate) fn begin_layout_deps_with_queue(queue: Weak<ComposerSubscription>) -> DependencyFrameGuard {
    push_dependency_frame(DepMode::Layout, Some(queue))
}

/// Begin a read-tracking frame that subscribes nothing: reads still land in the buffer for
/// [`take_deps`], but no recorder queue is attached, so `State::get` only records.
///
/// This is what lets a stream recompute a closure and learn which states it read —
/// `snapshot_flow` (`docs/flow-design.md` layer 2) runs the closure inside one of these.
pub(crate) fn begin_tracking_frame() -> DependencyFrameGuard {
    push_dependency_frame(DepMode::Compose, None)
}

/// Run a block with dependency recording OFF, restoring whatever frame was active afterwards.
///
/// This is for user code the framework runs at a point where some *other* frame happens to be
/// active — a `side_effect` drained at the end of a compose that was itself started from a layout
/// pass has the outer Layout frame live again, and a read there would be recorded as that
/// composer's layout dependency under a slot key that does not belong to it.
pub(crate) fn begin_no_deps_frame() -> DependencyFrameGuard {
    push_dependency_frame(DepMode::None, None)
}

/// Finish recording and move the current buffer out without touching the outer frame.
pub(crate) fn take_deps() -> Vec<(Arc<StateSignal>, u64)> {
    DEP_MODE.with(|m| m.set(DepMode::None));
    DEP_BUFFER.with(|b| std::mem::take(&mut *b.borrow_mut()))
}

/// Checkpoint reads within one active frame. A caught user-code panic must not turn failed reads
/// into dependencies of a successful retry at the same stored slot identity.
pub(crate) struct DependencyReadCheckpoint {
    length: usize,
    subscription: Option<Arc<ComposerSubscription>>,
    baseline_signals: std::collections::HashSet<StateId>,
    committed: bool,
}

pub(crate) fn checkpoint_dependency_reads() -> DependencyReadCheckpoint {
    let subscription = RECORDER_QUEUE.with(|q| q.borrow().as_ref().and_then(Weak::upgrade));
    let baseline_signals = subscription.as_ref().map(|queue| queue.signal_ids()).unwrap_or_default();
    DependencyReadCheckpoint {
        length: DEP_BUFFER.with(|b| b.borrow().len()),
        subscription,
        baseline_signals,
        committed: false,
    }
}

impl DependencyReadCheckpoint {
    pub(crate) fn commit(&mut self) { self.committed = true; }
}

impl Drop for DependencyReadCheckpoint {
    fn drop(&mut self) {
        if !self.committed {
            DEP_BUFFER.with(|b| b.borrow_mut().truncate(self.length));
            if let Some(queue) = &self.subscription { queue.retain_signals(&self.baseline_signals); }
        }
    }
}

/// State::get records only while a compose/layout dependency frame is active.
pub(crate) fn record_dep(
    signal: Arc<StateSignal>,
    slot_key: u64,
) -> Option<(Arc<ComposerSubscription>, u64)> {
    if DEP_MODE.with(|m| m.get()) == DepMode::None {
        return None;
    }
    DEP_BUFFER.with(|b| b.borrow_mut().push((signal.clone(), slot_key)));

    // Do not hold a TLS RefCell borrow while tracking or subscribing. Those
    // operations may invoke code that reads State again.
    let recorder = RECORDER_QUEUE.with(|q| q.borrow().clone().and_then(|weak| weak.upgrade()));
    let Some(recorder) = recorder else { return None };
    let observed_revision = recorder.subscribe_signal(&signal);
    observed_revision.map(|revision| (recorder, revision))
}

// ── Event-loop wake bridge ──

/// 全局唤醒回调（由 app::run_app 注入 EventLoopProxy，协程中 State 变更时唤醒事件循环）
static WAKE_FN: std::sync::Mutex<Option<Box<dyn Fn() + Send + Sync + 'static>>> =
    std::sync::Mutex::new(None);

pub(crate) fn set_wake_fn(f: impl Fn() + Send + Sync + 'static) {
    *WAKE_FN.lock().unwrap() = Some(Box::new(f));
}

/// 唤醒事件循环一次（动画注册后调用）。
pub(crate) fn wake_loop() {
    if let Some(ref f) = *WAKE_FN.lock().unwrap() { f(); }
}

// ── 实例化依赖记录（替代全局 RECORDED_DEPS + DEP_REGISTRAR）──

/// State::get 中调用：若在 compose 上下文中，记录依赖
pub(crate) fn register_dependency(
    signal: Arc<StateSignal>,
) -> Option<(Arc<ComposerSubscription>, u64)> {
    // 依赖注册目标：scope 栈非空 → 最内层 scope（组合 scope 内、组件外的读取）；
    // 否则 → 当前 slot key（组件内 build 的读取）
    let mut registration = None;
    crate::runtime::composer::with_active_scope(|key| {
        registration = record_dep(signal, key);
    });
    registration
}

