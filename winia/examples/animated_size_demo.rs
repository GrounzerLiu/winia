//! AnimatedSize demo — a container that animates its own size when its content changes
//! (Compose's `Modifier.animateContentSize`).
//!
//! Four things the sections show, in the order the component's behaviour matters:
//!
//! 1. **Grow and shrink** — the content takes its new size at once and the box travels to it, so the
//!    two are different sizes for the length of the animation. That is what the clip is for.
//! 2. **The clip** — the growing child would otherwise paint past the box; the page's stripe pattern
//!    makes that visible if it ever regresses.
//! 3. **`alignment`** — a child that shrinks sits inside the box that has not caught up yet: `Start`
//!    pins it to the top-left, `Center` centres it, `End` puts it at the bottom-right.
//! 4. **`finished_listener`** — what Compose calls when the animation ends, with the size it started
//!    from and the one it reached; the count and the last pair are shown under the boxes.
//!
//! Run with `cargo run -p winia --example animated_size_demo`.

use letclone::clone;
use winia::prelude::*;
use winia::components::animated_size::AnimatedSize;

// Shared example chrome: top app bar with the settings sheet (theme mode + layout direction).
#[path = "common/settings.rs"]
mod settings;

/// Sized block the boxes animate around, with a readable label.
#[composable]
fn block(ctx: &mut ComposeCtx, width: f32, height: f32, color: Color, label: &str) {
    Column::new()
        .modifier(Modifier::new().size(width, height).background(color, Shape::rounded(6.0)))
        .arrangement(winia::layout::Arrangement::Center)
        .build(ctx, |ctx| {
            Text::new(label).font_size(11.0).build(ctx);
        });
}

#[composable]
fn animated_size_demo(ctx: &mut ComposeCtx) {
    // One toggle for every section, so a click moves all of them at the same moment.
    let grown = ctx.remember(|| false);
    let align = ctx.remember(|| 0u32);
    // The listener's report: how many times it ran, and the pair it last saw.
    let reports = ctx.remember(|| 0u32);
    let last = ctx.remember(|| String::from("—"));

    Column::new()
        .modifier(Modifier::new().padding(16.0).fill_max_size())
        .spacing(12.0)
        .build(ctx, |ctx| {
            Row::new()
                .modifier(Modifier::new().fill_max_width())
                .spacing(8.0)
                .build(ctx, |ctx| {
                    Button::new()
                        .on_click({ clone!(grown); move || { grown.update(|v| *v = !*v); } })
                        .build(ctx, |ctx| {
                            Text::new(if grown.get() { "Shrink" } else { "Grow" }).build(ctx);
                        });
                    Button::new()
                        .on_click({ clone!(align); move || { align.update(|v| *v = (*v + 1) % 3); } })
                        .build(ctx, |ctx| {
                            let name = match align.get() {
                                0 => "alignment: Start",
                                1 => "alignment: Center",
                                _ => "alignment: End",
                            };
                            Text::new(name).build(ctx);
                        });
                });

            // ── 1 + 2. Grow/shrink, with the box's own border so its edge is visible against the
            // child that is outgrowing it. The child is opaque on purpose: if the clip regresses, its
            // colour appears outside the box immediately. This is the box the listener watches,
            // because it is the one whose size changes.
            AnimatedSize::default()
                .finished_listener({
                    let reports_count = reports.clone();
                    let last_pair = last.clone();
                    move |from: Size, to: Size| {
                        reports_count.update(|v| *v += 1);
                        last_pair.set(format!(
                            "{:.0}x{:.0} → {:.0}x{:.0}",
                            from.width, from.height, to.width, to.height
                        ));
                    }
                })
                .modifier(
                    Modifier::new()
                        .border(1.0, WiniaTheme::colors().outline, Shape::rounded(6.0)),
                )
                .build(ctx, |ctx| {
                    let (w, h) = if grown.get() { (320.0, 120.0) } else { (140.0, 44.0) };
                    block(ctx, w, h, Color::from_argb(255, 66, 133, 244), "content");
                });

            // ── 3. The child shrinks in BOTH axes while the box lags, so the vertical half of the
            // alignment has slack too.
            let alignment = match align.get() {
                0 => winia::layout::Alignment::Start,
                1 => winia::layout::Alignment::Center,
                _ => winia::layout::Alignment::End,
            };
            AnimatedSize::default()
                .alignment(alignment)
                .modifier(
                    Modifier::new()
                        .size(360.0, 90.0)
                        .background(WiniaTheme::colors().surface_container_high, Shape::rounded(6.0)),
                )
                .build(ctx, |ctx| {
                    let (w, h) = if grown.get() { (340.0, 74.0) } else { (90.0, 30.0) };
                    block(ctx, w, h, Color::from_argb(255, 219, 68, 55), "aligned child");
                });

            // ── 4. What the listener saw. It is called when an animation above finishes, so the
            // count follows the clicks and the pair is the size it started from and the one it
            // reached.
            Column::new()
                .modifier(Modifier::new().fill_max_width().padding(8.0).background(
                    WiniaTheme::colors().surface_container,
                    Shape::rounded(6.0),
                ))
                .build(ctx, |ctx| {
                    Text::new(format!("finished_listener calls: {}", reports.get()))
                        .font_size(12.0)
                        .build(ctx);
                    Text::new(format!("last: {}", last.get())).font_size(12.0).build(ctx);
                });

            Text::new(
                "The content takes its new size immediately; the box animates to it. The border is the \
                 box, so during a grow the child is larger than the box it is clipped to.",
            )
            .font_size(11.0)
            .build(ctx);
        });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(560.0, 520.0)
                .title("AnimatedSize Demo")
                .build(ctx, |ctx| {
                    settings::shell("AnimatedSize Demo", ctx, |ctx| animated_size_demo(ctx));
                });
        });
    });
}
