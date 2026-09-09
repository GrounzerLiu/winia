//! Image 组件演示——固有尺寸布局 / ContentScale / 对齐 / alpha（对齐 Compose foundation Image）
//!
//! 展示：
//! - 固有尺寸（无 modifier——按位图像素尺寸布局）
//! - 固定尺寸 + ContentScale：Fit / Crop / Inside / None / FillWidth / FillHeight
//! - 对齐：TopStart / Center / BottomEnd
//! - alpha 透明度
//! - colorFilter：Tint 染色 / Matrix 灰度（对标 Compose ColorFilter）
//! - filterQuality：None 最近邻 vs Low 双线性放大
//! - SVG 来源完整缩放（与位图统一 content_scale_rect）
//! - SVG 文件来源（复用 IconSource 解码）

use winia::prelude::*;

const SAMPLE_PNG: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/assets/sample.png");
const HOME_SVG: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/assets/home.svg");

/// 灰度颜色矩阵（亮度加权——`ColorMatrix.setToSaturation(0f)` 等效）
const GRAY_MATRIX: [f32; 20] = [
    0.2126, 0.7152, 0.0722, 0.0, 0.0,
    0.2126, 0.7152, 0.0722, 0.0, 0.0,
    0.2126, 0.7152, 0.0722, 0.0, 0.0,
    0.0, 0.0, 0.0, 1.0, 0.0,
];

#[composable]
fn section_title(ctx: &mut ComposeCtx, text: &str) {
    Text::new(text)
        .font_size(14.0)
        .color(Color::from_argb(255, 90, 90, 90))
        .modifier(Modifier::new().padding_top(14.0).padding_bottom(6.0))
        .build(ctx);
}

/// 固定 120x90 边框内展示图片（每行一个 scale/对齐）
#[composable]
fn scale_demo_row(ctx: &mut ComposeCtx, label: &str, scale: ContentScale, alignment: ImageAlignment) {
    Row::new()
        .modifier(Modifier::new().padding_vertical(3.0))
        .build(ctx, |ctx| {
            Text::new(label)
                .font_size(13.0)
                .modifier(Modifier::new().width(120.0))
                .build(ctx);
            Image::file(SAMPLE_PNG)
                .modifier(Modifier::new().size(120.0, 90.0).border(1.0, Color::from_argb(255, 180, 180, 180), Shape::Rectangle))
                .content_scale(scale)
                .alignment(alignment)
                .build(ctx);
        });
}

#[composable]
fn image_demo(ctx: &mut ComposeCtx) {
    let scroll_y = ctx.remember(|| ScrollState::new()).get();

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0).vertical_scroll(scroll_y))
        .spacing(8.0)
        .build(ctx, |ctx| {
            Text::new("Image 组件演示（对齐 Compose foundation Image）")
                .font_size(20.0)
                .build(ctx);

            // ── 固有尺寸 ──
            section_title(ctx, "固有尺寸（无 modifier——按位图像素布局）");
            Image::file(SAMPLE_PNG).build(ctx);

            // ── ContentScale（固定 120x90 边框）──
            section_title(ctx, "ContentScale（120x90 边框内）");
            scale_demo_row(ctx, "Fit（默认）", ContentScale::Fit, ImageAlignment::Center);
            scale_demo_row(ctx, "Crop", ContentScale::Crop, ImageAlignment::Center);
            scale_demo_row(ctx, "Inside", ContentScale::Inside, ImageAlignment::Center);
            scale_demo_row(ctx, "None（拉伸）", ContentScale::None, ImageAlignment::Center);
            scale_demo_row(ctx, "FillWidth", ContentScale::FillWidth, ImageAlignment::Center);
            scale_demo_row(ctx, "FillHeight", ContentScale::FillHeight, ImageAlignment::Center);

            // ── 对齐 ──
            section_title(ctx, "对齐（Fit 内容在框内）");
            scale_demo_row(ctx, "TopStart", ContentScale::Fit, ImageAlignment::TopStart);
            scale_demo_row(ctx, "Center", ContentScale::Fit, ImageAlignment::Center);
            scale_demo_row(ctx, "BottomEnd", ContentScale::Fit, ImageAlignment::BottomEnd);

            // ── alpha ──
            section_title(ctx, "alpha（透明叠加）");
            Row::new()
                .modifier(Modifier::new().padding_vertical(3.0))
                .spacing(8.0)
                .build(ctx, |ctx| {
                    Image::file(SAMPLE_PNG).modifier(Modifier::new().size(64.0, 64.0)).alpha(1.0).build(ctx);
                    Image::file(SAMPLE_PNG).modifier(Modifier::new().size(64.0, 64.0)).alpha(0.6).build(ctx);
                    Image::file(SAMPLE_PNG).modifier(Modifier::new().size(64.0, 64.0)).alpha(0.3).build(ctx);
                });

            // ── colorFilter ──
            section_title(ctx, "colorFilter（Tint 染色 / Matrix 灰度）");
            Row::new()
                .modifier(Modifier::new().padding_vertical(3.0))
                .spacing(8.0)
                .build(ctx, |ctx| {
                    Image::file(SAMPLE_PNG).modifier(Modifier::new().size(64.0, 64.0)).build(ctx);
                    Image::file(SAMPLE_PNG)
                        .modifier(Modifier::new().size(64.0, 64.0))
                        .color_filter(ColorFilter::Tint {
                            color: Color::from_argb(255, 220, 60, 60),
                            blend_mode: BlendMode::SrcIn,
                        })
                        .build(ctx);
                    Image::file(SAMPLE_PNG)
                        .modifier(Modifier::new().size(64.0, 64.0))
                        .color_filter(ColorFilter::Matrix(GRAY_MATRIX))
                        .build(ctx);
                });

            // ── filterQuality ──
            section_title(ctx, "filterQuality（None 最近邻 / Low 双线性放大）");
            Row::new()
                .modifier(Modifier::new().padding_vertical(3.0))
                .spacing(8.0)
                .build(ctx, |ctx| {
                    Image::file(SAMPLE_PNG)
                        .modifier(Modifier::new().size(128.0, 128.0))
                        .filter_quality(FilterQuality::None)
                        .build(ctx);
                    Image::file(SAMPLE_PNG)
                        .modifier(Modifier::new().size(128.0, 128.0))
                        .filter_quality(FilterQuality::Low)
                        .build(ctx);
                });

            // ── SVG 完整缩放（与位图统一）──
            section_title(ctx, "SVG 缩放（Fit / Crop / FillWidth）");
            Row::new()
                .modifier(Modifier::new().padding_vertical(3.0))
                .spacing(8.0)
                .build(ctx, |ctx| {
                    Image::file(HOME_SVG).modifier(Modifier::new().size(64.0, 64.0)).content_scale(ContentScale::Fit).build(ctx);
                    Image::file(HOME_SVG).modifier(Modifier::new().size(64.0, 64.0)).content_scale(ContentScale::Crop).build(ctx);
                    Image::file(HOME_SVG).modifier(Modifier::new().size(64.0, 64.0)).content_scale(ContentScale::FillWidth).build(ctx);
                });

            // ── SVG 文件来源 ──
            section_title(ctx, "SVG 文件来源（IconSource 解码）");
            Image::file(HOME_SVG)
                .modifier(Modifier::new().size(64.0, 64.0))
                .build(ctx);
        });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::light(ctx, |ctx| {
            Window::new()
                .size(520.0, 720.0)
                .title("Image Demo")
                .build(ctx, |ctx| { image_demo(ctx); });
        });
    });
}
