//! Skia Graphite rendering into a wgpu swapchain texture.
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::convert::AsRef;
use std::future::Future;
use std::ops::Deref;
use std::pin::pin;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, LazyLock, Mutex};
use std::task::{Context, Poll, Waker};

use skia_safe::gpu::graphite;
use skia_safe::wgpu;
use skia_safe::{ColorType, Image, Surface, image::RequiredProperties};
use winit::event_loop::ActiveEventLoop;
use winit::raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};
use winit::window::Window;

use super::capture;

/// Own the winit window so wgpu can hold a `'static` surface handle.
struct OwnedWindow(Arc<Box<dyn Window>>);

impl HasWindowHandle for OwnedWindow {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        self.0.window_handle()
    }
}

impl HasDisplayHandle for OwnedWindow {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        self.0.display_handle()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum RendererInitState {
    #[default]
    Idle,
    Pending,
    Ready,
    Failed,
}

impl RendererInitState {
    fn start_async(&mut self) -> bool {
        if *self != Self::Idle {
            return false;
        }
        *self = Self::Pending;
        true
    }

    fn finish(&mut self, success: bool) {
        *self = if success { Self::Ready } else { Self::Failed };
    }

    fn reset(&mut self) {
        *self = Self::Idle;
    }
}

pub(crate) struct WgpuWindow {
    window: Arc<Box<dyn Window>>,
    renderer: Option<WgpuRenderer>,
    pending_renderer: Option<Receiver<Result<GpuSurface, String>>>,
    transparent: bool,
    async_renderer: bool,
    init_state: RendererInitState,
}

impl WgpuWindow {
    pub(crate) fn new(
        _event_loop: &dyn ActiveEventLoop,
        window: Arc<Box<dyn Window>>,
        transparent: bool,
    ) -> Self {
        Self::new_inner(window, transparent, false)
    }

    pub(crate) fn new_async(
        _event_loop: &dyn ActiveEventLoop,
        window: Arc<Box<dyn Window>>,
        transparent: bool,
    ) -> Self {
        Self::new_inner(window, transparent, true)
    }

    fn new_inner(window: Arc<Box<dyn Window>>, transparent: bool, async_renderer: bool) -> Self {
        let mut window = Self {
            window,
            renderer: None,
            pending_renderer: None,
            transparent,
            async_renderer,
            init_state: RendererInitState::Idle,
        };
        if async_renderer {
            window.start_renderer_task();
        }
        window
    }

    fn start_renderer_task(&mut self) {
        if !self.init_state.start_async() {
            return;
        }
        let surface = match GPU_INSTANCE.create_surface(OwnedWindow(self.window.clone())) {
            Ok(surface) => surface,
            Err(error) => {
                self.init_state.finish(false);
                log::error!("[wgpu] could not create the surface: {error}");
                return;
            }
        };
        let size = self.window.surface_size();
        let transparent = self.transparent;
        let redraw_window = self.window.clone();
        let (sender, receiver) = mpsc::channel();
        match std::thread::Builder::new()
            .name("winia-wgpu-surface".into())
            .spawn(move || {
                let _ = sender.send(GpuSurface::from_surface(surface, size, transparent));
                redraw_window.request_redraw();
            }) {
            Ok(_) => self.pending_renderer = Some(receiver),
            Err(error) => {
                self.init_state.finish(false);
                log::error!("[wgpu] could not start surface initialization: {error}");
            }
        }
    }

