use ash::vk::Handle;
use std::sync::Arc;

use parking_lot::Mutex;
use vulkano::{
    device::Queue,
    image::{view::ImageView, ImageLayout, ImageUsage},
    render_pass::{Framebuffer, FramebufferCreateInfo, RenderPass},
    swapchain::{
        acquire_next_image, PresentMode, Surface, Swapchain, SwapchainAcquireFuture,
        SwapchainCreateInfo, SwapchainPresentInfo,
    },
    sync::{self, GpuFuture},
    Validated, VulkanError, VulkanObject,
};

use skia_safe::{
    gpu::{self, backend_render_targets, surfaces, vk},
    ColorType,
};

use winit::{dpi::PhysicalSize, window::Window};

pub struct VulkanRenderer {
    pub window: Arc<Box<dyn Window>>,
    /// Shared Skia context - NOT destroyed on drop to allow reuse (e.g., Android background/foreground)
    skia_ctx: Arc<Mutex<gpu::DirectContext>>,
    queue: Arc<Queue>,
    swapchain: Arc<Swapchain>,
    framebuffers: Vec<Arc<Framebuffer>>,
    render_pass: Arc<RenderPass>,
    last_render: Option<Box<dyn GpuFuture>>,
    swapchain_is_valid: bool,
}

impl Drop for VulkanRenderer {
    fn drop(&mut self) {
        // Wait for GPU to finish all in-flight operations
        if let Some(last_render) = self.last_render.take() {
            if last_render.queue().is_some() {
                if let Ok(fence) = last_render.then_signal_fence_and_flush() {
                    let _ = fence.wait(None);
                }
            }
        }

        // Ensure device is completely idle
        let _ = unsafe { self.queue.device().wait_idle() };

        // Flush any pending Skia work (don't abandon - context is shared)
        self.skia_ctx.lock().flush_and_submit();

        // Clear framebuffers before swapchain is dropped
        self.framebuffers.clear();
    }
}

impl VulkanRenderer {
    pub fn new(
        window: Arc<Box<dyn Window>>,
        queue: Arc<Queue>,
        skia_ctx: Arc<Mutex<gpu::DirectContext>>,
    ) -> Self {
        let instance = queue.device().instance();
        let device = queue.device();
        let queue = queue.clone();

        let surface = Surface::from_window(instance.clone(), window.clone()).unwrap_or_else(|e| {
            let msg = format!("Failed to create Vulkan surface in renderer: {e}");
            log::error!("{msg}");
            #[cfg(debug_assertions)]
            panic!("{msg}");
            std::process::exit(1);
        });
        let window_size = window.surface_size();

        let (swapchain, _images) = {
            let surface_capabilities = device
                .physical_device()
                .surface_capabilities(&surface, Default::default())
                .unwrap_or_else(|e| {
                    let msg = format!("Failed to get surface capabilities: {e}");
                    log::error!("{msg}");
                    #[cfg(debug_assertions)]
                    panic!("{msg}");
                    std::process::exit(1);
                });

            let surface_formats = device
                .physical_device()
                .surface_formats(&surface, Default::default())
                .unwrap_or_else(|e| {
                    let msg = format!("Failed to get surface formats: {e}");
                    log::error!("{msg}");
                    #[cfg(debug_assertions)]
                    panic!("{msg}");
                    std::process::exit(1);
                });


            let (image_format, _) = surface_formats
                .iter()
                .find(|(format, _)| {
                    *format == vulkano::format::Format::B8G8R8A8_UNORM
                        || *format == vulkano::format::Format::R8G8B8A8_UNORM
                })
                .cloned()
                .unwrap_or_else(|| {
                    let msg = "No supported surface format found (need B8G8R8A8_UNORM or R8G8B8A8_UNORM)".to_string();
                    log::error!("{msg}");
                    #[cfg(debug_assertions)]
                    panic!("{msg}");
                    std::process::exit(1);
                });

            Swapchain::new(
                device.clone(),
                surface,
                SwapchainCreateInfo {
                    // Use triple buffering for better performance
                    min_image_count: {
                        let desired = surface_capabilities.min_image_count.max(3);
                        match surface_capabilities.max_image_count {
                            Some(max) => desired.min(max),
                            None => desired,
                        }
                    },
                    image_extent: window_size.into(),
                    image_usage: ImageUsage::COLOR_ATTACHMENT,
                    image_format,
                    present_mode: PresentMode::Fifo,
                    composite_alpha: surface_capabilities
                        .supported_composite_alpha
                        .into_iter()
                        .next()
                        .unwrap_or(vulkano::swapchain::CompositeAlpha::Opaque),
                    ..Default::default()
                },
            )
            .unwrap_or_else(|e| {
                let msg = format!("Failed to create swapchain: {e}");
                log::error!("{msg}");
                #[cfg(debug_assertions)]
                panic!("{msg}");
                std::process::exit(1);
            })
        };

        let render_pass = vulkano::single_pass_renderpass!(
            device.clone(),
            attachments: {
                color: {
                    format: swapchain.image_format(),
                    samples: 1,
                    load_op: DontCare,
                    store_op: Store,
                    initial_layout: ImageLayout::Undefined,
                    final_layout: ImageLayout::PresentSrc,
                },
            },
            pass: {
                color: [color],
                depth_stencil: {},
            },
        )
        .unwrap_or_else(|e| {
            let msg = format!("Failed to create render pass: {e}");
            log::error!("{msg}");
            #[cfg(debug_assertions)]
            panic!("{msg}");
            std::process::exit(1);
        });

        let framebuffers = vec![];
        let swapchain_is_valid = false;
        let last_render = Some(sync::now(device.clone()).boxed());

        VulkanRenderer {
            skia_ctx,
            queue,
            window,
            swapchain,
            swapchain_is_valid,
            render_pass,
            framebuffers,
            last_render,
        }
    }

