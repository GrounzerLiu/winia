//! Dispatch semantics pinned against Compose's `OnBackPressedDispatcher` behaviour.

use super::*;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Default)]
struct Log(Rc<RefCell<Vec<String>>>);

impl Log {
    fn new() -> Self {
        Self::default()
    }

    fn push(&self, line: impl Into<String>) {
        self.0.borrow_mut().push(line.into());
    }

    fn take(&self) -> Vec<String> {
        self.0.borrow_mut().drain(..).collect()
    }
}

fn event(progress: f32) -> BackEvent {
    BackEvent {
        touch: (12.0, 24.0),
        progress,
        edge: BackSwipeEdge::Left,
        frame_time: std::time::Duration::ZERO,
    }
}

fn predictive(log: Log, tag: &'static str) -> BackHandlerSpec {
    BackHandlerSpec::predictive(move |update| match update {
        BackUpdate::Started(event) => {
            log.push(format!("{tag}:started:{}:{:?}", event.progress, event.edge))
        }
        BackUpdate::Progressed(event) => log.push(format!("{tag}:progressed:{}", event.progress)),
        BackUpdate::Completed => log.push(format!("{tag}:completed")),
        BackUpdate::Cancelled => log.push(format!("{tag}:cancelled")),
    })
}

fn discrete(log: Log, tag: &'static str) -> BackHandlerSpec {
    BackHandlerSpec::discrete(move || log.push(format!("{tag}:back")))
}

#[test]
fn composed_handlers_follow_composition_order() {
    use crate::runtime::composer::Composer;

    let log: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
    let mut composer = Composer::new();
    let first = Rc::clone(&log);
    let second = Rc::clone(&log);
    composer.compose(move |ctx| {
        let first = Rc::clone(&first);
        back_handler(ctx, true, move || first.borrow_mut().push("first"));
        let second = Rc::clone(&second);
        back_handler(ctx, true, move || second.borrow_mut().push("second"));
    });

    assert!(composer.back_dispatcher().dispatch(BackUpdate::Completed));
    assert_eq!(&*log.borrow(), &["second"]);
}

#[test]
fn a_handler_leaves_the_table_when_its_slot_is_dropped() {
    use crate::runtime::composer::Composer;

    let mut composer = Composer::new();
    composer.compose(|ctx| {
        back_handler(ctx, true, || {});
    });
    assert!(composer.back_dispatcher().has_enabled());

    composer.compose(|_ctx| {});
    assert!(!composer.back_dispatcher().has_enabled());
}

#[test]
fn predictive_handlers_receive_the_gesture_in_composition() {
    use crate::runtime::composer::Composer;

    let log: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
    let mut composer = Composer::new();
    let sink = Rc::clone(&log);
    composer.compose(move |ctx| {
        let sink = Rc::clone(&sink);
        predictive_back_handler(ctx, true, move |update| match update {
            BackUpdate::Started(_) => sink.borrow_mut().push("started"),
            BackUpdate::Progressed(event) if event.progress > 0.5 => {
                sink.borrow_mut().push("progressed-high")
            }
            BackUpdate::Progressed(_) => sink.borrow_mut().push("progressed-low"),
            BackUpdate::Completed => sink.borrow_mut().push("completed"),
            BackUpdate::Cancelled => sink.borrow_mut().push("cancelled"),
        });
    });

    let dispatcher = composer.back_dispatcher();
    assert!(dispatcher.dispatch(BackUpdate::Started(event(0.0))));
    assert!(dispatcher.dispatch(BackUpdate::Progressed(event(0.7))));
    assert!(dispatcher.dispatch(BackUpdate::Completed));
    assert_eq!(&*log.borrow(), &["started", "progressed-high", "completed"]);
}

