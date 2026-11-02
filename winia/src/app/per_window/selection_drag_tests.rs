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

/// Keep the multi-click window fresh between presses.
///
/// The window is 300 ms of real time, and the suite runs under load: a test thread that gets
/// descheduled between the two presses would turn the second click of a double click into a fresh
/// single click and fail for no reason. Only the clock is refreshed — the position and the count stay
/// whatever the press recorded, so the rules under test (slop, wrap-around) are unchanged.
fn refresh_click_clock(pw: &mut PerWindow, at: (f32, f32), count: u8) {
    pw.input.selection_click = Some((std::time::Instant::now(), at, count));
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

/// M-2 through the real pointer path: a drag has to reach a bound `SelectionState`, not only the
/// registrar's own callback.
#[test]
fn a_drag_publishes_into_a_bound_selection_state() {
    use crate::components::selection_container::SelectionState;
    use std::sync::Arc;

    let mut pw = PerWindow::new(Box::new(|_| {}), 400.0, 300.0, ThemeColors::default_light());
    let state = SelectionState::new();
    let state_in = state.clone();
    pw.content = Box::new(move |ctx| {
        SelectionContainer::new()
            .state(state_in.clone())
            .modifier(Modifier::new().size(300.0, 200.0))
            .build(ctx, |ctx| {
                crate::components::Text::new("aaaa bbbb")
                    .modifier(Modifier::new().width(300.0))
                    .build(ctx);
            });
    });
    let content = std::mem::replace(&mut pw.content, Box::new(|_| {}));
    pw.composer.recompose(|ctx| content(ctx));
    pw.content = content;
    pw.composer.layout(Constraints::new(0.0, 400.0, 0.0, 300.0));

    assert!(state.selection().is_none(), "nothing selected before the drag");
    assert!(press(&mut pw, (8.0, 10.0)), "the press lands in the text");
    assert!(drag(&mut pw, (60.0, 10.0)), "the move selects");

    // The registrar is read straight out of the arena rather than captured in the closure: the
    // point of this test is the pointer path, not the registration.
    let reg = {
        let nodes = pw.composer.arena_nodes();
        let mut found = None;
        for n in nodes.iter() {
            if let Some(r) = n.registrar.borrow().as_ref() { found = Some(r.clone()); }
        }
        found.expect("the text registered")
    };
    reg.fire_on_change(); // what a pointer release does
    let published = state.selected_text();
    assert!(published.is_some(), "the state saw the drag: {published:?}");
    assert!(published.unwrap_or_default().starts_with('a'), "and it is the dragged text");
}
// ── M-5: double click selects the word, triple click the line ──

/// Two presses inside the same word, close in time and place: the second one is a double click.
#[test]
fn a_double_click_selects_the_word_under_the_pointer() {
    let (mut pw, reg) = container_with_padding_below("aaaa bbbb");
    let r = reg.lock().unwrap().clone().expect("registered");

    assert!(press(&mut pw, (60.0, 10.0)), "the press lands in the second word");
    refresh_click_clock(&mut pw, (60.0, 10.0), 1);
    assert!(press(&mut pw, (60.0, 10.0)), "the second press of the double click");

    assert_eq!(
        r.build_selection().map(|s| s.text().to_string()),
        Some("bbbb".to_string()),
        "a double click selects the whole word, not the two characters it covers"
    );
}

/// A third press turns it into a line selection.
#[test]
fn a_triple_click_selects_the_whole_line() {
    let (mut pw, reg) = container_with_padding_below("aaaa bbbb");
    let r = reg.lock().unwrap().clone().expect("registered");

    assert!(press(&mut pw, (60.0, 10.0)));
    refresh_click_clock(&mut pw, (60.0, 10.0), 1);
    assert!(press(&mut pw, (60.0, 10.0)));
    refresh_click_clock(&mut pw, (60.0, 10.0), 2);
    assert!(press(&mut pw, (60.0, 10.0)));

    assert_eq!(
        r.build_selection().map(|s| s.text().to_string()),
        Some("aaaa bbbb".to_string()),
        "a triple click selects the paragraph (the line)"
    );
}

/// A second press further away is a new single click, not a double click: nothing is word-selected.
#[test]
fn a_second_press_elsewhere_is_not_a_double_click() {
    let (mut pw, reg) = container_with_padding_below("aaaa bbbb");
    let r = reg.lock().unwrap().clone().expect("registered");

    assert!(press(&mut pw, (10.0, 10.0)));
    assert!(press(&mut pw, (90.0, 10.0)), "80 px away is outside the multi-click slop");

    assert!(
        r.build_selection().is_none(),
        "a click elsewhere starts a new selection, so nothing is selected yet"
    );
}
/// S-8: a SECOND composer laying out must not touch this one's table.
///
/// The sweep that empties a container whose last text left used to be thread-wide, so any other
/// window — or an overlay layer — laying out with no selectable text emptied every container on the
/// thread. Measured before the fix: window A went from `total=11` to `total=0` and its selection from
/// `Some("hello world")` to `None`, from nothing but window B's layout.
#[test]
fn a_second_window_laying_out_keeps_the_first_windows_table() {
    let (_pw_a, reg_a) = container_with_padding_below("hello world");
    let r = reg_a.lock().unwrap().clone().expect("registered");
    r.set_selection(0, 11);

    let mut pw_b = PerWindow::new(Box::new(|_| {}), 300.0, 200.0, ThemeColors::default_light());
    pw_b.content = Box::new(|ctx| { crate::components::Text::new("plain").build(ctx); });
    let content = std::mem::replace(&mut pw_b.content, Box::new(|_| {}));
    pw_b.composer.recompose(|ctx| content(ctx));
    pw_b.content = content;
    pw_b.composer.layout(Constraints::new(0.0, 300.0, 0.0, 200.0));

    assert_eq!(r.total_text_len(), 11, "window A keeps its runs");
    assert_eq!(
        r.build_selection().map(|s| s.text().to_string()),
        Some("hello world".to_string()),
        "and its selection"
    );
}

/// S-9: a press in another window leaves this window's selection alone.
///
/// The press sweep was thread-wide too (measured before the fix: a press on window B's padding turned
/// window A's `Some("hello")` into `None`).
#[test]
fn a_press_in_another_window_keeps_this_windows_selection() {
    let (_pw_a, reg_a) = container_with_padding_below("hello world");
    let (mut pw_b, _reg_b) = container_with_padding_below("second");
    let r = reg_a.lock().unwrap().clone().expect("registered");
    r.set_selection(0, 5);

    assert!(press(&mut pw_b, (200.0, 200.0)), "the press lands on window B's padding");

    assert_eq!(
        r.build_selection().map(|s| s.text().to_string()),
        Some("hello".to_string()),
        "window A's selection survives a press in window B"
    );
}

/// And the sweep still does its job inside the window it belongs to.
#[test]
fn a_press_in_the_same_window_still_ends_its_selection() {
    let (mut pw, reg) = container_with_padding_below("aaaa bbbb");
    let r = reg.lock().unwrap().clone().expect("registered");
    r.set_selection(0, 4);
    assert!(r.build_selection().is_some());
    assert!(press(&mut pw, (200.0, 120.0)), "the press lands on the container's padding");
    assert!(
        r.build_selection().is_none(),
        "a press inside the container still ends the selection"
    );
}
/// A container inside a container keeps its own table: the inner one must not steal the outer's
/// runs, and a text after the inner container belongs to the outer again.
#[test]
fn nested_containers_keep_their_own_tables() {
    let outer: Arc<Mutex<Option<SelectionRegistrar>>> = Default::default();
    let inner: Arc<Mutex<Option<SelectionRegistrar>>> = Default::default();
    let mut pw = PerWindow::new(Box::new(|_| {}), 400.0, 300.0, ThemeColors::default_light());
    let (o, i) = (outer.clone(), inner.clone());
    pw.content = Box::new(move |ctx| {
        let o = o.clone();
        let i = i.clone();
        SelectionContainer::new().modifier(Modifier::new().size(300.0, 200.0)).build(ctx, move |ctx| {
            *o.lock().unwrap() = Some(crate::text::selection::LOCAL_SELECTION_REGISTRAR.current());
            crate::components::Text::new("aa").build(ctx);
            let i2 = i.clone();
            SelectionContainer::new().modifier(Modifier::new().size(200.0, 100.0)).build(ctx, move |ctx| {
                *i2.lock().unwrap() = Some(crate::text::selection::LOCAL_SELECTION_REGISTRAR.current());
                crate::components::Text::new("bb").build(ctx);
            });
            crate::components::Text::new("cc").build(ctx);
        });
    });
    let content = std::mem::replace(&mut pw.content, Box::new(|_| {}));
    pw.composer.recompose(|ctx| content(ctx));
    pw.content = content;
    pw.composer.layout(Constraints::new(0.0, 400.0, 0.0, 300.0));
    let o = outer.lock().unwrap().clone();
    let i = inner.lock().unwrap().clone();
    let (o, i) = (o.expect("outer registrar"), i.expect("inner registrar"));
    assert!(!o.is_same(&i), "two containers, two registrars");
    assert_eq!(o.selectable_texts(), vec!["aa".to_string(), "cc".to_string()], "the outer keeps its own runs");
    assert_eq!(i.selectable_texts(), vec!["bb".to_string()], "the inner holds only its own");
    assert_eq!(o.total_text_len(), 4);
    assert_eq!(i.total_text_len(), 2);
}

/// The container's group still Skips: M-1 hangs a `CopySelectionKey` on its modifier, and a modifier
/// that compares unequal to itself would re-enter the whole subtree on every frame.
#[test]
fn a_container_body_skips_a_frame_with_no_changes() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let body = Arc::new(AtomicUsize::new(0));
    let mut pw = PerWindow::new(Box::new(|_| {}), 400.0, 300.0, ThemeColors::default_light());
    let b = body.clone();
    pw.content = Box::new(move |ctx| {
        let b = b.clone();
        SelectionContainer::new().build(ctx, move |ctx| {
            b.fetch_add(1, Ordering::SeqCst);
            crate::components::Text::new("hello").build(ctx);
        });
    });
    let content = std::mem::replace(&mut pw.content, Box::new(|_| {}));
    for _ in 0..3 {
        pw.composer.recompose(|ctx| content(ctx));
        pw.composer.layout(Constraints::new(0.0, 400.0, 0.0, 300.0));
    }
    pw.content = content;
    assert_eq!(body.load(Ordering::SeqCst), 1, "the body ran once, not once per frame");
}
/// S-10: a `SelectionState` has to be released when its container leaves composition.
///
/// Compose does it in a `DisposableEffect` (`state.manager = null; state.selection = null`); without
/// the release the state stayed bound to a dead container, so handing it to the next one panicked
/// (measured: frame 3 of this test died with "A SelectionState can only be bound to one
/// SelectionContainer") and the stale selection kept being published.
#[test]
fn a_state_binds_again_after_its_container_leaves() {
    use crate::components::selection_container::SelectionState;
    use crate::runtime::state::State;
    let show = State::new(true);
    let state = SelectionState::new();
    let mut pw = PerWindow::new(Box::new(|_| {}), 400.0, 300.0, ThemeColors::default_light());
    let st = state.clone();
    let show_in = show.clone();
    pw.content = Box::new(move |ctx| {
        let st = st.clone();
        let show = show_in.clone();
        Column::new().build(ctx, move |ctx| {
            let st = st.clone();
            let visible = show.get();
            if visible {
                SelectionContainer::new()
                    .state(st)
                    .modifier(Modifier::new().size(200.0, 100.0))
                    .build(ctx, |ctx| { crate::components::Text::new("aa").build(ctx); });
            }
        });
    });
    let content = std::mem::replace(&mut pw.content, Box::new(|_| {}));
    pw.composer.recompose(|ctx| content(ctx));
    pw.composer.layout(Constraints::new(0.0, 400.0, 0.0, 300.0));
    show.set(false);
    pw.composer.recompose(|ctx| content(ctx));
    pw.composer.layout(Constraints::new(0.0, 400.0, 0.0, 300.0));
    show.set(true);
    pw.composer.recompose(|ctx| content(ctx));
    pw.composer.layout(Constraints::new(0.0, 400.0, 0.0, 300.0));
    pw.content = content;
    assert!(
        state.selection().is_none(),
        "the state publishes nothing once its container is gone"
    );
}


