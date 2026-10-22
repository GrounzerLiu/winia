//! 组合引擎 — ComposeCtx 和 Composer
//!
//! ComposeCtx 是 composable 函数的入口点，提供:
//! - remember(): 在组合中持久化状态（重组时返回同一个 State 实例）
//! - next_key(): 生成组合 key，用于 start_node/end_node
//!
//! Composer 管理组合树的生命周期:
//! - SlotTable: 存储组合节点和 remembered 状态
//! - 重组调度: 批处理状态变化，在下一帧重组
//! - Key 管理: 全局唯一 key 计数器

use crate::runtime::state::{ComposerSubscription, State, StateId, StateSignal};
use crate::transition::{ActiveFlight, FlightId, PendingSource, SharedBounds};
use crate::layout::constraints::Constraints;
use crate::layout::node::{LayoutNode, MeasurePolicy};
use crate::modifier::Modifier;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::Hasher;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::any::Any;
use std::cell::Cell;
use std::cell::RefCell;

/// RAII guard：语句作用域结束（含 return/break/continue/panic 提前退出）自动 pop_stmt。
/// 零大小——Drop 直接操作 thread_local 栈（不持有 &mut ctx，无借用冲突）。
pub struct StmtGuard;

impl Drop for StmtGuard {
    fn drop(&mut self) {
        STMT_STACK.with(|s| { s.borrow_mut().pop(); });
    }
}

/// RAII 组合 scope guard——Drop 时调用 end_scope（配对 start_scope_guarded）。
/// 持有 composer 裸指针：guard 生命周期内 composer 必须存活且无并发访问
/// （组合单线程）；guard 由 `#[composable]` 宏注入声明在函数开头、函数返回
/// 时最后 drop——end_scope 在所有语句 guard pop 之后执行，配对正确。
pub struct ScopeGuard {
    composer: *mut Composer,
}
impl Drop for ScopeGuard {
    fn drop(&mut self) {
        unsafe { (*self.composer).end_scope(); }
    }
}

thread_local! { static ACTIVE_SLOT_KEY: Cell<u64> = const { Cell::new(0) }; }
thread_local! { static GROUP_STACK: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) }; }
thread_local! { static STMT_STACK: RefCell<Vec<(u32, u32)>> = const { RefCell::new(Vec::new()) }; }

/// A saved outer runtime context. The current TLS values are replaced with
/// empty values while a nested Composer compose/layout call is active.
struct RuntimeFrame {
    id: u64,
    active_slot_key: u64,
    group_stack: Vec<u64>,
    stmt_stack: Vec<(u32, u32)>,
    measured_layout_keys: Option<HashSet<u64>>,
}

/// Restores the surrounding runtime context on normal return and panic.
pub(crate) struct RuntimeFrameGuard {
    id: u64,
    active: bool,
}

static NEXT_RUNTIME_FRAME_ID: AtomicU64 = AtomicU64::new(1);
/// Stable Composer identities (Phase 4 cross-composer flight matching).
static NEXT_COMPOSER_ID: AtomicU64 = AtomicU64::new(1);

thread_local! {
    /// Saved outer runtime contexts, kept in strict LIFO order.
    static RUNTIME_FRAME_STACK: RefCell<Vec<RuntimeFrame>> = const { RefCell::new(Vec::new()) };
    /// Set only while layout measurement is traversing nodes.
    static MEASURED_LAYOUT_KEYS: RefCell<Option<HashSet<u64>>> = const { RefCell::new(None) };
    /// Set by a `MeasurePolicy` that needs the frame to compose again after this layout.
    static COMPOSE_AFTER_LAYOUT: Cell<bool> = const { Cell::new(false) };
}

/// A measure policy asks the frame to compose again after this layout pass.
///
/// A container can discover DURING measurement that the composition it was handed cannot satisfy the
/// space it is being measured in: `LazyColumn` composes its window in `build` from the viewport the
/// previous frame measured, so the frame the viewport grows on composes too few items and shows an empty
/// bottom for one frame (`docs/lazy-column.md`). Only a compose can add them, and the only place that
/// can ask for one is the policy that noticed.
///
/// **Precision is the point.** A frame handler could instead converge whenever composition work is
/// pending after `layout`, which is simpler and costs double: a measure writes back derived values too
/// (`LazyColumn`'s anchor), and those are pending nearly every scrolling frame. Measured on a 800-row
/// list scrolling one row per frame, that loop took 2 passes on 4 of 5 frames; this request is raised
/// only by the "I am short of content" case.
pub fn request_compose_after_layout() {
    COMPOSE_AFTER_LAYOUT.with(|c| c.set(true));
}

/// Whether a measure asked for a compose after this layout, clearing the request. The frame handler
/// loops on this — see [`request_compose_after_layout`] for why it is not a pending-state query.
pub fn take_compose_after_layout() -> bool {
    COMPOSE_AFTER_LAYOUT.with(|c| c.replace(false))
}

fn begin_runtime_frame() -> RuntimeFrameGuard {
    let id = NEXT_RUNTIME_FRAME_ID.fetch_add(1, Ordering::Relaxed);
    let previous = RuntimeFrame {
        id,
        active_slot_key: ACTIVE_SLOT_KEY.with(|slot| {
            let previous = slot.get();
            slot.set(0);
            previous
        }),
        group_stack: GROUP_STACK.with(|groups| std::mem::take(&mut *groups.borrow_mut())),
        stmt_stack: STMT_STACK.with(|stmts| std::mem::take(&mut *stmts.borrow_mut())),
        measured_layout_keys: MEASURED_LAYOUT_KEYS.with(|keys| {
            std::mem::replace(&mut *keys.borrow_mut(), None)
        }),
    };
    RUNTIME_FRAME_STACK.with(|frames| frames.borrow_mut().push(previous));
    RuntimeFrameGuard { id, active: true }
}

fn restore_runtime_frame(id: u64) -> bool {
    let previous = RUNTIME_FRAME_STACK.with(|frames| {
        let mut frames = frames.borrow_mut();
        if frames.last().map(|frame| frame.id) != Some(id) {
            return None;
        }
        frames.pop()
    });
    let Some(previous) = previous else {
        // Drop must not panic while unwinding. A non-LIFO guard is a caller
        // error; leave the active frame untouched rather than corrupting it.
        return false;
    };

    ACTIVE_SLOT_KEY.with(|slot| slot.set(previous.active_slot_key));
    GROUP_STACK.with(|groups| *groups.borrow_mut() = previous.group_stack);
    STMT_STACK.with(|stmts| *stmts.borrow_mut() = previous.stmt_stack);
    MEASURED_LAYOUT_KEYS.with(|keys| *keys.borrow_mut() = previous.measured_layout_keys);
    true
}

impl Drop for RuntimeFrameGuard {
    fn drop(&mut self) {
        if self.active {
            let _ = restore_runtime_frame(self.id);
            self.active = false;
        }
    }
}


/// 参数值（阶段5 参数相等跳过用）——`ComposeCtx::changed` 暂存的参数，
/// 支持跨帧按类型比较（`Box<dyn Any>` 无法通用 PartialEq，用 trait object 桥接）。
pub(crate) trait ParamValue: Any {
    fn eq_any(&self, other: &dyn Any) -> bool;

    /// Overwrite this value with `value` when it has this slot's type; report whether it did.
    ///
    /// This is what lets `changed` REUSE a box instead of allocating one per call: the buffer it writes
    /// into is the previous frame's parameter vector, handed back by the slot (see
    /// `Composer::commit_pending_params`), so a container that declares the same parameter types every
    /// frame — every container in a list — overwrites storage that is already the right shape. A value
    /// whose type does not match reports `false` and the caller allocates instead.
    fn set_from_any(&mut self, value: &dyn Any) -> bool;
}

impl<T: PartialEq + Clone + 'static> ParamValue for T {
    fn eq_any(&self, other: &dyn Any) -> bool {
        other.downcast_ref::<T>() == Some(self)
    }

    fn set_from_any(&mut self, value: &dyn Any) -> bool {
        match value.downcast_ref::<T>() {
            Some(value) => {
                *self = value.clone();
                true
            }
            None => false,
        }
    }
}

/// Test-only count of parameter BOXES allocated, so a test can assert the recycling actually recycles.
/// A behavioural test cannot see it: reusing a box and allocating one produce identical state.
#[cfg(test)]
pub(crate) mod param_alloc {
    use std::cell::Cell;

    thread_local! {
        static ALLOCS: Cell<u64> = const { Cell::new(0) };
    }

    pub(crate) fn count() -> u64 {
        ALLOCS.with(|c| c.get())
    }

    pub(crate) fn reset() {
        ALLOCS.with(|c| c.set(0));
    }

    #[inline]
    pub(super) fn note() {
        ALLOCS.with(|c| c.set(c.get() + 1));
    }
}

/// 两个参数序列是否逐项相等（数量相同 + 类型/值全等）
fn params_equal(a: &[Box<dyn ParamValue>], b: &[Box<dyn ParamValue>]) -> bool {
    if a.len() != b.len() { return false; }
    a.iter().zip(b.iter()).all(|(x, y)| x.eq_any(&**y))
}

/// 读取当前组合作用域的依赖注册目标：最内层 scope（容器组件/函数 scope）；
/// scope 栈空（组合外/测量）→ ACTIVE_SLOT_KEY。
pub(crate) fn with_active_scope(f: impl FnOnce(u64)) {
    // 统一依赖注册目标 = 最内层 scope（容器组件 start_restartable_group 时 push、
    // `#[composable]` 函数 start_scope 时 push）：组件内读取（Text build）注册到最近
    // 容器 scope（对标 Compose ReplaceGroup 内联语义）；content 闭包内表达式注册到
    // 所在容器 scope。NODE_DEPTH 不再参与（此前导致 content scope 收不到依赖——
    // content 闭包内 NODE_DEPTH 恒 ≥1，永远走 ACTIVE_SLOT_KEY）。
    // scope 栈空（组合外/测量阶段）→ 回退 ACTIVE_SLOT_KEY（测量时节点）
    GROUP_STACK.with(|s| {
        let s = s.borrow();
        if let Some(&k) = s.last() {
            f(k);
        } else {
            with_active_slot_key(f);
        }
    });
}

/// 读取当前 compose 位置的 slot key（供 state.rs 依赖追踪使用）
pub(crate) fn with_active_slot_key(f: impl FnOnce(u64)) {
    ACTIVE_SLOT_KEY.with(|c| f(c.get()));
}

/// key = fnv(base, 序号)——全 64 位混合身份与实例序号，不丢熵。
/// 拼接方案（`(base << 32) | c` 或 `base 高 32 位 | c`）会把 base 截到 32 位
/// → 身份空间 2^32（碰撞概率高——checkbox 循环子项即碰撞）。
pub(crate) fn mix_key(base: u64, c: u64) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    h ^= base; h = h.wrapping_mul(0x100000001b3);
    h ^= c; h = h.wrapping_mul(0x100000001b3);
    h
}

/// 设置当前 slot key（measure_node 用它把动态尺寸的依赖注册到节点）
pub(crate) fn set_active_slot_key(key: u64) {
    ACTIVE_SLOT_KEY.with(|c| c.set(key));
    MEASURED_LAYOUT_KEYS.with(|keys| {
        if let Some(keys) = keys.borrow_mut().as_mut() {
            keys.insert(key);
        }
    });
}

/// The slot key measurement is currently attributing reads to.
///
/// Read by `measure_node_inner` before it arms its own key, so the key it displaced can be put back
/// when it returns — see `ActiveSlotKeyGuard` there for why a parent's post-child read needs that.
pub(crate) fn active_slot_key() -> u64 {
    ACTIVE_SLOT_KEY.with(|c| c.get())
}

fn begin_layout_measure_tracking() {
    MEASURED_LAYOUT_KEYS.with(|keys| *keys.borrow_mut() = Some(HashSet::new()));
}

fn take_layout_measure_keys() -> HashSet<u64> {
    MEASURED_LAYOUT_KEYS
        .with(|keys| keys.borrow_mut().take().unwrap_or_default())
}

// ── ComposeCtx ──

/// 组合上下文。
///
/// 传递给每个 composable 函数，用于:
/// - `remember()`: 在组合中持久化状态
/// - `next_key()`: 生成唯一的组合 key
///
/// 注意: ComposeCtx 不实现 Clone —— 组合树是严格树形遍历。
pub struct ComposeCtx<'a> {
    composer: &'a mut Composer,
}

impl<'a> ComposeCtx<'a> {
    pub(crate) fn new(composer: &'a mut Composer) -> Self {
        Self {
            composer,
        }
    }

    /// The frame clock of this composition's window.
    ///
    /// Prefer this over the process-wide `with_frame_nanos`: with more than one window the global
    /// clock wakes a waiter on ANY window's frame, while this one wakes it only on its own.
    pub fn frame_clock(&self) -> crate::effect::FrameClock {
        crate::effect::FrameClock::for_window(self.composer.window_id)
    }

    /// Return the Window lifecycle context owned by this Composer.
    pub(crate) fn window_lifecycle(&self) -> crate::runtime::lifecycle::LifecycleState {
        self.composer.lifecycle.clone()
    }


