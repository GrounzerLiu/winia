//! AnimatedContent 演示——targetState 切换 + sizeTransform（对标 Compose）
//!
//! 两个页面（不同尺寸内容）切换：旧内容淡出 → 新内容淡入，容器宽度
//! 从旧内容尺寸平滑动画到新内容尺寸。
//!
//! 用法：`cargo run -p winia --example animated_content_demo`

use letclone::clone;
use winia::animation::{AnimationSpec, SpringSpec, TweenSpec};
use winia::prelude::*;

// Shared example chrome: top app bar with the settings sheet (theme mode + layout direction).
#[path = "common/settings.rs"]
mod settings;

#[composable]
fn animated_content_demo(ctx: &mut ComposeCtx) {
    let page = ctx.remember(|| 0u32);

    Column::new()
        .modifier(Modifier::new().padding(16.0).fill_max_size())
        .build(ctx, |ctx| {
            // No heading of its own: the shared chrome's top app bar carries the title.
            Row::new()
                .modifier(Modifier::new().fill_max_width().padding_vertical(8.0))
                .build(ctx, |ctx| {
                    Button::new()
                        .on_click({ clone!(page); move || { page.set(0); } })
                        .build(ctx, |ctx| { Text::new("Page A (narrow)").build(ctx); });
                    Button::new()
                        .on_click({ clone!(page); move || { page.set(1); } })
                        .build(ctx, |ctx| { Text::new("Page B (wide)").build(ctx); });
                });

            // 内容切换：fade 300ms + 尺寸 Spring
            AnimatedContent::new(page.clone())
                .animation(TweenSpec::default())
                .size_animation(AnimationSpec::Spring(SpringSpec::bouncy()))
                .build(ctx, |ctx, p| {
                    if p == 0 {
                        // 窄内容（200 宽卡片）
                        Column::new()
                            .modifier(Modifier::new()
                                .width(200.0)
                                .padding(16.0)
                                .background(Color::from_argb(255, 66, 133, 244), Shape::rounded(8.0)))
                            .build(ctx, |ctx| {
                                Text::new("Page A")
                                    .font_size(16.0)
                                    .color(Color::from_argb(255, 255, 255, 255))
                                    .build(ctx);
                                Text::new("Narrow content")
                                    .font_size(12.0)
                                    .color(Color::from_argb(220, 255, 255, 255))
                                    .build(ctx);
                            });
                    } else {
                        // 宽内容（360 宽卡片）
                        Column::new()
                            .modifier(Modifier::new()
                                .width(360.0)
                                .padding(16.0)
                                .background(Color::from_argb(255, 76, 175, 80), Shape::rounded(8.0)))
                            .build(ctx, |ctx| {
                                Text::new("Page B")
                                    .font_size(16.0)
                                    .color(Color::from_argb(255, 255, 255, 255))
                                    .build(ctx);
                                Text::new("Wide content with more text")
                                    .font_size(12.0)
                                    .color(Color::from_argb(220, 255, 255, 255))
                                    .build(ctx);
                                Text::new("Container width animates 200 → 360")
                                    .font_size(12.0)
                                    .color(Color::from_argb(200, 255, 255, 255))
                                    .build(ctx);
                            });
                    }
                });

            Text::new("fade 用 Tween；尺寸用 Spring（bouncy）——切换时卡片宽度平滑过渡")
                .font_size(12.0)
                .color(WiniaTheme::colors().on_surface_variant)
                .build(ctx);
        });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                // Taller than the content needs on its own: the shared chrome's top app bar
                // takes its 64 px off the top of the window.
                .size(460.0, 380.0)
                .title("AnimatedContent Demo")
                .build(ctx, |ctx| {
                    settings::shell("AnimatedContent Demo", ctx, |ctx| animated_content_demo(ctx));
                });
        });
    });
}
