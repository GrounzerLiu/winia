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
    /// Reserve the left edge of the current monitor.
    Left,
    /// Reserve the top edge of the current monitor.
    Top,
    /// Reserve the right edge of the current monitor.
    Right,
    /// Reserve the bottom edge of the current monitor.
    Bottom,
}

/// Creation options for a Windows application desktop toolbar.
///
/// The shell may negotiate a smaller rectangle when another app bar or the system taskbar already
/// occupies the same edge. These options describe the request, not the final native rectangle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AppBarOptions {
    /// The screen edge to reserve.
    pub edge: AppBarEdge,
    /// Toolbar thickness in logical pixels. Windows converts it with the current monitor scale.
    pub thickness: f32,
}

impl AppBarOptions {
    /// Create options for an edge and a logical-pixel thickness.
    #[must_use]
    pub const fn new(edge: AppBarEdge, thickness: f32) -> Self {
        Self { edge, thickness }
    }

    /// Create options for the left edge.
    #[must_use]
    pub const fn left(thickness: f32) -> Self {
        Self::new(AppBarEdge::Left, thickness)
    }

    /// Create options for the top edge.
    #[must_use]
    pub const fn top(thickness: f32) -> Self {
        Self::new(AppBarEdge::Top, thickness)
    }

    /// Create options for the right edge.
    #[must_use]
    pub const fn right(thickness: f32) -> Self {
        Self::new(AppBarEdge::Right, thickness)
    }

    /// Create options for the bottom edge.
    #[must_use]
    pub const fn bottom(thickness: f32) -> Self {
        Self::new(AppBarEdge::Bottom, thickness)
    }

    /// Whether the thickness can be converted to a physical pixel extent.
    #[must_use]
    pub fn is_valid(self) -> bool {
        self.thickness.is_finite() && self.thickness > 0.0
    }
}

/// Why an app-bar request could not be applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppBarError {
    /// The requested thickness was not finite and greater than zero.
    InvalidThickness,
    /// The Windows shell or native window operation was unavailable.
    ShellUnavailable,
    /// The monitor that owns the native window could not be queried.
    MonitorUnavailable,
}

impl std::fmt::Display for AppBarError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidThickness => "app-bar thickness must be finite and greater than zero",
            Self::ShellUnavailable => "the Windows shell is unavailable",
            Self::MonitorUnavailable => "the app-bar monitor is unavailable",
        })
    }
}

impl std::error::Error for AppBarError {}

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

impl AppBarStatus {
    /// Whether the shell currently accepts this app bar.
    #[must_use]
    pub const fn is_registered(self) -> bool {
        matches!(self, Self::Registered { .. })
    }

    /// Whether the current platform does not implement app bars.
    #[must_use]
    pub const fn is_unsupported(self) -> bool {
        matches!(self, Self::Unsupported)
    }

    /// Whether the request or a later shell operation failed.
    #[must_use]
    pub const fn is_failed(self) -> bool {
        matches!(self, Self::Failed(_))
    }

    /// The shell-accepted edge, or `None` unless the status is [`Self::Registered`].
    #[must_use]
    pub const fn edge(self) -> Option<AppBarEdge> {
        match self {
            Self::Registered { edge } => Some(edge),
            _ => None,
        }
    }
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

    #[test]
    fn status_helpers_report_only_the_matching_lifecycle_state() {
        let registered = AppBarStatus::Registered {
            edge: AppBarEdge::Bottom,
        };
        assert!(registered.is_registered());
        assert!(!registered.is_unsupported());
        assert!(!registered.is_failed());
        assert_eq!(registered.edge(), Some(AppBarEdge::Bottom));

        assert!(AppBarStatus::Unsupported.is_unsupported());
        assert_eq!(AppBarStatus::Unsupported.edge(), None);
        let failed = AppBarStatus::Failed(AppBarError::ShellUnavailable);
        assert!(failed.is_failed());
        assert_eq!(failed.edge(), None);
    }

    #[test]
    fn errors_have_stable_display_messages() {
        assert_eq!(
            AppBarError::InvalidThickness.to_string(),
            "app-bar thickness must be finite and greater than zero",
        );
        assert_eq!(
            AppBarError::ShellUnavailable.to_string(),
            "the Windows shell is unavailable",
        );
        assert_eq!(
            AppBarError::MonitorUnavailable.to_string(),
            "the app-bar monitor is unavailable",
        );
    }
}
