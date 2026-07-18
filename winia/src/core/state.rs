//! 响应式状态系统 — 类似 Jetpack Compose 的 MutableState
//!
//! 核心概念:
//! - State<T>: 可观察的值容器，读时自动追踪依赖，写时通知重组
//! - 基于 thread-local 的依赖追踪，无需显式传递 CompositionContext
//! - 通过 PartialEq 去重，避免无效重组
//! - Subscription 支持精确取消，避免内存泄漏

use parking_lot::RwLock;
use std::cell::RefCell;
use std::fmt::{Debug, Display, Formatter};
use std::sync::Arc;

// ── thread-local: 当前正在进行的组合（读操作时记录依赖）──
thread_local! {
    static CURRENT_COMPOSER: RefCell<Option<*const ()>> = const { RefCell::new(None) };
}

/// 设置当前组合上下文指针（由 Composer 在进入组合时调用）
pub(crate) fn set_current_composer(ptr: *const ()) {
    CURRENT_COMPOSER.with(|c| *c.borrow_mut() = Some(ptr));
}

/// 清除当前组合上下文指针（由 Composer 在退出组合时调用）
pub(crate) fn clear_current_composer() {
    CURRENT_COMPOSER.with(|c| *c.borrow_mut() = None);
}

/// 当在组合上下文中时，执行给定的注册回调
pub(crate) fn with_current_composer(f: impl FnOnce(*const ())) {
    CURRENT_COMPOSER.with(|c| {
        if let Some(ptr) = *c.borrow() {
            f(ptr);
        }
    });
}

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
}

// 全局 State ID 生成器
static NEXT_STATE_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

fn next_state_id() -> u32 {
    NEXT_STATE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

impl<T: 'static> State<T> {
    /// 创建新的状态
    pub fn new(value: T) -> Self {
        Self {
            inner: Arc::new(StateInner {
                id: next_state_id(),
                value: RwLock::new(value),
                subscribers: RwLock::new(Vec::new()),
            }),
        }
    }
}

impl<T: Clone + 'static> State<T> {
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
        notify_state_changed(self.inner.id);
        set_global_dirty();
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
/// State::get() 在 compose 期间调用 → record_dep(state_id, slot_key)
/// State::notify() → notify_state_changed(state_id)
/// Composer::compose() → consume pending → mark slot dirty

use parking_lot::Mutex;

static PENDING_STATES: Mutex<Vec<u32>> = Mutex::new(Vec::new());
static RECORDED_DEPS: Mutex<Vec<(u32, u64)>> = Mutex::new(Vec::new());
static GLOBAL_DIRTY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static DEP_REGISTRAR: Mutex<Option<Box<dyn Fn(u32, u64) + Send>>> = Mutex::new(None);

/// State::notify 调用：记录变化的 state_id
pub(crate) fn notify_state_changed(state_id: u32) {
    PENDING_STATES.lock().push(state_id);
}

/// Composer 消费：获取并清空 pending states
pub fn take_pending_states() -> Vec<u32> {
    std::mem::take(&mut *PENDING_STATES.lock())
}

/// Composer 设置依赖注册器：State::get 时调用
pub fn set_dependency_registrar(f: impl Fn(u32, u64) + Send + 'static) {
    *DEP_REGISTRAR.lock() = Some(Box::new(f));
}

/// Composer 消费：获取本帧记录的依赖
pub fn take_recorded_deps() -> Vec<(u32, u64)> {
    std::mem::take(&mut *RECORDED_DEPS.lock())
}

/// 注册器内部调用：记录 state_id 依赖当前 slot_key
pub fn record_dep(state_id: u32, slot_key: u64) {
    RECORDED_DEPS.lock().push((state_id, slot_key));
}

/// State::get 中调用：触发依赖注册器
fn call_registrar(state_id: u32, slot_key: u64) {
    if let Some(ref reg) = *DEP_REGISTRAR.lock() {
        reg(state_id, slot_key);
    }
}

/// 全局脏标志（State::notify 设置）
pub fn take_global_dirty() -> bool {
    GLOBAL_DIRTY.swap(false, std::sync::atomic::Ordering::AcqRel)
}

pub fn set_global_dirty() {
    GLOBAL_DIRTY.store(true, std::sync::atomic::Ordering::Release);
}

/// State::get 中调用（旧接口兼容），通过 thread-local 获取当前 slot key
pub fn register_dependency(state_id: u32) {
    crate::core::state::with_current_composer(|_composer_ptr| {
        // slot_key 由 Composer 在 compose 期间通过 registrar 提供
        call_registrar(state_id, 0); // slot_key 在 registrar 内部通过 active_slot_key 获取
    });
}
