//! Progress Indicator 演示（material3 对齐——Compose material3 ProgressIndicator）
//!
//! 展示：
//! - Linear determinate（进度实时驱动，带 stop indicator）
//! - Linear indeterminate（无限双线动画）
//! - Circular determinate（12 点起顺时针弧 + track）
//! - Circular indeterminate（旋转 + 进度呼吸）
//! - 自定义颜色 / stroke cap / gap
//! - 进度切换动画（ProgressIndicatorDefaults::progress_animation_spec 弹簧）

use winia::prelude::*;

fn section_title(ctx: &mut ComposeCtx, text: &str) {
    Text::new(text)
        .font_size(14.0)
        .color(Color::from_argb(255, 90, 90, 90))
        .modifier(Modifier::new().padding_top(14.0).padding_bottom(6.0))
        .build(ctx);
}

#[composable]
fn progress_demo(ctx: &mut ComposeCtx) {
    let scroll_y = ctx.remember(|| ScrollState::new()).get();
    // 进度状态（0..1）
    let progress = ctx.remember(|| 0.5f32);
    let _p = progress.clone();

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0).vertical_scroll(scroll_y))
        .build(ctx, |ctx| {
            Text::new("Progress Indicator 演示（material3 对齐）")
                .font_size(20.0)
                .build(ctx);

            section_title(ctx, "Linear determinate（进度 50%）");
            LinearProgressIndicator::new(progress.get()).build(ctx);

            section_title(ctx, "Linear determinate（25% / 100% / 0%）");
            LinearProgressIndicator::new(0.25).build(ctx);
            LinearProgressIndicator::new(1.0).build(ctx);
            LinearProgressIndicator::new(0.0).build(ctx);

            section_title(ctx, "Linear determinate（无 stop indicator / Butt cap）");
            LinearProgressIndicator::new(0.6)
                .draw_stop_indicator(false)
                .build(ctx);
            LinearProgressIndicator::new(0.6)
                .stroke_cap(ProgressIndicatorStrokeCap::Butt)
                .build(ctx);

            section_title(ctx, "Linear indeterminate（无限动画）");
            LinearProgressIndicator::indeterminate().build(ctx);

            section_title(ctx, "Linear 自定义颜色 + 进度驱动");
            LinearProgressIndicator::new(progress.get())
                .color(Color::from_argb(255, 46, 125, 50))
                .track_color(Color::from_argb(255, 200, 230, 200))
                .build(ctx);

            section_title(ctx, "Circular determinate（25% / 50% / 100%）");
            Row::new()
                .spacing(24.0)
                .build(ctx, |ctx| {
                    CircularProgressIndicator::new(0.25).build(ctx);
                    CircularProgressIndicator::new(0.5).build(ctx);
                    CircularProgressIndicator::new(1.0).build(ctx);
                });

            section_title(ctx, "Circular indeterminate（旋转 + 进度呼吸）");
            Row::new()
                .spacing(24.0)
                .build(ctx, |ctx| {
                    CircularProgressIndicator::indeterminate().build(ctx);
                    CircularProgressIndicator::indeterminate()
                        .color(Color::from_argb(255, 46, 125, 50))
                        .build(ctx);
                    CircularProgressIndicator::indeterminate()
                        .stroke_width(6.0)
                        .build(ctx);
                });

            section_title(ctx, "进度弹簧动画（ProgressAnimationSpec）");
            // 点击按钮切换目标进度——用 animate_float_as_state 弹簧过渡
            let animated = ctx.animate_float_as_state(
                progress.get(),
                ProgressIndicatorDefaults::progress_animation_spec(),
            );
            LinearProgressIndicator::new(animated.get()).build(ctx);
            let p2 = progress.clone();
            Slider::new(progress.get())
                .on_value_change(move |nv| p2.update(|s| *s = nv))
                .build(ctx);
            Text::new(format!("拖动滑块：progress = {:.2}（弹簧过渡）", progress.get()))
                .font_size(12.0)
                .color(Color::from_argb(255, 100, 100, 100))
                .build(ctx);

            section_title(ctx, "满宽（modifier 覆盖默认 240dp）");
            LinearProgressIndicator::new(0.7)
                .modifier(Modifier::new().fill_max_width())
                .build(ctx);

            Text::new("")
                .modifier(Modifier::new().height(40.0))
                .build(ctx);
        });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();

    winia::run_app!(|ctx| {
        WiniaTheme::light(ctx, |ctx| {
            Window::new()
                .size(420.0, 720.0)
                .title("Progress Indicator Demo")
                .build(ctx, progress_demo);
        });
    });
}
