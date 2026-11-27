//! Live per-window state, and the handles that expose it.
//!
//! [`WindowId`] is minted when a `Window` declaration first composes, not when the platform window
//! appears, so a handle is always valid; [`WindowId::is_realized`] answers the platform question
//! separately.
//!
//! Everything here is UI-thread state. Platform writes are queued and applied by `AppState`, so a
//! setter never touches the platform window from inside a composition. Reads use tracked snapshots:
//! size, position, placement, realization, and focus report platform observations, while title,
//! visibility, and resizable report application-owned requests.

use crate::runtime::composer::ComposeCtx;

pub use super::window_shell::{AppBarEdge, AppBarError, AppBarOptions, AppBarStatus};
use crate::app::window_backdrop::{BackdropStatus, WindowBackdrop};
use crate::runtime::state::State;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
#[cfg(target_os = "linux")]
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicU64, Ordering};

/// One window, for the lifetime of the process.
///
/// Minted by the declaration, so it outlives the platform window it names: `is_realized()` is false
/// between the declaration and the platform creating the window, and false again after the window
/// is gone.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowId(pub(crate) u64);

impl WindowId {
    pub(crate) fn new(raw: u64) -> Self {
        WindowId(raw)
    }

    /// The process-wide raw id (the same number the debug server and the back queue use).
    pub fn raw(self) -> u64 {
        self.0
    }

    /// Whether the platform window exists right now.
    pub fn is_realized(self) -> bool {
        with_cell(self, |cell| cell.snapshot.get().realized).unwrap_or(false)
    }

    /// Whether the OS currently gives this window keyboard focus.
    pub fn is_focused(self) -> bool {
        with_cell(self, |cell| cell.snapshot.get().focused).unwrap_or(false)
    }

    /// Reactive insets for this window. Unrealized declarations share the controller that the
    /// future platform window will use. Unknown or destroyed ids return an independent zero controller.
    pub fn insets(self) -> crate::window::insets::WindowInsetsController {
        with_cell(self, |cell| cell.insets.clone())
            .unwrap_or_else(crate::window::insets::WindowInsetsController::new)
    }

    /// The declaration that owns this window's lifetime. Root windows return `None`.
    /// This is not a native parent/modal relationship; that API is not implemented yet.
    pub fn owner(self) -> Option<WindowId> {
        with_cell(self, |cell| cell.owner).flatten()
    }

    /// The live state of this window.
    pub fn state(self) -> WindowState {
        WindowState { id: self }
    }

    /// Observe semantic window events outside composition.
    ///
    /// The first item is the most recent event, or `None` before one has been observed. Delivery is
    /// conflated: writes between polls surface as the latest value once. Create the stream on the
    /// UI thread, like every other `WindowId` accessor. Dropping the stream unsubscribes it.
    pub fn events(self) -> crate::runtime::state::StateChanges<Option<WindowEvent>> {
        with_cell(self, |cell| cell.last_event.changes())
            .unwrap_or_else(|| State::new(None::<WindowEvent>).changes())
    }

    /// What this platform can do with a window at all.
    pub fn capabilities(self) -> WindowCapabilities {
        WindowCapabilities::platform()
    }

    /// Ask the platform to restore and focus this window. Unsupported platforms ignore it.
    pub fn activate(self) {
        if !self.capabilities().activate {
            return;
        }
        with_cell(self, |cell| cell.push(Command::Activate));
    }

    /// Compose-style alias for `activate`.
    pub fn request_focus(self) {
        self.activate();
    }

    /// Ask to close the window.
    ///
    /// This is a request: `on_close_requested` runs and the application may ignore it, in which
    /// case nothing happens. Use [`WindowId::destroy`] to close without asking.
    pub fn close(self) {
        with_cell(self, |cell| {
            cell.push(Command::CloseRequest(CloseReason::Command))
        });
    }

    /// Close the window now, without asking.
    pub fn destroy(self) {
        with_cell(self, |cell| cell.push(Command::Destroy));
    }
}

impl std::fmt::Debug for WindowId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "WindowId({})", self.0)
    }
}

/// Where a window sits in the platform's stacking model.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WindowPlacement {
    Windowed,
    Maximized,
    Fullscreen,
    Minimized,
}

/// Where a window's top-left corner goes.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum WindowPosition {
    /// The platform decides (the usual choice for a first window).
    Default,
    /// Logical pixels from the top-left corner of the desktop.
    At(f32, f32),
}

/// A semantic observation about a window's platform-owned state.
///
/// Events describe the state the platform actually applied, not a request made through
/// [`WindowState`]. Close requests remain on the dedicated close callback.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum WindowEvent {
    Resized {
        width: f32,
        height: f32,
    },
    Moved {
        position: WindowPosition,
    },
    PlacementChanged {
        placement: WindowPlacement,
    },
    FocusChanged {
        focused: bool,
    },
    ScaleFactorChanged {
        scale_factor: f32,
    },
    InsetsChanged {
        insets: crate::window::insets::WindowInsets,
    },
    /// A Windows app-bar request changed lifecycle state.
    AppBarStatusChanged(AppBarStatus),
    /// A system backdrop request changed state.
    BackdropStatusChanged(BackdropStatus),
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct GeometryPosition {
    pub x: f32,
    pub y: f32,
}

/// Versioned normal-window geometry stored under a declaration's persistence key.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct WindowGeometry {
    pub version: u64,
    pub width: f32,
    pub height: f32,
    pub position: Option<GeometryPosition>,
    pub placement: WindowPlacement,
    pub scale_factor: f32,
}

/// What the current platform can honour.
///
/// Android composites every winit window into one Activity surface, so position, placement,
/// multi-window and native close are all unavailable there; applications should branch on these
/// flags rather than on `#[cfg]`. `title` and `resizable` are accepted on Android and have no
/// effect; unsupported chrome and state features are normalized away.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct WindowCapabilities {
    pub position: bool,
    pub placement: bool,
    pub multi_window: bool,
    pub native_close: bool,
    pub activate: bool,
    pub always_on_top: bool,
    pub transparent: bool,
    /// Windows 11 build 22621 and later: whether system backdrop materials are supported.
    pub backdrop: bool,
    /// Windows only: whether taskbar-button hiding is supported.
    pub skip_taskbar: bool,
    /// Windows only: whether the `WS_EX_TOOLWINDOW` style is supported.
    pub tool_window: bool,
    /// Windows only: whether the `WS_EX_NOACTIVATE` style is supported.
    pub no_activate: bool,
    /// Windows only: whether application desktop toolbars are supported.
    pub app_bar: bool,
    pub decorations: bool,
    pub size_constraints: bool,
    /// Whether native child popup windows and platform popup positioning are available.
    ///
    /// This is true on Windows, macOS and Linux with the Wayland backend.
    pub native_popup: bool,
}

#[cfg(target_os = "linux")]
static WAYLAND: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "linux")]
pub(crate) fn set_wayland(wayland: bool) {
    WAYLAND.store(wayland, Ordering::Relaxed);
}

impl WindowCapabilities {
    /// Capabilities of the current platform and windowing backend.
    pub fn current() -> Self {
        Self::platform()
    }

    pub(crate) fn platform() -> Self {
        let desktop = !cfg!(target_os = "android");
        #[cfg(target_os = "linux")]
        let wayland = WAYLAND.load(Ordering::Relaxed);
        #[cfg(not(target_os = "linux"))]
        let wayland = false;
        Self::for_backend(desktop, wayland)
    }

    fn for_backend(desktop: bool, wayland: bool) -> Self {
        WindowCapabilities {
            position: desktop,
            placement: desktop,
            multi_window: desktop,
            native_close: desktop,
            activate: desktop && !wayland,
            always_on_top: desktop && !wayland,
            transparent: desktop,
            backdrop: cfg!(target_os = "windows") && desktop,
            skip_taskbar: cfg!(target_os = "windows") && desktop,
            tool_window: cfg!(target_os = "windows") && desktop,
            no_activate: cfg!(target_os = "windows") && desktop,
            app_bar: cfg!(target_os = "windows") && desktop,
            decorations: desktop,
            size_constraints: desktop,
            native_popup: desktop
                && (cfg!(target_os = "windows") || cfg!(target_os = "macos") || wayland),
        }
    }
}

/// The live state of one window.
///
/// A cheap handle: getting one costs a key lookup, and holding it past the window's life is safe
/// (every accessor then reports a default).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowState {
    id: WindowId,
}

impl WindowState {
    fn app_bar_owns_window(&self) -> bool {
        self.id.capabilities().app_bar
            && with_cell(self.id, |cell| {
                cell.snapshot.peek().app_bar_options.is_some()
            })
            .unwrap_or(false)
    }

    /// The current inner size in logical pixels.
    pub fn size(&self) -> (f32, f32) {
        with_cell(self.id, |cell| cell.snapshot.get().size).unwrap_or((0.0, 0.0))
    }

    /// The display scale most recently reported by the platform.
    pub fn scale_factor(&self) -> f32 {
        with_cell(self.id, |cell| cell.snapshot.get().scale_factor).unwrap_or(1.0)
    }

    /// Request an inner size. Only meaningful while the window is `Windowed`: with another
    /// placement the value is remembered and applied when the window returns to `Windowed`.
    pub fn set_size(&self, width: f32, height: f32) {
        if self.app_bar_owns_window()
            || with_cell(self.id, |cell| cell.terminal.get()).unwrap_or(true)
        {
            return;
        }
        match self.geometry_placement() {
            WindowPlacement::Windowed => {
                with_cell(self.id, |cell| cell.push(Command::Size(width, height)));
            }
            other => {
                // Remember it for the return to `Windowed`; do not fight the placement.
                with_cell(self.id, |cell| {
                    cell.deferred_size.set(Some((width, height)))
                });
                crate::debug_log!("[window] size deferred, placement {:?}", other);
            }
        }
    }

