//! RichText 组件演示 — 全部属性展示

use winia::prelude::*;
use winia::app;

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    app::run_app(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(680.0, 1200.0)
                .title("RichText — All Features")
                .build(ctx, |ctx| rich_text_demo_ui(ctx));
        });
    });
}

fn check() -> SvgDrawable {
    SvgDrawable::from_str(r##"<svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="10" fill="#4CAF50"/><path d="M8 12l3 3 5-5" stroke="white" stroke-width="2" fill="none"/></svg>"##, 18.0, 18.0).expect("check")
}
fn star() -> SvgDrawable {
    SvgDrawable::from_str(r##"<svg viewBox="0 0 24 24"><path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z" fill="#FFC107"/></svg>"##, 20.0, 20.0).expect("star")
}
fn info() -> SvgDrawable {
    SvgDrawable::from_str(r##"<svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="10" fill="#2196F3"/><path d="M12 16v-4M12 8h.01" stroke="white" stroke-width="2" fill="none" stroke-linecap="round"/></svg>"##, 16.0, 16.0).expect("info")
}
fn big_decoration() -> SvgDrawable {
    SvgDrawable::from_str(r##"<svg viewBox="0 0 48 48"><defs><linearGradient id="g" x1="0%" y1="0%" x2="100%" y2="100%"><stop offset="0%" stop-color="#FF6B6B"/><stop offset="50%" stop-color="#4ECDC4"/><stop offset="100%" stop-color="#45B7D1"/></linearGradient></defs><circle cx="24" cy="24" r="22" fill="url(#g)"/><text x="24" y="30" text-anchor="middle" fill="white" font-size="20">✦</text></svg>"##, 36.0, 36.0).expect("decoration")
}

fn rich_text_demo_ui(ctx: &mut ComposeCtx) {
    let scroll_state = ctx.remember(|| ScrollState::new()).get();

    Column::new()
        .modifier(Modifier::new().size(Dimension::Fill, 1200.0).padding(16.0).vertical_scroll(scroll_state))
        .spacing(12.0)
        .build(ctx, |ctx| {

    // ═══ 1. 基础样式 ═══
    sec("1. Basic — font_size / bold / italic / color", ctx);
    card(ctx, |ctx| { RichText::new().build(ctx, |x| {
        x.text("Normal ");
        x.bold(|x| { x.font_size(18.0, |x| { x.text("Bold large "); }); });
        x.italic(|x| { x.text("Italic "); });
        x.color(Color::RED, |x| { x.bold(|x| { x.text("Red bold!"); }); });
    });});

    // ═══ 2. 装饰线 ═══
    sec("2. Decorations — underline / overline / strikethrough / color / style", ctx);
    card(ctx, |ctx| { RichText::new().build(ctx, |x| {
        x.font_size(14.0, |x| {
            x.underline(|x| { x.text("Underline "); });
            x.overline(|x| { x.text("Overline "); });
            x.strikethrough(|x| { x.text("Strikethrough "); });
            x.underline(|x| { x.strikethrough(|x| { x.color(Color::RED, |x| { x.text("Both!"); }); }); });
            x.text(". ");
            x.style(|s| s.underline().decoration_color(Color::RED), |x| { x.text("Red underline"); });
            x.text(". ");
            x.style(|s| s.strikethrough().decoration_style(DecoStyle::Double), |x| { x.text("Double strike"); });
            x.text(".");
        });
    });});

    // ═══ 3. 上标/下标 ═══
    sec("3. Subscript / Superscript / Baseline shift", ctx);
    card(ctx, |ctx| { RichText::new().build(ctx, |x| {
        x.font_size(16.0, |x| {
            x.text("Normal ");
            x.superscript(|x| { x.text("superscript "); });
            x.text("normal ");
            x.subscript(|x| { x.text("subscript "); });
            x.text("normal. ");
            x.baseline_shift(0.3, |x| { x.text("shift+0.3 "); });
        });
    });});

    // ═══ 4. 背景 / 前景 ═══
    sec("4. Background / Foreground color", ctx);
    card(ctx, |ctx| { RichText::new().build(ctx, |x| {
        x.font_size(14.0, |x| {
            x.background(Color::from_argb(60, 255, 255, 0), |x| { x.text("Highlight "); });
            x.foreground_color(Color::RED, |x| { x.text("Foreground red "); });
            x.style(|s| s.bold().background(Color::from_argb(80, 100, 150, 255)).foreground_color(Color::WHITE), |x| {
                x.text(" Bold white on blue ");
            });
        });
    });});

    // ═══ 5. 字间距 / 词间距 / 行高 ═══
    sec("5. Letter spacing / Word spacing / Height multiple", ctx);
    card(ctx, |ctx| { RichText::new().build(ctx, |x| {
        x.letter_spacing(3.0, |x| { x.text("Wide spacing "); });
        x.letter_spacing(0.0, |x| { x.text("normal ") });
        x.word_spacing(10.0, |x| { x.text("word  spacing"); });
    });});

    // ═══ 6. 行高 / Half-leading ═══
    sec("6. Line height — height_multiple / half_leading", ctx);
    card(ctx, |ctx| { RichText::new().build(ctx, |x| {
        x.text("Default height line 1\n");
        x.text("Default height line 2\n");
        x.height_multiple(1.8, |x| {
            x.text("1.8x height line 1\n");
            x.text("1.8x height line 2\n");
        });
        x.text("back to normal\n");
        x.half_leading(|x| {
            x.text("half leading line 1\n");
            x.text("half leading line 2");
        });
    });});

    // ═══ 7. 字体族 / 字宽 ═══
    sec("7. Font families / Font width", ctx);
    card(ctx, |ctx| { RichText::new().build(ctx, |x| {
        x.text("Default ");
        x.style(|s| s.font_width(3), |x| { x.text("Condensed "); });
        x.style(|s| s.font_width(7), |x| { x.text("Expanded "); });
        x.text("normal width.");
    });});

    // ═══ 8. 渲染精度 ═══
    sec("8. Font edging / hinting / subpixel", ctx);
    card(ctx, |ctx| { RichText::new().build(ctx, |x| {
        x.text("Default anti-alias text. ");
        x.style(|s| s.font_edging(FontEdge::Alias), |x| { x.text("Aliased text. "); });
        x.style(|s| s.subpixel(), |x| { x.text("Subpixel text. "); });
    });});

    // ═══ 9. 行内 SVG 图片 ═══
    sec("9. Inline SVG icons", ctx);
    card(ctx, |ctx| { RichText::new().build(ctx, |x| {
        x.text("Icons: ");
        x.image(star()); x.text(" ");
        x.image(info()); x.text(" ");
        x.image(check());
        x.text(" inline in text.");
    });});
    card(ctx, |ctx| { RichText::new().build(ctx, |x| {
        x.text("Rating: ");
        x.image(star()); x.image(star()); x.image(star()); x.image(star());
        x.text(" 4/5");
    });});

    // ═══ 10. 图片在样式作用域内 ═══
    sec("10. Image within styled spans", ctx);
    card(ctx, |ctx| { RichText::new().build(ctx, |x| {
        x.bold(|x| {
            x.text("Bold ");
            x.image(star());
            x.text(" bold again");
        });
    });});

    // ═══ 11. 深层嵌套 ═══
    sec("11. Deep nesting — bold > italic > underline (3 levels)", ctx);
    card(ctx, |ctx| { RichText::new().build(ctx, |x| {
        x.bold(|x| {
            x.text("Bold. ");
            x.italic(|x| {
                x.text("Bold+Italic. ");
                x.underline(|x| { x.text("All three. "); });
            });
            x.text("Bold again. ");
        });
    });});

    // ═══ 12. style() 多属性闭包 ═══
    sec("12. style() — multiple attrs in one closure", ctx);
    card(ctx, |ctx| { RichText::new().build(ctx, |x| {
        x.style(|s| s.bold().italic().underline().superscript().font_size(16.0).color(Color::RED).letter_spacing(0.5), |x| {
            x.text("Bold+Italic+Super+Red+Spaced!");
        });
    });});

    // ═══ 13. text_styled 快捷样式 ═══
    sec("13. text_styled — quick inline via TextStyle", ctx);
    card(ctx, |ctx| { RichText::new().build(ctx, |x| {
        x.text("Normal ");
        x.text_styled("red bold ", &TextStyle::new().color(Color::RED).bold());
        x.text_styled("big italic ", &TextStyle::new().font_size(18.0).italic());
        x.text("end.");
    });});

    // ═══ 14. Locale ═══
    sec("14. Locale (affects line breaking / glyph selection)", ctx);
    card(ctx, |ctx| { RichText::new().build(ctx, |x| {
        x.locale("ja-JP", |x| { x.text("日本語テキスト "); });
        x.locale("ar-SA", |x| { x.text("نص عربي "); });
        x.text("Default locale.");
    });});

    // ═══ 15. Center alignment ═══
    sec("15. Center alignment (via ProvideTextStyle)", ctx);
    ProvideTextStyle(TextStyle::new().align(TextAlign::Center), ctx, |ctx| {
        card(ctx, |ctx| { RichText::new().build(ctx, |x| {
            x.text("Centered text with ");
            x.image(star());
            x.text(" icon.");
        });});
    });

    // ═══ 16. 大图行内 ═══
    sec("16. Large inline decoration", ctx);
    card(ctx, |ctx| { RichText::new().build(ctx, |x| {
        x.text("Leading — ");
        x.image(big_decoration());
        x.text(" — trailing.");
    });});

    // ═══ 17. 综合长文 ═══
    sec("17. Mixed long paragraph", ctx);
    card(ctx, |ctx| { RichText::new().build(ctx, |x| {
        x.text("Welcome to ");
        x.color(Color::from_argb(255, 76, 175, 80), |x| { x.bold(|x| { x.text("Winia"); }); });
        x.text(" — a declarative cross-platform GUI framework ");
        x.image(check());
        x.text(" inspired by ");
        x.color(Color::from_argb(255, 33, 150, 243), |x| { x.italic(|x| { x.text("Jetpack Compose"); }); });
        x.text(" and built with ");
        x.bold(|x| { x.text("Skia"); });
        x.text(" + ");
        x.bold(|x| { x.text("Winit"); });
        x.text(". Rich text enables ");
        x.image(star());
        x.text(" inline elements, ");
        x.image(info());
        x.text(" and ");
        x.italic(|x| { x.text("styled text"); });
        x.text(". Use ");
        x.superscript(|x| { x.text("superscript"); });
        x.text(" for math, ");
        x.subscript(|x| { x.text("subscript"); });
        x.text(" for chemistry (");
        x.subscript(|x| { x.text("2"); });
        x.text("O), ");
        x.style(|s| s.underline().decoration_color(Color::RED), |x| { x.text("red underline"); });
        x.text(" for warnings, and ");
        x.background(Color::from_argb(60, 255, 255, 0), |x| { x.text("highlight"); });
        x.text(" for emphasis!");
    });});

    // ═══ 18. 通知卡片 ═══
    sec("18. Notification card", ctx);
    Stack::new()
        .modifier(Modifier::new().fill_max_width().background(Color::from_argb(240, 230, 240, 255), Shape::rounded(8.0)))
        .build(ctx, |ctx| {
            Text::new("").modifier(Modifier::new().size(4.0, 60.0).background(Color::from_argb(255, 33, 150, 243), Shape::rounded(2.0))).build(ctx);
            Column::new().modifier(Modifier::new().fill_max_width().padding(14.0)).build(ctx, |ctx| {
                RichText::new().build(ctx, |x| {
                    x.image(info()); x.text("  ");
                    x.bold(|x| { x.text("Update available"); });
                });
                Text::new("Version 2.5.0 is ready. Click to update.")
                    .font_size(12.0).color(Color::from_argb(255, 100, 100, 100)).build(ctx);
            });
        });

    // ═══ 19. 全部属性叠加 ═══
    sec("19. ALL attributes combined", ctx);
    card(ctx, |ctx| { RichText::new().build(ctx, |x| {
        x.style(|s| s
            .bold().italic().underline().overline().strikethrough()
            .decoration_color(Color::RED).decoration_style(DecoStyle::Double)
            .font_size(16.0).color(Color::from_argb(255, 156, 39, 176))
            .background(Color::from_argb(40, 255, 255, 0))
            .letter_spacing(1.0).superscript()
            .font_width(6).subpixel(),
            |x| { x.text("ALL attrs!"); });
    });});

    // ═══ 20. Consecutive images ═══
    sec("20. Consecutive images (no text between)", ctx);
    card(ctx, |ctx| { RichText::new().build(ctx, |x| {
        x.text("Three: ");
        x.image(star()); x.image(star()); x.image(star());
        x.text(" — done!");
    });});
        });
}

fn sec(text: &str, ctx: &mut ComposeCtx) {
    Text::new(text).font_size(15.0).color(Color::from_argb(255, 100, 100, 100)).build(ctx);
}

fn card(ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
    Column::new()
        .modifier(Modifier::new().fill_max_width().padding(10.0).background(Color::from_argb(18, 0, 0, 0), Shape::rounded(6.0)))
        .build(ctx, content);
}
