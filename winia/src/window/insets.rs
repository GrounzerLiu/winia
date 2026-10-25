//! Window insets — Compose's WindowInsets, desktop-shaped.
//!
//! A window declares how much of its surface is taken by system chrome: status bars, navigation bars,
//! the on-screen keyboard, a display cutout and the system gesture areas. Modifiers pad content by the
//! parts that are currently occupied, and mark them consumed for the subtree, so two nested
//! status_bars_padding calls do not double-pad.
//!
//! On Windows the status bar does not exist. The intended use is a custom window decoration (a
//! transparent title bar that acts like Android's status bar): it publishes its own height through
//! WindowInsetsController::set_status_bar_insets (or the free set_status_bars), and every component
//! that asks for status-bar padding moves below it. A platform backend can push the same values with
//! WindowInsetsController::set when the OS reports them (Android, or winit's IME/insets events).

use std::cell::Cell;
use std::sync::LazyLock;

use crate::runtime::composition_local::CompositionLocal;
use crate::runtime::state::State;

/// Insets in logical pixels — Compose's InsetsValues.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Insets {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Insets {
    pub const ZERO: Self = Self { left: 0.0, top: 0.0, right: 0.0, bottom: 0.0 };

    pub const fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self { left, top, right, bottom }
    }

    pub const fn all(value: f32) -> Self {
        Self { left: value, top: value, right: value, bottom: value }
    }

    /// The union of two insets — the larger value per side.
    pub fn max(self, other: Self) -> Self {
        Self {
            left: self.left.max(other.left),
            top: self.top.max(other.top),
            right: self.right.max(other.right),
            bottom: self.bottom.max(other.bottom),
        }
    }

    /// What remains after other is consumed — never negative.
    pub fn exclude(self, other: Self) -> Self {
        Self {
            left: (self.left - other.left).max(0.0),
            top: (self.top - other.top).max(0.0),
            right: (self.right - other.right).max(0.0),
            bottom: (self.bottom - other.bottom).max(0.0),
        }
    }

    pub fn is_zero(self) -> bool {
        self.left == 0.0 && self.top == 0.0 && self.right == 0.0 && self.bottom == 0.0
    }

    pub fn horizontal_sum(self) -> f32 {
        self.left + self.right
    }

    pub fn vertical_sum(self) -> f32 {
        self.top + self.bottom
    }
}

/// The window's insets, split by kind — Compose's WindowInsets object graph, as one value.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct WindowInsets {
    pub status_bars: Insets,
    pub navigation_bars: Insets,
    pub ime: Insets,
    pub display_cutout: Insets,
    pub system_gestures: Insets,
    pub tappable_elements: Insets,
}

impl WindowInsets {
    /// Status bars plus navigation bars.
    pub fn system_bars(&self) -> Insets {
        self.status_bars.max(self.navigation_bars)
    }

    /// Bars plus IME plus cutout — the area content must stay out of to remain visible.
    pub fn safe_drawing(&self) -> Insets {
        self.system_bars().max(self.ime).max(self.display_cutout)
    }

    /// The areas a system back/home gesture can start in.
    pub fn safe_gestures(&self) -> Insets {
        self.system_bars().max(self.display_cutout).max(self.system_gestures)
    }

    /// Safe drawing, safe gestures and tappable elements — the broadest safe area.
    pub fn safe_content(&self) -> Insets {
        self.safe_drawing().max(self.safe_gestures()).max(self.tappable_elements)
    }

    /// The part of the gesture area that the system always keeps for itself.
    pub fn mandatory_system_gestures(&self) -> Insets {
        self.display_cutout.max(self.system_gestures)
    }
}

/// Which part of the window insets a modifier asks for — Compose's WindowInsets.statusBars etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsetsKind {
    StatusBars,
    NavigationBars,
    SystemBars,
    Ime,
    DisplayCutout,
    SystemGestures,
    TappableElements,
    SafeDrawing,
    SafeGestures,
    SafeContent,
    MandatorySystemGestures,
}

impl InsetsKind {
    /// The value this kind resolves to in window, before consumption.
    pub fn value(self, window: WindowInsets) -> Insets {
        match self {
            InsetsKind::StatusBars => window.status_bars,
            InsetsKind::NavigationBars => window.navigation_bars,
            InsetsKind::SystemBars => window.system_bars(),
            InsetsKind::Ime => window.ime,
            InsetsKind::DisplayCutout => window.display_cutout,
            InsetsKind::SystemGestures => window.system_gestures,
            InsetsKind::TappableElements => window.tappable_elements,
            InsetsKind::SafeDrawing => window.safe_drawing(),
            InsetsKind::SafeGestures => window.safe_gestures(),
            InsetsKind::SafeContent => window.safe_content(),
            InsetsKind::MandatorySystemGestures => window.mandatory_system_gestures(),
        }
    }

