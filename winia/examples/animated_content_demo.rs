//! AnimatedContent demo — a container that swaps one GENERATION of content for another.
//!
//! The thing that separates it from `Crossfade` is what is on screen mid-switch: both generations,
//! each with its own enter/exit, over a container whose size lerps from the old content's to the
//! new one's. The cards below take the three parts of that apart:
//!
//! - the **transition** (`enter`/`exit`) drives a generation's fade, slide and scale;
//! - the **size animation** decides how the frame travels between two content sizes, and `clip`
//!   decides whether a generation larger than the frame is cut off while it is up;
//! - **`content_key`** says when two targets are the SAME content — the state changes and nothing
//!   animates, which is how a caller avoids a transition for a no-op change.
//!
//! Try: step the segmented control and watch the outgoing page stay on screen while the incoming
//! one arrives; then change the size page from the short one to the tall one and watch the frame
//! grow under both. Turn `clip` off and the tall page paints outside the frame while it grows.
//!
//! Run with: `cargo run -p winia --example animated_content_demo`.

use letclone::clone;
use std::time::Duration;
use winia::animation::{AnimationSpec, TweenSpec, interpolator};
use winia::components::Chip;
use winia::prelude::*;

#[path = "common/settings.rs"]
mod settings;

pub fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(760.0, 1000.0)
                .title("AnimatedContent demo")
                .build(ctx, |ctx| {
                    settings::shell("AnimatedContent", ctx, demo_body);
                });
        });
    });
}

#[composable]
fn demo_body(ctx: &mut ComposeCtx, scaffold: ScaffoldContentPadding) {
    let page = ctx.remember(|| 0u32);
    let grouped = ctx.remember(|| 0u32);
    let tall = ctx.remember(|| 0u32);
    let clip = ctx.remember(|| true);
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
                "`AnimatedContent` keeps the OUTGOING content composed while the incoming one \
                 enters, so a switch is a real transition between two trees rather than a swap. \
                 The container's size follows the content as it goes.",
            )
            .font_size(14.0)
            .build(ctx);

            transition_card(ctx, page);
            content_key_card(ctx, grouped);
            size_card(ctx, tall.clone(), clip.clone());
            alignment_card(ctx);
        });
}

/// Three pages, each with its own label and colour: the basic enter/exit.
#[composable]
fn transition_card(ctx: &mut ComposeCtx, page: State<u32>) {
    const PAGES: [&str; 3] = ["first", "second", "third"];

    Card::new().build(ctx, |ctx| {
        Column::new()
            .spacing(12.0)
            .modifier(Modifier::new().fill_max_width().padding(16.0))
            .build(ctx, |ctx| {
                Text::new("Both generations are up")
                    .font_size(18.0)
                    .build(ctx);
                Text::new(
                    "The exit runs fast (90 ms) and the enter slower (220 ms) plus a scale from \
                     0.92, which is Compose's default pair. Mid-switch you can see the outgoing \
                     page behind the incoming one — that overlap is the whole point.",
                )
                .font_size(14.0)
                .build(ctx);
                // A fixed-height frame so the container's own size animation is not what moves
                // here; that is the next card's subject.
                Row::new()
                    .modifier(Modifier::new().fill_max_width().height(96.0))
                    .build(ctx, |ctx| {
                        AnimatedContent::new(page.clone()).build(ctx, |ctx, value| {
                            page_chip(ctx, value as usize, &PAGES);
                        });
                    });
                SingleChoiceSegmentedButtonRow::new().build(ctx, |ctx| {
                    let selected = page.get();
                    for (index, label) in PAGES.iter().enumerate() {
                        let index = index as u32;
                        SegmentedButton::new(selected == index, {
                            clone!(page);
                            move || page.set(index)
                        })
                        .shape(SegmentedButtonDefaults::item_shape(
                            index as usize,
                            PAGES.len(),
                        ))
                        .build(ctx, |ctx| {
                            Text::new(*label).font_size(12.0).build(ctx);
                        });
                    }
                });
            });
    });
}

/// One page of the basic card: three of the component's own variants, so the pages differ without
/// any local styling.
#[composable]
fn page_chip(ctx: &mut ComposeCtx, index: usize, labels: &'static [&'static str]) {
    let label = move |ctx: &mut ComposeCtx| {
        Text::new(labels[index]).build(ctx);
    };
    let chip = match index % 3 {
        0 => Chip::assist(label, || {}),
        1 => Chip::suggestion(label, || {}),
        _ => Chip::elevated_assist(label, || {}),
    };
    Row::new()
        .modifier(Modifier::new().padding(8.0))
        .build(ctx, |ctx| chip.build(ctx));
}

