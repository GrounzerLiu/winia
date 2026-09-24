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

use crate::{SkiwinError, SkiwinResult};

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
    ) -> SkiwinResult<GlRenderer> {
        if self.display.is_none() {
            self.init_display(event_loop, window.clone())?;
        }

        let display = self
            .display
            .as_ref()
            .ok_or_else(|| SkiwinError::OpenGl("display was not initialised".into()))?;
        let config = self
            .config
            .as_ref()
            .ok_or_else(|| SkiwinError::OpenGl("no framebuffer config".into()))?;

        let surface = Self::create_gl_surface(display, config, window.clone())?;

        let raw_window_handle = window
            .window_handle()
            .map(|h| h.as_raw())
            .map_err(|e| SkiwinError::OpenGl(format!("no window handle: {e}")))?;
        let context_attributes = ContextAttributesBuilder::new().build(Some(raw_window_handle));
        // GLES is the fallback for platforms whose desktop-GL entry points are missing (and for
        // software rasterisers that only advertise ES).
        let fallback_context_attributes = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::Gles(None))
            .build(Some(raw_window_handle));

        let not_current_context = unsafe {
            display
                .create_context(config, &context_attributes)
                .or_else(|_| display.create_context(config, &fallback_context_attributes))
        }
        .map_err(|e| SkiwinError::OpenGl(format!("creating the context: {e}")))?;

        let context = not_current_context
            .make_current(&surface)
            .map_err(|e| SkiwinError::OpenGl(format!("making the context current: {e}")))?;

        let skia_interface = gl::Interface::new_load_with_cstr(|name| display.get_proc_address(name))
            .ok_or_else(|| SkiwinError::OpenGl("Skia could not load the GL interface".into()))?;

        let skia_ctx = direct_contexts::make_gl(skia_interface, None)
            .ok_or_else(|| SkiwinError::OpenGl("Skia could not create a GL context".into()))?;

        let _ = surface.set_swap_interval(&context, SwapInterval::Wait(NonZeroU32::new(1).unwrap()));

        Ok(GlRenderer::new(window, surface, context, Arc::new(Mutex::new(skia_ctx))))
    }

    fn init_display(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        window: Arc<Box<dyn Window>>,
    ) -> SkiwinResult<()> {
        let raw_window_handle = window
            .window_handle()
            .map(|h| h.as_raw())
            .map_err(|e| SkiwinError::OpenGl(format!("no window handle: {e}")))?;

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
                event_loop
                    .display_handle()
                    .map_err(|e| SkiwinError::OpenGl(format!("no display handle: {e}")))?
                    .as_raw(),
                display_api_preference,
            )
        }
        .map_err(|e| SkiwinError::OpenGl(format!("creating the display: {e}")))?;

        let config = unsafe { display.find_configs(template) }
            .map_err(|e| SkiwinError::OpenGl(format!("finding framebuffer configs: {e}")))?
            .reduce(|accum, config| {
                if config.num_samples() > accum.num_samples() {
                    config
                } else {
                    accum
                }
            })
            .ok_or_else(|| SkiwinError::OpenGl("no framebuffer config for this window".into()))?;

        self.display = Some(display);
        self.config = Some(config);
        Ok(())
    }

    fn create_gl_surface(
        display: &Display,
        config: &Config,
        window: Arc<Box<dyn Window>>,
    ) -> SkiwinResult<Surface<WindowSurface>> {
        let size = window.surface_size();
        let width = NonZeroU32::new(size.width).unwrap_or(NonZeroU32::new(1).unwrap());
        let height = NonZeroU32::new(size.height).unwrap_or(NonZeroU32::new(1).unwrap());

        let raw_window_handle = window
            .window_handle()
            .map(|h| h.as_raw())
            .map_err(|e| SkiwinError::OpenGl(format!("no window handle: {e}")))?;
        let surface_attrs =
            SurfaceAttributesBuilder::<WindowSurface>::new().build(raw_window_handle, width, height);

        unsafe { display.create_window_surface(config, &surface_attrs) }
            .map_err(|e| SkiwinError::OpenGl(format!("creating the window surface: {e}")))
    }
}
