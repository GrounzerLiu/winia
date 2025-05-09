pub mod cpu;
#[cfg(feature = "vulkan")]
pub mod vulkan;
#[cfg(feature = "vulkan")]
pub use vulkano;
#[cfg(feature = "gl")]
pub mod gl;
// mod d3d;

#[cfg(feature = "gl")]
pub use glutin;

use parking_lot::Mutex;
use skia_safe::gpu::{Budgeted, DirectContext, SurfaceOrigin};
use skia_safe::{ImageInfo, Surface};
use softbuffer::SoftBufferError;
use std::ops::Deref;
use std::sync::Arc;
use winit::dpi::PhysicalSize;
use winit::window::Window;

pub trait SkiaWindow: Deref<Target = Window> {
    // fn resumed(&mut self);
    fn resize(&mut self) -> Result<(), SoftBufferError>;
    fn surface(&self) -> Arc<Mutex<Surface>>;
    fn present(&mut self);
}

pub(crate) fn create_surface(
    skia_context: &mut DirectContext,
    size: impl Into<PhysicalSize<u32>>,
) -> Arc<Mutex<Surface>> {
    let size = size.into();
    let width = size.width;
    let height = size.height;
    let image_info = ImageInfo::new_n32_premul((width as i32, height as i32), None);
    Arc::new(Mutex::new(
        skia_safe::gpu::surfaces::render_target(
            skia_context,
            Budgeted::Yes,
            &image_info,
            None,
            SurfaceOrigin::TopLeft,
            None,
            false,
            None,
        )
        .unwrap(),
    ))
}

#[macro_export]
macro_rules! impl_skia_window {
    ($ty:ty) => {
        impl SkiaWindow for $ty {
            fn resize(&mut self) -> Result<(), SoftBufferError> {
                let size = self.soft_buffer_surface.window().inner_size();
                let width = NonZeroU32::new(size.width).unwrap();
                let height = NonZeroU32::new(size.height).unwrap();
                let result = self.soft_buffer_surface.resize(width, height);
                match result {
                    Ok(_) => {
                        self.skia_surface = create_surface(&mut self.skia_context, size);
                        Ok(())
                    }
                    Err(e) => Err(e),
                }
            }

            fn surface(&self) -> Arc<Mutex<Surface>> {
                self.skia_surface.clone()
            }

            fn present(&mut self) {
                let size = self.soft_buffer_surface.window().inner_size();
                let mut soft_buffer = self.soft_buffer_surface.buffer_mut().unwrap();
                let u8_slice = bytemuck::cast_slice_mut::<u32, u8>(&mut soft_buffer);
                let image_info =
                    ImageInfo::new_n32_premul((size.width as i32, size.height as i32), None);
                self.skia_surface.lock().read_pixels(
                    &image_info,
                    u8_slice,
                    size.width as usize * 4,
                    (0, 0),
                );
                soft_buffer.present().unwrap();
            }
        }

        impl Deref for $ty {
            type Target = Window;

            fn deref(&self) -> &Self::Target {
                self.soft_buffer_surface.window()
            }
        }

        impl AsRef<Window> for $ty {
            fn as_ref(&self) -> &Window {
                self.soft_buffer_surface.window()
            }
        }
    };
}
