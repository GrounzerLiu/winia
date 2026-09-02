//! ModalBottomSheet 演示（对标 Compose Material3 ModalBottomSheet）
//!
//! 验证：
//! - 底部弹出 + 遮罩
//! - 上滑进入 / 下滑退出（SheetState 三态：Hidden/PartiallyExpanded/Expanded）
//! - 拖拽关闭/展开（sheet_gestures_enabled）
//! - 顶部把手 + 内容
//! - 点击遮罩关闭
//!
//! 运行：cargo run -p winia --example bottom_sheet_demo

use winia::prelude::*;
use std::sync::Arc;

#[derive(Clone)]
struct SheetItem {
    id: u64,
    title: String,
    subtitle: String,
}

#[composable]
fn sheet_demo(ctx: &mut ComposeCtx) {
    // 主树 visible 状态
    let visible = ctx.remember(|| false);
    let vis = visible.clone();

    // 120 项长列表数据（LazyColumn 稳定 key 演示）
    let items = ctx
        .remember(|| {
            let v: Vec<SheetItem> = (0..120)
                .map(|i| SheetItem {
                    id: i as u64,
                    title: format!("列表项 {}", i),
                    subtitle: format!("副标题 · 分组 {} · 长按拖动面板", i % 8),
                })
                .collect();
            Arc::new(v)
        })
        .get();

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .spacing(12.0)
        .build(ctx, |ctx| {
            Text::new("ModalBottomSheet 演示").font_size(16.0).build(ctx);
            Button::text()
                .on_click(move || vis.set(true))
                .build(ctx, |ctx| Text::new("打开底部面板").build(ctx));
            Text::new("说明：底部弹出、可拖拽、三态（Hidden/半展开/展开）、点击遮罩关闭。内部已改为 LazyColumn（120 项，虚拟滚动）。")
                .font_size(12.0)
                .color(Color::from_argb(255, 120, 120, 120))
                .build(ctx);
        });

    // 共享滚动状态（跨重组保持 firstVisible/offset，避免每帧重建）
    let lazy_state = ctx.remember(|| LazyListState::new()).get();
    // ModalBottomSheet：visible 参数化（对齐 Popup/Dialog——build 总执行，
    // 内部 record_overlay_active 供 sync 删除；外层 if 包裹会导致 Skip≠主动关闭）
    let v = visible.clone();
    let sheet_items = items.clone();
    let lazy_for_overlay = lazy_state.clone();
    ModalBottomSheet::new(visible.get())
        .on_dismiss_request(move || v.set(false))
        .build(ctx, move |ctx| {
            let items_inner = sheet_items.clone();
            let st = lazy_for_overlay.clone();
            // 头部固定 + 下方 LazyColumn 占满剩余高度（与 Compose 底部抽屉内 LazyColumn 语义一致）
            // 关键：LazyColumn 必须有确定高度约束——用 Stack + fill_max_size 包裹后由 Column 的剩余 flex 决定高度，
            // 但 BottomSheet 面板高度本身是 wrap/由内容撑开；这里用 weight 语义让列表占满“可滚动区”
            Column::new()
                .modifier(Modifier::new().fill_max_width().padding(16.0))
                .spacing(8.0)
                .build(ctx, |ctx| {
                    Text::new("这是底部面板内容（上拖展开显示更多，在列表内可单独滚动）")
                        .font_size(16.0)
                        .build(ctx);
                    Text::new("拖拽面板：半展开 ↔ 全展开 ↔ 下滑关闭；点击遮罩关闭。列表已切 LazyColumn，支持虚拟滚动与嵌套滚动。")
                        .font_size(12.0)
                        .color(Color::from_argb(255, 120, 120, 120))
                        .build(ctx);
                    let sc = st.clone();
                    let items_for_list = items_inner.clone();
                    // ⚠ LazyColumn 嵌套在"可拖拽 BottomSheet 的偏移容器"内（上层 Modifier.offset），
                    // 若直接把滚轮/拖拽分发给 LazyColumn，会与"拖 Sheet 上下展开"的 on_drag 冲突
                    // （Compose 中是 nestedScroll 链协作：先让 Sheet 吃不到的增量再给列表）。
                    // 这里用 nested_scroll 做简单放行：LazyColumn 自带垂直滚动容器，BottomSheet
                    // 的 on_drag 只在"列表已到顶/底且再沿同一方向拖"时才应接管——简化实现中先让
                    // LazyColumn 仅作为**可滚动容器**（不参与 nested 抢占），Sheet 拖动仍由外层
                    // 面板 on_drag 吃 dy（列表消费完的剩余 dy 透过 hit_test 未命中？暂不协作——先可滚）。
                    Stack::new()
                        .modifier(Modifier::new().fill_max_width().height(360.0).clip(Shape::RoundedRect { corner_radius: 12.0 }))
                        .build(ctx, |ctx| {
                            LazyColumn::new()
                                .state(sc.clone())
                                .modifier(Modifier::new().fill_max_width().fill_max_height())
                                .spacing(8.0)
                                .items_from(
                                    items_for_list.clone(),
                                    |it: &SheetItem| it.id,
                                    move |ctx, _i, it| {
                                        // 行：标题 + 副标题（类似 ListItem 双行）
                                        Column::new()
                                            .modifier(
                                                Modifier::new()
                                                    .fill_max_width()
                                                    .padding(12.0)
                                                    .background(
                                                        Color::from_argb(255, 245, 245, 247),
                                                        Shape::RoundedRect { corner_radius: 10.0 },
                                                    ),
                                            )
                                            .spacing(4.0)
                                            .build(ctx, |ctx| {
                                                Text::new(it.title.clone()).font_size(13.0).build(ctx);
                                                Text::new(it.subtitle.clone())
                                                    .font_size(11.0)
                                                    .color(Color::from_argb(255, 120, 120, 120))
                                                    .build(ctx);
                                            });
                                    },
                                )
                                .build(ctx);
                        });
                    // 底部操作区不随列表滚动（固定在列表下方）
                    Row::new().spacing(8.0).build(ctx, |ctx| {
                        Button::text()
                            .on_click({
                                let sc2 = st.clone();
                                move || sc2.scroll_to_item(0, 0.0)
                            })
                            .build(ctx, |ctx| Text::new("回到顶部").build(ctx));
                        Button::text()
                            .on_click({
                                let sc2 = st.clone();
                                move || sc2.animate_scroll_to_item(90, 0.0)
                            })
                            .build(ctx, |ctx| Text::new("跳到 90").build(ctx));
                    });
                });
        });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(480.0, 560.0)
                .title("ModalBottomSheet 演示")
                .build(ctx, |ctx| sheet_demo(ctx));
        });
    });
}
