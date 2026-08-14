//! LazyColumn 演示（material3 对齐——Compose foundation lazy）
//!
//! 展示：
//! - 1000 项列表（懒加载：只组合可见项）
//! - 稳定 key 迭代（items_from：数据前部增删后滚动位置按 key 保持）
//! - item / items / items_plain 混合

use winia::prelude::*;
use std::sync::Arc;

fn item_row(ctx: &mut ComposeCtx, text: &str, index: usize) {
    Text::new(text)
        .font_size(14.0)
        .color(if index % 2 == 0 { Color::from_argb(255, 40, 40, 40) } else { Color::from_argb(255, 90, 90, 90) })
        .modifier(Modifier::new().padding(12.0).fill_max_width())
        .build(ctx);
}

#[derive(Clone)]
struct Item {
    id: u64,
    name: String,
}

#[composable]
fn lazy_demo(ctx: &mut ComposeCtx) {
    // 1000 项数据（id 稳定）
    let items = ctx.remember(|| {
        let v: Vec<Item> = (0..1000)
            .map(|i| Item { id: i as u64, name: format!("Item {}", i) })
            .collect();
        Arc::new(v)
    }).get();
    let state = ctx.remember(|| LazyListState::new()).get();

    Column::new()
        .modifier(Modifier::new().fill_max_size())
        .build(ctx, |ctx| {
            Text::new("LazyColumn 演示（1000 项懒加载）")
                .font_size(20.0)
                .modifier(Modifier::new().padding(8.0))
                .build(ctx);
            Text::new("拖拽滚动 + 松手惯性 fling（滚轮离散滚动）")
                .font_size(12.0)
                .color(Color::from_argb(255, 120, 120, 120))
                .modifier(Modifier::new().padding(8.0))
                .build(ctx);
            Text::new(format!("firstVisible = {} (offset {:.0})", state.first_visible(), state.offset()))
                .font_size(12.0)
                .color(Color::from_argb(255, 120, 120, 120))
                .modifier(Modifier::new().padding(8.0))
                .build(ctx);

            // 程序化滚动：scroll_to_item（锚点权威——对齐 Compose scrollToItem，
            // 不需要高度缓存；越界 clamp 在测量期）
            Row::new()
                .modifier(Modifier::new().padding(8.0))
                .build(ctx, |ctx| {
                    let s0 = state.clone();
                    Button::text()
                        .on_click(move || s0.scroll_to_item(0, 0.0))
                        .build(ctx, |ctx| Text::new("顶部").build(ctx));
                    let s1 = state.clone();
                    Button::text()
                        .on_click(move || s1.scroll_to_item(500, 0.0))
                        .build(ctx, |ctx| Text::new("跳转 500").build(ctx));
                    let s2 = state.clone();
                    Button::text()
                        .on_click(move || s2.scroll_to_item(999, 0.0))
                        .build(ctx, |ctx| Text::new("末尾").build(ctx));
                });

            // LazyColumn：稳定 key 迭代
            let items_clone = items.clone();
            LazyColumn::new()
                .state(state.clone())
                .modifier(Modifier::new().fill_max_width().fill_max_height())
                .items_from(
                    items_clone,
                    |it: &Item| it.id,          // 稳定 key = id
                    move |ctx, i, it| item_row(ctx, &it.name, i),
                )
                .build(ctx);
        });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();

    winia::run_app!(|ctx| {
        WiniaTheme::light(ctx, |ctx| {
            Window::new()
                .size(420.0, 620.0)
                .title("LazyColumn Demo")
                .build(ctx, lazy_demo);
        });
    });
}
