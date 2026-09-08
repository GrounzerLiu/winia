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

use letclone::clone;
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
    // skipPartiallyExpanded 演示：无半展开中间态，下滑直接关闭
    let skip_visible = ctx.remember(|| false);

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
                .on_click({ clone!(visible); move || visible.set(true) })
                .build(ctx, |ctx| Text::new("打开底部面板").build(ctx));
            Button::text()
                .on_click({ clone!(skip_visible); move || skip_visible.set(true) })
                .build(ctx, |ctx| Text::new("打开（跳过半展开）").build(ctx));
            Text::new("说明：底部弹出、可拖拽、三态（Hidden/半展开/展开）、点击遮罩关闭。内部已改为 LazyColumn（120 项，虚拟滚动）。")
                .font_size(12.0)
                .color(Color::from_argb(255, 120, 120, 120))
                .build(ctx);
        });

    // 共享滚动状态（跨重组保持 firstVisible/offset，避免每帧重建）
    let lazy_state = ctx.remember(|| LazyListState::new()).get();
    // ModalBottomSheet：visible 参数化（对齐 Popup/Dialog——build 总执行，
    // 内部 record_overlay_active 供 sync 删除；外层 if 包裹会导致 Skip≠主动关闭）
    ModalBottomSheet::new(visible.get())
        .on_dismiss_request({ clone!(visible); move || visible.set(false) })
        .build(ctx, {
            clone!(items, lazy_state);
            move |ctx| {
            // Sheet content: fixed header + LazyColumn filling remaining height.
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
                    // ⚠ LazyColumn nested inside the draggable sheet offset container.
                    Stack::new()
                        .modifier(Modifier::new().fill_max_width().height(360.0).clip(Shape::RoundedRect { corner_radius: 12.0 }))
                        .build(ctx, |ctx| {
                            LazyColumn::new()
                                .state(lazy_state.clone())
                                .modifier(Modifier::new().fill_max_width().fill_max_height())
                                .spacing(8.0)
                                .items_from(
                                    items.clone(),
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
                                clone!(lazy_state);
                                move || lazy_state.scroll_to_item(0, 0.0)
                            })
                            .build(ctx, |ctx| Text::new("回到顶部").build(ctx));
                        Button::text()
                            .on_click({
                                clone!(lazy_state);
                                move || lazy_state.animate_scroll_to_item(90, 0.0)
                            })
                            .build(ctx, |ctx| Text::new("跳到 90").build(ctx));
                    });
                });
            }
        });

    // 第二个 ModalBottomSheet：skipPartiallyExpanded（无半展开——下滑直接折到关闭）
    ModalBottomSheet::new(skip_visible.get())
        .skip_partially_expanded(true)
        .on_dismiss_request({ clone!(skip_visible); move || skip_visible.set(false) })
        .build(ctx, |ctx| {
            Text::new("无半展开模式：Expanded ↔ Hidden，下滑直接关闭")
                .font_size(13.0)
                .modifier(Modifier::new().padding(24.0).fill_max_width())
                .build(ctx);
        });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(480.0, 560.0)
                .title("ModalBottomSheet 演示")
                .build(ctx, |ctx| sheet_demo(ctx));
        });
    });
}
