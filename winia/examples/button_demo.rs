//! Button 样式演示——不同样式/状态/形状/参数（对标 material3 Button）
//!
//! 展示：
//! - ButtonStyle: Filled / Tonal / Outlined / Text
//! - enabled / disabled 状态
//! - shape 参数（默认胶囊、圆角、矩形）
//! - content_padding / min_size 覆盖
//! - elevation（ElevatedButton 近似）与自定义 colors
//! - interactionSource hoist（实时显示 press/hover/focus 状态）

use letclone::clone;
use winia::prelude::*;
use winia::animation::{AnimationSpec, TweenSpec};

fn section_title(ctx: &mut ComposeCtx, text: &str) {
    Text::new(text)
        .font_size(14.0)
        .color(Color::from_argb(255, 90, 90, 90))
        .modifier(Modifier::new().padding_top(14.0).padding_bottom(6.0))
        .build(ctx);
}

#[composable]
fn demo_row(ctx: &mut ComposeCtx, label: &str, build_btn: impl FnOnce(&State<i32>) -> Button) {
    let count = ctx.remember(|| 0i32);
    Row::new()
        .modifier(Modifier::new().padding_vertical(3.0))
        .build(ctx, |ctx| {
            Text::new(label)
                .font_size(13.0)
                .modifier(Modifier::new().width(150.0))
                .build(ctx);
            build_btn(&count).build(ctx, |ctx| {
                Text::new(format!("Click {}", count.get())).font_size(13.0).build(ctx);
            });
        });
}

