//! Winia Counter — 验证焦点系统

use winia::prelude::*;
use winia::app;

fn counter_ui(ctx: &mut ComposeCtx) {
    let count = ctx.remember(|| 0i32);
    let btn1 = ctx.remember(|| FocusRequester::new()).get();
    let btn2 = ctx.remember(|| FocusRequester::new()).get();
    let btn3 = ctx.remember(|| FocusRequester::new()).get();

    // clone 供 on_click 回调使用（FocusRequester 需 move 进闭包）
    let b1c = btn1.clone(); let b2c = btn2.clone(); let b3c = btn3.clone();

    Column::new()
        .modifier(Modifier::new().padding(16.0))
        .build(ctx, |ctx| {
            Text::new(format!("Count: {}", count.get())).font_size(24.0).build(ctx);

            let c = count.clone();
            Button::new().on_click(move || { c.update(|v| *v += 1); b2c.request_focus(); })
                .modifier(Modifier::new().size(200.0, 40.0).background(Color::BLUE, Shape::rounded(4.0))
                    .focusable().focus_requester(&btn1))  // & 引用，不消耗所有权
                .build(ctx, |ctx| { Text::new("+1 → focus btn2").color(Color::WHITE).font_size(14.0).build(ctx); });

            let c = count.clone();
            Button::new().on_click(move || { c.update(|v| *v += 1); b3c.request_focus(); })
                .modifier(Modifier::new().size(200.0, 40.0).background(Color::from_argb(255, 0, 150, 0), Shape::rounded(4.0))
                    .focusable().focus_requester(&btn2))
                .build(ctx, |ctx| { Text::new("+1 → focus btn3").color(Color::WHITE).font_size(14.0).build(ctx); });

            let c = count.clone();
            Button::new().on_click(move || { c.update(|v| *v += 1); b1c.request_focus(); })
                .modifier(Modifier::new().size(200.0, 40.0).background(Color::from_argb(255, 150, 80, 0), Shape::rounded(4.0))
                    .focusable().focus_requester(&btn3))
                .build(ctx, |ctx| { Text::new("+1 → focus btn1").color(Color::WHITE).font_size(14.0).build(ctx); });
        });
}

fn main() { app::run_app(counter_ui, 400.0, 300.0); }
