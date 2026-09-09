//! FlowRow / FlowColumn 演示 — 流式换行/换列 + chip 组联动
//!
//! 对标 Compose `FlowRowSimpleUsageExample`（chip 过滤器 UI）。

use letclone::clone;
use winia::prelude::*;
use winia::ui::Chip;

const FILTERS: &[&str] = &[
    "Price: High to Low",
    "Avg rating: 4+",
    "Free breakfast",
    "Free cancellation",
    "£50 pn",
    "Pool",
    "Pet friendly",
    "Ocean view",
];

#[composable]
fn flow_demo_ui(ctx: &mut ComposeCtx) {
    let selected: State<Vec<bool>> = ctx.remember(|| vec![false; FILTERS.len()]);
    let scroll = ctx.remember(|| ScrollState::new()).get();

    Column::new()
        .modifier(
            Modifier::new()
                .fill_max_size()
                .padding(16.0)
                .vertical_scroll(scroll),
        )
        .spacing(12.0)
        .build(ctx, |ctx| {
            Text::new("■ FlowRow — filter chips wrap")
                .font_size(16.0)
                .build(ctx);

            FlowRow::new()
                .main_spacing(8.0)
                .cross_spacing(8.0)
                .modifier(
                    Modifier::new()
                        .fill_max_width()
                        .background(
                            Color::from_argb(20, 50, 100, 200),
                            Shape::rounded(8.0),
                        )
                        .padding(8.0),
                )
                .build(ctx, |ctx| {
                    for (i, label) in FILTERS.iter().enumerate() {
                        let label = label.to_string();
                        ctx.key(i, {
                            clone!(selected);
                            move |ctx| {
                                let is_sel = selected.get()[i];
                                Chip::filter(
                                    is_sel,
                                    {
                                        clone!(label);
                                        move |ctx| {
                                            Text::new(label.clone()).font_size(13.0).build(ctx);
                                        }
                                    },
                                    {
                                        clone!(selected);
                                        move || {
                                            selected.update(|v| v[i] = !v[i]);
                                        }
                                    },
                                )
                                .build(ctx);
                            }
                        });
                    }
                });

            Text::new("■ FlowRow — max 3 per row")
                .font_size(16.0)
                .build(ctx);

            FlowRow::new()
                .max_items_in_row(3)
                .main_spacing(8.0)
                .cross_spacing(8.0)
                .modifier(
                    Modifier::new()
                        .fill_max_width()
                        .background(
                            Color::from_argb(20, 0, 150, 50),
                            Shape::rounded(8.0),
                        )
                        .padding(8.0),
                )
                .build(ctx, |ctx| {
                    for n in 0..7 {
                        ctx.key(("box", n), |ctx| {
                            Stack::new()
                                .modifier(
                                    Modifier::new()
                                        .size(48.0, 48.0)
                                        .background(
                                            Color::from_argb(255, 80, 160, 100),
                                            Shape::rounded(8.0),
                                        ),
                                )
                                .build(ctx, |ctx| {
                                    Text::new(n.to_string())
                                        .font_size(14.0)
                                        .build(ctx);
                                });
                        });
                    }
                });

            Text::new("■ FlowColumn — wraps to next column")
                .font_size(16.0)
                .build(ctx);

            FlowColumn::new()
                .main_spacing(8.0)
                .cross_spacing(8.0)
                .modifier(
                    Modifier::new()
                        .fill_max_width()
                        .height(220.0)
                        .background(
                            Color::from_argb(20, 150, 50, 0),
                            Shape::rounded(8.0),
                        )
                        .padding(8.0),
                )
                .build(ctx, |ctx| {
                    for n in 0..8 {
                        ctx.key(("col", n), |ctx| {
                            Stack::new()
                                .modifier(
                                    Modifier::new()
                                        .size(90.0, 40.0)
                                        .background(
                                            Color::from_argb(255, 200, 120, 60),
                                            Shape::rounded(8.0),
                                        ),
                                )
                                .build(ctx, |ctx| {
                                    Text::new(format!("C{n}"))
                                        .font_size(14.0)
                                        .build(ctx);
                                });
                        });
                    }
                });
        });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(480.0, 640.0)
                .title("Flow Demo")
                .build(ctx, |ctx| {
                    flow_demo_ui(ctx);
                });
        });
    });
}
