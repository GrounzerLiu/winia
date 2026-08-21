//! Deterministic CPU visual regression matrix for Material components.
//!
//! The matrix deliberately uses explicit theme/direction inputs and semantic pixel
//! checks instead of platform-dependent golden images.

use winia::core::composer::{ComposeCtx, Composer};
use winia::layout::constraints::Constraints;
use winia::layout::LayoutDirection;
use winia::modifier::{Color, Modifier, ScrollState};
use winia::render;
use winia::State;
use winia::unit::{Sp, TextUnit};
use winia::ui::{
    Button, Chip, FloatingActionButton, FloatingActionButtonSize, Icon, NavigationBar,
    NavigationBarItem, Text, TextField, TextFieldValue, ThemeColors, TopAppBar, TopAppBarColors,
    TopAppBarScrollBehavior, TopAppBarVariant, Scaffold, Typography, WiniaTheme,
};

const WIDTH: i32 = 520;
const HEIGHT: i32 = 420;
const SEED: u32 = 0xff6750a4;

fn raster_surface() -> skia_safe::Surface {
    skia_safe::surfaces::raster_n32_premul((WIDTH, HEIGHT)).expect("raster surface")
}

fn pixel(surface: &mut skia_safe::Surface, x: i32, y: i32) -> (u8, u8, u8, u8) {
    let mut pixels = [0u8; 4];
    let info = skia_safe::ImageInfo::new(
        (1, 1),
        skia_safe::ColorType::RGBA8888,
        skia_safe::AlphaType::Premul,
        None,
    );
    surface.read_pixels(&info, &mut pixels, 4, (x, y));
    (pixels[0], pixels[1], pixels[2], pixels[3])
}

fn non_white_pixels(surface: &mut skia_safe::Surface) -> usize {
    let mut count = 0;
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let (r, g, b, _) = pixel(surface, x, y);
            if r < 248 || g < 248 || b < 248 {
                count += 1;
            }
        }
    }
    count
}

fn find_tag(nodes: &[winia::layout::node::LayoutNode], idx: usize, tag: &str) -> Option<usize> {
    if nodes[idx].modifier.get_test_tag() == Some(tag) {
        return Some(idx);
    }
    nodes[idx]
        .children
        .iter()
        .find_map(|&child| find_tag(nodes, child, tag))
}

fn render_scroll_color_probe(variant: TopAppBarVariant, offset: f32, colors: TopAppBarColors) -> skia_safe::Surface {
    let scroll = ScrollState::new();
    scroll.offset.set(offset);
    let expanded = match variant {
        TopAppBarVariant::Medium => winia::ui::TOP_APP_BAR_MEDIUM_HEIGHT,
        TopAppBarVariant::Large => winia::ui::TOP_APP_BAR_LARGE_HEIGHT,
        _ => winia::ui::TOP_APP_BAR_HEIGHT,
    };
    let mut composer = Composer::new();
    composer.compose(winia::app_root!(|ctx| {
        let behavior = TopAppBarScrollBehavior::new(scroll.clone(), expanded);
        match variant {
            TopAppBarVariant::Standard => TopAppBar::new(|_ctx| {}).scroll_behavior(behavior).colors(colors).modifier(Modifier::new().test_tag("color-probe")).build(ctx),
            TopAppBarVariant::CenterAligned => TopAppBar::center_aligned(|_ctx| {}).scroll_behavior(behavior).colors(colors).modifier(Modifier::new().test_tag("color-probe")).build(ctx),
            TopAppBarVariant::Medium => TopAppBar::medium(|_ctx| {}).scroll_behavior(behavior).colors(colors).modifier(Modifier::new().test_tag("color-probe")).build(ctx),
            TopAppBarVariant::Large => TopAppBar::large(|_ctx| {}).scroll_behavior(behavior).colors(colors).modifier(Modifier::new().test_tag("color-probe")).build(ctx),
        }
    }));
    composer.layout(Constraints::new(0.0, 320.0, 0.0, 180.0));
    let mut surface = skia_safe::surfaces::raster_n32_premul((320, 180)).expect("scroll color surface");
    surface.canvas().clear(skia_safe::Color::WHITE);
    let root = composer.layout_root_idx().unwrap();
    render::render(composer.arena_nodes(), root, surface.canvas());
    surface
}

