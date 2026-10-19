//! snapshot_flow: a derived stream that recomputes when the states its closure read change.
//!
//! Layer 2 of docs/flow-design.md. State::changes observes ONE state; this observes whatever a
//! closure reads, and emits only when the computed value actually changes — the property that makes
//! it useful for "do something when this becomes true" work (a scroll position crossing a threshold,
//! a debounced query, a filtered list becoming empty).

pub mod ops;

pub use ops::{debounce, Debounce};

use crate::runtime::state::{begin_tracking_frame, take_deps, StateId, StateObserver, StateSignal};
use std::pin::Pin;
use parking_lot::Mutex;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};

/// Wakes the collecting task when any state the closure read changes.
struct FlowTracker {
    waker: Mutex<Option<Waker>>,
}

impl StateObserver for FlowTracker {
    fn on_change(&self, _id: StateId) {
        // Take the waker OUT before waking: the guard from an `if let` scrutinee lives for the whole
        // body, so waking inside it would run the waker while the tracker mutex is held (and the
        // mutex is not reentrant).
        let waker = self.waker.lock().take();
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

/// The stream returned by [snapshot_flow].
pub struct SnapshotFlow<T, F> {
    compute: F,
    tracker: Arc<FlowTracker>,
    /// Signals the last run read; the subscription set is exactly this.
    subscribed: Vec<Arc<StateSignal>>,
    /// Last value handed out; None until the first poll.
    last: Option<T>,
}

/// Nothing here relies on pinning.
impl<T, F> Unpin for SnapshotFlow<T, F> {}

/// A stream over compute(), re-evaluated whenever a state it read changes.
///
/// The first item is the current value. After that an item is produced only when the recomputed
/// value differs (PartialEq), so a state change that leaves the result the same costs no item —
/// which is the whole point: snapshot_flow(|| index > 0) emits twice, not once per scroll pixel.
///
/// The closure must not write the states it reads; that is a feedback loop with no natural stop.
pub fn snapshot_flow<T, F>(compute: F) -> SnapshotFlow<T, F>
where
    F: Fn() -> T,
{
    SnapshotFlow {
        compute,
        tracker: Arc::new(FlowTracker {
            waker: Mutex::new(None),
        }),
        subscribed: Vec::new(),
        last: None,
    }
}

impl<T, F> SnapshotFlow<T, F>
where
    F: Fn() -> T,
{
    /// Make the subscription set exactly the given signals: drop the ones the closure stopped
    /// reading, add the ones it started reading. Without this, a closure whose reads depend on a
    /// branch would keep waking on states it no longer looks at.
    fn resubscribe(&mut self, signals: Vec<Arc<StateSignal>>) {
        let tracker: Arc<dyn StateObserver> = self.tracker.clone();
        for old in &self.subscribed {
            if !signals.iter().any(|s| s.id() == old.id()) {
                old.unsubscribe_observer(&tracker);
            }
        }
        for new in &signals {
            if !self.subscribed.iter().any(|s| s.id() == new.id()) {
                new.subscribe_observer(&tracker);
            }
        }
        self.subscribed = signals;
    }
}

/// How many times one poll may recompute while it is learning the closure's read set. A closure
/// whose reads grow every pass would otherwise spin here; four covers first-poll and branch-change
/// cases with room to spare.
const MAX_TRACKING_PASSES: usize = 4;

impl<T: Clone + PartialEq, F: Fn() -> T> futures_util::Stream for SnapshotFlow<T, F> {
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
        let this = self.get_mut();
        // Waker BEFORE the run: a change that lands while compute() executes must not be lost.
        *this.tracker.waker.lock() = Some(cx.waker().clone());

        // A value is only trustworthy when every state it was computed from was ALREADY subscribed:
        // then a write that lands after the read still reaches the tracker and this poll is
        // repeated. A first run (or one that crossed a branch) reads states the tracker is not
        // listening to yet, and a write between that read and the subscription would be delivered to
        // nobody — the stream would park on a stale value. Reconciling and running again closes that
        // window; in the steady state the first run is covered and the loop body runs once.
        let mut value = None;
        for _ in 0..MAX_TRACKING_PASSES {
            let mut frame = begin_tracking_frame();
            let computed = (this.compute)();
            let deps = take_deps();
            frame.commit();

            let mut signals: Vec<Arc<StateSignal>> = Vec::with_capacity(deps.len());
            for (signal, _slot_key) in deps {
                if !signals.iter().any(|s| s.id() == signal.id()) {
                    signals.push(signal);
                }
            }
            let covered = signals
                .iter()
                .all(|signal| this.subscribed.iter().any(|s| s.id() == signal.id()));

            value = Some(computed);
            // Always reconcile: a run that stopped reading a state has to drop it, or the tracker
            // keeps waking on a state the closure no longer looks at. Only the VALUE is conditional
            // on the run having read nothing new.
            this.resubscribe(signals);
            if covered {
                break;
            }
        }
        let value = value.expect("the loop runs at least once");

        if this.last.as_ref() != Some(&value) {
            this.last = Some(value.clone());
            Poll::Ready(Some(value))
        } else {
            Poll::Pending
        }
    }
}

impl<T, F> Drop for SnapshotFlow<T, F> {
    fn drop(&mut self) {
        let tracker: Arc<dyn StateObserver> = self.tracker.clone();
        for signal in &self.subscribed {
            signal.unsubscribe_observer(&tracker);
        }
    }
}
/// A `State` that mirrors `compute()`, notified only when the computed value changes.
///
/// Reading the source states in composition makes the caller recompose on every write — a scroll
/// offset moving at 60 Hz recomposes a bar that only cares whether it is at the top. This keeps the
/// recomposition tied to the RESULT: the derive subscribes to whatever the closure reads and
/// recomputes on each of those writes, but `State::set`'s dedup drops the ones that did not change
/// the value, so a reader recomposes only on a real flip. It is synchronous — the write that changes
/// the result is the write that notifies.
///
/// The closure has to read through `get()`: `peek()` records no dependency, so a derived value built
/// on it would never recompute.
///
/// The price is that the closure runs on whichever thread wrote the source, so keep it a cheap
/// comparison. For an ASYNC source use `produce_state`, not this.
#[crate::composable]
pub fn remember_derived_state<T: Clone + PartialEq + Send + Sync + 'static>(
    ctx: &mut crate::runtime::composer::ComposeCtx,
    compute: impl Fn() -> T + Send + Sync + 'static,
) -> crate::runtime::state::State<T> {
    let compute: std::sync::Arc<dyn Fn() -> T + Send + Sync> = std::sync::Arc::new(compute);
    let seed = std::sync::Arc::clone(&compute);
    // `remember` already wraps the value in a `State`: building one inside would nest two.
    let state: crate::runtime::state::State<T> = ctx.remember(move || seed());
    let observer: std::sync::Arc<Derived<T>> = {
        let state = state.clone();
        ctx.remember_handle(move || {
            let derived = std::sync::Arc::new(Derived::new(compute, state));
            let _ = derived.me.set(std::sync::Arc::downgrade(&derived));
            derived
        })
    };
    observer.start();
    ctx.remember_on_forget({
        let observer = observer.clone();
        move || observer.stop()
    });
    state
}

