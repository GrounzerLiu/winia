//! AnimatedSize demo — a container that animates its OWN size when the content resizes.
//!
//! Compose's `Modifier.animateContentSize`, and the cards below separate the three things that are
//! easy to conflate:
//!
//! - the animated size is the CONTAINER's. The content takes its new size at once and is clipped to
//!   the box while the box catches up, so a grow never paints outside the frame;
//! - `.modifier(..)` decorates that container, so a background or a border eases along with the
//!   animation. The content's own background does not;
//! - `content_alignment` decides where the content sits while the two sizes disagree — the box lags
//!   the content on every grow and leads it on every shrink, so the gap appears on one side or the
//!   other depending on the anchor.
//!
//! The last card is `finished_listener`: it reports `(start, target)` once a size animation ends,
//! which is the hook a caller uses to chain a step after the box has settled.
//!
//! Try: press `+ row` and watch the box, not the text, be what moves; slide the tween card's
//! duration and compare it with the spring default; switch the alignment while a grow is running.
//!
//! Run with: `cargo run -p winia --example animated_size_demo`.

use letclone::clone;
use std::time::Duration;
use winia::animation::{AnimationSpec, TweenSpec, interpolator};
use winia::prelude::*;

#[path = "common/settings.rs"]
mod settings;

pub fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(760.0, 980.0)
                .title("AnimatedSize demo")
                .build(ctx, |ctx| {
                    settings::shell("AnimatedSize", ctx, demo_body);
                });
        });
    });
}

#[composable]
fn demo_body(ctx: &mut ComposeCtx, scaffold: ScaffoldContentPadding) {
    let rows = ctx.remember(|| 2usize);
    let tween_ms = ctx.remember(|| 400.0f32);
    let from_top = ctx.remember(|| true);
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
            // No page heading: `settings::shell` already names the demo in the top app bar.
            Text::new(
                "`AnimatedSize` animates the CONTAINER's size, measured from its content. The \
                 content takes its new size immediately and is clipped while the box catches up — \
                 so what you watch move is the frame, not the glyphs.",
            )
            .font_size(14.0)
            .build(ctx);

            grow_card(ctx, rows.clone());
            decoration_card(ctx);
            alignment_card(ctx, from_top.clone());
            spec_card(ctx, tween_ms.clone());
            listener_card(ctx);
        });
}

/// Adding and removing rows: the box is what animates, and its neighbours follow.
#[composable]
fn grow_card(ctx: &mut ComposeCtx, rows: State<usize>) {
    Card::new().build(ctx, |ctx| {
        Column::new()
            .spacing(12.0)
            .modifier(Modifier::new().fill_max_width().padding(16.0))
            .build(ctx, |ctx| {
                Text::new("The container is what moves")
                    .font_size(18.0)
                    .build(ctx);
                Text::new(
                    "A `Column` of rows wrapped in `AnimatedSize`. Adding a row grows the content \
                     at once; the box springs to it, and the buttons below ride along.",
                )
                .font_size(14.0)
                .build(ctx);
                AnimatedSize::default()
                    .modifier(Modifier::new().fill_max_width())
                    .build(ctx, |ctx| {
                        Column::new()
                            .spacing(6.0)
                            .modifier(Modifier::new().fill_max_width())
                            .build(ctx, |ctx| {
                                for index in 1..=rows.get() {
                                    Text::new(format!("row {index}"))
                                        .font_size(14.0)
                                        .modifier(Modifier::new().fill_max_width().padding(8.0))
                                        .build(ctx);
                                }
                            });
                    });
                FlowRow::new()
                    .main_spacing(8.0)
                    .cross_spacing(8.0)
                    .build(ctx, |ctx| {
                        Button::new()
                            .style(ButtonStyle::Text)
                            .on_click({
                                clone!(rows);
                                move || rows.set((rows.get() + 1).min(8))
                            })
                            .build(ctx, |ctx| {
                                Text::new("+ row").build(ctx);
                            });
                        Button::new()
                            .style(ButtonStyle::Text)
                            .on_click({
                                clone!(rows);
                                move || rows.set(rows.get().saturating_sub(1).max(1))
                            })
                            .build(ctx, |ctx| {
                                Text::new("− row").build(ctx);
                            });
                    });
            });
    });
}

