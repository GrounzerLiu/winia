//! Surface 组件演示 — 验证外观底板（shape/color/border/shadow/content_color）
//!
//! 运行：cargo run -p winia --example surface_demo

use winia::prelude::*;

#[composable]
fn surface_demo(ctx: &mut ComposeCtx) {
    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(24.0))
        .spacing(24.0)
        .build(ctx, |ctx| {
            Text::new("Surface 组件演示").font_size(18.0).build(ctx);

            // ① 默认 surface + 圆角 16（主题 surface / on_surface 匹配）
            Surface::new()
                .shape(Shape::rounded(16.0))
                .modifier(Modifier::new().fill_max_width())
                .build(ctx, |ctx| {
                    Column::new()
                        .modifier(Modifier::new().fill_max_width().padding(16.0))
                        .spacing(8.0)
                        .build(ctx, |ctx| {
                            Text::new("默认 Surface").font_size(14.0).build(ctx);
                            Text::new("主题 surface 底 + on_surface 内容色")
                                .font_size(11.0)
                                .color(Color::from_argb(255, 120, 120, 120))
                                .build(ctx);
                        });
                });

            // ② 显式容器色 + 阴影 + 边框
            Surface::new()
                .shape(Shape::rounded(12.0))
                .color(Color::from_argb(255, 51, 92, 153))
                .content_color(Color::from_argb(255, 255, 255, 255))
                .shadow_elevation(6.0)
                .border(SurfaceBorder::new(2.0, Color::from_argb(255, 200, 100, 100)))
                .modifier(Modifier::new().fill_max_width())
                .build(ctx, |ctx| {
                    Column::new()
                        .modifier(Modifier::new().fill_max_width().padding(16.0))
                        .spacing(8.0)
                        .build(ctx, |ctx| {
                            Text::new("自定义 Surface").font_size(14.0).build(ctx);
                            Text::new("蓝底白字 + 阴影 6 + 2px 红边框")
                                .font_size(11.0)
                                .build(ctx);
                        });
                });
        });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(480.0, 480.0)
                .title("Surface 演示")
                .build(ctx, |ctx| surface_demo(ctx));
        });
    });
}
