//! Checkbox 演示（material3 对齐）
//!
//! 展示：
//! - checked / unchecked / disabled 状态
//! - 点击切换（on_checked_change + State<bool>）
//! - 自定义 colors
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
fn state_row(ctx: &mut ComposeCtx, label: &str, checked: bool, enabled: bool) {
    Row::new()
        .modifier(Modifier::new().padding_vertical(3.0))
        .build(ctx, |ctx| {
            Text::new(label)
                .font_size(13.0)
                .modifier(Modifier::new().width(150.0).padding_top(10.0))
                .build(ctx);
            Checkbox::new(checked)
                .enabled(enabled)
                .on_checked_change(|_| {})
                .build(ctx);
        });
}

#[composable]
fn checkbox_demo(ctx: &mut ComposeCtx) {
    let scroll_y = ctx.remember(|| ScrollState::new()).get();
    let checked = ctx.remember(|| false);
    let c = checked.clone();

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0).vertical_scroll(scroll_y))
        .build(ctx, |ctx| {
            Text::new("Checkbox 演示（material3 对齐）")
                .font_size(20.0)
                .build(ctx);

            section_title(ctx, "状态");
            state_row(ctx, "未选中", false, true);
            state_row(ctx, "已选中", true, true);
            state_row(ctx, "禁用未选中", false, false);
            state_row(ctx, "禁用已选中", true, false);
            Row::new()
                .modifier(Modifier::new().padding_vertical(3.0))
                .build(ctx, |ctx| {
                    Text::new("禁用不确定")
                        .font_size(13.0)
                        .modifier(Modifier::new().width(150.0).padding_top(10.0))
                        .build(ctx);
                    TriStateCheckbox::new(ToggleableState::Indeterminate)
                        .enabled(false)
                        .build(ctx);
                });

            section_title(ctx, "TriState 父子联动");
            let c1 = ctx.remember(|| true);
            let c2 = ctx.remember(|| true);
            let c3 = ctx.remember(|| false);
            Row::new()
                .modifier(Modifier::new().padding_vertical(3.0))
                .build(ctx, |ctx| {
                    // ps 计算必须在 Row content 内：c1/c2/c3.get() 依赖注册到
                    // Row scope——子项变化 → Row Enter → 全选重算。
                    // 若在 Row 外（Column 顶层）计算，Row 参数未变 → Skip →
                    // 全选不随子项联动（视觉停在旧状态）。
                    let all = c1.get() && c2.get() && c3.get();
                    let none = !c1.get() && !c2.get() && !c3.get();
                    let parent_state = if all {
                        ToggleableState::On
                    } else if none {
                        ToggleableState::Off
                    } else {
                        ToggleableState::Indeterminate
                    };
                    Text::new("全选")
                        .font_size(13.0)
                        .modifier(Modifier::new().width(150.0).padding_top(10.0))
                        .build(ctx);
                    let (p1, p2, p3) = (c1.clone(), c2.clone(), c3.clone());
                    TriStateCheckbox::new(parent_state)
                        .on_click(move || {
                            let target = !(p1.get() && p2.get() && p3.get());
                            p1.update(|v| *v = target);
                            p2.update(|v| *v = target);
                            p3.update(|v| *v = target);
                        })
                        .build(ctx);
                });
            for (label, c) in [
                ("子项 1", c1.clone()),
                ("子项 2", c2.clone()),
                ("子项 3", c3.clone()),
            ] {
                // 列表必须显式 key：循环内 remember/next_key 的 per-base 序号按
                // "执行次数"分配——部分迭代 Skip 时序号前移 → 与历史 key 碰撞
                // （dup-key panic——Compose 同语义：列表项显式 key）
                ctx.key(label, |ctx| {
                    Row::new()
                        .modifier(Modifier::new().padding_sides(20.0, 3.0, 0.0, 3.0))
                        .build(ctx, |ctx| {
                            Text::new(label)
                                .font_size(13.0)
                                .modifier(Modifier::new().width(130.0).padding_top(10.0))
                                .build(ctx);
                            let cc = c.clone();
                            Checkbox::new(c.get())
                                .on_checked_change(move |v| cc.update(|s| *s = v))
                                .build(ctx);
                        });
                });
            }

            section_title(ctx, "点击切换");
            Row::new()
                .modifier(Modifier::new().padding_vertical(3.0))
                .build(ctx, |ctx| {
                    Text::new("切换")
                        .font_size(13.0)
                        .modifier(Modifier::new().width(150.0).padding_top(10.0))
                        .build(ctx);
                    let c2 = c.clone();
                    Checkbox::new(c.get())
                        .on_checked_change(move |v| c2.update(|s| *s = v))
                        .build(ctx);
                });
            Text::new(if c.get() { "已勾选" } else { "未勾选" })
                .font_size(13.0)
                .color(Color::from_argb(255, 100, 100, 100))
                .modifier(Modifier::new().padding_top(4.0))
                .build(ctx);

            section_title(ctx, "自定义颜色");
            let theme = WiniaTheme::colors();
            let mut custom = CheckboxColors::from_theme(&theme);
            custom.checked_box = Color::from_argb(255, 46, 125, 50);
            custom.checked_border = custom.checked_box;
            custom.checked_checkmark = Color::WHITE;
            custom.unchecked_border = Color::from_argb(255, 46, 125, 50);
            Row::new()
                .modifier(Modifier::new().padding_vertical(3.0))
                .build(ctx, |ctx| {
                    Text::new("自定义")
                        .font_size(13.0)
                        .modifier(Modifier::new().width(150.0).padding_top(10.0))
                        .build(ctx);
                    let c2 = c.clone();
                    Checkbox::new(c.get())
                        .colors(custom)
                        .on_checked_change(move |v| c2.update(|s| *s = v))
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
                    let c2 = c.clone();
                    Checkbox::new(c.get())
                        .interaction_source(src.clone())
                        .on_checked_change(move |v| c2.update(|s| *s = v))
                        .build(ctx);
                });
            Text::new(format!(
                "pressed={} hovered={} focused={} | 当前 {}",
                st.pressed, st.hovered, st.focused,
                if c.get() { "已勾选" } else { "未勾选" },
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
                .size(420.0, 600.0)
                .title("Checkbox Demo")
                .build(ctx, checkbox_demo);
        });
    });
}
