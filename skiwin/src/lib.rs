pub mod cpu;
pub mod error;
#[cfg(feature = "vulkan")]
pub mod vulkan;
#[cfg(feature = "vulkan")]
pub use vulkano;
#[cfg(feature = "gl")]
pub mod gl;
// mod d3d;

#[cfg(feature = "gl")]
pub use glutin;

use skia_safe::Surface;
use std::ops::Deref;
use std::sync::Arc;
use winit::window::Window;

pub type SoftBufferSurface = softbuffer::Surface<Arc<Box<dyn Window>>, Arc<Box<dyn Window>>>;

pub trait SkiaWindowTrait: Deref<Target=dyn Window> + AsRef<dyn Window> {
    fn destroy_surface(&mut self);
    fn recreate_surface(&mut self);
    fn resize(&mut self);
    fn draw(&mut self, draw_fn: impl FnOnce(&mut Surface));
}

pub enum SkiaWindow {
    Cpu(cpu::SoftSkiaWindow),
    #[cfg(feature = "gl")]
    Gl(gl::GlSkiaWindow),
    #[cfg(feature = "vulkan")]
    Vulkan(vulkan::VulkanSkiaWindow),
}

impl Deref for SkiaWindow {
    type Target = dyn Window;

    fn deref(&self) -> &Self::Target {
        match self {
            SkiaWindow::Cpu(window) => window.deref(),
            #[cfg(feature = "gl")]
            SkiaWindow::Gl(window) => window.deref(),
            #[cfg(feature = "vulkan")]
            SkiaWindow::Vulkan(window) => window.deref(),
        }
    }
}

impl AsRef<dyn Window> for SkiaWindow {
    fn as_ref(&self) -> &dyn Window {
        match self {
            SkiaWindow::Cpu(window) => window.as_ref(),
            #[cfg(feature = "gl")]
            SkiaWindow::Gl(window) => window.as_ref(),
            #[cfg(feature = "vulkan")]
            SkiaWindow::Vulkan(window) => window.as_ref(),
        }
    }
}

impl SkiaWindowTrait for SkiaWindow {
    fn destroy_surface(&mut self) {
        match self {
            SkiaWindow::Cpu(window) => window.destroy_surface(),
            #[cfg(feature = "gl")]
            SkiaWindow::Gl(window) => window.destroy_surface(),
            #[cfg(feature = "vulkan")]
            SkiaWindow::Vulkan(window) => window.destroy_surface(),
        }
    }

    fn recreate_surface(&mut self) {
        match self {
            SkiaWindow::Cpu(window) => window.recreate_surface(),
            #[cfg(feature = "gl")]
            SkiaWindow::Gl(window) => window.recreate_surface(),
            #[cfg(feature = "vulkan")]
            SkiaWindow::Vulkan(window) => window.recreate_surface(),
        }
    }

    fn resize(&mut self) {
        match self {
            SkiaWindow::Cpu(window) => window.resize(),
            #[cfg(feature = "gl")]
            SkiaWindow::Gl(window) => window.resize(),
            #[cfg(feature = "vulkan")]
            SkiaWindow::Vulkan(window) => window.resize(),
        }
    }

    fn draw(&mut self, draw_fn: impl FnOnce(&mut Surface)) {
        match self {
            SkiaWindow::Cpu(window) => window.draw(draw_fn),
            #[cfg(feature = "gl")]
            SkiaWindow::Gl(window) => window.draw(draw_fn),
            #[cfg(feature = "vulkan")]
            SkiaWindow::Vulkan(window) => window.draw(draw_fn),
        }
    }
}

impl From<cpu::SoftSkiaWindow> for SkiaWindow {
    fn from(window: cpu::SoftSkiaWindow) -> Self {
        SkiaWindow::Cpu(window)
    }
}

#[cfg(feature = "gl")]
impl From<gl::GlSkiaWindow> for SkiaWindow {
    fn from(window: gl::GlSkiaWindow) -> Self {
        SkiaWindow::Gl(window)
    }
}

#[cfg(feature = "vulkan")]
impl From<vulkan::VulkanSkiaWindow> for SkiaWindow {
    fn from(window: vulkan::VulkanSkiaWindow) -> Self {
        SkiaWindow::Vulkan(window)
    }
}

