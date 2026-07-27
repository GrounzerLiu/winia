//! StreamObverse 演示 — watch channel → stream → State → recompose

use winia::prelude::*;
use winia::app;
use std::time::Duration;

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();

    app::run_app(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(320.0, 320.0)
                .title("Stream Observer Demo")
                .build(ctx, |ctx| stream_demo_ui(ctx));
        });
    });
}

fn stream_demo_ui(ctx: &mut ComposeCtx) {
    // 创建 watch channel → 转成 Stream
    let (tx, rx) = tokio::sync::watch::channel(0i32);
    let rx_stream = tokio_stream::wrappers::WatchStream::new(rx);

    // Stream 自动转 State，离开组合时取消消费
    let count = rx_stream.observe(ctx, 0);

    // 后台模拟定时推送
    let tx2 = tx.clone();
    let scope = winia::effect::remember_coroutine_scope(ctx);
    scope.spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let v = *tx2.borrow() + 1;
            if v > 20 { break; }
            let _ = tx2.send(v);
        }
    });

    Column::new()
        .modifier(Modifier::new().padding(24.0))
        .spacing(12.0)
        .build(ctx, |ctx| {

            Text::new("📡 Stream Observer")
                .font_size(22.0)
                .build(ctx);

            Text::new("watch channel → stream → observe() → State")
                .font_size(12.0)
                .build(ctx);

            Text::new(format!("Auto count: {}", count.get()))
                .font_size(36.0)
                .modifier(Modifier::new()
                    .size(200.0, 48.0)
                    .background(Color::from_argb(255, 230, 240, 255), Shape::rounded(8.0)))
                .build(ctx);

            // 手动重置
            Button::new()
                .on_click(move || { let _ = tx.send(0); })
                .modifier(Modifier::new().size(120.0, 36.0))
                .build(ctx, |ctx| { Text::new("Reset").build(ctx); });

            if count.get() >= 10 {
                Text::new("🎉 Stream reached 10!")
                    .font_size(16.0)
                    .color(Color::RED)
                    .build(ctx);
            }
        });
}
