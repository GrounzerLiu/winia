//! Colour animation demo — what a `Color` transition does now that it matches Compose.
//!
//! Three sections, in the order the change matters:
//!
//! 1. **Oklab, not the encoded bytes.** The same black -> white move is driven twice from one button:
//!    the left box by the engine, the right one by a byte-wise blend of the SAME progress. Halfway
//!    through, the engine's box is `#636363` — Oklab's midpoint, the same lightness to the eye — while
//!    the byte blend shows the lighter-looking `#7F7F7F`. Compose animates a colour exactly this way
//!    (`animation/ColorVectorConverter.kt:31-49`: `convertToVector` moves the colour to Oklab and hands
//!    the animation `AnimationVector4D(alpha, L, a, b)`).
//! 2. **A colour spring is a spring.** The same move under an explicit `SpringSpec` (left) and a
//!    `TweenSpec` (right). Before this change the spring was silently replaced by a 300 ms linear
//!    tween, so both boxes moved identically; now the spring keeps its own curve and still lands
//!    exactly on the target.
//! 3. **The components whose specs changed.** A checkbox, a radio pair and a tab row — their M3 motion
//!    (`DefaultEffects` = spring(1.0, 1600), `FastEffects` = spring(1.0, 3800)) now runs for real
//!    (`material3/Checkbox.kt:685-694`, `material3/RadioButton.kt:237`, `material3/Tab.kt:270-291`).
//!    Click them: the colour arrives on a spring instead of a 300 ms linear ramp.
//!
//! Run with `cargo run -p winia --example colour_animation_demo`.

use letclone::clone;
use winia::animation::{AnimationSpec, SpringSpec, TweenSpec};
use winia::components::divider::Divider;
use winia::prelude::*;

// Shared example chrome: top app bar with the settings sheet (theme mode + layout direction).
#[path = "common/settings.rs"]
mod settings;

const BLACK: Color = Color { r: 0, g: 0, b: 0, a: 255 };
const WHITE: Color = Color { r: 255, g: 255, b: 255, a: 255 };
const FROM: Color = Color { r: 33, g: 150, b: 243, a: 255 };
const TO: Color = Color { r: 244, g: 67, b: 54, a: 255 };

/// The blend the engine used before Oklab: mix the sRGB BYTES. Written out here only as the control
/// the engine's box is compared against.
fn byte_blend(from: Color, to: Color, t: f32) -> Color {
    let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
    Color::from_argb(
        mix(from.a, to.a),
        mix(from.r, to.r),
        mix(from.g, to.g),
        mix(from.b, to.b),
    )
}

fn hex(color: Color) -> String {
    format!("#{:02X}{:02X}{:02X}", color.r, color.g, color.b)
}

/// A labelled colour box. `fill` is read at PAINT time (`peek`), which is what keeps an animating
/// box out of the recomposition path: only the renderer follows it.
#[composable]
fn swatch(
    ctx: &mut ComposeCtx,
    tag: &str,
    fill: impl Fn() -> Color + Send + Sync + 'static,
    caption: &str,
) {
    Column::new()
        .modifier(Modifier::new().width(200.0))
        .spacing(2.0)
        .build(ctx, |ctx| {
            Column::new()
                .modifier(
                    Modifier::new()
                        .test_tag(tag)
                        .height(72.0)
                        .fill_max_width()
                        .background(fill, Shape::rounded(8.0)),
                )
                .build(ctx, |_ctx| {});
            Text::new(caption).font_size(11.0).build(ctx);
        });
}

