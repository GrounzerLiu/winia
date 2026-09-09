//! 手势/指针事件测试示例
//! 测试 Compose 风格 click 检测 + on_pointer_event 完整生命周期

use letclone::clone;
use winia::core::composer::ComposeCtx;
use winia::composable;
use winia::modifier::{Modifier, Color, Dimension, PointerEvent, PointerEventType};
use winia::ui::text::Text;
use winia::ui::Window;
use winia::ui::theme::WiniaTheme;
use winia::ui::Column;
use winia::ui::layout_components::Row;
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
                    clone!(count, last);
                    move || {
                        count.update(|v| *v -= 1);
                        last.set(format!("click at #{}", count.get()));
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
                                clone!(count, last);
                                move |e: &PointerEvent| {
                                    if matches!(e.event_type, PointerEventType::Up) {
                                        count.update(|v| *v += 1);
                                        last.set(format!("pointer Up at #{}", count.get()));
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
                                clone!(last, drag);
                                move |e: &PointerEvent| {
                                    match e.event_type {
                                        PointerEventType::Down => {
                                            last.set(format!("Down at ({:.0},{:.0})", e.position.0, e.position.1));
                                            true
                                        }
                                        PointerEventType::Move => {
                                            drag.set((e.position.0, e.position.1));
                                            true
                                        }
                                        PointerEventType::Up => {
                                            last.set(format!("Up at ({:.0},{:.0})", e.position.0, e.position.1));
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

                // ── 4. 手势：tap / double-tap / long-press（对标 detectTapGestures） ──
                let tap_info = ctx.remember(|| String::from("等待手势…"));
                Text::new("Tap / Double / Long-press 手势区")
                    .modifier(
                        Modifier::new()
                            .padding(12.0)
                            .size(240.0, Dimension::Auto)
                            .background(Color::from_argb(80, 156, 39, 176), winia::modifier::Shape::rounded(6.0))
                            .on_press({ clone!(tap_info); move |p: (f32, f32)| {
                                tap_info.set(format!("press at ({:.0},{:.0})", p.0, p.1));
                            } })
                            .on_tap({ clone!(tap_info); move |p: (f32, f32)| {
                                tap_info.set(format!("tap at ({:.0},{:.0})", p.0, p.1));
                            } })
                            .on_double_tap({ clone!(tap_info); move |p: (f32, f32)| {
                                tap_info.set(format!("double-tap at ({:.0},{:.0})", p.0, p.1));
                            } })
                            .on_long_press({ clone!(tap_info); move |p: (f32, f32)| {
                                tap_info.set(format!("long-press at ({:.0},{:.0})", p.0, p.1));
                            } }),
                    )
                    .build(ctx);
                Text::new(format!("手势: {}", tap_info.get()))
                    .font_size(12.0)
                    .color(Color::from_argb(180, 80, 80, 80))
                    .build(ctx);

                // ── 5. 手势：drag（方块跟随——对标 detectDragGestures） ──
                let drag_x = ctx.remember(|| 0.0f32);
                let drag_y = ctx.remember(|| 0.0f32);
                let drag_state = ctx.remember(|| String::from("未拖拽"));
                Row::new()
                    .modifier(Modifier::new().padding_vertical(12.0))
                    .build(ctx, |ctx| {
                        Column::new()
                            .modifier(
                                Modifier::new()
                                    .size(70.0, 70.0)
                                    .offset(drag_x.clone(), drag_y.clone())
                                    .background(Color::from_argb(255, 255, 87, 34), winia::modifier::Shape::rounded(8.0))
                                    .on_drag_start({ clone!(drag_state); move |_p: (f32, f32)| {
                                        drag_state.set(String::from("拖拽开始"));
                                    } })
                                    .on_drag({
                                        clone!(drag_x, drag_y);
                                        move |_p: (f32, f32), delta: (f32, f32)| {
                                            drag_x.update(|v| *v += delta.0);
                                            drag_y.update(|v| *v += delta.1);
                                        }
                                    })
                                    .on_drag_end({ clone!(drag_state); move || {
                                        drag_state.set(String::from("拖拽结束"));
                                    } })
                                    .on_drag_cancel({ clone!(drag_state); move || {
                                        drag_state.set(String::from("拖拽取消"));
                                    } }),
                            )
                            .build(ctx, |ctx| {
                                Text::new("拖我")
                                    .font_size(12.0)
                                    .color(Color::WHITE)
                                    .build(ctx);
                            });
                    });
                Text::new(format!("drag: {} (pos {:.0},{:.0})", drag_state.get(), drag_x.get(), drag_y.get()))
                    .font_size(12.0)
                    .color(Color::from_argb(180, 80, 80, 80))
                    .build(ctx);
            });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(480.0, 620.0)
                .title("Gesture Demo")
                .build(ctx, gesture_ui);
        });
    });
}
