use std::num::NonZeroU32;
use std::sync::Arc;

use parking_lot::Mutex;

use glutin::context::{PossiblyCurrentContext, PossiblyCurrentGlContext};
use glutin::surface::{GlSurface, Surface, WindowSurface};
use skia_safe::gpu::gl::FramebufferInfo;
use skia_safe::gpu::{backend_render_targets, surfaces, DirectContext, SurfaceOrigin};
use skia_safe::ColorType;
use winit::window::Window;

pub struct GlRenderer {
    pub window: Arc<Box<dyn Window>>,
    surface: Surface<WindowSurface>,
    context: PossiblyCurrentContext,
    skia_ctx: Arc<Mutex<DirectContext>>,
    swapchain_is_valid: bool,
}

impl GlRenderer {
    pub fn new(
        window: Arc<Box<dyn Window>>,
        surface: Surface<WindowSurface>,
        context: PossiblyCurrentContext,
        skia_ctx: Arc<Mutex<DirectContext>>,
    ) -> Self {
        GlRenderer {
            window,
            surface,
            context,
            skia_ctx,
            swapchain_is_valid: true,
        }
    }

    pub fn invalidate_swapchain(&mut self) {
        self.swapchain_is_valid = false;
    }

    pub fn prepare_swapchain(&mut self) {
        if !self.swapchain_is_valid {
            let size = self.window.surface_size();
            if size.width > 0 && size.height > 0 {
                // Ensure context is current before resizing
                if let Err(e) = self.context.make_current(&self.surface) {
                    eprintln!("Failed to make context current for resize: {:?}", e);
                    return;
                }
                // For GL, resizing the surface is handled by calling resize
                let width = NonZeroU32::new(size.width).unwrap();
                let height = NonZeroU32::new(size.height).unwrap();
                self.surface.resize(&self.context, width, height);
                self.swapchain_is_valid = true;
            }
        }
    }

    pub fn draw<F>(&mut self, f: F)
    where
        F: FnOnce(&mut skia_safe::Surface),
    {
        // Ensure context is current for this window (important for multi-window)
        if let Err(e) = self.context.make_current(&self.surface) {
            eprintln!("Failed to make GL context current: {:?}", e);
            return;
        }

        // Ensure swapchain is valid
        if !self.swapchain_is_valid {
            self.prepare_swapchain();
        }

        let size = self.window.surface_size();
        if size.width == 0 || size.height == 0 {
            return;
        }

        // Create Skia surface for the GL framebuffer
        let fb_info = FramebufferInfo {
            fboid: 0, // Default framebuffer
            format: skia_safe::gpu::gl::Format::RGBA8.into(),
            ..Default::default()
        };

        let backend_render_target = backend_render_targets::make_gl(
            (size.width as i32, size.height as i32),
            Some(0), // samples
            0,       // stencil bits
            fb_info,
        );

        let mut skia_ctx = self.skia_ctx.lock();

        let mut skia_surface = match surfaces::wrap_backend_render_target(
            &mut skia_ctx,
            &backend_render_target,
            SurfaceOrigin::BottomLeft,
            ColorType::RGBA8888,
            None,
            None,
        ) {
            Some(s) => s,
            None => {
                eprintln!("Failed to create Skia surface from GL framebuffer");
                return;
            }
        };

        // Call the user's draw function
        f(&mut skia_surface);

        // Flush Skia rendering
        skia_ctx.flush_and_submit();

        // Drop the lock before swapping
        drop(skia_ctx);

        // Swap buffers to present
        if let Err(e) = self.surface.swap_buffers(&self.context) {
            eprintln!("Failed to swap buffers: {:?}", e);
        }
    }
}

impl Drop for GlRenderer {
    fn drop(&mut self) {
        // Make context current before cleanup (ignore errors - context may already be invalid)
        if self.context.make_current(&self.surface).is_ok() {
            let mut ctx = self.skia_ctx.lock();
            ctx.flush_and_submit();
            ctx.release_resources_and_abandon();
        }
    }
}
