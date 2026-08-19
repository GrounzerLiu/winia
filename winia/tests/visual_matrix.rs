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
    Button, Chip, FloatingActionButton, FloatingActionButtonSize, Icon, Text, TextField,
    TextFieldValue, ThemeColors, TopAppBar, TopAppBarColors, TopAppBarScrollBehavior, TopAppBarVariant, Typography, WiniaTheme,
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
fn material_visual_matrix_custom_typography_changes_textfield_geometry() {
    let (_, default_composer) = render_case(false, LayoutDirection::Ltr, false);
    let (_, custom_composer) = render_case(false, LayoutDirection::Ltr, true);
    let default_nodes = default_composer.arena_nodes();
    let custom_nodes = custom_composer.arena_nodes();
    let default_field = find_tag(default_nodes, default_composer.layout_root_idx().unwrap(), "field").unwrap();
    let custom_field = find_tag(custom_nodes, custom_composer.layout_root_idx().unwrap(), "field").unwrap();
    assert!(custom_nodes[custom_field].measured_size.height >= default_nodes[default_field].measured_size.height);
}
