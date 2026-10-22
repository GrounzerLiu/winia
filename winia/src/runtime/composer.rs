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


mod ctx;
mod engine;
mod slot_table;
mod transactions;

pub use ctx::ComposeCtx;
pub use engine::Composer;
pub use slot_table::GroupStatus;
pub(crate) use slot_table::{SlotStatus, SlotTable};
use slot_table::{NodeDesc, SlotTableRuntimeSnapshot};

#[cfg(test)]
mod tests;
