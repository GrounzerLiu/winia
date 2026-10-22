use super::*;
use super::paint::*;

// ── TextField 容器视觉（M3 Filled/Outlined）──

/// M3 文本输入框容器：Filled = 容器色背景 + 底部指示线
/// （focused 2px / unfocused 1px，色 indicator_color）；
/// Outlined = 边框（focused 2px / unfocused 1px）。
/// 状态优先级 disabled > error > focused > unfocused（组合期已解析）。
/// M3 文本输入框容器：Filled = 容器色背景 + 底部指示线
/// （focused 2px / unfocused 1px）；Outlined = 边框（圆角 4dp，
/// `cutout` 为悬浮 label 缺口区域）。颜色来自动画 State
/// （`animate_color_as_state`——状态切换过渡）；`focus_p` 0..1 插值宽度。
pub(super) fn draw_text_field_container(
    canvas: &Canvas,
    rect: Rect,
    variant: &crate::text::field::TextFieldVariant,
    _shape: &crate::graphics::Shape,
    colors: &crate::text::field::TextFieldColors,
    indicator: &crate::graphics::Color,
    focus_p: f32,
    cutout: Option<Rect>,
) {
    let stroke_w = 1.0 + focus_p.clamp(0.0, 1.0);
    match variant {
        crate::text::field::TextFieldVariant::Filled => {
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
        crate::text::field::TextFieldVariant::Outlined => {
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


/// TextField label/支持文本绘制（无布局缓存的独立小段文本——
/// build_plain_paragraph 每次构建；辅助文本量小，开销可接受）
pub(super) fn draw_text_field_aux_text(
    canvas: &Canvas,
    content: &str,
    font_size: f32,
    font_weight: crate::text::FontWeight,
    font_style: crate::text::FontSlant,
    letter_spacing: f32,
    line_height: Option<f32>,
    color: &crate::graphics::Color,
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
        crate::text::TextAlign::Left,
        crate::text::TextOverflow::Clip,
        true,
        letter_spacing,
        line_height,
        max_width,
    );
    // ⚠ 必须 layout 后才能 paint（skia Paragraph 未布局时绘制为空）
    para.layout(max_width);
    para.paint(canvas, pos.0, pos.1);
}

/// Border with flight-morphed radii (layout-space pairs — drawn under the
/// flight transform; inset logic mirrors `draw_border`).
pub(super) fn draw_border_morphed(
    canvas: &Canvas,
    x: f32, y: f32, w: f32, h: f32,
    width: f32,
    color: &crate::graphics::Color,
    r: [(f32, f32); 4],
) {
    let mut paint = Paint::default();
    paint.set_color4f(Color4f::from(color), None);
    paint.set_style(skia_safe::paint::Style::Stroke);
    paint.set_stroke_width(width);
    paint.set_anti_alias(true);
    let inset = width / 2.0;
    let sr = Rect::new(x + inset, y + inset, x + w - inset, y + h - inset);
    let v = |i: usize| {
        skia_safe::Vector::new((r[i].0 - inset).max(0.0), (r[i].1 - inset).max(0.0))
    };
    canvas.draw_rrect(
        skia_safe::RRect::new_rect_radii(sr, &[v(0), v(1), v(2), v(3)]),
        &paint,
    );
}

pub(super) fn draw_border(canvas: &Canvas, x: f32, y: f32, w: f32, h: f32, width: f32, color: &crate::graphics::Color, shape: &crate::graphics::Shape) {
    let mut paint = Paint::default();
    paint.set_color4f(Color4f::from(color), None);
    paint.set_style(skia_safe::paint::Style::Stroke);
    paint.set_stroke_width(width);
    paint.set_anti_alias(true);
    let inset = width / 2.0;
    let sr = Rect::new(x + inset, y + inset, x + w - inset, y + h - inset);
    match shape {
        crate::graphics::Shape::Rectangle => { canvas.draw_rect(sr, &paint); }
        crate::graphics::Shape::RoundedRect { corner_radius } => {
            canvas.draw_rrect(RRect::new_rect_xy(sr, (*corner_radius - inset).max(0.0), (*corner_radius - inset).max(0.0)), &paint);
        }
        crate::graphics::Shape::PillToRounded { corner_radius, t } => {
            let percent = sr.width().min(sr.height()) / 2.0;
            let r = (percent + (*corner_radius - percent) * *t - inset).max(0.0);
            canvas.draw_rrect(RRect::new_rect_xy(sr, r, r), &paint);
        }
        crate::graphics::Shape::TopRoundedRect { radius } => {
            let r = (*radius - inset).max(0.0);
            let rr = RRect::new_rect_radii(sr, &[
                skia_safe::Vector::new(r, r),
                skia_safe::Vector::new(r, r),
                skia_safe::Vector::new(0.0, 0.0),
                skia_safe::Vector::new(0.0, 0.0),
            ]);
            canvas.draw_rrect(rr, &paint);
        }
        crate::graphics::Shape::RightRoundedRect { radius } => {
            canvas.draw_rrect(rrect_right_rounded(sr, (*radius - inset).max(0.0)), &paint);
        }
        crate::graphics::Shape::LeftRoundedRect { radius } => {
            canvas.draw_rrect(rrect_left_rounded(sr, (*radius - inset).max(0.0)), &paint);
        }
        crate::graphics::Shape::Corners { top_left, top_right, bottom_right, bottom_left } => {
            // Same inset rule as the other radii: the border is drawn on the inset rect, so its
            // corners shrink by the stroke's half width (a 1 dp border on a 4 dp inner corner must
            // not paint outside the fill it wraps).
            canvas.draw_rrect(
                rrect_corners(
                    sr,
                    (*top_left - inset).max(0.0),
                    (*top_right - inset).max(0.0),
                    (*bottom_right - inset).max(0.0),
                    (*bottom_left - inset).max(0.0),
                ),
                &paint,
            );
        }
        crate::graphics::Shape::Pill => {
            // 路径圆角 = 节点 pill 半径 - inset（不要对 sr 再减一次——
            // 否则外缘在圆角处比背景内缩 1px，边框不像内边框）
            let r = (w.min(h) / 2.0 - inset).max(0.0);
            canvas.draw_rrect(RRect::new_rect_xy(sr, r, r), &paint);
        }
        crate::graphics::Shape::Circle => {
            let r = sr.width().min(sr.height()) / 2.0;
            canvas.draw_rrect(RRect::new_rect_xy(sr, r, r), &paint);
        }
    }
}

/// 焦点环（M3 focus indicator）——宽 3、完全位于组件**外部**（环内侧距
/// 组件边缘 2）、形状跟随组件、颜色来自组件组合期捕获的主题色。
pub(crate) fn draw_focus(
    canvas: &Canvas,
    rect: Rect,
    shape: &crate::graphics::Shape,
    // Device-space corner radii of the FLIGHT morph, when this node is flying: the ring
    // must use the same corners as the background it surrounds. Resolving the node's own
    // shape instead measures a percent corner (Circle/Pill) on the un-transformed content
    // box, and the flight's `canvas.scale` then stretches it into an ellipse while the
    // background paints `min(lerped)/2`.
    flight_radii: Option<[(f32, f32); 4]>,
    color: crate::graphics::Color,
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
    if let Some(r) = flight_radii {
        // Same corners as the morph. The ring sits `inset` outside the box (and the
        // fade-in `scale` grows it), so the radii grow with it. These pairs come from
        // `radii_pairs`, which pre-divides by `paint_scale` — the same factors this
        // canvas applies — so the transform lands the ring on the lerped rect exactly
        // as it lands the background. (A second review round measured this being false
        // before the radii divided by the paint scale: the ring was elliptical by the
        // same factor as the background, up to 8.7% on a width-preserving flight.)
        let grow = if rect.width() > 0.0 { sr.width() / rect.width() } else { 1.0 };
        let rr = RRect::new_rect_radii(
            sr,
            &crate::transition::rrect_vectors(
                r.map(|(x, y)| ((x + inset).max(0.0) * grow, (y + inset).max(0.0) * grow)),
            ),
        );
        canvas.draw_rrect(rr, &paint);
        return;
    }
    match shape {
        crate::graphics::Shape::Rectangle => { canvas.draw_rect(sr, &paint); }
        crate::graphics::Shape::RoundedRect { corner_radius } => {
            // 外扩后圆角同步放大（保持与组件同心）
            let r = (*corner_radius + inset).max(0.0) * scale;
            canvas.draw_rrect(RRect::new_rect_xy(sr, r, r), &paint);
        }
        crate::graphics::Shape::PillToRounded { corner_radius, t } => {
            let percent = sr.width().min(sr.height()) / 2.0;
            let r = (percent + (*corner_radius - percent) * *t + inset).max(0.0) * scale;
            canvas.draw_rrect(RRect::new_rect_xy(sr, r, r), &paint);
        }
        crate::graphics::Shape::TopRoundedRect { radius } => {
            let r = (*radius + inset).max(0.0) * scale;
            let rr = RRect::new_rect_radii(sr, &[
                skia_safe::Vector::new(r, r),
                skia_safe::Vector::new(r, r),
                skia_safe::Vector::new(0.0, 0.0),
                skia_safe::Vector::new(0.0, 0.0),
            ]);
            canvas.draw_rrect(rr, &paint);
        }
        crate::graphics::Shape::RightRoundedRect { radius } => {
            let r = (*radius + inset).max(0.0) * scale;
            canvas.draw_rrect(rrect_right_rounded(sr, r), &paint);
        }
        crate::graphics::Shape::LeftRoundedRect { radius } => {
            let r = (*radius + inset).max(0.0) * scale;
            canvas.draw_rrect(rrect_left_rounded(sr, r), &paint);
        }
        crate::graphics::Shape::Corners { top_left, top_right, bottom_right, bottom_left } => {
            let grow = |value: f32| (value + inset).max(0.0) * scale;
            canvas.draw_rrect(
                rrect_corners(
                    sr,
                    grow(*top_left),
                    grow(*top_right),
                    grow(*bottom_right),
                    grow(*bottom_left),
                ),
                &paint,
            );
        }
        crate::graphics::Shape::Pill => {
            let r = sr.width().min(sr.height()) / 2.0;
            canvas.draw_rrect(RRect::new_rect_xy(sr, r, r), &paint);
        }
        crate::graphics::Shape::Circle => {
            let r = sr.width().min(sr.height()) / 2.0;
            canvas.draw_rrect(RRect::new_rect_xy(sr, r, r), &paint);
        }
    }
}

/// draw_text + 选中高亮
pub(super) fn draw_text_with_selection(
    canvas: &Canvas,
    content: &str,
    font_size: f32,
    color: &crate::graphics::Color,
    font_weight: crate::text::FontWeight,
    font_style: crate::text::FontSlant,
    x: f32, y: f32, w: f32,
    max_lines: usize, align: crate::text::TextAlign, overflow: crate::text::TextOverflow, soft_wrap: bool,
    letter_spacing: f32, line_height: Option<f32>,
    registrar: Option<crate::text::selection::SelectionRegistrar>,
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
    draw_text(canvas, &para, x, y, w, align, soft_wrap);
}

pub(super) fn draw_text(
    canvas: &Canvas,
    para: &crate::text::Paragraph,
    x: f32,
    y: f32,
    max_width: f32,
    align: crate::text::TextAlign,
    soft_wrap: bool,
) {
    // 计算 x 偏移以支持 Center/Right/Justify 对齐——`soft_wrap(false)` 的段落按固有宽度排版，
    // 可能比盒子宽，此时居中/右对齐要往反方向位移（与上面缓存路径同一规则）
    let slack = if soft_wrap {
        (max_width - para.max_intrinsic_width()).max(0.0)
    } else {
        max_width - para.max_intrinsic_width()
    };
    let x_offset = match align {
        crate::text::TextAlign::Left | crate::text::TextAlign::Justify => x,
        crate::text::TextAlign::Center => x + slack / 2.0,
        crate::text::TextAlign::Right => x + slack,
    };
    para.paint(canvas, x_offset, y);
}

