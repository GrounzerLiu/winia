//! Loading Indicator demo (M3 Expressive — Compose's `LoadingIndicator` / `ContainedLoadingIndicator`).
//!
//! Shows:
//! - Uncontained (the default: no container, a Primary indicator)
//! - Contained (a round container, PrimaryContainer + OnPrimaryContainer)
//! - Custom colours, container shapes and scales (24–96dp)
//! - The seven-shape morph loop
//! - The determinate variant, which morphs between its two shapes by progress instead of spinning

use winia::prelude::*;
// `MaterialShapes` is not re-exported by winia (only `RoundedPolygon` is), so this demo names the shapes
// crate directly — it is a dependency of winia, which is all an example in this package needs.
use material_shapes::MaterialShapes;
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
fn loading_indicator_demo(ctx: &mut ComposeCtx) {
    // The determinate section made this taller than the window, so the column scrolls.
    let scroll_y = ctx.remember(|| ScrollState::new()).get();
    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0).vertical_scroll(scroll_y))
        .build(ctx, |ctx| {
            section_title(ctx, "Uncontained（默认）");
            Row::new().spacing(24.0).build(ctx, |ctx| {
                LoadingIndicator::new().build(ctx);
                LoadingIndicator::new()
                    .indicator_color(Color::from_argb(255, 46, 125, 50))
                    .build(ctx);
            });

            section_title(ctx, "Container（SecondaryContainer）");
            Row::new().spacing(24.0).build(ctx, |ctx| {
                LoadingIndicator::new().contained(true).build(ctx);
                LoadingIndicator::new()
                    .contained(true)
                    .indicator_color(Color::from_argb(255, 255, 255, 255))
                    .container_color(Color::from_argb(255, 46, 125, 50))
                    .build(ctx);
            });

            section_title(ctx, "Container 自定义形状（RoundedRect）");
            Row::new().spacing(24.0).build(ctx, |ctx| {
                LoadingIndicator::new()
                    .contained(true)
                    .container_shape(Shape::rounded(12.0))
                    .build(ctx);
                LoadingIndicator::new()
                    .contained(true)
                    .container_shape(Shape::Pill)
                    .build(ctx);
            });

            section_title(ctx, "缩放尺寸（24 / 48 / 96dp）");
            Row::new().spacing(24.0).build(ctx, |ctx| {
                LoadingIndicator::new()
                    .contained(true)
                    .modifier(Modifier::new().size(24.0, 24.0))
                    .build(ctx);
                LoadingIndicator::new().build(ctx);
                LoadingIndicator::new()
                    .contained(true)
                    .modifier(Modifier::new().size(96.0, 96.0))
                    .build(ctx);
            });

            // Determinate: `progress` picks the morph between two shapes (circle → soft burst), with no
            // looping and no rotation. The three fixed ones show the ends and a midpoint of that one morph.
            section_title(ctx, "Determinate — driven by progress, no spin");
            Row::new().spacing(24.0).build(ctx, |ctx| {
                LoadingIndicator::determinate(ctx.remember(|| 0.0_f32)).build(ctx);
                LoadingIndicator::determinate(ctx.remember(|| 0.5_f32)).build(ctx);
                LoadingIndicator::determinate(ctx.remember(|| 1.0_f32)).build(ctx);
            });
            // ... and one driven from state, so the morph AND the −180° rotation can both be watched as
            // the progress moves. A determinate indicator registers no animation of its own: it redraws
            // only when this state changes.
            let determinate_progress = ctx.remember(|| 0.25_f32);
            Row::new().spacing(16.0).build(ctx, |ctx| {
                LoadingIndicator::determinate(determinate_progress.clone()).build(ctx);
                let dragged = determinate_progress.clone();
                Slider::new(determinate_progress.get())
                    .value_range(0.0, 1.0)
                    .modifier(Modifier::new().width(200.0))
                    .on_value_change(move |v| dragged.update(|p| *p = v))
                    .build(ctx);
            });
            Text::new("drag the slider: the glyph morphs circle → soft burst and turns as progress goes 0 → 1")
                .font_size(11.0)
                .color(WiniaTheme::colors().on_surface_variant)
                .build(ctx);

            // `shapes` replaces the built-in sequence, as upstream's `polygons` parameter does. On the
            // determinate variant the first two shapes are the ends of its single morph; the indeterminate
            // one walks the whole sequence in order and wraps from the last shape back to the first.
            section_title(ctx, "Custom shape sequence");
            Row::new().spacing(24.0).build(ctx, |ctx| {
                LoadingIndicator::determinate(ctx.remember(|| 0.5_f32))
                    .shapes(vec![
                        MaterialShapes::soft_burst().normalized(),
                        MaterialShapes::pentagon().normalized(),
                    ])
                    .build(ctx);
                LoadingIndicator::new()
                    .shapes(vec![
                        MaterialShapes::circle().normalized(),
                        MaterialShapes::pill().normalized(),
                        MaterialShapes::sunny().normalized(),
                    ])
                    .build(ctx);
            });
        });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(420.0, 620.0)
                .title("Loading Indicator Demo")
                .build(ctx, |ctx| {
                    settings::shell("Loading Indicator Demo", ctx, loading_indicator_demo);
                });
        });
    });
}
