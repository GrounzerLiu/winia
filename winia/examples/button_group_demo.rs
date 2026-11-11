//! Button group demo — the two forms, the press growth, weights, the overflow menu, and the two
//! parameters that decide where an item sits.
//!
//! `ButtonGroup` is material3's M3 Expressive group: a row of buttons that share their state and,
//! in the CONNECTED form, their container. The cards below take apart what the two forms change
//! and the parts that are not obvious from the signature:
//!
//! - the PLAIN form spaces items by 12 dp and each keeps its own pill container; the CONNECTED form
//!   spaces them by 2 dp and the CONTAINER is shared, which only reads as one shape if every item
//!   paints the corner pair its position calls for. `ButtonGroupDefaults::item_shapes(i, count,
//!   direction)` hands exactly that, already resolved for the layout direction;
//! - a CHECKED item leaves that shared container: its shape becomes a stadium wherever it sits, so
//!   a selection pops out of the row;
//! - pressing an item GROWS it by `expandedRatio` (0.15 by default) and its neighbours give up the
//!   room — `Modifier.animateWidth` upstream, the item's `press_source` here. The growth is not
//!   opt-in: what IS required is that the item reports its presses through the source the group
//!   hands it, which is why every item below passes it to `Button::interaction_source`;
//! - the OVERFLOW indicator appears only when the row cannot fit its items. `desiredWidth` is
//!   tested BEFORE anything is reserved, so a group with room to spare shows no "more" button at
//!   all — and when it does appear, the items that did not fit are in its menu.
//!
//! Try: press and hold an item and watch it widen while its neighbours narrow; drag the width
//! slider in the overflow card until the indicator appears, then click it; switch the layout
//! direction in the settings sheet — the connected corners AND the positions mirror together.
//!
//! Run with: `cargo run -p winia --example button_group_demo`.

use letclone::clone;
use winia::prelude::*;

#[path = "common/settings.rs"]
mod settings;

/// Material Symbols `calendar_today`.
const CALENDAR_ICON: &str = "M20,3h-1V1h-2v2H7V1H5v2H4c-1.1,0-2,0.9-2,2v16c0,1.1,0.9,2,2,2h16c1.1,0,2-0.9,2-2V5c0-1.1-0.9-2-2-2zM20,21H4V8h16v13z";
/// Material Symbols `event`.
const EVENT_ICON: &str = "M17,12h-5v5h5v-5zM16,1v2H8V1H6v2H5c-1.11,0-1.99,0.9-1.99,2L3,19c0,1.1,0.89,2,2,2h14c1.1,0,2-0.9,2-2V5c0-1.1-0.9-2-2-2h-1V1h-2zM19,19H5V8h14v11z";

/// The labels the connected cards share, so the corner sets and the labels cannot drift apart.
const CONNECTED_LABELS: [&str; 3] = ["Day", "Week", "Month"];

pub fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(760.0, 1120.0)
                .title("Button group demo")
                .build(ctx, |ctx| {
                    settings::shell("Button group", ctx, demo_body);
                });
        });
    });
}

#[composable]
fn demo_body(ctx: &mut ComposeCtx, scaffold: ScaffoldContentPadding) {
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
                "A button group is a row of buttons that share their state — and, in the connected \
                 form, their container. The row is as tall as its tallest item, spaces its items \
                 by the form's token, and never takes a height of its own.",
            )
            .font_size(14.0)
            .build(ctx);

            forms_card(ctx);
            press_growth_card(ctx);
            weights_card(ctx);
            overflow_card(ctx);
            placement_card(ctx);
            tokens_card(ctx);
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

/// A caption above a row — the demos use these instead of a second heading level.
#[composable]
fn caption(ctx: &mut ComposeCtx, text: &'static str) {
    Text::new(text)
        .font_size(12.0)
        .color(WiniaTheme::colors().on_surface_variant)
        .build(ctx);
}

