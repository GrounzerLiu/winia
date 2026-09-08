//! Badge / BadgedBox 演示（material3 对齐）
//!
//! 展示：
//! - 小徽章（无内容圆点）与大徽章（数字/短文本）
//! - BadgedBox 组合（图标右上角挂徽章——导航/消息场景）
//! - 数字徽章 "99+" 最大字符形态
//! - 自定义颜色

use winia::prelude::*;

fn section_title(ctx: &mut ComposeCtx, text: &str) {
    Text::new(text)
        .font_size(14.0)
        .color(Color::from_argb(255, 90, 90, 90))
        .modifier(Modifier::new().padding_top(14.0).padding_bottom(6.0))
        .build(ctx);
}

#[composable]
fn badge_demo(ctx: &mut ComposeCtx) {
    let scroll_y = ctx.remember(|| ScrollState::new()).get();

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0).vertical_scroll(scroll_y))
        .build(ctx, |ctx| {
            Text::new("Badge 演示（material3 对齐）")
                .font_size(20.0)
                .build(ctx);

            section_title(ctx, "BadgedBox 组合（小徽章 / 数字 / 最大字符）");
            Row::new()
                .modifier(Modifier::new().padding_vertical(3.0))
                .spacing(24.0)
                .build(ctx, |ctx| {
                    // 小圆点（无内容）
                    BadgedBox::new(|ctx| { Badge::new().build(ctx); })
                        .build(ctx, |ctx| { Text::new("📧").font_size(24.0).build(ctx); });
                    // 数字徽章
                    BadgedBox::new(|ctx| {
                        Badge::new().content(|ctx| { Text::new("3").build(ctx); }).build(ctx);
                    }).build(ctx, |ctx| { Text::new("🔔").font_size(24.0).build(ctx); });
                    // 最大字符（99+）
                    BadgedBox::new(|ctx| {
                        Badge::new().content(|ctx| { Text::new("99+").build(ctx); }).build(ctx);
                    }).build(ctx, |ctx| { Text::new("💬").font_size(24.0).build(ctx); });
                });

            section_title(ctx, "纯 Badge（状态行）");
            Row::new()
                .modifier(Modifier::new().padding_vertical(3.0))
                .build(ctx, |ctx| {
                    Text::new("小圆点")
                        .font_size(13.0)
                        .modifier(Modifier::new().width(150.0).padding_top(3.0))
                        .build(ctx);
                    Badge::new().build(ctx);
                });
            Row::new()
                .modifier(Modifier::new().padding_vertical(3.0))
                .build(ctx, |ctx| {
                    Text::new("数字 5")
                        .font_size(13.0)
                        .modifier(Modifier::new().width(150.0).padding_top(3.0))
                        .build(ctx);
                    Badge::new().content(|ctx| { Text::new("5").build(ctx); }).build(ctx);
                });
            Row::new()
                .modifier(Modifier::new().padding_vertical(3.0))
                .build(ctx, |ctx| {
                    Text::new("最大字符 100+")
                        .font_size(13.0)
                        .modifier(Modifier::new().width(150.0).padding_top(3.0))
                        .build(ctx);
                    Badge::new().content(|ctx| { Text::new("100+").build(ctx); }).build(ctx);
                });

            section_title(ctx, "自定义颜色");
            let theme = WiniaTheme::colors();
            let prim = theme.primary;
            let on_prim = theme.on_primary;
            Row::new()
                .modifier(Modifier::new().padding_vertical(3.0))
                .spacing(24.0)
                .build(ctx, |ctx| {
                    BadgedBox::new(|ctx| {
                        Badge::new()
                            .container_color(Color::from_argb(255, 46, 125, 50))
                            .content_color(Color::WHITE)
                            .content(|ctx| { Text::new("3").build(ctx); })
                            .build(ctx);
                    }).build(ctx, |ctx| { Text::new("📈").font_size(24.0).build(ctx); });
                    BadgedBox::new(move |ctx| {
                        Badge::new()
                            .container_color(prim)
                            .content_color(on_prim)
                            .build(ctx);
                    }).build(ctx, |ctx| { Text::new("⭐").font_size(24.0).build(ctx); });
                });

            section_title(ctx, "自定义 modifier");
            Row::new()
                .modifier(Modifier::new().padding_vertical(3.0))
                .build(ctx, |ctx| {
                    Text::new("偏移徽章")
                        .font_size(13.0)
                        .modifier(Modifier::new().width(150.0).padding_top(3.0))
                        .build(ctx);
                    Badge::new()
                        .content(|ctx| { Text::new("8").build(ctx); })
                        .modifier(Modifier::new().padding_top(8.0))
                        .build(ctx);
                });

            Text::new("")
                .modifier(Modifier::new().height(40.0))
                .build(ctx);
        });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::light(ctx, |ctx| {
            Window::new()
                .size(420.0, 600.0)
                .title("Badge Demo")
                .build(ctx, badge_demo);
        });
    });
}
