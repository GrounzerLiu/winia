//! Text 组件功能演示 — 字体、颜色、对齐、溢出、样式层级
//!
//! 展示 Text 组件的各种配置组合。

use winia::prelude::*;
use winia::app;

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();

    app::run_app(winia::app_root!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(540.0, 700.0)
                .title("Text Demo")
                .build(ctx, |ctx| {
                    text_demo_ui(ctx);
                });
        });
    }));
}

#[composable]
fn text_demo_ui(ctx: &mut ComposeCtx) {
    // 用于演示 justify 对齐的切换状态
    let justify_count = ctx.remember(|| 10i32);
    let long_text = "The quick brown fox jumps over the lazy dog. ";
    let scroll_state = ctx.remember(|| ScrollState::new()).get();

    Column::new()
        .modifier(Modifier::new().size(Dimension::Fill, 700.0).padding(16.0).vertical_scroll(scroll_state))
        .spacing(8.0)
        .build(ctx, |ctx| {
            // ── 1. 基础文字 + 字号 ──
            Text::new("■ Font Size").font_size(16.0).color(Color::from_argb(255, 100, 100, 100)).build(ctx);

            Row::new().spacing(12.0).build(ctx, |ctx| {
                Text::new("12px").font_size(12.0).build(ctx);
                Text::new("16px").font_size(16.0).build(ctx);
                Text::new("20px").font_size(20.0).build(ctx);
                Text::new("28px").font_size(28.0).build(ctx);
                Text::new("36px").font_size(36.0).build(ctx);
            });

            Column::new().modifier(Modifier::new().fill_max_width().height(1.0).background(Color::from_argb(40, 0, 0, 0), Shape::Rectangle)).build(ctx, |_| {});

            // ── 2. 文字颜色（内置 Color + Theme 颜色）──
            Text::new("■ Colors").font_size(16.0).color(Color::from_argb(255, 100, 100, 100)).build(ctx);
            Row::new().spacing(8.0).build(ctx, |ctx| {
                Text::new("Red").color(Color::RED).font_size(16.0).build(ctx);
                Text::new("Green").color(Color::GREEN).font_size(16.0).build(ctx);
                Text::new("Blue").color(Color::BLUE).font_size(16.0).build(ctx);
                Text::new("Custom").color(Color::from_argb(255, 200, 100, 50)).font_size(16.0).build(ctx);
            });

            Text::new("Theme colors via .color() override")
                .color(Color::from_argb(255, 120, 80, 200))
                .font_size(14.0)
                .build(ctx);

            // ── 3. 对齐方式（TextAlign）──
            Text::new("■ Text Alignment").font_size(16.0).color(Color::from_argb(255, 100, 100, 100)).build(ctx);

            // Left
            Text::new("⬅ Left aligned (default)")
                .font_size(14.0)
                .modifier(Modifier::new().fill_max_width().height(20.0).background(Color::from_argb(20, 0, 0, 0), Shape::Rectangle))
                .build(ctx);

            // Center
            Text::new("⬇ Center aligned")
                .font_size(14.0)
                .align(TextAlign::Center)
                .modifier(Modifier::new().fill_max_width().height(20.0).background(Color::from_argb(20, 0, 0, 0), Shape::Rectangle))
                .build(ctx);

            // Right
            Text::new("➡ Right aligned")
                .font_size(14.0)
                .align(TextAlign::Right)
                .modifier(Modifier::new().fill_max_width().height(20.0).background(Color::from_argb(20, 0, 0, 0), Shape::Rectangle))
                .build(ctx);

            // Justify
            Text::new("⇔ Justify: ").font_size(12.0).build(ctx);
            let txt = long_text.repeat(justify_count.get() as usize / 5 + 1);
            Text::new(&txt)
                .font_size(12.0)
                .align(TextAlign::Justify)
                .modifier(Modifier::new().fill_max_width().height(60.0).background(Color::from_argb(20, 0, 0, 0), Shape::Rectangle))
                .build(ctx);

            Button::new()
                .on_click({ let c = justify_count.clone(); move || { c.update(|v| *v += 1); } })
                .modifier(Modifier::new().size(80.0, 24.0))
                .build(ctx, |ctx| { Text::new("+ word").font_size(11.0).build(ctx); });

            // ── 4. 溢出处理（max_lines + overflow）──
            Text::new("■ Overflow: max_lines + Ellipsis").font_size(16.0).color(Color::from_argb(255, 100, 100, 100)).build(ctx);

            // Clip (default) — 截断
            Text::new("Clip (default):")
                .font_size(12.0)
                .color(Color::from_argb(255, 120, 120, 120))
                .build(ctx);

            Text::new("This is a very long line that should be clipped because we only have one line available")
                .font_size(12.0)
                .max_lines(1)
                .overflow(TextOverflow::Clip)
                .modifier(Modifier::new().fill_max_width().height(20.0).background(Color::from_argb(20, 255, 0, 0), Shape::Rectangle))
                .build(ctx);

            // Ellipsis — 省略号
            Text::new("Ellipsis:")
                .font_size(12.0)
                .color(Color::from_argb(255, 120, 120, 120))
                .build(ctx);

            Text::new("This is a very long line that should end with an ellipsis character…")
                .font_size(12.0)
                .max_lines(1)
                .overflow(TextOverflow::Ellipsis)
                .modifier(Modifier::new().fill_max_width().height(20.0).background(Color::from_argb(20, 0, 255, 0), Shape::Rectangle))
                .build(ctx);

            // Multi-line with max_lines + ellipsis
            Text::new("Multi-line (max_lines=2, ellipsis):")
                .font_size(12.0)
                .color(Color::from_argb(255, 120, 120, 120))
                .build(ctx);

            Text::new("This is a multi-line text that will be truncated after two lines with an ellipsis character at the end of the second line.")
                .font_size(12.0)
                .max_lines(2)
                .overflow(TextOverflow::Ellipsis)
                .modifier(Modifier::new().fill_max_width().height(50.0).background(Color::from_argb(20, 0, 0, 255), Shape::Rectangle))
                .build(ctx);

            // ── 5. 粗体与斜体（FontWeight / FontSlant）──
            Text::new("■ Font Weight & Style").font_size(16.0).color(Color::from_argb(255, 100, 100, 100)).build(ctx);

            Row::new().spacing(12.0).build(ctx, |ctx| {
                Text::new("Normal").font_size(14.0).build(ctx);
                Text::new("Bold").bold().font_size(14.0).build(ctx);
                Text::new("Italic").italic().font_size(14.0).build(ctx);
                Text::new("B+I").bold().italic().font_size(14.0).build(ctx);
            });

            Text::new("Custom weight (Light 300) — makes text thinner")
                .font_weight(FontWeight::LIGHT).font_size(14.0).build(ctx);

            Text::new("Custom weight (SemiBold 600) — a bit heavier")
                .font_weight(FontWeight::SEMI_BOLD).font_size(14.0).build(ctx);

            Text::new("TextStyle.bold() via ProvideTextStyle")
                .font_size(14.0).build(ctx);

            ProvideTextStyle(TextStyle::new().bold(), ctx, |ctx| {
                Text::new("This line inherits bold from ProvideTextStyle")
                    .font_size(13.0)
                    .build(ctx);

                Text::new("This one overrides back to normal weight")
                    .font_weight(FontWeight::NORMAL).font_size(13.0)
                    .build(ctx);
            });

            // ── 6. soft_wrap — 不换行 ──
            Text::new("■ soft_wrap = false (no wrap)").font_size(16.0).color(Color::from_argb(255, 100, 100, 100)).build(ctx);

            Text::new("This text does NOT wrap even if it exceeds the container width → ")
                .font_size(12.0)
                .soft_wrap(false)
                .modifier(Modifier::new().fill_max_width().height(20.0).background(Color::from_argb(30, 0, 0, 0), Shape::Rectangle))
                .build(ctx);

            // ── 7. 样式层级（ProvideTextStyle → style → 单独参数）──
            Text::new("■ Style Cascading").font_size(16.0).color(Color::from_argb(255, 100, 100, 100)).build(ctx);

            ProvideTextStyle(
                TextStyle::new().color(Color::from_argb(255, 200, 100, 0)).font_size(18.0).align(TextAlign::Center),
                ctx,
                |ctx| {
                    Text::new("Style: (orange, 18px, center) via ProvideTextStyle")
                        .modifier(Modifier::new().fill_max_width().height(24.0).background(Color::from_argb(15, 0, 0, 0), Shape::Rectangle))
                        .build(ctx);

                    // 单独参数覆盖 style 的部分字段
                    Text::new("Style: color overridden to green by .color()")
                        .color(Color::GREEN)
                        .modifier(Modifier::new().fill_max_width().height(24.0).background(Color::from_argb(15, 0, 0, 0), Shape::Rectangle))
                        .build(ctx);

                    // 完整覆盖
                    Text::new("Style: all overridden (purple, 11px, left)")
                        .color(Color::from_argb(255, 150, 50, 200))
                        .font_size(11.0)
                        .align(TextAlign::Left)
                        .build(ctx);
                },
            );

            // ── 8. 综合示例 ──
            Text::new("■ Combined Example").font_size(16.0).color(Color::from_argb(255, 100, 100, 100)).build(ctx);

            Text::new(
                "This is a longer paragraph that demonstrates multiple features at once: \
                 it uses text alignment, max lines with ellipsis, and a larger font size. \
                 The color comes from the theme's on_surface by default."
            )
                .font_size(13.0)
                .max_lines(3)
                .overflow(TextOverflow::Ellipsis)
                .align(TextAlign::Justify)
                .modifier(Modifier::new()
                    .fill_max_width()
                    .height(60.0)
                    .padding(8.0)
                    .background(Color::from_argb(12, 0, 0, 0), Shape::rounded(6.0)))
                .build(ctx);

            // Title Card
            Text::new("Title Card")
                .font_size(22.0)
                .color(Color::from_argb(255, 50, 50, 180))
                .modifier(Modifier::new().padding(4.0))
                .build(ctx);
        });
}