/// A still ramp of five steps, so the difference is readable without watching the loop:
/// `from_oklab` mixes the Oklab components, the plain blend mixes the encoded bytes.
#[composable]
fn ramp(ctx: &mut ComposeCtx, caption: &str, step: impl Fn(f32) -> Color + Send + Sync + 'static) {
    Column::new()
        .modifier(Modifier::new().width(200.0))
        .spacing(2.0)
        .build(ctx, |ctx| {
            Row::new()
                .modifier(Modifier::new().fill_max_width())
                .build(ctx, |ctx| {
                    for index in 0..5 {
                        let t = index as f32 / 4.0;
                        let color = step(t);
                        Column::new()
                            .modifier(
                                Modifier::new()
                                    .height(36.0)
                                    .fill_max_width()
                                    .layout_weight(1.0)
                                    .background(color, Shape::rounded(4.0)),
                            )
                            .build(ctx, |_ctx| {});
                    }
                });
            Text::new(caption).font_size(11.0).build(ctx);
        });
}

fn swatch_of(ctx: &mut ComposeCtx, tag: &str, color: State<Color>, caption: &str) {
    let fill_state = color.clone();
    swatch(ctx, tag, move || fill_state.peek(), caption);
}

fn section(ctx: &mut ComposeCtx, text: &str) {
    Text::new(text)
        .font_size(13.0)
        .color(WiniaTheme::colors().on_surface_variant)
        .modifier(Modifier::new().padding_top(10.0))
        .build(ctx);
}

