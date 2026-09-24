//! Shared example chrome: a top app bar with a settings button, and the bottom sheet it opens.
//!
//! The sheet switches the theme mode (Auto / Light / Dark) and the layout direction (LTR / RTL)
//! for the whole window, so an example can be checked in both schemes and both directions without
//! touching the OS settings. It is one file every example includes:
//!
//! ```ignore
//! #[path = "common/settings.rs"]
//! mod settings;
//!
//! // Inside the window's content closure:
//! settings::shell("Example title", ctx, |ctx| example_body(ctx));
//! ```
//!
//! Both switches take the route the platform would take, so a bug they uncover is one the OS
//! switch hits too: the theme mode pins through `set_system_dark_mode` (the entry point
//! `WindowEvent::ThemeChanged` ends at), and the direction is provided by
//! `WiniaTheme::with_theme_and_direction` (the nesting `icon_demo` and `component_demo` already
//! use for their own RTL toggles).
//!
//! The state lives in the example's own composition — three `remember`ed values held by the window
//! root — so it survives every recomposition the switches cause, and nothing here is global.

use letclone::clone;
use winia::prelude::*;
use winia::ui::set_system_dark_mode;

/// Material Symbols `settings` (24dp, filled), copied from fonts.google.com/icons. The path is
/// wrapped in a minimal SVG document by `Icon::svg_path` and handed to the SVG parser, so the
/// original commas and decimals stay as copied.
const SETTINGS_ICON: &str = "M19.14,12.94c0.04-0.3,0.06-0.61,0.06-0.94c0-0.32-0.02-0.64-0.07-0.94l2.03-1.58c0.18-0.14,0.23-0.41,0.12-0.61l-1.92-3.32c-0.12-0.22-0.37-0.29-0.59-0.22l-2.39,0.96c-0.5-0.38-1.03-0.7-1.62-0.94L14.4,2.81c-0.04-0.24-0.24-0.41-0.48-0.41h-3.84c-0.24,0-0.43,0.17-0.47,0.41L9.25,5.35C8.66,5.59,8.12,5.92,7.63,6.29L5.24,5.33c-0.22-0.08-0.47,0-0.59,0.22L2.74,8.87C2.62,9.08,2.66,9.34,2.86,9.48l2.03,1.58C4.84,11.36,4.8,11.69,4.8,12s0.02,0.64,0.07,0.94l-2.03,1.58c-0.18,0.14-0.23,0.41-0.12,0.61l1.92,3.32c0.12,0.22,0.37,0.29,0.59,0.22l2.39-0.96c0.5,0.38,1.03,0.7,1.62,0.94l0.36,2.54c0.05,0.24,0.24,0.41,0.48,0.41h3.84c0.24,0,0.44-0.17,0.47-0.41l0.36-2.54c0.59-0.24,1.13-0.56,1.62-0.94l2.39,0.96c0.22,0.08,0.47,0,0.59-0.22l1.92-3.32c0.12-0.22,0.07-0.47-0.12-0.61L19.14,12.94z M12,15.6c-1.98,0-3.6-1.62-3.6-3.6s1.62-3.6,3.6-3.6s3.6,1.62,3.6,3.6S13.98,15.6,12,15.6z";

/// What the theme switch is set to. `Auto` is the state an example starts in: it follows the
/// system (or whatever the process pinned last, which is why the other two options can exist).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Auto,
    Light,
    Dark,
}

impl ThemeMode {
    const ALL: [ThemeMode; 3] = [ThemeMode::Auto, ThemeMode::Light, ThemeMode::Dark];

    fn label(self) -> &'static str {
        match self {
            ThemeMode::Auto => "Auto",
            ThemeMode::Light => "Light",
            ThemeMode::Dark => "Dark",
        }
    }

    /// The value `set_system_dark_mode` takes: `None` follows the system, `Some` pins.
    fn pinned(self) -> Option<bool> {
        match self {
            ThemeMode::Auto => None,
            ThemeMode::Light => Some(false),
            ThemeMode::Dark => Some(true),
        }
    }
}

/// What the direction switch is set to.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Ltr,
    Rtl,
}

impl Direction {
    const ALL: [Direction; 2] = [Direction::Ltr, Direction::Rtl];

    fn label(self) -> &'static str {
        match self {
            Direction::Ltr => "LTR",
            Direction::Rtl => "RTL",
        }
    }

    fn layout(self) -> LayoutDirection {
        match self {
            Direction::Ltr => LayoutDirection::Ltr,
            Direction::Rtl => LayoutDirection::Rtl,
        }
    }
}

