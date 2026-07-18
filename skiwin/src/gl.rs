mod context;
mod renderer;

use crate::gl::context::GlRenderContext;
use crate::gl::renderer::GlRenderer;
use crate::SkiaWindowTrait;
use skia_safe::Surface;
use std::ops::Deref;
use std::sync::Arc;
use winit::event_loop::ActiveEventLoop;
use winit::window::Window;

pub struct GlSkiaWindow {
    #[allow(dead_code)]
    render_ctx: GlRenderContext,
    renderer: Option<GlRenderer>,
    window: Arc<Box<dyn Window>>,
}

impl GlSkiaWindow {
    pub fn new(event_loop: &dyn ActiveEventLoop, window: Arc<Box<dyn Window>>) -> Self {
        let mut render_ctx = GlRenderContext::default();
        let renderer = render_ctx.renderer_for_window(event_loop, window.clone());
        Self {
            render_ctx,
            renderer: Some(renderer),
            window,
        }
    }
}

impl SkiaWindowTrait for GlSkiaWindow {
    fn destroy_surface(&mut self) {
        if let Some(renderer) = self.renderer.take() {
            drop(renderer);
        }
    }

    fn recreate_surface(&mut self) {
        // For GL, surface recreation is handled in prepare_swapchain
    }

    fn resize(&mut self) {
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.invalidate_swapchain();
        }
    }

    fn draw(&mut self, draw_fn: impl FnOnce(&mut Surface)) {
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.prepare_swapchain();
            renderer.draw(draw_fn)
        }
    }
}

impl Deref for GlSkiaWindow {
    type Target = dyn Window;

    fn deref(&self) -> &Self::Target {
        self.window.as_ref().deref()
    }
}

impl AsRef<dyn Window> for GlSkiaWindow {
    fn as_ref(&self) -> &dyn Window {
        self.window.as_ref().as_ref()
    }
}

