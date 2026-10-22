//! Shadow elevation demo: drag the slider and watch the layer shadow grow.
//!
//! The renderer compresses elevation before it becomes Skia's Z plane (see
//! `docs/draw-layer-extras.md`), so the steps get smaller as the value rises: 1 dp keeps its size,
//! 8 dp looks like 5.33, 24 dp like 9.6. The fixed strip below shows the same curve at the Material
//! token levels, so the saturation is visible next to the live slider.
//!
//! Run: cargo run -p winia --example shadow_elevation_demo

use winia::graphics::GraphicsLayerParams;
use winia::prelude::*;

#[path = "common/settings.rs"]
mod settings;

fn swatch(ctx: &mut ComposeCtx, elevation: f32, theme: winia::theme::ThemeColors) {
    let shape = Shape::rounded(10.0);
    let label = format!("{elevation:.0}");
    Column::new()
        .spacing(6.0)
        .build(ctx, |ctx| {
            Stack::new()
                .content_alignment(ContentAlignment::CENTER)
                .modifier(Modifier::new().size(56.0, 44.0))
                .build(ctx, |ctx| {
                    Column::new()
                        .modifier(
                            Modifier::new()
                                .size(56.0, 44.0)
                                .graphics_layer(move || GraphicsLayerParams {
                                    shadow_elevation: elevation,
                                    shadow_shape: Some(shape),
                                    ..Default::default()
                                })
                                .background(theme.surface_container_highest, shape),
                        )
                        .build(ctx, |_| {});
                });
            Text::new(label)
                .font_size(11.0)
                .color(theme.on_surface_variant)
                .build(ctx);
        });
}

#[composable]
fn shadow_elevation_demo(ctx: &mut ComposeCtx) {
    let theme = WiniaTheme::colors();
    let elevation = ctx.remember(|| 8.0f32);
    let shape = Shape::rounded(20.0);

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(24.0))
        .spacing(18.0)
        .build(ctx, |ctx| {
            Text::new("Elevation shadow")
                .font_size(22.0)
                .build(ctx);
            Text::new("Skia draws ambient + spot from the Z plane; winia compresses Z with K*e/(K+e), K = 16.")
                .font_size(12.0)
                .color(theme.on_surface_variant)
                .build(ctx);

            Stack::new()
                .content_alignment(ContentAlignment::CENTER)
                .modifier(Modifier::new().fill_max_width().height(240.0))
                .build(ctx, {
                    let elevation = elevation.clone();
                    move |ctx| {
                        let live = elevation.clone();
                        let readout = elevation.clone();
                        Column::new()
                            .modifier(
                                Modifier::new()
                                    .size(280.0, 150.0)
                                    .graphics_layer(move || GraphicsLayerParams {
                                        shadow_elevation: live.peek(),
                                        shadow_shape: Some(shape),
                                        ..Default::default()
                                    })
                                    .background(theme.surface_container_highest, shape),
                            )
                            .build(ctx, move |ctx| {
                                Stack::new()
                                    .content_alignment(ContentAlignment::CENTER)
                                    .modifier(Modifier::new().fill_max_size())
                                    .build(ctx, move |ctx| {
                                        Text::new(format!("{:.1} dp", readout.get()))
                                            .font_size(24.0)
                                            .color(theme.on_surface)
                                            .build(ctx);
                                    });
                            });
                    }
                });

            Slider::new(elevation.get())
                .value_range(0.0, 32.0)
                .on_value_change({
                    let elevation = elevation.clone();
                    move |value| elevation.set(value)
                })
                .build(ctx);
            Text::new("0 dp")
                .font_size(10.0)
                .color(theme.on_surface_variant)
                .modifier(Modifier::new().align_self(Alignment::Start))
                .build(ctx);

            Text::new("Fixed token levels")
                .font_size(13.0)
                .color(theme.on_surface_variant)
                .build(ctx);
            Row::new()
                .spacing(10.0)
                .build(ctx, |ctx| {
                    for step in [1.0, 3.0, 6.0, 8.0, 12.0, 16.0] {
                        swatch(ctx, step, theme);
                    }
                });
        });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(620.0, 640.0)
                .title("Shadow elevation")
                .build(ctx, |ctx| {
                    settings::shell("Shadow elevation", ctx, |ctx| shadow_elevation_demo(ctx));
                });
        });
    });
}
