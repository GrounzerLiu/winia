//! 渲染管线 — 遍历 LayoutNode 树绘制到 Skia Canvas
//!
//! 单阶段深度遍历：节点级修饰符（阴影/背景/边框/文本/子节点）按序绘制；
//! BackdropBlur 在节点自身内容绘制前即时 snapshot→blur→画回（见
//! `draw_backdrop_blur`）——语义对齐 Compose：只模糊"位于其下"的内容。

use crate::debug_log;
use crate::layout::LayoutDirection;
use crate::layout::node::LayoutNode;
use crate::modifier::ModifierElement;
use crate::ui::icon::{DecodedIcon, IconSource, IconSpec, decoded_icon};
use skia_safe::{BlendMode, Canvas, Color4f, IRect, Paint, RRect, Rect, SamplingOptions};
use skia_safe::sampling_options::{FilterMode, MipmapMode};
use skia_safe::image_filters;
use skia_safe::svg;
use std::cell::RefCell;

// ── 入口 ──

pub fn render(nodes: &[LayoutNode], root_idx: usize, canvas: &Canvas) {
    render_pass1(nodes, root_idx, root_idx, canvas, 0.0, 0.0);
}

// ── 视觉 Modifier 渲染（Background / Border / TextContent）──

struct TextParams<'a> {
    content: &'a str,
    font_size: f32,
    color: &'a crate::modifier::Color,
    font_weight: crate::ui::text::FontWeight,
    font_style: crate::ui::text::FontSlant,
    max_lines: usize,
    align: crate::ui::TextAlign,
    overflow: crate::ui::TextOverflow,
    soft_wrap: bool,
    letter_spacing: f32,
    line_height: Option<f32>,
}

/// 单层阴影绘制——严格对齐 Compose `DropShadowPainter`：
/// 1. Alpha8 离屏画布（扩边 = radius×2 + spread×2——Compose outset）
/// 2. 画**黑色**形状（BlurMaskFilter(radius) 近似：sigma = radius×0.3535，
///    Skia kBlurRadiusToSigma=1/√8；spread>0 时 Fill + Stroke 两遍）
///    ——Alpha8 画布只写 alpha（黑形状 → alpha 形状）
/// 3. drawImage 带 paint：colorFilter = Blend(color, SrcIn)（Compose
///    ColorFilter.tint 等价——保留 alpha 形状染成 color）+ alpha（整体透明度）
/// 4. 平移 offset（Compose onDrawShadow：offset = -(radius+spread)）
/// graphicsLayer 变换序列：transformOrigin（枢轴=节点绝对位置+偏移）
/// → 平移 → 2D/3D 变换 → 平移回。阴影绘制与内容绘制共用。
fn apply_gl_transform(
    canvas: &Canvas,
    gl: &crate::modifier::GraphicsLayerParams,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) {
    // 枢轴 = 节点绝对位置 + transformOrigin 偏移（绘制用绝对坐标——
    // 只平移 (px,py) 会让旋转中心落在相对窗口原点的位置）
    let (px, py) = (w * gl.transform_origin.0, h * gl.transform_origin.1);
    let (pivot_x, pivot_y) = (x + px, y + py);
    canvas.translate((gl.translation_x, gl.translation_y));
    canvas.translate((pivot_x, pivot_y));
    // 3D 旋转（rotationX/Y + cameraDistance 透视）：整体拼成 4x4 矩阵
    // （scale → rotationZ → rotationY → rotationX → 相机透视）
    match build_gl_3d_matrix(gl, w, h) {
        Some(m) => { canvas.concat_44(&m); }
        None => {
            canvas.scale((gl.scale_x, gl.scale_y));
            canvas.rotate(gl.rotation_z, None);
        }
    }
    canvas.translate((-pivot_x, -pivot_y));
}

/// graphicsLayer 3D 变换矩阵（rotationX/Y + cameraDistance 透视）。
/// 顺序：scale → rotationZ → rotationY → rotationX → 相机透视
/// （矩阵左乘序与 Compose RenderNode 一致；调用方负责先平移到 pivot）。
/// 无 3D 旋转时返回 None（走 2D 路径）。
fn build_gl_3d_matrix(
    gl: &crate::modifier::GraphicsLayerParams,
    w: f32,
    h: f32,
) -> Option<skia_safe::M44> {
    if gl.rotation_x == 0.0 && gl.rotation_y == 0.0 {
        return None;
    }
    // 相机透视投影必须**左乘**到组合矩阵（P·Rx·Ry·Rz·S）：
    // 直接 set_rc(3,2) 只改 w 行 z 列，2D 点 z 恒为 0 → 透视永远不参与
    // （相机距离无效的根因）。左乘后 w 行自动折叠出 -sinθ/cam 等系数。
    // 有效相机距离下限 = max(视图宽, 高)（Compose 文档：cameraDistance
    // 应 ≥ 视图尺寸）：默认 8 对 170dp 卡片太小，近边 z' 会超过相机 →
    // w 变负 → 卡片“飞走”。半尺寸在 rotationX+rotationY 组合大角度下
    // 仍可能越界（z' ≤ h/2 + w/2），用 max(w,h) 保证任意角度 w>0。
    // ⚠ 钳制按中心枢轴估算——自定义 transformOrigin 时边缘距离不同。
    // 低于下限的相机值统一表现为最大合理透视。
    let effective_cam = gl.camera_distance
        .max(w.max(h))
        .max(1.0);
    let mut persp = skia_safe::M44::new_identity();
    persp.set_rc(3, 2, -1.0 / effective_cam);
    let mut m = persp;
    if gl.rotation_z != 0.0 {
        m = skia_safe::M44::concat(&m, &skia_safe::M44::rotate(
            skia_safe::V3::new(0.0, 0.0, 1.0),
            gl.rotation_z.to_radians(),
        ));
    }
    if gl.rotation_y != 0.0 {
        m = skia_safe::M44::concat(&m, &skia_safe::M44::rotate(
            skia_safe::V3::new(0.0, 1.0, 0.0),
            gl.rotation_y.to_radians(),
        ));
    }
    if gl.rotation_x != 0.0 {
        m = skia_safe::M44::concat(&m, &skia_safe::M44::rotate(
            skia_safe::V3::new(1.0, 0.0, 0.0),
            gl.rotation_x.to_radians(),
        ));
    }
    // 缩放最内层（先缩放后旋转——Compose 变换序）
    let mut s = skia_safe::M44::new_identity();
    s.set_scale(gl.scale_x, gl.scale_y, 1.0);
    m = skia_safe::M44::concat(&m, &s);
    Some(m)
}

/// GraphicsLayer shadow：使用 Skia `SkShadowUtils::draw_shadow` 绘制真正的
/// ambient + spot 阴影。elevation 是 Compose 语义的 Z 高度，不再手工换算
/// blur radius 或 spot offset；光源参数集中在这里，便于后续校准平台差异。
fn draw_elevation_shadow(
    canvas: &Canvas,
    rect: Rect,
    shape: &crate::modifier::Shape,
    elevation: f32,
    ambient_color: crate::modifier::Color,
    spot_color: crate::modifier::Color,
) {
    if elevation <= 0.0 || !elevation.is_finite() {
        return;
    }

    let path = shadow_path(rect, shape);
    let z_plane = (0.0, 0.0, elevation);
    let light_pos = (270.0, 0.0, 600.0);
    let light_radius = 800.0;
    skia_safe::utils::shadow_utils::draw_shadow(
        canvas,
        &path,
        z_plane,
        light_pos,
        light_radius,
        skia_color(ambient_color),
        skia_color(spot_color),
        None,
    );
}

fn shadow_path(rect: Rect, shape: &crate::modifier::Shape) -> skia_safe::Path {
    match shape {
        crate::modifier::Shape::Rectangle => skia_safe::Path::rect(rect, None),
        crate::modifier::Shape::RoundedRect { corner_radius } => {
            skia_safe::Path::rrect(RRect::new_rect_xy(rect, *corner_radius, *corner_radius), None)
        }
        crate::modifier::Shape::TopRoundedRect { radius } => {
            let rr = RRect::new_rect_radii(rect, &[
                skia_safe::Vector::new(*radius, *radius),
                skia_safe::Vector::new(*radius, *radius),
                skia_safe::Vector::new(0.0, 0.0),
                skia_safe::Vector::new(0.0, 0.0),
            ]);
            skia_safe::Path::rrect(rr, None)
        }
        crate::modifier::Shape::Pill => {
            let radius = rect.width().min(rect.height()) / 2.0;
            skia_safe::Path::rrect(RRect::new_rect_xy(rect, radius, radius), None)
        }
        crate::modifier::Shape::Circle => skia_safe::Path::circle(
            (rect.center_x(), rect.center_y()),
            rect.width().min(rect.height()) / 2.0,
            None,
        ),
    }
}


/// 单层阴影绘制——严格按参考实现（D:\winia 阴影绘制）：
/// 1. 离屏 surface = **内容尺寸**（不扩边）
/// 2. 白色形状（含 spread 外圈 stroke）画到离屏
/// 3. draw_paint(SrcIn) 染成阴影色（透明区域保持透明）
/// 4. 主画布 draw_image + ImageFilter::blur（**blur 在绘制时应用**——
///    不存在离屏边缘 clamp/裁切）+ CropRect 限定模糊输入范围
///    （内容 rect ± pad——blur 可正确溢出 image 边界）
/// 5. 偏移应用在 draw_image 位置（参考实现：y + shadow_offset）
fn draw_shadow_layer(
    canvas: &Canvas,
    rect: Rect,
    shape: &crate::modifier::Shape,
    params: &crate::modifier::ShadowParams,
) {
    use skia_safe::{BlendMode, Color4f, Paint, PaintStyle, surfaces};

    let w = rect.width();
    let h = rect.height();
    if w <= 0.0 || h <= 0.0 || (params.radius <= 0.0 && params.spread <= 0.0) {
        return;
    }
    let sigma = params.radius * 0.57735; // Skia ConvertRadiusToSigma：BlurMaskFilter(radius) → sigma = radius/√3
    // blur 输入范围扩边（CropRect 边界 = 内容 ± pad）——参考实现 e×6 同款
    let pad = params.radius * 2.0 + params.spread * 2.0;

    // 1. 离屏（内容尺寸）
    let Some(mut surface) = surfaces::raster_n32_premul((
        w.ceil().max(1.0) as i32,
        h.ceil().max(1.0) as i32,
    )) else {
        return;
    };
    let sc = surface.canvas();
    sc.clear(skia_safe::Color::TRANSPARENT);
    let local = Rect::new(0.0, 0.0, w, h);
    // 2. 白色形状
    let mut mask = Paint::default();
    mask.set_color(skia_safe::Color::WHITE);
    mask.set_anti_alias(true);
    match shape {
        crate::modifier::Shape::Rectangle => { sc.draw_rect(local, &mask); }
        crate::modifier::Shape::RoundedRect { corner_radius } => {
            sc.draw_rrect(RRect::new_rect_xy(local, *corner_radius, *corner_radius), &mask);
        }
        crate::modifier::Shape::TopRoundedRect { radius } => {
            let rr = RRect::new_rect_radii(local, &[
                skia_safe::Vector::new(*radius, *radius),
                skia_safe::Vector::new(*radius, *radius),
                skia_safe::Vector::new(0.0, 0.0),
                skia_safe::Vector::new(0.0, 0.0),
            ]);
            sc.draw_rrect(rr, &mask);
        }
        crate::modifier::Shape::Pill => {
            let r = local.width().min(local.height()) / 2.0;
            sc.draw_rrect(RRect::new_rect_xy(local, r, r), &mask);
        }
        crate::modifier::Shape::Circle => {
            sc.draw_circle((local.center_x(), local.center_y()), local.width().min(local.height()) / 2.0, &mask);
        }
    }
    // spread：外圈 stroke（Fill + Stroke 两遍——Compose createOuterShadowBitmap 同款）
    if params.spread > 0.0 {
        let mut stroke = Paint::default();
        stroke.set_color(skia_safe::Color::WHITE);
        stroke.set_anti_alias(true);
        stroke.set_style(PaintStyle::Stroke);
        stroke.set_stroke_width(params.spread * 2.0);
        match shape {
            crate::modifier::Shape::Rectangle => { sc.draw_rect(local, &stroke); }
            crate::modifier::Shape::RoundedRect { corner_radius } => {
                sc.draw_rrect(RRect::new_rect_xy(local, *corner_radius, *corner_radius), &stroke);
            }
            crate::modifier::Shape::TopRoundedRect { radius } => {
                let rr = RRect::new_rect_radii(local, &[
                    skia_safe::Vector::new(*radius, *radius),
                    skia_safe::Vector::new(*radius, *radius),
                    skia_safe::Vector::new(0.0, 0.0),
                    skia_safe::Vector::new(0.0, 0.0),
                ]);
                sc.draw_rrect(rr, &stroke);
            }
            crate::modifier::Shape::Pill => {
                let r = local.width().min(local.height()) / 2.0;
                sc.draw_rrect(RRect::new_rect_xy(local, r, r), &stroke);
            }
            crate::modifier::Shape::Circle => {
                sc.draw_circle((local.center_x(), local.center_y()), local.width().min(local.height()) / 2.0, &stroke);
            }
        }
    }
    // 3. SrcIn 染色（阴影色 × 形状 alpha——透明区保持透明）
    let a = params.color.a as f32 / 255.0 * params.alpha.clamp(0.0, 1.0);
    let mut tint = Paint::default();
    tint.set_color4f(Color4f::new(
        params.color.r as f32 / 255.0,
        params.color.g as f32 / 255.0,
        params.color.b as f32 / 255.0,
        a,
    ), None);
    tint.set_blend_mode(BlendMode::SrcIn);
    sc.draw_paint(&tint);
    let image = surface.image_snapshot();

    // 4. 主画布：draw_image（绝对位置，参考实现同款）+ blur（绘制时应用）
    //    + CropRect 限定模糊输入范围（绝对坐标——与 draw 同一空间）
    let mut blur_paint = Paint::default();
    if sigma > 0.0 {
        let crop = skia_safe::image_filters::CropRect::from(Rect::from_xywh(
            rect.x() + params.offset_x - pad,
            rect.y() + params.offset_y - pad,
            w + pad * 2.0,
            h + pad * 2.0,
        ));
        blur_paint.set_image_filter(image_filters::blur(
            (sigma, sigma),
            skia_safe::TileMode::Clamp,
            None,
            crop,
        ));
    }
    canvas.draw_image(
        &image,
        (rect.x() + params.offset_x, rect.y() + params.offset_y),
        Some(&blur_paint),
    );
}

