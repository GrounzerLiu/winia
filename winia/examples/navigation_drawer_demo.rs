//! Navigation drawer demo — all THREE Compose drawers, switched from the top bar.
//!
//! - **Modal** (`ModalNavigationDrawer`) slides over the page behind a scrim.
//! - **Dismissible** (`DismissibleNavigationDrawer`) PUSHES the page aside, with no scrim.
//! - **Permanent** (`PermanentNavigationDrawer`) has no state at all: the sheet is always on
//!   screen and the page sits beside it.
//!
//! The drawer items carry REAL Material Icons path data (`google/material-design-icons`:
//! inbox, star, send, delete) rather than shapes drawn to look like icons, and each is drawn at
//! its own 24dp size — the item no longer forces its icon slot to 24.
//!
//! Run: cargo run -p winia --example navigation_drawer_demo

use letclone::clone;
use winia::prelude::*;
// Shared example chrome: top app bar with the settings sheet (theme mode + layout direction).
#[path = "common/settings.rs"]
mod settings;

/// Material Icons `inbox`.
const INBOX_PATH: &str = "M19 3H4.99c-1.11 0-1.98.89-1.98 2L3 19c0 1.1.88 2 1.99 2H19c1.1 0 2-.9 2-2V5c0-1.11-.9-2-2-2zm0 12h-4c0 1.66-1.35 3-3 3s-3-1.34-3-3H4.99V5H19v10z";
/// Material Icons `star`.
const STAR_PATH: &str = "M12 17.27L18.18 21l-1.64-7.03L22 9.24l-7.19-.61L12 2 9.19 8.63 2 9.24l5.46 4.73L5.82 21z";
/// Material Icons `send`.
const SEND_PATH: &str = "M2.01 21L23 12 2.01 3 2 10l15 2-15 2z";
/// Material Icons `delete`.
const DELETE_PATH: &str = "M6 19c0 1.1.9 2 2 2h8c1.1 0 2-.9 2-2V7H6v12zM19 4h-3.5l-1-1h-5l-1 1H5v2h14V4z";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Page {
    Inbox,
    Starred,
    Sent,
    Trash,
}

impl Page {
    fn all() -> [Page; 4] {
        [Page::Inbox, Page::Starred, Page::Sent, Page::Trash]
    }
    fn title(self) -> &'static str {
        match self {
            Page::Inbox => "Inbox",
            Page::Starred => "Starred",
            Page::Sent => "Sent",
            Page::Trash => "Trash",
        }
    }
    /// The destination's Material icon.
    fn icon(self) -> &'static str {
        match self {
            Page::Inbox => INBOX_PATH,
            Page::Starred => STAR_PATH,
            Page::Sent => SEND_PATH,
            Page::Trash => DELETE_PATH,
        }
    }
    fn body(self) -> &'static str {
        match self {
            Page::Inbox => "12 conversations. Drag the drawer back to its edge to close it \
                            (the scrim also closes it in the modal variant), or switch the \
                            drawer with the three buttons above.",
            Page::Starred => "Nothing starred yet.",
            Page::Sent => "Messages you have sent.",
            Page::Trash => "Deleted messages are kept for 30 days.",
        }
    }
}

/// Which of the three drawers wraps the page.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum DrawerKind {
    Modal,
    Dismissible,
    Permanent,
}

impl DrawerKind {
    fn all() -> [DrawerKind; 3] {
        [DrawerKind::Modal, DrawerKind::Dismissible, DrawerKind::Permanent]
    }
    fn title(self) -> &'static str {
        match self {
            DrawerKind::Modal => "Modal",
            DrawerKind::Dismissible => "Dismissible",
            DrawerKind::Permanent => "Permanent",
        }
    }
}

/// The page behind (or beside) the drawer.
#[composable]
fn page_body(ctx: &mut ComposeCtx, page: State<Page>, drawer: DrawerState, kind: State<DrawerKind>) {
    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .spacing(16.0)
        .build(ctx, |ctx| {
            Row::new()
                .spacing(12.0)
                .alignment(Alignment::Center)
                .build(ctx, |ctx| {
                    // The permanent drawer has nothing to open: its sheet never leaves.
                    if kind.get() != DrawerKind::Permanent {
                        Button::new()
                            .on_click({
                                clone!(drawer);
                                move || drawer.open()
                            })
                            .build(ctx, |ctx| {
                                Text::new("Menu").build(ctx);
                            });
                    }
                    Text::new(page.get().title()).font_size(20.0).build(ctx);
                });
            Row::new()
                .spacing(8.0)
                .alignment(Alignment::Center)
                .build(ctx, |ctx| {
                    for option in DrawerKind::all() {
                        let selected = kind.get() == option;
                        let mut b = if selected { Button::filled() } else { Button::outlined() };
                        b = b.on_click({
                            clone!(kind);
                            move || kind.set(option)
                        });
                        b.build(ctx, |ctx| {
                            Text::new(option.title()).font_size(13.0).build(ctx);
                        });
                    }
                });
            Divider::horizontal().build(ctx);
            Text::new(page.get().body()).font_size(14.0).build(ctx);
        });
}

