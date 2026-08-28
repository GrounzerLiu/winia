//! 方向一（measure/build convergence）验证 demo
//!
//! 验证：LazyColumn build 感知真实视口后，视口放大（>2000px）时底部可见项
//! 被组合覆盖（不再漏项）。用 State 控制容器高度模拟窗口 resize——
//! 切到 2500 高时，原固定 2000 估算窗会让底部 ~500px 空白；方向一修复后
//! build 读真实视口，底部项被组合渲染。
//!
//! 运行：cargo run -p winia --example hc_conv_demo --features debug-server
//! 端口：WINIA_DEBUG_PORT（默认 9998）
//! 操作：点「视口 400」/「视口 2500」切换，抓树/截图观察可见项覆盖。

use winia::prelude::*;
use std::sync::Arc;

#[derive(Clone, PartialEq)]
struct Item {
    id: u64,
    name: String,
}

#[composable]
fn conv_demo(ctx: &mut ComposeCtx) {
    let viewport_h = ctx.remember(|| State::new(400.0f32)).get();
    let state = ctx.remember(|| LazyListState::new()).get();

    Column::new()
        .modifier(Modifier::new().fill_max_size())
        .build(ctx, |ctx| {
            Text::new("方向一：build 感知真实视口（measure/build convergence）")
                .font_size(16.0)
                .modifier(Modifier::new().padding(8.0))
                .build(ctx);
            Text::new(format!(
                "viewport_h={:.0} firstVisible={} offset={:.0}",
                viewport_h.get(),
                state.first_visible(),
                state.offset()
            ))
            .font_size(12.0)
            .color(Color::from_argb(255, 90, 120, 200))
            .modifier(Modifier::new().padding(8.0))
            .build(ctx);
            Row::new()
                .modifier(Modifier::new().padding(8.0))
                .build(ctx, |ctx| {
                    let v = viewport_h.clone();
                    Button::text()
                        .on_click(move || v.set(400.0))
                        .build(ctx, |ctx| Text::new("视口 400").build(ctx));
                    let v = viewport_h.clone();
                    Button::text()
                        .on_click(move || v.set(2500.0))
                        .build(ctx, |ctx| Text::new("视口 2500").build(ctx));
                });

            // LazyColumn 直接固定尺寸（有限约束——measure 写回真实视口，模拟 resize）
            let vh = viewport_h.get();
            let items: Arc<Vec<Item>> = (0..200)
                .map(|i| Item { id: i as u64, name: format!("Item {}", i) })
                .collect::<Vec<_>>()
                .into();
            LazyColumn::new()
                .state(state.clone())
                .modifier(Modifier::new().fill_max_width().size(vh, vh))
                .items_from(items.clone(), |it: &Item| it.id, move |ctx, _i, it| {
                    Text::new(format!("{}", it.name))
                        .font_size(14.0)
                        .modifier(Modifier::new().padding(12.0))
                        .build(ctx);
                })
                .build(ctx);
        });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(480.0, 900.0)
                .title("方向一 convergence 验证")
                .build(ctx, |ctx| conv_demo(ctx));
        });
    });
}
