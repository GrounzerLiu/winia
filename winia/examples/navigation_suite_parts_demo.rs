//! `NavigationSuite` and `NavigationSuiteScaffoldLayout` on their own — the two halves the scaffold is
//! built from, shown WITHOUT it.
//!
//! 1. `NavigationSuite`: one shape, its items, nothing else. That is what you reach for when the
//!    navigation has to live inside your own layout.
//! 2. `NavigationSuiteScaffoldLayout`: the placement. Bring your own navigation and your own content and
//!    still get the scaffold's geometry — the navigation pinned to the bottom, the content above it, and
//!    the FAB slot 16dp over the navigation. This example builds the BAR case; for a RAIL shape
//!    upstream never places the action at all (it belongs to the navigation there), and winia parks
//!    that slot off-view instead of dropping it.
//!
//! Run: cargo run -p winia --example navigation_suite_parts_demo

use letclone::clone;
use winia::prelude::*;
// Shared example chrome: top app bar with the settings sheet (theme mode + layout direction).
#[path = "common/settings.rs"]
mod settings;

const HOME_PATH: &str = "M10 20v-6h4v6h5v-9h3L12 3 2 11h3v9z";
const SEARCH_PATH: &str = "M15.5 14h-.79l-.28-.27C15.41 12.59 16 11.11 16 9.5 16 5.91 13.09 3 9.5 3S3 5.91 3 9.5 5.91 16 9.5 16c1.61 0 3.09-.59 4.23-1.57l.27.28v.79l5 4.99L20.49 19l-4.99-5zm-6 0C7.01 14 5 11.99 5 9.5S7.01 5 9.5 5 14 7.01 14 9.5 11.99 14 9.5 14z";
const FAVORITE_PATH: &str = "M12 21.35l-1.45-1.32C5.4 15.36 2 12.28 2 8.5 2 5.42 4.42 3 7.5 3c1.74 0 3.41.81 4.5 2.09C13.09 3.81 14.76 3 16.5 3 19.58 3 22 5.42 22 8.5c0 3.78-3.4 6.86-8.55 11.54L12 21.35z";
const ADD_PATH: &str = "M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z";

const DESTINATIONS: [(&str, &str); 3] = [
    ("Home", HOME_PATH),
    ("Search", SEARCH_PATH),
    ("Favorites", FAVORITE_PATH),
];

/// 1. The same three items in three shapes, side by side, no scaffold anywhere.
#[composable]
fn standalone_shapes(ctx: &mut ComposeCtx) {
    let selected = ctx.remember(|| 0usize);
    Row::new()
        .spacing(32.0)
        .alignment(Alignment::Start)
        .build(ctx, |ctx| {
            // Each shape is BOUNDED: a bar fills its width and a rail fills its height, so an unbounded one
            // inside a Row takes the whole axis and pushes its neighbours — and everything below — out of
            // the window.
            for (label, shape, width, height) in [
                ("short bar (compact)", NavigationSuiteType::ShortNavigationBarCompact, 320.0, 80.0),
                ("wide rail (collapsed)", NavigationSuiteType::WideNavigationRailCollapsed, 96.0, 240.0),
                ("classic rail", NavigationSuiteType::NavigationRail, 80.0, 240.0),
            ] {
                Column::new()
                    .spacing(6.0)
                    .alignment(Alignment::Center)
                    .build(ctx, |ctx| {
                        Text::new(label).font_size(12.0).build(ctx);
                        Stack::new()
                            .modifier(Modifier::new().width(width).height(height))
                            .build(ctx, |ctx| {
                                NavigationSuite::new(
                            {
                                clone!(selected);
                                move |items| {
                                    for (index, (name, path)) in DESTINATIONS.iter().enumerate() {
                                        let (name, path) = (*name, *path);
                                        items.item(
                                            selected.get() == index,
                                            move |ctx| Icon::svg_path(path).size(24.0).build(ctx),
                                            move |ctx| Text::new(name).build(ctx),
                                            {
                                                clone!(selected);
                                                move || selected.set(index)
                                            },
                                        );
                                    }
                                }
                            },
                                    shape,
                                )
                                .build(ctx);
                            });
                    });
            }
        });
}

/// 2. The placement half with a navigation that is NOT the suite's: a plain coloured bar of clickable
/// icon columns, the app content beside it, and the FAB slot the layout floats above both.
#[composable]
fn scaffold_layout_section(ctx: &mut ComposeCtx, picked: State<usize>, taps: State<i32>) {
    NavigationSuiteScaffoldLayout::new(
        {
            clone!(picked);
            move |ctx| {
                Row::new()
                    .modifier(
                        Modifier::new()
                            .fill_max_width()
                            .height(80.0)
                            .background(Color::from_argb(255, 60, 56, 92), Shape::Rectangle),
                    )
                    .spacing(40.0)
                    .alignment(Alignment::Center)
                    .build(ctx, |ctx| {
                        for (index, (name, path)) in DESTINATIONS.iter().enumerate() {
                            let (name, path) = (*name, *path);
                            Column::new()
                                .modifier(Modifier::new().padding(6.0).clickable({
                                    clone!(picked);
                                    move || picked.set(index)
                                }))
                                .spacing(2.0)
                                .alignment(Alignment::Center)
                                .build(ctx, |ctx| {
                                    let tint = if picked.get() == index {
                                        Color::from_argb(255, 208, 188, 255)
                                    } else {
                                        Color::from_argb(255, 202, 196, 208)
                                    };
                                    WiniaTheme::with_content_color(tint, ctx, |ctx| {
                                        Icon::svg_path(path).size(24.0).build(ctx);
                                        Text::new(name).font_size(12.0).build(ctx);
                                    });
                                });
                        }
                    });
            }
        },
        {
            clone!(picked, taps);
            move |ctx| {
                Column::new()
                    .modifier(Modifier::new().fill_max_size().padding(16.0))
                    .spacing(12.0)
                    .build(ctx, |ctx| {
                        Text::new(format!("picked: {}", DESTINATIONS[picked.get()].0)).build(ctx);
                        Text::new(format!("taps: {}", taps.get())).build(ctx);
                        Button::filled()
                            .on_click({
                                clone!(taps);
                                move || taps.set(taps.get() + 1)
                            })
                            .build(ctx, |ctx| {
                                Text::new("Tap the content").build(ctx);
                            });
                    });
            }
        },
    )
    .layout_type(NavigationSuiteType::ShortNavigationBarCompact)
    .primary_action(|ctx| {
        FloatingActionButton::new().on_click(|| {}).build(ctx, |ctx| {
            Icon::svg_path(ADD_PATH).size(24.0).build(ctx);
        });
    })
    .build(ctx);
}

#[composable]
fn navigation_suite_parts_demo(ctx: &mut ComposeCtx) {
    let picked = ctx.remember(|| 0usize);
    let taps = ctx.remember(|| 0i32);
    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .spacing(12.0)
        .build(ctx, |ctx| {
            Text::new("1. NavigationSuite - one shape each, no scaffold").build(ctx);
            standalone_shapes(ctx);
            Text::new("2. NavigationSuiteScaffoldLayout - your navigation, your content, the FAB slot")
                .build(ctx);
            Stack::new()
                .modifier(Modifier::new().fill_max_width().height(320.0))
                .build(ctx, |ctx| {
                    scaffold_layout_section(ctx, picked.clone(), taps.clone());
                });
        });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(760.0, 680.0)
                .title("Navigation suite parts")
                .build(ctx, |ctx| {
                    settings::shell("Navigation suite parts", ctx, |ctx| {
                        navigation_suite_parts_demo(ctx)
                    });
                });
        });
    });
}
