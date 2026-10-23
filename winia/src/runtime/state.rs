//! Reactive state system — winia's analogue of Jetpack Compose's `MutableState`.
//!
//! The core ideas:
//! - `State<T>` is an observable value cell: a read inside a compose or layout pass
//!   records a dependency, a write schedules the work that read implies.
//! - Dependency tracking runs through thread-local buffers, so no `CompositionContext`
//!   has to be threaded through a call that only wants to read a value.
//! - Writes dedupe on `PartialEq`, so writing an equal value costs nothing.
//! - Notification goes through `StateSignal`, which fans out to the Composers that
//!   actually read the value. There is no second delivery channel.

use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::fmt::{Debug, Display, Formatter};
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::OnceLock;
use std::sync::{Arc, Weak};
use std::task::{Context, Poll, Waker};

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
    /// Signals subscribed during the current and partial frames, keyed by signal id. Weak entries
    /// let Composer drop clean up even when composition unwinds through panic.
    ///
    /// Keyed rather than a list because this is touched on EVERY `State::get`: the registration is
    /// "is this signal already tracked", and with a list that question was two full scans over every
    /// signal the composer had ever read — quadratic in the number of rows a list reads, and measured
    /// at 6.2 µs per read (a 800-row frame spent 5 ms of its 7.4 ms re-reading 800 states, each read
    /// scanning the 800 it had already tracked). A dead entry is simply overwritten on the next read
    /// of that signal; the scans that actually drop entries are `retain_signals` (per frame) and
    /// `unsubscribe_all` (teardown).
    signals: Mutex<HashMap<StateId, Weak<StateSignal>>>,
    /// Once a Composer drops, no in-flight read may create a new subscription.
    closed: std::sync::atomic::AtomicBool,
}

static NEXT_SUBSCRIPTION_ID: AtomicU64 = AtomicU64::new(1);