    /// 在组合中记住一个状态。初次调用时执行 init 创建 State，后续重组时返回上次的同一个 State 实例。
    pub fn remember<T: Clone + 'static>(&mut self, init: impl FnOnce() -> T) -> State<T> {
        let slot_key = self.next_remember_key();
        self.composer.slot_table.remember(slot_key, || State::new(init()))
    }

    /// Run `f` when the slot being composed RIGHT NOW is dropped — a lifecycle hook that costs no
    /// layout node.
    ///
    /// This is what effects use for their cleanup. The alternative the framework had first was an
    /// implicit leaf node carrying an `on_remove`: it worked, but the node was a real child of the
    /// enclosing container, so an effect inside a `Column(spacing = 10)` pushed its next sibling
    /// down by one spacing slot (measured: y 29 -> 39). Compose has the same shape of hook — a
    /// `RememberObserver` lives in the slot table and never materializes.
    ///
    /// Registering again at the same call site REPLACES the previous callback rather than stacking,
    /// which is what a recomposition needs.
    pub fn remember_on_forget(&mut self, f: impl FnOnce() + Send + 'static) {
        let slot_key = self.next_remember_key();
        self.composer.slot_table.remember_on_forget(slot_key, Box::new(f));
    }

    /// Tie a motion to the CURRENT slot: when the slot is forgotten — the branch changed shape, the
    /// component left the tree — the motion is removed from the engine.
    ///
    /// Compose expresses this with `DisposableEffect`. Without it a slot-owned animation keeps
    /// ticking, and keeps writing to a state nobody reads, until the whole `Composer` is dropped:
    /// a conditionally composed `animateFloatAsState` is the common case, and it used to outlive
    /// its composable by however long the window stayed open.
    ///
    /// The registration is unconditional. The key is `mix_key(statement, n)`, where `n` counts the
    /// `remember` calls made from that statement: registering only on the first pass would renumber
    /// every LATER `own_motion` of the same scope, leaving stale callbacks in the slot's forget map at
    /// keys nothing ever revisits. Re-registering is cheap and REPLACES the callback at the same key
    /// (see `remember_on_forget`).
    pub(crate) fn own_motion(&mut self, state_id: crate::runtime::state::StateId) {
        self.composer.animation_state_ids.insert(state_id);
        self.remember_on_forget(move || crate::animation::remove_animation_by_state(state_id));
    }

    /// Remember a callable whose placed state and nodes move between hosts without changing identity.
    /// The callable is initialized once, like `remember { movableContentOf { ... } }`; read dynamic
    /// values through State or an explicit latest-value channel rather than replacing its captures.
    ///
    /// An invocation conservatively enters its child scopes so current payloads and placement locals
    /// are applied. A completed composition with no placement disposes its stored state and nodes.
    ///
    /// Current winia restriction: a handle has one placement per composition, and movable invocation
    /// cannot be nested. Violations are rejected explicitly. Compose permits multiple independent
    /// placements of a callable; use separate handles for simultaneous copies in this minimal API.
    pub fn remember_movable_content(
        &mut self,
        content: impl Fn(&mut ComposeCtx) + Send + Sync + 'static,
    ) -> crate::runtime::movable::MovableContent {
        let slot_key = self.next_remember_key();
        self.composer.slot_table.remember_handle(slot_key, || {
            crate::runtime::movable::MovableContent::new(content)
        })
    }

    /// Compose this content here.
    /// Hand the slot table over to movable content #id — see
    /// [`MovableContent::compose`](crate::runtime::movable::MovableContent::compose).
    pub(crate) fn begin_movable(&mut self, id: u64) {
        self.composer.slot_table.validate_movable(id);
        let key = self.next_key();
        let caller_active = self.composer.slot_table.active_slot_key;
        let caller_group = self.composer.current_group_key;
        self.composer.slot_table.begin_movable(key, id);
        self.composer.movable_saved_skip_depth = self.composer.group_skip_stack.len();
        self.composer.movable_saved_path_counters = std::mem::take(&mut self.composer.path_counters);
        self.composer.movable_saved_remember_counters = std::mem::take(&mut self.composer.remember_path_counters);
        self.composer.movable_saved_params = std::mem::take(&mut self.composer.pending_params);
        self.composer.movable_saved_pending_next = std::mem::replace(&mut self.composer.pending_next, 0);
        self.composer.movable_saved_group_key = caller_group;
        self.composer.movable_saved_active = caller_active;
        // A placement owns its identities and dependency targets; inherited CompositionLocals remain
        // available, but caller statement/explicit-key/group stacks must not enter the stored tree.
        self.composer.movable_saved_scopes = std::mem::take(&mut self.composer.scope_source_stack);
        self.composer.movable_saved_stmts = STMT_STACK.with(|s| std::mem::take(&mut *s.borrow_mut()));
        self.composer.movable_saved_keys = std::mem::take(&mut self.composer.key_override_stack);
        self.composer.movable_saved_groups = GROUP_STACK.with(|s| std::mem::take(&mut *s.borrow_mut()));
        let root = self.composer.slot_table.current_slot().key;
        self.composer.slot_table.active_slot_key = root;
        ACTIVE_SLOT_KEY.with(|s| s.set(root));
        self.composer.entered_compose_keys.insert(root);
        GROUP_STACK.with(|s| s.borrow_mut().push(root));
    }

    /// Restore caller stacks even if the caller catches a panic and continues this composition.
    pub(crate) fn abort_movable(&mut self) {
        self.composer.slot_table.abort_movable();
        self.restore_movable_context();
    }

    /// Give the slot table and invocation context back to the caller.
    pub(crate) fn end_movable(&mut self) {
        self.composer.slot_table.end_movable();
        self.restore_movable_context();
    }

    fn restore_movable_context(&mut self) {
        self.composer.scope_source_stack = std::mem::take(&mut self.composer.movable_saved_scopes);
        self.composer.key_override_stack = std::mem::take(&mut self.composer.movable_saved_keys);
        STMT_STACK.with(|s| *s.borrow_mut() = std::mem::take(&mut self.composer.movable_saved_stmts));
        GROUP_STACK.with(|s| *s.borrow_mut() = std::mem::take(&mut self.composer.movable_saved_groups));
        self.composer.path_counters = std::mem::take(&mut self.composer.movable_saved_path_counters);
        self.composer.remember_path_counters = std::mem::take(&mut self.composer.movable_saved_remember_counters);
        self.composer.pending_params = std::mem::take(&mut self.composer.movable_saved_params);
        self.composer.pending_next = self.composer.movable_saved_pending_next;
        self.composer.group_skip_stack.truncate(self.composer.movable_saved_skip_depth);
        self.composer.current_group_key = self.composer.movable_saved_group_key;
        self.composer.slot_table.active_slot_key = self.composer.movable_saved_active;
        ACTIVE_SLOT_KEY.with(|s| s.set(self.composer.movable_saved_active));
    }

    /// Remember a write-back channel: same slot stability as `remember`, but
    /// the stored value never notifies (Backchannel write). For measure/layout
    /// write-back and cross-frame staging (see docs/state-handles.md).
    pub fn remember_backchannel<T: Clone + 'static>(
        &mut self,
        init: impl FnOnce() -> T,
    ) -> crate::runtime::state::Backchannel<T> {
        let slot_key = self.next_remember_key();
        self.composer.slot_table.remember_handle(slot_key, || {
            crate::runtime::state::Backchannel::new(init())
        })
    }

    /// Remember an animation-tick handle: recompose without waking the event
    /// loop (see docs/state-handles.md).
    pub fn remember_animating<T: Clone + PartialEq + 'static>(
        &mut self,
        init: impl FnOnce() -> T,
    ) -> crate::runtime::state::Animating<T> {
        let slot_key = self.next_remember_key();
        self.composer.slot_table.remember_handle(slot_key, || {
            crate::runtime::state::Animating::new(init())
        })
    }

    /// Remember a draw-layer handle: writes land without recomposition
    /// (see docs/state-handles.md).
    pub fn remember_visual<T: Clone + 'static>(
        &mut self,
        init: impl FnOnce() -> T,
    ) -> crate::runtime::state::Visual<T> {
        let slot_key = self.next_remember_key();
        self.composer.slot_table.remember_handle(slot_key, || {
            crate::runtime::state::Visual::new(init())
        })
    }

    /// 注册顶层弹出层（Popup/Dialog/DropdownMenu 内部调用）——组合期收集，
    /// compose 后由 app.rs 取走并独立物化/渲染
    pub fn open_overlay(&mut self, mut desc: crate::overlay::OverlayDesc) {
        // ⚠ 捕获 CompositionLocal 快照（主树 provides 内——Theme 等）——
        // overlay 独立 Composer 在 provides 弹栈后 recompose，读不到主树
        // 隐式上下文；快照重放让 overlay 继承主树主题/方向/排版。
        desc.local_snapshot = crate::runtime::composition_local::capture();
        self.composer.overlays.push(desc);
    }

    /// 组合期记录 overlay 的 active 状态（Popup/Dialog build 总执行时调用——
    /// 无论 visible 都记录；Skip 帧 build 不执行 → 本帧无记录 → sync 保留上帧）。
    /// sync_overlays 用此区分"注册方 Skip"（保留）与"主动关闭 visible=false"
    /// （记录 false → 删除）——slot 层无法区分，组合期显式记录是唯一正解。
    pub fn record_overlay_active(&mut self, id: u64, active: bool) {
        self.composer.overlay_active.insert(id, active);
    }

    /// Remember a write-back channel under an EXPLICIT key.
    ///
    /// The sibling of [`ComposeCtx::remember_at_key`] for state a provider has to keep per frame about
    /// ITSELF: no notification (the composer is already running), and no stable-call-site lookup — that
    /// lookup panics outside a `#[composable]`/keyed context, which a provider cannot require, since
    /// `WiniaTheme::provide_resolved` also runs from a window's per-frame wrapper.
    ///
    /// The caller owns key uniqueness: use [`crate::runtime::composer::mix_key`] over the position's
    /// identity so two providers in one composer never share a slot.
    pub fn remember_backchannel_at_key<T: Clone + 'static>(
        &mut self,
        key: u64,
        init: impl FnOnce() -> crate::runtime::state::Backchannel<T>,
    ) -> crate::runtime::state::Backchannel<T> {
        self.composer.slot_table.remember_handle(key, init)
    }

    /// A key for "the composition position we are at right now", mixed with `namespace`.
    ///
    /// A provider needs a per-SITE key: two `WiniaTheme::with_*` calls in one composer must not share
    /// the memory of what they last provided, or each would see the other's value as a change and dirty
    /// its subtree every frame. The position is the current group (a container's or scope's key, which
    /// is statement-derived and therefore stable) or, before any group, the slot path.
    pub fn position_key(&self, namespace: u64) -> u64 {
        let base = match GROUP_STACK.with(|s| s.borrow().last().copied()) {
            Some(key) => key,
            None => {
                // No group yet (a window's content wrapper is the common case): hash the slot path.
                let mut h: u64 = 0xcbf29ce484222325;
                for &idx in self.composer.slot_table.current_path() {
                    h ^= idx as u64;
                    h = h.wrapping_mul(0x100000001b3);
                }
                h
            }
        };
        mix_key(base, namespace)
    }

    /// Invalidate everything composed INSIDE this position — what a `CompositionLocal` provider calls
    /// when the value it is about to provide differs from the previous frame's.
    ///
    /// Why a provider has to do this itself: `provides` only changes what the NEXT reader reads. A
    /// reader re-reads a local when its own group runs again, and a group is re-entered only when it is
    /// dirty. A container that declares nothing about itself (`start_restartable_group` with an
    /// unchanged modifier and no `ctx.changed`) is therefore Skipped and keeps the value it composed
    /// with — the layout, padding and colors it built from the old one. Marking the inner slots makes
    /// those readers run again.
    ///
    /// Call it only when the value actually changed: it costs a re-entry of the whole subtree.
    pub fn mark_subtree_dirty(&mut self) {
        self.composer.mark_inner_slots_dirty();
    }

    /// 当前组合节点的 slot_key（DropdownMenu 锚点用）
    pub fn composer_slot_key(&self) -> u64 {
        self.composer.slot_table.active_slot_key()
    }

    /// 当前作用域内最后一个已组合兄弟的 slot_key（Popup 锚点用）——
    /// 紧跟最后组合的兄弟之后，等价于 Compose Popup 零尺寸占位节点在父布局中的位置。
    /// 无兄弟时返回 None（Popup 回退窗口对齐）。
    pub fn prev_sibling_slot_key(&self) -> Option<u64> {
        self.composer.slot_table.prev_sibling_slot_key()
    }

    /// Remember a plain handle (no `State` wrapper): the slot stores `T` itself.
    ///
    /// `remember` always allocates a `State<T>` — a `StateInner` plus a `StateSignal` — even when
    /// the caller only wants somewhere to keep a handle, which for an effect is every instance.
    /// This is the same storage the typed helpers below use; it is crate-private because exposing
    /// it would hand callers a slot they can never invalidate, which is a decision the framework
    /// has not made yet (`docs/state-handles.md` §7.5).
    pub(crate) fn remember_handle<T: Clone + 'static>(&mut self, init: impl FnOnce() -> T) -> T {
        let slot_key = self.next_remember_key();
        self.composer.slot_table.remember_handle(slot_key, init)
    }

    /// Queue `effect` to run once the composition being built has committed.
    ///
    /// This is Compose's `SideEffect`: it runs at the end of this `compose()`, after the slot tree
    /// is materialized and after the dependency frame closed. Two consequences: it sees the tree
    /// this frame actually built (not a half-built one), and a `State` it writes survives the pass
    /// that is ending rather than being swallowed by it.
    ///
    /// The write lands on the NEXT PASS, not the next frame: `recompose_until_stable` runs that pass
    /// before layout, so the value is on screen in the same frame.
    ///
    /// No cleanup and no rollback — a composition that panics before the drain drops the callback
    /// without running it. Use it to publish to something outside the composition (a platform
    /// window, a store, an analytics sink), not to mutate composition state.
    pub fn side_effect(&mut self, effect: impl FnOnce() + Send + 'static) {
        self.composer.post_compose.push(Box::new(effect));
    }

    /// 使用固定 key 记住一个状态（不受 remember_counter 影响，适合跨分支持久化的值）
    pub fn remember_at_key<T: Clone + 'static>(&mut self, key: u64, init: impl FnOnce() -> T) -> State<T> {
        self.composer.slot_table.remember(key, || State::new(init()))
    }

    /// 生成下一个组合 key（公开 API，用于 start_node）
    pub fn next_key(&mut self) -> u64 {
        self.composer.next_group_key()
    }

    /// Begins a composition scope: a `State::get()` inside it (a component body, outside any container
    /// group) registers its dependency on this scope, and a change re-enters the scope's body together
    /// with its DIRECT children — deeper descendants skip unless their own dependencies fired. That one
    /// level exists because winia cannot compare a value captured by a closure the way Compose compares
    /// a lambda parameter (see `mark_dirty_path_scope`); a component that needs its whole subtree
    /// re-entered says so with `mark_subtree_dirty()`. Pair with `end_scope`.
    pub fn start_scope(&mut self) -> u64 {
        self.composer.start_scope()
    }

    /// `#[composable]` 宏注入：以源码哈希为 scope key 开始（函数级 key 稳定——
    /// 结构变化不漂移）。内部节点的 next_key 以 scope 源码哈希为 key 基。
    /// ⚠ 不调用 start_scope（它也会 push None——双重 push 后栈顶是 None，
    /// next_key 读 scope=0 → 跨函数同 stmt id 的 key 碰撞 → 节点复用串位）
    pub fn start_scope_keyed(&mut self, source_hash: u64) -> u64 {
        self.composer.scope_source_stack.push(Some(source_hash));
        // scope key = 源码哈希本身（稳定唯一——不依赖 next_group_key：scope 是
        // 组合第一条调用（STMT_STACK 空），走路径哈希在宏外（app_root!/根）会
        // 触发稳定 key panic；且路径哈希在结构变化时漂移——hash 反而更稳）
        let key = self.composer.namespace_key_base(source_hash);
        self.composer.slot_table.start_scope(key);
        self.composer.entered_compose_keys.insert(key);
        GROUP_STACK.with(|s| s.borrow_mut().push(key));
        key
    }

    /// RAII 版 scope 开始（Drop 时自动 end_scope）——支持返回值函数与提前
    /// return（显式 end_scope 在提前退出时泄漏 scope 栈——新 key 系统
    /// `#[composable]` 宏展开使用此版本；guard 声明在函数开头、存活到函数
    /// 返回——end_scope 在所有语句 guard pop 之后执行，配对正确）。
    /// ⚠ guard 内持有 composer 裸指针——调用方必须保证 guard 生命周期内
    /// composer 存活且无并发访问（组合单线程——成立）。
    pub fn start_scope_guarded(&mut self, source_hash: u64) -> ScopeGuard {
        self.composer.scope_source_stack.push(Some(source_hash));
        let key = self.composer.namespace_key_base(source_hash);
        self.composer.slot_table.start_scope(key);
        self.composer.entered_compose_keys.insert(key);
        GROUP_STACK.with(|s| s.borrow_mut().push(key));
        ScopeGuard { composer: self.composer as *mut Composer }
    }

    /// 调用链版 RAII scope 开始：scope key = 当前调用链哈希（try_stable_base）
    /// 或 fallback 哈希（调用链空——测试/组合顶层）。组件方法宏化用——
    /// 同一方法多次实例化（16 字段）靠调用点链隔离；独立组合函数多实例
    /// （列表）同样隔离。⚠ scope 在函数开头创建——此刻 STMT_STACK 栈顶
    /// 即调用点（父语句），函数内部语句注入在其后——链不含自身。
    pub fn start_scope_callchain(&mut self, fallback_hash: u64) -> ScopeGuard {
        let raw_placement = self.composer.slot_table.movable_composing.is_some()
            && self.composer.key_override_stack.is_empty()
            && STMT_STACK.with(|s| s.borrow().is_empty());
        let key = if raw_placement {
            // A raw placement is a stable source for node ordinals, not for function identity.
            // Different composable functions must retain their own compiler fallback hashes.
            let base = self.composer.namespace_key_base(fallback_hash);
            let ordinal = self.composer.path_counters.entry(base).or_insert(0);
            let key = mix_key(base, *ordinal as u64);
            *ordinal += 1;
            key
        } else {
            self.composer.try_stable_base().unwrap_or_else(|| self.composer.namespace_key_base(fallback_hash))
        };
        #[cfg(debug_assertions)]
        if std::env::var("WINIA_SCOPE_TRACE").is_ok() {
            let stack = STMT_STACK.with(|s| s.borrow().clone());
            let src = self.composer.scope_source_stack.last().and_then(|s| *s);
            eprintln!("[scope] fallback={:#x} key={:#x} src={:?} stmt_stack={:?}", fallback_hash, key, src, stack);
        }
        self.composer.scope_source_stack.push(Some(key));
        self.composer.slot_table.start_scope(key);
        self.composer.entered_compose_keys.insert(key);
        GROUP_STACK.with(|s| s.borrow_mut().push(key));
        ScopeGuard { composer: self.composer as *mut Composer }
    }

    /// `#[composable]` 宏注入：进入一条语句（id 为编译期固定的源码位置序号）。
    /// 返回 RAII guard——语句块结束时 drop 自动 pop_stmt：闭包体/循环体内的
    /// `return`/`break`/`continue`/`panic!` 提前退出也不会泄漏 stmt 栈
    /// （显式 push/pop 在提前退出时栈会永久错位——后续语句 key 静默漂移）。
    ///
    /// seq（迭代位置）解析：**max(自身执行计数, 栈顶外层语句的 seq)**——
    /// ① for 体语句每次迭代都执行：自身计数 = 迭代位置（1..30）✓；② content
    /// 闭包内的语句只在容器 Enter 时执行（行 Skip 时 content 不跑）——自身计数
    /// 会漂移（首帧 seq=30，滚动后首次执行 seq=1）→ key 碰撞（text29 撞 text0）
    /// → 槽树 truncate 重建 → 内容丢失——继承外层行语句的迭代位置（max 兜底）。
    /// 已知限制：嵌套循环（for i { for j { … } }）内层语句取 max(内层次数, 外层
    /// 位置)——内层迭代与外层位置可能混淆，需显式 key（文档化）。
    pub fn enter_stmt(&mut self, id: u32) -> StmtGuard {
        // seq = 完整 child_counters 链哈希（位置而非执行次数）——start_slot 每帧
        // 无条件执行（Skip 帧也执行）→ 链跨帧稳定。替代旧 STMT_SEQ 执行计数
        // （每 compose 清空 + Skip 帧不执行 → 滚动时计数漂移 → seq≠迭代位置
        // → key 漂移 → dup-key panic——animation_demo 滚动卡死根因）。
        // 链含父层 index：content 闭包内语句不同行实例链不同 → seq 区分
        // （旧 last() 恒 0 + max(outer) 全继承行容器 seq → 行间冲突）。
        let seq = self.composer.slot_table.sibling_position();
        #[cfg(debug_assertions)]
        if std::env::var("WINIA_STMT_TRACE").is_ok() {
            eprintln!("[stmt] compose={} id={} seq={}", self.composer.compose_count, id, seq);
        }
        STMT_STACK.with(|s| s.borrow_mut().push((id, seq)));
        StmtGuard
    }

    /// `#[composable]` 宏注入：退出语句（与 push_stmt 配对）——保留兼容旧用法
    pub fn push_stmt(&mut self, id: u32) {
        let seq = self.composer.slot_table.sibling_position();
        STMT_STACK.with(|s| s.borrow_mut().push((id, seq)));
    }

    /// `#[composable]` 宏注入：退出语句（与 push_stmt 配对）
    pub fn pop_stmt(&mut self) {
        STMT_STACK.with(|s| { s.borrow_mut().pop(); });
    }

    /// 显式 key（对标 Compose `key(id)`）：包裹的子树用 id 哈希为 key 基——
    /// 结构变化（列表重排/子树移动）时 remember/复用仍稳定。
    /// 用法：`ctx.key("scroll_list", |ctx| { ... });`
    ///
    /// 重排这条直到 `start_slot` 加上按 key 的查找才真正成立：在那之前，一个 key 换了位置的子节点
    /// 会被当成新节点、从该索引截断重建（`a_keyed_sibling_that_moves_keeps_its_slots` 是那条测量，
    /// 去掉查找它就会数出 4 个槽而不是 2 个）。
    /// 显式 key 作用域（对标 Compose `key(key1, content)`）：`id` 参与内部所有
    /// 语句/组件的 key 基——`id` 变化 → 内部槽 key 变化 → 旧子树重建（结构切换）；
    /// `id` 稳定 → 跨重组 key 稳定（remember State 保留）。
    /// 支持任意 `Hash` 值：`key("section", ...)`、`key(i, ...)`（循环迭代变量——
    /// 嵌套循环内层迭代位置的显式正解，语句级 seq 只携带外层位置）、
    /// `key((row, col), ...)`。
    pub fn key<R>(&mut self, id: impl std::hash::Hash, f: impl FnOnce(&mut Self) -> R) -> R {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        id.hash(&mut hasher);
        let h = hasher.finish();
        let depth = self.composer.key_override_stack.len();
        self.composer.key_override_stack.push(h);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(self)));
        self.composer.key_override_stack.truncate(depth);
        match result {
            Ok(value) => value,
            Err(panic) => std::panic::resume_unwind(panic),
        }
    }


    /// 参数比较（对标 Compose `$composer.changed(param)`）。
    ///
    /// 在 start 组件**前**调用（参数求值处）：与"即将 start 的 slot"上帧记录的参数
    /// 按序比较——相等返回 `false`（参数未变），不等/首次返回 `true`。
    /// 暂存本帧参数（start_node 时写入 slot.params，供下帧比较）。
    ///
    /// 用法（`#[composable]` 组件内——参数未变 + slot clean 时容器 Skip，content 不重跑）：
    /// ```ignore
    /// `#[composable]`
    /// fn card(ctx: &mut ComposeCtx, title: &str) {
    ///     let _title_changed = ctx.changed(&title.to_string());  // 参数声明（start 容器前）
    ///     Column::new()
    ///         .modifier(Modifier::new().padding(8.0))
    ///         .build(ctx, |ctx| {
    ///             Text::new(title).build(ctx);
    ///             // title 未变 + slot clean → Column Skip（content 不执行）
    ///         });
    /// }
    /// ```
    pub fn changed<T: PartialEq + Clone + 'static>(&mut self, param: &T) -> bool {
        // 与"即将 start 的 child slot"的上帧 params 按序比较
        let param_idx = self.composer.pending_next;
        let unchanged = {
            let idx = *self.composer.slot_table.child_counters.last().unwrap_or(&0);
            let prev = self.composer.slot_table.current_slot().children.get(idx)
                .and_then(|c| c.params.get(param_idx))
                .map(|p| p.eq_any(param));
            prev == Some(true)
        };
        // Write into the buffer at this position, REUSING the box already there when it can hold this
        // type. The buffer is the previous frame's parameter vector (the slot hands it back at
        // `commit_pending_params`), so in a list — where every row declares the same parameter types —
        // the same handful of allocations circulates down the rows instead of one box per row per
        // frame. A slot holding a different type is dropped and re-allocated, which is also the path
        // a first frame takes.
        let reused = match self.composer.pending_params.get_mut(param_idx) {
            Some(existing) => existing.set_from_any(param),
            None => false,
        };
        if !reused {
            self.composer.pending_params.truncate(param_idx);
            self.composer.pending_params.push(Box::new(param.clone()));
            #[cfg(test)]
            param_alloc::note();
        }
        self.composer.pending_next = param_idx + 1;
        !unchanged
    }

    /// 结束组合 scope（与 start_scope 配对）
    pub fn end_scope(&mut self) {
        self.composer.end_scope();
    }

    /// 获取当前正在构建的节点 ID（用于注册选中、焦点等外部状态）
    pub fn current_node_id(&self) -> Option<u64> {
        self.composer.current_node_id()
    }

    pub fn layout_root(&self) -> Option<&crate::layout::node::LayoutNode> {
        self.composer.layout_root()
    }

    /// 设置当前选区注册表（由 SelectionContainer::build 调用）
    pub fn set_selection_registrar(&mut self, reg: crate::text::selection::SelectionRegistrar) {
        self.composer.selection_registrar = Some(reg);
    }

    pub fn clear_selection_registrar(&mut self) {
        self.composer.selection_registrar = None;
    }

    /// 给当前节点设 registrar 引用（供后续渲染/事件从中读取）。
    /// 组合期（物化前）写入当前 slot 的 desc——物化时应用到 arena 节点
    /// （组合/布局分离后 node_stack 在组合期为空，直接写节点会丢失）。
    pub fn set_current_node_registrar(&mut self, reg: crate::text::selection::SelectionRegistrar) {
        if let Some(desc) = &mut self.composer.slot_table.current_slot().desc {
            desc.registrar = Some(reg);
        }
    }

    /// 设置当前节点的光标位置和可见性，同时设置光标回调。
    /// ⚠ 组合/布局分离：组合期没有 arena 节点（node_stack 已废弃）——
    /// 写入当前 slot 的 desc，物化时应用到节点（与 focus_color 同一通道）。
    pub fn set_current_node_cursor_and_callback(
        &mut self,
        cursor_index: usize,
        visible: bool,
        callback: Box<dyn Fn(usize) + Send>,
    ) {
        if let Some(desc) = &mut self.composer.slot_table.current_slot().desc {
            desc.cursor_index = Some(cursor_index);
            desc.cursor_visible = Some(visible);
            desc.cursor_callback = Some(callback);
        }
    }

    /// 设置当前节点的焦点环颜色（组合期调用——主题色在此捕获；
    /// 渲染期 CompositionLocal 已退出，不能读主题）
    pub fn set_current_node_focus_color(&mut self, color: crate::graphics::Color) {
        // 组合/布局分离：组合期没有 arena 节点——写入当前 slot 的 desc，
        // 物化时应用到节点（与 set_current_node_registrar 同一通道）
        if let Some(desc) = &mut self.composer.slot_table.current_slot().desc {
            desc.focus_color = Some(color);
        }
    }

    /// 设置当前节点的 IME 组合下划线颜色（组合期调用——主题 primary 在此捕获；
    /// 渲染期 CompositionLocal 已退出，不能读主题，Phase 4.2）
    pub fn set_current_node_composing_color(&mut self, color: crate::graphics::Color) {
        if let Some(desc) = &mut self.composer.slot_table.current_slot().desc {
            desc.composing_color = Some(color);
        }
    }

    /// 设置当前节点的显示聚焦标记（text-field-v2 容器化：焦点在容器，
    /// 输入子节点用此标记渲染光标/选区）
    pub fn set_current_node_display_focused(&mut self, focused: bool) {
        if let Some(desc) = &mut self.composer.slot_table.current_slot().desc {
            desc.display_focused = Some(focused);
        }
    }

    /// animateFloatAsState — 动画浮点值到目标值
    pub fn animate_float_as_state(&mut self, target: f32, spec: crate::animation::AnimationSpec) -> State<f32> {
        self.animate_value_as_state(target, spec).into_state()
    }

    /// `animateFloatAsState(targetValue, animationSpec, label, finishedListener)` — the completion
    /// callback Compose's overload carries, fired once when a motion this call registered finishes.
    ///
    /// Prefer this over remembering a state and driving it with the free `push_animatable_with_done`:
    /// the state here is created and owned by the slot, so no second mechanism can land on it and make
    /// the dedup swallow the callback. An interrupted motion does not report (the retarget replaces its
    /// callback, as Compose's `animateTo` replaces its block).
    pub fn animate_float_as_state_with_done(
        &mut self,
        target: f32,
        spec: crate::animation::AnimationSpec,
        done: impl FnOnce() + Send + 'static,
    ) -> State<f32> {
        self.animate_value_as_state_with_done(target, spec, done).into_state()
    }

    /// `animateValueAsState` + the completion callback (Compose's `finishedListener`); any
    /// `AnimatableValue` works. See `animate_float_as_state_with_done` for the semantics.
    pub fn animate_value_as_state_with_done<
        T: Clone + PartialEq + crate::animation::AnimatableValue + Send + Sync + 'static,
    >(
        &mut self,
        target: T,
        spec: crate::animation::AnimationSpec,
        done: impl FnOnce() + Send + 'static,
    ) -> crate::runtime::state::Animating<T> {
        let slot_key = self.next_remember_key();
        let handle = self.composer.slot_table.remember_handle(slot_key, || {
            crate::runtime::state::Animating::new(target.clone())
        });
        self.own_motion(handle.state_id());
        crate::animation::push_animatable_with_done(
            State::from_raw(handle.as_raw().clone()),
            target,
            spec,
            done,
        );
        handle
    }

    /// animateColorAsState — 动画颜色值到目标值（RGBA 插值，Tween 驱动）
    pub fn animate_color_as_state(&mut self, target: crate::graphics::Color, spec: crate::animation::AnimationSpec) -> State<crate::graphics::Color> {
        let slot_key = self.next_remember_key();
        let handle = self.composer.slot_table.remember_handle(slot_key, || {
            crate::runtime::state::Animating::new(target)
        });
        self.own_motion(handle.state_id());
        crate::animation::push_animatable_color(
            crate::runtime::state::State::from_raw(handle.as_raw().clone()),
            target,
            spec,
        );
        handle.into_state()
    }

    /// animateDpAsState — 动画 Dp 值（对标 Compose animateDpAsState）
    pub fn animate_dp_as_state(&mut self, target: crate::unit::Dp, spec: crate::animation::AnimationSpec) -> State<crate::unit::Dp> {
        self.animate_value_as_state(target, spec).into_state()
    }

    /// animateOffsetAsState — 动画 Offset 值（对标 Compose animateOffsetAsState）
    pub fn animate_offset_as_state(&mut self, target: crate::unit::Offset, spec: crate::animation::AnimationSpec) -> State<crate::unit::Offset> {
        self.animate_value_as_state(target, spec).into_state()
    }

    /// animateSizeAsState — 动画 Size 值（对标 Compose animateSizeAsState）
    pub fn animate_size_as_state(&mut self, target: crate::unit::Size, spec: crate::animation::AnimationSpec) -> State<crate::unit::Size> {
        self.animate_value_as_state(target, spec).into_state()
    }

    /// animateIntAsState — 动画整数值（对标 Compose animateIntAsState）
    pub fn animate_int_as_state(&mut self, target: i32, spec: crate::animation::AnimationSpec) -> State<i32> {
        self.animate_value_as_state(target, spec).into_state()
    }

    /// animateValueAsState — 泛型值动画（对标 Compose animateValueAsState——
    /// 任何实现 AnimatableValue 的类型：lerp/to_f32/from_f32）
    pub fn animate_value_as_state<T: crate::animation::AnimatableValue + Send + Sync + 'static>(
        &mut self,
        target: T,
        spec: crate::animation::AnimationSpec,
    ) -> crate::runtime::state::Animating<T> {
        let slot_key = self.next_remember_key();
        let handle = self.composer.slot_table.remember_handle(slot_key, || {
            crate::runtime::state::Animating::new(target.clone())
        });
        self.own_motion(handle.state_id());
        crate::animation::push_animatable_handle(handle.clone(), target, spec);
        handle
    }

    /// 设置当前节点的 IME 预输入回调
    /// 设置当前节点的 IME 预输入回调（app.rs 的 Ime::Preedit 直接调用）。
    /// desc 通道（组合期无 arena 节点）
    pub fn set_current_node_ime_callback(&mut self, callback: Box<dyn Fn(&str, Option<(usize, usize)>) + Send>) {
        if let Some(desc) = &mut self.composer.slot_table.current_slot().desc {
            desc.ime_callback = Some(callback);
        }
    }

    /// 同步 composing_range 到当前节点（渲染画下划线用）。desc 通道——
    /// 外层 Option 区分"未设置"（非 TextField）与"清空"（组合结束）
    pub fn sync_composing_range(&mut self, range: Option<std::ops::Range<usize>>) {
        if let Some(desc) = &mut self.composer.slot_table.current_slot().desc {
            desc.composing_range = Some(range);
        }
    }

    /// 获取选区注册表
    pub fn selection_registrar(&self) -> Option<crate::text::selection::SelectionRegistrar> {
        self.composer.selection_registrar.clone()
    }

    /// 为 remember 调用生成位置 key。
    ///
    /// 位置 key 编码方式: 基于 slot 树路径（结构稳定——不随 Enter/Skip 的
    /// next_key 序列漂移，保证同一组合位置跨重组复用同一 State）。
    fn next_remember_key(&mut self) -> u64 {
        // key 基与 next_group_key 一致：显式 key() > 语句 id（源码位置）。
        // remember 的 State 跨帧稳定依赖 key 稳定——结构变化时语句 id 不动 → State 保留。
        // 无稳定源 → panic（fail-fast）。
        let base = match self.composer.try_stable_base() {
            Some(b) => b,
            None if cfg!(test) => {
                // 测试路径：路径哈希 fallback（同 next_group_key——测试自控结构）
                let path = self.composer.slot_table.current_path().to_vec();
                let mut h: u64 = 0xcbf29ce484222325;
                for &idx in &path {
                    h ^= idx as u64;
                    h = h.wrapping_mul(0x100000001b3);
                }
                h
            }
            None => self.composer.panic_no_stable_key("remember"),
        };
        let counter = self.composer.remember_path_counters.entry(base).or_insert(0);
        let c = *counter;
        *counter += 1;
        #[cfg(debug_assertions)]
        if std::env::var("WINIA_REMEMBER_TRACE").is_ok() {
            eprintln!(
                "[remember] compose={} base={:#x} n={} key={:#x} stmt={:?}",
                self.composer.compose_count,
                base,
                c,
                crate::runtime::composer::mix_key(base, c as u64),
                STMT_STACK.with(|s| s.borrow().last().copied())
            );
        }
        // key = fnv(base, 序号)——全 64 位混合，不丢身份熵。⚠ 不能用
        // (base << 32) | c（左移丢弃 base 高 32 位）或 base 高 32 位 | c
        // （丢弃 base 低 32 位 → 身份只剩 2^32 空间——checkbox 循环子项碰撞）。
        crate::runtime::composer::mix_key(base, c as u64)
    }

    /// 开始一个布局节点（叶子组件如 Text 使用）
    pub fn start_leaf(&mut self, key: u64, modifier: Modifier) {
        self.composer.start_node(key, modifier, None, None);
    }

    /// 开始一个容器节点（布局组件如 Button/Column 使用）
    pub fn start_container(
        &mut self,
        key: u64,
        modifier: Modifier,
        policy: impl MeasurePolicy + 'static,
    ) {
        self.composer
            .start_node(key, modifier, Some(Box::new(policy)), None);
    }

    /// Start a leaf node that carries an `on_remove` callback — one that needs a NODE. For a cleanup
    /// with no layout meaning, use [`crate::effect::on_forget`] instead.
    ///
    /// The distinction is not cosmetic. A node created here is a real child of the enclosing
    /// container, so it takes part in layout and consumes a spacing slot: an effect hung on a leaf
    /// between two rows of a `Column(spacing = 10)` pushed the second row from y=29 to y=39. That is
    /// exactly why `effect::on_forget` exists — it keeps the callback in the slot and materializes
    /// nothing (`docs/effects.md` §2).
    ///
    /// Reach for this only when the callback belongs to a node's actual lifetime — `Window` and
    /// `remember_infinite_transition` both used it for a while and both were migrated away, which is
    /// a fair signal that "I have a cleanup callback" is not a reason to use it.
    pub fn start_leaf_with_remove(
        &mut self,
        key: u64,
        modifier: Modifier,
        on_remove: Box<dyn FnOnce() + Send>,
    ) {
        self.composer.start_node(key, modifier, None, Some(on_remove));
    }

    /// 结束当前节点
    pub fn end_node(&mut self) {
        self.composer.end_node();
    }

    /// 开始一个可重启的组合分组（容器节点）。
    /// 返回 Enter（正常执行闭包）或 Skip（跳过内容，从缓存重放子树）。
    pub fn start_restartable_group(
        &mut self,
        key: u64,
        modifier: Modifier,
        policy: impl MeasurePolicy + 'static,
    ) -> GroupStatus {
        self.composer
            .start_restartable_group(key, modifier, Some(Box::new(policy)), None)
    }

    /// 结束一个可重启分组。
    /// 在 Enter 模式下等同于 end_node()；
    /// 在 Skip 模式下重放子 slot 结构并创建 stub LayoutNode。
    pub fn end_restartable_group(&mut self) {
        self.composer.end_restartable_group();
    }
}

