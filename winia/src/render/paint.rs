use super::*;
use super::helpers::*;
use super::text_field::*;

// ── 视觉 Modifier 渲染（Background / Border / TextContent）──

pub(super) struct TextParams<'a> {
    pub(super) content: &'a str,
    pub(super) font_size: f32,
    pub(super) color: &'a crate::graphics::Color,
    pub(super) font_weight: crate::text::FontWeight,
    pub(super) font_style: crate::text::FontSlant,
    pub(super) max_lines: usize,
    pub(super) align: crate::text::TextAlign,
    pub(super) overflow: crate::text::TextOverflow,
    pub(super) soft_wrap: bool,
    pub(super) letter_spacing: f32,
    pub(super) line_height: Option<f32>,
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
pub(super) fn apply_gl_transform(
    canvas: &Canvas,
    gl: &crate::graphics::GraphicsLayerParams,
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
pub(super) fn build_gl_3d_matrix(
    gl: &crate::graphics::GraphicsLayerParams,
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

/// Whether the layer needs an offscreen `save_layer`: a colour filter, alpha, a render effect, or
/// `CompositingStrategy::Offscreen` (blend modes and group opacity need a real offscreen).
/// `ModulateAlpha` has no per-draw equivalent in winia, so it takes the `Auto` path.
pub(super) fn graphics_layer_needs_layer(gl: &crate::graphics::GraphicsLayerParams) -> bool {
    gl.color_filter.is_some()
        || gl.alpha < 1.0
        || gl.render_effect.is_some()
        || gl.compositing == crate::graphics::CompositingStrategy::Offscreen
}

/// The knee of the elevation compression curve: the asymptote is ELEVATION_KNEE + 1.
pub(super) const ELEVATION_KNEE: f32 = 16.0;

/// The compressed Z handed to Skia: (K+1)·e/(K+e) with K = ELEVATION_KNEE.
///
/// f(0) = 0, f(1) = 1 (the smallest Material step keeps its physical size), monotonic, and bounded
/// by K+1, so a huge or animated value can never make the shadow run away. Not applied to the
/// colour alpha: Skia's ambient/spot strength is the configured colour's alpha, and the curve only
/// changes the geometry (offset, blur, scale).
pub(super) fn compressed_elevation(elevation: f32) -> f32 {
    if !elevation.is_finite() || elevation <= 0.0 {
        return 0.0;
    }
    (ELEVATION_KNEE + 1.0) * elevation / (ELEVATION_KNEE + elevation)
}

/// GraphicsLayer shadow：使用 Skia `SkShadowUtils::draw_shadow` 绘制真正的
/// ambient + spot 阴影。elevation 是 Compose 语义的 Z 高度，不再手工换算
/// blur radius 或 spot offset；光源参数集中在这里，便于后续校准平台差异。
///
/// The elevation is COMPRESSED before it becomes Skia's Z (see compressed_elevation): Compose's
/// physical Z makes every extra dp grow the shadow about linearly (Skia's spot blur and offset
/// scale with z/(600-z)), while Material's token ladder 1/3/6/8/12 already expects diminishing
/// visual steps. The curve keeps 0 at 0, leaves 1 dp at 1.0, and maps 8/16/24 dp to 5.67/8.5/10.2,
/// so the top of the ladder no longer dwarfs the bottom. This is a deliberate deviation from
/// Compose; it lives here so every consumer (Modifier::shadow, Surface, Card, Button, FAB) gets the
/// same curve, and no component compresses twice.
pub(super) fn draw_elevation_shadow(
    canvas: &Canvas,
    rect: Rect,
    shape: &crate::graphics::Shape,
    elevation: f32,
    ambient_color: crate::graphics::Color,
    spot_color: crate::graphics::Color,
) {
    let elevation = compressed_elevation(elevation);
    if elevation <= 0.0 {
        return;
    }

    let path = shadow_path(rect, shape);
    let z_plane = (0.0, 0.0, elevation);
    // The light sits directly above the component's centre, so the shadow falls straight down with no
    // horizontal bias — Compose's own appearance (Android's spot shadow is a vertical skirt). It used to be
    // `(270, 0, 600)`: off to one side AND level with the object, which threw the shadow up and sideways.
    // Measured on a 60x40 box with an 8 dp shadow (`tests/shadow_direction.rs`): the sample 12 px ABOVE the box
    // was 216 grey while the one 12 px below stayed pure white. Skia's spot shadow moves AWAY from the light, so
    // in canvas coordinates (y down) a light above the object is a NEGATIVE y: `(0, -600, 600)` puts the skirt
    // below, and `+600` moves the shadow further up instead (measured: 206 above, white below).
    let light_pos = (0.0, -600.0, 600.0);
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

/// `RRect` rounded on the two RIGHT corners only (upper-right + lower-right).
/// Skia's `new_rect_radii` corner order is UL, UR, LR, LL.
pub(super) fn rrect_right_rounded(rect: Rect, r: f32) -> RRect {
    RRect::new_rect_radii(rect, &[
        skia_safe::Vector::new(0.0, 0.0),
        skia_safe::Vector::new(r, r),
        skia_safe::Vector::new(r, r),
        skia_safe::Vector::new(0.0, 0.0),
    ])
}

/// `RRect` rounded on the two LEFT corners only (upper-left + lower-left).
pub(super) fn rrect_left_rounded(rect: Rect, r: f32) -> RRect {
    RRect::new_rect_radii(rect, &[
        skia_safe::Vector::new(r, r),
        skia_safe::Vector::new(0.0, 0.0),
        skia_safe::Vector::new(0.0, 0.0),
        skia_safe::Vector::new(r, r),
    ])
}

/// `RRect` with an independent radius per corner, in Skia's order (upper-left, upper-right,
/// lower-right, lower-left) — the render half of [`crate::graphics::Shape::Corners`].
pub(super) fn rrect_corners(rect: Rect, top_left: f32, top_right: f32, bottom_right: f32, bottom_left: f32) -> RRect {
    RRect::new_rect_radii(rect, &[
        skia_safe::Vector::new(top_left, top_left),
        skia_safe::Vector::new(top_right, top_right),
        skia_safe::Vector::new(bottom_right, bottom_right),
        skia_safe::Vector::new(bottom_left, bottom_left),
    ])
}

pub(super) fn shadow_path(rect: Rect, shape: &crate::graphics::Shape) -> skia_safe::Path {
    match shape {
        crate::graphics::Shape::Rectangle => skia_safe::Path::rect(rect, None),
        crate::graphics::Shape::RoundedRect { corner_radius } => {
            skia_safe::Path::rrect(RRect::new_rect_xy(rect, *corner_radius, *corner_radius), None)
        }
        crate::graphics::Shape::PillToRounded { corner_radius, t } => {
            let percent = rect.width().min(rect.height()) / 2.0;
            let r = percent + (*corner_radius - percent) * *t;
            skia_safe::Path::rrect(RRect::new_rect_xy(rect, r, r), None)
        }
        crate::graphics::Shape::TopRoundedRect { radius } => {
            let rr = RRect::new_rect_radii(rect, &[
                skia_safe::Vector::new(*radius, *radius),
                skia_safe::Vector::new(*radius, *radius),
                skia_safe::Vector::new(0.0, 0.0),
                skia_safe::Vector::new(0.0, 0.0),
            ]);
            skia_safe::Path::rrect(rr, None)
        }
        crate::graphics::Shape::RightRoundedRect { radius } => {
            skia_safe::Path::rrect(rrect_right_rounded(rect, *radius), None)
        }
        crate::graphics::Shape::LeftRoundedRect { radius } => {
            skia_safe::Path::rrect(rrect_left_rounded(rect, *radius), None)
        }
        crate::graphics::Shape::Corners { top_left, top_right, bottom_right, bottom_left } => {
            skia_safe::Path::rrect(
                rrect_corners(rect, *top_left, *top_right, *bottom_right, *bottom_left),
                None,
            )
        }
        crate::graphics::Shape::Pill => {
            let radius = rect.width().min(rect.height()) / 2.0;
            skia_safe::Path::rrect(RRect::new_rect_xy(rect, radius, radius), None)
        }
        crate::graphics::Shape::Circle => {
            // Compose's `CircleShape` IS `RoundedCornerShape(50)`: a percent corner
            // evaluated against the box, so a non-square box gives a stadium — not
            // a true circle (which leaves the box mostly unpainted) and not an
            // ellipse. Identical to `Pill`, as in Compose.
            let radius = rect.width().min(rect.height()) / 2.0;
            skia_safe::Path::rrect(RRect::new_rect_xy(rect, radius, radius), None)
        }
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
pub(super) fn draw_shadow_layer(
    canvas: &Canvas,
    rect: Rect,
    shape: &crate::graphics::Shape,
    params: &crate::graphics::ShadowParams,
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
        crate::graphics::Shape::Rectangle => { sc.draw_rect(local, &mask); }
        crate::graphics::Shape::RoundedRect { corner_radius } => {
            sc.draw_rrect(RRect::new_rect_xy(local, *corner_radius, *corner_radius), &mask);
        }
        crate::graphics::Shape::PillToRounded { corner_radius, t } => {
            let percent = local.width().min(local.height()) / 2.0;
            let r = percent + (*corner_radius - percent) * *t;
            sc.draw_rrect(RRect::new_rect_xy(local, r, r), &mask);
        }
        crate::graphics::Shape::TopRoundedRect { radius } => {
            let rr = RRect::new_rect_radii(local, &[
                skia_safe::Vector::new(*radius, *radius),
                skia_safe::Vector::new(*radius, *radius),
                skia_safe::Vector::new(0.0, 0.0),
                skia_safe::Vector::new(0.0, 0.0),
            ]);
            sc.draw_rrect(rr, &mask);
        }
        crate::graphics::Shape::RightRoundedRect { radius } => {
            sc.draw_rrect(rrect_right_rounded(local, *radius), &mask);
        }
        crate::graphics::Shape::LeftRoundedRect { radius } => {
            sc.draw_rrect(rrect_left_rounded(local, *radius), &mask);
        }
        crate::graphics::Shape::Corners { top_left, top_right, bottom_right, bottom_left } => {
            sc.draw_rrect(
                rrect_corners(local, *top_left, *top_right, *bottom_right, *bottom_left),
                &mask,
            );
        }
        crate::graphics::Shape::Pill => {
            let r = local.width().min(local.height()) / 2.0;
            sc.draw_rrect(RRect::new_rect_xy(local, r, r), &mask);
        }
        crate::graphics::Shape::Circle => {
            let r = local.width().min(local.height()) / 2.0;
            sc.draw_rrect(RRect::new_rect_xy(local, r, r), &mask);
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
            crate::graphics::Shape::Rectangle => { sc.draw_rect(local, &stroke); }
            crate::graphics::Shape::RoundedRect { corner_radius } => {
                sc.draw_rrect(RRect::new_rect_xy(local, *corner_radius, *corner_radius), &stroke);
            }
            crate::graphics::Shape::PillToRounded { corner_radius, t } => {
                let percent = local.width().min(local.height()) / 2.0;
                let r = percent + (*corner_radius - percent) * *t;
                sc.draw_rrect(RRect::new_rect_xy(local, r, r), &stroke);
            }
            crate::graphics::Shape::TopRoundedRect { radius } => {
                let rr = RRect::new_rect_radii(local, &[
                    skia_safe::Vector::new(*radius, *radius),
                    skia_safe::Vector::new(*radius, *radius),
                    skia_safe::Vector::new(0.0, 0.0),
                    skia_safe::Vector::new(0.0, 0.0),
                ]);
                sc.draw_rrect(rr, &stroke);
            }
            crate::graphics::Shape::RightRoundedRect { radius } => {
                sc.draw_rrect(rrect_right_rounded(local, *radius), &stroke);
            }
            crate::graphics::Shape::LeftRoundedRect { radius } => {
                sc.draw_rrect(rrect_left_rounded(local, *radius), &stroke);
            }
            crate::graphics::Shape::Corners { top_left, top_right, bottom_right, bottom_left } => {
                sc.draw_rrect(
                    rrect_corners(local, *top_left, *top_right, *bottom_right, *bottom_left),
                    &stroke,
                );
            }
            crate::graphics::Shape::Pill => {
                let r = local.width().min(local.height()) / 2.0;
                sc.draw_rrect(RRect::new_rect_xy(local, r, r), &stroke);
            }
            crate::graphics::Shape::Circle => {
                let r = local.width().min(local.height()) / 2.0;
                sc.draw_rrect(RRect::new_rect_xy(local, r, r), &stroke);
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
pub(super) fn render_modifier_element<'a>(
    canvas: &Canvas,
    el: &'a ModifierElement,
    rect: Rect,
    last_background: &mut Option<(crate::graphics::Color, crate::graphics::Shape)>,
    morph_radii: Option<[(f32, f32); 4]>,
) -> Option<TextParams<'a>> {
    match el {
        ModifierElement::Background { color: bg, shape } => {
            // Evaluated ONCE per element per paint: the morph path, the fill and the border-merge
            // bookkeeping all see the same shape.
            let shape = shape.eval();
            let color = bg.eval();
            *last_background = Some((color, shape));
            match morph_radii {
                Some(r) => {
                    let mut paint = skia_safe::Paint::default();
                    paint.set_anti_alias(true);
                    paint.set_color4f(Color4f::from(&color), None);
                    canvas.draw_rrect(
                        skia_safe::RRect::new_rect_radii(
                            rect,
                            &crate::transition::rrect_vectors(r),
                        ),
                        &paint,
                    );
                }
                None => draw_background(canvas, rect, &color, &shape),
            }
            None
        }
        ModifierElement::BackgroundBrush { brush: brush_spec, shape } => {
            let shape = shape.eval();
            // The brush is resolved here, at paint time, which is what lets a dynamic value read
            // animated state per frame. `last_background` is deliberately NOT set: the border-merge
            // optimization below compares colors, and a gradient is not one.
            let brush = brush_spec.eval();
            match morph_radii {
                Some(r) => {
                    let Some(paint) = brush_paint(&brush, rect) else { return None };
                    canvas.draw_rrect(
                        skia_safe::RRect::new_rect_radii(
                            rect,
                            &crate::transition::rrect_vectors(r),
                        ),
                        &paint,
                    );
                }
                None => draw_brush(canvas, rect, &brush, &shape),
            }
            None
        }
        ModifierElement::Border { width, color, shape } => {
            let shape = shape.eval();
            let color = color.eval();
            // When the border's colour and shape both equal the container's, collapse it into the fill
            // (M3 `drawBox` semantics): stroking a translucent colour over that same fill blends it
            // twice, and the border reads visibly darker than the interior.
            if *last_background != Some((color, shape)) {
                // `rect`, not the node: the outline belongs to the container, which is smaller than the
                // node once `minimum_interactive_size` grew the node to the touch target.
                let (bx, by, bw, bh) = (rect.x(), rect.y(), rect.width(), rect.height());
                let w = width.to_logical_px();
                match morph_radii {
                    Some(r) => draw_border_morphed(canvas, bx, by, bw, bh, w, &color, r),
                    None => draw_border(canvas, bx, by, bw, bh, w, &color, &shape),
                }
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
pub(super) fn draw_icon(canvas: &Canvas, rect: Rect, direction: LayoutDirection, spec: &IconSpec) {
    let mirror = spec.auto_mirror && direction == LayoutDirection::Rtl;
    if mirror {
        canvas.save();
        let c = rect.center();
        canvas.translate((c.x, c.y));
        canvas.scale((-1.0, 1.0));
        canvas.translate((-c.x, -c.y));
    }
    // Evaluated once per paint: the bitmap filter, the SVG tint and the symbol colour all read the
    // same value, and a dynamic tint costs no recomposition.
    let tint = spec.tint.as_ref().map(|t| t.eval());
    match &spec.source {
        IconSource::SvgPath { .. } | IconSource::Svg(_) | IconSource::File(_) => {
            if let Some(decoded) = decoded_icon(&spec.source) {
                match decoded.as_ref() {
                    DecodedIcon::Bitmap { image, width, height } => {
                        let dst = fit_rect(rect, *width, *height);
                        let src = Rect::new(0.0, 0.0, *width, *height);
                        let mut paint = Paint::default();
                        paint.set_anti_alias(true);
                        if let Some(tint) = tint {
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
                        let cf = tint.map(|c| crate::graphics::ColorFilter::Tint {
                            color: c,
                            blend_mode: crate::graphics::BlendMode::SrcIn,
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
                if let Some(blob) = crate::graphics::symbol_blob(*symbol, size, &spec.axes) {
                    let x = rect.left + (rect.width() - size) / 2.0;
                    let y = rect.top + (rect.height() - size) / 2.0 + size;
                    let mut paint = Paint::default();
                    paint.set_anti_alias(true);
                    paint.set_color(skia_color(tint.unwrap_or(crate::graphics::Color::BLACK)));
                    canvas.draw_text_blob(&blob, (x, y), &paint);
                }
            }
        }
    }
    if mirror {
        canvas.restore();
    }
}

/// winia `Color` → Skia color. Public because the `Canvas`/`DrawScope` API
/// (`crate::components::draw_scope`) draws through it too — one conversion for the whole crate rather than a
/// second copy inside the public drawing surface.
/// 映射 winia BlendMode → skia BlendMode（同源 29 值）
pub(super) fn to_skia_blend_mode(bm: crate::graphics::BlendMode) -> BlendMode {
    use crate::graphics::BlendMode as BM;
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
pub(super) fn to_skia_color_filter(cf: &crate::graphics::ColorFilter) -> Option<skia_safe::ColorFilter> {
    use crate::graphics::ColorFilter as CF;
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
pub(super) fn sampling_options_for(q: crate::graphics::FilterQuality) -> SamplingOptions {
    use crate::graphics::FilterQuality as FQ;
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
pub(super) fn draw_image_content(
    canvas: &Canvas,
    rect: Rect,
    source: &crate::graphics::IconSource,
    content_scale: crate::graphics::ContentScale,
    alignment: crate::graphics::ImageAlignment,
    alpha: f32,
    color_filter: Option<&crate::graphics::ColorFilter>,
    filter_quality: crate::graphics::FilterQuality,
    direction: crate::layout::LayoutDirection,
) {
    let Some(decoded) = crate::graphics::decoded_icon(source) else { return };
    let rtl = direction == crate::layout::LayoutDirection::Rtl;
    match decoded.as_ref() {
        crate::graphics::DecodedIcon::Bitmap { image, width, height } => {
            let dst = crate::graphics::content_scale_rect(
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
        crate::graphics::DecodedIcon::Svg { dom, width, height } => {
            // 与位图同一事实来源：content_scale_rect 完整缩放/对齐 + clipToBounds
            let dst = crate::graphics::content_scale_rect(
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
pub(super) fn fit_rect(rect: Rect, iw: f32, ih: f32) -> Rect {
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

pub(super) fn draw_svg_dom(
    canvas: &Canvas,
    dom: &RefCell<svg::Dom>,
    dst: Rect,
    color_filter: Option<&crate::graphics::ColorFilter>,
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

