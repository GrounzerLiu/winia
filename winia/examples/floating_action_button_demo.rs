//! Floating Action Button demo — what each section shows, and what to look at.
//!
//! Run: `cargo run -p winia --example floating_action_button_demo`
//!
//! 1. Elevation animates on the INTERACTION PAIR (`ElevationDefaults`, the rule Button/Card share):
//!    with an exaggerated 0 / pressed 6 / hovered 20 set, hovering lifts the shadow over 120 ms of
//!    FastOutSlowIn, pressing pulls it back to 6 on the incoming curve, and leaving the hover drops it
//!    over 150 ms of the exit curve. The second FAB carries the token set M3 ships, 6 / 6 / 6 / 8.
//! 2. The four size variants, each with the icon size and shape it resolves (40 / 56 / 80 / 96 dp).
//! 3. The colour mappings: primary container, secondary container, tertiary, and a disabled FAB
//!    (on-surface at 12 % / 38 %).
//! 4. Lowered elevation, a custom `Shape::Circle`, and a shadow-free FAB: `bottom_app_bar`'s every
//!    state is 0, so `is_shadow_free` builds no shadow layer at all — it can never grow one.
//! 5. Extended FAB: collapsed 56×56 with the icon centred, expanded to [16 | icon | 12 | label | 20]
//!    with a minimum width of 80 — width, icon position and the label's alpha all follow one progress.
//!    Hover it too: its elevation animates through the same rule (it used to jump). Switch the layout
//!    direction in the settings sheet to watch the expanded FAB mirror.
//! 6. The content-only extended FAB — material3's other overload. One caller-composed row instead of
//!    the icon/text split: 20 dp of horizontal padding, an 80 dp floor, 56 dp tall, and nothing to
//!    expand or collapse. The second FAB builds its own icon-plus-label row, gap and all.

use letclone::clone;
use winia::prelude::*;
// Shared example chrome: top app bar with the settings sheet (theme mode + layout direction).
#[path = "common/settings.rs"]
mod settings;

const STAR_PATH: &str = "M12 17.27L18.18 21l-1.64-7.03L22 9.24l-7.19-.61L12 2 9.19 8.63 2 9.24l5.46 4.73L5.82 21z";
const PLUS_PATH: &str = "M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z";

fn label_color() -> Color {
    WiniaTheme::colors().on_surface_variant
}

fn section_title(ctx: &mut ComposeCtx, text: &str) {
    Text::new(text)
        .font_size(14.0)
        .color(label_color())
        .modifier(Modifier::new().padding_top(14.0).padding_bottom(6.0))
        .build(ctx);
}

fn caption(ctx: &mut ComposeCtx, text: &str) {
    Text::new(text)
        .font_size(11.0)
        .color(label_color())
        .modifier(Modifier::new().padding_top(4.0))
        .build(ctx);
}