// ── SlotTable ──

/// 节点描述（组合产物——组合树持有，布局阶段物化为 LayoutNode）
struct NodeDesc {
    key: u64,
    modifier: Modifier,
    policy: Option<Box<dyn MeasurePolicy>>,
    on_remove: Option<Box<dyn FnOnce() + Send>>,
    /// 本帧是否需重测（start_slot 的 Dirty 状态——slot.dirty 在 start_slot
    /// 被消费清 false，物化时须从 desc 携带）
    dirty: bool,
    /// 文本选择 registrar（组合期 set_current_node_registrar 写入——物化时应用）
    registrar: Option<crate::text::selection::SelectionRegistrar>,
    /// 焦点环颜色（组合期 set_current_node_focus_color 写入——物化时应用；
    /// 渲染期 CompositionLocal 已退出，必须组合期捕获）
    focus_color: Option<crate::graphics::Color>,
    /// IME 组合下划线颜色（组合期 set_current_node_composing_color 写入——
    /// 物化时应用；渲染期不能读 CompositionLocal（Phase 4.2），组合期捕获主题 primary）
    composing_color: Option<crate::graphics::Color>,
    /// 光标（TextField）——组合期写入，物化时应用（node_stack 已废弃——
    /// 组合期无 arena 节点，直接写节点会静默失效）
    cursor_index: Option<usize>,
    cursor_visible: Option<bool>,
    cursor_callback: Option<Box<dyn Fn(usize) + Send>>,
    /// 显示聚焦标记（text-field-v2 容器化：焦点/交互在容器节点，输入
    /// 子节点渲染光标/选区需组合期标记——渲染端优先用此，回退 node.focused）
    display_focused: Option<bool>,
    /// IME 预输入回调（TextField——app.rs 的 Ime::Preedit 直接调用）
    ime_callback: Option<Box<dyn Fn(&str, Option<(usize, usize)>) + Send>>,
    /// IME 组合范围（渲染画下划线用）——外层 Option 区分"未设置"与"清空"
    composing_range: Option<Option<std::ops::Range<usize>>>,
    /// 布局方向（组合期捕获——provides 作用域内读 CompositionLocal；
    /// 物化在组合回调后执行——届时主题的 provider 已退出作用域，
    /// 必须从 desc 携带，否则 RTL 下节点快照恒 Ltr → offset/padding 镜像失效）
    direction: crate::layout::LayoutDirection,
}

/// 组合节点的一个槽位。每个 composable 调用对应一个 Slot。
struct Slot {
    key: u64,
    remembered: HashMap<u64, Box<dyn Any>>,
    children: Vec<Slot>,
    /// 重组时是否需要执行（State 变化标记）
    dirty: bool,
    /// 当前帧中此 slot 的子树总 slot 数（含自身；用于 skip 时重放）
    children_count: usize,
    /// 是否为组合 scope（无 LayoutNode 的作用域节点——依赖注册目标 + 失效传播单位）
    is_scope: bool,
    /// 上帧参数（`ComposeCtx::changed` 写入，按序比较——对标 Compose `$composer.changed`）
    params: Vec<Box<dyn ParamValue>>,
    /// 节点描述（组合产物）——is_scope 或纯组合 Slot 为 None（物化阶段消费）
    desc: Option<NodeDesc>,
    /// 本帧是否被访问（start_slot 置 true；reset 每帧清）——物化只收集活跃
    /// slot：结构回退时（content 少建子节点）末尾残留的上帧 slot 不收集——
    /// 其 desc 不物化，对应 arena 节点由 prev_node_by_key 回收（free）
    visited: bool,
    /// Skip 子树时保留的组合产物 modifier（content 未执行——desc 为 None；
    /// 但 build 的 modifier 参数可能变化（父层重跑传入的 offset/背景等视觉
    /// 属性）——物化 Skip 恢复时应用，避免视觉卡旧值（动画中间值不渲染）
    skip_modifier: Option<Modifier>,
    /// 布局方向（组合期捕获——物化期读不到 CompositionLocal）
    direction: crate::layout::LayoutDirection,
    /// Skip 时保存的容器 policy（外层传入——content 未执行但 policy 可用，
    /// 物化降级/恢复时避免 policy 缺失导致测量 0 尺寸）
    skip_policy: Option<Box<dyn MeasurePolicy>>,
    /// This slot IS the storage root of movable content #id — Compose's `movableContentOf`.
    ///
    /// The content's slots live here, OUTSIDE the tree, so the branch that invokes the content can
    /// disappear without taking the content's `remember`ed state with it. A container that ends its
    /// group without visiting a child drops that child (`end_restartable_group`), which is exactly
    /// what loses the state today — putting the content in a store keeps it out of harm's way.
    movable_store: Option<u64>,
    /// Movable content #id is composed HERE. This slot holds nothing of its own: the descriptor walk
    /// inlines the store's subtree in its place, so the content's nodes are parented by whoever
    /// invokes it, while its slots (and so its state) live in the store.
    movable_ref: Option<u64>,
    /// Lifecycle callbacks owned by this slot, keyed like `remembered` so a recomposition
    /// REPLACES rather than stacks. Drained — and run — when the slot is dropped. This is what
    /// lets an effect clean up without materializing a layout node to carry the callback.
    forget: HashMap<u64, Box<dyn FnOnce() + Send>>,
}

impl Drop for Slot {
    /// The removal path with no collector: a slot can be dropped wholesale — the whole `Composer`
    /// going away, a `movable_store` entry removed, a `retain` in `end_scope` — and none of those
    /// go through `truncate_children`. `LayoutNode` has the same `Drop` for the same reason, and
    /// losing it here was a real leak: a `remember_infinite_transition`'s ids stayed in the global
    /// animation table after its composer was dropped (measured 1 -> 0 before the migration, 1 -> 1
    /// after it).
    ///
    /// The explicit sites drain `forget` before dropping, so this fires only for the paths they do
    /// not cover, and never twice.
    fn drop(&mut self) {
        for (_, f) in self.forget.drain() {
            f();
        }
    }
}

impl Slot {
    fn new(key: u64) -> Self {
        Self {
            key,
            remembered: HashMap::new(),
            children: Vec::new(),
            dirty: true, // 新创建的 slot 总是 dirty（首次必须执行）
            children_count: 1, // 自身
            is_scope: false,
            params: Vec::new(),
            desc: None,
            visited: true, // 新建即本帧活跃
            skip_modifier: None,
            skip_policy: None,
            movable_store: None,
            movable_ref: None,
            forget: HashMap::new(),
            direction: crate::layout::LayoutDirection::Ltr,
        }
    }

    fn remember<T: Clone + 'static>(
        &mut self,
        slot_key: u64,
        init: impl FnOnce() -> State<T>,
    ) -> State<T> {
        if let Some(existing) = self.remembered.get(&slot_key) {
            if let Some(state) = existing.downcast_ref::<State<T>>() {
                return state.clone();
            }
        }
        let state = init();
        self.remembered.insert(slot_key, Box::new(state.clone()));
        state
    }

    fn remember_handle<H: Clone + 'static>(
        &mut self,
        slot_key: u64,
        init: impl FnOnce() -> H,
    ) -> H {
        if let Some(existing) = self.remembered.get(&slot_key) {
            if let Some(handle) = existing.downcast_ref::<H>() {
                return handle.clone();
            }
        }
        let handle = init();
        self.remembered.insert(slot_key, Box::new(handle.clone()));
        handle
    }

}

/// 可重启分组状态 — start_restartable_group() 返回
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum GroupStatus {
    /// 子树完全 clean → 跳过内容闭包，重放 slot 结构
    Skip,
    /// 子树有变化 → 正常执行闭包
    Enter,
}


/// 槽位状态 — start_slot() 返回
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum SlotStatus {
    /// slot 与上一帧相同，且未被标记为脏 → 可跳过子树重建
    Clean,
    /// slot 与上一帧相同，但被标记为脏 → 需要重新测量
    Dirty,
}

/// 槽位表 — 组合树的内部数据结构（树形嵌套）

pub(crate) struct SlotTable {
    path: Vec<usize>,
    root_slot: Slot,
    child_counters: Vec<usize>,
    /// The slot subtrees of movable content, by id — Compose's movable content store. They are NOT
    /// in the tree: the invocation site holds a `movable_ref` slot and the descriptor walk inlines
    /// the store's subtree there, so the content survives its branch going away.
    movable_store: std::collections::HashMap<u64, Slot>,
    /// Which store is being composed right now, if any. While this is set, `current_slot` addresses
    /// the store instead of `root_slot`, so `remember`/`changed`/`start_slot` need no special case
    /// inside movable content.
    movable_composing: Option<u64>,
    /// The `path` while a store is being composed — saved on entry because the content's indices are
    /// relative to its own root, not to the invocation site.
    movable_saved_path: Vec<usize>,
    /// The `child_counters` while a store is being composed — the content's positions are its own.
    movable_saved_counters: Vec<usize>,
    /// Frame of the last invocation, used to reject unsupported duplicate placement explicitly.
    /// Compose supports multiple independent placements; this minimal API requires distinct handles.
    movable_frame: crate::layout::node::SlotKeyMap<u64>,
    /// The frame number `movable_frame` is compared against; `Composer::compose` sets it.
    frame_no: u64,
    /// 当前 compose 期间活跃的 slot key（用于 State→Slot 的脏标记）
    active_slot_key: u64,
    /// 被 State 变化标记为 dirty 的 slot key 集合
    dirty_keys: crate::layout::node::SlotKeySet,
}

struct SlotTableRuntimeSnapshot {
    path: Vec<usize>,
    child_counters: Vec<usize>,
    active_slot_key: u64,
    dirty_keys: crate::layout::node::SlotKeySet,
    movable_composing: Option<u64>,
    movable_saved_path: Vec<usize>,
    movable_saved_counters: Vec<usize>,
}

struct ComposeRuntimeSnapshot {
    slot_table: SlotTableRuntimeSnapshot,
    current_group_key: u32,
    path_counters: crate::layout::node::SlotKeyMap<u32>,
    remember_path_counters: crate::layout::node::SlotKeyMap<u32>,
    scope_source_stack: Vec<Option<u64>>,
    key_override_stack: Vec<u64>,
    pending_recomposition: VecDeque<u64>,
    needs_recomposition: bool,
    node_stack: Vec<usize>,
    group_skip_stack: Vec<bool>,
    overlay_active: HashMap<u64, bool>,
    entered_compose_keys: crate::layout::node::SlotKeySet,
    reused_nodes: crate::layout::node::NodeMarks,
}

/// Restores the small, non-owning compose runtime context on panic. The full
/// SlotTable/arena transaction remains deliberately separate because it owns
/// `Box<dyn Any>` and user callbacks.
struct ComposeRuntimeTransaction {
    composer: *mut Composer,
    committed: bool,
}

impl ComposeRuntimeTransaction {
    fn new(composer: &mut Composer) -> Self {
        composer.compose_transaction = Some(composer.capture_compose_runtime());
        Self { composer, committed: false }
    }

    fn commit(&mut self) {
        self.committed = true;
        unsafe { (*self.composer).compose_transaction = None; }
    }
}

impl Drop for ComposeRuntimeTransaction {
    fn drop(&mut self) {
        if !self.committed {
            unsafe { (*self.composer).rollback_compose_runtime(); }
        }
    }
}

/// State the claim walk needs while the slot tree is walked (`SlotTable::collect_desc_tree`).
struct ClaimCtx<'a> {
    arena: &'a crate::layout::node::NodeArena,
    prev: &'a mut crate::layout::node::SlotKeyMap<usize>,
    reused: &'a mut crate::layout::node::NodeMarks,
    /// Indices marked by the claim currently being verified. Merged into `reused` only once the whole
    /// subtree verified, so a mismatch leaves nothing behind.
    scratch: crate::layout::node::NodeMarks,
    /// Every index this frame's claims have taken, across all of them. The index walk drops these
    /// from `prev_node_by_key` ONCE per frame at the end — doing it per claim would make a list of N
    /// small skipped rows cost N scans of an N-entry map (measured: the one-row update went from
    /// 2023 µs to 3064 µs before this was hoisted out).
    claimed_frame: crate::layout::node::NodeMarks,
    /// Subtrees claimed in place during this walk, and subtrees whose claim was REJECTED because the
    /// arena did not match (test-visible through `Composer::skip_claims` / `skip_claim_bails`).
    claims: usize,
    bails: usize,
}

impl SlotTable {
    fn runtime_snapshot(&self) -> SlotTableRuntimeSnapshot {
        SlotTableRuntimeSnapshot {
            path: self.path.clone(),
            child_counters: self.child_counters.clone(),
            active_slot_key: self.active_slot_key,
            dirty_keys: self.dirty_keys.clone(),
            movable_composing: self.movable_composing,
            movable_saved_path: self.movable_saved_path.clone(),
            movable_saved_counters: self.movable_saved_counters.clone(),
        }
    }

    fn restore_runtime(&mut self, snapshot: SlotTableRuntimeSnapshot) {
        self.path = snapshot.path;
        self.child_counters = snapshot.child_counters;
        self.active_slot_key = snapshot.active_slot_key;
        self.dirty_keys = snapshot.dirty_keys;
        self.movable_composing = snapshot.movable_composing;
        self.movable_saved_path = snapshot.movable_saved_path;
        self.movable_saved_counters = snapshot.movable_saved_counters;
    }

    fn new() -> Self {
        Self {
            path: Vec::new(),
            root_slot: Slot::new(0),
            child_counters: vec![0],
            movable_store: std::collections::HashMap::new(),
            movable_composing: None,
            movable_saved_path: Vec::new(),
            movable_saved_counters: Vec::new(),
            movable_frame: crate::layout::node::SlotKeyMap::default(),
            frame_no: 0,
            active_slot_key: 0,
            dirty_keys: crate::layout::node::SlotKeySet::default(),
        }
    }

    fn current_slot(&mut self) -> &mut Slot {
        // Inside movable content the tree is the STORE, not the root: the content's slots live there
        // so that the branch invoking it can vanish without taking its state along.
        if let Some(id) = self.movable_composing {
            let store = self
                .movable_store
                .get_mut(&id)
                .expect("a store exists for the content being composed");
            let mut slot = store;
            for &idx in &self.path {
                slot = &mut slot.children[idx];
            }
            return slot;
        }
        let mut slot = &mut self.root_slot;
        for &idx in &self.path {
            slot = &mut slot.children[idx];
        }
        slot
    }

