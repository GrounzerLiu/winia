//! The app shell: run_app, window management and the event loop (multi-window).

pub mod gesture;
pub mod overlay_host;

use gesture::{
    GestureState, PtrDownState,    drag_scroll_up, fire_gesture_action, fire_in_gesture_arena, gesture_down,
    gesture_move, gesture_up, inner_component_drag, path_drag_idx, path_scroll_idx,
    press_gesture_target, process_pending_taps_on_down,
};
use overlay_host::{
    OverlayHost,
    begin_overlay_close, claim_keyboard_for_overlay, exec_overlay_click, focus_scope_is_open, hit_overlay, keyboard_scope,
    layout_overlays, overlay_down, overlay_drag_up, overlay_exit_hover_at,
    overlay_update_hover, render_overlays, sync_overlays,
};
pub mod scroll;
pub mod window;

use scroll::{
    dispatch_nested_scroll_delta, dispatch_nested_scroll_fling,
    find_scroll_target, scroll_delta_with_shift, DragScroll,
};

use crate::runtime::composer::{ComposeCtx, Composer};
use crate::debug;
use crate::layout::constraints::Constraints;
use crate::layout::node::{hit_test_with_flights, focus_next, focus_prev, LayoutNode};
use crate::render;

/// Drag/nested-scroll debug trace switch (debug_assertions builds plus the WINIA_DRAG_TRACE env var).
/// Consolidates seven repeated checks (review E1); a release build pays nothing.
#[cfg(debug_assertions)]
pub(crate) fn drag_trace_enabled() -> bool {
    std::env::var("WINIA_DRAG_TRACE").is_ok()
}
#[cfg(not(debug_assertions))]
pub(crate) fn drag_trace_enabled() -> bool {
    false
}