    /// The current outer position, when the platform reports one.
    pub fn position(&self) -> WindowPosition {
        with_cell(self.id, |cell| cell.snapshot.get().position).unwrap_or(WindowPosition::Default)
    }

    /// Request an outer position. `Default` clears a deferred request and otherwise leaves
    /// the platform-chosen position untouched; there is no portable "reset position" API.
    pub fn set_position(&self, position: WindowPosition) {
        if self.app_bar_owns_window()
            || with_cell(self.id, |cell| cell.terminal.get()).unwrap_or(true)
        {
            return;
        }
        with_cell(self.id, |cell| cell.requested_position.set(position));
        if position == WindowPosition::Default {
            with_cell(self.id, |cell| cell.deferred_position.set(None));
            return;
        }
        match self.geometry_placement() {
            WindowPlacement::Windowed => {
                with_cell(self.id, |cell| cell.push(Command::Position(position)));
            }
            other => {
                with_cell(self.id, |cell| cell.deferred_position.set(Some(position)));
                crate::debug_log!("[window] position deferred, placement {:?}", other);
            }
        }
    }

    /// Placement that governs geometry writes: application intent while it is non-windowed,
    /// otherwise the platform observation.
    fn geometry_placement(&self) -> WindowPlacement {
        with_cell(self.id, |cell| {
            let requested = cell.requested_placement.get();
            let observed = cell.snapshot.peek().placement;
            if requested == WindowPlacement::Windowed {
                observed
            } else {
                requested
            }
        })
        .unwrap_or(WindowPlacement::Windowed)
    }

    /// The placement reported by the platform. Before realization this is the initial seed.
    pub fn placement(&self) -> WindowPlacement {
        with_cell(self.id, |cell| cell.snapshot.get().placement)
            .unwrap_or(WindowPlacement::Windowed)
    }

    /// Request a placement. Leaving `Windowed` applies any deferred size and position on return.
    pub fn set_placement(&self, placement: WindowPlacement) {
        if self.app_bar_owns_window()
            || with_cell(self.id, |cell| cell.terminal.get()).unwrap_or(true)
        {
            return;
        }
        with_cell(self.id, |cell| {
            cell.requested_placement.set(placement);
            cell.push(Command::Placement(placement));
            if placement == WindowPlacement::Windowed {
                if let Some((width, height)) = cell.deferred_size.take() {
                    cell.push(Command::Size(width, height));
                }
                if let Some(position) = cell.deferred_position.take() {
                    cell.push(Command::Position(position));
                }
            }
        });
    }

    /// The application's current visibility request.
    pub fn visible(&self) -> bool {
        with_cell(self.id, |cell| cell.snapshot.get().visible).unwrap_or(false)
    }

    /// Show or hide the window. Hiding is not destruction: the composition and its state survive.
    pub fn set_visible(&self, visible: bool) {
        if with_cell(self.id, |cell| cell.terminal.get()).unwrap_or(true) {
            return;
        }
        with_cell(self.id, |cell| {
            cell.snapshot.update(|snapshot| snapshot.visible = visible);
            cell.push(Command::Visible(visible));
        });
    }

    /// The window title.
    pub fn title(&self) -> String {
        with_cell(self.id, |cell| cell.snapshot.get().title).unwrap_or_default()
    }

    /// Set the window title.
    pub fn set_title(&self, title: &str) {
        if with_cell(self.id, |cell| cell.terminal.get()).unwrap_or(true) {
            return;
        }
        with_cell(self.id, |cell| {
            cell.snapshot
                .update(|snapshot| snapshot.title = title.to_owned());
            cell.push(Command::Title(title.to_owned()));
        });
    }

    /// Whether the user may resize the window by dragging its edges.
    pub fn resizable(&self) -> bool {
        with_cell(self.id, |cell| cell.snapshot.get().resizable).unwrap_or(true)
    }

    /// Allow or forbid user resizing.
    pub fn set_resizable(&self, resizable: bool) {
        if self.app_bar_owns_window()
            || with_cell(self.id, |cell| cell.terminal.get()).unwrap_or(true)
        {
            return;
        }
        with_cell(self.id, |cell| {
            cell.snapshot
                .update(|snapshot| snapshot.resizable = resizable);
            cell.push(Command::Resizable(resizable));
        });
    }

    /// Whether the window is kept above normal windows.
    pub fn always_on_top(&self) -> bool {
        with_cell(self.id, |cell| cell.snapshot.get().always_on_top).unwrap_or(false)
    }

    /// Whether the declaration requested taskbar-button hiding.
    ///
    /// Windows only; other platforms normalize the request to `false`.
    pub fn skip_taskbar(&self) -> bool {
        with_cell(self.id, |cell| cell.snapshot.get().skip_taskbar).unwrap_or(false)
    }

    /// Whether the declaration requested the Windows tool-window style.
    ///
    /// Windows only; other platforms normalize the request to `false`.
    pub fn tool_window(&self) -> bool {
        with_cell(self.id, |cell| cell.snapshot.get().tool_window).unwrap_or(false)
    }

    /// Whether the declaration requested `WS_EX_NOACTIVATE`.
    ///
    /// Windows only; other platforms normalize the request to `false`.
    pub fn no_activate(&self) -> bool {
        with_cell(self.id, |cell| cell.snapshot.get().no_activate).unwrap_or(false)
    }

    /// The app-bar options carried by the declaration, if any.
    ///
    /// This is the requested configuration, not the shell-negotiated native rectangle. It remains
    /// available on unsupported platforms so callers can explain the fallback.
    pub fn app_bar_options(&self) -> Option<AppBarOptions> {
        with_cell(self.id, |cell| cell.snapshot.get().app_bar_options).unwrap_or(None)
    }

    /// Current app-bar lifecycle state. Non-Windows platforms report `Unsupported` when requested.
    pub fn app_bar_status(&self) -> AppBarStatus {
        with_cell(self.id, |cell| cell.snapshot.get().app_bar_status)
            .unwrap_or(AppBarStatus::Unregistered)
    }

    /// Request a always-on-top stacking level.
    pub fn set_always_on_top(&self, always_on_top: bool) {
        if self.app_bar_owns_window()
            || !self.id.capabilities().always_on_top
            || with_cell(self.id, |cell| cell.terminal.get()).unwrap_or(true)
        {
            return;
        }
        with_cell(self.id, |cell| {
            cell.snapshot
                .update(|snapshot| snapshot.always_on_top = always_on_top);
            cell.push(Command::AlwaysOnTop(always_on_top));
        });
    }

    /// Whether this window was created with a transparent native surface.
    pub fn transparent(&self) -> bool {
        with_cell(self.id, |cell| cell.snapshot.get().transparent).unwrap_or(false)
    }

    /// The system backdrop material this window asks for.
    ///
    /// This is the request, not the outcome: [`WindowState::backdrop_status`] reports what the
    /// platform did with it. The request is kept even where the platform cannot honour it.
    pub fn backdrop(&self) -> WindowBackdrop {
        with_cell(self.id, |cell| cell.snapshot.get().backdrop).unwrap_or(WindowBackdrop::None)
    }

    /// What the platform did with the backdrop request.
    pub fn backdrop_status(&self) -> BackdropStatus {
        with_cell(self.id, |cell| cell.snapshot.get().backdrop_status)
            .unwrap_or(BackdropStatus::NotRequested)
    }

    /// Request a system backdrop material.
    ///
    /// Windows applies it through `DWMWA_SYSTEMBACKDROP_TYPE`; other platforms keep the request and
    /// report [`BackdropStatus::Unsupported`]. Nothing else changes: a whole-window material only
    /// shows where the client area is not opaque, so request transparency and undecorated chrome
    /// yourself when that is what you want.
    pub fn set_backdrop(&self, backdrop: WindowBackdrop) {
        if with_cell(self.id, |cell| cell.terminal.get()).unwrap_or(true) {
            return;
        }
        let supported = self.id.capabilities().backdrop;
        with_cell(self.id, |cell| {
            cell.snapshot
                .update(|snapshot| snapshot.backdrop = backdrop);
            if supported {
                cell.push(Command::Backdrop(backdrop));
            }
        });
        let pending = if backdrop.is_material() {
            if supported {
                BackdropStatus::Pending
            } else {
                BackdropStatus::Unsupported
            }
        } else {
            BackdropStatus::NotRequested
        };
        set_backdrop_status(self.id, pending);
    }

    /// Whether the platform currently decorates the window.
    pub fn decorations(&self) -> bool {
        with_cell(self.id, |cell| cell.snapshot.get().decorations).unwrap_or(true)
    }

    /// Enable or disable native decorations.
    pub fn set_decorations(&self, decorations: bool) {
        if self.app_bar_owns_window()
            || !self.id.capabilities().decorations
            || with_cell(self.id, |cell| cell.terminal.get()).unwrap_or(true)
        {
            return;
        }
        with_cell(self.id, |cell| {
            cell.snapshot
                .update(|snapshot| snapshot.decorations = decorations);
            cell.push(Command::Decorations(decorations));
        });
    }

    /// Minimum requested inner size in logical pixels.
    pub fn minimum_size(&self) -> Option<(f32, f32)> {
        with_cell(self.id, |cell| cell.snapshot.get().minimum_size).flatten()
    }

    /// Set or clear the minimum inner size in logical pixels.
    pub fn set_minimum_size(&self, size: Option<(f32, f32)>) {
        let size = match size {
            Some(size) if valid_size_constraint(&size) => Some(size),
            None => None,
            Some(_) => return,
        };
        if self.app_bar_owns_window()
            || !self.id.capabilities().size_constraints
            || with_cell(self.id, |cell| cell.terminal.get()).unwrap_or(true)
        {
            return;
        }
        with_cell(self.id, |cell| {
            cell.snapshot
                .update(|snapshot| snapshot.minimum_size = size);
            cell.push(Command::MinimumSize(size));
        });
    }

    /// Maximum requested inner size in logical pixels.
    pub fn maximum_size(&self) -> Option<(f32, f32)> {
        with_cell(self.id, |cell| cell.snapshot.get().maximum_size).flatten()
    }

