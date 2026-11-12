//! Card demo — the three M3 variants, the shape every one of them shares, and the three things a
//! card does that its signature does not say out loud: a clickable card reserves a touch target, a
//! disabled card changes its CONTAINER colour (not just its text), and hovering lifts it.
//!
//! The page walks the variants side by side, then the shape parameter, then the interaction states,
//! then the two overrides a caller actually reaches for (`colors`, `elevation`), and finally the
//! content colour the card hands down to its subtree.
//!
//! Try: hover the cards — Filled and Outlined lift from 0 to 1 dp and an Elevated one from 1 to 3;
//! press and hold — they flatten back and the raised card shows its pressed container. Toggle the
//! switch to disable a row and watch the container change; the Elevated card moves from
//! `surfaceContainerLow` to `surface`, which is a different colour, not the same one dimmed.
//!
//! Run with: `cargo run -p winia --example card_demo`.

use letclone::clone;
use winia::prelude::*;

#[path = "common/settings.rs"]
mod settings;

/// Material Symbols `favorite` (24dp, filled).
const HEART_ICON: &str = "M12,21.35l-1.45-1.32C5.4,15.36,2,12.28,2,8.5C2,5.42,4.42,3,7.5,3c1.74,0,3.41,0.81,4.5,2.09C13.09,3.81,14.76,3,16.5,3C19.58,3,22,5.42,22,8.5c0,3.78-3.4,6.86-8.55,11.54L12,21.35z";
/// Material Symbols `bookmark` (24dp, filled).
const BOOKMARK_ICON: &str = "M17,3H7c-1.1,0-1.99,0.9-1.99,2L5,21l7-3l7,3V5c0-1.1-0.9-2-2-2z";

pub fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(760.0, 1180.0)
                .title("Card demo")
                .build(ctx, |ctx| {
                    settings::shell("Card", ctx, demo_body);
                });
        });
    });
}

#[composable]
fn demo_body(ctx: &mut ComposeCtx, scaffold: ScaffoldContentPadding) {
    let scroll = ctx.remember(ScrollState::new).get();
    // One switch drives every disabled example on the page, so the difference between the three
    // styles' disabled containers is visible at the same moment.
    let disabled = ctx.remember(|| false);

    Column::new()
        .spacing(16.0)
        .modifier(
            Modifier::new()
                .fill_max_size()
                .padding_sides(scaffold.start, scaffold.top, scaffold.end, scaffold.bottom)
                .vertical_scroll(scroll)
                .padding(16.0),
        )
        .build(ctx, |ctx| {
            // No page heading: `settings::shell` already puts this demo's name in the top app bar.
            Text::new(
                "A card is a surface that groups content about one subject. The three variants \
                 differ only in container and shadow — Filled separates itself by a lighter \
                 surface, Elevated by a shadow, Outlined by a border — and all three share the \
                 same 12 dp corner, because material3's `CardTokens.ContainerShape` is \
                 `CornerMedium` for every one of them.",
            )
            .font_size(14.0)
            .build(ctx);

            variants_card(ctx);
            shape_card(ctx);
            interaction_card(ctx, disabled);
            overrides_card(ctx);
            content_colour_card(ctx);
        });
}

/// The three variants side by side — the one decision a caller makes first.
#[composable]
fn variants_card(ctx: &mut ComposeCtx) {
    section(
        ctx,
        "Three variants",
        "Filled sits on `surfaceContainerHighest`, Elevated on `surfaceContainerLow` plus a shadow, \
         Outlined on plain `surface` with a 1 dp `outlineVariant` border. The shadow is what tells \
         Elevated apart from Outlined at rest — without hovering it, the container colours are only \
         a step apart.",
        |ctx| {
            Row::new().spacing(12.0).build(ctx, |row| {
                for style in [CardStyle::Filled, CardStyle::Elevated, CardStyle::Outlined] {
                    // `weight` is a `RowScope` method, not a `Row` one: it hands back the modifier
                    // this child needs, and the scope derefs to the ctx the card builds with.
                    let share = row.weight(1.0);
                    Card::new()
                        .style(style)
                        .modifier(share)
                        .build(row, |ctx| {
                            Column::new()
                                .spacing(6.0)
                                .modifier(Modifier::new().padding(16.0))
                                .build(ctx, |ctx| {
                                    Text::new(variant_name(style)).font_size(15.0).build(ctx);
                                    Text::new(variant_note(style)).font_size(12.0).build(ctx);
                                });
                        });
                }
            });
        },
    );
}

