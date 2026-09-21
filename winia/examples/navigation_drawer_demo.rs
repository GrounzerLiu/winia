//! Modal navigation drawer demo — Compose `ModalNavigationDrawer` parity.
//!
//! Shows: opening from a button, closing via the scrim, closing by dragging the
//! drawer back toward its edge (a flick clears the drawer's 400dp/s threshold),
//! the selected item's pill indicator, and the content staying composed underneath.
//!
//! Run: cargo run -p winia --example navigation_drawer_demo

use letclone::clone;
use winia::prelude::*;

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
    fn body(self) -> &'static str {
        match self {
            Page::Inbox => "12 conversations. Drag the drawer back to the left edge, \
                            or click the scrim, to close it.",
            Page::Starred => "Nothing starred yet.",
            Page::Sent => "Messages you have sent.",
            Page::Trash => "Deleted messages are kept for 30 days.",
        }
    }
}

#[composable]
fn navigation_drawer_demo(ctx: &mut ComposeCtx) {
    let page = ctx.remember(|| Page::Inbox);
    let drawer = ctx.remember(|| DrawerState::new(DrawerValue::Closed)).get();

    ModalNavigationDrawer::new({
        let drawer = drawer.clone();
        let page = page.clone();
        move |ctx| {
            Column::new()
                .modifier(Modifier::new().fill_max_size().padding(16.0))
                .spacing(16.0)
                .build(ctx, |ctx| {
                    Row::new()
                        .spacing(12.0)
                        .alignment(Alignment::Center)
                        .build(ctx, |ctx| {
                            Button::new()
                                .on_click({
                                    clone!(drawer);
                                    move || drawer.open()
                                })
                                .build(ctx, |ctx| {
                                    Text::new("Menu").build(ctx);
                                });
                            Text::new(page.get().title()).font_size(20.0).build(ctx);
                        });
                    Divider::horizontal().build(ctx);
                    Text::new(page.get().body()).font_size(14.0).build(ctx);
                });
        }
    })
    .drawer_state(drawer.clone())
    .drawer_content({
        let page = page.clone();
        let drawer = drawer.clone();
        move |ctx| {
            ModalDrawerSheet::new({
                let page = page.clone();
                let drawer = drawer.clone();
                move |ctx| {
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
                                    Badge::new().content(|ctx| { Text::new("3").font_size(11.0).build(ctx); }).build(ctx);
                                });
                            for item in Page::all() {
                                let selected = page.get() == item;
                                let mut item_builder =
                                    NavigationDrawerItem::text_label(item.title(), selected);
                                if item == Page::Inbox {
                                    // The badge slot sits after the label.
                                    item_builder =
                                        item_builder.badge(|ctx| {
                                        Badge::new()
                                            .content(|ctx| { Text::new("3").font_size(11.0).build(ctx); })
                                            .build(ctx);
                                    });
                                }
                                item_builder
                                    .on_click({
                                        let page = page.clone();
                                        let drawer = drawer.clone();
                                        move || {
                                            page.set(item);
                                            drawer.close();
                                        }
                                    })
                                    .build(ctx);
                            }
                        });
                }
            })
            .build(ctx);
        }
    })
    .build(ctx);


}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(520.0, 620.0)
                .title("Navigation drawer")
                .build(ctx, |ctx| navigation_drawer_demo(ctx));
        });
    });
}
