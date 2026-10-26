//! Grid demo — the four things the layout does: fixed tracks, weighted tracks, spans, dense flow.
//!
//! Each card is one case, and every cell carries a `test_tag` so the UI suite can read the geometry
//! off the real window (`tests/ui_grid_test.rs`, fixture scenario `grid`). The cells are sized
//! smaller than their track on purpose: the default cell alignment STRETCHES a grid child to its
//! cell, so the tag's rect reveals the resolved track, not the child's own size.
//!
//! Run with `cargo run -p winia --example grid_demo`.

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

/// One stretched cell: Compose's `gridItem` default is TopStart, so a cell that is meant to fill its
/// track says so with an explicit STRETCH (and then a tag's rect is the resolved track, which is what
/// the UI test reads).
fn cell(ctx: &mut ComposeCtx, tag: &str, height: f32, color: Color) {
    Stack::new()
        .modifier(
            Modifier::new()
                .size(10.0, height)
                .grid_item_aligned(1, 1, ContentAlignment::STRETCH)
                .background(color, Shape::Rectangle)
                .test_tag(tag),
        )
        .build(ctx, |_| {});
}

/// A stretched cell spanning `columns` columns.
fn wide_cell(ctx: &mut ComposeCtx, tag: &str, columns: u32, color: Color) {
    Stack::new()
        .modifier(
            Modifier::new()
                .size(10.0, 30.0)
                .grid_item_aligned(columns, 1, ContentAlignment::STRETCH)
                .background(color, Shape::Rectangle)
                .test_tag(tag),
        )
        .build(ctx, |_| {});
}

fn card(ctx: &mut ComposeCtx, tag: &str, title: &str, body: impl FnOnce(&mut ComposeCtx)) {
    Column::new()
        .modifier(
            Modifier::new()
                .fill_max_width()
                .padding(12.0)
                .background(Color { r: 0xF2, g: 0xF3, b: 0xF7, a: 255 }, Shape::Rectangle)
                .test_tag(tag),
        )
        .spacing(8.0)
        .build(ctx, |ctx| {
            Text::new(title).font_size(13.0).build(ctx);
            body(ctx);
        });
}

#[composable]
fn grid_demo(ctx: &mut ComposeCtx) {
    let scroll = ctx.remember(|| ScrollState::new()).get();

    Column::new()
        .modifier(
            Modifier::new()
                .fill_max_size()
                .padding(16.0)
                .vertical_scroll(scroll),
        )
        .spacing(14.0)
        .build(ctx, |ctx| {
            Text::new("Grid").font_size(18.0).build(ctx);

            // 1. Three fixed 80 dp columns: each cell lands one track apart, and the fourth child
            //    starts the second (implicit) row.
            card(ctx, "grid-fixed-card", "1. Fixed tracks (80 dp, gap 8)", |ctx| {
                Grid::new(vec![GridTrackSpec::fixed(80.0); 3])
                    .gap(8.0)
                    .build(ctx, |ctx| {
                        for i in 0..6 {
                            cell(ctx, &format!("grid-fixed-{i}"), 40.0, palette(i));
                        }
                    });
            });

            // 2. A fixed column and two weighted ones: the weight shares what the 90 dp track and
            //    the gaps leave over, so the second column is half the third.
            card(ctx, "grid-weight-card", "2. Fixed + weights (90 | 1 | 2)", |ctx| {
                Grid::new(vec![
                    GridTrackSpec::fixed(90.0),
                    GridTrackSpec::weight(1.0),
                    GridTrackSpec::weight(2.0),
                ])
                .gap(8.0)
                .build(ctx, |ctx| {
                    for i in 0..3 {
                        cell(ctx, &format!("grid-weight-{i}"), 40.0, palette(i));
                    }
                });
            });

            // 3. A span: the child covers two columns, so it is measured at their combined width and
            //    the cell after it moves past both.
            card(ctx, "grid-span-card", "3. A child spanning two columns", |ctx| {
                Grid::new(vec![
                    GridTrackSpec::fixed(70.0),
                    GridTrackSpec::fixed(70.0),
                    GridTrackSpec::fixed(70.0),
                ])
                .gap(6.0)
                .build(ctx, |ctx| {
                    cell(ctx, "grid-span-0", 30.0, palette(0));
                    wide_cell(ctx, "grid-span-2col", 2, palette(1));
                    cell(ctx, "grid-span-1", 30.0, palette(2));
                });
            });

            // 4. Dense flow: the second child needs two of the three columns, so it drops to the next
            //    row and leaves the last cell of row 0 free. The third child is single-cell, and a
            //    DENSE scan sends it back to fill that hole.
            card(ctx, "grid-dense-card", "4. Dense flow fills the hole", |ctx| {
                Grid::new(vec![GridTrackSpec::fixed(70.0); 3])
                    .gap(6.0)
                    .flow(GridFlow::RowDense)
                    .build(ctx, |ctx| {
                        wide_cell(ctx, "grid-dense-0", 2, palette(0));
                        wide_cell(ctx, "grid-dense-1", 2, palette(1));
                        cell(ctx, "grid-dense-2", 30.0, palette(2));
                    });
            });

            // 5. A cell alignment: a centred child keeps its own size inside the cell.
            card(ctx, "grid-align-card", "5. A centred child inside its cell", |ctx| {
                Grid::new(vec![GridTrackSpec::fixed(120.0)])
                    .build(ctx, |ctx| {
                        Stack::new()
                            .modifier(
                                Modifier::new()
                                    .size(30.0, 30.0)
                                    .grid_item_aligned(1, 1, ContentAlignment::CENTER)
                                    .background(palette(3), Shape::Rectangle)
                                    .test_tag("grid-align-center"),
                            )
                            .build(ctx, |_| {});
                    });
            });
        });
}

pub fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(460.0, 860.0)
                .title("Grid demo")
                .build(ctx, |ctx| {
                    settings::shell("Grid", ctx, grid_demo);
                });
        });
    });
}
