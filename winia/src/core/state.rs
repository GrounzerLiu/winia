//! 响应式状态系统 — 类似 Jetpack Compose 的 MutableState
//!
//! 核心概念:
//! - State<T>: 可观察的值容器，读时自动追踪依赖，写时通知重组
//! - 基于 thread-local 的依赖追踪，无需显式传递 CompositionContext
//! - 通过 PartialEq 去重，避免无效重组
//! - 通知走 StateSignal（按读取订阅的 Composer）+ notify_version

use parking_lot::{Mutex, RwLock};
use std::fmt::{Debug, Display, Formatter};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Weak};

// ── State identity and Composer invalidation queue ──

/// Opaque stable identity for StateSignal routing and dependency maps.
///
/// The public animation API still accepts the legacy u32 returned by
/// State::id(); new callers should use State::state_id() for the u64-backed ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StateId(u64);

impl StateId {
    pub(crate) const fn new(raw: u64) -> Self {
        Self(raw)
    }

    /// Return the raw u64 value for diagnostics and persistence.
    pub const fn raw(self) -> u64 {
        self.0
    }
}


/// Per-Composer invalidation queue. State creation is intentionally ownerless;
/// a Composer subscribes when its compose/layout code reads a State.
pub(crate) struct ComposerSubscription {
    id: u64,
    pending: Mutex<Vec<StateId>>,
    /// Signals subscribed during the current and partial frames. Weak entries
    /// let Composer drop clean up even when composition unwinds through panic.
    signals: Mutex<Vec<Weak<StateSignal>>>,
    /// Once a Composer drops, no in-flight read may create a new subscription.
    closed: std::sync::atomic::AtomicBool,
}

static NEXT_SUBSCRIPTION_ID: AtomicU64 = AtomicU64::new(1);

impl ComposerSubscription {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            id: NEXT_SUBSCRIPTION_ID.fetch_add(1, Ordering::Relaxed),
            pending: Mutex::new(Vec::new()),
            signals: Mutex::new(Vec::new()),
            closed: std::sync::atomic::AtomicBool::new(false),
        })
    }

    pub(crate) fn id(&self) -> u64 {
        self.id
    }

    /// Subscribe this live Composer to a signal and return its revision snapshot.
    /// The tracking lock serializes registration with Composer drop cleanup.
    pub(crate) fn subscribe_signal(self: &Arc<Self>, signal: &Arc<StateSignal>) -> Option<u64> {
        let mut signals = self.signals.lock();
        if self.closed.load(Ordering::Acquire) {
            return None;
        }
        signals.retain(|weak| weak.upgrade().is_some());
        let queue = Arc::downgrade(self);
        let observed_revision = signal.subscribe(queue.clone());
        if observed_revision.is_some()
            && !signals.iter().any(|weak| weak.upgrade().is_some_and(|item| item.id() == signal.id()))
        {
            signals.push(Arc::downgrade(signal));
        }
        observed_revision
    }

    pub(crate) fn unsubscribe_all(self: &Arc<Self>) {
        // Keep the tracking lock while removing signal-side entries so a
        // concurrent read cannot add a new subscription during Composer drop.
        let mut signals = self.signals.lock();
        self.closed.store(true, Ordering::Release);
        let tracked = std::mem::take(&mut *signals);
        for weak in tracked {
            if let Some(signal) = weak.upgrade() {
                signal.unsubscribe(self.id);
            }
        }
        self.pending.lock().clear();
    }

    fn signal_ids(&self) -> std::collections::HashSet<StateId> {
        self.signals
            .lock()
            .iter()
            .filter_map(|weak| weak.upgrade().map(|signal| signal.id()))
            .collect()
    }

    /// Reconcile tracked signals after a frame, including reads recorded before
    /// a panic. Keep the tracking lock through removal so a concurrent read cannot
    /// re-add a signal before its old subscription is removed.
    pub(crate) fn retain_signals(&self, live_ids: &std::collections::HashSet<StateId>) {
        let mut signals = self.signals.lock();
        let mut removed = Vec::new();
        signals.retain(|weak| {
            let Some(signal) = weak.upgrade() else { return false };
            if live_ids.contains(&signal.id()) {
                true
            } else {
                removed.push(signal);
                false
            }
        });
        for signal in removed {
            signal.unsubscribe(self.id);
        }
    }

    /// Enqueue a notification if this Composer subscription is still live.
    /// Returns false when the queue was closed, allowing StateSignal to avoid a
    /// wake for a queue that raced with Composer teardown.
    pub(crate) fn enqueue(&self, state_id: StateId) -> bool {
        let mut pending = self.pending.lock();
        if self.closed.load(Ordering::Acquire) {
            return false;
        }
        if !pending.contains(&state_id) {
            pending.push(state_id);
        }
        true
    }

    pub(crate) fn drain(&self) -> Vec<StateId> {
        std::mem::take(&mut *self.pending.lock())
    }

    pub(crate) fn pending_ids(&self) -> Vec<StateId> {
        self.pending.lock().clone()
    }

    /// Requeue a snapshot after a failed frame, preserving notifications that
    /// arrived while the frame was running.
    pub(crate) fn restore_pending(&self, ids: &[StateId]) {
        let mut pending = self.pending.lock();
        if self.closed.load(Ordering::Acquire) {
            return;
        }
        for &state_id in ids {
            if !pending.contains(&state_id) {
                pending.push(state_id);
            }
        }
    }

    pub(crate) fn drain_matching(&self, mut predicate: impl FnMut(StateId) -> bool) -> Vec<StateId> {
        let mut pending = self.pending.lock();
        let mut drained = Vec::new();
        pending.retain(|&state_id| {
            if predicate(state_id) {
                drained.push(state_id);
                false
            } else {
                true
            }
        });
        drained
    }

    /// Atomically classify layout reads and remove only IDs that do not also
    /// require composition. Notifications arriving after this lock are next-batch.
    pub(crate) fn drain_non_compose_collect_layout(
        &self,
        compose_ids: &std::collections::HashSet<StateId>,
        layout_ids: &std::collections::HashSet<StateId>,
    ) -> Vec<StateId> {
        let mut pending = self.pending.lock();
        let mut layout_pending = Vec::new();
        pending.retain(|&state_id| {
            if layout_ids.contains(&state_id) {
                layout_pending.push(state_id);
            }
            compose_ids.contains(&state_id)
        });
        layout_pending
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.pending.lock().is_empty()
    }

    pub(crate) fn len(&self) -> usize {
        self.pending.lock().len()
    }
}