/// Wrap an example's body in the shared chrome: the theme and direction the sheet edits, the top
/// app bar that opens it, and the sheet itself.
///
/// `title` is the top app bar's title — the window's own title stays where the example sets it.
/// `content` is the example body, composed once per frame exactly as it would be without the
/// chrome. It must emit exactly ONE root node filling the space it is given (`fill_max_size`): the
/// body sits in `Scaffold`'s content slot, which lays out a single child — an example whose body
/// emitted several siblings straight into the window root (as several of them used to) has to put
/// them in a `Column`.
///
/// `#[composable]` is load-bearing here, not decoration — it is what makes the direction switch
/// reach the whole page. This function READS the direction state and hands it to
/// `WiniaTheme::with_theme_and_direction`, and a local is a plain thread-local: `provides` does not
/// invalidate its readers, it only changes what the NEXT reader reads. So the read has to land in a
/// scope, because marking a scope dirty marks its whole subtree dirty (every group re-enters, every
/// reader re-reads the local). A plain function called at the root of the window's content — which
/// is exactly where an example calls this — has no scope to land in: the composer IS notified and
/// the content DOES re-run with the new direction (traced: `pending=true → COMPOSE`, and the state
/// id in the consumed batch), but the subtree below skips on cached slots and keeps the layout it
/// built with the old direction. The sheet has a scope of its own, which is why IT flipped while the
/// page behind it did not.
#[composable]
pub fn shell(
    title: &'static str,
    ctx: &mut ComposeCtx,
    content: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static,
) {
    let mode = ctx.remember(|| ThemeMode::Auto);
    let direction = ctx.remember(|| Direction::Ltr);
    let sheet_open = ctx.remember(|| false);

    // The body composes under the direction the sheet is editing. The colors are re-read every
    // frame from the enclosing theme, so an `Auto` example still follows the system while the
    // wrapper only pins the direction.
    let layout_direction = direction.get().layout();
    WiniaTheme::with_theme_and_direction(WiniaTheme::colors(), layout_direction, ctx, |ctx| {
        Scaffold::new(move |ctx, _padding| content(ctx))
            .top_bar({
                clone!(sheet_open);
                move |ctx| {
                    TopAppBar::new(move |ctx| Text::new(title).build(ctx))
                        .actions({
                            clone!(sheet_open);
                            move |ctx| {
                                IconButton::new()
                                    .on_click(move || sheet_open.set(true))
                                    .build(ctx, |ctx| {
                                        Icon::svg_path(SETTINGS_ICON).build(ctx);
                                    });
                            }
                        })
                        .build(ctx);
                }
            })
            .build(ctx);

        settings_sheet(ctx, mode.clone(), direction.clone(), sheet_open.clone());
    });
}

/// The sheet: a title and one switch per setting, applied the moment it is tapped.
///
/// It stays open across a change — that is the point of it being a sheet and not a dialog: the
/// whole window behind it re-themes and re-mirrors while the finger is still on the button.
fn settings_sheet(
    ctx: &mut ComposeCtx,
    mode: State<ThemeMode>,
    direction: State<Direction>,
    open: State<bool>,
) {
    let label_color = WiniaTheme::colors().on_surface_variant;
    ModalBottomSheet::new(open.get())
        // A settings sheet is a fixed, short list: the half-expanded stop would only ever show it
        // cut in half.
        .skip_partially_expanded(true)
        .on_dismiss_request({
            clone!(open);
            move || open.set(false)
        })
        .build(ctx, move |ctx| {
            Column::new()
                .modifier(Modifier::new().fill_max_width().padding(24.0))
                .spacing(12.0)
                .build(ctx, |ctx| {
                    Text::new("Settings").font_size(20.0).build(ctx);
                    Divider::horizontal().build(ctx);

                    Text::new("Theme").font_size(12.0).color(label_color).build(ctx);
                    SingleChoiceSegmentedButtonRow::new().build(ctx, |ctx| {
                        for (index, option) in ThemeMode::ALL.iter().enumerate() {
                            let option = *option;
                            SegmentedButton::new(mode.get() == option, {
                                clone!(mode);
                                move || {
                                    set_system_dark_mode(option.pinned());
                                    mode.set(option);
                                }
                            })
                            .shape(SegmentedButtonDefaults::item_shape(index, ThemeMode::ALL.len()))
                            .build(ctx, |ctx| {
                                Text::new(option.label()).build(ctx);
                            });
                        }
                    });

                    Text::new("Layout direction")
                        .font_size(12.0)
                        .color(label_color)
                        .modifier(Modifier::new().padding_top(6.0))
                        .build(ctx);
                    SingleChoiceSegmentedButtonRow::new().build(ctx, |ctx| {
                        for (index, option) in Direction::ALL.iter().enumerate() {
                            let option = *option;
                            SegmentedButton::new(direction.get() == option, {
                                clone!(direction);
                                move || direction.set(option)
                            })
                            .shape(SegmentedButtonDefaults::item_shape(index, Direction::ALL.len()))
                            .build(ctx, |ctx| {
                                Text::new(option.label()).build(ctx);
                            });
                        }
                    });

                    Text::new("The window follows immediately — the sheet stays open on purpose.")
                        .font_size(12.0)
                        .color(label_color)
                        .modifier(Modifier::new().padding_top(6.0))
                        .build(ctx);
                });
        });
}