    fn ensure_renderer(&mut self) -> bool {
        if self.renderer.is_some() {
            return true;
        }
        if self.init_state == RendererInitState::Failed {
            return false;
        }
        let pending = self.pending_renderer.as_ref().map(|rx| rx.try_recv());
        match pending {
            Some(Ok(Ok(surface))) => {
                self.pending_renderer = None;
                match WgpuRenderer::from_surface(surface) {
                    Ok(mut renderer) => {
                        let size = self.window.surface_size();
                        renderer.resize(size.width.max(1), size.height.max(1));
                        self.renderer = Some(renderer);
                        self.init_state.finish(true);
                    }
                    Err(error) => {
                        self.init_state.finish(false);
                        log::error!("[wgpu] could not create the renderer: {error}");
                        return false;
                    }
                }
            }
            Some(Ok(Err(error))) => {
                self.pending_renderer = None;
                self.init_state.finish(false);
                log::error!("[wgpu] could not create the surface: {error}");
                return false;
            }
            Some(Err(TryRecvError::Empty)) => {
                self.window.request_redraw();
                return false;
            }
            Some(Err(TryRecvError::Disconnected)) => {
                self.pending_renderer = None;
                self.init_state.finish(false);
                log::error!("[wgpu] surface initialization thread disconnected");
                return false;
            }
            None => {
                if self.async_renderer {
                    self.start_renderer_task();
                    self.window.request_redraw();
                    return false;
                }
                match WgpuRenderer::new(self.window.clone(), self.transparent) {
                    Ok(renderer) => {
                        self.renderer = Some(renderer);
                        self.init_state.finish(true);
                    }
                    Err(error) => {
                        self.init_state.finish(false);
                        log::error!("[wgpu] could not create the renderer: {error}");
                        return false;
                    }
                }
            }
        }
        true
    }

    pub(crate) fn window(&self) -> &dyn Window {
        &**self.window
    }

    pub(crate) fn destroy_surface(&mut self) {
        self.renderer = None;
        self.pending_renderer = None;
        self.init_state.reset();
    }

    pub(crate) fn recreate_surface(&mut self, _event_loop: &dyn ActiveEventLoop) {
        self.renderer = None;
        self.pending_renderer = None;
        self.init_state.reset();
        if self.async_renderer {
            self.start_renderer_task();
        } else {
            let _ = self.ensure_renderer();
        }
    }

    pub(crate) fn resize(&mut self) {
        if let Some(renderer) = self.renderer.as_mut() {
            let size = self.window.surface_size();
            renderer.resize(size.width, size.height);
        }
    }

    pub(crate) fn draw(&mut self, draw_fn: impl FnOnce(&mut Surface)) -> bool {
        if !self.ensure_renderer() {
            return false;
        }
        let Some(renderer) = self.renderer.as_mut() else {
            log::error!("[wgpu] draw without a surface; dropping the frame");
            return false;
        };
        let frame = match renderer.gpu.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                renderer.configure();
                return false;
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return false;
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                log::error!("[wgpu] acquiring the frame failed validation");
                return false;
            }
        };
        let Some(color_type) = color_type_for_format(renderer.gpu.config.format) else {
            log::error!(
                "[wgpu] unsupported surface format {:?}",
                renderer.gpu.config.format
            );
            return false;
        };
        let want_capture = capture::take_capture_request();
        let mut surface = graphite::wgpu::surfaces::wrap_texture(
            &mut renderer.recorder,
            &frame.texture,
            color_type,
            None,
            None,
        )
        .unwrap_or_else(|| panic!("could not wrap the wgpu swapchain texture for Skia"));

        draw_fn(&mut surface);

        let mut recording = renderer
            .recorder
            .snap()
            .expect("could not snap the Graphite recording");
        let status = renderer
            .context
            .insert_recording(&graphite::InsertRecordingInfo::new(&mut recording));
        if status != graphite::InsertStatus::Success {
            log::error!("[wgpu] Graphite insert_recording failed: {status:?}");
            return false;
        }
        if !renderer.context.submit(None) {
            log::error!("[wgpu] Graphite submit failed");
            return false;
        }
        if want_capture {
            capture::capture_surface(&mut renderer.context, &mut surface);
        }
        drop(surface);
        drop(recording);
        self.window.pre_present_notify();
        renderer.gpu.queue.present(frame);
        true
    }
}

impl Deref for WgpuWindow {
    type Target = dyn Window;

    fn deref(&self) -> &Self::Target {
        self.window.as_ref().as_ref()
    }
}

impl AsRef<dyn Window> for WgpuWindow {
    fn as_ref(&self) -> &dyn Window {
        self.window.as_ref().as_ref()
    }
}

