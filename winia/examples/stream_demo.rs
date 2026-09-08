//! observe_watch 演示 — watch channel → State → recompose

use letclone::clone;
use winia::prelude::*;
use winia::app;
use std::time::Duration;

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(360.0, 340.0)
                .title("Stream Observer Demo")
                .build(ctx, |ctx| stream_demo_ui(ctx));
        });
    });
}

#[composable]
fn stream_demo_ui(ctx: &mut ComposeCtx) {
    let theme = WiniaTheme::colors();

    // watch channel — remember 防止重组时重建
    let ch = ctx.remember(|| tokio::sync::watch::channel(0i32));
    let (tx, rx) = ch.get();
    let count = winia::effect::observe_watch(ctx, rx, 0);

    // 后台每秒自增（LaunchedEffect 自动管理生命周期）
    LaunchedEffect::new(()).build(ctx, {
        clone!(tx);
        move |_| async move {
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                tx.send_modify(|v| { if *v < 20 { *v += 1; } });
            }
        }
    });

    Column::new()
        .modifier(Modifier::new().padding(24.0))
        .spacing(12.0)
        .build(ctx, |ctx| {

            Text::new("Stream Observer")
                .font_size(20.0)
                .build(ctx);

            Text::new("watch channel → observe() → State")
                .font_size(12.0)
                .build(ctx);

            // 数值 display
            Text::new(format!("{}", count.get()))
                .font_size(40.0)
                .color(theme.on_primary)
                .modifier(Modifier::new()
                    .size(260.0, 60.0)
                    .background(theme.primary, Shape::rounded(10.0)))
                .build(ctx);

            if count.get() >= 10 {
                Text::new("Reached 10!")
                    .font_size(16.0)
                    .color(theme.primary)
                    .build(ctx);
            }

            // 复位
            Button::new()
                .on_click({ clone!(tx); move || { let _ = tx.send(0); } })
                .modifier(Modifier::new().size(120.0, 36.0))
                .build(ctx, |ctx| {
                    Text::new("Reset").color(theme.on_primary).build(ctx);
                });
        });
}
