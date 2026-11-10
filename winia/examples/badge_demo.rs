//! Badge demo — the M3 badge, the two sizes it takes, and `BadgedBox` hanging one on an anchor.
//!
//! The page walks the three shapes a badge can take (the 6dp dot, a count, a capped "99+"), then
//! hangs a live count on an icon through `BadgedBox`, then shows that the label colour is DERIVED
//! from the container rather than fixed.
//!
//! Try: press `+1` / `−1` and watch the bell's badge follow; press `120` and watch it cap at
//! "99+"; open the settings sheet and switch the layout direction — a `BadgedBox` places its
//! badge with `placeRelative`, so the whole badge mirrors to the icon's other top corner.
//!
//! Run with: `cargo run -p winia --example badge_demo`.

use letclone::clone;
use winia::prelude::*;

#[path = "common/settings.rs"]
mod settings;

/// Material Symbols `notifications` (24dp, filled), copied from fonts.google.com/icons.
const BELL_ICON: &str = "M12,22c1.1,0,2-0.9,2-2h-4c0,1.1,0.89,2,2,2zM18,16v-5c0-3.07-1.64-5.64-4.5-6.32V4c0-0.83-0.67-1.5-1.5-1.5s-1.5,0.67-1.5,1.5v0.68C7.63,5.36,6,7.92,6,11v5l-2,2v1h16v-1l-2-2z";
/// Material Symbols `mail` (24dp, filled).
const MAIL_ICON: &str = "M20,4H4c-1.1,0-1.99,0.9-1.99,2L2,18c0,1.1,0.9,2,2,2h16c1.1,0,2-0.9,2-2V6c0-1.1-0.9-2-2-2zM20,8l-8,5l-8-5V6l8,5l8-5v2z";
/// Material Symbols `shopping_cart` (24dp, filled).
const CART_ICON: &str = "M7,18c-1.1,0-1.99,0.9-1.99,2S5.9,22,7,22s2-0.9,2-2S8.1,18,7,18zM1,2v2h2l3.6,7.59l-1.35,2.45C5.09,14.32,5,14.65,5,15c0,1.1,0.9,2,2,2h12v-2H7.42c-0.14,0-0.25-0.11-0.25-0.25l0.03-0.12L8.1,13h7.45c0.75,0,1.41-0.41,1.75-1.03l3.58-6.49C21.25,4.82,20.77,4,20,4H5.21l-0.94-2H1zM17,18c-1.1,0-1.99,0.9-1.99,2s0.89,2,1.99,2s2-0.9,2-2S18.1,18,17,18z";

pub fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(760.0, 940.0)
                .title("Badge demo")
                .build(ctx, |ctx| {
                    settings::shell("Badge", ctx, demo_body);
                });
        });
    });
}

#[composable]
fn demo_body(ctx: &mut ComposeCtx, scaffold: ScaffoldContentPadding) {
    // `ctx.remember` already returns a `State`, so there is no nested handle here — see
    // docs/state-handles.md section 7.4.
    let count = ctx.remember(|| 3u32);
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
            Text::new("Badges").font_size(22.0).build(ctx);
            Text::new(
                "A badge is the small status marker Material hangs on an icon: a bare dot when \
                 there is only \"something\", or a short label when there is a count. `BadgedBox` \
                 is the pair — the anchor plus the badge it positions on the anchor's corner.",
            )
            .font_size(14.0)
            .build(ctx);

            shapes_card(ctx);
            badged_box_card(ctx, count.clone());
            live_count_card(ctx, count);
            colour_card(ctx);
        });
}

