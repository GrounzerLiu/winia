use crate::SkiaWindow;
use skia_safe::{ISize, ImageInfo, Surface};
use softbuffer::SoftBufferError;
use std::num::NonZeroU32;
use std::ops::Deref;
use std::sync::Arc;
use winit::dpi::PhysicalSize;
use winit::window::Window;

pub struct SoftSkiaWindow {
    skia_surface: Surface,
    soft_buffer_surface: softbuffer::Surface<Arc<Window>, Arc<Window>>,
}

fn create_surface(size: impl Into<PhysicalSize<u32>>) -> Surface {
    let size = size.into();
    let width = size.width;
    let height = size.height;
    skia_safe::surfaces::raster_n32_premul(ISize::new(width as i32, height as i32)).unwrap()
}

impl SoftSkiaWindow {
    pub fn new(window: Window) -> Self {
        let window = Arc::new(window);
        let size = window.inner_size();
        let skia_surface = create_surface(size);
        let soft_buffer_context = softbuffer::Context::new(window.clone()).unwrap();
        let soft_buffer_surface = softbuffer::Surface::new(&soft_buffer_context, window.clone()).unwrap();
        Self {
            skia_surface,
            soft_buffer_surface,
        }
    }
}

impl SkiaWindow for SoftSkiaWindow {

    fn resize(&mut self) -> Result<(), SoftBufferError> {
        let size = self.soft_buffer_surface.window().inner_size();
        let width = NonZeroU32::new(size.width).unwrap();
        let height = NonZeroU32::new(size.height).unwrap();
        let result = self.soft_buffer_surface.resize(width, height);
        match result {
            Ok(_) => {
                self.skia_surface = create_surface(size);
                Ok(())
            }
            Err(e) => {
                Err(e)
            }
        }
    }

    fn surface(&mut self) -> &mut Surface {
        &mut self.skia_surface
    }

    fn present(&mut self) {
        let size = self.soft_buffer_surface.window().inner_size();
        let mut soft_buffer = self.soft_buffer_surface.buffer_mut().unwrap();
        let u8_slice = bytemuck::cast_slice_mut::<u32, u8>(&mut soft_buffer);
        let image_info = ImageInfo::new_n32_premul((size.width as i32, size.height as i32), None);
        self.skia_surface.read_pixels(
            &image_info,
            u8_slice,
            size.width as usize * 4,
            (0, 0),
        );
        soft_buffer.present().unwrap();
    }
}

impl Deref for SoftSkiaWindow {
    type Target = Window;

    fn deref(&self) -> &Self::Target {
        self.soft_buffer_surface.window()
    }
}

impl AsRef<Window> for SoftSkiaWindow {
    fn as_ref(&self) -> &Window {
        self.soft_buffer_surface.window()
    }
}