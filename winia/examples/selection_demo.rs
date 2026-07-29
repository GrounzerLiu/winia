//! 文本选中演示 — SelectionContainer + on_selection_change + 跨 Text 合并选择

use winia::prelude::*;
use winia::app;

fn selection_ui(ctx: &mut ComposeCtx) {
    let t1 = "Hello! 👋😊 The SelectionContainer makes text selectable.";
    let t2 = "🎉 Drag across 🚀 multiple texts! The highlight follows.";
    let t3 = "You can select across multiple texts! The blue highlight follows the mouse as you drag.";
    let all = format!("{}{}{}", t1, t2, t3);

    let selected = ctx.remember(|| String::from("(none)"));

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .build(ctx, |ctx| {
            Text::new("Text Selection Demo (cross-text)")
                .font_size(22.0).font_weight(FontWeight::BOLD)
                .modifier(Modifier::new().padding(4.0))
                .build(ctx);

            let s = selected.clone();
            let all_text = all.clone();
            SelectionContainer::new()
                .modifier(Modifier::new()
                    .fill_max_width()
                    .padding(12.0)
                    .background(Color::from_argb(30, 200, 200, 100), Shape::rounded(8.0)))
                .on_selection_change(move |start, end| {
                    let txt = &all_text[start.min(end)..start.max(end)];
                    s.set(format!("\"{}\"", txt));
                })
                .build(ctx, |ctx| {
                    Column::new().build(ctx, |ctx| {
                        Text::new(t1).font_size(16.0).modifier(Modifier::new().padding(4.0)).build(ctx);
                        Text::new(t2).font_size(14.0).color(Color::from_argb(200, 60, 60, 60)).modifier(Modifier::new().padding(4.0)).build(ctx);
                        Text::new(t3).font_size(13.0).modifier(Modifier::new().padding(4.0)).build(ctx);
                    });
                });

            Text::new(format!("Selected: {}", selected.get()))
                .font_size(14.0).font_weight(FontWeight::BOLD)
                .color(Color::from_argb(255, 0, 100, 200))
                .modifier(Modifier::new().padding(8.0))
                .build(ctx);
        });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    app::run_app(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(520.0, 550.0)
                .title("Text Selection Demo")
                .build(ctx, selection_ui);
        });
    });
}
