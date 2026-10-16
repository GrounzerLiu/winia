//! Navigation drawer demo — the three Compose drawers, laid out like the M3 spec's own example.
//!
//! - **Modal** (`ModalNavigationDrawer`) slides over the page behind a scrim.
//! - **Dismissible** (`DismissibleNavigationDrawer`) PUSHES the page aside, with no scrim.
//! - **Permanent** (`PermanentNavigationDrawer`) has no state at all: the sheet is always on
//!   screen and the page sits beside it.
//!
//! The sheet follows https://m3.material.io/components/navigation-drawer/overview : a `Mail`
//! header, four destinations with the unread count at the TRAILING edge of Inbox, then a divider,
//! a `Labels` section header and a label destination. Every icon is REAL Material Icons path data
//! (the outlined theme, `google/material-design-icons`) — inbox, send, favorite_border,
//! delete_outline, label — looked up rather than drawn.
//!
//! Run: cargo run -p winia --example navigation_drawer_demo

use letclone::clone;
use winia::prelude::*;
// Shared example chrome: top app bar with the settings sheet (theme mode + layout direction).
#[path = "common/settings.rs"]
mod settings;

/// Material Icons outlined `inbox`.
const INBOX_PATH: &str = "M19 3H5c-1.1 0-2 .9-2 2v14c0 1.1.89 2 2 2h14c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2zm0 16H5v-3h3.56c.69 1.19 1.97 2 3.45 2s2.75-.81 3.45-2H19v3zm0-5h-4.99c0 1.1-.9 2-2 2s-2-.9-2-2H5V5h14v9z";
/// Material Icons outlined `send` (the spec's Outbox).
const SEND_PATH: &str = "M4.01 6.03l7.51 3.22-7.52-1 .01-2.22m7.5 8.72L4 17.97v-2.22l7.51-1M2.01 3L2 10l15 2-15 2 .01 7L23 12 2.01 3z";
/// Material Icons outlined `favorite_border` (the spec's Favorites).
const FAVORITE_PATH: &str = "M16.5 3c-1.74 0-3.41.81-4.5 2.09C10.91 3.81 9.24 3 7.5 3 4.42 3 2 5.42 2 8.5c0 3.78 3.4 6.86 8.55 11.54L12 21.35l1.45-1.32C18.6 15.36 22 12.28 22 8.5 22 5.42 19.58 3 16.5 3zm-4.4 15.55l-.1.1-.1-.1C7.14 14.24 4 11.39 4 8.5 4 6.5 5.5 5 7.5 5c1.54 0 3.04.99 3.57 2.36h1.87C13.46 5.99 14.96 5 16.5 5c2 0 3.5 1.5 3.5 3.5 0 2.89-3.14 5.74-7.9 10.05z";
/// Material Icons outlined `delete`.
const DELETE_PATH: &str = "M6 19c0 1.1.9 2 2 2h8c1.1 0 2-.9 2-2V7H6v12zM8 9h8v10H8V9zm7.5-5l-1-1h-5l-1 1H5v2h14V4h-3.5z";
/// Material Icons outlined `label`.
const LABEL_PATH: &str = "M17.63 5.84C17.27 5.33 16.67 5 16 5L5 5.01C3.9 5.01 3 5.9 3 7v10c0 1.1.9 1.99 2 1.99L16 19c.67 0 1.27-.33 1.63-.84L22 12l-4.37-6.16zM16 17H5V7h11l3.55 5L16 17z";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Page {
    Inbox,
    Outbox,
    Favorites,
    Trash,
    Label,
}

impl Page {
    /// The four destinations above the divider, in the spec's order.
    fn primary() -> [Page; 4] {
        [Page::Inbox, Page::Outbox, Page::Favorites, Page::Trash]
    }
    fn title(self) -> &'static str {
        match self {
            Page::Inbox => "Inbox",
            Page::Outbox => "Outbox",
            Page::Favorites => "Favorites",
            Page::Trash => "Trash",
            Page::Label => "Label",
        }
    }
    fn icon(self) -> &'static str {
        match self {
            Page::Inbox => INBOX_PATH,
            Page::Outbox => SEND_PATH,
            Page::Favorites => FAVORITE_PATH,
            Page::Trash => DELETE_PATH,
            Page::Label => LABEL_PATH,
        }
    }
    /// The trailing count, where the spec shows one.
    fn count(self) -> Option<&'static str> {
        match self {
            Page::Inbox => Some("24"),
            _ => None,
        }
    }
    fn body(self) -> &'static str {
        match self {
            Page::Inbox => "24 conversations. Drag the drawer back to its edge to close it \
                            (the scrim also closes it in the modal variant), or switch the \
                            drawer with the three buttons above.",
            Page::Outbox => "Messages you have sent.",
            Page::Favorites => "Nothing favourited yet.",
            Page::Trash => "Deleted messages are kept for 30 days.",
            Page::Label => "Messages carrying this label.",
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

/// One destination row: its icon, its label, and the trailing count when it has one.
#[composable]
fn destination(ctx: &mut ComposeCtx, item: Page, page: State<Page>, drawer: DrawerState) {
    let mut row = NavigationDrawerItem::text_label(item.title(), page.get() == item).icon({
        move |ctx| {
            Icon::svg_path(item.icon()).size(24.0).build(ctx);
        }
    });
    if let Some(count) = item.count() {
        // The count is PLAIN TEXT at the trailing edge, as the spec draws it — the item's badge slot
        // is caller-composed, so a `Badge` pill would also be legal, but this is the reference look.
        row = row.badge(move |ctx| {
            Text::new(count).font_size(14.0).build(ctx);
        });
    }
    row.on_click({
        clone!(page, drawer);
        move || {
            page.set(item);
            drawer.close();
        }
    })
    .build(ctx);
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
            // A FlowRow, not a Row: the three names do not fit the 248dp a PERMANENT sheet leaves
            // of a 640dp window, and a squeezed button wraps its label into five lines.
            FlowRow::new()
                .main_spacing(8.0)
                .cross_spacing(8.0)
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

/// The sheet: header, the four destinations, a divider, a section header and a label, as the M3
/// spec lays a navigation drawer out.
#[composable]
fn sheet_body(ctx: &mut ComposeCtx, page: State<Page>, drawer: DrawerState) {
    Column::new()
        .modifier(Modifier::new().fill_max_width().padding_vertical(12.0))
        .spacing(4.0)
        .build(ctx, |ctx| {
            Row::new()
                .modifier(Modifier::new().fill_max_width().padding_sides(28.0, 12.0, 24.0, 12.0))
                .alignment(Alignment::Center)
                .build(ctx, |ctx| {
                    Text::new("Mail").font_size(22.0).build(ctx);
                });
            for item in Page::primary() {
                destination(ctx, item, page.clone(), drawer.clone());
            }
            Divider::horizontal()
                .modifier(Modifier::new().padding_sides(28.0, 4.0, 28.0, 4.0))
                .build(ctx);
            // A section header lines up with the item ICONS, not with their labels.
            Text::new("Labels")
                .modifier(Modifier::new().padding_sides(28.0, 8.0, 24.0, 8.0))
                .font_size(14.0)
                .build(ctx);
            destination(ctx, Page::Label, page.clone(), drawer.clone());
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
