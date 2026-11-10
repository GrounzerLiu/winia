//! Bottom sheet scaffold demo — the DOCKED sheet, as opposed to the modal one.
//!
//! A `BottomSheetScaffold` keeps a sheet on screen at all times: it peeks above the bottom edge,
//! drags up to its expanded height and back down to the peek, and never covers the page the way
//! `ModalBottomSheet` does. This demo shows the parts that differ from the modal sheet:
//!
//! - a **peek** of 96 dp (the component's default is `SCAFFOLD_SHEET_PEEK_HEIGHT` = 56 dp; the
//!   demo raises it so the collapsed strip has something readable in it);
//! - `skip_hidden_state`, Compose's `rememberStandardBottomSheetState`: a downward drag stops at the
//!   peek instead of taking the panel off the screen, because a standard sheet has no hidden anchor;
//! - the **handle as a button**: clicking it expands a peeking sheet and collapses an expanded one,
//!   which is Compose's `DragHandleWithTooltip` state machine;
//! - the **snackbar slot**, whose placement belongs to the scaffold: centred just above the panel
//!   while it is partially expanded, on the window's bottom edge once it is expanded, and riding
//!   the panel's actual edge while it animates;
//! - a list inside the sheet that scrolls on its own — an upward delta expands the sheet first and
//!   only the leftover reaches the list.
//!
//! Try: drag the handle up and watch the panel stop at its expanded height; drag down and watch it
//! stop at the peek rather than disappear; click the handle to toggle between the two; press "Show
//! snackbar" while collapsed and again after expanding, and watch the card move from the panel's top
//! edge to the window's bottom edge.
//!
//! Run with: `cargo run -p winia --example bottom_sheet_scaffold_demo`.

use letclone::clone;
use winia::prelude::*;

#[path = "common/settings.rs"]
mod settings;

pub fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(760.0, 900.0)
                .title("Bottom sheet scaffold demo")
                .build(ctx, |ctx| {
                    settings::shell("Bottom sheet scaffold", ctx, demo_body);
                });
        });
    });
}

#[composable]
fn demo_body(ctx: &mut ComposeCtx, scaffold: ScaffoldContentPadding) {
    // A standard (docked) sheet's state: it starts at the peek and skips the hidden anchor, which is
    // Compose's `rememberStandardBottomSheetState(skipHiddenState = true)`. Passing one in rather than
    // letting the component remember its own is what lets the page's buttons drive it.
    let sheet_state = ctx
        .remember(|| {
            let s = SheetState::new(SheetValue::PartiallyExpanded);
            s.set_skip_hidden_state(true);
            s
        })
        .get();
    let snack = ctx.remember(SnackbarHostState::new).get();
    let list_state = ctx.remember(LazyListState::new).get();

    // The scaffold sits INSIDE `settings::shell`'s own `Scaffold`, so it is anchored from its own box
    // rather than from the window — `a_nested_scaffold_anchors_from_its_own_height` pins that.
    BottomSheetScaffold::new()
        .sheet_state(sheet_state.clone())
        .sheet_peek_height(96.dp())
        // The snackbar slot. Where the card lands is the scaffold's decision, not the caller's: this
        // closure only supplies the card.
        .snackbar({
            clone!(snack);
            move |ctx| SnackbarHost::new(snack.clone()).build(ctx)
        })
        .build(
            ctx,
            {
                clone!(list_state);
                // `sheet_content` is an `Fn`, so it runs more than once — the state is cloned per
                // call rather than moved into the body.
                move |ctx| sheet_body(ctx, list_state.clone())
            },
            {
                clone!(sheet_state, snack);
                move |ctx| page_body(ctx, scaffold, sheet_state, snack)
            },
        );
}

