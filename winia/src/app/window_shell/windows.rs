//! Win32 implementation of taskbar hiding, tool-window styles and application desktop toolbars.

use super::{AppBarEdge, AppBarError, AppBarOptions, AppBarStatus};
use crate::app::window_state::{self, WindowId};
use std::cell::RefCell;
use std::mem::size_of;
use std::ptr::NonNull;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
};
use windows::Win32::UI::Shell::{
    ABE_BOTTOM, ABE_LEFT, ABE_RIGHT, ABE_TOP, ABM_NEW, ABM_QUERYPOS, ABM_REMOVE, ABM_SETPOS,
    ABN_POSCHANGED, APPBARDATA, DefSubclassProc, RemoveWindowSubclass, SHAppBarMessage,
    SetWindowSubclass,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetWindowLongPtrW, RegisterWindowMessageW, SWP_FRAMECHANGED, SWP_NOACTIVATE,
    SWP_NOZORDER, SetWindowLongPtrW, SetWindowPos, WM_APP, WM_DESTROY, WM_DISPLAYCHANGE,
    WM_DPICHANGED, WM_WINDOWPOSCHANGED, WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};
use windows::core::w;
use winit::platform::windows::WindowExtWindows;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

const SUBCLASS_ID: usize = 0x5749_4e41;

/// Owns the subclass and app-bar registration for one native window.
pub(crate) struct ShellWindow {
    state: Option<Box<ShellState>>,
}

struct ShellState {
    // The box is stable: the subclass stores this pointer for the window's whole lifetime.
    inner: RefCell<Inner>,
    taskbar_created: u32,
    // ShellWindow is dropped before WgpuWindow, so the winit window outlives every callback.
    window: Option<NonNull<dyn winit::window::Window>>,
}

struct Inner {
    hwnd: HWND,
    window_id: Option<WindowId>,
    skip_taskbar: bool,
    app_bar: Option<AppBarOptions>,
    scale_factor: f64,
    appbar_registered: bool,
    monitor: isize,
    in_shell_call: bool,
}

impl ShellWindow {
    pub(crate) fn configure(
        window: &dyn winit::window::Window,
        options: crate::app::WindowOptions,
        window_id: Option<WindowId>,
    ) -> Option<Self> {
        if !options.skip_taskbar
            && !options.tool_window
            && !options.no_activate
            && options.app_bar.is_none()
        {
            return None;
        }
        let hwnd = hwnd_of(window)?;
        apply_extended_styles(
            hwnd,
            options.skip_taskbar || options.tool_window || options.app_bar.is_some(),
            options.tool_window,
            options.no_activate,
        );
        if options.skip_taskbar {
            window.set_skip_taskbar(true);
        }
        let taskbar_created = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
        let state = Box::new(ShellState {
            taskbar_created,
            inner: RefCell::new(Inner {
                hwnd,
                window_id,
                skip_taskbar: options.skip_taskbar,
                app_bar: options.app_bar,
                scale_factor: window.scale_factor(),
                appbar_registered: false,
                monitor: 0,
                in_shell_call: false,
            }),
            window: Some(unsafe {
                NonNull::new_unchecked(
                    window as *const dyn winit::window::Window as *mut dyn winit::window::Window,
                )
            }),
        });
        let installed = unsafe {
            SetWindowSubclass(
                hwnd,
                Some(subclass_proc),
                SUBCLASS_ID,
                (&*state as *const ShellState) as usize,
            )
            .as_bool()
        };
        if !installed {
            if let Some(id) = window_id {
                window_state::set_app_bar_status(
                    id,
                    AppBarStatus::Failed(AppBarError::ShellUnavailable),
                );
            }
            log::warn!("[window-shell] SetWindowSubclass failed");
            return None;
        }
        let shell = Self { state: Some(state) };
        if let Some(app_bar) = options.app_bar {
            if !app_bar.is_valid() {
                shell.set_status(AppBarStatus::Failed(AppBarError::InvalidThickness));
            } else {
                shell.reposition_app_bar();
            }
        }
        Some(shell)
    }

    fn set_status(&self, status: AppBarStatus) {
        let Some(state) = self.state.as_ref() else {
            return;
        };
        if let Some(id) = state.inner.borrow().window_id {
            window_state::set_app_bar_status(id, status);
        }
    }

    fn reposition_app_bar(&self) {
        let Some(state) = self.state.as_ref() else {
            return;
        };
        let Ok(mut inner) = state.inner.try_borrow_mut() else {
            return;
        };
        if inner.in_shell_call || inner.app_bar.is_none() {
            return;
        }
        let edge = inner.app_bar.unwrap().edge;
        inner.in_shell_call = true;
        let result = inner.reposition();
        inner.in_shell_call = false;
        drop(inner);
        match result {
            Ok(()) => self.set_status(AppBarStatus::Registered { edge }),
            Err(error) => self.set_status(AppBarStatus::Failed(error)),
        }
    }