/// The shape parameter, and the fact that its default is a token rather than a number a component
/// picked.
#[composable]
fn shape_card(ctx: &mut ComposeCtx) {
    section(
        ctx,
        "The corner is a token",
        "`CardDefaults::shape()` is `CornerMedium` — 12 dp, from `ShapeTokens.kt`. `shape(..)` \
         replaces it for the container, the border, the shadow AND the focus ring, so one call \
         keeps all four in agreement; passing a stadium here would clip the ripple the same way.",
        |ctx| {
            FlowRow::new()
                .main_spacing(12.0)
                .cross_spacing(12.0)
                .build(ctx, |ctx| {
                    for (label, shape) in [
                        ("default — 12 dp", CardDefaults::shape()),
                        ("8 dp", Shape::rounded(8.0)),
                        ("none", Shape::rounded(0.0)),
                        ("stadium", Shape::pill()),
                    ] {
                        Card::new()
                            .shape(shape)
                            .modifier(Modifier::new().width(150.0))
                            .build(ctx, |ctx| {
                                Text::new(label)
                                    .font_size(12.0)
                                    .modifier(Modifier::new().padding(16.0))
                                    .build(ctx);
                            });
                    }
                });
        },
    );
}

/// A clickable card, a disabled one, and what changes when either flag moves.
#[composable]
fn interaction_card(ctx: &mut ComposeCtx, disabled: State<bool>) {
    section(
        ctx,
        "Clickable, and disabled",
        "A card with `on_click` becomes a button: it takes a 48 dp touch target (material3's \
         `Surface(onClick)` applies `minimumInteractiveComponentSize`), announces `role = Button`, \
         and reports `enabled`. A card without `on_click` declares no semantics at all — it stays \
         transparent in the accessibility tree, as a plain surface should.",
        move |ctx| {
            // The switch is the page's own control, not part of what is being demonstrated.
            Row::new().spacing(10.0).build(ctx, |ctx| {
                Text::new("disabled").font_size(12.0).build(ctx);
                Switch::new(disabled.get())
                    .on_checked_change({
                        clone!(disabled);
                        move |on| disabled.set(on)
                    })
                    .build(ctx, |_| {});
            });

            // A clickable card is wrapped in its own container node for the 48 dp touch target, so
            // the wrapper — not the surface inside it — is what the Row measures. `weight` reaches
            // it because the component splits the caller's chain (`Modifier::split_parent_data`):
            // Compose gets the same effect from parent data, which a container reads back off the
            // `Placeable` however many nodes sit in between.
            Row::new().spacing(12.0).build(ctx, |row| {
                for style in [CardStyle::Filled, CardStyle::Elevated, CardStyle::Outlined] {
                    let share = row.weight(1.0);
                    Card::new()
                        .style(style)
                        .enabled(!disabled.get())
                        .on_click(|| {})
                        .modifier(share)
                        .build(row, |ctx| {
                            Column::new()
                                .spacing(4.0)
                                .modifier(Modifier::new().padding(16.0))
                                .build(ctx, |ctx| {
                                    Text::new(variant_name(style)).font_size(14.0).build(ctx);
                                    Text::new(if disabled.get() {
                                        "disabled"
                                    } else {
                                        "clickable"
                                    })
                                    .font_size(12.0)
                                    .build(ctx);
                                });
                        });
                }
            });

            // A card that takes no clicks: no touch target, no role, no ripple.
            Card::new().build(ctx, |ctx| {
                Column::new()
                    .spacing(4.0)
                    .modifier(Modifier::new().fill_max_width().padding(16.0))
                    .build(ctx, |ctx| {
                        Text::new("No `on_click` — a plain surface")
                            .font_size(14.0)
                            .build(ctx);
                        Text::new(
                            "Its height is exactly its content. The clickable cards above are at \
                             least 48 dp tall whether or not they are enabled, because Compose's \
                             `Surface(onClick)` applies that minimum unconditionally.",
                        )
                        .font_size(12.0)
                        .build(ctx);
                    });
            });
        },
    );
}