fn render_scaffold_case(direction: LayoutDirection, custom: bool) -> (skia_safe::Surface, Composer) {
    let theme = ThemeColors::default_light();
    let typography = if custom { custom_typography() } else { Typography::default() };
    let mut composer = Composer::new();
    composer.compose(winia::app_root!(|ctx| {
        WiniaTheme::with_theme_typography_and_direction(theme, typography, direction, ctx, |ctx| {
            Scaffold::new(|ctx, _| {
                let key = ctx.next_key();
                ctx.start_leaf(key, Modifier::new().fill_max_size().background(Color::from_argb(255, 245, 245, 245), winia::modifier::Shape::Rectangle).test_tag("scaffold-content"));
                ctx.end_node();
            })
            .top_bar(|ctx| {
                let key = ctx.next_key();
                ctx.start_leaf(key, Modifier::new().fill_max_width().height(64.0).background(Color::from_argb(255, 220, 220, 225), winia::modifier::Shape::Rectangle).test_tag("scaffold-top"));
                ctx.end_node();
            })
            .bottom_bar(|ctx| {
                let key = ctx.next_key();
                ctx.start_leaf(key, Modifier::new().fill_max_width().height(80.0).background(Color::from_argb(255, 230, 225, 235), winia::modifier::Shape::Rectangle).test_tag("scaffold-bottom"));
                ctx.end_node();
            })
            .floating_action_button(|ctx| {
                let key = ctx.next_key();
                ctx.start_leaf(key, Modifier::new().size(56.0, 56.0).background(Color::from_argb(255, 103, 80, 164), winia::modifier::Shape::Circle).test_tag("scaffold-fab"));
                ctx.end_node();
            })
            .build(ctx);
        });
    }));
    composer.layout(Constraints::new(0.0, 360.0, 0.0, 640.0));
    let mut surface = skia_safe::surfaces::raster_n32_premul((360, 640)).expect("scaffold surface");
    surface.canvas().clear(skia_safe::Color::WHITE);
    let root = composer.layout_root_idx().unwrap();
    render::render(composer.arena_nodes(), root, surface.canvas());
    (surface, composer)
}

fn custom_typography() -> Typography {
    Typography {
        body_large: winia::ui::TextStyle::new()
            .font_size(TextUnit::Sp(Sp(18.0)))
            .line_height(28.0)
            .letter_spacing(0.9),
        body_small: winia::ui::TextStyle::new()
            .font_size(TextUnit::Sp(Sp(13.0)))
            .line_height(19.0)
            .letter_spacing(0.6),
        label_large: winia::ui::TextStyle::new()
            .font_size(TextUnit::Sp(Sp(15.0)))
            .line_height(22.0)
            .letter_spacing(0.4)
            .font_weight(winia::ui::FontWeight::BOLD),
        ..Typography::default()
    }
}

fn render_case(dark: bool, direction: LayoutDirection, custom: bool) -> (skia_safe::Surface, Composer) {
    let theme = if dark {
        ThemeColors::dark_from_seed(SEED)
    } else {
        ThemeColors::light_from_seed(SEED)
    };
    let typography = if custom { custom_typography() } else { Typography::default() };
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime");
    let _guard = rt.enter();
    let mut composer = Composer::new();
    composer.compose(winia::app_root!(|ctx| {
        WiniaTheme::with_theme_typography_and_direction(theme, typography, direction, ctx, |ctx| {
            build_matrix(ctx);
        });
    }));
    composer.layout(Constraints::new(0.0, WIDTH as f32, 0.0, HEIGHT as f32));
    let mut surface = raster_surface();
    surface.canvas().clear(skia_safe::Color::WHITE);
    let root = composer.layout_root_idx().expect("matrix root");
    render::render(composer.arena_nodes(), root, surface.canvas());
    (surface, composer)
}

