//! GraphicsLayer 3D 演示——rotationX/rotationY + cameraDistance + shadowElevation。
//!
//! 运行：`cargo run -p winia --example graphics_layer_demo --features debug-server`

use winia::prelude::*;
use winia::core::composer::ComposeCtx;
use winia::composable;
use winia::modifier::{Color, GraphicsLayerParams, Modifier, Shape};

#[composable]
fn graphics_layer_ui(ctx: &mut ComposeCtx) {
    let rx = ctx.remember(|| 0.0f32);
    let ry = ctx.remember(|| 0.0f32);
    let cam = ctx.remember(|| 8.0f32);
    let elev = ctx.remember(|| 0.0f32);

    let theme = WiniaTheme::colors();
    Column::new()
        .spacing(14.0)
        .modifier(Modifier::new().padding(24.0).fill_max_size())
        .build(ctx, |ctx| {
            Text::new("GraphicsLayer 3D 演示")
                .font_size(22.0)
                .color(Color::from_argb(255, 40, 40, 40))
                .build(ctx);
            Text::new(format!(
                "rotationX={:.0}° rotationY={:.0}° camera={:.0} shadow={:.0}",
                rx.get(), ry.get(), cam.get(), elev.get()
            ))
            .font_size(14.0)
            .color(Color::from_argb(255, 90, 90, 90))
            .build(ctx);

            // ── 控制行 ──
            Row::new().spacing(8.0).build(ctx, |ctx| {
                for (label, dx, dy) in [
                    ("X+30", 30.0f32, 0.0f32),
                    ("X-30", -30.0, 0.0),
                    ("Y+45", 0.0, 45.0),
                    ("Y-45", 0.0, -45.0),
                    ("重置", -rx.get(), -ry.get()),
                ] {
                    Button::new()
                        .on_click({
                            let rx = rx.clone();
                            let ry = ry.clone();
                            move || {
                                rx.update(|v| *v = (*v + dx).clamp(-90.0, 90.0));
                                ry.update(|v| *v = (*v + dy).clamp(-90.0, 90.0));
                            }
                        })
                        .build(ctx, |ctx| {
                            Text::new(label).font_size(13.0).build(ctx);
                        });
                }
            });
            Row::new().spacing(8.0).build(ctx, |ctx| {
                // 卡片 260x170——相机下限 = 半尺寸 131：按钮必须超过下限
                // 才能看到切换差异（低于下限的值统一表现为最大合理透视）
                // 框架默认字段是 8（对标 Compose），但对真实视图会被钳制到
                // 半尺寸——低于下限没有可见差异，demo 直接用超过下限的值
                for (label, v) in [("相机近(200)", 200.0f32), ("相机中(400)", 400.0), ("相机远(1200)", 1200.0)] {
                    Button::new()
                        .on_click({
                            let cam = cam.clone();
                            move || cam.set(v)
                        })
                        .build(ctx, |ctx| {
                            Text::new(label).font_size(13.0).build(ctx);
                        });
                }
                for (label, v) in [("阴影0", 0.0f32), ("阴影4", 4.0), ("阴影8", 8.0)] {
                    Button::new()
                        .on_click({
                            let elev = elev.clone();
                            move || elev.set(v)
                        })
                        .build(ctx, |ctx| {
                            Text::new(label).font_size(13.0).build(ctx);
                        });
                }
            });

            // ── 3D 卡片（动态 graphicsLayer：旋转/相机/阴影全读 State）──
            Column::new()
                .modifier(
                    Modifier::new()
                        .size(260.0, 170.0)
                        .background(theme.primary_container, Shape::rounded(14.0))
                        .border(1.0, theme.outline_variant, Shape::rounded(14.0))
                        .graphics_layer({
                            let rx = rx.clone();
                            let ry = ry.clone();
                            let cam = cam.clone();
                            let elev = elev.clone();
                            move || GraphicsLayerParams {
                                rotation_x: rx.get(),
                                rotation_y: ry.get(),
                                camera_distance: cam.get(),
                                shadow_elevation: elev.get(),
                                shadow_shape: Some(Shape::rounded(14.0)),
                                ..Default::default()
                            }
                        }),
                )
                .build(ctx, |ctx| {
                    Text::new("3D 卡片")
                        .font_size(18.0)
                        .color(theme.on_primary_container)
                        .build(ctx);
                    Text::new("rotationX / rotationY\ncameraDistance / shadowElevation")
                        .font_size(13.0)
                        .color(theme.on_primary_container)
                        .build(ctx);
                });
        });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        WiniaTheme::light(ctx, |ctx| {
            Window::new()
                .size(640.0, 520.0)
                .title("GraphicsLayer 3D Demo")
                .build(ctx, graphics_layer_ui);
        });
    });
}
