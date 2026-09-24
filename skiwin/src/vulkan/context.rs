use std::ptr;
use std::sync::Arc;

use ash::vk::Handle;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use skia_safe::gpu::{self, direct_contexts, vk};
use vulkano::{
    device::{
        physical::PhysicalDeviceType, Device, DeviceCreateInfo, DeviceExtensions, Queue,
        QueueCreateInfo, QueueFlags,
    },
    instance::{Instance, InstanceCreateFlags, InstanceCreateInfo},
    swapchain::Surface,
    VulkanLibrary, VulkanObject,
};
use winit::event_loop::ActiveEventLoop;
use winit::raw_window_handle::HasDisplayHandle;
use winit::window::Window;

use crate::{SkiwinError, SkiwinResult};

use super::renderer::VulkanRenderer;

/// Global shared Vulkan queue that persists across all windows.
static SHARED_QUEUE: Lazy<Mutex<Option<Arc<Queue>>>> = Lazy::new(|| Mutex::new(None));

pub struct VulkanRenderContext {
    pub queue: Option<Arc<Queue>>,
    pub skia_ctx: Option<Arc<Mutex<gpu::DirectContext>>>,
}

impl VulkanRenderContext {
    pub fn renderer_for_window(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        window: Arc<Box<dyn Window>>,
    ) -> SkiwinResult<VulkanRenderer> {
        let queue = {
            let mut shared = SHARED_QUEUE.lock();
            if shared.is_none() {
                *shared = Some(Self::shared_queue(event_loop, window.clone())?);
            }
            shared
                .as_ref()
                .ok_or_else(|| SkiwinError::Vulkan("shared queue was not initialised".into()))?
                .clone()
        };

        let skia_ctx = match self.skia_ctx.clone() {
            Some(ctx) => ctx,
            None => {
                let ctx = Self::create_skia_context(&queue)?;
                self.skia_ctx = Some(ctx.clone());
                ctx
            }
        };

        self.queue = Some(queue.clone());

        Ok(VulkanRenderer::new(window.clone(), queue, skia_ctx)?)
    }

    fn create_skia_context(queue: &Arc<Queue>) -> SkiwinResult<Arc<Mutex<gpu::DirectContext>>> {
        let library = queue.device().instance().library();
        let instance = queue.device().instance();
        let device = queue.device();

        unsafe {
            let get_proc = |gpo| {
                let get_device_proc_addr = instance.fns().v1_0.get_device_proc_addr;

                match gpo {
                    vk::GetProcOf::Instance(instance, name) => {
                        let vk_instance = ash::vk::Instance::from_raw(instance as _);
                        library.get_instance_proc_addr(vk_instance, name)
                    }
                    vk::GetProcOf::Device(device, name) => {
                        let vk_device = ash::vk::Device::from_raw(device as _);
                        get_device_proc_addr(vk_device, name)
                    }
                }
                .map(|f| f as _)
                .unwrap_or_else(|| {
                    eprintln!("Vulkan: failed to resolve proc");
                    ptr::null()
                })
            };

            let direct_context = direct_contexts::make_vulkan(
                &vk::BackendContext::new(
                    instance.handle().as_raw() as _,
                    device.physical_device().handle().as_raw() as _,
                    device.handle().as_raw() as _,
                    (
                        queue.handle().as_raw() as _,
                        queue.queue_family_index() as usize,
                    ),
                    &get_proc,
                ),
                None,
            )
            .ok_or_else(|| {
                SkiwinError::Vulkan("Skia could not create a Vulkan context".into())
            })?;

            Ok(Arc::new(Mutex::new(direct_context)))
        }
    }

    fn shared_queue(event_loop: &dyn ActiveEventLoop, window: Arc<Box<dyn Window>>) -> SkiwinResult<Arc<Queue>> {
        let library = VulkanLibrary::new()
            .map_err(|e| SkiwinError::Vulkan(format!("no Vulkan loader or ICD: {e}")))?;

        let display_handle = event_loop
            .display_handle()
            .map_err(|e| SkiwinError::Vulkan(format!("no display handle: {e}")))?;
        let required_extensions = Surface::required_extensions(&display_handle)
            .map_err(|e| SkiwinError::Vulkan(format!("querying surface extensions: {e}")))?;

        #[cfg(debug_assertions)]
        let enabled_layers = {
            let available_layers: Vec<_> = library
                .layer_properties()
                .unwrap()
                .map(|l| l.name().to_owned())
                .collect();

            let validation_layer = "VK_LAYER_KHRONOS_validation";
            if available_layers.iter().any(|l| l == validation_layer) {
                vec![validation_layer.to_owned()]
            } else {
                vec![]
            }
        };

        #[cfg(not(debug_assertions))]
        let enabled_layers = vec![];

        let instance = Instance::new(
            library,
            InstanceCreateInfo {
                flags: InstanceCreateFlags::ENUMERATE_PORTABILITY,
                enabled_extensions: required_extensions.clone(),
                enabled_layers,
                ..Default::default()
            },
        )
        .map_err(|e| {
            SkiwinError::Vulkan(format!(
                "no instance supports {required_extensions:?}: {e}"
            ))
        })?;

        let device_extensions = DeviceExtensions {
            khr_swapchain: true,
            ..DeviceExtensions::empty()
        };

        let surface = Surface::from_window(instance.clone(), window.clone())
            .map_err(|e| SkiwinError::Vulkan(format!("creating the window surface: {e}")))?;

        let (physical_device, queue_family_index) = instance
            .enumerate_physical_devices()
            .map_err(|e| SkiwinError::Vulkan(format!("enumerating devices: {e}")))?
            .filter(|p| p.supported_extensions().contains(&device_extensions))
            .filter_map(|p| {
                p.queue_family_properties()
                    .iter()
                    .enumerate()
                    .position(|(i, q)| {
                        q.queue_flags.intersects(QueueFlags::GRAPHICS)
                            && p.surface_support(i as u32, &surface).unwrap_or(false)
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
            .ok_or(SkiwinError::NoDevice)?;


        let (_, mut queues) = Device::new(
            physical_device,
            DeviceCreateInfo {
                enabled_extensions: device_extensions,
                queue_create_infos: vec![QueueCreateInfo {
                    queue_family_index,
                    ..Default::default()
                }],
                ..Default::default()
            },
        )
        .map_err(|e| SkiwinError::Vulkan(format!("device initialisation failed: {e}")))?;

        queues
            .next()
            .ok_or_else(|| SkiwinError::Vulkan("no queue returned from the device".into()))
    }
}

impl Default for VulkanRenderContext {
    fn default() -> Self {
        VulkanRenderContext {
            queue: None,
            skia_ctx: None,
        }
    }
}

impl Drop for VulkanRenderContext {
    fn drop(&mut self) {
        // Wait for pending GPU operations before dropping
        if let Some(queue) = &self.queue {
            let _ = unsafe { queue.device().wait_idle() };
        }

        // Clear local references (shared resources persist globally)
        self.queue = None;
        self.skia_ctx = None;
    }
}

