//! Selection demo — drag-select across runs, the observable `SelectionState`, `DisableSelection`,
//! nested containers, multi-click words and lines, and Ctrl+C.
//!
//! Everything here is the public API: a `SelectionContainer` registers the texts below it, the
//! container's `SelectionState` is read and driven from the panel at the top, and the cards below
//! show one behaviour each so nothing is only reachable through a control.
//!
//! Try: drag across a paragraph, double click a word, triple click a line, press Select all, press
//! Extend, press Ctrl+C and paste somewhere.
//!
//! Run with: cargo run -p winia --example selection_demo

use letclone::clone;
use winia::components::{DisableSelection, SelectionState, remember_selection_state};
use winia::prelude::*;

#[path = "common/settings.rs"]
mod settings;

const CARD_BG: Color = Color {
    r: 0x2A,
    g: 0x27,
    b: 0x2E,
    a: 255,
};
const MUTED: Color = Color {
    r: 0x9E,
    g: 0x9A,
    b: 0xA6,
    a: 255,
};
const ACCENT: Color = Color {
    r: 0xD0,
    g: 0xBC,
    b: 0xFF,
    a: 255,
};
const WARM: Color = Color {
    r: 0xFF,
    g: 0xB4,
    b: 0xAB,
    a: 255,
};

/// One titled card. The body is a composition scope of its own, like every container.
#[composable]
fn card(ctx: &mut ComposeCtx, title: &str, body: impl FnOnce(&mut ComposeCtx)) {
    Column::new()
        .spacing(8.0)
        .modifier(
            Modifier::new()
                .fill_max_width()
                .padding(12.0)
                .background(CARD_BG, Shape::rounded(12.0)),
        )
        .build(ctx, |ctx| {
            Text::new(title).font_size(12.0).color(ACCENT).build(ctx);
            body(ctx);
        });
}

/// What the bound state currently holds, labelled so two cards' readouts stay distinguishable.
#[composable]
fn readout(ctx: &mut ComposeCtx, label: &str, state: &SelectionState) {
    let selection = state.selection();
    let text = state.selected_text().unwrap_or_default();
    let summary = match &selection {
        Some(sel) => format!(
            "{} .. {} — {} chars",
            sel.start(),
            sel.end(),
            text.chars().count()
        ),
        None => "no selection".to_string(),
    };
    Text::new(format!("[{label}] {summary}"))
        .font_size(12.0)
        .color(WARM)
        .modifier(Modifier::new().test_tag(&format!("sel-cover-{label}")))
        .build(ctx);
    // The text itself, quoted so an empty selection is still a line of its own.
    let quoted = if text.is_empty() {
        "(nothing)".to_string()
    } else {
        format!("\"{}\"", text.replace('\n', " "))
    };
    Text::new(format!("[{label}] {quoted}"))
        .font_size(12.0)
        .color(MUTED)
        .max_lines(2)
        .modifier(Modifier::new().test_tag(&format!("sel-text-{label}")))
        .build(ctx);
}

pub fn main() {
    winia::run_app!(|ctx| {
        WiniaTheme::auto(ctx, |ctx| {
            Window::new()
                .size(760.0, 1040.0)
                .title("Selection demo")
                .build(ctx, |ctx| {
                    // The Scaffold's content slot is a Box: everything built straight into it lands
                    // at the same origin (the cards overlapped until this Column wrapped them). The
                    // Column scrolls, because the cards are taller than the window.
                    settings::shell("Selection", ctx, |ctx| {
                        let scroll = ctx.remember(|| ScrollState::new()).get();
                        Column::new()
                            .spacing(12.0)
                            .modifier(
                                Modifier::new()
                                    .fill_max_size()
                                    .vertical_scroll(scroll)
                                    .padding(16.0),
                            )
                            .build(ctx, selection_demo);
                    });
                });
        });
    });
}

