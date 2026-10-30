//! S-5: a selection is anchored to its own characters, so a sibling coming or going does not move
//! it, and a change to the SELECTED run's own text ends it (Compose's `onSelectableChangeCallback`).
//!
//! The runs are built under stable `ctx.key`s. That is not decoration: winia allocates slot keys by
//! call position, so inserting a run above an unkeyed one shifts every key below it, and the
//! registrar then sees the shifted key's content change — which by Compose's rule ends the
//! selection. `a_shifted_key_ends_the_selection_rather_than_moving_it` pins that case.

use super::PerWindow;
use crate::components::selection_container::SelectionContainer;
use crate::layout::Constraints;
use crate::layout::components::Column;
use crate::modifier::Modifier;
use crate::runtime::state::State;
use crate::text::selection::SelectionRegistrar;
use crate::theme::ThemeColors;
use std::sync::{Arc, Mutex};

type RegSlot = Arc<Mutex<Option<SelectionRegistrar>>>;

/// A container holding up to two runs, both keyed, both fed by a State.
fn two_rs(second_initial: &str, keys: bool) -> (PerWindow, RegSlot, State<String>, State<String>) {
    let mut pw = PerWindow::new(Box::new(|_| {}), 400.0, 400.0, ThemeColors::default_light());
    let reg: RegSlot = Default::default();
    let first = State::new(String::new());
    let second = State::new(second_initial.to_string());
    let (reg_in, first_in, second_in) = (reg.clone(), first.clone(), second.clone());
    pw.content = Box::new(move |ctx| {
        let r = reg_in.clone();
        let (f, s) = (first_in.clone(), second_in.clone());
        SelectionContainer::new()
            .modifier(Modifier::new().size(300.0, 200.0))
            .build(ctx, move |ctx| {
                *r.lock().unwrap() = Some(crate::text::selection::LOCAL_SELECTION_REGISTRAR.current());
                Column::new().build(ctx, move |ctx| {
                    // Both States are read INSIDE the column body: a read outside it never re-enters
                    // this group, so the subtree would never change (the trap in the review).
                    let (a, b) = (f.get(), s.get());
                    let mut run = |ctx: &mut crate::runtime::composer::ComposeCtx, which: u8, text: String| {
                        let build = move |ctx: &mut crate::runtime::composer::ComposeCtx| {
                            if !text.is_empty() {
                                crate::components::Text::new(text)
                                    .modifier(Modifier::new().width(300.0))
                                    .build(ctx);
                            }
                        };
                        if keys {
                            ctx.key(("selection-anchor-test", which), build);
                        } else {
                            build(ctx);
                        }
                    };
                    run(ctx, 1, a);
                    run(ctx, 2, b);
                });
            });
    });
    let content = std::mem::replace(&mut pw.content, Box::new(|_| {}));
    pw.composer.recompose(|ctx| content(ctx));
    pw.content = content;
    pw.composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
    (pw, reg, first, second)
}

fn relayout(pw: &mut PerWindow) {
    let content = std::mem::replace(&mut pw.content, Box::new(|_| {}));
    pw.composer.recompose(|ctx| content(ctx));
    pw.content = content;
    pw.composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
}

/// Selecting "BBB" and inserting "AAA" above used to leave the range at `0..3`, which then read
/// "AAA" (measured when the item was filed). The anchor keeps the selection on its own run; only
/// its global offsets move.
#[test]
fn inserting_a_keyed_sibling_above_keeps_the_selection_on_its_own_text() {
    let (mut pw, reg, first, _second) = two_rs("BBB", true);
    let r = reg.lock().unwrap().clone().expect("registered");

    r.set_selection(0, 3);
    assert_eq!(
        r.build_selection().map(|s| (s.start(), s.end(), s.text().to_string())),
        Some((0, 3, "BBB".to_string())),
        "precondition: BBB is selected"
    );

    first.set("AAA".to_string());
    relayout(&mut pw);

    assert_eq!(
        r.build_selection().map(|s| (s.start(), s.end(), s.text().to_string())),
        Some((3, 6, "BBB".to_string())),
        "still BBB, now at 3..6"
    );
}

/// The mirror case: the run above goes away. Before anchors the range was clamped to the new total
/// and the selection was lost.
#[test]
fn removing_a_keyed_sibling_above_keeps_the_selection() {
    let (mut pw, reg, first, _second) = two_rs("BBB", true);
    first.set("AAA".to_string());
    relayout(&mut pw);
    let r = reg.lock().unwrap().clone().expect("registered");

    r.set_selection(3, 6);
    assert_eq!(r.build_selection().map(|s| s.text().to_string()), Some("BBB".to_string()));

    first.set(String::new());
    relayout(&mut pw);

    assert_eq!(
        r.build_selection().map(|s| (s.start(), s.end(), s.text().to_string())),
        Some((0, 3, "BBB".to_string())),
        "the selection follows its run to the new offsets"
    );
}

/// A change to the COVERED run's own text ends the selection, the way Compose's
/// `onSelectableChangeCallback` does: its offsets no longer name the same characters. A change to a
/// run the selection does not cover leaves it alone.
#[test]
fn changing_the_selected_run_ends_the_selection_but_a_neighbour_does_not() {
    let (mut pw, reg, first, second) = two_rs("BBB", true);
    let r = reg.lock().unwrap().clone().expect("registered");

    r.set_selection(0, 2);
    assert_eq!(r.build_selection().map(|s| s.text().to_string()), Some("BB".to_string()));

    // A neighbour changing must NOT end it.
    first.set("AAA".to_string());
    relayout(&mut pw);
    assert_eq!(
        r.build_selection().map(|s| s.text().to_string()),
        Some("BB".to_string()),
        "another run's content is not this run's content"
    );

    // The covered run itself changing must.
    second.set("BBBB".to_string());
    relayout(&mut pw);
    assert!(r.build_selection().is_none(), "the covered run was rewritten");
}

/// Without a stable key the run below a newly inserted one is RE-KEYED, so the registrar sees the
/// shifted slot's content change and ends the selection. That is Compose's rule applied to winia's
/// positional identity, and it is why this file's other tests key their runs: ending is the safe
/// answer, sliding onto the newly inserted text is not.
#[test]
fn a_shifted_key_ends_the_selection_rather_than_moving_it() {
    let (mut pw, reg, first, _second) = two_rs("BBB", false);
    let r = reg.lock().unwrap().clone().expect("registered");

    r.set_selection(0, 3);
    assert_eq!(r.build_selection().map(|s| s.text().to_string()), Some("BBB".to_string()));

    first.set("AAA".to_string());
    relayout(&mut pw);

    let after = r.build_selection().map(|s| s.text().to_string());
    assert_eq!(
        after,
        None,
        "the re-keyed run reads as a content change, so Compose's rule ends the selection"
    );
}