/// 渲染 Background / Border / 提取 TextContent
fn render_modifier_element<'a>(
    canvas: &Canvas,
    el: &'a ModifierElement,
    rect: Rect,
    x: f32, y: f32, w: f32, h: f32,
    last_background: &mut Option<(crate::modifier::Color, crate::modifier::Shape)>,
) -> Option<TextParams<'a>> {
    match el {
        ModifierElement::Background { color_fn, shape } => {
            let color = (color_fn)();
            *last_background = Some((color, shape.clone()));
            draw_background(canvas, rect, &color, shape);
            None
        }
        ModifierElement::Border { width, color, shape } => {
            // 边框色与形状均等于容器时合并为纯填充（M3 drawBox 语义）：
            // 半透明色在填充上再叠一层 stroke 会双重混合，边框带明显深于内部
            if *last_background != Some((*color, shape.clone())) {
                draw_border(canvas, x, y, w, h, *width, color, shape);
            }
            None
        }
        ModifierElement::BorderDynamic { width, color_fn, shape } => {
            let color = (color_fn)();
            if *last_background != Some((color, shape.clone())) {
                draw_border(canvas, x, y, w, h, *width, &color, shape);
            }
            None
        }
        ModifierElement::TextContent { content, font_size, color, font_weight, font_style, max_lines, align, overflow, soft_wrap, letter_spacing, line_height } => {
            Some(TextParams {
                content, font_size: *font_size, color,
                font_weight: *font_weight, font_style: *font_style,
                max_lines: *max_lines, align: *align, overflow: *overflow,
                soft_wrap: *soft_wrap,
                letter_spacing: *letter_spacing, line_height: *line_height,
            })
        }
        // DrawIcon 由 render_pass1 主循环显式分支绘制（内容区域 rect——含 padding 偏移）
        _ => None,
    }
}

/// 图标绘制：SVG path / SVG 文档 / 图片文件 / 可变字体符号
fn draw_icon(canvas: &Canvas, rect: Rect, direction: LayoutDirection, spec: &IconSpec) {
    let mirror = spec.auto_mirror && direction == LayoutDirection::Rtl;
    if mirror {
        canvas.save();
        let c = rect.center();
        canvas.translate((c.x, c.y));
        canvas.scale((-1.0, 1.0));
        canvas.translate((-c.x, -c.y));
    }
    match &spec.source {
        IconSource::SvgPath { .. } | IconSource::Svg(_) | IconSource::File(_) => {
            if let Some(decoded) = decoded_icon(&spec.source) {
                match decoded.as_ref() {
                    DecodedIcon::Bitmap { image, width, height } => {
                        let dst = fit_rect(rect, *width, *height);
                        let src = Rect::new(0.0, 0.0, *width, *height);
                        let mut paint = Paint::default();
                        paint.set_anti_alias(true);
                        if let Some(tint) = spec.tint {
                            if let Some(filter) =
                                skia_safe::color_filters::blend(skia_color(tint), BlendMode::SrcIn)
                            {
                                paint.set_color_filter(filter);
                            }
                        }
                        // 位图缩放用线性 + 线性 mipmap（三线性）与 Strict 约束：
                        // 避免放大锯齿/缩小摩尔纹（默认 Fast + 低质量采样）
                        canvas.draw_image_rect_with_sampling_options(
                            image,
                            Some((&src, skia_safe::canvas::SrcRectConstraint::Strict)),
                            &dst,
                            SamplingOptions::new(FilterMode::Linear, MipmapMode::Linear),
                            &paint,
                        );
                    }
                    DecodedIcon::Svg { dom, width, height } => {
                        let cf = spec.tint.map(|c| crate::modifier::ColorFilter::Tint {
                            color: c,
                            blend_mode: crate::modifier::BlendMode::SrcIn,
                        });
                        draw_svg_dom(canvas, dom, fit_rect(rect, *width, *height), cf.as_ref(), 1.0);
                    }
                }
            }
        }
        #[cfg(any(
            feature = "material-symbols-outlined",
            feature = "material-symbols-rounded",
            feature = "material-symbols-sharp"
        ))]
        IconSource::Symbol(symbol) => {
            let size = rect.width().min(rect.height());
            if size > 0.0 {
                if let Some(blob) = crate::ui::icon::symbol_blob(*symbol, size, &spec.axes) {
                    let x = rect.left + (rect.width() - size) / 2.0;
                    let y = rect.top + (rect.height() - size) / 2.0 + size;
                    let mut paint = Paint::default();
                    paint.set_anti_alias(true);
                    paint.set_color(skia_color(spec.tint.unwrap_or(crate::modifier::Color::BLACK)));
                    canvas.draw_text_blob(&blob, (x, y), &paint);
                }
            }
        }
    }
    if mirror {
        canvas.restore();
    }
}

fn skia_color(c: crate::modifier::Color) -> skia_safe::Color {
    skia_safe::Color::from_argb(c.a, c.r, c.g, c.b)
}

/// 映射 winia BlendMode → skia BlendMode（同源 29 值）
fn to_skia_blend_mode(bm: crate::modifier::BlendMode) -> BlendMode {
    use crate::modifier::BlendMode as BM;
    match bm {
        BM::Clear => BlendMode::Clear,
        BM::Src => BlendMode::Src,
        BM::Dst => BlendMode::Dst,
        BM::SrcOver => BlendMode::SrcOver,
        BM::DstOver => BlendMode::DstOver,
        BM::SrcIn => BlendMode::SrcIn,
        BM::DstIn => BlendMode::DstIn,
        BM::SrcOut => BlendMode::SrcOut,
        BM::DstOut => BlendMode::DstOut,
        BM::SrcATop => BlendMode::SrcATop,
        BM::DstATop => BlendMode::DstATop,
        BM::Xor => BlendMode::Xor,
        BM::Plus => BlendMode::Plus,
        BM::Modulate => BlendMode::Modulate,
        BM::Screen => BlendMode::Screen,
        BM::Overlay => BlendMode::Overlay,
        BM::Darken => BlendMode::Darken,
        BM::Lighten => BlendMode::Lighten,
        BM::ColorDodge => BlendMode::ColorDodge,
        BM::ColorBurn => BlendMode::ColorBurn,
        BM::HardLight => BlendMode::HardLight,
        BM::SoftLight => BlendMode::SoftLight,
        BM::Difference => BlendMode::Difference,
        BM::Exclusion => BlendMode::Exclusion,
        BM::Multiply => BlendMode::Multiply,
        BM::Hue => BlendMode::Hue,
        BM::Saturation => BlendMode::Saturation,
        BM::Color => BlendMode::Color,
        BM::Luminosity => BlendMode::Luminosity,
    }
}

/// 映射 winia ColorFilter → skia ColorFilter（Tint/Matrix/Lighting）
fn to_skia_color_filter(cf: &crate::modifier::ColorFilter) -> Option<skia_safe::ColorFilter> {
    use crate::modifier::ColorFilter as CF;
    match cf {
        CF::Tint { color, blend_mode } => {
            skia_safe::color_filters::blend(skia_color(*color), to_skia_blend_mode(*blend_mode))
        }
        CF::Matrix(m) => Some(skia_safe::color_filters::matrix_row_major(m, None)),
        CF::Lighting { multiply, add } => {
            skia_safe::color_filters::lighting(skia_color(*multiply), skia_color(*add))
        }
    }
}

/// FilterQuality → SamplingOptions（None/Low/Medium/High）
fn sampling_options_for(q: crate::modifier::FilterQuality) -> SamplingOptions {
    use crate::modifier::FilterQuality as FQ;
    match q {
        FQ::None => SamplingOptions::new(FilterMode::Nearest, MipmapMode::None),
        FQ::Low => SamplingOptions::new(FilterMode::Linear, MipmapMode::None),
        FQ::Medium => SamplingOptions::new(FilterMode::Linear, MipmapMode::Nearest),
        FQ::High => SamplingOptions::new(FilterMode::Linear, MipmapMode::Linear),
    }
}

/// Image 组件绘制：ContentScale 缩放 + 对齐 + alpha + colorFilter + filterQuality
///（内容区域 rect）。位图/SVG 统一走 content_scale_rect（完整缩放/对齐/RTL），
/// 超出 bounds 时 clipToBounds（Crop/FillWidth/FillHeight 裁剪）。
fn draw_image_content(
    canvas: &Canvas,
    rect: Rect,
    source: &crate::ui::icon::IconSource,
    content_scale: crate::ui::image::ContentScale,
    alignment: crate::ui::image::ImageAlignment,
    alpha: f32,
    color_filter: Option<&crate::modifier::ColorFilter>,
    filter_quality: crate::modifier::FilterQuality,
    direction: crate::layout::LayoutDirection,
) {
    let Some(decoded) = crate::ui::icon::decoded_icon(source) else { return };
    let rtl = direction == crate::layout::LayoutDirection::Rtl;
    match decoded.as_ref() {
        crate::ui::icon::DecodedIcon::Bitmap { image, width, height } => {
            let dst = crate::ui::image::content_scale_rect(
                content_scale, rect, *width, *height, alignment, rtl,
            );
            if dst.width() <= 0.0 || dst.height() <= 0.0 {
                return;
            }
            // 对齐 Compose Image 的 clipToBounds：内容超出 bounds（Crop/FillWidth/
            // FillHeight 等比放大）时裁剪到 bounds；Fit/Inside 不超出则零开销
            let clipped = dst.left < rect.left || dst.top < rect.top
                || dst.right > rect.right || dst.bottom > rect.bottom;
            if clipped {
                canvas.save();
                canvas.clip_rect(rect, None, Some(false));
            }
            let src = Rect::new(0.0, 0.0, *width, *height);
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_alpha_f(alpha.clamp(0.0, 1.0));
            if let Some(cf) = color_filter {
                if let Some(filter) = to_skia_color_filter(cf) {
                    paint.set_color_filter(filter);
                }
            }
            canvas.draw_image_rect_with_sampling_options(
                image,
                Some((&src, skia_safe::canvas::SrcRectConstraint::Strict)),
                &dst,
                sampling_options_for(filter_quality),
                &paint,
            );
            if clipped {
                canvas.restore();
            }
        }
        crate::ui::icon::DecodedIcon::Svg { dom, width, height } => {
            // 与位图同一事实来源：content_scale_rect 完整缩放/对齐 + clipToBounds
            let dst = crate::ui::image::content_scale_rect(
                content_scale, rect, *width, *height, alignment, rtl,
            );
            if dst.width() <= 0.0 || dst.height() <= 0.0 {
                return;
            }
            let clipped = dst.left < rect.left || dst.top < rect.top
                || dst.right > rect.right || dst.bottom > rect.bottom;
            if clipped {
                canvas.save();
                canvas.clip_rect(rect, None, Some(false));
            }
            draw_svg_dom(canvas, dom, dst, color_filter, alpha);
            if clipped {
                canvas.restore();
            }
        }
    }
}

