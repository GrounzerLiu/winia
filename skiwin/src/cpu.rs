use crate::{capture, SkiwinError, SkiwinResult, SkiaWindowTrait, SoftBufferSurface};
use skia_safe::{ImageInfo, Surface};
use std::num::NonZeroU32;
use std::ops::Deref;
use std::sync::Arc;
use winit::window::Window;

/// Software renderer: Skia draws into the softbuffer surface's own pixels, so the frame reaches the
/// window with no GPU involved. This is the fallback that always works — it needs a window and
/// nothing else — which is why every failure below is reported rather than fatal: `SkiaWindow::new`
/// decides whether a failure is the end of the road, not this file.
pub struct SoftSkiaWindow {
    soft_buffer_surface: Option<SoftBufferSurface>,
    window: Arc<Box<dyn Window>>,
}

impl SoftSkiaWindow {
    pub fn new(window: Arc<Box<dyn Window>>) -> SkiwinResult<Self> {
        let mut this = Self {
            soft_buffer_surface: None,
            window,
        };
        this.ensure_surface()?;
        // Size the buffer now: `draw` wraps the buffer's pixels in a Skia surface, and a surface
        // that was never sized has none to wrap.
        this.resize();
        Ok(this)
    }

    fn ensure_surface(&mut self) -> SkiwinResult<()> {
        if self.soft_buffer_surface.is_none() {
            let context = softbuffer::Context::new(self.window.clone())
                .map_err(|e| SkiwinError::SoftBuffer(format!("creating the context: {e}")))?;
            let surface = softbuffer::Surface::new(&context, self.window.clone())
                .map_err(|e| SkiwinError::SoftBuffer(format!("creating the surface: {e}")))?;
            self.soft_buffer_surface = Some(surface);
        }
        Ok(())
    }
}

impl SkiaWindowTrait for SoftSkiaWindow {
    fn destroy_surface(&mut self) {
        self.soft_buffer_surface = None;
    }

    fn recreate_surface(&mut self) {
        if let Err(e) = self.ensure_surface() {
            log::error!("[skiwin] could not recreate the softbuffer surface: {e}");
        }
    }

    fn resize(&mut self) {
        if let Err(e) = self.ensure_surface() {
            log::error!("[skiwin] resizing without a softbuffer surface: {e}");
            return;
        }
        let Some(soft_buffer_surface) = self.soft_buffer_surface.as_mut() else {
            return;
        };
        let size = self.window.surface_size();
        let width = NonZeroU32::new(size.width).unwrap_or(NonZeroU32::new(1).unwrap());
        let height = NonZeroU32::new(size.height).unwrap_or(NonZeroU32::new(1).unwrap());
        if let Err(e) = soft_buffer_surface.resize(width, height) {
            log::error!("[skiwin] could not resize the softbuffer surface: {e}");
            return;
        }
        // `buffer_mut` derefs to `&mut [u32]`; clear it so a resize that is not followed by a draw
        // shows transparent black rather than the previous frame stretched over the new size.
        if let Ok(mut buffer) = soft_buffer_surface.buffer_mut() {
            buffer.fill(0);
        }
    }

    fn draw(&mut self, draw_fn: impl FnOnce(&mut Surface)) {
        let Some(soft_buffer_surface) = self.soft_buffer_surface.as_mut() else {
            log::error!("[skiwin] draw without a softbuffer surface; dropping the frame");
            return;
        };
        let mut buffer = match soft_buffer_surface.buffer_mut() {
            Ok(buffer) => buffer,
            Err(e) => {
                log::error!("[skiwin] could not get the softbuffer pixels: {e}");
                return;
            }
        };
        let h = buffer.height().get();
        let w = buffer.width().get();
        if w == 0 || h == 0 {
            return;
        }

        // SAFETY: `u32` is 4 bytes with no padding, so the slice is exactly `w * h * 4` bytes of
        // pixel memory, and Skia is confined to it by the `ImageInfo` and stride below.
        let pixels: &mut [u32] = &mut *buffer;
        let bytes: &mut [u8] = unsafe {
            std::slice::from_raw_parts_mut(pixels.as_mut_ptr() as *mut u8, pixels.len() * 4)
        };
        let stride = (w as usize) * 4;
        let image_info = ImageInfo::new_n32_premul((w as i32, h as i32), None);
        let Some(mut surface) = skia_safe::surfaces::wrap_pixels(&image_info, bytes, stride, None)
        else {
            log::error!("[skiwin] could not wrap the softbuffer pixels in a Skia surface");
            return;
        };

        let want_capture = capture::take_capture_request();
        draw_fn(&mut surface);
        // The pixels ARE the presented frame here, so a capture reads them straight back — no flush
        // to order against, unlike the GPU backends.
        if want_capture {
            capture::capture_surface(&mut surface);
        }
        drop(surface);

        // softbuffer 0.4+: present() consumes the buffer.
        if let Err(e) = buffer.present() {
            log::error!("[skiwin] could not present the softbuffer frame: {e}");
        }
    }
}

impl Deref for SoftSkiaWindow {
    type Target = dyn Window;

    fn deref(&self) -> &Self::Target {
        self.window.deref().deref()
    }
}

impl AsRef<dyn Window> for SoftSkiaWindow {
    fn as_ref(&self) -> &dyn Window {
        self.window.deref().as_ref()
    }
}
