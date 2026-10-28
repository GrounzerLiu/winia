//! Text demo — every parameter Text resolves, with live controls for the ones that are worth
//! turning, plus the numbers the measure pass actually produced.
//!
//! The playground at the top drives one sample through font size, weight, slant, family, letter
//! spacing, line height, the line caps, alignment, overflow, wrapping and decoration. Below it every
//! API gets its own card, so nothing here is only reachable through the controls.
//!
//! Run with: cargo run -p winia --example text_demo

use letclone::clone;
use winia::prelude::*;
use winia::text::{measure_text, TextMeasureSpec};

#[path = "common/settings.rs"]
mod settings;

const LONG: &str = "the quick brown fox jumps over the lazy dog";
const SAMPLE: &str = "The quick brown fox jumps over the lazy dog, then keeps going for a while.";

const WEIGHTS: [(&str, FontWeight); 6] = [
    ("Light", FontWeight::LIGHT),
    ("Regular", FontWeight::NORMAL),
    ("Medium", FontWeight::MEDIUM),
    ("SemiBold", FontWeight::SEMI_BOLD),
    ("Bold", FontWeight::BOLD),
    ("Black", FontWeight::BLACK),
];

const ALIGNS: [(&str, TextAlign); 6] = [
    ("Left", TextAlign::Left),
    ("Center", TextAlign::Center),
    ("Right", TextAlign::Right),
    ("Justify", TextAlign::Justify),
    ("Start", TextAlign::Start),
    ("End", TextAlign::End),
];

const OVERFLOWS: [(&str, TextOverflow); 3] = [
    ("Clip", TextOverflow::Clip),
    ("Visible", TextOverflow::Visible),
    ("Ellipsis", TextOverflow::Ellipsis),
];

const DECORATIONS: [(&str, TextDecoration); 4] = [
    ("None", TextDecoration::NONE),
    ("Underline", TextDecoration::UNDERLINE),
    ("Strike", TextDecoration::LINE_THROUGH),
    ("Both", TextDecoration::UNDERLINE_AND_LINE_THROUGH),
];

const FAMILIES: [(&str, Option<&str>); 3] = [
    ("Default", None),
    ("Segoe UI", Some("Segoe UI")),
    ("Consolas", Some("Consolas")),
];

const CARD_BG: Color = Color { r: 0x2A, g: 0x27, b: 0x2E, a: 255 };
const MUTED: Color = Color { r: 0x9E, g: 0x9A, b: 0xA6, a: 255 };
const ACCENT: Color = Color { r: 0xD0, g: 0xBC, b: 0xFF, a: 255 };

/// Every live knob the playground drives. One State each, so a change only re-enters the card that
/// reads it — a single struct state would re-run the whole page on every slider tick.
struct Knobs {
    size: State<f32>,
    weight: State<usize>,
    slant: State<usize>,
    family: State<usize>,
    spacing: State<f32>,
    line_height: State<f32>,
    max_lines: State<f32>,
    min_lines: State<f32>,
    align: State<usize>,
    overflow: State<usize>,
    wrap: State<bool>,
    decoration: State<usize>,
    auto_size: State<bool>,
}

impl Knobs {
    fn remember(ctx: &mut ComposeCtx) -> Self {
        Knobs {
            size: ctx.remember(|| State::new(18.0)).get(),
            weight: ctx.remember(|| State::new(1usize)).get(),
            slant: ctx.remember(|| State::new(0usize)).get(),
            family: ctx.remember(|| State::new(0usize)).get(),
            spacing: ctx.remember(|| State::new(0.0)).get(),
            line_height: ctx.remember(|| State::new(0.0)).get(),
            max_lines: ctx.remember(|| State::new(0.0)).get(),
            min_lines: ctx.remember(|| State::new(1.0)).get(),
            align: ctx.remember(|| State::new(0usize)).get(),
            overflow: ctx.remember(|| State::new(0usize)).get(),
            wrap: ctx.remember(|| State::new(true)).get(),
            decoration: ctx.remember(|| State::new(0usize)).get(),
            auto_size: ctx.remember(|| State::new(false)).get(),
        }
    }

