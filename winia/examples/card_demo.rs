//! Card styles, states, and interactions — the material3 Card family.
//!
//! What the sections show:
//! 1. the three variants side by side (Filled / Elevated / Outlined) with the token each one uses;
//! 2. a browser-style content card, which is what a card is usually made of (title, supporting text,
//!    a body block and an action row);
//! 3. interaction: a clickable card (ripple, hover/press elevation, and a focus ring you can reach with
//!    Tab — `CardTokens.FocusIndicatorColor` is `secondary`), beside a disabled one;
//! 4. the 48 dp touch target a CLICKABLE card reserves even when its content is smaller, which Compose
//!    gets from the `Surface(onClick)` it is built on;
//! 5. custom shape / colors / border.

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

fn caption(ctx: &mut ComposeCtx, text: &str) {
    Text::new(text)
        .font_size(11.0)
        .color(WiniaTheme::colors().on_surface_variant)
        .modifier(Modifier::new().padding_top(4.0))
        .build(ctx);
}

/// A card whose content is a small piece of UI: what most real cards look like.
#[composable]
fn content_card(ctx: &mut ComposeCtx, clicks: State<i32>) {
    Card::elevated()
        .on_click({ clone!(clicks); move || clicks.update(|v| *v += 1) })
        .build(ctx, |ctx| {
            Column::new()
                .modifier(Modifier::new().fill_max_width().padding(16.0))
                .spacing(8.0)
                .build(ctx, |ctx| {
                    Text::new("Quarterly report").font_size(16.0).build(ctx);
                    Text::new("Numbers, charts and the two things nobody read.")
                        .font_size(12.0)
                        .color(WiniaTheme::colors().on_surface_variant)
                        .build(ctx);
                    Row::new()
                        .modifier(Modifier::new().fill_max_width())
                        .alignment(Alignment::Center)
                        .spacing(8.0)
                        .build(ctx, |ctx| {
                            Button::filled().build(ctx, |ctx| Text::new("Open").build(ctx));
                            Button::text().build(ctx, |ctx| Text::new("Share").build(ctx));
                            Text::new(format!("clicks: {}", clicks.get()))
                                .font_size(12.0)
                                .build(ctx);
                        });
                });
        });
}

#[composable]
fn card_demo(ctx: &mut ComposeCtx) {
    let scroll_y = ctx.remember(|| ScrollState::new()).get();
    let clicks = ctx.remember(|| 0i32);
    let theme = WiniaTheme::colors();

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0).vertical_scroll(scroll_y))
        .spacing(12.0)
        .build(ctx, |ctx| {
            // ── 1. the three variants, no clicks ──
            section_title(ctx, "1. Variants (display only)");
            Card::new().build(ctx, |ctx| {
                Text::new("Filled — surfaceContainerHighest, elevation 0 (1 on hover)")
                    .modifier(Modifier::new().padding(16.0))
                    .build(ctx);
            });
            Card::elevated().build(ctx, |ctx| {
                Text::new("Elevated — surfaceContainerLow, elevation 1 (2 on hover)")
                    .modifier(Modifier::new().padding(16.0))
                    .build(ctx);
            });
            Card::outlined().build(ctx, |ctx| {
                Text::new("Outlined — surface + a 1 dp outlineVariant border, elevation 0")
                    .modifier(Modifier::new().padding(16.0))
                    .build(ctx);
            });

            // ── 2. a card made of real content ──
            section_title(ctx, "2. A content card");
            content_card(ctx, clicks.clone());
            caption(
                ctx,
                "Clickable: ripple, hover/press raise the shadow, and Tab gives it a focus ring \
                 (secondary, per CardTokens.FocusIndicatorColor).",
            );

            // ── 3. disabled ──
            section_title(ctx, "3. Disabled");
            Card::outlined()
                .enabled(false)
                .on_click({ clone!(clicks); move || clicks.update(|v| *v += 1) })
                .build(ctx, |ctx| {
                    Text::new("Disabled — content at 38%, clicks ignored")
                        .modifier(Modifier::new().padding(16.0))
                        .build(ctx);
                });

            // ── 4. the touch target ──
            section_title(ctx, "4. The 48 dp touch target");
            Card::filled()
                .on_click({ clone!(clicks); move || clicks.update(|v| *v += 1) })
                .build(ctx, |ctx| {
                    // Content that is deliberately smaller than a touch target: the card's visual box is this
                    // 120x12 strip, and the clickable box around it is 48 tall.
                    Column::new()
                        .modifier(
                            Modifier::new()
                                .size(120.0, 12.0)
                                .background(theme.primary, Shape::rounded(3.0)),
                        )
                        .build(ctx, |_ctx| {});
                });
            caption(
                ctx,
                "A clickable card reserves at least 48 dp on both axes with its content centred — Compose \
                 gets that from the Surface(onClick) it is built on. A card with no on_click does not.",
            );

            // ── 5. custom shape / colors / border ──
            section_title(ctx, "5. Custom shape / colors / border");
            Card::filled()
                .shape(Shape::rounded(4.0))
                .colors(CardColors::new(
                    Color::from_argb(255, 232, 222, 248),
                    Color::from_argb(255, 33, 33, 33),
                    Color::from_argb(255, 232, 222, 248),
                    Color::from_argb(255, 120, 120, 120),
                ))
                .build(ctx, |ctx| {
                    Text::new("Custom colors + a 4 dp corner")
                        .font_size(14.0)
                        .modifier(Modifier::new().padding(16.0))
                        .build(ctx);
                });
            Card::outlined()
                .border(CardBorder::new(2.0, theme.primary))
                .on_click({ clone!(clicks); move || clicks.update(|v| *v += 1) })
                .build(ctx, |ctx| {
                    Text::new("A custom 2 px border (still clickable)")
                        .modifier(Modifier::new().padding(16.0))
                        .build(ctx);
                });
        });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(520.0, 760.0)
                .title("Card Demo")
                .build(ctx, |ctx| {
                    settings::shell("Card Demo", ctx, |ctx| {
                        card_demo(ctx);
                    });
                });
        });
    });
}
