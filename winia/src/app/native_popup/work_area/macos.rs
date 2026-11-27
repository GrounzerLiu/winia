use super::PhysicalRect;
use objc2::MainThreadMarker;
use objc2_app_kit::NSScreen;
use objc2_core_graphics::{CGDisplayBounds, CGMainDisplayID};
use winit::window::Window;

pub(super) fn work_area(window: &dyn Window) -> Option<PhysicalRect> {
    let mtm = MainThreadMarker::new()?;
    let monitor = window.current_monitor()?;
    let display_id = monitor.native_id() as u32;
    let screens = NSScreen::screens(mtm);
    let desktop_height = unsafe { CGDisplayBounds(CGMainDisplayID()) }.size.height;
    for screen in screens.iter() {
        if screen.CGDirectDisplayID() != display_id {
            continue;
        }
        let visible = screen.visibleFrame();
        let scale = screen.backingScaleFactor();
        if !scale.is_finite() || scale <= 0.0 {
            return None;
        }
        let x = visible.origin.x * scale;
        let y = (desktop_height - visible.origin.y - visible.size.height) * scale;
        return Some(PhysicalRect {
            x: x.round() as i32,
            y: y.round() as i32,
            width: (visible.size.width * scale).round() as i32,
            height: (visible.size.height * scale).round() as i32,
        });
    }
    None
}
