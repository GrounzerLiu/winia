//! Taskbar demo - one shared window declaration with Windows-only shell behavior.
//!
//! On Windows the window is an app bar, skips the taskbar, and uses the tool-window style.
//! Other platforms accept the same builder chain and show a normal window; always-on-top is used where supported.
//!
//! Run with: cargo run -p winia --example taskbar_demo
//! Pass --hidden to verify that an app-bar request does not reserve work area before first show.

use winia::components::Button;
use winia::prelude::*;

pub fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| app(ctx));
    });
}

#[composable]
fn app(ctx: &mut ComposeCtx) {
    let hidden = std::env::args().any(|argument| argument == "--hidden");
    let taskbar_id = ctx.remember(|| None::<WindowId>);
    let status = ctx.remember(|| String::from("waiting for the platform window"));
    let body_id = taskbar_id.clone();
    let body_status = status.clone();
    let event_status = status.clone();

    let id = Window::new()
        .title("Winia taskbar demo")
        .size(960.0, 48.0)
        .visible(!hidden)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .tool_window(true)
        .app_bar(AppBarOptions::bottom(48.0))
        .on_event(move |_, event| {
            if let WindowEvent::AppBarStatusChanged(value) = event {
                event_status.set(format!("app bar: {value:?}"));
            }
        })
        .build(ctx, move |ctx| {
            let click_id = body_id.clone();
            Row::new()
                .spacing(12.0)
                .modifier(
                    Modifier::new()
                        .fill_max_size()
                        .padding_sides(12.0, 4.0, 12.0, 4.0)
                        .background(Color::from_argb(255, 0x1E, 0x1E, 0x1E), Shape::Rectangle),
                )
                .build(ctx, |ctx| {
                    Text::new("Winia taskbar")
                        .font_size(16.0)
                        .color(Color::WHITE)
                        .build(ctx);
                    Text::new(body_status.get())
                        .font_size(12.0)
                        .color(Color::from_argb(255, 0xB8, 0xB8, 0xB8))
                        .build(ctx);
                    Spacer::new()
                        .modifier(Modifier::new().layout_weight(1.0))
                        .build(ctx);
                    Button::filled()
                        .min_size(72.0, 32.0)
                        .content_padding((12.0, 4.0, 12.0, 4.0))
                        .on_click(move || {
                            if let Some(id) = click_id.get() {
                                id.destroy();
                            }
                        })
                        .build(ctx, |ctx| {
                            Text::new("Exit").build(ctx);
                        });
                });
        });
    taskbar_id.set(Some(id));
    status.set(format!("app bar: {:?}", id.state().app_bar_status()));
}
