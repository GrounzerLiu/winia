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
    // 全局布局方向切换（RTL/LTR——顶部固定按钮，不跟随滚动）
    let rtl_state = ctx.remember(|| false);
    // ScrollState 必须 remember（跨重组保留 offset）
    let scroll_y = ctx.remember(|| winia::modifier::ScrollState::new()).get();

    // ── 顶部固定：方向切换按钮（不滚动） ──
    Row::new()
        .modifier(Modifier::new().padding(8.0))
        .build(ctx, |ctx| {
            Button::new()
                .on_click({ let r = rtl_state.clone(); move || r.set(!r.get()) })
                .modifier(Modifier::new().size(200.0, 36.0))
                .build(ctx, |ctx| {
                    Text::new(if rtl_state.get() { "切换为 LTR（当前 RTL）" } else { "切换为 RTL（当前 LTR）" })
                        .font_size(12.0)
                        .color(Color::WHITE)
                        .build(ctx);
                });
        });

    let dir = if rtl_state.get() {
        winia::layout::LayoutDirection::Rtl
    } else {
        winia::layout::LayoutDirection::Ltr
    };
    // 整个 demo 的方向作用域（对标 Compose CompositionLocalProvider
    // (LocalLayoutDirection)——所有 Row/Text/padding start-end 随方向镜像）
    WiniaTheme::with_theme_and_direction(WiniaTheme::colors(), dir, ctx, |ctx| {
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
            TextField::new(v1.clone())
                .no_container()
                .placeholder(|ctx| { winia::ui::Text::new("请输入内容…").build(ctx); })
                .single_line(true)
                .modifier(Modifier::new().size(300.0, 36.0))
                .build(ctx);
            // 只读
            let v2 = ctx.remember(|| winia::ui::text_field::TextFieldValue::new("只读文本"));
            TextField::new(v2.clone())
                .no_container()
                .read_only(true)
                .modifier(Modifier::new().size(300.0, 36.0))
                .build(ctx);
            // 禁用
            let v3 = ctx.remember(|| winia::ui::text_field::TextFieldValue::new("禁用"));
            TextField::new(v3.clone())
                .no_container()
                .enabled(false)
                .modifier(Modifier::new().size(300.0, 36.0))
                .build(ctx);
            // 多行 + minLines=3（宽度限定——size 含 Fixed(0) 高度会覆盖
            // minLines 动态高度）
            let v4 = ctx.remember(|| winia::ui::text_field::TextFieldValue::new("多行输入\n第二行\n第三行"));
            TextField::new(v4.clone())
                .no_container()
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

            // ── 8. padding（start/end/单边/动态动画） ──
            Text::new("8. padding start/end/单边 + 动态（动画作用于 padding）")
                .font_size(14.0)
                .color(Color::from_argb(200, 100, 100, 100))
                .modifier(Modifier::new().padding_vertical(8.0))
                .build(ctx);
            let pad_anim = ctx.animate_float_as_state(
                if clicked.get() { 40.0 } else { 4.0 },
                winia::animation::AnimationSpec::Spring(winia::animation::SpringSpec::bouncy()),
            );
            Button::new()
                .on_click({ let c = clicked.clone(); move || c.set(!c.get()) })
                .modifier(Modifier::new().size(160.0, 36.0))
                .build(ctx, |ctx| {
                    Text::new(if clicked.get() { "padding 40（点击还原）" } else { "padding 4（点击动画）" })
                        .font_size(12.0)
                        .color(Color::WHITE)
                        .build(ctx);
                });
            Column::new()
                .modifier(
                    Modifier::new()
                        .size(200.0, 60.0)
                        // 动态 padding：动画 State 驱动 → 每帧重测 → 内容平滑移动
                        .padding_sides(
                            pad_anim.clone(), // start 动画
                            8.0,              // top 固定
                            4.0,              // end 固定
                            2.0,              // bottom 固定
                        )
                        .background(Color::from_argb(255, 63, 81, 181), Shape::rounded(8.0)),
                )
                .build(ctx, |ctx| {
                    Text::new("内容随 padding_start 动画移动")
                        .font_size(13.0)
                        .color(Color::WHITE)
                        .build(ctx);
                });

            // ── 9. 布局方向（跟随顶部全局切换） ──
            Text::new("9. 布局方向（跟随全局切换：Row 镜像 / 文本右对齐 / padding start 在右）")
                .font_size(14.0)
                .color(Color::from_argb(200, 100, 100, 100))
                .modifier(Modifier::new().padding_vertical(8.0))
                .build(ctx);
            // Row：RTL 下子节点从右到左
            Row::new()
                .modifier(Modifier::new().padding_vertical(4.0))
                .build(ctx, |ctx| {
                    Text::new("[1]").font_size(13.0).build(ctx);
                    Text::new("[2]").font_size(13.0).build(ctx);
                    Text::new("[3]").font_size(13.0).build(ctx);
                });
            // 文本：RTL 下默认右对齐（fill_max_width 让文字框全宽——
            // 右对齐差异才可见；wrap 宽度时对齐无效果）
            Text::new("默认对齐跟随方向（未指定 align）——RTL 下此段文字靠右对齐，LTR 下靠左；多行换行时差异更明显。")
                .font_size(13.0)
                .modifier(Modifier::new().fill_max_width())
                .build(ctx);
            // padding start：RTL 下 start 在右
            Column::new()
                .modifier(
                    Modifier::new()
                        .size(200.0, 44.0)
                        .padding_start(12.0)
                        .background(Color::from_argb(255, 255, 152, 0), Shape::rounded(6.0)),
                )
                .build(ctx, |ctx| {
                    Text::new("padding_start 12")
                        .font_size(12.0)
                        .color(Color::WHITE)
                        .build(ctx);
                });

            // ── 10. offset（动态动画 + RTL 镜像 / absolute_offset 豁免） ──
            Text::new("10. offset（点击动画；RTL 下普通 offset 镜像、absolute_offset 豁免）")
                .font_size(14.0)
                .color(Color::from_argb(200, 100, 100, 100))
                .modifier(Modifier::new().padding_vertical(8.0))
                .build(ctx);
            let off_anim = ctx.animate_float_as_state(
                if clicked.get() { 60.0 } else { 0.0 },
                winia::animation::AnimationSpec::Spring(winia::animation::SpringSpec::bouncy()),
            );
            Button::new()
                .on_click({ let c = clicked.clone(); move || c.set(!c.get()) })
                .modifier(Modifier::new().size(180.0, 36.0))
                .build(ctx, |ctx| {
                    Text::new(if clicked.get() { "offset 60（点击还原）" } else { "offset 0（点击动画）" })
                        .font_size(12.0)
                        .color(Color::WHITE)
                        .build(ctx);
                });
            Row::new()
                .modifier(Modifier::new().padding_vertical(8.0))
                .build(ctx, |ctx| {
                    // 普通 offset：RTL 下动画方向镜像（x 反向）
                    Column::new()
                        .modifier(
                            Modifier::new()
                                .size(90.0, 36.0)
                                .offset_x(off_anim.clone())
                                .background(Color::from_argb(255, 76, 175, 80), Shape::rounded(6.0)),
                        )
                        .build(ctx, |ctx| {
                            Text::new("offset 镜像").font_size(11.0).color(Color::WHITE).build(ctx);
                        });
                    // absolute_offset：RTL 下不镜像
                    Column::new()
                        .modifier(
                            Modifier::new()
                                .size(90.0, 36.0)
                                .absolute_offset_x(off_anim.clone())
                                .background(Color::from_argb(255, 255, 152, 0), Shape::rounded(6.0)),
                        )
                        .build(ctx, |ctx| {
                            Text::new("absolute 不镜像").font_size(11.0).color(Color::WHITE).build(ctx);
                        });
                });
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