/// 等比缩放居中（ContentScale.Fit 语义）
fn fit_rect(rect: Rect, iw: f32, ih: f32) -> Rect {
    if iw <= 0.0 || ih <= 0.0 {
        return rect;
    }
    let scale = (rect.width() / iw).min(rect.height() / ih);
    let w = iw * scale;
    let h = ih * scale;
    Rect::from_xywh(
        rect.left + (rect.width() - w) / 2.0,
        rect.top + (rect.height() - h) / 2.0,
        w,
        h,
    )
}

fn draw_svg_dom(
    canvas: &Canvas,
    dom: &RefCell<svg::Dom>,
    dst: Rect,
    color_filter: Option<&crate::modifier::ColorFilter>,
    alpha: f32,
) {
    if dst.width() <= 0.0 || dst.height() <= 0.0 {
        return;
    }
    // 直接画布渲染（与字体路径一致）：容器尺寸设为目标尺寸后 render。
    // colorFilter 用 saveLayer 颜色滤镜 + alpha 在图层合成时统一应用。
    canvas.save();
    canvas.translate((dst.left, dst.top));
    let need_layer = color_filter.is_some() || alpha < 1.0;
    let layer = if need_layer {
        let mut paint = Paint::default();
        if let Some(cf) = color_filter {
            if let Some(filter) = to_skia_color_filter(cf) {
                paint.set_color_filter(filter);
            }
        }
        if alpha < 1.0 {
            paint.set_alpha_f(alpha.clamp(0.0, 1.0));
        }
        Some(canvas.save_layer(&skia_safe::canvas::SaveLayerRec::default().paint(&paint)))
    } else {
        None
    };
    // 缓存线程本地，绘制期 set_container_size 不跨线程
    dom.borrow_mut().set_container_size((dst.width(), dst.height()));
    dom.borrow().render(canvas);
    if layer.is_some() {
        canvas.restore();
    }
    canvas.restore();
}

// ── 渲染遍历（含 BackdropBlur 即时处理）──

