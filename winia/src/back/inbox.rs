//! The cross-thread queue between platform callbacks and the UI-thread dispatcher.
//!
//! Java callbacks arrive on the Android main thread, while dispatch must run on the event-loop
//! thread that owns the `State` values a handler writes. This queue is the only `Send + Sync`
//! boundary between the two; nothing else in the back module may be touched from JNI.

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

/// Enough for a slow frame; the queue never grows with gesture duration because progress coalesces.
const CAPACITY: usize = 64;

static INBOX: Mutex<VecDeque<Signal>> = Mutex::new(VecDeque::new());

/// Queue one platform event and wake the event loop.
///
/// A progress frame replaces an undrained predecessor: only the latest value drives an animation,
/// and a fast gesture must not build a backlog. Terminal events are never dropped.
pub(crate) fn push(signal: Signal) {
    let mut inbox = INBOX.lock().unwrap_or_else(|error| error.into_inner());
    if matches!(signal, Signal::Progressed(_)) && matches!(inbox.back(), Some(Signal::Progressed(_))) {
        let last = inbox.len() - 1;
        inbox[last] = signal;
    } else {
        if inbox.len() >= CAPACITY {
            // Never drop a start or a terminal event while an older progress frame can go first.
            match inbox
                .iter()
                .position(|queued| matches!(queued, Signal::Progressed(_)))
            {
                Some(position) => {
                    inbox.remove(position);
                }
                None => {
                    inbox.pop_front();
                }
            }
        }
        inbox.push_back(signal);
    }
    drop(inbox);
    crate::runtime::state::wake_loop();
}

/// Take everything queued since the last drain.
pub(crate) fn drain() -> Vec<Signal> {
    let mut inbox = INBOX.lock().unwrap_or_else(|error| error.into_inner());
    inbox.drain(..).collect()
}

#[cfg(test)]
pub(crate) fn pending() -> usize {
    INBOX.lock().unwrap().len()
}
