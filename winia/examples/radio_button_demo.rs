//! RadioButton 演示（material3 对齐）
//!
//! 展示：
//! - selected / unselected / disabled 状态
//! - 单选组联动（一组选项互斥选择——对齐 Compose RadioGroupSample）
//! - 自定义 colors
//! - interactionSource hoist（实时显示 press/hover/focus 状态）

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
fn state_row(ctx: &mut ComposeCtx, label: &str, selected: bool, enabled: bool) {
    Row::new()
        .modifier(Modifier::new().padding_vertical(3.0))
        .build(ctx, |ctx| {
            Text::new(label)
                .font_size(13.0)
                .modifier(Modifier::new().width(150.0).padding_top(10.0))
                .build(ctx);
            RadioButton::new(selected)
                .enabled(enabled)
                .on_click(|| {})
                .build(ctx);
        });
}

#[composable]
fn radio_button_demo(ctx: &mut ComposeCtx) {
    let scroll_y = ctx.remember(|| ScrollState::new()).get();
    // 单选组状态：当前选中项（demo 顶层——变化触发重组）
    let selected = ctx.remember(|| "选项 A".to_string());

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0).vertical_scroll(scroll_y))
        .build(ctx, |ctx| {
            Text::new("RadioButton 演示（material3 对齐）")
                .font_size(20.0)
                .build(ctx);

            section_title(ctx, "状态");
            state_row(ctx, "未选中", false, true);
            state_row(ctx, "已选中", true, true);
            state_row(ctx, "禁用未选中", false, false);
            state_row(ctx, "禁用已选中", true, false);

            section_title(ctx, "单选组联动");
            // 一组互斥选项：点击切换当前选中项（RadioGroup 语义）
            for label in ["选项 A", "选项 B", "选项 C"] {
                ctx.key(label, |ctx| {
                    Row::new()
                        .modifier(Modifier::new().padding_vertical(3.0))
                        .build(ctx, |ctx| {
                            RadioButton::new(selected.get() == label)
                                .on_click({ clone!(selected); move || selected.update(|v| *v = label.to_string()) })
                                .build(ctx);
                            Text::new(label)
                                .font_size(13.0)
                                .modifier(Modifier::new().padding_top(10.0).padding_start(8.0))
                                .build(ctx);
                        });
                });
            }
            Text::new(format!("当前选中：{}", selected.get()))
                .font_size(13.0)
                .color(Color::from_argb(255, 100, 100, 100))
                .modifier(Modifier::new().padding_top(4.0))
                .build(ctx);

            section_title(ctx, "自定义颜色");
            let theme = WiniaTheme::colors();
            let mut custom = RadioButtonColors::from_theme(&theme);
            custom.selected_color = Color::from_argb(255, 46, 125, 50);
            custom.unselected_color = Color::from_argb(255, 46, 125, 50);
            Row::new()
                .modifier(Modifier::new().padding_vertical(3.0))
                .build(ctx, |ctx| {
                    Text::new("自定义")
                        .font_size(13.0)
                        .modifier(Modifier::new().width(150.0).padding_top(10.0))
                        .build(ctx);
                    RadioButton::new(true)
                        .colors(custom)
                        .on_click(|| {})
                        .build(ctx);
                });

            section_title(ctx, "interactionSource hoist");
            let src = ctx.remember(|| MutableInteractionSource::new()).get();
            let st = src.state(true);
            Row::new()
                .modifier(Modifier::new().padding_vertical(3.0))
                .build(ctx, |ctx| {
                    Text::new("状态")
                        .font_size(13.0)
                        .modifier(Modifier::new().width(150.0).padding_top(10.0))
                        .build(ctx);
                    RadioButton::new(selected.get() == "选项 A")
                        .interaction_source(src.clone())
                        .on_click({ clone!(selected); move || selected.update(|v| *v = "选项 A".to_string()) })
                        .build(ctx);
                });
            Text::new(format!(
                "pressed={} hovered={} focused={} | 当前 {}",
                st.pressed, st.hovered, st.focused,
                selected.get(),
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
                .size(420.0, 600.0)
                .title("RadioButton Demo")
                .build(ctx, radio_button_demo);
        });
    });
}
