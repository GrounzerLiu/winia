//! 手势/指针事件测试示例
//! 测试 Compose 风格 click 检测 + on_pointer_event 完整生命周期

use winia::core::composer::ComposeCtx;
use winia::composable;
use winia::modifier::{Modifier, Color, Dimension, PointerEvent, PointerEventType};
use winia::ui::text::Text;
use winia::ui::Window;
use winia::ui::theme::WiniaTheme;
use winia::ui::Column;
use winia::ui::button::{Button, ButtonStyle};
use winia::app;

#[composable]
fn gesture_ui(ctx: &mut ComposeCtx) {
        // ── 状态 ──
        let count = ctx.remember(|| 0i32);
        let last = ctx.remember(|| String::new());
        let drag = ctx.remember(|| (0.0f32, 0.0f32));

        Column::new()
            .modifier(Modifier::new().fill_max_size().padding(16.0))
            .build(ctx, |ctx| {
                // 1. Button（Compose 风格 click —— Down→Up 配对 + slop）
                Button::new().on_click({
                    let c = count.clone();
                    let l = last.clone();
                    move || {
                        c.update(|v| *v -= 1);
                        l.set(format!("click at #{}", c.get()));
                    }
                })
                .style(ButtonStyle::Filled)
                .modifier(Modifier::new().size(160.0, 36.0))
                .build(ctx, |ctx| {
                    Text::new("-1").build(ctx);
                });

                // 2. on_pointer_event 模拟 click（只响应 Up）
                Text::new("Click me (pointer event)")
                    .modifier(
                        Modifier::new()
                            .padding(8.0)
                            .background(Color::from_argb(40, 100, 149, 237), winia::modifier::Shape::rounded(4.0))
                            .on_pointer_event({
                                let c = count.clone();
                                let l = last.clone();
                                move |e: &PointerEvent| {
                                    if matches!(e.event_type, PointerEventType::Up) {
                                        c.update(|v| *v += 1);
                                        l.set(format!("pointer Up at #{}", c.get()));
                                        true
                                    } else { false }
                                }
                            })
                    )
                    .build(ctx);

                // 显示计数
                Text::new(format!("Count: {}", count.get()))
                    .font_size(24.0)
                    .modifier(Modifier::new().padding(4.0))
                    .build(ctx);

                // 显示上次操作
                Text::new(format!("Last: {}", last.get()))
                    .font_size(14.0)
                    .color(Color::from_argb(200, 128, 128, 128))
                    .build(ctx);

                // 3. on_pointer_event 完整 Down/Up/Move 测试
                Text::new("Pointer event test (drag)")
                    .modifier(
                        Modifier::new()
                            .padding(12.0)
                            .size(200.0, Dimension::Auto)
                            .background(Color::from_argb(60, 200, 200, 80), winia::modifier::Shape::rounded(6.0))
                            .on_pointer_event({
                                let l = last.clone();
                                let d = drag.clone();
                                move |e: &PointerEvent| {
                                    match e.event_type {
                                        PointerEventType::Down => {
                                            l.set(format!("Down at ({:.0},{:.0})", e.position.0, e.position.1));
                                            true
                                        }
                                        PointerEventType::Move => {
                                            d.set((e.position.0, e.position.1));
                                            true
                                        }
                                        PointerEventType::Up => {
                                            l.set(format!("Up at ({:.0},{:.0})", e.position.0, e.position.1));
                                            true
                                        }
                                        _ => false,
                                    }
                                }
                            })
                    )
                    .build(ctx);

                Text::new(format!("Drag pos: ({:.0},{:.0})", drag.get().0, drag.get().1))
                    .font_size(12.0)
                    .color(Color::from_argb(180, 80, 80, 80))
                    .build(ctx);
            });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    app::run_app(winia::app_root!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(480.0, 620.0)
                .title("Gesture Demo")
                .build(ctx, gesture_ui);
        });
    }));
}
