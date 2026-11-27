//! System-drawn window backdrop materials.
//!
//! The public API exists on every platform so one window declaration compiles everywhere, but only
//! Windows implements it. On Windows a request maps 1:1 onto `DWMWA_SYSTEMBACKDROP_TYPE` through
//! `DwmSetWindowAttribute`, and nothing else about the window is touched: this module never changes
//! transparency, decorations, the frame, or the rendering surface. An application that wants the
//! material to reach its client area has to ask for those itself.
//!
//! [`BackdropStatus`] reports what the platform did with the request. [`BackdropStatus::Applied`]
//! means DWM accepted the attribute and reads it back; it does not prove that material pixels were
//! composited, only that the request is in place.

use std::fmt;

#[cfg(target_os = "windows")]
pub(crate) mod windows;

/// A system-drawn backdrop material.
///
/// The variants map onto `DWM_SYSTEMBACKDROP_TYPE`, keeping the documented placement:
///
/// | Variant | Windows value | Where the system draws it |
/// | --- | --- | --- |
/// | [`None`](Self::None) | `DWMSBT_NONE` | nowhere |
/// | [`Auto`](Self::Auto) | `DWMSBT_AUTO` | just behind the default Win32 title bar |
/// | [`Mica`](Self::Mica) | `DWMSBT_MAINWINDOW` | behind the entire window bounds |
/// | [`Acrylic`](Self::Acrylic) | `DWMSBT_TRANSIENTWINDOW` | behind the entire window bounds |
/// | [`MicaAlt`](Self::MicaAlt) | `DWMSBT_TABBEDWINDOW` | behind the entire window bounds |
///
/// A whole-window material only shows where the client area does not paint an opaque pixel, so it
/// normally needs [`Window::transparent`](crate::app::window::Window::transparent) together with
/// [`Window::decorations`](crate::app::window::Window::decorations)`(false)`. winia does not apply
/// either of them for you, and both are independent requests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum WindowBackdrop {
    /// No system backdrop material.
    #[default]
    None,
    /// Let the system decide; documented as a material just behind the default Win32 title bar.
    Auto,
    /// Mica behind the entire window bounds.
    Mica,
    /// Desktop Acrylic behind the entire window bounds.
    Acrylic,
    /// Mica Alt behind the entire window bounds.
    MicaAlt,
}

impl WindowBackdrop {
    /// Whether this request asks for any material at all.
    #[must_use]
    pub const fn is_material(self) -> bool {
        !matches!(self, Self::None)
    }
}

/// What the platform did with a [`WindowBackdrop`] request.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BackdropStatus {
    /// No material was requested, or a previous request was cleared.
    #[default]
    NotRequested,
    /// Requested, but the window does not exist on the platform yet.
    Pending,
    /// DWM accepted the request and reads back the requested value.
    ///
    /// This confirms the attribute, not the pixels: whether anything is visible still depends on
    /// the window's transparency, decorations and client-area content.
    Applied,
    /// The current platform does not implement system backdrops; the request is kept for inspection.
    Unsupported,
    /// The platform rejected the request with this HRESULT.
    ///
    /// Windows 10 and Windows 11 before build 22621 reject `DWMWA_SYSTEMBACKDROP_TYPE` with
    /// `E_INVALIDARG`.
    Rejected(i32),
    /// The call succeeded but DWM reports a different value; `reported` is the raw platform value.
    Mismatch {
        /// The backdrop that was requested.
        requested: WindowBackdrop,
        /// The raw `DWM_SYSTEMBACKDROP_TYPE` value DWM reports.
        reported: i32,
    },
}

impl BackdropStatus {
    /// Whether the request is in place on the platform.
    #[must_use]
    pub const fn is_applied(self) -> bool {
        matches!(self, Self::Applied)
    }

    /// Whether the current platform does not implement system backdrops.
    #[must_use]
    pub const fn is_unsupported(self) -> bool {
        matches!(self, Self::Unsupported)
    }

    /// Whether the platform rejected the request or reported a different value.
    #[must_use]
    pub const fn is_failed(self) -> bool {
        matches!(self, Self::Rejected(_) | Self::Mismatch { .. })
    }
}

impl fmt::Display for BackdropStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotRequested => f.write_str("not requested"),
            Self::Pending => f.write_str("pending"),
            Self::Applied => f.write_str("applied"),
            Self::Unsupported => f.write_str("unsupported on this platform"),
            Self::Rejected(hresult) => write!(f, "rejected with HRESULT {hresult:#010x}"),
            Self::Mismatch {
                requested,
                reported,
            } => {
                write!(f, "requested {requested:?}, DWM reports {reported}")
            }
        }
    }
}

/// Apply a backdrop request to a live window and report the outcome.
pub(crate) fn apply(
    window: &dyn winit::window::Window,
    backdrop: WindowBackdrop,
) -> BackdropStatus {
    #[cfg(target_os = "windows")]
    {
        windows::apply(window, backdrop)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = window;
        if backdrop.is_material() {
            BackdropStatus::Unsupported
        } else {
            BackdropStatus::NotRequested
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_request_is_no_material() {
        assert_eq!(WindowBackdrop::default(), WindowBackdrop::None);
        assert!(!WindowBackdrop::default().is_material());
        assert_eq!(BackdropStatus::default(), BackdropStatus::NotRequested);
    }

    #[test]
    fn status_helpers_classify_each_outcome() {
        assert!(BackdropStatus::Applied.is_applied());
        assert!(!BackdropStatus::Applied.is_failed());
        assert!(BackdropStatus::Unsupported.is_unsupported());
        assert!(BackdropStatus::Rejected(-2147024809).is_failed());
        assert!(
            BackdropStatus::Mismatch {
                requested: WindowBackdrop::Mica,
                reported: 0,
            }
            .is_failed()
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn every_variant_maps_to_its_documented_dwm_value() {
        assert_eq!(windows::dwm_value(WindowBackdrop::None), 1);
        assert_eq!(windows::dwm_value(WindowBackdrop::Auto), 0);
        assert_eq!(windows::dwm_value(WindowBackdrop::Mica), 2);
        assert_eq!(windows::dwm_value(WindowBackdrop::Acrylic), 3);
        assert_eq!(windows::dwm_value(WindowBackdrop::MicaAlt), 4);
    }
}