    /// 位置哈希（enter_stmt 的 seq 分量）：**完整 child_counters 链的 fnv**——
    /// 行容器语句（循环内）链 = [...,父层, 迭代位置]；content 闭包内语句链 =
    /// [...,行index, 行内位置]——不同行实例的链不同（父层 index 不同）→ seq
    /// 区分。start_slot 每帧无条件执行（Skip 帧也执行）→ 链跨帧稳定（位置
    /// 而非执行次数——替代旧 STMT_SEQ，滚动时 Skip/Enter 交替不再漂移）。
    pub(crate) fn sibling_position(&self) -> u32 {
        let mut start = 0;
        if let Some(id) = self.movable_composing {
            let mut slot = &self.movable_store[&id];
            for (depth, &index) in self.path.iter().enumerate() {
                slot = &slot.children[index];
                if slot.is_scope { start = depth + 1; }
            }
        }
        // Compiler scopes inside a placement already own stable identities. Their statement positions
        // are local to that scope, not to the ordinal at which the scope was called in the store.
        let mut h: u64 = 0xcbf29ce484222325;
        for &c in &self.child_counters[start..] {
            h ^= c as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        h as u32
    }


    /// 当前活跃 slot 的 key（overlay 锚点用）
    fn active_slot_key(&self) -> u64 {
        self.active_slot_key
    }

    /// 当前作用域内"前一个已组合兄弟"的 slot key（Popup 锚点用：对标 Compose
    /// Popup 在父布局中的位置）。按本帧 child_counters 索引——不能取
    /// children.last()：重组帧中上一帧的后缀兄弟尚未 truncate，会锚错节点。
    /// 无前一个兄弟时返回 None（调用方回退窗口对齐）。
    fn prev_sibling_slot_key(&self) -> Option<u64> {
        let idx = *self.child_counters.last().unwrap_or(&0);
        if idx == 0 {
            return None;
        }
        let mut slot = match self.movable_composing {
            Some(id) => self.movable_store.get(&id)?,
            None => &self.root_slot,
        };
        for &i in &self.path {
            slot = &slot.children[i];
        }
        slot.children.get(idx - 1).map(|c| c.key)
    }

    /// Hands `incoming` to the current slot and returns what the slot held before.
    ///
    /// The exchange (rather than an assignment) is what recycles: the returned vector goes back to
    /// `Composer::pending_params`, where the next `changed` calls overwrite its boxes in place
    /// (`ParamValue::set_from_any`) instead of allocating new ones.
    fn replace_current_params(&mut self, mut incoming: Vec<Box<dyn ParamValue>>) -> Vec<Box<dyn ParamValue>> {
        std::mem::swap(&mut self.current_slot().params, &mut incoming);
        incoming
    }

    /// 设置当前 slot 的节点描述（组合产物——物化阶段消费）
    fn set_current_desc(&mut self, desc: Option<NodeDesc>) {
        self.current_slot().desc = desc;
    }

    /// Skip 路径保留组合产物 modifier（content 未执行——desc=None；物化
    /// Skip 恢复节点时应用新 modifier——视觉属性（offset/背景）随父层重跑更新）
    fn set_skip_modifier(&mut self, modifier: Modifier) {
        self.current_slot().skip_modifier = Some(modifier);
    }

    /// Skip 分支组合期捕获布局方向（物化期读不到 CompositionLocal）
    fn set_skip_direction(&mut self, direction: crate::layout::LayoutDirection) {
        self.current_slot().direction = direction;
    }
    fn set_skip_policy(&mut self, policy: Option<Box<dyn MeasurePolicy>>) {
        self.current_slot().skip_policy = policy;
    }

    /// 收集物化描述树：Slot 树 → 纯节点树（scope 跳过——children 提升；
    /// Skip 子树 slot 记 skip 标记——物化时从 prev_node_by_key 恢复）。
    /// 消费 desc（take——policy/on_remove 移出）——物化阶段调用。
    /// visited 语义：本帧活跃（start_slot 置 true；reset 每帧清）——结构回退的
    /// 残留（visited false 且不在 Skip 子树内）不收集；Skip 子树（visited false
    /// 但属于 Skip group）整体收集（skip 标记——物化恢复）
    ///
    /// A skipped subtree whose slots are all INERT (no desc left, no skip payload left) is claimed
    /// IN PLACE instead of being re-encoded: the walk verifies node-for-node that the arena already
    /// holds exactly this subtree, marks the nodes as reused, drops their keys from
    /// `prev_node_by_key` (so the compose tail does not recycle them), and emits ONE descriptor
    /// carrying the claimed index. See `try_claim_skipped_subtree` for why that is equivalent, and
    /// `docs/benchmarks.md` for what it is worth: at 800 rows an idle frame spent ~390 µs building
    /// and then walking a descriptor tree for a subtree where nothing had changed.
    pub(crate) fn collect_desc_tree(
        &mut self,
        out: &mut Vec<crate::runtime::materialize::DescNode>,
        arena: &crate::layout::node::NodeArena,
        prev: &mut crate::layout::node::SlotKeyMap<usize>,
        reused: &mut crate::layout::node::NodeMarks,
    ) -> (usize, usize) {
        fn rec(
            slot: &mut Slot,
            out: &mut Vec<crate::runtime::materialize::DescNode>,
            in_skip: bool,
            depth: usize,
            ctx: &mut ClaimCtx,
            store: &mut std::collections::HashMap<u64, Slot>,
        ) {
            if !slot.visited && !in_skip { return; }
            // A live reference inlines its placement. When the invoking host skipped, replay the
            // stored tree as a skipped subtree; never revive an unvisited reference from an entered
            // host or an uncomposed tail from a placement whose body ran.
            if let Some(id) = slot.movable_ref {
                // Held out of the map across the walk so the recursion can keep its own `&mut` to the
                // store (movable content inside movable content is not supported, so nothing else can
                // reach for this same entry).
                let Some(mut content) = store.remove(&id) else { return };
                let replay = !content.visited;
                for child in &mut content.children {
                    rec(child, out, replay, depth + 1, ctx, store);
                }
                store.insert(id, content);
                return;
            }
            if let Some(desc) = slot.desc.take() {
                let mut node = crate::runtime::materialize::DescNode {
                    key: desc.key,
                    skip: false,
                    modifier: desc.modifier,
                    preserve_modifier: false,
                    policy: desc.policy,
                    on_remove: desc.on_remove,
                    dirty: desc.dirty, // start_slot 的 Dirty 状态（slot.dirty 已消费）
                    registrar: desc.registrar,
                    focus_color: desc.focus_color,
                    composing_color: desc.composing_color,
                    cursor_index: desc.cursor_index,
                    cursor_visible: desc.cursor_visible,
                    cursor_callback: desc.cursor_callback,
                    display_focused: desc.display_focused,
                    ime_callback: desc.ime_callback,
                    composing_range: desc.composing_range,
                    direction: desc.direction,
                    claimed: None,
                    children: Vec::new(),
                };
                for child in &mut slot.children {
                    rec(child, &mut node.children, false, depth + 1, ctx, store);
                }
                out.push(node);
            } else if !slot.is_scope {
                // Skip 子树 slot（content 未执行——desc 空但非 scope）：
                // 整棵子树按 key 结构恢复（物化时从 prev_node_by_key 恢复——
                // 不物化上帧 desc——子树整体保留，children 重新挂接）
                let claimed = SlotTable::try_claim_skipped_subtree(slot, ctx);
                let sm = slot.skip_modifier.take();
                let sp = slot.skip_policy.take();
                let mut node = crate::runtime::materialize::DescNode {
                    key: slot.key,
                    skip: true,
                    claimed,
                    modifier: sm.clone().unwrap_or_default(),
                    // 容器自身 build 被调（set_skip_modifier 写入）→ 应用新 modifier；
                    // 后代（Skip 子树内未执行）→ 保留缓存节点 modifier（不清空视觉）
                    preserve_modifier: sm.is_none(),
                    policy: sp,
                    on_remove: None,
                    dirty: false,
                    registrar: None,
                    focus_color: None,
                    composing_color: None,
                    cursor_index: None,
                    cursor_visible: None,
                    cursor_callback: None,
                    display_focused: None,
                    ime_callback: None,
                    composing_range: None,
                    direction: slot.direction,
                    children: Vec::new(),
                };
                if claimed.is_none() {
                    for child in &mut slot.children {
                        rec(child, &mut node.children, true, depth + 1, ctx, store); // Skip 子树内：子也按同一规则（收集）
                    }
                }
                #[cfg(debug_assertions)]
                if std::env::var("WINIA_MAT_PROBE").is_ok() {
                    eprintln!(
                        "[collect] skip key={:x} kids={} claimed={:?}",
                        slot.key, node.children.len(), claimed
                    );
                }
                out.push(node);
            } else {
                // scope：不物化——children 提升到最近物化父（保持 in_skip 状态）
                for child in &mut slot.children {
                    rec(child, out, in_skip, depth + 1, ctx, store);
                }
            }
        }
        let mut ctx = ClaimCtx {
            arena,
            prev,
            reused,
            scratch: crate::layout::node::NodeMarks::default(),
            claimed_frame: crate::layout::node::NodeMarks::default(),
            claims: 0,
            bails: 0,
        };
        for child in &mut self.root_slot.children {
            rec(child, out, false, 0, &mut ctx, &mut self.movable_store);
        }
        if ctx.claims > 0 {
            let claimed = std::mem::take(&mut ctx.claimed_frame);
            ctx.prev.retain(|_, idx| !claimed.contains(*idx));
        }
        (ctx.claims, ctx.bails)
    }

    /// Tries to keep a skipped subtree exactly where it already is.
    ///
    /// A skipped subtree is, by definition, one whose content did not run — so its slots still say
    /// what they said the last time it was built, and the arena already holds the result of building
    /// exactly that. Re-encoding it into descriptors and walking them back into the arena (the
    /// fallback this replaces) reproduces the tree node for node; measured at 800 rows, that cost
    /// ~390 µs of an idle frame (`docs/benchmarks.md`).
    ///
    /// The claim is only taken when it is provably a no-op beyond bookkeeping. That means every slot
    /// in the subtree must be INERT:
    ///
    /// * no `desc` (a live desc is this frame's content waiting to be applied, and the descriptor path
    ///   consumes it — `take()` — so skipping that would leave the slot in a different state),
    /// * no `skip_modifier` / `skip_policy` for anything but the root (the same `take()`
    ///   choreography: a descendant holding a payload would have it applied today and cleared).
    ///
    /// The root is exempt because its payload is what the caller carries on the descriptor anyway.
    /// With every slot inert, the fallback's per-node work reduces to: reuse the node, clear its
    /// children and re-add the same ones in the same order, keep the same modifier, keep the same
    /// dirty flag — all of which the arena already satisfies. What is left is the bookkeeping this
    /// does: mark the nodes (so the compose tail does not recycle them) and drop their keys from
    /// `prev_node_by_key`.
    ///
    /// Verification is node for node, not a count: at every level the arena node's children must be
    /// exactly the nodes the slot children claim, in order. That subsumes the shape check the
    /// fallback performs at skip boundaries (which exists so a structure change cannot resurrect a
    /// stale measurement — `docs/benchmarks.md` tells that story), and it is what makes "the arena
    /// already holds this subtree" a checked fact rather than an assumption. A mismatch claims
    /// nothing (the marks go to a scratch set that is merged only on success), so the caller's
    /// fallback starts from an untouched arena.
    ///
    /// The verification is not a formality — something DOES mutate the arena between the `layout` that
    /// fills `prev_node_by_key` and this walk: `app.rs` runs `poll_shared_flights()` after `layout()`,
    /// and a flight detachment unlinks nodes (`detach_source`'s `children.retain`). That is the window
    /// the child-order comparison above exists for, and `test_stale_arena_shape_makes_the_claim_bail`
    /// exercises it directly by swapping a container's children.
    ///
    /// **The walk has two jobs and both are load-bearing.** It verifies, and it MARKS every node of the
    /// subtree as reused — the compose tail then frees everything in `prev_node_by_key` that is not
    /// marked, and `free_node_skip` reads the same set. Skipping the walk (rather than just the
    /// comparisons) was tried as a measurement and corrupts the tree on the first idle frame: the
    /// unmarked descendants get recycled under the live subtree, come back as `LayoutNode::default()`
    /// with key 0, and `collect_node_keys` panics with `[dup-key]` on two nodes claiming that key. So
    /// "claim without walking" is not a cheaper claim; it is a broken one.
    fn try_claim_skipped_subtree(slot: &Slot, ctx: &mut ClaimCtx) -> Option<usize> {
        let root = *ctx.prev.get(&slot.key)?;
        ctx.scratch.clear();
        if !SlotTable::verify_claimed_node(slot, root, ctx) {
            ctx.bails += 1;
            return None;
        }
        // Committed: the subtree is the frame's tree, so the drain must not reclaim it (the index is
        // swept once, at the end of the walk) and every node in it counts as reused (the
        // shared-element detach reads this set).
        ctx.reused.merge(&ctx.scratch);
        ctx.claimed_frame.merge(&ctx.scratch);
        ctx.claims += 1;
        Some(root)
    }

    /// Marks `node_idx` as claimed and verifies it is the node `slot` materializes to, children
    /// included.
    fn verify_claimed_node(slot: &Slot, node_idx: usize, ctx: &mut ClaimCtx) -> bool {
        if !ctx.scratch.insert(node_idx) {
            return false; // already claimed: a cycle, or two keys naming one node
        }
        if ctx.prev.get(&slot.key) != Some(&node_idx) {
            return false;
        }
        let mut pos = 0usize;
        if !SlotTable::verify_claimed_children(
            &slot.children,
            &ctx.arena.nodes[node_idx].children,
            &mut pos,
            ctx,
        ) {
            return false;
        }
        pos == ctx.arena.nodes[node_idx].children.len()
    }

    /// Walks `slots` against `expected` — the arena children list of the node whose children they
    /// are — advancing `pos` once per materialized slot. Scopes materialize nothing and pass their
    /// children through, exactly as the descriptor walk hoists them.
    fn verify_claimed_children(
        slots: &[Slot],
        expected: &[usize],
        pos: &mut usize,
        ctx: &mut ClaimCtx,
    ) -> bool {
        for slot in slots {
            if slot.desc.is_some() {
                // A live desc: the fallback applies it and consumes it. Not inert — bail.
                return false;
            }
            if slot.is_scope {
                if !SlotTable::verify_claimed_children(&slot.children, expected, pos, ctx) {
                    return false;
                }
                continue;
            }
            // A skipped group or an inert leaf. Only the ROOT may still hold a payload; anything
            // reached here is a descendant, so a payload means the fallback would apply it.
            if slot.skip_modifier.is_some() || slot.skip_policy.is_some() {
                return false;
            }
            let Some(&node_idx) = expected.get(*pos) else {
                return false; // the parent lists fewer children than the slots materialize
            };
            *pos += 1;
            if !SlotTable::verify_claimed_node(slot, node_idx, ctx) {
                return false;
            }
        }
        true
    }

    fn start_slot(&mut self, key: u64) -> SlotStatus {
        let idx = *self.child_counters.last().unwrap_or(&0);
        self.active_slot_key = key;
        ACTIVE_SLOT_KEY.with(|c| c.set(key));
        let is_dirty = self.dirty_keys.remove(&key);
        // Lifecycle callbacks from slots this call drops; run at the END, once the tree and the
        // cursors are consistent again.
        let mut orphaned: Vec<Box<dyn FnOnce() + Send>> = Vec::new();
        #[cfg(debug_assertions)] {
            if std::env::var("WINIA_SLOT_TRACE").is_ok() {
                eprintln!("[slot] key={} path={:?} dirty={}", key >> 32, self.path.clone(), is_dirty);
            }
        }
        let parent = self.current_slot();
        #[cfg(debug_assertions)] {
            if std::env::var("WINIA_SLOT_TRACE").is_ok() {
                // Report the slot's OWN dirty flag as well as the key-set one: the two are different
                // sources (path marking vs a state write) and only their combination decides Skip.
                let slot_state = match parent.children.get(idx) {
                    Some(child) if child.key == key => format!("slot_dirty={}", child.dirty),
                    Some(child) => format!("slot_key_mismatch={:x}", child.key),
                    None => "no_slot".to_string(),
                };
                eprintln!(
                    "[slot] key={} idx={} parent_kids={} key_dirty={} {}",
                    key >> 32,
                    idx,
                    parent.children.len(),
                    is_dirty,
                    slot_state
                );
            }
        }

        // key 匹配，或"同位置"（key 高位 = slot 路径哈希相同）——Enter/Skip 的
        // counter 漂移不改位置，按索引复用（更新 key 保持同步），避免 truncate
        // 重建 slot 导致 remember 的 State 丢失
        let same_position = idx < parent.children.len()
            && parent.children[idx].key >> 32 == key >> 32;
        // A key that sits LATER among the siblings is a reorder — an explicit `ctx.key` list that
        // moved an item, since statement order cannot change on its own. Rotate that slot into the
        // current position instead of truncating from here: truncating would drop every sibling
        // after `idx` and rebuild them, losing the `remember`ed state the key exists to preserve.
        // Measured before this: a two-entry keyed swap recreated both slots.
        //
        // The search only fires when the index does not already match (below), and only among this
        // parent's direct children, so the unkeyed path is untouched: two unkeyed siblings have
        // different statement keys and neither is ever found elsewhere.
        if !(idx < parent.children.len()
            && (parent.children[idx].key == key || same_position))
        {
            if let Some(found) = parent.children.iter().position(|c| c.key == key) {
                if found > idx {
                    let moved = parent.children.remove(found);
                    parent.children.insert(idx, moved);
                }
            }
        }
        if idx < parent.children.len() && (parent.children[idx].key == key || same_position) {
            parent.children[idx].key = key; // 同步最新 key（counter 可能漂移）
            parent.children[idx].visited = true; // 本帧活跃（物化收集依据）
            // An ordinary scope/node can reuse a former reference position. Only begin_movable may
            // mark this visit as a reference; stale roles would replay an already removed payload.
            parent.children[idx].movable_ref = None;
            if !parent.children[idx].dirty && !is_dirty {
                self.path.push(idx);
                self.child_counters.last_mut().map(|c| *c += 1);
                self.child_counters.push(0);
                return SlotStatus::Clean;
            }
            parent.children[idx].dirty = false;
            self.path.push(idx);
        } else {
            #[cfg(debug_assertions)]
            if std::env::var("WINIA_SLOT_TRACE").is_ok() {
                let plen = parent.children.len();
                let pkey = parent.key;
                eprintln!("[slot-trunc] key={:x} parent={:x} idx={} len={}", key, pkey, idx, plen);
            }
            // Dropped slots take their lifecycle callbacks with them; run them after the
            // structural change so a callback that composes nothing sees a consistent tree.
            orphaned = SlotTable::truncate_children(&mut parent.children, idx);
            parent.children.push(Slot::new(key));
            // 新建 slot：本帧返回 Dirty 即已执行；立即消费 dirty 标记，
            // 否则残留 true 会让下一帧本应 clean 的 slot 误判为 Dirty
            parent.children.last_mut().unwrap().dirty = false;
            self.path.push(idx);
        }
        if let Some(last) = self.child_counters.last_mut() { *last += 1; }
        self.child_counters.push(0);
        // Last, not first: `parent.children` and `self.path` are already in their new shape, so a
        // panicking cleanup unwinds from a consistent composer instead of one missing the slot it
        // was about to create.
        for f in orphaned { f(); }
        SlotStatus::Dirty
    }

    fn end_slot(&mut self) {
        // 计算子树 slot 总数（自身 + 所有子 slot 的 children_count 之和）
        let count = 1 + self.current_slot().children.iter()
            .map(|c| c.children_count)
            .sum::<usize>();
        self.current_slot().children_count = count;
        self.path.pop();
        self.child_counters.pop();
    }

    /// Register a lifecycle callback on the CURRENT slot, replacing any earlier one at the same
    /// key. It runs when the slot is dropped, never on a plain recomposition.
    fn remember_on_forget(&mut self, slot_key: u64, f: Box<dyn FnOnce() + Send>) {
        self.current_slot().forget.insert(slot_key, f);
    }

    /// Drain a whole removed subtree's callbacks into `out`. Moves out of the slot rather than
    /// borrowing, because the caller drops the slot straight after.
    fn collect_forgets(mut slot: Slot, out: &mut Vec<Box<dyn FnOnce() + Send>>) {
        out.extend(std::mem::take(&mut slot.forget).into_values());
        for child in std::mem::take(&mut slot.children) {
            SlotTable::collect_forgets(child, out);
        }
    }

    /// Truncate `children` to `len`, handing back the callbacks owned by whatever was dropped.
    /// Every place that drops slots goes through here, so a `remember_on_forget` callback can
    /// never leak just because a branch took a different shape.
    fn truncate_children(
        children: &mut Vec<Slot>,
        len: usize,
    ) -> Vec<Box<dyn FnOnce() + Send>> {
        let mut out = Vec::new();
        for dropped in children.drain(len.min(children.len())..) {
            SlotTable::collect_forgets(dropped, &mut out);
        }
        out
    }

    fn remember<T: Clone + 'static>(
        &mut self,
        slot_key: u64,
        init: impl FnOnce() -> State<T>,
    ) -> State<T> {
        self.current_slot().remember(slot_key, init)
    }

    fn remember_handle<H: Clone + 'static>(
        &mut self,
        slot_key: u64,
        init: impl FnOnce() -> H,
    ) -> H {
        self.current_slot().remember_handle(slot_key, init)
    }

    fn reset(&mut self) {
        self.path.clear();
        self.child_counters = vec![0];
        self.active_slot_key = 0;
        // 每帧清 visited——物化只收集本帧活跃 slot（结构回退的残留不收集）
        // 同时清空未消费的 desc（panic 残留帧产物），避免下一帧 Clean 复用
        // slot 时 collect_desc_tree 误收集旧 desc 物化错误节点。
        // 正常成功帧的 desc 已被 collect_desc_tree take（=None），此操作无副作用。
        fn clear_visited(slot: &mut Slot) {
            slot.visited = false;
            let _ = slot.desc.take(); // 丢弃 panic 残留的帧产物 desc
            for child in &mut slot.children {
                clear_visited(child);
            }
        }
        clear_visited(&mut self.root_slot);
        // The store is not part of the tree, so it needs the same per-frame clearing by hand: an
        // unvisited/leftover desc there must not be materialized on the strength of a stale frame.
        for store in self.movable_store.values_mut() {
            clear_visited(store);
        }
    }

    fn truncate(&mut self) {
        if let Some(&idx) = self.child_counters.first() {
            let orphaned = SlotTable::truncate_children(&mut self.root_slot.children, idx);
            for f in orphaned { f(); }
        }
    }

    fn dispose_unplaced_movable(&mut self) {
        fn clear_references(slot: &mut Slot) {
            slot.movable_ref = None;
            for child in &mut slot.children { clear_references(child); }
        }
        fn references(slot: &mut Slot, in_skip: bool, live: &mut crate::layout::node::SlotKeySet) {
            if !slot.visited && !in_skip {
                // Scope residue can survive in the owning tree. Once omitted from an entered branch,
                // its references must never revive when that branch skips in a later composition.
                clear_references(slot);
                return;
            }
            if let Some(id) = slot.movable_ref {
                assert!(live.insert(id), "multiple live placements of one movable handle are not supported; use distinct handles");
                return;
            }
            let child_skip = in_skip || (slot.desc.is_none() && !slot.is_scope && slot.skip_modifier.is_some());
            for child in &mut slot.children { references(child, child_skip, live); }
        }
        let mut live = crate::layout::node::SlotKeySet::default();
        for child in &mut self.root_slot.children { references(child, false, &mut live); }
        // A movable store that is going away can carry lifecycle callbacks (an effect placed inside
        // movable content). Its slots are dropped here, not truncated out of a parent, so collect
        // them explicitly — otherwise that cleanup would silently never run.
        let mut orphaned = Vec::new();
        for (id, store) in self.movable_store.iter_mut() {
            if !live.contains(id) {
                SlotTable::collect_forgets(std::mem::replace(store, Slot::new(0)), &mut orphaned);
            }
        }
        for f in orphaned { f(); }
        self.movable_store.retain(|id, _| live.contains(id));
        self.movable_frame.retain(|id, _| live.contains(id));
    }

    /// 外部标记 slot key 为 dirty（由 State 变化触发）。
    /// 同时递归标记所有祖先 slot，确保父级 start_restartable_group 返回 Enter。
    pub(crate) fn mark_dirty(&mut self, key: u64) {
        self.dirty_keys.insert(key);
        SlotTable::mark_dirty_path_scope(&mut self.root_slot, key);
        // Stored readers are not descendants of their host in the ownership tree. Route their
        // invalidation through each live reference so an otherwise clean host enters the placement.
        let mut invalidated = Vec::new();
        for (&id, store) in &mut self.movable_store {
            if SlotTable::mark_dirty_path_scope(store, key) {
                invalidated.push(id);
            }
        }
        for id in invalidated {
            SlotTable::mark_reference_path(&mut self.root_slot, id);
        }
    }

    fn mark_reference_path(slot: &mut Slot, id: u64) -> bool {
        let mut found = slot.movable_ref == Some(id);
        for child in &mut slot.children {
            found |= SlotTable::mark_reference_path(child, id);
        }
        if found { slot.dirty = true; }
        found
    }

    /// Find the slot holding `key`: mark it and all its ancestors dirty. Ancestors are marked so the
    /// walk can REACH the reader (a clean group's body does not run, and its children are never
    /// visited) — Compose instead schedules the invalidated scope directly, without re-running the
    /// groups above it.
    ///
    /// A read that lands on a SCOPE (the common `let v = state.get(); Child { … v … }` shape inside a
    /// `#[composable]` body) marks that scope and its DIRECT children. The body itself always runs, so
    /// its children are what decide Skip/Enter — and winia cannot compare a captured value as a
    /// parameter the way Compose compares a lambda, so without this the child would keep rendering the
    /// value it captured last time. Deeper descendants still skip unless their own dependencies fired:
    /// this is a one-level approximation of Compose's "invalidate the reading scope", not the
    /// previously unbounded subtree re-entry, and a component that needs the whole subtree says so
    /// explicitly with `ctx.mark_subtree_dirty()`.
    fn mark_dirty_path_scope(slot: &mut Slot, key: u64) -> bool {
        if slot.key == key {
            slot.dirty = true;
            if slot.is_scope {
                for child in &mut slot.children {
                    child.dirty = true;
                }
            }
            return true;
        }
        for child in &mut slot.children {
            if SlotTable::mark_dirty_path_scope(child, key) {
                slot.dirty = true;
                return true;
            }
        }
        false
    }

    /// 标记 slot 及其所有后代 dirty（scope 失效传播）
    fn mark_dirty_subtree(slot: &mut Slot) {
        slot.dirty = true;
        for child in &mut slot.children {
            SlotTable::mark_dirty_subtree(child);
        }
    }

    /// 开始一个组合 scope（无 LayoutNode 的作用域节点——依赖注册目标 + 失效传播单位）
    fn start_scope(&mut self, key: u64) -> SlotStatus {
        let status = self.start_slot(key);
        // 标记当前（刚进入的）slot 为 scope
        self.current_slot().is_scope = true;
        status
    }

    /// Start composing movable content #id at the current position.
    ///
    /// A `movable_ref` slot goes into the tree here. It holds nothing of its own — the descriptor walk
    /// inlines the store's subtree in its place — and composition then switches INTO the store, so the
    /// content's `remember`ed values live somewhere no branch can drop them.
    ///
    /// Pair with [`SlotTable::end_movable`].
    fn validate_movable(&self, id: u64) {
        assert!(self.movable_composing.is_none(), "nested movable placement is not supported");
        assert!(self.movable_frame.get(&id) != Some(&self.frame_no),
            "multiple placements of one movable handle are not supported; use distinct handles");
    }

    fn begin_movable(&mut self, key: u64, id: u64) {
        self.movable_frame.insert(id, self.frame_no);
        // The reference itself is an ordinary slot in the tree: the position it sits at is where the
        // content's nodes will be parented.
        let status = self.start_slot(key);
        self.current_slot().is_scope = true;
        self.current_slot().movable_ref = Some(id);
        let _ = status;

        // The reference slot is COMPLETE: it holds nothing of its own (the walk inlines the store in
        // its place), so close it before switching into the store. Leaving its path frame open put the
        // rest of the invoking component's composition INSIDE the ref slot — measured: the navigation
        // suite's item lost its label child, because everything composed after the payload landed
        // under a slot the walk ignores (`navigation_bar.rs:659`, `navigation_rail.rs:1396`,
        // "index out of bounds: the len is 2 but the index is 2").
        self.end_slot();

        if let Some(store) = self.movable_store.get_mut(&id) {
            // Calling the retained lambda can supply fresh captured values even without a move.
            // Until lambda/parameter revisions are modeled explicitly, re-enter its readers rather
            // than skipping clean wrappers around the latest payload. Remembered values stay intact.
            SlotTable::mark_dirty_subtree(store);
        }
        let store = self.movable_store.entry(id).or_insert_with(|| {
            let mut store = Slot::new(mix_key(0x6d6f_7661_626c_6501, id));
            store.is_scope = true;
            store.movable_store = Some(id);
            store
        });
        store.visited = true;
        // Save the invoking site's position and hand composition over to the store. The order matters:
        // the save must happen AFTER the ref slot is closed, or the saved path carries the ref's own
        // frame and every later invocation lands one level too deep.
        self.movable_saved_path = std::mem::take(&mut self.path);
        self.movable_saved_counters = std::mem::take(&mut self.child_counters);
        self.child_counters.push(0);
        self.movable_composing = Some(id);
    }

    /// Stop composing movable content, back at the position that invoked it.
    fn end_movable(&mut self) {
        assert!(self.path.is_empty(), "unbalanced nodes inside movable content");
        let count = self.child_counters.first().copied().unwrap_or(0);
        // This root is not closed by end_restartable_group. Trim its uncomposed tail here, otherwise
        // descriptor replay resurrects removed conditional/list children.
        let orphaned = {
            let slot = self.current_slot();
            SlotTable::truncate_children(&mut slot.children, count)
        };
        for f in orphaned { f(); }
        self.movable_composing = None;
        self.path = std::mem::take(&mut self.movable_saved_path);
        self.child_counters = std::mem::take(&mut self.movable_saved_counters);
    }

    fn abort_movable(&mut self) {
        let id = self.movable_composing.take().expect("a movable placement is active");
        self.movable_store.remove(&id);
        self.movable_frame.remove(&id);
        fn clear_reference(slot: &mut Slot, id: u64) {
            if slot.movable_ref == Some(id) { slot.movable_ref = None; }
            for child in &mut slot.children { clear_reference(child, id); }
        }
        clear_reference(&mut self.root_slot, id);
        self.path = std::mem::take(&mut self.movable_saved_path);
        self.child_counters = std::mem::take(&mut self.movable_saved_counters);
    }

    /// 结束组合 scope
    fn end_scope(&mut self) {
        if self.movable_composing.is_some() {
            // Compiler-only scopes have no restartable container to prune their removed children.
            // Their bodies ran, so unvisited descendants are absent, not cached movable state.
            self.current_slot().children.retain(|child| child.visited);
        }
        self.end_slot();
    }

    /// 设置当前（栈顶）slot 是否为 scope（start_node 复用 scope slot 时重置为普通节点）
    fn set_current_scope(&mut self, is_scope: bool) {
        self.current_slot().is_scope = is_scope;
    }

    /// 在 slot 树中查找 key 对应的节点，并将其及所有祖先标记 dirty。
    /// 返回 true 表示找到了目标。
    /// 返回当前 slot 在树中的路径（用于 LayoutNode 复用时的 measured_size 查找）
    fn current_path(&self) -> &[usize] {
        &self.path
    }

    /// An upper bound on how many layout nodes this composition can materialize: every slot in the
    /// tree, plus the root. Used by `materialize` to size the arena ONCE instead of letting it grow,
    /// and by `collect_live_keys` for the live-key set.
    ///
    /// Computed by SUMMING the root's children's subtree counts, not by reading the root's own
    /// `children_count`: that field is written by `end_slot`, and the root slot is never ended, so it
    /// holds its constructor value (1) forever. A first version of this read it directly and every
    /// "reserve" built on it asked for 2 nodes — which is how a reserve can look like a win (the
    /// hasher it shipped with was doing the work) and still be a no-op (`docs/benchmarks.md`).
    ///
    /// The bound over-counts (scope slots materialize nothing, and a claimed subtree allocates
    /// nothing), which is the safe direction: capacity that goes unused costs address space, not work,
    /// and a reserve against capacity already sufficient is a comparison.
    pub(crate) fn slot_count_bound(&self) -> usize {
        1 + self
            .root_slot
            .children
            .iter()
            .map(|child| child.children_count)
            .sum::<usize>()
    }

    /// Collect slot keys that remain part of the current composition. A skipped
    /// subtree is retained structurally even though its descendants were not visited.
    ///
    /// The set is RESERVED from the slot tree's own node count (`children_count`, maintained by
    /// `end_slot`) rather than growing: a `HashSet` filling up to 4000 entries rehashes everything
    /// several times on the way, and this runs every frame. `SlotKeySet` carries the cheap hasher for
    /// the same reason — both together were worth 2x on this call (`docs/benchmarks.md`).
    ///
    /// Returns how many slots it SKIPPED: slots that are still in the tree but were not composed this
    /// frame and are not inside a skipped subtree. The count is free here (the walk is already
    /// descending past exactly those slots) and it is the evidence for a claim that matters when
    /// somebody wants to replace this set with "every slot in the tree" — see the test that reads it.
    /// Note it only means anything at THIS moment: materialize later consumes the frame's markers
    /// (`desc` and `skip_modifier` are `take()`n), so the same tree read after a frame looks like it
    /// has far more residue than it did during composition.
    fn collect_live_keys(&self, out: &mut crate::layout::node::SlotKeySet) -> usize {
        fn visit(
            slot: &Slot,
            out: &mut crate::layout::node::SlotKeySet,
            in_skip: bool,
            skipped: &mut usize,
            stores: &HashMap<u64, Slot>,
        ) {
            if !slot.visited && !in_skip {
                *skipped += 1;
                return;
            }
            out.insert(slot.key);
            if let Some(id) = slot.movable_ref {
                if let Some(store) = stores.get(&id) {
                    let replay = !store.visited;
                    out.insert(store.key);
                    for child in &store.children { visit(child, out, replay, skipped, stores); }
                }
                return;
            }
            let child_in_skip = in_skip
                || (slot.desc.is_none() && !slot.is_scope && slot.skip_modifier.is_some());
            for child in &slot.children {
                visit(child, out, child_in_skip, skipped, stores);
            }
        }

        // An upper bound: every slot in the tree, whether or not it turns out to be live.
        out.reserve(self.slot_count_bound());

        let mut skipped = 0usize;
        for child in &self.root_slot.children {
            visit(child, out, false, &mut skipped, &self.movable_store);
        }
        skipped
    }
}

