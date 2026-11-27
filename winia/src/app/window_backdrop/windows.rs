//! Win32 implementation of the system backdrop request.
//!
//! Only the documented surface is used: `DwmSetWindowAttribute` with `DWMWA_SYSTEMBACKDROP_TYPE`
//! (Windows 11 build 22621 and later), then `DwmGetWindowAttribute` to report what DWM kept. Older
//! systems reject the call with `E_INVALIDARG`; that surfaces as [`BackdropStatus::Rejected`] rather
//! than probing the build number through undocumented means.

use super::{BackdropStatus, WindowBackdrop};
use std::mem::size_of;
use windows::Win32::Graphics::Dwm::{
    DWMSBT_AUTO, DWMSBT_MAINWINDOW, DWMSBT_NONE, DWMSBT_TABBEDWINDOW, DWMSBT_TRANSIENTWINDOW,
    DWMWA_SYSTEMBACKDROP_TYPE, DwmGetWindowAttribute, DwmSetWindowAttribute,
};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// The `DWM_SYSTEMBACKDROP_TYPE` value a request stands for.
pub(crate) const fn dwm_value(backdrop: WindowBackdrop) -> i32 {
    match backdrop {
        WindowBackdrop::None => DWMSBT_NONE.0,
        WindowBackdrop::Auto => DWMSBT_AUTO.0,
        WindowBackdrop::Mica => DWMSBT_MAINWINDOW.0,
        WindowBackdrop::Acrylic => DWMSBT_TRANSIENTWINDOW.0,
        WindowBackdrop::MicaAlt => DWMSBT_TABBEDWINDOW.0,
    }
}

/// Set the backdrop attribute on `window` and report what DWM kept.
pub(crate) fn apply(
    window: &dyn winit::window::Window,
    backdrop: WindowBackdrop,
) -> BackdropStatus {
    let Some(hwnd) = hwnd_of(window) else {
        // Android composites into one Activity surface and never reaches this module; a window
        // that has no Win32 handle cannot carry a DWM attribute either.
        return BackdropStatus::Unsupported;
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
    if !backdrop.is_material() {
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
            requested: backdrop,
            reported,
        },
        Err(error) => BackdropStatus::Rejected(error.code().0),
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
