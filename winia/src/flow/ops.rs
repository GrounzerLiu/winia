//! Stream operators that the futures/tokio-stream set does not provide.
//!
//! debounce is the one Compose's Flow has that UI code actually reaches for: a search field that
//! should fire once the user stops typing, a scroll position that should settle before it is
//! published. The others (combine, collectLatest) compose out of existing combinators.

use futures_util::Stream;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::time::{sleep_until, Instant, Sleep};

/// The stream returned by [debounce].
pub struct Debounce<S: Stream> {
    inner: S,
    period: Duration,
    /// The newest value seen since the last quiet period ended.
    pending: Option<S::Item>,
    /// Deadline for that value; reset by every newer one.
    timer: Option<Pin<Box<Sleep>>>,
    /// The upstream ended. Its last value still has to wait out the period.
    ended: bool,
}

// Nothing is pinned in place: the boxed timer is pinned by its own pointer, and the pending value is
// only ever moved. The impl covers every S for that reason; the Stream impl below still requires
// S: Unpin, because polling the inner stream needs a Pin::new.
impl<S: Stream> Unpin for Debounce<S> {}

/// Emit an item only once the upstream has been quiet for [period].
///
/// An item that arrives before the period elapses replaces the pending one and restarts the clock,
/// so a burst of N values produces exactly one emission - the last - once the burst ends. When the
/// upstream ends, the last pending value is still delivered after its period; the stream then
/// finishes.
///
/// Requires a tokio runtime with time enabled (the same requirement LaunchedEffect has): the timer
/// is created on the first item, inside the poll.
pub fn debounce<S>(stream: S, period: Duration) -> Debounce<S>
where
    S: Stream,
{
    Debounce {
        inner: stream,
        period,
        pending: None,
        timer: None,
        ended: false,
    }
}

impl<S: Stream + Unpin> Stream for Debounce<S> {
    type Item = S::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<S::Item>> {
        let this = self.get_mut();

        // A Stream is only allowed to be polled until it reports the end, and a pending value still
        // has its own period to wait out — so the drain is skipped once the end is known, rather than
        // the whole poll short-circuiting.
        if !this.ended {
            // Drain whatever the upstream has, so the period is measured from the LAST value.
            loop {
                match Pin::new(&mut this.inner).poll_next(cx) {
                    Poll::Ready(Some(item)) => {
                        this.pending = Some(item);
                        let deadline = Instant::now() + this.period;
                        match &mut this.timer {
                            Some(timer) => timer.as_mut().reset(deadline),
                            None => this.timer = Some(Box::pin(sleep_until(deadline))),
                        }
                    }
                    Poll::Ready(None) => {
                        this.ended = true;
                        break;
                    }
                    Poll::Pending => break,
                }
            }
        }

        if this.pending.is_some() {
            let fired = match &mut this.timer {
                Some(timer) => timer.as_mut().poll(cx).is_ready(),
                None => true,
            };
            if fired {
                let value = this.pending.take().expect("pending was checked above");
                this.timer = None;
                return Poll::Ready(Some(value));
            }
        }

        if this.ended && this.pending.is_none() {
            return Poll::Ready(None);
        }

        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};
    use std::task::Waker;

    /// A stream the test feeds by hand: it yields what is queued, then parks until more arrives —
    /// unless the test has finished it, in which case it reports the end.
    #[derive(Clone, Default)]
    struct Feed {
        queue: Arc<Mutex<VecDeque<i32>>>,
        ended: Arc<std::sync::atomic::AtomicBool>,
    }

    impl Feed {
        fn push(&self, value: i32) {
            self.queue.lock().unwrap().push_back(value);
        }

        fn finish(&self) {
            self.ended.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }

    struct FeedStream(Feed);

    impl Stream for FeedStream {
        type Item = i32;
        fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<i32>> {
            if let Some(value) = self.0.queue.lock().unwrap().pop_front() {
                return Poll::Ready(Some(value));
            }
            if self.0.ended.load(std::sync::atomic::Ordering::SeqCst) {
                return Poll::Ready(None);
            }
            Poll::Pending
        }
    }

    fn poll<S: Stream + Unpin>(stream: &mut S, waker: &Waker) -> Poll<Option<S::Item>> {
        let mut cx = Context::from_waker(waker);
        Pin::new(stream).poll_next(&mut cx)
    }

    fn paused_runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .start_paused(true)
            .build()
            .expect("current-thread runtime")
    }

