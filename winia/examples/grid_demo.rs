//! Grid demo — every knob live: track presets, gaps, flow, spans, cell alignment, areas and RTL.
//!
//! The controls drive a static `Grid`: "tracks" swaps the column presets (fixed / weighted /
//! mixed / intrinsic), "flow" walks Row / Column / RowDense / ColumnDense, "align" re-places the cells,
//! "cells +" grows the set, the switches give cell 0 a two-column span, reserve a named area for cell 1
//! and mirror the grid. Every cell carries `grid-cell-N` and the controls carry their own tags, so
//! `tests/ui_grid_test.rs` drives them through fixture scenario `grid`.
//!
//! Run with `cargo run -p winia --example grid_demo`.

use letclone::clone;
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

/// The column presets the "tracks" button cycles.
fn tracks(preset: u32) -> Vec<GridTrackSpec> {
    match preset % 4 {
        0 => vec![GridTrackSpec::fixed(90.0); 3],
        1 => vec![GridTrackSpec::weight(1.0); 3],
        2 => vec![
            GridTrackSpec::fixed(80.0),
            GridTrackSpec::weight(1.0),
            GridTrackSpec::percent(0.25),
        ],
        _ => vec![
            GridTrackSpec::FitContent,
            GridTrackSpec::FitContent,
            GridTrackSpec::fixed(90.0),
        ],
    }
}

fn flow_of(index: u32) -> GridFlow {
    match index % 4 {
        0 => GridFlow::Row,
        1 => GridFlow::Column,
        2 => GridFlow::RowDense,
        _ => GridFlow::ColumnDense,
    }
}

fn alignment_of(index: u32) -> ContentAlignment {
    match index % 4 {
        0 => ContentAlignment::TOP_START,
        1 => ContentAlignment::CENTER,
        2 => ContentAlignment::STRETCH,
        _ => ContentAlignment::BOTTOM_END,
    }
}

#[composable]
fn grid_demo(ctx: &mut ComposeCtx) {
    let preset = ctx.remember(|| State::new(0u32)).get();
    let gap = ctx.remember(|| State::new(8.0f32)).get();
    let flow = ctx.remember(|| State::new(0u32)).get();
    let span = ctx.remember(|| State::new(false)).get();
    let align = ctx.remember(|| State::new(2u32)).get();
    let area = ctx.remember(|| State::new(false)).get();
    let rtl = ctx.remember(|| State::new(false)).get();
    let count = ctx.remember(|| State::new(12usize)).get();

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .spacing(10.0)
        .build(ctx, |ctx| {
            Text::new("Grid — every knob live").font_size(18.0).build(ctx);
            Text::new(format!(
                "tracks {} | gap {:.0} | flow {} | span {} | align {} | areas {} | {} | {} cells",
                preset.get() % 4,
                gap.get(),
                flow.get() % 4,
                if span.get() { "2" } else { "1" },
                align.get() % 4,
                if area.get() { "on" } else { "off" },
                if rtl.get() { "RTL" } else { "LTR" },
                count.get()
            ))
            .font_size(11.0)
            .modifier(Modifier::new().test_tag("grid-status"))
            .build(ctx);

            Row::new().spacing(6.0).build(ctx, |ctx| {
                Button::new()
                    .modifier(Modifier::new().test_tag("tracks"))
                    .on_click({
                        clone!(preset);
                        move || preset.set((preset.get() + 1) % 4)
                    })
                    .build(ctx, |ctx| Text::new("tracks").font_size(11.0).build(ctx));
                Button::new()
                    .modifier(Modifier::new().test_tag("flow"))
                    .on_click({
                        clone!(flow);
                        move || flow.set((flow.get() + 1) % 4)
                    })
                    .build(ctx, |ctx| Text::new("flow").font_size(11.0).build(ctx));
                Button::new()
                    .modifier(Modifier::new().test_tag("align"))
                    .on_click({
                        clone!(align);
                        move || align.set((align.get() + 1) % 4)
                    })
                    .build(ctx, |ctx| Text::new("align").font_size(11.0).build(ctx));
                Button::new()
                    .modifier(Modifier::new().test_tag("cells-more"))
                    .on_click({
                        clone!(count);
                        move || count.set((count.get() + 3).min(24))
                    })
                    .build(ctx, |ctx| Text::new("cells +").font_size(11.0).build(ctx));
            });
            Row::new().spacing(6.0).build(ctx, |ctx| {
                Switch::new(span.get())
                    .modifier(Modifier::new().test_tag("span"))
                    .on_checked_change({
                        clone!(span);
                        move |value| span.set(value)
                    })
                    .build(ctx, |_| {});
                Text::new("span 2").font_size(11.0).build(ctx);
                Switch::new(area.get())
                    .modifier(Modifier::new().test_tag("areas"))
                    .on_checked_change({
                        clone!(area);
                        move |value| area.set(value)
                    })
                    .build(ctx, |_| {});
                Text::new("areas").font_size(11.0).build(ctx);
                Switch::new(rtl.get())
                    .modifier(Modifier::new().test_tag("rtl"))
                    .on_checked_change({
                        clone!(rtl);
                        move |value| rtl.set(value)
                    })
                    .build(ctx, |_| {});
                Text::new("RTL").font_size(11.0).build(ctx);
            });
            Row::new().spacing(8.0).build(ctx, |ctx| {
                Text::new("gap").font_size(11.0).build(ctx);
                Slider::new(gap.get())
                    .modifier(Modifier::new().test_tag("gap"))
                    .value_range(0.0, 24.0)
                    .on_value_change({
                        clone!(gap);
                        move |value| gap.set(value)
                    })
                    .build(ctx);
            });

            Grid::new(tracks(preset.get()))
                .gap(gap.get())
                .flow(flow_of(flow.get()))
                .areas(if area.get() {
                    vec![GridArea::new(7, (0, 1), (1, 2))]
                } else {
                    Vec::new()
                })
                // `test_tag` carries the CHILD-shape switches on purpose: a child's span is not a Grid
                // parameter, so the grid's Skip decision cannot see it — the modifier is part of that
                // decision, so keying it on the switches forces the re-entry a real app would get from
                // its own data signature.
                .modifier(Modifier::new()
                    .fill_max_width()
                    .test_tag(format!("grid-shape-{}-{}", span.get(), area.get()))
                    .layout_direction(if rtl.get() {
                    LayoutDirection::Rtl
                } else {
                    LayoutDirection::Ltr
                }))
                .build(ctx, |ctx| {
                    for index in 0..count.get() {
                        // The cells take their cell's width explicitly: a Grid places the child, it does
                        // not stretch it (Compose's TopStart default). Without this a cell with only a
                        // height measures ZERO wide and paints nothing — a blank grid area.
                        let mut modifier = Modifier::new()
                            .test_tag(format!("grid-cell-{index}"))
                            .fill_max_width()
                            .height(48.0)
                            .background(palette(index), Shape::rounded(6.0));
                        if index == 0 {
                            modifier = modifier.grid_item(if span.get() { 2 } else { 1 }, 1);
                        }
                        if area.get() && index == 1 {
                            modifier = modifier.grid_item_in_area(7);
                        }
                        modifier = modifier.grid_item_aligned(1, 1, alignment_of(align.get()));
                        Stack::new().modifier(modifier).build(ctx, |_| {});
                    }
                });
        });
}

pub fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(560.0, 860.0)
                .title("Grid demo")
                .build(ctx, |ctx| {
                    settings::shell("Grid", ctx, grid_demo);
                });
        });
    });
}
