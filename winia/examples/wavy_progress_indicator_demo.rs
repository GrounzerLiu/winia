//! Wavy Progress Indicator 演示（M3 Expressive——Compose Linear/CircularWavyProgressIndicator）
//!
//! 展示：
//! - Linear determinate（默认振幅函数：两端退化为直线，中间满幅波浪）
//! - Linear indeterminate（4 条 head/tail + 波浪滚动）
//! - Circular determinate（圆 ↔ 星形 Morph + 波浪滚动）
//! - Circular indeterminate（全局旋转 + 进度呼吸 + 波浪滚动）
//! - 自定义振幅函数 / 波长 / 波速

use letclone::clone;
use winia::prelude::*;

fn section_title(ctx: &mut ComposeCtx, text: &str) {
    Text::new(text)
        .font_size(14.0)
        .color(Color::from_argb(255, 90, 90, 90))
        .modifier(Modifier::new().padding_top(14.0).padding_bottom(6.0))
        .build(ctx);
}

#[composable]
fn wavy_progress_indicator_demo(ctx: &mut ComposeCtx) {
    // Slider 控制的 determinate 进度（0..1）
    let progress = ctx.remember(|| 0.5f32);

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .build(ctx, |ctx| {
            Text::new("Wavy Progress Indicator 演示（M3 Expressive）")
                .font_size(20.0)
                .build(ctx);

            section_title(ctx, "Slider 控制进度（Linear + Circular determinate）");
            Slider::new(progress.get())
                .on_value_change({ clone!(progress); move |nv| progress.update(|s| *s = nv) })
                .build(ctx);
            Text::new(format!("progress = {:.2}", progress.get()))
                .font_size(12.0)
                .color(Color::from_argb(255, 100, 100, 100))
                .build(ctx);
            LinearWavyProgressIndicator::new(progress.get()).build(ctx);
            Row::new().spacing(24.0).build(ctx, |ctx| {
                CircularWavyProgressIndicator::new(progress.get()).build(ctx);
            });

            section_title(ctx, "Linear determinate（progress 0.3 / 0.5 / 0.8）");
            Column::new().spacing(12.0).build(ctx, |ctx| {
                LinearWavyProgressIndicator::new(0.3).build(ctx);
                LinearWavyProgressIndicator::new(0.5).build(ctx);
                LinearWavyProgressIndicator::new(0.8).build(ctx);
            });

            section_title(ctx, "Linear determinate（自定义振幅=1，wavelength=30）");
            LinearWavyProgressIndicator::new(0.5)
                .amplitude_fn(|_| 1.0)
                .wavelength(30.0)
                .build(ctx);

            section_title(ctx, "Linear indeterminate");
            LinearWavyProgressIndicator::indeterminate().build(ctx);

            section_title(ctx, "Circular determinate（0.25 / 0.5 / 0.75）");
            Row::new().spacing(24.0).build(ctx, |ctx| {
                CircularWavyProgressIndicator::new(0.25).build(ctx);
                CircularWavyProgressIndicator::new(0.5).build(ctx);
                CircularWavyProgressIndicator::new(0.75).build(ctx);
            });

            section_title(ctx, "Circular indeterminate");
            Row::new().spacing(24.0).build(ctx, |ctx| {
                CircularWavyProgressIndicator::indeterminate().build(ctx);
                CircularWavyProgressIndicator::indeterminate()
                    .color(Color::from_argb(255, 46, 125, 50))
                    .build(ctx);
            });
        });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::light(ctx, |ctx| {
            Window::new()
                .size(480.0, 720.0)
                .title("Wavy Progress Indicator Demo")
                .build(ctx, wavy_progress_indicator_demo);
        });
    });
}
