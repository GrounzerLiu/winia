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
