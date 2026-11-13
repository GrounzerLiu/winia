//! Checkbox demo — the two constructors, the six colour states, the node size that follows whether
//! the control takes clicks, and the select-all group the rest of the codebase cites.
//!
//! The page walks the six states (`Off`, `On`, `Indeterminate`, each enabled and disabled), then the
//! 48/24 dp node the signature does not mention, then a parent/child select-all group, then the two
//! ways a caller drives one, then the colour overrides, then an interaction source a caller can
//! hoist.
//!
//! Try: click the select-all row — it sets all three children at once and reads `Indeterminate`
//! while only some of them are on; toggle a single child and watch the parent follow. Press the
//! palette button in the colours card: the BOX follows at paint time, but the CHECKMARK repaints
//! only because the component declares the colour as a group parameter. Hover and press the last
//! checkbox to watch its interaction state report.
//!
//! Run with: `cargo run -p winia --example checkbox_demo`.

use letclone::clone;
use winia::prelude::*;

#[path = "common/settings.rs"]
mod settings;

pub fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(760.0, 1240.0)
                .title("Checkbox demo")
                .build(ctx, |ctx| {
                    settings::shell("Checkbox", ctx, demo_body);
                });
        });
    });
}

#[composable]
fn demo_body(ctx: &mut ComposeCtx, scaffold: ScaffoldContentPadding) {
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
                "A checkbox is a CONTROLLED control: it keeps no state of its own, it reports the \
                 value it wants, and the next pass draws whatever the caller passed back in. \
                 `Checkbox::new(bool)` is the two-state form; `TriStateCheckbox::new(ToggleableState)` \
                 adds `Indeterminate`, the dash a parent shows while only some of its children are on.",
            )
            .font_size(14.0)
            .build(ctx);

            states_card(ctx);
            sizes_card(ctx);
            select_all_card(ctx);
            governed_card(ctx);
            overrides_card(ctx);
            interaction_card(ctx);
        });
}

/// The six colour states — `enabled × state`, the two dimensions M3 resolves a checkbox on.
#[composable]
fn states_card(ctx: &mut ComposeCtx) {
    section(
        ctx,
        "Six states, two dimensions",
        "The box and the border split on BOTH `enabled` and the state. The checkmark splits on the \
         state alone: its two disabled rows repeat the enabled colours above them, because M3's \
         default branch calls `checkmarkColor(state)` and never looks at `enabled` \
         (`ComposeMaterial3Flags.kt:49`). That is the behaviour to match, not an omission — section \
         5 of `docs/checkbox.md` records the token the flag-on path would use instead.",
        |ctx| {
            FlowRow::new()
                .main_spacing(28.0)
                .cross_spacing(14.0)
                .build(ctx, |ctx| {
                    for (label, state, enabled) in [
                        ("Off", ToggleableState::Off, true),
                        ("On", ToggleableState::On, true),
                        ("Indeterminate", ToggleableState::Indeterminate, true),
                        ("Off, disabled", ToggleableState::Off, false),
                        ("On, disabled", ToggleableState::On, false),
                        (
                            "Indeterminate, disabled",
                            ToggleableState::Indeterminate,
                            false,
                        ),
                    ] {
                        state_sample(ctx, label, state, enabled);
                    }
                });
        },
    );
}

/// One checkbox with its caption underneath, at its natural size.
#[composable]
fn state_sample(ctx: &mut ComposeCtx, label: &'static str, state: ToggleableState, enabled: bool) {
    let caption = WiniaTheme::colors().on_surface_variant;
    Column::new().spacing(6.0).build(ctx, |ctx| {
        // `on_click` even when disabled: M3's minimum-interactive-size gate is `onClick != null`, so a
        // disabled checkbox that still HAS a handler keeps its 48 dp target (the next card shows why
        // that matters). Without a handler every one of these would be 24 dp and the row would jog.
        TriStateCheckbox::new(state)
            .enabled(enabled)
            .on_click(|| {})
            .modifier(Modifier::new().semantics(SemanticsConfig::new().content_description(label)))
            .build(ctx);
        Text::new(label).font_size(11.0).color(caption).build(ctx);
    });
}