fn render_pass1(
    nodes: &[LayoutNode],
    root_idx: usize,
    idx: usize,
    canvas: &Canvas,
    parent_x: f32, parent_y: f32,
) {
    let node = &nodes[idx];
    let x = parent_x + node.position.x;
    let y = parent_y + node.position.y;
    let w = node.measured_size.width;
    let h = node.measured_size.height;
    if w <= 0.0 || h <= 0.0 { return; }

    let rect = Rect::new(x, y, x + w, y + h);

    // 背景模糊：在节点**自身任何内容（背景/文本/子节点）绘制之前**处理——
    // snapshot 只含"位于其下"的已画内容（祖先 + 前面的兄弟），对齐 Compose
    // backdropBlur 语义。参考 v1 item.rs 实现：物理像素 snapshot + CropRect
    // blur + clip 到节点 + 像素网格对齐画回（draw_image 无采样缩放）。
    if let Some(radius) = node.modifier.backdrop_blur_radius() {
        draw_backdrop_blur(canvas, x, y, w, h, radius);
    }

    // 图形层：包住整个节点（background + text + children），应用 alpha/变换/颜色滤镜
    let gl_params = node.modifier.graphics_layer_params();
    let gl_saved = if let Some(gl) = gl_params {
        let has_filter = gl.color_filter.is_some();
        let has_alpha = gl.alpha < 1.0;
        if has_filter || has_alpha {
            let mut paint = skia_safe::Paint::default();
            if let Some(cf) = &gl.color_filter {
                if let Some(filter) = to_skia_color_filter(cf) {
                    paint.set_color_filter(filter);
                }
            }
            if has_alpha {
                paint.set_alpha_f(gl.alpha.clamp(0.0, 1.0));
            }
            canvas.save_layer(&skia_safe::canvas::SaveLayerRec::default().paint(&paint));
        } else {
            canvas.save();
        }
        // graphicsLayer shadowElevation：独立“变换 → 绘制 → 恢复”——阴影
        // 随 3D 形变，且不参与内容 clip（对标 Compose：层阴影在边界外）
        if gl.shadow_elevation > 0.0 {
            canvas.save();
            apply_gl_transform(canvas, &gl, x, y, w, h);
            let shape = gl.shadow_shape.clone().unwrap_or(crate::modifier::Shape::Rectangle);
            draw_elevation_shadow(
                canvas,
                rect,
                &shape,
                gl.shadow_elevation,
                gl.ambient_shadow_color,
                gl.spot_shadow_color,
            );
            canvas.restore();
        }
        // clip 到节点原始 bounds（变换**前**——Compose 语义：先裁剪到图层
        // bounds 再变换，旋转后角落会被切掉；变换后裁剪会随内容一起转，
        // clip 变成无效操作）
        if gl.clip {
            canvas.clip_rect(rect, None, true);
        }
        apply_gl_transform(canvas, &gl, x, y, w, h);
        true
    } else { false };
    let mut blur_radius: Option<f32> = None;
    let mut clip_shape: Option<crate::modifier::Shape> = None;
    // 阴影（elevation, shape, color）——链序中与 background 同层绘制；
    // clip=true 时并入 clip_shape（内容裁剪，阴影不受裁——Compose 语义）
    // 阴影层（预扫描直接绘制——见下方 pre-scan 注释）
    let mut text: Option<(&str, f32, &crate::modifier::Color, usize, crate::ui::TextAlign, crate::ui::TextOverflow, crate::ui::text::FontWeight, crate::ui::text::FontSlant, bool, f32, Option<f32>)> = None;
    let mut scroll_offset_v: Option<f32> = None;
    let mut scroll_offset_h: Option<f32> = None;

    // 叶子 padding 渲染偏移（测量已回加尺寸——绘制内容按内边距内缩；
    // 解析与测量期 get_padding_sides 一致：静态/动态统一，RTL 时 start 在右）
    let (pad_s, pad_t, pad_e, pad_b) = node.modifier.get_padding_sides();
    let pad_rtl = node.layout_direction == crate::layout::LayoutDirection::Rtl;
    let content_x = x + if pad_rtl { pad_e } else { pad_s };
    let content_y = y + pad_t;
    let content_w = (w - pad_s - pad_e).max(0.0);
    let content_h = (h - pad_t - pad_b).max(0.0);

    // ⚠ 阴影必须**垫底**（主循环绘制背景之前——无论链序）：Compose shadow
    // 是 graphicsLayer 独立层（垫底）。此前预扫描代码误放在主循环之后——
    // 阴影画在背景之上 → 卡片被压暗（绿卡 ×(1-0.43)≈0.6——实测 bug）
    for el in node.modifier.elements() {
        if let ModifierElement::Shadow { params, shape, .. } = el {
            draw_shadow_layer(canvas, rect, shape, params);
        }
    }

    // 同节点链序中最后绘制的背景（颜色+形状）——边框色与形状都等于它时
    // 跳过描边（合并为纯填充），避免半透明同色双重混合
    let mut last_background: Option<(crate::modifier::Color, crate::modifier::Shape)> = None;
    for el in node.modifier.elements() {
        match el {
            ModifierElement::Blur { radius } => {
                blur_radius = Some(*radius);
            }
            ModifierElement::Clip { shape } => {
                clip_shape = Some(shape.clone());
            }
            ModifierElement::Shadow { shape, clip, .. } => {
                if *clip {
                    clip_shape = Some(shape.clone());
                }
            }
            ModifierElement::VerticalScroll { state } => {
                let mut off = state.offset.get();
                // reverseLayout（LazyColumn 反向）：滚动平移镜像为
                // scroll_origin = content - vh - offset（内容坐标从底向上排布）
                if node.scroll_reverse {
                    off = (node.scroll_content_height - node.scroll_viewport_height - off).max(0.0);
                }
                scroll_offset_v = Some(off);
            }
            ModifierElement::HorizontalScroll { state, .. } => {
                let mut off = state.offset.get();
                if node.scroll_reverse {
                    off = (node.scroll_content_width - node.scroll_viewport_width - off).max(0.0);
                }
                scroll_offset_h = Some(off);
            }
            // 文本输入框容器（M3 Filled/Outlined——背景/指示线/边框/label/支持文本）
            ModifierElement::TextFieldVisual { variant, shape, colors, enabled: _, focused: _, is_error: _, cursor_color: _, indicator_color, focus_progress, offset_mapping, supporting } => {
                // 容器 rect：有支持文本时扣除其区域（supporting 画在容器底部外
                // 4dp，节点总高 = 容器 + 4 + 16）
                let supporting_h = supporting.as_ref().map_or(0.0, |sv| sv.height());
                let container_rect = Rect::new(x, y, x + w, y + h - supporting_h);
                // ⚠ label/placeholder/图标/前后缀均为子节点（text-field-v2
                // 容器化——TextFieldLayout 定位）；Outlined label 缺口由子节点
                // Label 的 placement 构造（跨边框悬浮时——label 顶越出容器顶，
                // 缺口 = label 水平范围 ± 4dp（M3 populated label padding））
                let cutout = if *variant == crate::ui::TextFieldVariant::Outlined {
                    node.children.iter().find_map(|&ci| {
                        let cn = &nodes[ci];
                        let is_label = cn.modifier.elements().iter().any(|el| {
                            matches!(el, ModifierElement::TextFieldSlot { role }
                                if *role == crate::ui::text_field::TextFieldSlotRole::Label)
                        });
                        if !is_label { return None; }
                        // ⚠ 子节点 position 相对**容器**（本节点）——用本节点
                        // 绝对坐标 x/y（parent_x 是父层坐标——用错缺口画偏）
                        let (lx, ly) = (x + cn.position.x, y + cn.position.y);
                        if ly >= container_rect.top { return None; } // 展开态：无缺口
                        Some(Rect::new(
                            lx - 4.0, ly,
                            lx + cn.measured_size.width + 4.0, ly + cn.measured_size.height,
                        ))
                    })
                } else { None };
                let focus_p = focus_progress.peek();
                draw_text_field_container(canvas, container_rect, variant, shape, colors, &indicator_color.peek(), focus_p, cutout);
                // 支持文本：容器底部外侧 4dp
                if let Some(sv) = supporting {
                    draw_text_field_aux_text(
                        canvas,
                        sv.content.as_str(),
                        sv.font_size,
                        sv.font_weight,
                        sv.font_style,
                        sv.letter_spacing,
                        sv.line_height,
                        &sv.color,
                        (x + 16.0, container_rect.bottom + 4.0),
                        w - 32.0,
                    );
                }
            }
            // 自定义绘制（Slider 轨道/thumb 等动态图形——闭包以节点 rect 调用）
            ModifierElement::CustomDraw { f } => {
                f(canvas, rect);
            }
            // 图标绘制于内容区域（padding 内缩——叶子 padding 渲染偏移）
            ModifierElement::DrawIcon { spec } => {
                draw_icon(canvas, Rect::new(content_x, content_y, content_x + content_w, content_y + content_h), node.layout_direction, spec);
            }
            // 图片（Image 组件）：ContentScale + 对齐 + alpha + colorFilter + filterQuality
            ModifierElement::ImageContent { source, content_scale, alignment, alpha, color_filter, filter_quality } => {
                draw_image_content(
                    canvas,
                    Rect::new(content_x, content_y, content_x + content_w, content_y + content_h),
                    source,
                    *content_scale,
                    *alignment,
                    *alpha,
                    color_filter.as_ref(),
                    *filter_quality,
                    node.layout_direction,
                );
            }
            el => {
                if let Some(tp) = render_modifier_element(
                    canvas,
                    el,
                    rect,
                    x,
                    y,
                    w,
                    h,
                    &mut last_background,
                ) {
                    text = Some((tp.content, tp.font_size, tp.color, tp.max_lines, tp.align, tp.overflow, tp.font_weight, tp.font_style, tp.soft_wrap, tp.letter_spacing, tp.line_height));
                }
            }
        }
    }

    // Open draw nodes (exp/modifier-node): node chain runs after the enum chain
    // (P1-1 fix — nodes used to run before enums and were covered by them).
    // NOTE: chain order is not preserved — nodes paint uniformly after all enum
    // draws (incl. icon/image), before text. Background-layer approximation.
    for draw_node in node.modifier.draw_nodes() {
        draw_node.draw(canvas, rect);
    }
    // Wrapping draw nodes, before half (exp/draw-wrap-node): same background-layer
    // slot as DrawNode. The after half runs after children + ripple (see below).
    for wrap_node in node.modifier.wrap_nodes() {
        wrap_node.draw_before(canvas, rect, &node.modifier);
    }

    // 内容模糊：saveLayer
    if let Some(r) = blur_radius {
        let mut paint = Paint::default();
        paint.set_image_filter(image_filters::blur((r, r), skia_safe::TileMode::Clamp, None, None));
        let rec = skia_safe::canvas::SaveLayerRec::default().paint(&paint);
        canvas.save_layer(&rec);
    }

    // Clip：在绘制内容前设置裁剪区域
    let mut clipped = false;
    if let Some(ref shape) = clip_shape {
        canvas.save();
        match shape {
            crate::modifier::Shape::Rectangle => { canvas.clip_rect(rect, None, Some(false)); }
            crate::modifier::Shape::RoundedRect { corner_radius } => {
                canvas.clip_rrect(RRect::new_rect_xy(rect, *corner_radius, *corner_radius), None, Some(false));
            }
            crate::modifier::Shape::TopRoundedRect { radius } => {
                let rr = RRect::new_rect_radii(rect, &[
                    skia_safe::Vector::new(*radius, *radius),
                    skia_safe::Vector::new(*radius, *radius),
                    skia_safe::Vector::new(0.0, 0.0),
                    skia_safe::Vector::new(0.0, 0.0),
                ]);
                canvas.clip_rrect(rr, None, Some(false));
            }
            crate::modifier::Shape::Pill => {
                let r = rect.width().min(rect.height()) / 2.0;
                canvas.clip_rrect(RRect::new_rect_xy(rect, r, r), None, Some(false));
            }
            crate::modifier::Shape::Circle => {
                canvas.clip_rrect(RRect::new_oval(rect), None, Some(false));
            }
        }
        clipped = true;
    }

    if let Some((content, font_size, color, max_lines, align, overflow, font_weight, font_style, soft_wrap, letter_spacing, line_height)) = text {
        // 优先用测量阶段缓存的 Paragraph（避免重建）
        if let Some(para) = node.cached_paragraph.borrow_mut().as_mut() {
            para.layout(content_w);
            let x_off = match align {
                crate::ui::TextAlign::Left | crate::ui::TextAlign::Justify => content_x,
                crate::ui::TextAlign::Center => content_x + (content_w - para.max_intrinsic_width()).max(0.0) / 2.0,
                crate::ui::TextAlign::Right => content_x + (content_w - para.max_intrinsic_width()).max(0.0),
            };
            // 选中高亮
            if let Some(range) = node.registrar.borrow().as_ref().cloned().and_then(|reg| reg.selected_range(node.slot_key)) {
                debug_log!("[selection] render node={} range={}..{}", node.id, range.start, range.end);
                let rects: Vec<_> = if range.start < range.end {
                para.get_rects_for_range(range.start..range.end, skia_safe::textlayout::RectHeightStyle::Max, skia_safe::textlayout::RectWidthStyle::Max) } else { Vec::new() };
                let mut paint = skia_safe::Paint::default();
                paint.set_color(skia_safe::Color::from_argb(80, 100, 150, 255));
                for tb in &rects {
                    canvas.draw_rect(skia_safe::Rect::new(x_off + tb.rect.left, content_y + tb.rect.top, x_off + tb.rect.right, content_y + tb.rect.bottom), &paint);
                }
            }
            para.paint(canvas, x_off, content_y);
            // 绘制光标（聚焦的 TextField 节点；色 = M3 cursor（primary/error））。
            // ⚠ 有选区（非零宽）时不显示——对齐 Compose：光标仅 collapsed
            // selection 时绘制
            let has_selection = node.registrar.borrow().as_ref()
                .and_then(|reg| reg.selected_range(node.slot_key))
                .map(|r| r.start < r.end)
                .unwrap_or(false);
            // 聚焦判定：优先组合期标记（text-field-v2 容器化——焦点在容器，
            // 输入子节点用 display_focused），回退节点自身 focused
            let focused = node.display_focused.get() || node.focused;
            if focused && !has_selection {
                if node.cursor_visible.get() {
                    // ⚠ cursor_color 在容器 TextFieldVisual（组合期解析 primary/
                    // error）——输入 leaf 无此元素，从 leaf 找会回退文本色。
                    // 沿 parent 链向上找容器（offset_mapping_for_node 同路径）
                    let cursor = crate::ui::text_field::text_field_visual_color(nodes, root_idx, idx)
                        .unwrap_or(*color);
                    let mut cp = skia_safe::Paint::default();
                    cp.set_color(skia_safe::Color::from_argb(255, cursor.r, cursor.g, cursor.b));
                    cp.set_stroke_width(1.5);
                    // 空文本：无 glyph 可定位——光标画在内容起点（M3 光标高度
                    // 用行高近似；TextLayout 空特判内部 get_glyph_cluster_at(0)
                    // 同样失败）
                    if content.is_empty() {
                        let ch = font_size * 1.4;
                        canvas.draw_line(
                            skia_safe::Point::new(content_x, content_y),
                            skia_safe::Point::new(content_x, content_y + ch),
                            &cp,
                        );
                    } else {
                        // ⚠ length = 文本真实字节数（空文本为 0 → 空特判）——
                        // 映射表长度恒 ≥1（含 end-of-text 映射），传它空文本
                        // 时走主路径 get_by_right(0) 失败 → 光标不显示
                        let tl = crate::text::TextLayout::new(para, content.len());
                        // 光标索引是编辑偏移——经 OffsetMapping 转显示偏移
                        // （密码掩码/格式化输入显示文本 ≠ 编辑文本；
                        // TextFieldVisual 在容器——向上找）
                        let offset_mapping = crate::ui::text_field::offset_mapping_for_node(nodes, root_idx, idx);
                        let idx = offset_mapping.as_ref()
                            .map(|m| m.original_to_transformed(node.cursor_index.get()))
                            .unwrap_or_else(|| node.cursor_index.get());
                        if let Some((cx, cy, ch)) = tl.get_cursor_position(idx) {
                            debug_log!("[render] cursor pos=({:.0},{:.0}) h={:.0}", cx, cy, ch);
                            canvas.draw_line(skia_safe::Point::new(x_off + cx, content_y + cy), skia_safe::Point::new(x_off + cx, content_y + cy + ch), &cp);
                        } else { debug_log!("[render] get_cursor_position returned None for idx={}", node.cursor_index.get()); }
                    }
                } else { debug_log!("[render] cursor_visible is false"); }
            }
            // IME 组合文本下划线（编辑偏移 → 显示偏移）
            if let Some(comp_range) = node.composing_range.borrow().as_ref() {
                if comp_range.start < comp_range.end {
                    let offset_mapping = crate::ui::text_field::offset_mapping_for_node(nodes, root_idx, idx);
                    let (cs, ce) = offset_mapping.as_ref().map(|m| {
                        (m.original_to_transformed(comp_range.start), m.original_to_transformed(comp_range.end))
                    }).unwrap_or((comp_range.start, comp_range.end));
                    let rects = para.get_rects_for_range(cs..ce, skia_safe::textlayout::RectHeightStyle::Max, skia_safe::textlayout::RectWidthStyle::Max);
                    let mut und_paint = skia_safe::Paint::default();
                    // Phase 4.2：组合期捕获的主题 primary（render 阶段 CompositionLocal
                    // 已退出，读 WiniaTheme::colors() 会得到默认主题）
                    let c = node.composing_color.get();
                    und_paint.set_color(skia_safe::Color::from_argb(c.a, c.r, c.g, c.b));
                    und_paint.set_stroke_width(1.0);
                    for tb in &rects {
                        let r = tb.rect;
                        canvas.draw_line(skia_safe::Point::new(x_off + r.left, content_y + r.bottom), skia_safe::Point::new(x_off + r.right, content_y + r.bottom), &und_paint);
                    }
                }
            }
        } else {
            draw_text_with_selection(canvas, content, font_size, color, font_weight, font_style, content_x, content_y, content_w, max_lines, align, overflow, soft_wrap, letter_spacing, line_height, node.registrar.borrow().as_ref().cloned(), node.slot_key);
        }
    }

    // ═══ 富文本（RichText）渲染 ═══
    if node.has_richtext_content {
        if let Some(para) = node.cached_paragraph.borrow_mut().as_mut() {
            para.layout(content_w);
            // 选中高亮
            if let Some(range) = node.registrar.borrow().as_ref().cloned().and_then(|reg| reg.selected_range(node.slot_key)) {
                let rects: Vec<_> = if range.start < range.end {
                para.get_rects_for_range(range.start..range.end, skia_safe::textlayout::RectHeightStyle::Max, skia_safe::textlayout::RectWidthStyle::Max) } else { Vec::new() };
                let mut paint = skia_safe::Paint::default();
                paint.set_color(skia_safe::Color::from_argb(80, 100, 150, 255));
                for tb in &rects {
                    canvas.draw_rect(skia_safe::Rect::new(content_x + tb.rect.left, content_y + tb.rect.top, content_x + tb.rect.right, content_y + tb.rect.bottom), &paint);
                }
            }
            para.paint(canvas, content_x, content_y);
        }
    }

    // 焦点环：聚焦淡入、失焦淡出——透明度来自交互源 focus_indicator_alpha
    // （失焦后动画期间 alpha>0 继续绘制，实现平滑淡出）。
    // ⚠ TextField 容器（M3）不用焦点环——用指示线/边框色变化提示焦点
    let has_tf_container = node.modifier.elements().iter()
        .any(|el| matches!(el, ModifierElement::TextFieldVisual { .. }));
    // NoFocusRing：组件自绘焦点环（Slider 焦点环包围 thumb 而非整组件）
    let no_focus_ring = node.modifier.elements().iter()
        .any(|el| matches!(el, ModifierElement::NoFocusRing));
    let focus_alpha = node.modifier.focusable_interaction()
        .map(|src| src.focus_indicator_alpha_value())
        .unwrap_or(if node.focused { 1.0 } else { 0.0 });
    if !has_tf_container && !no_focus_ring && (node.focused || focus_alpha > 0.001) {
        // 焦点环形状跟随组件（最近 Background/Border/Clip 形状，回退矩形）；
        // 颜色由组件组合期从主题捕获（node.focus_color）
        let focus_shape = node.modifier.elements().iter().rev().find_map(|el| match el {
            ModifierElement::Background { shape, .. }
            | ModifierElement::Border { shape, .. }
            | ModifierElement::BorderDynamic { shape, .. }
            | ModifierElement::Clip { shape } => Some(*shape),
            _ => None,
        }).unwrap_or(crate::modifier::Shape::Rectangle);
        draw_focus(canvas, rect, &focus_shape, node.focus_color.get(), focus_alpha, 2.0);
    }

    // Scroll clip + translate
    let mut scrolled = false;
    if scroll_offset_v.is_some() || scroll_offset_h.is_some() {
        // clip 用 visible 尺寸，优先从 modifier 中提取 Size 或 FillMax 信息
        let mut cw = w;
        let mut ch = h;
        for el in node.modifier.elements() {
            match el {
                ModifierElement::Size { width, height } => {
                    if let crate::modifier::SizeValue::Static(dw) = width {
                        if dw.is_fixed() { cw = dw.to_logical_px(); }
                    }
                    if let crate::modifier::SizeValue::Static(dh) = height {
                        if dh.is_fixed() { ch = dh.to_logical_px(); }
                    }
                }
                ModifierElement::FillMaxWidth | ModifierElement::FillMaxSize => {
                    cw = w; // measured width = parent max width
                }
                ModifierElement::FillMaxHeight => {
                    ch = h;
                }
                _ => {}
            }
        }
        // scroll 容器的可视尺寸以测量期回写的 viewport 为准——measured_size
        // 是内容全高（apply_scroll_delta 的 max_offset 依赖它），直接用会把
        // 裁剪矩形拉到内容末端，滚动内容会越界绘制到后续兄弟（如 Scaffold
        // bottom bar）之上
        if node.scroll_viewport_height > 0.0 {
            ch = node.scroll_viewport_height;
        }
        if node.scroll_viewport_width > 0.0 {
            cw = node.scroll_viewport_width;
        }
        let clip_rect = Rect::new(x, y, x + cw, y + ch);
        let dx = -scroll_offset_h.unwrap_or(0.0);
        let dy = -scroll_offset_v.unwrap_or(0.0);
        canvas.save();
        canvas.clip_rect(clip_rect, None, Some(false));
        canvas.translate((dx, dy));
        scrolled = true;
    }

    // 穿行子节点（backdrop 节点自身内容照常绘制在模糊层之上）
    for &child in &node.children {
        render_pass1(nodes, root_idx, child, canvas, x, y);
    }

    // Ripple indication — above content, inside shape/scroll clipping
    draw_ripple(node, canvas, x, y, w, h);

    // Wrapping draw nodes, after half (exp/draw-wrap-node): above children and
    // above ripple, inside the scroll translate (same stack as ripple — the node
    // follows scrolled content). This is the slot a future RippleNode migration
    // would use; the enum Ripple path stays untouched.
    for wrap_node in node.modifier.wrap_nodes() {
        wrap_node.draw_after(canvas, rect, &node.modifier);
    }

    if scrolled {
        canvas.restore();
    }

    if clipped {
        canvas.restore();
    }

    if blur_radius.is_some() {
        canvas.restore();
    }

    if gl_saved {
        canvas.restore();
    }
}