/// S-12: a drag that leaves every node's box still extends the selection.
///
/// The move used to be dropped when the hit test found nothing (the pointer past the content, or out
/// of the window): the whole drag block sat behind `if let Some(&innermost) = path.last()`, so the
/// selection froze at its last value (measured: `3..6` while the pointer moved on to y=280). It now
/// falls back to the text the press anchored on, whose paragraph clamps the pointer — the same
/// fallback that already covered "drag past the end of the line" while still inside the container.
#[test]
fn dragging_out_of_every_node_still_extends_the_selection() {
    let (mut pw, a, _b) = two_containers_in_a_row();
    let r = a.lock().unwrap().clone().expect("A registered");
    let total = r.total_text_len();

    assert!(press(&mut pw, (20.0, 10.0)), "the press lands in A's text");
    assert!(drag(&mut pw, (40.0, 10.0)), "a move inside the text selects");
    assert_eq!(r.build_selection().map(|s| (s.start(), s.end())), Some((3, 6)));

    // (380, 280) is below both containers and past the window content: no node contains it.
    assert!(drag(&mut pw, (380.0, 280.0)), "the move still belongs to the anchored text");
    assert_eq!(
        r.build_selection().map(|s| (s.start(), s.end())),
        Some((3, total)),
        "dragging out of the tree reaches the end of the anchored run"
    );
}

