//! Window-shell options shared by every platform.
//!
//! The public API is deliberately available everywhere so one window declaration can be used on
//! every target. Only Windows implements these options today; other platforms normalize the style
//! flags away and report [`AppBarStatus::Unsupported`] for an app-bar request.

#[cfg(target_os = "windows")]
pub(crate) mod windows;
#[cfg(target_os = "windows")]
pub(crate) use windows::ShellWindow;

/// The screen edge occupied by an application desktop toolbar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AppBarEdge {
    Left,
    Top,
    Right,
    Bottom,
}

/// Creation options for a Windows application desktop toolbar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AppBarOptions {
    /// The screen edge to reserve.
    pub edge: AppBarEdge,
    /// Toolbar thickness in logical pixels. Windows converts it with the current monitor scale.
    pub thickness: f32,
}

impl AppBarOptions {
    pub const fn new(edge: AppBarEdge, thickness: f32) -> Self {
        Self { edge, thickness }
    }

    pub const fn left(thickness: f32) -> Self {
        Self::new(AppBarEdge::Left, thickness)
    }

    pub const fn top(thickness: f32) -> Self {
        Self::new(AppBarEdge::Top, thickness)
    }

    pub const fn right(thickness: f32) -> Self {
        Self::new(AppBarEdge::Right, thickness)
    }

    pub const fn bottom(thickness: f32) -> Self {
        Self::new(AppBarEdge::Bottom, thickness)
    }

    /// Whether the thickness can be converted to a physical pixel extent.
    pub fn is_valid(self) -> bool {
        self.thickness.is_finite() && self.thickness > 0.0
    }
}

/// Why an app-bar request could not be applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppBarError {
    InvalidThickness,
    ShellUnavailable,
    MonitorUnavailable,
    InvalidWindowKind,
}

/// Observable app-bar lifecycle state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppBarStatus {
    /// The current platform does not implement app bars.
    Unsupported,
    /// No app bar was requested, or a previous registration was removed.
    Unregistered,
    /// Registration is in progress on the platform thread.
    Registering,
    /// The shell accepted the bar on `edge`.
    Registered { edge: AppBarEdge },
    /// The request was rejected or a later shell operation failed.
    Failed(AppBarError),
}

impl Default for AppBarStatus {
    fn default() -> Self {
        Self::Unregistered
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_constructors_keep_the_requested_thickness() {
        assert_eq!(AppBarOptions::bottom(48.0).edge, AppBarEdge::Bottom);
        assert_eq!(AppBarOptions::top(32.0).edge, AppBarEdge::Top);
        assert_eq!(AppBarOptions::left(40.0).edge, AppBarEdge::Left);
        assert_eq!(AppBarOptions::right(40.0).edge, AppBarEdge::Right);
        assert_eq!(AppBarOptions::bottom(48.0).thickness, 48.0);
    }

    #[test]
    fn thickness_must_be_finite_and_positive() {
        assert!(AppBarOptions::bottom(1.0).is_valid());
        assert!(!AppBarOptions::bottom(0.0).is_valid());
        assert!(!AppBarOptions::bottom(-1.0).is_valid());
        assert!(!AppBarOptions::bottom(f32::NAN).is_valid());
        assert!(!AppBarOptions::bottom(f32::INFINITY).is_valid());
    }
}
