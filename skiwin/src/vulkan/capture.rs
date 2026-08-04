//! 调试截图捕获：在 `flush_and_submit` **之后**读回 swapchain 图像——
//! 保证读到的是真实呈现的帧（draw closure 内的读回在 flush 前，会读到陈旧/部分内容）。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

static CAPTURE_REQUEST: AtomicBool = AtomicBool::new(false);
static CAPTURE_RESULT: Mutex<Option<(u32, u32, Vec<u8>)>> = Mutex::new(None);

/// 请求下一帧绘制完成后捕获像素。
pub fn request_capture() {
    CAPTURE_REQUEST.store(true, Ordering::Relaxed);
}

/// 取走最近一次捕获的像素（RGBA8888，尺寸为物理像素）。无捕获时返回 None。
pub fn take_capture() -> Option<(u32, u32, Vec<u8>)> {
    CAPTURE_RESULT.lock().unwrap().take()
}

/// draw 入口消费捕获请求（提前返回路径也消费——避免 flag 残留被
/// 其他窗口/下一帧的 draw 消费造成跨窗口串扰）。
pub(crate) fn take_capture_request() -> bool {
    CAPTURE_REQUEST.swap(false, Ordering::Relaxed)
}

/// draw 内部（flush 后）调用：读回图像并保存。仅当 take_capture_request 返回 true。
pub(crate) fn capture_surface(surface: &mut skia_safe::Surface) {
    let w = surface.width();
    let h = surface.height();
    if w <= 0 || h <= 0 {
        eprintln!("[capture] surface 尺寸异常: {}x{}", w, h);
        return;
    }
    let info = skia_safe::ImageInfo::new(
        (w, h),
        skia_safe::ColorType::RGBA8888,
        skia_safe::AlphaType::Premul,
        None,
    );
    let mut pixels = vec![0u8; (w * h * 4) as usize];
    if surface.read_pixels(&info, &mut pixels, w as usize * 4, (0, 0)) {
        *CAPTURE_RESULT.lock().unwrap() = Some((w as u32, h as u32, pixels));
    } else {
        eprintln!("[capture] read_pixels 失败");
    }
}
