//! RichText 组件演示 — Compose 风格嵌套作用域 API
//!
//! 展示嵌套作用域、装饰属性、图文混排、深层嵌套等功能。

use winia::prelude::*;
use winia::app;

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    app::run_app(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(600.0, 900.0)
                .title("RichText Demo")
                .build(ctx, |ctx| rich_text_demo_ui(ctx));
        });
    });
}

// ── SVG 图标 ──

fn check() -> SvgDrawable {
    SvgDrawable::from_str(r##"<svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="10" fill="#4CAF50"/><path d="M8 12l3 3 5-5" stroke="white" stroke-width="2" fill="none"/></svg>"##, 18.0, 18.0).expect("check")
}
fn cross() -> SvgDrawable {
    SvgDrawable::from_str(r##"<svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="10" fill="#F44336"/><path d="M9 9l6 6M15 9l-6 6" stroke="white" stroke-width="2" fill="none"/></svg>"##, 18.0, 18.0).expect("cross")
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
        .modifier(Modifier::new().size(Dimension::Fill, 900.0).padding(16.0).vertical_scroll(scroll_state))
        .spacing(12.0)
        .build(ctx, |ctx| {

            // ═══════════════════════════════════════════════════════════
            // 1. 基础样式作用域
            // ═══════════════════════════════════════════════════════════
            heading("Basic — font_size / bold / italic / color scopes", ctx);

            card(ctx, |ctx| {
                RichText::new().build(ctx, |x| {
                    x.text("Normal ");
                    x.bold(|x| { x.font_size(18.0, |x| { x.text("Bold large "); }); });
                    x.italic(|x| { x.text("Italic "); });
                    x.color(Color::RED, |x| { x.bold(|x| { x.text("Red bold!"); }); });
                });
            });

            // ═══════════════════════════════════════════════════════════
            // 2. 行内 SVG
            // ═══════════════════════════════════════════════════════════
            heading("Inline SVG icons", ctx);

            card(ctx, |ctx| {
                RichText::new().build(ctx, |x| {
                    x.text("Task 1: ");
                    x.bold(|x| { x.text("Completed "); });
                    x.image(check());
                });
            });
            card(ctx, |ctx| {
                RichText::new().build(ctx, |x| {
                    x.text("Rating: ");
                    x.image(star()); x.text(" ");
                    x.image(star()); x.text(" ");
                    x.image(star()); x.text(" ");
                    x.image(star()); x.text(" 4/5");
                });
            });

            // ═══════════════════════════════════════════════════════════
            // 3. 装饰属性
            // ═══════════════════════════════════════════════════════════
            heading("Decorations — underline / strikethrough / background", ctx);

            card(ctx, |ctx| {
                RichText::new().build(ctx, |x| {
                    x.font_size(14.0, |x| {
                        x.underline(|x| { x.text("Underlined. "); });
                        x.strikethrough(|x| { x.text("Strikethrough. "); });
                        x.background(Color::from_argb(60, 255, 255, 0), |x| { x.text("Highlight. "); });
                        x.underline(|x| { x.strikethrough(|x| { x.color(Color::RED, |x| { x.text("Both!"); }); }); });
                    });
                });
            });

            // ═══════════════════════════════════════════════════════════
            // 4. 深层嵌套（3 层 + 多属性合并）
            // ═══════════════════════════════════════════════════════════
            heading("Deep nesting — bold > italic > underline (3 levels)", ctx);

            card(ctx, |ctx| {
                RichText::new().build(ctx, |x| {
                    x.bold(|x| {
                        x.text("Bold. ");
                        x.italic(|x| {
                            x.text("Bold+Italic. ");
                            x.underline(|x| {
                                x.text("Bold+Italic+Underline. ");
                            });
                        });
                        x.text("Bold again. ");
                    });
                });
            });

            // ═══════════════════════════════════════════════════════════
            // 5. style() 闭包一次性设置多个属性
            // ═══════════════════════════════════════════════════════════
            heading("Style modifier — multiple attrs in one closure", ctx);

            card(ctx, |ctx| {
                RichText::new().build(ctx, |x| {
                    x.text("Plain. ");
                    x.color(Color::from_argb(255, 33, 150, 243), |x| {
                        x.bold(|x| { x.text("Blue bold. "); });
                    });
                    x.style(|s| s.bold().italic().underline().font_size(16.0), |x| {
                        x.text("Bold+Italic+Underline+Big! ");
                    });
                    x.text("Plain again.");
                });
            });

            // ═══════════════════════════════════════════════════════════
            // 6. 图片嵌入带样式的文本
            // ═══════════════════════════════════════════════════════════
            heading("Image within styled text spans", ctx);

            card(ctx, |ctx| {
                RichText::new().build(ctx, |x| {
                    x.bold(|x| {
                        x.text("Bold before ");
                        x.image(star());
                        x.text(" after bold");
                    });
                });
            });

            card(ctx, |ctx| {
                RichText::new().build(ctx, |x| {
                    x.text("Icon in mid: ");
                    x.image(info());
                    x.text(" between text.");
                });
            });

            // ═══════════════════════════════════════════════════════════
            // 7. 连续图片
            // ═══════════════════════════════════════════════════════════
            heading("Consecutive images (no text between)", ctx);

            card(ctx, |ctx| {
                RichText::new().build(ctx, |x| {
                    x.text("Three icons: ");
                    x.image(star()); x.image(star()); x.image(star());
                    x.text(" — done!");
                });
            });

            // ═══════════════════════════════════════════════════════════
            // 8. 多行混合长文
            // ═══════════════════════════════════════════════════════════
            heading("Multi-line paragraph with wrapping", ctx);

            card(ctx, |ctx| {
                RichText::new().build(ctx, |x| {
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
                    x.text(" inline elements like icons, images ");
                    x.image(info());
                    x.text(" mixed with ");
                    x.italic(|x| { x.text("styled text"); });
                    x.text(".");
                });
            });

            // ═══════════════════════════════════════════════════════════
            // 9. text_styled 快捷用法
            // ═══════════════════════════════════════════════════════════
            heading("text_styled — quick inline style", ctx);

            card(ctx, |ctx| {
                RichText::new().build(ctx, |x| {
                    x.text("Normal ");
                    x.text_styled("red bold ", &TextStyle::new().color(Color::RED).bold());
                    x.text("normal ");
                    x.text_styled("big italic ", &TextStyle::new().font_size(18.0).italic());
                    x.text("end.");
                });
            });

            // ═══════════════════════════════════════════════════════════
            // 10. 对齐（通过 ProvideTextStyle）
            // ═══════════════════════════════════════════════════════════
            heading("Center alignment (via ProvideTextStyle)", ctx);

            ProvideTextStyle(TextStyle::new().align(TextAlign::Center), ctx, |ctx| {
                card(ctx, |ctx| {
                    RichText::new().build(ctx, |x| {
                        x.text("Centered text with ");
                        x.image(star());
                        x.text(" inline icon.");
                    });
                });
            });

            // ═══════════════════════════════════════════════════════════
            // 11. 通知卡片 — 综合实践
            // ═══════════════════════════════════════════════════════════
            heading("Notification card — practical combo", ctx);

            Stack::new()
                .modifier(Modifier::new().fill_max_width().background(Color::from_argb(240, 230, 240, 255), Shape::rounded(8.0)))
                .build(ctx, |ctx| {
                    // 蓝色条
                    Text::new("").modifier(Modifier::new().size(4.0, Dimension::Fill).background(Color::from_argb(255, 33, 150, 243), Shape::rounded(2.0))).build(ctx);
                    Column::new().modifier(Modifier::new().fill_max_size().padding(14.0)).build(ctx, |ctx| {
                        RichText::new().build(ctx, |x| {
                            x.image(info());
                            x.text("  ");
                            x.bold(|x| { x.text("Update available"); });
                        });
                        Text::new("Version 2.5.0 is ready to install. Click to update.")
                            .font_size(12.0).color(Color::from_argb(255, 100, 100, 100))
                            .build(ctx);
                    });
                });

            // ═══════════════════════════════════════════════════════════
            // 12. 小图 + 文字基线对齐
            // ═══════════════════════════════════════════════════════════
            heading("Small inline icons with text baseline", ctx);

            card(ctx, |ctx| {
                RichText::new().build(ctx, |x| {
                    x.bold(|x| { x.text("NOTE: "); });
                    x.image(info());
                    x.text(" This shows how ");
                    x.italic(|x| { x.text("inline elements"); });
                    x.text(" align with text baseline. ");
                    x.image(star());
                    x.text(" All inline!");
                });
            });

            // ═══════════════════════════════════════════════════════════
            // 13. 大图 + 文字混排
            // ═══════════════════════════════════════════════════════════
            heading("Large decoration inline", ctx);

            card(ctx, |ctx| {
                RichText::new().build(ctx, |x| {
                    x.text("Leading text — ");
                    x.image(big_decoration());
                    x.text(" — trailing text with large inline decoration. The line height should adjust to accommodate the decoration.");
                });
            });

            // ═══════════════════════════════════════════════════════════
            // 14. 多属性全作用于同一文本
            // ═══════════════════════════════════════════════════════════
            heading("All attributes on one text", ctx);

            card(ctx, |ctx| {
                RichText::new().build(ctx, |x| {
                    x.style(|s| s.bold().italic().underline().strikethrough().font_size(16.0).color(Color::from_argb(255, 156, 39, 176)), |x| {
                        x.text("Bold + Italic + Underline + Strikethrough + Purple + Big");
                    });
                });
            });
        });
}

// ── 辅助函数 ──

fn heading(text: &str, ctx: &mut ComposeCtx) {
    Text::new(text).font_size(16.0).color(Color::from_argb(255, 100, 100, 100)).build(ctx);
}

fn card(ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
    Column::new()
        .modifier(Modifier::new().fill_max_width().padding(10.0).background(Color::from_argb(18, 0, 0, 0), Shape::rounded(6.0)))
        .build(ctx, content);
}
