//! Loading Indicator 演示（M3 Expressive 对齐——Compose LoadingIndicator / ContainedLoadingIndicator）
//!
//! 展示：
//! - Uncontained（默认，无容器，Primary indicator）
//! - Contained（圆形容器，PrimaryContainer + OnPrimaryContainer indicator）
//! - 自定义颜色 / 容器形状 / 缩放尺寸（24–96dp）
//! - 与旧版一致的 7-shape Morph 动画

use winia::prelude::*;

fn section_title(ctx: &mut ComposeCtx, text: &str) {
    Text::new(text)
        .font_size(14.0)
        .color(Color::from_argb(255, 90, 90, 90))
        .modifier(Modifier::new().padding_top(14.0).padding_bottom(6.0))
        .build(ctx);
}

#[composable]
fn loading_indicator_demo(ctx: &mut ComposeCtx) {
    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .build(ctx, |ctx| {
            Text::new("Loading Indicator 演示（M3 Expressive）")
                .font_size(20.0)
                .build(ctx);

            section_title(ctx, "Uncontained（默认）");
            Row::new().spacing(24.0).build(ctx, |ctx| {
                LoadingIndicator::new().build(ctx);
                LoadingIndicator::new()
                    .indicator_color(Color::from_argb(255, 46, 125, 50))
                    .build(ctx);
            });

            section_title(ctx, "Container（SecondaryContainer）");
            Row::new().spacing(24.0).build(ctx, |ctx| {
                LoadingIndicator::new().contained(true).build(ctx);
                LoadingIndicator::new()
                    .contained(true)
                    .indicator_color(Color::from_argb(255, 255, 255, 255))
                    .container_color(Color::from_argb(255, 46, 125, 50))
                    .build(ctx);
            });

            section_title(ctx, "Container 自定义形状（RoundedRect）");
            Row::new().spacing(24.0).build(ctx, |ctx| {
                LoadingIndicator::new()
                    .contained(true)
                    .container_shape(Shape::rounded(12.0))
                    .build(ctx);
                LoadingIndicator::new()
                    .contained(true)
                    .container_shape(Shape::Pill)
                    .build(ctx);
            });

            section_title(ctx, "缩放尺寸（24 / 48 / 96dp）");
            Row::new().spacing(24.0).build(ctx, |ctx| {
                LoadingIndicator::new()
                    .contained(true)
                    .modifier(Modifier::new().size(24.0, 24.0))
                    .build(ctx);
                LoadingIndicator::new().build(ctx);
                LoadingIndicator::new()
                    .contained(true)
                    .modifier(Modifier::new().size(96.0, 96.0))
                    .build(ctx);
            });
        });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::light(ctx, |ctx| {
            Window::new()
                .size(420.0, 620.0)
                .title("Loading Indicator Demo")
                .build(ctx, loading_indicator_demo);
        });
    });
}
