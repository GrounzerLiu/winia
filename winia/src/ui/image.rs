//! Image 组件 — 对齐 Compose foundation `Image`
//!
//! 布局按图片固有尺寸（位图像素尺寸 / SVG viewBox），`Modifier` 可覆盖
//! （size / fillMaxWidth 等——与 Compose 语义一致：未指定维度以固有尺寸为基准）。
//! 绘制支持 `ContentScale`（Fit 默认）缩放 + 9 向对齐 + alpha；
//! 来源复用 `IconSource`（文件位图 png/jpg/webp/bmp + SVG 文档，统一解码缓存）。
//!
//! 差距（对标 Compose foundation Image）：
//! - `contentDescription`：winia 无 semantics 树（全框架缺口），参数保留预留；
//! - SVG 来源与位图统一走 `content_scale_rect`（完整缩放/对齐/RTL + clipToBounds）。

use crate::composable;
use crate::core::composer::ComposeCtx;
use crate::modifier::{ColorFilter, FilterQuality, Modifier, ModifierElement};
use crate::ui::icon::IconSource;
use skia_safe::Rect;

/// 内容缩放模式（对标 Compose `ContentScale`）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentScale {
    /// 原尺寸（不缩放——对标 ContentScale.None；超出 bounds 的部分由裁剪决定）
    None,
    /// 完整放入 bounds（保持比例——默认，对标 ContentScale.Fit）
    Fit,
    /// 覆盖 bounds（保持比例，超出部分裁剪——对标 ContentScale.Crop）
    Crop,
    /// 保持比例且不超过 bounds（缩小不放大——对标 ContentScale.Inside）
    Inside,
    /// 宽填满 bounds，高按比例（可超出——对标 ContentScale.FillWidth）
    FillWidth,
    /// 高填满 bounds，宽按比例（可超出——对标 ContentScale.FillHeight）
    FillHeight,
    /// 两轴独立拉伸到 bounds（不保持比例——对标 ContentScale.FillBounds）。
    /// NOTE: this is what winia's `None` used to be before it was aligned with Compose;
    /// the shared-element flight path needs the explicit member, because FillBounds is the
    /// behaviour it has to be able to reproduce.
    FillBounds,
}

impl Default for ContentScale {
    fn default() -> Self { Self::Fit }
}

impl ContentScale {
    /// The `(sx, sy)` this mode applies from a content size to a bounds size — the single
    /// source of truth for the scaling maths. `content_scale_rect` (Image) and the
    /// shared-element flight path both go through it.
    pub(crate) fn scale_factors(self, content: (f32, f32), bounds: (f32, f32)) -> (f32, f32) {
        let ((cw, ch), (bw, bh)) = (content, bounds);
        if cw <= 0.0 || ch <= 0.0 || bw <= 0.0 || bh <= 0.0 {
            return (1.0, 1.0);
        }
        let (rx, ry) = (bw / cw, bh / ch);
        match self {
            ContentScale::None => (1.0, 1.0),
            ContentScale::FillBounds => (rx, ry),
            ContentScale::FillWidth => (rx, rx),
            ContentScale::FillHeight => (ry, ry),
            ContentScale::Fit => (rx.min(ry), rx.min(ry)),
            ContentScale::Crop => (rx.max(ry), rx.max(ry)),
            ContentScale::Inside => {
                let s = rx.min(ry).min(1.0);
                (s, s)
            }
        }
    }

    /// Top-left offset of the scaled content inside the bounds, from the alignment.
    pub(crate) fn align_offset(
        alignment: ImageAlignment,
        scaled: (f32, f32),
        bounds: (f32, f32),
        rtl: bool,
    ) -> (f32, f32) {
        let ((sw, sh), (bw, bh)) = (scaled, bounds);
        let fx = match alignment {
            ImageAlignment::TopStart
            | ImageAlignment::CenterStart
            | ImageAlignment::BottomStart => {
                if rtl { 1.0 } else { 0.0 }
            }
            ImageAlignment::TopCenter
            | ImageAlignment::Center
            | ImageAlignment::BottomCenter => 0.5,
            _ => {
                if rtl { 0.0 } else { 1.0 }
            }
        };
        let fy = match alignment {
            ImageAlignment::TopStart | ImageAlignment::TopCenter | ImageAlignment::TopEnd => 0.0,
            ImageAlignment::CenterStart | ImageAlignment::Center | ImageAlignment::CenterEnd => 0.5,
            _ => 1.0,
        };
        ((bw - sw) * fx, (bh - sh) * fy)
    }
}