static GPU_INSTANCE: LazyLock<wgpu::Instance> = LazyLock::new(wgpu::Instance::default);

#[derive(Clone)]
struct GpuAdapter {
    adapter: wgpu::Adapter,
}

static GPU_ADAPTER: Mutex<Option<GpuAdapter>> = Mutex::new(None);

/// Reuse the process-wide instance and adapter across surfaces. Each window still owns its device,
/// swapchain, Graphite context and recorder: Skia Graphite contexts on one shared device invalidate
/// each other's resources when one window is closed.
fn shared_gpu_adapter(
    surface: &wgpu::Surface<'_>,
) -> Result<(GpuAdapter, wgpu::SurfaceCapabilities), String> {
    if let Some(gpu) = GPU_ADAPTER
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
    {
        let capabilities = surface.get_capabilities(&gpu.adapter);
        if capabilities
            .formats
            .iter()
            .any(|format| color_type_for_format(*format).is_some())
        {
            return Ok((gpu, capabilities));
        }
    }

    let adapter = block_on(GPU_INSTANCE.request_adapter(&wgpu::RequestAdapterOptions {
        compatible_surface: Some(surface),
        ..Default::default()
    }))
    .map_err(|error| format!("request_adapter: {error}"))?;
    let gpu = GpuAdapter { adapter };
    let capabilities = surface.get_capabilities(&gpu.adapter);
    *GPU_ADAPTER
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = Some(gpu.clone());
    Ok((gpu, capabilities))
}

struct GpuSurface {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    device: wgpu::Device,
    queue: wgpu::Queue,
}

impl GpuSurface {
    fn new(window: Arc<Box<dyn Window>>, transparent: bool) -> Result<Self, String> {
        let surface = GPU_INSTANCE
            .create_surface(OwnedWindow(window.clone()))
            .map_err(|error| format!("create_surface: {error}"))?;
        let size = window.surface_size();
        Self::from_surface(surface, size, transparent)
    }

    fn from_surface(
        surface: wgpu::Surface<'static>,
        size: winit::dpi::PhysicalSize<u32>,
        transparent: bool,
    ) -> Result<Self, String> {
        let (gpu, capabilities) = shared_gpu_adapter(&surface)?;
        let adapter = gpu.adapter.clone();
        let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .map_err(|error| format!("request_device: {error}"))?;
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or_else(|| "the surface does not support the adapter".to_string())?;
        if let Some(format) = capabilities
            .formats
            .iter()
            .find(|format| !format.is_srgb() && color_type_for_format(**format).is_some())
        {
            config.format = *format;
        }
        config.usage = wgpu::TextureUsages::RENDER_ATTACHMENT;
        if capabilities.usages.contains(wgpu::TextureUsages::COPY_SRC) {
            config.usage |= wgpu::TextureUsages::COPY_SRC;
        } else {
            log::warn!("[wgpu] the surface cannot copy pixels for debug capture");
        }
        config.alpha_mode = if transparent {
            if capabilities
                .alpha_modes
                .contains(&wgpu::CompositeAlphaMode::PreMultiplied)
            {
                wgpu::CompositeAlphaMode::PreMultiplied
            } else if capabilities
                .alpha_modes
                .contains(&wgpu::CompositeAlphaMode::PostMultiplied)
            {
                wgpu::CompositeAlphaMode::PostMultiplied
            } else {
                log::warn!(
                    "[wgpu] transparent window requested, but the surface only offers {:?}",
                    capabilities.alpha_modes
                );
                wgpu::CompositeAlphaMode::Opaque
            }
        } else if capabilities
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::Opaque)
        {
            wgpu::CompositeAlphaMode::Opaque
        } else {
            config.alpha_mode
        };
        surface.configure(&device, &config);
        Ok(Self {
            surface,
            config,
            device,
            queue,
        })
    }

    fn configure(&self) {
        self.surface.configure(&self.device, &self.config);
    }

    fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.configure();
    }
}