/// The two overrides a caller reaches for: a colour set, and an elevation set.
#[composable]
fn overrides_card(ctx: &mut ComposeCtx) {
    let theme = WiniaTheme::colors();
    section(
        ctx,
        "Overriding colours and elevation",
        "`colors(..)` replaces the pair for every state, not just the resting one — the disabled \
         entries are part of the set. `elevation(..)` takes six values (default, pressed, focused, \
         hovered, dragged, disabled) in DP, and the transition between them picks its curve from \
         the PAIR of interactions, so a press that interrupts a hover animates like Compose rather \
         than with one fixed duration.",
        move |ctx| {
            Row::new().spacing(12.0).build(ctx, |row| {
                // A custom container, and a disabled container that is NOT the theme's
                // surfaceVariant@38% — the four values travel together.
                let share = row.weight(1.0);
                Card::new()
                    .colors(CardColors::new(
                        theme.primary_container,
                        theme.on_primary_container,
                        theme.surface_variant,
                        theme.on_surface_variant,
                    ))
                    .on_click(|| {})
                    .modifier(share)
                    .build(row, |ctx| {
                        Column::new()
                            .spacing(4.0)
                            .modifier(Modifier::new().padding(16.0))
                            .build(ctx, |ctx| {
                                Text::new("Custom colours").font_size(14.0).build(ctx);
                                Text::new("primaryContainer").font_size(12.0).build(ctx);
                            });
                    });

                // Flat at rest, high on hover: the whole set is visible in one hover.
                //
                // `on_click` is what makes the OTHER five values reachable, not decoration. A card
                // with no click takes no interactions, so it has no hover/focus/press to report and
                // the animated layer sits at `default` forever — this card said `hovered = 8` and
                // stayed at 0, which is exactly what Compose does: its non-clickable `Card` overload
                // passes `interactionSource = null` to `shadowElevation`, and that returns
                // `remember { mutableStateOf(defaultElevation) }` with no animation
                // (`material3/Card.kt:660-668`). winia reaches the same place from the other side —
                // `Hoverable` is installed by `clickable` (`modifier/builders/input.rs:22-31`), and
                // the hover router only looks at nodes that declare it
                // (`app/per_window/pointer.rs:225-227`).
                let share = row.weight(1.0);
                Card::new()
                    .elevation(CardElevation::new(0.0, 0.0, 0.0, 8.0, 12.0, 0.0))
                    .on_click(|| {})
                    .modifier(share)
                    .build(row, |ctx| {
                        Column::new()
                            .spacing(4.0)
                            .modifier(Modifier::new().padding(16.0))
                            .build(ctx, |ctx| {
                                Text::new("Hover lifts 8 dp").font_size(14.0).build(ctx);
                                Text::new("flat at rest").font_size(12.0).build(ctx);
                            });
                    });
            });
        },
    );
}

/// The content colour a card hands down — why an `Icon` with the default tint follows it.
#[composable]
fn content_colour_card(ctx: &mut ComposeCtx) {
    let theme = WiniaTheme::colors();
    section(
        ctx,
        "Content colour reaches the subtree",
        "The card provides `LocalContentColor` for its content, so an `Icon` with the default \
         `Tint::Auto` takes on the card's own content colour instead of a colour the caller had to \
         state. That is what keeps a label legible when the container is overridden — the pair \
         travels together.",
        move |ctx| {
            Row::new().spacing(12.0).build(ctx, |row| {
                let share = row.weight(1.0);
                Card::new().modifier(share).build(row, |ctx| {
                    Row::new()
                        .spacing(10.0)
                        .modifier(Modifier::new().padding(16.0))
                        .build(ctx, |ctx| {
                            Icon::svg_path(HEART_ICON)
                                .size(20.0)
                                .content_description("Favourite")
                                .build(ctx);
                            Text::new("theme content").font_size(13.0).build(ctx);
                        });
                });

                // The only difference from the card above is the colour set; the icon's tint
                // follows it with no per-icon call.
                let share = row.weight(1.0);
                Card::new()
                    .colors(CardColors::new(
                        theme.primary,
                        theme.on_primary,
                        theme.primary,
                        theme.on_primary,
                    ))
                    .modifier(share)
                    .build(row, |ctx| {
                        Row::new()
                            .spacing(10.0)
                            .modifier(Modifier::new().padding(16.0))
                            .build(ctx, |ctx| {
                                Icon::svg_path(BOOKMARK_ICON)
                                    .size(20.0)
                                    .content_description("Bookmark")
                                    .build(ctx);
                                Text::new("onPrimary").font_size(13.0).build(ctx);
                            });
                    });
            });
        },
    );
}

/// The page's card shell: title, explanation, then the body.
#[composable]
fn section(
    ctx: &mut ComposeCtx,
    title: &'static str,
    body: &'static str,
    content: impl FnOnce(&mut ComposeCtx),
) {
    // Outlined, not the default Filled — the shell is the page's chrome, and Filled paints
    // `surfaceContainerHighest` behind every section, which is exactly the colour a Filled card
    // inside that section paints. The example then disappears into its own frame.
    //
    // `surface` is the right backdrop because it is the LOWEST of the three containers: a Filled
    // card (a lighter role) stands off it by colour and an Elevated one by its shadow. Outlined's
    // own container colour IS `surface` (`OutlinedCardTokens.ContainerColor`), so on this shell the
    // border is the only thing you see — which is what an outlined container looks like with
    // nothing behind it, rather than a translucent box.
    Card::new().style(CardStyle::Outlined).build(ctx, |ctx| {
        Column::new()
            .spacing(12.0)
            .modifier(Modifier::new().fill_max_width().padding(16.0))
            .build(ctx, |ctx| {
                Text::new(title).font_size(18.0).build(ctx);
                Text::new(body).font_size(14.0).build(ctx);
                content(ctx);
            });
    });
}

fn variant_name(style: CardStyle) -> &'static str {
    match style {
        CardStyle::Filled => "Filled",
        CardStyle::Elevated => "Elevated",
        CardStyle::Outlined => "Outlined",
    }
}

fn variant_note(style: CardStyle) -> &'static str {
    match style {
        CardStyle::Filled => "surfaceContainerHighest",
        CardStyle::Elevated => "surfaceContainerLow",
        CardStyle::Outlined => "surface + 1 dp border",
    }
}
