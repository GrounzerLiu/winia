//! Lazy grid demo — the vertical grid (fixed cells) and the horizontal one (adaptive, RTL).
//!
//! The vertical grid shows three equal cells per line; the horizontal grid takes as many cells as fit
//! at 80 dp and is laid out right to left, which the horizontal scroll modifier mirrors the same way it
//! does for `LazyRow`. Every cell carries a `test_tag`, so the UI suite reads the geometry off
//! the real window (`tests/ui_lazy_grid_test.rs`, fixture scenario `lazy_grid`).
//!
//! Run with `cargo run -p winia --example lazy_grid_demo`.

use winia::prelude::*;

#[path = "common/settings.rs"]
mod settings;

fn palette(i: usize) -> Color {
    const COLORS: [Color; 6] = [
        Color { r: 0x4C, g: 0xAF, b: 0x50, a: 255 },
        Color { r: 0x21, g: 0x96, b: 0xF3, a: 255 },
        Color { r: 0xFF, g: 0x98, b: 0x00, a: 255 },
        Color { r: 0x9C, g: 0x27, b: 0xB0, a: 255 },
        Color { r: 0x00, g: 0xBC, b: 0xD4, a: 255 },
        Color { r: 0xE9, g: 0x1E, b: 0x63, a: 255 },
    ];
    COLORS[i % COLORS.len()]
}

fn cell(ctx: &mut ComposeCtx, tag: String, height: f32, color: Color) {
    Stack::new()
        .modifier(
            Modifier::new()
                // The grid gives a cell its share of the line; the content has to take it, the same
                // way a Compose grid item fills its span only when it asks to.
                .fill_max_width()
                .height(height)
                .background(color, Shape::rounded(6.0))
                .test_tag(tag),
        )
        .build(ctx, |_| {});
}

#[composable]
fn lazy_grid_demo(ctx: &mut ComposeCtx) {
    let vertical = ctx.remember(|| LazyListState::new()).get();
    let horizontal = ctx.remember(|| LazyListState::new()).get();

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .spacing(12.0)
        .build(ctx, |ctx| {
            Text::new("Lazy grids").font_size(18.0).build(ctx);

            // Three equal cells per line, scrolling vertically.
            LazyVerticalGrid::new()
                .cells(GridCells::Fixed(3))
                .state(vertical)
                .spacing(8.0)
                .cross_spacing(8.0)
                .modifier(Modifier::new().fill_max_width().height(260.0))
                .items(60, |ctx, index| {
                    cell(ctx, format!("vcell-{index}"), 56.0, palette(index));
                })
                .build(ctx);

            // As many 80 dp cells as fit, laid out right to left.
            LazyHorizontalGrid::new()
                .cells(GridCells::Adaptive(80.0))
                .state(horizontal)
                .spacing(8.0)
                .cross_spacing(8.0)
                .modifier(
                    Modifier::new()
                        .fill_max_width()
                        .height(180.0)
                        .layout_direction(LayoutDirection::Rtl),
                )
                .items(60, |ctx, index| {
                    cell(ctx, format!("hcell-{index}"), 48.0, palette(index + 2));
                })
                .build(ctx);
        });
}

pub fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(460.0, 760.0)
                .title("Lazy grid demo")
                .build(ctx, |ctx| {
                    settings::shell("Lazy Grid", ctx, lazy_grid_demo);
                });
        });
    });
}