fn build_matrix(ctx: &mut ComposeCtx) {
    winia::ui::Column::new()
        .modifier(Modifier::new().fill_max_width().padding(16.0))
        .build(ctx, |ctx| {
            Chip::assist(|ctx| Text::new("Assist").build(ctx), || {})
                .modifier(Modifier::new().test_tag("chip"))
                .build(ctx);
            Chip::filter(true, |ctx| Text::new("Selected").build(ctx), || {})
                .modifier(Modifier::new().test_tag("chip-selected"))
                .build(ctx);
            Chip::input(false, |ctx| Text::new("Disabled").build(ctx), || {})
                .enabled(false)
                .modifier(Modifier::new().test_tag("chip-disabled"))
                .build(ctx);

            let value = State::new(TextFieldValue::new("value"));
            TextField::new(value)
                .filled()
                .supporting_text("Supporting")
                .modifier(Modifier::new().test_tag("field"))
                .build(ctx);
            let empty = State::new(TextFieldValue::new(""));
            TextField::new(empty)
                .outlined()
                .placeholder(|ctx| Text::new("Placeholder").build(ctx))
                .is_error(true)
                .modifier(Modifier::new().test_tag("field-error"))
                .build(ctx);
            let disabled = State::new(TextFieldValue::new("Disabled"));
            TextField::new(disabled)
                .filled()
                .enabled(false)
                .modifier(Modifier::new().test_tag("field-disabled"))
                .build(ctx);

            Button::elevated()
                .modifier(Modifier::new().test_tag("button"))
                .build(ctx, |ctx| Text::new("Action").build(ctx));
            Button::new()
                .enabled(false)
                .modifier(Modifier::new().test_tag("button-disabled"))
                .build(ctx, |ctx| Text::new("Disabled").build(ctx));

            FloatingActionButton::new()
                .modifier(Modifier::new().test_tag("fab"))
                .build(ctx, |ctx| Icon::svg_path("M12 5v14M5 12h14").build(ctx));
            FloatingActionButton::new()
                .enabled(false)
                .modifier(Modifier::new().test_tag("fab-disabled"))
                .build(ctx, |ctx| Icon::svg_path("M12 5v14M5 12h14").build(ctx));

            TopAppBar::new(|ctx| Text::new("Standard").build(ctx))
                .navigation_icon(|ctx| Text::new("‹").build(ctx))
                .actions(|ctx| Text::new("•••").build(ctx))
                .modifier(Modifier::new().test_tag("appbar-standard"))
                .build(ctx);
            TopAppBar::center_aligned(|ctx| Text::new("Center").build(ctx))
                .modifier(Modifier::new().test_tag("appbar-center"))
                .build(ctx);
            TopAppBar::medium(|ctx| Text::new("Medium").build(ctx))
                .subtitle(|ctx| Text::new("Subtitle").build(ctx))
                .modifier(Modifier::new().test_tag("appbar-medium"))
                .build(ctx);
            TopAppBar::large(|ctx| Text::new("Large").build(ctx))
                .subtitle(|ctx| Text::new("Subtitle").build(ctx))
                .modifier(Modifier::new().test_tag("appbar-large"))
                .build(ctx);
        });
}

#[test]
fn material_visual_matrix_covers_theme_direction_and_typography() {
    for dark in [false, true] {
        for direction in [LayoutDirection::Ltr, LayoutDirection::Rtl] {
            for custom in [false, true] {
                let (mut surface, composer) = render_case(dark, direction, custom);
                let root = composer.layout_root_idx().expect("matrix root");
                let nodes = composer.arena_nodes();
                assert_eq!(nodes[root].measured_size.width, WIDTH as f32);
                assert!(nodes[root].measured_size.height > 0.0);
                for tag in [
                    "chip", "chip-selected", "chip-disabled", "field", "field-error",
                    "field-disabled", "button", "button-disabled", "fab", "fab-disabled",
                    "appbar-standard", "appbar-center", "appbar-medium", "appbar-large",
                ] {
                    let idx = find_tag(nodes, root, tag).unwrap_or_else(|| panic!("missing {tag}"));
                    assert!(nodes[idx].measured_size.width > 0.0, "{tag} width");
                    assert!(nodes[idx].measured_size.height > 0.0, "{tag} height");
                }
                let chip = find_tag(nodes, root, "chip").unwrap();
                let field = find_tag(nodes, root, "field").unwrap();
                let button = find_tag(nodes, root, "button").unwrap();
                let fab = find_tag(nodes, root, "fab").unwrap();
                assert_eq!(nodes[chip].measured_size.height, 32.0, "chip height");
                assert!(nodes[field].measured_size.height >= 56.0, "field minimum height");
                assert!(nodes[button].measured_size.height >= 40.0, "button minimum height");
                assert_eq!(nodes[fab].measured_size.width, FloatingActionButtonSize::Regular.container_size());
                assert_eq!(nodes[fab].measured_size.height, FloatingActionButtonSize::Regular.container_size());
                for (tag, height) in [
                    ("appbar-standard", 64.0),
                    ("appbar-center", 64.0),
                    ("appbar-medium", 112.0),
                    ("appbar-large", 152.0),
                ] {
                    let appbar = find_tag(nodes, root, tag).unwrap();
                    assert_eq!(nodes[appbar].measured_size.width, WIDTH as f32 - 32.0);
                    assert_eq!(nodes[appbar].measured_size.height, height, "{tag} height");
                }
                assert!(non_white_pixels(&mut surface) > 1000, "matrix case rendered no meaningful pixels");
            }
        }
    }
}

