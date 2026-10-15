//! ButtonGroup demo — the plain row, the connected row, the selected item, the overflow and
//! weighted items.
//!
//! Every group here is laid out from the token numbers the component carries: 12 dp between the
//! items of a plain group, 2 dp between the items of a connected one (which also share their
//! container's corners), 40 dp tall either way.

use letclone::clone;
use winia::prelude::*;
// Shared example chrome: top app bar with the settings sheet (theme mode + layout direction).
#[path = "common/settings.rs"]
mod settings;

/// Material Icons "add" — icon paths are data, and data gets looked up.
const PLUS_PATH: &str = "M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z";
/// Material Icons "more_vert" — the VERTICAL ellipsis, which is what upstream's default overflow
/// indicator shows (`Icons.Filled.MoreVert`, `ButtonGroup.kt:333`).
const MORE_VERT_PATH: &str = "M12 8c1.1 0 2-.9 2-2s-.9-2-2-2-2 .9-2 2 .9 2 2 2zm0 2c-1.1 0-2 .9-2 2s.9 2 2 2 2-.9 2-2-.9-2-2-2zm0 6c-1.1 0-2 .9-2 2s.9 2 2 2 2-.9 2-2-.9-2-2-2z";
/// Material Icons "check".
const CHECK_PATH: &str = "M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z";
/// Material Icons "favorite".
const FAVORITE_PATH: &str = "M12 21.35l-1.45-1.32C5.4 15.36 2 12.28 2 8.5 2 5.42 4.42 3 7.5 3c1.74 0 3.41.81 4.5 2.09C13.09 3.81 14.76 3 16.5 3 19.58 3 22 5.42 22 8.5c0 3.78-3.4 6.86-8.55 11.54L12 21.35z";
/// Material Icons "home".
const HOME_PATH: &str = "M10 20v-6h4v6h5v-9h3L12 3 2 11h3v9z";
/// Material Icons "edit".
const EDIT_PATH: &str = "M3 17.25V21h3.75L17.81 9.94l-3.75-3.75L3 17.25zM20.71 7.04a.996.996 0 0 0 0-1.41l-2.34-2.34a.996.996 0 0 0-1.41 0l-1.83 1.83 3.75 3.75 1.83-1.83z";
/// Material Icons "search".
const SEARCH_PATH: &str = "M15.5 14h-.79l-.28-.27C15.41 12.59 16 11.11 16 9.5 16 5.91 13.09 3 9.5 3S3 5.91 3 9.5 5.91 16 9.5 16c1.61 0 3.09-.59 4.23-1.57l.27.28v.79l5 4.99L20.49 19l-4.99-5zm-6 0C7.01 14 5 11.99 5 9.5S7.01 5 9.5 5 14 7.01 14 9.5 11.99 14 9.5 14z";

/// How wide an overflow menu row is. winia's menu sizes itself to its content, and a bare
/// `Button::text()` comes out as a tight pill — the row is given a width instead.
const BUTTON_GROUP_MENU_WIDTH: f32 = 160.0;

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
        .font_size(12.0)
        .color(label_color())
        .modifier(Modifier::new().padding_top(6.0).padding_bottom(2.0))
        .build(ctx);
}

/// One item of a group: a filled button painting whatever shape its position gives it.
/// One item of a group: a button painting whatever shape its position gives it, and reporting its
/// presses through the source the group hands it — which is what makes it grow while pressed and
/// squeeze its neighbours. Upstream attaches that to EVERY item its own constructors build
/// (`clickableItem` and `toggleableItem`, `ButtonGroup.kt:1075` and `:1146`), and so does this:
/// the STYLE is not part of the rule, which is why the outlined section below grows too.
fn styled_item(
    label: &'static str,
    shape: Shape,
    style: ButtonStyle,
    on_click: impl Fn() + Send + Sync + 'static,
) -> impl FnOnce(&mut ComposeCtx, MutableInteractionSource) + 'static {
    move |ctx, source| {
        Button::new()
            .style(style)
            .shape(shape)
            .interaction_source(source)
            .on_click(on_click)
            .build(ctx, |ctx| {
                // ONE line, never wrapped — upstream's item does the same
                // (`ButtonGroup.kt:1118-1124`: `maxLines = 1, softWrap = false`). A wrapped label
                // makes the item taller than the 40 dp the group lays out, and while an item is
                // pressed its neighbours narrow, so a wrapping label would reflow mid-press.
                // `Button` does not force this, in material3 or here — it is the ITEM's rule.
                Text::new(label)
                    .max_lines(1)
                    .soft_wrap(false)
                    .overflow(TextOverflow::Visible)
                    .align(TextAlign::Center)
                    .build(ctx);
            });
    }
}

