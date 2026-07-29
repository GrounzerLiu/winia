//! 响应式状态系统 — 类似 Jetpack Compose 的 MutableState
//!
//! 核心概念:
//! - State<T>: 可观察的值容器，读时自动追踪依赖，写时通知重组
//! - 基于 thread-local 的依赖追踪，无需显式传递 CompositionContext
//! - 通过 PartialEq 去重，避免无效重组
//! - Subscription 支持精确取消，避免内存泄漏

use parking_lot::RwLock;
use std::fmt::{Debug, Display, Formatter};

// ── SubscriberId ──

/// 订阅者标识符，用于精确取消订阅。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SubscriberId(u64);

static NEXT_SUBSCRIBER_ID: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(1);

fn next_subscriber_id() -> SubscriberId {
    SubscriberId(NEXT_SUBSCRIBER_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
}

/// 订阅者条目
struct Subscriber {
    id: SubscriberId,
    callback: Box<dyn Fn() + Send + Sync>,
}

// ── State<T> ──

/// 响应式状态容器。
///
/// Clone 是廉价操作（Arc clone），多个持有者共享同一状态。
pub struct State<T> {
    inner: Arc<StateInner<T>>,
}

struct StateInner<T> {
    id: u32,
    value: RwLock<T>,
    /// 订阅者列表。使用 SubscriberId 实现精确删除。
    subscribers: RwLock<Vec<Subscriber>>,
    /// 通知版本号——每次 set/update 自增，compose 消费后归零
    notify_version: std::sync::atomic::AtomicU32,
    /// 创建此 State 的 Composer 队列（用于定向通知，避免跨窗口污染）
    owner_queue: Option<Weak<parking_lot::Mutex<Vec<u32>>>>,
}

// 全局 State ID 生成器
static NEXT_STATE_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

fn next_state_id() -> u32 {
    NEXT_STATE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

thread_local! {
    /// 当前正在创建 State 的 Composer 队列引用
    pub(crate) static STATE_OWNER_QUEUE: std::cell::RefCell<Option<Weak<parking_lot::Mutex<Vec<u32>>>>> = const { std::cell::RefCell::new(None) };
}

impl<T: 'static> State<T> {
    /// 创建新的状态
    pub fn new(value: T) -> Self {
        let owner_queue = STATE_OWNER_QUEUE.with(|q| q.borrow().clone());
        let inner = Arc::new(StateInner {
            id: next_state_id(),
            value: RwLock::new(value),
            subscribers: RwLock::new(Vec::new()),
            notify_version: Default::default(),
            owner_queue: owner_queue.clone(),
        });
        // 注册到全局映射表，供 notify_state_changed 定向推送
        if let Some(ref w) = owner_queue {
            STATE_QUEUE_MAP.lock().insert(inner.id, w.clone());
        }
        Self { inner }
    }
}

impl<T: Clone + 'static> State<T> {
    /// 读取并重置通知版本号（供 compose 消费确认）
    pub fn take_notify_version(&self) -> u32 {
        self.inner.notify_version.swap(0, std::sync::atomic::Ordering::AcqRel)
    }

    /// 读取当前值的快照。
    ///
    /// 如果在组合上下文中调用（即 Composer 正在执行 composable 函数），
    /// 会自动注册依赖关系：当此 State 变化时，对应的 composable 会被标记为需要重组。
    pub fn get(&self) -> T {
        let state_id = self.inner.id;
        register_dependency(state_id);

        self.inner.value.read().clone()
    }
}

impl<T: PartialEq + 'static> State<T> {
    /// 设置新值。若新值与当前值相等（通过 PartialEq），则跳过通知。
    pub fn set(&self, value: T) {
        let mut current = self.inner.value.write();
        if *current == value {
            return; // 值未变化，避免无效重组
        }
        *current = value;
        drop(current);
        self.notify();
    }
}

impl<T: 'static> State<T> {
    /// 原地更新值，始终触发通知。
    pub fn update(&self, f: impl FnOnce(&mut T)) {
        let mut current = self.inner.value.write();
        f(&mut *current);
        drop(current);
        self.notify();
    }

    /// 返回此 State 的唯一 ID
    pub fn id(&self) -> u32 {
        self.inner.id
    }

    /// 通知所有订阅者（通常触发重组）
    fn notify(&self) {
        let subscribers = self.inner.subscribers.read();
        for sub in subscribers.iter() {
            (sub.callback)();
        }
        self.inner.notify_version.fetch_add(1, std::sync::atomic::Ordering::Release);
        notify_state_changed(self.inner.id);
    }

    /// 订阅状态变化。返回 Subscription，drop 时精确取消。
    ///
    /// 每次 set()/update() 时会调用 callback。
    pub fn subscribe(&self, callback: impl Fn() + Send + Sync + 'static) -> Subscription {
        let id = next_subscriber_id();
        self.inner.subscribers.write().push(Subscriber {
            id,
            callback: Box::new(callback),
        });

        let weak = Arc::downgrade(&self.inner);
        Subscription::new(id, move || {
            if let Some(inner) = weak.upgrade() {
                inner.subscribers.write().retain(|s| s.id != id);
            }
        })
    }
}