/// How many times one refresh may recompute while the world keeps moving under it.
///
/// A writer that never pauses would otherwise spin here; past this the last attempt is published.
const MAX_DERIVE_ATTEMPTS: usize = 4;

/// The subscription behind [remember_derived_state]: recompute on every source write, publish only a
/// changed result.
struct Derived<T> {
    compute: std::sync::Arc<dyn Fn() -> T + Send + Sync>,
    state: crate::runtime::state::State<T>,
    sources: std::sync::Mutex<Vec<std::sync::Arc<crate::runtime::state::StateSignal>>>,
    /// Guards against a closure that writes a source it also reads: that write would re-enter
    /// `refresh` through `notify`. Held until the value is published, so a write in that window
    /// marks `dirty` instead of being dropped or racing a second refresh.
    running: std::sync::atomic::AtomicBool,
    /// Set when a source changed while a refresh was running; the refresh loops again.
    dirty: std::sync::atomic::AtomicBool,
    me: std::sync::OnceLock<std::sync::Weak<Derived<T>>>,
}

impl<T: Clone + PartialEq + Send + Sync + 'static> Derived<T> {
    fn new(
        compute: std::sync::Arc<dyn Fn() -> T + Send + Sync>,
        state: crate::runtime::state::State<T>,
    ) -> Self {
        Self {
            compute,
            state,
            sources: std::sync::Mutex::new(Vec::new()),
            running: std::sync::atomic::AtomicBool::new(false),
            dirty: std::sync::atomic::AtomicBool::new(false),
            me: std::sync::OnceLock::new(),
        }
    }

    /// Subscribe and publish the first value. Runs once, from composition.
    fn start(&self) {
        let Some(me) = self.me.get().and_then(|weak| weak.upgrade()) else { return };
        let observer: std::sync::Arc<dyn crate::runtime::state::StateObserver> = me;
        self.refresh(&observer);
    }

    /// Drop every subscription. Runs from the composition point's `on_forget`.
    fn stop(&self) {
        let Some(me) = self.me.get().and_then(|weak| weak.upgrade()) else { return };
        let observer: std::sync::Arc<dyn crate::runtime::state::StateObserver> = me;
        self.reconcile(&observer, Vec::new());
    }

    /// Replace the subscription set with the states `deps` names.
    fn reconcile(
        &self,
        observer: &std::sync::Arc<dyn crate::runtime::state::StateObserver>,
        deps: Vec<(std::sync::Arc<crate::runtime::state::StateSignal>, u64)>,
    ) {
        let mut signals: Vec<std::sync::Arc<crate::runtime::state::StateSignal>> =
            Vec::with_capacity(deps.len());
        for (signal, _slot_key) in deps {
            if !signals.iter().any(|s| s.id() == signal.id()) {
                signals.push(signal);
            }
        }
        let mut sources = self.sources.lock().unwrap_or_else(|e| e.into_inner());
        for old in sources.iter() {
            if !signals.iter().any(|s| s.id() == old.id()) {
                old.unsubscribe_observer(observer);
            }
        }
        for new in signals.iter() {
            if !sources.iter().any(|s| s.id() == new.id()) {
                new.subscribe_observer(observer);
            }
        }
        *sources = signals;
    }

    /// Recompute, re-derive the subscription set, publish. The user closure runs outside any lock.
    ///
    /// The computation is a seqlock: the global write version is sampled before and after, and a
    /// computation the world changed underneath is thrown away and run again (bounded). Without it, a
    /// closure reading two states could publish a pair that never coexisted — the same half-pair a
    /// `write_batch` prevents on the writing side.
    fn refresh(&self, observer: &std::sync::Arc<dyn crate::runtime::state::StateObserver>) {
        if self.running.swap(true, std::sync::atomic::Ordering::AcqRel) {
            // A refresh is already running; it publishes before clearing the guard and will see this.
            self.dirty.store(true, std::sync::atomic::Ordering::Release);
            return;
        }

        let mut rounds = 0;
        loop {
            self.dirty.store(false, std::sync::atomic::Ordering::Release);
            let mut attempts = 0;
            let (value, deps) = loop {
                let version = crate::runtime::state::global_version();
                let mut frame = crate::runtime::state::begin_tracking_frame();
                let value = (self.compute)();
                let deps = crate::runtime::state::take_deps();
                frame.commit();
                attempts += 1;
                let stable = crate::runtime::state::global_version() == version;
                if stable || attempts >= MAX_DERIVE_ATTEMPTS {
                    break (value, deps);
                }
            };
            self.reconcile(observer, deps);
            // Publish while the guard is still held: a source written between the last version
            // sample and here lands in `dirty` instead of being dropped or racing a second refresh.
            // Dedup lives in `set`: an unchanged result notifies nobody.
            self.state.set(value);
            rounds += 1;
            if !self.dirty.load(std::sync::atomic::Ordering::Acquire) || rounds >= MAX_DERIVE_ATTEMPTS {
                break;
            }
        }
        self.running.store(false, std::sync::atomic::Ordering::Release);
    }
}