pub(crate) struct PendingWindow {
    pub width: f32,
    pub height: f32,
    pub title: String,
    pub content: Option<Box<dyn Fn(&mut ComposeCtx) + Send>>,
    pub on_close: Option<Box<dyn FnMut() + Send>>,
    pub created_id: Option<u64>,
    /// Stable key for saveable state, when the caller has one (the declaring call site's salted key).
    /// `created_id` is minted fresh for every window, so it cannot carry state across a reopen.
    pub saveable_key: Option<u64>,
    pub theme: Option<crate::theme::WindowTheme>,
}
use skiwin::capture::{request_capture, take_capture};
use skiwin::{SkiaWindow, SkiaWindowTrait};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use winit::application::ApplicationHandler;
use winit::event::{StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{ImeRequest, WindowId};

/// Tell the window whether it takes IME text input.
///
/// `Window::set_ime_allowed(bool)` is deprecated; the replacement is an `ImeRequest`, which carries
/// the cursor area and the hint/purpose alongside the flag. winia has no model for either — no
/// component asks for a numeric keyboard and nothing reports the caret's screen box to the OS (the
/// gap `docs/text-field.md` records) — so this reproduces what winit's own deprecated shim did
/// (`winit-core/src/window.rs:1338-1356`) and keeps the two parts separable: the enable path is the
/// one that has to change when the hint plumbing lands, and the disable path is `ImeRequest::Disable`
/// exactly.
///
/// The request returns a `Result`; winit's platforms treat a refused IME update as non-fatal, and
/// there is nothing here to fall back to, so it is dropped like the shim drops it.
pub(crate) fn set_ime_enabled(sw: &SkiaWindow, enabled: bool) {
    let request = if enabled {
        ime_enable_request()
    } else {
        ImeRequest::Disable
    };
    let _ = sw.request_ime_update(request);
}

/// The enable half, with winit's placeholder values spelled out.
///
/// The cursor area is `0x0 at 0,0` — winit's own comment where this came from reads "there's nothing
/// sensible to use here by default". A real one needs the focused node's caret rectangle in window
/// coordinates, which is what the composed tree knows and the OS does not.
fn ime_enable_request() -> ImeRequest {
    use winit::window::{ImeCapabilities, ImeEnableRequest, ImeHint, ImePurpose, ImeRequestData};
    let capabilities = ImeCapabilities::new().with_hint_and_purpose().with_cursor_area();
    let data = ImeRequestData::default()
        .with_hint_and_purpose(ImeHint::NONE, ImePurpose::Normal)
        .with_cursor_area(
            winit::dpi::LogicalPosition::new(0.0, 0.0).into(),
            winit::dpi::LogicalSize::new(0.0, 0.0).into(),
        );
    ImeRequest::Enable(
        ImeEnableRequest::new(capabilities, data)
            .expect("the capabilities and the data above cover the same two fields"),
    )
}

/// Arrow-key focus navigation direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FocusDir { Left, Right, Up, Down }

/// Queries the refresh rate of the monitor the window is on and returns the frame interval (mHz:
/// 300000 = 300 Hz). None when the query fails; the caller keeps the old value (creation falls back to 16 ms).
fn current_frame_interval(sw: &SkiaWindow) -> Option<std::time::Duration> {
    let mhz = sw.current_monitor()
        .and_then(|m| m.current_video_mode())
        .and_then(|v| v.refresh_rate_millihertz())
        .map(|m| m.get())?;
    Some(std::time::Duration::from_nanos(1_000_000_000_000 / mhz as u64))
}

// ── WindowThemeState ──

/// What this window draws with, and what it has already drawn with.
///
/// The pair has to move together: `applied` is the comparison that decides whether a theme change
/// needs the tree to run again (a component resolved its colours and type when it composed, so a
/// new palette is invisible until it recomposes). One field on `PerWindow` now, two values that
/// only mean anything next to each other.
pub(crate) struct WindowThemeState {
    /// The palette this window clears with, and the typography/direction its tree resolved. Shared
    /// with the `Window` node that manages the window (which re-samples all of it every frame) —
    /// see `theme::WindowTheme`.
    pub(crate) cell: crate::theme::WindowTheme,
    /// The values `cell` last resolved to, i.e. what the tree has already drawn with.
    pub(crate) applied: crate::theme::AppliedTheme,
}

impl WindowThemeState {
    /// A window with the given intent: its palette is whatever the caller passed (nothing to follow),
    /// and its type scale is the default.
    pub(crate) fn from_theme(theme: crate::theme::ThemeColors) -> Self {
        let cell = crate::theme::WindowTheme::new(crate::theme::ThemeSpec::Fixed(theme));
        let applied = cell.applied();
        Self { cell, applied }
    }
}

// ── FrameClock ──

/// The frame clock: the throttle, the counters, and the give-up state.
///
/// Split out of `PerWindow` because these eight fields are only ever read and written by the frame
/// loop's own bookkeeping — nothing outside `app.rs` touches them, and together they are what makes
/// "should this frame be drawn" have an answer.
pub(crate) struct FrameClock {
    /// Rendered-frame counter (vsync study: under Fifo this should sit near 60 fps).
    pub(crate) frame_counter: u64,
    /// When request_redraw last ran (the request throttle keeps its own clock so it does not share
    /// last_render_time with the render throttle, which would halve the theoretical 2I interval:
    /// WM_PAINT is handled later than the request (ε > 0), so a timer waking at I sees
    /// now - last_render = I - ε < I and is always rejected).
    pub(crate) last_request_time: std::time::Instant,
    /// Frame interval, aligned to the screen refresh rate (read from the monitor at window creation;
    /// a refresh-rate change needs a new window, since there is no dynamic tracking today).
    pub(crate) frame_interval: std::time::Duration,
    /// Forced render: frames that must be drawn (resize, animations stopping), fixing the broken
    /// request chain when a frame is skipped.
    pub(crate) force_redraw: bool,
    /// Crash boundary (P3-3): consecutive render panics, so a panic storm stops redrawing.
    pub(crate) consecutive_panics: u32,
    /// Rendering is disabled (after consecutive panics: keep the last frame, stop spinning).
    pub(crate) render_disabled: bool,
    /// Last render time (frame-rate cap: on Windows acquire does not block on vsync, so the app
    /// throttles to 60 fps).
    pub(crate) last_render_time: std::time::Instant,
    /// Last refresh-rate query (Moved/ScaleFactorChanged fire often, so this debounces for 300 ms).
    pub(crate) last_refresh_check: std::time::Instant,
}

impl Default for FrameClock {
    fn default() -> Self {
        Self {
            frame_counter: 0,
            last_request_time: std::time::Instant::now(),
            frame_interval: std::time::Duration::from_millis(16),
            force_redraw: false,
            consecutive_panics: 0,
            render_disabled: false,
            last_render_time: std::time::Instant::now(),
            last_refresh_check: std::time::Instant::now(),
        }
    }
}


// ── Public API ──

// ── Public API ──

static GLOBAL_PENDING: std::sync::Mutex<Vec<PendingWindow>> = std::sync::Mutex::new(Vec::new());
static APP_PROXY: Mutex<Option<winit::event_loop::EventLoopProxy>> = Mutex::new(None);
/// Windows whose frame has been requested from code that cannot reach them (a `Window` node lives in its
/// declaring tree, the window in another composer). `created_id`s, not `WindowId`s: the declarer knows the
/// former, the app loop the latter.
static PENDING_REDRAW: Mutex<Vec<u64>> = Mutex::new(Vec::new());

/// Ask for `created_id`'s window to be redrawn — see [`PENDING_REDRAW`]. Wakes the loop, so it is safe
/// from a background thread too.
pub(crate) fn request_redraw_created(created_id: u64) {
    PENDING_REDRAW.lock().unwrap().push(created_id);
    wake_impl();
}

fn take_redraw_requests() -> Vec<u64> {
    std::mem::take(&mut *PENDING_REDRAW.lock().unwrap())
}

pub fn open_window_with_title(width: f32, height: f32, title: String, content: Option<Box<dyn Fn(&mut ComposeCtx) + Send>>, on_close: Option<Box<dyn FnMut() + Send>>, created_id: Option<u64>, theme: Option<crate::theme::WindowTheme>) {
    open_window_with_saveable_key(width, height, title, content, on_close, created_id, None, theme);
}

/// [`open_window_with_title`] plus a STABLE key for saveable state.
///
/// The key has to survive a close: it is what a re-opened window is matched against. `created_id`
/// cannot do that job (a fresh one is minted per window), so the declarative `Window` passes the
/// salted key of its own call site.
pub fn open_window_with_saveable_key(width: f32, height: f32, title: String, content: Option<Box<dyn Fn(&mut ComposeCtx) + Send>>, on_close: Option<Box<dyn FnMut() + Send>>, created_id: Option<u64>, saveable_key: Option<u64>, theme: Option<crate::theme::WindowTheme>) {
    GLOBAL_PENDING.lock().unwrap().push(PendingWindow { width, height, title, content, on_close, created_id, saveable_key, theme });
    wake_impl();
}

fn wake_impl() {
    if let Some(ref proxy) = *APP_PROXY.lock().unwrap() {
        let _ = proxy.wake_up();
        return;
    }
    debug::wake();
}

fn take_pending_windows() -> Vec<PendingWindow> {
    std::mem::take(&mut *GLOBAL_PENDING.lock().unwrap())
}


/// Apply a change of this window's theme: re-resolve it, and when what was drawn is now wrong, re-run the
/// tree — no component looks at the theme by itself, they resolved their colours, type scale and direction
/// when they composed.
///
/// Per WINDOW on purpose: the system theme is process-wide, but "already applied" is not (a single global
/// pending flag is consumed by whichever window renders first, which left every other window — and every
/// sub-window of a tree that switched its own theme node — on its old palette). A `Fixed` window with the
/// default type scale resolves the same values again and stops here, so a system theme change costs it
/// nothing.




/// Which node a press lands on, measured — the rule material3's `SecondaryEditable` anchor depends on.
///
/// material3 consumes the pointer DOWN there so a click on the icon inside the field does not move the
/// caret (`ExposedDropdownMenu.kt:1427-1429`). winia has no consume flag; what it has instead is "the
/// press goes to the innermost node on the hit path that declares a gesture"
/// (`press_gesture_target`). These tests pin that rule where the menu anchor needs it, including the
/// trap a first attempt hits: a `Clickable` is NOT a gesture (`Modifier::has_gesture` lists the tap and
/// drag callbacks only), so an anchor element carrying just `clickable` still lets the field's own
/// press — the caret placement — fire.

/// §3.6 coordinate-consistency regression: the local coordinates dispatch_ptr_event hands a handler
/// must match hit_test/scene_to_node_local (the same coordinate space), so a custom PointerEvent's
/// drag/capture position inside a scrolling container is not offset.

/// Dual track (exp/modifier-node): KeyNode and the enum KbEvent are dispatched together.
/// dispatch_key_to_focus needs a PerWindow (the ancestor lookup walks the arena), so this builds a
/// real PerWindow + Composer tree and checks end to end: the enum and the node each receive the
/// event once, enum first.

/// §3.8 chain-level nested-scroll integration test: dispatch_nested_scroll_delta's pre/post order and
/// target participation. The tree is root(connection R) -> mid(connection M) ->
/// target(scroll + connection T). Verifies:
/// 1. pre runs in forward order R -> M -> T
/// 2. after the child consumes, post runs in reverse order M -> T -> R (**including target T**: a
///    TopAppBar-style connection hangs off the scroll container node, and excluding the target would
///    break its post callback)

/// The release-velocity floor at the fling call site, and the exception a paged list gets from it.

/// Drag-scroll target reproduction (user report: pressing on the outer content and dragging scrolled
/// the inner list). Builds a tree with the same geometry as nested_scroll_demo: an outer scroll
/// viewport of 536 (y=184..720) and an inner one of 180 (content flow at y=296, visually 480..660,
/// scroll_viewport set). Verifies that a press in the outer content's 0-7 band (say y=250) has
/// hit_test hit the outer content node, not the inner one, so drag_scroll's target selection (walk the
/// path in reverse for a scroll) picks the outer node.

pub fn run_app(app: impl FnOnce(&mut ComposeCtx) + 'static) {
    // Own the tokio runtime: effects (LaunchedEffect, debug WS server) need
    // `Handle::try_current()` during composition. Demos must NOT create their
    // own runtime (nesting `Runtime::new()` under an entered guard is a no-op
    // shadow — spawns would land on the orphaned runtime, never driven).
    let owned = match tokio::runtime::Handle::try_current() {
        Ok(_) => None,
        Err(_) => Some(tokio::runtime::Runtime::new().expect("tokio runtime")),
    };
    // Hold the guard above all blocking calls (EventLoop::run_app never
    // returns): `Runtime::new()` panics ("Cannot start a runtime from within
    // a runtime") if called while entered; `Runtime::block_on` on a nested
    // `Runtime::new()` hits the same panic.
    let _guard = owned.as_ref().map(|rt| rt.enter());
    let event_loop = EventLoop::new().expect("event loop");
    debug::begin_session();
    let proxy = event_loop.create_proxy();
    debug::set_event_loop_proxy(proxy.clone());
    let proxy2 = proxy.clone();
    *APP_PROXY.lock().unwrap() = Some(proxy);
    crate::runtime::state::set_wake_fn(move || { let _ = proxy2.wake_up(); });
    debug::start_stdin_channel();
    debug::start_ws_server();
    let state = AppState {
        init: Some(Box::new(app)),
        was_animating: false,
        saveable: HashMap::new(),
        windows: HashMap::new(),
        pending_content: Vec::new(),
        parent_window_id: None,
    };
    event_loop.run_app(state).expect("run_app");
    debug::end_session();
}


/// The wheel's page step: one notch pages, a stream of tiny high-resolution events has to bank a notch's
/// worth of travel before any of them may, and a plain list never pages at all.
///
/// These pin the decisions `WheelPager` and `wheel_page_step` make, which is where a pager's feel lives.
/// Without the gate, a 3 px `PixelDelta` at 120 Hz turned a page per event (a dozen pages for one
/// flick); without the shared finder, the overlay layers could not page at all and a HorizontalPager
/// could not be moved by a plain wheel.




#[cfg(test)]
mod window_theme_tests;
#[cfg(test)]
mod frame_throttle_tests;
#[cfg(test)]
mod press_target_tests;
#[cfg(test)]
mod pointer_dispatch_coord_tests;
#[cfg(test)]
mod key_node_dual_track_tests;
#[cfg(test)]
mod drag_target_selection_tests;
#[cfg(test)]
mod wheel_paging_tests;
#[cfg(test)]
mod pointer_icon_tests;

mod app_state;
mod per_window;
mod wheel;

pub(crate) use app_state::*;
pub(crate) use per_window::*;
pub(crate) use wheel::*;