    /// Set or clear the maximum inner size in logical pixels.
    pub fn set_maximum_size(&self, size: Option<(f32, f32)>) {
        let size = match size {
            Some(size) if valid_size_constraint(&size) => Some(size),
            None => None,
            Some(_) => return,
        };
        if self.app_bar_owns_window()
            || !self.id.capabilities().size_constraints
            || with_cell(self.id, |cell| cell.terminal.get()).unwrap_or(true)
        {
            return;
        }
        with_cell(self.id, |cell| {
            cell.snapshot
                .update(|snapshot| snapshot.maximum_size = size);
            cell.push(Command::MaximumSize(size));
        });
    }
}

fn valid_size_constraint((width, height): &(f32, f32)) -> bool {
    width.is_finite() && height.is_finite() && *width > 0.0 && *height > 0.0
}

impl std::fmt::Debug for WindowState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "WindowState({})", self.id.0)
    }
}

/// Why a close was requested.
///
/// Only these two reach `on_close_requested`: a declaration removal and an application exit do not
/// ask for permission, they simply proceed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CloseReason {
    /// The user closed the window from its chrome.
    User,
    /// The application called `WindowId::close()`.
    Command,
}

/// One queued platform mutation.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Command {
    Size(f32, f32),
    Position(WindowPosition),
    Placement(WindowPlacement),
    Visible(bool),
    Title(String),
    Resizable(bool),
    Activate,
    AlwaysOnTop(bool),
    Decorations(bool),
    Backdrop(WindowBackdrop),
    MinimumSize(Option<(f32, f32)>),
    MaximumSize(Option<(f32, f32)>),
    /// Ask the application whether the window may close.
    CloseRequest(CloseReason),
    /// Close without asking.
    Destroy,
}

/// A command stamped at submission time so batches from different windows share one order.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct QueuedCommand {
    pub(crate) sequence: u64,
    pub(crate) command: Command,
}

static NEXT_COMMAND_SEQUENCE: AtomicU64 = AtomicU64::new(1);

/// The platform operations WindowState can request. Kept small so command ordering can be tested
/// without constructing a native window.
pub(crate) trait WindowCommandTarget {
    fn request_surface_size(&self, size: winit::dpi::Size);
    fn set_outer_position(&self, position: winit::dpi::Position);
    fn set_fullscreen(&self, fullscreen: Option<winit::monitor::Fullscreen>);
    fn set_maximized(&self, maximized: bool);
    fn set_minimized(&self, minimized: bool);
    fn set_visible(&self, visible: bool);
    fn set_title(&self, title: &str);
    fn set_resizable(&self, resizable: bool);
    fn focus_window(&self);
    fn set_window_level(&self, level: winit::window::WindowLevel);
    fn set_decorations(&self, decorations: bool);
    /// Apply a backdrop request; the default reports that the platform cannot.
    fn set_backdrop(&self, backdrop: WindowBackdrop) -> BackdropStatus {
        let _ = backdrop;
        BackdropStatus::Unsupported
    }
    fn set_min_surface_size(&self, size: Option<winit::dpi::Size>);
    fn set_max_surface_size(&self, size: Option<winit::dpi::Size>);
    fn request_redraw(&self);
}

impl WindowCommandTarget for dyn winit::window::Window {
    fn request_surface_size(&self, size: winit::dpi::Size) {
        let _ = winit::window::Window::request_surface_size(self, size);
    }

    fn set_outer_position(&self, position: winit::dpi::Position) {
        let _ = winit::window::Window::set_outer_position(self, position);
    }

    fn set_fullscreen(&self, fullscreen: Option<winit::monitor::Fullscreen>) {
        let _ = winit::window::Window::set_fullscreen(self, fullscreen);
    }

    fn set_maximized(&self, maximized: bool) {
        let _ = winit::window::Window::set_maximized(self, maximized);
    }

    fn set_minimized(&self, minimized: bool) {
        let _ = winit::window::Window::set_minimized(self, minimized);
    }

    fn set_visible(&self, visible: bool) {
        winit::window::Window::set_visible(self, visible);
    }

    fn set_title(&self, title: &str) {
        winit::window::Window::set_title(self, title);
    }

    fn set_resizable(&self, resizable: bool) {
        let _ = winit::window::Window::set_resizable(self, resizable);
    }

    fn focus_window(&self) {
        winit::window::Window::focus_window(self);
    }

    fn set_window_level(&self, level: winit::window::WindowLevel) {
        winit::window::Window::set_window_level(self, level);
    }

    fn set_decorations(&self, decorations: bool) {
        winit::window::Window::set_decorations(self, decorations);
    }

    fn set_backdrop(&self, backdrop: WindowBackdrop) -> BackdropStatus {
        crate::app::window_backdrop::apply(self, backdrop)
    }

    fn set_min_surface_size(&self, size: Option<winit::dpi::Size>) {
        let _ = winit::window::Window::set_min_surface_size(self, size);
    }

    fn set_max_surface_size(&self, size: Option<winit::dpi::Size>) {
        let _ = winit::window::Window::set_max_surface_size(self, size);
    }

    fn request_redraw(&self) {
        winit::window::Window::request_redraw(self);
    }
}

pub(crate) fn apply_command<T: WindowCommandTarget + ?Sized>(
    target: &T,
    command: Command,
) -> Option<BackdropStatus> {
    use winit::monitor::Fullscreen;
    match command {
        Command::Size(width, height) => target.request_surface_size(winit::dpi::Size::Logical(
            winit::dpi::LogicalSize::new(width as f64, height as f64),
        )),
        Command::Position(WindowPosition::At(x, y)) => {
            target.set_outer_position(winit::dpi::Position::Logical(
                winit::dpi::LogicalPosition::new(x as f64, y as f64),
            ));
        }
        Command::Position(WindowPosition::Default) => {}
        Command::Placement(WindowPlacement::Windowed) => {
            target.set_fullscreen(None);
            target.set_maximized(false);
            target.set_minimized(false);
        }
        Command::Placement(WindowPlacement::Maximized) => {
            target.set_fullscreen(None);
            target.set_minimized(false);
            target.set_maximized(true);
        }
        Command::Placement(WindowPlacement::Fullscreen) => {
            target.set_minimized(false);
            target.set_maximized(false);
            target.set_fullscreen(Some(Fullscreen::Borderless(None)));
        }
        Command::Placement(WindowPlacement::Minimized) => target.set_minimized(true),
        Command::Visible(visible) => target.set_visible(visible),
        Command::Title(title) => target.set_title(&title),
        Command::Resizable(resizable) => target.set_resizable(resizable),
        Command::Activate => {
            target.set_minimized(false);
            target.focus_window();
        }
        Command::AlwaysOnTop(always_on_top) => target.set_window_level(if always_on_top {
            winit::window::WindowLevel::AlwaysOnTop
        } else {
            winit::window::WindowLevel::Normal
        }),
        Command::Decorations(decorations) => target.set_decorations(decorations),
        Command::Backdrop(backdrop) => return Some(target.set_backdrop(backdrop)),
        Command::MinimumSize(size) => target.set_min_surface_size(size.map(|(width, height)| {
            winit::dpi::Size::Logical(winit::dpi::LogicalSize::new(width as f64, height as f64))
        })),
        Command::MaximumSize(size) => target.set_max_surface_size(size.map(|(width, height)| {
            winit::dpi::Size::Logical(winit::dpi::LogicalSize::new(width as f64, height as f64))
        })),
        Command::CloseRequest(_) | Command::Destroy => {}
    }
    None
}

/// Observable projection of one window. Reads through WindowState register the current
/// Composer as a dependent; platform observations and application-owned writes publish through State.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WindowSnapshot {
    pub(crate) app_bar_options: Option<AppBarOptions>,
    pub(crate) app_bar_status: AppBarStatus,
    pub(crate) realized: bool,
    pub(crate) focused: bool,
    pub(crate) size: (f32, f32),
    pub(crate) scale_factor: f32,
    pub(crate) position: WindowPosition,
    pub(crate) placement: WindowPlacement,
    pub(crate) visible: bool,
    pub(crate) resizable: bool,
    pub(crate) always_on_top: bool,
    pub(crate) transparent: bool,
    pub(crate) skip_taskbar: bool,
    pub(crate) tool_window: bool,
    pub(crate) no_activate: bool,
    pub(crate) decorations: bool,
    pub(crate) minimum_size: Option<(f32, f32)>,
    pub(crate) maximum_size: Option<(f32, f32)>,
    pub(crate) backdrop: WindowBackdrop,
    pub(crate) backdrop_status: BackdropStatus,
    pub(crate) title: String,
}

impl Default for WindowSnapshot {
    fn default() -> Self {
        Self {
            app_bar_options: None,
            app_bar_status: AppBarStatus::Unregistered,
            realized: false,
            focused: false,
            size: (0.0, 0.0),
            scale_factor: 1.0,
            position: WindowPosition::Default,
            placement: WindowPlacement::Windowed,
            visible: false,
            resizable: true,
            always_on_top: false,
            transparent: false,
            skip_taskbar: false,
            tool_window: false,
            no_activate: false,
            decorations: true,
            minimum_size: None,
            maximum_size: None,
            backdrop: WindowBackdrop::None,
            backdrop_status: BackdropStatus::NotRequested,
            title: String::new(),
        }
    }
}

/// What happens when the last window is destroyed.
///
/// Hiding a window is not destruction, so it never triggers this: an application that only hides
/// keeps running until it destroys something.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LastWindowPolicy {
    /// End the process (the default for an ordinary application).
    Exit,
    /// Keep running with no visible window (tray applications).
    StayResident,
}

thread_local! {
    static LAST_WINDOW_POLICY: Cell<LastWindowPolicy> = const { Cell::new(LastWindowPolicy::Exit) };
}

/// Choose what happens when the last window is destroyed.
pub(crate) fn set_last_window_policy(policy: LastWindowPolicy) {
    LAST_WINDOW_POLICY.with(|cell| cell.set(policy));
}

