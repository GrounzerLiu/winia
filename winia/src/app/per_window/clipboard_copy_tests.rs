//! The copy path end to end on the winia side: a Ctrl+C in a focused `TextField` has to enqueue the
//! selected text through `crate::clipboard`, which the app's window-event handler then hands to
//! winit's `set_clipboard` (measured against the OS clipboard separately — see
//! `docs/selection-review.md` M-1).

use super::{dispatch_key_to_focus, PerWindow};
use crate::components::text_field::TextFieldValue;
use crate::input::{KbEvent, KbEventType};
use crate::layout::Constraints;
use crate::runtime::state::State;
use crate::theme::ThemeColors;

fn ctrl_c() -> KbEvent {
    KbEvent {
        key: winit::keyboard::Key::Character("c".into()),
        event_type: KbEventType::KeyDown,
        is_alt_pressed: false,
        is_ctrl_pressed: true,
        is_shift_pressed: false,
        is_meta_pressed: false,
        repeat: false,
    }
}

/// Build the window, focus the field, and hand back its value so a test can set a selection.
fn focused_field(text: &str) -> (PerWindow, State<TextFieldValue>) {
    // A TextField's effects need an active tokio runtime, exactly as every other TextField test
    // notes; `run_app` provides one in the real app. Per test, so each keeps its own.
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let _guard = rt.enter();
    let mut pw = PerWindow::new(Box::new(|_| {}), 400.0, 200.0, ThemeColors::default_light());
    let value = State::new(TextFieldValue::new(text));
    let value_in = value.clone();
    pw.content = Box::new(move |ctx| {
        let v = value_in.clone();
        crate::components::TextField::new(v)
            .modifier(crate::modifier::Modifier::new().width(300.0))
            .build(ctx);
    });
    let content = std::mem::replace(&mut pw.content, Box::new(|_| {}));
    pw.composer.recompose(|ctx| content(ctx));
    pw.content = content;
    pw.composer.layout(Constraints::new(0.0, 400.0, 0.0, 200.0));

    let first_focusable = {
        let root = pw.composer.layout_root_idx().expect("layout root");
        let nodes = pw.composer.arena_nodes();
        let mut focusable = Vec::new();
        crate::layout::node::collect_focusable_ids(nodes, root, &mut focusable);
        focusable.first().copied().expect("the field is focusable")
    };
    assert!(pw.focus_by_node_id(first_focusable), "the field takes focus");
    (pw, value)
}

/// Ctrl+C on a real selection must queue exactly the selected text.
#[test]
fn a_copy_in_a_text_field_asks_the_clipboard_for_the_selected_text() {
    crate::clipboard::take_pending();
    let (mut pw, value) = focused_field("hello world");
    value.update(|val| val.selection = 0..5);
    dispatch_key_to_focus(&mut pw, &ctrl_c());

    let queued = crate::clipboard::take_pending();
    assert_eq!(queued, vec!["hello".to_string()], "Ctrl+C queues exactly the selected text");
}

/// A copy with a collapsed caret queues nothing: an empty write would wipe what the user had.
#[test]
fn a_copy_with_no_selection_queues_nothing() {
    crate::clipboard::take_pending();
    let (mut pw, value) = focused_field("hello world");
    value.update(|val| val.selection = 3..3);
    dispatch_key_to_focus(&mut pw, &ctrl_c());

    assert!(crate::clipboard::take_pending().is_empty(), "a caret is not a selection");
}

/// Ctrl+X copies and clears: the clipboard gets the text and the field loses it.
#[test]
fn a_cut_queues_the_text_and_removes_it() {
    crate::clipboard::take_pending();
    let (mut pw, value) = focused_field("hello world");
    value.update(|val| val.selection = 0..5);
    let mut cut = ctrl_c();
    cut.key = winit::keyboard::Key::Character("x".into());
    dispatch_key_to_focus(&mut pw, &cut);

    assert_eq!(crate::clipboard::take_pending(), vec!["hello".to_string()]);
    assert_eq!(value.get().text, " world", "the cut removed the selection");
}