#[composable]
fn floating_action_button_demo(ctx: &mut ComposeCtx) {
    let clicks = ctx.remember(|| 0i32);
    let ext_expanded = ctx.remember(|| true);
    let scroll = ctx.remember(|| ScrollState::new()).get();
    let theme = WiniaTheme::colors();

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0).vertical_scroll(scroll))
        .build(ctx, |ctx| {
            Text::new(format!("Clicks: {}", clicks.get()))
                .font_size(13.0)
                .build(ctx);

            // ── 1: the interaction pair drives the elevation animation ──
            section_title(ctx, "1. Elevation animates on the interaction pair");
            Row::new().spacing(28.0).build(ctx, |ctx| {
                for (elevation, label) in [
                    (
                        FloatingActionButtonElevation::new(0.0, 6.0, 0.0, 20.0, 0.0),
                        "Exaggerated 0 / 6 / 20",
                    ),
                    (
                        FloatingActionButtonDefaults::elevation(),
                        "M3 tokens 6 / 6 / 6 / 8",
                    ),
                ] {
                    Column::new()
                        .alignment(Alignment::Center)
                        .spacing(6.0)
                        .build(ctx, |ctx| {
                            FloatingActionButton::new()
                                .elevation(elevation)
                                .on_click({ clone!(clicks); move || clicks.update(|v| *v += 1) })
                                .build(ctx, |ctx| Icon::svg_path(STAR_PATH).build(ctx));
                            Text::new(label).font_size(11.0).color(label_color()).build(ctx);
                        });
                }
            });
            caption(
                ctx,
                "Left: an exaggerated 0 / 6 / 20 set so the motion is easy to see — hover it (up, 120 ms \
                 FastOutSlowIn), press it (back to 6), leave it (down, 150 ms of the exit curve). Right: the \
                 set M3 ships, 6 / 6 / 6 / 8.",
            );

            // ── 2: sizes ──
            section_title(ctx, "2. Sizes — 40 / 56 / 80 / 96 dp");
            Row::new().spacing(20.0).build(ctx, |ctx| {
                for (size, label) in [
                    (FloatingActionButtonSize::Small, "40 dp"),
                    (FloatingActionButtonSize::Regular, "56 dp"),
                    (FloatingActionButtonSize::Medium, "80 dp"),
                    (FloatingActionButtonSize::Large, "96 dp"),
                ] {
                    Column::new()
                        .alignment(Alignment::Center)
                        .spacing(6.0)
                        .build(ctx, |ctx| {
                            FloatingActionButton::new()
                                .size(size)
                                .on_click({ clone!(clicks); move || clicks.update(|v| *v += 1) })
                                .build(ctx, |ctx| {
                                    Icon::svg_path(STAR_PATH).size(size.icon_size()).build(ctx);
                                });
                            Text::new(label).font_size(11.0).color(label_color()).build(ctx);
                        });
                }
            });
            caption(
                ctx,
                "Each size carries its own shape (CornerMedium / CornerLarge / CornerLargeIncreased / \
                 CornerExtraLarge) and its own icon size (24 / 24 / 28 / 32).",
            );

            // ── 3: colour mappings, plus disabled ──
            section_title(ctx, "3. Colour mappings");
            Row::new().spacing(20.0).build(ctx, |ctx| {
                FloatingActionButton::new()
                    .colors(FloatingActionButtonDefaults::primary_colors(&theme))
                    .on_click({ clone!(clicks); move || clicks.update(|v| *v += 1) })
                    .build(ctx, |ctx| Icon::svg_path(STAR_PATH).build(ctx));
                FloatingActionButton::new()
                    .colors(FloatingActionButtonDefaults::secondary_colors(&theme))
                    .on_click({ clone!(clicks); move || clicks.update(|v| *v += 1) })
                    .build(ctx, |ctx| Icon::svg_path(STAR_PATH).build(ctx));
                FloatingActionButton::new()
                    .colors(FloatingActionButtonDefaults::tertiary_colors(&theme))
                    .on_click({ clone!(clicks); move || clicks.update(|v| *v += 1) })
                    .build(ctx, |ctx| Icon::svg_path(STAR_PATH).build(ctx));
                FloatingActionButton::new()
                    .colors(FloatingActionButtonDefaults::tertiary_colors(&theme))
                    .enabled(false)
                    .on_click(|| {})
                    .build(ctx, |ctx| Icon::svg_path(STAR_PATH).build(ctx));
            });
            caption(
                ctx,
                "Primary / secondary / tertiary container, then the same FAB disabled: on-surface fill at \
                 12 %, content at 38 %, and no click target.",
            );

            // ── 4: lowered / circle / shadow-free ──
            section_title(ctx, "4. Lowered elevation, a circle, and a shadow-free FAB");
            Row::new().spacing(20.0).build(ctx, |ctx| {
                FloatingActionButton::medium()
                    .elevation(FloatingActionButtonDefaults::lowered_elevation())
                    .on_click({ clone!(clicks); move || clicks.update(|v| *v += 1) })
                    .build(ctx, |ctx| Icon::svg_path(STAR_PATH).build(ctx));
                FloatingActionButton::new()
                    .shape(Shape::Circle)
                    .on_click({ clone!(clicks); move || clicks.update(|v| *v += 1) })
                    .build(ctx, |ctx| Icon::svg_path(STAR_PATH).build(ctx));
                FloatingActionButton::new()
                    .elevation(FloatingActionButtonDefaults::bottom_app_bar_elevation())
                    .on_click({ clone!(clicks); move || clicks.update(|v| *v += 1) })
                    .build(ctx, |ctx| Icon::svg_path(STAR_PATH).build(ctx));
            });
            caption(
                ctx,
                "Lowered is 1 / 1 / 1 / 3 dp and still animates; the circle is the shape override. The third \
                 carries bottomAppBar's set — every state is 0 — so `is_shadow_free` skips the shadow layer \
                 outright: hover it and nothing lifts, unlike the two beside it.",
            );

            // ── 5: extended FAB ──
            section_title(ctx, "5. Extended FAB — collapsed 56×56 / expanded min 80");
            Row::new().spacing(20.0).build(ctx, |ctx| {
                ExtendedFloatingActionButton::new(
                    |ctx| Text::new("Create").build(ctx),
                    |ctx| Icon::svg_path(PLUS_PATH).size(24.0).build(ctx),
                    ext_expanded.clone(),
                )
                .on_click({ clone!(clicks); move || clicks.update(|v| *v += 1) })
                .build(ctx);

                Button::new()
                    .style(ButtonStyle::Tonal)
                    .on_click({ clone!(ext_expanded); move || ext_expanded.update(|v| *v = !*v) })
                    .build(ctx, |ctx| {
                        Text::new(if ext_expanded.get() { "Collapse" } else { "Expand" }).build(ctx);
                    });
            });
            caption(
                ctx,
                "Collapsed it is 56×56 with the icon centred; expanded it is [16 | icon | 12 | label | 20] \
                 with a minimum width of 80. Width, icon position and the label's alpha all follow one \
                 progress. Hover it: its elevation now animates through the same interaction-pair rule as \
                 the others — it used to jump. Switch the settings sheet's direction to watch it mirror.",
            );

            // ── 6: the content-only extended FAB ──
            section_title(ctx, "6. Extended FAB, content-only — one row, no icon slot");
            Row::new().spacing(20.0).build(ctx, |ctx| {
                // The case this overload exists for: a label with no icon at all.
                ExtendedFloatingActionButton::content(|ctx| Text::new("Create").build(ctx))
                    .on_click({ clone!(clicks); move || clicks.update(|v| *v += 1) })
                    .build(ctx);

                // The row is the caller's, so an icon plus a label is just as easy — you write the gap.
                ExtendedFloatingActionButton::content(|ctx| {
                    Row::new()
                        .spacing(8.0)
                        .alignment(Alignment::Center)
                        .build(ctx, |ctx| {
                            Icon::svg_path(PLUS_PATH).size(24.0).build(ctx);
                            Text::new("Compose freely").build(ctx);
                        });
                })
                .on_click({ clone!(clicks); move || clicks.update(|v| *v += 1) })
                .build(ctx);
            });
            caption(
                ctx,
                "One caller-composed row instead of the icon/text split: 20 dp of horizontal padding, an \
                 80 dp floor, 56 dp tall, and nothing to expand or collapse. The first is the case the \
                 overload exists for — a label with no icon; the second shows the row is yours, so the \
                 icon, the 8 dp gap and the order are all the caller's.",
            );
        });
}

fn main() {
    winia::run_app!(|ctx| {
        winia::theme::WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(760.0, 820.0)
                .title("Floating Action Button Demo")
                .build(ctx, |ctx| {
                    settings::shell("Floating Action Button Demo", ctx, floating_action_button_demo);
                });
        });
    });
}