/// 水波纹（按旧版 D:\winia ripple.rs 的绘制方式）：
/// - 两层模型：背景层（hover/focus 状态层）= 节点中心圆（半径=对角线/2，
///   直径=对角线）；前景层 = 按压点实心圆（半径=对角线×progress）
/// - 前景层裁剪到背景的范围和形状：bounded → 节点形状；unbounded →
///   背景圆（直径=对角线）——波纹不会超出背景层
/// - hover/focus 状态层透明度动画值；每层扩散/淡出由动画系统驱动
fn draw_ripple(node: &LayoutNode, canvas: &Canvas, x: f32, y: f32, w: f32, h: f32) {
    for el in node.modifier.elements() {
        let ModifierElement::Ripple { source, color, bounded, shape } = el else { continue };

        let diagonal = (w * w + h * h).sqrt();
        let rect = Rect::new(x, y, x + w, y + h);

        // bounded：裁剪到波纹形状——显式 shape（Button 传入容器 shape——
        // Outlined/Text 无背景元素时仍按胶囊裁剪）优先；否则从链上最近的
        // Background/Border 推断（通用 clickable + ripple 保持可用）
        let mut clipped = false;
        if *bounded {
            let clip_shape = (*shape).or_else(|| {
                node.modifier.elements().iter().rev().find_map(|el| match el {
                    ModifierElement::Background { shape, .. }
                    | ModifierElement::Border { shape, .. }
                    | ModifierElement::BorderDynamic { shape, .. } => {
                        Some(*shape)
                    }
                    _ => None,
                })
            });
            canvas.save();
            match clip_shape {
                Some(crate::modifier::Shape::RoundedRect { corner_radius }) => {
                    canvas.clip_rrect(
                        skia_safe::RRect::new_rect_xy(rect, corner_radius, corner_radius),
                        None,
                        Some(false),
                    );
                }
                Some(crate::modifier::Shape::TopRoundedRect { radius }) => {
                    let rr = skia_safe::RRect::new_rect_radii(rect, &[
                        skia_safe::Vector::new(radius, radius),
                        skia_safe::Vector::new(radius, radius),
                        skia_safe::Vector::new(0.0, 0.0),
                        skia_safe::Vector::new(0.0, 0.0),
                    ]);
                    canvas.clip_rrect(rr, None, Some(false));
                }
                Some(crate::modifier::Shape::Pill) => {
                    let r = rect.width().min(rect.height()) / 2.0;
                    canvas.clip_rrect(
                        skia_safe::RRect::new_rect_xy(rect, r, r),
                        None,
                        Some(false),
                    );
                }
                Some(crate::modifier::Shape::Circle) => {
                    // 圆形裁剪（此前落入 _ => clip_rect 被裁成矩形——
                    // IconButton 等圆形容器的波纹呈矩形）
                    canvas.clip_rrect(skia_safe::RRect::new_oval(rect), None, Some(false));
                }
                _ => {
                    canvas.clip_rect(rect, None, Some(false));
                }
            }
            clipped = true;
        } else {
            // unbounded：前景裁剪到背景圆（直径=对角线，节点中心）
            let r = diagonal / 2.0;
            canvas.save();
            canvas.clip_rrect(
                skia_safe::RRect::new_oval(Rect::from_xywh(
                    x + w / 2.0 - r,
                    y + h / 2.0 - r,
                    r * 2.0,
                    r * 2.0,
                )),
                None,
                Some(false),
            );
            clipped = true;
        }

        // ── 状态层：节点中心实心圆（半径=对角线/2，旧版 draw_circle）──
        let state_alpha = source.hover_opacity_value() + source.focus_opacity_value();
        if state_alpha > 0.0 {
            let mut paint = skia_safe::Paint::default();
            paint.set_anti_alias(true);
            paint.set_color(skia_safe::Color::from_argb(
                (color.a as f32 * state_alpha) as u8,
                color.r,
                color.g,
                color.b,
            ));
            canvas.draw_circle(
                skia_safe::Point::new(x + w / 2.0, y + h / 2.0),
                diagonal / 2.0,
                &paint,
            );
        }

        // ── 前景波纹层：按压点实心圆（半径=对角线×progress——旧版 draw_circle；
        //    直径=2×对角线——按压点靠角落时圆足以覆盖整个组件）──
        // 中心存的是节点本地坐标（press_interaction_down 已从场景坐标换算）；
        // 画布坐标 = 布局原点 + 本地坐标——祖先 scroll translate 与自身
        // graphics_layer 变换都已作用在画布上，波纹视觉位置自动跟随节点
        // （按下后滚动/变换动画中不脱离按钮）。
        for layer in source.ripple_layers() {
            let progress = layer.progress.get();
            let opacity = layer.opacity.get();
            let r = diagonal * progress;
            if r <= 0.0 || opacity <= 0.0 {
                continue;
            }
            let mut paint = skia_safe::Paint::default();
            paint.set_anti_alias(true);
            paint.set_color(skia_safe::Color::from_argb(
                (color.a as f32 * opacity) as u8,
                color.r,
                color.g,
                color.b,
            ));
            canvas.draw_circle(
                skia_safe::Point::new(x + layer.center.0, y + layer.center.1),
                r,
                &paint,
            );
        }

        if clipped {
            canvas.restore();
        }
    }
}

// ── 背景模糊（BackdropBlur）──

/// 背景模糊采样区：blur sigma = radius（3σ 采样范围），快照扩展必须覆盖
/// 3σ 才不触及 snapshot 边界（Clamp 重复像素产生边缘条纹）。
const BACKDROP_BLUR_MARGIN: f32 = 3.0;

/// 节点背景模糊——参考 v1 `item.rs` 实现，关键点：
/// 1. **物理像素 snapshot**：节点四角经当前画布矩阵（含全局 HiDPI scale、
///    祖先 scroll/overlay 平移）映射为屏幕物理坐标，取 AABB + margin 扩展，
///    clamp 到 surface 边界（跨顶/左边缘时以相交原点锚定，防内容偏移；
///    仅用裸逻辑坐标 ×sf 会在滚动容器/overlay 中错位）
/// 2. **CropRect 限定滤镜输出** = 快照尺寸（防滤镜越界处理）
/// 3. **clip 到节点矩形**：只显示节点内部，边缘带不参与合成（无白雾晕开）
/// 4. **draw_image 画回**：逆矩阵把快照原点（物理像素）映射回画布逻辑坐标，
///    scale(1/矩阵缩放) —— 快照像素与物理像素严格 1:1，无采样缩放，
///    移动时边缘不闪烁
/// 语义：在节点自身内容（背景/文本/子节点）绘制**之前**执行——snapshot
/// 只含"位于其下"的已画内容（祖先 + 前面的兄弟），对齐 Compose。
/// 已知限制（与 v1 一致）：blur 绘制在 graphicsLayer 变换/alpha 之前——
/// 节点自身 alpha/scale/rotation 不作用于模糊层；旋转祖先下画回仅
/// translate+scale 为近似。这些场景未在 demo 覆盖。
fn draw_backdrop_blur(
    canvas: &Canvas,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    radius: f32,
) {
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    let margin = radius * BACKDROP_BLUR_MARGIN;

    // 节点屏幕物理位置：当前画布矩阵映射（含全局 sf 与祖先 scroll/gl/overlay）
    let m = canvas.local_to_device_as_3x3();
    let corners = [
        m.map_xy(x, y),
        m.map_xy(x + w, y),
        m.map_xy(x, y + h),
        m.map_xy(x + w, y + h),
    ];
    let (sx0, sy0) = corners.iter().fold((f32::MAX, f32::MAX), |acc, p| {
        (acc.0.min(p.x), acc.1.min(p.y))
    });
    let (sx1, sy1) = corners.iter().fold((f32::MIN, f32::MIN), |acc, p| {
        (acc.0.max(p.x), acc.1.max(p.y))
    });
    if !sx1.is_finite() || !sy1.is_finite() {
        return;
    }
    // 物理像素边界（floor/ceil——截断会丢边缘像素），并 clamp 到 surface：
    // `image_snapshot_with_bounds` 会与 surface 相交——若节点（或其 3σ
    // 采样区）跨出顶/左边缘，返回图像锚定在相交原点 (0,0)，画回若按未
    // clamp 的 (left,top) 锚定会整体偏移 (-left,-top) 物理像素（回归：
    // 旧两阶段实现有 `.max(0.0)`）。clamp 后以 clamp 原点锚定即对齐。
    let Some(surface) = (unsafe { canvas.surface() }) else { return; };
    let (sw_s, sh_s) = (surface.width(), surface.height());
    let Some(bounds) = backdrop_snapshot_irect(sx0, sy0, sx1, sy1, margin, sw_s, sh_s) else {
        return;
    };
    let (left, top, right, bottom) = (bounds.left, bounds.top, bounds.right, bounds.bottom);

    let Some(snap) = surface_snapshot(canvas, IRect::from_ltrb(left, top, right, bottom)) else {
        return;
    };
    let (sw, sh) = {
        let info = snap.image_info();
        (info.width(), info.height())
    };
    if sw <= 0 || sh <= 0 {
        return;
    }

    // CropRect 限定模糊输出 = 快照范围（参考 v1：防滤镜对越界区域采样）
    let mut paint = Paint::default();
    paint.set_image_filter(image_filters::blur(
        (radius, radius),
        skia_safe::TileMode::Clamp,
        None,
        image_filters::CropRect::from(Rect::from_wh(sw as f32, sh as f32)),
    ));

    // 画回：快照原点物理 (left,top) → 画布逻辑坐标（逆矩阵）
    let Some(inv) = m.invert() else { return };
    let origin = inv.map_xy(left as f32, top as f32);
    // 矩阵缩放（无旋转/斜切时直接取对角线；否则均匀 scale = 1/√|det| 近似）
    let (rc00, rc01, rc10, rc11) = (m.rc(0, 0), m.rc(0, 1), m.rc(1, 0), m.rc(1, 1));
    let det = rc00 * rc11 - rc01 * rc10;
    if !det.is_finite() || det.abs() < 1e-9 {
        return;
    }
    let (sc_x, sc_y) = if rc01.abs() < 1e-4 && rc10.abs() < 1e-4 && rc00.abs() > 1e-4 && rc11.abs() > 1e-4 {
        (1.0 / rc00, 1.0 / rc11)
    } else {
        let s = 1.0 / det.abs().sqrt();
        (s, s)
    };

    canvas.save();
    // clip 到节点矩形（逻辑坐标——被画布矩阵自动变换）
    canvas.clip_rect(Rect::from_xywh(x, y, w, h), None, None);
    canvas.translate((origin.x, origin.y));
    canvas.scale((sc_x, sc_y));
    canvas.draw_image(&snap, skia_safe::Point::new(0.0, 0.0), Some(&paint));
    canvas.restore();
}