#[test]
fn a_cloned_dispatcher_shares_the_handler_table() {
    let dispatcher = BackDispatcher::new();
    let shared = dispatcher.clone();
    let log = Log::new();
    shared.register(BackPriority::Default, true, discrete(log.clone(), "page"));

    assert!(dispatcher.has_enabled());
    assert!(dispatcher.dispatch(BackUpdate::Completed));
    assert_eq!(log.take(), ["page:back"]);
}

#[test]
fn the_newest_enabled_default_handler_wins() {
    let dispatcher = BackDispatcher::new();
    let log = Log::new();
    dispatcher.register(BackPriority::Default, true, discrete(log.clone(), "older"));
    dispatcher.register(BackPriority::Default, true, discrete(log.clone(), "newer"));

    assert!(dispatcher.dispatch(BackUpdate::Completed));
    assert_eq!(log.take(), ["newer:back"]);
}

#[test]
fn an_overlay_handler_runs_before_a_newer_default_handler() {
    let dispatcher = BackDispatcher::new();
    let log = Log::new();
    dispatcher.register(BackPriority::Overlay, true, discrete(log.clone(), "dialog"));
    dispatcher.register(BackPriority::Default, true, discrete(log.clone(), "page"));

    assert!(dispatcher.dispatch(BackUpdate::Completed));
    assert_eq!(log.take(), ["dialog:back"]);
}

#[test]
fn a_disabled_handler_is_skipped_until_it_is_enabled() {
    let dispatcher = BackDispatcher::new();
    let log = Log::new();
    dispatcher.register(BackPriority::Default, true, discrete(log.clone(), "older"));
    let newer = dispatcher.register(BackPriority::Default, false, discrete(log.clone(), "newer"));

    assert!(dispatcher.dispatch(BackUpdate::Completed));
    assert_eq!(log.take(), ["older:back"]);

    dispatcher.set_enabled(newer, true);
    assert!(dispatcher.dispatch(BackUpdate::Completed));
    assert_eq!(log.take(), ["newer:back"]);
}

#[test]
fn an_empty_table_lets_the_platform_handle_back() {
    let dispatcher = BackDispatcher::new();
    assert!(!dispatcher.dispatch(BackUpdate::Completed));
    assert!(!dispatcher.dispatch(BackUpdate::Started(event(0.0))));
    assert!(!dispatcher.dispatch(BackUpdate::Cancelled));
    assert!(!dispatcher.has_enabled());
}

#[test]
fn a_gesture_stays_with_the_handler_that_started_it() {
    let dispatcher = BackDispatcher::new();
    let log = Log::new();
    dispatcher.register(
        BackPriority::Default,
        true,
        predictive(log.clone(), "first"),
    );
    assert!(dispatcher.dispatch(BackUpdate::Started(event(0.1))));

    // A handler composed after the gesture began must not steal it.
    dispatcher.register(
        BackPriority::Default,
        true,
        predictive(log.clone(), "second"),
    );
    assert!(dispatcher.dispatch(BackUpdate::Progressed(event(0.6))));
    assert!(dispatcher.dispatch(BackUpdate::Completed));

    assert_eq!(
        log.take(),
        [
            "first:started:0.1:Left",
            "first:progressed:0.6",
            "first:completed"
        ]
    );
}

#[test]
fn a_discrete_owner_only_sees_the_completion() {
    let dispatcher = BackDispatcher::new();
    let log = Log::new();
    dispatcher.register(BackPriority::Default, true, discrete(log.clone(), "page"));

    assert!(dispatcher.dispatch(BackUpdate::Started(event(0.0))));
    assert!(dispatcher.dispatch(BackUpdate::Progressed(event(0.4))));
    assert_eq!(log.take(), Vec::<String>::new());

    assert!(dispatcher.dispatch(BackUpdate::Completed));
    assert_eq!(log.take(), ["page:back"]);
}

