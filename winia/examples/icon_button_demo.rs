//! IconButton demo — what each section shows, and what to look at.
//!
//! Run: `cargo run -p winia --example icon_button_demo`
//!
//! 1. The four styles — Standard / Filled / FilledTonal / Outlined — each with the colours it
//!    resolves from the theme, and the Outlined border from the content colour — only the VIBRANT
//!    outlined set pins `outlineVariant`.
//! 2. The five sizes (32 / 40 / 56 / 96 / 136 dp), each with its own icon size and outline width.
//!    Every size reserves a 48 dp touch target, so on Small and XSmall the pressable area is wider
//!    than the paint — click just outside the circle and it still responds.
//! 3. The width option: one size at Narrow / Uniform / Wide. Only the horizontal space around the
//!    icon changes; `Uniform` is the square container.
//! 4. The shape family — Round (a circle) or Square (the size's corner token) — and the press morph:
//!    hold one down and its container re-corners itself to the size's `PressedContainerShape`.
//! 5. Vibrant colour sets for Standard and Outlined, each beside its plain counterpart.

use winia::prelude::*;
// Shared example chrome: top app bar with the settings sheet (theme mode + layout direction).
#[path = "common/settings.rs"]
mod settings;

const STAR_PATH: &str = "M12 17.27L18.18 21l-1.64-7.03L22 9.24l-7.19-.61L12 2 9.19 8.63 2 9.24l5.46 4.73L5.82 21z";

fn label_color() -> Color {
    WiniaTheme::colors().on_surface_variant
}

fn section_title(ctx: &mut ComposeCtx, text: &str) {
    Text::new(text)
        .font_size(14.0)
        .color(label_color())
        .modifier(Modifier::new().padding_top(14.0).padding_bottom(6.0))
        .build(ctx);
}

fn caption(ctx: &mut ComposeCtx, text: &str) {
    Text::new(text)
        .font_size(11.0)
        .color(label_color())
        .modifier(Modifier::new().padding_top(4.0))
        .build(ctx);
}

/// An IconButton with the demo's star icon, so each row stays readable.
fn star(ctx: &mut ComposeCtx, button: IconButton, clicks: &State<i32>) {
    let counted = clicks.clone();
    button
        .on_click(move || counted.update(|v| *v += 1))
        .build(ctx, |ctx| Icon::svg_path(STAR_PATH).build(ctx));
}

