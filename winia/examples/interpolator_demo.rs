//! 插值器效果展示 demo——每种插值器一行：色块沿轨道往返移动。
//!
//! 运行：`cargo run -p winia --example interpolator_demo --features debug-server`
//!
//! 原理：一个无限进度 0↔1（线性时间）驱动所有行；每行的色块 x 位置 =
//! `插值器(progress) * 轨道宽度`，在 graphics_layer 渲染期求值（零重组）。
//! 对比同一时刻各行色块的位置即可直观看出缓动曲线差异：
//! - Sine/Quad/Cubic：进入/退出/全程缓动
//! - Back：越过终点再回弹
//! - Elastic：橡皮筋式振荡
//! - Bounce：落地弹跳

use winia::animation::interpolator::{self, Interpolator};
use winia::animation::InfiniteRepeatableSpec;
use winia::core::composer::ComposeCtx;
use winia::modifier::{Color, Modifier, Shape};
use winia::prelude::{Column, Row, ScrollState, Text, WiniaTheme, Window};
use winia::app;

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();

    app::run_app(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(480.0, 620.0)
                .title("Interpolator Demo")
                .build(ctx, |ctx| interpolator_demo(ctx));
        });
    });
}

#[winia::composable]
fn interpolator_demo(ctx: &mut ComposeCtx) {
    let scroll_y = ctx.remember(|| ScrollState::new()).get();

    Column::new()
        .modifier(Modifier::new().fill_max_size())
        .build(ctx, |ctx| {
            // 标题
            Text::new("Interpolator Demo — 30 种缓动曲线对比")
                .font_size(16.0)
                .bold()
                .modifier(Modifier::new().padding(12.0))
                .build(ctx);

            // 无限进度 0↔1（线性时间，Reverse 往返 1.6s）
            let mut infinite = ctx.remember_infinite_transition();
            let progress = infinite.animate_float(
                ctx, 0.0, 1.0,
                InfiniteRepeatableSpec::reverse(std::time::Duration::from_millis(1600)),
            );

            Column::new()
                .modifier(Modifier::new()
                    .fill_max_size()
                    .vertical_scroll(scroll_y)
                    .padding_horizontal(12.0))
                .build(ctx, |ctx| {
                    // 每种插值器一行
                    interpolator_row(ctx, "Linear", interpolator::Linear::boxed(), &progress);
                    interpolator_row(ctx, "EaseInSine", interpolator::EaseInSine::boxed(), &progress);
                    interpolator_row(ctx, "EaseOutSine", interpolator::EaseOutSine::boxed(), &progress);
                    interpolator_row(ctx, "EaseInOutSine", interpolator::EaseInOutSine::boxed(), &progress);
                    interpolator_row(ctx, "EaseInQuad", interpolator::EaseInQuad::boxed(), &progress);
                    interpolator_row(ctx, "EaseOutQuad", interpolator::EaseOutQuad::boxed(), &progress);
                    interpolator_row(ctx, "EaseInOutQuad", interpolator::EaseInOutQuad::boxed(), &progress);
                    interpolator_row(ctx, "EaseInCubic", interpolator::EaseInCubic::boxed(), &progress);
                    interpolator_row(ctx, "EaseOutCubic", interpolator::EaseOutCubic::boxed(), &progress);
                    interpolator_row(ctx, "EaseInOutCubic", interpolator::EaseInOutCubic::boxed(), &progress);
                    interpolator_row(ctx, "EaseInQuart", interpolator::EaseInQuart::boxed(), &progress);
                    interpolator_row(ctx, "EaseOutQuart", interpolator::EaseOutQuart::boxed(), &progress);
                    interpolator_row(ctx, "EaseInQuint", interpolator::EaseInQuint::boxed(), &progress);
                    interpolator_row(ctx, "EaseOutQuint", interpolator::EaseOutQuint::boxed(), &progress);
                    interpolator_row(ctx, "EaseInExpo", interpolator::EaseInExpo::boxed(), &progress);
                    interpolator_row(ctx, "EaseOutExpo", interpolator::EaseOutExpo::boxed(), &progress);
                    interpolator_row(ctx, "EaseInOutExpo", interpolator::EaseInOutExpo::boxed(), &progress);
                    interpolator_row(ctx, "EaseInCirc", interpolator::EaseInCirc::boxed(), &progress);
                    interpolator_row(ctx, "EaseOutCirc", interpolator::EaseOutCirc::boxed(), &progress);
                    interpolator_row(ctx, "EaseInOutCirc", interpolator::EaseInOutCirc::boxed(), &progress);
                    interpolator_row(ctx, "EaseInBack", interpolator::EaseInBack::boxed(), &progress);
                    interpolator_row(ctx, "EaseOutBack", interpolator::EaseOutBack::boxed(), &progress);
                    interpolator_row(ctx, "EaseInOutBack", interpolator::EaseInOutBack::boxed(), &progress);
                    interpolator_row(ctx, "EaseInElastic", interpolator::EaseInElastic::boxed(), &progress);
                    interpolator_row(ctx, "EaseOutElastic", interpolator::EaseOutElastic::boxed(), &progress);
                    interpolator_row(ctx, "EaseInOutElastic", interpolator::EaseInOutElastic::boxed(), &progress);
                    interpolator_row(ctx, "EaseInBounce", interpolator::EaseInBounce::boxed(), &progress);
                    interpolator_row(ctx, "EaseOutBounce", interpolator::EaseOutBounce::boxed(), &progress);
                    interpolator_row(ctx, "EaseInOutBounce", interpolator::EaseInOutBounce::boxed(), &progress);
                });
        });
}

/// 一行：名称 + 轨道 + 移动色块
const TRACK_W: f32 = 300.0;

#[winia::composable]
fn interpolator_row(
    ctx: &mut ComposeCtx,
    name: &'static str,
    interp: Box<dyn Interpolator>,
    progress: &winia::core::state::State<f32>,
) {
    Row::new()
        .alignment(winia::layout::node::Alignment::Center)
        .modifier(Modifier::new()
            .fill_max_width()
            .padding_vertical(3.0))
        .build(ctx, |ctx| {
            Text::new(name)
                .font_size(12.0)
                .color(Color::from_argb(220, 120, 120, 120))
                .modifier(Modifier::new().width(110.0))
                .build(ctx);

            Column::new()
                .modifier(Modifier::new().width(6.0).height(16.0))
                .build(ctx, |_| {});

            // 轨道（浅色背景）
            Column::new()
                .modifier(Modifier::new()
                    .width(TRACK_W)
                    .height(16.0)
                    .background(Color::from_argb(25, 255, 255, 255), Shape::rounded(3.0)))
                .build(ctx, |_| {});

            // 色块：x = interp(progress) * (TRACK_W - 16) —— 渲染期求值（零重组）
            let progress = progress.clone();
            Column::new()
                .modifier(Modifier::new()
                    .size(16.0, 16.0)
                    .graphics_layer(move || {
                        let t = interp.interpolate(progress.peek());
                        winia::modifier::GraphicsLayerParams {
                            translation_x: t * (TRACK_W - 16.0),
                            ..Default::default()
                        }
                    })
                    .background(Color::from_argb(255, 255, 152, 0), Shape::rounded(3.0)))
                .build(ctx, |_| {});
        });
}