    /// The sample with every knob applied. The two line caps are clamped against each other, because
    /// Text validates that pair the way Compose validateMinMaxLines does and would panic otherwise.
    fn text(&self, content: &str) -> Text {
        let min = self.min_lines.get().round().max(1.0) as usize;
        let raw_max = self.max_lines.get().round().max(0.0) as usize;
        let max = if raw_max == 0 { usize::MAX } else { raw_max.max(min) };
        let mut text = Text::new(content)
            .font_size(self.size.get())
            .font_weight(WEIGHTS[self.weight.get().min(WEIGHTS.len() - 1)].1)
            .align(ALIGNS[self.align.get().min(ALIGNS.len() - 1)].1)
            .overflow(OVERFLOWS[self.overflow.get().min(OVERFLOWS.len() - 1)].1)
            .soft_wrap(self.wrap.get())
            .text_decoration(DECORATIONS[self.decoration.get().min(DECORATIONS.len() - 1)].1);
        match self.slant.get() {
            1 => text = text.italic(),
            2 => text = text.oblique(),
            _ => {}
        }
        if let Some(family) = FAMILIES[self.family.get().min(FAMILIES.len() - 1)].1 {
            text = text.font_family(family);
        }
        let spacing = self.spacing.get();
        if spacing.abs() > f32::EPSILON {
            text = text.letter_spacing(spacing);
        }
        let line_height = self.line_height.get();
        if line_height > 0.0 {
            text = text.line_height(line_height);
        }
        if max != usize::MAX {
            text = text.max_lines(max);
        }
        if min > 1 {
            text = text.min_lines(min);
        }
        text
    }
}

/// A card: a title line and a body, on the demo dark surface.
#[composable]
fn card(ctx: &mut ComposeCtx, tag: &str, title: &str, body: impl FnOnce(&mut ComposeCtx)) {
    Stack::new()
        .modifier(
            Modifier::new()
                .fill_max_width()
                .padding(16.0)
                .background(CARD_BG, Shape::rounded(12.0))
                .test_tag(format!("tx-{tag}-card")),
        )
        .build(ctx, |ctx| {
            Column::new()
                .spacing(10.0)
                .modifier(Modifier::new().fill_max_width())
                .build(ctx, |ctx| {
                    Text::new(title).font_size(12.0).color(MUTED).build(ctx);
                    body(ctx);
                });
        });
}

/// A label on the left, a control on the right.
#[composable]
fn labelled(ctx: &mut ComposeCtx, title: &str, control: impl FnOnce(&mut ComposeCtx)) {
    Row::new()
        .spacing(12.0)
        .modifier(Modifier::new().fill_max_width())
        .build(ctx, |ctx| {
            Stack::new()
                .modifier(Modifier::new().width(120.0))
                .build(ctx, |ctx| Text::new(title).font_size(12.0).color(MUTED).build(ctx));
            Stack::new()
                .modifier(Modifier::new().layout_weight(1.0))
                .build(ctx, control);
        });
}

#[composable]
fn slider(ctx: &mut ComposeCtx, state: State<f32>, min: f32, max: f32, tag: &str) {
    Slider::new(state.get())
        .value_range(min, max)
        .modifier(Modifier::new().test_tag(tag.to_string()))
        .on_value_change(move |value| state.set(value))
        .build(ctx);
}

#[composable]
fn switch(ctx: &mut ComposeCtx, state: State<bool>, tag: &str) {
    Switch::new(state.get())
        .modifier(Modifier::new().test_tag(tag.to_string()))
        .on_checked_change(move |checked| state.set(checked))
        .build(ctx, |_| {});
}

