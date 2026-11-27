use super::PhysicalRect;
use objc2::MainThreadMarker;
use objc2_app_kit::NSScreen;
use winit::window::Window;

pub(super) fn work_area(window: &dyn Window) -> Option<PhysicalRect> {
    let mtm = MainThreadMarker::new()?;
    let monitor = window.current_monitor()?;
    let display_id = monitor.native_id() as u32;
    let screens = NSScreen::screens(mtm);
    let main_height = NSScreen::mainScreen(mtm).map(|screen| screen.frame().size.height);
    for screen in screens.iter() {
        if screen.CGDirectDisplayID() != display_id {
            continue;
        }
        let frame = screen.frame();
        let visible = screen.visibleFrame();
        let scale = screen.backingScaleFactor();
        if !scale.is_finite() || scale <= 0.0 {
            return None;
        }
        let desktop_height = main_height.unwrap_or(frame.size.height);
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
