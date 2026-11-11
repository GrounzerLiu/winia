//! Button demo — the five variants, the five sizes, and the three things that are easy to miss.
//!
//! Material 3 has five button composables that differ only in defaults; winia keeps ONE builder and
//! names them (`Button::filled()` / `::elevated()` / `::filled_tonal()` / `::outlined()` /
//! `::text()`). The cards below take apart what those defaults change, plus the parts of this
//! component that are not obvious from the signature:
//!
//! - the VARIANTS differ in container, content colour, shadow and content padding, not in shape;
//! - the SIZES drive five things at once (height, horizontal AND vertical padding, icon size, icon
//!   gap, outline width). The vertical padding is per size for a reason: each value leaves exactly
//!   that size's icon height in the box, so the container measures its `ContainerHeight`;
//! - the CONTENT is a centred `Row`, and the icon gap is applied by the button — M3 makes the caller
//!   do it. Passing `ButtonDefaults::button_with_icon_content_padding()` is what pulls a leading
//!   icon closer to the edge, as M3's own `ButtonWithIconContentPadding` does.
//!
//! Try: hover the buttons and watch the shadow rise on Filled (0 → 1 dp) and Elevated (1 → 3 dp);
//! press one and watch the container morph from a pill to an 8 dp corner. Turn the switch off to
//! see that a disabled button keeps its shape and its role but loses its clickable.
//!
//! Run with: `cargo run -p winia --example button_demo`.

use letclone::clone;
use winia::prelude::*;

#[path = "common/settings.rs"]
mod settings;

/// Material Symbols `add`.
const ADD_ICON: &str = "M19,13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z";
/// Material Symbols `check`.
const CHECK_ICON: &str = "M9,16.17L4.83,12l-1.42,1.41L9,19L21,7l-1.41,-1.41z";
/// Material Symbols `arrow_forward`.
const FORWARD_ICON: &str = "M12,4l-1.41,1.41L16.17,11H4v2h12.17l-5.58,5.59L12,20l8,-8z";

pub fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(760.0, 1000.0)
                .title("Button demo")
                .build(ctx, |ctx| {
                    settings::shell("Button", ctx, demo_body);
                });
        });
    });
}

#[composable]
fn demo_body(ctx: &mut ComposeCtx, scaffold: ScaffoldContentPadding) {
    let enabled = ctx.remember(|| true);
    let scroll = ctx.remember(ScrollState::new).get();

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
                "One builder, five names. The variants differ in container colour, content colour, \
                 shadow and content padding — the shape is `CornerFull` for all of them, and it \
                 morphs to an 8 dp corner while the button is pressed.",
            )
            .font_size(14.0)
            .build(ctx);

            variants_card(ctx, enabled.clone());
            sizes_card(ctx, enabled.clone());
            content_card(ctx, enabled.clone());
            styling_card(ctx);
        });
}