    pub fn invalidate_swapchain(&mut self) {
        self.swapchain_is_valid = false;
    }

    pub fn prepare_swapchain(&mut self) {
        let window_size: PhysicalSize<u32> = self.window.surface_size();
        if window_size.width > 0 && window_size.height > 0 && !self.swapchain_is_valid {
            // Wait for any pending GPU operations before recreating swapchain
            if let Some(last_render) = self.last_render.take() {
                if last_render.queue().is_some() {
                    match last_render.then_signal_fence_and_flush() {
                        Ok(fence) => {
                            let _ = fence.wait(None);
                        }
                        Err(_) => {
                            let _ = unsafe { self.queue.device().wait_idle() };
                        }
                    }
                } else {
                    let _ = unsafe { self.queue.device().wait_idle() };
                }
            } else {
                let _ = unsafe { self.queue.device().wait_idle() };
            }

            let (new_swapchain, new_images) = match self
                .swapchain
                .recreate(SwapchainCreateInfo {
                    image_extent: window_size.into(),
                    ..self.swapchain.create_info()
                }) {
                Ok(result) => result,
                Err(e) => {
                    log::error!("Failed to recreate swapchain: {e}");
                    #[cfg(debug_assertions)]
                    panic!("Failed to recreate swapchain: {e}");
                    // Release: 保持 swapchain_is_valid = false，下一帧重试
                    return;
                }
            };

            self.swapchain = new_swapchain;

            self.framebuffers = new_images
                .iter()
                .filter_map(|image| {
                    let view = match ImageView::new_default(image.clone()) {
                        Ok(v) => v,
                        Err(e) => {
                            log::error!("Failed to create image view: {e}");
                            #[cfg(debug_assertions)]
                            panic!("Failed to create image view: {e}");
                            return None;
                        }
                    };
                    match Framebuffer::new(
                        self.render_pass.clone(),
                        FramebufferCreateInfo {
                            attachments: vec![view],
                            ..Default::default()
                        },
                    ) {
                        Ok(fb) => Some(fb),
                        Err(e) => {
                            log::error!("Failed to create framebuffer: {e}");
                            #[cfg(debug_assertions)]
                            panic!("Failed to create framebuffer: {e}");
                            None
                        }
                    }
                })
                .collect::<Vec<_>>();

            self.last_render = Some(sync::now(self.queue.device().clone()).boxed());

            self.swapchain_is_valid = true;
        }
    }

    fn get_next_frame(&mut self) -> Option<(u32, SwapchainAcquireFuture)> {
        // acquire 带 timeout：vkAcquireNextImageKHR 会 CPU 阻塞直到图像从显示引擎
        // 释放（free function 内部自带 fence+semaphore）。绘制前必须等图像可用，
        // 否则绘制命令与扫描输出竞态——间歇性呈现部分渲染帧（动画启动时一帧闪烁）。
        let (image_index, suboptimal, acquire_future) =
            match acquire_next_image(self.swapchain.clone(), Some(std::time::Duration::from_millis(100)))
                .map_err(Validated::unwrap)
            {
                Ok(r) => r,
                Err(VulkanError::OutOfDate) => {
                    self.swapchain_is_valid = false;
                    return None;
                }
                Err(VulkanError::DeviceLost) => {
                    log::error!("GPU device lost!");
                    #[cfg(debug_assertions)]
                    panic!("GPU device lost — cannot recover");
                    #[cfg(not(debug_assertions))]
                    return None;
                }
                Err(e) => {
                    log::error!("Failed to acquire next image: {e}");
                    #[cfg(debug_assertions)]
                    panic!("Render loop error: {e}");
                    #[cfg(not(debug_assertions))]
                    return None; // Release: 跳过此帧
                }
            };

        if suboptimal {
            self.swapchain_is_valid = false;
        }

        Some((image_index, acquire_future))
    }

