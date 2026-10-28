//! RichText demo — the nested-scope API, every span style it carries, inline images and
//! placeholders, the paragraph parameters, and selection.
//!
//! The playground at the top applies the live knobs to one rich paragraph; the cards below show each
//! scope on its own so nothing is only reachable through a control.
//!
//! Run with: cargo run -p winia --example rich_text_demo

use letclone::clone;
use winia::prelude::*;
use winia::text::{AnnotatedString, DecoStyle, SpanStyle, SvgDrawable, TextAlign, TextOverflow};
use winia::components::RichTextScope;

#[path = "common/settings.rs"]
mod settings;

const SAMPLE: &str = "Rich text can mix weights, sizes, families and decorations inside one paragraph.";

/// A small star, so the inline-image cards do not need a file.
const STAR: &str = "<svg xmlns='http://www.w3.org/2000/svg' width='16' height='16' viewBox='0 0 16 16'><path d='M8 1l2.1 4.6 5 .5-3.8 3.3 1.1 4.9L8 11.7 3.6 14.3l1.1-4.9L1 6.1l5-.5z' fill='#D0BCFF'/></svg>";

const CARD_BG: Color = Color { r: 0x2A, g: 0x27, b: 0x2E, a: 255 };
const MUTED: Color = Color { r: 0x9E, g: 0x9A, b: 0xA6, a: 255 };
const ACCENT: Color = Color { r: 0xD0, g: 0xBC, b: 0xFF, a: 255 };
const WARM: Color = Color { r: 0xFF, g: 0xB4, b: 0xAB, a: 255 };

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

struct Knobs {
    size: State<f32>,
    spacing: State<f32>,
    word_spacing: State<f32>,
    height_multiple: State<f32>,
    bold: State<bool>,
    italic: State<bool>,
    underline: State<bool>,
    strike: State<bool>,
    overline: State<bool>,
    wrap: State<bool>,
    align: State<usize>,
    overflow: State<usize>,
    max_lines: State<f32>,
    min_lines: State<f32>,
    line_height: State<f32>,
}

impl Knobs {
    fn remember(ctx: &mut ComposeCtx) -> Self {
        Knobs {
            size: ctx.remember(|| State::new(16.0)).get(),
            spacing: ctx.remember(|| State::new(0.0)).get(),
            word_spacing: ctx.remember(|| State::new(0.0)).get(),
            height_multiple: ctx.remember(|| State::new(0.0)).get(),
            bold: ctx.remember(|| State::new(false)).get(),
            italic: ctx.remember(|| State::new(false)).get(),
            underline: ctx.remember(|| State::new(false)).get(),
            strike: ctx.remember(|| State::new(false)).get(),
            overline: ctx.remember(|| State::new(false)).get(),
            wrap: ctx.remember(|| State::new(true)).get(),
            align: ctx.remember(|| State::new(0usize)).get(),
            overflow: ctx.remember(|| State::new(0usize)).get(),
            max_lines: ctx.remember(|| State::new(0.0)).get(),
            min_lines: ctx.remember(|| State::new(1.0)).get(),
            line_height: ctx.remember(|| State::new(0.0)).get(),
        }
    }

    /// Apply the live knobs to a rich block. `RichText` takes the same paragraph parameters `Text`
    /// does (they were added for T-3), so the playground drives all of them.
    fn apply(&self, mut rich: RichText) -> RichText {
        let min = self.min_lines.get().round().max(1.0) as usize;
        let raw_max = self.max_lines.get().round().max(0.0) as usize;
        rich = rich
            .soft_wrap(self.wrap.get())
            .align(ALIGNS[self.align.get().min(ALIGNS.len() - 1)].1)
            .overflow(OVERFLOWS[self.overflow.get().min(OVERFLOWS.len() - 1)].1)
            .max_lines(if raw_max == 0 { usize::MAX } else { raw_max.max(min) })
            .min_lines(min);
        if self.line_height.get() > 0.0 {
            rich = rich.line_height(self.line_height.get());
        }
        rich
    }