/// 图片内容在 bounds 内的对齐（对标 Compose `Alignment` 9 值；
/// Start/End 随布局方向镜像——RTL 下 Start 在右）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageAlignment {
    TopStart,
    TopCenter,
    TopEnd,
    CenterStart,
    Center,
    CenterEnd,
    BottomStart,
    BottomCenter,
    BottomEnd,
}

impl Default for ImageAlignment {
    fn default() -> Self { Self::Center }
}

/// 计算内容在 bounds 内的目标矩形（ContentScale 缩放 + 对齐偏移）。
/// 渲染与测量共用——单一事实来源。
pub(crate) fn content_scale_rect(
    scale: ContentScale,
    rect: Rect,
    iw: f32,
    ih: f32,
    alignment: ImageAlignment,
    rtl: bool,
) -> Rect {
    if iw <= 0.0 || ih <= 0.0 {
        return rect;
    }
    // 缩放：走 `ContentScale::scale_factors` 这一处唯一事实来源（Image 与共享元素飞行共用）
    let (fsx, fsy) = scale.scale_factors((iw, ih), (rect.width(), rect.height()));
    let (w, h) = (iw * fsx, ih * fsy);
    // 对齐偏移（0.0/0.5/1.0；RTL 时 Start↔End 镜像）
    let sx = match alignment {
        ImageAlignment::TopStart | ImageAlignment::CenterStart | ImageAlignment::BottomStart => {
            if rtl { 1.0 } else { 0.0 }
        }
        ImageAlignment::TopCenter | ImageAlignment::Center | ImageAlignment::BottomCenter => 0.5,
        ImageAlignment::TopEnd | ImageAlignment::CenterEnd | ImageAlignment::BottomEnd => {
            if rtl { 0.0 } else { 1.0 }
        }
    };
    let sy = match alignment {
        ImageAlignment::TopStart | ImageAlignment::TopCenter | ImageAlignment::TopEnd => 0.0,
        ImageAlignment::CenterStart | ImageAlignment::Center | ImageAlignment::CenterEnd => 0.5,
        ImageAlignment::BottomStart | ImageAlignment::BottomCenter | ImageAlignment::BottomEnd => 1.0,
    };
    Rect::from_xywh(
        rect.left + (rect.width() - w) * sx,
        rect.top + (rect.height() - h) * sy,
        w,
        h,
    )
}

/// 图片组件（对标 Compose foundation `Image`）。
///
/// ```ignore
/// Image::file("assets/sample.png")
///     .modifier(Modifier::new().width(200.0))
///     .content_scale(ContentScale::Crop)
///     .build(ctx);
/// ```
pub struct Image {
    source: IconSource,
    modifier: Modifier,
    alignment: ImageAlignment,
    content_scale: ContentScale,
    alpha: f32,
    color_filter: Option<ColorFilter>,
    filter_quality: FilterQuality,
    /// a11y 描述（winia 无 semantics 树——预留；不影响渲染）
    content_description: Option<String>,
}

impl Image {
    /// 从图标源创建（位图文件 / SVG 文档——复用 IconSource 解码缓存）
    pub fn new(source: IconSource) -> Self {
        Image {
            source,
            modifier: Modifier::new(),
            alignment: ImageAlignment::Center,
            content_scale: ContentScale::Fit,
            alpha: 1.0,
            color_filter: None,
            filter_quality: FilterQuality::Low,
            content_description: None,
        }
    }

    /// 从图片文件创建（png/jpg/jpeg/webp/bmp/svg，按扩展名解码）
    pub fn file(path: impl Into<std::sync::Arc<str>>) -> Self {
        Self::new(IconSource::file(path))
    }

    /// 从 SVG 文档字符串创建
    pub fn svg(data: impl Into<std::sync::Arc<str>>) -> Self {
        Self::new(IconSource::svg(data))
    }

