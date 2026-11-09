//! The selection context menu: a small window-hosted popup drawn after the content.
//!
//! It is NOT a composition child of the container it describes. Hosting a `DropdownMenu` under a
//! `SelectionContainer` was tried twice and both times broke selection IN that container (the second
//! time only after an open/dismiss cycle, measured: a drag that used to select `2..28` stopped
//! selecting at all). A menu that is only paint plus hit-testing cannot perturb anything.

use crate::app::gesture::SelectionMenu;
use skia_safe::{Canvas, Paint};

pub(crate) const WIDTH: f32 = 168.0;
pub(crate) const ROW_HEIGHT: f32 = 34.0;
pub(crate) const PADDING: f32 = 6.0;
/// The labels, in order. The actions live in the pointer layer (they need the registrar).
pub(crate) const ITEMS: [&str; 2] = ["Copy", "Select all"];

const BG: (u8, u8, u8) = (44, 41, 50);
const HOVER: (u8, u8, u8) = (66, 62, 76);
const TEXT: (u8, u8, u8) = (232, 227, 240);

/// The menu box in scene coordinates, clamped so it stays inside the window.
pub(crate) fn menu_rect(menu: &SelectionMenu, window: (f32, f32)) -> (f32, f32, f32, f32) {
    let height = PADDING * 2.0 + ROW_HEIGHT * ITEMS.len() as f32;
    // The box starts one padding ABOVE the press point, so the first row begins exactly at the
    // press: a menu that hangs from the point leaves the pointer in its own padding, which reads
    // as "nothing highlighted" even though the pointer is over the menu.
    let x = menu.x.min(window.0 - WIDTH - 4.0).max(4.0);
    let y = (menu.y - PADDING).min(window.1 - height - 4.0).max(4.0);
    (x, y, WIDTH, height)
}

/// Which row `point` is over, if any.
pub(crate) fn row_at(menu: &SelectionMenu, window: (f32, f32), point: (f32, f32)) -> Option<usize> {
    let (x, y, w, h) = menu_rect(menu, window);
    if point.0 < x || point.0 > x + w || point.1 < y || point.1 > y + h {
        return None;
    }
    let row = ((point.1 - y - PADDING) / ROW_HEIGHT).floor();
    (row >= 0.0 && (row as usize) < ITEMS.len()).then_some(row as usize)
}

pub(crate) fn draw(menu: Option<&SelectionMenu>, window: (f32, f32), canvas: &Canvas) {
    let Some(menu) = menu else { return };
    // Compose's menu motion: a short scale+fade. The scale is about the press point, so the menu
    // grows out of where the user clicked.
    let progress = menu.progress(std::time::Instant::now());
    if progress <= 0.0 {
        return;
    }
    let alpha = (progress * 255.0) as u8;
    let scale = 0.85 + 0.15 * progress;
    canvas.save();
    canvas.translate((menu.x, menu.y));
    canvas.scale((scale, scale));
    canvas.translate((-menu.x, -menu.y));
    let (x, y, w, h) = menu_rect(menu, window);
    let mut bg = Paint::default();
    bg.set_anti_alias(true);
    bg.set_color(skia_safe::Color::from_argb(alpha, BG.0, BG.1, BG.2));
    canvas.draw_round_rect(skia_safe::Rect::from_xywh(x, y, w, h), 8.0, 8.0, &bg);
    for (index, label) in ITEMS.iter().enumerate() {
        let row_y = y + PADDING + ROW_HEIGHT * index as f32;
        if menu.hovered == Some(index) {
            let mut hover = Paint::default();
            hover.set_anti_alias(true);
            hover.set_color(skia_safe::Color::from_argb(
                alpha, HOVER.0, HOVER.1, HOVER.2,
            ));
            canvas.draw_round_rect(
                skia_safe::Rect::from_xywh(x + 3.0, row_y + 1.0, w - 6.0, ROW_HEIGHT - 2.0),
                6.0,
                6.0,
                &hover,
            );
        }
        let color = crate::graphics::Color::from_argb(alpha, TEXT.0, TEXT.1, TEXT.2);
        let paragraph = crate::text::build_plain_paragraph(label, 14.0, color, w - 24.0);
        paragraph.paint(canvas, x + 12.0, row_y + 9.0);
    }
    canvas.restore();
}
