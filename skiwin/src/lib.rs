pub mod capture;
pub mod cpu;
pub mod error;
#[cfg(feature = "vulkan")]
pub mod vulkan;
#[cfg(feature = "vulkan")]
pub use vulkano;
#[cfg(feature = "gl")]
pub mod gl;
// mod d3d;

pub use error::{SkiwinError, SkiwinResult};

#[cfg(feature = "gl")]
pub use glutin;

use skia_safe::Surface;
use std::ops::Deref;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use winit::event_loop::ActiveEventLoop;
use winit::window::Window;

/// The environment variable that pins a backend by name. Unset means "pick automatically".
pub const BACKEND_ENV: &str = "WINIA_RENDER_BACKEND";

/// Which renderer a window draws through. Ordered by capability: `Vulkan` is the default choice,
/// `Gl` the GPU fallback, `Cpu` the software path that needs nothing beyond a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Backend {
    Vulkan,
    Gl,
    Cpu,
}

impl Backend {
    /// Every backend, most capable first.
    pub const ALL: [Backend; 3] = [Backend::Vulkan, Backend::Gl, Backend::Cpu];

    pub fn name(self) -> &'static str {
        match self {
            Backend::Vulkan => "vulkan",
            Backend::Gl => "gl",
            Backend::Cpu => "cpu",
        }
    }

    /// Parse a [`BACKEND_ENV`] value. Case-insensitive; `opengl` is accepted for `gl`.
    pub fn from_name(name: &str) -> Option<Backend> {
        match name.trim().to_ascii_lowercase().as_str() {
            "vulkan" => Some(Backend::Vulkan),
            "gl" | "opengl" => Some(Backend::Gl),
            "cpu" | "softbuffer" => Some(Backend::Cpu),
            _ => None,
        }
    }

    /// Whether this backend is compiled into the build (`vulkan` / `gl` cargo features).
    /// `Cpu` always is.
    pub fn is_compiled(self) -> bool {
        match self {
            Backend::Vulkan => cfg!(feature = "vulkan"),
            Backend::Gl => cfg!(feature = "gl"),
            Backend::Cpu => true,
        }
    }

    /// The backends compiled into this build, most capable first.
    pub fn compiled() -> Vec<Backend> {
        Self::ALL.into_iter().filter(|b| b.is_compiled()).collect()
    }

    /// The backend [`BACKEND_ENV`] asks for, if it is set to a backend name.
    ///
    /// Read once per process: a window that silently drew through a different backend than its
    /// sibling would be worse than any build-time choice. An unrecognised value is reported and
    /// ignored rather than fatal, so a typo degrades to automatic selection.
    pub fn requested() -> Option<Backend> {
        static REQUESTED: OnceLock<Option<Backend>> = OnceLock::new();
        *REQUESTED.get_or_init(|| {
            let Ok(value) = std::env::var(BACKEND_ENV) else {
                return None;
            };
            if value.trim().is_empty() {
                return None;
            }
            match Backend::from_name(&value) {
                Some(backend) => Some(backend),
                None => {
                    let names = Backend::ALL.map(Backend::name);
                    eprintln!(
                        "[skiwin] {BACKEND_ENV}={value} is not a backend {names:?}; \
                         choosing one automatically instead"
                    );
                    None
                }
            }
        })
    }
}

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

