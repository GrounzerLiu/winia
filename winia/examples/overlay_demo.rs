//! Overlay demo——Popup / Dialog / DropdownMenu 顶层弹出组件演示。
//!
//! 运行：`cargo run -p winia --example overlay_demo --features debug-server`

use winia::prelude::*;
use winia::ui::{Dialog, DropdownMenu, DropdownMenuItem, Popup, PopupPosition};
use winia::core::composer::ComposeCtx;
use winia::composable;

#[composable]
fn overlay_ui(ctx: &mut ComposeCtx) {
    let popup_open = ctx.remember(|| false);
    let menu_open = ctx.remember(|| false);
    let dialog_open = ctx.remember(|| false);
    let last = ctx.remember(|| String::from("(none)"));

    Column::new()
        .spacing(16.0)
        .modifier(winia::modifier::Modifier::new().padding(24.0))
        .build(ctx, move |ctx| {
            // 外层 State 全部 clone 进闭包（avoid borrow——content 生命周期不受控）
            let popup_open = popup_open.clone();
            let menu_open = menu_open.clone();
            let dialog_open = dialog_open.clone();
            // last 需要 3 份（菜单 / Dialog 内容 / 显示）——每处 move 独立副本
            let last_display = last.clone();
            let last_menu = last.clone();
            let last_dialog = last.clone();

            Text::new("Overlay 组件演示")
                .font_size(22.0)
                .color(winia::modifier::Color::from_argb(255, 40, 40, 40))
                .build(ctx);
            Text::new(format!("上次选择: {}", last_display.get()))
                .font_size(14.0)
                .color(winia::modifier::Color::from_argb(255, 120, 120, 120))
                .build(ctx);

            // ── Popup：锚点下方弹出 ──
            Button::new()
                .modifier(winia::modifier::Modifier::new().width(220.0))
                .on_click({
                    let popup_open = popup_open.clone();
                    move || {
                        let v = popup_open.get();
                        popup_open.set(!v);
                    }
                })
                .build(ctx, |ctx| {
                    Text::new("1. Popup（锚点下方）").font_size(13.0).build(ctx);
                });
            if popup_open.get() {
                let popup_open = popup_open.clone();
                Popup::new()
                    .position(PopupPosition::BottomLeft)
                    .offset(0.0, 6.0)
                    .on_dismiss_request(move || popup_open.set(false))
                    .build(ctx, move |ctx| {
                        Column::new()
                            .spacing(6.0)
                            .modifier(
                                winia::modifier::Modifier::new()
                                    .size(200.0, 90.0)
                                    .padding(winia::modifier::Dimension::Fixed(14.0))
                                    .background(
                                        winia::modifier::Color::from_argb(255, 250, 250, 250),
                                        winia::modifier::Shape::RoundedRect { corner_radius: 8.0 },
                                    )
                                    .border(
                                        1.0,
                                        winia::modifier::Color::from_argb(255, 210, 210, 210),
                                        winia::modifier::Shape::RoundedRect { corner_radius: 8.0 },
                                    ),
                            )
                            .build(ctx, |ctx| {
                                Text::new("这是一个 Popup")
                                    .font_size(14.0)
                                    .color(winia::modifier::Color::from_argb(255, 60, 60, 60))
                                    .build(ctx);
                                Text::new("点击外部关闭")
                                    .font_size(12.0)
                                    .color(winia::modifier::Color::from_argb(255, 140, 140, 140))
                                    .build(ctx);
                            });
                    });
            }

            // ── DropdownMenu：下拉菜单 ──
            Button::new()
                .modifier(winia::modifier::Modifier::new().width(220.0))
                .on_click({
                    let menu_open = menu_open.clone();
                    move || {
                        let v = menu_open.get();
                        menu_open.set(!v);
                    }
                })
                .build(ctx, |ctx| {
                    Text::new("2. DropdownMenu").font_size(13.0).build(ctx);
                });
            DropdownMenu::new(menu_open.clone())
                .on_dismiss_request({
                    let menu_open = menu_open.clone();
                    move || menu_open.set(false)
                })
                .build(
                    ctx,
                    |ctx| {
                        // 锚点：占位（与按钮同位置——按钮上方）
                        Text::new("").font_size(1.0).build(ctx);
                    },
                    move |ctx| {
                        {
                            let menu_open = menu_open.clone();
                            let last = last_menu.clone();
                            DropdownMenuItem::new("新建文件")
                                .on_click(move || {
                                    last.set(String::from("新建文件"));
                                    menu_open.set(false);
                                })
                                .build(ctx);
                        }
                        {
                            let menu_open = menu_open.clone();
                            let last = last_menu.clone();
                            DropdownMenuItem::new("打开…")
                                .on_click(move || {
                                    last.set(String::from("打开…"));
                                    menu_open.set(false);
                                })
                                .build(ctx);
                        }
                        {
                            let menu_open = menu_open.clone();
                            let last = last_menu.clone();
                            DropdownMenuItem::new("退出")
                                .enabled(false)
                                .on_click(move || {
                                    last.set(String::from("退出"));
                                    menu_open.set(false);
                                })
                                .build(ctx);
                        }
                    },
                );

            // ── Dialog：模态对话框 ──
            Button::new()
                .modifier(winia::modifier::Modifier::new().width(220.0))
                .on_click({
                    let dialog_open = dialog_open.clone();
                    move || {
                        let v = dialog_open.get();
                        dialog_open.set(!v);
                    }
                })
                .build(ctx, |ctx| {
                    Text::new("3. Dialog（模态）").font_size(13.0).build(ctx);
                });
            if dialog_open.get() {
                let dialog_open = dialog_open.clone();
                let last = last_dialog.clone();
                Dialog::new()
                    .on_dismiss_request({
                        let dialog_open = dialog_open.clone();
                        move || dialog_open.set(false)
                    })
                    .build(ctx, move |ctx| {
                        Column::new()
                            .spacing(10.0)
                            .modifier(
                                winia::modifier::Modifier::new()
                                    .size(300.0, 170.0)
                                    .padding(winia::modifier::Dimension::Fixed(20.0))
                                    .background(
                                        winia::modifier::Color::from_argb(255, 255, 255, 255),
                                        winia::modifier::Shape::RoundedRect { corner_radius: 12.0 },
                                    ),
                            )
                            .build(ctx, |ctx| {
                                Text::new("确认操作？")
                                    .font_size(17.0)
                                    .color(winia::modifier::Color::from_argb(255, 40, 40, 40))
                                    .build(ctx);
                                Text::new("对话框内容区——模态遮罩下主树不可交互")
                                    .font_size(12.0)
                                    .color(winia::modifier::Color::from_argb(255, 130, 130, 130))
                                    .build(ctx);
                                Row::new()
                                    .spacing(12.0)
                                    .modifier(winia::modifier::Modifier::new())
                                    .build(ctx, |ctx| {
                                        Button::new()
                                            .modifier(winia::modifier::Modifier::new().size(90.0, 34.0))
                                            .on_click({
                                                let dialog_open = dialog_open.clone();
                                                move || dialog_open.set(false)
                                            })
                                            .build(ctx, |ctx| {
                                                Text::new("取消").font_size(13.0).build(ctx);
                                            });
                                        Button::new()
                                            .modifier(winia::modifier::Modifier::new().size(90.0, 34.0))
                                            .on_click({
                                                let dialog_open = dialog_open.clone();
                                                let last = last.clone();
                                                move || {
                                                    last.set(String::from("对话框确定"));
                                                    dialog_open.set(false);
                                                }
                                            })
                                            .build(ctx, |ctx| {
                                                Text::new("确定").font_size(13.0).build(ctx);
                                            });
                                    });
                            });
                    });
            }
        });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(560.0, 620.0)
                .title("Overlay Demo")
                .build(ctx, overlay_ui);
        });
    });
}