struct Subscriber {
    id: u64,
    queue: Weak<ComposerSubscription>,
}

/// A State's signal owns its subscribers. There is no global state-id -> owner
/// map, so an unowned State cannot accidentally notify a stale Composer.
pub(crate) struct StateSignal {
    id: StateId,
    /// Monotonic read/notify handshake. Subscription and the revision snapshot
    /// are linearized under the subscriber lock before the value is read.
    revision: AtomicU64,
    subscribers: Mutex<Vec<Subscriber>>,
}

impl StateSignal {
    pub(crate) fn id(&self) -> StateId {
        self.id
    }

    pub(crate) fn current_revision(&self) -> u64 {
        self.revision.load(Ordering::Acquire)
    }

    fn new(id: StateId) -> Arc<Self> {
        Arc::new(Self {
            id,
            revision: AtomicU64::new(0),
            subscribers: Mutex::new(Vec::new()),
        })
    }

    fn subscribe(&self, queue: Weak<ComposerSubscription>) -> Option<u64> {
        let Some(queue_ref) = queue.upgrade() else {
            return None;
        };
        let id = queue_ref.id();
        let mut subscribers = self.subscribers.lock();
        subscribers.retain(|entry| entry.queue.upgrade().is_some());
        if !subscribers.iter().any(|entry| entry.id == id) {
            subscribers.push(Subscriber { id, queue });
        }
        // The notification revision is read while holding the same lock that
        // notify() uses to increment and snapshot subscribers.
        Some(self.revision.load(Ordering::Acquire))
    }

    fn enqueue_if_changed(&self, queue: &ComposerSubscription, observed_revision: u64) {
        let mut subscribers = self.subscribers.lock();
        subscribers.retain(|entry| entry.queue.upgrade().is_some());
        let is_subscribed = subscribers.iter().any(|entry| entry.id == queue.id());
        if is_subscribed && self.revision.load(Ordering::Acquire) != observed_revision {
            // Keep the post-read handshake linearized with unsubscribe and notify.
            if !queue.enqueue(self.id) {
                subscribers.retain(|entry| entry.id != queue.id());
            }
        }
    }

    pub(crate) fn unsubscribe(&self, subscription_id: u64) {
        self.subscribers
            .lock()
            .retain(|entry| entry.id != subscription_id);
    }

    fn notify(&self, wake: bool) {
        let has_live_queues = {
            let mut subscribers = self.subscribers.lock();
            self.revision.fetch_add(1, Ordering::AcqRel);
            let mut live_entries = Vec::with_capacity(subscribers.len());
            let mut has_live_queues = false;
            for entry in subscribers.drain(..) {
                if let Some(queue) = entry.queue.upgrade() {
                    // Keep enqueue linearized with unsubscribe. enqueue() only
                    // takes the queue mutex; wake_loop remains outside this lock.
                    if queue.enqueue(self.id) {
                        has_live_queues = true;
                        live_entries.push(entry);
                    }
                }
            }
            *subscribers = live_entries;
            has_live_queues
        };

        // Subscription and notify linearize on the signal lock, so a queue
        // removed by cleanup cannot receive a late snapshot notification.
        if wake && has_live_queues {
            wake_loop();
        }
    }
}