/// `content_key`: two targets that map to the same key are the SAME content, so nothing animates.
#[composable]
fn content_key_card(ctx: &mut ComposeCtx, grouped: State<u32>) {
    const STEPS: [&str; 4] = ["0", "1", "2", "3"];

    Card::new().build(ctx, |ctx| {
        Column::new()
            .spacing(12.0)
            .modifier(Modifier::new().fill_max_width().padding(16.0))
            .build(ctx, |ctx| {
                Text::new("content_key decides what counts as a switch")
                    .font_size(18.0)
                    .build(ctx);
                Text::new(
                    "Here the key is `value / 2`, so steps 0 and 1 are ONE content and 2 and 3 are \
                     another. Moving within a pair swaps the label with no transition at all; \
                     crossing a pair animates. Without the key, every value change is a switch.",
                )
                .font_size(14.0)
                .build(ctx);
                Row::new()
                    .modifier(Modifier::new().fill_max_width().height(80.0))
                    .build(ctx, |ctx| {
                        AnimatedContent::new(grouped.clone())
                            .content_key(|value: &u32| (*value / 2) as u64)
                            .build(ctx, |ctx, value| {
                                Chip::assist(
                                    move |ctx| {
                                        Text::new(format!("step {value}")).build(ctx);
                                    },
                                    || {},
                                )
                                .build(ctx);
                            });
                    });
                SingleChoiceSegmentedButtonRow::new().build(ctx, |ctx| {
                    let selected = grouped.get();
                    for (index, label) in STEPS.iter().enumerate() {
                        let index = index as u32;
                        SegmentedButton::new(selected == index, {
                            clone!(grouped);
                            move || grouped.set(index)
                        })
                        .shape(SegmentedButtonDefaults::item_shape(
                            index as usize,
                            STEPS.len(),
                        ))
                        .build(ctx, |ctx| {
                            Text::new(*label).font_size(12.0).build(ctx);
                        });
                    }
                });
            });
    });
}

/// The size animation plus `clip`: the frame travels between two content sizes.
#[composable]
fn size_card(ctx: &mut ComposeCtx, tall: State<u32>, clip: State<bool>) {
    Card::new().build(ctx, |ctx| {
        Column::new()
            .spacing(12.0)
            .modifier(Modifier::new().fill_max_width().padding(16.0))
            .build(ctx, |ctx| {
                Text::new("The frame follows the content")
                    .font_size(18.0)
                    .build(ctx);
                Text::new(
                    "The container lerps from the outgoing content's size to the incoming one's \
                     (a spring by default). `clip` is what stops a page that is already larger \
                     from painting outside the frame that is still growing — turn it off and the \
                     tall page spills out while it arrives.",
                )
                .font_size(14.0)
                .build(ctx);
                AnimatedContent::new(tall.clone())
                    .clip(clip.get())
                    .content_alignment(ContentAlignment::TOP_START)
                    .modifier(Modifier::new().fill_max_width().background(
                        WiniaTheme::colors().surface_container_high,
                        Shape::rounded(12.0),
                    ))
                    .build(ctx, |ctx, value| {
                        Column::new()
                            .spacing(6.0)
                            .modifier(Modifier::new().fill_max_width().padding(12.0))
                            .build(ctx, |ctx| {
                                Text::new(format!("page {value}"))
                                    .font_size(14.0)
                                    .build(ctx);
                                // Page 1 is deliberately three rows taller than page 0, so the
                                // container has something to travel.
                                for line in 1..=if value == 1 { 4 } else { 1 } {
                                    Text::new(format!("line {line}")).font_size(13.0).build(ctx);
                                }
                            });
                    });
                Row::new()
                    .spacing(12.0)
                    .modifier(Modifier::new().fill_max_width())
                    .build(ctx, |ctx| {
                        Button::new()
                            .style(ButtonStyle::Text)
                            .on_click({
                                clone!(tall);
                                move || tall.set(1 - tall.peek())
                            })
                            .build(ctx, |ctx| {
                                Text::new("Switch page").build(ctx);
                            });
                        Text::new(if clip.get() { "clip: on" } else { "clip: off" })
                            .font_size(13.0)
                            .build(ctx);
                        Switch::new(clip.get())
                            .on_checked_change({
                                clone!(clip);
                                move |checked| clip.set(checked)
                            })
                            .build(ctx, |_| {});
                    });
            });
    });
}

/// `content_alignment`: where a generation sits while the frame's size disagrees with its own.
#[composable]
fn alignment_card(ctx: &mut ComposeCtx) {
    let theme = WiniaTheme::colors();
    Card::new().build(ctx, |ctx| {
        Column::new()
            .spacing(12.0)
            .modifier(Modifier::new().fill_max_width().padding(16.0))
            .build(ctx, |ctx| {
                Text::new("Where each generation sits")
                    .font_size(18.0)
                    .build(ctx);
                Text::new(
                    "Both generations are composed in the same frame, which during a switch is a \
                     size neither of them asked for. `content_alignment` picks the corner they \
                     share: `TOP_START` is Compose's default and pins them to the top, `CENTER` \
                     keeps a small page in the middle of a frame that is briefly larger.",
                )
                .font_size(14.0)
                .build(ctx);
                // One content that changes size as it moves, so the frame and the content
                // disagree on every switch.
                let state = ctx.remember(|| 0u32);
                AnimatedContent::new(state.clone())
                    .content_alignment(ContentAlignment::CENTER)
                    .size_animation(AnimationSpec::Tween(TweenSpec::new(
                        Duration::from_millis(250),
                        interpolator::fast_out_slow_in(),
                    )))
                    .modifier(
                        Modifier::new()
                            .fill_max_width()
                            .background(theme.surface_container, Shape::rounded(12.0)),
                    )
                    .build(ctx, |ctx, value| {
                        Text::new(if value == 0 {
                            "small"
                        } else {
                            "a much wider label"
                        })
                        .font_size(14.0)
                        .modifier(Modifier::new().padding(12.0))
                        .build(ctx);
                    });
                Button::new()
                    .style(ButtonStyle::Text)
                    .on_click({
                        clone!(state);
                        move || state.set(1 - state.peek())
                    })
                    .build(ctx, |ctx| {
                        Text::new("Toggle width").build(ctx);
                    });
            });
    });
}
