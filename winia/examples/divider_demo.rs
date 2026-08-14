//! Divider 演示（material3 对齐——Compose material3 Divider.kt）
//!
//! 展示：
//! - HorizontalDivider（默认 1dp OutlineVariant）
//! - VerticalDivider（行内垂直分隔）
//! - 自定义颜色/厚度（含 Hairline 单像素）
//! - M3 尺寸变体：full-width / inset / middle-inset（modifier padding 实现）

use winia::prelude::*;

fn section_title(ctx: &mut ComposeCtx, text: &str) {
    Text::new(text)
        .font_size(14.0)
        .color(Color::from_argb(255, 90, 90, 90))
        .modifier(Modifier::new().padding_top(14.0).padding_bottom(6.0))
        .build(ctx);
}

fn list_item(ctx: &mut ComposeCtx, text: &str) {
    Text::new(text)
        .font_size(14.0)
        .modifier(Modifier::new().padding(8.0))
        .build(ctx);
}

#[composable]
fn divider_demo(ctx: &mut ComposeCtx) {
    let scroll_y = ctx.remember(|| ScrollState::new()).get();

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0).vertical_scroll(scroll_y))
        .build(ctx, |ctx| {
            Text::new("Divider 演示（material3 对齐）")
                .font_size(20.0)
                .build(ctx);

            section_title(ctx, "列表分隔（HorizontalDivider 默认 1dp）");
            list_item(ctx, "收件箱");
            Divider::horizontal().build(ctx);
            list_item(ctx, "星标");
            Divider::horizontal().build(ctx);
            list_item(ctx, "草稿");

            section_title(ctx, "M3 尺寸变体（modifier 实现）");
            list_item(ctx, "Full-width divider");
            Divider::horizontal().build(ctx);

            list_item(ctx, "Inset divider（左 16dp）");
            Divider::horizontal()
                .modifier(Modifier::new().padding_start(16.0))
                .build(ctx);

            list_item(ctx, "Middle-inset divider（左右各 16dp）");
            Divider::horizontal()
                .modifier(Modifier::new().padding_horizontal(16.0))
                .build(ctx);

            section_title(ctx, "自定义颜色与厚度");
            list_item(ctx, "2dp + 绿色");
            Divider::horizontal()
                .thickness(2.0)
                .color(Color::from_argb(255, 46, 125, 50))
                .build(ctx);

            list_item(ctx, "Hairline（1 物理像素）");
            Divider::horizontal()
                .thickness(DIVIDER_HAIRLINE)
                .color(Color::from_argb(255, 180, 60, 60))
                .build(ctx);

            section_title(ctx, "VerticalDivider（行内）");
            Row::new()
                .spacing(8.0)
                .build(ctx, |ctx| {
                    Text::new("左")
                .font_size(14.0)
                        .modifier(Modifier::new().height(20.0))
                        .build(ctx);
                    // 垂直分隔线：Row 无固定高度（内容驱动）时 fill_max_height 无界约束不可用——
                    // 显式高度与兄弟文本对齐（Compose 同用法）
                    Divider::vertical()
                        .modifier(Modifier::new().height(20.0))
                        .build(ctx);
                    Text::new("中")
                .font_size(14.0)
                        .modifier(Modifier::new().height(20.0))
                        .build(ctx);
                    Divider::vertical()
                        .thickness(2.0)
                        .color(Color::from_argb(255, 46, 125, 50))
                        .modifier(Modifier::new().height(20.0))
                        .build(ctx);
                    Text::new("右")
                .font_size(14.0)
                        .modifier(Modifier::new().height(20.0))
                        .build(ctx);
                });

            Text::new("")
                .modifier(Modifier::new().height(40.0))
                .build(ctx);
        });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();

    winia::run_app!(|ctx| {
        WiniaTheme::light(ctx, |ctx| {
            Window::new()
                .size(420.0, 560.0)
                .title("Divider Demo")
                .build(ctx, divider_demo);
        });
    });
}