    fn unregister_app_bar(&self) {
        let Some(state) = self.state.as_ref() else {
            return;
        };
        let Ok(mut inner) = state.inner.try_borrow_mut() else {
            return;
        };
        if !inner.appbar_registered {
            return;
        }
        inner.appbar_registered = false;
        let mut data = app_bar_data(inner.hwnd, inner.app_bar.unwrap().edge);
        unsafe { SHAppBarMessage(ABM_REMOVE, &mut data) };
        let id = inner.window_id;
        drop(inner);
        if let Some(id) = id {
            window_state::set_app_bar_status(id, AppBarStatus::Unregistered);
        }
    }

    fn shutdown(&self) {
        self.unregister_app_bar();
        let Some(state) = self.state.as_ref() else {
            return;
        };
        let Ok(inner) = state.inner.try_borrow() else {
            return;
        };
        let hwnd = inner.hwnd;
        let _ = unsafe { RemoveWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID) };
    }
}

impl Drop for ShellWindow {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl Inner {
    fn reposition(&mut self) -> Result<(), AppBarError> {
        let options = self.app_bar.ok_or(AppBarError::InvalidWindowKind)?;
        if !options.is_valid() {
            return Err(AppBarError::InvalidThickness);
        }
        let monitor = unsafe { MonitorFromWindow(self.hwnd, MONITOR_DEFAULTTONEAREST) };
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..MONITORINFO::default()
        };
        if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
            return Err(AppBarError::MonitorUnavailable);
        }
        let monitor_changed =
            self.appbar_registered && self.monitor != 0 && self.monitor != monitor.0 as isize;
        if monitor_changed {
            let mut remove = app_bar_data(self.hwnd, options.edge);
            unsafe { SHAppBarMessage(ABM_REMOVE, &mut remove) };
            self.appbar_registered = false;
        }
        self.monitor = monitor.0 as isize;
        let mut data = app_bar_data(self.hwnd, options.edge);
        data.rc = info.rcMonitor;
        if !self.appbar_registered && unsafe { SHAppBarMessage(ABM_NEW, &mut data) } == 0 {
            return Err(AppBarError::ShellUnavailable);
        }
        self.appbar_registered = true;
        let extent = match options.edge {
            AppBarEdge::Left | AppBarEdge::Right => info.rcMonitor.bottom - info.rcMonitor.top,
            AppBarEdge::Top | AppBarEdge::Bottom => info.rcMonitor.right - info.rcMonitor.left,
        };
        if extent <= 0 {
            return Err(AppBarError::MonitorUnavailable);
        }
        let thickness = (options.thickness as f64 * self.scale_factor)
            .round()
            .clamp(1.0, extent as f64) as i32;
        match options.edge {
            AppBarEdge::Left => data.rc.right = data.rc.left + thickness,
            AppBarEdge::Top => data.rc.bottom = data.rc.top + thickness,
            AppBarEdge::Right => data.rc.left = data.rc.right - thickness,
            AppBarEdge::Bottom => data.rc.top = data.rc.bottom - thickness,
        }
        unsafe { SHAppBarMessage(ABM_QUERYPOS, &mut data) };
        unsafe { SHAppBarMessage(ABM_SETPOS, &mut data) };
        let width = data.rc.right - data.rc.left;
        let height = data.rc.bottom - data.rc.top;
        unsafe {
            SetWindowPos(
                self.hwnd,
                None,
                data.rc.left,
                data.rc.top,
                width,
                height,
                SWP_FRAMECHANGED | SWP_NOACTIVATE | SWP_NOZORDER,
            )
        }
        .map_err(|_| AppBarError::ShellUnavailable)?;
        Ok(())
    }
}

fn app_bar_data(hwnd: HWND, edge: AppBarEdge) -> APPBARDATA {
    APPBARDATA {
        cbSize: size_of::<APPBARDATA>() as u32,
        hWnd: hwnd,
        uCallbackMessage: WM_APP,
        uEdge: match edge {
            AppBarEdge::Left => ABE_LEFT,
            AppBarEdge::Top => ABE_TOP,
            AppBarEdge::Right => ABE_RIGHT,
            AppBarEdge::Bottom => ABE_BOTTOM,
        },
        rc: windows::Win32::Foundation::RECT::default(),
        lParam: LPARAM(0),
    }
}

fn apply_extended_styles(hwnd: HWND, force_no_taskbar: bool, tool_window: bool, no_activate: bool) {
    let current = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32;
    let styles = extended_styles(current, force_no_taskbar, tool_window, no_activate);
    unsafe { SetWindowLongPtrW(hwnd, GWL_EXSTYLE, styles as isize) };
}

fn extended_styles(
    current: u32,
    force_no_taskbar: bool,
    tool_window: bool,
    no_activate: bool,
) -> u32 {
    let mut styles = current;
    if tool_window {
        styles |= WS_EX_TOOLWINDOW.0;
    }
    if no_activate {
        styles |= WS_EX_NOACTIVATE.0;
    }
    if force_no_taskbar {
        styles &= !WS_EX_APPWINDOW.0;
    }
    styles
}