impl<T> Clone for State<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T: Debug> Debug for State<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("State")
            .field("id", &self.inner.id)
            .field("value", &*self.inner.value.read())
            .finish()
    }
}

impl<T: Display> Display for State<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&*self.inner.value.read(), f)
    }
}

impl<T: PartialEq> PartialEq for State<T> {
    fn eq(&self, other: &Self) -> bool {
        self.inner.id == other.inner.id
            || *self.inner.value.read() == *other.inner.value.read()
    }
}

impl<T> Eq for State<T> where T: Eq {}

// ── Subscription ──

/// 订阅句柄。Drop 时自动精确取消订阅（从 subscribers 列表中移除对应条目）。
pub struct Subscription {
    id: SubscriberId,
    /// 取消函数：在 drop 时执行。None 表示已取消。
    cancel_fn: Option<Box<dyn FnOnce()>>,
}

impl Subscription {
    fn new(id: SubscriberId, cancel_fn: impl FnOnce() + 'static) -> Self {
        Self {
            id,
            cancel_fn: Some(Box::new(cancel_fn)),
        }
    }

    /// 返回此订阅的 ID（调试用）
    pub fn id(&self) -> SubscriberId {
        self.id
    }

    /// 手动取消订阅（提前取消，不走 Drop）
    pub fn cancel(mut self) {
        if let Some(cancel) = self.cancel_fn.take() {
            cancel();
        }
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(cancel) = self.cancel_fn.take() {
            cancel();
        }
    }
}

// ── 依赖注册桥接 ──
///
/// 设计：不用全局 Mutex，改用 thread-local 指针直接写入 Composer 实例的 vec。
/// Composer::compose() 开始前 set，结束后 clear；State::get() 通过指针写入。
/// 这消除了 DEP_REGISTRAR + RECORDED_DEPS 两个全局 Mutex。

use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Weak};

thread_local! {
    /// compose 期间指向当前 Composer 的 recorded_deps vec
    static RECORDING_TARGET: std::cell::RefCell<Option<*mut Vec<(u32, u64)>>> = const { std::cell::RefCell::new(None) };
}

// ── Composer 注册表：每个 Composer 注册自己的通知队列 ──
// State 变化时通知所有活动 Composer，替代全局 PENDING_STATES + GLOBAL_DIRTY

/// 全局唤醒回调（由 app::run_app 注入 EventLoopProxy，协程中 State 变更时唤醒事件循环）
static WAKE_FN: std::sync::Mutex<Option<Box<dyn Fn() + Send + Sync + 'static>>> =
    std::sync::Mutex::new(None);

pub(crate) fn set_wake_fn(f: impl Fn() + Send + Sync + 'static) {
    *WAKE_FN.lock().unwrap() = Some(Box::new(f));
}

static COMPOSER_REGISTRY: LazyLock<Mutex<Vec<Weak<Mutex<Vec<u32>>>>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

/// State ID → 创建者 Composer 队列映射（用于定向通知）
static STATE_QUEUE_MAP: LazyLock<Mutex<HashMap<u32, Weak<parking_lot::Mutex<Vec<u32>>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Composer 启动时注册自己的队列（传入 Weak 引用，Composer drop 后自动清理）
pub(crate) fn register_composer_queue(queue: Weak<Mutex<Vec<u32>>>) {
    COMPOSER_REGISTRY.lock().push(queue);
}

/// State 值变化时调用：定向通知创建此 State 的 Composer
pub(crate) fn notify_state_changed(state_id: u32) {
    // 定向通知：只推送到创建此 State 的 Composer 队列，避免跨窗口污染
    if let Some(q) = STATE_QUEUE_MAP.lock().get(&state_id).and_then(|w| w.upgrade()) {
        q.lock().push(state_id);
    }
    if let Some(ref f) = *WAKE_FN.lock().unwrap() { f(); }
}

// ── 实例化依赖记录（替代全局 RECORDED_DEPS + DEP_REGISTRAR）──

/// Composer 调用：设置当前 compose 的依赖记录目标
pub(crate) fn set_recording_target(target: *mut Vec<(u32, u64)>) {
    RECORDING_TARGET.with(|c| *c.borrow_mut() = Some(target));
}

/// Composer 调用：清除记录目标（compose 结束后）
pub(crate) fn clear_recording_target() {
    RECORDING_TARGET.with(|c| *c.borrow_mut() = None);
}

/// State::get 时调用：向当前 Composer 的 recorded_deps 写入依赖
pub fn record_dep(state_id: u32, slot_key: u64) {
    RECORDING_TARGET.with(|c| {
        if let Some(ptr) = c.borrow().as_ref() {
            // SAFETY: ptr 在 compose() 期间有效，compose 持有 &mut self
            unsafe { &mut **ptr }.push((state_id, slot_key));
        }
    });
}

/// State::get 中调用：若在 compose 上下文中，记录依赖
pub fn register_dependency(state_id: u32) {
    // 通过 thread-local ACTIVE_SLOT_KEY 获取当前 slot key
    crate::core::composer::with_active_slot_key(|key| {
        record_dep(state_id, key);
    });
}
