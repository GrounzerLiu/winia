//! A transparent window decoration that plays Android's status-bar role, window insets, and
//! material3's pull-to-refresh — built from the component library (Surface, Card, ListItem,
//! NavigationBar, Button, LoadingIndicator inside PullToRefreshBox).
//!
//! Windows has no status bar, so the app draws its own decoration (the Surface strip at the top) and
//! publishes its height as the status-bar inset. The page keeps clear of the bars with
//! status_bars_padding() / navigation_bars_padding(); the simulated keyboard publishes the IME inset,
//! which ime_padding() consumes. The list pulls to refresh with a real nested-scroll drag and the
//! indicator is the material3 LoadingIndicator (determinate while pulling, spinning while refreshing).
//!
//! Run with: cargo run --example insets_pull_refresh_demo

use std::time::Duration;

use winia::animation::interpolator::EaseOutCubic;
use winia::animation::{AnimationSpec, TweenSpec};
use winia::insets::Insets;
use winia::prelude::*;

const STATUS_BAR_HEIGHT: f32 = 36.0;
const NAV_BAR_HEIGHT: f32 = 80.0;
const KEYBOARD_HEIGHT: f32 = 200.0;
const REFRESH_MS: u64 = 5000;

#[composable]
fn insets_pull_refresh_demo(ctx: &mut ComposeCtx) {
    let keyboard = ctx.remember(|| false);
    let refreshing = ctx.remember(|| false);
    let refresh_count = ctx.remember(|| 0u32);
    let refresh_timer = ctx.remember(|| State::new(0.0f32)).get();
    let list_scroll = ctx.remember(|| ScrollState::new()).get();
    let tab = ctx.remember(|| 0usize);

    // This window owns its decoration, so it also owns the insets.
    winia::insets::set_status_bars(Insets::new(0.0, STATUS_BAR_HEIGHT, 0.0, 0.0));
    winia::insets::set_navigation_bars(Insets::new(0.0, 0.0, 0.0, NAV_BAR_HEIGHT));
    winia::insets::set_ime(if keyboard.get() {
        Insets::new(0.0, 0.0, 0.0, KEYBOARD_HEIGHT)
    } else {
        Insets::ZERO
    });

    let colors = WiniaTheme::colors();
    let on_refresh = {
        let r = refreshing.clone();
        let timer = refresh_timer.clone();
        let count = refresh_count.clone();
        move || {
            r.set(true);
            timer.set(0.0);
            let done_refreshing = r.clone();
            let done_count = count.clone();
            winia::animation::push_animatable_with_done(
                timer.clone(),
                1.0,
                AnimationSpec::Tween(TweenSpec::new(
                    Duration::from_millis(REFRESH_MS),
                    EaseOutCubic::new(),
                )),
                move || {
                    done_refreshing.set(false);
                    done_count.update(|value| *value += 1);
                },
            );
        }
    };

    Stack::new()
        .modifier(Modifier::new().fill_max_size())
        .content_alignment(ContentAlignment::TOP_CENTER)
        .build(ctx, move |ctx| {
            // ── the page: clear of the decoration, the navigation bar and the keyboard ──
            Column::new()
                .modifier(
                    Modifier::new()
                        .fill_max_size()
                        .status_bars_padding()
                        .navigation_bars_padding()
                        .ime_padding(),
                )
                .build(ctx, |ctx| {
                    Row::new()
                        .modifier(Modifier::new().fill_max_width().padding(12.0))
                        .build(ctx, |ctx| {
                            Text::new("Pull to refresh")
                                .font_size(18.0)
                                .color(colors.on_surface)
                                .build(ctx);
                            Spacer::horizontal(12.0).build(ctx);
                            let k = keyboard.clone();
                            Button::new()
                                .on_click(move || k.update(|value| *value = !*value))
                                .build(ctx, |ctx| {
                                    Text::new("keyboard").font_size(11.0).build(ctx);
                                });
                            Spacer::horizontal(8.0).build(ctx);
                            let refresh = on_refresh.clone();
                            Button::new()
                                .on_click(move || refresh())
                                .build(ctx, |ctx| {
                                    Text::new("refresh now").font_size(11.0).build(ctx);
                                });
                        });

                    Card::new()
                        // The card's weight gives it the remaining height; clip so the list (which
                        // measures taller than the card) cannot paint over the footer below it.
                        .modifier(
                            Modifier::new()
                                .layout_weight(1.0)
                                .fill_max_width()
                                .padding(12.0)
                                .clip(CardDefaults::shape()),
                        )
                        .build(ctx, |ctx| {
                            PullToRefreshBox::new(refreshing.get(), on_refresh.clone())
                                .modifier(Modifier::new().fill_max_size())
                                .build(ctx, move |ctx| {
                                    Column::new()
                                        .modifier(
                                            Modifier::new()
                                                .fill_max_size()
                                                .vertical_scroll(list_scroll),
                                        )
                                        .build(ctx, |ctx| {
                                            for i in 0..24 {
                                                ListItem::new(move |ctx| {
                                                    Text::new(format!("row {i}"))
                                                        .font_size(13.0)
                                                        .build(ctx);
                                                })
                                                .supporting_content(|ctx| {
                                                    Text::new("drag down to refresh")
                                                        .font_size(10.5)
                                                        .build(ctx);
                                                })
                                                .build(ctx);
                                            }
                                        });
                                });
                        });

                    Text::new(format!(
                        "refreshed {} time(s) — statusBars = 36 dp, navigationBars = 80 dp",
                        refresh_count.get()
                    ))
                    .font_size(10.5)
                    .color(colors.on_surface_variant)
                    .modifier(Modifier::new().padding(12.0))
                    .build(ctx);
                });

            // ── the custom decoration: an app-owned Surface that replaces the Windows title bar ──
            Surface::new()
                .color(colors.primary)
                .modifier(
                    Modifier::new()
                        .fill_max_width()
                        .height(STATUS_BAR_HEIGHT),
                )
                .build(ctx, |ctx| {
                    Text::new("winia · custom window decoration (the app's status bar)")
                        .font_size(12.0)
                        .color(colors.on_primary)
                        .modifier(Modifier::new().padding(10.0))
                        .build(ctx);
                });

            let (_, window_h) = window_size();

            // ── material3 NavigationBar: sized to its bar, absolutely placed at the bottom ──
            let bar_tab = tab.clone();
            NavigationBar::new(move |ctx| {
                for (index, label) in ["Home", "Library", "Settings"].iter().enumerate() {
                    let selected = bar_tab.get() == index;
                    let t = bar_tab.clone();
                    NavigationBarItem::new(selected, |ctx| {
                        Text::new("●").font_size(12.0).build(ctx);
                    })
                    .label(|ctx| {
                        Text::new(label.to_string()).font_size(10.0).build(ctx);
                    })
                    .on_click(move || t.set(index))
                    .build(ctx);
                }
            })
            .modifier(
                Modifier::new()
                    .fill_max_width()
                    .height(NAV_BAR_HEIGHT)
                    .absolute_offset(0.0, window_h - NAV_BAR_HEIGHT),
            )
            .build(ctx);

            // ── the keyboard: lives under the IME inset, so the page shrinks above it ──
            if keyboard.get() {
                Surface::new()
                    .color(Color::from_argb(255, 40, 40, 46))
                    .shape(Shape::rounded(12.0))
                    .modifier(
                        Modifier::new()
                            .fill_max_width()
                            .height(KEYBOARD_HEIGHT)
                            .absolute_offset(0.0, window_h - KEYBOARD_HEIGHT),
                    )
                    .build(ctx, |ctx| {
                        Text::new("simulated keyboard · ime = 200 dp")
                            .font_size(12.0)
                            .color(Color::WHITE)
                            .modifier(Modifier::new().padding(14.0))
                            .build(ctx);
                    });
            }
        });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(480.0, 820.0)
                .title("Insets & Pull-to-Refresh")
                .build(ctx, |ctx| {
                    insets_pull_refresh_demo(ctx);
                });
        });
    });
}
