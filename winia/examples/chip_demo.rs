//! Chip demo — the seven M3 variants, the two colour families behind them, and the four knobs a
//! caller reaches for that the constructor does not show.
//!
//! The page walks the seven variants side by side, then the `selected` state filter and input chips
//! carry, then the slots a chip can hold (leading icon, trailing icon, avatar), then the border, the
//! gap and the colours a caller can override, then the shadow the elevated variants draw, and
//! finally a hoisted interaction source and a disabled row driven by one switch.
//!
//! Try: click a filter chip — `selected` fills its container and drops its outline, which is the
//! one place a chip's border is decided by its state rather than by its variant. Give an input chip
//! an avatar AND a leading icon: only the avatar paints (`Chip.kt:3528`). Toggle the switch at the
//! bottom to see that a disabled chip keeps its selected/unselected distinction — the selected one
//! still fills, at 12 % `onSurface`.
//!
//! Run with: `cargo run -p winia --example chip_demo`.

use letclone::clone;
use winia::components::{Chip, ChipBorder, ChipDefaults, ChipElevation};
use winia::prelude::*;

#[path = "common/settings.rs"]
mod settings;

/// Material Symbols `favorite` (24dp, filled) — the assist chip's leading icon.
const FAVORITE_ICON: &str = "M12,21.35l-1.45-1.32C5.4,15.36,2,12.28,2,8.5C2,5.42,4.42,3,7.5,3c1.74,0,3.41,0.81,4.5,2.09C13.09,3.81,14.76,3,16.5,3C19.58,3,22,5.42,22,8.5c0,3.78-3.4,6.86-8.55,11.54L12,21.35z";
/// Material Symbols `close` (24dp) — an input chip's trailing dismiss icon.
const CLOSE_ICON: &str = "M19,6.41L17.59,5L12,10.59L6.41,5L5,6.41L10.59,12L5,17.59L6.41,19L12,13.41L17.59,19L19,17.59L13.41,12z";
/// Material Symbols `lightbulb` (24dp, filled) — the suggestion chip's icon.
const BULB_ICON: &str = "M9,21c0,0.55,0.45,1,1,1h4c0.55,0,1-0.45,1-1v-1H9v1zM12,2C8.14,2,5,5.14,5,9c0,2.38,1.19,4.47,3,5.74V17c0,0.55,0.45,1,1,1h6c0.55,0,1-0.45,1-1v-2.26c1.81-1.27,3-3.36,3-5.74C19,5.14,15.86,2,12,2z";

pub fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(760.0, 1240.0)
                .title("Chip demo")
                .build(ctx, |ctx| {
                    settings::shell("Chip", ctx, demo_body);
                });
        });
    });
}

#[composable]
fn demo_body(ctx: &mut ComposeCtx, scaffold: ScaffoldContentPadding) {
    let scroll = ctx.remember(ScrollState::new).get();
    // One switch drives every disabled example on the page, so the difference between the variants'
    // disabled containers is visible at the same moment.
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
                "A chip is a compact action or a piece of input. The seven variants differ only in \
                 container and outline — and in whether they carry a `selected` state, which is the \
                 one thing that changes a chip's border from the inside. `assist`, `suggestion` and \
                 `input` take callbacks; the label is a closure, so the text is the caller's.",
            )
            .font_size(14.0)
            .build(ctx);

            variants_card(ctx, disabled.get());
            selection_card(ctx);
            slots_card(ctx);
            overrides_card(ctx);
            elevation_card(ctx);
            interaction_card(ctx);
        });
}

