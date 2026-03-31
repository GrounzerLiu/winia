use std::num::NonZeroU32;
use std::sync::Arc;

use glutin::config::{Config, ConfigSurfaceTypes, ConfigTemplateBuilder, GlConfig};
use glutin::context::{ContextApi, ContextAttributesBuilder, NotCurrentGlContext};
use glutin::display::{Display, GlDisplay};
use glutin::surface::{GlSurface, Surface, SurfaceAttributesBuilder, SwapInterval, WindowSurface};
use parking_lot::Mutex;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use skia_safe::gpu::{direct_contexts, gl};
use winit::event_loop::ActiveEventLoop;
use winit::window::Window;

use super::renderer::GlRenderer;

pub struct GlRenderContext {
    pub display: Option<Display>,
    pub config: Option<Config>,
}

impl Default for GlRenderContext {
    fn default() -> Self {
        Self {
            display: None,
            config: None,
        }
    }
}

impl GlRenderContext {
    pub fn renderer_for_window(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        window: Arc<Box<dyn Window>>,
    ) -> GlRenderer {
        if self.display.is_none() {
            self.init_display(event_loop, window.clone());
        }

        let display = self.display.as_ref().unwrap();
        let config = self.config.as_ref().unwrap();

        let surface = Self::create_gl_surface(display, config, window.clone());

        let raw_window_handle = window.window_handle().unwrap().as_raw();
        let context_attributes = ContextAttributesBuilder::new().build(Some(raw_window_handle));
        let fallback_context_attributes = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::Gles(None))
            .build(Some(raw_window_handle));

        let not_current_context = unsafe {
            display
                .create_context(config, &context_attributes)
                .unwrap_or_else(|_| {
                    display
                        .create_context(config, &fallback_context_attributes)
                        .expect("Failed to create GL context")
                })
        };

        let context = not_current_context.make_current(&surface).unwrap();

        let skia_interface =
            gl::Interface::new_load_with_cstr(|name| display.get_proc_address(name))
                .expect("Failed to create Skia GL interface");

        let skia_ctx =
            direct_contexts::make_gl(skia_interface, None).expect("Failed to create Skia context");

        let _ = surface.set_swap_interval(&context, SwapInterval::Wait(NonZeroU32::new(1).unwrap()));

        GlRenderer::new(window, surface, context, Arc::new(Mutex::new(skia_ctx)))
    }

    fn init_display(&mut self, event_loop: &dyn ActiveEventLoop, window: Arc<Box<dyn Window>>) {
        let raw_window_handle = window.window_handle().unwrap().as_raw();

        let template = ConfigTemplateBuilder::new()
            .with_alpha_size(8)
            .with_surface_type(ConfigSurfaceTypes::WINDOW)
            .compatible_with_native_window(raw_window_handle)
            .build();

        #[cfg(target_os = "windows")]
        let display_api_preference =
            glutin::display::DisplayApiPreference::Wgl(Some(raw_window_handle));

        #[cfg(target_os = "macos")]
        let display_api_preference = glutin::display::DisplayApiPreference::Cgl;

        #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
        let display_api_preference = glutin::display::DisplayApiPreference::Egl;

        let display = unsafe {
            Display::new(
                event_loop.display_handle().unwrap().as_raw(),
                display_api_preference,
            )
        }
        .expect("Failed to create GL display");

        let config = unsafe { display.find_configs(template) }
            .unwrap()
            .reduce(|accum, config| {
                if config.num_samples() > accum.num_samples() {
                    config
                } else {
                    accum
                }
            })
            .expect("No suitable GL config found");


        self.display = Some(display);
        self.config = Some(config);
    }

    fn create_gl_surface(
        display: &Display,
        config: &Config,
        window: Arc<Box<dyn Window>>,
    ) -> Surface<WindowSurface> {
        let size = window.surface_size();
        let width = NonZeroU32::new(size.width).unwrap_or(NonZeroU32::new(1).unwrap());
        let height = NonZeroU32::new(size.height).unwrap_or(NonZeroU32::new(1).unwrap());

        let raw_window_handle = window.window_handle().unwrap().as_raw();
        let surface_attrs =
            SurfaceAttributesBuilder::<WindowSurface>::new().build(raw_window_handle, width, height);

        unsafe { display.create_window_surface(config, &surface_attrs) }
            .expect("Failed to create GL window surface")
    }
}

