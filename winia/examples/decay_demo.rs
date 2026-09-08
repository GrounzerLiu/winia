//! fling 演示——Decay 指数衰减动画（对标 Compose exponentialDecay）
//!
//! 点击 "Fling →"：方块以初始速度 2400px/s 向右飞，指数衰减自然停止。
//! 物理：`value(t) = v0/friction·(1 - e^(-friction·t))`，极限 = v0/friction
//! （默认 friction=4.2 → 极限 ≈ 571px，约 1.6s 停稳）。
//!
//! 再点一次 "Fling →"：从当前位置重新 fling（新动画取代进行中的动画）。
//!
//! 用法：`cargo run -p winia --example decay_demo`

use letclone::clone;
use winia::animation::{exponential_decay, push_decay};
use winia::modifier::GraphicsLayerParams;
use winia::prelude::*;

#[composable]
fn decay_demo(ctx: &mut ComposeCtx) {
    // 方块 x 偏移（px）——动画推进写 set_no_wake（不触发重组）
    let x = ctx.remember(|| 0.0f32);

    Column::new()
        .modifier(Modifier::new().padding(16.0).fill_max_size())
        .build(ctx, |ctx| {
            Text::new("Decay / fling (exponentialDecay)")
                .font_size(20.0)
                .color(Color::from_argb(255, 233, 30, 99))
                .build(ctx);

            Row::new()
                .modifier(Modifier::new().fill_max_width().padding_vertical(8.0))
                .build(ctx, |ctx| {
                    Button::new()
                        .on_click({
                            clone!(x);
                            move || {
                                // 从当前位置以 1344px/s 向右 fling（极限 = 1344/4.2 = 320px，
                                // 方块右缘 320+40 = 360 精确贴轨道尽头）
                                push_decay(x.clone(), 1344.0, exponential_decay(4.2));
                            }
                        })
                        .build(ctx, |ctx| {
                            Text::new("Fling →").build(ctx);
                        });
                    Button::new()
                        .on_click({
                            clone!(x);
                            move || {
                                // 先取消进行中的 fling（否则 set 后下一帧被动画覆盖——reset 无效）
                                winia::animation::cancel_animation(&x);
                                x.set(0.0);
                            }
                        })
                        .build(ctx, |ctx| {
                            Text::new("Reset").build(ctx);
                        });
                });

            // 轨道 + 方块（Stack 叠放——方块 graphics_layer 平移，渲染期 peek 零重组）
            Stack::new().build(ctx, |ctx| {
                Column::new()
                    .modifier(Modifier::new()
                        .size(360.0, 40.0)
                        .background(Color::from_argb(60, 120, 120, 120), Shape::rounded(6.0)))
                    .build(ctx, |_| {});
                let gfx = {
                    clone!(x);
                    move || {
                        let mut p = GraphicsLayerParams::default();
                        p.translation_x = x.peek();
                        p
                    }
                };
                Column::new()
                    .modifier(Modifier::new()
                        .size(40.0, 40.0)
                        .background(Color::from_argb(255, 76, 175, 80), Shape::rounded(6.0))
                        .graphics_layer(gfx))
                    .build(ctx, |_| {});
            });

            Text::new("方块位移由 Decay 驱动（渲染期 peek——动画推进不触发重组）")
                .font_size(12.0)
                .color(Color::from_argb(180, 140, 140, 140))
                .build(ctx);
        });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(420.0, 280.0)
                .title("Decay / fling Demo")
                .build(ctx, |ctx| decay_demo(ctx));
        });
    });
}