/// Snapshot of mutable Composer state touched by layout. A measurement policy
/// can execute user code, so panic must preserve the last committed graph and
/// the invalidation batch for a retry.
struct LayoutTransactionSnapshot {
    pending: Vec<StateId>,
    layout_dirty_keys: HashSet<u64>,
    prev_node_by_key: crate::layout::node::SlotKeyMap<usize>,
    layout_slot_reads: crate::layout::node::SlotKeyMap<HashSet<StateId>>,
    layout_deps: HashMap<StateId, HashSet<u64>>,
    layout_signal_handles: HashMap<StateId, Arc<StateSignal>>,
    removed_slot_keys: HashSet<u64>,
    root: Option<usize>,
    nodes_len: usize,
    free_nodes: Vec<usize>,
    free_policies: Vec<usize>,
    node_state: Vec<LayoutNodeTransactionState>,
    scroll_limits: Vec<(crate::runtime::state::Backchannel<f32>, f32)>,
}

/// The state a layout frame can change on a node that already existed.
///
/// Deliberately NOT the whole node. `LayoutTransaction` exists so a panic mid-layout leaves the
/// composer recoverable, and what it keeps is the set of fields layout actually writes: measure writes
/// the size, the flags and the scroll metrics, and placement writes the position. What is left out is
/// out because layout cannot touch it — the modifier chain, the child list, the measure policy and the
/// content flags are all composition products, written by `materialize::materialize` while COMPOSING,
/// and the transaction's whole lifetime sits inside `layout()`.
///
/// Cloning them cost a `Vec` per node plus — through a `TextContent`'s `String` — a string per text
/// node, every frame. Measured at 4001 nodes, that clone was the single largest item in layout, larger
/// than measurement itself; removing it cut an idle 800-row frame by 40% (`docs/benchmarks.md`).
///
/// Rollback already forces a complete retry (`dirty = true`, cached constraints and paragraph cleared),
/// so nothing here has to preserve a partially rebuilt measurement either.
#[derive(Clone)]
struct LayoutNodeTransactionState {
    idx: usize,
    focused: bool,
    measured_size: crate::unit::Size,
    position: crate::unit::Offset,
    scroll_viewport_height: f32,
    scroll_viewport_width: f32,
    scroll_content_height: f32,
    scroll_content_width: f32,
    scroll_reverse: bool,
}

struct LayoutTransaction {
    composer: *mut Composer,
    snapshot: Option<LayoutTransactionSnapshot>,
    committed: bool,
}

impl LayoutTransaction {
    /// Takes the whole-tree reuse index layout is about to rebuild, and keeps it for rollback.
    ///
    /// `prev_node_by_key` is cleared and rebuilt from the arena by `layout()`
    /// (`collect_layout_index`), so the snapshot needs its old content only because a panic has to
    /// leave the composer exactly as it was — and that content is genuinely needed. It does not follow
    /// that it must be COPIED: the rebuild begins by clearing, so while the new content is being built
    /// nothing reads what the composer had. Moving the map into the snapshot is therefore equivalent,
    /// and it removed a deep clone of every node's cached entry — a `Modifier` (`Vec` + any strings)
    /// and a `RefCell` per node. Measured: 149 µs of an idle 800-row layout (4001 nodes), on a frame
    /// whose actual measurement folded in 0.04 µs (`docs/benchmarks.md`).
    ///
    /// What remains is the drop of the snapshot's map at commit (which the clone paid too) plus one
    /// allocation for the map being rebuilt — which is why `collect_layout_index` reserves up front
    /// rather than letting a `HashMap` grow into 4000 entries. Rollback is unchanged: it restores the
    /// map it took.
    fn new(composer: &mut Composer) -> Self {
        let node_state = composer
            .arena
            .nodes
            .iter()
            .enumerate()
            .map(|(idx, node)| LayoutNodeTransactionState {
                idx,
                focused: node.focused,
                measured_size: node.measured_size,
                position: node.position,
                scroll_viewport_height: node.scroll_viewport_height,
                scroll_viewport_width: node.scroll_viewport_width,
                scroll_content_height: node.scroll_content_height,
                scroll_content_width: node.scroll_content_width,
                scroll_reverse: node.scroll_reverse,
            })
            .collect();
        let mut scroll_limits = Vec::new();
        for node in &composer.arena.nodes {
            if let Some(scroll) = node.modifier.vertical_scroll_state() {
                scroll_limits.push((scroll.fling_limit.clone(), scroll.fling_limit.peek()));
            }
            if let Some(scroll) = node.modifier.horizontal_scroll_state() {
                scroll_limits.push((scroll.fling_limit.clone(), scroll.fling_limit.peek()));
            }
        }

        Self {
            composer: composer as *mut Composer,
            snapshot: Some(LayoutTransactionSnapshot {
                pending: composer.pending_states.pending_ids(),
                layout_dirty_keys: composer.layout_dirty_keys.clone(),
                prev_node_by_key: std::mem::take(&mut composer.prev_node_by_key),
                layout_slot_reads: composer.layout_slot_reads.clone(),
                layout_deps: composer.layout_deps.clone(),
                layout_signal_handles: composer.layout_signal_handles.clone(),
                removed_slot_keys: composer.removed_slot_keys.clone(),
                root: composer.arena.root,
                nodes_len: composer.arena.nodes.len(),
                free_nodes: composer.arena.free.clone(),
                free_policies: composer.arena.free_policies.clone(),
                node_state,
                scroll_limits,
            }),
            committed: false,
        }
    }

    fn commit(&mut self) {
        self.committed = true;
        self.snapshot = None;
    }

    unsafe fn rollback(&mut self) {
        let Some(snapshot) = self.snapshot.take() else { return };
        let composer = unsafe { &mut *self.composer };

        composer.pending_states.restore_pending(&snapshot.pending);
        composer.layout_dirty_keys = snapshot.layout_dirty_keys;
        composer.prev_node_by_key = snapshot.prev_node_by_key;
        composer.layout_slot_reads = snapshot.layout_slot_reads;
        composer.layout_deps = snapshot.layout_deps;
        composer.layout_signal_handles = snapshot.layout_signal_handles;
        composer.removed_slot_keys = snapshot.removed_slot_keys;
        composer.arena.root = snapshot.root;
        composer.arena.nodes.truncate(snapshot.nodes_len);
        composer.arena.free = snapshot.free_nodes;
        composer.arena.free_policies = snapshot.free_policies;

        for state in snapshot.node_state {
            if let Some(node) = composer.arena.nodes.get_mut(state.idx) {
                node.focused = state.focused;
                node.measured_size = state.measured_size;
                node.position = state.position;
                node.scroll_viewport_height = state.scroll_viewport_height;
                node.scroll_viewport_width = state.scroll_viewport_width;
                node.scroll_content_height = state.scroll_content_height;
                node.scroll_content_width = state.scroll_content_width;
                node.scroll_reverse = state.scroll_reverse;
                // Force a complete retry. This is safer than restoring a
                // partially rebuilt paragraph or measurement cache.
                node.dirty = true;
                node.layout_dirty = false;
                node.cached_constraints = None;
                if let Ok(mut paragraph) = node.cached_paragraph.try_borrow_mut() {
                    *paragraph = None;
                }
            }
        }
        for (state, value) in snapshot.scroll_limits {
            state.set(value);
        }
    }
}

impl Drop for LayoutTransaction {
    fn drop(&mut self) {
        if !self.committed {
            // Rollback must not panic while an application panic is unwinding.
            unsafe { self.rollback(); }
        }
    }
}

/// Snapshot of the dependency graph committed by the last successful compose.
/// This is intentionally narrower than a SlotTable/arena transaction: it restores
/// only graph state and subscriptions after a late compose panic.
struct ComposeDependencySnapshot {
    compose_slot_reads: crate::layout::node::SlotKeyMap<HashSet<StateId>>,
    slot_deps: HashMap<StateId, HashSet<u64>>,
    signal_handles: HashMap<StateId, Arc<StateSignal>>,
    layout_slot_reads: crate::layout::node::SlotKeyMap<HashSet<StateId>>,
    layout_deps: HashMap<StateId, HashSet<u64>>,
    layout_signal_handles: HashMap<StateId, Arc<StateSignal>>,
    pending: Vec<StateId>,
    layout_dirty_keys: HashSet<u64>,
    removed_slot_keys: HashSet<u64>,
}

/// Restores dependency maps and live signal subscriptions if compose panics
/// after reconciliation but before its cleanup completes.
struct ComposeDependencyTransaction {
    composer: *mut Composer,
    snapshot: Option<ComposeDependencySnapshot>,
    committed: bool,
}

impl ComposeDependencyTransaction {
    fn new(composer: &Composer) -> Self {
        Self {
            composer: composer as *const Composer as *mut Composer,
            snapshot: Some(ComposeDependencySnapshot {
                compose_slot_reads: composer.compose_slot_reads.clone(),
                slot_deps: composer.slot_deps.clone(),
                signal_handles: composer.signal_handles.clone(),
                layout_slot_reads: composer.layout_slot_reads.clone(),
                layout_deps: composer.layout_deps.clone(),
                layout_signal_handles: composer.layout_signal_handles.clone(),
                pending: composer.pending_states.pending_ids(),
                layout_dirty_keys: composer.layout_dirty_keys.clone(),
                removed_slot_keys: composer.removed_slot_keys.clone(),
            }),
            committed: false,
        }
    }

    fn commit(&mut self) {
        self.snapshot = None;
        self.committed = true;
    }

    unsafe fn rollback(&mut self) {
        let Some(snapshot) = self.snapshot.take() else { return };
        let composer = unsafe { &mut *self.composer };
        composer.compose_slot_reads = snapshot.compose_slot_reads;
        composer.slot_deps = snapshot.slot_deps;
        composer.signal_handles = snapshot.signal_handles;
        composer.layout_slot_reads = snapshot.layout_slot_reads;
        composer.layout_deps = snapshot.layout_deps;
        composer.layout_signal_handles = snapshot.layout_signal_handles;
        composer.layout_dirty_keys = snapshot.layout_dirty_keys;
        composer.removed_slot_keys = snapshot.removed_slot_keys;

        // Remove every live subscription created or retained by the failed
        // frame before restoring the committed signal set. Otherwise a signal
        // that only appeared in the failed graph can still enqueue the queue.
        composer.pending_states.retain_signals(&HashSet::new());

        // Restore the exact pending batch from before this compose began.
        // Notifications produced by the failed frame must not leak into the
        // restored dependency graph.
        composer.pending_states.drain();
        composer.pending_states.restore_pending(&snapshot.pending);

        // Replace the queue's signal set with the last committed handles.
        let mut restored = HashSet::new();
        for signal in composer
            .signal_handles
            .values()
            .chain(composer.layout_signal_handles.values())
        {
            if restored.insert(signal.id()) {
                composer.pending_states.subscribe_signal(signal);
            }
        }

        composer.debug_assert_dependency_graphs();
    }
}

impl Drop for ComposeDependencyTransaction {
    fn drop(&mut self) {
        if !self.committed {
            // Rollback must not panic while another application panic unwinds.
            unsafe { self.rollback(); }
        }
    }
}

/// Keeps compose-triggering invalidations queued until the compose transaction
/// reaches its final cleanup. A panic restores only the consumed batch; other
/// Composer mutations remain governed by the existing self-healing path.
///
/// # 不变量（形式化，2026-08-26）
///
/// 1. **帧内新通知只进入下一批**：`drain_matching`（state.rs:159）在队列锁内
///    原子消费匹配的 IDs，未匹配的 IDs 留在队列。帧内 `State::set` → `enqueue`
///    （state.rs:126）在锁内追加新 ID → 新通知一定在队列中等待下帧，不会在当前帧
///    被消费。请勿在 `drain_matching` 锁外修改 pending 队列。
///
/// 2. **panic 恢复不重复**：`Drop` 未 commit 时调用 `restore_pending`
///    （state.rs:147），在锁内**去重**追加——帧内已到来的新通知不会被重复添加。
///
/// 3. **commit 后不再回滚**：commit 后 `ids` 清空、`committed=true`，`Drop` 不
///    做任何操作——已消费的批被视为已提交。
///
/// 4. **布局 pending 不被 compose 误消费**：`drain_matching` 只取出 `slot_deps`
///    中存在的 IDs（composeIds），layout-only 的 IDs 留在队列，由 `layout()`
///    通过 `drain_non_compose_collect_layout`（state.rs:175）原子消费。
struct PendingBatchGuard {
    queue: Arc<ComposerSubscription>,
    ids: Vec<StateId>,
    committed: bool,
}

impl PendingBatchGuard {
    fn new(queue: Arc<ComposerSubscription>, ids: Vec<StateId>) -> Self {
        Self {
            queue,
            ids,
            committed: false,
        }
    }

    fn ids(&self) -> &[StateId] {
        &self.ids
    }

    fn commit(&mut self) {
        self.ids.clear();
        self.committed = true;
    }
}

impl Drop for PendingBatchGuard {
    fn drop(&mut self) {
        if !self.committed {
            self.queue.restore_pending(&self.ids);
        }
    }
}

// ── Composer ──

