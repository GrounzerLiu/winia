//! Icon 演示——SVG path / SVG 字符串 / 图片文件 / 可变字体（feature 启用时）/ RTL 镜像

use winia::prelude::*;

fn section_title(ctx: &mut ComposeCtx, text: &str) {
    Text::new(text)
        .font_size(14.0)
        .color(Color::from_argb(255, 90, 90, 90))
        .modifier(Modifier::new().padding_top(14.0).padding_bottom(6.0))
        .build(ctx);
}

#[composable]
fn icon_demo(ctx: &mut ComposeCtx) {
    let rtl = ctx.remember(|| false);
    let theme = WiniaTheme::colors();
    let toggle = rtl.clone();
    let dir = if rtl.get() { LayoutDirection::Rtl } else { LayoutDirection::Ltr };
    WiniaTheme::with_theme_and_direction(theme.clone(), dir, ctx, |ctx| {
        let scroll_y = ctx.remember(|| ScrollState::new()).get();
        Column::new()
            .modifier(Modifier::new().fill_max_size().padding(16.0).vertical_scroll(scroll_y))
            .build(ctx, |ctx| {
                Row::new().modifier(Modifier::new().padding_bottom(6.0)).build(ctx, |ctx| {
                    Text::new(if rtl.get() { "Icon 演示（RTL）" } else { "Icon 演示（LTR）" })
                        .font_size(20.0)
                        .build(ctx);
                    Button::new()
                        .on_click(move || toggle.update(|v| *v = !*v))
                        .modifier(Modifier::new().padding_start(16.0))
                        .build(ctx, |ctx| {
                            Text::new("切换 LTR/RTL").font_size(12.0).build(ctx);
                        });
                });

            section_title(ctx, "SVG path（fonts.google.com/icons 复制）");
            Row::new().modifier(Modifier::new().padding_vertical(3.0)).build(ctx, |ctx| {
                // add
                Icon::svg_path("M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z").build(ctx);
                // star（tint 主题色 + 放大）
                Icon::svg_path("M12 17.27L18.18 21l-1.64-7.03L22 9.24l-7.19-.61L12 2 9.19 8.63 2 9.24l5.46 4.73L5.82 21z")
                    .tint(theme.primary)
                    .size(32.0)
                    .build(ctx);
                // arrow_back
                Icon::svg_path("M20 11H7.83l5.59-5.59L12 4l-8 8 8 8 1.41-1.41L7.83 13H20v-2z")
                    .tint(theme.on_surface)
                    .build(ctx);
            });

            section_title(ctx, "完整 SVG 字符串");
            Row::new().modifier(Modifier::new().padding_vertical(3.0)).build(ctx, |ctx| {
                Icon::svg(
                    "<svg viewBox='0 0 24 24' xmlns='http://www.w3.org/2000/svg'>\
                     <path d='M12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 \
                     5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2Z'/></svg>",
                )
                .tint(theme.tertiary)
                .build(ctx);
            });

            section_title(ctx, "图片文件（png / svg 等路径）");
            Row::new().modifier(Modifier::new().padding_vertical(3.0)).build(ctx, |ctx| {
                // 官方 Material Symbols home（outlined）SVG——文件源默认不染色
                Icon::file(concat!(env!("CARGO_MANIFEST_DIR"), "/examples/assets/home.svg"))
                    .size(48.0)
                    .build(ctx);
                // 文件源也可以显式 tint
                Icon::file(concat!(env!("CARGO_MANIFEST_DIR"), "/examples/assets/home.svg"))
                    .size(48.0)
                    .tint(theme.primary)
                    .build(ctx);
                // 彩色 PNG——保留原色（不染色）
                Icon::file(concat!(env!("CARGO_MANIFEST_DIR"), "/examples/assets/sample.png"))
                    .size(48.0)
                    .build(ctx);
            });

            section_title(ctx, "IconButton（标准 / Filled / Tonal / Outlined / Disabled）");
            let ib_clicks = ctx.remember(|| 0i32);
            let ibc = ib_clicks.clone();
            Row::new().modifier(Modifier::new().padding_vertical(3.0)).build(ctx, |ctx| {
                let star = "M12 17.27L18.18 21l-1.64-7.03L22 9.24l-7.19-.61L12 2 9.19 8.63 2 9.24l5.46 4.73L5.82 21z";
                let c0 = ibc.clone();
                IconButton::new().on_click(move || c0.update(|v| *v += 1)).build(ctx, |ctx| {
                    Icon::svg_path(star).build(ctx);
                });
                let c1 = ibc.clone();
                IconButton::filled().on_click(move || c1.update(|v| *v += 1)).build(ctx, |ctx| {
                    Icon::svg_path(star).build(ctx);
                });
                let c2 = ibc.clone();
                IconButton::filled_tonal().on_click(move || c2.update(|v| *v += 1)).build(ctx, |ctx| {
                    Icon::svg_path(star).build(ctx);
                });
                let c3 = ibc.clone();
                IconButton::outlined().on_click(move || c3.update(|v| *v += 1)).build(ctx, |ctx| {
                    Icon::svg_path(star).build(ctx);
                });
                IconButton::new().enabled(false).on_click(|| {}).build(ctx, |ctx| {
                    Icon::svg_path(star).build(ctx);
                });
            });
            Text::new(format!("IconButton 总点击 {}", ib_clicks.get()))
                .font_size(12.0)
                .color(Color::from_argb(255, 100, 100, 100))
                .build(ctx);

            section_title(ctx, "可变字体（--features material-symbols-outlined）");
            #[cfg(feature = "material-symbols-outlined")]
            {
                let toggle = ctx.remember(|| false);
                let target = if toggle.get() { 1.0 } else { 0.0 };
                let fill = ctx.animate_float_as_state(
                    target,
                    winia::animation::AnimationSpec::Tween(winia::animation::TweenSpec::new(
                        Duration::from_millis(400),
                        winia::animation::interpolator::EaseOutCubic::new(),
                    )),
                );
                let t = toggle.clone();
                Row::new().modifier(Modifier::new().padding_vertical(3.0)).build(ctx, |ctx| {
                    Icon::symbol(winia::icon::Outlined::FAVORITE)
                        .fill(&fill)
                        .wght(500.0)
                        .grad(25.0)
                        .tint(theme.error)
                        .build(ctx);
                    Icon::symbol(winia::icon::Outlined::HOME)
                        .opsz(40.0)
                        .wght(700.0)
                        .tint(theme.primary)
                        .build(ctx);
                    Button::new()
                        .on_click(move || t.update(|v| *v = !*v))
                        .build(ctx, |ctx| {
                            Text::new("切换 FILL").font_size(12.0).build(ctx);
                        });
                });
            }
            #[cfg(not(feature = "material-symbols-outlined"))]
            {
                Text::new("（构建时加 --features material-symbols-outlined 查看）")
                    .font_size(12.0)
                    .color(Color::from_argb(255, 120, 120, 120))
                    .build(ctx);
            }

            section_title(ctx, "方向镜像（auto_mirror）——全局方向由上方按钮切换");
            Row::new().modifier(Modifier::new().padding_vertical(3.0)).build(ctx, |ctx| {
                // auto_mirror(true)：跟随全局方向（LTR 原样 / RTL 镜像）
                Column::new().build(ctx, |ctx| {
                    Icon::svg_path("M20 11H7.83l5.59-5.59L12 4l-8 8 8 8 1.41-1.41L7.83 13H20v-2z")
                        .auto_mirror(true)
                        .build(ctx);
                });
                // 默认 auto_mirror(false)：不镜像
                Column::new().modifier(Modifier::new().padding_start(24.0)).build(ctx, |ctx| {
                    Icon::svg_path("M20 11H7.83l5.59-5.59L12 4l-8 8 8 8 1.41-1.41L7.83 13H20v-2z").build(ctx);
                });
                // auto_mirror(true) + 容器强制 LTR：不跟随全局
                Column::new()
                    .modifier(Modifier::new().padding_start(24.0).layout_direction(LayoutDirection::Ltr))
                    .build(ctx, |ctx| {
                        Icon::svg_path("M20 11H7.83l5.59-5.59L12 4l-8 8 8 8 1.41-1.41L7.83 13H20v-2z")
                            .auto_mirror(true)
                            .build(ctx);
                    });
            });

            Text::new("").modifier(Modifier::new().height(40.0)).build(ctx);
        });
    });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();

    winia::run_app!(|ctx| {
        WiniaTheme::light(ctx, |ctx| {
            Window::new()
                .size(480.0, 640.0)
                .title("Icon Demo")
                .build(ctx, icon_demo);
        });
    });
}