/// The node is 48 dp or 24 dp, decided by whether the control takes clicks.
#[composable]
fn sizes_card(ctx: &mut ComposeCtx) {
    let backdrop = WiniaTheme::colors().surface_container_highest;
    section(
        ctx,
        "48 dp or 24 dp",
        "M3 applies `minimumInteractiveComponentSize()` only when `onClick != null`. A checkbox with \
         a handler is a 48 dp node with its 20 dp box centred; one without is the box plus 2 dp of \
         padding a side — 24 dp. Disabled does not decide it: a disabled checkbox that still has a \
         handler keeps the 48 dp target, which is what the third sample below shows. The tinted slot \
         is the point — both sizes hold the same 20 dp box, so without something behind them the two \
         are indistinguishable.",
        move |ctx| {
            FlowRow::new()
                .main_spacing(20.0)
                .cross_spacing(14.0)
                .build(ctx, |ctx| {
                    sized_sample(ctx, backdrop, "no handler — 24 dp", false, true);
                    sized_sample(ctx, backdrop, "handler — 48 dp", true, true);
                    sized_sample(ctx, backdrop, "disabled, handler — 48 dp", true, false);
                });
        },
    );
}

/// One checkbox on a tinted backdrop, so its NODE size is visible rather than inferred.
#[composable]
fn sized_sample(
    ctx: &mut ComposeCtx,
    backdrop: Color,
    label: &'static str,
    takes_clicks: bool,
    enabled: bool,
) {
    let caption = WiniaTheme::colors().on_surface_variant;
    Column::new().spacing(6.0).build(ctx, |ctx| {
        Stack::new()
            .alignment(Alignment::Center)
            .modifier(Modifier::new().background(backdrop, Shape::rounded(8.0)))
            .build(ctx, |ctx| {
                let checkbox = TriStateCheckbox::new(ToggleableState::On).enabled(enabled);
                match takes_clicks {
                    true => checkbox.on_click(|| {}),
                    false => checkbox,
                }
                .modifier(
                    Modifier::new().semantics(SemanticsConfig::new().content_description(label)),
                )
                .build(ctx);
            });
        Text::new(label).font_size(11.0).color(caption).build(ctx);
    });
}

/// The select-all group: one parent, three children, and a parent state DERIVED from them.
#[composable]
fn select_all_card(ctx: &mut ComposeCtx) {
    // Each child is its own `State`, so the parent has nothing of its own to drift out of sync.
    let first = ctx.remember(|| true);
    let second = ctx.remember(|| true);
    let third = ctx.remember(|| false);

    section(
        ctx,
        "A parent derived from its children",
        "The parent stores no state: it recomputes from the three children every pass, so \
         `Indeterminate` cannot disagree with them. Clicking it moves all three to the value that \
         makes them agree — the CHILDREN's handlers do the writing, not the parent's. Toggle a single \
         child afterwards and the parent recomputes to `Indeterminate`.",
        move |ctx| {
            Column::new().spacing(2.0).build(ctx, |ctx| {
                // The derivation runs in THIS closure. A read made here registers on the card's own
                // scope, so the whole group re-runs whenever a child moves; computing it one level up
                // would register on the parent, which then has no reason to re-enter.
                let all = first.get() && second.get() && third.get();
                let none = !first.get() && !second.get() && !third.get();
                let parent = if all {
                    ToggleableState::On
                } else if none {
                    ToggleableState::Off
                } else {
                    ToggleableState::Indeterminate
                };
                labelled(ctx, "All", parent, {
                    clone!(first, second, third);
                    move || {
                        let target = !(first.get() && second.get() && third.get());
                        first.set(target);
                        second.set(target);
                        third.set(target);
                    }
                });
                // The children are one level in, as they would be in a real list.
                for (label, value) in [
                    ("First", first.clone()),
                    ("Second", second.clone()),
                    ("Third", third.clone()),
                ] {
                    // An explicit key: the per-base key counter is handed out by execution order, so a
                    // list whose iteration count can change would collide with its own history.
                    ctx.key(label, |ctx| {
                        Row::new()
                            .spacing(8.0)
                            // Indent the children past the parent's box, so the tree reads as a tree
                            // instead of four peers. `padding_start` rather than a left padding keeps
                            // it on the leading edge in RTL (the settings sheet switches direction).
                            .modifier(Modifier::new().padding_start(24.0))
                            // The children are built through `row`, not the outer `ctx`: the scope
                            // already holds the context mutably (`RowScope` derefs to it), so naming
                            // both would be two overlapping borrows.
                            .build(ctx, |row| {
                                TriStateCheckbox::new(ToggleableState::from_bool(value.get()))
                                    .on_click({
                                        clone!(value);
                                        move || value.set(!value.get())
                                    })
                                    .modifier(Modifier::new().semantics(
                                        SemanticsConfig::new().content_description(label),
                                    ))
                                    .build(row);
                                Text::new(label).font_size(14.0).build(row);
                            });
                    });
                }
            });
        },
    );
}