#[composable]
fn selection_demo(ctx: &mut ComposeCtx) {
    Text::new("Selection").font_size(22.0).build(ctx);
    Text::new("Drag to select, double click for a word, triple click for a line, Ctrl+C to copy.")
        .font_size(12.0)
        .color(MUTED)
        .build(ctx);

    // Every remembered state is created HERE: `remember_selection_state` needs `ctx` exclusively, so
    // it cannot be called inside a closure argument that already borrows it.

    let state = remember_selection_state(ctx);
    let disabled_state = remember_selection_state(ctx);
    let outer = remember_selection_state(ctx);
    let inner = remember_selection_state(ctx);
    let left = remember_selection_state(ctx);
    let right = remember_selection_state(ctx);
    let words = remember_selection_state(ctx);

    // ── 1. The state: observe it, and drive it ──────────────────────────────
    card(ctx, "SelectionState", {
        clone!(state);
        move |ctx| {
            SelectionContainer::new()
                .state(state.clone())
                .modifier(Modifier::new().fill_max_width())
                .build(ctx, |ctx| {
                    Column::new().spacing(4.0).build(ctx, |ctx| {
                        Text::new("The first run is selectable.")
                            .font_size(14.0)
                            .modifier(Modifier::new().fill_max_width().test_tag("sel-a"))
                            .build(ctx);
                        Text::new("The second run sits in the same container, so one drag can cross both.")
                            .font_size(14.0)
                            .modifier(Modifier::new().fill_max_width().test_tag("sel-b"))
                            .build(ctx);
                    });
                });

            readout(ctx, "main", &state);
            Row::new().spacing(8.0).build(ctx, |ctx| {
                Button::new()
                    .modifier(Modifier::new().test_tag("sel-select-all"))
                    .on_click({
                        clone!(state);
                        move || state.select_all()
                    })
                    .build(ctx, |ctx| {
                        Text::new("Select all").font_size(12.0).build(ctx)
                    });
                Button::new()
                    .modifier(Modifier::new().test_tag("sel-extend"))
                    .on_click({
                        clone!(state);
                        move || state.extend_selection_by_word()
                    })
                    .build(ctx, |ctx| {
                        Text::new("Extend by word").font_size(12.0).build(ctx)
                    });
                Button::new()
                    .modifier(Modifier::new().test_tag("sel-clear"))
                    .on_click({
                        clone!(state);
                        move || state.clear()
                    })
                    .build(ctx, |ctx| Text::new("Clear").font_size(12.0).build(ctx));
            });
        }
    });

    // ── 2. DisableSelection: the excluded run is not part of the container ──
    card(ctx, "DisableSelection", {
        clone!(disabled_state);
        move |ctx| {
            SelectionContainer::new()
                .state(disabled_state.clone())
                .modifier(Modifier::new().fill_max_width())
                .build(ctx, |ctx| {
                    Column::new().spacing(4.0).build(ctx, |ctx| {
                        Text::new("This line is selectable.")
                            .font_size(14.0)
                            .modifier(Modifier::new().fill_max_width().test_tag("sel-keep"))
                            .build(ctx);
                        DisableSelection::new().build(ctx, |ctx| {
                            Text::new("This line is excluded — Select all skips it.")
                                .font_size(14.0)
                                .color(MUTED)
                                .modifier(Modifier::new().fill_max_width().test_tag("sel-off"))
                                .build(ctx);
                        });
                        Text::new("And this one is back in.")
                            .font_size(14.0)
                            .modifier(Modifier::new().fill_max_width().test_tag("sel-keep2"))
                            .build(ctx);
                    });
                });
            readout(ctx, "disabled", &disabled_state);
            Button::new()
                .modifier(Modifier::new().test_tag("sel-disabled-select-all"))
                .on_click({
                    clone!(disabled_state);
                    move || disabled_state.select_all()
                })
                .build(ctx, |ctx| {
                    Text::new("Select all here").font_size(12.0).build(ctx)
                });
        }
    });

    // ── 3. Nested containers keep their own tables ──────────────────────────
    card(ctx, "Nested containers", {
        clone!(outer, inner);
        move |ctx| {
            SelectionContainer::new()
                .state(outer.clone())
                .modifier(Modifier::new().fill_max_width())
                .build(ctx, |ctx| {
                    Column::new().spacing(4.0).build(ctx, |ctx| {
                        Text::new("Outer run before the inner container.")
                            .font_size(14.0)
                            .modifier(Modifier::new().fill_max_width().test_tag("sel-outer-1"))
                            .build(ctx);
                        SelectionContainer::new()
                            .state(inner.clone())
                            .modifier(Modifier::new().fill_max_width().padding(8.0).background(
                                Color::from_argb(30, 208, 188, 255),
                                Shape::rounded(8.0),
                            ))
                            .build(ctx, |ctx| {
                                Text::new(
                                    "Inner run — Select all here must not take the outer text.",
                                )
                                .font_size(14.0)
                                .color(ACCENT)
                                .modifier(Modifier::new().fill_max_width().test_tag("sel-inner"))
                                .build(ctx);
                            });
                        Text::new("Outer run after the inner container.")
                            .font_size(14.0)
                            .modifier(Modifier::new().fill_max_width().test_tag("sel-outer-2"))
                            .build(ctx);
                    });
                });
            Text::new(format!(
                "outer: {} — inner: {}",
                outer
                    .selected_text()
                    .unwrap_or_else(|| "(none)".to_string()),
                inner
                    .selected_text()
                    .unwrap_or_else(|| "(none)".to_string())
            ))
            .font_size(11.0)
            .color(MUTED)
            .max_lines(2)
            .modifier(Modifier::new().test_tag("sel-nested-readout"))
            .build(ctx);
        }
    });

    // ── 4. Two sibling containers: a press in one leaves the other alone ────
    card(ctx, "Two containers side by side", {
        clone!(left, right);
        move |ctx| {
            Row::new().spacing(12.0).build(ctx, |ctx| {
                SelectionContainer::new()
                    .state(left.clone())
                    .modifier(
                        Modifier::new()
                            .width(320.0)
                            .padding(8.0)
                            .background(Color::from_argb(20, 255, 180, 171), Shape::rounded(8.0)),
                    )
                    .build(ctx, |ctx| {
                        Text::new("Left container: select me, then press the right one.")
                            .font_size(13.0)
                            .modifier(Modifier::new().fill_max_width().test_tag("sel-left"))
                            .build(ctx);
                    });
                SelectionContainer::new()
                    .state(right.clone())
                    .modifier(
                        Modifier::new()
                            .width(320.0)
                            .padding(8.0)
                            .background(Color::from_argb(20, 208, 188, 255), Shape::rounded(8.0)),
                    )
                    .build(ctx, |ctx| {
                        Text::new("Right container: pressing here leaves the left selection.")
                            .font_size(13.0)
                            .modifier(Modifier::new().fill_max_width().test_tag("sel-right"))
                            .build(ctx);
                    });
            });
            Text::new(format!(
                "left: {} — right: {}",
                left.selected_text().unwrap_or_else(|| "(none)".to_string()),
                right
                    .selected_text()
                    .unwrap_or_else(|| "(none)".to_string())
            ))
            .font_size(11.0)
            .color(MUTED)
            .max_lines(2)
            .modifier(Modifier::new().test_tag("sel-siblings-readout"))
            .build(ctx);
        }
    });

    // ── 5. Multi-click: a word, then a line ─────────────────────────────────
    card(ctx, "Double and triple click", {
        clone!(words);
        move |ctx| {
            SelectionContainer::new()
                .state(words.clone())
                .modifier(Modifier::new().fill_max_width())
                .build(ctx, |ctx| {
                    Text::new("alpha beta gamma delta\nsecond line of the same run")
                        .font_size(15.0)
                        .modifier(Modifier::new().fill_max_width().test_tag("sel-words"))
                        .build(ctx);
                });
            readout(ctx, "words", &words);
        }
    });

    Text::new("Ctrl+C copies the current selection of whichever container it belongs to.")
        .font_size(11.0)
        .color(MUTED)
        .build(ctx);
}
