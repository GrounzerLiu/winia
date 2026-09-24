mod context;
mod renderer;

use crate::vulkan::context::VulkanRenderContext;
use crate::vulkan::renderer::VulkanRenderer;
use crate::{SkiwinResult, SkiaWindowTrait};
use skia_safe::Surface;
use std::ops::Deref;
use std::sync::Arc;
use winit::event_loop::ActiveEventLoop;
use winit::window::Window;

pub struct VulkanSkiaWindow {
    render_ctx: VulkanRenderContext, // the shared vulkan device, queue, etc.
    renderer: Option<VulkanRenderer>, // the window-specific skia <-> vulkan bridge
    window: Arc<Box<dyn Window>>,
}

impl VulkanSkiaWindow {
    /// Fails when this machine has no usable Vulkan (no loader, no device, no swapchain) — the
    /// error propagates so `SkiaWindow::new` can fall through to another backend.
    pub fn new(
        event_loop: &dyn ActiveEventLoop,
        window: Arc<Box<dyn Window>>,
    ) -> SkiwinResult<Self> {
        let mut render_ctx = VulkanRenderContext::default();
        let renderer = render_ctx.renderer_for_window(event_loop, window.clone())?;
        Ok(Self {
            render_ctx,
            renderer: Some(renderer),
            window,
        })
    }
}

impl SkiaWindowTrait for VulkanSkiaWindow {
    fn destroy_surface(&mut self) {
        if let Some(renderer) = self.renderer.take() {
            // Explicitly drop the renderer to ensure proper cleanup
            drop(renderer);
        }
    }

    fn recreate_surface(&mut self) {
    }

    fn resize(&mut self) {
        if let Some(renderer) = self.renderer.as_mut() {
            // When the window size changes, the framebuffers need to be reallocated to match
            // before redrawing the window contents
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

impl Deref for VulkanSkiaWindow {
    type Target = dyn Window;

    fn deref(&self) -> &Self::Target {
        self.window.as_ref().deref()
    }
}

impl AsRef<dyn Window> for VulkanSkiaWindow {
    fn as_ref(&self) -> &dyn Window {
        self.window.as_ref().as_ref()
    }
}
