//! 应用壳 — run_app + 窗口管理 + 事件循环（多窗口）

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

/// 拖拽/嵌套滚动调试 trace 开关（debug_assertions 下 + 环境变量 WINIA_DRAG_TRACE）。
/// 收敛 7 处重复判断（review E1）——release 构建零成本。
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

/// 方向键焦点导航方向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FocusDir { Left, Right, Up, Down }

/// 查询窗口当前所在显示器的刷新率 → 帧间隔（mHz：300000 = 300Hz）。
/// 查询失败返回 None——调用方保留旧值（创建期才回退 16ms）。
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
    /// 渲染帧计数（vsync 研究——Fifo 下应 ~60fps）
    pub(crate) frame_counter: u64,
    /// 上次 request_redraw 时刻（request 节流独立计时——避免与渲染节流共用
    /// last_render_time 导致理论上的 2I 间隔减半：WM_PAINT 处理晚于 request（ε>0），
    /// 定时器按 I 唤醒时 now-last_render = I-ε < I 恒拦截）
    pub(crate) last_request_time: std::time::Instant,
    /// 帧间隔（屏幕刷新率对齐——窗口创建时从 monitor 获取；刷新率变化（显示器
    /// 切换）需重建窗口——当前不做动态跟踪）
    pub(crate) frame_interval: std::time::Duration,
    /// 强制渲染（resize/动画停止等必须显示的帧——跳过分支的请求链断裂修复）
    pub(crate) force_redraw: bool,
    /// 崩溃边界（P3-3）：连续渲染 panic 计数（防风暴停更）
    pub(crate) consecutive_panics: u32,
    /// 渲染已禁用（连续 panic 后停更——保留最后画面，不再自旋）
    pub(crate) render_disabled: bool,
    /// 上次渲染时间（帧率限制——Windows acquire 不阻塞 vsync，应用层节流 60fps）
    pub(crate) last_render_time: std::time::Instant,
    /// 上次刷新率查询时刻（Moved/ScaleFactorChanged 高频触发——300ms 去抖）
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


// ── 公共 API ──

// ── 公共 API ──

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

/// §3.6 坐标一致性回归：dispatch_ptr_event 传给 handler 的局部坐标必须与
/// hit_test/scene_to_node_local（同一坐标空间）一致——滚动容器内自定义
/// PointerEvent 的拖拽/capture 位置不偏移。

/// 双轨（exp/modifier-node）：KeyNode 与枚举 KbEvent 同场分发。
/// dispatch_key_to_focus 需 PerWindow（ ancestors 查找走 arena），此处用真实
/// PerWindow + Composer 组树验证端到端：枚举与 node 各收到一次，顺序为枚举先。

/// §3.8 nested scroll 链级集成测试：dispatch_nested_scroll_delta 的 pre/post
/// 顺序与 target 参与。树结构 root(connection R) → mid(connection M) →
/// target(scroll + connection T)。验证：
/// 1. pre 按 R→M→T 正序
/// 2. child 消费后 post 按 M→T→R 逆序（**含 target T**——TopAppBar 等
///    connection 挂在 scroll 容器节点上，排除 target 会破坏 post 回调）

/// The release-velocity floor at the fling call site, and the exception a paged list gets from it.

/// 拖拽滚动目标复现（用户报告：鼠标在外层内容上按下拖拽，内层却滚动了）。
/// 构造与 nested_scroll_demo 同几何的树：外层 scroll 视口 536（y=184..720），
/// 内层 scroll 视口 180（内容流 y=296，视觉 480..660，scroll_viewport 已设）。
/// 验证：鼠标在外层内容 0-7 区域（如 y=250）按下时，hit_test 命中的是外层
/// 内容节点而非内层——drag_scroll 目标选择（path 逆序找 scroll）应选外层。

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