impl SkiaWindow {
    /// Open `window` on the best backend this build has, falling back when one cannot initialize.
    ///
    /// Without [`BACKEND_ENV`] the backends are tried most-capable-first and the first one that
    /// opens the window wins; a backend that fails is reported once per process (a laptop without a
    /// Vulkan ICD should still run, but it should not run *quietly* slower than it has to).
    ///
    /// With [`BACKEND_ENV`] set, only that backend is tried — a failure is fatal, with the backend's
    /// own error, because a pinned backend that silently falls back would hide what the pin was
    /// there to test. Panics when nothing can open the window; there is nothing left to draw with.
    pub fn new(event_loop: &dyn ActiveEventLoop, window: Arc<Box<dyn Window>>) -> SkiaWindow {
        if let Some(backend) = Backend::requested() {
            return match Self::try_new(backend, event_loop, window) {
                Ok(skia_window) => skia_window,
                Err(e) => panic!(
                    "{BACKEND_ENV}={} was requested, but this window could not be opened on it: {e}",
                    backend.name()
                ),
            };
        }

        let mut failures: Vec<(&'static str, SkiwinError)> = Vec::new();
        for backend in Backend::compiled() {
            match Self::try_new(backend, event_loop, window.clone()) {
                Ok(skia_window) => {
                    report_failures(&failures);
                    return skia_window;
                }
                Err(e) => failures.push((backend.name(), e)),
            }
        }
        panic!("no rendering backend could open a window: {failures:?}");
    }

    /// Try one specific backend. `Cpu` cannot fail except when softbuffer itself refuses the window.
    fn try_new(
        backend: Backend,
        // Only the GPU backends need it; a CPU-only build would otherwise warn about the parameter.
        #[allow(unused_variables)] event_loop: &dyn ActiveEventLoop,
        window: Arc<Box<dyn Window>>,
    ) -> SkiwinResult<SkiaWindow> {
        match backend {
            Backend::Cpu => Ok(cpu::SoftSkiaWindow::new(window)?.into()),
            #[cfg(feature = "gl")]
            Backend::Gl => Ok(gl::GlSkiaWindow::new(event_loop, window)?.into()),
            #[cfg(not(feature = "gl"))]
            Backend::Gl => Err(SkiwinError::UnsupportedBackend(backend.name())),
            #[cfg(feature = "vulkan")]
            Backend::Vulkan => Ok(vulkan::VulkanSkiaWindow::new(event_loop, window)?.into()),
            #[cfg(not(feature = "vulkan"))]
            Backend::Vulkan => Err(SkiwinError::UnsupportedBackend(backend.name())),
        }
    }

    /// The backend this window actually opened on.
    pub fn backend(&self) -> Backend {
        match self {
            SkiaWindow::Cpu(_) => Backend::Cpu,
            #[cfg(feature = "gl")]
            SkiaWindow::Gl(_) => Backend::Gl,
            #[cfg(feature = "vulkan")]
            SkiaWindow::Vulkan(_) => Backend::Vulkan,
        }
    }
}

/// Announce the backends that failed on the way to the one that worked — once per backend per
/// process, so a multi-window app says it once instead of once per window.
fn report_failures(failures: &[(&'static str, SkiwinError)]) {
    static REPORTED: [AtomicBool; 3] = [AtomicBool::new(false), AtomicBool::new(false), AtomicBool::new(false)];
    for (name, error) in failures {
        let Some(index) = Backend::ALL.iter().position(|b| b.name() == *name) else {
            continue;
        };
        if REPORTED[index].swap(true, Ordering::Relaxed) {
            continue;
        }
        log::warn!("[skiwin] the {name} backend is unavailable: {error}");
        // No logger is installed by default, and a user whose app fell back to software rendering
        // needs to hear about it from somewhere.
        eprintln!("[skiwin] {name} backend unavailable ({error}); falling back");
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_names_round_trip_and_aliases_resolve() {
        for backend in Backend::ALL {
            assert_eq!(Backend::from_name(backend.name()), Some(backend));
            assert_eq!(
                Backend::from_name(&backend.name().to_ascii_uppercase()),
                Some(backend)
            );
        }
        assert_eq!(Backend::from_name("  opengl "), Some(Backend::Gl));
        assert_eq!(Backend::from_name("softbuffer"), Some(Backend::Cpu));
        assert_eq!(Backend::from_name("banana"), None);
    }

    #[test]
    fn the_fallback_chain_keeps_its_order_and_always_ends_in_software() {
        let compiled = Backend::compiled();
        // A filter of ALL keeps the capability order that `SkiaWindow::new` walks.
        let expected: Vec<Backend> = Backend::ALL.into_iter().filter(|b| b.is_compiled()).collect();
        assert_eq!(compiled, expected);
        // The CPU path is what makes "this machine has no GPU for us" survivable, so it is not
        // feature-gated and it is the last resort, never the first choice.
        assert!(compiled.contains(&Backend::Cpu));
        assert_eq!(*compiled.last().unwrap(), Backend::Cpu);
        assert_ne!(Backend::ALL[0], Backend::Cpu);
    }
}

