use crate::GpuRenderer;
use ash::vk::Handle;
use skia_safe::gpu::vk::GetProcOf;
use skia_safe::gpu::DirectContext;
use std::sync::Arc;
use thiserror::Error;
use vulkano::device::physical::PhysicalDeviceType;
use vulkano::device::{
    Device, DeviceCreateInfo, DeviceExtensions, Queue, QueueCreateInfo, QueueFlags,
};
use vulkano::format::Format;
use vulkano::image::{Image, ImageUsage};
use vulkano::instance::{Instance, InstanceCreateFlags, InstanceCreateInfo};
use vulkano::swapchain::{
    acquire_next_image, Surface, Swapchain, SwapchainAcquireFuture, SwapchainCreateInfo,
    SwapchainPresentInfo,
};
use vulkano::sync::GpuFuture;
use vulkano::{sync, Validated, VulkanError, VulkanLibrary, VulkanObject};
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::raw_window_handle::{HandleError, HasDisplayHandle};
use winit::window::{Window, WindowId};

struct SkiaRenderer {
    surface: Vec<skia_safe::Surface>,
    context: DirectContext,
}

impl Drop for SkiaRenderer {
    fn drop(&mut self) {
        self.context.abandon();
    }
}

struct RenderContext {
    previous_frame_end: Option<Box<dyn GpuFuture>>,
    images: Vec<Arc<Image>>,
    swapchain: Arc<Swapchain>,
    window: Arc<Window>,
    recreate_swapchain: bool,
}

pub struct VkRenderer {
    image_index: Option<u32>,
    acquire_future: Option<SwapchainAcquireFuture>,
    skia_renderer: Option<SkiaRenderer>,
    rcx: Option<RenderContext>,
    queue: Arc<Queue>,
    device: Arc<Device>,
    instance: Arc<Instance>,
    library: Arc<VulkanLibrary>,
}