/// A single-choice row: one segment per option, tagged with the selection so a test can see it move.
///
/// `ui_text_test::a_segmented_pick_updates_the_playground` drives this at runtime: the pick flips the
/// tag and the playground summary follows on the same frame chain, so a segment click needs no other
/// event to take effect.
#[composable]
fn segmented(
    ctx: &mut ComposeCtx,
    tag: &'static str,
    title: &'static str,
    options: &'static [&'static str],
    state: State<usize>,
) {
    Row::new()
        .modifier(Modifier::new().fill_max_width())
        .spacing(8.0)
        .build(ctx, |ctx| {
            Text::new(title).font_size(11.0).color(MUTED).build(ctx);
            SingleChoiceSegmentedButtonRow::new().build(ctx, |ctx| {
                let selected = state.get();
                for (index, label) in options.iter().enumerate() {
                    clone!(state);
                    SegmentedButton::new(selected == index, move || state.set(index))
                        .shape(SegmentedButtonDefaults::item_shape(index, options.len()))
                        .modifier(Modifier::new().test_tag(format!(
                            "{tag}-{index}-{}",
                            if selected == index { "on" } else { "off" }
                        )))
                        .build(ctx, |ctx| Text::new(*label).font_size(10.0).build(ctx));
                }
            });
        });
}