/// The plain form, the connected form, and the connected form with a selection.
#[composable]
fn forms_card(ctx: &mut ComposeCtx) {
    card(
        ctx,
        "Two forms",
        "Plain spaces its items by 12 dp and gives each its own pill container. Connected spaces \
         them by 2 dp so the containers read as ONE shape — which only works if each item paints \
         the corner pair its position calls for, so the caller hands every item the set \
         `item_shapes(index, count, direction)` picks for it. A CHECKED item then leaves that \
         shared container entirely: it becomes a stadium wherever it sits.",
        |ctx| {
            caption(ctx, "Plain — 12 dp between items, one container each");
            ButtonGroup::new()
                .add(ButtonGroupItem::clickable("Day", || {}))
                .add(ButtonGroupItem::clickable("Week", || {}))
                .add(ButtonGroupItem::clickable("Month", || {}))
                .build(ctx);

            caption(ctx, "Connected — 2 dp between items, one shared container");
            connected_row(ctx);

            caption(
                ctx,
                "Connected with a selection — the checked item pops out",
            );
            connected_selectable_row(ctx);
        },
    );
}

/// The three corner sets a connected row needs, applied by position.
///
/// The shape is a value the item has to choose ITSELF: `Button` takes one `Shape`, so the pressed
/// variant is picked by reading the item's own interaction state — reading it in composition
/// registers the dependency, so a press recomposes the item and it switches to
/// `pressed_shape` (inner corners 8 -> 4). That switch is a JUMP, not a morph, and it cannot be
/// anything else here: `Button` only interpolates `Pill`/`Circle` and `RoundedRect`, because a
/// per-corner shape has no single radius to travel (`button.rs`: "Per-corner, top-only and
/// rectangle shapes have nothing to interpolate"). The rows that matter visually — the item growing
/// and its neighbours narrowing — still animate, because the WIDTH comes from the group's own
/// spring, not from this shape.
#[composable]
fn connected_row(ctx: &mut ComposeCtx) {
    let direction = winia::layout::direction::current();
    let mut group = ButtonGroup::connected();
    for index in 0..CONNECTED_LABELS.len() {
        let shapes = ButtonGroupDefaults::item_shapes(index, CONNECTED_LABELS.len(), direction);
        group = group.add(ButtonGroupItem::new(move |ctx, source| {
            let pressed = source.state(true).pressed;
            Button::filled()
                .shape(shapes.shape_for(pressed, false))
                .interaction_source(source)
                .on_click(|| {})
                .build(ctx, |ctx| {
                    Text::new(CONNECTED_LABELS[index])
                        .max_lines(1)
                        .soft_wrap(false)
                        .build(ctx);
                });
        }));
    }
    group.build(ctx);
}

/// The same row as a single-choice selection: the checked item takes `checked_shape` (a stadium)
/// and the `primary` container, so it reads as lifted out of the shared container.
#[composable]
fn connected_selectable_row(ctx: &mut ComposeCtx) {
    let selected = ctx.remember(|| 0usize);
    let direction = winia::layout::direction::current();
    let mut group = ButtonGroup::connected();
    for index in 0..CONNECTED_LABELS.len() {
        let shapes = ButtonGroupDefaults::item_shapes(index, CONNECTED_LABELS.len(), direction);
        let picked = selected.clone();
        group = group.add(ButtonGroupItem::new(move |ctx, source| {
            let checked = picked.get() == index;
            let pressed = source.state(true).pressed;
            Button::filled()
                .shape(shapes.shape_for(pressed, checked))
                .colors(toggle_colors(&WiniaTheme::colors(), checked))
                .interaction_source(source)
                .on_click({
                    clone!(picked);
                    move || picked.set(index)
                })
                .build(ctx, |ctx| {
                    Text::new(CONNECTED_LABELS[index])
                        .max_lines(1)
                        .soft_wrap(false)
                        .build(ctx);
                });
        }));
    }
    group.build(ctx);
}

/// `ToggleButton`'s two containers: `SurfaceContainer` unchecked, `Primary` checked
/// (`FilledButtonTokens.UnselectedContainerColor` / `SelectedContainerColor`).
fn toggle_colors(theme: &winia::theme::ThemeColors, checked: bool) -> ButtonColors {
    let alpha = |c: Color, a: f32| Color::from_argb((c.a as f32 * a) as u8, c.r, c.g, c.b);
    let (container, content) = if checked {
        (theme.primary, theme.on_primary)
    } else {
        (theme.surface_container, theme.on_surface)
    };
    ButtonColors::new(
        container,
        content,
        alpha(theme.on_surface, 0.12),
        alpha(theme.on_surface, 0.38),
    )
}