#[composable]
fn icon_button_demo(ctx: &mut ComposeCtx) {
    let clicks = ctx.remember(|| 0i32);
    let scroll = ctx.remember(|| ScrollState::new()).get();
    let theme = WiniaTheme::colors();

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0).vertical_scroll(scroll))
        .build(ctx, |ctx| {
            Text::new(format!("Clicks: {}", clicks.get()))
                .font_size(13.0)
                .build(ctx);

            // ── 1: the four styles ──
            section_title(ctx, "1. Styles — Standard / Filled / FilledTonal / Outlined");
            Row::new().spacing(20.0).build(ctx, |ctx| {
                star(ctx, IconButton::new(), &clicks);
                star(ctx, IconButton::filled(), &clicks);
                star(ctx, IconButton::filled_tonal(), &clicks);
                star(ctx, IconButton::outlined(), &clicks);
                star(ctx, IconButton::filled().enabled(false), &clicks);
            });
            caption(
                ctx,
                "Standard and Outlined paint no container and take their icon colour from the \
                 surrounding content colour; Filled is primary/on-primary, FilledTonal is \
                 secondary-container; Outlined adds a border in the content colour. The last one is \
                 disabled: a transparent container, an on-surface icon at 38 %, and no click target.",
            );

            // ── 2: sizes ──
            section_title(ctx, "2. Sizes — 32 / 40 / 56 / 96 / 136 dp");
            Row::new().spacing(16.0).build(ctx, |ctx| {
                for (size, label) in [
                    (IconButtonSize::XSmall, "32 dp"),
                    (IconButtonSize::Small, "40 dp"),
                    (IconButtonSize::Medium, "56 dp"),
                    (IconButtonSize::Large, "96 dp"),
                    (IconButtonSize::XLarge, "136 dp"),
                ] {
                    Column::new()
                        .alignment(Alignment::Center)
                        .spacing(4.0)
                        .build(ctx, |ctx| {
                            star(ctx, IconButton::filled().size(size), &clicks);
                            Text::new(label).font_size(10.0).color(label_color()).build(ctx);
                        });
                }
            });
            caption(
                ctx,
                "Each size carries its own icon size (20 / 24 / 24 / 32 / 40) and outlined border \
                 width (1 / 1 / 1 / 2 / 3). All of them reserve a 48 dp touch target, so on the two \
                 smallest the clickable area reaches past the painted circle — click just outside it \
                 and the count still moves.",
            );

            // ── 3: width options ──
            section_title(ctx, "3. Width option — Narrow / Uniform / Wide");
            Row::new().spacing(16.0).build(ctx, |ctx| {
                for (option, label) in [
                    (IconButtonWidthOption::Narrow, "Narrow"),
                    (IconButtonWidthOption::Uniform, "Uniform"),
                    (IconButtonWidthOption::Wide, "Wide"),
                ] {
                    Column::new()
                        .alignment(Alignment::Center)
                        .spacing(4.0)
                        .build(ctx, |ctx| {
                            star(
                                ctx,
                                IconButton::filled_tonal().width_option(option),
                                &clicks,
                            );
                            Text::new(label).font_size(10.0).color(label_color()).build(ctx);
                        });
                }
            });
            caption(
                ctx,
                "One Medium container at the token set's three widths: the icon keeps its size and \
                 only the leading/trailing space changes, so Uniform stays square while Narrow and \
                 Wide trade height for width.",
            );

            // ── 4: shape family and the press morph ──
            section_title(ctx, "4. Shape family and the press morph");
            Row::new().spacing(20.0).build(ctx, |ctx| {
                star(ctx, IconButton::filled_tonal(), &clicks);
                star(
                    ctx,
                    IconButton::filled_tonal().shape_family(IconButtonShape::Square),
                    &clicks,
                );
                star(
                    ctx,
                    IconButton::filled_tonal().size(IconButtonSize::Large),
                    &clicks,
                );
            });
            caption(
                ctx,
                "The first is the Round family (CornerFull, a circle). The second is Square — the \
                 size's corner token, CornerMedium for Small. Hold any of them down: the container \
                 re-corners to that size's PressedContainerShape (CornerSmall here, CornerLarge on \
                 the third) while the pointer is down, and returns on release.",
            );

            // ── 5: vibrant colours ──
            section_title(ctx, "5. Vibrant colour sets");
            Row::new().spacing(20.0).build(ctx, |ctx| {
                star(ctx, IconButton::new(), &clicks);
                star(
                    ctx,
                    IconButton::new()
                        .colors(IconButtonDefaults::icon_button_vibrant_colors(&theme)),
                    &clicks,
                );
                star(ctx, IconButton::outlined(), &clicks);
                star(
                    ctx,
                    IconButton::outlined()
                        .colors(IconButtonDefaults::outlined_icon_button_vibrant_colors(&theme))
                        .border(IconButtonDefaults::outlined_icon_button_vibrant_border(
                            &theme, true, 1.0,
                        )),
                    &clicks,
                );
            });
            caption(
                ctx,
                "Plain then vibrant, for Standard and for Outlined. The vibrant sets pin the token \
                 role (on-surface-variant) instead of following the surrounding content colour, and \
                 the vibrant Outlined border takes outlineVariant where the plain one follows the \
                 content colour.",
            );
        });
}

fn main() {
    winia::run_app!(|ctx| {
        winia::theme::WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(760.0, 760.0)
                .title("Icon Button Demo")
                .build(ctx, |ctx| {
                    settings::shell("Icon Button Demo", ctx, icon_button_demo);
                });
        });
    });
}
