//! S-1 (drag past the text) and S-3 (horizontal cross-container drag), through the real pointer path.

use super::PerWindow;
use crate::components::selection_container::SelectionContainer;
use crate::layout::Constraints;
use crate::layout::components::{Column, Row};
use crate::modifier::Modifier;
use crate::text::selection::SelectionRegistrar;
use crate::theme::ThemeColors;
use std::sync::{Arc, Mutex};

type RegSlot = Arc<Mutex<Option<SelectionRegistrar>>>;

fn press(pw: &mut PerWindow, at: (f32, f32)) -> bool {
    super::pointer::handle_pointer_down(
        pw,
        at,
        crate::input::PointerKind::Mouse { button: crate::input::PointerButton::Primary },
        &winit::keyboard::ModifiersState::empty(),
        true,
    )
}
fn drag(pw: &mut PerWindow, to: (f32, f32)) -> bool {
    super::pointer::handle_pointer_move(
        pw,
        to,
        crate::input::PointerKind::Mouse { button: crate::input::PointerButton::Primary },
        &winit::keyboard::ModifiersState::empty(),
    )
}

/// One container, 300x200, holding a single 300-wide line of text at the top.
fn container_with_padding_below(text: &'static str) -> (PerWindow, RegSlot) {
    let mut pw = PerWindow::new(Box::new(|_| {}), 400.0, 400.0, ThemeColors::default_light());
    let reg: RegSlot = Default::default();
    let reg_in = reg.clone();
    pw.content = Box::new(move |ctx| {
        let r = reg_in.clone();
        SelectionContainer::new()
            .modifier(Modifier::new().size(300.0, 200.0))
            .build(ctx, move |ctx| {
                *r.lock().unwrap() = Some(crate::text::selection::LOCAL_SELECTION_REGISTRAR.current());
                crate::components::Text::new(text)
                    .modifier(Modifier::new().width(300.0))
                    .build(ctx);
            });
    });
    let content = std::mem::replace(&mut pw.content, Box::new(|_| {}));
    pw.composer.recompose(|ctx| content(ctx));
    pw.content = content;
    pw.composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
    (pw, reg)
}

/// S-1: pressing inside the text and dragging BELOW it (into the container's own padding) has to
/// select through the end of the text. It used to freeze: the move block only ran when the node
/// under the pointer had a paragraph, and padding has none.
#[test]
fn dragging_below_the_text_selects_through_its_end() {
    let (mut pw, reg) = container_with_padding_below("aaaa bbbb");
    let r = reg.lock().unwrap().clone().expect("registered");
    let total = r.total_text_len();

    assert!(press(&mut pw, (8.0, 10.0)), "the press lands in the text");
    assert!(drag(&mut pw, (8.0, 10.0)), "a move inside the text selects");
    let inside = r.build_selection().map(|s| (s.start(), s.end()));
    assert!(drag(&mut pw, (200.0, 120.0)), "a move into the padding still selects");
    let past = r.build_selection().map(|s| (s.start(), s.end()));

    assert!(past.is_some(), "dragging past the text keeps a selection ({inside:?} -> {past:?})");
    assert_eq!(past.map(|(_, e)| e), Some(total), "it reaches the end of the text");
}

/// Two containers side by side (a Row), each 200x60 with one text.
fn two_containers_in_a_row() -> (PerWindow, RegSlot, RegSlot) {
    let mut pw = PerWindow::new(Box::new(|_| {}), 600.0, 300.0, ThemeColors::default_light());
    let a: RegSlot = Default::default();
    let b: RegSlot = Default::default();
    let (a_in, b_in) = (a.clone(), b.clone());
    pw.content = Box::new(move |ctx| {
        let (a_body, b_body) = (a_in.clone(), b_in.clone());
        Row::new().build(ctx, move |ctx| {
            let an = a_body.clone();
            SelectionContainer::new()
                .modifier(Modifier::new().size(200.0, 60.0))
                .build(ctx, move |ctx| {
                    *an.lock().unwrap() = Some(crate::text::selection::LOCAL_SELECTION_REGISTRAR.current());
                    crate::components::Text::new("aaaa bbbb").modifier(Modifier::new().width(200.0)).build(ctx);
                });
            let bn = b_body.clone();
            SelectionContainer::new()
                .modifier(Modifier::new().size(200.0, 60.0))
                .build(ctx, move |ctx| {
                    *bn.lock().unwrap() = Some(crate::text::selection::LOCAL_SELECTION_REGISTRAR.current());
                    crate::components::Text::new("cccc dddd").modifier(Modifier::new().width(200.0)).build(ctx);
                });
        });
    });
    let content = std::mem::replace(&mut pw.content, Box::new(|_| {}));
    pw.composer.recompose(|ctx| content(ctx));
    pw.content = content;
    pw.composer.layout(Constraints::new(0.0, 600.0, 0.0, 300.0));
    (pw, a, b)
}

/// S-3: press inside B's text and drag LEFT into A — the anchor is B, so the selection has to reach
/// B's START. A y-only direction test answered "the end" for both directions.
#[test]
fn dragging_left_into_another_container_reaches_the_anchor_start() {
    let (mut pw, _a, b) = two_containers_in_a_row();
    let reg = b.lock().unwrap().clone().expect("B registered");

    // B occupies x 200..400; its text runs along the top.
    assert!(press(&mut pw, (208.0, 10.0)), "the press lands in B's text");
    assert!(drag(&mut pw, (100.0, 10.0)), "dragging left into A updates the selection");
    let sel = reg.build_selection().map(|s| (s.start(), s.end()));
    assert_eq!(sel, Some((0, 1)), "leftward drag reaches B's start, not its end");
}

/// The same press dragged RIGHT (into the empty area beside or past B) reaches B's END.
#[test]
fn dragging_right_reaches_the_anchor_end() {
    let (mut pw, _a, b) = two_containers_in_a_row();
    let reg = b.lock().unwrap().clone().expect("B registered");
    assert!(press(&mut pw, (208.0, 10.0)));
    assert!(drag(&mut pw, (390.0, 10.0)));
    let sel = reg.build_selection().map(|s| (s.start(), s.end()));
    assert_eq!(sel.map(|(_, e)| e), Some(reg.total_text_len()));
}