/// The seven variants, side by side.
#[composable]
fn variants_card(ctx: &mut ComposeCtx, disabled: bool) {
    section(
        ctx,
        "Seven variants",
        "`Assist` and `Suggestion` are non-selectable: transparent container, one outline. `Filter` \
         and `Input` add `selected`. The three `Elevated*` variants swap the transparent container \
         for `surfaceContainerLow` and drop the outline by default — which is why they read as raised \
         even before any shadow is looked for.",
        move |ctx| {
            FlowRow::new()
                .main_spacing(12.0)
                .cross_spacing(12.0)
                .build(ctx, |ctx| {
                    Chip::assist(|ctx| Text::new("Assist").build(ctx), || {})
                        .enabled(!disabled)
                        .build(ctx);
                    Chip::elevated_assist(|ctx| Text::new("Elevated assist").build(ctx), || {})
                        .enabled(!disabled)
                        .build(ctx);
                    Chip::filter(false, |ctx| Text::new("Filter").build(ctx), || {})
                        .enabled(!disabled)
                        .build(ctx);
                    Chip::elevated_filter(false, |ctx| Text::new("Elevated filter").build(ctx), || {})
                        .enabled(!disabled)
                        .build(ctx);
                    Chip::input(false, |ctx| Text::new("Input").build(ctx), || {})
                        .enabled(!disabled)
                        .build(ctx);
                    Chip::suggestion(|ctx| Text::new("Suggestion").build(ctx), || {})
                        .enabled(!disabled)
                        .build(ctx);
                    Chip::elevated_suggestion(
                        |ctx| Text::new("Elevated suggestion").build(ctx),
                        || {},
                    )
                    .enabled(!disabled)
                    .build(ctx);
                });
        },
    );
}

/// The `selected` state, which filter and input chips carry.
#[composable]
fn selection_card(ctx: &mut ComposeCtx) {
    // Each chip owns its own flag, so the two rows can disagree — that is the point of a controlled
    // `selected`: the component never stores it.
    let filter_on = ctx.remember(|| true);
    let input_on = ctx.remember(|| false);

    section(
        ctx,
        "Selected fills the container and drops the outline",
        "A chip's border is decided twice: the variant chooses the colour role, and the STATE decides \
         whether there is one at all. A selected chip paints `secondaryContainer` and draws no border; \
         an unselected one is transparent with 1 dp of `outlineVariant`. That is why clicking one looks \
         like it grows rather than lights up.",
        move |ctx| {
            FlowRow::new()
                .main_spacing(12.0)
                .cross_spacing(12.0)
                .build(ctx, |ctx| {
                    Chip::filter(
                        filter_on.get(),
                        |ctx| Text::new("Filter").build(ctx),
                        {
                            clone!(filter_on);
                            move || filter_on.update(|on| *on = !*on)
                        },
                    )
                    .build(ctx);
                    Chip::input(
                        input_on.get(),
                        |ctx| Text::new("Input").build(ctx),
                        {
                            clone!(input_on);
                            move || input_on.update(|on| *on = !*on)
                        },
                    )
                    .build(ctx);
                });
        },
    );
}

/// The three slots a chip can hold.
#[composable]
fn slots_card(ctx: &mut ComposeCtx) {
    section(
        ctx,
        "A leading icon, a trailing icon, and an avatar",
        "The two icons take DIFFERENT colour roles — `Primary` leading, `onSurfaceVariant` trailing — \
         which is why `ChipColors` has a slot for each rather than one `icon_color`. An input chip can \
         also carry a 24 dp avatar with a 12 dp corner (a rounded square, not a circle), and when an \
         avatar and a leading icon are set together only the avatar paints.",
        |ctx| {
            FlowRow::new()
                .main_spacing(12.0)
                .cross_spacing(12.0)
                .build(ctx, |ctx| {
                    Chip::assist(|ctx| Text::new("Leading").build(ctx), || {})
                        .leading_icon(|ctx| {
                            Icon::new(IconSource::svg_path(FAVORITE_ICON)).build(ctx);
                        })
                        .build(ctx);
                    Chip::assist(|ctx| Text::new("Both icons").build(ctx), || {})
                        .leading_icon(|ctx| {
                            Icon::new(IconSource::svg_path(FAVORITE_ICON)).build(ctx);
                        })
                        .trailing_icon(|ctx| {
                            Icon::new(IconSource::svg_path(CLOSE_ICON)).build(ctx);
                        })
                        .build(ctx);
                    Chip::suggestion(|ctx| Text::new("Suggestion").build(ctx), || {})
                        .icon(|ctx| {
                            Icon::new(IconSource::svg_path(BULB_ICON)).build(ctx);
                        })
                        .build(ctx);
                    Chip::input(false, |ctx| Text::new("Avatar, no icon").build(ctx), || {})
                        .avatar(|ctx| {
                            Icon::new(IconSource::svg_path(FAVORITE_ICON)).build(ctx);
                        })
                        .trailing_icon(|ctx| {
                            Icon::new(IconSource::svg_path(CLOSE_ICON)).build(ctx);
                        })
                        .build(ctx);
                    // Both supplied: the avatar wins and the leading icon is not drawn at all.
                    Chip::input(false, |ctx| Text::new("Avatar wins").build(ctx), || {})
                        .avatar(|ctx| {
                            Icon::new(IconSource::svg_path(FAVORITE_ICON)).build(ctx);
                        })
                        .leading_icon(|ctx| {
                            Icon::new(IconSource::svg_path(BULB_ICON)).build(ctx);
                        })
                        .build(ctx);
                });
        },
    );
}

