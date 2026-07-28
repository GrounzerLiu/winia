//! RichText 组件演示 — Compose 风格嵌套作用域 API

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
                .build(ctx, |ctx| rich_text_demo_ui(ctx));
        });
    });
}

fn checkmark_svg() -> SvgDrawable {
    SvgDrawable::from_str(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
            <circle cx="12" cy="12" r="10" fill="#4CAF50"/>
            <path d="M8 12l3 3 5-5" stroke="white" stroke-width="2" fill="none"/>
        </svg>"##, 18.0, 18.0).expect("checkmark")
}
fn cross_svg() -> SvgDrawable {
    SvgDrawable::from_str(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
            <circle cx="12" cy="12" r="10" fill="#F44336"/>
            <path d="M9 9l6 6M15 9l-6 6" stroke="white" stroke-width="2" fill="none"/>
        </svg>"##, 18.0, 18.0).expect("cross")
}
fn star_svg() -> SvgDrawable {
    SvgDrawable::from_str(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
            <path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z" fill="#FFC107"/>
        </svg>"##, 20.0, 20.0).expect("star")
}
fn info_svg() -> SvgDrawable {
    SvgDrawable::from_str(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
            <circle cx="12" cy="12" r="10" fill="#2196F3"/>
            <path d="M12 16v-4M12 8h.01" stroke="white" stroke-width="2" fill="none" stroke-linecap="round"/>
        </svg>"##, 16.0, 16.0).expect("info")
}
fn decoration_svg() -> SvgDrawable {
    SvgDrawable::from_str(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48">
            <defs><linearGradient id="g" x1="0%" y1="0%" x2="100%" y2="100%">
                <stop offset="0%" stop-color="#FF6B6B"/>
                <stop offset="50%" stop-color="#4ECDC4"/>
                <stop offset="100%" stop-color="#45B7D1"/>
            </linearGradient></defs>
            <circle cx="24" cy="24" r="22" fill="url(#g)"/>
            <text x="24" y="30" text-anchor="middle" fill="white" font-size="20">✦</text>
        </svg>"##, 32.0, 32.0).expect("decoration")
}

fn rich_text_demo_ui(ctx: &mut ComposeCtx) {
    let scroll_state = ctx.remember(|| ScrollState::new()).get();

    Column::new()
        .modifier(Modifier::new().size(Dimension::Fill, 680.0).padding(16.0).vertical_scroll(scroll_state))
        .spacing(12.0)
        .build(ctx, |ctx| {

            // ═══ 1. 基础 ═══
            Text::new("■ Basic — font_size / bold / italic / color scopes")
                .font_size(16.0).color(Color::from_argb(255, 100, 100, 100)).build(ctx);

            do_box(ctx, |ctx| {
                RichText::new().build(ctx, |x| {
                    x.text("Normal ");
                    x.bold(|x| {
                        x.font_size(18.0, |x| { x.text("Bold large "); });
                    });
                    x.italic(|x| { x.text("Italic "); });
                    x.color(Color::RED, |x| { x.bold(|x| { x.text("Red bold!"); }); });
                });
            });

            // ═══ 2. 行内 SVG ═══
            Text::new("■ Inline SVG icons").font_size(16.0).color(Color::from_argb(255, 100, 100, 100)).build(ctx);

            do_box(ctx, |ctx| {
                RichText::new().build(ctx, |x| {
                    x.text("Task 1: ");
                    x.bold(|x| { x.text("Completed "); });
                    x.image(checkmark_svg());
                });
            });
            do_box(ctx, |ctx| {
                RichText::new().build(ctx, |x| {
                    x.text("Rating: ");
                    x.image(star_svg()); x.text(" ");
                    x.image(star_svg()); x.text(" ");
                    x.image(star_svg()); x.text(" ");
                    x.image(star_svg()); x.text(" 4/5");
                });
            });

            // ═══ 3. 混合图文 ═══
            Text::new("■ Mixed — icons within paragraph")
                .font_size(16.0).color(Color::from_argb(255, 100, 100, 100)).build(ctx);

            do_box(ctx, |ctx| {
                RichText::new().build(ctx, |x| {
                    x.text("This is a ");
                    x.color(Color::from_argb(255, 33, 150, 243), |x| {
                        x.bold(|x| { x.text("rich text"); });
                    });
                    x.text(" paragraph with ");
                    x.image(info_svg());
                    x.text(" inline icons and ");
                    x.italic(|x| { x.text("styled text"); });
                    x.text(".");
                });
            });

            // ═══ 4. 装饰属性 ═══
            Text::new("■ Decorations — underline / strikethrough / background")
                .font_size(16.0).color(Color::from_argb(255, 100, 100, 100)).build(ctx);

            do_box(ctx, |ctx| {
                RichText::new().build(ctx, |x| {
                    x.font_size(14.0, |x| {
                        x.underline(|x| { x.text("Underlined text. "); });
                        x.strikethrough(|x| { x.text("Strikethrough text. "); });
                        x.background(Color::from_argb(60, 255, 255, 0), |x| { x.text("Highlighted text. "); });
                        x.underline(|x| { x.strikethrough(|x| { x.color(Color::RED, |x| { x.text("Both!"); }); }); });
                    });
                });
            });

            // ═══ 5. 大尺寸内联 ═══
            Text::new("■ Large inline elements").font_size(16.0).color(Color::from_argb(255, 100, 100, 100)).build(ctx);

            do_box(ctx, |ctx| {
                RichText::new().build(ctx, |x| {
                    x.text("Leading — ");
                    x.image(decoration_svg());
                    x.text(" — trailing.");
                });
            });

            // ═══ 6. 多行混合 ═══
            Text::new("■ Multi-line with wrapping").font_size(16.0).color(Color::from_argb(255, 100, 100, 100)).build(ctx);

            do_box(ctx, |ctx| {
                RichText::new().build(ctx, |x| {
                    x.text("Welcome to ");
                    x.color(Color::from_argb(255, 76, 175, 80), |x| { x.bold(|x| { x.text("Winia"); }); });
                    x.text(" — a declarative cross-platform GUI framework ");
                    x.image(checkmark_svg());
                    x.text(" inspired by ");
                    x.color(Color::from_argb(255, 33, 150, 243), |x| { x.italic(|x| { x.text("Jetpack Compose"); }); });
                    x.text(" and built with ");
                    x.bold(|x| { x.text("Skia"); });
                    x.text(" + ");
                    x.bold(|x| { x.text("Winit"); });
                    x.text(". Rich text enables ");
                    x.image(star_svg());
                    x.text(" inline elements like icons, images ");
                    x.image(info_svg());
                    x.text(" mixed with ");
                    x.italic(|x| { x.text("styled text"); });
                    x.text(".");
                });
            });

            // ═══ 7. 对齐测试 ═══
            Text::new("■ Center alignment").font_size(16.0).color(Color::from_argb(255, 100, 100, 100)).build(ctx);

            ProvideTextStyle(TextStyle::new().align(TextAlign::Center), ctx, |ctx| {
                do_box(ctx, |ctx| {
                    RichText::new().build(ctx, |x| {
                        x.text("Centered text with ");
                        x.image(star_svg());
                        x.text(" inline icon");
                    });
                });
            });
        });
}

fn do_box(ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
    Column::new()
        .modifier(Modifier::new().fill_max_width().padding(10.0).background(Color::from_argb(18, 0, 0, 0), Shape::rounded(6.0)))
        .build(ctx, content);
}