impl ComposerSubscription {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            id: NEXT_SUBSCRIPTION_ID.fetch_add(1, Ordering::Relaxed),
            pending: Mutex::new(Vec::new()),
            signals: Mutex::new(HashMap::new()),
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
        let queue = Arc::downgrade(self);
        let observed_revision = signal.subscribe(queue.clone());
        if observed_revision.is_some() {
            // One entry per signal id: re-reading a state already tracked costs a hash lookup and a
            // weak-pointer store, not a scan. (Overwriting the entry is also how a dead one — its
            // strong count gone — is replaced.)
            signals.insert(signal.id(), Arc::downgrade(signal));
        }
        observed_revision
    }

    pub(crate) fn unsubscribe_all(self: &Arc<Self>) {
        // Keep the tracking lock while removing signal-side entries so a
        // concurrent read cannot add a new subscription during Composer drop.
        let mut signals = self.signals.lock();
        self.closed.store(true, Ordering::Release);
        let tracked = std::mem::take(&mut *signals);
        for weak in tracked.values() {
            if let Some(signal) = weak.upgrade() {
                signal.unsubscribe(self.id);
            }
        }
        self.pending.lock().clear();
    }

    fn signal_ids(&self) -> std::collections::HashSet<StateId> {
        self.signals
            .lock()
            .values()
            .filter_map(|weak| weak.upgrade().map(|signal| signal.id()))
            .collect()
    }

    /// Reconcile tracked signals after a frame, including reads recorded before
    /// a panic. Keep the tracking lock through removal so a concurrent read cannot
    /// re-add a signal before its old subscription is removed.
    pub(crate) fn retain_signals(&self, live_ids: &std::collections::HashSet<StateId>) {
        let mut signals = self.signals.lock();
        let mut removed = Vec::new();
        signals.retain(|id, weak| {
            let Some(signal) = weak.upgrade() else { return false };
            if live_ids.contains(id) {
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

/// A consumer of a signal's notifications that is not a Composer.
///
/// The Compose path above (a coalesced `pending` queue drained by a frame loop) and this one stay
/// separate on purpose — see `docs/flow-design.md` §5, "Subscriber model". Unifying them would put
/// the read handshake and the per-frame reconcile behind a trait object for no user-visible gain.
pub(crate) trait StateObserver: Send + Sync {
    /// The value changed. Called WITHOUT the signal's subscriber lock held, because an observer
    /// may read or subscribe to the same signal and the mutex is not reentrant.
    fn on_change(&self, id: StateId);
}

/// A State's signal owns its subscribers. There is no global state-id -> owner
/// map, so an unowned State cannot accidentally notify a stale Composer.
pub(crate) struct StateSignal {
    id: StateId,
    /// Monotonic read/notify handshake. Subscription and the revision snapshot
    /// are linearized under the subscriber lock before the value is read.
    revision: AtomicU64,
    subscribers: Mutex<Vec<Subscriber>>,
    /// `State::changes` collectors. Lazily created: most states never have one, and an empty
    /// `OnceLock` costs nothing on the common path.
    observers: OnceLock<Mutex<Vec<Weak<dyn StateObserver>>>>,
    /// The global version this state was last written at, and the pass that wrote it (0 = no pass).
    /// Together they let a read inside a pass tell whether it is looking at a value that landed after
    /// the pass began, and whether this thread is the one that put it there.
    last_write_version: AtomicU64,
    last_write_pass: AtomicU64,
}

impl StateSignal {
    pub(crate) fn id(&self) -> StateId {
        self.id
    }


    fn new(id: StateId) -> Arc<Self> {
        Arc::new(Self {
            id,
            revision: AtomicU64::new(0),
            subscribers: Mutex::new(Vec::new()),
            observers: OnceLock::new(),
            last_write_version: AtomicU64::new(0),
            last_write_pass: AtomicU64::new(0),
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

        // Stream observers, OUTSIDE every lock: `on_change` is user-visible code (it wakes a
        // task), and an observer that reads this same state would re-enter `subscribe`.
        if let Some(list) = self.observers.get() {
            let live: Vec<Arc<dyn StateObserver>> = {
                let mut list = list.lock();
                list.retain(|weak| weak.strong_count() > 0);
                list.iter().filter_map(|weak| weak.upgrade()).collect()
            };
            for observer in live {
                observer.on_change(self.id);
            }
        }
    }

    /// Register a non-composer observer. The signal only holds a weak handle, so a dropped
    /// `StateChanges` unsubscribes even if its `Drop` never runs to completion.
    pub(crate) fn subscribe_observer(&self, observer: &Arc<dyn StateObserver>) {
        let list = self.observers.get_or_init(|| Mutex::new(Vec::new()));
        let mut list = list.lock();
        list.retain(|weak| weak.strong_count() > 0);
        list.push(Arc::downgrade(observer));
    }

    pub(crate) fn unsubscribe_observer(&self, observer: &Arc<dyn StateObserver>) {
        let Some(list) = self.observers.get() else {
            return;
        };
        let mut list = list.lock();
        list.retain(|weak| match weak.upgrade() {
            Some(existing) => !Arc::ptr_eq(&existing, observer),
            None => false,
        });
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
        self.inner.signal.notify(wake);
    }

    /// Stamp a write that is ABOUT to change the value.
    ///
    /// The version has to move before the store, not after it: a reader that samples the version
    /// around its computation only catches a write that both moved the version and became visible
    /// inside that window. Bumping afterwards leaves a gap where the value is already new and the
    /// version still old, and the reader accepts a pair that never existed.
    fn begin_write(&self) {
        let version = GLOBAL_VERSION.fetch_add(1, Ordering::Release) + 1;
        #[cfg(debug_assertions)]
        {
            self.inner
                .signal
                .last_write_version
                .store(version, Ordering::Release);
            self.inner
                .signal
                .last_write_pass
                .store(current_pass_id(), Ordering::Release);
        }
    }

    /// Publish a value change that is already stored: deferred into an open batch, or notified now.
    fn publish(&self, wake: bool) {
        if defer_write(&self.inner.signal, wake) {
            return;
        }
        self.notify(wake);
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
        });
        Self { inner }
    }

    pub(crate) fn get_tracked(&self) -> T
    where
        T: Clone,
    {
        let registration = register_dependency(self.inner.signal.clone());
        let value = self.inner.value.read().clone();
        // AFTER the read: a write that lands between the two would otherwise go unseen.
        #[cfg(debug_assertions)]
        note_pass_read(&self.inner.signal);
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
        self.begin_write();
        *current = value;
        drop(current);
        self.publish(true);
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
        self.begin_write();
        *current = staged;
        drop(current);
        self.publish(true);
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
        self.begin_write();
        *current = value;
        drop(current);
        self.publish(false);
    }

    /// Draw-layer write: no dedup, no notify, no wake. The animation engine
    /// drives repaint via `request_redraw`; values are consumed via `peek`.
    pub(crate) fn set_visual(&self, value: T) {
        *self.inner.value.write() = value;
        VISUAL_EPOCH.fetch_add(1, Ordering::Relaxed);
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

            /// The macro emits this for every handle type; the ones the dependency maps do not
            /// ask for are still part of what a handle is, and the callers that will want them
            /// are in this file.
            #[allow(dead_code)]
            pub(crate) fn signal_id(&self) -> StateId {
                self.0.signal_id()
            }

            #[allow(dead_code)] // see `signal_id` above
            pub(crate) fn from_raw(raw: RawState<T>) -> Self {
                Self(raw)
            }

            #[allow(dead_code)] // see `signal_id` above
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

pub(crate) struct StateInner<T> {
    signal: Arc<StateSignal>,
    /// Transitional u32 identity kept for animation/public compatibility.
    /// Internal Composer routing uses `signal.id()` exclusively.
    public_id: u32,
    value: RwLock<T>,
}

// Internal routing IDs are 64-bit; the public animation compatibility ID
// remains a separate u32 sequence until that API is migrated.
static NEXT_STATE_ID: AtomicU64 = AtomicU64::new(1);
/// Incremented by draw-layer writes (`Visual<T>`). The item exit-picture cache uses it to
/// invalidate a ghost after a paint-only change that never measures a node.
static VISUAL_EPOCH: AtomicU64 = AtomicU64::new(1);

/// The current paint-only change epoch (see `RawState::set_visual`).
pub(crate) fn visual_epoch() -> u64 {
    VISUAL_EPOCH.load(Ordering::Relaxed)
}
static NEXT_PUBLIC_STATE_ID: AtomicU32 = AtomicU32::new(1);

// ── Write batches and the global version ──

/// The whole-world version: advances once per committed write, and once per write batch.
///
/// It is the cheap half of a snapshot system (see docs/snapshot-feasibility.md): no per-snapshot
/// records, just a monotonic counter a reader can sample before and after a computation to tell
/// whether the world moved underneath it.
static GLOBAL_VERSION: AtomicU64 = AtomicU64::new(1);

/// The version of the last committed write or batch.
pub(crate) fn global_version() -> u64 {
    GLOBAL_VERSION.load(Ordering::Acquire)
}

thread_local! {
    /// Writes that still have to be published while a batch is open, with the wake flag each one
    /// asked for (an `Animating` tick must not start waking the event loop just because it landed
    /// inside a batch).
    static DEFERRED_WRITES: std::cell::RefCell<Option<Vec<(Arc<StateSignal>, bool)>>> =
        const { std::cell::RefCell::new(None) };
}

/// Apply several state writes as ONE change.
///
/// Every `set`/`update` inside the closure takes effect immediately — the writer reads its own
/// writes — but observers are notified once, after the closure returns. Without this, a reader
/// woken by the first write of a pair observes the second one still at its old value.
///
/// Each write still advances the global version as it happens: the version orders writes against
/// the values they published, and a batch that bumped only at commit would leave a window where a
/// concurrent reader could sample an unchanged version across a changed pair.
///
/// Batches nest: only the outermost commits, so a component's own batch inside a larger one merges
/// into it. A panic inside the closure still publishes what was written — the values are already
/// applied, and leaving observers unwoken would be worse than a spurious wake.
///
/// A state written twice in one batch notifies once even if the net effect was nil; the batch
/// deliberately does not diff.
///
/// The batch is THREAD-SCOPED: a write from another thread during the closure is published
/// immediately, as a batch of its own. That is the honest boundary — making it global would mean
/// blocking writers behind the composing thread.
pub fn write_batch<R>(f: impl FnOnce() -> R) -> R {
    let outermost = DEFERRED_WRITES.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_some() {
            false
        } else {
            *slot = Some(Vec::new());
            true
        }
    });
    if !outermost {
        return f();
    }
    let _commit = BatchCommit;
    f()
}

struct BatchCommit;

impl Drop for BatchCommit {
    fn drop(&mut self) {
        let deferred = DEFERRED_WRITES.with(|slot| slot.borrow_mut().take()).unwrap_or_default();
        if deferred.is_empty() {
            return;
        }
        // Coalesce by state, keeping "wake" if ANY write to it wanted one.
        let mut merged: Vec<(Arc<StateSignal>, bool)> = Vec::with_capacity(deferred.len());
        for (signal, wake) in deferred {
            match merged.iter_mut().find(|(seen, _)| seen.id() == signal.id()) {
                Some((_, wanted)) => *wanted |= wake,
                None => merged.push((signal, wake)),
            }
        }

        // Notify one state at a time, surviving a panicking observer: the rest of the batch still
        // has to be told, and a panic escaping a Drop during an unwind aborts the process.
        let mut first_panic = None;
        for (signal, wake) in merged {
            let outcome =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| signal.notify(wake)));
            if let Err(payload) = outcome {
                if first_panic.is_none() {
                    first_panic = Some(payload);
                }
            }
        }
        if let Some(payload) = first_panic {
            if !std::thread::panicking() {
                std::panic::resume_unwind(payload);
            }
        }
    }
}

// ── Pass instrumentation (debug only) ──

static NEXT_PASS_ID: AtomicU64 = AtomicU64::new(1);
static TORN_PASS_READS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[derive(Clone, Copy)]
struct PassInfo {
    id: u64,
    start_version: u64,
}

thread_local! {
    static CURRENT_PASS: std::cell::Cell<Option<PassInfo>> = const { std::cell::Cell::new(None) };
}

/// The current thread's pass id, or 0 when it is not composing.
fn current_pass_id() -> u64 {
    CURRENT_PASS.with(|slot| slot.get().map(|info| info.id).unwrap_or(0))
}

/// Marks this thread as being inside a compose pass until dropped.
pub(crate) struct PassGuard {
    previous: Option<PassInfo>,
}

impl Drop for PassGuard {
    fn drop(&mut self) {
        let previous = self.previous;
        CURRENT_PASS.with(|slot| slot.set(previous));
    }
}

/// Begin a compose pass on this thread: the id tags writes made from here, and the version is the
/// line a read has to stay behind to count as consistent.
pub(crate) fn begin_pass() -> PassGuard {
    let previous = CURRENT_PASS.with(|slot| slot.get());
    // A nested pass on the same thread (a subcomposition composing during layout) is part of the
    // same composition: it inherits the id and the start version, so a write it makes is not
    // reported as a foreign write of the pass that contains it.
    let info = match previous {
        Some(outer) => outer,
        None => PassInfo {
            id: NEXT_PASS_ID.fetch_add(1, Ordering::Relaxed),
            start_version: global_version(),
        },
    };
    CURRENT_PASS.with(|slot| slot.set(Some(info)));
    PassGuard { previous }
}

/// Count a read that observed a state written by someone ELSE after this pass began.
///
/// That is a torn pass: two reads in one composition see different worlds. Writes this pass made
/// itself are expected (a component may set state while it builds) and are not counted.
#[cfg(debug_assertions)]
pub(crate) fn note_pass_read(signal: &Arc<StateSignal>) {
    let Some(pass) = CURRENT_PASS.with(|slot| slot.get()) else {
        return;
    };
    if signal.last_write_version.load(Ordering::Acquire) <= pass.start_version {
        return;
    }
    if signal.last_write_pass.load(Ordering::Acquire) == pass.id {
        return;
    }
    let count = TORN_PASS_READS.fetch_add(1, Ordering::Relaxed) + 1;
    if count == 1 {
        // Once per process: the first time this fires is the one worth looking at.
        eprintln!("[torn-pass] a read observed a mid-pass write from another thread");
    }
}

/// How many torn reads this process has observed (debug builds only).
pub(crate) fn torn_pass_reads() -> usize {
    TORN_PASS_READS.load(Ordering::Relaxed)
}

/// Hand `signal` to the open batch, or say no so the caller publishes immediately.
fn defer_write(signal: &Arc<StateSignal>, wake: bool) -> bool {
    DEFERRED_WRITES.with(|slot| {
        let mut slot = slot.borrow_mut();
        match slot.as_mut() {
            Some(list) => {
                list.push((Arc::clone(signal), wake));
                true
            }
            None => false,
        }
    })
}

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



}

impl<T: Clone + 'static> State<T> {
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
    ///
    /// This is the only write `State` offers, and it means the one thing: recompose
    /// and wake. Every narrower scheduling decision belongs to a handle —
    /// `Backchannel` (write only), `Animating` (recompose, no wake), `Visual` (write
    /// only, draw layer) — so a site states what it wants by what type it holds
    /// rather than by which setter it calls.
    pub fn set(&self, value: T) {
        self.raw.set_reactive(value);
    }
}