/// A plain filled button that reports its presses through the group's source.
fn grouped_button(ctx: &mut ComposeCtx, source: MutableInteractionSource, label: &'static str) {
    Button::filled()
        .interaction_source(source)
        .on_click(|| {})
        .build(ctx, |ctx| {
            Text::new(label).max_lines(1).soft_wrap(false).build(ctx);
        });
}

/// The press growth, with `expandedRatio` on a slider.
///
/// The ratio is a fraction of the pressed item's OWN width: the item grows by
/// `ratio * width` (halved for a middle item, which takes the room off both neighbours), and each
/// neighbour caps what it gives up at its `compressionLimit` — 24 dp, the button's end padding.
#[composable]
fn press_growth_card(ctx: &mut ComposeCtx) {
    let ratio = ctx.remember(|| BUTTON_GROUP_EXPANDED_RATIO);
    card(
        ctx,
        "Press to grow",
        "Every item grows while it is pressed and takes the room off its neighbours, so the row's \
         total width never moves. The growth is `expandedRatio` of the item's own width: 0.15 by \
         default, 0 turns it off, and a middle item halves it because it squeezes BOTH sides. Each \
         neighbour gives up at most its `compressionLimit` (24 dp).",
        move |ctx| {
            Text::new(format!("expandedRatio = {:.2}", ratio.get()))
                .font_size(12.0)
                .build(ctx);
            Slider::new(ratio.get())
                .value_range(0.0, 0.5)
                .on_value_change({
                    clone!(ratio);
                    move |value| ratio.set(value)
                })
                .build(ctx);
            ButtonGroup::new()
                .expanded_ratio(ratio.get())
                .item(|ctx, source| grouped_button(ctx, source, "One"))
                .item(|ctx, source| grouped_button(ctx, source, "Two"))
                .item(|ctx, source| grouped_button(ctx, source, "Three"))
                .build(ctx);
        },
    );
}

/// Weighted items: the space left after the natural-width items is divided by weight.
#[composable]
fn weights_card(ctx: &mut ComposeCtx) {
    card(
        ctx,
        "Weights",
        "An item can take a proportional share of the space left after the unweighted items have \
         taken theirs — upstream's `Modifier.weight`, which winia spells `weighted_item`. The row \
         is given a fixed width here so the shares are visible; a weighted item is measured at a \
         FIXED width, which is what makes it fill its share instead of shrinking to its label.",
        |ctx| {
            ButtonGroup::new()
                .modifier(Modifier::new().width(440.0))
                .item(|ctx, source| grouped_button(ctx, source, "Natural"))
                .weighted_item(1.0, |ctx, source| grouped_button(ctx, source, "1x"))
                .weighted_item(2.0, |ctx, source| grouped_button(ctx, source, "2x"))
                .build(ctx);
        },
    );
}

/// The overflow indicator, driven by a width slider so the threshold is observable.
#[composable]
fn overflow_card(ctx: &mut ComposeCtx) {
    let width = ctx.remember(|| 420.0f32);
    card(
        ctx,
        "The overflow indicator",
        "The indicator is measured only AFTER the row has failed to fit: `desiredWidth` is tested \
         against the available width first, so a group with room to spare shows no \"more\" button \
         at all. Once it appears it takes its width off the row, the items fill what is left \
         greedily, and the ones that did not fit are composed into its menu — `clickable` and \
         `toggleable` items build that row themselves.",
        move |ctx| {
            Text::new(format!("row width = {:.0} dp", width.get()))
                .font_size(12.0)
                .build(ctx);
            Slider::new(width.get())
                .value_range(220.0, 640.0)
                .on_value_change({
                    clone!(width);
                    move |value| width.set(value)
                })
                .build(ctx);
            ButtonGroup::new()
                .modifier(Modifier::new().width(width.get()))
                .add(ButtonGroupItem::clickable("Day", || {}).icon(|ctx| {
                    Icon::svg_path(CALENDAR_ICON).size(18.0).build(ctx);
                }))
                .add(ButtonGroupItem::clickable("Week", || {}))
                .add(ButtonGroupItem::clickable("Month", || {}).icon(|ctx| {
                    Icon::svg_path(EVENT_ICON).size(18.0).build(ctx);
                }))
                .add(ButtonGroupItem::clickable("Quarter", || {}))
                .add(ButtonGroupItem::clickable("Year", || {}))
                // Upstream's own default indicator: a filled MoreVert circle in a tooltip whose
                // click toggles the menu.
                .overflow_indicator(ButtonGroupDefaults::overflow_indicator)
                .build(ctx);
        },
    );
}

