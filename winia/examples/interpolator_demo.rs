//! 插值器演示——展示全部 30 个表驱动插值器（v1 移植）的动画曲线效果
//!
//! 每行：插值器名称 + 进度条（宽度 = 动画值 × 240）。点击 Play 全部同时
//! 播放 0→1（1200ms），进度条增长速度/形态直观展示插值曲线：
//! - Linear 匀速；EaseIn* 慢起快收；EaseOut* 快起慢收；Bounce 落地弹跳；
//!   Elastic 来回振荡；Back 越界回弹
//!
//! 用法：`cargo run -p winia --example interpolator_demo`

use std::sync::Arc;
use winia::animation::interpolator::Interpolator;
use winia::animation::{AnimationSpec, TweenSpec};
use winia::modifier::GraphicsLayerParams;
use winia::prelude::*;
use winia::app;

/// 全部 30 个插值器（Linear + 29 个表驱动）
fn all_interpolators() -> Vec<(&'static str, Arc<dyn Interpolator>)> {
    use winia::animation::interpolator::*;
    vec![
        ("Linear", Linear::new().into()),
        ("EaseInSine", EaseInSine::new().into()),
        ("EaseOutSine", EaseOutSine::new().into()),
        ("EaseInOutSine", EaseInOutSine::new().into()),
        ("EaseInQuad", EaseInQuad::new().into()),
        ("EaseOutQuad", EaseOutQuad::new().into()),
        ("EaseInOutQuad", EaseInOutQuad::new().into()),
        ("EaseInCubic", EaseInCubic::new().into()),
        ("EaseOutCubic", EaseOutCubic::new().into()),
        ("EaseInOutCubic", EaseInOutCubic::new().into()),
        ("EaseInQuart", EaseInQuart::new().into()),
        ("EaseOutQuart", EaseOutQuart::new().into()),
        ("EaseInOutQuart", EaseInOutQuart::new().into()),
        ("EaseInQuint", EaseInQuint::new().into()),
        ("EaseOutQuint", EaseOutQuint::new().into()),
        ("EaseInOutQuint", EaseInOutQuint::new().into()),
        ("EaseInExpo", EaseInExpo::new().into()),
        ("EaseOutExpo", EaseOutExpo::new().into()),
        ("EaseInOutExpo", EaseInOutExpo::new().into()),
        ("EaseInCirc", EaseInCirc::new().into()),
        ("EaseOutCirc", EaseOutCirc::new().into()),
        ("EaseInOutCirc", EaseInOutCirc::new().into()),
        ("EaseInBack", EaseInBack::new().into()),
        ("EaseOutBack", EaseOutBack::new().into()),
        ("EaseInOutBack", EaseInOutBack::new().into()),
        ("EaseInElastic", EaseInElastic::new().into()),
        ("EaseOutElastic", EaseOutElastic::new().into()),
        ("EaseInOutElastic", EaseInOutElastic::new().into()),
        ("EaseInBounce", EaseInBounce::new().into()),
        ("EaseOutBounce", EaseOutBounce::new().into()),
        ("EaseInOutBounce", EaseInOutBounce::new().into()),
    ]
}

#[composable]
fn interpolator_demo(ctx: &mut ComposeCtx) {
    // 播放状态：false=复位（0），true=播放（→1）
    let playing = ctx.remember(|| false);
    // get() 注册依赖（peek 不注册——点击后本 scope 不重跑，动画不启动）
    let is_playing = playing.get();

    Column::new()
        .modifier(Modifier::new().padding(16.0).fill_max_size())
        .build(ctx, |ctx| {
            let scroll_y = ctx.remember(|| ScrollState::new()).get();
            // ── 顶部固定：标题 + Play/Reset 按钮（不随滚动）──
            Row::new()
                .modifier(Modifier::new().fill_max_width())
                .alignment(winia::layout::Alignment::Center)
                .build(ctx, |ctx| {
                    Text::new("Interpolator Demo (30)")
                        .font_size(20.0)
                        .color(Color::from_argb(255, 233, 30, 99))
                        .build(ctx);
                });
            Row::new()
                .modifier(Modifier::new().fill_max_width().padding_vertical(8.0))
                .build(ctx, |ctx| {
                    Button::new()
                        .on_click({ let p = playing.clone(); move || { p.set(!p.peek()); } })
                        .build(ctx, |ctx| {
                            Text::new(if playing.peek() { "Reset" } else { "Play all" }).build(ctx);
                        });
                });

            // ── 滚动区：30 行插值器 ──
            Column::new()
                .modifier(Modifier::new()
                    .padding_vertical(8.0)
                    .fill_max_size()
                    .vertical_scroll(scroll_y))
                .build(ctx, |ctx| {
                    let list = all_interpolators();
                    for (name, interp) in &list {
                        // 位移动画：滑块 40px 在 240px 轨道上从 0 滑到 200——
                        // 插值器决定滑动速度曲线；graphics_layer 渲染层变换
                        // （渲染期 peek——零重排零重组，动画推进仅重绘）
                        let v = ctx.animate_float_as_state(
                            if is_playing { 200.0 } else { 0.0 },
                            AnimationSpec::Tween(TweenSpec::new(
                                std::time::Duration::from_millis(1200),
                                interp.clone(),
                            )),
                        );
                        Row::new()
                            .modifier(Modifier::new().fill_max_width().padding_vertical(3.0))
                            .build(ctx, |ctx| {
                                // 名称（固定宽 110）
                                Column::new()
                                    .modifier(Modifier::new().width(110.0))
                                    .build(ctx, |ctx| {
                                        Text::new(*name)
                                            .font_size(12.0)
                                            .color(Color::from_argb(200, 120, 120, 120))
                                            .build(ctx);
                                    });
                                // 轨道 + 滑块（Stack 叠放——滑块 graphics_layer 平移）
                                Stack::new().build(ctx, |ctx| {
                                    // 轨道（浅色底）
                                    Column::new()
                                        .modifier(Modifier::new()
                                            .size(240.0, 14.0)
                                            .background(
                                                Color::from_argb(60, 120, 120, 120),
                                                Shape::rounded(3.0),
                                            ))
                                        .build(ctx, |_| {});
                                    // 滑块（40px，translation_x = 动画值）
                                    let g = v.clone();
                                    let gfx = move || {
                                        let mut p = GraphicsLayerParams::default();
                                        p.translation_x = g.peek();
                                        p
                                    };
                                    Column::new()
                                        .modifier(Modifier::new()
                                            .size(40.0, 14.0)
                                            .background(
                                                Color::from_argb(255, 63, 81, 181),
                                                Shape::rounded(3.0),
                                            )
                                            .graphics_layer(gfx))
                                        .build(ctx, |_| {});
                                });
                            });
                    }
                });
        });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();

    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(420.0, 560.0)
                .title("Interpolator Demo")
                .build(ctx, |ctx| interpolator_demo(ctx));
        });
    });
}