/// 快照物理像素边界：AABB + margin 扩展，floor/ceil 取整后 clamp 到
/// surface 范围。返回 None 表示与 surface 无交集（无需模糊）。
fn backdrop_snapshot_irect(
    sx0: f32,
    sy0: f32,
    sx1: f32,
    sy1: f32,
    margin: f32,
    sw_s: i32,
    sh_s: i32,
) -> Option<IRect> {
    let left = ((sx0 - margin).floor() as i32).clamp(0, sw_s);
    let top = ((sy0 - margin).floor() as i32).clamp(0, sh_s);
    let right = ((sx1 + margin).ceil() as i32).clamp(0, sw_s);
    let bottom = ((sy1 + margin).ceil() as i32).clamp(0, sh_s);
    if right <= left || bottom <= top {
        None
    } else {
        Some(IRect::from_ltrb(left, top, right, bottom))
    }
}

// ── 辅助函数 ──

impl From<&crate::modifier::Color> for Color4f {
    fn from(c: &crate::modifier::Color) -> Self {
        Color4f::new(
            c.r as f32 / 255.0,
            c.g as f32 / 255.0,
            c.b as f32 / 255.0,
            c.a as f32 / 255.0,
        )
    }
}

/// 开放绘制节点用的背景绘制入口（exp/modifier-node 试点：第三方 DrawNode
/// 无需复刻背景绘制逻辑，直接调此函数；与枚举 Background 同实现）。
pub fn draw_background_for_node(canvas: &Canvas, rect: Rect, color: &crate::modifier::Color, shape: &crate::modifier::Shape) {
    draw_background(canvas, rect, color, shape);
}

fn draw_background(canvas: &Canvas, rect: Rect, color: &crate::modifier::Color, shape: &crate::modifier::Shape) {
    let mut paint = Paint::default();
    paint.set_color4f(Color4f::from(color), None);
    paint.set_anti_alias(true);
    match shape {
        crate::modifier::Shape::Rectangle => { canvas.draw_rect(rect, &paint); }
        crate::modifier::Shape::RoundedRect { corner_radius } => {
            canvas.draw_rrect(RRect::new_rect_xy(rect, *corner_radius, *corner_radius), &paint);
        }
        crate::modifier::Shape::TopRoundedRect { radius } => {
            let rr = RRect::new_rect_radii(rect, &[
                skia_safe::Vector::new(*radius, *radius),
                skia_safe::Vector::new(*radius, *radius),
                skia_safe::Vector::new(0.0, 0.0),
                skia_safe::Vector::new(0.0, 0.0),
            ]);
            canvas.draw_rrect(rr, &paint);
        }
        crate::modifier::Shape::Pill => {
            let r = rect.width().min(rect.height()) / 2.0;
            canvas.draw_rrect(RRect::new_rect_xy(rect, r, r), &paint);
        }
        crate::modifier::Shape::Circle => {
            canvas.draw_circle((rect.center_x(), rect.center_y()), rect.width().min(rect.height()) / 2.0, &paint);
        }
    }
}

// ── TextField 容器视觉（M3 Filled/Outlined）──

/// M3 文本输入框容器：Filled = 容器色背景 + 底部指示线
/// （focused 2px / unfocused 1px，色 indicator_color）；
/// Outlined = 边框（focused 2px / unfocused 1px）。
/// 状态优先级 disabled > error > focused > unfocused（组合期已解析）。
/// M3 文本输入框容器：Filled = 容器色背景 + 底部指示线
/// （focused 2px / unfocused 1px）；Outlined = 边框（圆角 4dp，
/// `cutout` 为悬浮 label 缺口区域）。颜色来自动画 State
/// （`animate_color_as_state`——状态切换过渡）；`focus_p` 0..1 插值宽度。
fn draw_text_field_container(
    canvas: &Canvas,
    rect: Rect,
    variant: &crate::ui::TextFieldVariant,
    shape: &crate::modifier::Shape,
    colors: &crate::ui::TextFieldColors,
    indicator: &crate::modifier::Color,
    focus_p: f32,
    cutout: Option<Rect>,
) {
    let stroke_w = 1.0 + focus_p.clamp(0.0, 1.0);
    match variant {
        crate::ui::TextFieldVariant::Filled => {
            // 容器背景（surfaceContainerHighest；M3 top 4dp 圆角）
            if colors.container.a > 0 {
                let mut bg = Paint::default();
                bg.set_color4f(Color4f::from(&colors.container), None);
                bg.set_anti_alias(true);
                canvas.draw_rrect(RRect::new_rect_xy(rect, 4.0, 4.0), &bg);
            }
            // 底部指示线（贴底 1/2px 高，focused 加粗）
            let w = stroke_w;
            let mut lp = Paint::default();
            lp.set_color4f(Color4f::from(indicator), None);
            lp.set_anti_alias(true);
            canvas.draw_rect(Rect::new(
                rect.left, rect.bottom - w,
                rect.right, rect.bottom,
            ), &lp);
        }
        crate::ui::TextFieldVariant::Outlined => {
            // 边框（stroke 居中——1/2px，四角 4dp 圆角；label 缺口处断开）
            let w = stroke_w;
            let mut bp = Paint::default();
            bp.set_color4f(Color4f::from(indicator), None);
            bp.set_anti_alias(true);
            bp.set_style(skia_safe::paint::Style::Stroke);
            bp.set_stroke_width(w);
            let inset = w / 2.0;
            let border_rect = Rect::new(
                rect.left + inset, rect.top + inset,
                rect.right - inset, rect.bottom - inset,
            );
            if let Some(cut) = cutout {
                // 缺口：clip Difference 挖掉 label 区域再画边框
                canvas.save();
                canvas.clip_rect(cut, Some(skia_safe::ClipOp::Difference), None);
                canvas.draw_rrect(RRect::new_rect_xy(border_rect, 4.0, 4.0), &bp);
                canvas.restore();
            } else {
                canvas.draw_rrect(RRect::new_rect_xy(border_rect, 4.0, 4.0), &bp);
            }
        }
    }
}

/// 量文本宽度（label 边框缺口用——一次 layout）
fn measure_text_width(content: &str, font_size: f32, max_width: f32) -> f32 {
    if content.is_empty() || max_width <= 0.0 {
        return 0.0;
    }
    let mut para = crate::layout::node::build_plain_paragraph(
        content,
        font_size,
        &crate::modifier::Color::BLACK,
        crate::ui::text::FontWeight::NORMAL,
        crate::ui::text::FontSlant::Upright,
        usize::MAX,
        crate::ui::TextAlign::Left,
        crate::ui::TextOverflow::Clip,
        true,
        0.0,
        None,
        max_width,
    );
    para.layout(max_width);
    para.max_intrinsic_width()
}

/// TextField label/支持文本绘制（无布局缓存的独立小段文本——
/// build_plain_paragraph 每次构建；辅助文本量小，开销可接受）
fn draw_text_field_aux_text(
    canvas: &Canvas,
    content: &str,
    font_size: f32,
    font_weight: crate::ui::text::FontWeight,
    font_style: crate::ui::text::FontSlant,
    letter_spacing: f32,
    line_height: Option<f32>,
    color: &crate::modifier::Color,
    pos: (f32, f32),
    max_width: f32,
) {
    if content.is_empty() || max_width <= 0.0 {
        return;
    }
    let mut para = crate::layout::node::build_plain_paragraph(
        content,
        font_size,
        color,
        font_weight,
        font_style,
        usize::MAX,
        crate::ui::TextAlign::Left,
        crate::ui::TextOverflow::Clip,
        true,
        letter_spacing,
        line_height,
        max_width,
    );
    // ⚠ 必须 layout 后才能 paint（skia Paragraph 未布局时绘制为空）
    para.layout(max_width);
    para.paint(canvas, pos.0, pos.1);
}

fn draw_border(canvas: &Canvas, x: f32, y: f32, w: f32, h: f32, width: f32, color: &crate::modifier::Color, shape: &crate::modifier::Shape) {
    let mut paint = Paint::default();
    paint.set_color4f(Color4f::from(color), None);
    paint.set_style(skia_safe::paint::Style::Stroke);
    paint.set_stroke_width(width);
    paint.set_anti_alias(true);
    let inset = width / 2.0;
    let sr = Rect::new(x + inset, y + inset, x + w - inset, y + h - inset);
    match shape {
        crate::modifier::Shape::Rectangle => { canvas.draw_rect(sr, &paint); }
        crate::modifier::Shape::RoundedRect { corner_radius } => {
            canvas.draw_rrect(RRect::new_rect_xy(sr, (*corner_radius - inset).max(0.0), (*corner_radius - inset).max(0.0)), &paint);
        }
        crate::modifier::Shape::TopRoundedRect { radius } => {
            let r = (*radius - inset).max(0.0);
            let rr = RRect::new_rect_radii(sr, &[
                skia_safe::Vector::new(r, r),
                skia_safe::Vector::new(r, r),
                skia_safe::Vector::new(0.0, 0.0),
                skia_safe::Vector::new(0.0, 0.0),
            ]);
            canvas.draw_rrect(rr, &paint);
        }
        crate::modifier::Shape::Pill => {
            // 路径圆角 = 节点 pill 半径 - inset（不要对 sr 再减一次——
            // 否则外缘在圆角处比背景内缩 1px，边框不像内边框）
            let r = (w.min(h) / 2.0 - inset).max(0.0);
            canvas.draw_rrect(RRect::new_rect_xy(sr, r, r), &paint);
        }
        crate::modifier::Shape::Circle => {
            canvas.draw_circle((sr.center_x(), sr.center_y()), sr.width().min(sr.height()) / 2.0, &paint);
        }
    }
}

