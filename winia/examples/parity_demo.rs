//! Compose-parity modifiers, demonstrated end to end.
//!
//! Run with: cargo run --example parity_demo
//!
//! Sections, top to bottom:
//! 1. sizing ranges / wrap content / fill fraction / default min size
//! 2. onGloballyPositioned + layoutId
//! 3. pointerHoverIcon (hover a tile: hand / text / crosshair cursor)
//! 4. focusGroup + onFocusChanged + focusProperties(canFocus = false)
//! 5. clearAndSetSemantics + customActions
//! 6. overscroll / clipScrollableContainer / animateItem / bringIntoViewRequester
//! 7. drawWithCache / innerShadow / renderEffect

use std::sync::Arc;

use winia::graphics::{CompositingStrategy, GraphicsLayerParams, RenderEffect};
use winia::input::PointerIcon;
use winia::layout::{BringIntoViewRequester, ContentAlignment};
use winia::overscroll::{Orientation, OverscrollEffect};
use winia::prelude::*;

fn heading(ctx: &mut ComposeCtx, title: &str, note: &str) {
    let colors = WiniaTheme::colors();
    Text::new(title.to_string())
        .font_size(15.0)
        .color(colors.on_surface)
        .build(ctx);
    Text::new(note.to_string())
        .font_size(11.0)
        .color(colors.on_surface_variant)
        .build(ctx);
    Spacer::vertical(6.0).build(ctx);
}

fn boxed(ctx: &mut ComposeCtx, modifier: Modifier, body: impl FnOnce(&mut ComposeCtx)) {
    Column::new().modifier(modifier).build(ctx, body);
}

