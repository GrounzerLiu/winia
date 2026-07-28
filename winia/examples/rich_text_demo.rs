//! RichText 组件演示 — 富文本嵌入图片、SVG 内联元素
//!
//! 展示 RichText 组件的内联图文混排功能，
//! 包括不同文字样式、SVG 图标、内联图片的混合排列。

use winia::prelude::*;
use winia::app;

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();

    app::run_app(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(520.0, 680.0)
                .title("RichText Demo")
                .build(ctx, |ctx| {
                    rich_text_demo_ui(ctx);
                });
        });
    });
}

fn checkmark_svg() -> SvgDrawable {
    SvgDrawable::from_str(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
            <circle cx="12" cy="12" r="10" fill="#4CAF50"/>
            <path d="M8 12l3 3 5-5" stroke="white" stroke-width="2" fill="none"/>
        </svg>"##,
        18.0, 18.0,
    ).expect("checkmark SVG")
}

fn cross_svg() -> SvgDrawable {
    SvgDrawable::from_str(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
            <circle cx="12" cy="12" r="10" fill="#F44336"/>
            <path d="M9 9l6 6M15 9l-6 6" stroke="white" stroke-width="2" fill="none"/>
        </svg>"##,
        18.0, 18.0,
    ).expect("cross SVG")
}

fn star_svg() -> SvgDrawable {
    SvgDrawable::from_str(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
            <path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z" fill="#FFC107"/>
        </svg>"##,
        20.0, 20.0,
    ).expect("star SVG")
}

fn info_svg() -> SvgDrawable {
    SvgDrawable::from_str(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
            <circle cx="12" cy="12" r="10" fill="#2196F3"/>
            <path d="M12 16v-4M12 8h.01" stroke="white" stroke-width="2" fill="none" stroke-linecap="round"/>
        </svg>"##,
        16.0, 16.0,
    ).expect("info SVG")
}

fn decoration_svg() -> SvgDrawable {
    SvgDrawable::from_str(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48">
            <defs>
                <linearGradient id="g" x1="0%" y1="0%" x2="100%" y2="100%">
                    <stop offset="0%" stop-color="#FF6B6B"/>
                    <stop offset="50%" stop-color="#4ECDC4"/>
                    <stop offset="100%" stop-color="#45B7D1"/>
                </linearGradient>
            </defs>
            <circle cx="24" cy="24" r="22" fill="url(#g)"/>
            <text x="24" y="30" text-anchor="middle" fill="white" font-size="20">✦</text>
        </svg>"##,
        32.0, 32.0,
    ).expect("decoration SVG")
}