struct WgpuRenderer {
    // Fields drop in declaration order: recorder, then context, then the wgpu device and queue.
    recorder: graphite::Recorder,
    context: graphite::Context,
    gpu: GpuSurface,
}

impl WgpuRenderer {
    fn new(window: Arc<Box<dyn Window>>, transparent: bool) -> Result<Self, String> {
        Self::from_surface(GpuSurface::new(window, transparent)?)
    }

    fn from_surface(gpu: GpuSurface) -> Result<Self, String> {
        let mut context = graphite::wgpu::context_factory::make_wgpu(&gpu.device, &gpu.queue, None)
            .ok_or_else(|| "Skia rejected the wgpu device".to_string())?;
        let mut options = graphite::RecorderOptions::new();
        options.set_image_provider(CachingImageProvider::default());
        let recorder = context
            .make_recorder(Some(&options))
            .ok_or_else(|| "could not create a Graphite recorder".to_string())?;
        Ok(Self {
            recorder,
            context,
            gpu,
        })
    }

    fn configure(&self) {
        self.gpu.configure();
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.gpu.resize(width, height);
    }
}

const MAX_CACHED_IMAGES: usize = 256;

#[derive(Default)]
struct CachingImageProvider {
    cache: RefCell<HashMap<(u32, bool), Image>>,
    order: RefCell<VecDeque<(u32, bool)>>,
}

impl graphite::ImageProvider for CachingImageProvider {
    fn find_or_create(
        &self,
        recorder: &graphite::BorrowedRecorder,
        image: &Image,
        required: RequiredProperties,
    ) -> Option<Image> {
        let key = (image.unique_id(), required.mipmapped);
        if let Some(cached) = self.cache.borrow().get(&key).cloned() {
            if let Some(index) = self
                .order
                .borrow_mut()
                .iter()
                .position(|entry| *entry == key)
            {
                self.order.borrow_mut().remove(index);
            }
            self.order.borrow_mut().push_back(key);
            return Some(cached);
        }

        // Uploading a picture-backed image can re-enter this provider, so never hold a cache borrow
        // across the upload. The returned texture preserves the source image's required properties.
        let uploaded = recorder.texture_from_image(image, required)?;
        self.cache.borrow_mut().insert(key, uploaded.clone());
        self.order.borrow_mut().push_back(key);
        loop {
            let len = self.order.borrow().len();
            if len <= MAX_CACHED_IMAGES {
                break;
            }
            let oldest = self.order.borrow_mut().pop_front();
            if let Some(oldest) = oldest {
                self.cache.borrow_mut().remove(&oldest);
            }
        }
        Some(uploaded)
    }
}

fn color_type_for_format(format: wgpu::TextureFormat) -> Option<ColorType> {
    match format {
        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb => {
            Some(ColorType::BGRA8888)
        }
        wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Rgba8UnormSrgb => {
            Some(ColorType::RGBA8888)
        }
        _ => None,
    }
}

fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swapchain_formats_map_to_skia_color_types() {
        assert_eq!(
            color_type_for_format(wgpu::TextureFormat::Bgra8Unorm),
            Some(ColorType::BGRA8888)
        );
        assert_eq!(
            color_type_for_format(wgpu::TextureFormat::Rgba8UnormSrgb),
            Some(ColorType::RGBA8888)
        );
        assert_eq!(
            color_type_for_format(wgpu::TextureFormat::Depth32Float),
            None
        );
    }

    #[test]
    fn async_renderer_failure_is_terminal_until_surface_recreation() {
        let mut state = RendererInitState::Idle;
        assert!(state.start_async());
        assert_eq!(state, RendererInitState::Pending);
        assert!(
            !state.start_async(),
            "a pending task must not be duplicated"
        );

        state.finish(false);
        assert_eq!(state, RendererInitState::Failed);
        assert!(
            !state.start_async(),
            "a failed async initialization must not retry synchronously"
        );

        state.reset();
        assert_eq!(state, RendererInitState::Idle);
        assert!(
            state.start_async(),
            "surface recreation starts a fresh task"
        );
        state.finish(true);
        assert_eq!(state, RendererInitState::Ready);
    }
}