impl<T: Clone + PartialEq + Send + Sync + 'static> crate::runtime::state::StateObserver
    for Derived<T>
{
    fn on_change(&self, _id: crate::runtime::state::StateId) {
        if let Some(me) = self.me.get().and_then(|weak| weak.upgrade()) {
            let observer: std::sync::Arc<dyn crate::runtime::state::StateObserver> = me;
            self.refresh(&observer);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::state::State;
    use futures_util::task::{waker, ArcWake};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::task::Context;

    struct CountingWaker(AtomicUsize);

    impl ArcWake for CountingWaker {
        fn wake_by_ref(arc_self: &Arc<Self>) {
            arc_self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn poll_once<T, F>(s: &mut SnapshotFlow<T, F>, cx: &mut Context<'_>) -> Poll<Option<T>>
    where
        T: Clone + PartialEq,
        F: Fn() -> T,
    {
        use futures_util::Stream as _;
        Pin::new(s).poll_next(cx)
    }

    #[test]
    fn emits_the_initial_value_then_only_when_the_result_changes() {
        let counter = Arc::new(CountingWaker(AtomicUsize::new(0)));
        let w = waker(counter.clone());
        let mut cx = Context::from_waker(&w);

        let state = State::new(0i32);
        let s = state.clone();
        let mut flow = snapshot_flow(move || s.get() > 0);

        assert_eq!(poll_once(&mut flow, &mut cx), Poll::Ready(Some(false)), "seed");
        assert_eq!(poll_once(&mut flow, &mut cx), Poll::Pending);

        state.set(1);
        assert_eq!(poll_once(&mut flow, &mut cx), Poll::Ready(Some(true)));

        // A change that does not flip the boolean emits nothing.
        state.set(2);
        assert_eq!(poll_once(&mut flow, &mut cx), Poll::Pending);
    }

    #[test]
    fn a_change_in_a_read_state_wakes_the_collector() {
        let counter = Arc::new(CountingWaker(AtomicUsize::new(0)));
        let w = waker(counter.clone());
        let mut cx = Context::from_waker(&w);

        let state = State::new(false);
        let s = state.clone();
        let mut flow = snapshot_flow(move || s.get());
        assert_eq!(poll_once(&mut flow, &mut cx), Poll::Ready(Some(false)));
        assert_eq!(poll_once(&mut flow, &mut cx), Poll::Pending);
        assert_eq!(counter.0.load(Ordering::SeqCst), 0, "nothing woke it yet");

        state.set(true);
        assert_eq!(counter.0.load(Ordering::SeqCst), 1, "a read state woke the collector");
        assert_eq!(poll_once(&mut flow, &mut cx), Poll::Ready(Some(true)));
    }

    #[test]
    fn ignores_states_the_closure_does_not_read() {
        let counter = Arc::new(CountingWaker(AtomicUsize::new(0)));
        let w = waker(counter.clone());
        let mut cx = Context::from_waker(&w);

        let watched = State::new(0i32);
        let other = State::new(0i32);
        let s = watched.clone();
        let mut flow = snapshot_flow(move || s.get() > 0);
        assert_eq!(poll_once(&mut flow, &mut cx), Poll::Ready(Some(false)));
        assert_eq!(poll_once(&mut flow, &mut cx), Poll::Pending);

        other.set(9);
        assert_eq!(
            counter.0.load(Ordering::SeqCst),
            0,
            "a state the closure never read must not wake the collector"
        );
    }

    #[test]
    fn a_branch_that_stops_reading_a_state_unsubscribes_from_it() {
        let counter = Arc::new(CountingWaker(AtomicUsize::new(0)));
        let w = waker(counter.clone());
        let mut cx = Context::from_waker(&w);

        let flag = State::new(true);
        let x = State::new(0i32);
        let (f, xx) = (flag.clone(), x.clone());
        let mut flow = snapshot_flow(move || if f.get() { xx.get() } else { -1 });

        assert_eq!(poll_once(&mut flow, &mut cx), Poll::Ready(Some(0)));
        x.set(5);
        assert_eq!(poll_once(&mut flow, &mut cx), Poll::Ready(Some(5)));

        // The branch flips: the closure no longer reads x.
        flag.set(false);
        assert_eq!(poll_once(&mut flow, &mut cx), Poll::Ready(Some(-1)));

        let woke_before = counter.0.load(Ordering::SeqCst);
        x.set(6);
        assert_eq!(
            counter.0.load(Ordering::SeqCst),
            woke_before,
            "x is no longer read, so its write must not wake the collector"
        );
        assert_eq!(poll_once(&mut flow, &mut cx), Poll::Pending);
    }

    /// A computation the world moved under is thrown away, not published.
    ///
    /// The closure writes a state it has already read on its second invocation (the first monitored
    /// one — the seed runs before it). The pair it computed did exist, but only for an instant, and
    /// the write it makes is the one whose notification would otherwise have been missed: the tracker
    /// is not subscribed yet. Left alone the stream parks on that half-pair; the version check makes
    /// the refresh run again and publish the pair that held afterwards.
    #[test]
    fn a_derive_retries_when_the_world_moves_under_it() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let a = State::new(0i32);
        let b = State::new(0i32);
        let calls = std::sync::Arc::new(AtomicUsize::new(0));
        let (aa, bb, c) = (a.clone(), b.clone(), calls.clone());
        let slot: std::sync::Arc<std::sync::Mutex<Option<State<(i32, i32)>>>> =
            std::sync::Arc::new(std::sync::Mutex::new(None));
        let out = slot.clone();
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(move |ctx| {
            let derived = crate::flow::remember_derived_state(ctx, move || {
                let av = aa.get();
                if c.fetch_add(1, Ordering::SeqCst) == 1 {
                    // Mid-computation write: the version moves, so this result is discarded.
                    aa.set(9);
                }
                let bv = bb.get();
                (av, bv)
            });
            *out.lock().unwrap() = Some(derived);
        });

        let derived = slot.lock().unwrap().clone().unwrap();
        assert_eq!(
            derived.peek(),
            (9, 0),
            "the published pair must be one that actually held at some instant"
        );
    }
    /// The derive is synchronous: the write that changes the result is the write that notifies.
    #[test]
    fn a_derived_state_updates_on_the_write_itself() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let source = State::new(0i32);
        let s = source.clone();
        let calls = std::sync::Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        let slot: std::sync::Arc<std::sync::Mutex<Option<State<bool>>>> =
            std::sync::Arc::new(std::sync::Mutex::new(None));
        let slot_in = slot.clone();
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(move |ctx| {
            let derived = crate::flow::remember_derived_state(ctx, move || {
                c.fetch_add(1, Ordering::SeqCst);
                s.get() != 0
            });
            *slot_in.lock().unwrap() = Some(derived);
        });
        let derived = slot.lock().unwrap().clone().unwrap();
        let settled = calls.load(Ordering::SeqCst);

        source.set(5);
        assert!(
            derived.peek(),
            "no runtime and no drive: the write itself published the new value"
        );
        assert_eq!(calls.load(Ordering::SeqCst), settled + 1, "one recompute per write");

        source.set(6);
        assert_eq!(
            calls.load(Ordering::SeqCst),
            settled + 2,
            "the recompute happens even when the result does not change (that is what the dedup is for)"
        );
    }

    /// Leaving the composition point drops the subscription.
    #[test]
    fn removing_the_composition_point_unsubscribes_the_derive() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let source = State::new(0i32);
        let s = source.clone();
        let calls = std::sync::Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(move |ctx| {
            let _ = crate::flow::remember_derived_state(ctx, move || {
                c.fetch_add(1, Ordering::SeqCst);
                s.get() != 0
            });
        });
        let settled = calls.load(Ordering::SeqCst);

        composer.compose(|_ctx| {});
        source.set(7);
        assert_eq!(
            calls.load(Ordering::SeqCst),
            settled,
            "the derive must not recompute after its composition point is gone"
        );
    }

    /// A branch that stops reading a state drops that subscription.
    #[test]
    fn a_derived_state_resubscribes_when_its_reads_change() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let flag = State::new(true);
        let a = State::new(1i32);
        let b = State::new(2i32);
        let calls = std::sync::Arc::new(AtomicUsize::new(0));
        let (f, x, y, c) = (flag.clone(), a.clone(), b.clone(), calls.clone());
        let slot: std::sync::Arc<std::sync::Mutex<Option<State<i32>>>> =
            std::sync::Arc::new(std::sync::Mutex::new(None));
        let slot_in = slot.clone();
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(move |ctx| {
            let derived = crate::flow::remember_derived_state(ctx, move || {
                c.fetch_add(1, Ordering::SeqCst);
                if f.get() { x.get() } else { y.get() }
            });
            *slot_in.lock().unwrap() = Some(derived);
        });
        let derived = slot.lock().unwrap().clone().unwrap();
        assert_eq!(derived.peek(), 1);

        flag.set(false);
        assert_eq!(derived.peek(), 2, "the branch flip recomputed through the flag it reads");

        let settled = calls.load(Ordering::SeqCst);
        a.set(9);
        assert_eq!(
            calls.load(Ordering::SeqCst),
            settled,
            "the state the closure stopped reading must not recompute it"
        );
        b.set(10);
        assert_eq!(derived.peek(), 10, "the state it reads now does");
    }

    /// The derived State notifies only when the computed RESULT changes.
    ///
    /// That is the whole point of the helper: a source that moves within one bucket must not
    /// recompose the reader.
    #[test]
    fn a_derived_state_notifies_only_when_the_result_changes() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _entered = rt.enter();

        let source = State::new(0i32);
        let s = source.clone();
        let slot: std::sync::Arc<std::sync::Mutex<Option<State<bool>>>> =
            std::sync::Arc::new(std::sync::Mutex::new(None));
        let slot_in = slot.clone();
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(move |ctx| {
            let derived = crate::flow::remember_derived_state(ctx, move || s.get() != 0);
            *slot_in.lock().unwrap() = Some(derived);
        });
        let drive = || rt.block_on(async { for _ in 0..50 { tokio::task::yield_now().await; } });
        drive();

        let derived = slot.lock().unwrap().clone().unwrap();
        assert!(!derived.peek(), "the seed comes from the first computation");
        let mut changes = derived.changes();
        // The seed is the stream's first item.
        assert_eq!(rt.block_on(async { futures_util::StreamExt::next(&mut changes).await }), Some(false));

        // A flip notifies once.
        source.set(5);
        drive();
        assert!(derived.peek(), "the derived value follows the source");
        assert_eq!(
            rt.block_on(async {
                tokio::time::timeout(
                    std::time::Duration::from_millis(50),
                    futures_util::StreamExt::next(&mut changes),
                )
                .await
                .expect("the flip notifies")
            }),
            Some(true)
        );

        // Movement inside the same bucket does not.
        source.set(6);
        drive();
        assert!(
            rt.block_on(async {
                tokio::time::timeout(
                    std::time::Duration::from_millis(50),
                    futures_util::StreamExt::next(&mut changes),
                )
                .await
                .is_err()
            }),
            "a value that stays in the same bucket must not notify"
        );
    }

    /// A write that lands INSIDE the computation is not lost.
    ///
    /// The closure reads the state and then writes it - the interleaving the subscription used to
    /// miss: the write happens after the read and before the tracker is listening to that state.
    /// Without the covered-run loop the stream emits the pre-write value and then parks with no wake.
    #[test]
    fn a_write_during_the_computation_is_not_lost() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let counter = Arc::new(CountingWaker(AtomicUsize::new(0)));
        let w = waker(counter.clone());
        let mut cx = Context::from_waker(&w);

        let state = State::new(0i32);
        let s = state.clone();
        let once = Arc::new(AtomicBool::new(true));
        let mut flow = snapshot_flow(move || {
            let value = s.get();
            if once.swap(false, Ordering::SeqCst) {
                // After this run's read, before the subscription: exactly the window.
                s.set(1);
            }
            value
        });

        assert_eq!(
            poll_once(&mut flow, &mut cx),
            Poll::Ready(Some(1)),
            "the emitted value must come from a run whose reads were already subscribed"
        );
        assert_eq!(poll_once(&mut flow, &mut cx), Poll::Pending, "and then it settles");
    }
}