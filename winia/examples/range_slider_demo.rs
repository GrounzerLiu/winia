//! RangeSlider demo (material3 aligned) — the two-thumb slider.
//!
//! Shows: a continuous range with its values printed, a discrete range (`steps`), a disabled range,
//! custom colors, and the two hoisted per-thumb interaction sources (a thumb halves in width while
//! its own gesture runs).

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
fn range_slider_demo(ctx: &mut ComposeCtx) {
    let scroll_y = ctx.remember(|| ScrollState::new()).get();
    let price = ctx.remember(|| RangeValue::new(0.25, 0.75));
    let hours = ctx.remember(|| RangeValue::new(9.0, 18.0));
    let custom = ctx.remember(|| RangeValue::new(0.4, 0.6));

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0).vertical_scroll(scroll_y))
        .build(ctx, |ctx| {
            Text::new("RangeSlider demo (material3 aligned)")
                .font_size(20.0)
                .build(ctx);

            section_title(ctx, "Continuous (a price window)");
            RangeSlider::new(price.get())
                .value_range(0.0, 1.0)
                .on_value_change({ clone!(price); move |v: RangeValue| price.set(v) })
                .build(ctx);
            Text::new(format!("{:.0}% – {:.0}%", price.get().start * 100.0, price.get().end * 100.0))
                .font_size(13.0)
                .color(Color::from_argb(255, 100, 100, 100))
                .build(ctx);

            section_title(ctx, "Discrete (steps = 7, working hours)");
            RangeSlider::new(hours.get())
                .value_range(0.0, 24.0)
                .steps(7)
                .on_value_change({ clone!(hours); move |v: RangeValue| hours.set(v) })
                .build(ctx);
            Text::new(format!("{:.0}:00 – {:.0}:00 (9 stops, every 3 h)", hours.get().start, hours.get().end))
                .font_size(13.0)
                .color(Color::from_argb(255, 100, 100, 100))
                .build(ctx);

            section_title(ctx, "Disabled");
            RangeSlider::new((0.3, 0.7))
                .value_range(0.0, 1.0)
                .enabled(false)
                .build(ctx);

            section_title(ctx, "Custom colors");
            let theme = WiniaTheme::colors();
            let mut colors = SliderColors::from_theme(&theme);
            colors.thumb_color = Color::from_argb(255, 46, 125, 50);
            colors.active_track_color = Color::from_argb(255, 46, 125, 50);
            colors.inactive_track_color = Color::from_argb(255, 200, 230, 200);
            colors.active_tick_color = Color::from_argb(255, 46, 125, 50);
            colors.inactive_tick_color = Color::from_argb(255, 46, 125, 50);
            RangeSlider::new(custom.get())
                .value_range(0.0, 1.0)
                .steps(4)
                .colors(colors)
                .on_value_change({ clone!(custom); move |v: RangeValue| custom.set(v) })
                .build(ctx);
            Text::new(format!("{:.2} – {:.2}", custom.get().start, custom.get().end))
                .font_size(13.0)
                .color(Color::from_argb(255, 100, 100, 100))
                .build(ctx);

            section_title(ctx, "Per-thumb interaction sources (hoisted)");
            let start_src = ctx.remember(|| MutableInteractionSource::new()).get();
            let end_src = ctx.remember(|| MutableInteractionSource::new()).get();
            let start_st = start_src.state(true);
            let end_st = end_src.state(true);
            RangeSlider::new(price.get())
                .value_range(0.0, 1.0)
                .interaction_sources(start_src.clone(), end_src.clone())
                .on_value_change({ clone!(price); move |v: RangeValue| price.set(v) })
                .build(ctx);
            Text::new(format!(
                "start: pressed={} hovered={} focused={} dragged={}\nend:   pressed={} hovered={} focused={} dragged={}",
                start_st.pressed, start_st.hovered, start_st.focused, start_st.dragged,
                end_st.pressed, end_st.hovered, end_st.focused, end_st.dragged,
            ))
            .font_size(12.0)
            .color(Color::from_argb(255, 100, 100, 100))
            .build(ctx);

            section_title(ctx, "Keyboard (focus a thumb, then the arrow keys)");
            Text::new("The component focuses as a whole and the arrow keys move the thumb the last gesture picked: ←/→ one step, PageUp/PageDown ten, Home/End the ends.")
                .font_size(12.0)
                .color(Color::from_argb(255, 120, 120, 120))
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
                .size(460.0, 720.0)
                .title("Range Slider Demo")
                .build(ctx, range_slider_demo);
        });
    });
}
