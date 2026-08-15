//! 反向列表演示（对齐 Compose `reverseLayout` + `animateScrollToItem`）
//!
//! 展示：
//! - `reverse_layout(true)`：index 0 在底部（聊天风格——最新消息在底部）
//! - `animate_scroll_to_item`：spring 动画滚动（对齐 Compose animateScrollToItem）
//! - `scroll_to_item` 立即跳转（反向时项**底**贴视口底）
//!
//! 运行：cargo run -p winia --example reverse_list_demo

use winia::prelude::*;
use std::sync::Arc;

const MESSAGE_COUNT: usize = 200;

#[derive(Clone)]
struct Message {
    id: u64,
    text: String,
}

#[composable]
fn reverse_demo(ctx: &mut ComposeCtx) {
    // 200 条消息（index 0 = 最旧——列表从底部向上排，offset=0 显示最新在底部）
    let messages = ctx.remember(|| {
        let v: Vec<Message> = (0..MESSAGE_COUNT)
            .map(|i| Message {
                id: i as u64,
                text: format!("消息 {}：这是第 {} 条消息的内容", i, i),
            })
            .collect();
        Arc::new(v)
    }).get();
    let state = ctx.remember(|| LazyListState::new()).get();

    Column::new()
        .modifier(Modifier::new().fill_max_size())
        .build(ctx, |ctx| {
            Text::new("reverseLayout 演示（最新消息在底部）")
                .font_size(20.0)
                .modifier(Modifier::new().padding(8.0))
                .build(ctx);
            Text::new("滚轮/拖拽：向上滚看更旧的消息；滚动方向与正向一致")
                .font_size(12.0)
                .color(Color::from_argb(255, 120, 120, 120))
                .modifier(Modifier::new().padding(8.0))
                .build(ctx);
            Text::new(format!(
                "firstVisible = {} (offset {:.0})",
                state.first_visible(),
                state.offset()
            ))
            .font_size(12.0)
            .color(Color::from_argb(255, 120, 120, 120))
            .modifier(Modifier::new().padding(8.0))
            .build(ctx);

            Row::new()
                .modifier(Modifier::new().padding(8.0))
                .build(ctx, |ctx| {
                    let s0 = state.clone();
                    Button::text()
                        .on_click(move || s0.scroll_to_item(0, 0.0))
                        .build(ctx, |ctx| Text::new("最新（底部）").build(ctx));
                    let s1 = state.clone();
                    Button::text()
                        .on_click(move || s1.animate_scroll_to_item(50, 0.0))
                        .build(ctx, |ctx| Text::new("动画到 #50").build(ctx));
                    let s2 = state.clone();
                    Button::text()
                        .on_click(move || s2.animate_scroll_to_item(MESSAGE_COUNT - 1, 0.0))
                        .build(ctx, |ctx| Text::new("动画到最旧").build(ctx));
                });

            LazyColumn::new()
                .state(state.clone())
                .reverse_layout(true)
                .modifier(Modifier::new().fill_max_width().fill_max_height())
                .items_from(messages, |m: &Message| m.id, |ctx, _i, m| {
                    Row::new()
                        .modifier(Modifier::new().fill_max_width().padding(12.0))
                        .build(ctx, |ctx| {
                            Text::new(format!("#{}", m.id))
                                .font_size(12.0)
                                .color(Color::from_argb(255, 0x67, 0x50, 0xA4))
                                .modifier(Modifier::new().width(48.0))
                                .build(ctx);
                            Text::new(m.text.as_str())
                                .font_size(14.0)
                                .build(ctx);
                        });
                })
                .build(ctx);
        });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();

    winia::run_app!(|ctx| {
        WiniaTheme::light(ctx, |ctx| {
            Window::new()
                .size(520.0, 600.0)
                .title("reverseLayout Demo")
                .build(ctx, reverse_demo);
        });
    });
}