/// The border, the gap and the colours a caller can override.
#[composable]
fn overrides_card(ctx: &mut ComposeCtx) {
    let theme = WiniaTheme::colors();
    // The per-slot form: `None` keeps the theme's value, so a caller names only what they move.
    // That is the whole contract behind `copy(..)` — an all-`None` copy is the identity.
    let warm = ChipDefaults::assist_chip_colors(&theme).copy(
        None,                 // container
        Some(theme.tertiary), // label
        None,                 // leading_icon
        None,                 // trailing_icon
        None,                 // disabled_container
        None,                 // disabled_label
        None,                 // disabled_leading_icon
        None,                 // disabled_trailing_icon
    );
    // The selectable family has its own copy, and one slot of its own to move.
    let selectable = ChipDefaults::selectable_chip_colors(&theme).copy(
        None,
        None,
        None,
        None,
        Some(theme.tertiary_container),
        Some(theme.on_tertiary_container),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    );

    section(
        ctx,
        "Border, gap and colours",
        "`border(..)` overrides whatever the variant resolved — `ChipBorder::none()` is Compose's \
         `border = null`. `horizontal_arrangement(..)` moves the gap between the children without \
         touching the padding. `colors(..)` and `selectable_colors(..)` take a whole set, and both \
         sets have `copy(..)` where `None` means \"keep\".",
        move |ctx| {
            Column::new().spacing(14.0).build(ctx, |ctx| {
                FlowRow::new()
                    .main_spacing(12.0)
                    .cross_spacing(12.0)
                    .build(ctx, |ctx| {
                        // The variant's own 1 dp outline.
                        Chip::assist(|ctx| Text::new("Default outline").build(ctx), || {}).build(ctx);
                        // A caller's own width and colour.
                        Chip::assist(|ctx| Text::new("2 dp tertiary").build(ctx), || {})
                            .border(ChipBorder::new(2.0, theme.tertiary))
                            .build(ctx);
                        // No border at all.
                        Chip::assist(|ctx| Text::new("No border").build(ctx), || {})
                            .border(ChipBorder::none())
                            .build(ctx);
                        // A border where the variant has none to begin with.
                        Chip::elevated_assist(|ctx| Text::new("Added to elevated").build(ctx), || {})
                            .border(ChipBorder::new(1.0, theme.outline))
                            .build(ctx);
                    });

                FlowRow::new()
                    .main_spacing(12.0)
                    .cross_spacing(12.0)
                    .build(ctx, |ctx| {
                        Chip::assist(|ctx| Text::new("Gap 8 (default)").build(ctx), || {})
                            .build(ctx);
                        Chip::assist(|ctx| Text::new("Gap 0").build(ctx), || {})
                            .horizontal_arrangement(0.0)
                            .build(ctx);
                        Chip::assist(|ctx| Text::new("Gap 20").build(ctx), || {})
                            .horizontal_arrangement(20.0)
                            .leading_icon(|ctx| {
                                Icon::new(IconSource::svg_path(FAVORITE_ICON)).build(ctx);
                            })
                            .build(ctx);
                        Chip::assist(|ctx| Text::new("Tertiary label").build(ctx), || {})
                            .colors(warm)
                            .build(ctx);
                        Chip::filter(true, |ctx| Text::new("Tertiary container").build(ctx), || {})
                            .selectable_colors(selectable)
                            .build(ctx);
                    });
            });
        },
    );
}