impl<T: Clone + PartialEq + 'static> State<T> {
    /// Mutate in place with dedup: no-op mutations skip notification like
    /// `set` does (clone-compare-swap; no lock held while running `f`).
    pub fn update(&self, f: impl FnOnce(&mut T)) {
        self.raw.update_reactive(f);
    }

    /// Observe this state from outside a composition: the current value first, then each change.
    ///
    /// Returns a `Stream` (futures), so `StreamExt` operators apply and a coroutine can await it.
    /// Delivery is **conflated** — the stream reads the cell at poll time, so several writes
    /// between two polls surface as the latest value once. Dropping the stream unsubscribes.
    ///
    /// A stream does not wake the event loop (it is not a composition); the awaiting task's waker
    /// is what gets woken. See `docs/flow-design.md` for the layer above this one.
    pub fn changes(&self) -> StateChanges<T>
    where
        T: Send + Sync,
    {
        let observer = Arc::new(ChangesObserver { waker: Mutex::new(None) });
        let signal = &self.raw.inner.signal;
        signal.subscribe_observer(&(observer.clone() as Arc<dyn StateObserver>));
        StateChanges {
            inner: Arc::clone(&self.raw.inner),
            observer,
            last: None,
        }
    }

}


mod changes;
mod deps;
mod derived;

pub(crate) use deps::*;
use changes::ChangesObserver;
pub use changes::*;
pub use derived::*;

#[cfg(test)]
mod tests;
