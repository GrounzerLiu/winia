use glutin::config::{ConfigSurfaceTypes, ConfigTemplate, ConfigTemplateBuilder, GlConfig};
use glutin::display::{GetGlDisplay, GlDisplay};
use skia_safe::gpu::DirectContext;
use skia_safe::{Borrows, Color4f, ImageInfo, Surface};
use std::num::NonZeroU32;
use std::ops::Deref;
use std::sync::Arc;
use winit::window::Window;

use crate::{create_surface, impl_skia_window, SkiaWindow};
#[cfg(target_os = "macos")]
use glutin::api::cgl::{device::Device, display::Display};
#[cfg(any(target_os = "android", target_os = "linux"))]
use glutin::api::egl::{device::Device, display::Display};
#[cfg(target_os = "windows")]
use glutin::api::egl::{device::Device, display::Display};
use glutin::context::{ContextApi, ContextAttributesBuilder};
use parking_lot::Mutex;
use pixels::{Error, Pixels, PixelsBuilder, SurfaceTexture};
use pixels::wgpu::{Backends, Color, TextureFormat};
use softbuffer::SoftBufferError;

pub struct GlSkiaWindow {
    pixels: Pixels<'static>,
    skia_surface: Arc<Mutex<Surface>>,
    skia_context: DirectContext,
    window: Arc<dyn Window>,
}

impl GlSkiaWindow {
    pub fn new(window: Arc<dyn Window>, device_selector: Option<Box<dyn Fn(&Device) -> bool>>) -> Self {
        let devices = Device::query_devices()
            .expect("Failed to query devices")
            .collect::<Vec<_>>();

        let device = if let Some(selector) = device_selector {
            devices.into_iter().find(|device| selector(device))
        } else {
            devices.into_iter().next()
        }
        .expect("No device found");

        // Create a display using the device.
        let display =
            unsafe { Display::with_device(&device, None) }.expect("Failed to create display");

        let template = config_template();
        let config = unsafe { display.find_configs(template) }
            .unwrap()
            .reduce(|config, acc| {
                if config.num_samples() > acc.num_samples() {
                    config
                } else {
                    acc
                }
            })
            .expect("No available configs");

        // println!("Picked a config with {} samples", config.num_samples());

        // Context creation.
        //
        // In particular, since we are doing offscreen rendering we have no raw window
        // handle to provide.
        let context_attributes = ContextAttributesBuilder::new().build(None);

        // Since glutin by default tries to create OpenGL core context, which may not be
        // present we should try gles.
        let fallback_context_attributes = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::OpenGl(None))
            .build(None);

        let not_current = unsafe {
            display
                .create_context(&config, &context_attributes)
                .unwrap_or_else(|_| {
                    display
                        .create_context(&config, &fallback_context_attributes)
                        .expect("failed to create context")
                })
        };

        // Make the context current for rendering
        let context = not_current.make_current_surfaceless().unwrap();
        // println!("Context created: {:?}", context.is_current());

        let interface = skia_safe::gpu::gl::Interface::new_load_with_cstr(|name| {
            context.display().get_proc_address(name)
        })
        .unwrap();

        let mut skia_context = skia_safe::gpu::direct_contexts::make_gl(interface, None).unwrap();
/*
        // let window = Arc::new(window);
        let size = window.inner_size();
        let skia_surface = create_surface(&mut skia_context, size);
        // let soft_buffer_context = softbuffer::Context::new(window.clone()).unwrap();
        // let soft_buffer_surface =
        //     softbuffer::Surface::new(&soft_buffer_context, window.clone()).unwrap();*/


        let pixels = {
            let window_size = window.surface_size();
            let surface_texture =
                SurfaceTexture::new(window_size.width, window_size.height, window.clone());
            // Pixels::new(window_size.width, window_size.height, surface_texture).unwrap()
            PixelsBuilder::new(window_size.width, window_size.height, surface_texture)
                .wgpu_backend(Backends::GL)
                // .render_texture_format(TextureFormat::Rgba8UnormSrgb)
                .texture_format(TextureFormat::Bgra8UnormSrgb)
                .build().unwrap()
        };

        // let skia_context = {
        //     let interface = skia_safe::gpu::gl::Interface::new_native().unwrap();
        //     skia_safe::gpu::direct_contexts::make_gl(interface, None).unwrap()
        // };

        let skia_surface = create_surface(&mut skia_context.clone(), window.surface_size());

        Self {
            pixels,
            skia_surface,
            skia_context,
            window,
        }
    }
}

// impl_skia_window!(GlSkiaWindow);
impl SkiaWindow for GlSkiaWindow {
    fn resize(&mut self) {
        let size = self.window.surface_size();
        self.pixels.resize_buffer(size.width, size.height).unwrap();
        self.pixels.resize_surface(size.width, size.height).unwrap();
        self.skia_surface = create_surface(&mut self.skia_context, size);
    }

    fn surface(&self) -> Arc<Mutex<Surface>> {
        self.skia_surface.clone()
    }

    fn present(&mut self) {
        // let size = self.window.inner_size();
        // let frame = self.pixels.frame_mut();
        // let u8_slice = bytemuck::cast_slice_mut::<u8, u8>(frame);
        // let image_info = ImageInfo::new_n32_premul((size.width as i32, size.height as i32), None);
        // self.skia_context.flush_and_submit();
        // self.skia_surface.lock().canvas().clear(Color4f::new(1.0, 0.0, 0.0, 1.0));
        // let color = self.skia_surface.lock().peek_pixels().unwrap().get_color((0, 0));
        // self.skia_surface.lock().read_pixels(
        //     &image_info,
        //     u8_slice,
        //     size.width as usize * 4,
        //     (0, 0),
        // );

        let frame = self.pixels.frame_mut();
        let size = self.window.surface_size();
        let image_info = ImageInfo::new_n32_premul((size.width as i32, size.height as i32), None);
        self.skia_surface.lock().read_pixels(
            &image_info,
            frame,
            size.width as usize * 4,
            (0, 0),
        );
        match self.pixels.render() {
            Ok(()) => {}
            Err(e) =>
                panic!("pixels.render() failed: {e}"),
        }
    }
}
impl Deref for GlSkiaWindow {
    type Target = dyn Window;

    fn deref(&self) -> &Self::Target {
        self.window.as_ref()
    }
}
impl AsRef<dyn Window> for GlSkiaWindow {
    fn as_ref(&self) -> &dyn Window {
        self.window.as_ref()
    }
}

fn config_template() -> ConfigTemplate {
    ConfigTemplateBuilder::default()
        .with_alpha_size(8)
        // Offscreen rendering has no support window surface support.
        .with_surface_type(ConfigSurfaceTypes::empty())
        .build()
}