/// 组合引擎。
///
/// 每个窗口/组合根拥有一个 Composer 实例。
/// 负责:
/// - 管理 SlotTable（组合节点的持久化存储）
/// - 调度重组（状态变化 → 标记 dirty → 下一帧批量重组）
/// - 持有全局 key 计数器
static NEXT_COMPOSER_UID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub struct Composer {
    pub(crate) slot_table: SlotTable,
    /// Identity of this composer, unique for the process lifetime. Anything that caches per composer
    /// must key on this rather than on an arena index: node indices are local to a composer, so two
    /// windows (or a page and an overlay) both have a node 0.
    pub(crate) uid: u64,
    /// Where this window's saveable values come from and go to. Provided to every compose pass, so
    /// `remember_saveable` sees it on every frame rather than only the first.
    pub(crate) saveable: Option<std::sync::Arc<crate::runtime::saveable::SaveableStateRegistry>>,
    pub(crate) current_group_key: u32,
    compose_transaction: Option<ComposeRuntimeSnapshot>,
    /// 每路径独立 counter（next_group_key 用）——同组合位置跨帧 counter 恒定，
    /// key 不随 Skip/Enter 的 next_key 调用序变化（全局 counter 会因 Skip 的
    /// content 不执行而平移 → key 漂移 → 节点复用错位 + 常量折叠冻结）
    path_counters: crate::layout::node::SlotKeyMap<u32>,
    /// 每路径独立 counter（next_remember_key 用——同上，防 remember key 漂移）
    remember_path_counters: crate::layout::node::SlotKeyMap<u32>,

    /// 组合 scope 的源码哈希栈（宏传——函数级 key 基）
    scope_source_stack: Vec<Option<u64>>,
    /// Invocation context restored when a movable placement finishes, including explicit keys and
    /// dependency targets. Neither identities nor readers inside a placement belong to its caller.
    movable_saved_scopes: Vec<Option<u64>>,
    movable_saved_stmts: Vec<(u32, u32)>,
    movable_saved_keys: Vec<u64>,
    movable_saved_groups: Vec<u64>,
    movable_saved_active: u64,
    movable_saved_skip_depth: usize,
    movable_saved_group_key: u32,
    movable_saved_path_counters: crate::layout::node::SlotKeyMap<u32>,
    movable_saved_remember_counters: crate::layout::node::SlotKeyMap<u32>,
    movable_saved_params: Vec<Box<dyn ParamValue>>,
    movable_saved_pending_next: usize,
    /// ctx.key() 显式 key 栈（最高优先级）
    key_override_stack: Vec<u64>,
    pending_recomposition: VecDeque<u64>,
    needs_recomposition: bool,
    pub(crate) arena: crate::layout::node::NodeArena,
    node_stack: Vec<usize>,
    /// 记录每个 start_restartable_group 的 skip 状态（用于 end_restartable_group 判断）
    group_skip_stack: Vec<bool>,
    /// 顶层弹出层（Popup/Dialog/DropdownMenu——组合期注册，compose 后取走；
    /// 内容为独立组合单元——独立 Composer 物化/布局/渲染，不参与主树布局）
    pub(crate) overlays: Vec<crate::overlay::OverlayDesc>,
    /// Callbacks queued by `ComposeCtx::side_effect`, run once at the END of a successful
    /// `compose()` — after the slot tree is materialized and after the dependency frame closed.
    /// Compose's `SideEffect`: publishing to the world outside the composition, with no cleanup
    /// and no rollback. A pass that panics before the drain drops them unrun.
    post_compose: Vec<Box<dyn FnOnce() + Send>>,
    /// 本帧组合期各 overlay 的 active 状态（Popup/Dialog build 总执行时记录——
    /// Skip 帧不记录 → sync 保留上帧；主动关闭 visible=false → 记录 false →
    /// sync 删除）。区分"注册方 Skip"（保留）与"主动关闭"（删除）——
    /// slot 层不可区分，需此组合期显式记录。
    pub(crate) overlay_active: HashMap<u64, bool>,
    /// state_id -> slot_keys 依赖映射
    slot_deps: HashMap<StateId, HashSet<u64>>,
    /// compose 期每个 slot 的完整读取集合；Enter 时替换，Skip 时保留。
    ///
    /// `SlotKeyMap` for the same reason as `entered_compose_keys` beside it: keyed by `slot_key`, and
    /// the convergence pass looks a key up here up to three times per ENTERING group — 800 groups in a
    /// list loop is 2400 lookups a frame, and they were paying the std hasher on keys the composer had
    /// already mixed. Measured on the row loop (800 rows, the container re-running, every row
    /// skipping): 889 → 840 µs, best of four interleaved runs each way, with the unchanged idle arm as
    /// the control — the cost is proportional to the ENTERED set, which is what the idle frame does not
    /// have (`docs/benchmarks.md`).
    compose_slot_reads: crate::layout::node::SlotKeyMap<HashSet<StateId>>,
    /// compose 依赖对应的 signal handle，用于移除 stale Composer 订阅。
    signal_handles: HashMap<StateId, Arc<StateSignal>>,
    /// layout 依赖对应的 signal handle；compose 与 layout 共享一个队列但独立收敛。
    layout_signal_handles: HashMap<StateId, Arc<StateSignal>>,
    /// 帧内实际执行过的 compose slot；用于按 Enter/Skip 语义收敛读取集合。
    ///
    /// `SlotKeySet` rather than a plain `HashSet`: this is keyed by `slot_key` and takes one insert per
    /// ENTERING group — 4000 of them on a cold frame, with the std hasher on keys the composer had
    /// already mixed (`docs/benchmarks.md`).
    entered_compose_keys: crate::layout::node::SlotKeySet,
    /// 布局期每个 slot 的读取集合；测量命中时替换，常量折叠时保留。
    /// 与 `compose_slot_reads` 同一理由取 `SlotKeyMap`（见上）。
    layout_slot_reads: crate::layout::node::SlotKeyMap<HashSet<StateId>>,
    /// 布局依赖反向表（state_id → slot_key；由 layout_slot_reads 重建）
    layout_deps: HashMap<StateId, HashSet<u64>>,
    /// 本帧 pending 消费收集的布局失效 key（layout() 应用后清空）。
    /// 协调器（shared_transition 的 flight writer）每帧往这里播种飞行端点的
    /// slot_key：`layout()` 每帧先清全树 layout_dirty 再按此集合重标祖先，
    /// 不播种则折叠的父级永不下探（逐帧重测就无从发生）。
    pub(crate) layout_dirty_keys: HashSet<u64>,
    /// Set by the shared-element writers when a flight attaches its layout override for the
    /// FIRST time (the node had none). The override can only be attached after layout —
    /// resolving a flight needs the target's measured rect — so the pass that just ran told
    /// the entering end's parent the target's NATURAL size; without this the frame renders
    /// with the parent stale and the rows below the hero dip and snap back. The app loop
    /// consumes it once per frame and re-lays-out, so at most one extra pass per composer per
    /// frame happens, and only on frames where a flight starts.
    pub(crate) layout_override_fresh: bool,
    /// 本帧确认移除的 slot_key（compose 末尾回收未复用节点时收集——layout_deps 死 key 清理用）
    removed_slot_keys: HashSet<u64>,
    /// 本 Composer 实例的 pending state 通知队列
    pending_states: Arc<crate::runtime::state::ComposerSubscription>,
    /// `ComposeCtx::changed` 暂存的参数（start_slot 时写入新 slot 的 params）
    ///
    /// Doubles as a RECYCLING buffer: `commit_pending_params` swaps it with the slot's previous vector,
    /// so the boxes (and the `Vec`'s capacity) come back here to be overwritten next frame instead of
    /// being dropped and re-allocated. Only the first `pending_next` entries are this frame's.
    pending_params: Vec<Box<dyn ParamValue>>,
    /// Write cursor into `pending_params` — the position the next `changed` writes, and the count of
    /// declarations made for the group being built. A cursor rather than `pending_params.len()`, because
    /// that vector now arrives carrying the previous frame's (longer or shorter) parameter list.
    pending_next: usize,
    /// 上帧布局树：slot_key → arena 节点索引（阶段D 节点复用——start_node 按 key 复用槽位）
    pub(crate) prev_node_by_key: crate::layout::node::SlotKeyMap<usize>,
    /// 本帧已复用的节点索引（free 时跳过——避免递归进本帧树形成环）
    pub(crate) reused_nodes: crate::layout::node::NodeMarks,
    /// Subcompositions parked during the current layout pass, waiting for adoption
    /// (`layout::subcompose`). Cleared at the start of every `layout()`; emptied by the adoption pass.
    pub(crate) subcompositions: Vec<(usize, crate::layout::subcompose::Subcomposition)>,
    /// A subcomposition this composer can re-arrange instead of re-composing (`layout::subcompose`).
    /// Taken by the next `subcompose()` call in the same frame and put back by `layout()`'s adoption
    /// pass, so a second layout pass in one frame reuses the first pass's composition.
    subcomposition_cache: Option<Box<Composer>>,
    /// How many skipped subtrees the last `materialize` claimed in place, instead of re-encoding them
    /// into descriptors (`SlotTable::try_claim_skipped_subtree`). Test-visible so the tests can tell
    /// "the fast path ran" from "it bailed" — the two are deliberately indistinguishable in the tree.
    #[cfg(test)]
    pub(crate) skip_claims: usize,
    /// Subtrees whose in-place claim was rejected this frame (see `skip_claims`).
    #[cfg(test)]
    pub(crate) skip_claim_bails: usize,
    /// Slots the last compose found still in the tree but not composed (and not inside a skipped
    /// subtree) — the slots `collect_live_keys` exists to exclude. Test-visible because the number
    /// only exists at that moment: materialize consumes the frame's markers (`desc`, `skip_modifier`),
    /// so counting the same tree after a frame reports slots that were live all along.
    #[cfg(test)]
    pub(crate) live_residue: usize,
    /// 当前选区注册表（SelectionContainer compose 时注入，供事件处理访问）
    pub(crate) selection_registrar: Option<crate::text::selection::SelectionRegistrar>,
    /// Window lifecycle flags are scoped to this Composer, not the thread.
    pub(crate) lifecycle: crate::runtime::lifecycle::LifecycleState,
    /// The winit window this composer belongs to; 0 when unwired (a test, an overlay composer).
    /// `ComposeCtx::frame_clock` hands out this window's clock.
    pub(crate) window_id: u64,
    /// Adaptive window size context owned by this Composer.
    pub(crate) adaptive: crate::layout::adaptive::AdaptiveContext,
    /// State IDs used by this Composer's animation registrations.
    pub(crate) animation_state_ids: HashSet<StateId>,

    #[cfg(test)]
    pub(crate) compose_clean_count: usize,
    #[cfg(test)]
    pub(crate) compose_dirty_count: usize,
    /// Test-only: how many RESTARTABLE GROUPS actually re-entered this pass (dirty OR parameter
    /// change). Unlike `compose_dirty_count`, this counts a parameter-driven Enter of a Clean slot -
    /// the probe a "no recomposition" test needs.
    #[cfg(test)]
    pub(crate) compose_entered_count: usize,
    /// 重组总次数（vsync 研究——单次渲染内多次 compose 的观测）
    pub(crate) compose_count: u64,
    /// Shared-element flights (Phase 2): active flights by id (Tier-0
    /// coordinator lives in components::shared_transition as `impl Composer`).
    pub(crate) shared_flights: HashMap<FlightId, ActiveFlight>,
    pub(crate) next_flight_id: u64,
    /// Last frame's (scope, key) → slot map for switch detection.
    pub(crate) prev_shared_endpoints: HashMap<(u64, String), u64>,
    /// Detached retained source roots (absolute coords, rendered after main tree).
    pub(crate) transition_layer: Vec<usize>,
    /// Per-frame render order for the transition layer: detached sources plus
    /// every elevated (in-tree) flight endpoint, z-sorted back-to-front.
    /// Rebuilt by `rebuild_layer_order` after each flight poll — render and
    /// hit testing both read it, so paint order and hit order can never drift.
    pub(crate) layer_order: Vec<usize>,
    /// In-tree endpoints that fly in this composer's layer (elevated visuals),
    /// recorded by the visual writers themselves. Rebuilt every poll: the
    /// writers are the only place that knows both the arena index and whether
    /// that end is elevated, and a Tier1 peer's flight lives in the MAIN
    /// composer's map, so the peer could not discover it by scanning its own.
    pub(crate) elevated_roots: Vec<usize>,
    /// Non-shared subtrees elevated for the duration of a flight (Compose
    /// `renderInSharedTransitionScopeOverlay`). Recomputed each poll by
    /// `refresh_scope_overlay_roots`; also drives their `LayoutNode` flag so
    /// the tree walk skips them and the layer re-draws them untransformed.
    pub(crate) scope_overlay_roots: Vec<usize>,
    /// True while any node carries a paint disposition other than `InTree` (a placeholder, or chrome in
    /// the layer). `refresh_paint_dispositions` can then skip its whole-arena walk on the frames where
    /// nothing is shared and the previous frame left nothing to clear — and it MUST run once more after
    /// the last non-`InTree` assignment, which is what this flag guarantees.
    pub(crate) paint_dirty: bool,
    /// Last-frame absolute bounds per live marked endpoint (Phase 3
    /// same-screen size-morph detection). Keyed by endpoint identity
    /// (scope, key) — never by slot: slots are positional identities a new
    /// key can reuse, which would seed a spurious morph from stale rects.
    pub(crate) shared_last_bounds: HashMap<(u64, String), SharedBounds>,
    /// Stable per-Composer identity for cross-composer flight matching
    /// (Phase 4 Tier1: main tree ↔ overlays share (scope, key) but never slots).
    pub(crate) composer_id: u64,
    /// Retired-detached sources awaiting cross-composer counterparts (Phase 4
    /// Tier1). Drained by cross-poll same frame — freed when unmatched.
    pub(crate) pending_cross: Vec<PendingSource>,
    /// Fresh (scope, key) → slot appearances THIS frame (Phase 4 Tier1 cross
    /// matching reads these). Overwritten every retain — peer prev maps absorb
    /// appearances before cross-poll runs, so this is the only freshness source.
    pub(crate) fresh_shared: Vec<((u64, String), u64)>,
    /// This composer's canvas-frame origin in window-logical units (Phase 4):
    /// main tree renders untranslated (0,0); overlays render translated by
    /// their screen_pos. Flight bounds are canonicalized to window coords.
    pub(crate) screen_origin: (f32, f32),
}

impl Composer {
/// 选区注册表（由 SelectionContainer 在 compose 时注入，供事件处理访问）

    /// This composer's process-unique identity; see the `uid` field.
    pub(crate) fn uid(&self) -> u64 {
        self.uid
    }

    pub(crate) fn take_layout_override_fresh(&mut self) -> bool {
        std::mem::take(&mut self.layout_override_fresh)
    }

    pub fn new() -> Self {
        let pending_states = crate::runtime::state::ComposerSubscription::new();
        Self {
            uid: NEXT_COMPOSER_UID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            saveable: None,
            slot_table: SlotTable::new(),
            current_group_key: 0,
            compose_transaction: None,
            layout_override_fresh: false,
            path_counters: crate::layout::node::SlotKeyMap::default(),
            remember_path_counters: crate::layout::node::SlotKeyMap::default(),
            scope_source_stack: Vec::new(),
            key_override_stack: Vec::new(),
            movable_saved_scopes: Vec::new(),
            movable_saved_stmts: Vec::new(),
            movable_saved_keys: Vec::new(),
            movable_saved_groups: Vec::new(),
            movable_saved_active: 0,
            movable_saved_skip_depth: 0,
            movable_saved_group_key: 0,
            movable_saved_path_counters: crate::layout::node::SlotKeyMap::default(),
            movable_saved_remember_counters: crate::layout::node::SlotKeyMap::default(),
            movable_saved_params: Vec::new(),
            movable_saved_pending_next: 0,
            pending_recomposition: VecDeque::new(),
            needs_recomposition: true,
            arena: crate::layout::node::NodeArena::new(),
            node_stack: Vec::new(),
            group_skip_stack: Vec::new(),
            overlays: Vec::new(),
            post_compose: Vec::new(),
            overlay_active: HashMap::new(),
            slot_deps: HashMap::new(),
            compose_slot_reads: crate::layout::node::SlotKeyMap::default(),
            signal_handles: HashMap::new(),
            layout_signal_handles: HashMap::new(),
            entered_compose_keys: crate::layout::node::SlotKeySet::default(),
            layout_slot_reads: crate::layout::node::SlotKeyMap::default(),
            layout_deps: HashMap::new(),
            layout_dirty_keys: HashSet::new(),
            removed_slot_keys: HashSet::new(),
            pending_states,
            pending_params: Vec::new(),
            pending_next: 0,
            prev_node_by_key: crate::layout::node::SlotKeyMap::default(),
            reused_nodes: crate::layout::node::NodeMarks::default(),
            subcompositions: Vec::new(),
            subcomposition_cache: None,
            #[cfg(test)]
            skip_claims: 0,
            #[cfg(test)]
            skip_claim_bails: 0,
            #[cfg(test)]
            live_residue: 0,
            selection_registrar: None,
            lifecycle: crate::runtime::lifecycle::LifecycleState::default(),
            window_id: 0,
            adaptive: crate::layout::adaptive::AdaptiveContext::new(),
            animation_state_ids: HashSet::new(),
            shared_flights: HashMap::new(),
            next_flight_id: 1,
            prev_shared_endpoints: HashMap::new(),
            transition_layer: Vec::new(),
            layer_order: Vec::new(),
            elevated_roots: Vec::new(),
            scope_overlay_roots: Vec::new(),
            paint_dirty: false,
            shared_last_bounds: HashMap::new(),
            composer_id: NEXT_COMPOSER_ID.fetch_add(1, Ordering::Relaxed),
            pending_cross: Vec::new(),
            fresh_shared: Vec::new(),
            screen_origin: (0.0, 0.0),
            #[cfg(test)]
            compose_clean_count: 0,
            #[cfg(test)]
            compose_dirty_count: 0,
            #[cfg(test)]
            compose_entered_count: 0,
            compose_count: 0,
        }
    }

    /// 获取当前正在构建的节点 ID（node_stack 栈顶）
    pub fn current_node_id(&self) -> Option<u64> {
        self.node_stack.last().map(|&idx| self.arena.nodes[idx].id)
    }

    /// Bind this composer to the window whose frames should drive `ComposeCtx::frame_clock`.
    pub(crate) fn set_window_id(&mut self, window: u64) {
        self.window_id = window;
    }

    pub fn window_id(&self) -> u64 {
        self.window_id
    }

    pub(crate) fn pending_window_close_id(&self) -> Option<u64> {
        self.lifecycle.pending_close_id()
    }

    pub(crate) fn reset_pending_window_remove(&self) {
        self.lifecycle.reset_pending_remove();
    }

    pub(crate) fn set_adaptive_window_size(&self, width: f32, height: f32) {
        self.adaptive.set_size(width, height);
    }

    pub(crate) fn focus_window(&self, window_id: u64) -> crate::modifier::FocusWindowGuard {
        crate::modifier::focus_window(window_id)
    }

    pub(crate) fn take_focus_requests(&self) -> Vec<u64> {
        crate::modifier::take_focus_requests()
    }

    /// 分配下一个 group key。
    ///
    /// 基于 slot 路径编码：结构稳定——Enter/Skip 的执行顺序不影响 key，
    /// 保证同一组合位置跨重组得到相同 slot（否则 slot 树 truncate 重建，
    /// 导致 remember 的 State 全部丢失重建）。
    /// 能否获得稳定 key 的 base（新 key 系统核心判定）：
    /// - 显式 ctx.key(id, f) 作用域内 → 稳定（用户保证唯一）
    /// - STMT_STACK 有宏注入的语句（`#[composable]`/keyed_stmt! 展开）→ 稳定
    ///   （base = fnv(scope_src, 语句id, 迭代seq)——编译期固定）
    /// - 都不是 → None（调用方 panic 兜底——fail-fast，不静默降级）
    /// Namespace compiler-derived and explicit identities by their movable placement.
    fn namespace_key_base(&self, base: u64) -> u64 {
        match self.slot_table.movable_composing {
            Some(id) => mix_key(base, mix_key(0x6d6f_7661_626c_6500, id)),
            None => base,
        }
    }

    pub(crate) fn try_stable_base(&self) -> Option<u64> {
        if let Some(&k) = self.key_override_stack.last() {
            // ctx.key(id) 语义：显式 id 替代位置——但**同一 id 跨调用点**（多
            // 字段同 role：16 个 field 的 label 都是 TextFieldSlotRole::Label）
            // 会碰撞——混合调用链隔离实例（同 id 不同调用点 → 不同 base）；
            // 链空（非宏顶层）时退化纯 id（兼容旧行为）。
            // ⚠ 只混合 scope_src+sid（不含迭代 seq）：ctx.key 是列表/重排的
            // 显式兜底（Compose 语义）——若混入 seq，重排后同 id 位置变 →
            // base 变 → key 漂移 → 兜底失效（P1-1，探针证实）。多实例区分
            // 由 per-base counter（path_counters/remember_path_counters）完成。
            let chain = self.chain_hash_no_seq().unwrap_or(0);
            let mut h: u64 = 0xcbf29ce484222325;
            h ^= k; h = h.wrapping_mul(0x100000001b3);
            h ^= chain; h = h.wrapping_mul(0x100000001b3);
            return Some(self.namespace_key_base(h));
        }
        self.chain_hash().map(|base| self.namespace_key_base(base)).or_else(|| {
            // Raw payload closures have no compiler statement on entry. The placement itself is a
            // stable key source; repeated children are distinguished by its local counters.
            self.slot_table.movable_composing.map(|_| self.namespace_key_base(0))
        })
    }

    /// 调用链哈希（不含迭代 seq）：fnv(scope_src, STMT栈顶语句id)——编译期
    /// 固定的组合位置身份。ctx.key(id) 混合用——同 id 跨位置（列表重排）base
    /// 不变（兜底稳定），不同调用点（16 字段）靠 scope_src/sid 隔离。
    fn chain_hash_no_seq(&self) -> Option<u64> {
        STMT_STACK.with(|s| {
            let s = s.borrow();
            s.last().map(|&(sid, _seq)| {
                let scope_src = self.scope_source_stack.last().and_then(|s| *s).unwrap_or(0);
                let mut h: u64 = 0xcbf29ce484222325;
                h ^= scope_src; h = h.wrapping_mul(0x100000001b3);
                h ^= sid as u64; h = h.wrapping_mul(0x100000001b3);
                h
            })
        })
    }

    /// 调用链哈希：fnv(scope_src, STMT栈顶语句id, 迭代seq)——编译期固定
    /// 的组合位置（`#[composable]`/keyed_stmt! 注入的语句）
    fn chain_hash(&self) -> Option<u64> {
        STMT_STACK.with(|s| {
            let s = s.borrow();
            s.last().map(|&(sid, seq)| {
                let scope_src = self.scope_source_stack.last().and_then(|s| *s).unwrap_or(0);
                let mut h: u64 = 0xcbf29ce484222325;
                h ^= scope_src; h = h.wrapping_mul(0x100000001b3);
                h ^= sid as u64; h = h.wrapping_mul(0x100000001b3);
                h ^= (seq as u64).wrapping_mul(0x9E3779B97F4A7C15);
                h
            })
        })
    }

    /// 没有稳定 key 源时的 panic（fail-fast——不静默降级为路径哈希）
    pub(crate) fn panic_no_stable_key(&self, api: &str) -> ! {
        panic!(
            "无法获得稳定 key（{}）：调用点不在 `#[composable]`/keyed_stmt! 注入内，\
             也无 ctx.key() 包裹——key 会在结构变化时漂移。修复：①将调用点放入 \
             `#[composable]` 函数内 ②用 ctx.key() 包裹 ③content 闭包参数名与 \
             #[composable(x)] 指定的标识符一致",
            api
        )
    }

    pub fn next_group_key(&mut self) -> u64 {
        // key 基优先级：显式 ctx.key() > `#[composable]` 语句 id（源码位置）。
        // 语句 id 由宏注入（编译期按源码结构固定编号）——结构变化（前面插入/移除兄弟
        // 节点）不影响语句 id → key 不漂移 → remember/复用稳定（对标 Compose 编译器
        // 的调用点 key）。无稳定源 → panic（fail-fast）。
        let base = match self.try_stable_base() {
            Some(b) => b,
            None if cfg!(test) => {
                // 测试路径：无语句级 key 时退化为路径哈希（测试自控结构——漂移由
                // 测试自己负责；生产代码禁止——见下方 panic）
                let path = self.slot_table.current_path().to_vec();
                let mut h: u64 = 0xcbf29ce484222325;
                for &idx in &path {
                    h ^= idx as u64;
                    h = h.wrapping_mul(0x100000001b3);
                }
                h
            }
            None => self.panic_no_stable_key("next_key"),
        };
        // 每路径独立 counter：同 key 基第 N 次调用跨帧恒定（Skip 的 content 不执行
        // 不平移——节点复用错位 + 常量折叠冻结的防护）
        let counter = self.path_counters.entry(base).or_insert(1);
        let c = *counter;
        *counter += 1;
        // key = fnv(base, 序号)——全 64 位混合，不丢身份熵（拼接方案把身份
        // 截到 32 位，2^32 碰撞空间——dup-key 根因）
        crate::runtime::composer::mix_key(base, c as u64)
    }

    /// 开始一个组合 scope（无 LayoutNode 的作用域节点——组合代码重跑的失效单位）。
    /// 返回 scope key；`State::get()` 在 scope 内（组件外）注册依赖到 scope。
    ///
    /// ⚠ 手动调用（无源码哈希）：scope_source_stack push None，与 start_scope_keyed
    /// 的 Some 区分——end_scope 严格配对，不破坏外层宏注入的 source。
    /// ⚠ release 下宏外调用会触发稳定 key panic（next_group_key 快速失败）——
    /// 生产代码应使用 `#[composable]`/`app_root!` 注入的 start_scope_keyed。
    pub fn start_scope(&mut self) -> u64 {
        self.scope_source_stack.push(None);
        let key = self.next_group_key();
        self.slot_table.start_scope(key);
        self.entered_compose_keys.insert(key);
        GROUP_STACK.with(|s| s.borrow_mut().push(key));
        key
    }

    /// 结束组合 scope
    pub fn end_scope(&mut self) {
        self.slot_table.end_scope();
        // 防御性配对：非空才 pop（start_scope/end_scope 不配对时避免 panic/污染其他 scope）
        GROUP_STACK.with(|s| {
            let mut s = s.borrow_mut();
            if !s.is_empty() { s.pop(); }
        });
        if !self.scope_source_stack.is_empty() { self.scope_source_stack.pop(); }
    }

    /// 物化：组合树（Slot desc）→ 布局树（arena LayoutNode）——完整分离的核心。
    /// 由 compose 末尾调用（layout 只测量）。实现拆到 core/materialize.rs（SRP）。
    pub fn materialize(&mut self) {
        crate::runtime::materialize::materialize(self);
    }

    /// Hands this frame's declared parameters to the slot being started, and takes the slot's previous
    /// parameters back as the buffer for the next declarations.
    ///
    /// A no-op when nothing was declared (`pending_next == 0`): the slot keeps its parameters, which is
    /// what stops a replayed stub from clearing them — a Skipped parent's children must not be forced to
    /// Enter just because their params were not re-declared.
    ///
    /// The exchange is the whole point. Before it, `changed` pushed a fresh `Box` per parameter per
    /// frame and `set_current_params` dropped the slot's previous vector — so a 800-row list allocated
    /// 800 boxes a frame (~503 µs of a cold frame's content closure, `docs/benchmarks.md`). Now the
    /// previous vector comes back and the next `changed` overwrites its boxes in place, which in a list
    /// means the same few allocations circulate down the rows.
    fn commit_pending_params(&mut self) {
        if self.pending_next == 0 {
            return;
        }
        // Hand over exactly this frame's declarations; any tail is last frame's leftovers (a group that
        // declared more parameters than this one does now). Dropping them here is what keeps the count
        // honest for a group whose parameter list shrank.
        self.pending_params.truncate(self.pending_next);
        let incoming = std::mem::take(&mut self.pending_params);
        self.pending_params = self.slot_table.replace_current_params(incoming);
        self.pending_next = 0;
    }

    /// 在组合树中开始一个节点（由组件的 build 方法调用）
    pub fn start_node(&mut self, key: u64, modifier: Modifier, policy: Option<Box<dyn MeasurePolicy>>, on_remove: Option<Box<dyn FnOnce() + Send>>) {
        self.current_group_key = key as u32;
        self.entered_compose_keys.insert(key);
        let slot_status = self.slot_table.start_slot(key);
        // 普通节点：复用 scope slot 时重置为普通（同路径类型切换场景）
        self.slot_table.set_current_scope(false);
        // 写入 `ComposeCtx::changed` 暂存的参数（供下帧比较）——
        // 仅当本帧有 changed 声明；否则保留上帧 params：
        // replay 的 stub start_node（pending 空）不清空子 slot params，
        // 避免父 Skip 后子组件参数未变也被强制 Enter
        self.commit_pending_params();
        #[cfg(test)] { match slot_status { SlotStatus::Clean => self.compose_clean_count += 1, _ => self.compose_dirty_count += 1, } }

        // 组合产物写入 Slot（物化阶段消费——完整分离：arena 建节点移出组合阶段）
        // ⚠ direction 必须在此捕获（组合期 provides 作用域内）——物化期
        // 主题的 provider 已退出作用域读不到（RTL 全局切换失效根因）
        let direction = modifier.get_layout_direction()
            .unwrap_or(crate::layout::direction::current());
        // A Clean slot still has to re-measure when this node's own modifier changed: the modifier is
        // where a container's or leaf's layout inputs live (`max_width`, `size`, `padding`), and a value
        // computed from a state read in an ANCESTOR's scope changes them without dirtying this slot.
        // See `node_modifier_changed`.
        let dirty = slot_status != SlotStatus::Clean || self.node_modifier_changed(key, &modifier);
        self.slot_table.set_current_desc(Some(NodeDesc {
            key,
            modifier,
            policy,
            on_remove,
            dirty, // 重测标记（slot.dirty 已消费）
            registrar: None,
            focus_color: None,
            composing_color: None,
            cursor_index: None,
            cursor_visible: None,
            cursor_callback: None,
            display_focused: None,
            ime_callback: None,
            composing_range: None,
            direction,
        }));
        // 统一依赖栈：节点 push（组件 build 期间 State 读取注册到最内层 Group——
        // 组件内读取失效目标 = 本节点（对标 Compose 最内层 Group 语义））
        GROUP_STACK.with(|s| s.borrow_mut().push(key));
    }

