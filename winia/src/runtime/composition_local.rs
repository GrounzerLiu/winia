//! Implicitly propagated values — winia's analogue of Jetpack Compose's `CompositionLocal`.
//!
//! The core ideas:
//! - A provider puts a value in scope for a subtree; a reader asks for it without a parameter.
//!   The stack is one thread-local `Vec<(id, Arc<Supplied>)>`: push before the content, pop after,
//!   and a read scans from the back so the innermost provider wins.
//! - **A read is the subscription.** `current()` hands the entry's `StateSignal` to
//!   `register_dependency`, the same call `State::get` makes. The scopes that re-run are exactly the
//!   scopes that read the value, so no component writes invalidation code and no subtree is dirtied
//!   because "we do not know who read it".
//! - Four ways in, and the difference is the whole API: `provide(value, ctx, content)` installs one
//!   local and invalidates; `ctx.provide(&[..], content)` installs several at ONE slot position;
//!   `provides(value, f)` is the ctx-less form and invalidates nobody; `provided(value)` only
//!   describes an entry for the other two.
//! - An entry's signal must OUTLIVE a change — readers point at the signal, not at the value — so a
//!   later pass swaps the value in place instead of replacing the entry.
//! - `capture`/`with_snapshot` replay the stack around an independent composition unit (an overlay),
//!   sharing the same entries and therefore the same signals.
//!
//! Two consequences worth knowing before changing this file: invalidation lands one pass late (the
//! pending queue drains at the START of a pass), and a provider's entry is addressed by a per-site
//! key whose keyless fallback drifts with the structure.
//!
//! Rationale and the redesign that produced this: `docs/composition-local-redesign.md`. Mechanism,
//! pitfalls and the invariant each test pins: `docs/composition-local.md`. How a component should
//! choose a provider: `docs/component-authoring.md` §13.

use std::any::Any;
use std::cell::RefCell;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use crate::runtime::composer::ComposeCtx;
use crate::runtime::state::{Backchannel, StateSignal, next_state_id};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// One provide site's state: its current value plus the signal that announces changes.
///
/// The signal is what makes a read a subscription. `CompositionLocal::current` hands it to
/// `register_dependency`, so the scope that read the value becomes a dependent and is marked dirty
/// when the provider moves — no `mark_subtree_dirty`, and no "remember to invalidate" duty on the
/// component that provides it.
///
/// The value sits behind a lock because the SIGNAL has to outlive a change: readers subscribed to it,
/// so replacing the whole entry with a fresh one would leave every existing subscriber pointing at a
/// signal nobody ever notifies again. One entry per provide site, one signal, value swapped in place.
///
/// `signal` is `None` for a provide that cannot invalidate anything — see [`Supplied::untracked`].
pub struct Supplied {
    value: std::sync::RwLock<Arc<dyn Any>>,
    signal: Option<Arc<StateSignal>>,
}

impl Supplied {
    /// A provide with no invalidation. Its readers subscribe to nothing, which is what the `ctx`-less
    /// [`CompositionLocal::provides`] needs: it wraps a whole FRAME (density, direction) in which the
    /// interesting readers run at measure/draw time, where composition invalidation could not reach
    /// them even if there were a signal to fire.
    fn untracked<T: Clone + 'static>(value: T) -> Self {
        Self {
            value: std::sync::RwLock::new(Arc::new(value)),
            signal: None,
        }
    }

    /// A provide whose readers subscribe, so a change re-enters exactly those scopes.
    fn tracked<T: Clone + 'static>(value: T) -> Self {
        Self {
            value: std::sync::RwLock::new(Arc::new(value)),
            signal: Some(StateSignal::new(next_state_id())),
        }
    }

    /// Swap in a new value. The signal stays, so readers already subscribed still hear about it.
    fn replace<T: Clone + 'static>(&self, value: T) {
        *self.value.write().unwrap_or_else(|e| e.into_inner()) = Arc::new(value);
    }

    /// [`Self::tracked`] for a value that arrives ALREADY type-erased.
    ///
    /// A multi-value provide holds several locals in one `&[ProvidedValue]`, so the concrete `T` is
    /// gone by then; this is what seeds each entry on the first frame.
    fn tracked_erased(value: Arc<dyn Any>) -> Self {
        Self {
            value: std::sync::RwLock::new(value),
            signal: Some(StateSignal::new(next_state_id())),
        }
    }

    /// [`Self::is_same`] + [`Self::replace`] for an erased value: swap `candidate` in when it moved,
    /// and report whether it did.
    ///
    /// `same` is the LOCAL's comparison, already erased to `dyn Any` by
    /// [`CompositionLocal::provided`]; it owns the downcast, so a type mismatch arrives here as
    /// "not the same" — the right answer, since no reader can call a different type unchanged.
    fn update_erased(
        &self,
        candidate: &Arc<dyn Any>,
        same: &dyn Fn(&dyn Any, &dyn Any) -> bool,
    ) -> bool {
        let mut guard = self.value.write().unwrap_or_else(|e| e.into_inner());
        if same(&**guard, &**candidate) {
            return false;
        }
        *guard = Arc::clone(candidate);
        true
    }

    fn get<T: Clone + 'static>(&self) -> T {
        let guard = self.value.read().unwrap_or_else(|e| e.into_inner());
        T::clone(
            guard
                .downcast_ref::<T>()
                .expect("CompositionLocal value read through the wrong local"),
        )
    }

    /// Whether the value held here matches `candidate` under the local's own comparison.
    fn is_same<T: 'static>(&self, candidate: &T, same: impl Fn(&T, &T) -> bool) -> bool {
        let guard = self.value.read().unwrap_or_else(|e| e.into_inner());
        guard
            .downcast_ref::<T>()
            .is_some_and(|previous| same(previous, candidate))
    }
}