#[derive(Error, Debug)]
pub enum VkRendererError {
    #[error("Failed to create Vulkan library: {0}")]
    Library(#[from] vulkano::LoadingError),
    #[error("Failed to get required extensions: {0}")]
    Extensions(#[from] HandleError),
    #[error("Failed to create Vulkan instance: {0}")]
    Instance(#[from] Validated<VulkanError>),
    #[error("Failed to enumerate physical devices: {0}")]
    EnumerateDevices(#[from] VulkanError),
    #[error("No suitable physical device found")]
    NoDevice,
    #[error("Failed to create logical device: {0}")]
    Device(Validated<VulkanError>),
    #[error("Failed to get next queue")]
    Queue,
}

#[derive(Error, Debug)]
pub enum CreateSwapchainError {
    #[error("Failed to create swapchain: {0}")]
    Swapchain(#[from] Validated<VulkanError>),
    #[error("Failed to create Skia Surface")]
    Skia,
    #[error("No render context available")]
    NoRenderContext,
}

impl VkRenderer {
    pub fn new(display_handle: &impl HasDisplayHandle) -> Result<Self, VkRendererError> {
        let library = VulkanLibrary::new()?;
        let required_extensions = Surface::required_extensions(display_handle)?;
        let instance = Instance::new(
            library.clone(),
            InstanceCreateInfo {
                flags: InstanceCreateFlags::ENUMERATE_PORTABILITY,
                enabled_extensions: required_extensions,
                ..Default::default()
            },
        )?;

        let device_extensions = DeviceExtensions {
            khr_swapchain: true,
            ..DeviceExtensions::empty()
        };

        let (physical_device, queue_family_index) = instance
            .enumerate_physical_devices()?
            .filter(|p| p.supported_extensions().contains(&device_extensions))
            .filter_map(|p| {
                p.queue_family_properties()
                    .iter()
                    .enumerate()
                    .position(|(i, q)| {
                        q.queue_flags.intersects(QueueFlags::GRAPHICS)
                            && p.presentation_support(i as u32, display_handle).unwrap()
                    })
                    .map(|i| (p, i as u32))
            })
            .min_by_key(|(p, _)| match p.properties().device_type {
                PhysicalDeviceType::DiscreteGpu => 0,
                PhysicalDeviceType::IntegratedGpu => 1,
                PhysicalDeviceType::VirtualGpu => 2,
                PhysicalDeviceType::Cpu => 3,
                PhysicalDeviceType::Other => 4,
                _ => 5,
            })
            .ok_or(VkRendererError::NoDevice)?;

        let (device, mut queues) = Device::new(
            physical_device.clone(),
            DeviceCreateInfo {
                enabled_extensions: device_extensions,
                queue_create_infos: vec![QueueCreateInfo {
                    queue_family_index,
                    ..Default::default()
                }],
                ..Default::default()
            },
        )
        .map_err(VkRendererError::Device)?;

        let queue = queues.next().ok_or(VkRendererError::Queue)?;

        Ok(VkRenderer {
            image_index: None,
            acquire_future: None,
            library,
            instance,
            device,
            queue,
            rcx: None,
            skia_renderer: None,
        })
    }

    fn recreate_surface(&mut self) -> Result<(), CreateSwapchainError> {
        match (&mut self.skia_renderer, &mut self.rcx) {
            (Some(skia_renderer), Some(rcx)) => {
                rcx.previous_frame_end
                    .as_mut()
                    .map(|it| it.cleanup_finished());
                skia_renderer.surface.clear();
                for image in rcx.images.iter() {
                    let image_info = unsafe {
                        skia_safe::gpu::vk::ImageInfo::new(
                            image.handle().as_raw() as _,
                            skia_safe::gpu::vk::Alloc::default(),
                            skia_safe::gpu::vk::ImageTiling::OPTIMAL,
                            skia_safe::gpu::vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                            from_i32(image.format() as i32).unwrap(),
                            1,
                            self.queue.queue_family_index(),
                            skia_safe::gpu::vk::YcbcrConversionInfo::default(),
                            skia_bindings::GrProtected::No,
                            skia_bindings::VkSharingMode::EXCLUSIVE,
                        )
                    };
                    let backend_render_target = skia_safe::gpu::backend_render_targets::make_vk(
                        rcx.window.inner_size().into(),
                        &image_info,
                    );
                    let surface = skia_safe::gpu::surfaces::wrap_backend_render_target(
                        &mut skia_renderer.context,
                        &backend_render_target,
                        skia_safe::gpu::SurfaceOrigin::TopLeft,
                        skia_safe::ColorType::RGBA8888,
                        skia_safe::ColorSpace::new_srgb(),
                        None,
                    )
                    .ok_or(CreateSwapchainError::Skia)?;
                    skia_renderer.surface.push(surface);
                }
                Result::Ok(())
            }
            _ => Err(CreateSwapchainError::NoRenderContext),
        }
    }

    fn recreate_swapchain(&mut self) -> Result<(), CreateSwapchainError> {
        let rcx = self.rcx.as_mut().unwrap();
        if rcx.recreate_swapchain {
            let create_info = rcx.swapchain.create_info();
            let (new_swapchain, new_images) = rcx.swapchain.recreate(SwapchainCreateInfo {
                image_extent: rcx.window.inner_size().into(),
                ..create_info
            })?;

            rcx.swapchain = new_swapchain.clone();
            rcx.recreate_swapchain = false;
            rcx.images = new_images;
            self.recreate_surface()?;
        }
        Ok(())
    }
}

impl VkRenderer {
    pub fn resumed(&mut self, window: &Arc<Window>) {
        let surface = Surface::from_window(self.instance.clone(), window.clone()).unwrap();

        let (swapchain, images) = {
            let surface_capabilities = self
                .device
                .physical_device()
                .surface_capabilities(&surface, Default::default())
                .unwrap();
            let (image_format, _) = self
                .device
                .physical_device()
                .surface_formats(&surface, Default::default())
                .unwrap()
                .iter()
                .find(|(format, color_space)| {
                    *format == Format::R8G8B8A8_UNORM
                        && *color_space == vulkano::swapchain::ColorSpace::SrgbNonLinear
                })
                .unwrap_or_else(|| {
                    panic!("No suitable surface format found for the swapchain");
                })
                .clone();

            Swapchain::new(
                self.device.clone(),
                surface.clone(),
                SwapchainCreateInfo {
                    min_image_count: surface_capabilities.min_image_count.max(2),
                    image_format,
                    image_extent: window.inner_size().into(),
                    image_usage: ImageUsage::COLOR_ATTACHMENT,
                    composite_alpha: surface_capabilities
                        .supported_composite_alpha
                        .into_iter()
                        .next()
                        .unwrap(),
                    ..Default::default()
                },
            )
            .unwrap()
        };

        let previous_frame_end = Some(sync::now(self.device.clone()).boxed());

        self.rcx = Some(RenderContext {
            window: window.clone(),
            swapchain,
            recreate_swapchain: false,
            previous_frame_end,
            images,
        });

        {
            let get_proc = |get_proc_of: GetProcOf| {
                let addr = match get_proc_of {
                    GetProcOf::Instance(instance, name) => unsafe {
                        self.library
                            .get_instance_proc_addr(
                                ash::vk::Instance::from_raw(self.instance.handle().as_raw() as _),
                                name,
                            )
                            .unwrap_or_else(|| {
                                self.library
                                    .get_instance_proc_addr(
                                        ash::vk::Instance::from_raw(instance as _),
                                        name,
                                    )
                                    .expect("Failed to get instance proc address")
                            })
                    },
                    GetProcOf::Device(device, name) => unsafe {
                        let ash_instance = ash::Instance::from_parts_1_3(
                            ash::vk::Instance::from_raw(self.instance.handle().as_raw()),
                            self.instance.fns().v1_0.clone(),
                            self.instance.fns().v1_1.clone(),
                            self.instance.fns().v1_3.clone(),
                        );
                        ash_instance
                            .get_device_proc_addr(
                                ash::vk::Device::from_raw(self.device.handle().as_raw() as _),
                                name,
                            )
                            .unwrap_or_else(|| {
                                ash_instance
                                    .get_device_proc_addr(
                                        ash::vk::Device::from_raw(device as _),
                                        name,
                                    )
                                    .expect("Failed to get device proc address")
                            })
                    },
                };
                addr as _
            };
            let backend_context = unsafe {
                skia_safe::gpu::vk::BackendContext::new(
                    self.instance.handle().as_raw() as _,
                    self.device.physical_device().handle().as_raw() as _,
                    self.device.handle().as_raw() as _,
                    (
                        self.queue.handle().as_raw() as _,
                        self.queue.queue_family_index() as usize,
                    ),
                    &get_proc,
                )
            };

            let context = skia_safe::gpu::direct_contexts::make_vulkan(&backend_context, None)
                .expect("Failed to create Skia Vulkan context");
            let skia_renderer = SkiaRenderer {
                context,
                surface: Vec::new(),
            };
            self.skia_renderer = Some(skia_renderer);
        }
        self.recreate_surface().unwrap();
    }

    pub fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let rcx = self.rcx.as_mut().unwrap();
        match event {
            WindowEvent::Resized(_) => {
                rcx.recreate_swapchain = true;
            }
            _ => {}
        }
/*        if rcx.recreate_swapchain {
            self.recreate_swapchain()
                .map_err(|e| {
                    eprintln!("Failed to recreate swapchain: {e}");
                })
                .ok();
        }*/
    }

    pub fn recreate(&mut self) {
        let recreate_swapchain = self.rcx.as_mut().unwrap().recreate_swapchain;
        if recreate_swapchain {
            self.recreate_swapchain()
                .map_err(|e| {
                    eprintln!("Failed to recreate swapchain: {e}");
                })
                .ok();
        }
        let rcx = self.rcx.as_mut().unwrap();
        rcx.recreate_swapchain = false;
    }

    pub fn draw(&mut self, mut func: impl FnMut(&mut skia_safe::Surface)) {
        let rcx = self.rcx.as_mut().unwrap();
        let window_size = rcx.window.inner_size();

        if window_size.width == 0 || window_size.height == 0 {
            return;
        }

        rcx.previous_frame_end.as_mut().unwrap().cleanup_finished();

        let (image_index, suboptimal, acquire_future) = match acquire_next_image(
            rcx.swapchain.clone(),
            None,
        )
            .map_err(Validated::unwrap)
        {
            Ok(r) => r,
            Err(VulkanError::OutOfDate) => {
                rcx.recreate_swapchain = true;
                return;
            }
            Err(e) => panic!("failed to acquire next image: {e}"),
        };

        if suboptimal {
            rcx.recreate_swapchain = true;
        }

        if let Some(skia_renderer) = &mut self.skia_renderer {
            if let Some(surface) = skia_renderer.surface.get_mut(image_index as usize) {
                func(surface);
                skia_renderer.context.flush_and_submit();
            }
        }

        let future = rcx
            .previous_frame_end
            .take()
            .unwrap()
            .join(acquire_future)
            .then_swapchain_present(
                self.queue.clone(),
                SwapchainPresentInfo::swapchain_image_index(rcx.swapchain.clone(), image_index),
            )
            .then_signal_fence_and_flush();

        match future.map_err(Validated::unwrap) {
            Ok(future) => {
                rcx.previous_frame_end = Some(future.boxed());
            }
            Err(VulkanError::OutOfDate) => {
                rcx.recreate_swapchain = true;
                rcx.previous_frame_end = Some(sync::now(self.device.clone()).boxed());
            }
            Err(_e) => {
                rcx.previous_frame_end = Some(sync::now(self.device.clone()).boxed());
            }
        }
    }
}

impl GpuRenderer for VkRenderer {
    fn surface(&mut self) -> Option<&mut skia_safe::Surface> {
        let rcx = self.rcx.as_mut().unwrap();
        let window_size = rcx.window.inner_size();

        if window_size.width == 0 || window_size.height == 0 {
            return None;
        }

        rcx.previous_frame_end.as_mut().unwrap().cleanup_finished();

        let (image_index, suboptimal, acquire_future) =
            match acquire_next_image(rcx.swapchain.clone(), None).map_err(Validated::unwrap) {
                Ok(r) => r,
                Err(VulkanError::OutOfDate) => {
                    rcx.recreate_swapchain = true;
                    return None;
                }
                Err(e) => panic!("failed to acquire next image: {e}"),
            };

        if suboptimal {
            rcx.recreate_swapchain = true;
        }

        if let Some(skia_renderer) = &mut self.skia_renderer {
            if let Some(surface) = skia_renderer.surface.get_mut(image_index as usize) {
                self.image_index = Some(image_index);
                self.acquire_future = Some(acquire_future);
                return Some(surface);
            }
        }
        None
    }

    fn submit(&mut self) {
        if let Some(renderer) = &mut self.skia_renderer {
            renderer.context.flush_and_submit();
        };
        if let (Some(image_index), Some(acquire_future)) =
            (self.image_index.take(), self.acquire_future.take())
        {
            let rcx = self.rcx.as_mut().unwrap();
            let future = rcx
                .previous_frame_end
                .take()
                .unwrap()
                .join(acquire_future)
                .then_swapchain_present(
                    self.queue.clone(),
                    SwapchainPresentInfo::swapchain_image_index(rcx.swapchain.clone(), image_index),
                )
                .then_signal_fence_and_flush();

            match future.map_err(Validated::unwrap) {
                Ok(future) => {
                    rcx.previous_frame_end = Some(future.boxed());
                }
                Err(VulkanError::OutOfDate) => {
                    rcx.recreate_swapchain = true;
                    rcx.previous_frame_end = Some(sync::now(self.device.clone()).boxed());
                }
                Err(_e) => {
                    rcx.previous_frame_end = Some(sync::now(self.device.clone()).boxed());
                }
            }
        }
    }
}

pub fn from_i32(value: i32) -> Option<skia_safe::gpu::vk::Format> {
    match value {
        0 => Some(skia_safe::gpu::vk::Format::UNDEFINED),
        1 => Some(skia_safe::gpu::vk::Format::R4G4_UNORM_PACK8),
        2 => Some(skia_safe::gpu::vk::Format::R4G4B4A4_UNORM_PACK16),
        3 => Some(skia_safe::gpu::vk::Format::B4G4R4A4_UNORM_PACK16),
        4 => Some(skia_safe::gpu::vk::Format::R5G6B5_UNORM_PACK16),
        5 => Some(skia_safe::gpu::vk::Format::B5G6R5_UNORM_PACK16),
        6 => Some(skia_safe::gpu::vk::Format::R5G5B5A1_UNORM_PACK16),
        7 => Some(skia_safe::gpu::vk::Format::B5G5R5A1_UNORM_PACK16),
        8 => Some(skia_safe::gpu::vk::Format::A1R5G5B5_UNORM_PACK16),
        9 => Some(skia_safe::gpu::vk::Format::R8_UNORM),
        10 => Some(skia_safe::gpu::vk::Format::R8_SNORM),
        11 => Some(skia_safe::gpu::vk::Format::R8_USCALED),
        12 => Some(skia_safe::gpu::vk::Format::R8_SSCALED),
        13 => Some(skia_safe::gpu::vk::Format::R8_UINT),
        14 => Some(skia_safe::gpu::vk::Format::R8_SINT),
        15 => Some(skia_safe::gpu::vk::Format::R8_SRGB),
        16 => Some(skia_safe::gpu::vk::Format::R8G8_UNORM),
        17 => Some(skia_safe::gpu::vk::Format::R8G8_SNORM),
        18 => Some(skia_safe::gpu::vk::Format::R8G8_USCALED),
        19 => Some(skia_safe::gpu::vk::Format::R8G8_SSCALED),
        20 => Some(skia_safe::gpu::vk::Format::R8G8_UINT),
        21 => Some(skia_safe::gpu::vk::Format::R8G8_SINT),
        22 => Some(skia_safe::gpu::vk::Format::R8G8_SRGB),
        23 => Some(skia_safe::gpu::vk::Format::R8G8B8_UNORM),
        24 => Some(skia_safe::gpu::vk::Format::R8G8B8_SNORM),
        25 => Some(skia_safe::gpu::vk::Format::R8G8B8_USCALED),
        26 => Some(skia_safe::gpu::vk::Format::R8G8B8_SSCALED),
        27 => Some(skia_safe::gpu::vk::Format::R8G8B8_UINT),
        28 => Some(skia_safe::gpu::vk::Format::R8G8B8_SINT),
        29 => Some(skia_safe::gpu::vk::Format::R8G8B8_SRGB),
        30 => Some(skia_safe::gpu::vk::Format::B8G8R8_UNORM),
        31 => Some(skia_safe::gpu::vk::Format::B8G8R8_SNORM),
        32 => Some(skia_safe::gpu::vk::Format::B8G8R8_USCALED),
        33 => Some(skia_safe::gpu::vk::Format::B8G8R8_SSCALED),
        34 => Some(skia_safe::gpu::vk::Format::B8G8R8_UINT),
        35 => Some(skia_safe::gpu::vk::Format::B8G8R8_SINT),
        36 => Some(skia_safe::gpu::vk::Format::B8G8R8_SRGB),
        37 => Some(skia_safe::gpu::vk::Format::R8G8B8A8_UNORM),
        38 => Some(skia_safe::gpu::vk::Format::R8G8B8A8_SNORM),
        39 => Some(skia_safe::gpu::vk::Format::R8G8B8A8_USCALED),
        40 => Some(skia_safe::gpu::vk::Format::R8G8B8A8_SSCALED),
        41 => Some(skia_safe::gpu::vk::Format::R8G8B8A8_UINT),
        42 => Some(skia_safe::gpu::vk::Format::R8G8B8A8_SINT),
        43 => Some(skia_safe::gpu::vk::Format::R8G8B8A8_SRGB),
        44 => Some(skia_safe::gpu::vk::Format::B8G8R8A8_UNORM),
        45 => Some(skia_safe::gpu::vk::Format::B8G8R8A8_SNORM),
        46 => Some(skia_safe::gpu::vk::Format::B8G8R8A8_USCALED),
        47 => Some(skia_safe::gpu::vk::Format::B8G8R8A8_SSCALED),
        48 => Some(skia_safe::gpu::vk::Format::B8G8R8A8_UINT),
        49 => Some(skia_safe::gpu::vk::Format::B8G8R8A8_SINT),
        50 => Some(skia_safe::gpu::vk::Format::B8G8R8A8_SRGB),
        51 => Some(skia_safe::gpu::vk::Format::A8B8G8R8_UNORM_PACK32),
        52 => Some(skia_safe::gpu::vk::Format::A8B8G8R8_SNORM_PACK32),
        53 => Some(skia_safe::gpu::vk::Format::A8B8G8R8_USCALED_PACK32),
        54 => Some(skia_safe::gpu::vk::Format::A8B8G8R8_SSCALED_PACK32),
        55 => Some(skia_safe::gpu::vk::Format::A8B8G8R8_UINT_PACK32),
        56 => Some(skia_safe::gpu::vk::Format::A8B8G8R8_SINT_PACK32),
        57 => Some(skia_safe::gpu::vk::Format::A8B8G8R8_SRGB_PACK32),
        58 => Some(skia_safe::gpu::vk::Format::A2R10G10B10_UNORM_PACK32),
        59 => Some(skia_safe::gpu::vk::Format::A2R10G10B10_SNORM_PACK32),
        60 => Some(skia_safe::gpu::vk::Format::A2R10G10B10_USCALED_PACK32),
        61 => Some(skia_safe::gpu::vk::Format::A2R10G10B10_SSCALED_PACK32),
        62 => Some(skia_safe::gpu::vk::Format::A2R10G10B10_UINT_PACK32),
        63 => Some(skia_safe::gpu::vk::Format::A2R10G10B10_SINT_PACK32),
        64 => Some(skia_safe::gpu::vk::Format::A2B10G10R10_UNORM_PACK32),
        65 => Some(skia_safe::gpu::vk::Format::A2B10G10R10_SNORM_PACK32),
        66 => Some(skia_safe::gpu::vk::Format::A2B10G10R10_USCALED_PACK32),
        67 => Some(skia_safe::gpu::vk::Format::A2B10G10R10_SSCALED_PACK32),
        68 => Some(skia_safe::gpu::vk::Format::A2B10G10R10_UINT_PACK32),
        69 => Some(skia_safe::gpu::vk::Format::A2B10G10R10_SINT_PACK32),
        70 => Some(skia_safe::gpu::vk::Format::R16_UNORM),
        71 => Some(skia_safe::gpu::vk::Format::R16_SNORM),
        72 => Some(skia_safe::gpu::vk::Format::R16_USCALED),
        73 => Some(skia_safe::gpu::vk::Format::R16_SSCALED),
        74 => Some(skia_safe::gpu::vk::Format::R16_UINT),
        75 => Some(skia_safe::gpu::vk::Format::R16_SINT),
        76 => Some(skia_safe::gpu::vk::Format::R16_SFLOAT),
        77 => Some(skia_safe::gpu::vk::Format::R16G16_UNORM),
        78 => Some(skia_safe::gpu::vk::Format::R16G16_SNORM),
        79 => Some(skia_safe::gpu::vk::Format::R16G16_USCALED),
        80 => Some(skia_safe::gpu::vk::Format::R16G16_SSCALED),
        81 => Some(skia_safe::gpu::vk::Format::R16G16_UINT),
        82 => Some(skia_safe::gpu::vk::Format::R16G16_SINT),
        83 => Some(skia_safe::gpu::vk::Format::R16G16_SFLOAT),
        84 => Some(skia_safe::gpu::vk::Format::R16G16B16_UNORM),
        85 => Some(skia_safe::gpu::vk::Format::R16G16B16_SNORM),
        86 => Some(skia_safe::gpu::vk::Format::R16G16B16_USCALED),
        87 => Some(skia_safe::gpu::vk::Format::R16G16B16_SSCALED),
        88 => Some(skia_safe::gpu::vk::Format::R16G16B16_UINT),
        89 => Some(skia_safe::gpu::vk::Format::R16G16B16_SINT),
        90 => Some(skia_safe::gpu::vk::Format::R16G16B16_SFLOAT),
        91 => Some(skia_safe::gpu::vk::Format::R16G16B16A16_UNORM),
        92 => Some(skia_safe::gpu::vk::Format::R16G16B16A16_SNORM),
        93 => Some(skia_safe::gpu::vk::Format::R16G16B16A16_USCALED),
        94 => Some(skia_safe::gpu::vk::Format::R16G16B16A16_SSCALED),
        95 => Some(skia_safe::gpu::vk::Format::R16G16B16A16_UINT),
        96 => Some(skia_safe::gpu::vk::Format::R16G16B16A16_SINT),
        97 => Some(skia_safe::gpu::vk::Format::R16G16B16A16_SFLOAT),
        98 => Some(skia_safe::gpu::vk::Format::R32_UINT),
        99 => Some(skia_safe::gpu::vk::Format::R32_SINT),
        100 => Some(skia_safe::gpu::vk::Format::R32_SFLOAT),
        101 => Some(skia_safe::gpu::vk::Format::R32G32_UINT),
        102 => Some(skia_safe::gpu::vk::Format::R32G32_SINT),
        103 => Some(skia_safe::gpu::vk::Format::R32G32_SFLOAT),
        104 => Some(skia_safe::gpu::vk::Format::R32G32B32_UINT),
        105 => Some(skia_safe::gpu::vk::Format::R32G32B32_SINT),
        106 => Some(skia_safe::gpu::vk::Format::R32G32B32_SFLOAT),
        107 => Some(skia_safe::gpu::vk::Format::R32G32B32A32_UINT),
        108 => Some(skia_safe::gpu::vk::Format::R32G32B32A32_SINT),
        109 => Some(skia_safe::gpu::vk::Format::R32G32B32A32_SFLOAT),
        110 => Some(skia_safe::gpu::vk::Format::R64_UINT),
        111 => Some(skia_safe::gpu::vk::Format::R64_SINT),
        112 => Some(skia_safe::gpu::vk::Format::R64_SFLOAT),
        113 => Some(skia_safe::gpu::vk::Format::R64G64_UINT),
        114 => Some(skia_safe::gpu::vk::Format::R64G64_SINT),
        115 => Some(skia_safe::gpu::vk::Format::R64G64_SFLOAT),
        116 => Some(skia_safe::gpu::vk::Format::R64G64B64_UINT),
        117 => Some(skia_safe::gpu::vk::Format::R64G64B64_SINT),
        118 => Some(skia_safe::gpu::vk::Format::R64G64B64_SFLOAT),
        119 => Some(skia_safe::gpu::vk::Format::R64G64B64A64_UINT),
        120 => Some(skia_safe::gpu::vk::Format::R64G64B64A64_SINT),
        121 => Some(skia_safe::gpu::vk::Format::R64G64B64A64_SFLOAT),
        122 => Some(skia_safe::gpu::vk::Format::B10G11R11_UFLOAT_PACK32),
        123 => Some(skia_safe::gpu::vk::Format::E5B9G9R9_UFLOAT_PACK32),
        124 => Some(skia_safe::gpu::vk::Format::D16_UNORM),
        125 => Some(skia_safe::gpu::vk::Format::X8_D24_UNORM_PACK32),
        126 => Some(skia_safe::gpu::vk::Format::D32_SFLOAT),
        127 => Some(skia_safe::gpu::vk::Format::S8_UINT),
        128 => Some(skia_safe::gpu::vk::Format::D16_UNORM_S8_UINT),
        129 => Some(skia_safe::gpu::vk::Format::D24_UNORM_S8_UINT),
        130 => Some(skia_safe::gpu::vk::Format::D32_SFLOAT_S8_UINT),
        131 => Some(skia_safe::gpu::vk::Format::BC1_RGB_UNORM_BLOCK),
        132 => Some(skia_safe::gpu::vk::Format::BC1_RGB_SRGB_BLOCK),
        133 => Some(skia_safe::gpu::vk::Format::BC1_RGBA_UNORM_BLOCK),
        134 => Some(skia_safe::gpu::vk::Format::BC1_RGBA_SRGB_BLOCK),
        135 => Some(skia_safe::gpu::vk::Format::BC2_UNORM_BLOCK),
        136 => Some(skia_safe::gpu::vk::Format::BC2_SRGB_BLOCK),
        137 => Some(skia_safe::gpu::vk::Format::BC3_UNORM_BLOCK),
        138 => Some(skia_safe::gpu::vk::Format::BC3_SRGB_BLOCK),
        139 => Some(skia_safe::gpu::vk::Format::BC4_UNORM_BLOCK),
        140 => Some(skia_safe::gpu::vk::Format::BC4_SNORM_BLOCK),
        141 => Some(skia_safe::gpu::vk::Format::BC5_UNORM_BLOCK),
        142 => Some(skia_safe::gpu::vk::Format::BC5_SNORM_BLOCK),
        143 => Some(skia_safe::gpu::vk::Format::BC6H_UFLOAT_BLOCK),
        144 => Some(skia_safe::gpu::vk::Format::BC6H_SFLOAT_BLOCK),
        145 => Some(skia_safe::gpu::vk::Format::BC7_UNORM_BLOCK),
        146 => Some(skia_safe::gpu::vk::Format::BC7_SRGB_BLOCK),
        147 => Some(skia_safe::gpu::vk::Format::ETC2_R8G8B8_UNORM_BLOCK),
        148 => Some(skia_safe::gpu::vk::Format::ETC2_R8G8B8_SRGB_BLOCK),
        149 => Some(skia_safe::gpu::vk::Format::ETC2_R8G8B8A1_UNORM_BLOCK),
        150 => Some(skia_safe::gpu::vk::Format::ETC2_R8G8B8A1_SRGB_BLOCK),
        151 => Some(skia_safe::gpu::vk::Format::ETC2_R8G8B8A8_UNORM_BLOCK),
        152 => Some(skia_safe::gpu::vk::Format::ETC2_R8G8B8A8_SRGB_BLOCK),
        153 => Some(skia_safe::gpu::vk::Format::EAC_R11_UNORM_BLOCK),
        154 => Some(skia_safe::gpu::vk::Format::EAC_R11_SNORM_BLOCK),
        155 => Some(skia_safe::gpu::vk::Format::EAC_R11G11_UNORM_BLOCK),
        156 => Some(skia_safe::gpu::vk::Format::EAC_R11G11_SNORM_BLOCK),
        157 => Some(skia_safe::gpu::vk::Format::ASTC_4x4_UNORM_BLOCK),
        158 => Some(skia_safe::gpu::vk::Format::ASTC_4x4_SRGB_BLOCK),
        159 => Some(skia_safe::gpu::vk::Format::ASTC_5x4_UNORM_BLOCK),
        160 => Some(skia_safe::gpu::vk::Format::ASTC_5x4_SRGB_BLOCK),
        161 => Some(skia_safe::gpu::vk::Format::ASTC_5x5_UNORM_BLOCK),
        162 => Some(skia_safe::gpu::vk::Format::ASTC_5x5_SRGB_BLOCK),
        163 => Some(skia_safe::gpu::vk::Format::ASTC_6x5_UNORM_BLOCK),
        164 => Some(skia_safe::gpu::vk::Format::ASTC_6x5_SRGB_BLOCK),
        165 => Some(skia_safe::gpu::vk::Format::ASTC_6x6_UNORM_BLOCK),
        166 => Some(skia_safe::gpu::vk::Format::ASTC_6x6_SRGB_BLOCK),
        167 => Some(skia_safe::gpu::vk::Format::ASTC_8x5_UNORM_BLOCK),
        168 => Some(skia_safe::gpu::vk::Format::ASTC_8x5_SRGB_BLOCK),
        169 => Some(skia_safe::gpu::vk::Format::ASTC_8x6_UNORM_BLOCK),
        170 => Some(skia_safe::gpu::vk::Format::ASTC_8x6_SRGB_BLOCK),
        171 => Some(skia_safe::gpu::vk::Format::ASTC_8x8_UNORM_BLOCK),
        172 => Some(skia_safe::gpu::vk::Format::ASTC_8x8_SRGB_BLOCK),
        173 => Some(skia_safe::gpu::vk::Format::ASTC_10x5_UNORM_BLOCK),
        174 => Some(skia_safe::gpu::vk::Format::ASTC_10x5_SRGB_BLOCK),
        175 => Some(skia_safe::gpu::vk::Format::ASTC_10x6_UNORM_BLOCK),
        176 => Some(skia_safe::gpu::vk::Format::ASTC_10x6_SRGB_BLOCK),
        177 => Some(skia_safe::gpu::vk::Format::ASTC_10x8_UNORM_BLOCK),
        178 => Some(skia_safe::gpu::vk::Format::ASTC_10x8_SRGB_BLOCK),
        179 => Some(skia_safe::gpu::vk::Format::ASTC_10x10_UNORM_BLOCK),
        180 => Some(skia_safe::gpu::vk::Format::ASTC_10x10_SRGB_BLOCK),
        181 => Some(skia_safe::gpu::vk::Format::ASTC_12x10_UNORM_BLOCK),
        182 => Some(skia_safe::gpu::vk::Format::ASTC_12x10_SRGB_BLOCK),
        183 => Some(skia_safe::gpu::vk::Format::ASTC_12x12_UNORM_BLOCK),
        184 => Some(skia_safe::gpu::vk::Format::ASTC_12x12_SRGB_BLOCK),
        1000156000 => Some(skia_safe::gpu::vk::Format::G8B8G8R8_422_UNORM),
        1000156001 => Some(skia_safe::gpu::vk::Format::B8G8R8G8_422_UNORM),
        1000156002 => Some(skia_safe::gpu::vk::Format::G8_B8_R8_3PLANE_420_UNORM),
        1000156003 => Some(skia_safe::gpu::vk::Format::G8_B8R8_2PLANE_420_UNORM),
        1000156004 => Some(skia_safe::gpu::vk::Format::G8_B8_R8_3PLANE_422_UNORM),
        1000156005 => Some(skia_safe::gpu::vk::Format::G8_B8R8_2PLANE_422_UNORM),
        1000156006 => Some(skia_safe::gpu::vk::Format::G8_B8_R8_3PLANE_444_UNORM),
        1000156007 => Some(skia_safe::gpu::vk::Format::R10X6_UNORM_PACK16),
        1000156008 => Some(skia_safe::gpu::vk::Format::R10X6G10X6_UNORM_2PACK16),
        1000156009 => Some(skia_safe::gpu::vk::Format::R10X6G10X6B10X6A10X6_UNORM_4PACK16),
        1000156010 => Some(skia_safe::gpu::vk::Format::G10X6B10X6G10X6R10X6_422_UNORM_4PACK16),
        1000156011 => Some(skia_safe::gpu::vk::Format::B10X6G10X6R10X6G10X6_422_UNORM_4PACK16),
        1000156012 => Some(skia_safe::gpu::vk::Format::G10X6_B10X6_R10X6_3PLANE_420_UNORM_3PACK16),
        1000156013 => Some(skia_safe::gpu::vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16),
        1000156014 => Some(skia_safe::gpu::vk::Format::G10X6_B10X6_R10X6_3PLANE_422_UNORM_3PACK16),
        1000156015 => Some(skia_safe::gpu::vk::Format::G10X6_B10X6R10X6_2PLANE_422_UNORM_3PACK16),
        1000156016 => Some(skia_safe::gpu::vk::Format::G10X6_B10X6_R10X6_3PLANE_444_UNORM_3PACK16),
        1000156017 => Some(skia_safe::gpu::vk::Format::R12X4_UNORM_PACK16),
        1000156018 => Some(skia_safe::gpu::vk::Format::R12X4G12X4_UNORM_2PACK16),
        1000156019 => Some(skia_safe::gpu::vk::Format::R12X4G12X4B12X4A12X4_UNORM_4PACK16),
        1000156020 => Some(skia_safe::gpu::vk::Format::G12X4B12X4G12X4R12X4_422_UNORM_4PACK16),
        1000156021 => Some(skia_safe::gpu::vk::Format::B12X4G12X4R12X4G12X4_422_UNORM_4PACK16),
        1000156022 => Some(skia_safe::gpu::vk::Format::G12X4_B12X4_R12X4_3PLANE_420_UNORM_3PACK16),
        1000156023 => Some(skia_safe::gpu::vk::Format::G12X4_B12X4R12X4_2PLANE_420_UNORM_3PACK16),
        1000156024 => Some(skia_safe::gpu::vk::Format::G12X4_B12X4_R12X4_3PLANE_422_UNORM_3PACK16),
        1000156025 => Some(skia_safe::gpu::vk::Format::G12X4_B12X4R12X4_2PLANE_422_UNORM_3PACK16),
        1000156026 => Some(skia_safe::gpu::vk::Format::G12X4_B12X4_R12X4_3PLANE_444_UNORM_3PACK16),
        1000156027 => Some(skia_safe::gpu::vk::Format::G16B16G16R16_422_UNORM),
        1000156028 => Some(skia_safe::gpu::vk::Format::B16G16R16G16_422_UNORM),
        1000156029 => Some(skia_safe::gpu::vk::Format::G16_B16_R16_3PLANE_420_UNORM),
        1000156030 => Some(skia_safe::gpu::vk::Format::G16_B16R16_2PLANE_420_UNORM),
        1000156031 => Some(skia_safe::gpu::vk::Format::G16_B16_R16_3PLANE_422_UNORM),
        1000156032 => Some(skia_safe::gpu::vk::Format::G16_B16R16_2PLANE_422_UNORM),
        1000156033 => Some(skia_safe::gpu::vk::Format::G16_B16_R16_3PLANE_444_UNORM),
        1000330000 => Some(skia_safe::gpu::vk::Format::G8_B8R8_2PLANE_444_UNORM),
        1000330001 => Some(skia_safe::gpu::vk::Format::G10X6_B10X6R10X6_2PLANE_444_UNORM_3PACK16),
        1000330002 => Some(skia_safe::gpu::vk::Format::G12X4_B12X4R12X4_2PLANE_444_UNORM_3PACK16),
        1000330003 => Some(skia_safe::gpu::vk::Format::G16_B16R16_2PLANE_444_UNORM),
        1000340000 => Some(skia_safe::gpu::vk::Format::A4R4G4B4_UNORM_PACK16),
        1000340001 => Some(skia_safe::gpu::vk::Format::A4B4G4R4_UNORM_PACK16),
        1000066000 => Some(skia_safe::gpu::vk::Format::ASTC_4x4_SFLOAT_BLOCK),
        1000066001 => Some(skia_safe::gpu::vk::Format::ASTC_5x4_SFLOAT_BLOCK),
        1000066002 => Some(skia_safe::gpu::vk::Format::ASTC_5x5_SFLOAT_BLOCK),
        1000066003 => Some(skia_safe::gpu::vk::Format::ASTC_6x5_SFLOAT_BLOCK),
        1000066004 => Some(skia_safe::gpu::vk::Format::ASTC_6x6_SFLOAT_BLOCK),
        1000066005 => Some(skia_safe::gpu::vk::Format::ASTC_8x5_SFLOAT_BLOCK),
        1000066006 => Some(skia_safe::gpu::vk::Format::ASTC_8x6_SFLOAT_BLOCK),
        1000066007 => Some(skia_safe::gpu::vk::Format::ASTC_8x8_SFLOAT_BLOCK),
        1000066008 => Some(skia_safe::gpu::vk::Format::ASTC_10x5_SFLOAT_BLOCK),
        1000066009 => Some(skia_safe::gpu::vk::Format::ASTC_10x6_SFLOAT_BLOCK),
        1000066010 => Some(skia_safe::gpu::vk::Format::ASTC_10x8_SFLOAT_BLOCK),
        1000066011 => Some(skia_safe::gpu::vk::Format::ASTC_10x10_SFLOAT_BLOCK),
        1000066012 => Some(skia_safe::gpu::vk::Format::ASTC_12x10_SFLOAT_BLOCK),
        1000066013 => Some(skia_safe::gpu::vk::Format::ASTC_12x12_SFLOAT_BLOCK),
        1000054000 => Some(skia_safe::gpu::vk::Format::PVRTC1_2BPP_UNORM_BLOCK_IMG),
        1000054001 => Some(skia_safe::gpu::vk::Format::PVRTC1_4BPP_UNORM_BLOCK_IMG),
        1000054002 => Some(skia_safe::gpu::vk::Format::PVRTC2_2BPP_UNORM_BLOCK_IMG),
        1000054003 => Some(skia_safe::gpu::vk::Format::PVRTC2_4BPP_UNORM_BLOCK_IMG),
        1000054004 => Some(skia_safe::gpu::vk::Format::PVRTC1_2BPP_SRGB_BLOCK_IMG),
        1000054005 => Some(skia_safe::gpu::vk::Format::PVRTC1_4BPP_SRGB_BLOCK_IMG),
        1000054006 => Some(skia_safe::gpu::vk::Format::PVRTC2_2BPP_SRGB_BLOCK_IMG),
        1000054007 => Some(skia_safe::gpu::vk::Format::PVRTC2_4BPP_SRGB_BLOCK_IMG),
        1000464000 => Some(skia_safe::gpu::vk::Format::R16G16_S10_5_NV),
        1000470000 => Some(skia_safe::gpu::vk::Format::A1B5G5R5_UNORM_PACK16_KHR),
        1000470001 => Some(skia_safe::gpu::vk::Format::A8_UNORM_KHR),
        2147483647 => Some(skia_safe::gpu::vk::Format::MAX_ENUM),
        _ => None,
    }
}