    /// 结束当前节点：组合侧出栈（物化建树由 materialize 统一处理）
    pub fn end_node(&mut self) {
        self.slot_table.end_slot();
        GROUP_STACK.with(|s| { s.borrow_mut().pop(); }); // 与 start_node 的 push 配对
    }

    /// 开始可重启分组（内部调用 start_node + 返回 skip/enter 状态）
    fn start_restartable_group(
        &mut self,
        key: u64,
        modifier: Modifier,
        policy: Option<Box<dyn MeasurePolicy>>,
        on_remove: Option<Box<dyn FnOnce() + Send>>,
    ) -> GroupStatus {
        self.current_group_key = key as u32;
        // 容器 build 期间维护（scope 栈由 start_restartable_group 管理）
        let slot_status = self.slot_table.start_slot(key);
        self.slot_table.set_current_scope(false);
        // 容器组件 = scope（对标 Compose RestartGroup）：push 组件 scope——
        // 组件内/ content 闭包内读取注册到本组件（最内层 scope）
        GROUP_STACK.with(|s| s.borrow_mut().push(key));
        #[cfg(test)] { match slot_status { SlotStatus::Clean => self.compose_clean_count += 1, _ => self.compose_dirty_count += 1, } }

        // Clean slot：从缓存恢复（阶段5：加参数相等条件——slot clean 且参数
        // 全相等才 Skip；参数变化（changed 比较）时即使 slot clean 也 Enter）
        let is_skip = if slot_status == SlotStatus::Clean {
            // 与上帧 slot.params 比较（pending_params 前 pending_next 项 = 本帧 changed 暂存；
            // slot.params 此刻仍是上帧的——本帧写入在其后。只比前 pending_next 项：那个 vector 现在
            // 承载着上一帧的（可能更长/更短的）参数表，是复用缓冲而不是本帧声明）
            let params_unchanged = params_equal(
                &self.pending_params[..self.pending_next],
                &self.slot_table.current_slot().params,
            ) && self.container_modifier_unchanged(key, &modifier);
            #[cfg(debug_assertions)] {
                if std::env::var("WINIA_SKIP_TRACE").is_ok() {
                    eprintln!("[skip] key={} clean={} params_u={} prev={} pending_len={}",
                        key >> 32, slot_status == SlotStatus::Clean, params_unchanged,
                        self.prev_node_by_key.contains_key(&key), self.pending_next);
                }
            }
            // Nothing else to ask: `container_modifier_unchanged` already failed for a key that is not
            // in the reuse index, and a key with no node is one materialize could not restore — the
            // same condition the old cache map was consulted for here.
            params_unchanged
        } else {
            false
        };
        // Enter 重新执行 content；Skip 保留上帧子树的读取集合。
        if !is_skip {
            self.entered_compose_keys.insert(key);
            #[cfg(test)] { self.compose_entered_count += 1; }
        }
        // 写入本帧参数（在 is_skip 比较之后——比较用上帧 slot.params）
        // 仅当 pending 非空（有 changed 声明）；空则保留上帧 params（replay stub 场景）
        self.commit_pending_params();

        // 组合产物写入 Slot：Enter 写完整描述（物化消费）；Skip 写 None——
        // content 不执行（无新描述），物化时按 key 恢复缓存节点（skip 标记）
        if is_skip {
            // 方向先算（modifier 随后 move 进 set_skip_modifier）
            let direction = modifier.get_layout_direction()
                .unwrap_or(crate::layout::direction::current());
            self.slot_table.set_current_desc(None);
            // 保留本帧组合产物 modifier（父层重跑传入的新 offset/背景——物化应用）
            self.slot_table.set_skip_modifier(modifier);
            // 保留 policy（外层传入——Skip 时 content 未执行但 policy 可用；
            // 物化恢复失败降级时避免 policy 缺失测量 0 尺寸）
            self.slot_table.set_skip_policy(policy);
            // Skip 容器方向也须组合期捕获（同 Enter——物化期读不到 theme）
            self.slot_table.set_skip_direction(direction);
        } else {
            // Enter：组合期捕获方向（provides 作用域内）——先算再 move
            let direction = modifier.get_layout_direction()
                .unwrap_or(crate::layout::direction::current());
        self.slot_table.set_current_desc(Some(NodeDesc {

                key,
                modifier,
                policy,
                on_remove,
                dirty: true, // Enter 即重测（content 重跑——参数/内容可能变；Skip 恢复不受影响）
                registrar: None,
                focus_color: None,
                composing_color: None,
            cursor_index: None,
            cursor_visible: None,
            cursor_callback: None,
            display_focused: None,
            ime_callback: None,
                composing_range: None,
                direction,
            }));
        }
        self.group_skip_stack.push(is_skip);

