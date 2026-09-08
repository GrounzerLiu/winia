//! Floating Action Button demo：尺寸/颜色变体、Extended FAB（收展动画）、
//! 滚动内容与布局方向（LTR/RTL）切换。

use letclone::clone;
use winia::prelude::*;

const STAR_PATH: &str = "M12 17.27L18.18 21l-1.64-7.03L22 9.24l-7.19-.61L12 2 9.19 8.63 2 9.24l5.46 4.73L5.82 21z";
const PLUS_PATH: &str = "M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z";

#[composable]
fn section_title(ctx: &mut ComposeCtx, title: &str) {
    Text::new(title.to_string())
        .font_size(14.0)
        .build(ctx);
}

#[composable]
fn floating_action_button_demo(ctx: &mut ComposeCtx) {
    let clicks = ctx.remember(|| 0i32);
    let rtl = ctx.remember(|| false);
    let ext_expanded = ctx.remember(|| true);
    let scroll = ctx.remember(|| ScrollState::new()).get();
    let direction = if rtl.get() { LayoutDirection::Rtl } else { LayoutDirection::Ltr };
    let click_count = clicks.get();
    let theme = WiniaTheme::colors();

    WiniaTheme::with_theme_and_direction(ThemeColors::default_light(), direction, ctx, |ctx| {
        Column::new()
            .modifier(Modifier::new().fill_max_size().vertical_scroll(scroll))
            .spacing(18.0)
            .build(ctx, |ctx| {
                // ── 顶栏：标题 + 方向切换 ──
                Row::new()
                    .modifier(Modifier::new().fill_max_width())
                    .build(ctx, |ctx| {
                        Column::new().build(ctx, |ctx| {
                            Text::new("Floating Action Button")
                                .font_size(24.0)
                                .build(ctx);
                            Text::new(format!(
                                "Clicks: {} | direction: {:?}",
                                click_count, direction
                            ))
                            .font_size(14.0)
                            .build(ctx);
                        });
                        Spacer::horizontal(12.0).build(ctx);
                        Button::text()
                            .on_click({ clone!(rtl); move || rtl.update(|v| *v = !*v) })
                            .build(ctx, |ctx| {
                                Text::new(if rtl.get() { "LTR" } else { "RTL" }).build(ctx)
                            });
                    });

                // ── 尺寸变体 ──
                section_title(ctx, "Sizes (small / regular / medium / large)");
                Row::new().spacing(16.0).build(ctx, |ctx| {
                    for size in [
                        FloatingActionButtonSize::Small,
                        FloatingActionButtonSize::Regular,
                        FloatingActionButtonSize::Medium,
                        FloatingActionButtonSize::Large,
                    ] {
                        FloatingActionButton::new()
                            .size(size)
                            .on_click({ clone!(clicks); move || clicks.update(|v| *v += 1) })
                            .build(ctx, |ctx| {
                                Icon::svg_path(STAR_PATH)
                                    .size(size.icon_size())
                                    .build(ctx);
                            });
                    }
                });

                // ── 颜色映射 ──
                section_title(ctx, "Primary / secondary / tertiary (+ disabled)");
                Row::new().spacing(16.0).build(ctx, |ctx| {
                    FloatingActionButton::new()
                        .colors(FloatingActionButtonDefaults::primary_colors(&theme))
                        .on_click({ clone!(clicks); move || clicks.update(|v| *v += 1) })
                        .build(ctx, |ctx| Icon::svg_path(STAR_PATH).build(ctx));
                    FloatingActionButton::new()
                        .colors(FloatingActionButtonDefaults::secondary_colors(&theme))
                        .on_click({ clone!(clicks); move || clicks.update(|v| *v += 1) })
                        .build(ctx, |ctx| Icon::svg_path(STAR_PATH).build(ctx));
                    FloatingActionButton::new()
                        .colors(FloatingActionButtonDefaults::tertiary_colors(&theme))
                        .enabled(false)
                        .on_click(|| {})
                        .build(ctx, |ctx| Icon::svg_path(STAR_PATH).build(ctx));
                });

                // ── 高程与自定义形状 ──
                section_title(ctx, "Lowered elevation / custom circle shape");
                Row::new().spacing(16.0).build(ctx, |ctx| {
                    FloatingActionButton::medium()
                        .elevation(FloatingActionButtonDefaults::lowered_elevation())
                        .on_click(|| {})
                        .build(ctx, |ctx| Icon::svg_path(STAR_PATH).build(ctx));
                    FloatingActionButton::new()
                        .shape(Shape::Circle)
                        .on_click(|| {})
                        .build(ctx, |ctx| Icon::svg_path(STAR_PATH).build(ctx));
                });

                // ── Extended FAB（M3 扩展 FAB）──
                section_title(ctx,
                    "Extended FAB - collapsed 56x56 / expanded [16|icon|12|text|20] min-w 80");
                Row::new().spacing(16.0).build(ctx, |ctx| {
                    // 绑定开合状态——宽度/图标位置/文本透明度按进度插值
                    ExtendedFloatingActionButton::new(
                        |ctx| Text::new("Create").build(ctx),
                        |ctx| Icon::svg_path(PLUS_PATH).size(24.0).build(ctx),
                        ext_expanded.clone(),
                    )
                    .on_click({ clone!(clicks); move || clicks.update(|v| *v += 1) })
                    .build(ctx);

                    Button::text()
                        .on_click({ clone!(ext_expanded); move || ext_expanded.update(|v| *v = !*v) })
                        .build(ctx, |ctx| {
                            Text::new(if ext_expanded.get() { "Collapse" } else { "Expand" })
                                .build(ctx)
                        });
                });
                Text::new(
                    "Toggle 切换收起/展开：宽度、图标位置、文本透明度均按进度插值\n（收起态同普通 FAB 56×56 仅图标居中；展开 min 宽 80）",
                )
                .font_size(12.0)
                .color(Color::from_argb(255, 90, 90, 90))
                .build(ctx);

                // ── 填充内容验证滚动 ──
                section_title(ctx, "Scroll filler");
                for i in 0..12 {
                    Text::new(format!("Filler row {i} - keep scrolling"))
                        .font_size(13.0)
                        .color(Color::from_argb(255, 120, 120, 120))
                        .build(ctx);
                }
            });
    });
}

fn main() {
    winia::run_app!(|ctx| {
        winia::ui::theme::WiniaTheme::light(ctx, |ctx| {
            Window::new()
                .size(720.0, 520.0)
                .title("Floating Action Button Demo")
                .build(ctx, |ctx| floating_action_button_demo(ctx));
        });
    });
}
