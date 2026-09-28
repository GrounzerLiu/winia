//! DropdownMenu demo — the material3-aligned menus, and the knobs the component now exposes.
//!
//! Run: `cargo run -p winia --example dropdown_menu_demo --features debug-server`
//!
//! What to look at, in order:
//!
//! 1. **Shape and surface.** One panel per menu (`surfaceContainer`, 4dp corners, level-2 shadow), 8dp
//!    above and below the items, items 48dp tall with 12dp of horizontal padding and `labelLarge` text.
//!    The items themselves paint nothing — the container does (before this round every item was its own
//!    white 160×36 box).
//! 2. **The disabled item** is the third one: it neither fires nor dismisses.
//! 3. **The long menu** (30 items) is taller than the window: it is capped, pinned inside the window and
//!    SCROLLS — wheel over it, or drag it.
//! 4. **The bottom trigger** opens its menu UPWARD, because a menu below it would not fit. That is
//!    material3's `DropdownMenuPositionProvider` candidate order (below → above → pinned to the edge).
//! 5. **The styled menu** shows `offset`, `shape`, `container_color` and item `colors` — every one of
//!    them the material3 parameter of the same name.
use letclone::clone;
use winia::prelude::*;
// Shared example chrome: top app bar with the settings sheet (theme mode + layout direction).
#[path = "common/settings.rs"]
mod settings;
use winia::core::composer::ComposeCtx;
use winia::modifier::Shape;
use winia::ui::overlay::{DropdownMenu, DropdownMenuItem, MenuItemColors};
use winia::composable;

