//! BottomSheetScaffold 演示——可拖动显示更多内容（对标 Compose BottomSheetScaffold）
//!
//! 验证：
//! - 常驻底部片，peek 56dp 露头
//! - 上拖展开显示更多，下拖回 peek
//! - 顶部圆角 16dp，底部直角
//! - 拖动手柄
//!
//! 运行：cargo run -p winia --example bottom_sheet_scaffold_demo --features debug-server

use letclone::clone;
use winia::prelude::*;
use std::sync::Arc;

#[derive(Clone)]
struct ScaffoldItem {
    id: u64,
    title: String,
}

#[composable]
fn scaffold_demo(ctx: &mut ComposeCtx) {
    // 外部 SheetState 可用于程序化控制（按钮展开/收起）
    let scaffold_state = ctx.remember(|| SheetState::new(SheetValue::PartiallyExpanded)).get();
    let lazy_state = ctx.remember(|| LazyListState::new()).get();
    let items = ctx
        .remember(|| {
            let v: Vec<ScaffoldItem> = (0..120)
                .map(|i| ScaffoldItem { id: i as u64, title: format!("Scaffold 列表项 {}", i) })
                .collect();
            Arc::new(v)
        })
        .get();

    BottomSheetScaffold::new()
        .sheet_state(scaffold_state.clone())
        .sheet_peek_height(96.dp())
        .sheet_drag_handle(true)
        .build(
            ctx,
            {
                clone!(scaffold_state, lazy_state, items);
                move |ctx| {
                    // Fixed header + LazyColumn list (virtualized, 120 items).
                    Column::new()
                        .modifier(Modifier::new().fill_max_width().padding(16.0))
                        .spacing(8.0)
                        .build(ctx, |ctx| {
                            Text::new("底部片内容（上拖展开，内部可单独滚动）").font_size(16.0).build(ctx);
                            // Fixed-height viewport (sheet height is anchor-driven;
                            // give the list a fixed height to enable virtualization).
                            // height 620 makes content ≈ viewport 720 → is_full fires
                            // on expand → corner 28→0 animation visible.
                            Stack::new()
                                .modifier(Modifier::new().fill_max_width().height(620.0).clip(Shape::RoundedRect { corner_radius: 12.0 }))
                                .build(ctx, |ctx| {
                                    LazyColumn::new()
                                        .state(lazy_state.clone())
                                        .modifier(Modifier::new().fill_max_width().fill_max_height())
                                        .items_from(items.clone(), |it: &ScaffoldItem| it.id, move |ctx, _i, it| {
                                            Text::new(it.title.clone())
                                                .font_size(13.0)
                                                .color(Color::from_argb(255, 90, 90, 90))
                                                .modifier(
                                                    Modifier::new()
                                                        .fill_max_width()
                                                        .padding(10.0)
                                                        .background(
                                                            Color::from_argb(255, 245, 245, 247),
                                                            Shape::RoundedRect { corner_radius: 10.0 },
                                                        ),
                                                )
                                                .build(ctx);
                                        })
                                        .build(ctx);
                                });
                            Row::new().spacing(8.0).build(ctx, |ctx| {
                                Button::text()
                                    .on_click({ clone!(scaffold_state); move || scaffold_state.expand() })
                                    .build(ctx, |ctx| Text::new("展开").build(ctx));
                                Button::text()
                                    .on_click({ clone!(scaffold_state); move || scaffold_state.partial_expand() })
                                    .build(ctx, |ctx| Text::new("收起").build(ctx));
                                Button::text()
                                    .on_click({ clone!(lazy_state); move || lazy_state.animate_scroll_to_item(80, 0.0) })
                                    .build(ctx, |ctx| Text::new("跳 80").build(ctx));
                            });
                        });
                }
            },
            |ctx| {
                // 主内容
                Column::new()
                    .modifier(Modifier::new().fill_max_size().padding(16.0))
                    .spacing(12.0)
                    .build(ctx, |ctx| {
                        Text::new("BottomSheetScaffold 演示").font_size(18.0).build(ctx);
                        Text::new("常驻底部片，peek 96dp，上拖显示更多内容（对标 Compose Standard BottomSheet）")
                            .font_size(12.0)
                            .color(Color::from_argb(255, 120, 120, 120))
                            .build(ctx);
                        Text::new("主内容区域（可放地图/列表）")
                            .font_size(14.0)
                            .build(ctx);
                    });
            },
        );
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(480.0, 720.0)
                .title("BottomSheetScaffold 演示")
                .build(ctx, |ctx| scaffold_demo(ctx));
        });
    });
}
