//! Live per-window state, and the handles that expose it.
//!
//! [`WindowId`] is minted when a `Window` declaration first composes, not when the platform window
//! appears, so a handle is always valid; [`WindowId::is_realized`] answers the platform question
//! separately.
//!
//! Everything here is UI-thread state. Writes are queued and applied once per frame by `AppState`,
//! so a setter never touches the platform window from inside a composition. Reads are projections:
//! they report what the window is, not what the last setter asked for, except for the attributes the
//! application owns outright (title, visibility, resizable, placement).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

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
        with_cell(self, |cell| cell.realized.get()).unwrap_or(false)
    }

    /// Whether this window currently holds focus.
    pub fn is_focused(self) -> bool {
        with_cell(self, |cell| cell.focused.get()).unwrap_or(false)
    }

    /// The live state of this window.
    pub fn state(self) -> WindowState {
        WindowState { id: self }
    }

    /// What this platform can do with a window at all.
    pub fn capabilities(self) -> WindowCapabilities {
        WindowCapabilities::platform()
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
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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
    /// Logical pixels from the top-left corner of the display's work area.
    At(f32, f32),
}

/// What the current platform can honour.
///
/// Android composites every winit window into one Activity surface, so position, placement,
/// multi-window and native close are all unavailable there; applications should branch on these
/// flags rather than on `#[cfg]`. `title`, `decorations` and `resizable` are accepted on Android and
/// have no effect.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct WindowCapabilities {
    pub position: bool,
    pub placement: bool,
    pub multi_window: bool,
    pub native_close: bool,
}

