//! Brushes: gradients, and where a plain color is not enough.
//!
//! Six panels, one per thing the API can do: a linear run, a diagonal one, a radial "spotlight" on a
//! wide box, a sweep, explicit stops, and a gradient over a rounded shape. The last one is the case a
//! `background` cannot express at all — the fill follows the component's own shape.
//!
//! Gradient coordinates are FRACTIONS of each node's bounds (see `docs/brush.md`), which is why the
//! same brush fills a short panel and a tall one. Each label sits ABOVE its panel rather than on it: a
//! gradient runs from one color to another, so no single text color stays readable across it.
//!
//! Run: `cargo run -p winia --example brush_demo`

use winia::prelude::*;
// Shared example chrome: top app bar with the settings sheet (theme mode + layout direction).
#[path = "common/settings.rs"]
mod settings;

/// One case: a label, then the gradient panel under it.
#[composable]
fn case(ctx: &mut ComposeCtx, label: &'static str, brush: Brush, shape: Shape, height: f32) {
    let colors = WiniaTheme::colors();
    Column::new()
        .modifier(Modifier::new().fill_max_width())
        .spacing(6.0)
        .build(ctx, |ctx| {
            Text::new(label)
                .font_size(12.0)
                .color(colors.on_surface_variant)
                .build(ctx);
            Column::new()
                .modifier(Modifier::new()
                    .fill_max_width()
                    .height(height)
                    .background_brush(brush, shape))
                .build(ctx, |_| {});
        });
}

#[composable]
fn brush_demo(ctx: &mut ComposeCtx) {
    // Colors come from the window's theme, so the demo follows the settings sheet's light/dark switch.
    let colors = WiniaTheme::colors();
    let (primary, secondary, tertiary) = (colors.primary, colors.secondary, colors.tertiary);

    LazyColumn::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .spacing(14.0)
        .items_plain(6, move |ctx, index| {
            match index {
                0 => case(
                    ctx,
                    "linear_gradient: left to right, the default",
                    Brush::linear_gradient([primary, tertiary]),
                    Shape::rounded(12.0),
                    64.0,
                ),
                1 => case(
                    ctx,
                    "diagonal(): top-left to bottom-right",
                    Brush::linear_gradient([tertiary, primary]).diagonal(),
                    Shape::rounded(12.0),
                    64.0,
                ),
                2 => case(
                    ctx,
                    "radial_gradient: a spotlight, radius = half the shorter side",
                    Brush::radial_gradient([primary, colors.surface_container_lowest]),
                    Shape::rounded(12.0),
                    64.0,
                ),
                3 => case(
                    ctx,
                    "sweep_gradient: one turn clockwise from 3 o'clock",
                    Brush::sweep_gradient([primary, secondary, tertiary, primary]),
                    Shape::rounded(12.0),
                    64.0,
                ),
                4 => case(
                    ctx,
                    "positions([0.6, 1.0]): hold the first color, then blend",
                    Brush::linear_gradient([tertiary, primary]).positions([0.6, 1.0]),
                    Shape::rounded(12.0),
                    64.0,
                ),
                _ => case(
                    ctx,
                    "the fill follows the SHAPE, not the box (off-centre, radius 1.2)",
                    Brush::radial_gradient([tertiary, primary]).center(0.5, 0.0).radius(1.2),
                    Shape::RoundedRect { corner_radius: 36.0 },
                    64.0,
                ),
            }
        })
        .build(ctx);
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(520.0, 620.0)
                .title("Brush demo")
                .build(ctx, |ctx| {
                    settings::shell("Brush demo", ctx, |ctx| brush_demo(ctx));
                });
        });
    });
}
