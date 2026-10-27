//! Lazy grid demo — every knob live: cell count (fixed or adaptive), both gaps, item count and the
//! jumps, over both axes.
//!
//! What to look at: the LAST line of the vertical grid is usually partial (the count starts at 62 with
//! three cells), and its cells must stay the width of a SLOT instead of stretching over the line. Every
//! fifth item spans two cells, so the slot arithmetic is visible too.
//!
//! Each grid sits inside a sized `Stack`: a size modifier applied directly to a lazy grid is ignored
//! (measured: it reports a single line), while a sized parent gives it a real viewport.
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

fn vcell(ctx: &mut ComposeCtx, tag: String, color: Color) {
    // A VERTICAL grid fixes the cross axis (width) and leaves the height to the content.
    Stack::new()
        .modifier(
            Modifier::new()
                .fill_max_width()
                .height(44.0)
                .background(color, Shape::rounded(6.0))
                .test_tag(tag),
        )
        .build(ctx, |_| {});
}

fn hcell(ctx: &mut ComposeCtx, tag: String, color: Color) {
    // A HORIZONTAL grid fixes the cross axis (height) and leaves the width to the content.
    Stack::new()
        .modifier(
            Modifier::new()
                .width(80.0)
                .height(44.0)
                .background(color, Shape::rounded(6.0))
                .test_tag(tag),
        )
        .build(ctx, |_| {});
}

#[composable]
fn lazy_grid_demo(ctx: &mut ComposeCtx) {
    let vertical = ctx.remember(|| LazyListState::new()).get();
    let horizontal = ctx.remember(|| LazyListState::new()).get();
    let cells = ctx.remember(|| State::new(3usize)).get();
    let adaptive = ctx.remember(|| State::new(false)).get();
    let count = ctx.remember(|| State::new(62usize)).get();
    let gap = ctx.remember(|| State::new(8.0f32)).get();
    let cross_gap = ctx.remember(|| State::new(8.0f32)).get();

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .spacing(10.0)
        .build(ctx, |ctx| {
            Text::new("Lazy grids — every knob live").font_size(18.0).build(ctx);
            Text::new(format!(
                "{} cells | {} items | gap {:.0}/{:.0} | {}",
                if adaptive.get() {
                    "adaptive 80".to_string()
                } else {
                    format!("fixed {}", cells.get())
                },
                count.get(),
                gap.get(),
                cross_gap.get(),
                "LTR"
            ))
            .font_size(11.0)
            .modifier(Modifier::new().test_tag("lazy-grid-status"))
            .build(ctx);

            Row::new().spacing(6.0).build(ctx, |ctx| {
                let s = cells.clone();
                Button::new()
                    .modifier(Modifier::new().test_tag("lg-cells-less"))
                    .on_click(move || s.set(s.get().saturating_sub(1).max(1)))
                    .build(ctx, |ctx| Text::new("cells -").font_size(11.0).build(ctx));
                let s = cells.clone();
                Button::new()
                    .modifier(Modifier::new().test_tag("lg-cells-more"))
                    .on_click(move || s.set((s.get() + 1).min(6)))
                    .build(ctx, |ctx| Text::new("cells +").font_size(11.0).build(ctx));
                let s = count.clone();
                Button::new()
                    .modifier(Modifier::new().test_tag("lg-items-more"))
                    .on_click(move || s.set((s.get() + 1).min(200)))
                    .build(ctx, |ctx| Text::new("items +").font_size(11.0).build(ctx));
                let s = count.clone();
                Button::new()
                    .modifier(Modifier::new().test_tag("lg-items-less"))
                    .on_click(move || s.set(s.get().saturating_sub(1).max(1)))
                    .build(ctx, |ctx| Text::new("items -").font_size(11.0).build(ctx));
            });

            Row::new().spacing(6.0).build(ctx, |ctx| {
                let s = adaptive.clone();
                Switch::new(adaptive.get())
                    .modifier(Modifier::new().test_tag("lg-adaptive"))
                    .on_checked_change(move |value| s.set(value))
                    .build(ctx, |_| {});
                Text::new("adaptive").font_size(11.0).build(ctx);
                let s = vertical.clone();
                Button::new()
                    .modifier(Modifier::new().test_tag("lg-top"))
                    .on_click(move || s.offset.set(0.0))
                    .build(ctx, |ctx| Text::new("top").font_size(11.0).build(ctx));
                let s = vertical.clone();
                Button::new()
                    .modifier(Modifier::new().test_tag("lg-end"))
                    .on_click(move || s.offset.set(1.0e9))
                    .build(ctx, |ctx| Text::new("end").font_size(11.0).build(ctx));
            });

            Row::new().spacing(8.0).build(ctx, |ctx| {
                Text::new("gap").font_size(11.0).build(ctx);
                let s = gap.clone();
                Slider::new(gap.get())
                    .modifier(Modifier::new().test_tag("lg-gap"))
                    .value_range(0.0, 24.0)
                    .on_value_change(move |value| s.set(value))
                    .build(ctx);
            });
            Row::new().spacing(8.0).build(ctx, |ctx| {
                Text::new("cross").font_size(11.0).build(ctx);
                let s = cross_gap.clone();
                Slider::new(cross_gap.get())
                    .modifier(Modifier::new().test_tag("lg-cross-gap"))
                    .value_range(0.0, 24.0)
                    .on_value_change(move |value| s.set(value))
                    .build(ctx);
            });

            // The tag carries the cell count and the adaptive flag: a pass-through wrapper must declare
            // what its content depends on, or it Skips and the grid never rebuilds.
            Stack::new()
                .modifier(
                    Modifier::new()
                        .fill_max_width()
                        .height(240.0)
                        .test_tag(format!("lg-vgrid-{}-{}", cells.get(), adaptive.get())),
                )
                .build(ctx, |ctx| {
                    LazyVerticalGrid::new()
                        .cells(if adaptive.get() {
                            GridCells::Adaptive(80.0)
                        } else {
                            GridCells::Fixed(cells.get())
                        })
                        .state(vertical)
                        .spacing(gap.get())
                        .cross_spacing(cross_gap.get())
                        .modifier(Modifier::new().fill_max_size())
                        .items_with(
                            count.get(),
                            |index| index as u64,
                            |index| if index % 5 == 4 { 2 } else { 1 },
                            |ctx, index| vcell(ctx, format!("vcell-{index}"), palette(index)),
                        )
                        .build(ctx);
                });

            Stack::new()
                .modifier(
                    Modifier::new()
                        .fill_max_width()
                        .height(120.0)
                        .test_tag("lg-hgrid"),
                )
                .build(ctx, |ctx| {
                    LazyHorizontalGrid::new()
                        .cells(GridCells::Adaptive(120.0))
                        .state(horizontal)
                        .spacing(gap.get())
                        .cross_spacing(cross_gap.get())
                        .modifier(Modifier::new().fill_max_size())
                        .items(40, |ctx, index| {
                            hcell(ctx, format!("hcell-{index}"), palette(index + 2));
                        })
                        .build(ctx);
                });
        });
}

pub fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(480.0, 960.0)
                .title("Lazy grid demo")
                .build(ctx, |ctx| {
                    settings::shell("Lazy Grid", ctx, lazy_grid_demo);
                });
        });
    });
}