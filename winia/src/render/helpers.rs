use super::*;
use super::paint::*;

// ── 辅助函数 ──

impl From<&crate::graphics::Color> for Color4f {
    fn from(c: &crate::graphics::Color) -> Self {
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
pub fn draw_background_for_node(canvas: &Canvas, rect: Rect, color: &crate::graphics::Color, shape: &crate::graphics::Shape) {
    draw_background(canvas, rect, color, shape);
}

pub(super) fn draw_background(canvas: &Canvas, rect: Rect, color: &crate::graphics::Color, shape: &crate::graphics::Shape) {
    let mut paint = Paint::default();
    paint.set_color4f(Color4f::from(color), None);
    paint.set_anti_alias(true);
    paint_shape(canvas, rect, &paint, shape);
}

/// Fill `shape` inside `rect` with an already prepared paint — the one place the shape set is turned
/// into a draw call, shared by solid backgrounds and brushes.
/// The rounded rect a shape normalises to inside `rect` — the path form of [`paint_shape`].
pub(super) fn shape_rrect(rect: Rect, shape: &crate::graphics::Shape) -> RRect {
    match shape {
        crate::graphics::Shape::Rectangle => RRect::new_rect_xy(rect, 0.0, 0.0),
        crate::graphics::Shape::RoundedRect { corner_radius } => {
            RRect::new_rect_xy(rect, *corner_radius, *corner_radius)
        }
        crate::graphics::Shape::PillToRounded { corner_radius, t } => {
            let percent = rect.width().min(rect.height()) / 2.0;
            let r = percent + (*corner_radius - percent) * *t;
            RRect::new_rect_xy(rect, r, r)
        }
        crate::graphics::Shape::TopRoundedRect { radius } => RRect::new_rect_radii(
            rect,
            &[
                skia_safe::Vector::new(*radius, *radius),
                skia_safe::Vector::new(*radius, *radius),
                skia_safe::Vector::new(0.0, 0.0),
                skia_safe::Vector::new(0.0, 0.0),
            ],
        ),
        crate::graphics::Shape::RightRoundedRect { radius } => rrect_right_rounded(rect, *radius),
        crate::graphics::Shape::LeftRoundedRect { radius } => rrect_left_rounded(rect, *radius),
        crate::graphics::Shape::Corners { top_left, top_right, bottom_right, bottom_left } => {
            rrect_corners(rect, *top_left, *top_right, *bottom_right, *bottom_left)
        }
        crate::graphics::Shape::Pill | crate::graphics::Shape::Circle => {
            let r = rect.width().min(rect.height()) / 2.0;
            RRect::new_rect_xy(rect, r, r)
        }
    }
}

pub(super) fn shape_path(rect: Rect, shape: &crate::graphics::Shape) -> skia_safe::Path {
    skia_safe::Path::rrect(&shape_rrect(rect, shape), skia_safe::PathDirection::CW)
}

/// Compose's inner shadow: a rim INSIDE the shape, made from the box minus the offset shape.
///
/// The box is filled into an offscreen layer, then the offset shape is erased with `DstOut` and a
/// blurred mask filter — what remains is soft on the inner edge and exact on the outer one, because
/// the whole layer is clipped to the shape. `blur` is the mask's sigma in logical pixels.
pub(super) fn draw_inner_shadow(
    canvas: &Canvas,
    rect: Rect,
    shape: &crate::graphics::Shape,
    color: &crate::graphics::Color,
    blur: f32,
    offset_x: f32,
    offset_y: f32,
) {
    let path = shape_path(rect, shape);
    let cut = shape_path(rect, shape).with_offset(skia_safe::Vector::new(offset_x, offset_y));
    canvas.save();
    canvas.clip_path(&path, None, Some(true));
    canvas.save_layer(&skia_safe::canvas::SaveLayerRec::default());
    let mut fill = Paint::default();
    fill.set_anti_alias(true);
    fill.set_color4f(Color4f::from(color), None);
    canvas.draw_rect(rect, &fill);
    let mut erase = Paint::default();
    erase.set_anti_alias(true);
    erase.set_blend_mode(BlendMode::DstOut);
    if blur > 0.0 {
        if let Some(filter) = skia_safe::MaskFilter::blur(skia_safe::BlurStyle::Normal, blur, None) {
            erase.set_mask_filter(filter);
        }
    }
    canvas.draw_path(&cut, &erase);
    canvas.restore();
    canvas.restore();
}

pub(super) fn paint_shape(canvas: &Canvas, rect: Rect, paint: &Paint, shape: &crate::graphics::Shape) {
    match shape {
        crate::graphics::Shape::Rectangle => { canvas.draw_rect(rect, paint); }
        crate::graphics::Shape::RoundedRect { corner_radius } => {
            canvas.draw_rrect(RRect::new_rect_xy(rect, *corner_radius, *corner_radius), paint);
        }
        crate::graphics::Shape::PillToRounded { corner_radius, t } => {
            let percent = rect.width().min(rect.height()) / 2.0;
            let r = percent + (*corner_radius - percent) * *t;
            canvas.draw_rrect(RRect::new_rect_xy(rect, r, r), paint);
        }
        crate::graphics::Shape::TopRoundedRect { radius } => {
            let rr = RRect::new_rect_radii(rect, &[
                skia_safe::Vector::new(*radius, *radius),
                skia_safe::Vector::new(*radius, *radius),
                skia_safe::Vector::new(0.0, 0.0),
                skia_safe::Vector::new(0.0, 0.0),
            ]);
            canvas.draw_rrect(rr, paint);
        }
        crate::graphics::Shape::RightRoundedRect { radius } => {
            canvas.draw_rrect(rrect_right_rounded(rect, *radius), paint);
        }
        crate::graphics::Shape::LeftRoundedRect { radius } => {
            canvas.draw_rrect(rrect_left_rounded(rect, *radius), paint);
        }
        crate::graphics::Shape::Corners { top_left, top_right, bottom_right, bottom_left } => {
            canvas.draw_rrect(
                rrect_corners(rect, *top_left, *top_right, *bottom_right, *bottom_left),
                paint,
            );
        }
        crate::graphics::Shape::Pill => {
            let r = rect.width().min(rect.height()) / 2.0;
            canvas.draw_rrect(RRect::new_rect_xy(rect, r, r), &paint);
        }
        crate::graphics::Shape::Circle => {
            // Circle == Pill == Compose `RoundedCornerShape(50)`: percent corners
            // against the box, so a non-square box is a stadium. Drawing a true
            // circle left most of a wide box unpainted.
            let r = rect.width().min(rect.height()) / 2.0;
            canvas.draw_rrect(RRect::new_rect_xy(rect, r, r), paint);
        }
    }
}

/// Fill `shape` with a [`crate::graphics::Brush`].
///
/// The two absolute cases are resolved here, against the node's rect: a gradient's coordinates are
/// fractions of the bounds (see the [`crate::graphics::brush`] module docs), so a gradient fills any size node.
pub(super) fn draw_brush(canvas: &Canvas, rect: Rect, brush: &crate::graphics::Brush, shape: &crate::graphics::Shape) {
    match brush {
        crate::graphics::Brush::Solid(color) => draw_background(canvas, rect, color, shape),
        _ => {
            let Some(paint) = brush_paint(brush, rect) else { return };
            paint_shape(canvas, rect, &paint, shape);
        }
    }
}

/// A paint whose shader is `brush`, in the node's `rect` — `None` for a degenerate brush (no colors,
/// or a zero-length gradient), which paints nothing rather than panicking.
pub(super) fn brush_paint(brush: &crate::graphics::Brush, rect: Rect) -> Option<Paint> {
    use skia_safe::gradient::{Colors, Gradient, Interpolation};

    let (stops, positions, tile, geometry) = match brush {
        crate::graphics::Brush::Solid(_) => return None,
        crate::graphics::Brush::Linear(g) => (
            g.stops(),
            g.positions_ref(),
            g.tile_mode(),
            (g.from(), g.to(), 0.0),
        ),
        crate::graphics::Brush::Radial(g) => (
            g.stops(),
            g.positions_ref(),
            g.tile_mode(),
            (g.from(), g.from(), g.radius_value()),
        ),
        crate::graphics::Brush::Sweep(g) => (
            g.stops(),
            g.positions_ref(),
            g.tile_mode(),
            (g.from(), g.from(), 0.0),
        ),
    };
    if stops.is_empty() {
        return None;
    }

    let color4fs: Vec<Color4f> = stops.iter().map(Color4f::from).collect();
    let colors = Colors::new(&color4fs, positions, skia_tile(tile), None);
    let gradient = Gradient::new(colors, Interpolation::default());

    // Fractional → absolute, against the node's own rect.
    let absolute = |(fx, fy): (f32, f32)| {
        skia_safe::Point::new(
            rect.left + fx * rect.width(),
            rect.top + fy * rect.height(),
        )
    };
    let (from, to, radius) = geometry;
    let shader = match brush {
        crate::graphics::Brush::Linear(_) => {
            let (start, end) = (absolute(from), absolute(to));
            if start == end {
                return None; // A zero-length gradient has no direction to run in.
            }
            skia_safe::gradient::shaders::linear_gradient((start, end), &gradient, None)
        }
        crate::graphics::Brush::Radial(_) => {
            // The radius is a fraction of the SHORTER side, so the gradient reaches the nearest pair
            // of edges whatever the aspect ratio.
            let r = radius * rect.width().min(rect.height());
            if r <= 0.0 {
                return None;
            }
            skia_safe::gradient::shaders::radial_gradient((absolute(from), r), &gradient, None)
        }
        // A full turn, in DEGREES: Skia documents the range as degrees with 0 at the +x axis and the
        // CSS conic-gradient convention (`skia/include/effects/SkGradient.h`, `SweepGradient`). The
        // sweep runs clockwise from 3 o'clock, so `Brush::sweep_gradient` needs no angle options.
        crate::graphics::Brush::Sweep(_) => skia_safe::gradient::shaders::sweep_gradient(
            absolute(from),
            (0.0, 360.0),
            &gradient,
            None,
        ),
        crate::graphics::Brush::Solid(_) => return None,
    }?;

    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_shader(shader);
    Some(paint)
}

pub(super) fn skia_tile(tile: crate::graphics::BrushTile) -> skia_safe::TileMode {
    match tile {
        crate::graphics::BrushTile::Clamp => skia_safe::TileMode::Clamp,
        crate::graphics::BrushTile::Repeat => skia_safe::TileMode::Repeat,
        crate::graphics::BrushTile::Mirror => skia_safe::TileMode::Mirror,
    }
}

