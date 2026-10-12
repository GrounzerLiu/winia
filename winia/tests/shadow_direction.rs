//! The elevation shadow's direction, measured in pixels.
//!
//! Compose draws its elevation shadows with the light directly above the component (Android's spot shadow is a
//! vertical skirt: no horizontal bias, and more shadow below than above). winia calls Skia's
//! `SkShadowUtils::draw_shadow` with an explicit point light, and had it at `(270, 0, 600)` — off to one side —
//! which skews every shadow horizontally. This test samples the four sides of a shadowed box.

use winia::prelude::*;
use winia::runtime::composer::Composer;

fn raster(w: i32, h: i32) -> skia_safe::Surface {
    let info = skia_safe::ImageInfo::new(
        (w, h),
        skia_safe::ColorType::RGBA8888,
        skia_safe::AlphaType::Premul,
        None,
    );
    skia_safe::surfaces::raster(&info, None, None).expect("a raster surface")
}

fn gray(surface: &mut skia_safe::Surface, x: i32, y: i32) -> u8 {
    let mut buf = [0u8; 4];
    let info = skia_safe::ImageInfo::new(
        (1, 1),
        skia_safe::ColorType::RGBA8888,
        skia_safe::AlphaType::Premul,
        None,
    );
    let _ = surface.read_pixels(&info, &mut buf, 4, (x, y));
    buf[1]
}

/// A 60x40 box with an 8 dp shadow, centred in a 120x120 surface so there is room on all four sides.
fn shadow_scene() -> skia_safe::Surface {
    let mut surface = raster(120, 120);
    surface.canvas().clear(skia_safe::Color::WHITE);
    let mut composer = Composer::new();
    composer.compose(|ctx| {
        ctx.key("shadow-scene", |ctx| {
        Stack::new()
            .modifier(Modifier::new().fill_max_size())
            .content_alignment(ContentAlignment::CENTER)
            .build(ctx, |ctx| {
                Column::new()
                    .modifier(
                        Modifier::new()
                            .size(60.0, 40.0)
                            .graphics_layer(|| winia::graphics::GraphicsLayerParams {
                                shadow_elevation: 8.0,
                                ..Default::default()
                            })
                            .background(Color::WHITE, Shape::Rectangle),
                    )
                    .build(ctx, |_| {});
            });
        });
    });
    composer.layout(Constraints::new(0.0, 120.0, 0.0, 120.0));
    let nodes = composer.arena_nodes();
    let root = composer.layout_root_idx().expect("laid out");
    winia::render::render(nodes, root, surface.canvas());
    surface
}

#[test]
fn the_elevation_shadow_falls_below_and_not_to_one_side() {
    let mut surface = shadow_scene();
    // The box is 60x40 in the middle of a 120x120 surface, so its edges are at x=30/90 and y=40/80.
    let above = gray(&mut surface, 60, 30);
    let below = gray(&mut surface, 60, 92);
    let left = gray(&mut surface, 20, 60);
    let right = gray(&mut surface, 100, 60);

    // Measured with the light at `(270, 0, 600)` (what the renderer used): below 255, left 250, right 255 —
    // the shadow was cast to the LEFT with no downward skirt at all. With it above the object: below 242 and
    // decaying outward, left 254, right 254.
    assert!(
        below < above,
        "the shadow is below the box, not above it (below {below} against above {above})"
    );
    assert!(
        (left as i32 - right as i32).abs() <= 6,
        "and it is not biased to one side (left {left} against right {right})"
    );
}
