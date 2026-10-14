//! Icon 演示——SVG path / SVG 字符串 / 图片文件 / 可变字体（feature 启用时）/ RTL 镜像

use letclone::clone;
use winia::prelude::*;
// Shared example chrome: top app bar with the settings sheet (theme mode + layout direction).
#[path = "common/settings.rs"]
mod settings;


fn section_title(ctx: &mut ComposeCtx, text: &str) {
    Text::new(text)
        .font_size(14.0)
        .color(WiniaTheme::colors().on_surface_variant)
        .modifier(Modifier::new().padding_top(14.0).padding_bottom(6.0))
        .build(ctx);
}

#[composable]
fn icon_demo(ctx: &mut ComposeCtx) {
    // The layout direction comes from the shared chrome's settings sheet now, so this demo's own
    // inline LTR/RTL switch is gone: it provided the direction for the whole page, and two switches
    // writing the same CompositionLocal meant the settings one looked dead here.
    let theme = WiniaTheme::colors();
    let scroll_y = ctx.remember(|| ScrollState::new()).get();
    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0).vertical_scroll(scroll_y))
        .build(ctx, |ctx| {

        section_title(ctx, "SVG path（fonts.google.com/icons 复制）");
        Row::new()
            .modifier(Modifier::new().padding_vertical(3.0))
            .spacing(12.0)
            .build(ctx, |ctx| {
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
        Row::new().modifier(Modifier::new().padding_vertical(3.0)).build(ctx, |ctx| {
            let star = "M12 17.27L18.18 21l-1.64-7.03L22 9.24l-7.19-.61L12 2 9.19 8.63 2 9.24l5.46 4.73L5.82 21z";
            IconButton::new().on_click({ clone!(ib_clicks); move || ib_clicks.update(|v| *v += 1) }).build(ctx, |ctx| {
                Icon::svg_path(star).build(ctx);
            });
            IconButton::filled().on_click({ clone!(ib_clicks); move || ib_clicks.update(|v| *v += 1) }).build(ctx, |ctx| {
                Icon::svg_path(star).build(ctx);
            });
            IconButton::filled_tonal().on_click({ clone!(ib_clicks); move || ib_clicks.update(|v| *v += 1) }).build(ctx, |ctx| {
                Icon::svg_path(star).build(ctx);
            });
            IconButton::outlined().on_click({ clone!(ib_clicks); move || ib_clicks.update(|v| *v += 1) }).build(ctx, |ctx| {
                Icon::svg_path(star).build(ctx);
            });
            IconButton::new().enabled(false).on_click(|| {}).build(ctx, |ctx| {
                Icon::svg_path(star).build(ctx);
            });
        });
        Text::new(format!("IconButton 总点击 {}", ib_clicks.get()))
            .font_size(12.0)
            .color(WiniaTheme::colors().on_surface_variant)
            .build(ctx);

        section_title(ctx, "IconButton 尺寸变体（XSmall ~ XLarge）");
        let star = "M12 17.27L18.18 21l-1.64-7.03L22 9.24l-7.19-.61L12 2 9.19 8.63 2 9.24l5.46 4.73L5.82 21z";
        Row::new()
            .modifier(Modifier::new().padding_vertical(3.0))
            .spacing(12.0)
            .build(ctx, |ctx| {
                for size in [
                    IconButtonSize::XSmall,
                    IconButtonSize::Small,
                    IconButtonSize::Medium,
                    IconButtonSize::Large,
                    IconButtonSize::XLarge,
                ] {
                    IconButton::new().size(size).on_click(|| {}).build(ctx, |ctx| {
                        Icon::svg_path(star).size(size.icon_size()).build(ctx);
                    });
                }
            });
        // Derived from the tokens instead of written out: this line still said "Small 48" long after the
        // Small container became 40. A caption that cannot drift is worth the extra lines.
        let size_note = [
            IconButtonSize::XSmall,
            IconButtonSize::Small,
            IconButtonSize::Medium,
            IconButtonSize::Large,
            IconButtonSize::XLarge,
        ]
        .iter()
        .map(|s| format!("{s:?} {:.0}", s.container_size()))
        .collect::<Vec<_>>()
        .join(" / ");
        Text::new(size_note)
            .font_size(12.0)
            .color(WiniaTheme::colors().on_surface_variant)
            .build(ctx);

        section_title(ctx, "IconToggleButton（点击切换 checked）");
        let t0 = ctx.remember(|| false);
        let t1 = ctx.remember(|| false);
        let t2 = ctx.remember(|| false);
        let t3 = ctx.remember(|| false);
        let star = "M12 17.27L18.18 21l-1.64-7.03L22 9.24l-7.19-.61L12 2 9.19 8.63 2 9.24l5.46 4.73L5.82 21z";
        let check = "M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z";
        Row::new()
            .modifier(Modifier::new().padding_vertical(3.0))
            .spacing(12.0)
            .build(ctx, |ctx| {
            let checked0 = t0.get();
            IconToggleButton::new(checked0).on_checked_change({ clone!(t0); move |v| t0.update(|cur| *cur = v) }).build(ctx, |ctx| {
                Icon::svg_path(if checked0 { check } else { star }).build(ctx);
            });
            let checked1 = t1.get();
            IconToggleButton::filled(checked1).on_checked_change({ clone!(t1); move |v| t1.update(|cur| *cur = v) }).build(ctx, |ctx| {
                Icon::svg_path(if checked1 { check } else { star }).build(ctx);
            });
            let checked2 = t2.get();
            IconToggleButton::filled_tonal(checked2).on_checked_change({ clone!(t2); move |v| t2.update(|cur| *cur = v) }).build(ctx, |ctx| {
                Icon::svg_path(if checked2 { check } else { star }).build(ctx);
            });
            let checked3 = t3.get();
            IconToggleButton::outlined(checked3).on_checked_change({ clone!(t3); move |v| t3.update(|cur| *cur = v) }).build(ctx, |ctx| {
                Icon::svg_path(if checked3 { check } else { star }).build(ctx);
            });
            // 禁用
            IconToggleButton::new(false).enabled(false).on_checked_change(|_| {}).build(ctx, |ctx| {
                Icon::svg_path(star).build(ctx);
            });
            });
        Text::new(format!(
            "checked = {}/{}/{}/{}（星形=未选，对勾=已选）",
            t0.get(),
            t1.get(),
            t2.get(),
            t3.get()
        ))
            .font_size(12.0)
            .color(WiniaTheme::colors().on_surface_variant)
            .build(ctx);

        section_title(ctx, "Button 带图标（内容 = 居中 Row，对标 M3）");
        let star = "M12 17.27L18.18 21l-1.64-7.03L22 9.24l-7.19-.61L12 2 9.19 8.63 2 9.24l5.46 4.73L5.82 21z";
        Row::new()
            .modifier(Modifier::new().padding_vertical(3.0))
            .spacing(12.0)
            .build(ctx, |ctx| {
            // Filled：图标 tint Auto 取按钮内容色 on_primary
            Button::filled()
                .content_padding(ButtonDefaults::button_with_icon_content_padding())
                .on_click(|| {})
                .build(ctx, |ctx| {
                    Icon::svg_path(star).build(ctx);
                    Text::new("收藏").font_size(13.0).build(ctx);
                });
            // Outlined：带图标左 16 / 右 24，间距由 Row 自动 8dp
            Button::outlined()
                .content_padding(ButtonDefaults::button_with_icon_content_padding())
                .on_click(|| {})
                .build(ctx, |ctx| {
                    Icon::svg_path(star).build(ctx);
                    Text::new("加星").font_size(13.0).build(ctx);
                });
            // Elevated：图标 + 阴影
            Button::elevated()
                .content_padding(ButtonDefaults::button_with_icon_content_padding())
                .on_click(|| {})
                .build(ctx, |ctx| {
                    Icon::svg_path(star).build(ctx);
                    Text::new("收藏").font_size(13.0).build(ctx);
                });
            // Text：紧凑 12/8/16/8
            Button::text()
                .content_padding(ButtonDefaults::text_button_with_icon_content_padding())
                .on_click(|| {})
                .build(ctx, |ctx| {
                    Icon::svg_path(star).build(ctx);
                    Text::new("加星").font_size(13.0).build(ctx);
                });
            });

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
                    .on_click({ clone!(toggle); move || toggle.update(|v| *v = !*v) })
                    .build(ctx, |ctx| {
                        Text::new("切换 FILL").font_size(12.0).build(ctx);
                    });
            });
        }
        #[cfg(not(feature = "material-symbols-outlined"))]
        {
            Text::new("（构建时加 --features material-symbols-outlined 查看）")
                .font_size(12.0)
                .color(WiniaTheme::colors().on_surface_variant)
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

        section_title(ctx, "Tint — Auto follows the content colour / explicit / None keeps the source");
        let two_tone = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\">\
                        <rect x=\"3\" y=\"3\" width=\"18\" height=\"18\" rx=\"4\" fill=\"#1B5E20\"/>\
                        <circle cx=\"12\" cy=\"12\" r=\"5\" fill=\"#B8860B\"/></svg>";
        Row::new()
            .spacing(16.0)
            .modifier(Modifier::new().padding_vertical(4.0))
            .build(ctx, |ctx| {
                // Auto: the icon is tinted with the content colour supplied around it.
                Icon::svg_path(star).build(ctx);
                // An explicit colour ignores the surroundings.
                Icon::svg_path(star)
                    .tint(Color::from_argb(255, 0x1B, 0x5E, 0x20))
                    .build(ctx);
                // `None` leaves the source alone. That is what it exists for, and the reason the caption
                // below is a two-fill document rather than a glyph: the tint is a layer-wide `SrcIn`, so
                // `Auto` would flatten these two colours into one.
                Icon::svg(two_tone).tint(Tint::None).build(ctx);
            });
        Text::new("Tint::None keeps both fills of the SVG above; Tint::Auto would render it in one colour")
            .font_size(11.0)
            .color(WiniaTheme::colors().on_surface_variant)
            .build(ctx);

        section_title(ctx, "Size — a plain f32, or a value resolved at LAYOUT time");
        let growth = ctx.remember(|| 0.0f32);
        Row::new()
            .spacing(12.0)
            .modifier(Modifier::new().padding_vertical(4.0))
            .build(ctx, |ctx| {
                // `size()` takes the same `SizeValue` `Modifier::size` does, so a closure is enough to make
                // the icon follow a size that changes during layout — no recomposition involved.
                let factor = growth.clone();
                Icon::svg_path(star)
                    .size(move || 24.0 + factor.get() * 16.0)
                    .build(ctx);
                Button::new()
                    .on_click({
                        clone!(growth);
                        move || growth.update(|g| *g = if *g > 0.5 { 0.0 } else { 1.0 })
                    })
                    .build(ctx, |ctx| {
                        Text::new("Grow").build(ctx);
                    });
            });
        Text::new("the star is 24 or 40 dp depending on the button, driven through SizeValue")
            .font_size(11.0)
            .color(WiniaTheme::colors().on_surface_variant)
            .build(ctx);

        section_title(ctx, "Accessibility — a described icon is announced, an undescribed one is not");
        Row::new()
            .spacing(16.0)
            .modifier(Modifier::new().padding_vertical(4.0))
            .build(ctx, |ctx| {
                Icon::svg_path(star).content_description("Favourite").build(ctx);
                // No description: decorative, and deliberately absent from the semantics tree rather than
                // announced as empty.
                Icon::svg_path(star).build(ctx);
            });

        Text::new("").modifier(Modifier::new().height(40.0)).build(ctx);
    });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(480.0, 640.0)
                .title("Icon Demo")
                .build(ctx, |ctx| {
                    settings::shell("Icon Demo", ctx, icon_demo);
                });
        });
    });
}
