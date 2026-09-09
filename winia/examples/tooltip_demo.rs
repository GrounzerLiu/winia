//! Tooltip 演示——hover 提示框（M3 plain tooltip + 自定义 rich 样式）
//!
//! 运行：`cargo run -p winia --example tooltip_demo`

use letclone::clone;
use winia::prelude::*;
use winia::ui::Tooltip;

#[composable]
fn tooltip_ui(ctx: &mut ComposeCtx) {
    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(32.0))
        .spacing(24.0)
        .build(ctx, |ctx| {
            // ── Plain tooltip（hover 显示）──
            Text::new("Plain tooltip（hover 显示）")
                .font_size(14.0)
                .color(WiniaTheme::colors().on_surface_variant)
                .build(ctx);
            Tooltip::new("This is a plain tooltip")
                .build(ctx, |ctx| {
                    Button::new()
                        .on_click(|| {})
                        .build(ctx, |ctx| { Text::new("Hover me").build(ctx); });
                });

            // ── 自定义 Rich tooltip（标题 + 正文 + 按钮）──
            Text::new("Rich tooltip（自定义内容 + 外部 visible 控制）")
                .font_size(14.0)
                .color(WiniaTheme::colors().on_surface_variant)
                .modifier(Modifier::new().padding_top(16.0))
                .build(ctx);
            let show = ctx.remember(|| false);
            // content 闭包需 'static——move 捕获 State 克隆（Arc 共享）
            Tooltip::new("")
                .content({
                    clone!(show);
                    move |ctx: &mut ComposeCtx| {
                // M3 rich tooltip：surface_container 容器 + on_surface_variant
                Column::new()
                    .modifier(Modifier::new()
                        .background(WiniaTheme::colors().surface_container, Shape::rounded(4.0))
                        .padding(16.0))
                    .build(ctx, |ctx| {
                        Text::new("Rich Tooltip")
                            .font_size(14.0)
                            .color(WiniaTheme::colors().on_surface)
                            .build(ctx);
                        Text::new("Supporting text for a rich tooltip with actions.")
                            .font_size(12.0)
                            .color(WiniaTheme::colors().on_surface_variant)
                            .modifier(Modifier::new().padding_top(4.0))
                            .build(ctx);
                        Button::new()
                            .on_click({
                                clone!(show);
                                move || show.set(false)
                            })
                            .modifier(Modifier::new().padding_top(8.0))
                            .build(ctx, |ctx| { Text::new("Close").build(ctx); });
                    });
                }
                })
            .visible(show.clone())
            .no_hover()
            .build(ctx, |ctx| {
                Button::new()
                    .on_click({ clone!(show); move || show.update(|v| *v = !*v) })
                    .build(ctx, |ctx| { Text::new("Toggle rich tooltip").build(ctx); });
            });
        });
}

fn main() {
    winia::run_app!(|ctx| {
        winia::ui::theme::WiniaTheme::auto(ctx, |ctx| {
            winia::ui::window::Window::new()
                .size(480.0, 300.0)
                .title("Tooltip Demo")
                .build(ctx, tooltip_ui);
        });
    });
}
