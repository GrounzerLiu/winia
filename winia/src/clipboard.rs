//! Clipboard access, and why the two directions differ.
//!
//! A READ has to answer synchronously: Ctrl+V inserts text inside the key handler, and the inserting
//! code cannot suspend. winit's clipboard API is asynchronous (it hands back a `DataTransferId` and
//! delivers the bytes later as a `WindowEvent::DataTransferReceived`), so reads still go through
//! `arboard`, which blocks and returns the string.
//!
//! A WRITE has no such constraint, and winit owns the data far better: `set_clipboard` keeps the
//! payload until another application takes the clipboard and flushes it on Windows
//! (`OleFlushClipboard`), so what this app copied survives this process exiting. `arboard` writes a
//! snapshot and has to be re-opened for every operation.
//!
//! The write cannot happen where it is decided: `set_clipboard` needs the `ActiveEventLoop`, which
//! only the app's event handler holds. A component therefore REQUESTS a write and the app applies it
//! once the event has been dispatched — the same event, no frame delay (see
//! `AppState::window_event`).

use std::cell::RefCell;

/// How many unapplied requests are kept. A request is drained on every window event, so more than
/// one is already unusual; the bound exists so a copy made before the app runs cannot grow without
/// limit.
const MAX_PENDING: usize = 8;

thread_local! {
    static PENDING: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// Ask for `text` to become the clipboard contents. Also returned by `copy`/`cut` in a text field.
///
/// Empty text is ignored: copying an empty selection must not wipe what the user had on the
/// clipboard.
pub(crate) fn request_text(text: &str) {
    if text.is_empty() {
        return;
    }
    PENDING.with(|pending| {
        let mut pending = pending.borrow_mut();
        if pending.len() == MAX_PENDING {
            pending.remove(0);
        }
        pending.push(text.to_owned());
    });
}

/// Take everything requested since the last call, oldest first.
pub(crate) fn take_pending() -> Vec<String> {
    PENDING.with(|pending| std::mem::take(&mut *pending.borrow_mut()))
}

/// Apply every pending request through `sink`, returning how many it accepted.
///
/// Split out from [`apply_pending`] so a test can watch the drain without an event loop.
fn drain_with(mut sink: impl FnMut(&str) -> bool) -> usize {
    let mut applied = 0;
    for text in take_pending() {
        if sink(&text) {
            applied += 1;
        }
    }
    applied
}

/// Hand every pending request to winit, falling back to `arboard` for the ones it refuses.
///
/// winit returns an error on a platform without clipboard support (it is implemented on Windows,
/// macOS, Wayland and X11 today). `arboard` covers more ground at the cost of the semantics
/// described in this module's header, so it is the fallback rather than the primary.
pub(crate) fn apply_pending(event_loop: &dyn winit::event_loop::ActiveEventLoop) {
    drain_with(|text| {
        use winit::data_transfer::{DataTransferSendBuilder, SendData, TypeHint};
        let payload = text.to_owned();
        let send = DataTransferSendBuilder::new(payload)
            .with_type(
                TypeHint::Plaintext,
                |state: &String, _ty: &dyn winit::data_transfer::TransferType| {
                    Some(SendData::String(state.clone()))
                },
            )
            .build();
        match event_loop.set_clipboard(send) {
            Ok(()) => true,
            Err(err) => {
                crate::debug_log!("[clipboard] winit set_clipboard failed ({err}); falling back to arboard");
                if let Ok(mut cb) = arboard::Clipboard::new() {
                    cb.set_text(text.to_owned()).is_ok()
                } else {
                    false
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole contract: what a component requested arrives at the sink exactly once, in order,
    /// and a second drain finds nothing to do.
    #[test]
    fn a_copy_request_reaches_the_sink_once() {
        take_pending();
        request_text("first");
        request_text("second");
        let mut seen: Vec<String> = Vec::new();
        let applied = drain_with(|text| {
            seen.push(text.to_owned());
            true
        });
        assert_eq!(applied, 2);
        assert_eq!(seen, vec!["first".to_string(), "second".to_string()]);
        let mut again = 0;
        assert_eq!(drain_with(|_| { again += 1; true }), 0, "the queue is empty after a drain");
        assert_eq!(again, 0);
    }

    /// Copying an empty selection must not replace the clipboard.
    #[test]
    fn an_empty_copy_is_not_requested() {
        take_pending();
        request_text("");
        assert_eq!(drain_with(|_| true), 0);
    }

    /// A refused write is counted as not applied, so a caller can tell the two apart.
    #[test]
    fn a_refused_write_is_not_counted() {
        take_pending();
        request_text("text");
        assert_eq!(drain_with(|_| false), 0);
    }

    /// The queue is bounded: the oldest request is dropped rather than growing without limit.
    #[test]
    fn the_queue_keeps_the_most_recent_requests() {
        take_pending();
        for i in 0..(MAX_PENDING + 3) {
            request_text(&format!("t{i}"));
        }
        let queued = take_pending();
        assert_eq!(queued.len(), MAX_PENDING);
        assert_eq!(queued.first().map(String::as_str), Some("t3"), "the oldest three were dropped");
        assert_eq!(queued.last().map(String::as_str), Some("t10"));
    }
}