// ── RawState<T> (crate-internal storage shared by all handles) ──
//
// Every public handle below is a #[repr(transparent)] wrapper over RawState.
// Handles differ ONLY in which methods they expose; the data (Arc + signal +
// value) is identical. Engine internals (animation ticks, write-back) operate
// on RawState directly so user-facing clipping can never block a required
// internal write.
//
// Scheduling semantics carried by each handle (see docs/state-handles.md):
// - Reactive:    recompose + wake event loop (default composition state)
// - Animating:   recompose, no wake (animation ticks; redraw already requested)
// - Visual:      write only, no recompose (draw-layer props: alpha/scale)
// - Backchannel: write only, no notify (measure/layout write-back, staging)

pub(crate) struct RawState<T> {
    pub(crate) inner: Arc<StateInner<T>>,
}

impl<T> Clone for RawState<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T> RawState<T> {
    pub(crate) fn take_notify_version(&self) -> u32 {
        self.inner.notify_version.swap(0, Ordering::AcqRel)
    }

    pub(crate) fn id(&self) -> u32 {
        self.inner.public_id
    }

    pub(crate) fn state_id(&self) -> StateId {
        self.inner.signal.id()
    }

    pub(crate) fn signal_id(&self) -> StateId {
        self.state_id()
    }

    pub(crate) fn notify(&self, wake: bool) {
        self.inner.notify_version.fetch_add(1, Ordering::Release);
        self.inner.signal.notify(wake);
    }
}

impl<T: 'static> RawState<T> {
    pub(crate) fn new(value: T) -> Self {
        let id = next_state_id();
        let public_id = next_public_state_id();
        let inner = Arc::new(StateInner {
            signal: StateSignal::new(id),
            public_id,
            value: RwLock::new(value),
            notify_version: Default::default(),
        });
        Self { inner }
    }

    pub(crate) fn get_tracked(&self) -> T
    where
        T: Clone,
    {
        let registration = register_dependency(self.inner.signal.clone());
        let value = self.inner.value.read().clone();
        if let Some((queue, observed_revision)) = registration {
            self.inner
                .signal
                .enqueue_if_changed(&queue, observed_revision);
        }
        value
    }

    pub(crate) fn peek_untracked(&self) -> T
    where
        T: Clone,
    {
        self.inner.value.read().clone()
    }

    /// Reactive write: dedup + notify + wake (default composition state).
    pub(crate) fn set_reactive(&self, value: T)
    where
        T: PartialEq,
    {
        let mut current = self.inner.value.write();
        if *current == value {
            return;
        }
        *current = value;
        drop(current);
        self.notify(true);
    }

    /// In-place mutation with dedup: clone-compare-swap so no-op updates
    /// (e.g. `|v| *v += 0`) skip notification like `set` does.
    pub(crate) fn update_reactive(&self, f: impl FnOnce(&mut T))
    where
        T: Clone + PartialEq,
    {
        // Fast path: mutate a clone, compare, swap only on change.
        // Holds no lock while running user code.
        let mut staged = self.inner.value.read().clone();
        f(&mut staged);
        let mut current = self.inner.value.write();
        if *current == staged {
            return;
        }
        *current = staged;
        drop(current);
        self.notify(true);
    }

    /// Animation-tick write: dedup + notify, skip WAKE_FN (redraw already
    /// requested by the animation engine; waking would spin recomposition).
    pub(crate) fn set_animating(&self, value: T)
    where
        T: PartialEq,
    {
        let mut current = self.inner.value.write();
        if *current == value {
            return;
        }
        *current = value;
        drop(current);
        self.notify(false);
    }

    /// Draw-layer write: no dedup, no notify, no wake. The animation engine
    /// drives repaint via `request_redraw`; values are consumed via `peek`.
    pub(crate) fn set_visual(&self, value: T) {
        *self.inner.value.write() = value;
    }

    /// Write-back write: no dedup, no notify, no wake. For measure/layout
    /// write-back and cross-frame staging; the next frame reads the value.
    pub(crate) fn set_backchannel(&self, value: T) {
        *self.inner.value.write() = value;
    }

}

// ── Public handles ──
//
// Each handle is a zero-cost transparent wrapper over RawState exposing
// exactly one scheduling semantic. Downgrade-only conversions
// (Reactive -> Animating/Visual/Backchannel) are allowed; upgrade back
// requires framework internals holding the RawState.