#[composable]
fn colour_animation_demo(ctx: &mut ComposeCtx) {
    let warm = ctx.remember(|| false);
    let checked = ctx.remember(|| true);
    let radio = ctx.remember(|| 0u32);
    let tab = ctx.remember(|| 0usize);

    // One deliberately SLOW duration for the two comparisons, so the curve is visible to the eye and
    // to a screenshot taken a third of the way in: what the sections compare is the interpolation and
    // the spec, never the duration.
    let long_tween = AnimationSpec::Tween(TweenSpec::new(
        std::time::Duration::from_millis(800),
        winia::animation::interpolator::Linear::new(),
    ));

    // ── 1. Oklab against the encoded bytes: one clock, two fills, looping so the divergence is
    // always on screen (a screenshot a third of the way in is enough to see it). ──
    let mut infinite = ctx.remember_infinite_transition();
    let loop_spec =
        winia::animation::InfiniteRepeatableSpec::reverse(std::time::Duration::from_millis(1800));
    let engine = infinite.animate_color(ctx, BLACK, WHITE, loop_spec.clone());
    let progress = infinite.animate_float(ctx, 0.0, 1.0, loop_spec);

    // ── 2. The same move under a spring and under a tween. ──
    let sprung = ctx.animate_color_as_state(
        if warm.get() { TO } else { FROM },
        AnimationSpec::Spring(SpringSpec::default()),
    );
    let tweened = ctx.animate_color_as_state(if warm.get() { TO } else { FROM }, long_tween);

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0))
        .spacing(4.0)
        .build(ctx, |ctx| {
            Row::new()
                .modifier(Modifier::new().fill_max_width())
                .spacing(8.0)
                .build(ctx, |ctx| {
                    Button::new()
                        .on_click({ clone!(warm); move || warm.update(|v| *v = !*v) })
                        .build(ctx, |ctx| {
                            Text::new(if warm.get() { "Back to blue" } else { "Blue to red" }).build(ctx);
                        });
                });

            // ── 1 ──
            section(ctx, "1. The engine (left, Oklab) against a byte-wise blend (right)");
            Row::new()
                .modifier(Modifier::new().fill_max_width())
                .spacing(12.0)
                .build(ctx, |ctx| {
                    let fill_engine = engine.clone();
                    swatch(ctx, "oklab-box", move || fill_engine.peek(), "engine (Oklab)");
                    let fill_progress = progress.clone();
                    swatch(
                        ctx,
                        "bytewise-box",
                        move || byte_blend(BLACK, WHITE, fill_progress.peek()),
                        "byte blend of the same progress",
                    );
                });
            Text::new(
                "Both move from the same progress. At the halfway mark the engine's box is #636363 \
                 while the byte-wise blend is #808080: Oklab keeps the lightness to the eye even, \
                 which is what Compose's Color.VectorConverter animates.",
            )
            .font_size(11.0)
            .build(ctx);

            // The same comparison as a still strip: five steps of each interpolation, so the
            // divergence is readable without watching the loop. The captions are the values the
            // swatches are actually painted with (measured through the debug server).
            Row::new()
                .modifier(Modifier::new().fill_max_width().padding_top(6.0))
                .spacing(12.0)
                .build(ctx, |ctx| {
                    ramp(ctx, "engine: #000000 #222222 #636363 #AEAEAE #FFFFFF", |t| {
                        Color::from_oklab([1.0, t, 0.0, 0.0])
                    });
                    ramp(ctx, "bytes:  #000000 #404040 #808080 #BFBFBF #FFFFFF", |t| {
                        byte_blend(BLACK, WHITE, t)
                    });
                });

            // ── 2 ──
            section(ctx, "2. SpringSpec::default() (left) against TweenSpec::default() (right)");
            Row::new()
                .modifier(Modifier::new().fill_max_width())
                .spacing(12.0)
                .build(ctx, |ctx| {
                    swatch_of(ctx, "spring-box", sprung.clone(), "engine (spring)");
                    swatch_of(ctx, "tween-box", tweened.clone(), "engine (tween)");
                });
            Text::new(format!(
                "spring {}   tween {}",
                hex(sprung.get()),
                hex(tweened.get())
            ))
            .font_size(11.0)
            .build(ctx);
            Text::new(
                "Before this change a colour spring was silently replaced by a 300 ms linear tween, so \
                 these two boxes moved as one. The spring now keeps its own curve — and still lands \
                 exactly on the target instead of stopping inside the threshold.",
            )
            .font_size(11.0)
            .build(ctx);

            Divider::horizontal().build(ctx);

            // ── 3 ──
            section(ctx, "3. The components: M3's effect springs (1600 arriving, 3800 leaving)");
            Row::new()
                .modifier(Modifier::new().fill_max_width().padding_vertical(6.0))
                .spacing(16.0)
                .alignment(Alignment::Center)
                .build(ctx, |ctx| {
                    Row::new()
                        .spacing(6.0)
                        .alignment(Alignment::Center)
                        .build(ctx, |ctx| {
                            Checkbox::new(checked.get())
                                .on_checked_change({ clone!(checked); move |v| checked.set(v) })
                                .build(ctx);
                            Text::new("Checkbox").font_size(12.0).build(ctx);
                        });
                    for index in 0..2u32 {
                        ctx.key(("radio", index), |ctx| {
                            Row::new()
                                .spacing(6.0)
                                .alignment(Alignment::Center)
                                .build(ctx, |ctx| {
                                    RadioButton::new(radio.get() == index)
                                        .on_click({ clone!(radio); move || radio.set(index) })
                                        .build(ctx);
                                    Text::new(format!("Radio {}", index + 1)).font_size(12.0).build(ctx);
                                });
                        });
                    }
                });
            TabRow::new(tab.get(), {
                clone!(tab);
                move |ctx| {
                    for index in 0..3usize {
                        let label = format!("Tab {}", index + 1);
                        Tab::new(
                            { clone!(tab); tab.get() == index },
                            { clone!(tab); move || tab.set(index) },
                        )
                        .text(move |ctx| Text::new(&label).build(ctx))
                        .build(ctx);
                    }
                }
            })
            .build(ctx);
            Text::new(
                "The tab's content colour is Compose's TabTransition: DefaultEffects while it arrives at \
                 the selected colour, FastEffects while it leaves. Switch, Chip and NavigationDrawer \
                 keep their original fades — M3 animates no colour in those files, so those are winia \
                 additions and are documented as such.",
            )
            .font_size(11.0)
            .modifier(Modifier::new().padding_top(4.0))
            .build(ctx);
        });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(620.0, 780.0)
                .title("Colour Animation Demo")
                .build(ctx, |ctx| {
                    settings::shell("Colour Animation Demo", ctx, |ctx| colour_animation_demo(ctx));
                });
        });
    });
}