fn build_rtl_probe(ctx: &mut ComposeCtx) {
    winia::ui::Row::new()
        .modifier(Modifier::new().width(200.0).test_tag("rtl-probe"))
        .build(ctx, |ctx| {
            let leading_key = ctx.next_key();
            ctx.start_leaf(leading_key, Modifier::new().size(30.0, 12.0).test_tag("rtl-leading"));
            ctx.end_node();
            let trailing_key = ctx.next_key();
            ctx.start_leaf(trailing_key, Modifier::new().size(70.0, 12.0).test_tag("rtl-trailing"));
            ctx.end_node();
        });
}
fn layout_rtl_probe(direction: LayoutDirection) -> Composer {
    let mut composer = Composer::new();
    composer.compose(winia::app_root!(|ctx| {
        WiniaTheme::with_theme_and_direction(
            ThemeColors::light_from_seed(SEED),
            direction,
            ctx,
            |ctx| build_rtl_probe(ctx),
        );
    }));
    composer.layout(Constraints::new(0.0, 200.0, 0.0, 20.0));
    composer
}

#[test]
fn material_visual_matrix_rtl_mirrors_horizontal_content() {
    let ltr = layout_rtl_probe(LayoutDirection::Ltr);
    let rtl = layout_rtl_probe(LayoutDirection::Rtl);
    let ltr_nodes = ltr.arena_nodes();
    let rtl_nodes = rtl.arena_nodes();
    let ltr_root = ltr.layout_root_idx().unwrap();
    let rtl_root = rtl.layout_root_idx().unwrap();
    let ltr_leading = find_tag(ltr_nodes, ltr_root, "rtl-leading").unwrap();
    let rtl_leading = find_tag(rtl_nodes, rtl_root, "rtl-leading").unwrap();
    let ltr_trailing = find_tag(ltr_nodes, ltr_root, "rtl-trailing").unwrap();
    let rtl_trailing = find_tag(rtl_nodes, rtl_root, "rtl-trailing").unwrap();
    assert_eq!(ltr_nodes[ltr_leading].position.x, 0.0);
    assert_eq!(rtl_nodes[rtl_leading].position.x, 170.0);
    assert_eq!(ltr_nodes[ltr_trailing].position.x, 30.0);
    assert_eq!(rtl_nodes[rtl_trailing].position.x, 100.0);
}

#[test]
fn top_app_bar_scroll_color_probe_uses_base_and_scrolled_containers() {
    let base = Color::from_argb(255, 17, 34, 51);
    let scrolled = Color::from_argb(255, 210, 70, 20);
    let colors = TopAppBarColors::new(base, Color::WHITE, Color::WHITE, Color::WHITE, Color::WHITE)
        .scrolled_container(scrolled);
    for variant in [TopAppBarVariant::Standard, TopAppBarVariant::CenterAligned, TopAppBarVariant::Medium, TopAppBarVariant::Large] {
        let mut initial = render_scroll_color_probe(variant, 0.0, colors);
        let p0 = pixel(&mut initial, 2, 2);
        assert_eq!(p0, (base.r, base.g, base.b, base.a), "{variant:?} initial container");
        let offset = if matches!(variant, TopAppBarVariant::Medium | TopAppBarVariant::Large) { 1_000.0 } else { 1.0 };
        let mut scrolled_surface = render_scroll_color_probe(variant, offset, colors);
        let p1 = pixel(&mut scrolled_surface, 2, 2);
        assert_eq!(p1, (scrolled.r, scrolled.g, scrolled.b, scrolled.a), "{variant:?} scrolled container");
    }
}

