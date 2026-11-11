//! AnimatedVisibility demo — the visibility container and the four channels a transition drives.
//!
//! A `VisibilityTransition` is not one animation. It carries a FADE, a SIZE, a SLIDE and a SCALE
//! channel, and each builder turns on the ones it names — so the cards below differ in WHAT they
//! move, not only in how long they take:
//!
//! - `fade_in` / `fade_out` touch the alpha alone, so the box keeps its space while its content
//!   dissolves;
//! - `expand_in` / `shrink_out` drive the SIZE channel, which runs through layout — the neighbours
//!   move as it grows and close up as it shrinks;
//! - `slide_in` / `slide_out` translate and `scale_in` / `scale_out` scale; both are paint-only;
//! - `with_slide` / `with_scale_from` / `with_expand_from` combine several channels on ONE
//!   transition, which is what most real uses want.
//!
//! The last card is the per-child form: `build_with_scope` hands the content a `VisibilityScope`,
//! and `animate_enter_exit` lets a child run its own curve on the container's visibility instead of
//! travelling with the box.
//!
//! Try: toggle each card and watch what actually moves. The fade card's neighbours never shift; the
//! size card's do. Then toggle the combined card, where a delayed fade rides a slide.
//!
//! Run with: `cargo run -p winia --example animated_visibility_demo`.

use letclone::clone;
use std::time::Duration;
use winia::animation::TweenSpec;
use winia::components::Chip;
use winia::prelude::*;

#[path = "common/settings.rs"]
mod settings;

pub fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(760.0, 980.0)
                .title("AnimatedVisibility demo")
                .build(ctx, |ctx| {
                    settings::shell("AnimatedVisibility", ctx, demo_body);
                });
        });
    });
}

#[composable]
fn demo_body(ctx: &mut ComposeCtx, scaffold: ScaffoldContentPadding) {
    // One switch per card, so a toggle shows exactly one transition rather than moving the page.
    let fade = ctx.remember(|| true);
    let slide = ctx.remember(|| true);
    let size = ctx.remember(|| true);
    let scale = ctx.remember(|| true);
    let combined = ctx.remember(|| true);
    let scoped = ctx.remember(|| true);
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
            // No page heading: `settings::shell` already puts this demo's name in the top app bar.
            Text::new(
                "A transition names which channels it drives. Fade and scale are paint-only; the \
                 size channel runs through layout, so it moves the neighbours. That difference is \
                 the whole reason this component takes a transition rather than a duration.",
            )
            .font_size(14.0)
            .build(ctx);

            fade_card(ctx, fade);
            slide_card(ctx, slide);
            size_card(ctx, size);
            scale_card(ctx, scale);
            combined_card(ctx, combined);
            scoped_card(ctx, scoped);
        });
}

/// A card with a title, a one-paragraph explanation, a fixed-height stage and its own toggle.
///
/// The stage keeps a constant height so a card whose content only fades does not appear to move:
/// the point of the fade card is that nothing else changes.
#[composable]
fn stage_card(
    ctx: &mut ComposeCtx,
    title: &'static str,
    body: &'static str,
    stage_height: f32,
    toggle: State<bool>,
    content: impl FnOnce(&mut ComposeCtx),
) {
    Card::new().build(ctx, |ctx| {
        Column::new()
            .spacing(12.0)
            .modifier(Modifier::new().fill_max_width().padding(16.0))
            .build(ctx, |ctx| {
                Text::new(title).font_size(18.0).build(ctx);
                Text::new(body).font_size(14.0).build(ctx);
                Row::new()
                    .modifier(Modifier::new().fill_max_width().height(stage_height))
                    .build(ctx, |ctx| content(ctx));
                Button::new()
                    .style(ButtonStyle::Text)
                    .on_click({
                        clone!(toggle);
                        move || toggle.set(!toggle.peek())
                    })
                    .build(ctx, |ctx| {
                        Text::new(if toggle.get() { "Hide" } else { "Show" }).build(ctx);
                    });
            });
    });
}

