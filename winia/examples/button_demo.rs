//! Button 样式演示——不同样式/状态/形状/参数（对标 material3 Button）
//!
//! 展示：
//! - ButtonStyle: Filled / Tonal / Outlined / Text
//! - enabled / disabled 状态
//! - shape 参数（默认胶囊、圆角、矩形）
//! - content_padding / min_size 覆盖
//! - elevation（ElevatedButton 近似）与自定义 colors
//! - interactionSource hoist（实时显示 press/hover/focus 状态）

use winia::prelude::*;

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
    let c = count.clone();
    Row::new()
        .modifier(Modifier::new().padding_vertical(3.0))
        .build(ctx, |ctx| {
            Text::new(label)
                .font_size(13.0)
                .modifier(Modifier::new().width(150.0))
                .build(ctx);
            build_btn(&count).build(ctx, |ctx| {
                Text::new(format!("Click {}", c.get())).font_size(13.0).build(ctx);
            });
        });
}

#[composable]
fn button_demo(ctx: &mut ComposeCtx) {
    let scroll_y = ctx.remember(|| ScrollState::new()).get();
    let clicks = ctx.remember(|| 0i32);
    let c = clicks.clone();

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0).vertical_scroll(scroll_y))
        .build(ctx, |ctx| {
            Text::new("Button 样式演示（material3 对齐）")
                .font_size(20.0)
                .build(ctx);

            section_title(ctx, "样式（ButtonStyle）");
            demo_row(ctx, "Filled（默认）", |count| {
                let count = count.clone();
                Button::new().on_click(move || count.update(|v| *v += 1))
            });
            demo_row(ctx, "Tonal", |count| {
                let count = count.clone();
                Button::new().style(ButtonStyle::Tonal).on_click(move || count.update(|v| *v += 1))
            });
            demo_row(ctx, "Outlined", |count| {
                let count = count.clone();
                Button::new().style(ButtonStyle::Outlined).on_click(move || count.update(|v| *v += 1))
            });
            demo_row(ctx, "Text", |count| {
                let count = count.clone();
                Button::new().style(ButtonStyle::Text).on_click(move || count.update(|v| *v += 1))
            });

            section_title(ctx, "状态");
            demo_row(ctx, "Enabled", |count| {
                let count = count.clone();
                Button::new().on_click(move || count.update(|v| *v += 1))
            });
            demo_row(ctx, "Disabled", |count| {
                let count = count.clone();
                Button::new().enabled(false).on_click(move || count.update(|v| *v += 1))
            });

            section_title(ctx, "形状（shape）");
            demo_row(ctx, "默认胶囊", |count| {
                let count = count.clone();
                Button::new().on_click(move || count.update(|v| *v += 1))
            });
            demo_row(ctx, "rounded(4)", |count| {
                let count = count.clone();
                Button::new().shape(Shape::rounded(4.0)).on_click(move || count.update(|v| *v += 1))
            });
            demo_row(ctx, "Rectangle", |count| {
                let count = count.clone();
                Button::new().shape(Shape::Rectangle).on_click(move || count.update(|v| *v += 1))
            });

            section_title(ctx, "尺寸与内边距");
            demo_row(ctx, "紧凑 padding", |count| {
                let count = count.clone();
                Button::new().content_padding((10.0, 4.0, 10.0, 4.0))
                    .on_click(move || count.update(|v| *v += 1))
            });
            demo_row(ctx, "min_size(0,0)", |count| {
                let count = count.clone();
                Button::new().min_size(0.0, 0.0)
                    .on_click(move || count.update(|v| *v += 1))
            });
            demo_row(ctx, "min_size(120,48)", |count| {
                let count = count.clone();
                Button::new().min_size(120.0, 48.0)
                    .on_click(move || count.update(|v| *v += 1))
            });

            section_title(ctx, "阴影、颜色与边框");
            demo_row(ctx, "Elevated", |count| {
                let count = count.clone();
                Button::new().elevation(ButtonElevation::elevated())
                    .on_click(move || count.update(|v| *v += 1))
            });
            demo_row(ctx, "自定义 colors", |count| {
                let count = count.clone();
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
                let count = count.clone();
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
                        .on_click({ let c = c.clone(); move || c.update(|v| *v += 1) })
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
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();

    winia::run_app!(|ctx| {
        WiniaTheme::light(ctx, |ctx| {
            Window::new()
                .size(480.0, 760.0)
                .title("Button Demo")
                .build(ctx, button_demo);
        });
    });
}