/// Where an item sits: `verticalAlignment` on the cross axis, `arrangement` on the main one.
#[composable]
fn placement_card(ctx: &mut ComposeCtx) {
    card(
        ctx,
        "Where an item sits",
        "`verticalAlignment` places an item SHORTER than the row on the cross axis (Top by \
         default), and an item's own `align` overrides it. `arrangement` distributes the main-axis \
         room no item claims — the row is given a fixed width below so there is room to \
         distribute.",
        |ctx| {
            caption(ctx, "verticalAlignment = Center, then an item-level align");
            ButtonGroup::new()
                .vertical_alignment(Alignment::Center)
                .add(ButtonGroupItem::new(|ctx, source| {
                    sized_button(ctx, source, ButtonSize::Medium, "56 dp")
                }))
                .add(ButtonGroupItem::new(|ctx, source| {
                    sized_button(ctx, source, ButtonSize::Small, "40 dp")
                }))
                .add(
                    ButtonGroupItem::new(|ctx, source| {
                        sized_button(ctx, source, ButtonSize::Small, "align End")
                    })
                    .align(Alignment::End),
                )
                .build(ctx);

            for arrangement in [
                Arrangement::Start,
                Arrangement::Center,
                Arrangement::End,
                Arrangement::SpaceBetween,
            ] {
                caption(ctx, arrangement_label(arrangement));
                ButtonGroup::new()
                    .modifier(Modifier::new().width(360.0))
                    .arrangement(arrangement)
                    .add(ButtonGroupItem::clickable("One", || {}))
                    .add(ButtonGroupItem::clickable("Two", || {}))
                    .build(ctx);
            }
        },
    );
}

/// A button whose height is the point: `ButtonSize` sets the container height.
fn sized_button(
    ctx: &mut ComposeCtx,
    source: MutableInteractionSource,
    size: ButtonSize,
    label: &'static str,
) {
    Button::filled()
        .size(size)
        .interaction_source(source)
        .on_click(|| {})
        .build(ctx, |ctx| {
            Text::new(label).max_lines(1).soft_wrap(false).build(ctx);
        });
}

fn arrangement_label(arrangement: Arrangement) -> &'static str {
    match arrangement {
        Arrangement::Start => "Arrangement::Start — packed to the leading edge",
        Arrangement::Center => "Arrangement::Center",
        Arrangement::End => "Arrangement::End — packed to the trailing edge",
        Arrangement::SpaceBetween => "Arrangement::SpaceBetween — the leftover becomes the gaps",
        _ => "other arrangement",
    }
}

/// The tokens the row reads, so they are readable as numbers rather than as eyeballed spacing.
#[composable]
fn tokens_card(ctx: &mut ComposeCtx) {
    card(
        ctx,
        "The tokens",
        "From `ButtonGroupSmallTokens` and `ConnectedButtonGroupSmallTokens`. `ContainerHeight` is \
         deliberately absent: it is a BUTTON size, and the group never applies it — the row takes \
         its tallest item's height instead, which is what lets a Medium (56 dp) button sit in one.",
        |ctx| {
            for line in [
                format!("between-space, plain        {BUTTON_GROUP_BETWEEN_SPACE:.0} dp"),
                format!("between-space, connected   {CONNECTED_BUTTON_GROUP_BETWEEN_SPACE:.0} dp"),
                format!("expanded ratio             {BUTTON_GROUP_EXPANDED_RATIO:.2}"),
                format!("compression limit          {BUTTON_GROUP_COMPRESSION_LIMIT:.0} dp"),
                format!("inner corner               {CONNECTED_INNER_CORNER_SIZE:.0} dp"),
                format!("pressed inner corner       {CONNECTED_PRESSED_INNER_CORNER_SIZE:.0} dp"),
                format!(
                    "cornerFull on a 40 dp item {:.0} dp",
                    BUTTON_GROUP_HEIGHT / 2.0
                ),
            ] {
                Text::new(line).font_size(12.0).build(ctx);
            }
        },
    );
}
