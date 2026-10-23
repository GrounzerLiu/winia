//! FlowRow / FlowColumn feature showcase.
//!
//! The same UI is compiled into the single fixture binary by `tests/ui_fixtures/fixture_all.rs`,
//! so the demo and the UI test exercise one source: wrapping, max items/lines, Clip vs Visible,
//! cross arrangement, weight, baseline alignment and fillMaxRowHeight / fillMaxColumnWidth.

use winia::layout::IntrinsicSize;
use winia::prelude::*;

#[path = "common/settings.rs"]
mod settings;

fn block(ctx: &mut ComposeCtx, tag: &'static str, w: f32, h: f32, color: Color, label: &str) {
    Stack::new()
        .modifier(
            Modifier::new()
                .size(w, h)
                .animate_item()
                .test_tag(tag)
                .background(color, Shape::rounded(6.0)),
        )
        .build(ctx, |ctx| {
            Text::new(label).font_size(12.0).build(ctx);
        });
}

#[composable]
fn flow_features(ctx: &mut ComposeCtx) {
    let scroll = ctx.remember(|| ScrollState::new()).get();
    let columns = ctx.remember(|| 3usize);

    Column::new()
        .modifier(
            Modifier::new()
                .fill_max_size()
                .padding(16.0)
                .vertical_scroll(scroll),
        )
        .spacing(16.0)
        .build(ctx, |ctx| {
            Text::new("FlowRow — animated reflow (3 x 2 <-> 2 x 3)")
                .font_size(16.0)
                .build(ctx);
            Text::new(format!("{} columns — the cards spring to their new slots", columns.get()))
                .font_size(12.0)
                .build(ctx);
            Button::new()
                .modifier(Modifier::new().test_tag("toggle-columns"))
                .on_click({
                    let columns = columns.clone();
                    move || {
                        let next = if columns.get() == 3 { 2 } else { 3 };
                        columns.set(next);
                    }
                })
                .build(ctx, |ctx| Text::new("toggle columns").build(ctx));
            AnimatedSize::default()
                .modifier(
                    Modifier::new()
                        .fill_max_width()
                        .test_tag("animated-box")
                        .background(
                            Color::from_argb(20, 50, 100, 200),
                            Shape::rounded(8.0),
                        )
                        .padding(8.0),
                )
                .build(ctx, |ctx| {
                    FlowRow::new()
                        .main_spacing(8.0)
                        .cross_spacing(8.0)
                        .max_items_in_row(columns.get())
                        .animate_placement(true)
                        .modifier(Modifier::new().fill_max_width().test_tag("flow-animated"))
                        .build(ctx, |ctx| {
                            ctx.keyed_range("animated", 6, |ctx, n| {
                                block(
                                    ctx,
                                    match n {
                                        0 => "animated-0",
                                        1 => "animated-1",
                                        2 => "animated-2",
                                        3 => "animated-3",
                                        4 => "animated-4",
                                        _ => "animated-5",
                                    },
                                    70.0,
                                    44.0,
                                    Color::from_argb(255, 220, 160, 100),
                                    &format!("A{n}"),
                                );
                            });
                        });
                });

            Text::new("FlowRow — wraps and caps items")
                .font_size(16.0)
                .build(ctx);

            FlowRow::new()
                .main_spacing(8.0)
                .cross_spacing(8.0)
                .max_items_in_row(3)
                .modifier(
                    Modifier::new()
                        .fill_max_width()
                        .test_tag("flow-wrap")
                        .background(Color::from_argb(20, 50, 100, 200), Shape::rounded(8.0))
                        .padding(8.0),
                )
                .build(ctx, |ctx| {
                    ctx.keyed_range("wrap", 7, |ctx, n| {
                        block(
                            ctx,
                            match n {
                                0 => "wrap-0",
                                1 => "wrap-1",
                                2 => "wrap-2",
                                3 => "wrap-3",
                                4 => "wrap-4",
                                5 => "wrap-5",
                                _ => "wrap-6",
                            },
                            56.0,
                            28.0,
                            Color::from_argb(255, 100, 160, 220),
                            &format!("W{n}"),
                        );
                    });
                });

            Text::new("FlowRow — max_lines(1), Clip vs Visible")
                .font_size(16.0)
                .build(ctx);

            FlowRow::new()
                .main_spacing(8.0)
                .max_items_in_row(3)
                .max_lines(1)
                .overflow(FlowOverflow::Clip)
                .modifier(
                    Modifier::new()
                        .fill_max_width()
                        .test_tag("flow-clip")
                        .background(Color::from_argb(20, 150, 50, 0), Shape::rounded(8.0))
                        .padding(8.0),
                )
                .build(ctx, |ctx| {
                    ctx.keyed_range("clip", 4, |ctx, n| {
                        block(
                            ctx,
                            match n {
                                0 => "clip-0",
                                1 => "clip-1",
                                2 => "clip-2",
                                _ => "clip-3",
                            },
                            64.0,
                            24.0,
                            Color::from_argb(255, 220, 120, 60),
                            &format!("C{n}"),
                        );
                    });
                });

            FlowRow::new()
                .main_spacing(8.0)
                .max_items_in_row(3)
                .max_lines(1)
                .overflow(FlowOverflow::Visible)
                .modifier(
                    Modifier::new()
                        .fill_max_width()
                        .test_tag("flow-visible")
                        .background(Color::from_argb(20, 100, 50, 100), Shape::rounded(8.0))
                        .padding(8.0),
                )
                .build(ctx, |ctx| {
                    ctx.keyed_range("visible", 4, |ctx, n| {
                        block(
                            ctx,
                            match n {
                                0 => "visible-0",
                                1 => "visible-1",
                                2 => "visible-2",
                                _ => "visible-3",
                            },
                            64.0,
                            24.0,
                            Color::from_argb(255, 160, 100, 200),
                            &format!("V{n}"),
                        );
                    });
                });

            Text::new("FlowRow — cross arrangement Center")
                .font_size(16.0)
                .build(ctx);

            FlowRow::new()
                .max_items_in_row(2)
                .cross_arrangement(Arrangement::Center)
                .main_spacing(8.0)
                .cross_spacing(8.0)
                .modifier(
                    Modifier::new()
                        .width(260.0)
                        .height(120.0)
                        .test_tag("flow-cross")
                        .background(Color::from_argb(20, 50, 100, 200), Shape::rounded(8.0))
                        .padding(8.0),
                )
                .build(ctx, |ctx| {
                    ctx.keyed_range("cross", 4, |ctx, n| {
                        block(
                            ctx,
                            match n {
                                0 => "cross-0",
                                1 => "cross-1",
                                2 => "cross-2",
                                _ => "cross-3",
                            },
                            80.0,
                            20.0,
                            Color::from_argb(255, 100, 200, 160),
                            &format!("X{n}"),
                        );
                    });
                });

            Text::new("FlowRow — weight + fillMaxRowHeight")
                .font_size(16.0)
                .build(ctx);

            FlowRow::new()
                .main_spacing(8.0)
                .modifier(
                    Modifier::new()
                        .width(280.0)
                        .height(84.0)
                        .test_tag("flow-weight")
                        .background(Color::from_argb(20, 100, 50, 100), Shape::rounded(8.0))
                        .padding(8.0),
                )
                .build(ctx, |ctx| {
                    block(
                        ctx,
                        "weight-fixed",
                        40.0,
                        40.0,
                        Color::from_argb(255, 100, 160, 220),
                        "fix",
                    );
                    Stack::new()
                        .modifier(
                            Modifier::new()
                                .size(40.0, 20.0)
                                .layout_weight(1.0)
                                .fill_max_row_height()
                                .test_tag("weight-fill")
                                .background(Color::from_argb(255, 220, 160, 100), Shape::rounded(6.0)),
                        )
                        .build(ctx, |ctx| {
                            Text::new("weight").font_size(12.0).build(ctx);
                        });
                });

            Text::new("FlowRow — baseline alignment")
                .font_size(16.0)
                .build(ctx);

            FlowRow::new()
                .main_spacing(10.0)
                .modifier(
                    Modifier::new()
                        .width(300.0)
                        .test_tag("flow-baseline")
                        .background(Color::from_argb(20, 50, 100, 200), Shape::rounded(8.0))
                        .padding(8.0),
                )
                .build(ctx, |ctx| {
                    Text::new("small")
                        .font_size(12.0)
                        .modifier(Modifier::new().test_tag("baseline-small").align_by_baseline())
                        .build(ctx);
                    Text::new("BIG")
                        .font_size(28.0)
                        .modifier(Modifier::new().test_tag("baseline-big").align_by_baseline())
                        .build(ctx);
                });

            Text::new("FlowRow — width(IntrinsicSize.Max)")
                .font_size(16.0)
                .build(ctx);

            Row::new()
                .modifier(Modifier::new().fill_max_width())
                .build(ctx, |ctx| {
                    FlowRow::new()
                        .main_spacing(8.0)
                        .modifier(
                            Modifier::new()
                                .width(IntrinsicSize::Max)
                                .test_tag("flow-intrinsic")
                                .background(
                                    Color::from_argb(20, 100, 50, 100),
                                    Shape::rounded(8.0),
                                )
                                .padding(8.0),
                        )
                        .build(ctx, |ctx| {
                            ctx.keyed_range("intrinsic", 3, |ctx, n| {
                                block(
                                    ctx,
                                    match n {
                                        0 => "intrinsic-0",
                                        1 => "intrinsic-1",
                                        _ => "intrinsic-2",
                                    },
                                    56.0,
                                    24.0,
                                    Color::from_argb(255, 100, 200, 160),
                                    &format!("I{n}"),
                                );
                            });
                        });
                });

            Text::new("FlowColumn — fillMaxColumnWidth")
                .font_size(16.0)
                .build(ctx);

            FlowColumn::new()
                .main_spacing(8.0)
                .modifier(
                    Modifier::new()
                        .width(240.0)
                        .height(140.0)
                        .test_tag("flow-column")
                        .background(Color::from_argb(20, 100, 50, 100), Shape::rounded(8.0))
                        .padding(8.0),
                )
                .build(ctx, |ctx| {
                    block(
                        ctx,
                        "column-wide",
                        40.0,
                        30.0,
                        Color::from_argb(255, 120, 80, 160),
                        "wide",
                    );
                    Stack::new()
                        .modifier(
                            Modifier::new()
                                .size(20.0, 30.0)
                                .fill_max_column_width()
                                .test_tag("column-fill")
                                .background(Color::from_argb(255, 100, 200, 160), Shape::rounded(6.0)),
                        )
                        .build(ctx, |ctx| {
                            Text::new("fill").font_size(12.0).build(ctx);
                        });
                });
        });
}

pub fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(560.0, 820.0)
                .title("Flow Features")
                .build(ctx, |ctx| {
                    settings::shell("Flow Features", ctx, flow_features);
                });
        });
    });
}
