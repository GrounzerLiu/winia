//! Winia Counter + Scroll 示例

use winia::prelude::*;
use winia::app;

fn counter_ui(ctx: &mut ComposeCtx) {
    let count = ctx.remember(|| 0i32);
    let show_alt = ctx.remember(|| false);
    let show_window = ctx.remember(|| false);
    let scroll_y = ctx.remember(|| ScrollState::new()).get();
    let btn1 = ctx.remember(|| FocusRequester::new()).get();
    let btn2 = ctx.remember(|| FocusRequester::new()).get();
    let b1c = btn1.clone(); let _b2c = btn2.clone();

    Column::new()
        .modifier(Modifier::new().padding(16.0))
        .build(ctx, |ctx| {
            Text::new(format!("Count: {}", count.get()))
                .font_size(24.0)
                .build(ctx);

            // Filled 按钮：自动使用 primary 背景 + on_primary 文字
            Button::new().on_click({ let c = count.clone(); let b = btn2.clone(); move || { c.update(|v| *v += 1); b.request_focus(); } })
                .modifier(Modifier::new().size(200.0, 36.0).focusable().focus_requester(&btn1))
                .build(ctx, |ctx| { Text::new("+1 focus btn2").font_size(12.0).build(ctx); });

            // Outlined 按钮：自动使用 outline 边框
            Button::new().on_click(move || { b1c.request_focus(); })
                .style(ButtonStyle::Outlined)
                .modifier(Modifier::new().size(200.0, 36.0).focusable().focus_requester(&btn2))
                .build(ctx, |ctx| { Text::new("focus btn1").font_size(12.0).build(ctx); });

            // ── if/else 条件渲染 ──
            Button::new().on_click({ let s = show_alt.clone(); move || { s.update(|v| *v = !*v); } })
                .modifier(Modifier::new().size(200.0, 32.0).background(
                    if show_alt.get() { Color::BLUE } else { Color::from_argb(255, 0, 150, 0) },
                    Shape::rounded(4.0),
                ))
                .build(ctx, |ctx| {
                    Text::new(if show_alt.get() { "Show Normal" } else { "Show Alt" })
                        .color(Color::WHITE).font_size(12.0).build(ctx);
                });

            if show_alt.get() {
                Text::new(format!("✳ Alternative! count={}", count.get()))
                    .font_size(18.0)
                    .modifier(Modifier::new()
                        .size(200.0, 60.0)
                        .background(Color::from_argb(255, 255, 240, 200), Shape::rounded(6.0))
                        .padding(8.0))
                    .build(ctx);
            } else {
                let c = count.clone();
                Button::new().on_click(move || { c.update(|v| *v += 10); })
                    .modifier(Modifier::new()
                        .size(200.0, 60.0)
                        .background(Color::from_argb(255, 200, 220, 255), Shape::rounded(6.0)))
                    .build(ctx, |ctx| {
                        Text::new(format!("Add 10 (now {})", count.get()))
                            .font_size(14.0).build(ctx);
                    });
            }

            // ── 声明式多窗口按钮 ──
            Button::new().on_click({ let s = show_window.clone(); move || { s.update(|v| *v = !*v); } })
                .modifier(Modifier::new().size(200.0, 32.0).background(Color::from_argb(255, 180, 100, 200), Shape::rounded(4.0)))
                .build(ctx, |ctx| {
                    Text::new(if show_window.get() { "Close sub window" } else { "Open sub window" })
                        .color(Color::WHITE).font_size(12.0).build(ctx);
                });

            // ── Window 子窗口（if 条件控制生命周期）──
            if show_window.get() {
                Window::new()
                    .size(250.0, 180.0)
                    .title("Sub Window")
                    .on_close({ let s = show_window.clone(); move || { s.update(|v| *v = false); } })
                    .build(ctx, |ctx| {
                        let sw_count = ctx.remember(|| 0i32);
                        Text::new(format!("Sub count: {}", sw_count.get()))
                            .font_size(18.0)
                            .modifier(Modifier::new().padding(8.0))
                            .build(ctx);
                        Button::new().on_click({ let c = sw_count.clone(); move || { c.update(|v| *v += 1); } })
                            .modifier(Modifier::new().size(120.0, 36.0).background(Color::BLUE, Shape::rounded(4.0)))
                            .build(ctx, |ctx| { Text::new("Inc").color(Color::WHITE).font_size(14.0).build(ctx); });
                    });
            }

            // 滚动区域
            Column::new()
                .modifier(Modifier::new().size(200.0, 150.0).vertical_scroll(scroll_y))
                .build(ctx, |ctx| {
                    for i in 0..30 {
                        let color = if i % 2 == 0 { Color::from_argb(255, 240, 240, 240) } else { Color::WHITE };
                        Row::new().modifier(Modifier::new().size(200.0, 24.0).background(color, Shape::Rectangle))
                            .build(ctx, |ctx| {
                                Text::new(format!("Line {}", i)).font_size(14.0).build(ctx);
                            });
                    }
                });
        });
}

fn main() {
    app::run_app(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(400.0, 500.0)
                .title("Winia Counter + Scroll")
                .build(ctx, |ctx| {
                    counter_ui(ctx);
                });
        });
    });
}