    /// 修饰符链（尺寸/裁剪等——覆盖固有尺寸布局）
    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = modifier;
        self
    }

    /// 内容对齐（默认 Center——对标 Compose `alignment`）
    pub fn alignment(mut self, alignment: ImageAlignment) -> Self {
        self.alignment = alignment;
        self
    }

    /// 缩放模式（默认 Fit——对标 Compose `contentScale`）
    pub fn content_scale(mut self, scale: ContentScale) -> Self {
        self.content_scale = scale;
        self
    }

    /// 整体透明度（默认 1.0——对标 Compose `alpha`）
    pub fn alpha(mut self, alpha: f32) -> Self {
        self.alpha = alpha;
        self
    }

    /// 颜色滤镜（默认无——对标 Compose `colorFilter`：Tint 染色/
    /// Matrix 矩阵/Lighting 光照）
    pub fn color_filter(mut self, filter: ColorFilter) -> Self {
        self.color_filter = Some(filter);
        self
    }

    /// 位图采样质量（默认 Low 双线性——对标 Compose `filterQuality`）
    pub fn filter_quality(mut self, quality: FilterQuality) -> Self {
        self.filter_quality = quality;
        self
    }

    /// a11y 描述（winia 无 semantics 树——预留参数，不影响渲染）
    pub fn content_description(mut self, desc: impl Into<String>) -> Self {
        self.content_description = Some(desc.into());
        self
    }

    /// 构建图片节点（叶子——布局按固有尺寸，绘制经 ImageContent modifier）
    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        // 参数暂存（source 变化 → 重测；content_scale/alignment/alpha/color_filter/
        // filter_quality 仅影响绘制——渲染每帧全量执行，无需声明 changed）
        ctx.changed(&self.source);
        let key = ctx.next_key();
        let modifier = self.modifier.image_content(
            self.source,
            self.content_scale,
            self.alignment,
            self.alpha,
            self.color_filter,
            self.filter_quality,
        );
        ctx.start_leaf(key, modifier);
        ctx.end_node();
    }

    // ── Getters（测试用）──
    pub fn get_source(&self) -> &IconSource { &self.source }
    pub fn get_content_scale(&self) -> ContentScale { self.content_scale }
    pub fn get_alignment(&self) -> ImageAlignment { self.alignment }
    pub fn get_alpha(&self) -> f32 { self.alpha }
    pub fn get_color_filter(&self) -> Option<&ColorFilter> { self.color_filter.as_ref() }
    pub fn get_filter_quality(&self) -> FilterQuality { self.filter_quality }
    pub fn get_modifier(&self) -> &Modifier { &self.modifier }
}