#[composable]
fn dropdown_menu_demo(ctx: &mut ComposeCtx) {
    let simple_open = ctx.remember(|| false);
    let long_open = ctx.remember(|| false);
    let bottom_open = ctx.remember(|| false);
    let styled_open = ctx.remember(|| false);
    let picked = ctx.remember(|| String::from("(none)"));

    Column::new()
        .spacing(16.0)
        .modifier(Modifier::new().padding(24.0))
        .build(ctx, {
            clone!(simple_open, long_open, bottom_open, styled_open, picked);
            let picked_text = picked.clone();
            move |ctx| {
                Text::new(format!("上次选择: {}", picked_text.get()))
                    .font_size(14.0)
                    .color(winia::modifier::Color::from_argb(255, 120, 120, 120))
                    .build(ctx);

                // ── 1. Plain menu: material3 defaults ──
                Button::text()
                    .modifier(Modifier::new().width(240.0))
                    .on_click({
                        clone!(simple_open);
                        move || {
                            let v = simple_open.get();
                            simple_open.set(!v);
                        }
                    })
                    .build(ctx, |ctx| Text::new("1. Plain menu").build(ctx));
                {
                    clone!(simple_open, picked);
                    let p_new = picked.clone();
                    let p_open = picked.clone();
                    DropdownMenu::new(simple_open.clone())
                        .on_dismiss_request({
                            clone!(simple_open);
                            move || simple_open.set(false)
                        })
                        .build(
                            ctx,
                            |ctx| {
                                Text::new("").font_size(1.0).build(ctx);
                            },
                            move |ctx| {
                                clone!(simple_open, picked);
                                DropdownMenuItem::new("新建文件")
                                    .on_click({
                                        clone!(simple_open, picked);
                                        move || {
                                            picked.set(String::from("新建文件"));
                                            simple_open.set(false);
                                        }
                                    })
                                    .build(ctx);
                                DropdownMenuItem::new("打开…")
                                    .on_click({
                                        clone!(simple_open, picked);
                                        move || {
                                            picked.set(String::from("打开…"));
                                            simple_open.set(false);
                                        }
                                    })
                                    .build(ctx);
                                // Disabled: not clickable at all, so it cannot dismiss either.
                                DropdownMenuItem::new("退出（禁用）")
                                    .enabled(false)
                                    .build(ctx);
                                let _ = (&p_new, &p_open);
                            },
                        );
                }

                // ── 2. Long menu: capped, pinned inside the window, scrollable ──
                Button::text()
                    .modifier(Modifier::new().width(240.0))
                    .on_click({
                        clone!(long_open);
                        move || {
                            let v = long_open.get();
                            long_open.set(!v);
                        }
                    })
                    .build(ctx, |ctx| Text::new("2. Long menu (30 items, scrolls)").build(ctx));
                DropdownMenu::new(long_open.clone())
                    .on_dismiss_request({
                        clone!(long_open);
                        move || long_open.set(false)
                    })
                    .build(
                        ctx,
                        |ctx| {
                            Text::new("").font_size(1.0).build(ctx);
                        },
                        {
                            clone!(long_open, picked);
                            move |ctx| {
                                for i in 0..30usize {
                                    clone!(long_open, picked);
                                    // Composed in a loop, so each item needs its own key: the call site is
                                    // the same line for all of them.
                                    ctx.key(("demo-item", i), |ctx| {
                                        DropdownMenuItem::new(format!("长项 {i}"))
                                            .on_click(move || {
                                                picked.set(format!("长项 {i}"));
                                                long_open.set(false);
                                            })
                                            .build(ctx);
                                    });
                                }
                            }
                        },
                    );

                // ── 3. Styled menu: the material3 knobs ──
                Button::text()
                    .modifier(Modifier::new().width(240.0))
                    .on_click({
                        clone!(styled_open);
                        move || {
                            let v = styled_open.get();
                            styled_open.set(!v);
                        }
                    })
                    .build(ctx, |ctx| Text::new("3. Styled (offset / shape / colors)").build(ctx));
                DropdownMenu::new(styled_open.clone())
                    .offset(0.0, 12.0)
                    .shape(Shape::RoundedRect { corner_radius: 16.0 })
                    .shadow_elevation(8.0)
                    .on_dismiss_request({
                        clone!(styled_open);
                        move || styled_open.set(false)
                    })
                    .build(
                        ctx,
                        |ctx| {
                            Text::new("").font_size(1.0).build(ctx);
                        },
                        {
                            clone!(styled_open, picked);
                            move |ctx| {
                                let accent = winia::modifier::Color::from_argb(255, 103, 80, 164);
                                // Disabled = the same role at `ListItemDisabled*Opacity` (0.38), the way
                                // every other component here does it.
                                let faded = |c: winia::modifier::Color| {
                                    winia::modifier::Color::from_argb((c.a as f32 * 0.38) as u8, c.r, c.g, c.b)
                                };
                                let colors = MenuItemColors {
                                    text: accent,
                                    leading_icon: accent,
                                    trailing_icon: accent,
                                    disabled_text: faded(accent),
                                    disabled_leading_icon: faded(accent),
                                    disabled_trailing_icon: faded(accent),
                                };
                                for label in ["选项 A", "选项 B"] {
                                    clone!(styled_open, picked);
                                    DropdownMenuItem::new(label)
                                        .colors(colors.clone())
                                        .content_padding(20.0, 6.0)
                                        .on_click(move || {
                                            picked.set(String::from(label));
                                            styled_open.set(false);
                                        })
                                        .build(ctx);
                                }
                            }
                        },
                    );

                // ── 4. Near the bottom: the menu opens UPWARD ──
                Spacer::vertical(220.0).build(ctx);
                Text::new("↓ this one has no room below it")
                    .font_size(12.0)
                    .color(winia::modifier::Color::from_argb(255, 150, 150, 150))
                    .build(ctx);
                Button::text()
                    .modifier(Modifier::new().width(240.0))
                    .on_click({
                        clone!(bottom_open);
                        move || {
                            let v = bottom_open.get();
                            bottom_open.set(!v);
                        }
                    })
                    .build(ctx, |ctx| Text::new("4. Menu from the bottom edge").build(ctx));
                DropdownMenu::new(bottom_open.clone())
                    .on_dismiss_request({
                        clone!(bottom_open);
                        move || bottom_open.set(false)
                    })
                    .build(
                        ctx,
                        |ctx| {
                            Text::new("").font_size(1.0).build(ctx);
                        },
                        {
                            clone!(bottom_open, picked);
                            move |ctx| {
                                for i in 0..8usize {
                                    clone!(bottom_open, picked);
                                    ctx.key(("demo-bottom-item", i), |ctx| {
                                        DropdownMenuItem::new(format!("底部项 {i}"))
                                            .on_click(move || {
                                                picked.set(format!("底部项 {i}"));
                                                bottom_open.set(false);
                                            })
                                            .build(ctx);
                                    });
                                }
                            }
                        },
                    );
            }
        });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(560.0, 720.0)
                .title("DropdownMenu Demo")
                .build(ctx, |ctx| {
                    settings::shell("DropdownMenu Demo", ctx, dropdown_menu_demo);
                });
        });
    });
}