/// Default composition state: reads subscribe (`get`), writes recompose +
/// wake the event loop (`set` / `update`).
#[repr(transparent)]
pub struct Reactive<T>(RawState<T>);

/// Animation-tick state: writes enqueue recomposition without waking the
/// event loop (redraw is already requested by the animation engine).
#[repr(transparent)]
pub struct Animating<T>(RawState<T>);

/// Draw-layer state: writes land without recomposition; consumed via `peek`
/// at render time (alpha/scale and other purely visual props).
#[repr(transparent)]
pub struct Visual<T>(RawState<T>);

/// Write-back channel: writes land without notify or wake; the next frame
/// reads the value (measure/layout write-back, cross-frame staging).
#[repr(transparent)]
pub struct Backchannel<T>(RawState<T>);

macro_rules! impl_handle_common {
    ($H:ident) => {
        impl<T> Clone for $H<T> {
            fn clone(&self) -> Self {
                Self(self.0.clone())
            }
        }

        impl<T: Debug> Debug for $H<T> {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                f.debug_struct(stringify!($H))
                    .field("id", &self.0.id())
                    .field("value", &*self.0.inner.value.read())
                    .finish()
            }
        }

        impl<T> $H<T> {
            pub fn id(&self) -> u32 {
                self.0.id()
            }

            pub fn state_id(&self) -> StateId {
                self.0.state_id()
            }

            pub(crate) fn signal_id(&self) -> StateId {
                self.0.signal_id()
            }

            pub(crate) fn take_notify_version(&self) -> u32 {
                self.0.take_notify_version()
            }

            pub(crate) fn from_raw(raw: RawState<T>) -> Self {
                Self(raw)
            }

            pub(crate) fn as_raw(&self) -> &RawState<T> {
                &self.0
            }
        }

        impl<T: PartialEq> PartialEq for $H<T> {
            fn eq(&self, other: &Self) -> bool {
                // Identity semantics: same signal means same state, even if a
                // concurrent write changed the value between the two reads.
                self.0.state_id() == other.0.state_id()
            }
        }

        impl<T> Eq for $H<T> where T: Eq {}
    };
}

impl_handle_common!(Reactive);
impl_handle_common!(Animating);
impl_handle_common!(Visual);
impl_handle_common!(Backchannel);

impl<T: 'static> Reactive<T> {
    pub fn new(value: T) -> Self {
        Self(RawState::new(value))
    }
}

impl<T: Clone + 'static> Reactive<T> {
    pub fn get(&self) -> T {
        self.0.get_tracked()
    }

    pub fn peek(&self) -> T {
        self.0.peek_untracked()
    }
}

impl<T: PartialEq + 'static> Reactive<T> {
    pub fn set(&self, value: T) {
        self.0.set_reactive(value);
    }

    pub fn update(&self, f: impl FnOnce(&mut T))
    where
        T: Clone,
    {
        self.0.update_reactive(f);
    }
}

impl<T> Reactive<T> {
    /// Downgrade to an animation-tick handle (recompose without wake).
    pub fn into_animating(self) -> Animating<T> {
        Animating(self.0)
    }

    /// Downgrade to a draw-layer handle (write without recompose).
    pub fn into_visual(self) -> Visual<T> {
        Visual(self.0)
    }

    /// Downgrade to a write-back channel (write without notify).
    pub fn into_backchannel(self) -> Backchannel<T> {
        Backchannel(self.0)
    }

    /// Downgrade by reference (keep the Reactive, share the new handle).
    pub fn as_animating(&self) -> Animating<T> {
        Animating(self.0.clone())
    }

    /// Downgrade by reference (keep the Reactive, share the new handle).
    pub fn as_visual(&self) -> Visual<T> {
        Visual(self.0.clone())
    }

    /// Downgrade by reference (keep the Reactive, share the new handle).
    pub fn as_backchannel(&self) -> Backchannel<T> {
        Backchannel(self.0.clone())
    }
}

impl<T: 'static> Animating<T> {
    pub fn new(value: T) -> Self {
        Self(RawState::new(value))
    }
}

impl<T: Clone + 'static> Animating<T> {
    pub fn get(&self) -> T {
        self.0.get_tracked()
    }

    pub fn peek(&self) -> T {
        self.0.peek_untracked()
    }
}

impl<T: PartialEq + 'static> Animating<T> {
    pub fn set(&self, value: T) {
        self.0.set_animating(value);
    }
}

impl<T: 'static> Animating<T> {
    /// Share the storage back as a legacy `State` handle (migration bridge
    /// for `animate_*_as_state` callers still on `State`; same signal).
    pub(crate) fn into_state(self) -> State<T> {
        State::from_raw(self.0)
    }
}