/// One labelled checkbox — the label is a SIBLING, so the control has to be named explicitly.
#[composable]
fn labelled(
    ctx: &mut ComposeCtx,
    label: &'static str,
    state: ToggleableState,
    on_click: impl Fn() + Send + Sync + 'static,
) {
    Row::new()
        .spacing(8.0)
        .modifier(Modifier::new().padding_vertical(2.0))
        .build(ctx, |ctx| {
            TriStateCheckbox::new(state)
                .on_click(on_click)
                // A label NEXT TO a control is not associated with it — Compose has the same rule.
                // Without this the checkbox announces an empty name and a screen reader has nothing
                // to say beyond the role. `docs/semantics.md` records the gap this closes.
                .modifier(
                    Modifier::new().semantics(SemanticsConfig::new().content_description(label)),
                )
                .build(ctx);
            Text::new(label).font_size(14.0).build(ctx);
        });
}

/// The control is CONTROLLED: the value lives in the caller, and a dropped report draws nothing.
#[composable]
fn governed_card(ctx: &mut ComposeCtx) {
    let agreed = ctx.remember(|| false);
    let caption = WiniaTheme::colors().on_surface_variant;

    section(
        ctx,
        "The value lives in the caller",
        "A checkbox never keeps its own `checked`. It reports the value it wants and the next pass \
         draws whatever the caller passed — which is why the row below and the buttons both read the \
         same `State` and always agree. A handler that DROPS the report snaps back on the next pass, \
         because nothing stored the new value.",
        move |ctx| {
            let now = agreed.get();
            Column::new().spacing(12.0).build(ctx, |ctx| {
                labelled(ctx, "Agreed", ToggleableState::from_bool(now), {
                    clone!(agreed);
                    move || agreed.update(|value| *value = !*value)
                });

                Row::new().spacing(8.0).build(ctx, |ctx| {
                    Checkbox::new(now)
                        // The report goes nowhere: this is the mistake the paragraph warns about, and
                        // it is visible — clicking it repaints the OLD value immediately.
                        .on_checked_change(|_| {})
                        .modifier(Modifier::new().semantics(
                            SemanticsConfig::new().content_description("Report dropped"),
                        ))
                        .build(ctx);
                    Text::new("handler drops the report — it snaps back")
                        .font_size(12.0)
                        .color(caption)
                        .build(ctx);
                });

                // A `FlowRow`, not a `Row`: a control row that runs off the card is a click the UI
                // test cannot reach (docs/component-authoring.md section 10).
                FlowRow::new()
                    .main_spacing(8.0)
                    .cross_spacing(8.0)
                    .build(ctx, |ctx| {
                        Button::new()
                            .modifier(Modifier::new().test_tag("checkbox-set-on"))
                            .on_click({
                                clone!(agreed);
                                move || agreed.set(true)
                            })
                            .build(ctx, |ctx| {
                                Text::new("Set on").build(ctx);
                            });
                        Button::new()
                            .style(ButtonStyle::Tonal)
                            .modifier(Modifier::new().test_tag("checkbox-set-off"))
                            .on_click({
                                clone!(agreed);
                                move || agreed.set(false)
                            })
                            .build(ctx, |ctx| {
                                Text::new("Set off").build(ctx);
                            });
                    });

                Text::new(format!("agreed = {now}"))
                    .font_size(12.0)
                    .color(caption)
                    .modifier(Modifier::new().test_tag("checkbox-agreed"))
                    .build(ctx);
            });
        },
    );
}

