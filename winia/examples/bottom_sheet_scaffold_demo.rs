//! BottomSheetScaffold 演示——可拖动显示更多内容（对标 Compose BottomSheetScaffold）
//!
//! 验证：
//! - 常驻底部片，peek 56dp 露头
//! - 上拖展开显示更多，下拖回 peek
//! - 顶部圆角 16dp，底部直角
//! - 拖动手柄
//!
//! 运行：cargo run -p winia --example bottom_sheet_scaffold_demo --features debug-server

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
    let s1 = scaffold_state.clone();
    let s2 = scaffold_state.clone();
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
                let s1 = s1.clone();
                let s2 = s2.clone();
                let st = lazy_state.clone();
                let sheet_items = items.clone();
                move |ctx| {
                    // 头部固定 + LazyColumn 列表（虚拟滚动，120 项）
                    Column::new()
                        .modifier(Modifier::new().fill_max_width().padding(16.0))
                        .spacing(8.0)
                        .build(ctx, |ctx| {
                            Text::new("底部片内容（上拖展开，内部可单独滚动）").font_size(16.0).build(ctx);
                            let sc = st.clone();
                            let list = sheet_items.clone();
                            // 有限高度视口（Scaffold 下片高度由锚点拖拽决定，未展开时有限高；给列表定高以启用虚拟滚动）
                            // height 620 使内容总高 ≈ 视口 720 → 展开时 is_full 触发 → 圆角 28→0 动画可见
                            Stack::new()
                                .modifier(Modifier::new().fill_max_width().height(620.0).clip(Shape::RoundedRect { corner_radius: 12.0 }))
                                .build(ctx, |ctx| {
                                    LazyColumn::new()
                                        .state(sc.clone())
                                        .modifier(Modifier::new().fill_max_width().fill_max_height())
                                        .items_from(list.clone(), |it: &ScaffoldItem| it.id, move |ctx, _i, it| {
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
                                let s1c = s1.clone();
                                Button::text()
                                    .on_click(move || s1c.expand())
                                    .build(ctx, |ctx| Text::new("展开").build(ctx));
                                let s2c = s2.clone();
                                Button::text()
                                    .on_click(move || s2c.partial_expand())
                                    .build(ctx, |ctx| Text::new("收起").build(ctx));
                                let sc2 = st.clone();
                                Button::text()
                                    .on_click(move || sc2.animate_scroll_to_item(80, 0.0))
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
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(480.0, 720.0)
                .title("BottomSheetScaffold 演示")
                .build(ctx, |ctx| scaffold_demo(ctx));
        });
    });
}