fn rich_text_demo_ui(ctx: &mut ComposeCtx) {
    let scroll_state = ctx.remember(|| ScrollState::new()).get();

    Column::new()
        .modifier(Modifier::new()
            .size(Dimension::Fill, 680.0)
            .padding(16.0)
            .vertical_scroll(scroll_state))
        .spacing(12.0)
        .build(ctx, |ctx| {

            // ═══ 1. 基础 RichText — with_style 切换样式 ═══
            Text::new("■ Basic — styled text spans")
                .font_size(16.0).color(Color::from_argb(255, 100, 100, 100))
                .build(ctx);

            RichText::new()
                .modifier(Modifier::new()
                    .fill_max_width()
                    .padding(10.0)
                    .background(Color::from_argb(18, 0, 0, 0), Shape::rounded(6.0)))
                .text("Normal. ")
                .with_style(|s| s.bold().font_size(18.0)).text("Bold large. ")
                .with_style(|s| s.italic()).text("Italic. ")
                .with_style(|s| s.color(Color::RED).bold()).text("Red bold!")
                .build(ctx);

            // ═══ 2. 行内 SVG 图标 ═══
            Text::new("■ Inline SVG icons in text")
                .font_size(16.0).color(Color::from_argb(255, 100, 100, 100))
                .build(ctx);

            RichText::new()
                .modifier(Modifier::new()
                    .fill_max_width()
                    .padding(10.0)
                    .background(Color::from_argb(18, 0, 0, 0), Shape::rounded(6.0)))
                .text("Task 1: ")
                .with_style(|s| s.bold()).text("Completed")
                .image(checkmark_svg())
                .build(ctx);

            RichText::new()
                .modifier(Modifier::new()
                    .fill_max_width()
                    .padding(10.0)
                    .background(Color::from_argb(18, 0, 0, 0), Shape::rounded(6.0)))
                .text("Task 2: ")
                .with_style(|s| s.bold()).text("Failed")
                .image(cross_svg())
                .build(ctx);

            RichText::new()
                .modifier(Modifier::new()
                    .fill_max_width()
                    .padding(10.0)
                    .background(Color::from_argb(18, 0, 0, 0), Shape::rounded(6.0)))
                .text("Rating: ")
                .image(star_svg()).text(" ")
                .image(star_svg()).text(" ")
                .image(star_svg()).text(" ")
                .image(star_svg()).text(" 4/5")
                .build(ctx);

            // ═══ 3. 混合图文 — 段落中嵌入图标 ═══
            Text::new("■ Mixed — icons within paragraph text")
                .font_size(16.0).color(Color::from_argb(255, 100, 100, 100))
                .build(ctx);

            RichText::new()
                .modifier(Modifier::new()
                    .fill_max_width()
                    .padding(10.0)
                    .background(Color::from_argb(18, 0, 0, 0), Shape::rounded(6.0)))
                .text("This is a ")
                .with_style(|s| s.bold().color(Color::from_argb(255, 33, 150, 243))).text("rich text")
                .text(" paragraph with ")
                .image(info_svg())
                .text(" inline icons, ")
                .with_style(|s| s.italic()).text("different styles")
                .text(", and even ")
                .image(star_svg())
                .text(" emoji-like elements.")
                .build(ctx);

            // ═══ 4. 装饰属性：下划线 / 删除线 / 背景色 ═══
            Text::new("■ Decorations — underline, strikethrough, background")
                .font_size(16.0).color(Color::from_argb(255, 100, 100, 100))
                .build(ctx);

            RichText::new()
                .modifier(Modifier::new()
                    .fill_max_width()
                    .padding(10.0)
                    .background(Color::from_argb(18, 0, 0, 0), Shape::rounded(6.0)))
                .with_style(|s| s.underline().font_size(14.0)).text("Underlined text. ")
                .with_style(|s| s.strikethrough().font_size(14.0)).text("Strikethrough text. ")
                .with_style(|s| s.background(Color::from_argb(60, 255, 255, 0)).font_size(14.0)).text("Highlighted text. ")
                .with_style(|s| s.underline().strikethrough().color(Color::RED)).text("Both!")
                .build(ctx);

            // ═══ 5. 大尺寸内联元素 ═══
            Text::new("■ Large inline elements")
                .font_size(16.0).color(Color::from_argb(255, 100, 100, 100))
                .build(ctx);

            RichText::new()
                .modifier(Modifier::new()
                    .fill_max_width()
                    .padding(10.0)
                    .background(Color::from_argb(18, 0, 0, 0), Shape::rounded(6.0)))
                .text("Leading text — ")
                .image(decoration_svg())
                .text(" — trailing text with a larger inline decoration.")
                .build(ctx);

            // ═══ 6. 复杂排版 — 多行混合 ═══
            Text::new("■ Multi-line rich text with wrapping")
                .font_size(16.0).color(Color::from_argb(255, 100, 100, 100))
                .build(ctx);

            RichText::new()
                .modifier(Modifier::new()
                    .fill_max_width()
                    .padding(10.0)
                    .background(Color::from_argb(18, 0, 0, 0), Shape::rounded(6.0)))
                .text("Welcome to ")
                .with_style(|s| s.bold().color(Color::from_argb(255, 76, 175, 80))).text("Winia")
                .text(" — a declarative cross-platform GUI framework ")
                .image(checkmark_svg())
                .text(" inspired by ")
                .with_style(|s| s.italic().color(Color::from_argb(255, 33, 150, 243))).text("Jetpack Compose")
                .text(" and built with ")
                .with_style(|s| s.bold()).text("Skia")
                .text(" + ")
                .with_style(|s| s.bold()).text("Winit")
                .text(". Rich text support enables ")
                .image(star_svg())
                .text(" inline elements")
                .text(" like icons, images, and SVG graphics ")
                .image(info_svg())
                .text(" seamlessly mixed with ")
                .with_style(|s| s.italic()).text("styled text")
                .text(".")
                .build(ctx);

            // ═══ 7. 对齐测试 ═══
            Text::new("■ Center alignment")
                .font_size(16.0).color(Color::from_argb(255, 100, 100, 100))
                .build(ctx);

            ProvideTextStyle(
                TextStyle::new().align(TextAlign::Center),
                ctx,
                |ctx| {
                    RichText::new()
                        .modifier(Modifier::new()
                            .fill_max_width()
                            .padding(12.0)
                            .background(Color::from_argb(18, 0, 0, 0), Shape::rounded(6.0)))
                        .text("Centered text with ")
                        .image(star_svg())
                        .text(" inline icon")
                        .build(ctx);
                },
            );

            // ═══ 8. 综合效果 — 类似通知卡片 ═══
            Text::new("■ Notification card style")
                .font_size(16.0).color(Color::from_argb(255, 100, 100, 100))
                .build(ctx);

            Stack::new()
                .modifier(Modifier::new()
                    .fill_max_width()
                    .background(Color::from_argb(240, 230, 240, 255), Shape::rounded(8.0)))
                .build(ctx, |ctx| {
                    Text::new("")
                        .modifier(Modifier::new()
                            .size(4.0, Dimension::Fill)
                            .background(Color::from_argb(255, 33, 150, 243), Shape::rounded(2.0)))
                        .build(ctx);
                    RichText::new()
                        .modifier(Modifier::new()
                            .fill_max_size()
                            .padding(14.0))
                        .image(info_svg())
                        .text("  Update available")
                        .build(ctx);
                    Text::new("Version 2.5.0 is ready to install. Click to update.")
                        .font_size(12.0)
                        .color(Color::from_argb(255, 100, 100, 100))
                        .modifier(Modifier::new()
                            .fill_max_width()
                            .padding(14.0)
                            .offset(22.0, 20.0))
                        .build(ctx);
                });
        });
}
