//! Material 3 top app bars with the shared-offset collapse behavior, and the two FLEXIBLE
//! variants (`MediumFlexibleTopAppBar` / `LargeFlexibleTopAppBar`).
//!
//! Cycle the bar with the button: `Large` (152 dp, HeadlineMedium title), `MediumFlexible`
//! (136 dp WITH a subtitle, 112 without, HeadlineMedium title + LabelLarge subtitle) and
//! `LargeFlexible` (152 with a subtitle, 120 without, DisplaySmall title + TitleMedium subtitle).
//! The flexible pair do not carry a fixed expanded height — the subtitle decides which of their
//! two constants applies, which is why the scroll behavior is built from
//! `top_app_bar_expanded_height(variant, has_subtitle)` rather than a constant.

use letclone::clone;
use winia::prelude::*;
// Shared example chrome: top app bar with the settings sheet (theme mode + layout direction).
#[path = "common/settings.rs"]
mod settings;


const ARROW_BACK_PATH: &str = "M20 11H7.83l5.59-5.59L12 4l-8 8 8 8 1.41-1.41L7.83 13H20v-2z";
const MORE_VERT_PATH: &str = "M12 8c1.1 0 2-.9 2-2s-.9-2-2-2-2 .9-2 2 .9 2 2 2zm0 2c-1.1 0-2 .9-2 2s.9 2 2 2 2-.9 2-2-.9-2-2-2zm0 6c-1.1 0-2 .9-2 2s.9 2 2 2 2-.9 2-2-.9-2-2-2z";

#[composable]
fn top_app_bar_demo(ctx: &mut ComposeCtx) {
    let scroll = ctx.remember(|| ScrollState::new()).get();
    let which = ctx.remember(|| 0usize);
    // The subtitle is always on here, so the flexible variants take their TALLER constant.
    let variant = match which.get() % 3 {
        0 => TopAppBarVariant::Large,
        1 => TopAppBarVariant::MediumFlexible,
        _ => TopAppBarVariant::LargeFlexible,
    };
    let expanded = top_app_bar_expanded_height(variant, true);
    let behavior = TopAppBarScrollBehavior::new(scroll.clone(), expanded);
    let fraction = behavior.collapse_fraction();
    let height = expanded - (expanded - TOP_APP_BAR_HEIGHT) * fraction;

    Column::new().modifier(Modifier::new().fill_max_size()).build(ctx, |ctx| {
        match variant {
            TopAppBarVariant::MediumFlexible => TopAppBar::medium_flexible(|ctx| Text::new("Flexible").build(ctx)),
            TopAppBarVariant::LargeFlexible => TopAppBar::large_flexible(|ctx| Text::new("Flexible").build(ctx)),
            _ => TopAppBar::large(|ctx| Text::new("TopAppBar").build(ctx)),
        }
            .navigation_icon(|ctx| Icon::svg_path(ARROW_BACK_PATH).size(24.0).build(ctx))
            .actions(|ctx| Icon::svg_path(MORE_VERT_PATH).size(24.0).build(ctx))
            .subtitle(|ctx| Text::new("Scroll to collapse the shared-offset app bar").build(ctx))
            .scroll_behavior(behavior)
            .build(ctx);
        Row::new().spacing(12.0).modifier(Modifier::new().padding(8.0).fill_max_width()).build(ctx, |ctx| {
            Button::text()
                .on_click({ clone!(which); move || which.update(|v| *v += 1) })
                .build(ctx, |ctx| Text::new("Next variant").build(ctx));
            // `which.get()` is read HERE, in the row's own scope, and not lifted out of it. The
            // row's parameters never change, so a Clean slot SKIPS the closure entirely — a value
            // captured from the enclosing scope then stays at whatever it was when the row last
            // composed, and the label kept naming the old variant while the bar above it had
            // already moved. Reading the state inside the scope that renders it subscribes THAT
            // scope, so the change re-runs it.
            let current = which.get() % 3;
            let shown_variant = match current {
                0 => TopAppBarVariant::Large,
                1 => TopAppBarVariant::MediumFlexible,
                _ => TopAppBarVariant::LargeFlexible,
            };
            let shown_expanded = top_app_bar_expanded_height(shown_variant, true);
            // `behavior` was moved into the bar above, so this scope builds its own over the same
            // scroll state.
            let shown_behavior = TopAppBarScrollBehavior::new(scroll.clone(), shown_expanded);
            Text::new(format!(
                "{shown_variant:?} — {shown_expanded:.0}dp expanded | fraction: {:.2} | height: {:.0}dp",
                shown_behavior.collapse_fraction(),
                shown_behavior.current_height(),
            ))
            .font_size(12.0)
            .build(ctx);
        });

        Column::new()
            .modifier(Modifier::new().fill_max_width().fill_max_height().vertical_scroll(scroll))
            .build(ctx, |ctx| {
                for index in 0..40 {
                    Text::new(format!("Scrollable content {index}"))
                        .modifier(Modifier::new().padding(16.0).fill_max_width())
                        .build(ctx);
                }
            });
    });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(520.0, 760.0)
                .title("TopAppBar Demo")
                .build(ctx, |ctx| {
                    settings::shell("TopAppBar Demo", ctx, top_app_bar_demo);
                });
        });
    });
}