/// `colors(..)` and `copy(..)`: the two ways to replace part of the theme's palette.
#[composable]
fn overrides_card(ctx: &mut ComposeCtx) {
    let theme = WiniaTheme::colors();
    let warm = ctx.remember(|| false);

    // The six-argument form: the slots a caller actually varies. `checkedColor` feeds both the box
    // and its border; the three unchecked slots are forced transparent, as upstream does.
    let tertiary = CheckboxDefaults::colors(
        &theme,
        Some(theme.tertiary),
        Some(theme.tertiary),
        Some(theme.on_tertiary),
        None,
        None,
        None,
    );
    // The precise form: every slot is optional and `None` keeps the theme's value, so a caller names
    // only the one slot they want moved. This is that call with ONE slot set — the unchecked border —
    // which is the proof that `copy` is per-slot rather than per-group.
    let border_only = CheckboxDefaults::checkbox_colors(&theme).copy(
        None,                 // checked_checkmark
        None,                 // unchecked_checkmark
        None,                 // checked_box
        None,                 // unchecked_box
        None,                 // disabled_checked_box
        None,                 // disabled_unchecked_box
        None,                 // disabled_indeterminate_box
        None,                 // checked_border
        Some(theme.tertiary), // unchecked_border
        None,                 // disabled_border
        None,                 // disabled_unchecked_border
        None,                 // disabled_indeterminate_border
    );
    // The live pair: switching this button moves a checkbox between two palettes.
    let live = if warm.get() {
        tertiary
    } else {
        CheckboxDefaults::checkbox_colors(&theme)
    };

    section(
        ctx,
        "Replacing part of the palette",
        "`CheckboxDefaults::colors(..)` is M3's six-argument convenience form; `CheckboxColors::copy(..)` \
         is the precise one, where `None` means \"keep the source value\". Press the button below: the \
         BOX follows at paint time because it is read from a `Dyn` closure, but the CHECKMARK is \
         consumed when the icons are composed — it repaints only because the component declares that \
         colour as a group parameter. Without that declaration the glyph keeps the colour it was \
         built with.",
        move |ctx| {
            FlowRow::new()
                .main_spacing(28.0)
                .cross_spacing(14.0)
                .build(ctx, |ctx| {
                    palette_sample(ctx, "theme", CheckboxDefaults::checkbox_colors(&theme));
                    palette_sample(ctx, "colors(..) — tertiary", tertiary);
                    palette_sample(ctx, "copy(..) — border only", border_only);
                    palette_sample(ctx, "live", live);
                });

            FlowRow::new()
                .main_spacing(8.0)
                .cross_spacing(8.0)
                .build(ctx, |ctx| {
                    Button::new()
                        .modifier(Modifier::new().test_tag("checkbox-palette"))
                        .on_click({
                            clone!(warm);
                            move || warm.update(|value| *value = !*value)
                        })
                        .build(ctx, |ctx| {
                            Text::new(if warm.get() {
                                "Back to the theme"
                            } else {
                                "Switch the live palette"
                            })
                            .build(ctx);
                        });
                });
        },
    );
}

/// One checked checkbox in a given palette, captioned underneath.
#[composable]
fn palette_sample(ctx: &mut ComposeCtx, label: &'static str, colors: CheckboxColors) {
    let caption = WiniaTheme::colors().on_surface_variant;
    Column::new().spacing(6.0).build(ctx, |ctx| {
        Checkbox::new(true)
            .colors(colors)
            .on_checked_change(|_| {})
            .modifier(Modifier::new().semantics(SemanticsConfig::new().content_description(label)))
            .build(ctx);
        Text::new(label).font_size(11.0).color(caption).build(ctx);
    });
}

/// A source the caller keeps, so the flags a component normally hides become readable.
#[composable]
fn interaction_card(ctx: &mut ComposeCtx) {
    let caption = WiniaTheme::colors().on_surface_variant;
    let source = ctx.remember(MutableInteractionSource::new).get();
    let checked = ctx.remember(|| false);
    // Read inside this function's own scope, so a press re-runs the panel rather than only the
    // component that owns the source.
    let state = source.state(true);

    section(
        ctx,
        "A hoisted interaction source",
        "`interaction_source(..)` hands the component a source the CALLER keeps, which makes pressed, \
         hovered and focused readable outside it. That is what a custom state layer, a tooltip or a \
         drag handle is built from — the component's own ripple is only the default consumer.",
        move |ctx| {
            Row::new().spacing(14.0).build(ctx, |ctx| {
                Checkbox::new(checked.get())
                    .interaction_source(source.clone())
                    .on_checked_change({
                        clone!(checked);
                        move |next| checked.set(next)
                    })
                    .modifier(
                        Modifier::new().semantics(
                            SemanticsConfig::new().content_description("Hoisted source"),
                        ),
                    )
                    .build(ctx);
                Text::new(format!(
                    "pressed={} hovered={} focused={} dragged={} · checked={}",
                    state.pressed,
                    state.hovered,
                    state.focused,
                    state.dragged,
                    checked.get()
                ))
                .font_size(12.0)
                .color(caption)
                .modifier(Modifier::new().test_tag("checkbox-interaction"))
                .build(ctx);
            });
        },
    );
}

/// The page's card shell: title, explanation, then the body.
#[composable]
fn section(
    ctx: &mut ComposeCtx,
    title: &'static str,
    body: &'static str,
    content: impl FnOnce(&mut ComposeCtx),
) {
    Card::new().build(ctx, |ctx| {
        Column::new()
            .spacing(12.0)
            .modifier(Modifier::new().fill_max_width().padding(16.0))
            .build(ctx, |ctx| {
                Text::new(title).font_size(18.0).build(ctx);
                Text::new(body).font_size(14.0).build(ctx);
                content(ctx);
            });
    });
}