#[test]
fn scaffold_visual_matrix_mirrors_fab_and_preserves_content_geometry() {
    for custom in [false, true] {
        let (mut ltr_surface, ltr) = render_scaffold_case(LayoutDirection::Ltr, custom);
        let (mut rtl_surface, rtl) = render_scaffold_case(LayoutDirection::Rtl, custom);
        let lr = ltr.layout_root_idx().unwrap();
        let rr = rtl.layout_root_idx().unwrap();
        let ln = ltr.arena_nodes();
        let rn = rtl.arena_nodes();
        let ltr_content = &ln[find_tag(ln, lr, "scaffold-content").unwrap()];
        let rtl_content = &rn[find_tag(rn, rr, "scaffold-content").unwrap()];
        let ltr_fab = &ln[ln[lr].children[3]];
        let rtl_fab = &rn[rn[rr].children[3]];
        assert_eq!(ltr_content.position.y, rtl_content.position.y);
        assert_eq!(ltr_content.measured_size.height, rtl_content.measured_size.height);
        assert_eq!((ltr_fab.measured_size.width, ltr_fab.measured_size.height), (56.0, 56.0));
        assert_eq!((rtl_fab.measured_size.width, rtl_fab.measured_size.height), (56.0, 56.0));
        assert!(rtl_fab.position.x < ltr_fab.position.x);
        let ltr_px = pixel(&mut ltr_surface, (ltr_fab.position.x + 28.0) as i32, (ltr_fab.position.y + 28.0) as i32);
        let rtl_px = pixel(&mut rtl_surface, (rtl_fab.position.x + 28.0) as i32, (rtl_fab.position.y + 28.0) as i32);
        assert!(ltr_px.0 < 240 || ltr_px.1 < 240 || ltr_px.2 < 240, "LTR FAB should render");
        assert!(rtl_px.0 < 240 || rtl_px.1 < 240 || rtl_px.2 < 240, "RTL FAB should render");
    }
}

#[test]
fn material_visual_matrix_custom_typography_changes_textfield_geometry() {
    let (_, default_composer) = render_case(false, LayoutDirection::Ltr, false);
    let (_, custom_composer) = render_case(false, LayoutDirection::Ltr, true);
    let default_nodes = default_composer.arena_nodes();
    let custom_nodes = custom_composer.arena_nodes();
    let default_field = find_tag(default_nodes, default_composer.layout_root_idx().unwrap(), "field").unwrap();
    let custom_field = find_tag(custom_nodes, custom_composer.layout_root_idx().unwrap(), "field").unwrap();
    assert!(custom_nodes[custom_field].measured_size.height >= default_nodes[default_field].measured_size.height);
}

// ── NavigationBar 视觉矩阵 ──

/// 渲染一个 360x80 的 NavigationBar：3 个 item（第 0 个选中，均带 label）。
fn render_navigation_bar_case(direction: LayoutDirection) -> skia_safe::Surface {
    let theme = ThemeColors::default_light();
    let mut composer = Composer::new();
    composer.compose(winia::app_root!(|ctx| {
        WiniaTheme::with_theme_and_direction(theme, direction, ctx, |ctx| {
            NavigationBar::new(|ctx| {
                for i in 0..3 {
                    NavigationBarItem::new(
                        i == 0,
                        |ctx| {
                            let key = ctx.next_key();
                            ctx.start_leaf(key, Modifier::new().size(24.0, 24.0));
                            ctx.end_node();
                        },
                    )
                    .label(move |ctx| {
                        Text::new(format!("Tab{i}")).build(ctx);
                    })
                    .on_click(|| {})
                    .build(ctx);
                }
            })
            .build(ctx);
        });
    }));
    composer.layout(Constraints::new(0.0, 360.0, 0.0, 80.0));
    let mut surface = skia_safe::surfaces::raster_n32_premul((360, 80)).expect("navbar surface");
    surface.canvas().clear(skia_safe::Color::WHITE);
    let root = composer.layout_root_idx().unwrap();
    render::render(composer.arena_nodes(), root, surface.canvas());
    surface
}

