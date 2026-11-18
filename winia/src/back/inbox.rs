//! The cross-thread queue between platform callbacks and the UI-thread dispatcher.
//!
//! Java callbacks arrive on the Android main thread, while dispatch must run on the event-loop
//! thread that owns the `State` values a handler writes. This queue is the only `Send + Sync`
//! boundary between the two; nothing else in the back module may be touched from JNI.

// Only the Android bridge pushes into this queue; the desktop build compiles it for tests.
#![cfg_attr(not(target_os = "android"), allow(dead_code))]

use std::collections::VecDeque;
use std::sync::Mutex;

use super::{BackEvent, BackUpdate};

/// One platform event, before it becomes a `BackUpdate`.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Signal {
    Started(BackEvent),
    Progressed(BackEvent),
    Completed,
    Cancelled,
}

impl Signal {
    pub(crate) fn into_update(self) -> BackUpdate {
        match self {
            Self::Started(event) => BackUpdate::Started(event),
            Self::Progressed(event) => BackUpdate::Progressed(event),
            Self::Completed => BackUpdate::Completed,
            Self::Cancelled => BackUpdate::Cancelled,
        }
    }
}

/// One queued event plus the window it is addressed to. `None` means "the focused window",
/// which is what the Android bridge produces; a debug injection can name a window explicitly.
#[derive(Clone, Copy, Debug)]
pub(crate) struct QueuedSignal {
    pub(crate) window: Option<u64>,
    pub(crate) signal: Signal,
}

/// Enough for a slow frame; the queue never grows with gesture duration because progress coalesces.
const CAPACITY: usize = 64;

static INBOX: Mutex<VecDeque<QueuedSignal>> = Mutex::new(VecDeque::new());

/// Queue one platform event and wake the event loop.
///
/// A progress frame replaces an undrained predecessor: only the latest value drives an animation,
/// and a fast gesture must not build a backlog. Terminal events are never dropped.
pub(crate) fn push(signal: Signal) {
    push_target(None, signal);
}

/// Queue one platform event for a specific window (the debug server uses this).
#[cfg_attr(not(feature = "debug-server"), allow(dead_code))]
pub(crate) fn push_for(window: u64, signal: Signal) {
    push_target(Some(window), signal);
}

fn push_target(window: Option<u64>, signal: Signal) {
    let mut inbox = INBOX.lock().unwrap_or_else(|error| error.into_inner());
    if matches!(signal, Signal::Progressed(_))
        && inbox.back().is_some_and(|queued| {
            queued.window == window && matches!(queued.signal, Signal::Progressed(_))
        })
    {
        let last = inbox.len() - 1;
        inbox[last] = QueuedSignal { window, signal };
    } else {
        if inbox.len() >= CAPACITY {
            // Never drop a start or a terminal event while an older progress frame can go first.
            match inbox
                .iter()
                .position(|queued| matches!(queued.signal, Signal::Progressed(_)))
            {
                Some(position) => {
                    inbox.remove(position);
                }
                None => {
                    inbox.pop_front();
                }
            }
        }
        inbox.push_back(QueuedSignal { window, signal });
    }
    drop(inbox);
    crate::runtime::state::wake_loop();
}

/// Take everything queued since the last drain, with its target window.
pub(crate) fn drain() -> Vec<QueuedSignal> {
    let mut inbox = INBOX.lock().unwrap_or_else(|error| error.into_inner());
    inbox.drain(..).collect()
}

#[cfg(test)]
pub(crate) fn pending() -> usize {
    INBOX.lock().unwrap().len()
}
