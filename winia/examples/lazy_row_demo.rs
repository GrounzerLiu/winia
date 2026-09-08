//! LazyRow 演示（水平懒列表——Compose foundation lazy LazyRow）
//!
//! 展示：
//! - 1000 项横向列表（懒加载：只组合可见项）
//! - 稳定 key 迭代（items_from：key = 数据 id）
//! - 横向拖拽滚动 + 松手惯性 fling（水平滚轮 Shift+滚轮 或 touchpad 横向滚动）

use letclone::clone;
use winia::prelude::*;
use std::sync::Arc;

#[derive(Clone)]
struct Item {
    id: u64,
    name: String,
    hue: u32,
}

fn chip(ctx: &mut ComposeCtx, name: &str) {
    let color = Color::from_argb(255, 0x67, 0x50, 0xA4);
    Text::new(name)
        .font_size(13.0)
        .color(Color::WHITE)
        .modifier(Modifier::new()
            .width(96.0)
            .padding(8.0)
            .background(color, Shape::rounded(8.0))
            .clip(Shape::rounded(8.0)))
        .build(ctx);
}

#[composable]
fn lazy_row_demo(ctx: &mut ComposeCtx) {
    let items = ctx.remember(|| {
        let v: Vec<Item> = (0..1000)
            .map(|i| Item { id: i as u64, name: format!("Item {}", i), hue: (i % 12) as u32 })
            .collect();
        Arc::new(v)
    }).get();
    let state = ctx.remember(|| LazyListState::new()).get();

    Column::new()
        .modifier(Modifier::new().fill_max_size())
        .build(ctx, |ctx| {
            Text::new("LazyRow 演示（1000 项横向懒加载）")
                .font_size(20.0)
                .modifier(Modifier::new().padding(8.0))
                .build(ctx);
            Text::new("横向拖拽滚动 + 松手惯性 fling（touchpad 横向滚轮）")
                .font_size(12.0)
                .color(Color::from_argb(255, 120, 120, 120))
                .modifier(Modifier::new().padding(8.0))
                .build(ctx);
            Text::new(format!("firstVisible = {} (offset {:.0})", state.first_visible(), state.offset()))
                .font_size(12.0)
                .color(Color::from_argb(255, 120, 120, 120))
                .modifier(Modifier::new().padding(8.0))
                .build(ctx);

            Row::new()
                .modifier(Modifier::new().padding(8.0))
                .build(ctx, |ctx| {
                    Button::text()
                        .on_click({ clone!(state); move || state.scroll_to_item(0, 0.0) })
                        .build(ctx, |ctx| Text::new("最左").build(ctx));
                    Button::text()
                        .on_click({ clone!(state); move || state.scroll_to_item(500, 0.0) })
                        .build(ctx, |ctx| Text::new("跳转 500").build(ctx));
                    Button::text()
                        .on_click({ clone!(state); move || state.scroll_to_item(999, 0.0) })
                        .build(ctx, |ctx| Text::new("末尾").build(ctx));
                });

            // LazyRow：稳定 key 迭代（与 LazyColumn 同一 LazyListState/机制）
            LazyRow::new()
                .state(state.clone())
                .spacing(8.0)
                .modifier(Modifier::new().fill_max_width().fill_max_height())
                .items_from(
                    items.clone(),
                    |it: &Item| it.id,          // 稳定 key = id
                    move |ctx, _i, it| chip(ctx, &it.name),
                )
                .build(ctx);
        });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::light(ctx, |ctx| {
            Window::new()
                .size(520.0, 360.0)
                .title("LazyRow Demo")
                .build(ctx, lazy_row_demo);
        });
    });
}
