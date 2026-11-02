//! D-2: a press that does not continue a selection ends it — wherever the press lands.

use super::PerWindow;
use crate::components::selection_container::SelectionContainer;
use crate::layout::Constraints;
use crate::layout::components::Column;
use crate::modifier::Modifier;
use crate::text::selection::SelectionRegistrar;
use crate::theme::ThemeColors;
use std::sync::{Arc, Mutex};

/// Two containers stacked (each 300x60, one text each) and a plain Text below them.
fn two_containers_and_a_plain_text()
    -> (PerWindow, Arc<Mutex<Option<SelectionRegistrar>>>, Arc<Mutex<Option<SelectionRegistrar>>>) {
    let mut pw = PerWindow::new(Box::new(|_| {}), 400.0, 400.0, ThemeColors::default_light());
    let a: Arc<Mutex<Option<SelectionRegistrar>>> = Default::default();
    let b: Arc<Mutex<Option<SelectionRegistrar>>> = Default::default();
    let (a_in, b_in) = (a.clone(), b.clone());
    pw.content = Box::new(move |ctx| {
        let (a_body, b_body) = (a_in.clone(), b_in.clone());
        Column::new().build(ctx, move |ctx| {
            let an = a_body.clone();
            SelectionContainer::new()
                .modifier(Modifier::new().size(300.0, 60.0))
                .build(ctx, move |ctx| {
                    *an.lock().unwrap() = Some(crate::text::selection::LOCAL_SELECTION_REGISTRAR.current());
                    crate::components::Text::new("aaaa bbbb")
                        .modifier(Modifier::new().width(300.0))
                        .build(ctx);
                });
            let bn = b_body.clone();
            SelectionContainer::new()
                .modifier(Modifier::new().size(300.0, 60.0))
                .build(ctx, move |ctx| {
                    *bn.lock().unwrap() = Some(crate::text::selection::LOCAL_SELECTION_REGISTRAR.current());
                    crate::components::Text::new("cccc dddd")
                        .modifier(Modifier::new().width(300.0))
                        .build(ctx);
                });
            crate::components::Text::new("plain output")
                .modifier(Modifier::new().size(300.0, 40.0))
                .build(ctx);
        });
    });
    let content = std::mem::replace(&mut pw.content, Box::new(|_| {}));
    pw.composer.recompose(|ctx| content(ctx));
    pw.content = content;
    pw.composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
    (pw, a, b)
}

fn press(pw: &mut PerWindow, at: (f32, f32)) -> bool {
    super::pointer::handle_pointer_down(
        pw,
        at,
        crate::input::PointerKind::Mouse { button: crate::input::PointerButton::Primary },
        &winit::keyboard::ModifiersState::empty(),
        true,
    )
}

fn select_all_in(reg: &SelectionRegistrar) {
    reg.set_selection(1, reg.total_text_len());
}

/// A press OUTSIDE the container leaves its selection alone.
///
/// Compose has no window-wide sweep: a tap only ends a selection when it reaches that container's
/// own pointer input (its `SelectionManager.onRelease`). Pressing a plain output Text under both
/// containers never reaches A, so A keeps its highlight. The press used to be swept window-wide --
/// first because the hit node carried no registrar at all (D-2), then because the sweep covered every
/// container in the window -- and both of those were over-corrections.
#[test]
fn pressing_outside_the_container_leaves_its_selection_alone() {
    let (mut pw, a, _b) = two_containers_and_a_plain_text();
    let reg = a.lock().unwrap().clone().expect("A registered");
    select_all_in(&reg);
    assert!(reg.build_selection().is_some(), "precondition: A has a selection");

    // y 130 is the plain Text under both containers (each 60 tall).
    assert!(press(&mut pw, (20.0, 130.0)), "the press has to hit something");
    assert!(
        reg.build_selection().is_some(),
        "a press outside the container never reaches it, so the selection stands"
    );
}

/// Pressing a SECOND container leaves the first alone: each container owns its selection.
///
/// Compose gives every container its own `SelectionManager`; a press in B is not delivered to A.
/// Pressing B here used to clear A because the sweep covered the whole window (measured before the
/// fix: `Some("aaaa")` -> `None`).
#[test]
fn pressing_another_container_leaves_the_first_alone() {
    let (mut pw, a, b) = two_containers_and_a_plain_text();
    let reg_a = a.lock().unwrap().clone().expect("A registered");
    let reg_b = b.lock().unwrap().clone().expect("B registered");
    select_all_in(&reg_a);

    // y 70 is inside B's container (60..120); its text sits at the top of that box.
    assert!(press(&mut pw, (20.0, 70.0)));
    assert!(
        reg_a.build_selection().is_some(),
        "a press inside B does not end A's selection"
    );
    assert!(reg_b.build_selection().is_none(), "and B has nothing selected");
    assert!(!reg_a.is_same(&reg_b), "different containers, different registrars");
}