    pub fn draw<F>(&mut self, f: F)
    where
        F: FnOnce(&mut skia_safe::Surface),
    {
        // Clean up finished resources to prevent memory leaks
        if let Some(last_render) = self.last_render.as_mut() {
            if last_render.queue().is_some() {
                last_render.cleanup_finished();
            }
        }

        if !self.swapchain_is_valid {
            self.prepare_swapchain();
        }

        let next_frame = self.get_next_frame().or_else(|| {
            self.prepare_swapchain();
            self.get_next_frame()
        });

        if let Some((image_index, acquire_future)) = next_frame {
            let skia_ctx = self.skia_ctx.clone();

            if self.framebuffers.is_empty() {
                eprintln!("Warning: framebuffers not initialized, skipping frame");
                return;
            }

            if (image_index as usize) >= self.framebuffers.len() {
                eprintln!(
                    "Warning: image_index {} out of bounds (framebuffers: {})",
                    image_index,
                    self.framebuffers.len()
                );
                self.swapchain_is_valid = false;
                return;
            }

            let framebuffer = self.framebuffers[image_index as usize].clone();

            let mut surface = match surface_for_framebuffer(skia_ctx.clone(), framebuffer.clone()) {
                Some(s) => s,
                None => {
                    eprintln!("Warning: Failed to create Skia surface, skipping frame");
                    self.swapchain_is_valid = false;
                    return;
                }
            };

            f(&mut surface);

            skia_ctx.lock().flush_and_submit();

            // 调试截图：flush 后读回（保证读到真实呈现帧）
            crate::vulkan::capture::capture_if_requested(&mut surface);

            let previous_future = self.last_render.take();

            let present_future: Box<dyn GpuFuture> = match previous_future {
                Some(prev) if prev.queue().is_some() => prev.join(acquire_future).boxed(),
                _ => acquire_future.boxed(),
            };

            let present_result = present_future
                .then_swapchain_present(
                    self.queue.clone(),
                    SwapchainPresentInfo::swapchain_image_index(
                        self.swapchain.clone(),
                        image_index,
                    ),
                )
                .then_signal_fence_and_flush();

            match present_result {
                Ok(future) => {
                    // 同步等待本帧 present 完成（GPU 同步）——异步 present 链与
                    // 当前帧绘制存在竞态（present 只 join 上一帧 present + acquire，
                    // 不显式等待本帧 draw 完成）——间歇性呈现部分渲染帧（动画启动
                    // 时一帧闪烁）。等待后 GPU 管线串行：draw → present → 下帧。
                    let _ = future.wait(None);
                    self.last_render = Some(future.boxed());
                }
                Err(Validated::Error(VulkanError::OutOfDate)) => {
                    self.swapchain_is_valid = false;
                    self.last_render = Some(sync::now(self.queue.device().clone()).boxed());
                }
                Err(e) => {
                    eprintln!("Failed to present frame: {:?}", e);
                    self.swapchain_is_valid = false;
                    self.last_render = Some(sync::now(self.queue.device().clone()).boxed());
                }
            }
        }
    }
}

/// Create a Skia Surface for the specified Framebuffer.
/// Returns None if the surface cannot be created.
fn surface_for_framebuffer(
    skia_ctx: Arc<Mutex<gpu::DirectContext>>,
    framebuffer: Arc<Framebuffer>,
) -> Option<skia_safe::Surface> {
    let [width, height] = framebuffer.extent();
    let image_access = &framebuffer.attachments()[0];
    let image_object = image_access.image().handle().as_raw();

    let format = image_access.format();

    let (vk_format, color_type) = match format {
        vulkano::format::Format::B8G8R8A8_UNORM => (
            skia_safe::gpu::vk::Format::B8G8R8A8_UNORM,
            ColorType::BGRA8888,
        ),
        vulkano::format::Format::R8G8B8A8_UNORM => (
            skia_safe::gpu::vk::Format::R8G8B8A8_UNORM,
            ColorType::RGBA8888,
        ),
        _ => {
            eprintln!("Unsupported color format: {:?}", format);
            return None;
        }
    };

    let alloc = vk::Alloc::default();
    let image_info = &unsafe {
        vk::ImageInfo::new(
            image_object as _,
            alloc,
            vk::ImageTiling::OPTIMAL,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk_format,
            1,
            None,
            None,
            None,
            None,
        )
    };

    let render_target = &backend_render_targets::make_vk(
        (width.try_into().unwrap(), height.try_into().unwrap()),
        image_info,
    );

    let mut ctx = skia_ctx.lock();

    surfaces::wrap_backend_render_target(
        &mut ctx,
        render_target,
        gpu::SurfaceOrigin::TopLeft,
        color_type,
        None,
        None,
    )
}