pub(crate) fn last_window_policy() -> LastWindowPolicy {
    LAST_WINDOW_POLICY.with(|cell| cell.get())
}

/// Turn an application-chosen key into the number the saveable store is keyed by.
///
/// FNV-1a rather than `DefaultHasher`: the value has to be identical across runs, and
/// `RandomState` is seeded per process. The prefix keeps these keys in their own space, away from
/// the declaration-site salted keys the framework mints internally.
pub(crate) fn persist_key(key: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in b"winia.window.".iter().chain(key.as_bytes()) {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    hash
}

type WindowEventHandler = Rc<dyn Fn(WindowId, WindowEvent)>;

/// The values a window starts with.
pub(crate) struct WindowSeed {
    pub(crate) app_bar: Option<AppBarOptions>,
    pub(crate) size: (f32, f32),
    pub(crate) title: String,
    pub(crate) resizable: bool,
    pub(crate) always_on_top: bool,
    pub(crate) transparent: bool,
    pub(crate) skip_taskbar: bool,
    pub(crate) tool_window: bool,
    pub(crate) no_activate: bool,
    pub(crate) decorations: bool,
    pub(crate) minimum_size: Option<(f32, f32)>,
    pub(crate) maximum_size: Option<(f32, f32)>,
    pub(crate) visible: bool,
    pub(crate) placement: WindowPlacement,
    pub(crate) position: WindowPosition,
    /// A notification, not a decision: the application expresses agreement by hiding, destroying,
    /// or dropping the declaration. No handler registered means "close without asking".
    ///
    /// The window id is passed in because the handler almost always acts on its own window
    /// (`destroy()`, `state().set_visible(false)`), and capturing it would require a `State` dance
    /// in the declaring scope.
    pub(crate) backdrop: WindowBackdrop,
    pub(crate) on_close_requested: Option<Rc<dyn Fn(WindowId, CloseReason)>>,
    pub(crate) on_event: Option<WindowEventHandler>,
}

impl Default for WindowSeed {
    fn default() -> Self {
        WindowSeed {
            app_bar: None,
            size: (300.0, 200.0),
            title: String::new(),
            resizable: true,
            always_on_top: false,
            transparent: false,
            skip_taskbar: false,
            tool_window: false,
            no_activate: false,
            decorations: true,
            minimum_size: None,
            maximum_size: None,
            visible: true,
            placement: WindowPlacement::Windowed,
            position: WindowPosition::Default,
            backdrop: WindowBackdrop::None,
            on_close_requested: None,
            on_event: None,
        }
    }
}

struct WindowCell {
    snapshot: State<WindowSnapshot>,
    /// Application intent, separate from the platform-observed placement in snapshot.
    requested_placement: Cell<WindowPlacement>,
    /// Position request used to seed native creation before the platform reports an observation.
    requested_position: Cell<WindowPosition>,
    /// Most recent windowed geometry, kept while another placement owns the native surface.
    normal_size: Cell<(f32, f32)>,
    normal_position: Cell<WindowPosition>,
    /// Last placement command handed to the platform, waiting for its observation.
    awaiting_placement: Cell<Option<WindowPlacement>>,
    /// Once an unconditional terminal command is queued, later setters are ignored.
    terminal: Cell<bool>,
    pending: RefCell<Vec<QueuedCommand>>,
    deferred_size: Cell<Option<(f32, f32)>>,
    deferred_position: Cell<Option<WindowPosition>>,
    on_close_requested: RefCell<Option<Rc<dyn Fn(WindowId, CloseReason)>>>,
    on_event: RefCell<Option<WindowEventHandler>>,
    /// Latest semantic observation, also backing `WindowId::events()`.
    last_event: State<Option<WindowEvent>>,
    /// Latest content published by the declaration. Consumed at the frame boundary so compose
    /// never replaces a live window's composer directly.
    pending_content: RefCell<Option<Box<dyn Fn(&mut ComposeCtx) + Send>>>,
    /// Reactive insets shared by WindowId and the window's composer.
    insets: crate::window::insets::WindowInsetsController,
    /// Declaration owner; root windows have none.
    owner: Option<WindowId>,
}

impl WindowCell {
    fn new(seed: WindowSeed, realized: bool) -> Self {
        let snapshot = WindowSnapshot {
            app_bar_options: seed.app_bar,
            realized,
            size: seed.size,
            scale_factor: 1.0,
            placement: seed.placement,
            position: seed.position,
            visible: seed.visible,
            resizable: seed.resizable,
            always_on_top: seed.always_on_top,
            transparent: seed.transparent,
            backdrop: seed.backdrop,
            backdrop_status: if seed.backdrop.is_material() {
                if cfg!(target_os = "windows") {
                    BackdropStatus::Pending
                } else {
                    BackdropStatus::Unsupported
                }
            } else {
                BackdropStatus::NotRequested
            },
            skip_taskbar: seed.skip_taskbar,
            tool_window: seed.tool_window,
            no_activate: seed.no_activate,
            app_bar_status: if seed.app_bar.is_some() {
                if cfg!(target_os = "windows") {
                    AppBarStatus::Registering
                } else {
                    AppBarStatus::Unsupported
                }
            } else {
                AppBarStatus::Unregistered
            },
            decorations: seed.decorations,
            minimum_size: seed.minimum_size,
            maximum_size: seed.maximum_size,
            title: seed.title,
            ..WindowSnapshot::default()
        };
        WindowCell {
            snapshot: State::new(snapshot),
            requested_placement: Cell::new(seed.placement),
            requested_position: Cell::new(seed.position),
            normal_size: Cell::new(seed.size),
            normal_position: Cell::new(seed.position),
            awaiting_placement: Cell::new(None),
            terminal: Cell::new(false),
            pending: RefCell::new(Vec::new()),
            deferred_size: Cell::new(None),
            deferred_position: Cell::new(None),
            on_close_requested: RefCell::new(seed.on_close_requested),
            on_event: RefCell::new(seed.on_event),
            last_event: State::new(None),
            pending_content: RefCell::new(None),
            insets: crate::window::insets::WindowInsetsController::new(),
            owner: None,
        }
    }

    fn push(&self, command: Command) {
        if self.terminal.get() {
            return;
        }
        let terminal = match command {
            Command::Destroy => true,
            Command::CloseRequest(_) => self.on_close_requested.borrow().is_none(),
            _ => false,
        };
        if terminal {
            self.terminal.set(true);
        }
        self.pending.borrow_mut().push(QueuedCommand {
            sequence: NEXT_COMMAND_SEQUENCE.fetch_add(1, Ordering::Relaxed),
            command,
        });
        crate::runtime::state::wake_loop();
    }
}

thread_local! {
    /// Every live window on this (the UI) thread. A `WindowId` used from another thread finds an
    /// empty table: setters become no-ops there, which is why window state is documented UI-only.
    static CELLS: RefCell<HashMap<u64, Rc<WindowCell>>> = RefCell::new(HashMap::new());
    /// Reactive list consumed by ComposeCtx::windows().
    static WINDOW_IDS: RefCell<Option<State<Vec<WindowId>>>> = const { RefCell::new(None) };
}

fn window_ids_state() -> State<Vec<WindowId>> {
    WINDOW_IDS.with(|slot| {
        if let Some(state) = slot.borrow().clone() {
            return state;
        }
        let state = State::new(Vec::new());
        *slot.borrow_mut() = Some(state.clone());
        state
    })
}

fn publish_window_ids() {
    let ids = CELLS.with(|cells| {
        let mut ids: Vec<u64> = cells.borrow().keys().copied().collect();
        ids.sort_unstable();
        ids.into_iter().map(WindowId::new).collect()
    });
    window_ids_state().set(ids);
}

fn with_cell<R>(id: WindowId, f: impl FnOnce(&WindowCell) -> R) -> Option<R> {
    CELLS.with(|cells| {
        let cells = cells.borrow();
        let cell = cells.get(&id.0)?;
        Some(f(cell))
    })
}

/// Register a declared window that the platform has not created yet.
pub(crate) fn declare(id: WindowId, seed: WindowSeed) {
    declare_owned(id, seed, None);
}

pub(crate) fn declare_owned(id: WindowId, seed: WindowSeed, owner: Option<WindowId>) {
    CELLS.with(|cells| {
        let mut cell = WindowCell::new(seed, false);
        cell.owner = owner;
        cells.borrow_mut().insert(id.0, Rc::new(cell));
    });
    publish_window_ids();
}

/// Publish the latest content closure for a declared window. The last write before the frame
/// boundary wins, including recompositions that happen before the platform window exists.
pub(crate) fn set_pending_content(id: WindowId, content: Box<dyn Fn(&mut ComposeCtx) + Send>) {
    with_cell(id, |cell| {
        *cell.pending_content.borrow_mut() = Some(content)
    });
}

/// Take the latest content published for a window, if its declaration recomposed.
pub(crate) fn take_pending_content(id: WindowId) -> Option<Box<dyn Fn(&mut ComposeCtx) + Send>> {
    with_cell(id, |cell| cell.pending_content.borrow_mut().take()).flatten()
}

/// Capture the normal-window geometry that should survive a close/reopen cycle.
pub(crate) fn capture_geometry(id: WindowId) -> Option<WindowGeometry> {
    with_cell(id, |cell| {
        let snapshot = cell.snapshot.peek();
        if snapshot.app_bar_options.is_some() && WindowCapabilities::platform().app_bar {
            return None;
        }
        let placement = if snapshot.placement == WindowPlacement::Minimized {
            WindowPlacement::Windowed
        } else {
            snapshot.placement
        };
        let position = match cell.normal_position.get() {
            WindowPosition::At(x, y) => Some(GeometryPosition { x, y }),
            WindowPosition::Default => None,
        };
        let (width, height) = cell.normal_size.get();
        Some(WindowGeometry {
            version: 1,
            width,
            height,
            position,
            placement,
            scale_factor: snapshot.scale_factor,
        })
    })
    .flatten()
}

/// Seed both the observable projection and the native-creation geometry request.
pub(crate) fn apply_geometry(id: WindowId, geometry: &WindowGeometry) {
    if WindowCapabilities::platform().app_bar
        && with_cell(id, |cell| cell.snapshot.peek().app_bar_options.is_some()).unwrap_or(false)
    {
        return;
    }
    let position = geometry
        .position
        .map(|position| WindowPosition::At(position.x, position.y))
        .unwrap_or(WindowPosition::Default);
    with_cell(id, |cell| {
        cell.snapshot.update(|snapshot| {
            snapshot.size = (geometry.width, geometry.height);
            snapshot.position = position;
            snapshot.placement = geometry.placement;
            snapshot.scale_factor = geometry.scale_factor;
        });
        cell.normal_size.set((geometry.width, geometry.height));
        cell.normal_position.set(position);
        cell.requested_position.set(position);
        cell.requested_placement.set(geometry.placement);
        cell.awaiting_placement.set(None);
        if geometry.placement == WindowPlacement::Windowed {
            cell.deferred_size.set(None);
            cell.deferred_position.set(None);
        } else {
            cell.deferred_size
                .set(Some((geometry.width, geometry.height)));
            cell.deferred_position
                .set((position != WindowPosition::Default).then_some(position));
        }
    });
}

/// Publish an app-bar lifecycle observation and notify the declaration once per change.
pub(crate) fn set_app_bar_status(id: WindowId, status: AppBarStatus) {
    let changed = with_cell(id, |cell| {
        if cell.snapshot.get().app_bar_status == status {
            false
        } else {
            cell.snapshot
                .update(|snapshot| snapshot.app_bar_status = status);
            true
        }
    })
    .unwrap_or(false);
    if changed {
        dispatch_event(id, WindowEvent::AppBarStatusChanged(status));
    }
}

/// Publish a backdrop observation and notify the declaration once per change.
pub(crate) fn set_backdrop_status(id: WindowId, status: BackdropStatus) {
    let changed = with_cell(id, |cell| {
        if cell.snapshot.get().backdrop_status == status {
            false
        } else {
            cell.snapshot
                .update(|snapshot| snapshot.backdrop_status = status);
            true
        }
    })
    .unwrap_or(false);
    if changed {
        dispatch_event(id, WindowEvent::BackdropStatusChanged(status));
    }
}

/// Mark a declared window as created by the platform, with the values it actually has.
pub(crate) fn realize(id: WindowId, size: (f32, f32)) {
    CELLS.with(|cells| {
        let cells = cells.borrow();
        let Some(cell) = cells.get(&id.0) else {
            return;
        };
        cell.snapshot.update(|snapshot| {
            snapshot.realized = true;
            snapshot.size = size;
        });
    });
}

/// Drop the state of a window that no longer exists.
pub(crate) fn forget(id: WindowId) {
    CELLS.with(|cells| {
        let cell = cells.borrow().get(&id.0).cloned();
        if let Some(cell) = cell {
            // Notify readers before dropping the cell so they recompose to the documented defaults.
            cell.snapshot.set(WindowSnapshot::default());
            cell.last_event.set(None);
            cell.insets
                .set(crate::window::insets::WindowInsets::default());
        }
        cells.borrow_mut().remove(&id.0);
    });
    publish_window_ids();
}

/// Position request to seed native creation, including a pre-realization reset to Default.
pub(crate) fn requested_position(id: WindowId) -> WindowPosition {
    with_cell(id, |cell| cell.requested_position.get()).unwrap_or(WindowPosition::Default)
}

/// Untracked application-owned visibility, for the event loop's first-frame decision.
pub(crate) fn visible_now(id: WindowId) -> bool {
    with_cell(id, |cell| cell.snapshot.peek().visible).unwrap_or(false)
}

/// Poll the platform-owned parts of a window's state. State deduplicates unchanged values.
pub(crate) fn observe_platform_state(
    window: &dyn winit::window::Window,
    id: WindowId,
    scale_factor: f64,
) {
    observed_scale_factor(id, scale_factor);
    if id.capabilities().position
        && let Ok(position) = window.outer_position()
    {
        let logical = position.to_logical::<f32>(scale_factor);
        observed_position(id, WindowPosition::At(logical.x, logical.y));
    }
    if id.capabilities().placement {
        let placement = if window.is_minimized() == Some(true) {
            WindowPlacement::Minimized
        } else if window.fullscreen().is_some() {
            WindowPlacement::Fullscreen
        } else if window.is_maximized() {
            WindowPlacement::Maximized
        } else {
            WindowPlacement::Windowed
        };
        observed_placement(id, placement);
    }
    observed_focus(id, window.has_focus());
}

/// The platform reported a new inner size.
pub(crate) fn observed_size(id: WindowId, size: (f32, f32)) {
    let changed = with_cell(id, |cell| {
        if cell.snapshot.peek().size == size {
            return false;
        }
        cell.snapshot.update(|snapshot| snapshot.size = size);
        if cell.snapshot.peek().placement == WindowPlacement::Windowed {
            cell.normal_size.set(size);
        }
        true
    })
    .unwrap_or(false);
    if changed {
        dispatch_event(
            id,
            WindowEvent::Resized {
                width: size.0,
                height: size.1,
            },
        );
    }
}

/// The platform reported an outer position in logical desktop coordinates.
pub(crate) fn observed_position(id: WindowId, position: WindowPosition) {
    let changed = with_cell(id, |cell| {
        if cell.snapshot.peek().position == position {
            return false;
        }
        cell.snapshot
            .update(|snapshot| snapshot.position = position);
        if cell.snapshot.peek().placement == WindowPlacement::Windowed {
            cell.normal_position.set(position);
        }
        true
    })
    .unwrap_or(false);
    if changed {
        dispatch_event(id, WindowEvent::Moved { position });
    }
}

/// The platform reported a placement change.
pub(crate) fn observed_placement(id: WindowId, placement: WindowPlacement) {
    let changed = with_cell(id, |cell| {
        let previous = cell.snapshot.peek().placement;
        let placement_pending = cell
            .pending
            .borrow()
            .iter()
            .any(|queued| matches!(queued.command, Command::Placement(_)));
        let awaiting = cell.awaiting_placement.get();
        cell.snapshot
            .update(|snapshot| snapshot.placement = placement);
        // Native chrome can supersede stale application intent, but not while an application
        // placement command is queued or waiting for the platform's asynchronous echo.
        if awaiting == Some(placement) {
            cell.awaiting_placement.set(None);
            cell.requested_placement.set(placement);
        } else if awaiting.is_none() && !placement_pending {
            cell.requested_placement.set(placement);
        }
        // The user can restore a window through native chrome. Apply geometry that was deferred
        // while the OS owned the placement, unless the application already consumed it.
        if previous != WindowPlacement::Windowed
            && placement == WindowPlacement::Windowed
            && cell.requested_placement.get() == WindowPlacement::Windowed
        {
            if let Some((width, height)) = cell.deferred_size.take() {
                cell.push(Command::Size(width, height));
            }
            if let Some(position) = cell.deferred_position.take() {
                cell.push(Command::Position(position));
            }
        }
        previous != placement
    })
    .unwrap_or(false);
    if changed {
        dispatch_event(id, WindowEvent::PlacementChanged { placement });
    }
}

/// The OS reported a keyboard-focus change.
pub(crate) fn observed_focus(id: WindowId, focused: bool) {
    let changed = with_cell(id, |cell| {
        if cell.snapshot.peek().focused == focused {
            return false;
        }
        cell.snapshot.update(|snapshot| snapshot.focused = focused);
        true
    })
    .unwrap_or(false);
    if changed {
        dispatch_event(id, WindowEvent::FocusChanged { focused });
    }
}

/// The platform reported a new display scale.
pub(crate) fn observed_scale_factor(id: WindowId, scale_factor: f64) {
    let scale_factor = scale_factor as f32;
    let changed = with_cell(id, |cell| {
        if cell.snapshot.peek().scale_factor == scale_factor {
            return false;
        }
        cell.snapshot
            .update(|snapshot| snapshot.scale_factor = scale_factor);
        true
    })
    .unwrap_or(false);
    if changed {
        dispatch_event(id, WindowEvent::ScaleFactorChanged { scale_factor });
    }
}

/// The platform reported a new system-inset configuration.
pub(crate) fn observed_insets(id: WindowId, insets: crate::window::insets::WindowInsets) {
    let changed = with_cell(id, |cell| {
        if cell.insets.current() == insets {
            return false;
        }
        cell.insets.set(insets);
        true
    })
    .unwrap_or(false);
    if changed {
        dispatch_event(id, WindowEvent::InsetsChanged { insets });
    }
}

fn dispatch_event(id: WindowId, event: WindowEvent) {
    let Some((last_event, handler)) = with_cell(id, |cell| {
        (cell.last_event.clone(), cell.on_event.borrow().clone())
    }) else {
        return;
    };
    last_event.set(Some(event));
    if let Some(handler) = handler {
        handler(id, event);
    }
}

/// Every live window, in creation order.
///
/// Ids are minted in declaration order and never reused, so ascending id order is creation order -
/// which is also the order Android composites several windows into its single Activity surface.
pub(crate) fn windows_in_creation_order() -> Vec<WindowId> {
    window_ids_state().get()
}

/// The application's close-request notification, if it registered one.
pub(crate) fn close_handler(id: WindowId) -> Option<Rc<dyn Fn(WindowId, CloseReason)>> {
    with_cell(id, |cell| cell.on_close_requested.borrow().clone()).flatten()
}

/// Replace the declaration's event handler. Recompositions publish the latest closure.
pub(crate) fn set_event_handler(id: WindowId, handler: Option<WindowEventHandler>) {
    with_cell(id, |cell| *cell.on_event.borrow_mut() = handler);
}

/// Take the queued mutations for one window, oldest first.
pub(crate) fn take_commands(id: WindowId) -> Vec<Command> {
    take_queued_commands(id)
        .into_iter()
        .map(|queued| queued.command)
        .collect()
}

/// Take commands with their global submission sequence.
pub(crate) fn take_queued_commands(id: WindowId) -> Vec<QueuedCommand> {
    with_cell(id, |cell| {
        let queued = std::mem::take(&mut *cell.pending.borrow_mut());
        for item in &queued {
            if let Command::Placement(placement) = item.command {
                cell.awaiting_placement.set(Some(placement));
            }
        }
        queued
    })
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wayland_reports_focus_and_stacking_as_unsupported() {
        let caps = WindowCapabilities::for_backend(true, true);
        assert!(!caps.activate);
        assert!(!caps.always_on_top);
        assert!(caps.decorations);
        assert!(caps.size_constraints);
    }

    #[test]
    fn x11_reports_focus_and_stacking_as_supported() {
        let caps = WindowCapabilities::for_backend(true, false);
        assert!(caps.activate);
        assert!(caps.always_on_top);
    }

    #[test]
    fn native_popup_capability_tracks_the_active_backend() {
        assert!(!WindowCapabilities::for_backend(false, false).native_popup);
        #[cfg(target_os = "linux")]
        {
            assert!(WindowCapabilities::for_backend(true, true).native_popup);
            assert!(!WindowCapabilities::for_backend(true, false).native_popup);
        }
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        assert!(WindowCapabilities::for_backend(true, false).native_popup);
    }

    #[test]
    fn app_bar_status_is_deduplicated_and_owns_geometry() {
        let id = WindowId::new(0xA55E_6A17);
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        declare(
            id,
            WindowSeed {
                app_bar: Some(AppBarOptions::bottom(48.0)),
                on_event: Some(Rc::new(move |_, event| sink.borrow_mut().push(event))),
                ..WindowSeed::default()
            },
        );
        assert_eq!(
            WindowState { id }.app_bar_options(),
            Some(AppBarOptions::bottom(48.0))
        );
        let initial = if cfg!(target_os = "windows") {
            AppBarStatus::Registering
        } else {
            AppBarStatus::Unsupported
        };
        assert_eq!(WindowState { id }.app_bar_status(), initial);
        assert_eq!(capture_geometry(id).is_some(), !cfg!(target_os = "windows"));

        let registered = AppBarStatus::Registered {
            edge: AppBarEdge::Bottom,
        };
        set_app_bar_status(id, registered);
        set_app_bar_status(id, registered);
        assert_eq!(WindowState { id }.app_bar_status(), registered);
        assert!(capture_geometry(id).is_none());
        set_app_bar_status(id, AppBarStatus::Unregistered);
        assert!(capture_geometry(id).is_none());
        assert_eq!(
            *seen.borrow(),
            vec![
                WindowEvent::AppBarStatusChanged(registered),
                WindowEvent::AppBarStatusChanged(AppBarStatus::Unregistered),
            ]
        );
        forget(id);
    }

    #[test]
    fn app_bar_event_handler_can_declare_another_window() {
        let id = WindowId::new(0xA55E_6A18);
        let child = WindowId::new(0xA55E_6A19);
        declare(
            id,
            WindowSeed {
                app_bar: Some(AppBarOptions::bottom(48.0)),
                on_event: Some(Rc::new(move |_, event| {
                    if matches!(event, WindowEvent::AppBarStatusChanged(_)) {
                        declare(child, WindowSeed::default());
                    }
                })),
                ..WindowSeed::default()
            },
        );
        set_app_bar_status(
            id,
            AppBarStatus::Registered {
                edge: AppBarEdge::Bottom,
            },
        );
        assert!(windows_in_creation_order().contains(&child));
        forget(child);
        forget(id);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn app_bar_setters_do_not_queue_conflicting_commands() {
        let id = WindowId::new(0xA55E_6A1A);
        declare(
            id,
            WindowSeed {
                app_bar: Some(AppBarOptions::bottom(48.0)),
                resizable: false,
                always_on_top: true,
                skip_taskbar: true,
                tool_window: true,
                ..WindowSeed::default()
            },
        );
        let state = WindowState { id };
        state.set_size(320.0, 240.0);
        state.set_position(WindowPosition::At(10.0, 20.0));
        state.set_placement(WindowPlacement::Fullscreen);
        state.set_resizable(true);
        state.set_always_on_top(false);
        state.set_decorations(false);
        state.set_minimum_size(Some((100.0, 100.0)));
        state.set_maximum_size(Some((500.0, 500.0)));
        assert_eq!(state.size(), (300.0, 200.0));
        assert_eq!(state.position(), WindowPosition::Default);
        assert_eq!(state.placement(), WindowPlacement::Windowed);
        assert!(!state.resizable());
        assert!(state.always_on_top());
        assert!(state.skip_taskbar());
        assert!(state.tool_window());
        assert!(!state.no_activate());
        assert_eq!(state.app_bar_options(), Some(AppBarOptions::bottom(48.0)));
        assert!(state.decorations());
        assert_eq!(state.minimum_size(), None);
        assert_eq!(state.maximum_size(), None);
        assert!(take_queued_commands(id).is_empty());
        forget(id);
    }

    fn fresh(raw: u64) -> WindowId {
        let id = WindowId::new(raw);
        declare(id, WindowSeed::default());
        id
    }

    #[test]
    fn window_events_report_only_changed_observations() {
        use crate::window::insets::{Insets, WindowInsets};

        let id = WindowId::new(9_401);
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        declare(
            id,
            WindowSeed {
                on_event: Some(Rc::new(move |event_id, event| {
                    assert_eq!(event_id, id);
                    sink.borrow_mut().push(event);
                })),
                ..WindowSeed::default()
            },
        );

        observed_size(id, (640.0, 480.0));
        observed_size(id, (640.0, 480.0));
        observed_position(id, WindowPosition::At(10.0, 20.0));
        observed_position(id, WindowPosition::At(10.0, 20.0));
        observed_placement(id, WindowPlacement::Maximized);
        observed_placement(id, WindowPlacement::Maximized);
        observed_focus(id, true);
        observed_focus(id, true);
        observed_scale_factor(id, 1.5);
        observed_scale_factor(id, 1.5);
        let insets = WindowInsets {
            ime: Insets::new(1.0, 2.0, 3.0, 4.0),
            ..WindowInsets::default()
        };
        observed_insets(id, insets);
        observed_insets(id, insets);

        assert_eq!(
            *seen.borrow(),
            vec![
                WindowEvent::Resized {
                    width: 640.0,
                    height: 480.0,
                },
                WindowEvent::Moved {
                    position: WindowPosition::At(10.0, 20.0),
                },
                WindowEvent::PlacementChanged {
                    placement: WindowPlacement::Maximized,
                },
                WindowEvent::FocusChanged { focused: true },
                WindowEvent::ScaleFactorChanged { scale_factor: 1.5 },
                WindowEvent::InsetsChanged { insets },
            ]
        );
        assert_eq!(id.state().scale_factor(), 1.5);
        forget(id);
    }

    #[test]
    fn window_event_flow_conflates_writes_between_polls() {
        use futures_util::Stream;
        use futures_util::task::noop_waker;
        use std::pin::Pin;
        use std::task::{Context, Poll};

        let id = fresh(9_402);
        let mut events = id.events();
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        assert_eq!(
            Pin::new(&mut events).poll_next(&mut cx),
            Poll::Ready(Some(None))
        );

        observed_size(id, (320.0, 240.0));
        observed_size(id, (640.0, 480.0));
        assert_eq!(
            Pin::new(&mut events).poll_next(&mut cx),
            Poll::Ready(Some(Some(WindowEvent::Resized {
                width: 640.0,
                height: 480.0,
            })))
        );
        observed_size(id, (640.0, 480.0));
        assert_eq!(Pin::new(&mut events).poll_next(&mut cx), Poll::Pending);

        observed_focus(id, true);
        assert_eq!(
            Pin::new(&mut events).poll_next(&mut cx),
            Poll::Ready(Some(Some(WindowEvent::FocusChanged { focused: true })))
        );
        forget(id);
        assert_eq!(
            Pin::new(&mut events).poll_next(&mut cx),
            Poll::Ready(Some(None))
        );
    }

    #[test]
    fn event_handler_can_be_replaced_and_can_write_window_state() {
        let id = fresh(9_403);
        let first = Rc::new(RefCell::new(Vec::new()));
        let first_sink = first.clone();
        set_event_handler(
            id,
            Some(Rc::new(move |_, event| first_sink.borrow_mut().push(event))),
        );
        observed_focus(id, true);

        set_event_handler(
            id,
            Some(Rc::new(move |event_id, _| {
                event_id.state().set_title("updated from event");
            })),
        );
        observed_focus(id, false);

        assert_eq!(
            *first.borrow(),
            vec![WindowEvent::FocusChanged { focused: true }]
        );
        assert_eq!(
            take_commands(id),
            vec![Command::Title("updated from event".into())]
        );
        forget(id);
    }

    #[test]
    fn geometry_capture_keeps_normal_size_while_maximized() {
        let id = WindowId::new(9_501);
        declare(
            id,
            WindowSeed {
                size: (800.0, 600.0),
                position: WindowPosition::At(100.0, 80.0),
                ..WindowSeed::default()
            },
        );
        observed_size(id, (800.0, 600.0));
        observed_position(id, WindowPosition::At(100.0, 80.0));
        observed_placement(id, WindowPlacement::Maximized);
        observed_size(id, (1920.0, 1080.0));
        observed_position(id, WindowPosition::At(0.0, 0.0));

        let geometry = capture_geometry(id).unwrap();
        assert_eq!(geometry.width, 800.0);
        assert_eq!(geometry.height, 600.0);
        assert_eq!(
            geometry.position,
            Some(GeometryPosition { x: 100.0, y: 80.0 })
        );
        assert_eq!(geometry.placement, WindowPlacement::Maximized);
        forget(id);
    }

    #[test]
    fn geometry_restore_seeds_deferred_normal_geometry() {
        let id = fresh(9_502);
        let geometry = WindowGeometry {
            version: 1,
            width: 800.0,
            height: 600.0,
            position: Some(GeometryPosition { x: 120.0, y: 90.0 }),
            placement: WindowPlacement::Maximized,
            scale_factor: 1.5,
        };
        apply_geometry(id, &geometry);
        assert_eq!(capture_geometry(id), Some(geometry));

        id.state().set_placement(WindowPlacement::Windowed);
        assert_eq!(
            take_commands(id),
            vec![
                Command::Placement(WindowPlacement::Windowed),
                Command::Size(800.0, 600.0),
                Command::Position(WindowPosition::At(120.0, 90.0)),
            ]
        );
        forget(id);
    }

    #[test]
    fn a_setter_queues_a_command_instead_of_touching_the_platform() {
        let id = fresh(9_001);
        let state = id.state();
        state.set_title("Settings");
        state.set_size(640.0, 480.0);
        assert_eq!(state.title(), "Settings");
        assert_eq!(
            take_commands(id),
            vec![
                Command::Title("Settings".into()),
                Command::Size(640.0, 480.0)
            ],
            "writes queue in submission order"
        );
        forget(id);
    }

    #[test]
    fn a_size_while_fullscreen_is_remembered_not_queued() {
        let id = fresh(9_002);
        let state = id.state();
        state.set_placement(WindowPlacement::Fullscreen);
        let _ = take_commands(id);

        state.set_size(800.0, 600.0);
        assert!(
            take_commands(id).is_empty(),
            "a fullscreen window must not be resized behind the placement"
        );

        state.set_placement(WindowPlacement::Windowed);
        assert!(
            take_commands(id).contains(&Command::Size(800.0, 600.0)),
            "the remembered size applies on the way back to Windowed"
        );
        forget(id);
    }

    #[test]
    fn size_reads_follow_the_platform_not_the_request() {
        let id = fresh(9_003);
        let state = id.state();
        state.set_size(700.0, 500.0);
        assert_eq!(
            state.size(),
            (300.0, 200.0),
            "the reported size is what the window has, not what was requested"
        );
        observed_size(id, (700.0, 500.0));
        assert_eq!(state.size(), (700.0, 500.0));
        forget(id);
    }

    #[test]
    fn a_window_that_is_not_registered_reports_defaults() {
        let id = WindowId::new(9_004);
        assert!(!id.is_realized());
        assert_eq!(id.state().size(), (0.0, 0.0));
        assert!(take_commands(id).is_empty());
    }

    #[test]
    fn a_close_handler_is_stored_with_its_window() {
        let id = WindowId::new(9_005);
        declare(
            id,
            WindowSeed {
                on_close_requested: Some(Rc::new(|_, _| {})),
                ..WindowSeed::default()
            },
        );
        assert!(close_handler(id).is_some());
        assert!(
            close_handler(WindowId::new(9_006)).is_none(),
            "an unregistered window has no handler"
        );
        forget(id);
    }

    #[test]
    fn terminal_commands_reject_later_eager_setter_writes() {
        let id = fresh(9_211);
        id.state().set_title("accepted");
        id.destroy();
        id.state().set_title("too late");
        id.state().set_visible(false);
        id.state().set_resizable(false);
        assert_eq!(id.state().title(), "accepted");
        assert!(id.state().visible());
        assert!(id.state().resizable());
        assert_eq!(
            take_commands(id),
            vec![Command::Title("accepted".into()), Command::Destroy]
        );
        forget(id);
    }

    #[test]
    fn close_and_destroy_queue_their_commands() {
        let id = WindowId::new(9_007);
        declare(
            id,
            WindowSeed {
                on_close_requested: Some(Rc::new(|_, _| {})),
                ..WindowSeed::default()
            },
        );
        id.close();
        id.destroy();
        assert_eq!(
            take_commands(id),
            vec![
                Command::CloseRequest(CloseReason::Command),
                Command::Destroy
            ],
            "a vetoable close is a request, destroy is a command"
        );
        forget(id);
    }

    #[test]
    fn accepted_close_rejects_later_commands_and_setters() {
        let id = fresh(9_212);
        id.close();
        id.destroy();
        id.state().set_title("too late");
        assert_eq!(
            take_commands(id),
            vec![Command::CloseRequest(CloseReason::Command)]
        );
        assert_eq!(id.state().title(), "");
        forget(id);
    }

    #[test]
    fn a_persist_key_is_stable_and_distinct() {
        assert_eq!(persist_key("settings"), persist_key("settings"));
        assert_ne!(persist_key("settings"), persist_key("preview"));
        assert_ne!(
            persist_key("settings"),
            0,
            "zero is the 'no key' sentinel used elsewhere"
        );
    }

    #[test]
    fn windows_are_listed_in_creation_order() {
        let a = WindowId::new(9_101);
        let b = WindowId::new(9_102);
        let c = WindowId::new(9_103);
        // Registered out of order on purpose: ids are minted by the declaration, so they are the
        // order Android also composites in.
        declare(c, WindowSeed::default());
        declare(a, WindowSeed::default());
        declare(b, WindowSeed::default());

        assert_eq!(windows_in_creation_order(), vec![a, b, c]);
        forget(a);
        forget(b);
        forget(c);
    }

    #[test]
    fn the_last_window_policy_is_process_wide_and_defaults_to_exit() {
        assert_eq!(last_window_policy(), LastWindowPolicy::Exit);
        set_last_window_policy(LastWindowPolicy::StayResident);
        assert_eq!(last_window_policy(), LastWindowPolicy::StayResident);
        set_last_window_policy(LastWindowPolicy::Exit);
        assert_eq!(last_window_policy(), LastWindowPolicy::Exit);
    }

    struct RecordingTarget {
        calls: RefCell<Vec<&'static str>>,
    }

    impl WindowCommandTarget for RecordingTarget {
        fn request_surface_size(&self, _: winit::dpi::Size) {
            self.calls.borrow_mut().push("size");
        }
        fn set_outer_position(&self, _: winit::dpi::Position) {
            self.calls.borrow_mut().push("position");
        }
        fn set_fullscreen(&self, _: Option<winit::monitor::Fullscreen>) {
            self.calls.borrow_mut().push("fullscreen");
        }
        fn set_maximized(&self, _: bool) {
            self.calls.borrow_mut().push("maximized");
        }
        fn set_minimized(&self, _: bool) {
            self.calls.borrow_mut().push("minimized");
        }
        fn set_visible(&self, _: bool) {
            self.calls.borrow_mut().push("visible");
        }
        fn set_title(&self, _: &str) {
            self.calls.borrow_mut().push("title");
        }
        fn set_resizable(&self, _: bool) {
            self.calls.borrow_mut().push("resizable");
        }
        fn focus_window(&self) {
            self.calls.borrow_mut().push("focus");
        }
        fn set_window_level(&self, _: winit::window::WindowLevel) {
            self.calls.borrow_mut().push("level");
        }
        fn set_decorations(&self, _: bool) {
            self.calls.borrow_mut().push("decorations");
        }
        fn set_min_surface_size(&self, _: Option<winit::dpi::Size>) {
            self.calls.borrow_mut().push("minimum-size");
        }
        fn set_max_surface_size(&self, _: Option<winit::dpi::Size>) {
            self.calls.borrow_mut().push("maximum-size");
        }
        fn request_redraw(&self) {
            self.calls.borrow_mut().push("redraw");
        }
    }

    #[test]
    fn windowed_placement_clears_every_other_placement_in_order() {
        let target = RecordingTarget {
            calls: RefCell::new(Vec::new()),
        };
        apply_command(&target, Command::Placement(WindowPlacement::Windowed));
        assert_eq!(
            *target.calls.borrow(),
            vec!["fullscreen", "maximized", "minimized"]
        );
    }

    #[test]
    fn activate_restores_then_focuses_the_native_window() {
        let target = RecordingTarget {
            calls: RefCell::new(Vec::new()),
        };
        apply_command(&target, Command::Activate);
        assert_eq!(*target.calls.borrow(), vec!["minimized", "focus"]);
    }

    #[test]
    fn runtime_window_properties_queue_platform_commands() {
        let id = fresh(9_216);
        id.state().set_always_on_top(true);
        id.state().set_decorations(false);
        id.state().set_minimum_size(Some((320.0, 240.0)));
        id.state().set_maximum_size(Some((1200.0, 900.0)));
        assert!(id.state().always_on_top());
        assert!(!id.state().decorations());
        assert_eq!(id.state().minimum_size(), Some((320.0, 240.0)));
        assert_eq!(id.state().maximum_size(), Some((1200.0, 900.0)));
        assert_eq!(
            take_commands(id),
            vec![
                Command::AlwaysOnTop(true),
                Command::Decorations(false),
                Command::MinimumSize(Some((320.0, 240.0))),
                Command::MaximumSize(Some((1200.0, 900.0))),
            ]
        );
        forget(id);
    }

    #[test]
    fn runtime_size_constraints_can_be_cleared() {
        let id = fresh(9_218);
        id.state().set_minimum_size(Some((320.0, 240.0)));
        id.state().set_maximum_size(Some((1200.0, 900.0)));
        id.state().set_minimum_size(None);
        id.state().set_maximum_size(None);

        assert_eq!(id.state().minimum_size(), None);
        assert_eq!(id.state().maximum_size(), None);
        assert_eq!(
            take_commands(id),
            vec![
                Command::MinimumSize(Some((320.0, 240.0))),
                Command::MaximumSize(Some((1200.0, 900.0))),
                Command::MinimumSize(None),
                Command::MaximumSize(None),
            ]
        );
        forget(id);
    }

    #[test]
    fn transparent_windows_can_enable_decorations() {
        let id = WindowId::new(9_217);
        declare(
            id,
            WindowSeed {
                transparent: true,
                decorations: false,
                ..WindowSeed::default()
            },
        );
        id.state().set_decorations(true);
        assert!(id.state().decorations());
        assert_eq!(take_commands(id), vec![Command::Decorations(true)]);
        forget(id);
    }

    #[test]
    fn a_declared_backdrop_reaches_the_snapshot() {
        let id = WindowId::new(9_218);
        declare(
            id,
            WindowSeed {
                backdrop: WindowBackdrop::Mica,
                ..WindowSeed::default()
            },
        );
        assert_eq!(id.state().backdrop(), WindowBackdrop::Mica);
        assert_eq!(
            id.state().backdrop_status(),
            if cfg!(target_os = "windows") {
                BackdropStatus::Pending
            } else {
                BackdropStatus::Unsupported
            }
        );
        forget(id);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn runtime_backdrop_requests_queue_in_submission_order() {
        let id = fresh(9_219);
        id.state().set_backdrop(WindowBackdrop::MicaAlt);
        id.state().set_decorations(false);
        assert_eq!(id.state().backdrop(), WindowBackdrop::MicaAlt);
        assert_eq!(
            take_commands(id),
            vec![
                Command::Backdrop(WindowBackdrop::MicaAlt),
                Command::Decorations(false),
            ]
        );
        assert_eq!(id.state().backdrop_status(), BackdropStatus::Pending);
        forget(id);
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn runtime_backdrop_requests_stay_unsupported_elsewhere() {
        let id = fresh(9_219);
        id.state().set_backdrop(WindowBackdrop::Mica);
        assert_eq!(id.state().backdrop(), WindowBackdrop::Mica);
        assert_eq!(id.state().backdrop_status(), BackdropStatus::Unsupported);
        assert!(take_commands(id).is_empty());
        forget(id);
    }

    #[test]
    fn clearing_a_backdrop_request_reports_no_material() {
        let id = fresh(9_221);
        id.state().set_backdrop(WindowBackdrop::Acrylic);
        let _ = take_commands(id);
        id.state().set_backdrop(WindowBackdrop::None);
        assert_eq!(id.state().backdrop(), WindowBackdrop::None);
        assert_eq!(id.state().backdrop_status(), BackdropStatus::NotRequested);
        #[cfg(target_os = "windows")]
        assert_eq!(
            take_commands(id),
            vec![Command::Backdrop(WindowBackdrop::None)]
        );
        forget(id);
    }

    #[test]
    fn backdrop_status_changes_reach_the_event_stream() {
        let id = WindowId::new(9_220);
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        declare(
            id,
            WindowSeed {
                on_event: Some(Rc::new(move |_, event| sink.borrow_mut().push(event))),
                ..WindowSeed::default()
            },
        );
        set_backdrop_status(id, BackdropStatus::Applied);
        set_backdrop_status(id, BackdropStatus::Applied);
        assert_eq!(id.state().backdrop_status(), BackdropStatus::Applied);
        assert_eq!(seen.borrow().len(), 1);
        assert!(matches!(
            seen.borrow()[0],
            WindowEvent::BackdropStatusChanged(BackdropStatus::Applied)
        ));
        forget(id);
    }

    #[test]
    fn commands_from_different_windows_share_submission_order() {
        let a = fresh(9_201);
        let b = fresh(9_202);
        a.state().set_title("first");
        b.state().set_size(640.0, 480.0);
        a.state().set_visible(false);

        let mut batch = Vec::new();
        for (raw, id) in [(a.raw(), a), (b.raw(), b)] {
            batch.extend(
                take_queued_commands(id)
                    .into_iter()
                    .map(|queued| (queued.sequence, raw, queued.command)),
            );
        }
        batch.sort_by_key(|(sequence, _, _)| *sequence);
        assert_eq!(
            batch
                .into_iter()
                .map(|(_, _, command)| command)
                .collect::<Vec<_>>(),
            vec![
                Command::Title("first".into()),
                Command::Size(640.0, 480.0),
                Command::Visible(false),
            ]
        );
        forget(a);
        forget(b);
    }

    #[test]
    fn native_restore_applies_deferred_geometry() {
        let id = fresh(9_207);
        observed_placement(id, WindowPlacement::Maximized);
        id.state().set_size(800.0, 600.0);
        id.state().set_position(WindowPosition::At(10.0, 20.0));
        assert!(take_commands(id).is_empty());
        observed_placement(id, WindowPlacement::Windowed);
        assert_eq!(
            take_commands(id),
            vec![
                Command::Size(800.0, 600.0),
                Command::Position(WindowPosition::At(10.0, 20.0)),
            ]
        );
        forget(id);
    }

    #[test]
    fn stale_observation_does_not_overwrite_a_waiting_placement_request() {
        let id = fresh(9_210);
        id.state().set_placement(WindowPlacement::Maximized);
        let _ = take_commands(id);
        observed_placement(id, WindowPlacement::Windowed);
        id.state().set_size(800.0, 600.0);
        assert!(
            take_commands(id).is_empty(),
            "geometry must keep deferring until the requested placement is observed"
        );
        observed_placement(id, WindowPlacement::Maximized);
        observed_placement(id, WindowPlacement::Windowed);
        assert_eq!(take_commands(id), vec![Command::Size(800.0, 600.0)]);
        forget(id);
    }

    #[test]
    fn native_restore_reconciles_stale_application_intent() {
        let id = fresh(9_209);
        id.state().set_placement(WindowPlacement::Minimized);
        let _ = take_commands(id);
        observed_placement(id, WindowPlacement::Minimized);
        id.state().set_size(800.0, 600.0);
        observed_placement(id, WindowPlacement::Windowed);
        assert_eq!(
            take_commands(id),
            vec![Command::Size(800.0, 600.0)],
            "native restore must apply deferred geometry"
        );
        id.state().set_size(900.0, 700.0);
        assert_eq!(
            take_commands(id),
            vec![Command::Size(900.0, 700.0)],
            "geometry writes after native restore must no longer defer"
        );
        forget(id);
    }

    #[test]
    fn default_position_cancels_a_pre_realization_seed() {
        let id = WindowId::new(9_215);
        declare(
            id,
            WindowSeed {
                position: WindowPosition::At(120.0, 80.0),
                ..WindowSeed::default()
            },
        );
        id.state().set_position(WindowPosition::Default);
        assert_eq!(requested_position(id), WindowPosition::Default);
        forget(id);
    }

    #[test]
    fn default_position_clears_a_deferred_request() {
        let id = fresh(9_208);
        observed_placement(id, WindowPlacement::Maximized);
        id.state().set_position(WindowPosition::At(10.0, 20.0));
        id.state().set_position(WindowPosition::Default);
        observed_placement(id, WindowPlacement::Windowed);
        assert!(take_commands(id).is_empty());
        forget(id);
    }

    #[test]
    fn owner_and_insets_follow_the_declaration_lifetime() {
        let parent = fresh(9_213);
        let child = WindowId::new(9_214);
        declare_owned(child, WindowSeed::default(), Some(parent));
        assert_eq!(parent.owner(), None);
        assert_eq!(child.owner(), Some(parent));

        let insets = child.insets();
        insets.set_status_bar_insets(crate::window::insets::Insets::all(12.0));
        assert_eq!(child.insets().current().status_bars.top, 12.0);

        forget(child);
        assert_eq!(child.owner(), None);
        assert_eq!(
            insets.current(),
            crate::window::insets::WindowInsets::default()
        );
        assert_eq!(
            child.insets().current(),
            crate::window::insets::WindowInsets::default()
        );
        forget(parent);
    }

    #[test]
    fn platform_observations_update_the_snapshot() {
        let id = fresh(9_203);
        observed_size(id, (640.0, 480.0));
        observed_position(id, WindowPosition::At(120.0, 80.0));
        observed_placement(id, WindowPlacement::Maximized);
        observed_focus(id, true);
        assert_eq!(id.state().size(), (640.0, 480.0));
        assert_eq!(id.state().position(), WindowPosition::At(120.0, 80.0));
        assert_eq!(id.state().placement(), WindowPlacement::Maximized);
        assert!(id.is_focused());
        forget(id);
    }

    #[test]
    fn a_tracked_window_state_read_recomposes_on_observation() {
        let id = fresh(9_204);
        let seen = Rc::new(Cell::new((0.0, 0.0)));
        let mut composer = crate::runtime::composer::Composer::new();
        let read = seen.clone();
        let observed = id;
        composer.compose(move |ctx| {
            let _scope = ctx.start_scope();
            read.set(observed.state().size());
            ctx.end_scope();
        });
        observed_size(id, (700.0, 500.0));
        let read = seen.clone();
        let observed = id;
        assert!(composer.recompose(move |ctx| {
            let _scope = ctx.start_scope();
            read.set(observed.state().size());
            ctx.end_scope();
        }));
        assert_eq!(seen.get(), (700.0, 500.0));
        forget(id);
    }

    #[test]
    fn a_tracked_window_list_read_recomposes_on_declare_and_forget() {
        let first = fresh(9_205);
        let seen = Rc::new(Cell::new(0usize));
        let mut composer = crate::runtime::composer::Composer::new();
        let read = seen.clone();
        composer.compose(move |ctx| {
            let _scope = ctx.start_scope();
            read.set(crate::runtime::composer::ComposeCtx::windows(ctx).len());
            ctx.end_scope();
        });
        let second = WindowId::new(9_206);
        declare(second, WindowSeed::default());
        let read = seen.clone();
        assert!(composer.recompose(move |ctx| {
            let _scope = ctx.start_scope();
            read.set(crate::runtime::composer::ComposeCtx::windows(ctx).len());
            ctx.end_scope();
        }));
        assert_eq!(seen.get(), 2);
        forget(second);
        forget(first);
    }

    #[test]
    fn capabilities_follow_the_platform() {
        let caps = WindowCapabilities::platform();
        assert_eq!(caps.multi_window, !cfg!(target_os = "android"));
        assert_eq!(caps.native_close, !cfg!(target_os = "android"));
    }
}
