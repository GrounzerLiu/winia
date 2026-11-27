use super::PhysicalRect;
use std::mem::size_of;
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::Graphics::Gdi::{
    ClientToScreen, GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
};
use winit::dpi::PhysicalPosition;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

pub(super) fn client_origin(window: &dyn Window) -> Option<PhysicalPosition<i32>> {
    let hwnd = hwnd(window)?;
    let mut point = POINT { x: 0, y: 0 };
    if !unsafe { ClientToScreen(hwnd, &mut point) }.as_bool() {
        return None;
    }
    Some(PhysicalPosition::new(point.x, point.y))
}

pub(super) fn work_area(window: &dyn Window) -> Option<PhysicalRect> {
    let hwnd = hwnd(window)?;
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return None;
    }
    let work = info.rcWork;
    Some(PhysicalRect {
        x: work.left,
        y: work.top,
        width: work.right - work.left,
        height: work.bottom - work.top,
    })
}

fn hwnd(window: &dyn Window) -> Option<HWND> {
    let RawWindowHandle::Win32(handle) = window.window_handle().ok()?.as_raw() else {
        return None;
    };
    Some(HWND(handle.hwnd.get() as *mut core::ffi::c_void))
}
