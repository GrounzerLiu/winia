use crate::{SkiaWindowTrait, SoftBufferSurface};
use skia_safe::{ImageInfo, Surface};
use softbuffer::Pixel;
use std::num::NonZeroU32;
use std::ops::Deref;
use std::sync::Arc;
use winit::window::Window;


pub struct SoftSkiaWindow {
    soft_buffer_surface: Option<SoftBufferSurface>,
    window: Arc<Box<dyn Window>>,
}


impl SoftSkiaWindow {
    pub fn new(window: Arc<Box<dyn Window>>) -> Self {
        let soft_buffer_context = softbuffer::Context::new(window.clone()).unwrap();
        let soft_buffer_surface =
            softbuffer::Surface::new(&soft_buffer_context, window.clone()).unwrap();
        Self {
            soft_buffer_surface: Some(soft_buffer_surface),
            window,
        }
    }
}

impl SkiaWindowTrait for SoftSkiaWindow {
    fn destroy_surface(&mut self) {
        self.soft_buffer_surface = None;
    }

    fn recreate_surface(&mut self) {
        if self.soft_buffer_surface.is_none() {
            let soft_buffer_context = softbuffer::Context::new(self.window.clone()).unwrap();
            self.soft_buffer_surface = Some(
                softbuffer::Surface::new(&soft_buffer_context, self.window.clone()).unwrap(),
            );
        }
    }

    fn resize(&mut self) {
        if self.soft_buffer_surface.is_none() {
            self.recreate_surface();
        }
        let soft_buffer_surface = self.soft_buffer_surface.as_mut().unwrap();
        let size = self.window.surface_size();
        let width = NonZeroU32::new(size.width).unwrap();
        let height = NonZeroU32::new(size.height).unwrap();
        soft_buffer_surface.resize(width, height).unwrap();
        soft_buffer_surface.buffer_mut().unwrap().pixels().fill(Pixel::new_rgb(0, 0, 0));
    }

    fn draw(&mut self, draw_fn: impl FnOnce(&mut Surface)) {
        let soft_buffer_surface = self.soft_buffer_surface.as_mut().unwrap();
        let mut buffer = soft_buffer_surface.buffer_mut().unwrap();
        let pixels = buffer.pixels();
        let bytes: &mut [u8] = unsafe {
            std::slice::from_raw_parts_mut(
                pixels.as_mut_ptr() as *mut u8,
                pixels.len() * size_of::<softbuffer::Pixel>(),
            )
        };
        let width = buffer.width().get();
        let height = buffer.height().get();
        let stride = buffer.byte_stride().get();
        let image_info =
            ImageInfo::new_n32_premul((width as i32, height as i32), None);
        let mut surface = skia_safe::surfaces::wrap_pixels(
            &image_info,
            bytes,
            stride as usize,
            None,
        ).unwrap();
        draw_fn(&mut surface);
        buffer.present().unwrap();
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
