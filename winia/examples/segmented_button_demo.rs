//! SegmentedButton demo (material3 aligned) — single-choice, multi-choice, disabled and customized
//! rows.
//!
//! The window follows the system theme (`WiniaTheme::auto`) and every label takes its color from the
//! theme, so the component can be inspected in light and dark alike.

use letclone::clone;
use winia::prelude::*;

/// A plain dot, used as the inactive half of the crossfading pair below.
const DOT_ICON_PATH: &str = "M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8z";

/// Secondary label color, from the theme so it stays readable in both light and dark.
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

#[composable]
fn segmented_button_demo(ctx: &mut ComposeCtx) {
    let scroll_y = ctx.remember(|| ScrollState::new()).get();
    let day = ctx.remember(|| 0usize);
    let bold = ctx.remember(|| true);
    let italic = ctx.remember(|| false);
    let align = ctx.remember(|| 1usize);
    let dot_icons = ctx.remember(|| 0usize);
    let view = ctx.remember(|| 0usize);

    Column::new()
        .modifier(Modifier::new().fill_max_size().padding(16.0).vertical_scroll(scroll_y))
        .build(ctx, |ctx| {
            Text::new("SegmentedButton demo (material3 aligned)")
                .font_size(20.0)
                .build(ctx);

            section_title(ctx, "Single choice (one item is selected)");
            SingleChoiceSegmentedButtonRow::new().build(ctx, |ctx| {
                for (i, name) in ["Day", "Week", "Month"].iter().enumerate() {
                    let d = day.clone();
                    SegmentedButton::new(day.get() == i, move || d.set(i))
                        .shape(SegmentedButtonDefaults::item_shape(i, 3))
                        .build(ctx, |ctx| {
                            Text::new(*name).build(ctx);
                        });
                }
            });
            Text::new(format!("selected: {}", ["Day", "Week", "Month"][day.get()]))
                .font_size(13.0)
                .color(label_color())
                .build(ctx);

            section_title(ctx, "Multi choice (each item toggles on its own)");
            MultiChoiceSegmentedButtonRow::new().build(ctx, |ctx| {
                let labels = ["Bold", "Italic"];
                let values = [bold.clone(), italic.clone()];
                for (i, (name, value)) in labels.iter().zip(values).enumerate() {
                    SegmentedButton::toggle(value.get(), move |now| value.set(now))
                        .shape(SegmentedButtonDefaults::item_shape(i, 2))
                        .build(ctx, |ctx| {
                            Text::new(*name).build(ctx);
                        });
                }
            });
            Text::new(format!("bold: {}  italic: {}", bold.get(), italic.get()))
                .font_size(13.0)
                .color(label_color())
                .build(ctx);

            section_title(ctx, "Disabled");
            SingleChoiceSegmentedButtonRow::new().build(ctx, |ctx| {
                for (i, name) in ["Left", "Center", "Right"].iter().enumerate() {
                    SegmentedButton::new(align.get() == i, || {})
                        .shape(SegmentedButtonDefaults::item_shape(i, 3))
                        .enabled(false)
                        .build(ctx, |ctx| {
                            Text::new(*name).build(ctx);
                        });
                }
            });

            section_title(ctx, "A crossfading icon pair (inactive_icon)");
            MultiChoiceSegmentedButtonRow::new().build(ctx, |ctx| {
                for (i, name) in ["Pinned", "Marked"].iter().enumerate() {
                    let v = dot_icons.clone();
                    let on = v.get() & (1 << i) != 0;
                    SegmentedButton::toggle(on, move |now| {
                        v.update(|n| if now { *n |= 1 << i } else { *n &= !(1 << i) })
                    })
                    .shape(SegmentedButtonDefaults::item_shape(i, 2))
                    .inactive_icon(|ctx| {
                        Icon::svg_path(DOT_ICON_PATH)
                            .size(10.0)
                            .tint(Tint::Color(label_color()))
                            .build(ctx);
                    })
                    .build(ctx, |ctx| {
                        Text::new(*name).build(ctx);
                    });
                }
            });

            section_title(ctx, "Thicker border (space matches the stroke)");
            SingleChoiceSegmentedButtonRow::new().space(2.0).build(ctx, |ctx| {
                for (i, name) in ["A", "B", "C"].iter().enumerate() {
                    let v = view.clone();
                    SegmentedButton::new(view.get() == i, move || v.set(i))
                        .shape(SegmentedButtonDefaults::item_shape(i, 3))
                        .border(2.0, WiniaTheme::colors().primary)
                        .build(ctx, |ctx| {
                            Text::new(*name).build(ctx);
                        });
                }
            });

            section_title(ctx, "Custom colors (tertiary)");
            let theme = WiniaTheme::colors();
            let mut colors = SegmentedButtonDefaults::colors(&theme);
            colors.active_container = theme.tertiary_container;
            colors.active_content = theme.on_tertiary_container;
            colors.active_border = theme.tertiary;
            colors.inactive_content = theme.on_surface_variant;
            SingleChoiceSegmentedButtonRow::new().build(ctx, |ctx| {
                for (i, name) in ["One", "Two", "Three"].iter().enumerate() {
                    let v = view.clone();
                    SegmentedButton::new(view.get() == i, move || v.set(i))
                        .shape(SegmentedButtonDefaults::item_shape(i, 3))
                        .colors(colors)
                        .build(ctx, |ctx| {
                            Text::new(*name).build(ctx);
                        });
                }
            });

            Text::new("")
                .modifier(Modifier::new().height(40.0))
                .build(ctx);
        });
}

fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(460.0, 640.0)
                .title("Segmented Button Demo")
                .build(ctx, segmented_button_demo);
        });
    });
}