#[test]
fn navigation_bar_visual_matrix_pill_container_and_rtl_mirror() {
    let theme = ThemeColors::default_light();
    let container = theme.surface_container;
    let indicator = theme.secondary_container;
    let to_rgba = |c: Color| (c.r, c.g, c.b, c.a);

    let mut ltr = render_navigation_bar_case(LayoutDirection::Ltr);
    // 容器底色（左下角背景区）
    assert_eq!(pixel(&mut ltr, 5, 75), to_rgba(container), "容器应为 surfaceContainer");
    // 选中项指示器胶囊：item0 中心 x≈57.3，胶囊横跨 29.3..85.3、y 12..44；
    // 取 (35, 28)——在胶囊内且避开图标（图标占 45.3..69.3）
    assert_eq!(pixel(&mut ltr, 35, 28), to_rgba(indicator), "选中项应有 SecondaryContainer 胶囊");
    // 未选中项无胶囊：item1 中心 x≈180，取 (155, 28)（避开其图标 168..192）
    assert_eq!(pixel(&mut ltr, 155, 28), to_rgba(container), "未选中项不应有胶囊");
    // 选中项 label 已渲染（y 48..64 区域存在非容器色像素）
    let mut label_pixels = 0;
    for y in 50..62 {
        for x in 38..78 {
            if pixel(&mut ltr, x, y) != to_rgba(container) {
                label_pixels += 1;
            }
        }
    }
    assert!(label_pixels > 0, "选中项 label 应可见");

    // RTL：选中项镜像到最右三分之一，胶囊跟随
    let mut rtl = render_navigation_bar_case(LayoutDirection::Rtl);
    assert_eq!(pixel(&mut rtl, 325, 28), to_rgba(indicator), "RTL 下选中项胶囊镜像到最右");
    assert_eq!(pixel(&mut rtl, 35, 28), to_rgba(container), "RTL 下最左三分之一无胶囊");
}

#[test]
fn navigation_bar_horizontal_item_pill_wraps_icon_label_group() {
    let theme = ThemeColors::default_light();
    let container = theme.surface_container;
    let indicator = theme.secondary_container;
    let to_rgba = |c: Color| (c.r, c.g, c.b, c.a);

    let mut composer = Composer::new();
    composer.compose(winia::app_root!(|ctx| {
        WiniaTheme::with_theme_and_direction(ThemeColors::default_light(), LayoutDirection::Ltr, ctx, |ctx| {
            NavigationBar::new(|ctx| {
                for i in 0..3 {
                    NavigationBarItem::new(
                        i == 0,
                        |ctx| {
                            let key = ctx.next_key();
                            ctx.start_leaf(key, Modifier::new().size(24.0, 24.0));
                            ctx.end_node();
                        },
                    )
                    .label(move |ctx| Text::new(format!("Tab{i}")).build(ctx))
                    .layout(winia::ui::NavigationBarItemLayout::Horizontal)
                    .on_click(|| {})
                    .build(ctx);
                }
            })
            .build(ctx);
        });
    }));
    composer.layout(Constraints::new(0.0, 360.0, 0.0, 80.0));
    let mut surface = skia_safe::surfaces::raster_n32_premul((360, 80)).expect("navbar surface");
    surface.canvas().clear(skia_safe::Color::WHITE);
    let root = composer.layout_root_idx().unwrap();
    render::render(composer.arena_nodes(), root, surface.canvas());

    // 从布局树取选中项（水平）指示器几何——胶囊应横向包裹 [icon+gap+label] 整组，高 40
    let nodes = composer.arena_nodes();
    let item0 = nodes[nodes[root].children[0]].children[0];
    let pill = &nodes[item0];
    assert_eq!(pill.measured_size.height, winia::ui::NAVIGATION_BAR_H_INDICATOR_HEIGHT,
        "水平指示器高应为 40");
    let px = pill.position.x;
    let py = pill.position.y;
    let pw = pill.measured_size.width;
    // 胶囊内四角附近（避开 CornerFull 圆角）：中心行/列取色
    let mut s = surface;
    assert_eq!(pixel(&mut s, (px + 6.0) as i32, (py + 20.0) as i32), to_rgba(indicator),
        "胶囊左端应为 SecondaryContainer");
    assert_eq!(pixel(&mut s, (px + pw - 6.0) as i32, (py + 20.0) as i32), to_rgba(indicator),
        "胶囊右端应包住 label 尾部");
    assert_eq!(pixel(&mut s, (px + pw / 2.0) as i32, (py + 4.0) as i32), to_rgba(indicator),
        "胶囊顶部（40 高居中于 80 bar）");
    // 胶囊外左右应为容器色（leading/trailing 各 16dp 之外）
    assert_eq!(pixel(&mut s, (px - 8.0) as i32, (py + 20.0) as i32), to_rgba(container),
        "胶囊左侧之外应为容器色");
    assert_eq!(pixel(&mut s, (px + pw + 8.0) as i32, (py + 20.0) as i32), to_rgba(container),
        "胶囊右侧之外应为容器色");
}
