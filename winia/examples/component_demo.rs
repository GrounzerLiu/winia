//! 组件属性与 Modifier 补全演示
//!
//! 展示 component-polish 分支新增能力：
//! - Modifier: alpha/rotate/scale（绕中心变换）、shadow、aspect_ratio、
//!   required_size、test_tag
//! - Button: enabled + ButtonColors（disabled 变体）
//! - TextField: enabled/readOnly/placeholder/singleLine/maxLines/minLines
//! - Text: letterSpacing/lineHeight

use winia::prelude::*;
use winia::app;

#[composable]
fn component_demo(ctx: &mut ComposeCtx) {
    let clicked = ctx.remember(|| false);
    let c = clicked.clone();
    let show = ctx.remember(|| false);
    let s = show.clone();
    // ScrollState 必须 remember（跨重组保留 offset）
    let scroll_y = ctx.remember(|| winia::modifier::ScrollState::new()).get();

    Column::new()
        .modifier(Modifier::new()
            .padding_vertical(8.0)
            .fill_max_size()
            .vertical_scroll(scroll_y))
        .build(ctx, |ctx| {
            // ── 1. alpha/rotate/scale（绕中心变换）──
            Text::new("1. alpha / rotate / scale（绕中心——transformOrigin 默认 Center）")
                .font_size(14.0)
                .color(Color::from_argb(200, 100, 100, 100))
                .modifier(Modifier::new().padding_vertical(8.0))
                .build(ctx);
            Column::new()
                .modifier(Modifier::new()
                    .size(200.0, 80.0)
                    .background(Color::from_argb(255, 66, 133, 244), Shape::rounded(8.0))
                    .alpha(if c.get() { 0.3 } else { 1.0 })
                    .rotate(if c.get() { 15.0 } else { 0.0 })
                    .scale(if c.get() { 1.2 } else { 1.0 }, if c.get() { 1.2 } else { 1.0 }))
                .build(ctx, |ctx| {
                    Text::new("变换（点击切换）")
                        .color(Color::WHITE)
                        .font_size(14.0)
                        .build(ctx);
                });

            // ── 2. shadow ──
            Text::new("2. shadow（elevation 便捷版 / drop_shadow 自定义）")
                .font_size(14.0)
                .color(Color::from_argb(200, 100, 100, 100))
                .modifier(Modifier::new().padding_vertical(8.0))
                .build(ctx);
            Row::new().build(ctx, |ctx| {
                Column::new()
                    .modifier(Modifier::new()
                        .size(120.0, 60.0)
                        .shadow(8.0, Shape::rounded(12.0), true, Color::from_argb(255, 0, 0, 0))
                        .background(Color::from_argb(255, 76, 175, 80), Shape::rounded(12.0)))
                    .build(ctx, |ctx| {
                        Text::new("shadow 8").color(Color::WHITE).font_size(13.0).build(ctx);
                    });
                Column::new()
                    .modifier(Modifier::new()
                        .size(120.0, 60.0)
                        .drop_shadow(Shape::rounded(12.0), winia::modifier::ShadowParams::new(6.0, 4.0, 6.0, Color::from_argb(180, 100, 60, 0), 0.8))
                        .background(Color::from_argb(255, 255, 152, 0), Shape::rounded(12.0)))
                    .build(ctx, |ctx| {
                        Text::new("drop 自定义").color(Color::WHITE).font_size(13.0).build(ctx);
                    });
            });

            // ── 3. aspect_ratio + required_size ──
            Text::new("3. aspect_ratio(2) + required_size(溢出父约束)")
                .font_size(14.0)
                .color(Color::from_argb(200, 100, 100, 100))
                .modifier(Modifier::new().padding_vertical(8.0))
                .build(ctx);
            Row::new().build(ctx, |ctx| {
                // aspect_ratio 2:1（约束内推导）
                Column::new()
                    .modifier(Modifier::new()
                        .width(200.0)
                        .aspect_ratio(2.0, false)
                        .background(Color::from_argb(255, 156, 39, 176), Shape::rounded(4.0)))
                    .build(ctx, |ctx| {
                        Text::new("aspect 2:1").color(Color::WHITE).font_size(12.0).build(ctx);
                    });
            });
            // required_size：父约束 300x50，requiredSize 溢出到 360x60
            Column::new()
                .modifier(Modifier::new()
                    .size(300.0, 50.0)
                    .padding(4.0)
                    .background(Color::from_argb(60, 0, 0, 0), Shape::rounded(4.0)))
                .build(ctx, |ctx| {
                    Column::new()
                        .modifier(Modifier::new()
                            .required_size(360.0, 60.0)
                            .background(Color::from_argb(255, 33, 150, 243), Shape::rounded(4.0)))
                        .build(ctx, |ctx| {
                            Text::new("required 360x60（溢出）").color(Color::WHITE).font_size(12.0).build(ctx);
                        });
                });

            // ── 4. Button enabled + colors ──
            Text::new("4. Button enabled + ButtonColors")
                .font_size(14.0)
                .color(Color::from_argb(200, 100, 100, 100))
                .modifier(Modifier::new().padding_vertical(8.0))
                .build(ctx);
            Row::new().build(ctx, |ctx| {
                Button::new()
                    .on_click(move || c.set(!c.get()))
                    .modifier(Modifier::new().size(120.0, 40.0))
                    .build(ctx, |ctx| {
                        Text::new("切换").color(Color::WHITE).build(ctx);
                    });
                Button::new()
                    .enabled(false)
                    .on_click(move || {})
                    .modifier(Modifier::new().size(120.0, 40.0))
                    .build(ctx, |ctx| {
                        Text::new("禁用").color(Color::WHITE).build(ctx);
                    });
                Button::new()
                    .colors(ButtonColors::new(
                        Color::from_argb(255, 255, 87, 34),
                        Color::from_argb(255, 255, 255, 255),
                        Color::from_argb(60, 255, 87, 34),
                        Color::from_argb(120, 255, 255, 255),
                    ))
                    .on_click(move || {})
                    .modifier(Modifier::new().size(120.0, 40.0))
                    .build(ctx, |ctx| {
                        Text::new("自定义色").color(Color::WHITE).build(ctx);
                    });
            });

            // ── 5. TextField 状态 ──
            Text::new("5. TextField enabled / readOnly / placeholder / 多行")
                .font_size(14.0)
                .color(Color::from_argb(200, 100, 100, 100))
                .modifier(Modifier::new().padding_vertical(8.0))
                .build(ctx);
            // 单行 + placeholder
            let v1 = ctx.remember(|| winia::ui::text_field::TextFieldValue::new(""));
            TextField::new(v1.clone(), |_| {})
                .placeholder("请输入内容…")
                .single_line(true)
                .modifier(Modifier::new().size(300.0, 36.0))
                .build(ctx);
            // 只读
            let v2 = ctx.remember(|| winia::ui::text_field::TextFieldValue::new("只读文本"));
            TextField::new(v2.clone(), |_| {})
                .read_only(true)
                .modifier(Modifier::new().size(300.0, 36.0))
                .build(ctx);
            // 禁用
            let v3 = ctx.remember(|| winia::ui::text_field::TextFieldValue::new("禁用"));
            TextField::new(v3.clone(), |_| {})
                .enabled(false)
                .modifier(Modifier::new().size(300.0, 36.0))
                .build(ctx);
            // 多行 + minLines=3（宽度限定——size 含 Fixed(0) 高度会覆盖
            // minLines 动态高度）
            let v4 = ctx.remember(|| winia::ui::text_field::TextFieldValue::new("多行输入\n第二行\n第三行"));
            TextField::new(v4.clone(), |_| {})
                .min_lines(3)
                .modifier(Modifier::new().width(300.0))
                .build(ctx);

            // ── 6. Text letterSpacing / lineHeight ──
            Text::new("6. Text letterSpacing / lineHeight")
                .font_size(14.0)
                .color(Color::from_argb(200, 100, 100, 100))
                .modifier(Modifier::new().padding_vertical(8.0))
                .build(ctx);
            Text::new("字间距 3px")
                .letter_spacing(3.0)
                .font_size(16.0)
                .build(ctx);
            Text::new("行高 32px\n第二行")
                .line_height(32.0)
                .font_size(16.0)
                .build(ctx);

            // ── 7. test_tag ──
            Text::new("7. test_tag（调试树 tag 字段定位）")
                .font_size(14.0)
                .color(Color::from_argb(200, 100, 100, 100))
                .modifier(Modifier::new().padding_vertical(8.0))
                .build(ctx);
            Button::new()
                .on_click(move || s.set(!s.get()))
                .modifier(Modifier::new().size(140.0, 40.0).test_tag("toggle-btn"))
                .build(ctx, |ctx| {
                    Text::new(if show.get() { "开" } else { "关" })
                        .color(Color::WHITE)
                        .build(ctx);
                });
        });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();

    winia::run_app!(|ctx| {
        // 亮色主题：暗色背景上看不清阴影
        WiniaTheme::light(ctx, |ctx| {
            Window::new()
                .size(420.0, 720.0)
                .title("Component Polish Demo")
                .build(ctx, component_demo);
        });
    });
}