fn item(
    label: &'static str,
    shape: Shape,
    on_click: impl Fn() + Send + Sync + 'static,
) -> impl FnOnce(&mut ComposeCtx, MutableInteractionSource) + 'static {
    styled_item(label, shape, ButtonStyle::Filled, on_click)
}

/// One item of a group carrying an ICON and nothing else. An `IconButton`, not a `Button`:
/// upstream's icon-only items are `FilledIconButton`s, and the icon button's own size and shape
/// are what make the row read as a row of circles.
fn icon_item(
    path: &'static str,
    on_click: impl Fn() + Send + Sync + 'static,
) -> impl FnOnce(&mut ComposeCtx, MutableInteractionSource) + 'static {
    move |ctx, source| {
        IconButton::new()
            .style(IconButtonStyle::Filled)
            .interaction_source(source)
            .on_click(on_click)
            .build(ctx, |ctx| {
                Icon::svg_path(path).size(24.0).build(ctx);
            });
    }
}

#[composable]
fn button_group_demo(ctx: &mut ComposeCtx) {
    let scroll = ctx.remember(|| ScrollState::new()).get();
    let plain = ctx.remember(|| 0usize);
    let connected = ctx.remember(|| 0usize);
    let direction = winia::layout::direction::current();

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0).vertical_scroll(scroll))
        .build(ctx, |ctx| {
            // The item closures below are `'static`, so they need OWNED handles rather than a
            // borrow of the outer ones.
            let plain = plain.clone();
            let connected = connected.clone();
            section_title(ctx, "1. Plain group — ButtonGroupSmallTokens.BetweenSpace, 12 dp");
            ButtonGroup::new()
                .item(item("Start", Shape::Pill, { clone!(plain); move || plain.set(1) }))
                .item(item("Middle", Shape::Pill, { clone!(plain); move || plain.set(2) }))
                .item(item("End", Shape::Pill, { clone!(plain); move || plain.set(3) }))
                .build(ctx);
            caption(
                ctx,
                "The items take their natural width and sit 12 dp apart. Nothing is shared between \
                 them — each one is its own pill, which is what the plain form looks like.",
            );

            section_title(ctx, "2. Connected group — one container, 2 dp between the items");
            ButtonGroup::connected()
                .item(item(
                    "Start",
                    ButtonGroupDefaults::item_shapes(0, 3, direction).shape,
                    { clone!(plain); move || plain.set(1) },
                ))
                .item(item(
                    "Middle",
                    ButtonGroupDefaults::item_shapes(1, 3, direction).shape,
                    { clone!(plain); move || plain.set(2) },
                ))
                .item(item(
                    "End",
                    ButtonGroupDefaults::item_shapes(2, 3, direction).shape,
                    { clone!(plain); move || plain.set(3) },
                ))
                .build(ctx);
            caption(
                ctx,
                "2 dp apart, so the items read as ONE container: the leading item rounds its START \
                 corners to CornerFull (20 on a 40 dp item) and squares its end pair to 8, and the \
                 trailing item mirrors that. The middle one is 8 all round. Switch the settings \
                 sheet's direction and the rounded pair follows — the corners are resolved \
                 against the direction, not baked in.",
            );

            section_title(ctx, "3. A selected item leaves the container — connectedButtonCheckedShape");
            let shapes = |i: usize| ButtonGroupDefaults::item_shapes(i, 3, direction);
            ButtonGroup::connected()
                .item(item(
                    "Day",
                    shapes(0).shape_for(false, connected.get() == 0),
                    { clone!(connected); move || connected.set(0) },
                ))
                .item(item(
                    "Week",
                    shapes(1).shape_for(false, connected.get() == 1),
                    { clone!(connected); move || connected.set(1) },
                ))
                .item(item(
                    "Month",
                    shapes(2).shape_for(false, connected.get() == 2),
                    { clone!(connected); move || connected.set(2) },
                ))
                .build(ctx);
            caption(
                ctx,
                "The checked shape is CornerFull whatever the position, so the selected item pops \
                 out as a stadium while its neighbours keep the shared container's corners.",
            );

            section_title(ctx, "4. Overflow — the indicator takes the room, the rest go in the menu");
            let too_narrow = ctx.remember(|| 0usize);
            ButtonGroup::new()
                .modifier(Modifier::new().width(360.0))
                .add(
                    ButtonGroupItem::new(item("One", Shape::Pill, { clone!(too_narrow); move || too_narrow.set(1) }))
                        .menu(|ctx, state| {
                            Button::text()
                                .modifier(Modifier::new().width(BUTTON_GROUP_MENU_WIDTH))
                                .on_click({ clone!(state); move || state.dismiss() })
                                .build(ctx, |ctx| Text::new("One").build(ctx));
                        }),
                )
                .add(
                    ButtonGroupItem::new(item("Two", Shape::Pill, { clone!(too_narrow); move || too_narrow.set(2) }))
                        .menu(|ctx, state| {
                            Button::text()
                                .modifier(Modifier::new().width(BUTTON_GROUP_MENU_WIDTH))
                                .on_click({ clone!(state); move || state.dismiss() })
                                .build(ctx, |ctx| Text::new("Two").build(ctx));
                        }),
                )
                .add(
                    ButtonGroupItem::new(item("Three", Shape::Pill, { clone!(too_narrow); move || too_narrow.set(3) }))
                        .menu(|ctx, state| {
                            Button::text()
                                .modifier(Modifier::new().width(BUTTON_GROUP_MENU_WIDTH))
                                .on_click({ clone!(state); move || state.dismiss() })
                                .build(ctx, |ctx| Text::new("Three").build(ctx));
                        }),
                )
                .add(
                    ButtonGroupItem::new(item("Four", Shape::Pill, { clone!(too_narrow); move || too_narrow.set(4) }))
                        .menu(|ctx, state| {
                            Button::text()
                                .modifier(Modifier::new().width(BUTTON_GROUP_MENU_WIDTH))
                                .on_click({ clone!(state); move || state.dismiss() })
                                .build(ctx, |ctx| Text::new("Four").build(ctx));
                        }),
                )
                // Upstream's default indicator is a `FilledIconButton` with `Icons.Filled.MoreVert`
                // and a click that TOGGLES the menu (`ButtonGroup.kt:296-336`) — a circle, not a
                // pill, and a vertical ellipsis, not a horizontal one.
                // Upstream's default indicator is a `FilledIconButton` with `Icons.Filled.MoreVert`
                // and a click that TOGGLES the menu (`ButtonGroup.kt:296-336`) — a circle, not a
                // pill, and a vertical ellipsis, not a horizontal one. It also sits in a
                // `TooltipBox` reading `Strings.ButtonGroupMoreOptions`, which is `Tooltip` here
                // (hover-triggered, anchored above, 8 dp away — the same defaults).
                .overflow_indicator(|ctx, state| {
                    Tooltip::new("More options").build(ctx, |ctx| {
                        IconButton::new()
                            .style(IconButtonStyle::Filled)
                            .on_click(move || {
                                if state.is_showing() {
                                    state.dismiss();
                                } else {
                                    state.show();
                                }
                            })
                            .build(ctx, |ctx| {
                                Icon::svg_path(MORE_VERT_PATH).size(24.0).build(ctx);
                            });
                    });
                })
                .build(ctx);
            caption(
                ctx,
                "The row is pinned to 360 dp, so four labels do not fit. The indicator is measured \
                 FIRST — that is what leaves room for the items — then the items fill what is left \
                 until one does not fit: it is measured at nothing and moves into the menu, which \
                 opens on the indicator.",
            );

            section_title(ctx, "5. Weighted items — the leftover, divided by the total weight");
            ButtonGroup::new()
                .modifier(Modifier::new().width(460.0))
                .item(item("Fixed", Shape::Pill, { clone!(plain); move || plain.set(1) }))
                .weighted_item(1.0, item("1 share", Shape::Pill, { clone!(plain); move || plain.set(2) }))
                .weighted_item(2.0, item("2 shares", Shape::Pill, { clone!(plain); move || plain.set(3) }))
                .build(ctx);
            caption(
                ctx,
                "'Fixed' takes its natural width first; the 360 dp that is left is split 1:2 between \
                 the other two, and each is measured at a FIXED width so it fills its share rather \
                 than shrinking to its label.",
            );

            section_title(ctx, "6. A connected group with an icon on one item");
            ButtonGroup::connected()
                .item(item("Add", Shape::Pill, { clone!(plain); move || plain.set(1) }))
                .item({
                    let plain = plain.clone();
                    move |ctx, source| {
                        Button::new()
                            .style(ButtonStyle::Filled)
                            .shape(ButtonGroupDefaults::connected_middle_shapes().shape)
                            .interaction_source(source)
                            .on_click(move || plain.set(2))
                            .build(ctx, |ctx| {
                                Icon::svg_path(PLUS_PATH).size(24.0).build(ctx);
                            });
                    }
                })
                .item(item(
                    "Check",
                    ButtonGroupDefaults::connected_trailing_shapes(direction).shape,
                    { clone!(plain); move || plain.set(3) },
                ))
                .build(ctx);
            caption(
                ctx,
                "An item can be anything — the group only lays them out and spaces them, which is \
                 upstream's customItem.",
            );

            section_title(ctx, "7. Outlined items grow too — the growth belongs to the ITEM");
            ButtonGroup::connected()
                .item(styled_item(
                    "Cut",
                    ButtonGroupDefaults::item_shapes(0, 3, direction).shape,
                    ButtonStyle::Outlined,
                    { clone!(plain); move || plain.set(1) },
                ))
                .item(styled_item(
                    "Copy",
                    ButtonGroupDefaults::item_shapes(1, 3, direction).shape,
                    ButtonStyle::Outlined,
                    { clone!(plain); move || plain.set(2) },
                ))
                .item(styled_item(
                    "Paste",
                    ButtonGroupDefaults::item_shapes(2, 3, direction).shape,
                    ButtonStyle::Outlined,
                    { clone!(plain); move || plain.set(3) },
                ))
                .build(ctx);
            caption(
                ctx,
                "The press growth is not a property of the group or of a style — it belongs to the \
                 ITEM, which is why upstream's two item constructors attach it to every item they \
                 build and its `customItem` does not. An item that uses the source the group hands \
                 it grows; one that ignores it does not, whatever style it is.",
            );

            section_title(ctx, "8. Icon-only items — five IconButtons in a plain group");
            ButtonGroup::new()
                .item(icon_item(PLUS_PATH, { clone!(plain); move || plain.set(1) }))
                .item(icon_item(FAVORITE_PATH, { clone!(plain); move || plain.set(2) }))
                .item(icon_item(HOME_PATH, { clone!(plain); move || plain.set(3) }))
                .item(icon_item(EDIT_PATH, { clone!(plain); move || plain.set(4) }))
                .item(icon_item(SEARCH_PATH, { clone!(plain); move || plain.set(5) }))
                .build(ctx);
            caption(
                ctx,
                "The item's content is the caller's, so an icon-only button is an item like any \
                 other — here a `FilledIconButton` per item, which is what makes the row five \
                 circles 12 dp apart rather than five pills: `IconButton`'s Round family is a \
                 circle, and its own size picks the 40 dp container and the 24 dp glyph. Press \
                 one and it still grows, squeezing its neighbour.",
            );

            Spacer::vertical(24.0).build(ctx);
        });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(560.0, 900.0)
                .title("ButtonGroup Demo")
                .build(ctx, |ctx| {
                    settings::shell("ButtonGroup Demo", ctx, button_group_demo);
                });
        });
    });
}
