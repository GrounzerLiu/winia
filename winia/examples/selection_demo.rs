//! 文本选中演示 — SelectionContainer + on_selection_change + 跨 Text 合并选择

use winia::prelude::*;
use winia::ui::RichText;
use winia::app;

#[composable]
fn selection_ui(ctx: &mut ComposeCtx) {
    let t1 = "Hello! 👋😊 The SelectionContainer makes text selectable.";
    let t2 = "🎉 Drag across 🚀 multiple texts! The highlight follows.";
    let t3 = "You can select across multiple texts! The blue highlight follows the mouse as you drag.";
    let rt = "RichText: bold italic red — also selectable!";
    // 注意：on_selection_change 的偏移是**容器全局字节偏移**（含容器内所有
    // 已注册 Text——包括标题 [A]）。all 必须与注册顺序对齐（[A] 在前 4 字节），
    // 否则 get(s..e) 错位后可能落在 emoji 多字节中间返回 None（空切片）。
    let all = format!("[A] {}{}{}{}", t1, t2, rt, t3);

    let a1 = "Container B: independent selection context.";
    let a2 = "This text is inside a separate SelectionContainer.";
    // 同上：B 容器的全局偏移含 [B]（前 4 字节）——all_b 对齐
    let all_b = format!("[B] {}{}", a1, a2);

    let selected_a = ctx.remember(|| String::from("(none)"));
    let selected_b = ctx.remember(|| String::from("(none)"));

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .build(ctx, |ctx| {
            Text::new("Two SelectionContainers — independent selection")
                .font_size(18.0).font_weight(FontWeight::BOLD)
                .modifier(Modifier::new().padding(4.0))
                .build(ctx);

            // ── Container A ──
            let sa = selected_a.clone();
            let all_a_text = all.clone();
            SelectionContainer::new()
                .modifier(Modifier::new()
                    .fill_max_width()
                    .padding(10.0)
                    .background(Color::from_argb(30, 200, 200, 100), Shape::rounded(8.0)))
                .on_selection_change(move |start, end| {
                    let s = start.min(end); let e = start.max(end);
                    let txt = all_a_text.get(s..e).unwrap_or(""); // 字节安全切片（emoji 多字节时 get 防越界）
                    sa.set(format!("A: \"{}\"", txt));
                })
                .build(ctx, |ctx| {
                    Column::new().build(ctx, |ctx| {
                        Text::new("[A] ").font_size(12.0).color(Color::from_argb(200, 0, 100, 0)).build(ctx);
                        Text::new(t1).font_size(15.0).modifier(Modifier::new().padding(2.0)).build(ctx);
                        Text::new(t2).font_size(13.0).color(Color::from_argb(200, 60, 60, 60)).modifier(Modifier::new().padding(2.0)).build(ctx);
                        RichText::new().build(ctx, |x| {
                            x.text("RichText: ");
                            x.bold(|x| { x.text("bold "); });
                            x.italic(|x| { x.text("italic "); });
                            x.color(Color::from_argb(255, 200, 50, 50), |x| { x.text("red"); });
                            x.text(" — selectable!");
                        });
                        Text::new(t3).font_size(12.0).modifier(Modifier::new().padding(2.0)).build(ctx);
                    });
                });

            Text::new(format!("Container A: {}", selected_a.get()))
                .font_size(12.0).color(Color::from_argb(200, 0, 120, 0))
                .modifier(Modifier::new().padding(4.0))
                .build(ctx);

            // ── Container B ──
            let sb = selected_b.clone();
            let all_b_text = all_b.clone();
            SelectionContainer::new()
                .modifier(Modifier::new()
                    .fill_max_width()
                    .padding(10.0)
                    .background(Color::from_argb(30, 200, 150, 200), Shape::rounded(8.0)))
                .on_selection_change(move |start, end| {
                    let s = start.min(end); let e = start.max(end);
                    let txt = all_b_text.get(s..e).unwrap_or(""); // 字节安全切片（emoji 多字节时 get 防越界）
                    sb.set(format!("B: \"{}\"", txt));
                })
                .build(ctx, |ctx| {
                    Column::new().build(ctx, |ctx| {
                        Text::new("[B] ").font_size(12.0).color(Color::from_argb(200, 150, 0, 150)).build(ctx);
                        Text::new(a1).font_size(15.0).modifier(Modifier::new().padding(2.0)).build(ctx);
                        Text::new(a2).font_size(14.0).color(Color::from_argb(200, 80, 80, 80)).modifier(Modifier::new().padding(2.0)).build(ctx);
                    });
                });

            Text::new(format!("Container B: {}", selected_b.get()))
                .font_size(12.0).color(Color::from_argb(200, 120, 0, 120))
                .modifier(Modifier::new().padding(4.0))
                .build(ctx);
        });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    app::run_app(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(520.0, 720.0)
                .title("Text Selection Demo")
                .build(ctx, selection_ui);
        });
    });
}
