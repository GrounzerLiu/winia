//! Debug screenshot capture: read the presented frame back into memory for the debug server.
//!
//! Backend-neutral by design. GPU backends (Vulkan, GL) call [`capture_surface`] **after** their
//! `flush_and_submit`, because reading inside the draw closure happens before the flush and sees
//! stale or partial content. The CPU backend calls it inside its draw closure — there is no flush
//! in between, the pixels being drawn *are* the presented frame.
//!
//! `take_capture_request` is consumed at the draw entry point, including early-return paths: a flag
//! left set would otherwise be consumed by another window's next frame and capture the wrong window.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

static CAPTURE_REQUEST: AtomicBool = AtomicBool::new(false);
static CAPTURE_RESULT: Mutex<Option<(u32, u32, Vec<u8>)>> = Mutex::new(None);

/// Ask for the pixels of the next completed frame.
pub fn request_capture() {
    CAPTURE_REQUEST.store(true, Ordering::Relaxed);
}

/// Take the most recent capture (RGBA8888, physical pixels), or `None` when nothing was captured.
pub fn take_capture() -> Option<(u32, u32, Vec<u8>)> {
    CAPTURE_RESULT.lock().unwrap().take()
}

/// Consume a pending capture request. Call this at the draw entry point, not at the capture site.
pub(crate) fn take_capture_request() -> bool {
    CAPTURE_REQUEST.swap(false, Ordering::Relaxed)
}

/// Read the surface back and store it as the pending capture. Only call when
/// [`take_capture_request`] returned true.
///
/// No row flipping, on purpose: Skia's `read_pixels` returns rows top-down whatever the surface's
/// origin is, even though GL framebuffers are created `BottomLeft` and Vulkan render targets
/// `TopLeft`. Measured against a desktop screenshot for both backends at 1050x930 — a surface the
/// flip *looked* like it needed came back upright, and the flip put it upside down.
pub(crate) fn capture_surface(surface: &mut skia_safe::Surface) {
    let w = surface.width();
    let h = surface.height();
    if w <= 0 || h <= 0 {
        eprintln!("[capture] implausible surface size: {w}x{h}");
        return;
    }
    let info = skia_safe::ImageInfo::new(
        (w, h),
        skia_safe::ColorType::RGBA8888,
        skia_safe::AlphaType::Premul,
        None,
    );
    let mut pixels = vec![0u8; (w * h * 4) as usize];
    if !surface.read_pixels(&info, &mut pixels, w as usize * 4, (0, 0)) {
        eprintln!("[capture] read_pixels failed");
        return;
    }
    *CAPTURE_RESULT.lock().unwrap() = Some((w as u32, h as u32, pixels));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_request_flag_is_consumed_once_and_the_result_survives_until_taken() {
        assert!(!take_capture_request());
        request_capture();
        // A second request is idempotent, and reading it clears it.
        request_capture();
        assert!(take_capture_request());
        assert!(!take_capture_request());
        // Nothing stored yet: a request alone does not fabricate a capture.
        assert!(take_capture().is_none());
        *CAPTURE_RESULT.lock().unwrap() = Some((1, 1, vec![9, 8, 7, 6]));
        assert_eq!(take_capture(), Some((1, 1, vec![9, 8, 7, 6])));
        assert!(take_capture().is_none());
    }
}