/// The shapes a badge takes, plus one with a custom container colour.
#[composable]
fn shapes_card(ctx: &mut ComposeCtx) {
    let theme = WiniaTheme::colors();
    Card::new().build(ctx, |ctx| {
        Column::new()
            .spacing(16.0)
            .modifier(Modifier::new().fill_max_width().padding(16.0))
            .build(ctx, |ctx| {
                Text::new("Three shapes").font_size(18.0).build(ctx);
                Text::new(
                    "No content is a 6×6 dot. Adding `.content(..)` makes it a large badge: at \
                     least 16×16, an 8dp corner and 4dp of horizontal padding, so a wider count \
                     grows the badge instead of overflowing it.",
                )
                .font_size(14.0)
                .build(ctx);

                FlowRow::new()
                    .main_spacing(28.0)
                    .cross_spacing(14.0)
                    .build(ctx, |ctx| {
                        sample(ctx, "dot", |ctx| {
                            Badge::new().build(ctx);
                        });
                        sample(ctx, "3", |ctx| {
                            Badge::new()
                                .content(|ctx| {
                                    Text::new("3").build(ctx);
                                })
                                .build(ctx);
                        });
                        sample(ctx, "99+", |ctx| {
                            Badge::new()
                                .content(|ctx| {
                                    Text::new("99+").build(ctx);
                                })
                                .build(ctx);
                        });
                        sample(ctx, "primary", {
                            // `sample`'s closure is `'static`, so it cannot borrow `theme` —
                            // `Color` is `Copy`, so take it by value.
                            let primary = theme.primary;
                            move |ctx| {
                                Badge::new()
                                    .container_color(primary)
                                    .content(|ctx| {
                                        Text::new("7").build(ctx);
                                    })
                                    .build(ctx);
                            }
                        });
                    });
            });
    });
}

/// `BadgedBox` holds an anchor and a badge, and places the badge on the anchor's top-trailing
/// corner. The badge's SIZE decides the offset: a dot overlaps the anchor by 6dp, a content badge
/// by 12dp horizontally and 14dp vertically.
#[composable]
fn badged_box_card(ctx: &mut ComposeCtx, count: State<u32>) {
    Card::new().build(ctx, |ctx| {
        Column::new()
            .spacing(16.0)
            .modifier(Modifier::new().fill_max_width().padding(16.0))
            .build(ctx, |ctx| {
                Text::new("BadgedBox on an icon").font_size(18.0).build(ctx);
                Text::new(
                    "The box measures with the ANCHOR's size — a badge never widens the icon it \
                     hangs on. Switch the layout direction in the settings sheet and the badges \
                     mirror to the icons' other top corner.",
                )
                .font_size(14.0)
                .build(ctx);

                FlowRow::new()
                    .main_spacing(32.0)
                    .cross_spacing(20.0)
                    .build(ctx, |ctx| {
                        // A live count: read INSIDE the badge's content closure so the dependency
                        // lands on the group that has to re-run (docs/component-authoring.md
                        // section 3.2).
                        badged_icon(ctx, BELL_ICON, "Notifications", {
                            clone!(count);
                            move |ctx| {
                                Badge::new()
                                    .content(move |ctx| {
                                        Text::new(count_text(count.get())).build(ctx);
                                    })
                                    .build(ctx);
                            }
                        });
                        // The bare dot: "there is something", with no number to show.
                        badged_icon(ctx, MAIL_ICON, "Mail", |ctx| {
                            Badge::new().build(ctx);
                        });
                        // A capped count widens the badge, and the corner stays fully rounded.
                        badged_icon(ctx, CART_ICON, "Cart", |ctx| {
                            Badge::new()
                                .content(|ctx| {
                                    Text::new("99+").build(ctx);
                                })
                                .build(ctx);
                        });
                    });
            });
    });
}

/// An anchor (a 24dp icon) with a badge on its corner.
#[composable]
fn badged_icon(
    ctx: &mut ComposeCtx,
    icon: &'static str,
    description: &'static str,
    badge: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static,
) {
    // The icon is the anchor: `BadgedBox` reports the anchor's size and hangs the badge past its
    // top-trailing corner.
    BadgedBox::new(badge).build(ctx, |ctx| {
        Icon::svg_path(icon)
            .size(24.0)
            .content_description(description)
            .build(ctx);
    });
}

