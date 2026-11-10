//! Modal bottom sheet demo — the M3 sheet's three anchors over a dimmed page.
//!
//! The sheet walks the states it has (`Hidden` / `PartiallyExpanded` / `Expanded`), and the page
//! shows what two of the builder's switches change: `sheet_gestures_enabled(false)` removes the drag
//! (the handle's included) while leaving the sheet on screen, and `skip_partially_expanded(true)`
//! drops the half stop so a swipe down goes straight out.
//!
//! Every sheet here is composed EVERY frame with `visible` as an argument, never wrapped in an `if`:
//! an overlay is released by its owner recording `active = false`, and an owner that simply stops
//! composing it counts as "skipped" and is kept. See `docs/alert-dialog.md` ("Composing it").
//!
//! Try: with the half stop off, open the sheet and drag it down — it goes straight out; turn the
//! half stop on and the same drag stops half way first. Turn gestures off and the drag stops working
//! while the sheet stays up. Press Escape or click the scrim and watch the panel SLIDE OUT before it
//! disappears: the dismissal callback fires when the slide finishes, not when it starts.
//!
//! Run with: `cargo run -p winia --example bottom_sheet_demo`.

use letclone::clone;
use winia::prelude::*;

#[path = "common/settings.rs"]
mod settings;

pub fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(760.0, 900.0)
                .title("Modal bottom sheet demo")
                .build(ctx, |ctx| {
                    settings::shell("Modal bottom sheet", ctx, demo_body);
                });
        });
    });
}

#[composable]
fn demo_body(ctx: &mut ComposeCtx, scaffold: ScaffoldContentPadding) {
    // `ctx.remember(|| value)` already returns a `State`, so there is no nested handle here — see
    // docs/state-handles.md section 7.4.
    let open = ctx.remember(|| false);
    let gestures = ctx.remember(|| true);
    let half_stop = ctx.remember(|| false);
    let scroll = ctx.remember(ScrollState::new).get();
    // The sheet's list state lives on the page, not inside the sheet: the sheet's content is an
    // `Fn` that runs every frame it is up, and the scroll position has to survive those runs.
    let list_state = ctx.remember(LazyListState::new).get();

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
            // No page heading here: `settings::shell` already puts this demo's name in the top app
            // bar, and a second large title directly under it read as a duplicated heading.
            Text::new(
                "A modal bottom sheet slides up from the bottom edge over a dimmed page and stops \
                 at one of three anchors. Dragging DOWN while a list inside it is at its top \
                 collapses the sheet instead of scrolling — that is Compose's routing rule, not a \
                 bug: an upward delta expands the sheet first, a downward one belongs to the list \
                 until the list runs out.",
            )
            .font_size(14.0)
            .build(ctx);

            control_panel(ctx, open.clone(), gestures.clone(), half_stop.clone());
            dragging_card(ctx);
            shape_card(ctx);
            dismissal_card(ctx);
        });

    // Composed unconditionally: `visible` decides whether the overlay is registered.
    sheet(ctx, open, gestures, half_stop, list_state);
}

/// The open button plus the two switches, so the page shows the state it is driving.
#[composable]
fn control_panel(
    ctx: &mut ComposeCtx,
    open: State<bool>,
    gestures: State<bool>,
    half_stop: State<bool>,
) {
    Stack::new()
        .modifier(
            Modifier::new()
                .fill_max_width()
                .background(
                    WiniaTheme::colors().surface_container_high,
                    Shape::rounded(12.0),
                )
                .padding(16.0),
        )
        .build(ctx, |ctx| {
            Column::new().spacing(12.0).build(ctx, |ctx| {
                Text::new("Controls").font_size(16.0).build(ctx);
                Row::new().spacing(8.0).build(ctx, |ctx| {
                    Button::new()
                        .modifier(Modifier::new().test_tag("open-sheet"))
                        .on_click({
                            clone!(open);
                            move || open.set(true)
                        })
                        .build(ctx, |ctx| {
                            Text::new("Open sheet").build(ctx);
                        });
                });
                FlowRow::new()
                    .main_spacing(8.0)
                    .cross_spacing(8.0)
                    .build(ctx, |ctx| {
                        toggle_row(
                            ctx,
                            "gestures enabled",
                            "toggle-gestures",
                            gestures,
                            "gestures-state",
                        );
                        toggle_row(
                            ctx,
                            "skip partially expanded",
                            "toggle-half",
                            half_stop,
                            "half-state",
                        );
                    });
            });
        });
}

/// A checkbox plus its current value, so the demo shows the state it is driving.
#[composable]
fn toggle_row(
    ctx: &mut ComposeCtx,
    label: &'static str,
    tag: &'static str,
    value: State<bool>,
    state_tag: &'static str,
) {
    Row::new()
        .spacing(6.0)
        .modifier(Modifier::new().test_tag(tag))
        .build(ctx, |ctx| {
            let now = value.get();
            Checkbox::new(now)
                .on_checked_change({
                    clone!(value);
                    move |next| value.set(next)
                })
                .build(ctx);
            Text::new(label).font_size(14.0).build(ctx);
            Text::new(if now { "on" } else { "off" })
                .font_size(14.0)
                .modifier(Modifier::new().test_tag(state_tag))
                .build(ctx);
        });
}