    fn sources(self) -> &'static [Source] {
        use Source::*;
        match self {
            InsetsKind::StatusBars => &[Status],
            InsetsKind::NavigationBars => &[Navigation],
            InsetsKind::SystemBars => &[Status, Navigation],
            InsetsKind::Ime => &[Ime],
            InsetsKind::DisplayCutout => &[Cutout],
            InsetsKind::SystemGestures => &[Gestures],
            InsetsKind::TappableElements => &[Tappable],
            InsetsKind::SafeDrawing => &[Status, Navigation, Ime, Cutout],
            InsetsKind::SafeGestures => &[Status, Navigation, Cutout, Gestures],
            InsetsKind::SafeContent => &[Status, Navigation, Ime, Cutout, Gestures, Tappable],
            InsetsKind::MandatorySystemGestures => &[Cutout, Gestures],
        }
    }

    fn source_value(self, window: WindowInsets, source: Source) -> Insets {
        match source {
            Source::Status => window.status_bars,
            Source::Navigation => window.navigation_bars,
            Source::Ime => window.ime,
            Source::Cutout => window.display_cutout,
            Source::Gestures => window.system_gestures,
            Source::Tappable => window.tappable_elements,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Source {
    Status,
    Navigation,
    Ime,
    Cutout,
    Gestures,
    Tappable,
}

/// Which inset sources a subtree has already consumed — per-field, because kinds overlap (system bars
/// are status plus navigation, and a safe-area kind is the max of several sources).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Consumed {
    status: Insets,
    navigation: Insets,
    ime: Insets,
    cutout: Insets,
    gestures: Insets,
    tappable: Insets,
}

impl Consumed {
    fn union(self, other: Self) -> Self {
        Self {
            status: self.status.max(other.status),
            navigation: self.navigation.max(other.navigation),
            ime: self.ime.max(other.ime),
            cutout: self.cutout.max(other.cutout),
            gestures: self.gestures.max(other.gestures),
            tappable: self.tappable.max(other.tappable),
        }
    }

    fn get(self, source: Source) -> Insets {
        match source {
            Source::Status => self.status,
            Source::Navigation => self.navigation,
            Source::Ime => self.ime,
            Source::Cutout => self.cutout,
            Source::Gestures => self.gestures,
            Source::Tappable => self.tappable,
        }
    }

    fn set(self, source: Source, value: Insets) -> Self {
        let mut next = self;
        match source {
            Source::Status => next.status = next.status.max(value),
            Source::Navigation => next.navigation = next.navigation.max(value),
            Source::Ime => next.ime = next.ime.max(value),
            Source::Cutout => next.cutout = next.cutout.max(value),
            Source::Gestures => next.gestures = next.gestures.max(value),
            Source::Tappable => next.tappable = next.tappable.max(value),
        }
        next
    }
}

thread_local! {
    static CONSUMED: Cell<Consumed> = const { Cell::new(Consumed {
        status: Insets::ZERO,
        navigation: Insets::ZERO,
        ime: Insets::ZERO,
        cutout: Insets::ZERO,
        gestures: Insets::ZERO,
        tappable: Insets::ZERO,
    }) };
}

/// What the current subtree has already consumed.
pub(crate) fn consumed() -> Consumed {
    CONSUMED.with(|cell| cell.get())
}

/// How much of kind is still available once the ancestors' consumption is subtracted.
pub(crate) fn remaining(kind: InsetsKind, window: WindowInsets, consumed: Consumed) -> Insets {
    let mut out = Insets::ZERO;
    for &source in kind.sources() {
        out = out.max(kind.source_value(window, source).exclude(consumed.get(source)));
    }
    out
}

/// Record kind's value as consumed for the subtree, merging with what ancestors already consumed.
pub(crate) fn consume(kind: InsetsKind, window: WindowInsets, consumed: Consumed) -> Consumed {
    let mut next = consumed;
    for &source in kind.sources() {
        next = next.set(source, kind.source_value(window, source));
    }
    next
}

/// Installs a consumption value for the duration of a node's measure; restores it on drop.
pub(crate) struct ConsumeScope {
    previous: Consumed,
}

impl ConsumeScope {
    pub(crate) fn enter(extra: Consumed) -> Self {
        let previous = CONSUMED.with(|cell| cell.get());
        let next = previous.union(extra);
        CONSUMED.with(|cell| cell.set(next));
        Self { previous }
    }
}

impl Drop for ConsumeScope {
    fn drop(&mut self) {
        CONSUMED.with(|cell| cell.set(self.previous));
    }
}

/// Read and update the window's insets — Compose's WindowInsets + WindowInsetsController.
///
/// Hold it from current; a custom window decoration (or a platform backend) writes the values, and
/// every inset-padding modifier reads them at measure time, so a change re-measures just the nodes
/// that asked for that inset.
#[derive(Clone)]
pub struct WindowInsetsController {
    state: State<WindowInsets>,
}

impl WindowInsetsController {
    pub fn new() -> Self {
        Self { state: State::new(WindowInsets::default()) }
    }

    pub fn current(&self) -> WindowInsets {
        self.state.get()
    }

    /// Replace the whole inset set at once (a platform backend reporting a frame).
    pub fn set(&self, insets: WindowInsets) {
        self.state.set(insets);
    }

    /// Update one or more parts in place.
    pub fn update(&self, f: impl FnOnce(&mut WindowInsets)) {
        let mut next = self.state.get();
        f(&mut next);
        self.state.set(next);
    }

    /// The custom-decoration entry point: the height (and left/right, for a vertical bar) that the
    /// window's own status-bar replacement occupies.
    pub fn set_status_bar_insets(&self, insets: Insets) {
        self.update(|window| window.status_bars = insets);
    }

    pub fn set_navigation_bar_insets(&self, insets: Insets) {
        self.update(|window| window.navigation_bars = insets);
    }

    pub fn set_ime_insets(&self, insets: Insets) {
        self.update(|window| window.ime = insets);
    }

    pub fn set_display_cutout_insets(&self, insets: Insets) {
        self.update(|window| window.display_cutout = insets);
    }

    pub fn set_system_gesture_insets(&self, insets: Insets) {
        self.update(|window| window.system_gestures = insets);
    }

    /// Whether two controllers drive the same window (element_param_eq).
    pub(crate) fn same(&self, other: &Self) -> bool {
        self.state.state_id() == other.state.state_id()
    }

    /// The state the modifier elements read at measure time (a tracked read registers a layout
    /// dependency, so writing an inset re-measures the affected nodes).
    pub(crate) fn state(&self) -> State<WindowInsets> {
        self.state.clone()
    }
}

impl Default for WindowInsetsController {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for WindowInsetsController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WindowInsetsController")
            .field("current", &self.state.peek())
            .finish()
    }
}

