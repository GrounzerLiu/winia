//! 输入子系统——指针事件、手势识别（对标 Compose pointerInput 语义）
pub(crate) mod gesture;
pub mod events;
pub mod interaction;
pub use events::{
    KbEvent, KbEventType, PenKind, PointerButton, PointerEvent, PointerEventType, PointerKind,
};

/// Compose's `PointerIcon` — the mouse cursor a node asks for while it is hovered.
///
/// A newtype over winit's cursor enum: the framework already targets winit, and re-declaring the
/// forty-odd OS cursors here would only add a mapping table that can drift. Compose's desktop
/// constants are provided (`HAND`, `TEXT`, `CROSSHAIR`, …); anything else is
/// `PointerIcon::new(winit::cursor::CursorIcon::…)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PointerIcon(pub winit::cursor::CursorIcon);

impl PointerIcon {
    pub const fn new(icon: winit::cursor::CursorIcon) -> Self {
        Self(icon)
    }

    /// The winit cursor the window is set to.
    pub const fn cursor_icon(self) -> winit::cursor::CursorIcon {
        self.0
    }

    /// The platform default arrow — what an unhovered node shows.
    pub const DEFAULT: Self = Self::new(winit::cursor::CursorIcon::Default);
    /// A context menu is available.
    pub const CONTEXT_MENU: Self = Self::new(winit::cursor::CursorIcon::ContextMenu);
    /// A pointer/hand — the "this is clickable" cursor.
    pub const HAND: Self = Self::new(winit::cursor::CursorIcon::Pointer);
    /// A help cursor (question mark / balloon).
    pub const HELP: Self = Self::new(winit::cursor::CursorIcon::Help);
    /// Work in progress.
    pub const PROGRESS: Self = Self::new(winit::cursor::CursorIcon::Progress);
    /// Busy — the user should wait.
    pub const WAIT: Self = Self::new(winit::cursor::CursorIcon::Wait);
    /// A cell or set of cells may be selected.
    pub const CELL: Self = Self::new(winit::cursor::CursorIcon::Cell);
    /// Precise selection, e.g. drawing.
    pub const CROSSHAIR: Self = Self::new(winit::cursor::CursorIcon::Crosshair);
    /// Text may be selected.
    pub const TEXT: Self = Self::new(winit::cursor::CursorIcon::Text);
    /// Vertical text may be selected.
    pub const VERTICAL_TEXT: Self = Self::new(winit::cursor::CursorIcon::VerticalText);
    /// An alias or shortcut is about to be created.
    pub const ALIAS: Self = Self::new(winit::cursor::CursorIcon::Alias);
    /// Something is about to be copied.
    pub const COPY: Self = Self::new(winit::cursor::CursorIcon::Copy);
    /// Something is to be moved.
    pub const MOVE: Self = Self::new(winit::cursor::CursorIcon::Move);
    /// Something may not be dropped here.
    pub const NO_DROP: Self = Self::new(winit::cursor::CursorIcon::NoDrop);
    /// The action is not allowed.
    pub const NOT_ALLOWED: Self = Self::new(winit::cursor::CursorIcon::NotAllowed);
    /// The content is grabbable (pan).
    pub const GRAB: Self = Self::new(winit::cursor::CursorIcon::Grab);
    /// The content is being grabbed.
    pub const GRABBING: Self = Self::new(winit::cursor::CursorIcon::Grabbing);
    /// Everything can be scrolled.
    pub const ALL_SCROLL: Self = Self::new(winit::cursor::CursorIcon::AllScroll);
    /// Zoom in.
    pub const ZOOM_IN: Self = Self::new(winit::cursor::CursorIcon::ZoomIn);
    /// Zoom out.
    pub const ZOOM_OUT: Self = Self::new(winit::cursor::CursorIcon::ZoomOut);
    /// Horizontal resize.
    pub const RESIZE_EAST_WEST: Self = Self::new(winit::cursor::CursorIcon::EwResize);
    /// Vertical resize.
    pub const RESIZE_NORTH_SOUTH: Self = Self::new(winit::cursor::CursorIcon::NsResize);
    /// Diagonal resize (top-left / bottom-right).
    pub const RESIZE_NORTH_WEST_SOUTH_EAST: Self =
        Self::new(winit::cursor::CursorIcon::NwseResize);
    /// Diagonal resize (top-right / bottom-left).
    pub const RESIZE_NORTH_EAST_SOUTH_WEST: Self =
        Self::new(winit::cursor::CursorIcon::NeswResize);
}

impl From<winit::cursor::CursorIcon> for PointerIcon {
    fn from(icon: winit::cursor::CursorIcon) -> Self {
        Self(icon)
    }
}
