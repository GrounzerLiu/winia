//! The debug server's surface for a build without the `debug-server` feature.
//!
//! Every function answers "nothing pending" and does nothing, so call sites stay unconditional —
//! `app` calls eight of these on its normal path. The list is deliberately short: it names what the
//! rest of the crate actually calls, and a new unconditional call site that the real server has and
//! this file does not will fail to compile here rather than silently do nothing.

use super::DebugEvent;

pub fn start_stdin_channel() {}
pub fn start_ws_server() {}
pub fn begin_session() {}
pub fn end_session() {}
pub fn has_pending() -> bool { false }
pub fn set_wake_callback(_cb: impl Fn() + Send + Sync + 'static) {}
pub fn set_event_loop_proxy(_proxy: winit::event_loop::EventLoopProxy) {}
pub fn update_tree(_window_id: u64, _json: &str) {}
pub fn set_overlay_trees(_window_id: u64, _trees: Vec<(u64, (f32, f32), String)>) {}
pub fn remove_tree(_window_id: u64) {}
pub fn screenshot_requested(_window_id: u64) -> bool { false }
pub fn screenshot_done(_window_id: u64) {}
pub fn wake() {}
pub fn force_shutdown() {}
pub fn is_shutdown() -> bool { false }
pub fn update_pixels(_window_id: u64, _pixels: &[u8], _width: u32, _height: u32) {}
pub fn update_frame_passes(_window_id: u64, _passes: u8) {}
pub fn clear_frame_passes() {}
pub fn set_legacy_target(_window_id: u64) {}
pub fn take_queued_events(_window_id: u64) -> Vec<DebugEvent> { Vec::new() }
pub fn queued_event_targets() -> std::collections::HashSet<u64> { std::collections::HashSet::new() }
pub fn queue_event(_event: DebugEvent) {}
pub fn simulate_native_click(_x: f32, _y: f32) {}
pub fn build_tree_json(_nodes: &[crate::layout::node::LayoutNode], _root_idx: usize) -> String { String::new() }
