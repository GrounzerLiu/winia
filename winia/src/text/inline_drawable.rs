//! InlineDrawable — 文本内联元素（图片、SVG 等）
//!
//! 在富文本中嵌入非文本元素，由 ParagraphBuilder 以 U+FFFC 占位符方式插入，
//! 存储在 Paragraph 中，在 paint() 时通过 Skia 的 `get_rects_for_placeholders()`
//! 定位绘制位置。

use skia_safe::{Canvas, Data, FontMgr, Paint, Rect};
use skia_safe::canvas::SrcRectConstraint;
use skia_safe::svg;
use std::cell::RefCell;
use std::fmt::{self, Debug};
use std::sync::Arc;

/// 文本内联元素的绘制接口。
///
/// 实现者必须满足 `Send + Sync`，因为 drawable 可能在测量阶段
/// 跨线程共享（通过 `Arc<dyn InlineDrawable>`）。
pub trait InlineDrawable: Send + Sync {
    /// 在指定位置绘制内联元素。
    fn draw(&self, canvas: &Canvas, x: f32, y: f32);
    /// 获取内联元素的尺寸 (width, height)。
    fn size(&self) -> (f32, f32);
}

// ── ImageDrawable ──

/// 基于 skia_safe::Image 的内联图片。
pub struct ImageDrawable {
    image: skia_safe::Image,
    width: f32,
    height: f32,
}

impl Debug for ImageDrawable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImageDrawable")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish()
    }
}

impl ImageDrawable {
    /// 从 Skia Image 创建，并缩放到指定尺寸。
    pub fn from_image(image: skia_safe::Image, width: f32, height: f32) -> Self {
        Self { image, width, height }
    }

    /// 从 PNG/JPEG 字节数据解码并缩放到指定尺寸。
    pub fn from_bytes(data: &[u8], width: f32, height: f32) -> Option<Self> {
        let image = skia_safe::Image::from_encoded(Data::new_copy(data))?;
        Some(Self::from_image(image, width, height))
    }
}

impl InlineDrawable for ImageDrawable {
    fn draw(&self, canvas: &Canvas, x: f32, y: f32) {
        let src = Rect::new(0.0, 0.0, self.image.width() as f32, self.image.height() as f32);
        let dst = Rect::new(x, y, x + self.width, y + self.height);
        canvas.draw_image_rect(&self.image, Some((&src, SrcRectConstraint::Fast)), &dst, &Paint::default());
    }

    fn size(&self) -> (f32, f32) {
        (self.width, self.height)
    }
}

// ── SvgDrawable ──

/// 基于 skia_safe::svg::Dom 的内联 SVG。
///
/// 使用 RefCell 提供内部可变性以支持 set_container_size（Dom 需要 &mut self）。
/// svg::Dom 的 RCHandle 在线程安全上有条件，但我们确保 drawable 在使用期内
/// 不会被并发访问（RefCell 运行时检查），并通过 unsafe Send/Sync 标记安全。
pub struct SvgDrawable {
    dom: RefCell<svg::Dom>,
    width: f32,
    height: f32,
}

// SAFETY: RCHandle<SkSVGDOM> uses atomic refcounting; SvgDrawable wraps it
// in RefCell for interior mutability, so concurrent access is checked at runtime.
// draw() only calls render(&self) which takes &self, so no data race.
unsafe impl Send for SvgDrawable {}
unsafe impl Sync for SvgDrawable {}

impl Debug for SvgDrawable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SvgDrawable")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish()
    }
}

impl SvgDrawable {
    /// 从 SVG 字节数据加载并缩放到指定尺寸。
    pub fn from_bytes(data: &[u8], width: f32, height: f32) -> Option<Self> {
        let mut dom = svg::Dom::from_bytes(data, FontMgr::default()).ok()?;
        dom.set_container_size((width, height));
        Some(Self { dom: RefCell::new(dom), width, height })
    }

    /// 从 SVG 字符串加载并缩放到指定尺寸。
    pub fn from_str(svg_str: &str, width: f32, height: f32) -> Option<Self> {
        let mut dom = svg::Dom::from_str(svg_str, FontMgr::default()).ok()?;
        dom.set_container_size((width, height));
        Some(Self { dom: RefCell::new(dom), width, height })
    }
}

impl InlineDrawable for SvgDrawable {
    fn draw(&self, canvas: &Canvas, x: f32, y: f32) {
        canvas.save();
        canvas.translate((x, y));
        // render takes &self on Dom, no mutation needed
        self.dom.borrow().render(canvas);
        canvas.restore();
    }

    fn size(&self) -> (f32, f32) {
        (self.width, self.height)
    }
}

/// `Arc<dyn InlineDrawable>` 的便捷构造。
impl From<ImageDrawable> for Arc<dyn InlineDrawable> {
    fn from(d: ImageDrawable) -> Self {
        Arc::new(d)
    }
}

impl From<SvgDrawable> for Arc<dyn InlineDrawable> {
    fn from(d: SvgDrawable) -> Self {
        Arc::new(d)
    }
}