impl<T: 'static> Visual<T> {
    pub fn new(value: T) -> Self {
        Self(RawState::new(value))
    }
}

impl<T: Clone + 'static> Visual<T> {
    pub fn peek(&self) -> T {
        self.0.peek_untracked()
    }
}

impl<T: 'static> Visual<T> {
    pub fn set(&self, value: T) {
        self.0.set_visual(value);
    }
}

impl<T: 'static> Backchannel<T> {
    pub fn new(value: T) -> Self {
        Self(RawState::new(value))
    }
}

impl<T: Clone + 'static> Backchannel<T> {
    pub fn peek(&self) -> T {
        self.0.peek_untracked()
    }

    /// Measure-phase read: registers a **layout** dependency when called
    /// during measurement (re-measure on change, no recompose), and a compose
    /// dependency when called during composition. Identical tracking to
    /// `Reactive::get` — the handle boundary only governs *writes*.
    pub fn get(&self) -> T {
        self.0.get_tracked()
    }
}

impl<T: 'static> Backchannel<T> {
    pub fn set(&self, value: T) {
        self.0.set_backchannel(value);
    }
}

impl<T: Display> Display for Reactive<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&*self.0.inner.value.read(), f)
    }
}

/// Default composition state handle.
///
/// `State` is the `Reactive` scheduling semantic: reads subscribe (`get`),
/// writes recompose + wake the event loop (`set` / `update`). It is a
/// `#[repr(transparent)]` wrapper over the shared `RawState` storage — see
/// `Reactive` for the full contract. `State` exists as the ergonomic default;
/// the other handles (`Animating`, `Visual`, `Backchannel`) cover narrower
/// scheduling needs (see `docs/state-handles.md`).
///
/// State creation is ownerless: only `get()` inside a compose/layout context
/// subscribes the current Composer, so composite constructors need no implicit
/// owner TLS.
pub struct State<T> {
    pub(crate) raw: RawState<T>,
}

struct StateInner<T> {
    signal: Arc<StateSignal>,
    /// Transitional u32 identity kept for animation/public compatibility.
    /// Internal Composer routing uses `signal.id()` exclusively.
    public_id: u32,
    value: RwLock<T>,
    /// 通知版本号——每次 set/update 自增，compose 消费后归零
    notify_version: AtomicU32,
}

// Internal routing IDs are 64-bit; the public animation compatibility ID
// remains a separate u32 sequence until that API is migrated.
static NEXT_STATE_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_PUBLIC_STATE_ID: AtomicU32 = AtomicU32::new(1);

fn next_state_id() -> StateId {
    StateId::new(NEXT_STATE_ID.fetch_add(1, Ordering::Relaxed))
}

fn next_public_state_id() -> u32 {
    NEXT_PUBLIC_STATE_ID.fetch_add(1, Ordering::Relaxed)
}

impl<T: 'static> State<T> {
    /// Create a new ownerless state value.
    pub fn new(value: T) -> Self {
        Self {
            raw: RawState::new(value),
        }
    }

    pub(crate) fn as_raw(&self) -> &RawState<T> {
        &self.raw
    }

    pub(crate) fn from_raw(raw: RawState<T>) -> Self {
        Self { raw }
    }

    pub(crate) fn into_reactive(self) -> Reactive<T> {
        Reactive::from_raw(self.raw)
    }

    pub fn as_backchannel(&self) -> Backchannel<T>
    where
        T: Clone,
    {
        Backchannel::from_raw(self.raw.clone())
    }

    pub(crate) fn into_animating(self) -> Animating<T> {
        Animating::from_raw(self.raw)
    }

    pub(crate) fn into_visual(self) -> Visual<T> {
        Visual::from_raw(self.raw)
    }

    pub(crate) fn into_backchannel(self) -> Backchannel<T> {
        Backchannel::from_raw(self.raw)
    }


}

impl<T: Clone + 'static> State<T> {
    pub(crate) fn take_notify_version(&self) -> u32 {
        self.raw.take_notify_version()
    }

    /// Read a snapshot of the current value.
    ///
    /// When called inside a composition context (a Composer executing a
    /// composable), registers a dependency: when this State changes, the
    /// corresponding composable is marked for recomposition.
    pub fn get(&self) -> T {
        self.raw.get_tracked()
    }

    /// Read without registering a dependency (render phase / animation engine
    /// internals — avoids recording the dependency at the creation site).
    pub fn peek(&self) -> T {
        self.raw.peek_untracked()
    }
}

