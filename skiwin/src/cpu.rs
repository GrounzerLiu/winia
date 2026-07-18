use crate::{SkiaWindowTrait, SoftBufferSurface};
use skia_safe::{ImageInfo, Surface};
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
        let soft_buffer_context = softbuffer::Context::new(window.clone()).unwrap_or_else(|e| {
            let msg = format!("Failed to create softbuffer context: {e}");
            log::error!("{msg}");
            #[cfg(debug_assertions)]
            panic!("{msg}");
            #[allow(unreachable_code)]
            std::process::exit(1);
        });
        let soft_buffer_surface =
            softbuffer::Surface::new(&soft_buffer_context, window.clone()).unwrap_or_else(|e| {
                let msg = format!("Failed to create softbuffer surface: {e}");
                log::error!("{msg}");
                #[cfg(debug_assertions)]
                panic!("{msg}");
                #[allow(unreachable_code)]
                std::process::exit(1);
            });
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
            let soft_buffer_context = softbuffer::Context::new(self.window.clone()).unwrap_or_else(|e| {
                let msg = format!("Failed to recreate softbuffer context: {e}");
                log::error!("{msg}");
                #[cfg(debug_assertions)]
                panic!("{msg}");
                #[allow(unreachable_code)]
                std::process::exit(1);
            });
            self.soft_buffer_surface = Some(
                softbuffer::Surface::new(&soft_buffer_context, self.window.clone()).unwrap_or_else(|e| {
                    let msg = format!("Failed to recreate softbuffer surface: {e}");
                    log::error!("{msg}");
                    #[cfg(debug_assertions)]
                    panic!("{msg}");
                    #[allow(unreachable_code)]
                    std::process::exit(1);
                }),
            );
        }
    }

    fn resize(&mut self) {
        if self.soft_buffer_surface.is_none() {
            self.recreate_surface();
        }
        let soft_buffer_surface = self.soft_buffer_surface.as_mut().unwrap();
        let size = self.window.surface_size();
        let width = NonZeroU32::new(size.width).unwrap_or(NonZeroU32::new(1).unwrap());
        let height = NonZeroU32::new(size.height).unwrap_or(NonZeroU32::new(1).unwrap());
        soft_buffer_surface.resize(width, height).unwrap_or_else(|e| {
            log::error!("Failed to resize softbuffer surface: {e}");
            #[cfg(debug_assertions)]
            panic!("Failed to resize softbuffer surface: {e}");
        });
        // Buffer derefs to &mut [u32]: fill with 0 (fully transparent black)
        soft_buffer_surface.buffer_mut().unwrap().fill(0);
    }

    fn draw(&mut self, draw_fn: impl FnOnce(&mut Surface)) {
        let soft_buffer_surface = self.soft_buffer_surface.as_mut().unwrap();
        let buffer_result = soft_buffer_surface.buffer_mut();
        if buffer_result.is_err() {
            let e = buffer_result.unwrap_err();
            log::error!("Failed to get softbuffer: {e}");
            #[cfg(debug_assertions)]
            panic!("Failed to get softbuffer: {e}");
            #[allow(unreachable_code)]
            return;
        }
        let mut buffer = buffer_result.unwrap();

        // 先获取尺寸信息
        let h = buffer.height().get();
        let w = buffer.width().get();

        // Buffer derefs to &mut [u32]; convert to &mut [u8] for Skia
        let pixels: &mut [u32] = &mut *buffer;
        let bytes: &mut [u8] = unsafe {
            std::slice::from_raw_parts_mut(
                pixels.as_mut_ptr() as *mut u8,
                pixels.len() * 4,
            )
        };
        let stride = (w as usize) * 4;
        let image_info =
            ImageInfo::new_n32_premul((w as i32, h as i32), None);
        let mut surface = skia_safe::surfaces::wrap_pixels(
            &image_info,
            bytes,
            stride,
            None,
        ).unwrap();
        draw_fn(&mut surface);

        // softbuffer 0.4+: present() consumes the buffer
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
