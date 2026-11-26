//! Explicit GPU smoke test: cargo test -p winia --test wgpu_offscreen_test -- --ignored
use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use skia_safe::gpu::graphite;
use skia_safe::wgpu;
use skia_safe::{Color, ColorType, Image, Paint, Rect, image::RequiredProperties};

const SIZE: u32 = 64;

#[test]
#[ignore = "requires a wgpu adapter"]
fn graphite_renders_expected_pixels_into_a_wgpu_texture() {
    let instance = wgpu::Instance::default();
    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        .expect("no wgpu adapter in this environment");
    println!("adapter: {:?}", adapter.get_info());
    let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
        .expect("no wgpu device");
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("winia graphite target"),
        size: wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });

    let mut context = graphite::wgpu::context_factory::make_wgpu(&device, &queue, None)
        .expect("no Graphite context for the wgpu device");
    let mut options = graphite::RecorderOptions::new();
    options.set_image_provider(UploadProvider);
    let mut recorder = context
        .make_recorder(Some(&options))
        .expect("no Graphite recorder");
    let mut surface = graphite::wgpu::surfaces::wrap_texture(
        &mut recorder,
        &texture,
        ColorType::RGBA8888,
        None,
        None,
    )
    .expect("no Skia surface for the wgpu texture");
    {
        let canvas = surface.canvas();
        canvas.clear(Color::WHITE);
        let mut paint = Paint::default();
        paint.set_anti_alias(false);
        paint.set_color(Color::from_rgb(255, 0, 0));
        canvas.draw_rect(Rect::from_xywh(0.0, 0.0, 16.0, 16.0), &paint);
        paint.set_color(Color::from_rgb(0, 0, 255));
        canvas.draw_rect(Rect::from_xywh(32.0, 32.0, 16.0, 16.0), &paint);

        let mut raster = skia_safe::surfaces::raster_n32_premul((16, 16)).unwrap();
        raster.canvas().clear(Color::from_rgb(0, 255, 0));
        let image = raster.image_snapshot();
        canvas.draw_image(&image, (16.0, 16.0), Some(&paint));
    }
    drop(surface);

    let mut recording = recorder.snap().expect("no recording");
    context.insert_recording(&graphite::InsertRecordingInfo::new(&mut recording));
    assert!(context.submit_and_wait(), "submitting to Graphite failed");

    let pixels = read_texture(&device, &queue, &texture);
    assert_eq!(pixel(&pixels, 8, 8), [255, 0, 0, 255]);
    assert_eq!(pixel(&pixels, 40, 40), [0, 0, 255, 255]);
    assert_eq!(pixel(&pixels, 24, 24), [0, 255, 0, 255]);
    assert_eq!(pixel(&pixels, 56, 8), [255, 255, 255, 255]);
}

struct UploadProvider;

impl graphite::ImageProvider for UploadProvider {
    fn find_or_create(
        &self,
        recorder: &graphite::BorrowedRecorder,
        image: &Image,
        required: RequiredProperties,
    ) -> Option<Image> {
        recorder.texture_from_image(image, required)
    }
}

fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let index = ((y * SIZE + x) * 4) as usize;
    [
        pixels[index],
        pixels[index + 1],
        pixels[index + 2],
        pixels[index + 3],
    ]
}

fn read_texture(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Vec<u8> {
    let bytes_per_row = SIZE * 4;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("winia readback"),
        size: u64::from(bytes_per_row * SIZE),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: None,
            },
        },
        texture.size(),
    );
    queue.submit([encoder.finish()]);
    buffer.map_async(wgpu::MapMode::Read, .., |result| {
        result.expect("mapping the readback buffer failed")
    });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .expect("polling the device failed");
    buffer
        .get_mapped_range(..)
        .expect("reading the mapped buffer failed")
        .to_vec()
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
