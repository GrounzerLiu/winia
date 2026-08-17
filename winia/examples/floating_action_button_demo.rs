//! Floating Action Button demo for Material 3 size and interaction variants.

use winia::prelude::*;

#[composable]
fn floating_action_button_demo(ctx: &mut ComposeCtx) {
    let clicks = ctx.remember(|| 0i32);
    let click_count = clicks.get();
    let theme = WiniaTheme::colors();
    let star =
        "M12 17.27L18.18 21l-1.64-7.03L22 9.24l-7.19-.61L12 2 9.19 8.63 2 9.24l5.46 4.73L5.82 21z";

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(24.0))
        .spacing(18.0)
        .build(ctx, |ctx| {
            Text::new("Floating Action Button")
                .font_size(24.0)
                .build(ctx);
            Text::new(format!("Clicks: {click_count}"))
                .font_size(14.0)
                .build(ctx);

            Row::new().spacing(16.0).build(ctx, |ctx| {
                for size in [
                    FloatingActionButtonSize::Small,
                    FloatingActionButtonSize::Regular,
                    FloatingActionButtonSize::Medium,
                    FloatingActionButtonSize::Large,
                ] {
                    let c = clicks.clone();
                    FloatingActionButton::new()
                        .size(size)
                        .on_click(move || c.update(|v| *v += 1))
                        .build(ctx, |ctx| {
                            Icon::svg_path(star).size(size.icon_size()).build(ctx);
                        });
                }
            });

            Text::new("Primary / secondary / tertiary color mappings")
                .font_size(14.0)
                .build(ctx);
            Row::new().spacing(16.0).build(ctx, |ctx| {
                let c = clicks.clone();
                FloatingActionButton::new()
                    .colors(FloatingActionButtonDefaults::primary_colors(&theme))
                    .on_click(move || c.update(|v| *v += 1))
                    .build(ctx, |ctx| Icon::svg_path(star).build(ctx));
                let c = clicks.clone();
                FloatingActionButton::new()
                    .colors(FloatingActionButtonDefaults::secondary_colors(&theme))
                    .on_click(move || c.update(|v| *v += 1))
                    .build(ctx, |ctx| Icon::svg_path(star).build(ctx));
                FloatingActionButton::new()
                    .colors(FloatingActionButtonDefaults::tertiary_colors(&theme))
                    .enabled(false)
                    .on_click(|| {})
                    .build(ctx, |ctx| Icon::svg_path(star).build(ctx));
            });

            Text::new("All sizes use M3 rounded-rectangle shape tokens by default.")
                .font_size(12.0)
                .color(Color::from_argb(255, 90, 90, 90))
                .build(ctx);
            FloatingActionButton::medium()
                .elevation(FloatingActionButtonDefaults::lowered_elevation())
                .on_click(|| {})
                .build(ctx, |ctx| Icon::svg_path(star).build(ctx));

            Text::new("Explicit custom shape")
                .font_size(12.0)
                .color(Color::from_argb(255, 90, 90, 90))
                .build(ctx);
            FloatingActionButton::new()
                .shape(Shape::Circle)
                .on_click(|| {})
                .build(ctx, |ctx| Icon::svg_path(star).build(ctx));
        });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        WiniaTheme::light(ctx, |ctx| {
            Window::new()
                .size(720.0, 520.0)
                .title("Floating Action Button Demo")
                .build(ctx, |ctx| floating_action_button_demo(ctx));
        });
    });
    drop(rt);
}