/// A card with a title, an explanation and a body — the shape every section here uses.
#[composable]
fn card(
    ctx: &mut ComposeCtx,
    title: &'static str,
    body: &'static str,
    content: impl FnOnce(&mut ComposeCtx),
) {
    Card::new().build(ctx, |ctx| {
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

/// The five constructors side by side, plus the enabled switch that governs all of them.
#[composable]
fn variants_card(ctx: &mut ComposeCtx, enabled: State<bool>) {
    card(
        ctx,
        "Five variants",
        "Filled is the high-emphasis one. Elevated adds a ContainerElevation shadow, Tonal fills \
         with SecondaryContainer, Outlined draws a 1 dp OutlineVariant border on nothing, and Text \
         has neither fill nor border — only its content padding differs from the rest.",
        move |ctx| {
            FlowRow::new()
                .main_spacing(12.0)
                .cross_spacing(12.0)
                .modifier(Modifier::new().fill_max_width())
                .build(ctx, |ctx| {
                    for (button, label) in [
                        (Button::filled(), "Filled"),
                        (Button::elevated(), "Elevated"),
                        (Button::filled_tonal(), "Filled tonal"),
                        (Button::outlined(), "Outlined"),
                        (Button::text(), "Text"),
                    ] {
                        let on = enabled.clone();
                        button
                            .enabled(on.get())
                            .on_click(|| {})
                            .build(ctx, |ctx| Text::new(label).build(ctx));
                    }
                });
            Row::new()
                .spacing(12.0)
                .modifier(Modifier::new().fill_max_width())
                .build(ctx, |ctx| {
                    Text::new(if enabled.get() { "enabled" } else { "disabled" })
                        .font_size(13.0)
                        .build(ctx);
                    Switch::new(enabled.get())
                        .on_checked_change({
                            clone!(enabled);
                            move |checked| enabled.set(checked)
                        })
                        .build(ctx, |_| {});
                });
        },
    );
}

/// All five sizes, each labelled with the tokens it carries.
///
/// The heights come from `ButtonSize::container_height()`; the vertical padding beside each one is
/// what keeps that height: `2 * vertical_padding + icon_size == container_height` for every size.
#[composable]
fn sizes_card(ctx: &mut ComposeCtx, enabled: State<bool>) {
    card(
        ctx,
        "Five sizes",
        "`.size(..)` sets height, horizontal padding, vertical padding, icon size, icon gap and \
         outline width together. Naming a size also switches the horizontal padding to that size's \
         own token — 16 dp for Small, where a button with NO size named keeps the baseline 24 dp.",
        move |ctx| {
            FlowRow::new()
                .main_spacing(12.0)
                .cross_spacing(12.0)
                .modifier(Modifier::new().fill_max_width())
                .build(ctx, |ctx| {
                    for size in [
                        ButtonSize::XSmall,
                        ButtonSize::Small,
                        ButtonSize::Medium,
                        ButtonSize::Large,
                        ButtonSize::XLarge,
                    ] {
                        let on = enabled.clone();
                        Button::filled()
                            .size(size)
                            .enabled(on.get())
                            .on_click(|| {})
                            .build(ctx, |ctx| Text::new(format!("{size:?}")).build(ctx));
                    }
                });
            // The numbers, so the sizes are readable as tokens rather than as eyeballed heights.
            for size in [
                ButtonSize::XSmall,
                ButtonSize::Small,
                ButtonSize::Medium,
                ButtonSize::Large,
                ButtonSize::XLarge,
            ] {
                Text::new(format!(
                    "{:<7} height {:.0}  ·  h-pad {:.0}  ·  v-pad {:.0}  ·  icon {:.0}",
                    format!("{size:?}"),
                    size.container_height(),
                    size.horizontal_padding(),
                    size.vertical_padding(),
                    size.icon_size(),
                ))
                .font_size(12.0)
                .build(ctx);
            }
        },
    );
}

/// What the content slot accepts: text, a leading icon, a trailing icon, and the icon paddings.
#[composable]
fn content_card(ctx: &mut ComposeCtx, enabled: State<bool>) {
    card(
        ctx,
        "Content and icons",
        "The content is a centred Row, and the button applies the 8 dp icon gap itself — M3 leaves \
         that to the caller's own Row. A leading icon wants \
         `ButtonDefaults::button_with_icon_content_padding()`, which pulls the icon to 16 dp so it \
         sits optically tight against the edge while the label keeps its 24 dp.",
        move |ctx| {
            FlowRow::new()
                .main_spacing(12.0)
                .cross_spacing(12.0)
                .modifier(Modifier::new().fill_max_width())
                .build(ctx, |ctx| {
                    // Leading icon with the icon-aware padding.
                    Button::filled()
                        .enabled(enabled.get())
                        .content_padding(ButtonDefaults::button_with_icon_content_padding())
                        .on_click(|| {})
                        .build(ctx, |ctx| {
                            Icon::svg_path(ADD_ICON).size(18.0).build(ctx);
                            Text::new("New").build(ctx);
                        });
                    // Trailing icon, default padding.
                    Button::filled_tonal()
                        .enabled(enabled.get())
                        .on_click(|| {})
                        .build(ctx, |ctx| {
                            Text::new("Continue").build(ctx);
                            Icon::svg_path(FORWARD_ICON).size(18.0).build(ctx);
                        });
                    // Text style with an icon wants the other constant (12 / 16).
                    Button::text()
                        .enabled(enabled.get())
                        .content_padding(ButtonDefaults::text_button_with_icon_content_padding())
                        .on_click(|| {})
                        .build(ctx, |ctx| {
                            Icon::svg_path(CHECK_ICON).size(18.0).build(ctx);
                            Text::new("Done").build(ctx);
                        });
                    // Icon alone is fine when the content description carries the meaning.
                    Button::outlined()
                        .enabled(enabled.get())
                        .on_click(|| {})
                        .build(ctx, |ctx| {
                            Icon::svg_path(ADD_ICON)
                                .size(20.0)
                                .content_description("Add")
                                .build(ctx);
                        });
                });
        },
    );
}

/// The appearance parameters: shape, border, colours and elevation are all overridable per button.
#[composable]
fn styling_card(ctx: &mut ComposeCtx) {
    let theme = WiniaTheme::colors();
    card(
        ctx,
        "Overriding the appearance",
        "`shape`, `border`, `colors` and `elevation` are ordinary builder parameters, so a one-off \
         button does not need a styling or a new variant. A caller-supplied fixed-radius shape \
         interpolates straight to the pressed radius instead of going through the pill.",
        move |ctx| {
            FlowRow::new()
                .main_spacing(12.0)
                .cross_spacing(12.0)
                .modifier(Modifier::new().fill_max_width())
                .build(ctx, |ctx| {
                    // A square-ish button: the press morph goes 8 dp → 8 dp, i.e. it does not morph.
                    Button::filled()
                        .shape(Shape::rounded(8.0))
                        .on_click(|| {})
                        .build(ctx, |ctx| Text::new("Rounded 8").build(ctx));
                    // An explicit border over a filled container.
                    Button::filled_tonal()
                        .border(ButtonBorder::new(2.0, theme.primary))
                        .on_click(|| {})
                        .build(ctx, |ctx| Text::new("Bordered").build(ctx));
                    // A custom colour set: `ButtonColors::new(container, content, disabled…)`.
                    Button::filled()
                        .colors(ButtonColors::new(
                            theme.tertiary,
                            theme.on_tertiary,
                            theme.surface_container_highest,
                            theme.on_surface_variant,
                        ))
                        .on_click(|| {})
                        .build(ctx, |ctx| Text::new("Custom colours").build(ctx));
                    // An explicit elevation is honoured as-is, including at rest.
                    Button::filled()
                        .elevation(ButtonElevation::new(4.0, 8.0, 2.0, 8.0, 0.0))
                        .on_click(|| {})
                        .build(ctx, |ctx| Text::new("Lifted").build(ctx));
                });
            Text::new(
                "Nothing here is special-cased: an explicit parameter wins over the style's token, \
                 and a styling (ComponentStyles) sits between the two.",
            )
            .font_size(12.0)
            .color(theme.on_surface_variant)
            .build(ctx);
        },
    );
}