/// Serialises the tests that mutate the process-global fallback controller: outside a window
/// `current()` resolves to one shared controller, so two tests setting different insets would reset
/// each other mid-measure and turn exact-size assertions flaky.
#[cfg(test)]
pub(crate) static INSET_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn fallback_controller() -> WindowInsetsController {
    static FALLBACK: LazyLock<WindowInsetsController> =
        LazyLock::new(WindowInsetsController::new);
    FALLBACK.clone()
}

/// The insets controller of the enclosing window. Outside a provided window a shared zero controller
/// answers, so a modifier still composes (and pads by zero).
pub static LOCAL_WINDOW_INSETS: LazyLock<CompositionLocal<WindowInsetsController>> =
    LazyLock::new(|| CompositionLocal::new(fallback_controller));

/// The enclosing window's insets controller.
pub fn current() -> WindowInsetsController {
    LOCAL_WINDOW_INSETS.current()
}

/// The insets the enclosing window currently declares.
pub fn current_insets() -> WindowInsets {
    current().current()
}

/// Publish the status-bar inset for the enclosing window — what a custom window decoration calls
/// with its own height.
pub fn set_status_bars(insets: Insets) {
    current().set_status_bar_insets(insets);
}

/// Publish the navigation-bar inset for the enclosing window.
pub fn set_navigation_bars(insets: Insets) {
    current().set_navigation_bar_insets(insets);
}

/// Publish the on-screen keyboard (IME) inset for the enclosing window.
pub fn set_ime(insets: Insets) {
    current().set_ime_insets(insets);
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insets_union_and_exclude() {
        let a = Insets::new(1.0, 5.0, 2.0, 0.0);
        let b = Insets::new(0.0, 3.0, 4.0, 8.0);
        assert_eq!(a.max(b), Insets::new(1.0, 5.0, 4.0, 8.0));
        assert_eq!(b.exclude(a), Insets::new(0.0, 0.0, 2.0, 8.0), "never negative");
        assert!(Insets::ZERO.is_zero());
    }

    #[test]
    fn safe_areas_are_the_union_of_the_sources() {
        let window = WindowInsets {
            status_bars: Insets::new(0.0, 24.0, 0.0, 0.0),
            navigation_bars: Insets::new(0.0, 0.0, 0.0, 48.0),
            ime: Insets::new(0.0, 0.0, 0.0, 300.0),
            ..Default::default()
        };
        assert_eq!(window.system_bars(), Insets::new(0.0, 24.0, 0.0, 48.0));
        assert_eq!(window.safe_drawing(), Insets::new(0.0, 24.0, 0.0, 300.0));
    }

    #[test]
    fn consumption_is_per_source_and_idempotent() {
        let window = WindowInsets {
            status_bars: Insets::new(0.0, 24.0, 0.0, 0.0),
            navigation_bars: Insets::new(0.0, 0.0, 0.0, 48.0),
            ..Default::default()
        };
        let start = Consumed::default();
        let after_status = consume(InsetsKind::StatusBars, window, start);
        assert_eq!(
            remaining(InsetsKind::StatusBars, window, after_status),
            Insets::ZERO,
            "the consumed bar pads by zero"
        );
        assert_eq!(
            remaining(InsetsKind::NavigationBars, window, after_status),
            Insets::new(0.0, 0.0, 0.0, 48.0),
            "a different source is untouched"
        );
        let twice = consume(InsetsKind::StatusBars, window, after_status);
        assert_eq!(twice, after_status, "consuming again changes nothing");
    }
}