/// One local's contribution to a multi-value provide — Compose's `A provides x`.
///
/// Built by [`CompositionLocal::provided`], consumed by
/// [`ComposeCtx::provide`](crate::runtime::composer::ComposeCtx::provide). The value is type-erased
/// because a `&[ProvidedValue]` is HOMOGENEOUS while the locals in it are not: `LocalContentColor`
/// holds a `Color` and `LocalTextStyle` a `TextStyle` in the same array. Both halves that need `T`
/// travel with the value — the payload as `dyn Any`, and the local's own comparison as a predicate
/// that downcasts inside itself.
pub struct ProvidedValue {
    pub(crate) id: u64,
    pub(crate) value: Arc<dyn Any>,
    /// The local's `same`, applied through the downcast. A type mismatch answers `false`: a site
    /// that now holds a different type is a change no reader can ignore.
    ///
    /// `Send + Sync` for the same reason the local's own `same_erased` carries them: it is shared
    /// from a `static` local.
    pub(crate) same: Arc<dyn Fn(&dyn Any, &dyn Any) -> bool + Send + Sync>,
    /// Whether a moved value may wake its readers — `false` for `new_static`. Carried per VALUE
    /// rather than per call, so one `ctx.provide` can mix a notifying local with a static one.
    /// It is a COPY of the local's own flag, taken when [`CompositionLocal::provided`] bundled it.
    pub(crate) notifies: bool,
}

thread_local! {
    /// (local_id, value + signal) stack — `provides` pushes, the closure pops.
    static SLOTS: RefCell<Vec<(u64, Arc<Supplied>)>> = RefCell::new(Vec::new());
}

/// CompositionLocal 栈快照（`capture` 产物——可在 later 时刻 `with_snapshot`
/// 重放，如 overlay 独立 Composer 继承主树主题/方向/排版等隐式上下文）
pub type LocalSnapshot = Vec<(u64, Arc<Supplied>)>;

/// 捕获当前 CompositionLocal 栈快照（Arc 引用共享——O(1) 浅拷贝）。
/// 需在相关 provides 闭包**内**调用（如 open_overlay 组合期——主树 Theme
/// provides 内）。
pub fn capture() -> LocalSnapshot {
    SLOTS.with(|s| {
        let slots = s.borrow();
        slots
            .iter()
            .map(|(id, arc)| (*id, Arc::clone(arc)))
            .collect()
    })
}

/// 在闭包执行期间重放快照（压栈全部 → 执行 → 弹栈）。用于独立组合单元
/// （overlay 等）继承捕获时的隐式上下文。
/// ⚠ 弹栈按**压栈前深度**恢复——若 f 内部又 provides 了新值（嵌套），它们
/// 留在栈上不被误弹（只弹快照压入的条目）。
pub fn with_snapshot<R>(snapshot: &LocalSnapshot, f: impl FnOnce() -> R) -> R {
    if snapshot.is_empty() {
        return f();
    }
    // 压栈前深度——重放后截断回此深度（f 内部新 provides 的值不被误弹）
    let base_len = SLOTS.with(|s| s.borrow().len());
    SLOTS.with(|s| {
        let mut slots = s.borrow_mut();
        for (id, arc) in snapshot {
            slots.push((*id, Arc::clone(arc)));
        }
    });
    let mut guard = TruncateGuard {
        base_len,
        used: false,
    };
    let result = f();
    // 正常弹栈（drop guard 兜底 panic 路径）
    SLOTS.with(|s| s.borrow_mut().truncate(base_len));
    guard.disarm();
    result
}

/// Install several locals at ONE composition site — Compose's
/// `CompositionLocalProvider(A provides x, B provides y, content = ...)`.
///
/// `entry` is the site's storage, created by the caller with
/// [`ComposeCtx::remember_backchannel_at_key`](crate::runtime::composer::ComposeCtx::remember_backchannel_at_key).
/// One CALL is one slot position, so the values no longer have to nest — which is what let three
/// levels of nesting overwrite each other's memory and made the per-site counter necessary.
///
/// Entries are matched by the LOCAL's id, not by array position. Two consequences that matter:
///
/// - a call site whose value list GROWS or SHRINKS between passes keeps working. Its existing
///   entries keep their signals (so readers already subscribed still hear about them), a local
///   joining the list gets a fresh entry seeded with the value it is given, and a local leaving it
///   simply stops being on the stack. Panicking here would abort a shipping app for a mistake whose
///   only symptom a user would ever see is a blank window.
/// - reordering the array is not a change. Zipping by position would report every local as moved
///   (each one downcasts against another's payload, fails, and answers "changed"), re-entering every
///   reader for a list that had not changed at all.
pub fn provide_all<R>(
    values: &[ProvidedValue],
    entry: &Backchannel<Vec<(u64, Arc<Supplied>)>>,
    content: impl FnOnce() -> R,
) -> R {
    let previous = entry.peek();
    // Reuse the entry a local already had, so its signal survives (a subscriber points at the
    // signal, not at the value). Only a local new to this site gets a fresh one.
    let mut supplied: Vec<(u64, Arc<Supplied>)> = Vec::with_capacity(values.len());
    let mut reshaped = previous.len() != values.len()
        || previous
            .iter()
            .zip(values)
            .any(|((id, _), value)| *id != value.id);
    for value in values {
        match previous.iter().find(|(id, _)| *id == value.id) {
            Some((id, slot)) => supplied.push((*id, Arc::clone(slot))),
            None => {
                reshaped = true;
                supplied.push((
                    value.id,
                    Arc::new(Supplied::tracked_erased(Arc::clone(&value.value))),
                ));
            }
        }
    }
    if reshaped {
        entry.set(supplied.clone());
    }
    // Compare first, so a notify lands before content runs and the readers it wakes are the ones
    // that composed against the OLD value.
    //
    // `notifies` is per VALUE, not per call: a `new_static` local in the same array still has its
    // entry replaced (a reader that composes later must see the current value) but must never wake
    // anybody.
    for ((_, slot), value) in supplied.iter().zip(values) {
        if slot.update_erased(&value.value, &*value.same)
            && value.notifies
            && let Some(signal) = &slot.signal
        {
            signal.notify(true);
        }
    }
    let base_len = SLOTS.with(|s| s.borrow().len());
    SLOTS.with(|s| {
        let mut slots = s.borrow_mut();
        for (id, slot) in supplied.iter() {
            slots.push((*id, Arc::clone(slot)));
        }
    });
    let mut guard = TruncateGuard {
        base_len,
        used: false,
    };
    let result = content();
    SLOTS.with(|s| s.borrow_mut().truncate(base_len));
    guard.disarm();
    result
}

