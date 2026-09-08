//! Overlay demo——Popup / Dialog / DropdownMenu 顶层弹出组件演示。
//!
//! 运行：`cargo run -p winia --example overlay_demo --features debug-server`

use letclone::clone;
use winia::prelude::*;
use winia::ui::{Dialog, DropdownMenu, DropdownMenuItem, OverlayAnimSpec, Popup, PopupPosition};
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
        .build(ctx, {
            // Clone outer states into the closure (avoid borrow — content outlives the call).
            // `last` needs 3 copies (menu / dialog content / display) — one per move site.
            clone!(popup_open, menu_open, dialog_open, last);
            let (last_display, last_menu, last_dialog) = (last.clone(), last.clone(), last.clone());
            move |ctx| {

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
                    clone!(popup_open);
                    move || {
                        let v = popup_open.get();
                        popup_open.set(!v);
                    }
                })
                .build(ctx, |ctx| {
                    Text::new("1. Popup（锚点下方）").font_size(13.0).build(ctx);
                });
            // ⚠ Popup 参数化（visible）：build 总执行并记录 active——主动关闭
            // visible=false → sync 删除 overlay；注册方 Skip → 保留。若用 if
            // 包裹（build 不执行），Skip 帧与主动关闭无法区分。
            {
                Popup::new(popup_open.clone().get())
                    .position(PopupPosition::BottomLeft)
                    .offset(0.0, 6.0)
                    .on_dismiss_request({ clone!(popup_open); move || popup_open.set(false) })
                    .build(ctx, |ctx| {
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
                    clone!(menu_open);
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
                    clone!(menu_open);
                    move || menu_open.set(false)
                })
                .build(
                    ctx,
                    |ctx| {
                        // 锚点：占位（与按钮同位置——按钮上方）
                        Text::new("").font_size(1.0).build(ctx);
                    },
                    {
                        clone!(menu_open, last_menu);
                        move |ctx| {
                        {
                            DropdownMenuItem::new("新建文件")
                                .on_click({
                                    clone!(menu_open, last_menu);
                                    move || {
                                        last_menu.set(String::from("新建文件"));
                                        menu_open.set(false);
                                    }
                                })
                                .build(ctx);
                        }
                        {
                            DropdownMenuItem::new("打开…")
                                .on_click({
                                    clone!(menu_open, last_menu);
                                    move || {
                                        last_menu.set(String::from("打开…"));
                                        menu_open.set(false);
                                    }
                                })
                                .build(ctx);
                        }
                        {
                            DropdownMenuItem::new("退出")
                                .enabled(false)
                                .on_click({
                                    clone!(menu_open, last_menu);
                                    move || {
                                        last_menu.set(String::from("退出"));
                                        menu_open.set(false);
                                    }
                                })
                                .build(ctx);
                        }
                    }
                    },
                );

            // ── Dialog：模态对话框 ──
            Button::new()
                .modifier(winia::modifier::Modifier::new().width(220.0))
                .on_click({
                    clone!(dialog_open);
                    move || {
                        let v = dialog_open.get();
                        dialog_open.set(!v);
                    }
                })
                .build(ctx, |ctx| {
                    Text::new("3. Dialog（模态）").font_size(13.0).build(ctx);
                });
            // ⚠ Dialog 参数化（visible）——同 Popup：build 总执行记录 active
            {
                Dialog::new(dialog_open.clone().get())
                    // 进入动画（默认 scale 0.8→1 + fade 200ms EaseOutCubic——
                    // 对齐 Compose material2 Dialog 打开效果）。可自定义：
                    //   .enter_animation(None)                        // 关闭进入动画（瞬时出现）
                    //   .exit_animation(None)                         // 关闭退出动画（瞬时消失）
                    //   .enter_animation(Some(OverlayAnimSpec::scale_only(0.5, Duration::from_millis(400))))
                    //   .exit_animation(Some(OverlayAnimSpec::fade_only(Duration::from_millis(150))))
                    //   .enter_animation(Some(OverlayAnimSpec::default_enter().scale_from(0.6).duration(Duration::from_millis(300))))
                    .on_dismiss_request({
                        clone!(dialog_open);
                        move || dialog_open.set(false)
                    })
                    .build(ctx, {
                        clone!(dialog_open, last_dialog);
                        move |ctx| {
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
                                                clone!(dialog_open);
                                                move || dialog_open.set(false)
                                            })
                                            .build(ctx, |ctx| {
                                                Text::new("取消").font_size(13.0).build(ctx);
                                            });
                                        Button::new()
                                            .modifier(winia::modifier::Modifier::new().size(90.0, 34.0))
                                            .on_click({
                                                clone!(dialog_open, last_dialog);
                                                move || {
                                                    last_dialog.set(String::from("对话框确定"));
                                                    dialog_open.set(false);
                                                }
                                            })
                                            .build(ctx, |ctx| {
                                                Text::new("确定").font_size(13.0).build(ctx);
                                            });
                                    });
                            });
                        }
                    });
                }
            }
        });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(560.0, 720.0)
                .title("Overlay Demo")
                .build(ctx, overlay_ui);
        });
    });
}