#[composable]
fn button_demo(ctx: &mut ComposeCtx) {
    let scroll_y = ctx.remember(|| ScrollState::new()).get();
    let clicks = ctx.remember(|| 0i32);

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0).vertical_scroll(scroll_y))
        .build(ctx, |ctx| {
            Text::new("Button 样式演示（material3 对齐）")
                .font_size(20.0)
                .build(ctx);

            section_title(ctx, "样式（ButtonStyle）");
            demo_row(ctx, "Button::filled()", |count| {
                clone!(count);
                Button::filled().on_click(move || count.update(|v| *v += 1))
            });
            demo_row(ctx, "Button::elevated()", |count| {
                clone!(count);
                Button::elevated().on_click(move || count.update(|v| *v += 1))
            });
            demo_row(ctx, "Button::filled_tonal()", |count| {
                clone!(count);
                Button::filled_tonal().on_click(move || count.update(|v| *v += 1))
            });
            demo_row(ctx, "Button::outlined()", |count| {
                clone!(count);
                Button::outlined().on_click(move || count.update(|v| *v += 1))
            });
            demo_row(ctx, "Button::text()", |count| {
                clone!(count);
                Button::text().on_click(move || count.update(|v| *v += 1))
            });

            section_title(ctx, "状态");
            demo_row(ctx, "Enabled", |count| {
                clone!(count);
                Button::new().on_click(move || count.update(|v| *v += 1))
            });
            demo_row(ctx, "Disabled", |count| {
                clone!(count);
                Button::new().enabled(false).on_click(move || count.update(|v| *v += 1))
            });

            section_title(ctx, "形状（shape）");
            demo_row(ctx, "默认胶囊", |count| {
                clone!(count);
                Button::new().on_click(move || count.update(|v| *v += 1))
            });
            demo_row(ctx, "rounded(4)", |count| {
                clone!(count);
                Button::new().shape(Shape::rounded(4.0)).on_click(move || count.update(|v| *v += 1))
            });
            demo_row(ctx, "Rectangle", |count| {
                clone!(count);
                Button::new().shape(Shape::Rectangle).on_click(move || count.update(|v| *v += 1))
            });

            section_title(ctx, "尺寸与内边距");
            demo_row(ctx, "紧凑 padding", |count| {
                clone!(count);
                Button::new().content_padding((10.0, 4.0, 10.0, 4.0))
                    .on_click(move || count.update(|v| *v += 1))
            });
            demo_row(ctx, "min_size(0,0)", |count| {
                clone!(count);
                Button::new().min_size(0.0, 0.0)
                    .on_click(move || count.update(|v| *v += 1))
            });
            demo_row(ctx, "min_size(120,48)", |count| {
                clone!(count);
                Button::new().min_size(120.0, 48.0)
                    .on_click(move || count.update(|v| *v += 1))
            });
            // 动画 min_size：SizeValue 动态闭包——measure 期求值，动画期间只重测不重组
            let min_toggle = ctx.remember(|| false);
            let min_target = if min_toggle.get() { 140.0 } else { 58.0 };
            let min_w_anim = ctx.animate_float_as_state(
                min_target,
                AnimationSpec::Tween(TweenSpec::new(
                    Duration::from_millis(400),
                    winia::animation::interpolator::EaseOutCubic::new(),
                )),
            );
            demo_row(ctx, "动画 min_size", |_| {
                Button::new()
                    .min_size(min_w_anim.clone(), {
                        clone!(min_w_anim);
                        move || min_w_anim.get() * 0.4
                    })
                    .on_click({ clone!(min_toggle); move || { min_toggle.update(|v| *v = !*v); } })
            });

            section_title(ctx, "尺寸变体（XSmall ~ XLarge）");
            Row::new()
                .modifier(Modifier::new().padding_vertical(3.0))
                .spacing(12.0)
                .build(ctx, |ctx| {
                    for size in [
                        ButtonSize::XSmall,
                        ButtonSize::Small,
                        ButtonSize::Medium,
                        ButtonSize::Large,
                        ButtonSize::XLarge,
                    ] {
                        Button::filled().size(size).on_click(|| {}).build(ctx, |ctx| {
                            Text::new("按钮").font_size(12.0).build(ctx);
                        });
                    }
                });
            Text::new("XSmall 32 / Small 40 / Medium 56 / Large 96 / XLarge 136")
                .font_size(12.0)
                .color(Color::from_argb(255, 100, 100, 100))
                .build(ctx);

            section_title(ctx, "带图标（Icon + Text，内容 = 居中 Row）");
            let star = "M12 17.27L18.18 21l-1.64-7.03L22 9.24l-7.19-.61L12 2 9.19 8.63 2 9.24l5.46 4.73L5.82 21z";
            Row::new()
                .modifier(Modifier::new().padding_vertical(3.0))
                .spacing(12.0)
                .build(ctx, |ctx| {
                    Button::filled()
                        .content_padding(ButtonDefaults::button_with_icon_content_padding())
                        .on_click(|| {})
                        .build(ctx, |ctx| {
                            Icon::svg_path(star).build(ctx);
                            Text::new("收藏").font_size(13.0).build(ctx);
                        });
                    Button::outlined()
                        .content_padding(ButtonDefaults::button_with_icon_content_padding())
                        .on_click(|| {})
                        .build(ctx, |ctx| {
                            Icon::svg_path(star).build(ctx);
                            Text::new("加星").font_size(13.0).build(ctx);
                        });
                    Button::elevated()
                        .content_padding(ButtonDefaults::button_with_icon_content_padding())
                        .on_click(|| {})
                        .build(ctx, |ctx| {
                            Icon::svg_path(star).build(ctx);
                            Text::new("收藏").font_size(13.0).build(ctx);
                        });
                    Button::text()
                        .content_padding(ButtonDefaults::text_button_with_icon_content_padding())
                        .on_click(|| {})
                        .build(ctx, |ctx| {
                            Icon::svg_path(star).build(ctx);
                            Text::new("加星").font_size(13.0).build(ctx);
                        });
                });

            section_title(ctx, "阴影、颜色与边框");
            demo_row(ctx, "Elevated", |count| {
                clone!(count);
                Button::new().elevation(ButtonElevation::elevated())
                    .on_click(move || count.update(|v| *v += 1))
            });
            demo_row(ctx, "自定义 colors", |count| {
                clone!(count);
                Button::new()
                    .colors(ButtonColors::new(
                        Color::from_argb(255, 126, 87, 194),
                        Color::WHITE,
                        Color::from_argb(120, 126, 87, 194),
                        Color::from_argb(180, 255, 255, 255),
                    ))
                    .on_click(move || count.update(|v| *v += 1))
            });
            demo_row(ctx, "自定义 border", |count| {
                clone!(count);
                Button::new()
                    .border(ButtonBorder::new(2.0, Color::from_argb(255, 33, 150, 243)))
                    .on_click(move || count.update(|v| *v += 1))
            });

            section_title(ctx, "interactionSource hoist");
            let src = ctx.remember(|| MutableInteractionSource::new()).get();
            let st = src.state(true);
            Row::new()
                .modifier(Modifier::new().padding_vertical(3.0))
                .build(ctx, |ctx| {
                    Text::new("状态")
                        .font_size(13.0)
                        .modifier(Modifier::new().width(150.0))
                        .build(ctx);
                    Button::new()
                        .interaction_source(src.clone())
                        .on_click({ clone!(clicks); move || clicks.update(|v| *v += 1) })
                        .build(ctx, |ctx| {
                            Text::new("Hover / Press / Focus").font_size(13.0).build(ctx);
                        });
                });
            Text::new(format!(
                "pressed={} hovered={} focused={} | 总点击 {}",
                st.pressed, st.hovered, st.focused, clicks.get(),
            ))
                .font_size(12.0)
                .color(Color::from_argb(255, 100, 100, 100))
                .modifier(Modifier::new().padding_top(4.0))
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
                .size(480.0, 760.0)
                .title("Button Demo")
                .build(ctx, button_demo);
        });
    });
}
