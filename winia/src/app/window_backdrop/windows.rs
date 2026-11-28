//! Win32 implementation of the system backdrop request.
//!
//! Only the documented surface is used: `DwmSetWindowAttribute` with `DWMWA_SYSTEMBACKDROP_TYPE`
//! (Windows 11 build 22621 and later), then `DwmGetWindowAttribute` to report what DWM kept. Older
//! systems reject the call with `E_INVALIDARG`; that surfaces as [`BackdropStatus::Rejected`] rather
//! than probing the build number through undocumented means.
//!
//! The read-back is treated as confirmation, not as part of the request: if DWM refuses to report
//! the attribute the write is still reported as [`BackdropStatus::Unverified`], never as rejected.

use super::{BackdropStatus, WindowBackdrop, WindowsBackdrop};
use std::mem::size_of;
use windows::Win32::Graphics::Dwm::{
    DWMSBT_AUTO, DWMSBT_MAINWINDOW, DWMSBT_NONE, DWMSBT_TABBEDWINDOW, DWMSBT_TRANSIENTWINDOW,
    DWMWA_SYSTEMBACKDROP_TYPE, DwmGetWindowAttribute, DwmSetWindowAttribute,
};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// The `DWM_SYSTEMBACKDROP_TYPE` value a request stands for; `None` clears the attribute.
pub(crate) const fn dwm_value(backdrop: Option<WindowsBackdrop>) -> i32 {
    match backdrop {
        None => DWMSBT_NONE.0,
        Some(WindowsBackdrop::Auto) => DWMSBT_AUTO.0,
        Some(WindowsBackdrop::Mica) => DWMSBT_MAINWINDOW.0,
        Some(WindowsBackdrop::Acrylic) => DWMSBT_TRANSIENTWINDOW.0,
        Some(WindowsBackdrop::MicaAlt) => DWMSBT_TABBEDWINDOW.0,
    }
}

/// Set the backdrop attribute on `window` and report what DWM kept.
pub(crate) fn apply(
    window: &dyn winit::window::Window,
    backdrop: Option<WindowsBackdrop>,
) -> BackdropStatus {
    let Some(hwnd) = hwnd_of(window) else {
        // Android composites into one Activity surface and never reaches this module; a window
        // that has no Win32 handle cannot carry a DWM attribute either.
        return BackdropStatus::Unsupported;
    };
    let request = match backdrop {
        Some(material) => WindowBackdrop::Windows(material),
        None => WindowBackdrop::None,
    };
    let requested = dwm_value(backdrop);
    let set = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            (&requested as *const i32).cast(),
            size_of::<i32>() as u32,
        )
    };
    if let Err(error) = set {
        return BackdropStatus::Rejected(error.code().0);
    }
    if backdrop.is_none() {
        return BackdropStatus::NotRequested;
    }
    let mut reported = i32::MIN;
    let read = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            (&mut reported as *mut i32).cast(),
            size_of::<i32>() as u32,
        )
    };
    match read {
        Ok(()) if reported == requested => BackdropStatus::Applied,
        Ok(()) => BackdropStatus::Mismatch {
            requested: request,
            reported,
        },
        Err(error) => BackdropStatus::Unverified(error.code().0),
    }
}

/// The Win32 handle behind a winit window, if this backend exposes one.
fn hwnd_of(window: &dyn winit::window::Window) -> Option<windows::Win32::Foundation::HWND> {
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(windows::Win32::Foundation::HWND(
            handle.hwnd.get() as *mut core::ffi::c_void
        )),
        _ => None,
    }
}
