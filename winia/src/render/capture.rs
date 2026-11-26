//! Debug screenshot capture for the wgpu/Graphite backend.
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use skia_safe::{AlphaType, ColorType, ImageInfo, Surface};

static CAPTURE_REQUEST: AtomicBool = AtomicBool::new(false);
static CAPTURE_RESULT: Mutex<Option<(u32, u32, Vec<u8>)>> = Mutex::new(None);

/// Ask for the pixels of the next completed frame.
pub(crate) fn request_capture() {
    CAPTURE_REQUEST.store(true, Ordering::Relaxed);
}

/// Take the most recent capture (RGBA8888, physical pixels).
pub(crate) fn take_capture() -> Option<(u32, u32, Vec<u8>)> {
    CAPTURE_RESULT
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .take()
}

/// Consume a request at draw entry so an early return cannot leak it to another window.
pub(crate) fn take_capture_request() -> bool {
    CAPTURE_REQUEST.swap(false, Ordering::Relaxed)
}

/// Read a submitted Graphite surface back into the debug capture slot.
pub(crate) fn capture_surface(
    context: &mut skia_safe::gpu::graphite::Context,
    surface: &mut Surface,
) {
    let width = surface.width();
    let height = surface.height();
    if width <= 0 || height <= 0 {
        eprintln!("[capture] implausible surface size: {width}x{height}");
        return;
    }
    let info = ImageInfo::new(
        (width, height),
        ColorType::RGBA8888,
        AlphaType::Premul,
        None,
    );
    let row_bytes = info.min_row_bytes();
    let mut pixels = vec![0u8; row_bytes * height as usize];
    if !context.read_pixels(surface, &info, &mut pixels, row_bytes, (0, 0)) {
        eprintln!("[capture] Graphite read_pixels failed");
        return;
    }
    *CAPTURE_RESULT
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = Some((width as u32, height as u32, pixels));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_request_flag_is_consumed_once_and_the_result_survives_until_taken() {
        assert!(!take_capture_request());
        request_capture();
        request_capture();
        assert!(take_capture_request());
        assert!(!take_capture_request());
        assert!(take_capture().is_none());
        *CAPTURE_RESULT.lock().unwrap() = Some((1, 1, vec![9, 8, 7, 6]));
        assert_eq!(take_capture(), Some((1, 1, vec![9, 8, 7, 6])));
        assert!(take_capture().is_none());
    }

    #[test]
    #[ignore = "requires a wgpu adapter"]
    fn graphite_capture_reads_the_submitted_surface() {
        use skia_safe::gpu::graphite;
        use skia_safe::wgpu;
        use skia_safe::{Color, ColorType};

        let instance = wgpu::Instance::default();
        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("no wgpu adapter");
        let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("no wgpu device");
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("capture target"),
            size: wgpu::Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let mut context = graphite::wgpu::context_factory::make_wgpu(&device, &queue, None)
            .expect("no Graphite context");
        let mut recorder = context.make_recorder(None).expect("no Graphite recorder");
        let mut surface = graphite::wgpu::surfaces::wrap_texture(
            &mut recorder,
            &texture,
            ColorType::RGBA8888,
            None,
            None,
        )
        .expect("no Skia surface");
        surface.canvas().clear(Color::from_rgb(255, 0, 0));
        let mut recording = recorder.snap().expect("no recording");
        context.insert_recording(&graphite::InsertRecordingInfo::new(&mut recording));
        assert!(context.submit_and_wait());

        capture_surface(&mut context, &mut surface);
        assert_eq!(
            take_capture(),
            Some((4, 4, vec![255, 0, 0, 255].repeat(16)))
        );
    }

    fn block_on<F: Future>(future: F) -> F::Output {
        use std::future::Future;
        use std::pin::pin;
        use std::task::{Context, Poll, Waker};

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
}