/// Auto-scroll: a selection drag that rests in the container's bottom edge band keeps scrolling, and
/// the selection follows the content instead of stopping at the viewport.
///
/// Compose's `isSelectionAutoScrollEnabled`. Without it the pointer stops at the edge and the content
/// never moves, so a selection can never reach past the first screenful.
#[test]
fn dragging_at_the_edge_scrolls_and_the_selection_follows() {
    let text: String = (0..40).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
    let mut pw = PerWindow::new(Box::new(|_| {}), 400.0, 300.0, ThemeColors::default_light());
    let scroll = crate::modifier::ScrollState::new();
    let reg: RegSlot = Default::default();
    let reg_in = reg.clone();
    let scroll_in = scroll.clone();
    pw.content = Box::new(move |ctx| {
        // The content closure runs on every recomposition, so the handles are cloned per run
        // instead of being moved out of the captured values.
        let r = reg_in.clone();
        let scroll_run = scroll_in.clone();
        let text_run = text.clone();
        SelectionContainer::new()
            .modifier(Modifier::new().size(300.0, 100.0))
            .build(ctx, move |ctx| {
                *r.lock().unwrap() = Some(crate::text::selection::LOCAL_SELECTION_REGISTRAR.current());
                Column::new()
                    .modifier(Modifier::new().fill_max_size().vertical_scroll(scroll_run))
                    .build(ctx, move |ctx| {
                        crate::components::Text::new(text_run)
                            .modifier(Modifier::new().width(300.0))
                            .build(ctx);
                    });
            });
    });
    let content = std::mem::replace(&mut pw.content, Box::new(|_| {}));
    pw.composer.recompose(|ctx| content(ctx));
    pw.content = content;
    pw.composer.layout(Constraints::new(0.0, 400.0, 0.0, 300.0));
    let reg = reg.lock().unwrap().clone().expect("registered");

    // Press on the first line, then hold the pointer against the container's bottom edge (y=95 of a
    // 100-tall viewport).
    assert!(press(&mut pw, (20.0, 10.0)), "the press lands in the text");
    assert!(drag(&mut pw, (20.0, 95.0)), "the move selects");
    pw.input.last_pos = Some((20.0, 95.0));
    let before = reg.build_selection().map(|s| s.end()).unwrap_or(0);

    // Frames: each tick scrolls by velocity*dt and re-runs the drag at the held position.
    let mut ticks = 0;
    for _ in 0..40 {
        pw.selection_autoscroll_tick();
        ticks += 1;
        std::thread::sleep(std::time::Duration::from_millis(16));
        if scroll.offset.get() > 200.0 { break; }
    }
    let offset = scroll.offset.get();
    let after = reg.build_selection().map(|s| s.end()).unwrap_or(0);
    assert!(offset > 1.0, "the container has to scroll ({ticks} ticks, offset {offset})");
    assert!(
        after > before,
        "the selection follows the content: {before} -> {after} (offset {offset})"
    );
}
/// Keyboard selection needs a target: the press records which container it landed in, because a
/// selection container has no focused node of its own to route keys through.
#[test]
fn a_press_records_the_container_for_keyboard_keys() {
    let (mut pw, reg) = container_with_padding_below("aaaa bbbb");
    let r = reg.lock().unwrap().clone().expect("registered");

    assert!(press(&mut pw, (8.0, 10.0)));
    let recorded = pw.input.last_selection_registrar.clone();
    assert!(
        recorded.as_ref().map(|x| x.is_same(&r)).unwrap_or(false),
        "the press has to name the container its keys belong to"
    );
}
/// The window key path: Ctrl+A after a press selects the whole container, and the change is
/// announced (the state, the caller's callback and the copy path all hang off `fire_on_change`).
#[test]
fn ctrl_a_through_the_window_key_path_selects_the_whole_container() {
    use crate::input::{KbEvent, KbEventType};
    let (mut pw, reg) = container_with_padding_below("aaaa bbbb");
    let r = reg.lock().unwrap().clone().expect("registered");
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen_in = seen.clone();
    r.set_on_change(move |sel| seen_in.lock().unwrap().push(sel.text().to_string()));

    assert!(press(&mut pw, (8.0, 10.0)), "the press lands in the text");
    let ke = KbEvent {
        key: winit::keyboard::Key::Character("a".into()),
        event_type: KbEventType::KeyDown,
        is_alt_pressed: false,
        is_ctrl_pressed: true,
        is_shift_pressed: false,
        is_meta_pressed: false,
        repeat: false,
    };
    assert!(
        crate::app::app_state::try_selection_key(&mut pw, &ke),
        "Ctrl+A belongs to the container the press landed in"
    );
    assert_eq!(
        r.build_selection().map(|s| (s.start(), s.end())),
        Some((0, r.total_text_len())),
        "everything is selected"
    );
    assert_eq!(seen.lock().unwrap().last().map(|s| s.as_str()), Some("aaaa bbbb"), "the change was announced");
}