//! 渲染管线 — 遍历 LayoutNode 树绘制到 Skia Canvas
//!
//! 两阶段渲染：
//!   Phase 1: 非 BackdropBlur 内容 + 收集背景模糊区域
//!   Phase 2: snapshot → 每个模糊区域 crop → blur → 画回 + 画子节点

use crate::debug_log;
use crate::layout::node::LayoutNode;
use crate::modifier::ModifierElement;
use skia_safe::{Canvas, Color4f, Paint, RRect, Rect};
use skia_safe::image_filters;

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
/// graphicsLayer 3D 变换矩阵（rotationX/Y + cameraDistance 透视）。
/// 顺序：scale → rotationZ → rotationY → rotationX → 相机透视
/// （矩阵左乘序与 Compose RenderNode 一致；调用方负责先平移到 pivot）。
/// 无 3D 旋转时返回 None（走 2D 路径）。
fn build_gl_3d_matrix(gl: &crate::modifier::GraphicsLayerParams) -> Option<skia_safe::M44> {
    if gl.rotation_x == 0.0 && gl.rotation_y == 0.0 {
        return None;
    }
    let mut m = skia_safe::M44::new_identity();
    m.pre_scale_xyz(gl.scale_x, gl.scale_y, 1.0);
    if gl.rotation_z != 0.0 {
        m.pre_concat(&skia_safe::M44::rotate(
            skia_safe::V3::new(0.0, 0.0, 1.0),
            gl.rotation_z.to_radians(),
        ));
    }
    if gl.rotation_y != 0.0 {
        m.pre_concat(&skia_safe::M44::rotate(
            skia_safe::V3::new(0.0, 1.0, 0.0),
            gl.rotation_y.to_radians(),
        ));
    }
    if gl.rotation_x != 0.0 {
        m.pre_concat(&skia_safe::M44::rotate(
            skia_safe::V3::new(1.0, 0.0, 0.0),
            gl.rotation_x.to_radians(),
        ));
    }
    // 相机透视（Compose Camera 近似）：w' = 1 - z/cameraDistance
    let cam = gl.camera_distance.max(1.0);
    m.set_rc(3, 2, -1.0 / cam);
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
    let strength = (elevation / 12.0).min(1.0);
    let color = Color::from_argb(255, 0, 0, 0);
    let ambient = ShadowParams::new(elevation, 0.0, 0.0, color, 0.18 * strength);
    let spot = ShadowParams::new(
        elevation * 0.25,
        0.0,
        elevation * 0.5,
        color,
        0.30 * strength,
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
        _ => None,
    }
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
    // graphicsLayer shadowElevation：与 Modifier.shadow 同层垫底（图层变换前——
    // 阴影基于未变换的节点形状，对标 Compose 层阴影）
    if !backdrop_pass {
        if let Some(gl) = &gl_params {
            if gl.shadow_elevation > 0.0 {
                let shape = gl.shadow_shape.clone().unwrap_or(crate::modifier::Shape::Rectangle);
                draw_elevation_shadow(canvas, rect, &shape, gl.shadow_elevation);
            }
        }
    }
    let gl_saved = if let Some(gl) = gl_params {
        if gl.alpha < 1.0 {
            canvas.save_layer_alpha_f(None, gl.alpha);
        } else {
            canvas.save();
        }
        // clip 到节点 bounds（内容坐标系——变换前应用，对标 Compose clip 语义）
        if gl.clip {
            canvas.clip_rect(rect, None, true);
        }
        // transformOrigin：先平移到 pivot → 变换 → 平移回（对标 Compose
        // transformOrigin 默认 Center——scale/rotate 绕中心而非左上）
        let (px, py) = (w * gl.transform_origin.0, h * gl.transform_origin.1);
        canvas.translate((gl.translation_x, gl.translation_y));
        canvas.translate((px, py));
        // 3D 旋转（rotationX/Y + cameraDistance 透视）：整体拼成 4x4 矩阵
        // （scale → rotationZ → rotationY → rotationX → 相机透视），
        // 与 2D 路径共用 pivot 语义
        match build_gl_3d_matrix(&gl) {
            Some(m) => { canvas.concat_44(&m); }
            None => {
                canvas.scale((gl.scale_x, gl.scale_y));
                canvas.rotate(gl.rotation_z, None);
            }
        }
        canvas.translate((-px, -py));
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
                if let Some(tp) = render_modifier_element(canvas, el, rect, x, y, w, h) {
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
            crate::modifier::Shape::Circle => {
                canvas.clip_rect(rect, None, Some(false));
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

    if node.focused {
        draw_focus(canvas, rect);
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
        let ModifierElement::Ripple { source, color, bounded } = el else { continue };

        let diagonal = (w * w + h * h).sqrt();
        let rect = Rect::new(x, y, x + w, y + h);

        // bounded：裁剪到节点背景形状（无 Background 则按矩形）
        let mut clipped = false;
        if *bounded {
            let shape = node.modifier.elements().iter().rev().find_map(|el| {
                if let ModifierElement::Background { shape, .. } = el {
                    Some(shape.clone())
                } else {
                    None
                }
            });
            canvas.save();
            match shape.as_ref() {
                Some(crate::modifier::Shape::RoundedRect { corner_radius }) => {
                    canvas.clip_rrect(
                        skia_safe::RRect::new_rect_xy(rect, *corner_radius, *corner_radius),
                        None,
                        Some(false),
                    );
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
            // 按压点已是场景（画布）坐标——非滚动/无 graphicsLayer 变换的节点直接可用
            canvas.draw_circle(
                skia_safe::Point::new(layer.center.0, layer.center.1),
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
        crate::modifier::Shape::Circle => {
            canvas.draw_circle((sr.center_x(), sr.center_y()), sr.width().min(sr.height()) / 2.0, &paint);
        }
    }
}

fn draw_focus(canvas: &Canvas, rect: Rect) {
    let mut paint = Paint::default();
    paint.set_color4f(Color4f::new(0.3, 0.6, 1.0, 0.8), None);
    paint.set_style(skia_safe::paint::Style::Stroke);
    paint.set_stroke_width(2.0);
    paint.set_anti_alias(true);
    canvas.draw_rect(rect, &paint);
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
    use crate::modifier::GraphicsLayerParams;

    #[test]
    fn test_gl_3d_matrix_none_without_3d() {
        let gl = GraphicsLayerParams::default();
        assert!(build_gl_3d_matrix(&gl).is_none(), "无 3D 旋转应走 2D 路径");
        let mut gl = GraphicsLayerParams::default();
        gl.rotation_z = 45.0; // 仅 rotationZ 不触发 3D 矩阵
        assert!(build_gl_3d_matrix(&gl).is_none());
    }

    #[test]
    fn test_gl_3d_matrix_perspective() {
        let mut gl = GraphicsLayerParams::default();
        gl.rotation_x = 45.0;
        let m = build_gl_3d_matrix(&gl).expect("rotationX 应返回矩阵");
        let mut row = [0.0f32; 16];
        m.get_row_major(&mut row);
        // 透视项 row3 col2（w 行 z 系数）= -1/cameraDistance
        let persp = row[3 * 4 + 2];
        assert!(persp < 0.0, "透视项应为负：{persp}");
        assert!((persp + 1.0 / 8.0).abs() < 1e-6, "默认相机距离 8 的透视项应为 -1/8");
        // 相机越远透视越平
        let mut far = GraphicsLayerParams::default();
        far.rotation_x = 45.0;
        far.camera_distance = 100.0;
        let mf = build_gl_3d_matrix(&far).unwrap();
        let mut rowf = [0.0f32; 16];
        mf.get_row_major(&mut rowf);
        assert!(rowf[14].abs() < persp.abs(), "相机距离大 → 透视项小");
    }
}