/// Who owns a drag: the sheet, the list inside it, or nothing at all.
#[composable]
fn dragging_card(ctx: &mut ComposeCtx) {
    Card::new().build(ctx, |ctx| {
        Column::new()
            .spacing(12.0)
            .modifier(Modifier::new().fill_max_width().padding(16.0))
            .build(ctx, |ctx| {
                Text::new("Who owns the drag").font_size(18.0).build(ctx);
                Text::new(
                    "The panel is draggable anywhere outside a scrollable — its header, its \
                     padding, its handle. Inside the list the scroll wins until it cannot move, \
                     and only the leftover delta reaches the sheet.",
                )
                .font_size(14.0)
                .build(ctx);
                Text::new(
                    "`sheet_gestures_enabled(false)` removes BOTH halves: the nested-scroll \
                     routing and the drag itself, the handle's included. Compose puts them behind \
                     one flag too (`anchoredDraggable(enabled = gesturesEnabled && currentValue != \
                     Hidden)`), so a sheet with gestures off cannot be dragged at all — it can only \
                     be dismissed through the scrim or Escape.",
                )
                .font_size(14.0)
                .build(ctx);
            });
    });
}

/// The sheet's own shape rule, which is winia's and not Material 3's.
#[composable]
fn shape_card(ctx: &mut ComposeCtx) {
    Card::new().build(ctx, |ctx| {
        Column::new()
            .spacing(12.0)
            .modifier(Modifier::new().fill_max_width().padding(16.0))
            .build(ctx, |ctx| {
                Text::new("Square when it fills the window")
                    .font_size(18.0)
                    .build(ctx);
                Text::new(
                    "An expanded sheet whose panel reaches the window height drops its 28dp top \
                     corners. This is winia's own rule and is kept deliberately: current M3 passes \
                     `shape` through untouched and never switches on the sheet's state. The \
                     surface is painted per frame, because a radius composed inside the content \
                     closure would never update — those reads happen outside any composition group \
                     and register nothing.",
                )
                .font_size(14.0)
                .build(ctx);
            });
    });
}

/// The three ways out, and when the callback actually runs.
#[composable]
fn dismissal_card(ctx: &mut ComposeCtx) {
    Card::new().build(ctx, |ctx| {
        Column::new()
            .spacing(12.0)
            .modifier(Modifier::new().fill_max_width().padding(16.0))
            .build(ctx, |ctx| {
                Text::new("Dismissal paths").font_size(18.0).build(ctx);
                Text::new(
                    "Clicking the scrim, pressing Escape, or dragging down past the threshold all \
                     end in `on_dismiss_request` — and that callback runs when the slide FINISHES, \
                     not when it starts. The panel animates itself out and only then flips \
                     `visible`, which is what removes the overlay; starting the removal earlier \
                     faded the sheet out mid-slide.",
                )
                .font_size(14.0)
                .build(ctx);
            });
    });
}

/// The sheet itself. Its content is a LAZY list, which is the case the drag routing is about: an
/// upward delta expands the sheet first, and only what the sheet cannot use reaches the list. The
/// list is also tall enough to make the sheet exceed half the window, which is what gives the
/// `PartiallyExpanded` anchor something to sit at.
#[composable]
fn sheet(
    ctx: &mut ComposeCtx,
    open: State<bool>,
    gestures: State<bool>,
    half_stop: State<bool>,
    list_state: LazyListState,
) {
    let close = open.clone();
    // The sheet's content closure is an `Fn` — it runs on every frame the sheet is up — so the
    // list state is cloned per call rather than moved into it.
    let list = list_state.clone();
    ModalBottomSheet::new(open.get())
        .on_dismiss_request(move || close.set(false))
        .sheet_gestures_enabled(gestures.get())
        .skip_partially_expanded(half_stop.get())
        .build(ctx, move |ctx| {
            let theme = WiniaTheme::colors();
            let label_color = theme.on_surface_variant;
            Column::new()
                .spacing(14.0)
                .modifier(Modifier::new().fill_max_width().padding(24.0))
                .build(ctx, |ctx| {
                    Text::new("Sheet content").font_size(20.0).build(ctx);
                    Text::new(
                        "Drag the handle, the header or any padding to move the sheet. Drag the \
                         list while it can still scroll and the list moves instead.",
                    )
                    .font_size(14.0)
                    .color(label_color)
                    .build(ctx);
                    Divider::horizontal().build(ctx);
                    // A FIXED-height viewport: a lazy list has no intrinsic height, and this sheet's
                    // own height is measured from its content (that measurement is what feeds the
                    // anchors), so the list has to be bounded for the panel to have a size at all.
                    Stack::new()
                        .modifier(
                            Modifier::new()
                                .fill_max_width()
                                .height(400.0)
                                .clip(Shape::rounded(12.0)),
                        )
                        .build(ctx, |ctx| {
                            LazyColumn::new()
                                .state(list.clone())
                                .modifier(Modifier::new().fill_max_width().fill_max_height())
                                .spacing(6.0)
                                .items(
                                    40,
                                    |index| index as u64,
                                    |ctx, index| {
                                        // Row fill and text come from the theme: a hardcoded light
                                        // grey reads right in light mode and glares in dark.
                                        let colors = WiniaTheme::colors();
                                        Text::new(format!("Lazy row {index}"))
                                            .font_size(14.0)
                                            .color(colors.on_surface)
                                            .modifier(
                                                Modifier::new()
                                                    .fill_max_width()
                                                    .padding(10.0)
                                                    .background(
                                                        colors.surface_container_high,
                                                        Shape::rounded(10.0),
                                                    ),
                                            )
                                            .build(ctx);
                                    },
                                )
                                .build(ctx);
                        });
                    Text::new(
                        "The list scrolls on its own: an upward drag expands the sheet to its \
                         expanded anchor first, then the leftover reaches these rows.",
                    )
                    .font_size(14.0)
                    .color(label_color)
                    .build(ctx);
                    Text::new(
                        "With `skip partially expanded` on, the anchors are only Hidden and \
                         Expanded: a downward drag no longer stops half way.",
                    )
                    .font_size(14.0)
                    .color(label_color)
                    .build(ctx);
                });
        });
}
