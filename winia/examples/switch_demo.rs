//! Switch 演示（material3 对齐）
//!
//! 展示：
//! - checked / unchecked / disabled 状态
//! - 点击切换（on_checked_change + State<bool>）
//! - thumbContent（拇指内图标）
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
fn state_row(ctx: &mut ComposeCtx, label: &str, checked: bool, enabled: bool) {
    Row::new()
        .modifier(Modifier::new().padding_vertical(3.0))
        .build(ctx, |ctx| {
            Text::new(label)
                .font_size(13.0)
                .modifier(Modifier::new().width(150.0).padding_top(8.0))
                .build(ctx);
            Switch::new(checked)
                .enabled(enabled)
                .on_checked_change(|_| {})
                .build(ctx, |_| {});
        });
}

#[composable]
fn switch_demo(ctx: &mut ComposeCtx) {
    let scroll_y = ctx.remember(|| ScrollState::new()).get();
    let checked = ctx.remember(|| false);

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0).vertical_scroll(scroll_y))
        .build(ctx, |ctx| {
            Text::new("Switch 演示（material3 对齐）")
                .font_size(20.0)
                .build(ctx);

            section_title(ctx, "状态");
            state_row(ctx, "未选中", false, true);
            state_row(ctx, "已选中", true, true);
            state_row(ctx, "禁用未选中", false, false);
            state_row(ctx, "禁用已选中", true, false);

            section_title(ctx, "点击切换");
            Row::new()
                .modifier(Modifier::new().padding_vertical(3.0))
                .build(ctx, |ctx| {
                    Text::new("切换")
                        .font_size(13.0)
                        .modifier(Modifier::new().width(150.0).padding_top(8.0))
                        .build(ctx);
                    Switch::new(checked.get())
                        .on_checked_change({ clone!(checked); move |v| checked.update(|s| *s = v) })
                        .build(ctx, |_| {});
                });
            Text::new(if checked.get() { "已开启" } else { "已关闭" })
                .font_size(13.0)
                .color(Color::from_argb(255, 100, 100, 100))
                .modifier(Modifier::new().padding_top(4.0))
                .build(ctx);

            section_title(ctx, "thumbContent 图标");
            Row::new()
                .modifier(Modifier::new().padding_vertical(3.0))
                .build(ctx, |ctx| {
                    Text::new("带图标")
                        .font_size(13.0)
                        .modifier(Modifier::new().width(150.0).padding_top(8.0))
                        .build(ctx);
                    Switch::new(checked.get())
                        .on_checked_change({ clone!(checked); move |v| checked.update(|s| *s = v) })
                        .build(ctx, |ctx| {
                            // 拇指内 16dp 图标（tint Auto 跟随 icon_color）
                            Icon::svg_path("M12 2L22 12 12 22 2 12Z")
                                .size(SwitchDefaults::icon_size())
                                .build(ctx);
                        });
                });

            section_title(ctx, "自定义颜色");
            let theme = WiniaTheme::colors();
            let mut custom = SwitchColors::from_theme(&theme);
            custom.checked_track = Color::from_argb(255, 46, 125, 50);
            custom.checked_thumb = Color::WHITE;
            custom.unchecked_track = Color::from_argb(255, 224, 224, 224);
            custom.unchecked_thumb = Color::from_argb(255, 100, 100, 100);
            custom.unchecked_border = custom.unchecked_thumb;
            Row::new()
                .modifier(Modifier::new().padding_vertical(3.0))
                .build(ctx, |ctx| {
                    Text::new("自定义")
                        .font_size(13.0)
                        .modifier(Modifier::new().width(150.0).padding_top(8.0))
                        .build(ctx);
                    Switch::new(checked.get())
                        .colors(custom)
                        .on_checked_change({ clone!(checked); move |v| checked.update(|s| *s = v) })
                        .build(ctx, |_| {});
                });

            section_title(ctx, "interactionSource hoist");
            let src = ctx.remember(|| MutableInteractionSource::new()).get();
            let st = src.state(true);
            Row::new()
                .modifier(Modifier::new().padding_vertical(3.0))
                .build(ctx, |ctx| {
                    Text::new("状态")
                        .font_size(13.0)
                        .modifier(Modifier::new().width(150.0).padding_top(8.0))
                        .build(ctx);
                    Switch::new(checked.get())
                        .interaction_source(src.clone())
                        .on_checked_change({ clone!(checked); move |v| checked.update(|s| *s = v) })
                        .build(ctx, |_| {});
                });
            Text::new(format!(
                "pressed={} hovered={} focused={} | 当前 {}",
                st.pressed, st.hovered, st.focused,
                if checked.get() { "已开启" } else { "已关闭" },
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
                .size(420.0, 560.0)
                .title("Switch Demo")
                .build(ctx, switch_demo);
        });
    });
}