/// 焦点环（M3 focus indicator）——宽 3、完全位于组件**外部**（环内侧距
/// 组件边缘 2）、形状跟随组件、颜色来自组件组合期捕获的主题色。
pub(crate) fn draw_focus(
    canvas: &Canvas,
    rect: Rect,
    shape: &crate::modifier::Shape,
    color: crate::modifier::Color,
    alpha: f32,
    gap: f32,
) {
    const FOCUS_WIDTH: f32 = 3.0;
    const FOCUS_SCALE_AMOUNT: f32 = 0.15; // 淡入起点放大倍数（大环收缩到贴合）
    // 环中心线在组件外 gap + 半宽处——stroke 居中绘制时环完全在外侧
    let inset = gap + FOCUS_WIDTH / 2.0;
    let mut sr = Rect::new(
        rect.left - inset,
        rect.top - inset,
        rect.right + inset,
        rect.bottom + inset,
    );
    // 淡入：alpha 0→1 时环从放大（1.15×）收缩到最终位置——以组件中心为锚
    let scale = 1.0 + (1.0 - alpha.clamp(0.0, 1.0)) * FOCUS_SCALE_AMOUNT;
    let (cx, cy) = (sr.center_x(), sr.center_y());
    sr = Rect::from_xywh(
        cx + (sr.left - cx) * scale,
        cy + (sr.top - cy) * scale,
        sr.width() * scale,
        sr.height() * scale,
    );
    if sr.width() <= 0.0 || sr.height() <= 0.0 {
        return;
    }
    let mut paint = Paint::default();
    // 透明度随 focus_indicator_alpha 动画（聚焦淡入/失焦淡出）
    paint.set_color4f(Color4f::new(
        color.r as f32 / 255.0,
        color.g as f32 / 255.0,
        color.b as f32 / 255.0,
        color.a as f32 / 255.0 * alpha.clamp(0.0, 1.0),
    ), None);
    paint.set_style(skia_safe::paint::Style::Stroke);
    paint.set_stroke_width(FOCUS_WIDTH);
    paint.set_anti_alias(true);
    match shape {
        crate::modifier::Shape::Rectangle => { canvas.draw_rect(sr, &paint); }
        crate::modifier::Shape::RoundedRect { corner_radius } => {
            // 外扩后圆角同步放大（保持与组件同心）
            let r = (*corner_radius + inset).max(0.0) * scale;
            canvas.draw_rrect(RRect::new_rect_xy(sr, r, r), &paint);
        }
        crate::modifier::Shape::TopRoundedRect { radius } => {
            let r = (*radius + inset).max(0.0) * scale;
            let rr = RRect::new_rect_radii(sr, &[
                skia_safe::Vector::new(r, r),
                skia_safe::Vector::new(r, r),
                skia_safe::Vector::new(0.0, 0.0),
                skia_safe::Vector::new(0.0, 0.0),
            ]);
            canvas.draw_rrect(rr, &paint);
        }
        crate::modifier::Shape::Pill => {
            let r = sr.width().min(sr.height()) / 2.0;
            canvas.draw_rrect(RRect::new_rect_xy(sr, r, r), &paint);
        }
        crate::modifier::Shape::Circle => {
            canvas.draw_circle(
                (sr.center_x(), sr.center_y()),
                sr.width().min(sr.height()) / 2.0,
                &paint,
            );
        }
    }
}

/// draw_text + 选中高亮
fn draw_text_with_selection(
    canvas: &Canvas,
    content: &str,
    font_size: f32,
    color: &crate::modifier::Color,
    font_weight: crate::ui::text::FontWeight,
    font_style: crate::ui::text::FontSlant,
    x: f32, y: f32, w: f32,
    max_lines: usize, align: crate::ui::TextAlign, overflow: crate::ui::TextOverflow, soft_wrap: bool,
    letter_spacing: f32, line_height: Option<f32>,
    registrar: Option<crate::ui::selection_container::SelectionRegistrar>,
    slot_key: u64,
) {
    if registrar.and_then(|reg| reg.selected_range(slot_key)).is_some() {
        let mut paint = skia_safe::Paint::default();
        paint.set_color(skia_safe::Color::from_argb(80, 100, 150, 255));
        canvas.draw_rect(skia_safe::Rect::new(x, y, x + w, y + font_size * 1.2), &paint);
    }
    // 兜底路径：缓存缺失时用共享构建函数重建（与测量期同一套逻辑——SSOT）
    let para = crate::layout::node::build_plain_paragraph(
        content, font_size, color, font_weight, font_style,
        max_lines, align, overflow, soft_wrap, letter_spacing, line_height, w,
    );
    draw_text(canvas, &para, x, y, w, align);
}

fn draw_text(
    canvas: &Canvas,
    para: &crate::text::Paragraph,
    x: f32,
    y: f32,
    max_width: f32,
    align: crate::ui::TextAlign,
) {
    // 计算 x 偏移以支持 Center/Right/Justify 对齐
    let x_offset = match align {
        crate::ui::TextAlign::Left | crate::ui::TextAlign::Justify => x,
        crate::ui::TextAlign::Center => x + (max_width - para.max_intrinsic_width()).max(0.0) / 2.0,
        crate::ui::TextAlign::Right => x + (max_width - para.max_intrinsic_width()).max(0.0),
    };
    para.paint(canvas, x_offset, y);
}