#[composable]
fn text_demo(ctx: &mut ComposeCtx) {
    // Remembered: a fresh ScrollState per build is a new identity every recomposition, so the page
    // could never Skip and the offset would reset on every frame.
    let scroll = ctx.remember(|| ScrollState::new()).get();
    let k = Knobs::remember(ctx);
    // on_text_layout writes HERE rather than into a State: a state write during measure asks for a
    // recomposition mid-layout, which the materialiser rejects (the [dup-key] storm). A mutex plus a
    // refresh button shows the callback without that hazard.
    let layout_log: std::sync::Arc<std::sync::Mutex<Option<String>>> = ctx
        .remember(|| std::sync::Arc::new(std::sync::Mutex::new(None)))
        .get();
    let refresh = ctx.remember(|| State::new(0i32)).get();

    Column::new()
        .spacing(12.0)
        .modifier(
            Modifier::new()
                .fill_max_size()
                .padding(16.0)
                .vertical_scroll(scroll),
        )
        .build(ctx, |ctx| {
            // 1. Playground.
            card(ctx, "play", "live playground", |ctx| {
                Column::new().spacing(8.0).build(ctx, |ctx| {
                    k.text(SAMPLE)
                        .modifier(
                            Modifier::new()
                                .width(340.0)
                                .height(if k.wrap.get() { 120.0 } else { 32.0 })
                                .padding(6.0)
                                .background(Color::from_argb(24, 255, 255, 255), Shape::rounded(6.0))
                                .test_tag("tx-play-sample"),
                        )
                        .build(ctx);

                    labelled(ctx, "font size", |ctx| slider(ctx, k.size.clone(), 8.0, 40.0, "tx-play-size"));
                    labelled(ctx, "letter spacing", |ctx| slider(ctx, k.spacing.clone(), -1.0, 8.0, "tx-play-spacing"));
                    labelled(ctx, "line height", |ctx| slider(ctx, k.line_height.clone(), 0.0, 48.0, "tx-play-line-height"));
                    labelled(ctx, "max lines", |ctx| slider(ctx, k.max_lines.clone(), 0.0, 6.0, "tx-play-max-lines"));
                    labelled(ctx, "min lines", |ctx| slider(ctx, k.min_lines.clone(), 1.0, 6.0, "tx-play-min-lines"));

                    segmented(ctx, "tx-play-weight", "weight", &["Light", "Regular", "Medium", "SemiBold", "Bold", "Black"], k.weight.clone());
                    segmented(ctx, "tx-play-slant", "slant", &["Upright", "Italic", "Oblique"], k.slant.clone());
                    segmented(ctx, "tx-play-family", "family", &["Default", "Segoe UI", "Consolas"], k.family.clone());
                    segmented(ctx, "tx-play-align", "align", &["Left", "Center", "Right", "Justify", "Start", "End"], k.align.clone());
                    segmented(ctx, "tx-play-overflow", "overflow", &["Clip", "Visible", "Ellipsis"], k.overflow.clone());
                    segmented(ctx, "tx-play-decoration", "decoration", &["None", "Underline", "Strike", "Both"], k.decoration.clone());

                    labelled(ctx, "soft wrap", |ctx| {
                        Row::new().spacing(16.0).build(ctx, |ctx| {
                            switch(ctx, k.wrap.clone(), "tx-play-wrap");
                            Text::new(if k.wrap.get() { "wraps at the box width" } else { "one line, at its own width" })
                                .font_size(12.0)
                                .color(MUTED)
                                .build(ctx);
                        });
                    });

                    // What the CURRENT knobs measure, from the public measurer (the same numbers
                    // on_text_layout sees). Shown as text because the callback cannot write state.
                    let min = k.min_lines.get().round().max(1.0) as usize;
                    let raw_max = k.max_lines.get().round().max(0.0) as usize;
                    let spec = TextMeasureSpec {
                        content: SAMPLE,
                        font_size: k.size.get(),
                        color: Color::BLACK,
                        max_width: 340.0,
                        font_weight: WEIGHTS[k.weight.get().min(WEIGHTS.len() - 1)].1,
                        font_style: match k.slant.get() {
                            1 => FontSlant::Italic,
                            2 => FontSlant::Oblique,
                            _ => FontSlant::Upright,
                        },
                        max_lines: if raw_max == 0 { usize::MAX } else { raw_max.max(min) },
                        align: ALIGNS[k.align.get().min(ALIGNS.len() - 1)].1,
                        overflow: OVERFLOWS[k.overflow.get().min(OVERFLOWS.len() - 1)].1,
                        soft_wrap: k.wrap.get(),
                        letter_spacing: k.spacing.get(),
                        line_height: if k.line_height.get() > 0.0 { Some(k.line_height.get()) } else { None },
                        min_lines: min,
                        font_family: FAMILIES[k.family.get().min(FAMILIES.len() - 1)].1,
                    };
                    // A plain-text summary of the active knobs. The controls are visual, so this is
                    // what a reader (and a UI test) can quote back: if a pick stops taking effect,
                    // the summary stops matching the control.
                    Text::new(format!(
                        "size {:.0} · weight {} · slant {} · family {} · align {} · overflow {} · deco {} · wrap {}",
                        k.size.get(),
                        WEIGHTS[k.weight.get().min(WEIGHTS.len() - 1)].0,
                        ["Upright", "Italic", "Oblique"][k.slant.get().min(2)],
                        FAMILIES[k.family.get().min(FAMILIES.len() - 1)].0,
                        ALIGNS[k.align.get().min(ALIGNS.len() - 1)].0,
                        OVERFLOWS[k.overflow.get().min(OVERFLOWS.len() - 1)].0,
                        DECORATIONS[k.decoration.get().min(DECORATIONS.len() - 1)].0,
                        k.wrap.get(),
                    ))
                    .font_size(11.0)
                    .color(MUTED)
                    .modifier(Modifier::new().test_tag("tx-play-knobs"))
                    .build(ctx);

                    let measured = measure_text(&spec);
                    Text::new(format!(
                        "measured {:.0}x{:.0}  text {:.0}x{:.0}  lines {}  baseline {:.1}/{:.1}  overflow w:{} h:{}",
                        measured.size.width, measured.size.height,
                        measured.text_size.width, measured.text_size.height,
                        measured.line_count, measured.first_baseline, measured.last_baseline,
                        measured.did_overflow_width, measured.did_overflow_height,
                    ))
                    .font_size(11.0)
                    .color(ACCENT)
                    .modifier(Modifier::new().test_tag("tx-play-readout"))
                    .build(ctx);
                });
            });

            // 2. Type scale and weights.
            card(ctx, "scale", "type scale / weights", |ctx| {
                Row::new()
                    .spacing(12.0)
                    .modifier(Modifier::new().fill_max_width())
                    .build(ctx, |ctx| {
                        for (index, size) in [14.0, 18.0, 22.0].into_iter().enumerate() {
                            Text::new("Ag")
                                .font_size(size)
                                .modifier(Modifier::new().test_tag(format!("tx-scale-{index}")))
                                .build(ctx);
                        }
                    });
                Row::new()
                    .spacing(10.0)
                    .modifier(Modifier::new().fill_max_width())
                    .build(ctx, |ctx| {
                        for (label, weight) in WEIGHTS {
                            Text::new(label).font_size(13.0).font_weight(weight).build(ctx);
                        }
                    });
            });

            // 3. Letter spacing and line height.
            card(ctx, "spacing", "letter spacing / line height", |ctx| {
                Row::new()
                    .spacing(24.0)
                    .modifier(Modifier::new().fill_max_width())
                    .build(ctx, |ctx| {
                        Text::new("AAAA")
                            .font_size(18.0)
                            .modifier(Modifier::new().test_tag("tx-space-0"))
                            .build(ctx);
                        Text::new("AAAA")
                            .font_size(18.0)
                            .letter_spacing(4.0)
                            .modifier(Modifier::new().test_tag("tx-space-1"))
                            .build(ctx);
                        Text::new("line height 32")
                            .font_size(16.0)
                            .line_height(32.0)
                            .build(ctx);
                    });
            });

            // 4. Line caps.
            card(ctx, "lines", "max lines / min lines", |ctx| {
                Row::new()
                    .spacing(12.0)
                    .modifier(Modifier::new().fill_max_width())
                    .build(ctx, |ctx| {
                        for (index, lines) in [1usize, 3].into_iter().enumerate() {
                            Text::new(LONG)
                                .font_size(15.0)
                                .max_lines(lines)
                                .modifier(
                                    Modifier::new()
                                        .width(180.0)
                                        .test_tag(format!("tx-lines-{index}")),
                                )
                                .build(ctx);
                        }
                    });
                Row::new()
                    .spacing(12.0)
                    .modifier(Modifier::new().fill_max_width())
                    .build(ctx, |ctx| {
                        Text::new("one line")
                            .font_size(15.0)
                            .modifier(Modifier::new().test_tag("tx-one-line"))
                            .build(ctx);
                        Text::new("reserved 3")
                            .font_size(15.0)
                            .min_lines(3)
                            .modifier(Modifier::new().test_tag("tx-minlines"))
                            .build(ctx);
                    });
            });

            // 5. Overflow: the box is narrower than the string, on purpose. The three text nodes
            // are DIRECT siblings of the row (a per-item Column clipped their spill, which is what
            // `a_nowrap_ellipsis_stops_at_the_box_and_visible_does_not` measures).
            card(ctx, "nowrap", "overflow (nowrap, 140 dp box)", |ctx| {
                Row::new()
                    .spacing(24.0)
                    .modifier(Modifier::new().fill_max_width())
                    .build(ctx, |ctx| {
                        for (index, (_, overflow)) in [
                            ("Ellipsis", TextOverflow::Ellipsis),
                            ("Visible", TextOverflow::Visible),
                            ("Clip", TextOverflow::Clip),
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            Text::new(LONG)
                                .font_size(15.0)
                                .soft_wrap(false)
                                .max_lines(1)
                                .overflow(overflow)
                                .modifier(
                                    Modifier::new()
                                        .width(140.0)
                                        .background(Color::from_argb(28, 255, 255, 255), Shape::Rectangle)
                                        .test_tag(format!("tx-nowrap-{index}")),
                                )
                                .build(ctx);
                        }
                    });
                // A centred, nowrap, ellipsized line in a 300 dp box: the case where the alignment
                // slack was once computed from the PRE-ellipsis intrinsic width and shifted the line
                // out of its own box (`a_centred_nowrap_ellipsis_stays_inside_its_box`).
                Text::new(LONG)
                    .font_size(15.0)
                    .soft_wrap(false)
                    .max_lines(1)
                    .align(TextAlign::Center)
                    .overflow(TextOverflow::Ellipsis)
                    .modifier(
                        Modifier::new()
                            .width(300.0)
                            .background(Color::from_argb(28, 255, 255, 255), Shape::Rectangle)
                            .test_tag("tx-nowrap-center"),
                    )
                    .build(ctx);
            });
            // 6. Alignment.
            card(ctx, "align", "alignment (300 dp box)", |ctx| {
                for (index, (label, align)) in ALIGNS.into_iter().enumerate() {
                    Text::new(format!("{label}: aligned"))
                        .font_size(15.0)
                        .align(align)
                        .modifier(
                            Modifier::new()
                                .width(300.0)
                                .background(Color::from_argb(18, 255, 255, 255), Shape::Rectangle)
                                .test_tag(format!("tx-align-{index}")),
                        )
                        .build(ctx);
                }
            });

            // 7. Decoration and background.
            card(ctx, "deco", "decoration / background", |ctx| {
                for (label, decoration) in DECORATIONS {
                    Text::new(format!("{label} decoration"))
                        .font_size(15.0)
                        .text_decoration(decoration)
                        .build(ctx);
                }
                Text::new("highlighted")
                    .font_size(15.0)
                    .background(Color::from_argb(90, 0xD0, 0xBC, 0xFF), Shape::rounded(4.0))
                    .build(ctx);
            });

            // 8. Inheritance: ProvideTextStyle, and a child clearing a decoration.
            card(ctx, "inherit", "ProvideTextStyle inheritance", |ctx| {
                ProvideTextStyle(
                    TextStyle::new()
                        .font_size(20.0)
                        .font_weight(FontWeight::BOLD)
                        .text_decoration(TextDecoration::UNDERLINE),
                    ctx,
                    |ctx| {
                        Column::new().spacing(6.0).build(ctx, |ctx| {
                            Text::new("inherits 20 sp bold underline").build(ctx);
                            Text::new("overrides size only").font_size(14.0).build(ctx);
                            // The child says NONE, which must beat the inherited underline.
                            Text::new("clears the decoration")
                                .font_size(14.0)
                                .text_decoration(TextDecoration::NONE)
                                .modifier(Modifier::new().test_tag("tx-inherit-cleared"))
                                .build(ctx);
                        });
                    },
                );
            });

            // 9. autoSize.
            card(ctx, "auto", "autoSize", |ctx| {
                Row::new().spacing(12.0).build(ctx, |ctx| {
                    Text::new("autoSize shrinks until it fits")
                        .font_size(26.0)
                        .modifier(
                            Modifier::new()
                                .size(180.0, 56.0)
                                .background(Color::from_argb(24, 255, 255, 255), Shape::Rectangle),
                        )
                        .build(ctx);
                    let mut auto = Text::new("autoSize shrinks until it fits").font_size(26.0);
                    if k.auto_size.get() {
                        auto = auto.auto_size(8.0, 26.0);
                    }
                    auto.modifier(
                        Modifier::new()
                            .size(180.0, 56.0)
                            .background(Color::from_argb(24, 255, 255, 255), Shape::Rectangle),
                    )
                    .build(ctx);
                });
                labelled(ctx, "auto size", |ctx| switch(ctx, k.auto_size.clone(), "tx-auto-toggle"));
            });

            // 10. on_text_layout, without a state write during measure.
            card(ctx, "callback", "onTextLayout", |ctx| {
                let _ = refresh.get();
                Text::new("this leaf reports its layout")
                    .font_size(15.0)
                    .on_text_layout({
                        clone!(layout_log);
                        move |result| {
                            *layout_log.lock().unwrap() = Some(format!(
                                "{:.0}x{:.0}, {} line(s), baseline {:.1}/{:.1}, overflow w:{} h:{}",
                                result.size.width, result.size.height, result.line_count,
                                result.first_baseline, result.last_baseline,
                                result.did_overflow_width, result.did_overflow_height,
                            ));
                        }
                    })
                    .build(ctx);
                Text::new(
                    layout_log
                        .lock()
                        .unwrap()
                        .clone()
                        .unwrap_or_else(|| "(not measured yet)".to_string()),
                )
                .font_size(11.0)
                .color(ACCENT)
                .modifier(Modifier::new().test_tag("tx-callback-log"))
                .build(ctx);
                Button::filled()
                    .on_click({
                        clone!(refresh);
                        move || refresh.set(refresh.get() + 1)
                    })
                    .modifier(Modifier::new().test_tag("tx-callback-refresh"))
                    .build(ctx, |ctx| {
                        Text::new("read the log again").font_size(12.0).build(ctx);
                    });
            });
        });
}

pub fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(640.0, 940.0)
                .title("Text demo")
                .build(ctx, |ctx| {
                    settings::shell("Text", ctx, text_demo);
                });
        });
    });
}