/// The page behind the sheet. It keeps the peek's inset clear, so nothing hides under the panel.
#[composable]
fn page_body(
    ctx: &mut ComposeCtx,
    scaffold: ScaffoldContentPadding,
    sheet_state: SheetState,
    snack: SnackbarHostState,
) {
    let scroll = ctx.remember(ScrollState::new).get();
    let colors = WiniaTheme::colors();

    Column::new()
        .spacing(12.0)
        .modifier(
            Modifier::new()
                .fill_max_size()
                .padding_sides(scaffold.start, scaffold.top, scaffold.end, scaffold.bottom)
                .vertical_scroll(scroll)
                .padding(16.0),
        )
        .build(ctx, |ctx| {
            Text::new(
                "A bottom sheet scaffold docks its sheet over the page instead of covering it. \
                 The sheet is always there — peeking above the bottom edge — and drags between \
                 that peek and its expanded height.",
            )
            .font_size(14.0)
            .build(ctx);

            FlowRow::new()
                .main_spacing(8.0)
                .cross_spacing(8.0)
                .build(ctx, |ctx| {
                    Button::new()
                        .modifier(Modifier::new().test_tag("expand-sheet"))
                        .on_click({
                            clone!(sheet_state);
                            move || sheet_state.expand()
                        })
                        .build(ctx, |ctx| {
                            Text::new("Expand").build(ctx);
                        });
                    Button::new()
                        .style(ButtonStyle::Text)
                        .modifier(Modifier::new().test_tag("collapse-sheet"))
                        .on_click({
                            clone!(sheet_state);
                            move || sheet_state.partial_expand()
                        })
                        .build(ctx, |ctx| {
                            Text::new("Collapse").build(ctx);
                        });
                    Button::new()
                        .style(ButtonStyle::Text)
                        .modifier(Modifier::new().test_tag("show-snackbar"))
                        .on_click({
                            clone!(snack);
                            move || snack.show(SnackbarData::new("Saved to drafts"))
                        })
                        .build(ctx, |ctx| {
                            Text::new("Show snackbar").build(ctx);
                        });
                });

            Text::new("The snackbar's placement")
                .font_size(18.0)
                .build(ctx);
            Text::new(
                "The slot above is only the card; where it goes is the scaffold's rule. While the \
                 sheet is partially expanded the card sits centred just above the panel's top \
                 edge. Once the sheet is expanded it drops to the window's bottom edge. During the \
                 drag it follows the panel's ACTUAL position — the rule reads the state the offset \
                 is closest to, not the one the animation was asked for, so the card does not jump \
                 ahead of the panel.",
            )
            .font_size(14.0)
            .build(ctx);

            Text::new("Peek, and why it never disappears")
                .font_size(18.0)
                .build(ctx);
            Text::new(
                "The peek is 96 dp here, against a component default of 56. Dragging down stops \
                 at that strip: a standard sheet skips the hidden anchor, so there is nowhere \
                 below the peek to go. That is Compose's `rememberStandardBottomSheetState`, and \
                 the reason a docked sheet cannot be swiped away the way a modal one can.",
            )
            .font_size(14.0)
            .build(ctx);

            Text::new("Who owns a drag").font_size(18.0).build(ctx);
            Text::new(
                "The panel is draggable anywhere outside the list. Inside the list the scroll wins \
                 while it can still move — an upward delta expands the sheet first, and only what \
                 the sheet cannot use reaches the list.",
            )
            .font_size(14.0)
            .color(colors.on_surface_variant)
            .build(ctx);
        });
}

/// The sheet's own content: a header and a scrolling list that keeps its own scroll.
#[composable]
fn sheet_body(ctx: &mut ComposeCtx, list_state: LazyListState) {
    let colors = WiniaTheme::colors();

    Column::new()
        .spacing(10.0)
        .modifier(Modifier::new().fill_max_width().padding(16.0))
        .build(ctx, |ctx| {
            Text::new("Docked sheet").font_size(18.0).build(ctx);
            Text::new(
                "Drag this area, the handle, or any padding to move the sheet. The list below \
                 keeps its own scroll.",
            )
            .font_size(13.0)
            .color(colors.on_surface_variant)
            .build(ctx);

            // A FIXED-height viewport, because the sheet's own height is anchor-driven rather than
            // content-driven: leaving the list unbounded would make the expanded height depend on
            // how many rows there happen to be.
            Stack::new()
                .modifier(
                    Modifier::new()
                        .fill_max_width()
                        .height(420.0)
                        .clip(Shape::rounded(12.0)),
                )
                .build(ctx, |ctx| {
                    LazyColumn::new()
                        .state(list_state)
                        .modifier(Modifier::new().fill_max_width().fill_max_height())
                        .items(
                            40,
                            |index| index as u64,
                            |ctx, index| {
                                // Row fill and text come from the theme: the sheet's container is
                                // `surface_container_low`, so its rows step up to `…_high`.
                                let colors = WiniaTheme::colors();
                                Text::new(format!("Docked row {index}"))
                                    .font_size(13.0)
                                    .color(colors.on_surface)
                                    .modifier(
                                        Modifier::new().fill_max_width().padding(10.0).background(
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
                "Clicking the handle toggles between the peek and fully expanded — Compose wires \
                 the same state machine onto `DragHandleWithTooltip`.",
            )
            .font_size(13.0)
            .color(colors.on_surface_variant)
            .build(ctx);
        });
}
