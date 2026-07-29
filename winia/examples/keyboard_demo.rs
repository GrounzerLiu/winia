//! 键盘事件演示 — onKeyEvent / onPreviewKeyEvent / 修饰键
//!
//! 使用 Tab 切换焦点，焦点的按键日志显示在屏幕上。

use winia::prelude::*;
use winia::app;
use winit::keyboard::{Key, NamedKey};

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    app::run_app(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(520.0, 500.0)
                .title("Keyboard Events Demo")
                .build(ctx, |ctx| keyboard_demo_ui(ctx));
        });
    });
}

fn keyboard_demo_ui(ctx: &mut ComposeCtx) {
    let log = ctx.remember(|| String::new());

    Column::new()
        .modifier(Modifier::new().fill_max_width().fill_max_height().padding(16.0))
        .spacing(12.0)
        .build(ctx, |ctx| {

            Text::new("■ Key Event Demo — Tab to switch focus, type keys")
                .font_size(15.0).color(Color::from_argb(255, 100, 100, 100)).build(ctx);

            // ═══ 第一个可焦点节点（响应所有按键）═══
            Text::new("▶ Node 1: captures all keys")
                .font_size(14.0)
                .modifier(Modifier::new()
                    .fill_max_width()
                    .padding(10.0)
                    .background(Color::from_argb(18, 0, 0, 0), Shape::rounded(6.0))
                    .focusable()
                    .on_key_event({
                        let log = log.clone();
                        move |e| {
                            let prev = log.get();
                            let new = format!("{}Node1: {:?} (Ctrl:{}, Shift:{})\n", prev, e.key, e.is_ctrl_pressed, e.is_shift_pressed);
                            let lines: Vec<&str> = new.lines().collect();
                            let last5 = lines.iter().rev().take(5).cloned().collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
                            log.set(last5 + "\n");
                            true
                        }
                    }))
                .build(ctx);

            // ═══ 第二个可焦点节点（只响应 Enter）═══
            Text::new("▶ Node 2: only Enter")
                .font_size(14.0)
                .modifier(Modifier::new()
                    .fill_max_width()
                    .padding(10.0)
                    .background(Color::from_argb(18, 0, 0, 0), Shape::rounded(6.0))
                    .focusable()
                    .on_key_event({
                        let log = log.clone();
                        move |e| {
                            if e.key == Key::Named(NamedKey::Enter) {
                                let prev = log.get();
                                log.set(format!("{}Node2: Enter pressed!\n", prev));
                                true
                            } else { false }
                        }
                    }))
                .build(ctx);

            // ═══ onPreviewKeyEvent 示例（拦截 Ctrl+S）═══
            Text::new("▶ Ctrl+S: captured by preview")
                .font_size(14.0)
                .modifier(Modifier::new()
                    .fill_max_width()
                    .padding(10.0)
                    .background(Color::from_argb(18, 0, 0, 0), Shape::rounded(6.0))
                    .focusable()
                    .on_pre_key_event({
                        let log = log.clone();
                        move |e| {
                            if e.is_ctrl_pressed && e.key == Key::Character("s".into()) {
                                let prev = log.get();
                                log.set(format!("{}Ctrl+S intercepted (preview)!\n", prev));
                                true
                            } else { false }
                        }
                    }))
                .build(ctx);

            // ═══ 按键日志显示 ═══
            Text::new("■ Event Log (last 5)")
                .font_size(15.0).color(Color::from_argb(255, 100, 100, 100)).build(ctx);

            Text::new("")
                .font_size(12.0)
                .color(Color::from_argb(255, 200, 200, 200))
                .modifier(Modifier::new()
                    .fill_max_width()
                    .height(120.0)
                    .padding(8.0)
                    .background(Color::from_argb(30, 255, 255, 255), Shape::rounded(4.0)))
                .build(ctx);

            // 显示日志
            Text::new(log.get())
                .font_size(12.0)
                .color(Color::from_argb(255, 180, 180, 180))
                .modifier(Modifier::new().fill_max_width().padding(8.0))
                .build(ctx);

            Text::new("Tip: Tab to cycle focus, Type keys to see events, Ctrl+S to test preview")
                .font_size(11.0).color(Color::from_argb(255, 120, 120, 120))
                .modifier(Modifier::new().padding(4.0))
                .build(ctx);
        });
}
