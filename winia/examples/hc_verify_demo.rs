//! 高度缓存方案 A 验证 demo（debug-server 交互驱动）
//!
//! 验证目标：LazyList 高度缓存按 item key 跟随——数据前部插入/重排后，
//! 滚动位置按 key 保持（原 first visible 项仍可见），高度不因 index 平移错位。
//!
//! 场景（配合 debug-server 操作）：
//! 1. 点「滚动到 50」→ firstVisible 应为 50（项 50 在视口顶）
//! 2. 点「前部插入 10 项」→ 原项 50 移到 index 60，但 firstVisible 应按 key
//!    校正到 60（仍显示原项 50 的文本「Item 50」），offset 应保持
//!    ≈ 60×48 = 2880（方案 A 前：高度按 index，offset 错位/项跳变）
//!
//! 运行：cargo run -p winia --example hc_verify_demo --features debug-server
//! 端口：WINIA_DEBUG_PORT（默认 9998）

use winia::prelude::*;
use std::sync::Arc;

#[derive(Clone, PartialEq)]
struct Item {
    id: u64,
    name: String,
}

#[composable]
fn hc_demo(ctx: &mut ComposeCtx) {
    // 可变数据（State<Arc<Vec<Item>>>）——「前部插入」按钮更新它
    let items_state = ctx.remember(|| State::new(Arc::new(make_items(0, 100)))).get();
    let state = ctx.remember(|| LazyListState::new()).get();
    // NEW 项起始 key（每次插入递增——避免重复 key 触发 rebuild panic）
    let new_key_base = ctx.remember(|| State::new(0u64)).get();
    // 高/矮项混合：让高度缓存有实际意义（48 / 96 交替）
    // 项高 = 48 + (id % 3 == 0) ? 48 : 0  → 每 3 项一项加倍高

    Column::new()
        .modifier(Modifier::new().fill_max_size())
        .build(ctx, |ctx| {
            Text::new("高度缓存方案 A 验证（debug-server）")
                .font_size(18.0)
                .modifier(Modifier::new().padding(8.0))
                .build(ctx);
            Text::new("步骤：滚动到 50 → 前部插入 10 项 → 观察 firstVisible 是否按 key 保持")
                .font_size(12.0)
                .color(Color::from_argb(255, 120, 120, 120))
                .modifier(Modifier::new().padding(8.0))
                .build(ctx);
            // 状态显示
            Text::new(format!(
                "total={} firstVisible={} offset={:.0}",
                items_state.get().len(),
                state.first_visible(),
                state.offset()
            ))
            .font_size(12.0)
            .color(Color::from_argb(255, 90, 120, 200))
            .modifier(Modifier::new().padding(8.0))
            .build(ctx);

            // 操作按钮
            Row::new()
                .modifier(Modifier::new().padding(8.0))
                .build(ctx, |ctx| {
                    let s0 = state.clone();
                    Button::text()
                        .on_click(move || s0.scroll_to_item(50, 0.0))
                        .build(ctx, |ctx| Text::new("滚动到 50").build(ctx));
                    let s1 = state.clone();
                    let items = items_state.clone();
                    let nkb = new_key_base.clone();
                    Button::text()
                        .on_click(move || {
                            // 前部插入 10 项（key 递增保证唯一）——原项后移，key 不变
                            let cur = items.get();
                            let base = nkb.get();
                            let mut v: Vec<Item> = Vec::with_capacity(cur.len() + 10);
                            for k in 0..10 {
                                let id = 1000 + base * 10 + k;
                                v.push(Item { id, name: format!("NEW{}", id) });
                            }
                            for it in cur.iter() { v.push(it.clone()); }
                            items.set(Arc::new(v));
                            nkb.set(base + 1);
                        })
                        .build(ctx, |ctx| Text::new("前部插入 10 项").build(ctx));
                    let s2 = state.clone();
                    let items = items_state.clone();
                    let nkb = new_key_base.clone();
                    Button::text()
                        .on_click(move || {
                            // 重置数据（回 100 项）——滚动回顶部便于重复验证
                            let mut v: Vec<Item> = Vec::with_capacity(100);
                            for i in 0..100 { v.push(Item { id: i as u64, name: format!("Item {}", i) }); }
                            items.set(Arc::new(v));
                            nkb.set(0);
                            s2.scroll_to_item(0, 0.0);
                        })
                        .build(ctx, |ctx| Text::new("重置").build(ctx));
                });

            // LazyColumn：稳定 key 迭代（key = id）
            let items = items_state.get();
            LazyColumn::new()
                .state(state.clone())
                .modifier(Modifier::new().fill_max_width().fill_max_height())
                .items_from(
                    items.clone(),
                    |it: &Item| it.id,
                    move |ctx, _i, it| {
                        // 高度：id % 3 == 0 的项加倍高（让高度缓存有区分度）
                        let h = if it.id % 3 == 0 { 96.0 } else { 48.0 };
                        Text::new(format!("{} (h={:.0})", it.name, h))
                            .font_size(14.0)
                            .modifier(Modifier::new().padding(12.0).size(h, h))
                            .build(ctx);
                    },
                )
                .build(ctx);
        });
}

fn make_items(start: u64, count: u64) -> Vec<Item> {
    (0..count).map(|k| Item { id: start + k, name: format!("Item {}", start + k) }).collect()
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(480.0, 640.0)
                .title("Height Cache Verify (方案 A)")
                .build(ctx, |ctx| hc_demo(ctx));
        });
    });
}