/// A site's first-pass storage for [`provide_all`].
pub fn seed_supplied(values: &[ProvidedValue]) -> Vec<(u64, Arc<Supplied>)> {
    values
        .iter()
        .map(|value| {
            (
                value.id,
                Arc::new(Supplied::tracked_erased(Arc::clone(&value.value))),
            )
        })
        .collect()
}

/// Restores the provide stack to `base_len` on drop, so a panicking content closure cannot leave
/// entries behind for the rest of the frame to see.
struct TruncateGuard {
    base_len: usize,
    used: bool,
}
impl TruncateGuard {
    fn disarm(&mut self) {
        self.used = true;
    }
}
impl Drop for TruncateGuard {
    fn drop(&mut self) {
        // panic 安全：正常路径已弹（disarm）；若 f panic，此处兜底清理
        if !self.used {
            SLOTS.with(|s| {
                let mut slots = s.borrow_mut();
                slots.truncate(self.base_len);
            });
        }
    }
}

/// Panic 安全：即使闭包 panic，Drop 时也会从栈中移除对应条目。
struct PopGuard {
    id: u64,
    used: bool,
}

impl PopGuard {
    fn new(id: u64) -> Self {
        Self { id, used: false }
    }
    fn disarm(&mut self) {
        self.used = true;
    }
}

impl Drop for PopGuard {
    fn drop(&mut self) {
        if !self.used {
            SLOTS.with(|s| {
                let mut slots = s.borrow_mut();
                if let Some(pos) = slots.iter().rposition(|(id, _)| *id == self.id) {
                    slots.remove(pos);
                }
            });
        }
    }
}

pub struct CompositionLocal<T: Clone + 'static> {
    id: u64,
    default: fn() -> T,
    /// Whether the value about to be provided counts as the same as the previous frame's — the
    /// whole reason a provided value can invalidate its readers, and therefore a property of the
    /// LOCAL rather than of each call site. A `new` local compares with `==`; `new_by` takes the
    /// comparison a type without `PartialEq` needs (an `Arc<dyn Fn>` is compared by pointer);
    /// `new_static` answers `false` here — see `notifies` for why that is the opposite of a bug.
    same: fn(&T, &T) -> bool,
    /// [`Self::same`] with the downcast folded in, for [`Self::provided`].
    ///
    /// A multi-value provide holds several locals in one `&[ProvidedValue]`, where the concrete `T`s
    /// are gone. This is what lets each entry compare a value it can no longer name. Built ONCE, at
    /// construction (locals are `static`), so a provider clones an `Arc` per frame instead of
    /// allocating a closure.
    same_erased: Arc<dyn Fn(&dyn Any, &dyn Any) -> bool + Send + Sync>,
    /// Whether a value that moved may WAKE its readers — `false` for `new_static`.
    ///
    /// Deliberately separate from `same`, because "does not invalidate" and "does not update" are
    /// different promises and conflating them is a real bug: a static local's value must still move,
    /// or every reader that composes LATER (for some other reason) would read a value older than the
    /// one in scope. That is why `new_static`'s `same` answers `false` — "always replace" — while
    /// this flag alone keeps the signal quiet.
    notifies: bool,
}

