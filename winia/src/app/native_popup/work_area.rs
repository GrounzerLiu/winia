//! Monitor work-area constraints for native popups.

use winit::dpi::PhysicalPosition;
use winit::window::Window;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct NativeWorkArea {
    /// Work-area origin in parent-client logical coordinates.
    pub(crate) origin: (f32, f32),
    pub(crate) size: (f32, f32),
}

#[derive(Clone, Copy)]
struct PhysicalRect {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

pub(crate) fn from_window(window: &dyn Window) -> Option<NativeWorkArea> {
    let scale = window.scale_factor();
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    let parent_origin = platform_parent_origin(window)?;
    let rect = platform_work_area(window)?;
    if rect.width <= 0 || rect.height <= 0 {
        return None;
    }
    Some(NativeWorkArea {
        origin: (
            (rect.x - parent_origin.x) as f32 / scale as f32,
            (rect.y - parent_origin.y) as f32 / scale as f32,
        ),
        size: (
            rect.width as f32 / scale as f32,
            rect.height as f32 / scale as f32,
        ),
    })
}

#[cfg(target_os = "windows")]
fn platform_parent_origin(window: &dyn Window) -> Option<PhysicalPosition<i32>> {
    windows::client_origin(window)
}

#[cfg(not(target_os = "windows"))]
fn platform_parent_origin(window: &dyn Window) -> Option<PhysicalPosition<i32>> {
    window.outer_position().ok()
}

#[cfg(target_os = "windows")]
fn platform_work_area(window: &dyn Window) -> Option<PhysicalRect> {
    windows::work_area(window).or_else(|| monitor_bounds(window))
}

#[cfg(target_os = "macos")]
fn platform_work_area(window: &dyn Window) -> Option<PhysicalRect> {
    macos::work_area(window).or_else(|| monitor_bounds(window))
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn platform_work_area(window: &dyn Window) -> Option<PhysicalRect> {
    // Winit has no portable work-area API. Linux compositors and Android keep the full monitor
    // bounds as a conservative fallback; native positioning may still apply compositor insets.
    monitor_bounds(window)
}

fn monitor_bounds(window: &dyn Window) -> Option<PhysicalRect> {
    let monitor = window.current_monitor()?;
    let position = monitor.position()?;
    let size = monitor.current_video_mode()?.size();
    if size.width == 0 || size.height == 0 {
        return None;
    }
    Some(PhysicalRect {
        x: position.x,
        y: position.y,
        width: size.width as i32,
        height: size.height as i32,
    })
}