/// The count badge driven by buttons, which is what a badge is for.
#[composable]
fn live_count_card(ctx: &mut ComposeCtx, count: State<u32>) {
    let theme = WiniaTheme::colors();
    Card::new().build(ctx, |ctx| {
        Column::new()
            .spacing(12.0)
            .modifier(Modifier::new().fill_max_width().padding(16.0))
            .build(ctx, |ctx| {
                Text::new("A live count").font_size(18.0).build(ctx);
                Text::new(
                    "M3 caps a count at \"99+\" so a runaway counter cannot stretch the badge \
                     across the anchor.",
                )
                .font_size(14.0)
                .build(ctx);

                FlowRow::new()
                    .main_spacing(8.0)
                    .cross_spacing(8.0)
                    .build(ctx, |ctx| {
                        Button::new()
                            .modifier(Modifier::new().test_tag("badge-dec"))
                            .on_click({
                                clone!(count);
                                move || count.update(|n| *n = n.saturating_sub(1))
                            })
                            .build(ctx, |ctx| {
                                Text::new("−1").build(ctx);
                            });
                        Button::new()
                            .style(ButtonStyle::Tonal)
                            .modifier(Modifier::new().test_tag("badge-inc"))
                            .on_click({
                                clone!(count);
                                move || count.update(|n| *n = n.saturating_add(1))
                            })
                            .build(ctx, |ctx| {
                                Text::new("+1").build(ctx);
                            });
                        Button::new()
                            .style(ButtonStyle::Text)
                            .modifier(Modifier::new().test_tag("badge-many"))
                            .on_click({
                                clone!(count);
                                move || count.set(120)
                            })
                            .build(ctx, |ctx| {
                                Text::new("120").build(ctx);
                            });
                    });

                Text::new(format!("count = {}", count.get()))
                    .font_size(12.0)
                    .color(theme.on_surface_variant)
                    .modifier(Modifier::new().test_tag("badge-count"))
                    .build(ctx);
            });
    });
}

/// The label colour is DERIVED from the container (`contentColorFor`), so a badge on a scheme
/// colour inherits that colour's paired on-colour instead of always being `onError`.
#[composable]
fn colour_card(ctx: &mut ComposeCtx) {
    let theme = WiniaTheme::colors();
    Card::new().build(ctx, |ctx| {
        Column::new()
            .spacing(16.0)
            .modifier(Modifier::new().fill_max_width().padding(16.0))
            .build(ctx, |ctx| {
                Text::new("The container colour picks the label colour")
                    .font_size(18.0)
                    .build(ctx);
                Text::new(
                    "Compose defaults `contentColor` to `contentColorFor(containerColor)`: a \
                     scheme colour maps to its paired on-colour, and anything else falls back to \
                     the ambient content colour. That is why the light-on-dark label flips here.",
                )
                .font_size(14.0)
                .build(ctx);

                FlowRow::new()
                    .main_spacing(28.0)
                    .cross_spacing(14.0)
                    .build(ctx, |ctx| {
                        container_sample(ctx, "error", theme.error);
                        container_sample(ctx, "primary", theme.primary);
                        container_sample(ctx, "tertiary", theme.tertiary);
                        container_sample(ctx, "surfaceContainerHigh", theme.surface_container_high);
                    });
            });
    });
}

/// One sample badge with a caption, in a fixed-height slot so the captions line up.
#[composable]
fn sample(
    ctx: &mut ComposeCtx,
    caption: &'static str,
    badge: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static,
) {
    let label_colour = WiniaTheme::colors().on_surface_variant;
    Column::new().spacing(8.0).build(ctx, |ctx| {
        // A fixed-height slot keeps the captions on one line: a 6dp dot and a 16dp badge are
        // different heights, and a bare `Column` would push each caption to its own offset.
        Stack::new()
            .alignment(Alignment::Center)
            .modifier(Modifier::new().height(24.0))
            .build(ctx, |ctx| badge(ctx));
        Text::new(caption)
            .font_size(11.0)
            .color(label_colour)
            .build(ctx);
    });
}

/// One badge on `colour`, captioned underneath — `sample` with the container pinned.
#[composable]
fn container_sample(ctx: &mut ComposeCtx, caption: &'static str, colour: Color) {
    sample(ctx, caption, move |ctx| {
        Badge::new()
            .container_color(colour)
            .content(|ctx| {
                Text::new("8").build(ctx);
            })
            .build(ctx);
    });
}

/// M3 caps a count badge at "99+".
fn count_text(n: u32) -> String {
    if n > 99 {
        "99+".to_string()
    } else {
        n.to_string()
    }
}
