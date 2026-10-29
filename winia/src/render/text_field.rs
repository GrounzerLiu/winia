use super::*;
use super::paint::*;

// ── The text-field container's visuals (M3 Filled/Outlined) ──

/// The M3 text-field container. `Filled` is a container-coloured background plus a bottom
/// indicator (2 px focused, 1 px not, in `indicator_color`); `Outlined` is an outline instead
/// (4 dp corners, with `cutout` the gap the floating label sits in).
///
/// State precedence - disabled > error > focused > unfocused - was already resolved at compose
/// time. The colours come from an animated `State` (`animate_color_as_state`, which is what makes
/// a state change a transition), and `focus_p` interpolates the 0..1 indicator width.
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
            // The container fill (surfaceContainerHighest, with M3's 4 dp top corners).
            if colors.container.a > 0 {
                let mut bg = Paint::default();
                bg.set_color4f(Color4f::from(&colors.container), None);
                bg.set_anti_alias(true);
                canvas.draw_rrect(RRect::new_rect_xy(rect, 4.0, 4.0), &bg);
            }
            // The bottom indicator: 1 or 2 px against the bottom edge, thicker when focused.
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
            // The outline: a centred 1 or 2 px stroke with 4 dp corners, broken at the label gap.
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
                // The gap: clip out the label's area (Difference) before stroking the outline.
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


/// Draw a label or supporting text: a small run with no layout cache, built through
/// `build_plain_paragraph` every time. The volume is tiny, so the cost is acceptable.
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
        false,
        false,
        None,
        None,
        max_width,
    );
    // The paragraph has to be laid out before it paints: an un-laid-out Skia paragraph draws
    // nothing.
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
            // The path's corner radius is the node's pill radius minus the inset. Do not subtract
            // the inset from the rounded rect again: that pulls the outer edge 1 px inside the
            // background at the corners, and the outline stops looking like an inner border.
            let r = (w.min(h) / 2.0 - inset).max(0.0);
            canvas.draw_rrect(RRect::new_rect_xy(sr, r, r), &paint);
        }
        crate::graphics::Shape::Circle => {
            let r = sr.width().min(sr.height()) / 2.0;
            canvas.draw_rrect(RRect::new_rect_xy(sr, r, r), &paint);
        }
    }
}

/// The focus ring (M3's focus indicator): 3 wide, entirely OUTSIDE the component (its inner edge
/// 2 from the component's edge), following the component's shape, in the theme colour the component
/// captured at compose time.
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
    // The ring's centre line sits a half-width outside the gap, so a centred stroke lands entirely
    // outside the component.
    let inset = gap + FOCUS_WIDTH / 2.0;
    let mut sr = Rect::new(
        rect.left - inset,
        rect.top - inset,
        rect.right + inset,
        rect.bottom + inset,
    );
    // The fade-in: as alpha goes 0 to 1 the ring shrinks from 1.15x into place, anchored on the
    // component's centre.
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
    // Alpha follows `focus_indicator_alpha`: in on focus, out on blur.
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
            // The radius grows with the expansion, so the ring stays concentric with the component.
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

/// `draw_text` plus the selection highlight.
pub(super) fn draw_text_with_selection(
    canvas: &Canvas,
    content: &str,
    font_size: f32,
    color: &crate::graphics::Color,
    font_weight: crate::text::FontWeight,
    font_style: crate::text::FontSlant,
    x: f32, y: f32, w: f32, h: f32,
    max_lines: usize, align: crate::text::TextAlign, overflow: crate::text::TextOverflow, soft_wrap: bool,
    letter_spacing: f32, line_height: Option<f32>,
    registrar: Option<crate::text::selection::SelectionRegistrar>,
    slot_key: u64,
    transform: Option<crate::text::TextGeometricTransform>,
    line_style: Option<crate::text::LineHeightStyle>,
) {
    // The fallback must agree with the cached path about a geometric transform: the paragraph is
    // built in its own space and painted through the same matrix (see `render::pass`).
    let scale_x = transform.map(|t| t.scale_x).unwrap_or(1.0).max(0.0001);
    if registrar.and_then(|reg| reg.selected_range(slot_key)).is_some() {
        let mut paint = skia_safe::Paint::default();
        paint.set_color(skia_safe::Color::from_argb(80, 100, 150, 255));
        canvas.draw_rect(skia_safe::Rect::new(x, y, x + w, y + font_size * 1.2), &paint);
    }
    // The fallback path rebuilds through the shared constructor when the cache is missing: the same
    // logic the measure pass runs, from one source of truth.
    let para = crate::layout::node::build_plain_paragraph(
        content, font_size, color, font_weight, font_style,
        max_lines, align, overflow, soft_wrap, letter_spacing, line_height,
        false, false, None, None, w / scale_x,
    );
    // The line-height offset has to be applied here too, or an uncached text would paint at a
    // different height than the same text once it has been measured. Shared arithmetic, so the two
    // paths cannot drift.
    let paint_dy = crate::layout::node::line_geometry_from(
        line_height,
        line_style,
        para.natural_line_height(),
    )
    .paint_dy;
    // Same rule as the cached path (see the text branch in `render::pass`): an overflowing layout
    // is clipped to the text node's own box unless the caller asked for `Visible`. Without this the
    // fallback — used when no measured paragraph is cached — would still spill and the two paths
    // would disagree.
    let clipped = overflow != crate::text::TextOverflow::Visible
        && (para.did_exceed_max_lines()
            || para.longest_line() * scale_x > w + 0.5
            || para.height() > h + 0.5);
    if clipped {
        canvas.save();
        canvas.clip_rect(skia_safe::Rect::new(x, y, x + w, y + para.height()), None, Some(false));
    }
    match transform {
        Some(t) => {
            canvas.save();
            canvas.translate((x, y + paint_dy));
            canvas.scale((t.scale_x, 1.0));
            if t.skew_x.abs() > f32::EPSILON {
                canvas.skew((t.skew_x, 0.0));
            }
            draw_text(canvas, &para, 0.0, 0.0, w / scale_x, align, soft_wrap);
            canvas.restore();
        }
        None => draw_text(canvas, &para, x, y + paint_dy, w, align, soft_wrap),
    }
    if clipped {
        canvas.restore();
    }
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
    // The x offset that makes Center/Right/Justify work: a `soft_wrap(false)` paragraph is laid
    // out at its intrinsic width, which can exceed the box, and centring or right-aligning it then
    // shifts it the other way. Same rule as the cached path above.
    let slack = if soft_wrap {
        (max_width - para.max_intrinsic_width()).max(0.0)
    } else {
        max_width - para.max_intrinsic_width()
    };
    let x_offset = match align {
        crate::text::TextAlign::Left
        | crate::text::TextAlign::Justify
        | crate::text::TextAlign::Start
        | crate::text::TextAlign::End => x,
        crate::text::TextAlign::Center => x + slack / 2.0,
        crate::text::TextAlign::Right => x + slack,
    };
    para.paint(canvas, x_offset, y);
}

