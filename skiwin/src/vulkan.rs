use crate::SkiaWindow;
use ash::vk;
use parking_lot::Mutex;
use pixels::wgpu::{Backends, TextureFormat};
use pixels::{Pixels, PixelsBuilder};
use skia_safe::gpu::vk::{BackendContext, GetProcOf};
use skia_safe::gpu::{Budgeted, DirectContext, SurfaceOrigin};
use skia_safe::{ImageInfo, Surface};
use std::ops::Deref;
use std::ptr;
use std::sync::Arc;
use vulkano::device::physical::PhysicalDevice;
use vulkano::device::{Device, DeviceCreateInfo, Queue, QueueCreateInfo, QueueFlags};
use vulkano::instance::{Instance, InstanceCreateFlags, InstanceCreateInfo, InstanceExtensions};
use vulkano::{Handle, VulkanLibrary, VulkanObject};
use winit::dpi::PhysicalSize;
use winit::window::Window;

pub struct VulkanSkiaWindow {
    pixels: Pixels<'static>,
    skia_surface: Arc<Mutex<Surface>>,
    // skia_context: DirectContext,
    vulkan_context: VulkanContext,
    window: Arc<Window>,
}

impl VulkanSkiaWindow {
    pub fn new(
        window: Arc<Window>,
        device_selector: Option<Box<dyn Fn(&PhysicalDevice) -> bool>>,
    ) -> Self {
        let vulkan_context = VulkanContext::new(window.title().as_str(), device_selector);
        let mut skia_context = {
            let get_proc = |of| unsafe {
                match vulkan_context.get_proc(of) {
                    Some(f) => f as _,
                    None => {
                        // println!("resolve of {} failed", of.name().to_str().unwrap());
                        ptr::null()
                    }
                }
            };

            let backend_context = unsafe {
                BackendContext::new(
                    vulkan_context.instance.handle().as_raw() as _,
                    vulkan_context.physical_device.handle().as_raw() as _,
                    vulkan_context.device.handle().as_raw() as _,
                    (
                        vulkan_context.queue_and_index.0.handle().as_raw() as _,
                        vulkan_context.queue_and_index.1,
                    ),
                    &get_proc,
                )
            };

            skia_safe::gpu::direct_contexts::make_vulkan(&backend_context, None).unwrap()
        };

        // let window = Arc::new(window);
        let size = window.inner_size();
        let skia_surface = create_surface(&mut skia_context, size);
        let pixels = {
            let surface_texture = pixels::SurfaceTexture::new(size.width, size.height, window.clone());
            PixelsBuilder::new(size.width, size.height, surface_texture)
                .wgpu_backend(Backends::VULKAN)
                .texture_format(TextureFormat::Bgra8UnormSrgb)
                .build()
                .unwrap()
        };
        // soft_buffer_surface.buffer_mut().unwrap().present()
        Self {
            pixels,
            skia_surface,
            // skia_context,
            vulkan_context,
            window,
        }
    }
}

fn create_surface(
    skia_context: &mut DirectContext,
    size: impl Into<PhysicalSize<u32>>,
) -> Arc<Mutex<Surface>> {
    let size = size.into();
    let width = size.width;
    let height = size.height;
    let image_info = ImageInfo::new_n32_premul((width as i32, height as i32), None);
    Arc::new(Mutex::new(
        skia_safe::gpu::surfaces::render_target(
            skia_context,
            Budgeted::Yes,
            &image_info,
            None,
            SurfaceOrigin::TopLeft,
            None,
            false,
            None,
        )
            .unwrap(),
    ))
}

// impl_skia_window!(VulkanSkiaWindow);

impl SkiaWindow for VulkanSkiaWindow {
    fn resize(&mut self) {
        let size = self.window.inner_size();
        self.pixels.resize_buffer(size.width, size.height).unwrap();
        self.pixels.resize_surface(size.width, size.height).unwrap();
/*        self.skia_surface = Arc::new(Mutex::new(self.skia_surface.lock().new_surface_with_dimensions(
            (size.width as i32, size.height as i32)
        ).unwrap()));*/
        let new_surface = self.skia_surface.lock().new_surface_with_dimensions(
            (size.width as i32, size.height as i32)
        ).unwrap();
        self.skia_surface = Arc::new(Mutex::new(new_surface));
    }

    fn surface(&self) -> Arc<Mutex<Surface>> {
        self.skia_surface.clone()
    }