    /// Three values in one burst produce one emission: the last one, after the quiet period.
    #[test]
    fn a_burst_collapses_to_its_last_value() {
        let rt = paused_runtime();
        rt.block_on(async {
            let waker = futures_util::task::noop_waker();
            let feed = Feed::default();
            let mut deb = debounce(FeedStream(feed.clone()), Duration::from_millis(100));

            feed.push(1);
            feed.push(2);
            feed.push(3);
            assert!(poll(&mut deb, &waker).is_pending(), "nothing is emitted yet");

            tokio::time::advance(Duration::from_millis(50)).await;
            assert!(poll(&mut deb, &waker).is_pending(), "still inside the period");

            tokio::time::advance(Duration::from_millis(60)).await;
            assert_eq!(poll(&mut deb, &waker), Poll::Ready(Some(3)), "the last value, once");
            assert!(poll(&mut deb, &waker).is_pending(), "and nothing more");
        });
    }

    /// A new value restarts the period instead of shortening it.
    #[test]
    fn a_later_value_restarts_the_period() {
        let rt = paused_runtime();
        rt.block_on(async {
            let waker = futures_util::task::noop_waker();
            let feed = Feed::default();
            let mut deb = debounce(FeedStream(feed.clone()), Duration::from_millis(100));

            feed.push(1);
            assert!(poll(&mut deb, &waker).is_pending());
            tokio::time::advance(Duration::from_millis(60)).await;
            assert!(poll(&mut deb, &waker).is_pending());

            feed.push(2);
            assert!(poll(&mut deb, &waker).is_pending(), "the clock restarted");
            tokio::time::advance(Duration::from_millis(60)).await;
            assert!(poll(&mut deb, &waker).is_pending(), "60 ms since value 2, not 120");

            tokio::time::advance(Duration::from_millis(60)).await;
            assert_eq!(poll(&mut deb, &waker), Poll::Ready(Some(2)));
        });
    }

    /// The upstream ending does not drop the pending value: it still waits out its period.
    #[test]
    fn the_last_value_survives_the_upstream_ending() {
        let rt = paused_runtime();
        rt.block_on(async {
            let waker = futures_util::task::noop_waker();
            let feed = Feed::default();
            let mut deb = debounce(FeedStream(feed.clone()), Duration::from_millis(30));

            feed.push(7);
            feed.finish();
            assert!(poll(&mut deb, &waker).is_pending(), "the period still has to run");

            tokio::time::advance(Duration::from_millis(40)).await;
            assert_eq!(poll(&mut deb, &waker), Poll::Ready(Some(7)));
            assert_eq!(poll(&mut deb, &waker), Poll::Ready(None), "and then the stream ends");
        });
    }

    /// The upstream is polled only until it reports the end.
    ///
    /// A `Stream` may panic when polled after `None`. This one does, so the test fails loudly if the
    /// drain loop ever touches a finished upstream again.
    #[test]
    fn a_finished_upstream_is_never_polled_again() {
        struct EndAfterOne {
            polls: usize,
        }

        impl Stream for EndAfterOne {
            type Item = i32;
            fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<i32>> {
                self.polls += 1;
                match self.polls {
                    1 => Poll::Ready(Some(7)),
                    2 => Poll::Ready(None),
                    _ => panic!("the upstream was polled after it reported the end"),
                }
            }
        }

        let rt = paused_runtime();
        rt.block_on(async {
            let waker = futures_util::task::noop_waker();
            let mut deb = debounce(EndAfterOne { polls: 0 }, Duration::from_millis(10));

            assert!(poll(&mut deb, &waker).is_pending(), "the value waits out the period");
            tokio::time::advance(Duration::from_millis(20)).await;
            assert_eq!(poll(&mut deb, &waker), Poll::Ready(Some(7)));
            // The end, without touching the upstream again.
            assert_eq!(poll(&mut deb, &waker), Poll::Ready(None));
            assert_eq!(poll(&mut deb, &waker), Poll::Ready(None));
        });
    }

    /// A real executor wakes this stream when the period elapses.
    ///
    /// The paused-clock tests above poll by hand with a noop waker, so an implementation that never
    /// registered the timer waker would pass every one of them and hang in a real runtime. This one
    /// awaits for real, under a timeout so a regression fails instead of hanging.
    #[test]
    fn a_real_executor_is_woken_when_the_period_elapses() {
        use futures_util::StreamExt;

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        rt.block_on(async {
            let feed = Feed::default();
            feed.push(9);
            let mut deb = debounce(FeedStream(feed.clone()), Duration::from_millis(10));
            let next = tokio::time::timeout(Duration::from_secs(2), deb.next())
                .await
                .expect("the timer must wake the waiting task");
            assert_eq!(next, Some(9));
        });
    }
}
