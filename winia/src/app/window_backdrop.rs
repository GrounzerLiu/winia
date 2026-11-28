//! System-drawn window backdrop materials.
//!
//! The public API exists on every platform so one window declaration compiles everywhere.
//! [`WindowBackdrop::System`] uses winit's blur on macOS and Wayland and DWM on Windows; the named
//! materials are Windows-only. Nothing here changes transparency, decorations, the frame, or the
//! rendering surface. An application that wants the material to reach its client area has to ask
//! for those itself.
//!
//! [`BackdropStatus`] reports what the platform did with the request. [`BackdropStatus::Applied`]
//! means the platform accepted it; on Windows DWM also reads the attribute back. macOS and Wayland
//! have no read-back through winit, so there it means the call was delivered, not that pixels were
//! confirmed. Where DWM refuses the read-back the write is reported as
//! [`BackdropStatus::Unverified`], because the set itself succeeded. The read-back has only been
//! verified on Windows 11 build 26300.

use std::fmt;

#[cfg(target_os = "windows")]
pub(crate) mod windows;

/// A system-drawn window material.
///
/// [`System`](Self::System) is the cross-platform default: Mica on Windows, winit's default vibrancy
/// on macOS, and the compositor's blur on Wayland. The named materials are explicit Windows DWM
/// requests; on other platforms they report [`BackdropStatus::Unsupported`] rather than silently
/// selecting a different material.
///
/// A whole-window material only shows where the client area does not paint an opaque pixel, so it
/// normally needs [`Window::transparent`](crate::app::window::Window::transparent) together with
/// [`Window::decorations`](crate::app::window::Window::decorations)`(false)`. winia does not apply
/// either of them for you, and both are independent requests.
///
/// See `docs/window-material.md` for the mapping and the status guarantees of each backend.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum WindowBackdrop {
    /// No system backdrop material.
    #[default]
    None,
    /// The running platform's default whole-window material.
    ///
    /// Windows maps this to `DWMSBT_MAINWINDOW` (Mica); macOS calls `Window::set_blur(true)` and
    /// keeps winit's default `FullScreenUI` material; Wayland calls `Window::set_blur(true)`.
    System,
    /// Windows Mica (`DWMSBT_MAINWINDOW`).
    Mica,
    /// Windows Desktop Acrylic (`DWMSBT_TRANSIENTWINDOW`).
    Acrylic,
    /// Windows Mica Alt (`DWMSBT_TABBEDWINDOW`).
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
    /// Queued: the window does not exist on the platform yet, or the write has not run yet.
    Pending,
    /// The platform accepted the request.
    ///
    /// On Windows DWM also reads the value back. macOS and Wayland have no read-back through
    /// winit, so there this means the call was delivered, not that pixels were confirmed. Whether
    /// anything is visible still depends on transparency, decorations and client-area content.
    Applied,
    /// The platform accepted the request, but reading the value back failed with this HRESULT.
    ///
    /// The write itself succeeded; only the confirmation is missing. Kept separate from
    /// [`Rejected`](Self::Rejected) so a read-back failure is not mistaken for a failed request.
    Unverified(i32),
    /// The platform has no backdrop backend, or it does not implement this particular request.
    /// The request is kept for inspection.
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
    /// Whether the platform accepted the request.
    #[must_use]
    pub const fn is_applied(self) -> bool {
        matches!(self, Self::Applied)
    }

    /// Whether the platform accepted the write but could not read the value back.
    #[must_use]
    pub const fn is_unverified(self) -> bool {
        matches!(self, Self::Unverified(_))
    }

    /// Whether the platform has no backend for this request.
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
            Self::Unverified(hresult) => {
                write!(f, "accepted, unverified (HRESULT {hresult:#010x})")
            }
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
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let Ok(handle) = window.window_handle() else {
        return if backdrop.is_material() {
            BackdropStatus::Unsupported
        } else {
            BackdropStatus::NotRequested
        };
    };
    match handle.as_raw() {
        #[cfg(target_os = "windows")]
        RawWindowHandle::Win32(_) => windows::apply(window, backdrop),
        RawWindowHandle::AppKit(_) | RawWindowHandle::Wayland(_) => {
            apply_winit_blur(window, backdrop)
        }
        _ if !backdrop.is_material() => BackdropStatus::NotRequested,
        _ => BackdropStatus::Unsupported,
    }
}

/// macOS and Wayland share winit's boolean blur API; only `System` uses it.
fn apply_winit_blur(
    window: &dyn winit::window::Window,
    backdrop: WindowBackdrop,
) -> BackdropStatus {
    match backdrop {
        WindowBackdrop::None => {
            winit::window::Window::set_blur(window, false);
            BackdropStatus::NotRequested
        }
        WindowBackdrop::System => {
            winit::window::Window::set_blur(window, true);
            BackdropStatus::Applied
        }
        WindowBackdrop::Mica | WindowBackdrop::Acrylic | WindowBackdrop::MicaAlt => {
            BackdropStatus::Unsupported
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
        assert!(BackdropStatus::Unverified(-2147024809).is_unverified());
        assert!(!BackdropStatus::Unverified(-2147024809).is_failed());
        assert!(!BackdropStatus::Unverified(-2147024809).is_applied());
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
        assert_eq!(windows::dwm_value(WindowBackdrop::System), 2);
        assert_eq!(windows::dwm_value(WindowBackdrop::Mica), 2);
        assert_eq!(windows::dwm_value(WindowBackdrop::Acrylic), 3);
        assert_eq!(windows::dwm_value(WindowBackdrop::MicaAlt), 4);
    }

    #[test]
    fn system_and_each_named_material_are_material_requests() {
        assert!(WindowBackdrop::System.is_material());
        assert!(WindowBackdrop::Mica.is_material());
        assert!(WindowBackdrop::Acrylic.is_material());
        assert!(WindowBackdrop::MicaAlt.is_material());
    }
}
