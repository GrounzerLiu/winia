//! Drawing the two selection handles.
//!
//! They are drawn with the CONTENT (after `render_pass1` returns), not per node: a handle belongs to
//! the selection, and its two ends can live in different runs. Compose keeps them in an overlay layer
//! for the same reason; here the pass simply runs last on the same canvas.

use crate::layout::node::LayoutNode;
use crate::layout::selection_handles::selection_handle_points;
use crate::text::selection::SelectionRegistrar;
use skia_safe::{Canvas, Paint};

/// Radius of the round grab area.
pub(crate) const HANDLE_RADIUS: f32 = 5.0;

pub(crate) fn draw(nodes: &[LayoutNode], root: usize, canvas: &Canvas) {
    let mut seen: Vec<SelectionRegistrar> = Vec::new();
    for idx in 0..nodes.len() {
        let Some(reg) = nodes[idx].registrar.borrow().as_ref().cloned() else {
            continue;
        };
        if seen.iter().any(|r| r.is_same(&reg)) {
            continue;
        }
        seen.push(reg.clone());
        let Some(points) = selection_handle_points(nodes, root, &reg) else {
            continue;
        };
        let color = nodes[idx].selection_color.get();
        let skia_color = skia_safe::Color::from_argb(255, color.r, color.g, color.b);
        for point in points {
            let centre = (point.x, point.y + HANDLE_RADIUS);
            let mut fill = Paint::default();
            fill.set_anti_alias(true);
            fill.set_color(skia_color);
            canvas.draw_circle(centre, HANDLE_RADIUS, &fill);
            // A short stem ties the dot to the line it belongs to (a plain rect: a Path would need
            // a builder here for two segments).
            let mut stroke = Paint::default();
            stroke.set_anti_alias(true);
            stroke.set_color(skia_color);
            canvas.draw_rect(
                skia_safe::Rect::from_xywh(point.x - 1.0, point.y, 2.0, HANDLE_RADIUS + 1.0),
                &stroke,
            );
        }
    }
}
