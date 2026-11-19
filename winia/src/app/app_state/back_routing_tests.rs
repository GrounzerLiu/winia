use super::*;
use crate::back::inbox::{self, Signal};
use crate::theme::ThemeColors;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// Windows' winit 0.30 has `WindowId::dummy()` but no public `From<u64>`, so a unit test has no
/// way to name two distinct windows. The values below are only hashed and compared, never
/// dereferenced as HWNDs.
#[cfg(windows)]
fn test_window_id(raw: u64) -> winit::window::WindowId {
    debug_assert_eq!(
        std::mem::size_of::<usize>(),
        std::mem::size_of::<winit::window::WindowId>()
    );
    unsafe { std::mem::transmute::<usize, winit::window::WindowId>(raw as usize) }
}

#[cfg(not(windows))]
fn test_window_id(raw: u64) -> winit::window::WindowId {
    winit::window::WindowId::from(raw)
}

fn window_with_back(id: u64, tag: &'static str, log: Rc<RefCell<Vec<&'static str>>>) -> PerWindow {
    let mut pw = PerWindow::new(Box::new(|_| {}), 100.0, 100.0, ThemeColors::default_light());
    pw.composer.set_window_id(id);
    pw.composer.set_created_id(id);
    // Register the declaration-side state too: the pump's unfocused fallback now takes its order
    // from this table, not from sorting the window map.
    crate::app::window_state::declare(
        crate::app::window_state::WindowId::new(id),
        crate::app::window_state::WindowSeed::default(),
    );
    pw.composer.compose(move |ctx| {
        crate::back::back_handler(ctx, true, move || log.borrow_mut().push(tag));
    });
    pw
}

fn state_with_two_windows(focus_first: bool) -> (AppState, Rc<RefCell<Vec<&'static str>>>) {
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut first = window_with_back(1, "one", log.clone());
    first.focused_id = focus_first.then_some(1);
    first.created_id = Some(1);
    let mut second = window_with_back(2, "two", log.clone());
    second.created_id = Some(2);

    let mut windows = HashMap::new();
    windows.insert(test_window_id(1), first);
    windows.insert(test_window_id(2), second);
    let state = AppState {
        platform: PlatformState::default(),
        windows,
        saveable: HashMap::new(),
        pending_content: Vec::new(),
        parent_window_id: None,
        init: None,
        root: None,
        was_animating: false,
    };
    (state, log)
}

#[test]
fn a_targeted_back_event_reaches_only_that_window() {
    let (mut state, log) = state_with_two_windows(true);
    let _ = inbox::drain();

    inbox::push_for(2, Signal::Completed);
    state.pump_back_events();
    assert_eq!(*log.borrow(), vec!["two"], "target wins over focus");
}

#[test]
fn a_global_back_event_goes_to_the_focused_window() {
    let (mut state, log) = state_with_two_windows(true);
    let _ = inbox::drain();

    inbox::push(Signal::Completed);
    state.pump_back_events();
    assert_eq!(*log.borrow(), vec!["one"], "global events follow focus");
}

#[test]
fn a_global_event_without_focus_goes_to_the_topmost_window() {
    let (mut state, log) = state_with_two_windows(false);
    let _ = inbox::drain();

    inbox::push(Signal::Completed);
    state.pump_back_events();
    assert_eq!(
        *log.borrow(),
        vec!["two"],
        "without focus, back belongs to the most recently created (topmost) window"
    );
}

#[test]
fn a_target_with_no_window_is_dropped() {
    let (mut state, log) = state_with_two_windows(true);
    let _ = inbox::drain();

    inbox::push_for(99, Signal::Completed);
    state.pump_back_events();
    assert!(
        log.borrow().is_empty(),
        "a stale target must not hit another window"
    );
}
