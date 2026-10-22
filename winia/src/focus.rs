//! Focus request plumbing: the handle a component holds to ask for focus, and the
//! per-window queue the app drains at the next frame.

mod requester;

pub use requester::FocusRequester;
pub(crate) use requester::{focus_window, take_focus_requests, FocusWindowGuard};
