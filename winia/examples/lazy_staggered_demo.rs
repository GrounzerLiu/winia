//! Staggered (waterfall) grid demo — two lanes of tiles with different heights.
//!
//! The tiles cycle through three heights, so the lanes drift apart the way a real waterfall does: an
//! item goes into the lane whose cursor is lowest, not into a shared row. Every tile carries a
//! `test_tag`, and the UI suite (`tests/ui_lazy_grid_test.rs`) reads the placement off the
//! real window (fixture scenario `lazy_staggered`).
//!
//! Run with `cargo run -p winia --example lazy_staggered_demo`.

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

#[composable]
fn lazy_staggered_demo(ctx: &mut ComposeCtx) {
    let scroll = ctx.remember(|| ScrollState::new()).get();

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .spacing(12.0)
        .build(ctx, |ctx| {
            Text::new("Staggered grid").font_size(18.0).build(ctx);
            LazyVerticalStaggeredGrid::new()
                .lanes(GridCells::Fixed(2))
                .spacing(8.0)
                .cross_spacing(8.0)
                .state(scroll)
                .modifier(Modifier::new().fill_max_width().height(600.0))
                .items(40, |ctx, index| {
                    // Three heights, so the lanes drift: the next item goes into the shorter one.
                    let height = 60.0 + (index % 3) as f32 * 30.0;
                    Stack::new()
                        .modifier(
                            Modifier::new()
                                .fill_max_width()
                                .height(height)
                                .background(palette(index), Shape::rounded(6.0))
                                .test_tag(format!("stagger-item-{index}")),
                        )
                        .build(ctx, |_| {});
                })
                .build(ctx);
        });
}

pub fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(460.0, 760.0)
                .title("Staggered grid demo")
                .build(ctx, |ctx| {
                    settings::shell("Staggered Grid", ctx, lazy_staggered_demo);
                });
        });
    });
}
