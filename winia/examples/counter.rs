//! Winia Counter + Scroll 示例

use winia::prelude::*;
use winia::app;

fn counter_ui(ctx: &mut ComposeCtx) {
    let count = ctx.remember(|| 0i32);
    let scroll_y = ctx.remember(|| ScrollState::new()).get();
    let btn1 = ctx.remember(|| FocusRequester::new()).get();
    let btn2 = ctx.remember(|| FocusRequester::new()).get();
    let b1c = btn1.clone(); let b2c = btn2.clone();

    Column::new()
        .modifier(Modifier::new().padding(16.0))
        .build(ctx, |ctx| {
            Text::new(format!("Count: {}", count.get())).font_size(24.0).build(ctx);

            // 两个按钮演示焦点
            Button::new().on_click({ let c = count.clone(); let b = btn2.clone(); move || { c.update(|v| *v += 1); b.request_focus(); } })
                .modifier(Modifier::new().size(200.0, 36.0).background(Color::BLUE, Shape::rounded(4.0)).focusable().focus_requester(&btn1))
                .build(ctx, |ctx| { Text::new("+1 focus btn2").color(Color::WHITE).font_size(12.0).build(ctx); });

            Button::new().on_click(move || { b1c.request_focus(); })
                .modifier(Modifier::new().size(200.0, 36.0).background(Color::from_argb(255, 0, 150, 0), Shape::rounded(4.0)).focusable().focus_requester(&btn2))
                .build(ctx, |ctx| { Text::new("focus btn1").color(Color::WHITE).font_size(12.0).build(ctx); });

            // 滚动区域：200×150 窗口内显示 30 行文字
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

fn main() { app::run_app(counter_ui, 400.0, 500.0); }