/// A chip that dissolves in place. Nothing else on the card moves — that is the fade channel.
#[composable]
fn fade_card(ctx: &mut ComposeCtx, visible: State<bool>) {
    stage_card(
        ctx,
        "Fade",
        "`fade_in` / `fade_out` drive the alpha channel only. The box keeps its space the whole \
         time, so its neighbours never shift — the content dissolves in place.",
        48.0,
        visible.clone(),
        move |ctx| {
            AnimatedVisibility::new(visible)
                .enter(VisibilityTransition::fade_in(TweenSpec::default()))
                .exit(VisibilityTransition::fade_out(TweenSpec::default()))
                .build(ctx, |ctx| {
                    Chip::assist(
                        |ctx| {
                            Text::new("alpha only").build(ctx);
                        },
                        || {},
                    )
                    .build(ctx);
                });
        },
    );
}

/// Four slides in one stage, each with its own direction.
#[composable]
fn slide_card(ctx: &mut ComposeCtx, visible: State<bool>) {
    stage_card(
        ctx,
        "Slide",
        "`slide_in` / `slide_out` translate the content by a fraction of its own extent. The \
         direction is part of the builder, and a slide on its own does not fade — the glyphs stay \
         fully opaque while they travel.",
        64.0,
        visible.clone(),
        move |ctx| {
            FlowRow::new()
                .main_spacing(16.0)
                .cross_spacing(8.0)
                .build(ctx, |ctx| {
                    for (direction, label, spec) in [
                        (SlideDirection::Left, "left", TweenSpec::default()),
                        (SlideDirection::Right, "right", TweenSpec::default()),
                        (SlideDirection::Up, "up", TweenSpec::default()),
                        (SlideDirection::Down, "down", TweenSpec::default()),
                    ] {
                        let v = visible.clone();
                        AnimatedVisibility::new(v)
                            .enter(VisibilityTransition::slide_in(direction, spec.clone()))
                            .exit(VisibilityTransition::slide_out(direction, spec))
                            .build(ctx, |ctx| {
                                Chip::assist(
                                    move |ctx| {
                                        Text::new(label).build(ctx);
                                    },
                                    || {},
                                )
                                .build(ctx);
                            });
                    }
                });
        },
    );
}

/// The size channel: this one moves layout, so the stage grows and the button below it moves.
#[composable]
fn size_card(ctx: &mut ComposeCtx, visible: State<bool>) {
    Card::new().build(ctx, |ctx| {
        Column::new()
            .spacing(12.0)
            .modifier(Modifier::new().fill_max_width().padding(16.0))
            .build(ctx, |ctx| {
                Text::new("Size").font_size(18.0).build(ctx);
                Text::new(
                    "`expand_in` / `shrink_out` drive the SIZE channel, the only one that runs \
                     through layout: the stage collapses to nothing and the button below it \
                     follows. A paint-only channel cannot do this, which is why the size channel \
                     is also the one a CHILD may not drive.",
                )
                .font_size(14.0)
                .build(ctx);
                AnimatedVisibility::new(visible.clone())
                    .enter(VisibilityTransition::expand_in(TweenSpec::default()))
                    .exit(VisibilityTransition::shrink_out(TweenSpec::default()))
                    .build(ctx, |ctx| {
                        Column::new()
                            .spacing(6.0)
                            .modifier(Modifier::new().fill_max_width().padding_vertical(12.0))
                            .build(ctx, |ctx| {
                                for line in 1..=3 {
                                    Text::new(format!("row {line} — laid out, then collapsed"))
                                        .font_size(14.0)
                                        .build(ctx);
                                }
                            });
                    });
                Button::new()
                    .style(ButtonStyle::Text)
                    .on_click({
                        clone!(visible);
                        move || visible.set(!visible.peek())
                    })
                    .build(ctx, |ctx| {
                        Text::new(if visible.get() { "Hide" } else { "Show" }).build(ctx);
                    });
            });
    });
}