impl<T: PartialEq + 'static> State<T> {
    /// Set a new value. Equal values (via PartialEq) skip notification,
    /// avoiding useless recomposition.
    pub fn set(&self, value: T) {
        self.raw.set_reactive(value);
    }

    /// Write-back without notify/recompose. For internal markers whose change
    /// needs no reactivity (e.g. Window `created_id`: read naturally next
    /// frame; notifying mid-compose would avalanche keys / collapse the tree).
    ///
    /// Deprecated: prefer an explicit `Backchannel` handle (see
    /// `docs/state-handles.md`). Emits a compile warning on use.
    #[deprecated(note = "use Backchannel::set; see docs/state-handles.md")]
    pub fn set_silent(&self, value: T) {
        self.raw.set_backchannel(value);
    }

    /// Set + notify (schedule recomposition) but **skip waking the event
    /// loop** (bypass WAKE_FN).
    ///
    /// Animation-engine only: ticks are already frame-driven by
    /// `request_redraw`; waking per tick would spin recomposition.
    ///
    /// Deprecated: prefer an explicit `Animating` handle (see
    /// `docs/state-handles.md`). Emits a compile warning on use.
    #[deprecated(note = "use Animating::set; see docs/state-handles.md")]
    pub fn set_no_wake(&self, value: T) {
        self.raw.set_animating(value);
    }

    /// Write without recomposition.
    ///
    /// Draw-layer animation only: alpha/scale/color changes repaint via the
    /// animation engine's per-frame `request_redraw` and never schedule
    /// `notify -> mark_dirty -> recompose` (matches Compose `graphicsLayer`).
    ///
    /// Deprecated: prefer an explicit `Visual` handle (see
    /// `docs/state-handles.md`). Emits a compile warning on use.
    #[deprecated(note = "use Visual::set; see docs/state-handles.md")]
    pub fn set_visual(&self, value: T) {
        self.raw.set_visual(value);
    }
}

impl<T: Clone + PartialEq + 'static> State<T> {
    /// Mutate in place with dedup: no-op mutations skip notification like
    /// `set` does (clone-compare-swap; no lock held while running `f`).
    pub fn update(&self, f: impl FnOnce(&mut T)) {
        self.raw.update_reactive(f);
    }

    /// Legacy always-notify in-place mutation (pre-handles behavior).
    /// Prefer `update` (deduped). Kept for call sites that rely on the
    /// unconditional notify (e.g. wrapping-add pulse counters).
    pub(crate) fn update_untracked(&self, f: impl FnOnce(&mut T)) {
        let mut current = self.raw.inner.value.write();
        f(&mut *current);
        drop(current);
        self.raw.notify(true);
    }
}

impl<T: 'static> State<T> {
    /// Legacy u32 identity (animation/public compatibility).
    pub fn id(&self) -> u32 {
        self.raw.id()
    }

    /// Return the u64-backed identity used by StateSignal and dependency graphs.
    /// This is the migration API for callers that must not rely on the legacy u32 ID.
    pub fn state_id(&self) -> StateId {
        self.raw.state_id()
    }

    /// Internal alias used by Composer dependency maps.
    pub(crate) fn signal_id(&self) -> StateId {
        self.raw.signal_id()
    }

    pub(crate) fn notify(&self, wake: bool) {
        self.raw.notify(wake);
    }
}

impl<T> Clone for State<T> {
    fn clone(&self) -> Self {
        Self {
            raw: self.raw.clone(),
        }
    }
}

impl<T: Debug> Debug for State<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("State")
            .field("id", &self.raw.id())
            .field("value", &*self.raw.inner.value.read())
            .finish()
    }
}

impl<T: Display> Display for State<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&*self.raw.inner.value.read(), f)
    }
}

impl<T: PartialEq> PartialEq for State<T> {
    fn eq(&self, other: &Self) -> bool {
        // Identity semantics: same signal means same state, even if a
        // concurrent write changed the value between the two reads.
        self.raw.state_id() == other.raw.state_id()
    }
}

impl<T> Eq for State<T> where T: Eq {}

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
enum DepMode {
    None,
    Compose,
    Layout,
}

