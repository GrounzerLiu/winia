//! Split button demo (material3 `SplitButtonLayout`, an M3 Expressive component).
//!
//! Shows the three things the component is about:
//! - the two halves act independently — the leading button runs the primary action, the trailing one
//!   owns a menu (here a real `DropdownMenu` anchored to the pair);
//! - the shape morphs: the inner corners grow on press, and the trailing button becomes a stadium with
//!   a state layer while its menu is open;
//! - the same component at every size tier and emphasis, because the geometry comes from the tokens.
//!
//! Run it with `cargo run --example split_button_demo`.

use letclone::clone;
use winia::prelude::*;
// The menu the trailing button opens (the same component the dropdown demo shows on its own).
use winia::ui::{DropdownMenu, DropdownMenuItem};

/// Material Icons "arrow_drop_down" (24 dp viewBox) — the menu trigger glyph.
const ARROW_DROP_DOWN_PATH: &str = "M7 10L12 15L17 10z";
/// Material Icons "add" (24 dp viewBox) — the leading button's icon in the sizes below.
const ADD_PATH: &str = "M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z";

/// A trailing menu button, shared by every split button on this screen.
fn trailing(
    open: State<bool>,
    size: ButtonSize,
) -> TrailingButton {
    TrailingButton::checked(open).size(size)
}

#[composable]
fn split_demo(ctx: &mut ComposeCtx) {
    let clicks = ctx.remember(|| 0usize);
    let picked = ctx.remember(|| String::from("nothing yet"));
    let menu_open = ctx.remember(|| false);
    let scroll_y = ctx.remember(|| ScrollState::new()).get();

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(24.0).vertical_scroll(scroll_y))
        .spacing(20.0)
        .build(ctx, |ctx| {
            Text::new("Split button").font_size(22.0).build(ctx);
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
                                            Icon::svg_path(ADD_PATH).size(20.0).build(ctx);
                                            Text::new("Add").build(ctx);
                                        });
                                });
                            },
                            |ctx| {
                                trailing(menu_open.clone(), ButtonSize::Small).build(ctx, |ctx| {
                                    Icon::svg_path(ARROW_DROP_DOWN_PATH)
                                        .size(SplitButtonDefaults::trailing_icon_size(ButtonSize::Small))
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
                                    Icon::svg_path(ARROW_DROP_DOWN_PATH)
                                        .size(SplitButtonDefaults::trailing_icon_size(ButtonSize::Small))
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
                                Icon::svg_path(ARROW_DROP_DOWN_PATH)
                                    .size(SplitButtonDefaults::trailing_icon_size(size))
                                    .build(ctx);
                            });
                    },
                );
            }
        });
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let _guard = rt.enter();
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(560.0, 720.0)
                .title("Split Button Demo")
                .build(ctx, |ctx| split_demo(ctx));
        });
    });
}