#[test]
fn disabling_the_owner_cancels_its_gesture() {
    let dispatcher = BackDispatcher::new();
    let log = Log::new();
    let registration = dispatcher.register(
        BackPriority::Default,
        true,
        predictive(log.clone(), "sheet"),
    );
    assert!(dispatcher.dispatch(BackUpdate::Started(event(0.0))));

    dispatcher.set_enabled(registration, false);
    assert_eq!(log.take(), ["sheet:started:0:Left", "sheet:cancelled"]);
    assert!(!dispatcher.dispatch(BackUpdate::Progressed(event(0.5))));
}

#[test]
fn disabling_a_handler_that_is_not_the_owner_keeps_the_gesture() {
    let dispatcher = BackDispatcher::new();
    let log = Log::new();
    let other = dispatcher.register(
        BackPriority::Default,
        true,
        predictive(log.clone(), "other"),
    );
    dispatcher.register(
        BackPriority::Default,
        true,
        predictive(log.clone(), "owner"),
    );
    assert!(dispatcher.dispatch(BackUpdate::Started(event(0.0))));

    dispatcher.set_enabled(other, false);
    assert!(dispatcher.dispatch(BackUpdate::Progressed(event(0.5))));
    assert_eq!(log.take(), ["owner:started:0:Left", "owner:progressed:0.5"]);
}

#[test]
fn removing_the_owner_drops_the_gesture_without_calling_it() {
    let dispatcher = BackDispatcher::new();
    let log = Log::new();
    let registration =
        dispatcher.register(BackPriority::Default, true, predictive(log.clone(), "gone"));
    assert!(dispatcher.dispatch(BackUpdate::Started(event(0.0))));

    assert!(dispatcher.remove(registration));
    assert!(!dispatcher.remove(registration));
    assert!(!dispatcher.dispatch(BackUpdate::Progressed(event(0.5))));
    assert_eq!(log.take(), ["gone:started:0:Left"]);
}

#[test]
fn a_predictive_handler_receives_a_synthetic_start_for_a_discrete_back() {
    let dispatcher = BackDispatcher::new();
    let log = Log::new();
    dispatcher.register(
        BackPriority::Default,
        true,
        predictive(log.clone(), "predictive"),
    );

    assert!(dispatcher.dispatch(BackUpdate::Completed));
    assert_eq!(
        log.take(),
        ["predictive:started:0:None", "predictive:completed"]
    );
}

#[test]
fn a_handler_may_remove_itself_from_its_own_callback() {
    let dispatcher = BackDispatcher::new();
    let registration = Rc::new(RefCell::new(None));
    let slot = Rc::clone(&registration);
    let dispatcher_inside = dispatcher.clone();
    let own = dispatcher.register(
        BackPriority::Default,
        true,
        BackHandlerSpec::discrete(move || {
            if let Some(registration) = *slot.borrow() {
                dispatcher_inside.remove(registration);
            }
        }),
    );
    *registration.borrow_mut() = Some(own);

    assert!(dispatcher.dispatch(BackUpdate::Completed));
    assert!(!dispatcher.has_enabled());
    assert!(!dispatcher.dispatch(BackUpdate::Completed));
}

#[test]
fn has_enabled_follows_registration_state() {
    let dispatcher = BackDispatcher::new();
    assert!(!dispatcher.has_enabled());

    let registration =
        dispatcher.register(BackPriority::Default, false, discrete(Log::new(), "page"));
    assert!(!dispatcher.has_enabled());

    dispatcher.set_enabled(registration, true);
    assert!(dispatcher.has_enabled());

    dispatcher.remove(registration);
    assert!(!dispatcher.has_enabled());
}

#[test]
fn cancelling_a_gesture_notifies_the_predictive_owner_once() {
    let dispatcher = BackDispatcher::new();
    let log = Log::new();
    dispatcher.register(BackPriority::Default, true, predictive(log.clone(), "page"));
    assert!(dispatcher.dispatch(BackUpdate::Started(event(0.0))));

    assert!(dispatcher.cancel_gesture());
    assert!(!dispatcher.cancel_gesture());
    assert_eq!(log.take(), ["page:started:0:Left", "page:cancelled"]);
}
