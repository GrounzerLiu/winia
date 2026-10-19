//! The frame clock: one process-wide (any window) and one per window.
//!
//! Two clocks on purpose. The global one is what `with_frame_nanos` waits on and what every window
//! ticks: a waiter there is woken by ANY window's frame, which is right for a single window and
//! wrong for several. `FrameClock::for_window` is the precise one — a waiter is woken only by its
//! own window's frames.

use std::collections::HashMap;
use std::future::Future;
use std::sync::{LazyLock, Mutex};
use tokio::sync::broadcast;

/// Every window ticks it; `with_frame_nanos` waits on it.
static FRAME_CLOCK: LazyLock<(broadcast::Sender<u64>, broadcast::Receiver<u64>)> = LazyLock::new(|| {
    let (tx, rx) = broadcast::channel(16);
    (tx, rx)
});

static FRAME_EPOCH: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

/// Monotonic nanoseconds from a process-local epoch (a wall clock can step backwards).
fn monotonic_nanos() -> u64 {
    let epoch = *FRAME_EPOCH.get_or_init(std::time::Instant::now);
    // max(1): the first call can land in the same nanosecond as the epoch.
    (epoch.elapsed().as_nanos() as u64).max(1)
}

/// Subscribe and discard whatever predates the call. Synchronous on purpose: a tick that lands
/// between the call and the first poll is the NEXT frame, not a stale one, so this has to run
/// before the future is handed back.
fn subscribe_next(tx: &broadcast::Sender<u64>) -> broadcast::Receiver<u64> {
    let mut rx = tx.subscribe();
    while rx.try_recv().is_ok() {}
    rx
}

async fn next_frame_on(mut rx: broadcast::Receiver<u64>) -> u64 {
    loop {
        match rx.recv().await {
            Ok(v) => return v,
            // The receiver fell behind the buffer (a long stall); skip to the next frame.
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            // The sender is gone — the window this clock belonged to was dropped.
            Err(broadcast::error::RecvError::Closed) => return 0,
        }
    }
}

/// The render loop calls this once per frame, for every window.
pub fn frame_tick() {
    let _ = FRAME_CLOCK.0.send(monotonic_nanos());
}

/// Wait for the next frame of ANY window and return its timestamp.
///
/// Use it in an example or a single-window app. With more than one window a waiter here is woken
/// by whichever window drew first; capture a [`FrameClock`] with `ctx.frame_clock()` when that
/// matters.
///
/// The clock is driven by the render loop: a window that is not rendering (minimised, no
/// `request_redraw`) does not tick it, so use tokio timers for time-driven work.
pub fn with_frame_nanos() -> impl Future<Output = u64> + Send {
    let rx = subscribe_next(&FRAME_CLOCK.0);
    next_frame_on(rx)
}

// ── per window ──

struct WindowClock {
    tx: broadcast::Sender<u64>,
    /// Keeps a receiver alive so a tick with no waiter is still delivered to the buffer.
    _keep: broadcast::Receiver<u64>,
}

static WINDOW_CLOCKS: LazyLock<Mutex<HashMap<u64, WindowClock>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn with_clock<R>(window: u64, f: impl FnOnce(&WindowClock) -> R) -> R {
    let mut clocks = WINDOW_CLOCKS.lock().unwrap_or_else(|e| e.into_inner());
    let clock = clocks.entry(window).or_insert_with(|| {
        let (tx, keep) = broadcast::channel(16);
        WindowClock { tx, _keep: keep }
    });
    f(clock)
}

/// The frame clock of one window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameClock {
    window: u64,
}

impl FrameClock {
    /// The clock of the window with this id (the winit window id).
    pub fn for_window(window: u64) -> Self {
        Self { window }
    }

    pub fn window(&self) -> u64 {
        self.window
    }

    /// Called by the render loop for this window's own frames.
    pub(crate) fn tick(&self) {
        // Clone the sender out and send AFTER the registry lock is released: a send wakes
        // waiters, and no waker should run while a registry guard is alive.
        let sender = with_clock(self.window, |clock| clock.tx.clone());
        let _ = sender.send(monotonic_nanos());
    }

    /// Wait for the next frame of THIS window.
    ///
    /// Resolves to 0 when the window's clock is gone (the window closed and forget_window ran): a 0
    /// timestamp means "no frame will come", the same sentinel the global clock uses for a closed
    /// channel.
    pub fn next_frame(&self) -> impl Future<Output = u64> + Send {
        let rx = with_clock(self.window, |clock| subscribe_next(&clock.tx));
        next_frame_on(rx)
    }
}

/// Forget a window's clock when the window goes away, so the registry does not grow per window.
pub(crate) fn forget_window(window: u64) {
    WINDOW_CLOCKS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&window);
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    fn poll_once<F: Future<Output = u64>>(fut: &mut Pin<Box<F>>) -> Poll<u64> {
        let waker = futures_util::task::noop_waker();
        let mut cx = Context::from_waker(&waker);
        fut.as_mut().poll(&mut cx)
    }

    /// A window's clock is woken only by that window's frames.
    #[test]
    fn a_window_clock_is_woken_only_by_its_own_window() {
        let a = FrameClock::for_window(0xA000);
        let b = FrameClock::for_window(0xB000);
        let mut fut = Box::pin(a.next_frame());

        b.tick();
        assert!(
            matches!(poll_once(&mut fut), Poll::Pending),
            "another window's frame must not wake it"
        );

        a.tick();
        assert!(
            matches!(poll_once(&mut fut), Poll::Ready(_)),
            "its own window's frame does"
        );
    }

    /// The process-wide clock stays "any window", which is what with_frame_nanos documents.
    #[test]
    fn the_global_clock_ignores_window_ticks() {
        let mut fut = Box::pin(with_frame_nanos());
        FrameClock::for_window(0xC000).tick();
        assert!(
            matches!(poll_once(&mut fut), Poll::Pending),
            "a window tick is not a global tick"
        );
        frame_tick();
        assert!(matches!(poll_once(&mut fut), Poll::Ready(_)));
    }
}
