
use std::error::Error;
use std::num::NonZeroU32;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use glutin::config::{Config, ConfigTemplateBuilder};
use glutin::context::{ContextAttributesBuilder, PossiblyCurrentContext};
use glutin::display::{Display, DisplayApiPreference, GetGlDisplay};
use glutin::prelude::{GlConfig, GlDisplay, NotCurrentGlContext, PossiblyCurrentGlContext};
use glutin::surface::{GlSurface, Surface, WindowSurface};
use glutin_winit::{ApiPreference, DisplayBuilder, GlWindow};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawWindowHandle};
use skia_safe::Surface as SkiaSurface;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::window::Window;
use crate::gl::types::GLfloat;
use crate::{gl_config_picker, GpuRenderer, Renderer};

#[derive(Default)]
pub struct GlRenderer {
    skia_surface: Option<skia_safe::Surface>,
    skia_context: Option<skia_safe::gpu::DirectContext>,
    render_context: Option<RenderContext>,
}

impl GlRenderer {
    pub fn resumed(&mut self, window: &Arc<Window>) {
        let render_context = create_window_with_render_context(window).unwrap();
        let interface = skia_safe::gpu::gl::Interface::new_native().unwrap();
        let skia_context = skia_safe::gpu::direct_contexts::make_gl(interface, None).unwrap();
        self.render_context = Some(render_context);
        self.skia_context = Some(skia_context);
    }

    pub fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::Resized(size) if size.width != 0 && size.height != 0 => {
                // if let (Some(render_context), Some(skia_context)) =
                //     (&mut self.render_context, &mut self.skia_context) {
                //     let image_info = skia_safe::ImageInfo::new_n32_premul(
                //         (size.width as i32, size.height as i32), None
                //     );
                //     let surface = skia_safe::gpu::surfaces::render_target(
                //         skia_context,
                //         skia_safe::gpu::Budgeted::Yes,
                //         &image_info,
                //         None,
                //         skia_safe::gpu::SurfaceOrigin::BottomLeft,
                //         None,
                //         false,
                //         None
                //     ).unwrap();
                //     self.skia_surface = Some(surface);
                // }
                if self.skia_surface.is_none() {
                    if let Some(skia_context) = &mut self.skia_context {
                        let image_info = skia_safe::ImageInfo::new_n32_premul(
                            (size.width as i32, size.height as i32), None
                        );
                        let surface = skia_safe::gpu::surfaces::render_target(
                            skia_context,
                            skia_safe::gpu::Budgeted::Yes,
                            &image_info,
                            None,
                            skia_safe::gpu::SurfaceOrigin::BottomLeft,
                            None,
                            false,
                            None
                        ).unwrap();
                        self.skia_surface = Some(surface);
                    }
                } else {
                    if let Some(render_context) = &mut self.render_context {
                        render_context.resize(PhysicalSize::new(
                            NonZeroU32::new(size.width).unwrap(),
                            NonZeroU32::new(size.height).unwrap(),
                        ));
                    }
                }
            },
            _ => (),
        }
    }
}

impl GpuRenderer for GlRenderer {
    fn surface(&mut self) -> Option<&mut SkiaSurface> {
        self.skia_surface.as_mut()
    }

    fn submit(&mut self) {
        if let (Some(skia_context), Some(render_context)) =
            (&mut self.skia_context, &mut self.render_context) {
            // render_context.make_current().unwrap();
            skia_context.flush_and_submit();
            render_context.swap_buffers().unwrap();
            // render_context.make_not_current().unwrap();
        }
    }
}

/// A rendering context that can be shared between tasks.
struct RenderContext {
    context: PossiblyCurrentContext,
    surface: Surface<WindowSurface>,
    renderer: Renderer,
}

unsafe impl Send for RenderContext {}

impl RenderContext {
    fn new(
        context: PossiblyCurrentContext,
        surface: Surface<WindowSurface>,
        renderer: Renderer,
    ) -> Self {
        Self { context, surface, renderer }
    }

    fn make_current(&mut self) -> Result<(), impl Error> {
        self.context.make_current(&self.surface)
    }

    fn make_not_current(&mut self) -> Result<(), impl Error> {
        self.context.make_not_current_in_place()
    }

    fn swap_buffers(&mut self) -> Result<(), impl Error> {
        self.surface.swap_buffers(&self.context)
    }

    fn draw_with_clear_color(&self, red: GLfloat, green: GLfloat, blue: GLfloat, alpha: GLfloat) {
        self.renderer.draw_with_clear_color(red, green, blue, alpha)
    }

