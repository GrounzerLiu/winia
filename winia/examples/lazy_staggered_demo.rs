//! Staggered grid demo — every knob live: lanes, item count, height pattern, both gaps, RTL, jumps.
//!
//! The tiles cycle through three height patterns, so the lanes drift the way a real waterfall does: an
//! item goes into the lane whose cursor is lowest, not into a shared row. Every tile carries a
//! `test_tag` (`stagger-item-N`) and the controls do too, so `tests/ui_lazy_grid_test.rs` can drive them
//! through the fixture scenario `lazy_staggered`.
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

/// The main size a tile takes under the chosen pattern.
fn tile_height(pattern: u32, index: usize) -> f32 {
    match pattern {
        // Uniform: a plain grid, for comparison with the waterfall below.
        0 => 56.0,
        // Tall: every third tile towers over its neighbours.
        1 => if index % 3 == 0 { 132.0 } else { 48.0 },
        // Short: a dense feed.
        2 => 24.0 + (index % 4) as f32 * 8.0,
        // Mixed: what the demo opens with.
        _ => 60.0 + (index % 3) as f32 * 30.0,
    }
}

#[composable]
fn lazy_staggered_demo(ctx: &mut ComposeCtx) {
    let scroll = ctx.remember(|| ScrollState::new()).get();
    let lanes = ctx.remember(|| State::new(2u32)).get();
    let count = ctx.remember(|| State::new(40usize)).get();
    let pattern = ctx.remember(|| State::new(3u32)).get();
    let gap = ctx.remember(|| State::new(8.0f32)).get();
    let cross_gap = ctx.remember(|| State::new(8.0f32)).get();
    let rtl = ctx.remember(|| State::new(false)).get();

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .spacing(10.0)
        .build(ctx, |ctx| {
            Text::new("Staggered grid — every knob live").font_size(18.0).build(ctx);
            Text::new(format!(
                "{} lanes | {} items | pattern {} | gap {:.0} / {:.0} | {}",
                lanes.get(),
                count.get(),
                pattern.get() % 4,
                gap.get(),
                cross_gap.get(),
                if rtl.get() { "RTL" } else { "LTR" }
            ))
            .font_size(12.0)
            .modifier(Modifier::new().test_tag("stagger-status"))
            .build(ctx);

            // Row 1: the structural knobs.
            Row::new().spacing(8.0).build(ctx, |ctx| {
                let l = lanes.clone();
                Button::new()
                    .modifier(Modifier::new().test_tag("lanes-less"))
                    .on_click(move || l.set(l.get().saturating_sub(1).max(1)))
                    .build(ctx, |ctx| Text::new("lanes -").font_size(12.0).build(ctx));
                let l = lanes.clone();
                Button::new()
                    .modifier(Modifier::new().test_tag("lanes-more"))
                    .on_click(move || l.set((l.get() + 1).min(4)))
                    .build(ctx, |ctx| Text::new("lanes +").font_size(12.0).build(ctx));
                let c = count.clone();
                Button::new()
                    .modifier(Modifier::new().test_tag("items-less"))
                    .on_click(move || c.set(c.get().saturating_sub(8).max(8)))
                    .build(ctx, |ctx| Text::new("items -").font_size(12.0).build(ctx));
                let c = count.clone();
                Button::new()
                    .modifier(Modifier::new().test_tag("items-more"))
                    .on_click(move || c.set((c.get() + 8).min(160)))
                    .build(ctx, |ctx| Text::new("items +").font_size(12.0).build(ctx));
            });

            // Row 2: the pattern, the jumps and the direction.
            Row::new().spacing(8.0).build(ctx, |ctx| {
                let p = pattern.clone();
                Button::new()
                    .modifier(Modifier::new().test_tag("pattern"))
                    .on_click(move || p.set((p.get() + 1) % 4))
                    .build(ctx, |ctx| Text::new("heights").font_size(12.0).build(ctx));
                let s = scroll.clone();
                Button::new()
                    .modifier(Modifier::new().test_tag("jump-top"))
                    .on_click(move || s.offset.set(0.0))
                    .build(ctx, |ctx| Text::new("top").font_size(12.0).build(ctx));
                let s = scroll.clone();
                Button::new()
                    .modifier(Modifier::new().test_tag("jump-end"))
                    .on_click(move || s.offset.set(1.0e9))
                    .build(ctx, |ctx| Text::new("end").font_size(12.0).build(ctx));
                let r = rtl.clone();
                Switch::new(rtl.get())
                    .modifier(Modifier::new().test_tag("rtl"))
                    .on_checked_change(move |value| r.set(value))
                    .build(ctx, |_| {});
            });

            // The two gaps.
            Row::new().spacing(8.0).build(ctx, |ctx| {
                Text::new("gap").font_size(12.0).build(ctx);
                let g = gap.clone();
                Slider::new(gap.get())
                    .modifier(Modifier::new().test_tag("gap"))
                    .value_range(0.0, 24.0)
                    .on_value_change(move |value| g.set(value))
                    .build(ctx);
            });
            Row::new().spacing(8.0).build(ctx, |ctx| {
                Text::new("cross").font_size(12.0).build(ctx);
                let g = cross_gap.clone();
                Slider::new(cross_gap.get())
                    .modifier(Modifier::new().test_tag("cross-gap"))
                    .value_range(0.0, 24.0)
                    .on_value_change(move |value| g.set(value))
                    .build(ctx);
            });

            LazyVerticalStaggeredGrid::new()
                .lanes(GridCells::Fixed(lanes.get() as usize))
                .spacing(gap.get())
                .cross_spacing(cross_gap.get())
                .state(scroll)
                .modifier(
                    Modifier::new()
                        .fill_max_width()
                        .height(460.0)
                        .layout_direction(if rtl.get() {
                            LayoutDirection::Rtl
                        } else {
                            LayoutDirection::Ltr
                        }),
                )
                .items(count.get(), move |ctx, index| {
                    Stack::new()
                        .modifier(
                            Modifier::new()
                                .test_tag(format!("stagger-item-{index}"))
                                .height(tile_height(pattern.get(), index))
                                .background(palette(index), Shape::rounded(6.0)),
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
                .size(460.0, 860.0)
                .title("Staggered grid demo")
                .build(ctx, |ctx| {
                    settings::shell("Staggered Grid", ctx, lazy_staggered_demo);
                });
        });
    });
}