#[composable]
fn parity_demo(ctx: &mut ComposeCtx) {
    let page = ctx.remember(|| ScrollState::new()).get();
    let position = ctx.remember(|| (0.0f32, 0.0f32));
    let focus_name = ctx.remember(|| String::from("none"));
    let action_hits = ctx.remember(|| 0u32);

    let inner = ctx.remember(|| ScrollState::new()).get();
    let sideways = ctx.remember(|| ScrollState::new()).get();
    let overscroll = ctx.remember(|| OverscrollEffect::new()).get();
    let reveal = ctx.remember(|| BringIntoViewRequester::new()).get();
    let lazy_state = ctx.remember(|| LazyListState::new()).get();
    let items = ctx.remember(|| vec![0u64, 1, 2, 3, 4, 5, 6, 7]);

    let colors = WiniaTheme::colors();
    let accent = colors.primary;
    let surface = colors.surface_variant;
    let text = colors.on_surface;
    let subtext = colors.on_surface_variant;

    Column::new()
        .modifier(
            Modifier::new()
                .fill_max_size()
                .padding(20.0)
                .vertical_scroll(page),
        )
        .build(ctx, move |ctx| {
            Text::new("Modifier parity demo")
                .font_size(24.0)
                .color(text)
                .build(ctx);
            Text::new("The Compose modifier gaps closed in this pass. Scroll the page; every section is live.")
                .font_size(12.0)
                .color(subtext)
                .build(ctx);
            Spacer::vertical(16.0).build(ctx);

            // 1 ── sizing ────────────────────────────────────────────────────────────
            heading(
                ctx,
                "1. sizing ranges, wrap content, fill fraction",
                "width_in positions the box at its minimum; the inner square is centred by wrap_content_size; the bar takes half of its container; the label's floor is default_min_size.",
            );
            Row::new().build(ctx, |ctx| {
                boxed(
                    ctx,
                    Modifier::new()
                        .width_in(110.0, 180.0)
                        .height(52.0)
                        .background(accent, Shape::rounded(10.0)),
                    |ctx| {
                        Text::new("width_in\n110..180")
                            .font_size(10.5)
                            .color(colors.on_primary)
                            .build(ctx);
                    },
                );
                Spacer::horizontal(12.0).build(ctx);
                boxed(
                    ctx,
                    Modifier::new()
                        .size(150.0, 96.0)
                        .wrap_content_size(ContentAlignment::CENTER, false)
                        .background(surface, Shape::rounded(10.0)),
                    |ctx| {
                        boxed(
                            ctx,
                            Modifier::new()
                                .size(40.0, 40.0)
                                .background(accent, Shape::rounded(6.0)),
                            |_| {},
                        );
                    },
                );
                Spacer::horizontal(12.0).build(ctx);
                boxed(
                    ctx,
                    Modifier::new().width(170.0).height(96.0),
                    |ctx| {
                        Text::new("fill_max_width_fraction(0.5)")
                            .font_size(10.5)
                            .color(subtext)
                            .build(ctx);
                        boxed(
                            ctx,
                            Modifier::new()
                                .fill_max_width_fraction(0.5)
                                .height(22.0)
                                .background(accent, Shape::rounded(6.0)),
                            |_| {},
                        );
                    },
                );
            });
            Spacer::vertical(8.0).build(ctx);
            Text::new("default_min_size(120, 36) on a short label:")
                .font_size(10.5)
                .color(subtext)
                .build(ctx);
            boxed(
                ctx,
                Modifier::new()
                    .default_min_size(120.0, 36.0)
                    .background(surface, Shape::rounded(8.0)),
                |ctx| {
                    Text::new("hi")
                        .font_size(12.0)
                        .color(text)
                        .modifier(Modifier::new().padding(6.0))
                        .build(ctx);
                },
            );
            Spacer::vertical(14.0).build(ctx);

            // 2 ── position callbacks ───────────────────────────────────────────────
            heading(
                ctx,
                "2. onGloballyPositioned + layoutId",
                "The tracker reports its window box whenever it moves (scroll the page and watch the numbers change). The box also carries layoutId(\"tracker\").",
            );
            let tracker = position.clone();
            boxed(
                ctx,
                Modifier::new()
                    .size(190.0, 56.0)
                    .layout_id("tracker")
                    .background(accent, Shape::rounded(10.0))
                    .on_globally_positioned(move |p| {
                        tracker.set((p.x, p.y));
                    }),
                |ctx| {
                    Text::new("tracker")
                        .font_size(12.0)
                        .color(colors.on_primary)
                        .modifier(Modifier::new().padding(10.0))
                        .build(ctx);
                },
            );
            let (px, py) = position.get();
            Text::new(format!("global position: ({px:.0}, {py:.0})"))
                .font_size(11.0)
                .color(subtext)
                .build(ctx);
            Spacer::vertical(14.0).build(ctx);

            // 3 ── pointer icons ───────────────────────────────────────────────────
            heading(
                ctx,
                "3. pointerHoverIcon",
                "Hover each tile: hand, text and crosshair cursors come from the nearest pointer_hover_icon on the hit path.",
            );
            Row::new().build(ctx, |ctx| {
                for (icon, label) in [
                    (PointerIcon::HAND, "hand"),
                    (PointerIcon::TEXT, "text"),
                    (PointerIcon::CROSSHAIR, "crosshair"),
                ] {
                    boxed(
                        ctx,
                        Modifier::new()
                            .size(84.0, 64.0)
                            .background(surface, Shape::rounded(10.0))
                            .pointer_hover_icon(icon),
                        |ctx| {
                            Text::new(label.to_string())
                                .font_size(11.0)
                                .color(text)
                                .modifier(Modifier::new().padding(10.0))
                                .build(ctx);
                        },
                    );
                    Spacer::horizontal(10.0).build(ctx);
                }
            });
            Spacer::vertical(14.0).build(ctx);

            // 4 ── focus ───────────────────────────────────────────────────────────
            heading(
                ctx,
                "4. focusGroup, onFocusChanged, focusProperties",
                "Tab through the buttons: the group remembers its last child, the label follows on_focus_changed, and the third button (canFocus = false) is skipped.",
            );
            let group = Modifier::new().focus_group();
            Column::new().modifier(group).build(ctx, |ctx| {
                for (name, enabled) in [("Button A", true), ("Button B", true), ("skipped", false)] {
                    let slot = focus_name.clone();
                    let modifier = Modifier::new()
                        .focus_properties_can_focus(enabled)
                        .on_focus_changed(move |focused| {
                            if focused {
                                slot.set(name.to_string());
                            }
                        });
                    Button::new()
                        .on_click(|| {})
                        .modifier(modifier)
                        .build(ctx, |ctx| {
                            Text::new(name.to_string()).font_size(12.0).build(ctx);
                        });
                    Spacer::vertical(4.0).build(ctx);
                }
            });
            Text::new(format!("focused: {}", focus_name.get()))
                .font_size(11.0)
                .color(subtext)
                .build(ctx);
            Spacer::vertical(14.0).build(ctx);

            // 5 ── semantics ───────────────────────────────────────────────────────
            heading(
                ctx,
                "5. clearAndSetSemantics + customActions",
                "The block below is visual only: clear_and_set_semantics replaces its declaration and hides its subtree from the semantics tree. The button carries a named action, listed in the debug semantics tree (debug server: sem).",
            );
            boxed(
                ctx,
                Modifier::new()
                    .size(150.0, 44.0)
                    .background(surface, Shape::rounded(8.0))
                    .clear_and_set_semantics(
                        winia::semantics::SemanticsConfig::new()
                            .content_description("Decorative block"),
                    ),
                |ctx| {
                    Text::new("decorative")
                        .font_size(11.0)
                        .color(subtext)
                        .modifier(Modifier::new().padding(8.0))
                        .build(ctx);
                },
            );
            let hits = action_hits.clone();
            let fire: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
                hits.update(|v| *v += 1);
            });
            let f = fire.clone();
            let config = winia::semantics::SemanticsConfig::new().custom_action("Say hello", move || {
                f();
            });
            let f2 = fire.clone();
            Button::new()
                .on_click(move || f2())
                .modifier(Modifier::new().semantics(config))
                .build(ctx, |ctx| {
                    Text::new("fire the action").font_size(12.0).build(ctx);
                });
            Text::new(format!("custom action fired: {} time(s)", action_hits.get()))
                .font_size(11.0)
                .color(subtext)
                .build(ctx);
            Spacer::vertical(14.0).build(ctx);

            // 6 ── scroll extras ───────────────────────────────────────────────────
            heading(
                ctx,
                "6. overscroll, clipScrollableContainer, animateItem, bringIntoView",
                "Wheel/drag the first scroller past its end for the rubber band. The second scroller keeps 30dp of cross-axis room so its shadows survive. Insert an item to see placement animation; Reveal scrolls the third scroller to its target.",
            );
            Row::new().build(ctx, |ctx| {
                boxed(
                    ctx,
                    Modifier::new()
                        .size(300.0, 150.0)
                        .vertical_scroll(inner.clone())
                        .overscroll(&overscroll)
                        .background(surface, Shape::rounded(10.0)),
                    |ctx| {
                        for i in 0..12 {
                            Text::new(format!("overscroll row {i}"))
                                .font_size(12.0)
                                .color(text)
                                .modifier(Modifier::new().height(38.0).fill_max_width().padding(10.0))
                                .build(ctx);
                        }
                    },
                );
                Spacer::horizontal(12.0).build(ctx);
                boxed(
                    ctx,
                    Modifier::new()
                        .size(300.0, 150.0)
                        .horizontal_scroll(sideways.clone())
                        .clip_scrollable_container(Orientation::Horizontal),
                    |ctx| {
                        Row::new().build(ctx, |ctx| {
                            for i in 0..6 {
                                boxed(
                                    ctx,
                                    Modifier::new()
                                        .size(90.0, 90.0)
                                        .shadow(6.0, Shape::rounded(12.0), false, colors.on_surface)
                                        .background(accent, Shape::rounded(12.0)),
                                    |ctx| {
                                        Text::new(format!("{i}"))
                                            .font_size(12.0)
                                            .color(colors.on_primary)
                                            .modifier(Modifier::new().padding(8.0))
                                            .build(ctx);
                                    },
                                );
                                Spacer::horizontal(6.0).build(ctx);
                            }
                        });
                    },
                );
            });
            Spacer::vertical(10.0).build(ctx);

            Row::new().build(ctx, |ctx| {
                let list = items.clone();
                Button::new()
                    .on_click(move || {
                        list.update(|v| {
                            let next = v.iter().copied().max().unwrap_or(0) + 1;
                            let at = v.len() / 2;
                            v.insert(at, next);
                        });
                    })
                    .build(ctx, |ctx| {
                        Text::new("insert in the middle").font_size(12.0).build(ctx);
                    });
                Spacer::horizontal(8.0).build(ctx);
                let list = items.clone();
                Button::new()
                    .on_click(move || {
                        list.update(|v| {
                            let next = v.iter().copied().max().unwrap_or(0) + 1;
                            v.insert(0, next);
                        });
                    })
                    .build(ctx, |ctx| {
                        Text::new("insert at top").font_size(12.0).build(ctx);
                    });
            });
            Spacer::vertical(8.0).build(ctx);

            let current = items.get();
            let list_items: Arc<Vec<u64>> = Arc::new(current.clone());
            LazyColumn::new()
                .state(lazy_state.clone())
                .modifier(Modifier::new().size(300.0, 200.0))
                .items_from(list_items, |v: &u64| *v, move |ctx, _i, v| {
                    Text::new(format!("animated item #{v}"))
                        .font_size(12.0)
                        .color(text)
                        .modifier(
                            Modifier::new()
                                .animate_item()
                                .height(40.0)
                                .fill_max_width()
                                .background(surface, Shape::rounded(6.0))
                                .padding(10.0),
                        )
                        .build(ctx);
                })
                .build(ctx);
            Spacer::vertical(8.0).build(ctx);

            let scroller = ctx.remember(|| ScrollState::new()).get();
            let target = reveal.clone();
            Button::new()
                .on_click(move || target.bring_into_view())
                .build(ctx, |ctx| {
                    Text::new("reveal the target").font_size(12.0).build(ctx);
                });
            Spacer::vertical(6.0).build(ctx);
            boxed(
                ctx,
                Modifier::new()
                    .size(300.0, 140.0)
                    .vertical_scroll(scroller)
                    .background(surface, Shape::rounded(10.0)),
                |ctx| {
                    Text::new("scroll me: the target is 360 px down")
                        .font_size(11.0)
                        .color(subtext)
                        .modifier(Modifier::new().padding(10.0))
                        .build(ctx);
                    Spacer::vertical(320.0).build(ctx);
                    boxed(
                        ctx,
                        Modifier::new()
                            .height(48.0)
                            .fill_max_width()
                            .background(accent, Shape::rounded(8.0))
                            .bring_into_view_requester(&reveal),
                        |ctx| {
                            Text::new("the target")
                                .font_size(12.0)
                                .color(colors.on_primary)
                                .modifier(Modifier::new().padding(10.0))
                                .build(ctx);
                        },
                    );
                    Spacer::vertical(360.0).build(ctx);
                },
            );
            Spacer::vertical(14.0).build(ctx);

            // 7 ── draw and layer ─────────────────────────────────────────────────
            heading(
                ctx,
                "7. drawWithCache, innerShadow, renderEffect",
                "The stripe panel's path is built once per size then reused every paint; the card has an inner shadow; the blurred card composites its whole layer through a Gaussian image filter.",
            );
            Row::new().build(ctx, |ctx| {
                boxed(
                    ctx,
                    Modifier::new()
                        .size(150.0, 90.0)
                        .background(surface, Shape::rounded(10.0))
                        .draw_with_cache(
                            |scope| {
                                // The cache holds plain geometry (Skia handles are not Send+Sync).
                                let mut stripes = Vec::new();
                                let mut x = 0.0;
                                while x < scope.width {
                                    stripes.push(skia_safe::Rect::from_xywh(
                                        x,
                                        0.0,
                                        6.0,
                                        scope.height,
                                    ));
                                    x += 12.0;
                                }
                                stripes
                            },
                            |stripes, canvas, rect| {
                                // The cached geometry is in node-local coordinates; place it.
                                canvas.save();
                                canvas.translate((rect.x(), rect.y()));
                                canvas.clip_rrect(
                                    skia_safe::RRect::new_rect_xy(
                                        skia_safe::Rect::from_xywh(0.0, 0.0, rect.width(), rect.height()),
                                        10.0,
                                        10.0,
                                    ),
                                    None,
                                    Some(true),
                                );
                                let mut paint = skia_safe::Paint::default();
                                paint.set_anti_alias(true);
                                paint.set_color(skia_safe::Color::from_rgb(0x67, 0x50, 0xA4));
                                for stripe in stripes {
                                    canvas.draw_rect(*stripe, &paint);
                                }
                                canvas.restore();
                            },
                        ),
                    |_| {},
                );
                Spacer::horizontal(12.0).build(ctx);
                let inner_shadow_shape = Shape::rounded(14.0);
                boxed(
                    ctx,
                    Modifier::new()
                        .size(170.0, 90.0)
                        .inner_shadow(
                            inner_shadow_shape,
                            Color::from_argb(190, 0, 0, 0),
                            10.0,
                            0.0,
                            6.0,
                        )
                        .background(surface, inner_shadow_shape),
                    |ctx| {
                        Text::new("inner shadow")
                            .font_size(12.0)
                            .color(text)
                            .modifier(Modifier::new().padding(12.0))
                            .build(ctx);
                    },
                );
                Spacer::horizontal(12.0).build(ctx);
                boxed(
                    ctx,
                    Modifier::new()
                        .size(170.0, 90.0)
                        .graphics_layer(|| GraphicsLayerParams {
                            render_effect: Some(RenderEffect::blur(4.0)),
                            compositing: CompositingStrategy::Auto,
                            ..Default::default()
                        })
                        .background(surface, Shape::rounded(14.0)),
                    |ctx| {
                        Text::new("blurred layer")
                            .font_size(14.0)
                            .color(text)
                            .modifier(Modifier::new().padding(14.0))
                            .build(ctx);
                    },
                );
            });
            Spacer::vertical(24.0).build(ctx);
        });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(980.0, 820.0)
                .title("Modifier Parity Demo")
                .build(ctx, |ctx| {
                    parity_demo(ctx);
                });
        });
    });
}