// ═══════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_scale_fit_keeps_aspect() {
        // bounds 200x100、图 100x60（1.67:1）→ Fit：s=min(2, 1.67)=1.67 → 166.7x100 居中
        let r = content_scale_rect(ContentScale::Fit, Rect::from_xywh(0.0, 0.0, 200.0, 100.0), 100.0, 60.0, ImageAlignment::Center, false);
        assert!((r.width() - 166.7).abs() < 0.1, "宽 {}", r.width());
        assert!((r.height() - 100.0).abs() < 0.01, "高 {}", r.height());
        assert!((r.left - 16.65).abs() < 0.1, "x {}", r.left);
        assert!((r.top - 0.0).abs() < 0.01, "y {}", r.top);
    }

    #[test]
    fn test_content_scale_crop_covers_bounds() {
        // bounds 200x100、图 100x100 → Crop：scale=max(2,1)=2 → 200x200，裁剪上下
        let r = content_scale_rect(ContentScale::Crop, Rect::from_xywh(0.0, 0.0, 200.0, 100.0), 100.0, 100.0, ImageAlignment::Center, false);
        assert_eq!((r.width(), r.height()), (200.0, 200.0));
        assert_eq!((r.left, r.top), (0.0, -50.0), "超出部分在上下");
    }

    #[test]
    fn test_content_scale_inside_never_upscales() {
        // bounds 200x100、图 100x50 → Inside：100x50（不放大）
        let r = content_scale_rect(ContentScale::Inside, Rect::from_xywh(0.0, 0.0, 200.0, 100.0), 100.0, 50.0, ImageAlignment::TopStart, false);
        assert_eq!((r.width(), r.height()), (100.0, 50.0));
        // 大图 400x200 → Inside：缩小到 200x100
        let r2 = content_scale_rect(ContentScale::Inside, Rect::from_xywh(0.0, 0.0, 200.0, 100.0), 400.0, 200.0, ImageAlignment::Center, false);
        assert_eq!((r2.width(), r2.height()), (200.0, 100.0));
    }

    #[test]
    fn test_content_scale_fill_width_height() {
        // 非对称 bounds 200x80（区分两分支）：
        // FillWidth：宽 200，高按比例（100x50 → 200x100，超出高）
        let r = content_scale_rect(ContentScale::FillWidth, Rect::from_xywh(0.0, 0.0, 200.0, 80.0), 100.0, 50.0, ImageAlignment::TopStart, false);
        assert_eq!((r.width(), r.height()), (200.0, 100.0));
        // FillHeight：高 80，宽按比例（100x50 → 160x80）
        let r2 = content_scale_rect(ContentScale::FillHeight, Rect::from_xywh(0.0, 0.0, 200.0, 80.0), 100.0, 50.0, ImageAlignment::TopStart, false);
        assert_eq!((r2.width(), r2.height()), (160.0, 80.0));
    }

    /// `None` follows Compose: the source is NOT scaled (its intrinsic size), which is what
    /// `ContentScale.None` means there. winia used to stretch to the bounds under this name,
    /// i.e. it implemented Compose's `FillBounds` instead — that behaviour now lives in the
    /// explicit `FillBounds` member (it is what the shared-element flight path has to be
    /// able to reproduce). Composer's alignment still positions the unscaled content.
    #[test]
    fn test_content_scale_none_keeps_the_intrinsic_size() {
        let r = content_scale_rect(ContentScale::None, Rect::from_xywh(0.0, 0.0, 200.0, 100.0), 100.0, 50.0, ImageAlignment::TopStart, false);
        assert_eq!((r.width(), r.height()), (100.0, 50.0), "None must not scale");
        let c = content_scale_rect(ContentScale::None, Rect::from_xywh(0.0, 0.0, 200.0, 100.0), 100.0, 50.0, ImageAlignment::Center, false);
        assert_eq!((c.left, c.top), (50.0, 25.0), "alignment still places it");
        // The old winia behaviour is still reachable, under Compose's real name.
        let f = content_scale_rect(ContentScale::FillBounds, Rect::from_xywh(0.0, 0.0, 200.0, 100.0), 100.0, 50.0, ImageAlignment::TopStart, false);
        assert_eq!((f.width(), f.height()), (200.0, 100.0), "FillBounds stretches");
    }

    #[test]
    fn test_alignment_positions_within_bounds() {
        let bounds = Rect::from_xywh(0.0, 0.0, 200.0, 100.0);
        // Fit 图 100x60 → 167x100：TopStart → (0,0)；BottomEnd → (33,0)
        let tl = content_scale_rect(ContentScale::Fit, bounds, 100.0, 60.0, ImageAlignment::TopStart, false);
        assert_eq!((tl.left, tl.top), (0.0, 0.0));
        let br = content_scale_rect(ContentScale::Fit, bounds, 100.0, 60.0, ImageAlignment::BottomEnd, false);
        assert!((br.left - 33.3).abs() < 0.1, "x {}", br.left);
        assert_eq!(br.top, 0.0, "高已填满，y=0");
        // RTL：TopStart 镜像到右侧
        let rtl = content_scale_rect(ContentScale::Fit, bounds, 100.0, 60.0, ImageAlignment::TopStart, true);
        assert!((rtl.left - 33.3).abs() < 0.1, "RTL x {}", rtl.left);
        assert_eq!(rtl.top, 0.0);
    }

    #[test]
    fn test_image_builder_defaults() {
        let img = Image::file("assets/sample.png");
        assert_eq!(img.get_content_scale(), ContentScale::Fit);
        assert_eq!(img.get_alignment(), ImageAlignment::Center);
        assert_eq!(img.get_alpha(), 1.0);
        assert_eq!(img.get_color_filter(), None, "默认无滤镜");
        assert_eq!(img.get_filter_quality(), FilterQuality::Low, "默认双线性");
        assert!(matches!(img.get_source(), IconSource::File(_)));
        let img2 = Image::svg("<svg viewBox=\"0 0 24 24\"/>");
        assert!(matches!(img2.get_source(), IconSource::Svg(_)));
    }

    #[test]
    fn test_image_builder_color_filter_and_quality() {
        use crate::modifier::{BlendMode, Color};
        let img = Image::file("a.png")
            .color_filter(ColorFilter::Tint { color: Color::RED, blend_mode: BlendMode::SrcIn })
            .filter_quality(FilterQuality::High);
        assert!(matches!(img.get_color_filter(), Some(ColorFilter::Tint { color: c, blend_mode: b }) if *c == Color::RED && *b == BlendMode::SrcIn));
        assert_eq!(img.get_filter_quality(), FilterQuality::High);

        let gray = ColorFilter::Matrix([
            0.2126, 0.7152, 0.0722, 0.0, 0.0,
            0.2126, 0.7152, 0.0722, 0.0, 0.0,
            0.2126, 0.7152, 0.0722, 0.0, 0.0,
            0.0, 0.0, 0.0, 1.0, 0.0,
        ]);
        let img2 = Image::file("a.png").color_filter(gray.clone());
        assert_eq!(img2.get_color_filter(), Some(&gray));
        assert_ne!(img.get_color_filter(), Some(&gray), "Tint 与 Matrix 是不同滤镜");
    }
}
