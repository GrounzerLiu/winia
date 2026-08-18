//! Material 3 ListItem 演示：一行、二行、三行、slots 与交互。

use winia::prelude::*;

fn label(ctx: &mut ComposeCtx, value: &str) {
    Text::new(value)
        .font_size(12.0)
        .color(Color::from_argb(255, 95, 99, 104))
        .modifier(Modifier::new().padding_top(12.0).padding_bottom(4.0))
        .build(ctx);
}

#[composable]
fn list_item_demo(ctx: &mut ComposeCtx) {
    let clicks = ctx.remember(|| 0i32);
    let click_count = clicks.clone();

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0).vertical_scroll(ctx.remember(|| ScrollState::new()).get()))
        .build(ctx, |ctx| {
            Text::new("ListItem (Material 3)").font_size(22.0).build(ctx);

            label(ctx, "一行");
            ListItem::new(|ctx| Text::new("账户设置").build(ctx))
                .leading_content(|ctx| Text::new("⚙").font_size(20.0).build(ctx))
                .trailing_content(|ctx| Text::new("›").font_size(22.0).build(ctx))
                .on_click({ let c = click_count.clone(); move || c.update(|v| *v += 1) })
                .build(ctx);

            label(ctx, "二行");
            ListItem::new(|ctx| Text::new("通知").build(ctx))
                .supporting_content(|ctx| Text::new("接收重要更新和提醒").build(ctx))
                .leading_content(|ctx| Text::new("●").font_size(20.0).build(ctx))
                .build(ctx);

            label(ctx, "三行");
            ListItem::new(|ctx| Text::new("离线内容").build(ctx))
                .overline_content(|ctx| Text::new("下载完成").build(ctx))
                .supporting_content(|ctx| Text::new("内容可在没有网络时继续访问").build(ctx))
                .leading_content(|ctx| Text::new("↓").font_size(22.0).build(ctx))
                .trailing_content(|ctx| Text::new("•••").font_size(16.0).build(ctx))
                .build(ctx);

            label(ctx, "状态");
            ListItem::new(|ctx| Text::new("已禁用项目").build(ctx))
                .supporting_content(|ctx| Text::new("当前不可用").build(ctx))
                .enabled(false)
                .build(ctx);

            Text::new(format!("点击次数: {}", clicks.get()))
                .font_size(12.0)
                .color(Color::from_argb(255, 95, 99, 104))
                .modifier(Modifier::new().padding_top(12.0))
                .build(ctx);
        });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        WiniaTheme::light(ctx, |ctx| {
            Window::new().size(460.0, 720.0).title("ListItem Demo").build(ctx, list_item_demo);
        });
    });
}
