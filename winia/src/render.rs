//! 渲染管线 — 遍历 LayoutNode 树绘制到 Skia Canvas
//!
//! 两阶段渲染：
//!   Phase 1: 非 BackdropBlur 内容 + 收集背景模糊区域
//!   Phase 2: snapshot → 每个模糊区域 crop → blur → 画回 + 画子节点

use crate::debug_log;
use crate::layout::LayoutDirection;
use crate::layout::node::LayoutNode;
use crate::modifier::ModifierElement;
use crate::ui::icon::{DecodedIcon, IconSource, IconSpec, decoded_icon};
use skia_safe::{BlendMode, Canvas, Color4f, Paint, RRect, Rect, SamplingOptions};
use skia_safe::sampling_options::{FilterMode, MipmapMode};
use skia_safe::image_filters;
use skia_safe::svg;
use std::cell::RefCell;

// ── 入口 ──

pub fn render(nodes: &[LayoutNode], root_idx: usize, canvas: &Canvas) {
    let mut backdrop_regions = Vec::new();
    // Phase 1: 非背景模糊内容 + 收集模糊区域
    render_pass1(nodes, root_idx, canvas, 0.0, 0.0, &mut backdrop_regions, false);
    // Phase 2: 背景模糊
    if !backdrop_regions.is_empty() {
        render_backdrop_blur(canvas, nodes, &backdrop_regions);
    }
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

/// graphicsLayer shadowElevation：按 Modifier.shadow 同款参数画
/// ambient + spot 两层（垫底——图层变换前绘制）
fn draw_elevation_shadow(
    canvas: &Canvas,
    rect: Rect,
    shape: &crate::modifier::Shape,
    elevation: f32,
) {
    use crate::modifier::{Color, ShadowParams};
    // sqrt 压缩：低高度（1~3dp）也有可感知投影，同时保留高阶（8/12）的
    // 相对差异——线性 1/12 会让 Elevated rest=1 几乎不可见。
    let strength = (elevation / 12.0).sqrt().min(1.0);
    let color = Color::from_argb(255, 0, 0, 0);
    // 系数 0.28/0.42 为实测可辨的最小档；配合 sqrt 压缩，rest 1 可见、
    // hover 3 明显更高。
    let ambient = ShadowParams::new(elevation, 0.0, 0.0, color, 0.28 * strength);
    let spot = ShadowParams::new(
        elevation * 0.25,
        0.0,
        elevation * 0.5,
        color,
        0.42 * strength,
    );
    draw_shadow_layer(canvas, rect, shape, &ambient);
    draw_shadow_layer(canvas, rect, shape, &spot);
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
    direction: LayoutDirection,
) -> Option<TextParams<'a>> {
    match el {
        ModifierElement::Background { color_fn, shape } => {
            draw_background(canvas, rect, &(color_fn)(), shape);
            None
        }
        ModifierElement::Border { width, color, shape } => {
            draw_border(canvas, x, y, w, h, *width, color, shape);
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
        ModifierElement::DrawIcon { spec } => {
            draw_icon(canvas, rect, direction, spec);
            None
        }
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
                        draw_svg_dom(canvas, dom, *width, *height, rect, spec.tint);
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
    vw: f32,
    vh: f32,
    rect: Rect,
    tint: Option<crate::modifier::Color>,
) {
    let dst = fit_rect(rect, vw, vh);
    if dst.width() <= 0.0 || dst.height() <= 0.0 {
        return;
    }
    // 直接画布渲染（与字体路径一致）：容器尺寸设为目标尺寸后 render。
    // tint 用 saveLayer 颜色滤镜（SrcIn）在图层合成时统一染色。
    canvas.save();
    canvas.translate((dst.left, dst.top));
    let layer = if let Some(tint) = tint {
        skia_safe::color_filters::blend(skia_color(tint), BlendMode::SrcIn).map(|filter| {
            let mut paint = Paint::default();
            paint.set_color_filter(filter);
            canvas.save_layer(&skia_safe::canvas::SaveLayerRec::default().paint(&paint));
        })
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

// ── Phase 1: 正常渲染（非 BackdropBlur 节点）──

fn render_pass1(
    nodes: &[LayoutNode],
    idx: usize,
    canvas: &Canvas,
    parent_x: f32, parent_y: f32,
    backdrop_regions: &mut Vec<(f32, f32, f32, f32, f32, usize)>,
    backdrop_pass: bool,
) {
    let node = &nodes[idx];
    let x = parent_x + node.position.x;
    let y = parent_y + node.position.y;
    let w = node.measured_size.width;
    let h = node.measured_size.height;
    if w <= 0.0 || h <= 0.0 { return; }

    let rect = Rect::new(x, y, x + w, y + h);
    // 图形层：包住整个节点（background + text + children），应用 alpha/变换
    let gl_params = node.modifier.graphics_layer_params();
    let gl_saved = if let Some(gl) = gl_params {
        if gl.alpha < 1.0 {
            canvas.save_layer_alpha_f(None, gl.alpha);
        } else {
            canvas.save();
        }
        // graphicsLayer shadowElevation：独立“变换 → 绘制 → 恢复”——阴影
        // 随 3D 形变，且不参与内容 clip（对标 Compose：层阴影在边界外）
        if gl.shadow_elevation > 0.0 && !backdrop_pass {
            canvas.save();
            apply_gl_transform(canvas, &gl, x, y, w, h);
            let shape = gl.shadow_shape.clone().unwrap_or(crate::modifier::Shape::Rectangle);
            draw_elevation_shadow(canvas, rect, &shape, gl.shadow_elevation);
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
    let mut is_backdrop = false;
    let mut clip_shape: Option<crate::modifier::Shape> = None;
    // 阴影（elevation, shape, color）——链序中与 background 同层绘制；
    // clip=true 时并入 clip_shape（内容裁剪，阴影不受裁——Compose 语义）
    // 阴影层（预扫描直接绘制——见下方 pre-scan 注释）
    let mut text: Option<(&str, f32, &crate::modifier::Color, usize, crate::ui::TextAlign, crate::ui::TextOverflow, crate::ui::text::FontWeight, crate::ui::text::FontSlant, bool, f32, Option<f32>)> = None;
    let mut scroll_offset_v: Option<f32> = None;
    let mut scroll_offset_h: Option<f32> = None;

    // ⚠ 阴影必须**垫底**（主循环绘制背景之前——无论链序）：Compose shadow
    // 是 graphicsLayer 独立层（垫底）。此前预扫描代码误放在主循环之后——
    // 阴影画在背景之上 → 卡片被压暗（绿卡 ×(1-0.43)≈0.6——实测 bug）
    if !backdrop_pass {
        for el in node.modifier.elements() {
            if let ModifierElement::Shadow { params, shape, .. } = el {
                draw_shadow_layer(canvas, rect, shape, params);
            }
        }
    }

    for el in node.modifier.elements() {
        match el {
            ModifierElement::Blur { radius } if !backdrop_pass => {
                blur_radius = Some(*radius);
            }
            ModifierElement::BackdropBlur { radius } if !backdrop_pass => {
                is_backdrop = true;
                backdrop_regions.push((x, y, w, h, *radius, idx));
            }
            ModifierElement::Clip { shape } if !backdrop_pass => {
                clip_shape = Some(shape.clone());
            }
            ModifierElement::Shadow { shape, clip, .. } if !backdrop_pass => {
                if *clip {
                    clip_shape = Some(shape.clone());
                }
            }
            ModifierElement::VerticalScroll { state } if !backdrop_pass => {
                scroll_offset_v = Some(state.get());
            }
            ModifierElement::HorizontalScroll { state } if !backdrop_pass => {
                scroll_offset_h = Some(state.get());
            }
            el => {
                if let Some(tp) = render_modifier_element(canvas, el, rect, x, y, w, h, node.layout_direction) {
                    text = Some((tp.content, tp.font_size, tp.color, tp.max_lines, tp.align, tp.overflow, tp.font_weight, tp.font_style, tp.soft_wrap, tp.letter_spacing, tp.line_height));
                }
            }
        }
    }

    // 内容模糊：saveLayer
    if !backdrop_pass {
        if let Some(r) = blur_radius {
        let mut paint = Paint::default();
        paint.set_image_filter(image_filters::blur((r, r), skia_safe::TileMode::Clamp, None, None));
        let rec = skia_safe::canvas::SaveLayerRec::default().paint(&paint);
        canvas.save_layer(&rec);
    }
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
            para.layout(w);
            let x_off = match align {
                crate::ui::TextAlign::Left | crate::ui::TextAlign::Justify => x,
                crate::ui::TextAlign::Center => x + (w - para.max_intrinsic_width()).max(0.0) / 2.0,
                crate::ui::TextAlign::Right => x + (w - para.max_intrinsic_width()).max(0.0),
            };
            // 选中高亮
            if let Some(range) = node.registrar.borrow().as_ref().cloned().unwrap_or_else(|| crate::ui::selection_container::active_registrar()).selected_range(node.slot_key) {
                debug_log!("[selection] render node={} range={}..{}", node.id, range.start, range.end);
                let rects: Vec<_> = if range.start < range.end {
                para.get_rects_for_range(range.start..range.end, skia_safe::textlayout::RectHeightStyle::Max, skia_safe::textlayout::RectWidthStyle::Max) } else { Vec::new() };
                let mut paint = skia_safe::Paint::default();
                paint.set_color(skia_safe::Color::from_argb(80, 100, 150, 255));
                for tb in &rects {
                    canvas.draw_rect(skia_safe::Rect::new(x_off + tb.rect.left, y + tb.rect.top, x_off + tb.rect.right, y + tb.rect.bottom), &paint);
                }
            }
            para.paint(canvas, x_off, y);
            // 绘制选中高亮（selection.start != selection.end）
            let sel_range = node.selection_range.borrow().clone()
                .filter(|r| r.start < r.end);
            if let Some(ref range) = sel_range {
                if range.start < range.end {
                    let rects = para.get_rects_for_range(range.clone(), skia_safe::textlayout::RectHeightStyle::Max, skia_safe::textlayout::RectWidthStyle::Max);
                    let mut sel_paint = skia_safe::Paint::default();
                    let tc = crate::ui::theme::WiniaTheme::colors().primary;
                    sel_paint.set_color(skia_safe::Color::from_argb(60, tc.r, tc.g, tc.b));
                    for tb in &rects {
                        canvas.draw_rect(skia_safe::Rect::new(x_off + tb.rect.left, y + tb.rect.top, x_off + tb.rect.right, y + tb.rect.bottom), &sel_paint);
                    }
                }
            }
            // 绘制光标（聚焦的 TextField 节点）
            if node.focused {
                debug_log!("[render] cursor focused=true idx={} cursor_visible={}", node.cursor_index.get(), node.cursor_visible.get());
                if node.cursor_visible.get() {
                    let length = para.paragraph_byte_to_real_indices.len();
                    let tl = crate::text::TextLayout::new(para, length);
                    let idx = node.cursor_index.get();
                    if let Some((cx, cy, ch)) = tl.get_cursor_position(idx) {
                        debug_log!("[render] cursor pos=({:.0},{:.0}) h={:.0}", cx, cy, ch);
                        let mut cp = skia_safe::Paint::default();
                        cp.set_color(skia_safe::Color::from_argb(255, color.r, color.g, color.b));
                        cp.set_stroke_width(1.5);
                        canvas.draw_line(skia_safe::Point::new(x_off + cx, y + cy), skia_safe::Point::new(x_off + cx, y + cy + ch), &cp);
                    } else { debug_log!("[render] get_cursor_position returned None for idx={}", node.cursor_index.get()); }
                } else { debug_log!("[render] cursor_visible is false"); }
            }
            // IME 组合文本下划线
            if let Some(comp_range) = node.composing_range.borrow().as_ref() {
                if comp_range.start < comp_range.end {
                    let rects = para.get_rects_for_range(comp_range.start..comp_range.end, skia_safe::textlayout::RectHeightStyle::Max, skia_safe::textlayout::RectWidthStyle::Max);
                    let mut und_paint = skia_safe::Paint::default();
                    let c = crate::ui::theme::WiniaTheme::colors().primary;
                    und_paint.set_color(skia_safe::Color::from_argb(c.a, c.r, c.g, c.b));
                    und_paint.set_stroke_width(1.0);
                    for tb in &rects {
                        let r = tb.rect;
                        canvas.draw_line(skia_safe::Point::new(x_off + r.left, y + r.bottom), skia_safe::Point::new(x_off + r.right, y + r.bottom), &und_paint);
                    }
                }
            }
        } else {
            draw_text_with_selection(canvas, content, font_size, color, font_weight, font_style, x, y, w, max_lines, align, overflow, soft_wrap, letter_spacing, line_height, node.slot_key);
        }
    }

    // ═══ 富文本（RichText）渲染 ═══
    if node.has_richtext_content {
        if let Some(para) = node.cached_paragraph.borrow_mut().as_mut() {
            para.layout(w);
            // 选中高亮
            if let Some(range) = node.registrar.borrow().as_ref().cloned().unwrap_or_else(|| crate::ui::selection_container::active_registrar()).selected_range(node.slot_key) {
                let rects: Vec<_> = if range.start < range.end {
                para.get_rects_for_range(range.start..range.end, skia_safe::textlayout::RectHeightStyle::Max, skia_safe::textlayout::RectWidthStyle::Max) } else { Vec::new() };
                let mut paint = skia_safe::Paint::default();
                paint.set_color(skia_safe::Color::from_argb(80, 100, 150, 255));
                for tb in &rects {
                    canvas.draw_rect(skia_safe::Rect::new(x + tb.rect.left, y + tb.rect.top, x + tb.rect.right, y + tb.rect.bottom), &paint);
                }
            }
            para.paint(canvas, x, y);
        }
    }

    // 焦点环：聚焦淡入、失焦淡出——透明度来自交互源 focus_indicator_alpha
    // （失焦后动画期间 alpha>0 继续绘制，实现平滑淡出）
    let focus_alpha = node.modifier.focusable_interaction()
        .map(|src| src.focus_indicator_alpha_value())
        .unwrap_or(if node.focused { 1.0 } else { 0.0 });
    if node.focused || focus_alpha > 0.001 {
        // 焦点环形状跟随组件（最近 Background/Border/Clip 形状，回退矩形）；
        // 颜色由组件组合期从主题捕获（node.focus_color）
        let focus_shape = node.modifier.elements().iter().rev().find_map(|el| match el {
            ModifierElement::Background { shape, .. }
            | ModifierElement::Border { shape, .. }
            | ModifierElement::Clip { shape } => Some(*shape),
            _ => None,
        }).unwrap_or(crate::modifier::Shape::Rectangle);
        draw_focus(canvas, rect, &focus_shape, node.focus_color.get(), focus_alpha);
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
        let clip_rect = Rect::new(x, y, x + cw, y + ch);
        let dx = -scroll_offset_h.unwrap_or(0.0);
        let dy = -scroll_offset_v.unwrap_or(0.0);
        canvas.save();
        canvas.clip_rect(clip_rect, None, Some(false));
        canvas.translate((dx, dy));
        scrolled = true;
    }

    // 穿行子节点（背景模糊节点跳过子节点——Phase 2 处理）
    if !is_backdrop {
        for &child in &node.children {
            render_pass1(nodes, child, canvas, x, y, backdrop_regions, false);
        }
    }

    // 水波纹（indication ripple）——覆盖内容之上、受 shape/scroll 裁剪
    draw_ripple(node, canvas, x, y, w, h);

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
/// - 实心圆（非径向渐变）：状态层 = 节点中心大圆（半径=对角线/2），
///   波纹层 = 按压点实心圆（半径=对角线×progress）
/// - 绘制前裁剪到节点背景形状（bounded）——避免圆溢出圆角按钮
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
                    ModifierElement::Background { shape, .. } | ModifierElement::Border { shape, .. } => {
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

        // ── 波纹层：按压点实心圆（半径=对角线×progress，旧版 draw_circle）──
        // 中心存的是节点本地坐标（press_interaction_down 已从场景坐标换算）；
        // 画布坐标 = 布局原点 + 本地坐标——祖先 scroll translate 与自身
        // graphics_layer 变换都已作用在画布上，波纹视觉位置自动跟随节点
        // （按下后滚动/变换动画中不脱离按钮）。
        for layer in source.ripple_layers() {
            let progress = layer.progress.get();
            let opacity = layer.opacity.get();
            let radius = diagonal * progress;
            if radius <= 0.0 || opacity <= 0.0 {
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
                radius,
                &paint,
            );
        }

        if clipped {
            canvas.restore();
        }
    }
}

// ── Phase 2: 背景模糊 ──

fn render_backdrop_blur(
    canvas: &Canvas,
    nodes: &[LayoutNode],
    regions: &[(f32, f32, f32, f32, f32, usize)],
) {
    // 取整张 surface snapshot（只回读一次）
    let mut snap_bounds: Option<Rect> = None;
    for &(x, y, w, h, r, _) in regions {
        let m = r * 2.0;
        let b = Rect::new((x - m).max(0.0), (y - m).max(0.0), x + w + m, y + h + m);
        snap_bounds = Some(match snap_bounds {
            Some(prev) => Rect::new(
                prev.left.min(b.left), prev.top.min(b.top),
                prev.right.max(b.right), prev.bottom.max(b.bottom),
            ),
            None => b,
        });
    }

    let snapshot = if let Some(bounds) = snap_bounds {
        let bi = skia_safe::IRect::from_ltrb(
            bounds.left as i32, bounds.top as i32,
            bounds.right as i32, bounds.bottom as i32,
        );
        if bi.width() > 0 && bi.height() > 0 {
            surface_snapshot(canvas, bi)
        } else {
            None
        }
    } else {
        None
    };

    // 为每个背景模糊区域画回
    for &(x, y, w, h, radius, backdrop_idx) in regions {
        if let Some(ref snap) = snapshot {
            let mut paint = Paint::default();
            paint.set_image_filter(image_filters::blur((radius, radius), skia_safe::TileMode::Clamp, None, None));
            let m = radius * 2.0;
            let src = Rect::new((x - m).max(0.0), (y - m).max(0.0), x + w + m, y + h + m);
            canvas.draw_image_rect(snap, None, &src, &paint);
        }
        // 画回模糊节点自己的子节点
        for &child in &nodes[backdrop_idx].children {
            render_pass1(nodes, child, canvas, x, y, &mut Vec::new(), true);
        }
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

fn draw_background(canvas: &Canvas, rect: Rect, color: &crate::modifier::Color, shape: &crate::modifier::Shape) {
    let mut paint = Paint::default();
    paint.set_color4f(Color4f::from(color), None);
    paint.set_anti_alias(true);
    match shape {
        crate::modifier::Shape::Rectangle => { canvas.draw_rect(rect, &paint); }
        crate::modifier::Shape::RoundedRect { corner_radius } => {
            canvas.draw_rrect(RRect::new_rect_xy(rect, *corner_radius, *corner_radius), &paint);
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
        crate::modifier::Shape::Pill => {
            let r = (sr.width().min(sr.height()) / 2.0 - inset).max(0.0);
            canvas.draw_rrect(RRect::new_rect_xy(sr, r, r), &paint);
        }
        crate::modifier::Shape::Circle => {
            canvas.draw_circle((sr.center_x(), sr.center_y()), sr.width().min(sr.height()) / 2.0, &paint);
        }
    }
}

/// 焦点环（M3 focus indicator）——宽 3、完全位于组件**外部**（环内侧距
/// 组件边缘 2）、形状跟随组件、颜色来自组件组合期捕获的主题色。
fn draw_focus(
    canvas: &Canvas,
    rect: Rect,
    shape: &crate::modifier::Shape,
    color: crate::modifier::Color,
    alpha: f32,
) {
    const FOCUS_GAP: f32 = 2.0;   // 环内侧与组件边缘的距离
    const FOCUS_WIDTH: f32 = 3.0;
    const FOCUS_SCALE_AMOUNT: f32 = 0.15; // 淡入起点放大倍数（大环收缩到贴合）
    // 环中心线在组件外 gap + 半宽处——stroke 居中绘制时环完全在外侧
    let inset = FOCUS_GAP + FOCUS_WIDTH / 2.0;
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
    slot_key: u64,
) {
    if crate::ui::selection_container::active_registrar().selected_range(slot_key).is_some() {
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
    fn fit_rect_preserves_aspect_and_centers() {
        let r = fit_rect(skia_safe::Rect::new(0.0, 0.0, 100.0, 50.0), 24.0, 24.0);
        assert_eq!((r.width(), r.height()), (50.0, 50.0), "等比缩放到短边");
        assert_eq!((r.left, r.top), (25.0, 0.0), "水平居中");
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
            let mut paint = Paint::default();
            paint.set_color(Color::WHITE);
            canvas.draw_rect(rect, &paint);
            draw_elevation_shadow(
                canvas,
                rect,
                &crate::modifier::Shape::Rectangle,
                elevation,
            );
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
                dark(at(60, 102)), // 下方 2px
                dark(at(60, 18)),  // 上方 2px
                dark(at(18, 60)),  // 左侧 2px
            )
        };
        let measure = |elevation: f32| -> (i32, i32, i32) {
            let mut surface = surfaces::raster_n32_premul((120, 120)).unwrap();
            let canvas = surface.canvas();
            canvas.clear(Color::WHITE);
            let rect = skia_safe::Rect::from_xywh(20.0, 20.0, 80.0, 80.0);
            // 真实管线顺序：先垫底阴影，再画内容
            draw_elevation_shadow(canvas, rect, &crate::modifier::Shape::Rectangle, elevation);
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
        assert!(rest_left >= 40, "rest 阴影过弱（左 1px）: {rest_left}");
        assert!(rest_above >= 5, "rest 阴影过弱（上 1px）: {rest_above}");
        let (hover_below, hover_above, hover_left) = measure(3.0);
        assert!(hover_below >= 60, "hover 下方阴影过弱: {hover_below}");
        assert!(hover_above >= 20, "hover 上方阴影过弱: {hover_above}");
        assert!(hover_left >= 30, "hover 左侧阴影过弱: {hover_left}");
        assert!(
            hover_below > rest_below,
            "hover 阴影应高于 rest: rest={rest_below} hover={hover_below}"
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
