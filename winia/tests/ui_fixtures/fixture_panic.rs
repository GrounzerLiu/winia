//! UI 测试 fixture：panic recovery——build 内 panic 被捕获后窗口存活。
//!
//! 验证：RedrawRequested 的 catch_unwind 捕获组合期 panic（本帧跳过），
//! 状态复位后下一帧正常渲染——窗口不崩、后续交互可用。

use winia::prelude::*;

#[composable]
fn panic_fixture(ctx: &mut ComposeCtx) {
    let count = ctx.remember(|| 0i32);
    let panic_flag = ctx.remember(|| false);

    // build 内触发一次性 panic（panic 前复位——下帧恢复）
    if panic_flag.get() {
        panic_flag.set(false);
        panic!("fixture intentional panic");
    }

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .spacing(8.0)
        .build(ctx, |ctx| {
            Text::new("Panic recovery fixture").font_size(20.0).build(ctx);
            let p = panic_flag.clone();
            Button::text()
                .on_click(move || p.set(true))
                .modifier(Modifier::new().test_tag("panic-btn"))
                .build(ctx, |ctx| Text::new("Panic").build(ctx));
            let c = count.clone();
            Button::text()
                .on_click(move || c.update(|v| *v += 1))
                .modifier(Modifier::new().test_tag("count-btn"))
                .build(ctx, |ctx| Text::new("Count").build(ctx));
            Text::new(format!("count: {}", count.get())).build(ctx);
        });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(400.0, 300.0)
                .title("Panic Fixture")
                .build(ctx, |ctx| panic_fixture(ctx));
        });
    });
}