    fn resize(&mut self, size: PhysicalSize<NonZeroU32>) {
        self.surface.resize(&self.context, size.width, size.height);
        self.renderer.resize(size.width.get() as i32, size.height.get() as i32);
    }
}

fn create_window_with_render_context(
    window: &Arc<Window>,
) -> Result<RenderContext, Box<dyn Error>> {

    let template = ConfigTemplateBuilder::new().with_alpha_size(8);

    let gl_config = get_config(
        window,
        template,
        gl_config_picker,
    )?;

    println!("Picked a config with {} samples", gl_config.num_samples());

    let raw_window_handle = window
        .as_ref().window_handle().map(|handle| handle.as_raw()).ok();

    let gl_display = gl_config.display();

    let context_attributes = ContextAttributesBuilder::new().build(raw_window_handle);

    let not_current_gl_context = unsafe {
        gl_display
            .create_context(&gl_config, &context_attributes)
            .expect("failed to create context")
    };

    let attrs = window
        .build_surface_attributes(<_>::default())
        .expect("Failed to build surface attributes");
    let gl_surface =
        unsafe { gl_config.display().create_window_surface(&gl_config, &attrs).unwrap() };

    // Make it current.
    let gl_context = not_current_gl_context.make_current(&gl_surface).unwrap();

    // The context needs to be current for the Renderer to set up shaders and
    // buffers. It also performs function loading, which needs a current context on
    // WGL.
    let renderer = Renderer::new(&gl_display);

    // let gl_context = gl_context.make_not_current().unwrap().treat_as_possibly_current();

    Ok(RenderContext::new(gl_context, gl_surface, renderer))
}


pub fn get_config<Picker>(
    window: &Arc<Window>,
    template_builder: ConfigTemplateBuilder,
    config_picker: Picker,
) -> Result<Config, Box<dyn Error>>
where
    Picker: FnOnce(Box<dyn Iterator<Item = Config> + '_>) -> Config,
{
    #[cfg(wgl_backend)]
    let raw_window_handle = window
        .as_ref()
        .and_then(|window| window.window_handle().ok())
        .map(|handle| handle.as_raw());
    #[cfg(not(wgl_backend))]
    let raw_window_handle = None;

    let gl_display = create_display(window, ApiPreference::FallbackEgl, raw_window_handle)?;

    // XXX the native window must be passed to config picker when WGL is used
    // otherwise very limited OpenGL features will be supported.
    #[cfg(wgl_backend)]
    let template_builder = if let Some(raw_window_handle) = raw_window_handle {
        template_builder.compatible_with_native_window(raw_window_handle)
    } else {
        template_builder
    };

    let template = template_builder.build();

    let gl_config = unsafe {
        let configs = gl_display.find_configs(template)?;
        config_picker(configs)
    };

    // #[cfg(not(wgl_backend))]
    // let window = if let Some(wa) = self.window_attributes.take() {
    //     Some(finalize_window(event_loop, wa, &gl_config)?)
    // } else {
    //     None
    // };

    Ok(gl_config)
}

fn create_display(
    window: &Arc<Window>,
    _api_preference: ApiPreference,
    _raw_window_handle: Option<RawWindowHandle>,
) -> Result<Display, Box<dyn Error>> {
    #[cfg(egl_backend)]
    let _preference = DisplayApiPreference::Egl;

    #[cfg(glx_backend)]
    let _preference = DisplayApiPreference::Glx(Box::new(register_xlib_error_hook));

    #[cfg(cgl_backend)]
    let _preference = DisplayApiPreference::Cgl;

    #[cfg(wgl_backend)]
    let _preference = DisplayApiPreference::Wgl(_raw_window_handle);

    #[cfg(all(egl_backend, glx_backend))]
    let _preference = match _api_preference {
        ApiPreference::PreferEgl => {
            DisplayApiPreference::EglThenGlx(Box::new(register_xlib_error_hook))
        },
        ApiPreference::FallbackEgl => {
            DisplayApiPreference::GlxThenEgl(Box::new(register_xlib_error_hook))
        },
    };

    #[cfg(all(wgl_backend, egl_backend))]
    let _preference = match _api_preference {
        ApiPreference::PreferEgl => DisplayApiPreference::EglThenWgl(_raw_window_handle),
        ApiPreference::FallbackEgl => DisplayApiPreference::WglThenEgl(_raw_window_handle),
    };

    let _preference = DisplayApiPreference::Egl; // Placeholder for compilation

    let handle = window.display_handle().unwrap().as_raw();
    unsafe { Ok(Display::new(handle, _preference)?) }
}