/// The scale channel, anchored at the centre so it grows from where it sits.
#[composable]
fn scale_card(ctx: &mut ComposeCtx, visible: State<bool>) {
    stage_card(
        ctx,
        "Scale",
        "`scale_in` / `scale_out` scale about an anchor. `with_scale_from` picks the starting \
         factor and the transform origin together, so the box appears to grow out of a corner or \
         out of its middle.",
        72.0,
        visible.clone(),
        move |ctx| {
            FlowRow::new()
                .main_spacing(16.0)
                .cross_spacing(8.0)
                .build(ctx, |ctx| {
                    let centre = visible.clone();
                    AnimatedVisibility::new(centre)
                        .enter(
                            VisibilityTransition::fade_in(TweenSpec::default())
                                .with_scale_from(0.6, (0.5, 0.5)),
                        )
                        .exit(
                            VisibilityTransition::fade_out(TweenSpec::default())
                                .with_scale_from(0.6, (0.5, 0.5)),
                        )
                        .build(ctx, |ctx| {
                            Chip::assist(
                                |ctx| {
                                    Text::new("from centre").build(ctx);
                                },
                                || {},
                            )
                            .build(ctx);
                        });
                    let corner = visible.clone();
                    AnimatedVisibility::new(corner)
                        .enter(
                            VisibilityTransition::fade_in(TweenSpec::default())
                                .with_scale_from(0.6, (0.0, 0.0)),
                        )
                        .exit(
                            VisibilityTransition::fade_out(TweenSpec::default())
                                .with_scale_from(0.6, (0.0, 0.0)),
                        )
                        .build(ctx, |ctx| {
                            Chip::assist(
                                |ctx| {
                                    Text::new("from top-start").build(ctx);
                                },
                                || {},
                            )
                            .build(ctx);
                        });
                });
        },
    );
}

/// Three channels on one transition: a delayed fade riding a slide, with the size left alone.
#[composable]
fn combined_card(ctx: &mut ComposeCtx, visible: State<bool>) {
    stage_card(
        ctx,
        "Combined",
        "`with_slide` adds a channel to the one the constructor already set. Here the fade is \
         delayed so the box arrives before it finishes appearing, which reads as motion rather \
         than as a cross-fade.",
        64.0,
        visible.clone(),
        move |ctx| {
            let delayed = TweenSpec::default().delay(Duration::from_millis(120));
            AnimatedVisibility::new(visible)
                .enter(
                    VisibilityTransition::fade_in(delayed.clone()).with_slide(SlideDirection::Left),
                )
                .exit(
                    VisibilityTransition::fade_out(TweenSpec::default())
                        .with_slide(SlideDirection::Left),
                )
                .build(ctx, |ctx| {
                    Chip::assist(
                        |ctx| {
                            Text::new("slide + delayed fade").build(ctx);
                        },
                        || {},
                    )
                    .build(ctx);
                });
        },
    );
}

/// Per-child curves: the container drives one transition, each child runs its own.
#[composable]
fn scoped_card(ctx: &mut ComposeCtx, visible: State<bool>) {
    stage_card(
        ctx,
        "Per-child curves",
        "`build_with_scope` hands the content a `VisibilityScope`. `animate_enter_exit` gives a \
         child its own fade, slide or scale on the container's visibility, so the badge can arrive \
         late and the label can travel further than the box it sits in.",
        64.0,
        visible.clone(),
        move |ctx| {
            AnimatedVisibility::new(visible)
                .enter(VisibilityTransition::expand_in(TweenSpec::default()))
                .exit(VisibilityTransition::shrink_out(TweenSpec::default()))
                .build_with_scope(ctx, |scope, ctx| {
                    Row::new()
                        .spacing(8.0)
                        .modifier(Modifier::new().padding(8.0))
                        .build(ctx, |ctx| {
                            // The label travels with the container's size but fades on its own,
                            // later than the box appears.
                            let late = TweenSpec::default().delay(Duration::from_millis(150));
                            let label_mod = scope.animate_enter_exit(
                                ctx,
                                VisibilityTransition::fade_in(late.clone()),
                                VisibilityTransition::fade_out(TweenSpec::default()),
                                (200.0, 40.0),
                            );
                            Text::new("label")
                                .font_size(14.0)
                                .modifier(label_mod)
                                .build(ctx);
                            // The badge scales instead, from its own corner.
                            let badge_mod = scope.animate_enter_exit(
                                ctx,
                                VisibilityTransition::fade_in(TweenSpec::default())
                                    .with_scale_from(0.4, (0.0, 0.5)),
                                VisibilityTransition::fade_out(TweenSpec::default())
                                    .with_scale_from(0.4, (0.0, 0.5)),
                                (40.0, 40.0),
                            );
                            Text::new("★")
                                .font_size(16.0)
                                .modifier(badge_mod)
                                .build(ctx);
                        });
                });
        },
    );
}