/// `.modifier` decorates the CONTAINER, so the decoration eases with the size animation.
#[composable]
fn decoration_card(ctx: &mut ComposeCtx) {
    let theme = WiniaTheme::colors();
    let wide = ctx.remember(|| false);
    Card::new().build(ctx, |ctx| {
        Column::new()
            .spacing(12.0)
            .modifier(Modifier::new().fill_max_width().padding(16.0))
            .build(ctx, |ctx| {
                Text::new("The decoration rides along")
                    .font_size(18.0)
                    .build(ctx);
                Text::new(
                    "The background and border here belong to the container (`.modifier(..)`), \
                     so they ease with it. A background on the CONTENT would instead jump to the \
                     new size on the frame it changes.",
                )
                .font_size(14.0)
                .build(ctx);
                Row::new()
                    .modifier(Modifier::new().fill_max_width())
                    .build(ctx, |ctx| {
                        AnimatedSize::default()
                            .modifier(
                                Modifier::new()
                                    .background(theme.secondary_container, Shape::rounded(12.0))
                                    .border(1.0, theme.outline, Shape::rounded(12.0)),
                            )
                            .build(ctx, |ctx| {
                                Text::new(if wide.get() {
                                    "a wider label than before"
                                } else {
                                    "short"
                                })
                                .font_size(14.0)
                                .color(theme.on_secondary_container)
                                .modifier(Modifier::new().padding_sides(12.0, 8.0, 12.0, 8.0))
                                .build(ctx);
                            });
                    });
                Button::new()
                    .style(ButtonStyle::Text)
                    .on_click({
                        clone!(wide);
                        move || wide.set(!wide.peek())
                    })
                    .build(ctx, |ctx| {
                        Text::new("Widen the label").build(ctx);
                    });
            });
    });
}

/// `content_alignment` decides which side the gap opens on while the box and the content differ.
#[composable]
fn alignment_card(ctx: &mut ComposeCtx, from_top: State<bool>) {
    Card::new().build(ctx, |ctx| {
        Column::new()
            .spacing(12.0)
            .modifier(Modifier::new().fill_max_width().padding(16.0))
            .build(ctx, |ctx| {
                Text::new("Where the content sits")
                    .font_size(18.0)
                    .build(ctx);
                Text::new(
                    "The box lags a grow and leads a shrink, so for those frames the content and \
                     its frame disagree. `content_alignment` picks the anchor: `TOP_START` pins \
                     the content to the top (Compose's default), `CENTER` keeps it in the middle \
                     and splits the difference.",
                )
                .font_size(14.0)
                .build(ctx);
                Row::new()
                    .modifier(Modifier::new().fill_max_width())
                    .spacing(12.0)
                    .build(ctx, |ctx| {
                        let alignment = if from_top.get() {
                            ContentAlignment::TOP_START
                        } else {
                            ContentAlignment::CENTER
                        };
                        AnimatedSize::default()
                            .content_alignment(alignment)
                            .modifier(Modifier::new().fill_max_width().background(
                                WiniaTheme::colors().surface_container_high,
                                Shape::rounded(12.0),
                            ))
                            .build(ctx, |ctx| {
                                Text::new("content anchored by content_alignment")
                                    .font_size(14.0)
                                    .modifier(Modifier::new().padding(12.0))
                                    .build(ctx);
                            });
                    });
                Row::new().spacing(8.0).build(ctx, |ctx| {
                    Text::new(if from_top.get() {
                        "content_alignment: TOP_START"
                    } else {
                        "content_alignment: CENTER"
                    })
                    .font_size(13.0)
                    .build(ctx);
                    Switch::new(from_top.get())
                        .on_checked_change({
                            clone!(from_top);
                            move |checked| from_top.set(checked)
                        })
                        .build(ctx, |_| {});
                });
            });
    });
}