struct DependencyFrame {
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

fn push_dependency_frame(mode: DepMode, recorder_queue: Option<Weak<ComposerSubscription>>) -> DependencyFrameGuard {
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

fn restore_dependency_frame(frame_id: u64) -> bool {
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

pub(crate) fn begin_compose_deps() -> DependencyFrameGuard {
    push_dependency_frame(DepMode::Compose, None)
}

/// Composer composition entry: subscribe reads to this Composer's queue.
pub(crate) fn begin_compose_deps_with_queue(queue: Weak<ComposerSubscription>) -> DependencyFrameGuard {
    push_dependency_frame(DepMode::Compose, Some(queue))
}

pub(crate) fn begin_layout_deps() -> DependencyFrameGuard {
    push_dependency_frame(DepMode::Layout, None)
}

/// Begin layout dependency recording for a live Composer subscription.
pub(crate) fn begin_layout_deps_with_queue(queue: Weak<ComposerSubscription>) -> DependencyFrameGuard {
    push_dependency_frame(DepMode::Layout, Some(queue))
}

/// Finish recording and move the current buffer out without touching the outer frame.
pub(crate) fn take_deps() -> Vec<(Arc<StateSignal>, u64)> {
    DEP_MODE.with(|m| m.set(DepMode::None));
    DEP_BUFFER.with(|b| std::mem::take(&mut *b.borrow_mut()))
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
    crate::core::composer::with_active_scope(|key| {
        registration = record_dep(signal, key);
    });
    registration
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
    fn state_id_is_internal_newtype_with_public_compatibility() {
        let state = State::new(7i32);
        let other = State::new(8i32);
        let internal = state.signal_id();
        assert_eq!(state.state_id(), internal);
        assert_eq!(state.raw.inner.public_id, state.id());
        assert_ne!(state.id(), other.id(), "public compatibility IDs remain distinct");
        assert_ne!(internal, other.signal_id());
        assert_eq!(internal.raw(), state.state_id().raw());
        assert_eq!(StateId::new(u64::MAX).raw(), u64::MAX);
        assert_ne!(internal, StateId::new(internal.raw().wrapping_add(1)));
    }

    #[test]
    fn test_backchannel_write_lands_without_notify() {
        let queue = ComposerSubscription::new();
        let channel = Backchannel::new(0i32);
        channel.get();
        let mut frame = begin_compose_deps_with_queue(Arc::downgrade(&queue));
        channel.get();
        take_deps();
        frame.commit();

        channel.set(42);
        assert_eq!(channel.get(), 42);
        assert!(queue.is_empty(), "Backchannel::set must not enqueue");
    }

    #[test]
    fn unread_state_does_not_notify() {
        let queue = ComposerSubscription::new();
        let state = State::new(0i32);
        state.set(1);
        assert!(queue.is_empty(), "未读取的 State 不应通知 Composer");
    }

    #[test]
    fn revision_handshake_catches_write_after_subscription() {
        let queue = ComposerSubscription::new();
        let state = State::new(0i32);
        let signal = state.raw.inner.signal.clone();
        let observed = queue.subscribe_signal(&signal).unwrap();

        // Simulate a write in the gap between the subscription snapshot and
        // the post-read check. The check must preserve the invalidation even
        // when the caller has not yet finished reading the value.
        state.set(1);
        signal.enqueue_if_changed(&queue, observed);

        assert_eq!(queue.drain(), vec![state.signal_id()]);
    }

    #[test]
    fn revision_handshake_does_not_requeue_after_unsubscribe() {
        let queue = ComposerSubscription::new();
        let state = State::new(0i32);
        let signal = state.raw.inner.signal.clone();
        let observed = queue.subscribe_signal(&signal).unwrap();

        queue.unsubscribe_all();
        state.set(1);
        signal.enqueue_if_changed(&queue, observed);

        assert!(queue.is_empty(), "取消订阅后 handshake 不应重新入队");
    }

    #[test]
    fn closed_subscription_cannot_readd_signal() {
        let queue = ComposerSubscription::new();
        let state = State::new(0i32);
        let signal = state.raw.inner.signal.clone();
        assert!(queue.subscribe_signal(&signal).is_some());
        queue.enqueue(state.signal_id());

        queue.unsubscribe_all();
        assert!(queue.is_empty(), "closed Composer queue must discard stale pending IDs");
        assert!(!queue.enqueue(state.signal_id()));
        assert!(queue.subscribe_signal(&signal).is_none());
        state.set(1);
        assert!(queue.is_empty(), "关闭后的 Composer queue 不应重新收到通知");
        assert!(signal.subscribers.lock().is_empty(), "notify 应清理 closed subscriber");
    }

    /// State 创建无 owner；两个 Composer 读取同一 State 后都订阅，重复读取去重。
    #[test]
    fn drain_non_compose_preserves_mixed_compose_pending() {
        let queue = ComposerSubscription::new();
        let compose_state = State::new(1i32);
        let layout_state = State::new(2i32);
        let unknown = StateId::new(u64::MAX);

        queue.enqueue(compose_state.signal_id());
        queue.enqueue(layout_state.signal_id());
        queue.enqueue(unknown);

        let compose_ids = std::collections::HashSet::from([compose_state.signal_id()]);
        let layout_ids = std::collections::HashSet::from([
            compose_state.signal_id(),
            layout_state.signal_id(),
        ]);
        let layout_pending = queue.drain_non_compose_collect_layout(&compose_ids, &layout_ids);

        assert_eq!(
            layout_pending,
            vec![compose_state.signal_id(), layout_state.signal_id()],
            "mixed compose/layout IDs must mark layout without being consumed from compose"
        );
        assert_eq!(queue.pending_ids(), vec![compose_state.signal_id()]);
    }

    #[test]
    fn cross_composer_subscription_fan_out_and_dedup() {
        let state = State::new(0i32);
        let main = ComposerSubscription::new();
        let overlay = ComposerSubscription::new();

        let mut main_frame = begin_compose_deps_with_queue(Arc::downgrade(&main));
        state.get();
        state.get();
        take_deps();
        main_frame.commit();

        let mut overlay_frame = begin_compose_deps_with_queue(Arc::downgrade(&overlay));
        state.get();
        take_deps();
        overlay_frame.commit();

        state.set(1);
        assert_eq!(main.drain(), vec![state.signal_id()]);
        assert_eq!(overlay.drain(), vec![state.signal_id()]);

        // Re-reading the same signal must not duplicate the subscriber.
        let mut reread_frame = begin_compose_deps_with_queue(Arc::downgrade(&overlay));
        state.get();
        take_deps();
        reread_frame.commit();
        state.set(2);
        assert_eq!(overlay.drain(), vec![state.signal_id()]);

        drop(overlay);
        state.set(3);
        assert_eq!(main.drain(), vec![state.signal_id()]);
    }

    #[test]
    fn explicit_unsubscribe_stops_notifications() {
        let queue = ComposerSubscription::new();
        let state = State::new(0i32);
        let mut frame = begin_compose_deps_with_queue(Arc::downgrade(&queue));
        state.get();
        take_deps();
        frame.commit();

        queue.unsubscribe_all();
        state.set(1);
        assert!(queue.is_empty(), "取消订阅后不应再收到 State 通知");
    }

    #[test]
    fn retain_signals_removes_stale_reads() {
        let queue = ComposerSubscription::new();
        let state = State::new(0i32);
        let mut frame = begin_compose_deps_with_queue(Arc::downgrade(&queue));
        state.get();
        take_deps();
        frame.commit();

        let live = std::collections::HashSet::new();
        queue.retain_signals(&live);
        state.set(1);
        assert!(queue.is_empty(), "读取集合移除后不应保留旧订阅");
    }

    #[test]
    fn nested_dependency_frames_restore_outer_reads() {
        let outer_a = State::new(1i32);
        let inner_b = State::new(2i32);
        let outer_c = State::new(3i32);
        let outer_queue = ComposerSubscription::new();
        let inner_queue = ComposerSubscription::new();

        let mut outer = begin_compose_deps_with_queue(Arc::downgrade(&outer_queue));
        assert_eq!(outer_a.get(), 1);

        let mut inner = begin_layout_deps_with_queue(Arc::downgrade(&inner_queue));
        assert_eq!(inner_b.get(), 2);
        let inner_deps = take_deps();
        assert_eq!(inner_deps.len(), 1);
        assert_eq!(inner_deps[0].0.id(), inner_b.signal_id());
        inner.commit();

        assert_eq!(outer_c.get(), 3);
        let outer_deps = take_deps();
        assert_eq!(outer_deps.len(), 2);
        assert!(outer_deps.iter().any(|(signal, _)| signal.id() == outer_a.signal_id()));
        assert!(outer_deps.iter().any(|(signal, _)| signal.id() == outer_c.signal_id()));
        outer.commit();

        inner_b.set(4);
        assert_eq!(inner_queue.drain(), vec![inner_b.signal_id()]);
        outer_a.set(5);
        outer_c.set(6);
        assert_eq!(outer_queue.drain(), vec![outer_a.signal_id(), outer_c.signal_id()]);
    }

    #[test]
    fn panic_drops_inner_frame_and_restores_outer_subscription() {
        let outer_state = State::new(1i32);
        let inner_state = State::new(2i32);
        let outer_queue = ComposerSubscription::new();
        let inner_queue = ComposerSubscription::new();

        let mut outer = begin_compose_deps_with_queue(Arc::downgrade(&outer_queue));
        outer_state.get();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _inner = begin_compose_deps_with_queue(Arc::downgrade(&inner_queue));
            inner_state.get();
            panic!("discard inner dependency frame");
        }));
        assert!(result.is_err());

        outer_state.get();
        let deps = take_deps();
        assert_eq!(deps.len(), 2);
        assert!(deps.iter().all(|(signal, _)| signal.id() == outer_state.signal_id()));
        outer.commit();

        inner_state.set(3);
        assert!(inner_queue.is_empty(), "panic frame 的新订阅应回滚");
        outer_state.set(4);
        assert_eq!(outer_queue.drain(), vec![outer_state.signal_id()]);
    }
}
