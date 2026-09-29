//! Split button demo — material3's `SplitButtonLayout`, an M3 Expressive component.
//!
//! Run: `cargo run -p winia --example split_button_demo --features debug-server`
//!
//! What to look at, in order:
//!
//! 1. **The two halves act independently.** The leading button runs the primary action and counts its
//!    clicks; the trailing one owns a real `DropdownMenu` anchored to the pair and reports the item it
//!    picked. Neither half needs the other's state.
//! 2. **The shape morphs.** Press either half and its inner corners grow (4 → 12 dp on the small tier);
//!    open the menu and the trailing half becomes a stadium with a 10 % state layer painted over its
//!    glyph, and its icon slides to the centre.
//! 3. **Emphasis levels.** Filled (default), tonal, elevated and outlined — material3 passes its Button
//!    defaults in, and so does winia.
//! 4. **Every size tier** material3 defines: heights, paddings, inner corners and the trailing icon all
//!    come from `SplitButton*Tokens`.
//! 5. **Direction.** The settings sheet in the top bar flips the layout: the halves mirror around the one
//!    gap between them, and the content's optical offset follows the gap in either direction.
//!
//! The glyphs are SVG path data from <https://fonts.google.com/icons> — `add` below, and the trailing
//! trigger through `ExposedDropdownMenuDefaults::ARROW_DROP_DOWN_PATH`, the same 24 dp asset material3
//! passes as `Icons.Filled.ArrowDropDown`, so the arrow exists once in the tree instead of as a copy here.

use letclone::clone;
use winia::prelude::*;
// Shared example chrome: top app bar with the settings sheet (theme mode + layout direction).
#[path = "common/settings.rs"]
mod settings;
// The menu the trailing button opens (the same component the dropdown demo shows on its own), and the
// published path data of the trigger glyph.
use winia::ui::overlay::ExposedDropdownMenuDefaults;
use winia::ui::{DropdownMenu, DropdownMenuItem};

/// Material Icons `add` (24 dp, filled), from <https://fonts.google.com/icons>: the leading button's
/// action glyph, at the size `ButtonSmallTokens.IconSize` names for the leading icon.
const ADD_ICON: &str = "M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z";

fn add_icon() -> Icon {
    Icon::svg_path(ADD_ICON)
}

fn menu_icon(size: f32) -> Icon {
    Icon::svg_path(ExposedDropdownMenuDefaults::ARROW_DROP_DOWN_PATH).size(size)
}

/// A trailing menu button, shared by every split button on this screen.
fn trailing(open: State<bool>, size: ButtonSize) -> TrailingButton {
    TrailingButton::checked(open).size(size)
}

#[composable]
fn split_button_demo(ctx: &mut ComposeCtx) {
    let clicks = ctx.remember(|| 0usize);
    let picked = ctx.remember(|| String::from("nothing yet"));
    let menu_open = ctx.remember(|| false);
    let scroll_y = ctx.remember(|| ScrollState::new()).get();

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(24.0).vertical_scroll(scroll_y))
        .spacing(20.0)
        .build(ctx, |ctx| {
            Text::new(format!("primary action ran {} times", clicks.get())).build(ctx);
            Text::new(format!("menu picked: {}", picked.get())).build(ctx);

            // The primary pattern: a leading button plus a trailing menu, anchored to the pair.
            DropdownMenu::new(menu_open.clone())
                .on_dismiss_request({
                    clone!(menu_open);
                    move || menu_open.set(false)
                })
                .build(
                    ctx,
                    |ctx| {
                        SplitButtonLayout::new().build(
                            ctx,
                            |ctx| {
                                SplitButtonDefaults::leading_button({
                                    clone!(clicks);
                                    move || clicks.update(|value| *value += 1)
                                })
                                .build(ctx, |ctx| {
                                    Row::new()
                                        .spacing(ButtonSize::Small.icon_label_space())
                                        .build(ctx, |ctx| {
                                            add_icon()
                                                .size(SplitButtonDefaults::leading_icon_size(
                                                    ButtonSize::Small,
                                                ))
                                                .build(ctx);
                                            Text::new("Add").build(ctx);
                                        });
                                });
                            },
                            |ctx| {
                                trailing(menu_open.clone(), ButtonSize::Small).build(ctx, |ctx| {
                                    menu_icon(SplitButtonDefaults::trailing_icon_size(ButtonSize::Small))
                                        .build(ctx);
                                });
                            },
                        );
                    },
                    {
                        let picked = picked.clone();
                        let menu_open = menu_open.clone();
                        move |ctx| {
                            for label in ["Save as draft", "Save and publish", "Save a copy"] {
                                DropdownMenuItem::new(label)
                                    .on_click({
                                        clone!(picked, menu_open);
                                        move || {
                                            picked.set(String::from(label));
                                            menu_open.set(false);
                                        }
                                    })
                                    .build(ctx);
                            }
                        }
                    },
                );

            // Emphasis levels: filled (default), tonal, elevated, outlined — material3 passes its
            // Button defaults in, and so does winia.
            Text::new("Emphasis").build(ctx);
            Row::new().spacing(12.0).build(ctx, |ctx| {
                for (style, name) in [
                    (ButtonStyle::Filled, "Filled"),
                    (ButtonStyle::Tonal, "Tonal"),
                    (ButtonStyle::Elevated, "Elevated"),
                    (ButtonStyle::Outlined, "Outlined"),
                ] {
                    SplitButtonLayout::new().build(
                        ctx,
                        |ctx| {
                            SplitButtonDefaults::leading_button(|| {})
                                .style(style)
                                .build(ctx, |ctx| {
                                    Text::new(name).build(ctx);
                                });
                        },
                        |ctx| {
                            SplitButtonDefaults::trailing_button()
                                .style(style)
                                .on_click(|| {})
                                .build(ctx, |ctx| {
                                    menu_icon(SplitButtonDefaults::trailing_icon_size(ButtonSize::Small))
                                        .build(ctx);
                                });
                        },
                    );
                }
            });

            // Every size tier material3 defines: the heights, the paddings, the inner corners and the
            // trailing icon all come from `SplitButton*Tokens`.
            Text::new("Sizes").build(ctx);
            for size in [
                ButtonSize::XSmall,
                ButtonSize::Small,
                ButtonSize::Medium,
                ButtonSize::Large,
                ButtonSize::XLarge,
            ] {
                SplitButtonLayout::new().build(
                    ctx,
                    |ctx| {
                        SplitButtonDefaults::leading_button(|| {})
                            .size(size)
                            .build(ctx, |ctx| {
                                Text::new(format!("{size:?}")).build(ctx);
                            });
                    },
                    |ctx| {
                        SplitButtonDefaults::trailing_button()
                            .size(size)
                            .on_click(|| {})
                            .build(ctx, |ctx| {
                                menu_icon(SplitButtonDefaults::trailing_icon_size(size)).build(ctx);
                            });
                    },
                );
            }
        });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(560.0, 860.0)
                .title("Split Button Demo")
                .build(ctx, |ctx| {
                    settings::shell("Split Button Demo", ctx, split_button_demo);
                });
        });
    });
}
