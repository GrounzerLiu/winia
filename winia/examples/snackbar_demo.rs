//! Snackbar 组件演示（对标 Compose Material3 SnackbarHost）
//!
//! 验证：
//! - 命令式弹出：按钮回调里 host.show()（任意位置可弹）
//! - 自动消失：Short（4s）/ Long（10s）时长到自动 dismiss（动画完成回调驱动）
//! - 覆盖语义：连续 show 新 snackbar 替换当前（旧定时器不误关）
//! - 手动关闭：with_dismiss_action 的 ✕ 按钮 / Indefinite 必须手动关
//! - action 按钮：右侧操作（如撤销）回调
//! - AnimatedVisibility 进出场：fade + 底部滑入/滑出
//!
//! 运行：cargo run -p winia --example snackbar_demo
//! 交互：点「显示 Snackbar（Short 4s 自动消失）」→ 4 秒后自动消失；
//!        点「带关闭按钮（Long 10s）」→ ✕ 手动关 / 10 秒后自动消失；
//!        点「Indefinite（手动关闭）」→ 一直显示，点 ✕ 才关；
//!        连续快速点多个 → 新的替换旧的。

use letclone::clone;
use winia::prelude::*;

#[composable]
fn snackbar_demo(ctx: &mut ComposeCtx) {
    // host 状态挂在整个 app 顶层（remember 跨重组稳定）——
    // SnackbarHost 显示在窗口底部（Scaffold bottomBar），show() 可从任意按钮回调调用
    let host = ctx.remember(|| SnackbarHostState::new()).get();

    Scaffold::new({
        clone!(host);
        move |ctx, _pad| {
        Column::new()
            .modifier(Modifier::new().fill_max_size().padding(16.0))
            .spacing(12.0)
            .build(ctx, |ctx| {
                Text::new("Snackbar 演示（Material3 SnackbarHost）")
                    .font_size(16.0)
                    .build(ctx);

                // Short：4s 自动消失
                Button::text()
                    .on_click({
                        clone!(host);
                        move || {
                            host.show(
                                SnackbarData::new("文件已保存")
                                    .action("撤销", || {})
                                    .duration(SnackbarDuration::Short),
                            );
                        }
                    })
                    .build(ctx, |ctx| Text::new("显示 Snackbar（Short · 4s 自动消失）").build(ctx));

                // Long + 手动关闭按钮：10s 自动消失 / ✕ 手动关
                Button::text()
                    .on_click({
                        clone!(host);
                        move || {
                            host.show(
                                SnackbarData::new("已复制到剪贴板")
                                    .with_dismiss_action()
                                    .duration(SnackbarDuration::Long),
                            );
                        }
                    })
                    .build(ctx, |ctx| Text::new("带关闭按钮（Long · 10s）").build(ctx));

                // Indefinite：不自动消失，必须手动关
                Button::text()
                    .on_click({
                        clone!(host);
                        move || {
                            host.show(
                                SnackbarData::new("操作被锁定，请手动关闭")
                                    .with_dismiss_action()
                                    .duration(SnackbarDuration::Indefinite),
                            );
                        }
                    })
                    .build(ctx, |ctx| Text::new("Indefinite（手动关闭）").build(ctx));

                // 覆盖：连续点几下看替换 + 旧定时器不误关
                Button::text()
                    .on_click({
                        clone!(host);
                        move || {
                            for i in 0..3 {
                                // 依次弹 3 条，每条都带自动消失——最后一条应覆盖前两条
                                let msg = format!("消息 #{i}");
                                host.show(
                                    SnackbarData::new(msg)
                                        .duration(SnackbarDuration::Custom(2000)),
                                );
                            }
                        }
                    })
                    .build(ctx, |ctx| Text::new("连续覆盖 3 条（2000ms）").build(ctx));

                // 手动 dismiss 全部
                Button::text()
                    .on_click({ clone!(host); move || host.dismiss() })
                    .build(ctx, |ctx| Text::new("立即隐藏").build(ctx));

                // 底部说明
                Text::new("说明：Snackbar 显示在窗口底部（Scaffold bottomBar），从底部滑入，自动消失在 show() 内由动画完成回调驱动。")
                    .font_size(12.0)
                    .color(Color::from_argb(255, 120, 120, 120))
                    .build(ctx);
            });
    }
    })
    // SnackbarHost 挂 Scaffold bottomBar——窗口底部、只占条自身高度、不遮挡内容点击
    .bottom_bar({
        clone!(host);
        move |ctx| {
            SnackbarHost::new(host.clone()).build(ctx);
        }
    })
    .build(ctx);
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(480.0, 560.0)
                .title("Snackbar 演示")
                .build(ctx, |ctx| snackbar_demo(ctx));
        });
    });
}