/// The shadow the elevated variants draw — and the rule that gates it.
#[composable]
fn elevation_card(ctx: &mut ComposeCtx) {
    section(
        ctx,
        "Shadows, and why a flat chip has none",
        "`ChipElevation` is per interaction state: the elevated variants rest at Level1 (1 dp) and \
         rise to Level2 (3 dp) on hover, while a filter chip lifts only once it is FILLED. A shadow \
         behind a transparent container reads as a smudge rather than a lift, so winia draws one only \
         while the container is opaque — which is why hovering an unselected filter chip moves \
         nothing.",
        |ctx| {
            FlowRow::new()
                .main_spacing(12.0)
                .cross_spacing(12.0)
                .build(ctx, |ctx| {
                    Chip::elevated_assist(|ctx| Text::new("Rests at 1 dp").build(ctx), || {})
                        .build(ctx);
                    Chip::elevated_filter(true, |ctx| Text::new("Filled, lifts on hover").build(ctx), || {})
                        .build(ctx);
                    Chip::filter(false, |ctx| Text::new("Unfilled, never lifts").build(ctx), || {})
                        .build(ctx);
                    // A caller's own set: flat everywhere except a deep drag.
                    Chip::assist(|ctx| Text::new("Custom 4 dp").build(ctx), || {})
                        .elevation(ChipElevation::new(4.0, 4.0, 4.0, 6.0, 10.0, 0.0))
                        .build(ctx);
                });
        },
    );
}

/// A hoisted interaction source.
#[composable]
fn interaction_card(ctx: &mut ComposeCtx) {
    let caption = WiniaTheme::colors().on_surface_variant;
    let source = ctx.remember(MutableInteractionSource::new).get();
    let disabled = ctx.remember(|| false);
    // Read inside this function's own scope, so a press re-runs the panel rather than only the
    // component that owns the source.
    let state = source.state(!disabled.get());

    section(
        ctx,
        "A hoisted interaction source, and the disabled state",
        "`interaction_source(..)` hands the chip a source the CALLER keeps, which makes pressed, \
         hovered and focused readable outside it. The switch below disables the whole row: a disabled \
         chip registers no clickable and no ripple, its outline becomes `onSurface` at 12 %, and a \
         disabled SELECTED chip keeps its fill at the same 12 % — the distinction does not collapse.",
        move |ctx| {
            Row::new().spacing(10.0).build(ctx, |ctx| {
                Text::new("disabled").font_size(12.0).build(ctx);
                Switch::new(disabled.get())
                    .on_checked_change({
                        clone!(disabled);
                        move |on| disabled.set(on)
                    })
                    .build(ctx, |_| {});
            });

            Row::new().spacing(14.0).build(ctx, |ctx| {
                Chip::input(false, |ctx| Text::new("Hoisted").build(ctx), || {})
                    .interaction_source(source.clone())
                    .enabled(!disabled.get())
                    .modifier(Modifier::new().semantics(
                        SemanticsConfig::new().content_description("Hoisted source"),
                    ))
                    .build(ctx);
                Text::new(format!(
                    "pressed={} hovered={} focused={} dragged={}",
                    state.pressed, state.hovered, state.focused, state.dragged
                ))
                .font_size(12.0)
                .color(caption)
                .modifier(Modifier::new().test_tag("chip-interaction"))
                .build(ctx);
            });

            FlowRow::new()
                .main_spacing(12.0)
                .cross_spacing(12.0)
                .build(ctx, |ctx| {
                    Chip::assist(|ctx| Text::new("Assist").build(ctx), || {})
                        .enabled(!disabled.get())
                        .build(ctx);
                    Chip::filter(false, |ctx| Text::new("Filter, unselected").build(ctx), || {})
                        .enabled(!disabled.get())
                        .build(ctx);
                    Chip::filter(true, |ctx| Text::new("Filter, selected").build(ctx), || {})
                        .enabled(!disabled.get())
                        .build(ctx);
                    Chip::input(false, |ctx| Text::new("Input + avatar").build(ctx), || {})
                        .avatar(|ctx| {
                            Icon::new(IconSource::svg_path(FAVORITE_ICON)).build(ctx);
                        })
                        .enabled(!disabled.get())
                        .build(ctx);
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
    // Outlined, so the shell does not paint `surfaceContainerHighest` behind a Filled chip — a chip
    // with a transparent container would otherwise sit on a colour that hides its own outline.
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
