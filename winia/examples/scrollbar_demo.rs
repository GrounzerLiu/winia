//! Scrollbar 演示 — 垂直/水平滚动条 + 拖 thumb + 常显开关
//!
//! 对标 CMP 桌面 `VerticalScrollbar` / `HorizontalScrollbar` 用法。

use winia::prelude::*;

#[composable]
fn scrollbar_demo_ui(ctx: &mut ComposeCtx) {
    let vscroll = ctx.remember(|| ScrollState::new()).get();
    let hscroll = ctx.remember(|| ScrollState::new()).get();
    let lazy_state = ctx.remember(|| LazyListState::new()).get();
    let always: State<bool> = ctx.remember(|| false);

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .spacing(8.0)
        .build(ctx, |ctx| {
            Text::new("■ Vertical Scrollbar — drag thumb, wheel the list")
                .font_size(16.0)
                .build(ctx);

            Row::new()
                .modifier(
                    Modifier::new()
                        .fill_max_width()
                        .height(300.0)
                        .background(
                            Color::from_argb(20, 50, 100, 200),
                            Shape::rounded(8.0),
                        )
                        .padding(8.0),
                )
                .build(ctx, |ctx| {
                    Column::new()
                        .modifier(
                            Modifier::new()
                                .fill_max_height()
                                .layout_weight(1.0)
                                .vertical_scroll(vscroll.clone()),
                        )
                        .build(ctx, |ctx| {
                            for n in 0..30 {
                                ctx.key(("v", n), |ctx| {
                                    Text::new(format!("Item {n:02} — scrollable content row"))
                                        .font_size(14.0)
                                        .modifier(
                                            Modifier::new()
                                                .fill_max_width()
                                                .padding_vertical(6.0),
                                        )
                                        .build(ctx);
                                });
                            }
                        });
                    VerticalScrollbar::new(vscroll.clone())
                        .always_show(always.get())
                        .build(ctx);
                });

            // 常显开关
            Row::new()
                .spacing(8.0)
                .build(ctx, |ctx| {
                    let a = always.clone();
                    Button::new()
                        .on_click(move || a.update(|v| *v = !*v))
                        .build(ctx, |ctx| {
                            Text::new(if always.get() {
                                "always_show: ON"
                            } else {
                                "always_show: OFF"
                            })
                            .font_size(13.0)
                            .build(ctx);
                        });
                });

            Text::new("■ Horizontal Scrollbar")
                .font_size(16.0)
                .build(ctx);

            Column::new()
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
                    Row::new()
                        .modifier(
                            Modifier::new()
                                .fill_max_width()
                                .horizontal_scroll(hscroll.clone()),
                        )
                        .build(ctx, |ctx| {
                            for n in 0..20 {
                                ctx.key(("h", n), |ctx| {
                                    Stack::new()
                                        .modifier(
                                            Modifier::new()
                                                .size(80.0, 60.0)
                                                .background(
                                                    Color::from_argb(255, 80, 160, 100),
                                                    Shape::rounded(8.0),
                                                ),
                                        )
                                        .build(ctx, |ctx| {
                                            Text::new(format!("{n}"))
                                                .font_size(14.0)
                                                .build(ctx);
                                        });
                                });
                            }
                        });
                    HorizontalScrollbar::new(hscroll.clone())
                        .always_show(true)
                        .build(ctx);
                });

            Text::new("■ Lazy Scrollbar (LazyColumn + LazyScrollbar)")
                .font_size(16.0)
                .build(ctx);

            Row::new()
                .modifier(
                    Modifier::new()
                        .fill_max_width()
                        .height(260.0)
                        .background(
                            Color::from_argb(20, 150, 50, 150),
                            Shape::rounded(8.0),
                        )
                        .padding(8.0),
                )
                .build(ctx, |ctx| {
                    LazyColumn::new()
                        .state(lazy_state.clone())
                        .modifier(
                            Modifier::new()
                                .fill_max_height()
                                .layout_weight(1.0),
                        )
                        .items_plain(50, |ctx, n| {
                            Text::new(format!("Lazy {n:02} — virtualized row"))
                                .font_size(14.0)
                                .modifier(
                                    Modifier::new()
                                        .fill_max_width()
                                        .padding_vertical(6.0),
                                )
                                .build(ctx);
                        })
                        .build(ctx);
                    LazyScrollbar::new(lazy_state.clone())
                        .always_show(always.get())
                        .build(ctx);
                });
        });
}

fn main() {
    // tokio 运行时（供 Scrollbar fade 的 LaunchedEffect 使用）
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(520.0, 720.0)
                .title("Scrollbar Demo")
                .build(ctx, |ctx| {
                    scrollbar_demo_ui(ctx);
                });
        });
    });
}