fn surface_snapshot(canvas: &Canvas, bounds: skia_safe::IRect) -> Option<skia_safe::Image> {
    let surface = unsafe { canvas.surface() }?;
    let mut surface = surface.clone();
    surface.image_snapshot_with_bounds(bounds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tf_aux_text_paints() {
        // label/支持文本绘制路径：build_plain_paragraph + layout + paint
        let mut surface = skia_safe::surfaces::raster_n32_premul((200, 40)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::WHITE);
        draw_text_field_aux_text(
            canvas,
            "Name",
            12.0,
            crate::ui::text::FontWeight::NORMAL,
            crate::ui::text::FontSlant::Upright,
            0.0,
            None,
            &crate::modifier::Color::from_argb(255, 255, 0, 0),
            (10.0, 10.0),
            100.0,
        );
        let pm = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        // 文本区域应出现非白像素（红色文本）
        let mut found = 0;
        for y in 10..30 {
            for x in 10..60 {
                if px[y * 200 + x][2] > 200 { // R 通道（BGRA）
                    found += 1;
                }
            }
        }
        assert!(found > 20, "aux 文本必须可见（红色像素 {found} 个）");
    }

    #[test]
    fn fit_rect_preserves_aspect_and_centers() {
        let r = fit_rect(skia_safe::Rect::new(0.0, 0.0, 100.0, 50.0), 24.0, 24.0);
        assert_eq!((r.width(), r.height()), (50.0, 50.0), "等比缩放到短边");
        assert_eq!((r.left, r.top), (25.0, 0.0), "水平居中");
    }

    #[test]
    fn backdrop_snapshot_bounds_cover_3sigma() {
        // 快照物理像素边界必须覆盖 3σ 采样范围（blur sigma = radius，
        // 边缘像素采样 ±3r）——不足则 Clamp 重复像素产生边缘条纹。
        // 画布矩阵 = scale(1.5)（主画布 HiDPI 场景）。
        use skia_safe::Matrix;
        let mut m = Matrix::default();
        m.set_scale_x(1.5); m.set_scale_y(1.5);
        let (x, y, w, h, r) = (300.0f32, 250.0f32, 260.0f32, 170.0f32, 12.0f32);
        let corners = [
            m.map_xy(x, y), m.map_xy(x + w, y),
            m.map_xy(x, y + h), m.map_xy(x + w, y + h),
        ];
        let (sx0, sy0) = corners.iter().fold((f32::MAX, f32::MAX), |acc, p| (acc.0.min(p.x), acc.1.min(p.y)));
        let (sx1, sy1) = corners.iter().fold((f32::MIN, f32::MIN), |acc, p| (acc.0.max(p.x), acc.1.max(p.y)));
        let left = (sx0 - r * 3.0).floor() as i32;
        let top = (sy0 - r * 3.0).floor() as i32;
        let right = (sx1 + r * 3.0).ceil() as i32;
        let bottom = (sy1 + r * 3.0).ceil() as i32;
        assert!(sx0 - left as f32 >= r * 3.0 - 1.0, "左侧扩展覆盖 3σ");
        assert!(right as f32 - sx1 >= r * 3.0 - 1.0, "右侧扩展覆盖 3σ");
        assert!(sy0 - top as f32 >= r * 3.0 - 1.0, "顶部扩展覆盖 3σ");
        assert!(bottom as f32 - sy1 >= r * 3.0 - 1.0, "底部扩展覆盖 3σ");
    }

    #[test]
    fn backdrop_snapshot_irect_clamps_to_surface() {
        // 节点跨出顶/左边缘（如滚动内容滚出顶部）：快照边界必须 clamp
        // 到 surface——image_snapshot_with_bounds 返回图像锚定在相交原点，
        // 画回按 clamp 后原点锚定才不错位（旧实现 .max(0.0) 语义回归点）
        // 节点物理 AABB 在 (-50,-30)..(200,150)，surface 1350x960
        let b = backdrop_snapshot_irect(-50.0, -30.0, 200.0, 150.0, 36.0, 1350, 960).expect("有交集");
        assert_eq!((b.left, b.top), (0, 0), "顶/左边缘 clamp 到 0（相交原点锚定）");
        assert_eq!((b.right, b.bottom), (236, 186), "右侧/底部保持完整扩展");
        // 完全在 surface 外 → None
        assert!(backdrop_snapshot_irect(2000.0, 2000.0, 3000.0, 3000.0, 36.0, 1350, 960).is_none());
        // 右/下边缘 clamp
        let b2 = backdrop_snapshot_irect(1300.0, 900.0, 1360.0, 980.0, 36.0, 1350, 960).expect("有交集");
        assert_eq!((b2.right, b2.bottom), (1350, 960), "右/下边缘 clamp 到 surface 尺寸");
    }

    #[test]
    fn backdrop_draw_back_alignment_is_pixel_exact() {
        // 画回：快照原点经逆矩阵映射回画布逻辑坐标——再经画布矩阵后精确
        // 等于物理像素边界（像素网格对齐 → draw_image 零插值，移动不闪烁）
        use skia_safe::Matrix;
        let mut m = Matrix::default();
        m.set_scale_x(1.5); m.set_scale_y(1.5);
        let (x, y, r) = (300.0f32, 250.0f32, 12.0f32);
        let left = (x * 1.5 - r * 3.0).floor() as i32;
        let top = (y * 1.5 - r * 3.0).floor() as i32;
        let inv = m.invert().expect("invert");
        let origin = inv.map_xy(left as f32, top as f32);
        let back = m.map_xy(origin.x, origin.y);
        let (bdx, bdy) = (back.x, back.y);
        assert!((bdx - left as f32).abs() < 1e-3, "像素网格精确对齐：{bdx} vs {left}");
        assert!((bdy - top as f32).abs() < 1e-3, "像素网格精确对齐：{bdy} vs {top}");
    }

    #[test]
    fn backdrop_matrix_handles_ancestor_translate() {
        // 滚动容器/overlay 场景：画布含 translate（如 overlay 定位或 scroll
        // 偏移）——快照与画回必须基于矩阵映射的屏幕物理位置，裸逻辑坐标×sf
        // 会错位。验证：translate 下逆矩阵画回仍精确回到物理像素。
        use skia_safe::Matrix;
        let mut m = Matrix::default();
        m.set_scale_x(1.5); m.set_scale_y(1.5);
        m.set_translate_x(120.0); m.set_translate_y(80.0);
        let inv = m.invert().expect("invert");
        // 快照边界（矩阵映射后取整）
        let left = 450i32; let top = 300i32;
        let origin = inv.map_xy(left as f32, top as f32);
        let back = m.map_xy(origin.x, origin.y);
        let (bdx, bdy) = (back.x, back.y);
        assert!((bdx - left as f32).abs() < 1e-3, "translate 场景像素精确对齐：{bdx} vs {left}");
        assert!((bdy - top as f32).abs() < 1e-3, "translate 场景像素精确对齐：{bdy} vs {top}");
    }
    use crate::modifier::GraphicsLayerParams;

    #[test]
    fn test_gl_3d_matrix_none_without_3d() {
        let gl = GraphicsLayerParams::default();
        assert!(build_gl_3d_matrix(&gl, 200.0, 160.0).is_none(), "无 3D 旋转应走 2D 路径");
        let mut gl = GraphicsLayerParams::default();
        gl.rotation_z = 45.0; // 仅 rotationZ 不触发 3D 矩阵
        assert!(build_gl_3d_matrix(&gl, 200.0, 160.0).is_none());
    }

    #[test]
    fn test_gl_3d_matrix_perspective() {
        let mut gl = GraphicsLayerParams::default();
        gl.rotation_x = 45.0;
        let m = build_gl_3d_matrix(&gl, 200.0, 160.0).expect("rotationX 应返回矩阵");
        let mut row = [0.0f32; 16];
        m.get_row_major(&mut row);
        // 有效相机距离 = max(8, max(200,160)) = 200
        let s = 45f32.to_radians().sin();
        let c = 45f32.to_radians().cos();
        let persp_y = row[3 * 4 + 1];
        let persp_z = row[3 * 4 + 2];
        assert!((persp_y + s / 200.0).abs() < 1e-6, "w 行 y 系数应为 -sinθ/cam：{persp_y}");
        assert!((persp_z + c / 200.0).abs() < 1e-6, "w 行 z 系数应为 -cosθ/cam：{persp_z}");
        // 相机越远透视越平
        let mut near = GraphicsLayerParams::default();
        near.rotation_x = 45.0;
        near.camera_distance = 2.0; // 低于视图尺寸 → 钳制到 200
        let mn = build_gl_3d_matrix(&near, 200.0, 160.0).unwrap();
        let mut rown = [0.0f32; 16];
        mn.get_row_major(&mut rown);
        let mut far = GraphicsLayerParams::default();
        far.rotation_x = 45.0;
        far.camera_distance = 500.0;
        let mf = build_gl_3d_matrix(&far, 200.0, 160.0).unwrap();
        let mut rowf = [0.0f32; 16];
        mf.get_row_major(&mut rowf);
        assert!(rowf[13].abs() < rown[13].abs(), "相机距离大 → 透视项小");
    }

    #[test]
    fn test_low_elevation_shadow_is_visible() {
        // Elevated rest=1 阴影必须可感知（“平时有高度”），hover=3 明显更高——
        // 矩形四周采样应比纯白背景明显更暗，防强度曲线被调回不可见。
        use skia_safe::{Color, Paint, surfaces};
        let measure = |elevation: f32| -> (i32, i32, i32) {
            let mut surface = surfaces::raster_n32_premul((120, 120)).unwrap();
            let canvas = surface.canvas();
            canvas.clear(Color::WHITE);
            let rect = skia_safe::Rect::from_xywh(20.0, 20.0, 80.0, 80.0);
            // 真实管线顺序：先垫底阴影，再画内容
            draw_elevation_shadow(
                canvas,
                rect,
                &crate::modifier::Shape::Rectangle,
                elevation,
                crate::modifier::Color { r: 0, g: 0, b: 0, a: 0x19 },
                crate::modifier::Color { r: 0, g: 0, b: 0, a: 0x40 },
            );
            let mut paint = Paint::default();
            paint.set_color(Color::WHITE);
            canvas.draw_rect(rect, &paint);
            let pm = surface.peek_pixels().expect("pixmap");
            let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
            let at = |x: usize, y: usize| -> [u8; 4] { px[y * 120 + x] };
            let bg = at(4, 4);
            let dark = |p: [u8; 4]| -> i32 {
                (bg[0] as i32 - p[0] as i32)
                    + (bg[1] as i32 - p[1] as i32)
                    + (bg[2] as i32 - p[2] as i32)
            };
            (
                dark(at(60, 101)), // 下方 1px
                dark(at(60, 19)),  // 上方 1px
                dark(at(19, 60)),  // 左侧 1px
            )
        };
        let (rest_below, rest_above, rest_left) = measure(1.0);
        assert!(rest_left > 0, "rest 阴影应可见（左 1px）: {rest_left}");
        assert!(rest_above > 0, "rest 阴影应可见（上 1px）: {rest_above}");
        let (hover_below, hover_above, hover_left) = measure(3.0);
        assert!(hover_below > rest_below, "hover 下方阴影应高于 rest: rest={rest_below} hover={hover_below}");
        assert!(hover_above >= rest_above, "hover 上方阴影不应弱于 rest: rest={rest_above} hover={hover_above}");
        assert!(hover_left >= rest_left, "hover 左侧阴影不应弱于 rest: rest={rest_left} hover={hover_left}");
        assert!(
            hover_below > 0 && hover_above > 0 && hover_left > 0,
            "hover 阴影应在各方向可见: below={hover_below} above={hover_above} left={hover_left}"
        );
    }

    fn render_rect_with_gl(gl: &GraphicsLayerParams) -> (usize, usize, bool) {
        use skia_safe::{Color, Paint, surfaces};
        let mut surface = surfaces::raster_n32_premul((400, 200)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(Color::BLACK);
        // 节点位于非原点 (40,30)、尺寸 200x160、transformOrigin 中心——
        // 复刻 render.rs 的 graphicsLayer 序列（枢轴必须含节点绝对位置）
        let (x0, y0, w, h) = (40.0f32, 30.0f32, 200.0f32, 160.0f32);
        let (px, py) = (w * 0.5, h * 0.5);
        let (pivot_x, pivot_y) = (x0 + px, y0 + py);
        let sf = 1.0f32;
        // 根平移：把节点中心（绝对 140,110）送到画布中心 (200,100)
        canvas.translate((200.0 - pivot_x * sf, 100.0 - pivot_y * sf));
        canvas.scale((sf, sf));
        apply_gl_transform(canvas, gl, x0, y0, w, h);
        let mut paint = Paint::default();
        paint.set_color(Color::WHITE);
        canvas.draw_rect(skia_safe::Rect::from_xywh(x0, y0, w, h), &paint);
        let pm = surface.peek_pixels().expect("pixmap");
        let px2: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        let at = |x: usize, y: usize| -> [u8; 4] { px2[y * 400 + x] };
        let center_white = at(200, 100) == [255, 255, 255, 255];
        let mut top = 200usize;
        let mut bottom = 0usize;
        for y in 0..200 {
            for x in (0..400).step_by(2) {
                if at(x, y) == [255, 255, 255, 255] {
                    if y < top { top = y; }
                    if y > bottom { bottom = y; }
                    break;
                }
            }
        }
        (top, bottom, center_white)
    }

    #[test]
    fn test_rotation_pivot_is_node_center() {
        // 非原点节点：旋转后中心像素必须仍在节点中心（枢轴含绝对位置）
        let mut gl = GraphicsLayerParams::default();
        gl.rotation_x = 45.0;
        gl.camera_distance = 150.0;
        let (top, bottom, center_white) = render_rect_with_gl(&gl);
        assert!(bottom > top, "矩形应可见：top={top} bottom={bottom}");
        assert!(center_white, "旋转应绕节点中心——中心像素应保持不动");
    }

    #[test]
    fn test_perspective_renders_in_full_sequence() {
        let mut gl = GraphicsLayerParams::default();
        gl.rotation_x = 45.0;
        gl.camera_distance = 300.0;
        let (top, bottom, _) = render_rect_with_gl(&gl);
        assert!(bottom > top, "矩形应可见：top={top} bottom={bottom}");
        // 与近似无透视（相机极远 → w≈1）的投影高度对比——透视会压缩投影
        let mut affine = GraphicsLayerParams::default();
        affine.rotation_x = 45.0;
        affine.camera_distance = 1.0e9;
        let (top_a, bottom_a, _) = render_rect_with_gl(&affine);
        let persp_h = (bottom - top) as f32;
        let affine_h = (bottom_a - top_a) as f32;
        assert!(
            (persp_h - affine_h).abs() > 2.0,
            "透视应显著压缩投影高度：persp_h={persp_h} affine_h={affine_h}"
        );
    }

    #[test]
    fn test_camera_distance_changes_projection() {
        let mut near = GraphicsLayerParams::default();
        near.rotation_x = 45.0;
        near.camera_distance = 20.0;
        let mut far = GraphicsLayerParams::default();
        far.rotation_x = 45.0;
        far.camera_distance = 500.0;
        let (t1, b1, _) = render_rect_with_gl(&near);
        let (t2, b2, _) = render_rect_with_gl(&far);
        assert_ne!((t1, b1), (t2, b2), "相机距离应改变投影结果");
    }

    #[test]
    fn test_rotation_never_disappears_with_default_camera() {
        // 默认相机 8 对 160dp 卡片：有效距离钳制到半尺寸，任意角度 w>0
        let mut gl = GraphicsLayerParams::default();
        gl.rotation_x = 75.0; // 75° 仍可见（90° 侧立成线是正确行为）
        let (top, bottom, _) = render_rect_with_gl(&gl);
        assert!(bottom > top, "rotationX=75 默认相机应仍可见：top={top} bottom={bottom}");
    }

    #[test]
    fn graphics_layer_color_filter_tints_content() {
        // 完整渲染路径回归：graphics_layer color_filter(Tint SrcIn) 必须
        // 经 saveLayer paint 染色层内内容（白背景 → 红）——render_pass1
        // 的 save_layer + paint color_filter 分支。
        use crate::core::composer::Composer;
        use crate::layout::constraints::Constraints;
        use crate::modifier::{BlendMode, ColorFilter, Modifier, Shape};
        use skia_safe::{surfaces, Color as SkColor};

        let mut c = Composer::new();
        c.compose(|ctx| {
            let k = ctx.next_key();
            ctx.start_leaf(
                k,
                Modifier::new()
                    .size(100.0, 100.0)
                    .background(crate::modifier::Color::from_argb(255, 255, 255, 255), Shape::Rectangle)
                    .graphics_layer(crate::modifier::GraphicsLayerParams {
                        color_filter: Some(ColorFilter::Tint {
                            color: crate::modifier::Color::from_argb(255, 255, 0, 0),
                            blend_mode: BlendMode::SrcIn,
                        }),
                        ..crate::modifier::GraphicsLayerParams::default()
                    }),
            );
            ctx.end_node();
        });
        c.layout(Constraints::new(0.0, 100.0, 0.0, 100.0));
        let root = c.layout_root_idx().unwrap();

        let mut surface = surfaces::raster_n32_premul((100, 100)).unwrap();
        surface.canvas().clear(SkColor::BLACK);
        crate::render::render(c.arena_nodes(), root, surface.canvas());

        let mut px = [0u8; 4];
        let info = skia_safe::ImageInfo::new(
            (1, 1),
            skia_safe::ColorType::RGBA8888,
            skia_safe::AlphaType::Premul,
            None,
        );
        surface.read_pixels(&info, &mut px, 4, (50, 50));
        // 中心像素：白底被 Tint(SrcIn) 染成红（R 高、G/B 低）
        assert!(px[0] > 200 && px[1] < 60 && px[2] < 60,
            "中心像素应为红（染白→红），实际 {:?}", px);
    }

    #[test]
    fn test_clip_applies_before_transform() {
        use skia_safe::{Color, Paint, surfaces};
        let render = |clip: bool| -> usize {
            let mut surface = surfaces::raster_n32_premul((400, 200)).unwrap();
            let canvas = surface.canvas();
            canvas.clear(Color::BLACK);
            let (x0, y0, w, h) = (40.0f32, 30.0f32, 200.0f32, 160.0f32);
            let mut gl = GraphicsLayerParams::default();
            gl.rotation_z = 45.0;
            gl.clip = clip;
            canvas.save();
            if clip {
                canvas.clip_rect(Rect::new(x0, y0, x0 + w, y0 + h), None, true);
            }
            apply_gl_transform(canvas, &gl, x0, y0, w, h);
            let mut paint = Paint::default();
            paint.set_color(Color::WHITE);
            canvas.draw_rect(Rect::from_xywh(x0, y0, w, h), &paint);
            canvas.restore();
            let pm = surface.peek_pixels().unwrap();
            let px2: &[[u8; 4]] = pm.pixels::<[u8; 4]>().unwrap();
            px2.iter().filter(|p| **p == [255, 255, 255, 255]).count()
        };
        let unclipped = render(false);
        let clipped = render(true);
        assert!(clipped > 0, "clip 后内容应仍可见");
        assert!(
            unclipped > clipped,
            "clip 应在变换前裁掉旋转后的角落：unclipped={unclipped} clipped={clipped}"
        );
    }
}
