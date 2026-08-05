//! 响应式状态系统 — 类似 Jetpack Compose 的 MutableState
//!
//! 核心概念:
//! - State<T>: 可观察的值容器，读时自动追踪依赖，写时通知重组
//! - 基于 thread-local 的依赖追踪，无需显式传递 CompositionContext
//! - 通过 PartialEq 去重，避免无效重组
//! - 通知走 STATE_QUEUE_MAP（per-Composer 定向推送）+ notify_version

use parking_lot::RwLock;
use std::fmt::{Debug, Display, Formatter};

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
            notify_version: Default::default(),
            owner_queue: owner_queue.clone(),
        });
        // 注册到全局映射表，供 notify_state_changed 定向推送
        #[cfg(test)] { if inner.id == 1 { eprintln!("[new-probe] id=1 owner={} bt={:?}", owner_queue.is_some(), std::backtrace::Backtrace::force_capture().to_string().lines().take(30).collect::<Vec<_>>()); } }
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

    /// 读取但不注册依赖（动画引擎内部用——避免把依赖记到动画创建处）
    pub fn peek(&self) -> T {
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

    /// 静默更新：改值但不触发 notify/重组。用于"内部标记"类 State——
    /// 值变化不需要响应式（如 Window 的 created_id：窗口创建标记，下次
    /// compose 自然读到新值；notify 会在 compose 中触发异常重组（pending
    /// 消费于物化后 → key 雪崩/树塌缩））
    pub fn set_silent(&self, value: T) {
        let mut current = self.inner.value.write();
        *current = value;
        drop(current);
    }

    /// 设置新值并通知（标记重组），但**不唤醒事件循环**（跳过 WAKE_FN）。
    ///
    /// 动画引擎专用：动画 tick 已由 `request_redraw` 驱动渲染帧，若每个动画
    /// state 的 set 再 wake_up，会触发 wake 自旋（每显示帧多次 compose，
    /// 重组风暴）。通知仍标记 pending → 下帧渲染时重组重测；
    /// 仅省去不必要的立即唤醒。
    pub fn set_no_wake(&self, value: T) {
        let mut current = self.inner.value.write();
        if *current == value {
            return;
        }
        *current = value;
        drop(current);
        self.notify_no_wake();
    }

    /// 设置新值但**不触发重组**。
    ///
    /// 绘制层动画专用：alpha/scale/颜色等视觉属性变化只触发重绘（由动画引擎
    /// 每帧 `request_redraw` 驱动），不触发 `notify → mark_dirty → 重组`。
    /// 与 Compose `graphicsLayer { }` 的"绘制层属性不触发重组"一致。
    pub fn set_visual(&self, value: T) {
        let mut current = self.inner.value.write();
        *current = value;
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
        self.notify_inner(true);
    }

    /// 通知但不唤醒事件循环（动画引擎用）
    fn notify_no_wake(&self) {
        self.notify_inner(false);
    }

    fn notify_inner(&self, wake: bool) {
        self.inner.notify_version.fetch_add(1, std::sync::atomic::Ordering::Release);
        notify_state_changed_inner(self.inner.id, wake);
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

// ── 依赖注册桥接 ──
///
/// 设计：不用全局 Mutex，改用 thread-local 指针直接写入 Composer 实例的 vec。
/// Composer::compose() 开始前 set，结束后 clear；State::get() 通过指针写入。
/// 这消除了 DEP_REGISTRAR + RECORDED_DEPS 两个全局 Mutex。

use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Weak};

// ── 依赖记录（两段式：组合依赖 → slot_deps 重组；布局依赖 → layout_deps 只重测）──
//
// thread_local 存**数据缓冲**（Vec）而非裸指针：composer 通过 begin/take 交接——
// begin_*_deps 清空缓冲并置模式，take_deps O(1) Vec 移动取走（无拷贝、无悬垂风险）。
// 多窗口安全：winit 单线程事件循环，同一时刻只有一个 composer 在 compose/layout。

#[derive(Clone, Copy, PartialEq, Eq)]
enum DepMode {
    None,
    Compose,
    Layout,
}

thread_local! {
    /// 依赖记录缓冲（State::get 写入；begin 清空、take 取走）
    ///
    /// ⚠️ 语义边界（有意为之，勿"修复"）：
    /// - **panic 路径**：content/measure panic 时 DEP_MODE 残留——无 UB（数据缓冲非裸指针），
    ///   下一次 begin_*_deps 清空自愈；测试线程复用时注意。
    /// - **嵌套 compose**：同线程递归 compose 时内层 begin 无条件清空缓冲，外层前半段
    ///   已记录依赖被丢弃。winit 单线程事件循环下不发生（同一时刻单 composer 在组合）。
    static DEP_BUFFER: std::cell::RefCell<Vec<(u32, u64)>> = const { std::cell::RefCell::new(Vec::new()) };
    /// 当前记录模式（None=非组合/布局上下文——get 不记录）
    static DEP_MODE: std::cell::Cell<DepMode> = const { std::cell::Cell::new(DepMode::None) };
}

/// Composer 调用：开始组合期依赖记录（清空缓冲——上一帧残留丢弃）
pub(crate) fn begin_compose_deps() {
    DEP_BUFFER.with(|b| b.borrow_mut().clear());
    DEP_MODE.with(|m| m.set(DepMode::Compose));
}

/// Composer 调用：开始布局期依赖记录（measure 中 State::get 写入——两段式分流）
pub(crate) fn begin_layout_deps() {
    DEP_BUFFER.with(|b| b.borrow_mut().clear());
    DEP_MODE.with(|m| m.set(DepMode::Layout));
}

/// Composer 调用：结束记录并取走缓冲（O(1) Vec 移动）
pub(crate) fn take_deps() -> Vec<(u32, u64)> {
    DEP_MODE.with(|m| m.set(DepMode::None));
    DEP_BUFFER.with(|b| std::mem::take(&mut *b.borrow_mut()))
}

/// State::get 时调用：记录当前依赖（模式 None 时忽略——组合/布局外读取不注册）
pub(crate) fn record_dep(state_id: u32, slot_key: u64) {
    if DEP_MODE.with(|m| m.get()) == DepMode::None {
        return;
    }
    DEP_BUFFER.with(|b| b.borrow_mut().push((state_id, slot_key)));
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
    notify_state_changed_inner(state_id, true);
}

/// 唤醒事件循环一次（动画注册后调用——启动推进轮次；动画活跃后 AboutToWait
/// 的 WaitUntil 接管每帧推进）。动画注册发生在渲染（RedrawRequested）中，
/// 渲染后事件循环 Wait 无限休眠（set_no_wake 不 wake）——无唤醒则动画不推进
/// （1 秒后外部事件才唤醒——大 dt 一次收敛——视觉"直接跳"）
pub(crate) fn wake_loop() {
    if let Some(ref f) = *WAKE_FN.lock().unwrap() { f(); }
}

pub(crate) fn notify_state_changed_inner(state_id: u32, wake: bool) {
    // 定向通知：只推送到创建此 State 的 Composer 队列，避免跨窗口污染
    let pushed = if let Some(q) = STATE_QUEUE_MAP.lock().get(&state_id).and_then(|w| w.upgrade()) {
        q.lock().push(state_id);
        true
    } else {
        false
    };
    #[cfg(test)] { eprintln!("[notify-inner] state={} pushed={} map={:?}", state_id, pushed, STATE_QUEUE_MAP.lock().keys().collect::<Vec<_>>()); }
    if wake {
        if let Some(ref f) = *WAKE_FN.lock().unwrap() { f(); }
    }
}

// ── 实例化依赖记录（替代全局 RECORDED_DEPS + DEP_REGISTRAR）──

/// State::get 中调用：若在 compose 上下文中，记录依赖
pub(crate) fn register_dependency(state_id: u32) {
    // 依赖注册目标：scope 栈非空 → 最内层 scope（组合 scope 内、组件外的读取）；
    // 否则 → 当前 slot key（组件内 build 的读取）
    crate::core::composer::with_active_scope(|key| {
        record_dep(state_id, key);
    });
}

// ═══════════════════════════════════════════════════════════
// DerivedValue<T> — 泛型派生值（State 变换的延迟表达式，对标 Compose derivedStateOf）
// ═══════════════════════════════════════════════════════════

/// 泛型派生值：`&State<f32>` 算术运算或其他 State 变换的延迟表达式。
///
/// 读取时执行闭包（内部 `State::get()` 在组合/测量上下文注册依赖），
/// 动画值变化 → 依赖节点 dirty → 重组重测 → 表达式重算。
/// f32 特化支持算术运算符（`&alpha * 200.0 + 50.0`）；任意类型用 `DerivedValue::new`。
#[derive(Clone)]
pub struct DerivedValue<T>(pub(crate) Arc<dyn Fn() -> T + Send + Sync>);

impl<T> DerivedValue<T> {
    /// 从闭包构建派生值（复杂表达式/自定义逻辑用，对标 Compose `derivedStateOf { }`）。
    /// 闭包内 `State::get()` 在组合/测量上下文注册依赖 → 动画值变化触发节点重组重测。
    pub fn new(f: impl Fn() -> T + Send + Sync + 'static) -> Self {
        DerivedValue(Arc::new(f))
    }

    /// 求值当前表达式（组合/测量上下文内调用 → 内部 State::get() 注册依赖）
    pub fn get(&self) -> T {
        (self.0)()
    }
}

/// f32 派生值别名（算术运算符的返回类型）
pub type DerivedFloat = DerivedValue<f32>;

macro_rules! impl_derived_arith {
    ($trait:ident, $method:ident, $op:tt) => {
        impl std::ops::$trait<f32> for DerivedFloat {
            type Output = DerivedFloat;
            fn $method(self, rhs: f32) -> DerivedFloat {
                let f = self.0.clone();
                DerivedValue(Arc::new(move || f() $op rhs))
            }
        }
        impl std::ops::$trait<f32> for &DerivedFloat {
            type Output = DerivedFloat;
            fn $method(self, rhs: f32) -> DerivedFloat {
                let f = self.0.clone();
                DerivedValue(Arc::new(move || f() $op rhs))
            }
        }
        impl std::ops::$trait<f32> for &State<f32> {
            type Output = DerivedFloat;
            fn $method(self, rhs: f32) -> DerivedFloat {
                let s = self.clone();
                DerivedValue(Arc::new(move || s.get() $op rhs))
            }
        }
    };
}

impl_derived_arith!(Add, add, +);
impl_derived_arith!(Sub, sub, -);
impl_derived_arith!(Mul, mul, *);
impl_derived_arith!(Div, div, /);

/// 常数在左：`2.0 * alpha`
impl std::ops::Mul<&State<f32>> for f32 {
    type Output = DerivedFloat;
    fn mul(self, rhs: &State<f32>) -> DerivedFloat {
        let s = rhs.clone();
        DerivedValue(Arc::new(move || self * s.get()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_silent_no_notify() {
        // set_silent 改值但不入 pending 队列（内部标记类 State——窗口创建标记）
        let s = State::new(0i32);
        // 模拟 remember 绑定：注册 owner queue（否则 notify 也不入队——无法区分）
        let q = std::sync::Arc::new(parking_lot::Mutex::new(Vec::new()));
        {
            STATE_OWNER_QUEUE.with(|o| *o.borrow_mut() = Some(std::sync::Arc::downgrade(&q)));
            let s2 = State::new(0i32);
            s2.set_silent(42);
            assert_eq!(s2.get(), 42);
            assert!(q.lock().is_empty(), "set_silent 不应入队");
            s2.set(43);
            assert_eq!(q.lock().len(), 1, "set 应入队");
            drop(s2);
            STATE_OWNER_QUEUE.with(|o| *o.borrow_mut() = None);
        }
        let _ = s.get();
    }
}