    fn present(&mut self) {
        // let size = self.soft_buffer_surface.window().inner_size();
        // let mut soft_buffer = self.soft_buffer_surface.buffer_mut().unwrap();
        // let u8_slice = bytemuck::cast_slice_mut::<u32, u8>(&mut soft_buffer);
        // let image_info =
        //     ImageInfo::new_n32_premul((size.width as i32, size.height as i32), None);
        // self.skia_surface.lock().read_pixels(
        //     &image_info,
        //     u8_slice,
        //     size.width as usize * 4,
        //     (0, 0),
        // );
        // soft_buffer.present().unwrap();
        let frame = self.pixels.frame_mut();
        let size = self.window.inner_size();
        let image_info =
            ImageInfo::new_n32_premul((size.width as i32, size.height as i32), None);
        self.skia_surface.lock().read_pixels(
            &image_info,
            frame,
            size.width as usize * 4,
            (0, 0),
        );
        self.pixels.render().unwrap();
    }
}
impl Deref for VulkanSkiaWindow {
    type Target = Window;

    fn deref(&self) -> &Self::Target {
        self.window.as_ref()
    }
}
impl AsRef<Window> for VulkanSkiaWindow {
    fn as_ref(&self) -> &Window {
        self.window.as_ref()
    }
}

pub struct VulkanContext {
    pub vulkan_library: Arc<VulkanLibrary>,
    pub instance: Arc<Instance>,
    pub physical_device: Arc<PhysicalDevice>,
    pub device: Arc<Device>,
    pub queue_and_index: (Arc<Queue>, usize),
}

impl VulkanContext {
    pub fn new(
        app_name: &str,
        device_selector: Option<Box<dyn Fn(&PhysicalDevice) -> bool>>,
    ) -> Self {
        let vulkan_library = VulkanLibrary::new().unwrap();

        let instance: Arc<Instance> = {
            let instance_extensions = InstanceExtensions {
                khr_get_physical_device_properties2: true,
                // khr_portability_enumeration: true,
                ..Default::default()
            };

            let create_info = InstanceCreateInfo {
                engine_name: Some(app_name.to_string()),
                enabled_extensions: instance_extensions,
                flags: InstanceCreateFlags::ENUMERATE_PORTABILITY,
                ..Default::default()
            };

            Instance::new(vulkan_library.clone(), create_info).unwrap()
        };

        let (physical_device, queue_family_index) = {
            let physical_devices = instance.enumerate_physical_devices().unwrap();

            let mut d = physical_devices.map(|physical_device| {
                physical_device
                    .queue_family_properties()
                    .iter()
                    .enumerate()
                    .find_map(|(index, info)| {
                        let supports_graphic = info.queue_flags.contains(QueueFlags::GRAPHICS);
                        supports_graphic.then_some((physical_device.clone(), index))
                    })
            });

            let result = if let Some(device_select) = device_selector {
                d.find_map(|v| {
                    if let Some((physical_device, queue_family_index)) = &v {
                        if device_select(physical_device.deref()) {
                            return Some((physical_device.clone(), *queue_family_index));
                        }
                    }
                    None
                })
                 .expect("No suitable device found")
            } else {
                d.find_map(|v| v).expect("No suitable device found")
            };

            #[cfg(debug_assertions)]
            {
                let (physical_device, _) = &result;
                let device_properties = physical_device.properties();
                // println!("Using device: {} ", device_properties.device_name);
            }

            result
        };

        let (device, queues) = {
            let queue_create_info = QueueCreateInfo {
                queue_family_index: queue_family_index as _,
                ..Default::default()
            };

            let device_create_info = DeviceCreateInfo {
                queue_create_infos: vec![queue_create_info],
                ..Default::default()
            };

            Device::new(physical_device.clone(), device_create_info).unwrap()
        };

        let queue_index = 0;
        let (_, queue) = queues.enumerate().nth(queue_index).unwrap();

        Self {
            vulkan_library,
            instance,
            physical_device,
            device,
            queue_and_index: (queue, 0),
        }
    }

    pub unsafe fn get_proc(&self, of: GetProcOf) -> Option<unsafe extern "system" fn()> {
        use ash::vk::Handle;
        match of {
            GetProcOf::Instance(instance, name) => {
                let ash_instance = vk::Instance::from_raw(instance as _);
                self.vulkan_library
                    .get_instance_proc_addr(ash_instance, name)
            }
            GetProcOf::Device(device, name) => {
                let ash_device = vk::Device::from_raw(device as _);
                (self.instance.fns().v1_0.get_device_proc_addr)(ash_device, name)
            }
        }
    }
}