/// Safety: 新建实例时分配唯一 ID。所有实例都应放在 `LazyLock` / `static` 中，
/// 确保只初始化一次（否则多次 `new()` 会产生不同的 ID）。
impl<T: Clone + 'static> CompositionLocal<T> {
    /// A local whose value is compared with `==` — the ordinary case.
    pub fn new(default: fn() -> T) -> Self
    where
        T: PartialEq,
    {
        Self::with_policy(default, |a, b| a == b, true)
    }

    /// A local whose value carries no usable `PartialEq`: an `Arc<dyn Fn>`, a handle compared by
    /// pointer or by id, an `Option` of either. `same` returning `true` means "do not touch the
    /// readers"; returning `true` when the values really differ leaves them on a stale value, so
    /// compare by the identity that actually distinguishes two states.
    pub fn new_by(default: fn() -> T, same: fn(&T, &T) -> bool) -> Self {
        Self::with_policy(default, same, true)
    }

    /// A local that never invalidates its readers, even when the provided value changes — Compose's
    /// `staticCompositionLocalOf`. The reader picks up the new value the next time it composes for
    /// some other reason; nothing is marked dirty on its behalf.
    ///
    /// Note what this does NOT do: it does not freeze the value. The entry is still replaced on every
    /// pass (`same` answers "different, replace"), it just never notifies — because a reader that
    /// composes later for its own reasons must still see the CURRENT value. Folding the two together
    /// would make the local a stale snapshot from its first pass.
    pub fn new_static(default: fn() -> T) -> Self {
        Self::with_policy(default, |_, _| false, false)
    }

    fn with_policy(default: fn() -> T, same: fn(&T, &T) -> bool, notifies: bool) -> Self {
        Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            default,
            same,
            same_erased: Arc::new(move |a: &dyn Any, b: &dyn Any| match (
                a.downcast_ref::<T>(),
                b.downcast_ref::<T>(),
            ) {
                (Some(previous), Some(next)) => same(previous, next),
                // A type mismatch is a CHANGE, not "unchanged": this provide site now holds a
                // different type than last frame, which no reader of the old value can ignore.
                _ => false,
            }),
            notifies,
        }
    }

    /// Bundle this local with `value` for a multi-value
    /// [`ComposeCtx::provide`](crate::runtime::composer::ComposeCtx::provide) — Compose's
    /// `LocalContentColor provides color`.
    ///
    /// Nothing is provided yet: this only describes one entry. Reads and writes happen in
    /// `ctx.provide`, which is what needs a composition position to key the entry on.
    pub fn provided(&self, value: T) -> ProvidedValue {
        ProvidedValue {
            id: self.id,
            value: Arc::new(value),
            same: Arc::clone(&self.same_erased),
            notifies: self.notifies,
        }
    }

    /// Read the value in scope, or `default` when nothing provides one.
    ///
    /// **The read is the subscription.** When the entry was supplied by a tracked provider, its
    /// signal goes to `register_dependency` — the same call `State::get` makes — so the scope doing
    /// this read is marked dirty when the provider moves. Nothing else has to happen for a reader to
    /// see a new value: no `mark_subtree_dirty`, no "invalidate on change" in the provider.
    pub fn current(&self) -> T {
        match self.find() {
            Some((value, signal)) => {
                Self::subscribe(signal);
                value
            }
            None => (self.default)(),
        }
    }

    /// 读取栈顶匹配的当前值；无匹配时返回 `None`（不调用 default——
    /// 用于区分"provides 内（有值）"与"provides 外（无值）"，如 Text 注册到
    /// SelectionContainer 的判定：provides 外不应注册到 default 空 registrar）
    pub fn try_current(&self) -> Option<T> {
        let (value, signal) = self.find()?;
        Self::subscribe(signal);
        Some(value)
    }

    /// The value in scope for this local, as (value, the signal it came from).
    ///
    /// The `SLOTS` borrow ENDS before the caller subscribes: `register_dependency` reaches into the
    /// composer and can run code that reads another local, which would re-enter the `RefCell`.
    fn find(&self) -> Option<(T, Option<Arc<StateSignal>>)> {
        SLOTS.with(|s| {
            let slots = s.borrow();
            slots
                .iter()
                .rev()
                .find(|(sid, _)| *sid == self.id)
                .map(|(_, supplied)| (supplied.get::<T>(), supplied.signal.clone()))
        })
    }

    /// Register this read against the providing entry's signal — the same call `State::get` makes.
    ///
    /// A `None` signal belongs to a provide that cannot invalidate (see [`Supplied::untracked`]);
    /// reading it registers nothing, which costs what the old pure-pull read cost.
    ///
    /// Outside a composition there is no scope to register against either, and
    /// `register_dependency` answers that by returning `None`; a measure/draw read falls back to the
    /// active node, which is a layout dependency.
    ///
    /// The return value is discarded on purpose. `State::get` keeps it for the post-read
    /// `enqueue_if_changed` handshake, which closes the window between reading the value and
    /// subscribing: a write landing in that gap would otherwise be lost. A local has no such window —
    /// the value it reads comes from the same entry the signal belongs to, and a write to it is a
    /// whole composition pass, not a concurrent `State::set` from another thread.
    fn subscribe(signal: Option<Arc<StateSignal>>) {
        if let Some(signal) = signal {
            crate::runtime::state::register_dependency(signal);
        }
    }

    /// 在闭包执行期间提供新值，**不让任何读者失效**。嵌套 provides 通过 unique ID 正确隔离——
    /// 即使是同一个 `CompositionLocal` 再次嵌套，`rposition` 也能弹出最内层。
    ///
    /// Use this ONLY where invalidation is impossible or meaningless — the four providers that wrap a
    /// whole frame (`with_density`, `direction::provides`) or install a value once
    /// (`with_saveable_registry`, `with_component_styles`). Everywhere else use
    /// [`Self::provide`]: it takes the `ctx`, compares the value, and lets its readers re-run on
    /// their own.
    pub fn provides<R>(&self, value: T, f: impl FnOnce() -> R) -> R {
        SLOTS.with(|s| {
            s.borrow_mut()
                .push((self.id, Arc::new(Supplied::untracked(value))))
        });
        let mut guard = PopGuard::new(self.id);
        let result = f();
        // 正常弹栈：找最后一个匹配 id（处理同实例嵌套）
        SLOTS.with(|s| {
            let mut slots = s.borrow_mut();
            if let Some(pos) = slots.iter().rposition(|(sid, _)| *sid == self.id) {
                slots.remove(pos);
            }
        });
        guard.disarm();
        result
    }

    /// Provide a value and let its readers re-run when it changes — the form every provider should
    /// use when it has a `ctx`.
    ///
    /// The entry lives at this SITE (a per-site key over the current position), so a second
    /// `provide` in the same group gets its own entry and its own signal; `text.rs` nests two
    /// `ProvideTextStyle` in one statement and neither disturbs the other. On a later frame the same
    /// entry is found again, its value compared with the local's own policy, and only a genuine
    /// change notifies — the readers already subscribed through [`Self::current`] are then re-entered
    /// by the runtime, which is the whole point: no component writes invalidation code.
    pub fn provide<R>(
        &self,
        value: T,
        ctx: &mut ComposeCtx,
        content: impl FnOnce(&mut ComposeCtx) -> R,
    ) -> R {
        let key = ctx.next_provider_key();
        let entry = ctx.remember_backchannel_at_key(key, || {
            Backchannel::new(Arc::new(Supplied::tracked(value.clone())))
        });
        let supplied = entry.peek();
        if !supplied.is_same(&value, self.same) {
            supplied.replace(value.clone());
            // A `new_static` local replaces its value but never wakes a reader — see `notifies`.
            if self.notifies
                && let Some(signal) = &supplied.signal
            {
                signal.notify(true);
            }
        }
        SLOTS.with(|s| s.borrow_mut().push((self.id, supplied)));
        let mut guard = PopGuard::new(self.id);
        let result = content(ctx);
        SLOTS.with(|s| {
            let mut slots = s.borrow_mut();
            if let Some(pos) = slots.iter().rposition(|(sid, _)| *sid == self.id) {
                slots.remove(pos);
            }
        });
        guard.disarm();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nested_same_local() {
        let local = CompositionLocal::new(|| 0i32);
        let result = local.provides(1, || {
            assert_eq!(local.current(), 1);
            local.provides(2, || {
                assert_eq!(local.current(), 2);
                42
            })
        });
        assert_eq!(result, 42);
        assert_eq!(local.current(), 0); // 恢复默认
    }

    #[test]
    fn test_nested_different_locals() {
        let a = CompositionLocal::new(|| "a_default".to_string());
        let b = CompositionLocal::new(|| "b_default".to_string());
        a.provides("A".to_string(), || {
            b.provides("B".to_string(), || {
                assert_eq!(a.current(), "A");
                assert_eq!(b.current(), "B");
            });
            assert_eq!(a.current(), "A");
            assert_eq!(b.current(), "b_default");
        });
        assert_eq!(a.current(), "a_default");
    }

    #[test]
    fn test_panic_safety() {
        let local = CompositionLocal::new(|| 0i32);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            local.provides(42, || {
                panic!("boom");
            });
        }));
        assert!(result.is_err());
        assert_eq!(local.current(), 0, "should restore default after panic");
    }

    #[test]
    fn test_default_when_no_provides() {
        let local = CompositionLocal::new(|| 99i32);
        assert_eq!(local.current(), 99);
    }

    #[test]
    fn test_try_current_scope_boundary() {
        let local = CompositionLocal::new(|| 0i32);
        assert_eq!(local.try_current(), None); // provides 外无值
        let result = local.provides(7, || {
            assert_eq!(local.try_current(), Some(7)); // provides 内有值
            local.current()
        });
        assert_eq!(result, 7);
        assert_eq!(local.try_current(), None); // 退出后恢复无值
    }

    /// 快照：provides 内捕获 → 闭包外重放（模拟 overlay 继承主树上下文）
    #[test]
    fn test_capture_and_with_snapshot() {
        let local = CompositionLocal::new(|| 0i32);
        let snap = local.provides(42, capture);
        // 闭包外：current 恢复默认
        assert_eq!(local.current(), 0);
        // 重放：provides 生效
        let r = with_snapshot(&snap, || local.current());
        assert_eq!(r, 42);
        // 重放后：恢复
        assert_eq!(local.current(), 0);
    }

    /// 快照嵌套重放不污染外部栈
    #[test]
    fn test_with_snapshot_restores_stack() {
        let a = CompositionLocal::new(|| "a0".to_string());
        let b = CompositionLocal::new(|| "b0".to_string());
        let snap_a = a.provides("A".to_string(), capture);
        // 内部 b.provides + 重放 a；b 在 with_snapshot 期间仍生效（不被误弹）
        let r = b.provides("B".to_string(), || {
            let inner = with_snapshot(&snap_a, || {
                assert_eq!(b.current(), "B", "快照重放期间外部 b 应仍生效");
                a.current()
            });
            assert_eq!(b.current(), "B", "with_snapshot 返回后外部 b 仍生效");
            inner
        });
        assert_eq!(r, "A");
        // b.provides 闭包结束 → b 弹栈回默认（与快照无关）
        assert_eq!(b.current(), "b0");
        assert_eq!(a.current(), "a0");
    }

    // ── `provide`: the invalidation half of a provider ──

    /// A reader between the provider and a leaf that bumps `runs` when it composes — the witness
    /// for whether the subtree was re-entered.
    ///
    /// It also READS the local, which is the whole point: a read is what subscribes to the providing
    /// entry's signal. Without the read there is nobody to wake, and the tests below would pass for
    /// the wrong reason — the old `mark_subtree_dirty` approach needed no such read, and a test that
    /// skips it cannot tell the two mechanisms apart.
    fn counting_row(
        ctx: &mut ComposeCtx,
        local: &CompositionLocal<i32>,
        runs: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    ) {
        crate::layout::Row::new().build(ctx, move |ctx| {
            runs.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let _ = local.current();
            let k = ctx.next_key();
            ctx.start_leaf(k, crate::modifier::Modifier::new().size(10.0, 10.0));
            ctx.end_node();
        });
    }

    /// The value did NOT move: the readers must be left alone.
    ///
    /// This is the guard for nesting. A provider's memory keyed by the enclosing GROUP (what
    /// `ComposeCtx::position_key` hands out) collides when one group holds two providers — `text.rs`
    /// nests two `ProvideTextStyle` in a single statement, and `alert_dialog.rs` puts
    /// `with_content_color` inside `if let` branches. Each would then read the other's value as "it
    /// changed" and dirty the subtree on EVERY frame, which is a permanent recompose loop. The
    /// per-site key is what stops that; this test is what would catch a regression to the group key.
    #[test]
    fn provide_does_not_touch_readers_when_the_value_is_unchanged() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        // A `static`, so the scene closure stays `Fn` — a `let`-bound local would be moved into the
        // `provide` closure and the scene could only compose once.
        static LOCAL: std::sync::LazyLock<CompositionLocal<i32>> =
            std::sync::LazyLock::new(|| CompositionLocal::new(|| 0i32));
        let runs = Arc::new(AtomicUsize::new(0));
        let make_scene = |runs: Arc<AtomicUsize>| {
            move |ctx: &mut ComposeCtx| {
                LOCAL.provide(7, ctx, |ctx| counting_row(ctx, &LOCAL, runs.clone()));
            }
        };
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(make_scene(runs.clone()));
        assert_eq!(runs.load(Ordering::SeqCst), 1, "the first pass composes it");
        composer.compose(make_scene(runs.clone()));
        assert_eq!(
            runs.load(Ordering::SeqCst),
            1,
            "the value never moved, so the reader must NOT run again — a second run here means the \
             provider is dirtying its subtree every frame"
        );
    }

    /// Two providers nested in ONE group must not share what they remember.
    ///
    /// This is the reason the per-site key exists rather than `ComposeCtx::position_key`, which
    /// resolves to the enclosing GROUP: both providers would then write the same remembered slot,
    /// each would read the other's value back as "it changed", and each would dirty the subtree on
    /// every frame — a recomposition loop that never settles. `text.rs`'s own
    /// `provide_text_style_inherits_and_builder_overrides_typography` test nests two
    /// `ProvideTextStyle` in a single statement, and `alert_dialog.rs` does the same with
    /// `with_content_color`; the invariant below is what keeps those from spinning.
    #[test]
    fn nested_providers_in_one_group_do_not_overwrite_each_others_memory() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        static OUTER: std::sync::LazyLock<CompositionLocal<i32>> =
            std::sync::LazyLock::new(|| CompositionLocal::new(|| 0i32));
        static INNER: std::sync::LazyLock<CompositionLocal<i32>> =
            std::sync::LazyLock::new(|| CompositionLocal::new(|| 0i32));
        let runs = Arc::new(AtomicUsize::new(0));
        let make_scene = |runs: Arc<AtomicUsize>| {
            move |ctx: &mut ComposeCtx| {
                OUTER.provide(1, ctx, |ctx| {
                    INNER.provide(2, ctx, |ctx| {
                        assert_eq!(OUTER.current(), 1, "the outer value still reaches the reader");
                        assert_eq!(INNER.current(), 2, "the inner one shadows nothing");
                        counting_row(ctx, &INNER, runs.clone());
                    });
                });
            }
        };
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(make_scene(runs.clone()));
        assert_eq!(runs.load(Ordering::SeqCst), 1, "the first pass composes it");
        composer.compose(make_scene(runs.clone()));
        composer.compose(make_scene(runs.clone()));
        assert_eq!(
            runs.load(Ordering::SeqCst),
            1,
            "neither value moved, so neither provider may dirty the subtree — more than one run here \
             means the two providers are sharing one remembered slot and are re-dirtying each other \
             every frame"
        );
    }

    // ── `ctx.provide`: several locals at ONE site ──

    /// One call installs every local it describes — Compose's
    /// `CompositionLocalProvider(A provides x, B provides y)`.
    ///
    /// The nesting this replaces spent one slot position per provider, so each needed its own
    /// remembered value and the ORDER carried meaning (the style read the colour that had just been
    /// pushed). One call is one position holding one entry per local, each compared under the policy
    /// of the local it came from.
    #[test]
    fn one_provide_call_installs_every_local_it_describes() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        static NUMBER: std::sync::LazyLock<CompositionLocal<i32>> =
            std::sync::LazyLock::new(|| CompositionLocal::new(|| 0i32));
        static LABEL: std::sync::LazyLock<CompositionLocal<String>> =
            std::sync::LazyLock::new(|| CompositionLocal::new(String::new));

        let runs = Arc::new(AtomicUsize::new(0));
        let make_scene = |runs: Arc<AtomicUsize>| {
            move |ctx: &mut ComposeCtx| {
                ctx.provide(
                    &[NUMBER.provided(7), LABEL.provided("big".to_string())],
                    |ctx| {
                        crate::layout::Row::new().build(ctx, move |ctx| {
                            runs.fetch_add(1, Ordering::SeqCst);
                            assert_eq!(NUMBER.current(), 7, "the first value is in scope");
                            assert_eq!(LABEL.current(), "big", "and so is the second");
                            let k = ctx.next_key();
                            ctx.start_leaf(k, crate::modifier::Modifier::new().size(10.0, 10.0));
                            ctx.end_node();
                        });
                    },
                );
            }
        };
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(make_scene(runs.clone()));
        assert_eq!(runs.load(Ordering::SeqCst), 1, "the first pass composes it");
        composer.compose(make_scene(runs.clone()));
        composer.compose(make_scene(runs.clone()));
        assert_eq!(
            runs.load(Ordering::SeqCst),
            1,
            "neither value moved, so neither entry may dirty the subtree — more than one run here \
             means an entry is comparing against something other than its own previous value"
        );
        // The entries were popped with the call, so both defaults are back.
        assert_eq!(NUMBER.current(), 0);
        assert_eq!(LABEL.current(), "");
    }

    /// Each entry compares under ITS OWN local's policy, not the first one's.
    ///
    /// `new_static` is what tells the entries apart: the same call also carries a `==`-compared
    /// local, and the static one must stay silent even though its payload moved. One shared policy
    /// per call would announce that move, and the reader would re-run for a value it is documented
    /// never to observe.
    #[test]
    fn a_static_local_in_one_provide_call_never_invalidates() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        static FIXED: std::sync::LazyLock<CompositionLocal<i32>> =
            std::sync::LazyLock::new(|| CompositionLocal::new_static(|| 0i32));

        let value = std::cell::Cell::new(1i32);
        let runs = Arc::new(AtomicUsize::new(0));
        let make_scene = |value: &std::cell::Cell<i32>, runs: Arc<AtomicUsize>| {
            let v = value.get();
            move |ctx: &mut ComposeCtx| {
                ctx.provide(&[FIXED.provided(v)], |ctx| {
                    crate::layout::Row::new().build(ctx, move |ctx| {
                        runs.fetch_add(1, Ordering::SeqCst);
                        let k = ctx.next_key();
                        ctx.start_leaf(k, crate::modifier::Modifier::new().size(10.0, 10.0));
                        ctx.end_node();
                    });
                });
            }
        };
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(make_scene(&value, runs.clone()));
        assert_eq!(runs.load(Ordering::SeqCst), 1);

        value.set(2);
        // A `Cell` also keeps the scene free of any `State` read, so nothing else could have queued
        // work. Asserting on the QUEUE rather than on `recompose`'s return value: that return also
        // reflects the composer's initial `needs_recomposition`, which only a `recompose` call
        // clears, so a first pass via `compose` leaves it set for reasons having nothing to do with
        // this provider.
        composer.recompose(make_scene(&value, runs.clone()));
        assert!(
            !composer.has_pending_states(),
            "a `new_static` local never announces a change, so its move must queue nothing"
        );
        assert_eq!(
            runs.load(Ordering::SeqCst),
            1,
            "the reader keeps what it composed with — that is what `staticCompositionLocalOf` means"
        );

        // The value still moved, though: a reader that composes later for its OWN reasons must see
        // the current one. Freezing the value here would be a different (wrong) promise.
        composer.compose(make_scene(&value, runs.clone()));
    }

    /// A move in ONE value of a multi-value provide re-enters the reader, and the reader sees it.
    ///
    /// The neighbour that did NOT move is the control: it stays equal, so its entry must not
    /// notify — an implementation that announced the whole call on any change would still pass a
    /// run count, but would recompose every reader of every local in the call.
    #[test]
    fn a_multi_value_provide_reenters_its_reader_when_one_value_moves() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        static NUMBER: std::sync::LazyLock<CompositionLocal<i32>> =
            std::sync::LazyLock::new(|| CompositionLocal::new(|| 0i32));
        static LABEL: std::sync::LazyLock<CompositionLocal<String>> =
            std::sync::LazyLock::new(|| CompositionLocal::new(String::new));

        let number = crate::runtime::state::State::new(7i32);
        let runs = Arc::new(AtomicUsize::new(0));
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let make_scene = |number: crate::runtime::state::State<i32>,
                          runs: Arc<AtomicUsize>,
                          seen: Arc<std::sync::Mutex<Vec<i32>>>| {
            move |ctx: &mut ComposeCtx| {
                let v = number.get();
                ctx.provide(
                    &[NUMBER.provided(v), LABEL.provided("fixed".to_string())],
                    |ctx| {
                        crate::layout::Row::new().build(ctx, move |ctx| {
                            runs.fetch_add(1, Ordering::SeqCst);
                            seen.lock().unwrap().push(NUMBER.current());
                            let k = ctx.next_key();
                            ctx.start_leaf(k, crate::modifier::Modifier::new().size(10.0, 10.0));
                            ctx.end_node();
                        });
                    },
                );
            }
        };
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(make_scene(number.clone(), runs.clone(), seen.clone()));
        assert_eq!(runs.load(Ordering::SeqCst), 1);

        number.set(9);
        composer.recompose_to_stable(|| make_scene(number.clone(), runs.clone(), seen.clone()));
        assert_eq!(
            runs.load(Ordering::SeqCst),
            2,
            "the moved value must reach the reader"
        );
        assert_eq!(
            seen.lock().unwrap().as_slice(),
            &[7, 9],
            "the reader composed against the old value first and the new one after — anything else \
             means the entry swapped without notifying, or notified without swapping"
        );
    }

    /// A call site's value list may change SHAPE between passes without taking the app down.
    ///
    /// A provider written inside a conditional branch is ordinary code, and the length of its array
    /// follows that branch. Panicking on the mismatch would abort a shipping window for a mistake
    /// whose only visible symptom is a missing colour; entries are matched by LOCAL instead, so a
    /// local joining the list gets a fresh entry seeded with the value it is being given.
    #[test]
    fn a_provide_call_may_change_its_value_list_between_passes() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        static NUMBER: std::sync::LazyLock<CompositionLocal<i32>> =
            std::sync::LazyLock::new(|| CompositionLocal::new(|| 0i32));
        static LABEL: std::sync::LazyLock<CompositionLocal<String>> =
            std::sync::LazyLock::new(|| CompositionLocal::new(String::new));

        // Read in the content closure, NOT inside a container's restartable group: the point of this
        // test is what the stack holds on the pass with the longer list, and a `Row` would Skip on
        // the second pass and replay the first one's value.
        let wide = Arc::new(AtomicBool::new(false));
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let make_scene = |wide: Arc<AtomicBool>,
                          seen: Arc<std::sync::Mutex<Vec<(i32, String)>>>| {
            move |ctx: &mut ComposeCtx| {
                // Both arrays are built every pass and one is selected, so the borrow outlives the
                // call — the LENGTH is what varies, which is the case that used to assert.
                let narrow = [NUMBER.provided(7)];
                let broad = [NUMBER.provided(7), LABEL.provided("wide".to_string())];
                let values = if wide.load(Ordering::SeqCst) {
                    &broad[..]
                } else {
                    &narrow[..]
                };
                let seen = seen.clone();
                ctx.provide(values, move |ctx| {
                    seen.lock().unwrap().push((NUMBER.current(), LABEL.current()));
                    let k = ctx.next_key();
                    ctx.start_leaf(k, crate::modifier::Modifier::new().size(10.0, 10.0));
                    ctx.end_node();
                });
            }
        };
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(make_scene(wide.clone(), seen.clone()));
        assert_eq!(
            seen.lock().unwrap().as_slice(),
            &[(7, String::new())],
            "the narrow list provides only the number, so the label falls back to its default"
        );

        wide.store(true, Ordering::SeqCst);
        composer.compose(make_scene(wide.clone(), seen.clone()));
        assert_eq!(
            seen.lock().unwrap().last().cloned(),
            Some((7, "wide".to_string())),
            "the local that joined the list is on the stack, with the value it was given"
        );
    }

    /// Reordering a call's array is NOT a change.
    ///
    /// Entries are matched by local, so a call site that happens to list its values in another order
    /// must not report every entry as moved. Zipping by position would: each slot would downcast
    /// against another local's payload, fail, and answer "changed" for a list that had not changed at
    /// all — re-entering every reader, on every pass, for nothing.
    #[test]
    fn reordering_a_provide_call_is_not_a_change() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        static NUMBER: std::sync::LazyLock<CompositionLocal<i32>> =
            std::sync::LazyLock::new(|| CompositionLocal::new(|| 0i32));
        static LABEL: std::sync::LazyLock<CompositionLocal<String>> =
            std::sync::LazyLock::new(|| CompositionLocal::new(String::new));

        let flipped = Arc::new(AtomicBool::new(false));
        let make_scene = |flipped: Arc<AtomicBool>| {
            move |ctx: &mut ComposeCtx| {
                let forward = [NUMBER.provided(7), LABEL.provided("x".to_string())];
                let backward = [LABEL.provided("x".to_string()), NUMBER.provided(7)];
                let values = if flipped.load(Ordering::SeqCst) {
                    &backward[..]
                } else {
                    &forward[..]
                };
                ctx.provide(values, |ctx| {
                    // READ both, so the content is subscribed to both: a reordering that looked like
                    // a change would land in the queue, and that is exactly what is asserted below.
                    let _ = (NUMBER.current(), LABEL.current());
                    let k = ctx.next_key();
                    ctx.start_leaf(k, crate::modifier::Modifier::new().size(10.0, 10.0));
                    ctx.end_node();
                });
            }
        };
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(make_scene(flipped.clone()));
        assert!(!composer.has_pending_states(), "the first pass queues nothing");

        flipped.store(true, Ordering::SeqCst);
        composer.compose(make_scene(flipped.clone()));
        assert!(
            !composer.has_pending_states(),
            "the same two locals in a different order are the same values, so nothing may be queued \
             — a queue here means an entry was compared against the OTHER local's payload"
        );
    }

    /// The value DID move: the reader has to run again even though its own group declares nothing.
    ///
    /// The move is consumed over TWO passes, and that is the design rather than a shortcoming of
    /// the test's loop. `Composer::compose` drains the pending queue at its START, before any
    /// content runs; the provider's `notify` therefore cannot dirty anything during the pass that
    /// discovered the move — it lands in the queue and the NEXT pass marks the subscribed scope
    /// dirty. `mark_subtree_dirty`, which this replaces, was same-pass: that immediacy is exactly
    /// what needs no subscription and therefore cannot be precise. Within one frame the difference
    /// is invisible because the frame path is `recompose_until_stable`, which loops until nothing
    /// is pending; what this test guards is that the notify REACHES the reader at all, so a run
    /// count of 1 after the loop below still fails (the edge is missing, not merely late).
    #[test]
    fn provide_reenters_readers_when_the_value_moves() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        static LOCAL: std::sync::LazyLock<CompositionLocal<i32>> =
            std::sync::LazyLock::new(|| CompositionLocal::new(|| 0i32));
        let value = crate::runtime::state::State::new(7i32);
        let runs = Arc::new(AtomicUsize::new(0));
        let make_scene = |value: crate::runtime::state::State<i32>, runs: Arc<AtomicUsize>| {
            move |ctx: &mut ComposeCtx| {
                let v = value.get();
                LOCAL.provide(v, ctx, |ctx| counting_row(ctx, &LOCAL, runs.clone()));
            }
        };
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(make_scene(value.clone(), runs.clone()));
        assert_eq!(runs.load(Ordering::SeqCst), 1);

        value.set(9);
        // The frame path's loop (`recompose_until_stable`): the pass that discovers the move only
        // queues the notify, the next pass acts on it.
        let mut composed_any = false;
        loop {
            let did_compose = composer.recompose(make_scene(value.clone(), runs.clone()));
            composed_any |= did_compose;
            if !did_compose && !composer.has_pending_states() {
                break;
            }
        }
        assert!(composed_any, "the state change must recompose");
        assert_eq!(
            runs.load(Ordering::SeqCst),
            2,
            "the value moved, so the reader must run again — its own group declares nothing, so only \
             the provider's notify can reach it. Still 1 here means the notify never reached the \
             reader that subscribed through `current`"
        );
    }

    /// The mechanism the whole redesign rests on: a signal's `notify` reaches the subscription that
    /// read it.
    ///
    /// `provides` builds one of these per provide site, and `current` hands it to
    /// `register_dependency` exactly as `State::get` does. Everything else in the design is a
    /// consequence of this edge existing — if it did not, readers would still need somebody to call
    /// `mark_subtree_dirty` for them. Pinned at the signal level so it fails here rather than as a
    /// stale colour three layers up.
    #[test]
    fn a_signal_notify_reaches_the_subscription_that_read_it() {
        use crate::runtime::state::{ComposerSubscription, StateId, StateSignal};

        let subscription = ComposerSubscription::new();
        let signal = StateSignal::new(StateId::new(9001));

        // No subscriber yet: nothing to wake, and the read below is what creates the edge.
        signal.notify(false);
        assert!(
            subscription.pending_ids().is_empty(),
            "an unread signal must not queue anything"
        );

        assert!(
            subscription.subscribe_signal(&signal).is_some(),
            "the subscription accepts a live signal"
        );
        assert!(
            subscription.pending_ids().is_empty(),
            "subscribing alone queues no work"
        );

        // `wake = false`: no event-loop proxy in a test, and this test is about the queue.
        signal.notify(false);
        assert_eq!(
            subscription.pending_ids(),
            vec![signal.id()],
            "the notify must land on the subscription that read the signal — this is the edge that \
             replaces `mark_subtree_dirty`"
        );
    }

    /// A provided value round-trips through the stack entry that now carries its signal.
    #[test]
    fn a_provided_value_keeps_its_type_and_its_signal() {
        let local = CompositionLocal::new(|| 0i32);
        let seen = local.provides(7, || {
            let supplied = SLOTS.with(|s| s.borrow().last().cloned());
            let (id, supplied) = supplied.expect("the value is on the stack");
            assert_eq!(id, local.id, "the entry belongs to this local");
            assert!(
                supplied.signal.is_none(),
                "the ctx-less provides invalidates nothing, so it carries no signal"
            );
            local.current()
        });
        assert_eq!(seen, 7, "the value reads back through its type");
    }
}
