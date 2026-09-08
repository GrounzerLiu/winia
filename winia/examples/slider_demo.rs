//! Slider 演示（material3 对齐——新版 M3 v2_3_5 视觉）
//!
//! 展示：
//! - 连续滑块（值实时显示）
//! - 离散滑块（steps 刻度吸附）
//! - 禁用状态
//! - 自定义颜色
//! - interactionSource hoist（拖动/聚焦时拇指宽度减半）

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
fn slider_demo(ctx: &mut ComposeCtx) {
    let scroll_y = ctx.remember(|| ScrollState::new()).get();
    let volume = ctx.remember(|| 0.5f32);
    let steps_v = ctx.remember(|| 0.0f32);
    let temp = ctx.remember(|| 50.0f32);

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0).vertical_scroll(scroll_y))
        .build(ctx, |ctx| {
            Text::new("Slider 演示（material3 对齐）")
                .font_size(20.0)
                .build(ctx);

            section_title(ctx, "连续滑块（音量）");
            Slider::new(volume.get())
                .value_range(0.0, 1.0)
                .on_value_change({ clone!(volume); move |nv| volume.update(|s| *s = nv) })
                .build(ctx);
            Text::new(format!("音量：{:.0}%", volume.get() * 100.0))
                .font_size(13.0)
                .color(Color::from_argb(255, 100, 100, 100))
                .build(ctx);

            section_title(ctx, "离散滑块（steps=4，温度）");
            Slider::new(steps_v.get())
                .value_range(0.0, 100.0)
                .steps(4)
                .on_value_change({ clone!(steps_v); move |nv| steps_v.update(|s| *s = nv) })
                .build(ctx);
            Text::new(format!("温度：{:.0}°C（{} 档）", steps_v.get(), 6))
                .font_size(13.0)
                .color(Color::from_argb(255, 100, 100, 100))
                .build(ctx);

            section_title(ctx, "禁用");
            Slider::new(0.3)
                .value_range(0.0, 1.0)
                .enabled(false)
                .build(ctx);

            section_title(ctx, "自定义颜色");
            let theme = WiniaTheme::colors();
            let mut custom = SliderColors::from_theme(&theme);
            custom.active_track_color = Color::from_argb(255, 46, 125, 50);
            custom.thumb_color = Color::from_argb(255, 46, 125, 50);
            custom.inactive_track_color = Color::from_argb(255, 200, 230, 200);
            custom.inactive_tick_color = Color::from_argb(255, 46, 125, 50);
            Slider::new(temp.get())
                .value_range(0.0, 100.0)
                .steps(4)
                .colors(custom)
                .on_value_change({ clone!(temp); move |nv| temp.update(|s| *s = nv) })
                .build(ctx);
            Text::new(format!("自定义：{:.0}", temp.get()))
                .font_size(13.0)
                .color(Color::from_argb(255, 100, 100, 100))
                .build(ctx);

            section_title(ctx, "键盘操作（点击聚焦后方向键/PageUp/Home/End）");
            Text::new("聚焦滑块后用 ←/→ 微调（1% 值域）、PageUp/PageDown 大步、Home/End 端点")
                .font_size(12.0)
                .color(Color::from_argb(255, 120, 120, 120))
                .build(ctx);

            section_title(ctx, "interactionSource hoist");
            let src = ctx.remember(|| MutableInteractionSource::new()).get();
            let st = src.state(true);
            Slider::new(volume.get())
                .value_range(0.0, 1.0)
                .interaction_source(src.clone())
                .on_value_change({ clone!(volume); move |nv| volume.update(|s| *s = nv) })
                .build(ctx);
            Text::new(format!(
                "pressed={} hovered={} focused={} dragged={} | 拖动/聚焦时拇指宽度减半",
                st.pressed, st.hovered, st.focused, st.dragged,
            ))
            .font_size(12.0)
            .color(Color::from_argb(255, 100, 100, 100))
            .build(ctx);

            Text::new("")
                .modifier(Modifier::new().height(40.0))
                .build(ctx);
        });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::light(ctx, |ctx| {
            Window::new()
                .size(420.0, 620.0)
                .title("Slider Demo")
                .build(ctx, slider_demo);
        });
    });
}