    fn body(&self, x: &mut RichTextScope) {
        x.text("plain ");
        if self.bold.get() {
            x.bold(|x| x.text("bold "));
        }
        if self.italic.get() {
            x.italic(|x| x.text("italic "));
        }
        if self.underline.get() {
            x.underline(|x| x.text("underline "));
        }
        if self.strike.get() {
            x.strikethrough(|x| x.text("strike "));
        }
        if self.overline.get() {
            x.overline(|x| x.text("overline "));
        }
        x.font_size(self.size.get(), |x| x.text("sized "));
        if self.spacing.get().abs() > f32::EPSILON {
            let spacing = self.spacing.get();
            x.letter_spacing(spacing, |x| x.text("tracked "));
        }
        if self.word_spacing.get().abs() > f32::EPSILON {
            let spacing = self.word_spacing.get();
            x.word_spacing(spacing, |x| x.text("word spaced "));
        }
        if self.height_multiple.get() > 0.0 {
            let multiple = self.height_multiple.get();
            x.height_multiple(multiple, |x| x.text("tall "));
        }
        x.text(SAMPLE);
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
                .test_tag(format!("rt-{tag}-card")),
        )
        .build(ctx, |ctx| {
            Column::new()
                .spacing(8.0)
                .modifier(Modifier::new().fill_max_width())
                .build(ctx, |ctx| {
                    Text::new(title).font_size(12.0).color(MUTED).build(ctx);
                    body(ctx);
                });
        });
}

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
fn switch(ctx: &mut ComposeCtx, state: State<bool>, tag: &str, title: &str) {
    Row::new().spacing(10.0).build(ctx, |ctx| {
        Switch::new(state.get())
            .modifier(Modifier::new().test_tag(tag.to_string()))
            .on_checked_change(move |checked| state.set(checked))
            .build(ctx, |_| {});
        Text::new(title).font_size(12.0).color(MUTED).build(ctx);
    });
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

/// The inline star, built once per call (an `InlineDrawable` is cheap to construct).
fn star() -> SvgDrawable {
    SvgDrawable::from_str(STAR, 16.0, 16.0).expect("the inline star parses")
}

#[composable]
fn rich_text_demo(ctx: &mut ComposeCtx) {
    let scroll = ctx.remember(|| ScrollState::new()).get();
    let k = Knobs::remember(ctx);
    let selected = ctx.remember(|| State::new(0usize)).get();

    Column::new()
        .spacing(12.0)
        .modifier(
            Modifier::new()
                .fill_max_size()
                .padding(16.0)
                .vertical_scroll(scroll),
        )
        .build(ctx, |ctx| {
            // 1. Playground: every knob against one rich paragraph.
            card(ctx, "play", "live playground", |ctx| {
                Column::new().spacing(8.0).build(ctx, |ctx| {
                    let rich = k.apply(RichText::new()).modifier(
                        Modifier::new()
                            .width(360.0)
                            .height(if k.wrap.get() { 140.0 } else { 40.0 })
                            .padding(6.0)
                            .background(Color::from_argb(24, 255, 255, 255), Shape::rounded(6.0))
                            .test_tag("rt-play-sample"),
                    );
                    rich.build(ctx, |x| k.body(x));

                    labelled(ctx, "font size", |ctx| slider(ctx, k.size.clone(), 8.0, 36.0, "rt-play-size"));
                    labelled(ctx, "letter spacing", |ctx| slider(ctx, k.spacing.clone(), -1.0, 8.0, "rt-play-spacing"));
                    labelled(ctx, "word spacing", |ctx| slider(ctx, k.word_spacing.clone(), 0.0, 12.0, "rt-play-word"));
                    labelled(ctx, "height multiple", |ctx| slider(ctx, k.height_multiple.clone(), 0.0, 3.0, "rt-play-height"));
                    labelled(ctx, "max lines", |ctx| slider(ctx, k.max_lines.clone(), 0.0, 6.0, "rt-play-max"));
                    labelled(ctx, "min lines", |ctx| slider(ctx, k.min_lines.clone(), 1.0, 6.0, "rt-play-min"));
                    labelled(ctx, "line height", |ctx| slider(ctx, k.line_height.clone(), 0.0, 48.0, "rt-play-lh"));

                    segmented(ctx, "rt-play-align", "align", &["Left", "Center", "Right", "Justify", "Start", "End"], k.align.clone());
                    segmented(ctx, "rt-play-overflow", "overflow", &["Clip", "Visible", "Ellipsis"], k.overflow.clone());

                    Row::new().spacing(8.0).build(ctx, |ctx| {
                        switch(ctx, k.bold.clone(), "rt-play-bold", "bold");
                        switch(ctx, k.italic.clone(), "rt-play-italic", "italic");
                        switch(ctx, k.underline.clone(), "rt-play-underline", "underline");
                    });
                    Row::new().spacing(8.0).build(ctx, |ctx| {
                        switch(ctx, k.strike.clone(), "rt-play-strike", "strike");
                        switch(ctx, k.overline.clone(), "rt-play-overline", "overline");
                        switch(ctx, k.wrap.clone(), "rt-play-wrap", "soft wrap");
                    });
                });
            });

            // 2. Nested scopes: the closure shape is the whole API.
            card(ctx, "nest", "nested scopes", |ctx| {
                RichText::new()
                    .modifier(Modifier::new().fill_max_width().test_tag("rt-nested"))
                    .build(ctx, |x| {
                        x.text("plain > ");
                        x.bold(|x| {
                            x.text("bold > ");
                            x.italic(|x| x.text("bold+italic > "));
                            x.font_size(22.0, |x| x.text("bold+big"));
                        });
                        x.text(" < back to plain");
                    });
            });

            // 3. Decorations, including the decoration-only knobs skia exposes.
            card(ctx, "deco", "decorations", |ctx| {
                RichText::new()
                    .modifier(Modifier::new().fill_max_width())
                    .build(ctx, |x| {
                        x.underline(|x| x.text("underline "));
                        x.strikethrough(|x| x.text("strike "));
                        x.overline(|x| x.text("overline "));
                        x.style(
                            |s| s.decoration_color(WARM).decoration_style(DecoStyle::Wavy)
                                .decoration_mode(winia::text::DecoMode::Gaps),
                            |x| x.underline(|x| x.text("wavy warm ")),
                        );
                        x.background(Color::from_argb(80, 0xD0, 0xBC, 0xFF), |x| x.text("highlighted"));
                        x.foreground_color(ACCENT, |x| x.text(" foreground-coloured"));
                    });
            });

            // 4. Baseline shifts and spacing.
            card(ctx, "shift", "baseline / spacing", |ctx| {
                RichText::new()
                    .modifier(Modifier::new().fill_max_width())
                    .build(ctx, |x| {
                        x.text("H");
                        x.subscript(|x| x.text("2"));
                        x.text("O  and  x");
                        x.superscript(|x| x.text("2"));
                        x.text("  ");
                        x.baseline_shift(0.3, |x| x.text("raised"));
                    });
                RichText::new()
                    .modifier(Modifier::new().fill_max_width())
                    .build(ctx, |x| {
                        x.letter_spacing(3.0, |x| x.text("tracked"));
                        x.text(" / ");
                        x.word_spacing(10.0, |x| x.text("word spaced"));
                        x.text(" / ");
                        x.height_multiple(2.0, |x| x.text("double height"));
                        x.half_leading(|x| x.text(" half leading"));
                    });
            });

            // 5. Inline content: an image, a multi-character placeholder, another image.
            card(ctx, "inline", "inline content", |ctx| {
                RichText::new()
                    .modifier(Modifier::new().fill_max_width().test_tag("rt-inline"))
                    .build(ctx, |x| {
                        x.text("text ");
                        x.image(star());
                        x.text(" image, text ");
                        x.placeholder("XYZ", star());
                        x.text(" placeholder (3 chars behind one icon)");
                    });
            });

            // 6. Paragraph parameters, shown side by side (these were T-3).
            card(ctx, "params", "paragraph parameters", |ctx| {
                Text::new("max_lines(1) + Ellipsis, nowrap").font_size(11.0).color(MUTED).build(ctx);
                RichText::new()
                    .soft_wrap(false)
                    .max_lines(1)
                    .overflow(TextOverflow::Ellipsis)
                    .modifier(
                        Modifier::new()
                            .width(300.0)
                            .background(Color::from_argb(20, 255, 255, 255), Shape::Rectangle)
                            .test_tag("rt-params-ellipsis"),
                    )
                    .build(ctx, |x| x.text(SAMPLE));

                Text::new("max_lines(1) + Clip").font_size(11.0).color(MUTED).build(ctx);
                RichText::new()
                    .max_lines(1)
                    .overflow(TextOverflow::Clip)
                    .modifier(Modifier::new().width(300.0).test_tag("rt-params-clip"))
                    .build(ctx, |x| x.text(SAMPLE));

                Text::new("centred").font_size(11.0).color(MUTED).build(ctx);
                RichText::new()
                    .align(TextAlign::Center)
                    .modifier(
                        Modifier::new()
                            .width(300.0)
                            .background(Color::from_argb(20, 255, 255, 255), Shape::Rectangle)
                            .test_tag("rt-params-center"),
                    )
                    .build(ctx, |x| {
                        x.text("one line\nsecond, shorter line");
                    });

                Text::new("min_lines(3) reserves the box").font_size(11.0).color(MUTED).build(ctx);
                RichText::new()
                    .min_lines(3)
                    .modifier(
                        Modifier::new()
                            .width(300.0)
                            .background(Color::from_argb(20, 255, 255, 255), Shape::Rectangle)
                            .test_tag("rt-params-minlines"),
                    )
                    .build(ctx, |x| x.text("short"));
            });

            // 7. Selection: the registrar covers rich text too, so drag-select works here.
            card(ctx, "select", "selection", |ctx| {
                SelectionContainer::new()
                    .on_selection_change({
                        clone!(selected);
                        move |selection| selected.set(selection.text().len())
                    })
                    .build(ctx, |ctx| {
                        RichText::new()
                            .modifier(Modifier::new().fill_max_width().test_tag("rt-selectable"))
                            .build(ctx, |x| {
                                x.text("drag across this paragraph — ");
                                x.bold(|x| x.text("bold runs are selectable too"));
                                x.text(", and the count below follows the selection.");
                            });
                    });
                Text::new(format!("selected {} characters", selected.get()))
                    .font_size(11.0)
                    .color(ACCENT)
                    .modifier(Modifier::new().test_tag("rt-select-count"))
                    .build(ctx);
            });

            // 8. A value built ahead of time: Compose's `Text(AnnotatedString)`. The same value could
            // be stored, sliced with `substring`, or queried with `style_at`.
            card(ctx, "annotated", "AnnotatedString", |ctx| {
                let annotated = AnnotatedString::new("annotated: plain, then bold accent, tracked")
                    .with_style(22..32, SpanStyle::new().bold().color(ACCENT))
                    .with_style(0..9, SpanStyle::new().letter_spacing(1.5));
                RichText::new()
                    .annotated(&annotated)
                    .modifier(Modifier::new().test_tag("rt-annotated"))
                    .build(ctx, |_| {});
            });
            // 9. ProvideTextStyle: a rich block inherits the base style, spans override it.
            card(ctx, "inherit", "inherits ProvideTextStyle", |ctx| {
                ProvideTextStyle(
                    TextStyle::new().font_size(18.0).color(WARM),
                    ctx,
                    |ctx| {
                        RichText::new()
                            .modifier(Modifier::new().fill_max_width())
                            .build(ctx, |x| {
                                x.text("inherits 18 sp warm, ");
                                x.font_size(12.0, |x| x.text("a 12 sp span, "));
                                x.bold(|x| x.text("a bold span"));
                            });
                    },
                );
            });
        });
}

pub fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(460.0, 900.0)
                .title("RichText demo")
                .build(ctx, |ctx| {
                    settings::shell("RichText", ctx, rich_text_demo);
                });
        });
    });
}