impl WindowCapabilities {
    pub(crate) fn platform() -> Self {
        let desktop = !cfg!(target_os = "android");
        WindowCapabilities {
            position: desktop,
            placement: desktop,
            multi_window: desktop,
            native_close: desktop,
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
    /// The current inner size in logical pixels.
    pub fn size(&self) -> (f32, f32) {
        with_cell(self.id, |cell| cell.size.get()).unwrap_or((0.0, 0.0))
    }

    /// Request an inner size. Only meaningful while the window is `Windowed`: with another
    /// placement the value is remembered and applied when the window returns to `Windowed`.
    pub fn set_size(&self, width: f32, height: f32) {
        match with_cell(self.id, |cell| cell.placement.get()) {
            Some(WindowPlacement::Windowed) => {
                with_cell(self.id, |cell| cell.push(Command::Size(width, height)));
            }
            Some(other) => {
                // Remember it for the return to `Windowed`; do not fight the placement.
                with_cell(self.id, |cell| {
                    cell.deferred_size.set(Some((width, height)))
                });
                crate::debug_log!("[window] size deferred, placement {:?}", other);
            }
            None => {}
        }
    }

    /// The current outer position, when the platform reports one.
    pub fn position(&self) -> WindowPosition {
        with_cell(self.id, |cell| cell.position.get()).unwrap_or(WindowPosition::Default)
    }

    /// Request an outer position. Remembered while the window is not `Windowed`.
    pub fn set_position(&self, position: WindowPosition) {
        match with_cell(self.id, |cell| cell.placement.get()) {
            Some(WindowPlacement::Windowed) => {
                with_cell(self.id, |cell| cell.push(Command::Position(position)));
            }
            Some(other) => {
                with_cell(self.id, |cell| cell.deferred_position.set(Some(position)));
                crate::debug_log!("[window] position deferred, placement {:?}", other);
            }
            None => {}
        }
    }

    /// The current placement.
    pub fn placement(&self) -> WindowPlacement {
        with_cell(self.id, |cell| cell.placement.get()).unwrap_or(WindowPlacement::Windowed)
    }

    /// Request a placement. Leaving `Windowed` applies any deferred size and position on return.
    pub fn set_placement(&self, placement: WindowPlacement) {
        with_cell(self.id, |cell| {
            cell.placement.set(placement);
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

    /// Whether the window is mapped by the platform.
    pub fn visible(&self) -> bool {
        with_cell(self.id, |cell| cell.visible.get()).unwrap_or(false)
    }

    /// Show or hide the window. Hiding is not destruction: the composition and its state survive.
    pub fn set_visible(&self, visible: bool) {
        with_cell(self.id, |cell| {
            cell.visible.set(visible);
            cell.push(Command::Visible(visible));
        });
    }

    /// The window title.
    pub fn title(&self) -> String {
        with_cell(self.id, |cell| cell.title.borrow().clone()).unwrap_or_default()
    }

    /// Set the window title.
    pub fn set_title(&self, title: &str) {
        with_cell(self.id, |cell| {
            *cell.title.borrow_mut() = title.to_owned();
            cell.push(Command::Title(title.to_owned()));
        });
    }

    /// Whether the user may resize the window by dragging its edges.
    pub fn resizable(&self) -> bool {
        with_cell(self.id, |cell| cell.resizable.get()).unwrap_or(true)
    }

    /// Allow or forbid user resizing.
    pub fn set_resizable(&self, resizable: bool) {
        with_cell(self.id, |cell| {
            cell.resizable.set(resizable);
            cell.push(Command::Resizable(resizable));
        });
    }
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
    /// Ask the application whether the window may close.
    CloseRequest(CloseReason),
    /// Close without asking.
    Destroy,
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

/// The values a window starts with.
pub(crate) struct WindowSeed {
    pub(crate) size: (f32, f32),
    pub(crate) title: String,
    pub(crate) resizable: bool,
    pub(crate) visible: bool,
    pub(crate) placement: WindowPlacement,
    /// A notification, not a decision: the application expresses agreement by hiding, destroying,
    /// or dropping the declaration. No handler registered means "close without asking".
    ///
    /// The window id is passed in because the handler almost always acts on its own window
    /// (`destroy()`, `state().set_visible(false)`), and capturing it would require a `State` dance
    /// in the declaring scope.
    pub(crate) on_close_requested: Option<Rc<dyn Fn(WindowId, CloseReason)>>,
    /// The application-chosen persistence key (`.key("settings")`), when there is one.
    pub(crate) persist_key: Option<u64>,
}

impl Default for WindowSeed {
    fn default() -> Self {
        WindowSeed {
            size: (300.0, 200.0),
            title: String::new(),
            resizable: true,
            visible: true,
            placement: WindowPlacement::Windowed,
            on_close_requested: None,
            persist_key: None,
        }
    }
}

struct WindowCell {
    realized: Cell<bool>,
    focused: Cell<bool>,
    size: Cell<(f32, f32)>,
    position: Cell<WindowPosition>,
    placement: Cell<WindowPlacement>,
    visible: Cell<bool>,
    resizable: Cell<bool>,
    title: RefCell<String>,
    pending: RefCell<Vec<Command>>,
    deferred_size: Cell<Option<(f32, f32)>>,
    deferred_position: Cell<Option<WindowPosition>>,
    on_close_requested: RefCell<Option<Rc<dyn Fn(WindowId, CloseReason)>>>,
    persist_key: Option<u64>,
}

impl WindowCell {
    fn new(seed: WindowSeed, realized: bool) -> Self {
        WindowCell {
            realized: Cell::new(realized),
            focused: Cell::new(false),
            size: Cell::new(seed.size),
            position: Cell::new(WindowPosition::Default),
            placement: Cell::new(seed.placement),
            visible: Cell::new(seed.visible),
            resizable: Cell::new(seed.resizable),
            title: RefCell::new(seed.title),
            pending: RefCell::new(Vec::new()),
            deferred_size: Cell::new(None),
            deferred_position: Cell::new(None),
            on_close_requested: RefCell::new(seed.on_close_requested),
            persist_key: seed.persist_key,
        }
    }

    fn push(&self, command: Command) {
        self.pending.borrow_mut().push(command);
        crate::runtime::state::wake_loop();
    }
}

thread_local! {
    /// Every live window on this (the UI) thread. A `WindowId` used from another thread finds an
    /// empty table: setters become no-ops there, which is why window state is documented UI-only.
    static CELLS: RefCell<HashMap<u64, Rc<WindowCell>>> = RefCell::new(HashMap::new());
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
    CELLS.with(|cells| {
        cells
            .borrow_mut()
            .insert(id.0, Rc::new(WindowCell::new(seed, false)));
    });
}

/// Mark a declared window as created by the platform, with the values it actually has.
pub(crate) fn realize(id: WindowId, size: (f32, f32), title: &str) {
    CELLS.with(|cells| {
        let cells = cells.borrow();
        let Some(cell) = cells.get(&id.0) else {
            return;
        };
        cell.realized.set(true);
        cell.size.set(size);
        *cell.title.borrow_mut() = title.to_owned();
    });
}

/// Drop the state of a window that no longer exists.
pub(crate) fn forget(id: WindowId) {
    CELLS.with(|cells| {
        cells.borrow_mut().remove(&id.0);
    });
}

/// The platform reported a new inner size.
pub(crate) fn observed_size(id: WindowId, size: (f32, f32)) {
    with_cell(id, |cell| cell.size.set(size));
}

/// The platform reported a focus change.
pub(crate) fn observed_focus(id: WindowId, focused: bool) {
    with_cell(id, |cell| cell.focused.set(focused));
}

/// The application-chosen persistence key of a window, when it declared one.
pub(crate) fn persist_key_of(id: WindowId) -> Option<u64> {
    with_cell(id, |cell| cell.persist_key).flatten()
}

/// Every live window, in creation order.
///
/// Ids are minted in declaration order and never reused, so ascending id order is creation order -
/// which is also the order Android composites several windows into its single Activity surface.
pub(crate) fn windows_in_creation_order() -> Vec<WindowId> {
    CELLS.with(|cells| {
        let mut ids: Vec<u64> = cells.borrow().keys().copied().collect();
        ids.sort_unstable();
        ids.into_iter().map(WindowId::new).collect()
    })
}

/// The application's close-request notification, if it registered one.
pub(crate) fn close_handler(id: WindowId) -> Option<Rc<dyn Fn(WindowId, CloseReason)>> {
    with_cell(id, |cell| cell.on_close_requested.borrow().clone()).flatten()
}

/// Take the queued mutations for one window, oldest first.
pub(crate) fn take_commands(id: WindowId) -> Vec<Command> {
    with_cell(id, |cell| std::mem::take(&mut *cell.pending.borrow_mut())).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh(raw: u64) -> WindowId {
        let id = WindowId::new(raw);
        declare(id, WindowSeed::default());
        id
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
    fn close_and_destroy_queue_their_commands() {
        let id = fresh(9_007);
        id.close();
        id.destroy();
        assert_eq!(
            take_commands(id),
            vec![
                Command::CloseRequest(CloseReason::Command),
                Command::Destroy
            ],
            "close is a request, destroy is a command"
        );
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

    #[test]
    fn capabilities_follow_the_platform() {
        let caps = WindowCapabilities::platform();
        assert_eq!(caps.multi_window, !cfg!(target_os = "android"));
        assert_eq!(caps.native_close, !cfg!(target_os = "android"));
    }
}
