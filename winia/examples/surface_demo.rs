//! Surface 组件演示 — 验证外观底板（shape/color/border/shadow/content_color）
//! + 三个交互重载（clickable 计数 / selectable 切换 / toggleable 开关）
//!
//! 运行：cargo run -p winia --example surface_demo

use letclone::clone;
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

            // 交互重载演示
            let click_count = ctx.remember(|| 0i32);
            let selected = ctx.remember(|| false);
            let checked = ctx.remember(|| false);

            // ③ clickable：可点击 + 波纹（点击计数）
            Surface::new()
                .shape(Shape::rounded(12.0))
                .on_click({ clone!(click_count); move || click_count.update(|v| *v += 1) })
                .modifier(Modifier::new().fill_max_width())
                .build(ctx, {
                    clone!(click_count);
                    move |ctx| {
                    Column::new()
                        .modifier(Modifier::new().fill_max_width().padding(16.0))
                        .build(ctx, |ctx| {
                            Text::new(format!("Clickable Surface (点击 {} 次)", click_count.get()))
                                .font_size(14.0)
                                .build(ctx);
                        });
                }
                });

            // ④ selectable：选中状态 + 点击切换（显示选中标记）
            Surface::new()
                .shape(Shape::rounded(12.0))
                .color(if selected.get() { Color::from_argb(255, 51, 92, 153) } else { Color::from_argb(255, 230, 230, 235) })
                .selectable(selected.get(), { clone!(selected); move || selected.update(|v| *v = !*v) })
                .modifier(Modifier::new().fill_max_width())
                .build(ctx, |ctx| {
                    Column::new()
                        .modifier(Modifier::new().fill_max_width().padding(16.0))
                        .build(ctx, |ctx| {
                            Text::new(if selected.get() {
                                "Selectable Surface（已选中）".to_string()
                            } else {
                                "Selectable Surface（未选中，点击选中）".to_string()
                            })
                            .font_size(14.0)
                            .build(ctx);
                        });
                });

            // ⑤ toggleable：开关状态 + 点击切换
            Surface::new()
                .shape(Shape::rounded(12.0))
                .border(SurfaceBorder::new(2.0, if checked.get() { Color::from_argb(255, 51, 92, 153) } else { Color::from_argb(255, 180, 180, 185) }))
                .toggleable(checked.get(), { clone!(checked); move |v| checked.set(v) })
                .modifier(Modifier::new().fill_max_width())
                .build(ctx, |ctx| {
                    Column::new()
                        .modifier(Modifier::new().fill_max_width().padding(16.0))
                        .build(ctx, |ctx| {
                            Text::new(if checked.get() {
                                "Toggleable Surface（开）".to_string()
                            } else {
                                "Toggleable Surface（关，点击切换）".to_string()
                            })
                            .font_size(14.0)
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
