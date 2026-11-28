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
//! composited, only that the request is in place. Where DWM refuses the read-back the write is
//! reported as [`BackdropStatus::Unverified`], because the set itself succeeded. The read-back has
//! only been verified on Windows 11 build 26300.

use std::fmt;

#[cfg(target_os = "windows")]
pub(crate) mod windows;

/// A system-drawn backdrop material.
///
/// Every variant compiles on every platform; a request that names another platform's material, or
/// a platform winia has no backend for yet, is kept and reported as
/// [`BackdropStatus::Unsupported`].
///
/// A whole-window material only shows where the client area does not paint an opaque pixel, so it
/// normally needs [`Window::transparent`](crate::app::window::Window::transparent) together with
/// [`Window::decorations`](crate::app::window::Window::decorations)`(false)`. winia does not apply
/// either of them for you, and both are independent requests.
///
/// See `docs/window-material.md` for the cross-platform mapping and what each backend can promise.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum WindowBackdrop {
    /// No system backdrop material.
    #[default]
    None,
    /// Windows 11 DWM system backdrop.
    Windows(WindowsBackdrop),
    /// macOS `NSVisualEffectView` vibrancy. No backend yet: reported as `Unsupported`.
    MacOs(MacOsBackdrop),
    /// Wayland blur protocol. No backend yet: reported as `Unsupported`.
    Wayland,
}

/// The Windows 11 DWM materials, mapped onto `DWM_SYSTEMBACKDROP_TYPE`:
///
/// | Variant | Windows value | Where the system draws it |
/// | --- | --- | --- |
/// | [`Auto`](Self::Auto) | `DWMSBT_AUTO` | just behind the default Win32 title bar |
/// | [`Mica`](Self::Mica) | `DWMSBT_MAINWINDOW` | behind the entire window bounds |
/// | [`Acrylic`](Self::Acrylic) | `DWMSBT_TRANSIENTWINDOW` | behind the entire window bounds |
/// | [`MicaAlt`](Self::MicaAlt) | `DWMSBT_TABBEDWINDOW` | behind the entire window bounds |
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WindowsBackdrop {
    /// Let the system decide; documented as a material just behind the default Win32 title bar.
    Auto,
    /// Mica behind the entire window bounds.
    Mica,
    /// Desktop Acrylic behind the entire window bounds.
    Acrylic,
    /// Mica Alt behind the entire window bounds.
    MicaAlt,
}

/// The macOS `NSVisualEffectView` materials.
///
/// Mirrors `winit_appkit::BlurMaterial` one-to-one, so the backend is a plain conversion rather than
/// a second taxonomy. winit marks that enum `#[non_exhaustive]`: a variant winia does not know maps
/// to [`BackdropStatus::Unsupported`] instead of silently falling back to some other material.
///
/// Nothing is applied until a macOS backend lands; every variant reports `Unsupported` today.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MacOsBackdrop {
    /// `NSVisualEffectMaterialFullScreenUI`: the least tinted material, closest to a plain blur.
    /// winit's default.
    FullScreenUi,
    /// `NSVisualEffectMaterialHUDWindow`.
    HudWindow,
    /// `NSVisualEffectMaterialMenu`.
    Menu,
    /// `NSVisualEffectMaterialPopover`.
    Popover,
    /// `NSVisualEffectMaterialSidebar`.
    Sidebar,
    /// `NSVisualEffectMaterialSelection`.
    Selection,
    /// `NSVisualEffectMaterialTitlebar`.
    Titlebar,
    /// `NSVisualEffectMaterialHeaderView`.
    HeaderView,
    /// `NSVisualEffectMaterialToolTip`.
    ToolTip,
    /// `NSVisualEffectMaterialUnderWindowBackground`.
    UnderWindowBackground,
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
    /// DWM accepted the request and reads back the requested value.
    ///
    /// This confirms the attribute, not the pixels: whether anything is visible still depends on
    /// the window's transparency, decorations and client-area content.
    Applied,
    /// The platform accepted the request, but reading the value back failed with this HRESULT.
    ///
    /// The write itself succeeded; only the confirmation is missing. Kept separate from
    /// [`Rejected`](Self::Rejected) so a read-back failure is not mistaken for a failed request.
    Unverified(i32),
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
    /// Whether the request is in place and confirmed by the platform.
    #[must_use]
    pub const fn is_applied(self) -> bool {
        matches!(self, Self::Applied)
    }

    /// Whether the platform accepted the write but could not read the value back.
    #[must_use]
    pub const fn is_unverified(self) -> bool {
        matches!(self, Self::Unverified(_))
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
    #[cfg(target_os = "windows")]
    {
        match backdrop {
            WindowBackdrop::None => windows::apply(window, None),
            WindowBackdrop::Windows(material) => windows::apply(window, Some(material)),
            // The mechanism exists on Windows, but only for DWM materials.
            WindowBackdrop::MacOs(_) | WindowBackdrop::Wayland => BackdropStatus::Unsupported,
        }
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
        assert!(BackdropStatus::Unverified(-2147024809).is_unverified());
        assert!(!BackdropStatus::Unverified(-2147024809).is_failed());
        assert!(!BackdropStatus::Unverified(-2147024809).is_applied());
        assert!(BackdropStatus::Rejected(-2147024809).is_failed());
        assert!(
            BackdropStatus::Mismatch {
                requested: WindowBackdrop::Windows(WindowsBackdrop::Mica),
                reported: 0,
            }
            .is_failed()
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn every_variant_maps_to_its_documented_dwm_value() {
        use WindowsBackdrop::*;

        assert_eq!(windows::dwm_value(None), 1);
        assert_eq!(windows::dwm_value(Some(Auto)), 0);
        assert_eq!(windows::dwm_value(Some(Mica)), 2);
        assert_eq!(windows::dwm_value(Some(Acrylic)), 3);
        assert_eq!(windows::dwm_value(Some(MicaAlt)), 4);
    }

    #[test]
    fn a_request_for_another_platform_is_still_a_material_request() {
        assert!(WindowBackdrop::MacOs(MacOsBackdrop::Sidebar).is_material());
        assert!(WindowBackdrop::Wayland.is_material());
        assert!(WindowBackdrop::Windows(WindowsBackdrop::Mica).is_material());
    }
}