fn hwnd_of(window: &dyn winit::window::Window) -> Option<HWND> {
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(HWND(handle.hwnd.get() as *mut core::ffi::c_void)),
        _ => None,
    }
}

unsafe extern "system" fn subclass_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    _lparam: LPARAM,
    _subclass_id: usize,
    ref_data: usize,
) -> LRESULT {
    // SAFETY: ref_data points at the stable ShellState box owned by ShellWindow.
    let state = unsafe { &*(ref_data as *const ShellState) };
    let taskbar_created = state.taskbar_created;
    if message != 0 && message == taskbar_created {
        // Explorer owns both the taskbar tab list and the app-bar registry after a restart.
        if skip_taskbar_requested(state) {
            if let Some(window) = state.window {
                unsafe { (&*window.as_ptr()).set_skip_taskbar(true) };
            }
        }
        if let Ok(mut inner) = state.inner.try_borrow_mut()
            && inner.app_bar.is_some()
        {
            inner.appbar_registered = false;
        }
        reposition_from_subclass(state);
    } else if message == WM_APP && wparam.0 as u32 == ABN_POSCHANGED {
        // The shell negotiated a new work area; run the query/set sequence again.
        reposition_from_subclass(state);
    } else if message == WM_DPICHANGED {
        let dpi = (wparam.0 as u32 >> 16) as f64;
        if dpi > 0.0 {
            if let Ok(mut inner) = state.inner.try_borrow_mut() {
                inner.scale_factor = dpi / 96.0;
            }
        }
        reposition_from_subclass(state);
    } else if message == WM_DISPLAYCHANGE {
        reposition_from_subclass(state);
    } else if message == WM_WINDOWPOSCHANGED {
        let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
        let changed = monitor_changed(state, monitor.0 as isize);
        if changed {
            reposition_from_subclass(state);
        }
    } else if message == WM_DESTROY {
        unregister_from_subclass(state);
    }
    unsafe { DefSubclassProc(hwnd, message, wparam, _lparam) }
}

fn skip_taskbar_requested(state: &ShellState) -> bool {
    state
        .inner
        .try_borrow()
        .map(|inner| inner.skip_taskbar)
        .unwrap_or(false)
}

fn monitor_changed(state: &ShellState, monitor: isize) -> bool {
    state
        .inner
        .try_borrow()
        .map(|inner| inner.monitor != monitor)
        .unwrap_or(false)
}

fn reposition_from_subclass(state: &ShellState) {
    let Ok(mut inner) = state.inner.try_borrow_mut() else {
        return;
    };
    if inner.in_shell_call || inner.app_bar.is_none() {
        return;
    }
    inner.in_shell_call = true;
    let result = inner.reposition();
    let status = match result {
        Ok(()) => AppBarStatus::Registered {
            edge: inner.app_bar.unwrap().edge,
        },
        Err(error) => AppBarStatus::Failed(error),
    };
    let id = inner.window_id;
    inner.in_shell_call = false;
    drop(inner);
    if let Some(id) = id {
        window_state::set_app_bar_status(id, status);
    }
}

fn unregister_from_subclass(state: &ShellState) {
    let Ok(mut inner) = state.inner.try_borrow_mut() else {
        return;
    };
    if !inner.appbar_registered {
        return;
    }
    inner.appbar_registered = false;
    let mut data = app_bar_data(inner.hwnd, inner.app_bar.unwrap().edge);
    unsafe { SHAppBarMessage(ABM_REMOVE, &mut data) };
    let id = inner.window_id;
    drop(inner);
    if let Some(id) = id {
        window_state::set_app_bar_status(id, AppBarStatus::Unregistered);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extended_style_mask_keeps_unrelated_bits() {
        let current = 0x1000u32 | WS_EX_APPWINDOW.0;
        let styles = extended_styles(current, true, true, true);
        assert_eq!(styles & 0x1000, 0x1000);
        assert!(styles & WS_EX_TOOLWINDOW.0 != 0);
        assert!(styles & WS_EX_NOACTIVATE.0 != 0);
        assert!(styles & WS_EX_APPWINDOW.0 == 0);
    }

    #[test]
    fn callback_reads_are_reentrant_safe() {
        let state = ShellState {
            taskbar_created: 123,
            inner: RefCell::new(Inner {
                hwnd: HWND(std::ptr::null_mut()),
                window_id: None,
                skip_taskbar: true,
                app_bar: None,
                scale_factor: 1.0,
                appbar_registered: false,
                monitor: 7,
                in_shell_call: true,
            }),
            window: None,
        };
        let _held = state.inner.borrow_mut();
        assert_eq!(state.taskbar_created, 123);
        assert!(!skip_taskbar_requested(&state));
        assert!(!monitor_changed(&state, 8));
    }

    #[test]
    fn no_activate_alone_does_not_hide_the_taskbar_button() {
        let current = 0x1000u32 | WS_EX_APPWINDOW.0;
        let styles = extended_styles(current, false, false, true);
        assert!(styles & WS_EX_NOACTIVATE.0 != 0);
        assert!(styles & WS_EX_APPWINDOW.0 != 0);
    }
}