/// The sheet's content: a header and the four destinations, each with its Material icon.
#[composable]
fn sheet_body(ctx: &mut ComposeCtx, page: State<Page>, drawer: DrawerState) {
    Column::new()
        .modifier(Modifier::new().fill_max_width().padding_vertical(12.0))
        .spacing(4.0)
        .build(ctx, |ctx| {
            Row::new()
                .modifier(Modifier::new().fill_max_width().padding_sides(16.0, 12.0, 24.0, 12.0))
                .spacing(12.0)
                .alignment(Alignment::Center)
                .build(ctx, |ctx| {
                    Text::new("Mail").font_size(22.0).build(ctx);
                });
            for item in Page::all() {
                let selected = page.get() == item;
                let mut item_builder = NavigationDrawerItem::text_label(item.title(), selected)
                    .icon(move |ctx| {
                        Icon::svg_path(item.icon()).size(24.0).build(ctx);
                    });
                if item == Page::Inbox {
                    // The count belongs HERE, not on the sheet header: M3 puts a badge on a
                    // DESTINATION, and the item's badge slot is laid out at the END of the row,
                    // after the label has taken the slack (upstream `NavigationDrawer.kt:1156-1160`).
                    item_builder = item_builder.badge(|ctx| {
                        Badge::new()
                            .content(|ctx| {
                                Text::new("3").font_size(11.0).build(ctx);
                            })
                            .build(ctx);
                    });
                }
                item_builder
                    .on_click({
                        clone!(page, drawer);
                        move || {
                            page.set(item);
                            drawer.close();
                        }
                    })
                    .build(ctx);
            }
        });
}

#[composable]
fn navigation_drawer_demo(ctx: &mut ComposeCtx) {
    let page = ctx.remember(|| Page::Inbox);
    let drawer = ctx.remember(|| DrawerState::new(DrawerValue::Closed)).get();
    let kind = ctx.remember(|| DrawerKind::Modal);

    // Each arm builds its OWN clones: a closure is `FnOnce`, so no two arms can share one.
    match kind.get() {
        DrawerKind::Modal => {
            let (p1, d1, k1) = (page.clone(), drawer.clone(), kind.clone());
            let (p2, d2) = (page.clone(), drawer.clone());
            ModalNavigationDrawer::new(move |ctx| page_body(ctx, p1, d1, k1))
                .drawer_state(drawer.clone())
                .drawer_content(move |ctx| {
                    ModalDrawerSheet::new(move |ctx| sheet_body(ctx, p2, d2)).build(ctx);
                })
                .build(ctx);
        }
        DrawerKind::Dismissible => {
            let (p1, d1, k1) = (page.clone(), drawer.clone(), kind.clone());
            let (p2, d2) = (page.clone(), drawer.clone());
            DismissibleNavigationDrawer::new(move |ctx| page_body(ctx, p1, d1, k1))
                .drawer_state(drawer.clone())
                .drawer_content(move |ctx| {
                    DrawerSheet::dismissible(move |ctx| sheet_body(ctx, p2, d2)).build(ctx);
                })
                .build(ctx);
        }
        DrawerKind::Permanent => {
            let (p1, d1, k1) = (page.clone(), drawer.clone(), kind.clone());
            let (p2, d2) = (page.clone(), drawer.clone());
            PermanentNavigationDrawer::new(move |ctx| page_body(ctx, p1, d1, k1))
                .drawer_content(move |ctx| {
                    DrawerSheet::permanent(move |ctx| sheet_body(ctx, p2, d2)).build(ctx);
                })
                .build(ctx);
        }
    }
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(640.0, 620.0)
                .title("Navigation drawer")
                .build(ctx, |ctx| {
                    settings::shell("Navigation drawer", ctx, |ctx| navigation_drawer_demo(ctx));
                });
        });
    });
}
