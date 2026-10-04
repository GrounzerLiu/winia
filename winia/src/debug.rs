//! DevTools — a stdin + WebSocket channel that drives the UI from outside the process.
//!
//! stdin:  `echo 'c 190 130' | ./app`   # click
//!         `echo t | ./app`              # print the UI tree
//!         `echo r | ./app`              # request a screenshot
//! WebSocket: `ws://127.0.0.1:9998` (override with `WINIA_DEBUG_PORT` — the UI tests use that to
//! keep parallel runs apart); `wscat -c ws://localhost:9998` then type `c 190 130`.
//!
//! # Why this module is always compiled
//!
//! Only the server is behind the `debug-server` feature; the vocabulary is not. [`DebugEvent`] is
//! what `app` drains and matches on, so a build without the feature still names it — that is why
//! the type lives here and the two implementations sit beside it:
//!
//! - `debug/server.rs` — the real channel, with the feature;
//! - `debug/stub.rs`  — the same surface answering "nothing pending", without it.
//!
//! The stub used to be a module body inside `lib.rs`, with its own hand-written copy of
//! `DebugEvent` that nothing kept in step with this one.

#[cfg(feature = "debug-server")]
#[path = "debug/server.rs"]
mod imp;

/// Outside the feature the same surface is a set of no-ops, so call sites stay unconditional.
#[cfg(not(feature = "debug-server"))]
#[path = "debug/stub.rs"]
mod imp;

pub use imp::*;

/// One input a debug client injects into the event loop. `app` drains the queue with
/// [`take_queued_events`] and routes each variant to the same code path a real event takes.
#[derive(Debug, Clone)]
pub enum DebugEvent {
    Click { x: f32, y: f32 },
    Key { key: String },
    Text { value: String },
    Scroll { dx: f32, dy: f32 },
    Resize { w: f32, h: f32 },
    FocusNext,
    RequestFocus { id: u64 },
    /// 模拟指针按下（选择拖动的起点）
    PointerDown { x: f32, y: f32 },
    /// 模拟指针移动（拖动选择）
    PointerMove { x: f32, y: f32 },
    /// 模拟指针释放
    PointerUp { x: f32, y: f32 },
}