/// The spec decides the feel: Compose's default is a spring, and a tween is a different motion.
#[composable]
fn spec_card(ctx: &mut ComposeCtx, tween_ms: State<f32>) {
    let theme = WiniaTheme::colors();
    Card::new().build(ctx, |ctx| {
        Column::new()
            .spacing(12.0)
            .modifier(Modifier::new().fill_max_width().padding(16.0))
            .build(ctx, |ctx| {
                Text::new("The spec is the feel").font_size(18.0).build(ctx);
                Text::new(
                    "`AnimatedSize::default()` is Compose's spring (`StiffnessMediumLow`, no \
                     bounce, a 1 px visibility threshold). `AnimatedSize::new(tween(..))` gives a \
                     fixed duration instead, which is the only way to make the motion predictable \
                     when several boxes have to land together.",
                )
                .font_size(14.0)
                .build(ctx);
                // The same row, once under the spring default and once under the tween, so the
                // difference is visible side by side rather than remembered.
                Row::new()
                    .spacing(12.0)
                    .modifier(Modifier::new().fill_max_width())
                    .build(ctx, |ctx| {
                        Column::new().spacing(4.0).build(ctx, |ctx| {
                            Text::new("spring (default)").font_size(12.0).build(ctx);
                            AnimatedSize::default()
                                .modifier(
                                    Modifier::new()
                                        .background(theme.primary_container, Shape::rounded(8.0)),
                                )
                                .build(ctx, |ctx| {
                                    size_sample(ctx, tween_ms.get());
                                });
                        });
                        Column::new().spacing(4.0).build(ctx, |ctx| {
                            Text::new("tween").font_size(12.0).build(ctx);
                            AnimatedSize::new(AnimationSpec::Tween(TweenSpec::new(
                                Duration::from_millis(tween_ms.get() as u64),
                                interpolator::fast_out_slow_in(),
                            )))
                            .modifier(
                                Modifier::new().background(
                                    theme.surface_container_highest,
                                    Shape::rounded(8.0),
                                ),
                            )
                            .build(ctx, |ctx| {
                                size_sample(ctx, tween_ms.get());
                            });
                        });
                    });
                Text::new(format!("tween duration: {:.0} ms", tween_ms.get()))
                    .font_size(13.0)
                    .build(ctx);
                Slider::new(tween_ms.get())
                    .value_range(80.0, 1200.0)
                    .on_value_change({
                        clone!(tween_ms);
                        move |value| tween_ms.set(value)
                    })
                    .build(ctx);
            });
    });
}

/// One sample box whose width follows the slider, shared by the two spec cards.
#[composable]
fn size_sample(ctx: &mut ComposeCtx, width: f32) {
    Column::new()
        .modifier(Modifier::new().width(width).padding(10.0))
        .build(ctx, |ctx| {
            Text::new("resize me").font_size(13.0).build(ctx);
        });
}

/// `finished_listener` reports `(start, target)` once a run ends — the hook for chaining a step.
#[composable]
fn listener_card(ctx: &mut ComposeCtx) {
    let theme = WiniaTheme::colors();
    let wide = ctx.remember(|| false);
    let last = ctx.remember(|| "no run yet".to_string());

    Card::new().build(ctx, |ctx| {
        Column::new()
            .spacing(12.0)
            .modifier(Modifier::new().fill_max_width().padding(16.0))
            .build(ctx, |ctx| {
                Text::new("finished_listener").font_size(18.0).build(ctx);
                Text::new(
                    "Called with the size the animation STARTED from and the one it settled on, \
                     once it finishes — not for the first measurement (that one snaps) and not for \
                     a target the animation abandoned. This is how a caller runs something after \
                     the box has actually stopped.",
                )
                .font_size(14.0)
                .build(ctx);
                AnimatedSize::default()
                    .modifier(
                        Modifier::new().background(theme.tertiary_container, Shape::rounded(12.0)),
                    )
                    .finished_listener({
                        clone!(last);
                        move |from: Size, to: Size| {
                            last.set(format!(
                                "{:.0}×{:.0} → {:.0}×{:.0}",
                                from.width, from.height, to.width, to.height
                            ));
                        }
                    })
                    .build(ctx, |ctx| {
                        Text::new(if wide.get() {
                            "a label of some width"
                        } else {
                            "wide"
                        })
                        .font_size(14.0)
                        .modifier(Modifier::new().padding(12.0))
                        .build(ctx);
                    });
                Row::new()
                    .spacing(12.0)
                    .modifier(Modifier::new().fill_max_width())
                    .build(ctx, |ctx| {
                        Button::new()
                            .style(ButtonStyle::Text)
                            .on_click({
                                clone!(wide);
                                move || wide.set(!wide.peek())
                            })
                            .build(ctx, |ctx| {
                                Text::new("Resize").build(ctx);
                            });
                        Text::new(format!("last run: {}", last.get()))
                            .font_size(13.0)
                            .build(ctx);
                    });
            });
    });
}
