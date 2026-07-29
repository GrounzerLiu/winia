//! 键盘事件演示 — onKeyEvent / onPreviewKeyEvent / 修饰键
//!
//! 使用 Tab 切换焦点。按键日志通过 RefCell 收集，不触发重组。

use winia::prelude::*;
use winia::app;
use winit::keyboard::{Key, NamedKey};
use std::cell::RefCell;

thread_local! {
    static KEY_LOG: RefCell<String> = const { RefCell::new(String::new()) };
}

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
    // 从 thread-local 读取当前日志
    let log_display = KEY_LOG.with(|l| l.borrow().clone());

    Column::new()
        .modifier(Modifier::new().fill_max_width().fill_max_height().padding(16.0))
        .spacing(12.0)
        .build(ctx, |ctx| {

            Text::new("■ Key Event Demo — Tab to switch focus, type keys")
                .font_size(15.0).color(Color::from_argb(255, 100, 100, 100)).build(ctx);

            // ═══ 第一个焦点节点（响应所有按键）═══
            Text::new("▶ Node 1: captures all keys")
                .font_size(14.0)
                .modifier(Modifier::new()
                    .fill_max_width()
                    .padding(10.0)
                    .background(Color::from_argb(18, 0, 0, 0), Shape::rounded(6.0))
                    .focusable()
                    .on_key_event(move |e| {
                        KEY_LOG.with(|l| {
                            let mut log = l.borrow_mut();
                            log.insert_str(0, &format!("Node1: {:?} (C:{},S:{})\n", e.key, e.is_ctrl_pressed, e.is_shift_pressed));
                            if log.lines().count() > 5 {
                                if let Some(pos) = log.rfind('\n') {
                                    log.truncate(pos);
                                }
                            }
                        });
                        true
                    }))
                .build(ctx);

            // ═══ 第二个焦点节点（只响应 Enter）═══
            Text::new("▶ Node 2: only Enter")
                .font_size(14.0)
                .modifier(Modifier::new()
                    .fill_max_width()
                    .padding(10.0)
                    .background(Color::from_argb(18, 0, 0, 0), Shape::rounded(6.0))
                    .focusable()
                    .on_key_event(move |e| {
                        if e.key == Key::Named(NamedKey::Enter) {
                            KEY_LOG.with(|l| l.borrow_mut().insert_str(0, "Node2: Enter!\n"));
                            true
                        } else { false }
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
                    .on_pre_key_event(move |e| {
                        if e.is_ctrl_pressed && e.key == Key::Character("s".into()) {
                            KEY_LOG.with(|l| l.borrow_mut().insert_str(0, "Ctrl+S intercepted!\n"));
                            true
                        } else { false }
                    }))
                .build(ctx);

            // ═══ 按键日志显示（用 State 仅在显示时触发重组）═══
            Text::new("■ Event Log (last 5)")
                .font_size(15.0).color(Color::from_argb(255, 100, 100, 100)).build(ctx);

            Text::new(&log_display)
                .font_size(12.0)
                .color(Color::from_argb(255, 180, 180, 180))
                .modifier(Modifier::new()
                    .fill_max_width()
                    .padding(8.0)
                    .background(Color::from_argb(30, 255, 255, 255), Shape::rounded(4.0)))
                .build(ctx);

            Text::new("Tip: Tab to cycle focus, Type keys to see events, Ctrl+S to test preview")
                .font_size(11.0).color(Color::from_argb(255, 120, 120, 120))
                .modifier(Modifier::new().padding(4.0))
                .build(ctx);
        });
}