        if is_skip {
            GroupStatus::Skip
        } else {
            GroupStatus::Enter
        }
    }

    /// 结束可重启分组：
    /// - Skip 模式：重放子 slot 结构并创建 stub LayoutNode
    /// - Enter 模式：等同于 end_node()
    fn end_restartable_group(&mut self) {
        let was_skip = self.group_skip_stack.pop().unwrap_or(false);
        // Enter（content 重跑）：清理未访问子槽——内容结构变化后残留的旧槽
        // （如 AnimatedContent B(3 文本) → A(2 文本) 的第 3 槽）若保留，下帧
        // Skip 收集时结构签名（desc children vs 缓存 children）不等 → 物化
        // 降级 0x0 → 子树塌缩。Skip 槽保留（content 未执行——结构需保留供恢复）。
        //
        // The same pruning applies inside a movable placement. Moving a surviving subtree preserves
        // its state; omitting a conditional/list child removes it, and reinsertion initializes anew.
        if !was_skip {
            // Pruning is a DROP, so it has to collect lifecycle callbacks the same way the other
            // removal sites do. `retain` alone silently discarded them: a `LaunchedEffect` inside a
            // branch that stopped composing kept its task running (caught by
            // `animation::test_infinite_transition_auto_dispose`).
            let slot = self.slot_table.current_slot();
            let mut orphaned = Vec::new();
            let mut kept = Vec::with_capacity(slot.children.len());
            for child in slot.children.drain(..) {
                if child.visited {
                    kept.push(child);
                } else {
                    SlotTable::collect_forgets(child, &mut orphaned);
                }
            }
            slot.children = kept;
            for f in orphaned { f(); }
        }
        // Skip：content 未执行——slot 树保留（上帧 children 结构）——物化时
        // 整棵子树按 key 从 prev_node_by_key 恢复（stub 机制已由物化替代）
        // GROUP_STACK pop 由 end_node 统一处理（与 start_restartable_group 的
        // push 配对——此前此处额外 pop 导致容器组件两次 pop 一次 push →
        // 栈错乱 → 后续依赖注册到错误 Group → State 变化不标记容器 dirty）
        self.end_node();
    }

    /// Whether this container's modifier matches the one the frame is running against — the second half
    /// of the Skip decision (`params_equal` is the first).
    ///
    /// The comparison target is the ARENA NODE's modifier, which during compose still holds what
    /// materialize applied last frame. That is the same value the slot used to keep a copy of
    /// (`Slot::prev_modifier`), and reading it instead removes a `Modifier` clone per Entering container
    /// per frame — the largest single item the content-closure probe found (~297 µs on a cold frame,
    /// ~93 ns per container, and a `Modifier` owns a `Vec` plus any strings its elements carry;
    /// `docs/benchmarks.md`).
    ///
    /// It is the same value, not an approximation, which is why this is preferred over storing a digest:
    /// a digest can only ever answer "unchanged" wrongly, and "unchanged" here means the container Skips
    /// with a stale modifier — the one failure direction this code must not have.
    ///
    /// A missing node means "no basis for comparison" and the container Enters, which is what a missing
    /// `prev_modifier` used to do. `compose` refills the index when it is empty (a second compose in the
    /// same frame drains it), so the node is found in every case where a Skip was possible before.
    fn container_modifier_unchanged(&self, key: u64, modifier: &Modifier) -> bool {
        match self.prev_node_by_key.get(&key) {
            Some(&idx) => modifier.param_eq(&self.arena.nodes[idx].modifier),
            None => false,
        }
    }

    /// Whether this frame's `modifier` differs from the one the node at `key` was last materialized with
    /// — i.e. the node's own LAYOUT INPUTS changed, so its measurement cannot be reused.
    ///
    /// A container or leaf created by `start_node` has no parameter list of its own; its modifier IS its
    /// parameter list. A value computed from a state read in an ANCESTOR's scope (`max_width(cap.get())`
    /// inside a parent's content closure) dirties that ancestor's slot and leaves this node's slot Clean,
    /// so `slot_status != SlotStatus::Clean` alone answers "nothing changed here" while this node's own
    /// constraints did change — and a folded measurement then keeps the old size and, for a subcomposing
    /// node, the content it composed with the old constraints (measured: the `bwc` UI fixture's click
    /// stopped reaching its `BoxWithConstraints` content). Compose compares the modifier chain for the
    /// same reason.
    ///
    /// `false` when there is no node to compare against: that is not "changed", it is "no basis" —
    /// materialize rebuilds a node it cannot find and starts it dirty on its own.
    fn node_modifier_changed(&self, key: u64, modifier: &Modifier) -> bool {
        match self.prev_node_by_key.get(&key) {
            Some(&idx) => !modifier.param_eq(&self.arena.nodes[idx].modifier),
            None => false,
        }
    }

    /// Rebuild reverse compose dependencies from the forward per-slot read graph.
    /// Keeping one canonical source prevents stale reverse edges after slot removal.
    fn rebuild_compose_reverse_deps(&mut self) {
        self.slot_deps.clear();
        for (&slot_key, state_ids) in &self.compose_slot_reads {
            for &state_id in state_ids {
                self.slot_deps.entry(state_id).or_default().insert(slot_key);
            }
        }
    }

    /// Rebuild reverse layout dependencies from the forward per-slot read graph.
    fn rebuild_layout_reverse_deps(&mut self) {
        self.layout_deps.clear();
        for (&slot_key, state_ids) in &self.layout_slot_reads {
            for &state_id in state_ids {
                self.layout_deps.entry(state_id).or_default().insert(slot_key);
            }
        }
    }

    #[cfg(debug_assertions)]
    fn debug_assert_dependency_graphs(&self) {
        let mut expected_compose = HashMap::<StateId, HashSet<u64>>::new();
        for (&slot_key, state_ids) in &self.compose_slot_reads {
            for &state_id in state_ids {
                expected_compose.entry(state_id).or_default().insert(slot_key);
            }
        }
        debug_assert_eq!(self.slot_deps, expected_compose, "compose dependency reverse graph drifted");

        let mut expected_layout = HashMap::<StateId, HashSet<u64>>::new();
        for (&slot_key, state_ids) in &self.layout_slot_reads {
            for &state_id in state_ids {
                expected_layout.entry(state_id).or_default().insert(slot_key);
            }
        }
        debug_assert_eq!(self.layout_deps, expected_layout, "layout dependency reverse graph drifted");
    }

    #[cfg(not(debug_assertions))]
    #[inline]
    fn debug_assert_dependency_graphs(&self) {}

    fn reconcile_compose_deps(
        &mut self,
        recorded: Vec<(Arc<StateSignal>, u64)>,
        live_keys: &crate::layout::node::SlotKeySet,
    ) {
        let mut reads_by_slot: crate::layout::node::SlotKeyMap<HashSet<StateId>> = crate::layout::node::SlotKeyMap::default();
        let mut current_signals: HashMap<StateId, Arc<StateSignal>> = HashMap::new();
        // Reads recorded against a slot that is not live: dropped from the graph, but the signal was
        // already subscribed to (the read happened), so the subscription is dirty even when the graph
        // is not — see `changed` below.
        let mut dropped_reads = 0usize;
        for (signal, slot_key) in recorded {
            current_signals.entry(signal.id()).or_insert_with(|| signal.clone());
            if live_keys.contains(&slot_key) {
                reads_by_slot.entry(slot_key).or_default().insert(signal.id());
            } else {
                dropped_reads += 1;
            }
        }

        // A skipped subtree keeps its previous read set; an Entered slot gets
        // the exact set observed in this frame, including an empty set.
        //
        // `changed` records whether that actually moved anything, because the reverse graph below is a
        // pure function of the forward one and rebuilding it is the expensive half: a whole-tree walk
        // that clears and re-creates a set per state. It ran every frame — including the frames where
        // every read set is exactly what it already was, which is the common case (measured at 800
        // rows: 110 µs of an idle frame's compose, and the same figure on the frame where one row's
        // state moved, since the container re-records the same 800 reads).
        let mut changed = false;
        let before = self.compose_slot_reads.len();
        self.compose_slot_reads.retain(|key, _| live_keys.contains(key));
        changed |= self.compose_slot_reads.len() != before;
        // TAKEN rather than cloned: this is the last reader of the set in the frame (the next compose
        // clears it), and on a cold frame it holds every entering key — 4000 of them. A panic after
        // this point restores the set from the frame's runtime snapshot (`capture_compose_runtime`),
        // so the take is not observable to a rollback either.
        let entered = std::mem::take(&mut self.entered_compose_keys);
        for key in entered {
            let new_set = match reads_by_slot.remove(&key) {
                Some(reads) if !reads.is_empty() => Some(reads),
                _ => None,
            };
            let same = match (self.compose_slot_reads.get(&key), new_set.as_ref()) {
                (None, None) => true,
                (Some(old), Some(new)) => old == new,
                _ => false,
            };
            changed |= !same;
            match new_set {
                Some(reads) => {
                    self.compose_slot_reads.insert(key, reads);
                }
                None => {
                    self.compose_slot_reads.remove(&key);
                }
            }
        }
        // Be tolerant of a read recorded by a node whose entry marker was not
        // reached (for example, a modifier callback during materialization).
        if !reads_by_slot.is_empty() {
            changed = true;
            for (key, reads) in reads_by_slot {
                self.compose_slot_reads.entry(key).or_default().extend(reads);
            }
        }
        // A read against a dead slot means the subscription may hold a signal the graph does not, so
        // the cleanup has to run even though the graph is unchanged.
        changed |= dropped_reads > 0;

        if changed {
            self.rebuild_compose_reverse_deps();
            self.cleanup_signal_subscriptions();
        }
        self.signal_handles.extend(current_signals);
        self.debug_assert_dependency_graphs();
    }

    fn cleanup_signal_subscriptions(&mut self) {
        let mut live_ids = HashSet::new();
        for state_ids in self.compose_slot_reads.values() {
            live_ids.extend(state_ids.iter().copied());
        }
        live_ids.extend(self.layout_deps.keys().copied());

        let mut removed = HashMap::<StateId, Arc<StateSignal>>::new();
        let mut compose_handles = std::mem::take(&mut self.signal_handles);
        compose_handles.retain(|id, signal| {
            if live_ids.contains(id) {
                true
            } else {
                removed.insert(*id, signal.clone());
                false
            }
        });
        self.signal_handles = compose_handles;

        let mut layout_handles = std::mem::take(&mut self.layout_signal_handles);
        layout_handles.retain(|id, signal| {
            if live_ids.contains(id) {
                true
            } else {
                removed.entry(*id).or_insert_with(|| signal.clone());
                false
            }
        });
        self.layout_signal_handles = layout_handles;

        for signal in removed.values() {
            signal.unsubscribe(self.pending_states.id());
        }
        self.pending_states.retain_signals(&live_ids);
    }

    fn capture_compose_runtime(&self) -> ComposeRuntimeSnapshot {
        ComposeRuntimeSnapshot {
            slot_table: self.slot_table.runtime_snapshot(),
            current_group_key: self.current_group_key,
            path_counters: self.path_counters.clone(),
            remember_path_counters: self.remember_path_counters.clone(),
            scope_source_stack: self.scope_source_stack.clone(),
            key_override_stack: self.key_override_stack.clone(),
            pending_recomposition: self.pending_recomposition.clone(),
            needs_recomposition: self.needs_recomposition,
            node_stack: self.node_stack.clone(),
            group_skip_stack: self.group_skip_stack.clone(),
            overlay_active: self.overlay_active.clone(),
            entered_compose_keys: self.entered_compose_keys.clone(),
            reused_nodes: self.reused_nodes.clone(),
        }
    }

    fn rollback_compose_runtime(&mut self) {
        let Some(snapshot) = self.compose_transaction.take() else { return };
        self.slot_table.restore_runtime(snapshot.slot_table);
        self.current_group_key = snapshot.current_group_key;
        self.path_counters = snapshot.path_counters;
        self.remember_path_counters = snapshot.remember_path_counters;
        self.scope_source_stack = snapshot.scope_source_stack;
        self.key_override_stack = snapshot.key_override_stack;
        self.pending_recomposition = snapshot.pending_recomposition;
        self.needs_recomposition = snapshot.needs_recomposition;
        self.node_stack = snapshot.node_stack;
        self.group_skip_stack = snapshot.group_skip_stack;
        self.overlay_active = snapshot.overlay_active;
        self.entered_compose_keys = snapshot.entered_compose_keys;
        self.reused_nodes = snapshot.reused_nodes;
    }

    /// 执行组合：运行 content 闭包，构建/更新组合树和布局树。
    pub fn compose(&mut self, content: impl FnOnce(&mut ComposeCtx)) {
        // The window's saveable registry is provided for EVERY pass, not just the first: a slot that
        // is re-entered has to be able to register again.
        match self.saveable.clone() {
            Some(registry) => {
                crate::runtime::saveable::with_registry_of(&registry, || self.compose_inner(content))
            }
            None => self.compose_inner(content),
        }
    }

    /// The registry this composer's window saves into, if it has one.
    pub(crate) fn saveable_registry(
        &self,
    ) -> Option<std::sync::Arc<crate::runtime::saveable::SaveableStateRegistry>> {
        self.saveable.clone()
    }

    /// Attach the registry this composer's window saves into and restores from.
    pub(crate) fn set_saveable_registry(
        &mut self,
        registry: std::sync::Arc<crate::runtime::saveable::SaveableStateRegistry>,
    ) {
        self.saveable = Some(registry);
    }

    fn compose_inner(&mut self, content: impl FnOnce(&mut ComposeCtx)) {
        // Tag this pass for the torn-read instrument: a read that observes another thread's write
        // from after this point is a pass split across two worlds.
        let _pass = crate::runtime::state::begin_pass();
        // Isolate shared TLS so a nested Composer can compose without replacing
        // the caller's active slot/group/statement context.
        let _runtime_frame = begin_runtime_frame();
        let mut compose_runtime_transaction = ComposeRuntimeTransaction::new(self);
        let _adaptive_context = crate::layout::adaptive::enter_context(self.adaptive.clone());
        let mut dependency_transaction = ComposeDependencyTransaction::new(self);
        #[cfg(test)] { self.compose_clean_count = 0; self.compose_dirty_count = 0; self.compose_entered_count = 0; }
        self.compose_count += 1;
        // Movable content is composed once per composition, so the table needs to know which
        // composition this is.
        self.slot_table.frame_no = self.compose_count;
        self.slot_table.reset();
        self.overlay_active.clear(); // 每帧组合期重记录（Skip 帧不记录）
        // A pass that panicked before its drain leaves its callbacks behind; they belong to a
        // composition that never committed, so they must not run in this one.
        self.post_compose.clear();
        self.current_group_key = 0;
        self.lifecycle.reset_for_compose();
        self.path_counters.clear();
        self.remember_path_counters.clear();
        STMT_STACK.with(|s| s.borrow_mut().clear());
        // The runtime frame starts empty; these clears also self-heal any state
        // left by an older caller that entered before frame isolation was added.
        GROUP_STACK.with(|s| s.borrow_mut().clear());
        ACTIVE_SLOT_KEY.with(|c| c.set(0));
        self.scope_source_stack.clear();
        self.key_override_stack.clear();
        // 注意：不在 compose 开头清 arena.root——materialize 管理 root
        // （开头清 + 末尾设）。此处若清，同帧第二次 compose 的 materialize
        // 守卫（prev 空 + root 有）失效 → 空 prev 重建 → 树塌缩。
        // Reset only this Composer's Window lifecycle state before old node
        // cleanup; its on_remove callbacks may set a pending close request.
        // 阶段D：保留上帧树（prev_node_by_key 由上帧 layout 构建）——
        // start_node 按 slot_key 复用节点槽位；本帧未复用的旧节点在
        // compose 末尾统一 free（见下方 drain）
        self.node_stack.clear();
        self.group_skip_stack.clear();
        // The parameter buffer is NOT cleared: it arrives holding a slot's previous parameters, which is
        // exactly the storage the next `changed` wants to overwrite in place. Only the write cursor
        // resets (`commit_pending_params` is what hands the buffer over, and it counts from the cursor).
        self.pending_next = 0;
        self.entered_compose_keys.clear();
        // Slot key 0 is the fallback target for top-level State::get().
        self.entered_compose_keys.insert(0);

        // Consume only invalidations that currently have compose dependencies.
        // Layout-only IDs stay queued for layout() to consume.
        let compose_ids: HashSet<StateId> = self.slot_deps.keys().copied().collect();
        let mut pending_batch = PendingBatchGuard::new(
            self.pending_states.clone(),
            self.pending_states.drain_matching(|id| compose_ids.contains(&id)),
        );
        #[cfg(debug_assertions)]
        if std::env::var("WINIA_RECOMPOSE_TRACE").is_ok() {
            // 触发本次重组的 State id 列表（对应 State::set/update 调用）
            eprintln!("[recompose] 触发 State: {:?}", pending_batch.ids());
        }
        for state_id in pending_batch.ids() {
            if let Some(keys) = self.slot_deps.get(state_id) {
                for &k in keys {
                    self.slot_table.mark_dirty(k);
                }
            }
            // 两段式依赖：布局期注册的依赖 → 只标布局失效（重测不重组）
            if let Some(keys) = self.layout_deps.get(state_id) {
                for &k in keys {
                    self.layout_dirty_keys.insert(k);
                }
            }
        }
        // The `slot_key` → arena-index map is valid for the whole content closure: it is filled by
        // `layout` and consumed by materialize at the END of compose, so during content it still names
        // the nodes the previous frame built — which is where a container's Skip decision reads the
        // modifier it is comparing against (`container_modifier_unchanged`).
        //
        // It can be EMPTY here: a second compose in the same frame follows a materialize that drained
        // it. Refilling costs one tree walk, and only on those frames — materialize's own repair
        // (`materialize`) does exactly this walk and would do it anyway, so this moves the same work
        // earlier rather than adding it.
        if self.prev_node_by_key.is_empty() {
            if let Some(root_idx) = self.arena.root {
                crate::runtime::materialize::collect_node_keys(&self.arena, root_idx, &mut self.prev_node_by_key);
            }
        }

        // Begin an isolated dependency frame. State::get() writes to its active
        // buffer, while nested Composer calls temporarily own their own frame.
        // The frame remains open through materialization and modifier reads.
        let mut dependency_frame = crate::runtime::state::begin_compose_deps_with_queue(
            std::sync::Arc::downgrade(&self.pending_states),
        );

        {
            let ctx = &mut ComposeCtx::new(self);
            content(ctx);
        }

        // modifier 依赖注册（scroll 等）移到 layout 的 materialize 之后——
        // 完整分离：compose 阶段不建 arena（此处 arena 为空）

        // 注意：recording target 不在此处清除——layout()（measure 阶段）的
        // SizeDynamic 闭包内 State::get() 也要记录依赖（kf/dp 尺寸动画），
        // 由 layout() 末尾统一 clear + drain（见 layout()）。
        self.slot_table.truncate();
        // Skipped hosts still place their referenced content. Sweep only after composition is done,
        // allowing a subtree removed from an earlier host to be claimed by a later host in this pass.
        self.slot_table.dispose_unplaced_movable();
        let mut live_compose_keys = crate::layout::node::SlotKeySet::default();
        let skipped_slots = self.slot_table.collect_live_keys(&mut live_compose_keys);
        #[cfg(test)]
        {
            self.live_residue = skipped_slots;
        }
        let _ = skipped_slots;
        live_compose_keys.insert(0);
        // 防御：scope 配对完整性（漏配 end_scope 会导致 SCOPE_STACK 残留跨帧，
        // 使下帧组件外读取注册到失效 scope → 失效静默丢失）
        debug_assert_eq!(GROUP_STACK.with(|s| s.borrow().len()), 0,
            "compose 结束时 GROUP_STACK 应清空（scope/节点配对不完整）");
        GROUP_STACK.with(|s| s.borrow_mut().clear());

        // 完整分离：组合完成后物化布局树（测试/调用方可直接 layout_root_idx）
        self.materialize();
        // A subcomposing node the composition touched has to be re-measured, and the LAYOUT path is
        // what does that: `layout()` clears every node's `dirty`/`layout_dirty` and re-derives them
        // from `layout_dirty_keys`, then uses that set to decide which parents descend at all. A node
        // whose slot status was Clean (its parent entered, so its body re-ran without its slot being
        // marked) arrives here with `dirty=false`, so the only place left to say "measure me" is this
        // set. Asked of the POLICY as well as of the node's `subcomposed` flag, because the flag is set
        // by a parking measurement and is therefore false on a node that was just rebuilt.
        //
        // The `dirty` guard is load-bearing and the flag has to be precise, or this seeding is what
        // makes a subcomposing tree re-measure everything: `subcomposed` used to be inferred from a
        // "how many subcompositions were parked while this node measured" counter, which is true for
        // every ANCESTOR of a subcomposing node, so the root of an 800-row tree was seeded on every
        // frame that changed anything and the whole tree re-measured — 800 subcompositions for one
        // row's update (74915 µs against ≈1600 µs; `docs/benchmarks.md`).
        for idx in 0..self.arena.nodes.len() {
            let declares = self.arena.nodes[idx].dirty
                && (self.arena.nodes[idx]
                    .measure_policy
                    .and_then(|p| self.arena.policies.get(p))
                    .map(|p| p.subcomposes())
                    .unwrap_or(false)
                    || self.arena.nodes[idx].subcomposed);
            #[cfg(debug_assertions)]
            if declares && std::env::var("WINIA_SUBCOMPOSE_TRACE").is_ok() {
                eprintln!(
                    "[compose-end] idx={idx} key={} sub={} dirty={} declares={}",
                    self.arena.nodes[idx].slot_key,
                    self.arena.nodes[idx].subcomposed,
                    self.arena.nodes[idx].dirty,
                    declares
                );
            }
            if declares {
                self.layout_dirty_keys.insert(self.arena.nodes[idx].slot_key);
            }
        }
        // Shared-element flights (Phase 2): detect switches + retain/detach
        // sources BEFORE the prev drain below frees them.
        self.retain_shared_sources();
        // Repair the arena listing invariant for EVERY compose — this must not live inside
        // `retain_shared_sources`, which returns early when the composer has no shared content:
        // that took the repair off the default path, and a stale listing then walked straight
        // into the `[dup-key]` guard during layout (measured: a composer with no shared content
        // kept `mid.children = [2, 2]` and panicked, while the same state with one shared
        // endpoint present was pruned to `[2]`). Order is load-bearing: after the retention
        // above (a flight ghost is a legitimate root and must keep its subtree) and before the
        // prev drain below (a stale listing's node still has to be reclaimed by it).
        crate::runtime::materialize::prune_stale_child_links(self);
        // 物化后：注册 modifier 中引用的 State 依赖（scroll 等——组合期 arena 空）。
        // 必须在 take_deps() 之前执行——其中 State::get() 依赖 DEP_MODE=Compose
        //（begin_compose_deps 后未复位）；先复位则 scroll 依赖被静默丢弃（滚动不刷新）
        if let Some(root_idx) = self.arena.root {
            register_modifier_deps_recursive(&self.arena, root_idx);
        }
        // 依赖注册（组合期 + modifier 期收集的 State 依赖 → 按 slot 收敛）。
        let recorded = crate::runtime::state::take_deps();
        self.reconcile_compose_deps(recorded, &live_compose_keys);
        // Commit only after the read graph is reconciled. If content/materialize
        // panics first, the guard restores the outer dependency frame and rolls
        // back subscriptions learned by this failed compose.
        dependency_frame.commit();
        // 回收本帧未复用的上帧节点（结构变化移除的子树——on_remove 触发）；
        // 跳过已复用节点（已挂入本帧树，free 会递归进本帧树形成环）
        let mut visited = crate::layout::node::NodeMarks::default();
        for (key, idx) in self.prev_node_by_key.drain() {
            // A node marked reused is part of THIS frame's tree — either claimed in place
            // (`try_claim_skipped_subtree`) or reached through a refilled index. It was not removed,
            // so neither its layout dependencies nor its `on_remove` are due: reporting it as removed
            // pruned a live slot's layout dependency and froze its animation (measured:
            // `test_layout_dep_survives_const_fold` fired exactly that way).
            if self.reused_nodes.contains(idx) {
                continue;
            }
            // 收集移除的 slot_key（layout_deps 死 key 清理）
            self.removed_slot_keys.insert(key);
            self.arena.free_node_skip(idx, &self.reused_nodes, &mut visited);
        }
        self.prev_node_by_key.clear();
        self.reused_nodes.clear();
        // A notification that arrived after the batch was drained may belong to
        // a read removed by this frame. Drop only IDs with no live channel.
        self.pending_states
            .drain_matching(|id| !self.slot_deps.contains_key(&id) && !self.layout_deps.contains_key(&id));
        pending_batch.commit();
        dependency_transaction.commit();
        compose_runtime_transaction.commit();
        // The composition has committed: the slot tree is materialized, the read graph is reconciled
        // and every transaction is closed, so user callbacks that publish to the outside world can
        // run. Deliberately LAST — a `State` written here notifies with the dependency frame closed,
        // so it becomes the NEXT PASS's input rather than an edge this one has to account for.
        //
        // Recording is forced OFF for the duration: when this compose() was started from a layout
        // pass (a subcomposition), committing the inner frame restores the outer LAYOUT frame, and a
        // read in a side effect would be filed as that composer's layout dependency.
        let _no_deps = crate::runtime::state::begin_no_deps_frame();
        for effect in std::mem::take(&mut self.post_compose) {
            effect();
        }
    }

    /// 返回 LayoutNode 树的根节点引用
    /// How many times this composer has composed. A subcomposing policy compares it to tell "the same
    /// frame measured me twice" (report the first result) from "a later frame measured me again"
    /// (compose the content again, because its parameters may have changed).
    pub fn compose_generation(&self) -> u64 {
        self.compose_count
    }

    /// The constraints this composer's last `layout()` ran under, if any.
    pub(crate) fn cached_root_constraints(&self) -> Option<Constraints> {
        self.arena
            .root
            .and_then(|root| self.arena.nodes.get(root))
            .and_then(|node| node.cached_constraints)
    }

    /// Re-run this composer's layout under new constraints — used for a CACHED subcomposition, whose
    /// content is already composed and only its arrangement can change.
    pub(crate) fn relayout_subcomposition(&mut self, constraints: Constraints) {
        self.layout(constraints);
    }

    /// Make a KEPT subcomposition ready for the next frame's composition.
    ///
    /// Adoption moves a subcomposition's arena into the outer one, which leaves this composer with an
    /// empty arena and the per-node bookkeeping `layout()` built from the OLD one. `prev_node_by_key` is
    /// the reuse index materialize consults before building nodes (`Some(idx)` into an arena that no
    /// longer has that index — measured as `index out of bounds: the len is 0 but the index is 0`); the
    /// per-frame reuse marks belong to the same dead frame. The SLOT TABLE is deliberately untouched:
    /// that is the half this reuse exists for.
    pub(crate) fn prepare_subcomposition_for_recompose(&mut self) {
        self.prev_node_by_key.clear();
        self.reused_nodes.clear();
        // The arena index the drained tree used. It is read by `layout()` to decide where to start the
        // reuse walk, and it names a node that is no longer in this arena — the second crash this
        // preparation had to clear (`index out of bounds` in `insert_reuse_key`, reached from
        // `collect_layout_index`).
        self.arena.root = None;
    }

    /// Take the cached subcomposition (see the field), if this frame's next `subcompose()` can    /// re-arrange it instead of composing again.
    pub(crate) fn take_cached_subcomposition(&mut self) -> Option<Box<Composer>> {
        self.subcomposition_cache.take()
    }

    /// Park a subcomposition against the node that composed it, for the adoption pass at the end of
    /// `layout` (`layout::subcompose`). The registry is cleared at the start of every layout pass.
    pub(crate) fn park_subcomposition(
        &mut self,
        node: usize,
        entry: crate::layout::subcompose::Subcomposition,
    ) {
        // The node's OWN policy composed content — recorded here, at the moment it happens, because this
        // is the only place that knows WHICH node subcomposed. `Composer::compose`'s compose-end seeding
        // reads the flag to decide whether the node has to be re-measured; inferring it from a
        // "subcompositions parked so far" counter (which is what the flag used to be) made it true for
        // every ancestor of a subcomposing node as well — see `LayoutNode::subcomposed`.
        if node < self.arena.nodes.len() {
            self.arena.nodes[node].subcomposed = true;
        }
        self.subcompositions.push((node, entry));
    }

    pub fn layout_root(&self) -> Option<&LayoutNode> {
        self.arena.root()
    }

    /// 返回 LayoutNode 树的根节点可变引用
    pub fn layout_root_mut(&mut self) -> Option<&mut LayoutNode> {
        self.arena.root_mut()
    }

    /// arena 节点池只读访问（arena 化遍历用）
    pub fn arena_nodes(&self) -> &[LayoutNode] {
        &self.arena.nodes
    }

    /// arena 节点池可变访问（arena 化遍历用）
    pub fn arena_nodes_mut(&mut self) -> &mut Vec<LayoutNode> {
        &mut self.arena.nodes
    }

    /// 根节点 arena 索引
    pub fn layout_root_idx(&self) -> Option<usize> {
        self.arena.root
    }

    /// 测量策略池只读访问（measure_node 用）
    pub fn arena_policies(&self) -> &[Box<dyn MeasurePolicy>] {
        &self.arena.policies
    }

    /// 执行整棵布局树的 measure + place，并缓存测量结果供下帧复用
    pub fn layout(&mut self, root_constraints: Constraints) {
        // Measure callbacks can read State and invoke another Composer. Keep
        // their active slot/group/statement context isolated as well.
        let _runtime_frame = begin_runtime_frame();
        let _adaptive_context = crate::layout::adaptive::enter_context(self.adaptive.clone());
        let mut layout_transaction = LayoutTransaction::new(self);
        // Layout can be called without a preceding compose; consume layout-only
        // invalidations here so measure sees the dirty path directly.
        self.consume_layout_pending();
        // 应用布局失效：清全树旧标记 → 按 layout_dirty_keys 标节点 + 祖先传播
        // （保守超集：祖先全链标脏——布局动画场景父必然依赖子尺寸，Compose 精确传播留待优化）
        if let Some(root_idx) = self.arena.root {
            let nodes = &mut self.arena.nodes;
            // 清全树旧标记（每帧重新标记）
            for n in nodes.iter_mut() { n.layout_dirty = false; }
            if !self.layout_dirty_keys.is_empty() {
                crate::layout::node::apply_layout_dirty(nodes, root_idx, &self.layout_dirty_keys);
            }
            self.layout_dirty_keys.clear();
        } else {
            self.layout_dirty_keys.clear();
        }
        // 物化只在 compose 末尾（完整分离：组合完成即建树）——layout 只测量。
        // 单独调 layout（无 compose）时树为空——measure 无操作（无害）
        // 开始布局期依赖记录（measure 中 State::get → 两段式分流）
        let mut dependency_frame = crate::runtime::state::begin_layout_deps_with_queue(
            Arc::downgrade(&self.pending_states),
        );
        begin_layout_measure_tracking();
        // Subcomposing policies park their compositions while measuring; adopt them right after the
        // tree is measured and BEFORE the reuse index is rebuilt — an adopted subtree has to be in
        // that index, or the next frame's reuse path never sees it.
        self.subcompositions.clear();
        let _layout_host = crate::layout::subcompose::LayoutHostGuard::arm(self as *mut Composer);
        if let Some(root_idx) = self.arena.root {
            let (_size, _placements) = crate::layout::measure_node(
                &mut self.arena.nodes, &self.arena.policies, root_idx, root_constraints);
            self.arena.nodes[root_idx].measured_size = _size;
            let parked = std::mem::take(&mut self.subcompositions);
            // `adopt_parked` hands back the last composition it moved, which becomes the cache a
            // second layout pass in this same frame can re-arrange instead of re-composing.
            self.subcomposition_cache = crate::layout::subcompose::adopt_parked(
                &mut self.arena,
                &mut self.reused_nodes,
                parked,
                self.uid,
            );
            // 阶段D：重建 slot_key → 节点索引映射（供下帧 start_node 复用）+ dirty 冒泡
            self.prev_node_by_key.clear();
            crate::runtime::materialize::collect_layout_index(
                &mut self.arena,
                root_idx,
                &mut self.prev_node_by_key,
            );
        } else {
            self.subcompositions.clear();
            // No root means every old layout dependency is stale.
            self.prev_node_by_key.clear();
        }

        let recorded = crate::runtime::state::take_deps();
        let measured_keys = take_layout_measure_keys();
        // A Composer with no root has no live layout readers. Clear the forward
        // graph before rebuilding the reverse index so direct layout() calls
        // cannot retain subscriptions after the tree disappeared.
        if self.arena.root.is_none() {
            self.layout_slot_reads.clear();
            self.layout_deps.clear();
        }
        let mut reads_by_slot: crate::layout::node::SlotKeyMap<HashSet<StateId>> = crate::layout::node::SlotKeyMap::default();
        for (signal, slot_key) in &recorded {
            reads_by_slot.entry(*slot_key).or_default().insert(signal.id());
            self.layout_signal_handles
                .entry(signal.id())
                .or_insert_with(|| signal.clone());
        }

        // Replace only slots that actually ran measure. A cached slot is absent
        // from measured_keys and therefore keeps its last successful reads.
        //
        // As in `reconcile_compose_deps`, the reverse index below is a pure function of this forward
        // one, so it is only rebuilt when something here actually moved: a layout in which every node
        // folded (nothing measured, nothing read) then costs a few map lookups instead of a graph
        // rebuild. `cleanup_signal_subscriptions` still runs every frame — it is the one place that
        // prunes both graphs' subscriptions, and it runs with their union.
        let mut changed = false;
        for slot_key in measured_keys {
            let new_set = match reads_by_slot.remove(&slot_key) {
                Some(reads) if !reads.is_empty() => Some(reads),
                _ => None,
            };
            let same = match (self.layout_slot_reads.get(&slot_key), new_set.as_ref()) {
                (None, None) => true,
                (Some(old), Some(new)) => old == new,
                _ => false,
            };
            changed |= !same;
            match new_set {
                Some(reads) => {
                    self.layout_slot_reads.insert(slot_key, reads);
                }
                None => {
                    self.layout_slot_reads.remove(&slot_key);
                }
            }
        }
        // Compose confirmed removals before rebuilding the reverse index so a
        // dead slot cannot reappear from an old forward edge.
        for &slot_key in &self.removed_slot_keys {
            changed |= self.layout_slot_reads.remove(&slot_key).is_some();
            self.layout_dirty_keys.remove(&slot_key);
        }
        self.removed_slot_keys.clear();

        if changed {
            self.rebuild_layout_reverse_deps();
        }

        self.cleanup_signal_subscriptions();
        self.debug_assert_dependency_graphs();
        self.pending_states
            .drain_matching(|id| !self.slot_deps.contains_key(&id) && !self.layout_deps.contains_key(&id));
        // Normal layout completion restores the outer dependency frame. A panic
        // before this point drops the guard and rolls back partial subscriptions.
        dependency_frame.commit();
        layout_transaction.commit();
        self.dispatch_layout_callbacks();
    }

    /// Fire the position callbacks whose box changed this pass, then serve bring-into-view requests.
    ///
    /// Runs AFTER the dependency frame and the layout transaction commit, so a callback that writes a
    /// State schedules the next frame instead of being recorded as a layout dependency.
    fn dispatch_layout_callbacks(&mut self) {
        let Some(root) = self.arena.root else { return };
        // Cheap guard: most trees use neither feature, and the guarded work is a full tree walk.
        let has_positions = self
            .arena
            .nodes
            .iter()
            .any(|node| node.modifier.has_position_callbacks());
        if has_positions {
            // The collector starts at this composer's origin, so its change gate stores WINDOW-space
            // boxes: an overlay layer whose origin moves re-fires the callback instead of reporting
            // local coordinates until its own box changes.
            let (placed, global) = crate::layout::node::collect_position_callbacks(
                &mut self.arena.nodes,
                root,
                self.screen_origin,
            );
            for (callback, position) in placed {
                callback(position);
            }
            for (callback, position) in global {
                callback(position);
            }
        }
        let has_requests = self
            .arena
            .nodes
            .iter()
            .any(|node| node.modifier.has_bring_into_view_requester());
        if has_requests {
            crate::layout::relocation::process_bring_into_view(&mut self.arena.nodes, root);
        }
    }

    /// The index of the node carrying `Modifier::layout_id(id)`, if there is one.
    pub fn find_by_layout_id(&self, id: &str) -> Option<usize> {
        self.arena
            .nodes
            .iter()
            .position(|node| node.modifier.layout_id_value() == Some(id))
    }

    /// 请求重组（由 State 变化触发）。
    pub fn request_recomposition(&mut self, _key: u64) {
        self.needs_recomposition = true;
    }

    /// Re-run the WHOLE content on the next pass: the root scope and every group under it are marked
    /// dirty, so nested groups enter regardless of their (unchanged) parameters.
    ///
    /// This is what a caller recomposing means for a subcomposition it owns — an overlay's composer is
    /// a separate one, so nothing inside it can see that the caller produced a NEW content closure, and
    /// a group re-enters only for state it read or parameters it declared. Without this, overlay content
    /// built from a value the caller captured (a filtered list, a formatted string) keeps the first
    /// closure's value — measured on `search_bar_demo`, whose docked dropdown and fullscreen panel both
    /// stayed on their first rows while the input field updated. Marking only the root is not enough:
    /// the caller's lambda usually sits inside wrapper groups of its own, which declare nothing.
    ///
    /// Cost: that pass re-runs the content subtree (remembered state survives, `LazyColumn` items are
    /// recomposed by key), so a caller should re-register an overlay when its inputs changed rather
    /// than unconditionally every frame.
    pub fn mark_content_dirty(&mut self) {
        self.needs_recomposition = true;
        SlotTable::mark_dirty_subtree(&mut self.slot_table.root_slot);
    }

    /// Mark every slot INSIDE the current composition position dirty, leaving the slot we are in alone.
    ///
    /// This is the CompositionLocal companion. A local is a stack, and a reader only picks a new value
    /// up when its own group runs again — so a provider whose value changed has to dirty the readers it
    /// is about to wrap, or a group that declared nothing about itself keeps what it composed with.
    /// Marking the position's children (not the position itself) keeps the enclosing group's own Skip
    /// decision out of it: it re-enters next frame only if something else asks it to.
    pub(crate) fn mark_inner_slots_dirty(&mut self) {
        let slot = self.slot_table.current_slot();
        for child in &mut slot.children {
            SlotTable::mark_dirty_subtree(child);
        }
    }

    fn has_pending_compose_states(&self) -> bool {
        let compose_ids: HashSet<StateId> = self.slot_deps.keys().copied().collect();
        self.pending_states
            .pending_ids()
            .into_iter()
            .any(|id| compose_ids.contains(&id))
    }

    /// Move layout-only invalidations into layout_dirty_keys without consuming
    /// an ID that also requires composition. This makes direct layout() calls
    /// correct while preserving a mixed compose/layout notification for compose().
    fn consume_layout_pending(&mut self) {
        let compose_ids: HashSet<StateId> = self.slot_deps.keys().copied().collect();
        let layout_ids: HashSet<StateId> = self.layout_deps.keys().copied().collect();
        let layout_pending = self
            .pending_states
            .drain_non_compose_collect_layout(&compose_ids, &layout_ids);
        for state_id in layout_pending {
            if let Some(keys) = self.layout_deps.get(&state_id) {
                self.layout_dirty_keys.extend(keys.iter().copied());
            }
        }
    }

    /// 是否有待处理的 state 变化
    pub fn has_pending_states(&self) -> bool {
        !self.pending_states.is_empty()
    }

    /// 取走本帧注册的顶层弹出层（compose 后调用——清空收集）
    pub fn take_overlays(&mut self) -> Vec<crate::overlay::OverlayDesc> {
        std::mem::take(&mut self.overlays)
    }

    /// 当前组合节点的 slot_key（overlay 锚点用）
    pub fn active_slot_key(&self) -> u64 {
        self.slot_table.active_slot_key()
    }

    /// 重组次数（vsync 研究——单次渲染内的 compose 次数）
    pub fn compose_count(&self) -> u64 {
        self.compose_count
    }

    /// 待消费 State 数（vsync 研究——渲染时刻的 pending 积压）
    pub fn pending_state_count(&self) -> usize {
        self.pending_states.len()
    }

    /// 执行待处理的重组。返回 true 表示实际执行了 compose。
    /// 若无待处理则跳过，保留上一帧的布局树。
    pub fn recompose(&mut self, content: impl FnOnce(&mut ComposeCtx)) -> bool {
        // Resolve layout-only notifications before deciding whether composition
        // is needed. This prevents the app frame loop from spinning on a pending
        // ID that belongs only to layout_deps.
        self.consume_layout_pending();
        let has_pending = self.has_pending_compose_states();
        let will_run = self.needs_recomposition || has_pending || !self.pending_recomposition.is_empty();
        #[cfg(debug_assertions)]
        if std::env::var("WINIA_RECOMPOSE_TRACE").is_ok() {
            // 重组触发诊断：何时执行/跳过 + 触发原因（pending=State 变化数）
            eprintln!(
                "[recompose] pending={} needs={} q={} → {}",
                has_pending, self.needs_recomposition, self.pending_recomposition.len(),
                if will_run { "COMPOSE" } else { "SKIP（无变化）" }
            );
        }

        if !will_run {
            return false;
        }

        // 处理队列中的待重组节点（当前简化：全量重组）
        self.pending_recomposition.clear();
        self.needs_recomposition = false;
        self.compose(content);
        true
    }
}

/// Compose 末尾：递归遍历 LayoutNode 树（arena），为所有 modifier 注册 State 依赖。
/// 模块级函数（impl 外）——impl 内直接调用。
fn register_modifier_deps_recursive(arena: &crate::layout::node::NodeArena, idx: usize) {
    // Modifier reads happen after GROUP_STACK is cleared; restore the node target
    // explicitly so scroll State dependencies do not all attach to the last node.
    set_active_slot_key(arena.nodes[idx].slot_key);
    arena.nodes[idx].modifier.register_state_deps();
    let children = arena.nodes[idx].children.clone();
    for c in children {
        register_modifier_deps_recursive(arena, c);
    }
}

impl Default for Composer {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Composer {
    fn drop(&mut self) {
        if self.compose_transaction.is_some() {
            self.rollback_compose_runtime();
        }
        crate::animation::clear_animations_for_states(
            &self.animation_state_ids.iter().copied().collect::<Vec<_>>(),
        );
        self.pending_states.unsubscribe_all();
    }
}

#[cfg(test)]
mod tests;
