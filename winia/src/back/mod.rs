//! Back navigation dispatch — winia's analogue of `OnBackPressedDispatcher`.
//!
//! The platform (Android today) turns a system back gesture into a small event stream: a start,
//! zero or more progress updates, then exactly one terminal event (completed or cancelled). This
//! module owns the handler table for one window and turns that stream into calls on the most
//! recently registered enabled handler.
//!
//! The behaviour follows Jetpack Compose's `BackHandler` / `PredictiveBackHandler`:
//!
//! - the newest enabled handler wins; `BackPriority::Overlay` handlers are considered before
//!   default ones, so a dialog or sheet can consume back before the page sees it;
//! - a gesture belongs to the handler that received its start; later registrations cannot steal it
//!   mid-gesture;
//! - disabling a handler removes it from selection immediately, and disabling the owner cancels
//!   the gesture with a `Cancelled` update;
//! - with no enabled handler, dispatch returns `false` so the platform can perform its own back
//!   action (finish the activity).
//!
//! # Threading
//!
//! A dispatcher is owned by one window and lives on the UI thread (`Rc<RefCell<..>>`). It is
//! deliberately `!Send`: platform callbacks arrive on a foreign thread and must be queued by the
//! bridge (see `docs/back-navigation.md`). Handlers are plain `Fn`/`FnMut` closures and may read
//! or write `State`; they run synchronously inside `dispatch`.
//!
//! # Deviations from Compose
//!
//! - Removing the active owner during its own callback clears the gesture without delivering
//!   `Cancelled`: the handler is already gone, and calling it again would re-enter a closure that
//!   is still on the stack. Disabling it (rather than removing it) does deliver `Cancelled`.
//! - After an owner disappears mid-gesture, the terminal event is dispatched against the current
//!   handler table. Compose cancels the gesture flow and has no route for the remaining events;
//!   the platform gesture still has to end somewhere, so winia lets the next enabled handler
//!   finish it.

mod dispatcher;

#[cfg(test)]
mod tests;

pub use dispatcher::{BackDispatcher, BackHandlerSpec, BackRegistration};

use crate::composable;
use crate::effect::{on_forget, remember_updated_state, side_effect};
use crate::runtime::composer::ComposeCtx;
use crate::runtime::state::State;
use dispatcher::{remove_for, set_enabled_for};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

/// The screen edge a predictive back gesture started from.
///
/// Mirrors Android's `BackEvent`: `EDGE_LEFT` and `EDGE_RIGHT` come from an edge swipe, while
/// `None` marks a discrete event (three-button navigation, a hardware key, or a synthetic event
/// such as Escape).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackSwipeEdge {
    Left,
    Right,
    None,
}

impl BackSwipeEdge {
    /// Map Android's `BackEvent.getSwipeEdge()` value onto the winia vocabulary.
    pub const fn from_android(raw: i32) -> Self {
        match raw {
            0 => Self::Left,
            1 => Self::Right,
            _ => Self::None,
        }
    }
}

/// One frame of a predictive back gesture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BackEvent {
    /// Touch point in logical pixels, in the window's coordinate space.
    pub touch: (f32, f32),
    /// Gesture progress from `0.0` (start) to `1.0` (fully committed).
    pub progress: f32,
    /// Edge the gesture started from; `None` for discrete events.
    pub edge: BackSwipeEdge,
    /// Frame timestamp when the platform provided one, otherwise zero.
    pub frame_time: Duration,
}

impl BackEvent {
    /// The neutral event used when a discrete back arrives without a gesture.
    pub const NONE: Self = Self {
        touch: (0.0, 0.0),
        progress: 0.0,
        edge: BackSwipeEdge::None,
        frame_time: Duration::ZERO,
    };
}

/// One step of a back interaction delivered to a predictive handler.
///
/// `Started` and `Progressed` describe the gesture; `Completed` and `Cancelled` are terminal and
/// close it. A discrete `BackHandler` only receives `Completed`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BackUpdate {
    /// A predictive gesture began, or a non-predictive event is being synthesized.
    Started(BackEvent),
    /// The gesture moved; `event.progress` is the authoritative value.
    Progressed(BackEvent),
    /// The gesture finished and the back action should happen.
    Completed,
    /// The gesture was abandoned (the user pulled back to the edge, or the handler went away).
    Cancelled,
}

impl BackUpdate {
    /// The gesture payload attached to `Started` / `Progressed`, if any.
    pub fn event(self) -> Option<BackEvent> {
        match self {
            Self::Started(event) | Self::Progressed(event) => Some(event),
            Self::Completed | Self::Cancelled => None,
        }
    }

    /// Gesture progress from the attached event; `1.0` for a completed discrete event.
    pub fn progress(self) -> f32 {
        match self {
            Self::Started(event) | Self::Progressed(event) => event.progress,
            Self::Completed => 1.0,
            Self::Cancelled => 0.0,
        }
    }

    /// Whether this update closes the gesture.
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled)
    }
}

/// Scan order for the handler table.
///
/// Overlay handlers are considered before default ones regardless of registration time, which is
/// what lets a dialog or a sheet take back before the page underneath. Within one priority the
/// most recently registered enabled handler wins, matching Compose's composition-order rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackPriority {
    Default,
    Overlay,
}

/// Register a discrete back handler for the rest of this composition point's lifetime.
///
/// The handler is registered once and removed when the call site leaves the composition;
/// `enabled` is applied on every pass. Composition order is dispatch order: the most recently
/// composed enabled handler wins.
///
/// Call it unconditionally and toggle `enabled` instead of putting it inside an `if`: a conditional
/// call changes the order of the handlers composed below it.
#[composable]
pub fn back_handler(ctx: &mut ComposeCtx, enabled: bool, on_back: impl Fn() + 'static) {
    register_discrete(ctx, BackPriority::Default, enabled, on_back);
}

/// Like [`back_handler`], but ordered above every default handler.
///
/// Dialogs, sheets and menus register here so their dismissal runs before the page's handler.
#[composable]
pub fn overlay_back_handler(ctx: &mut ComposeCtx, enabled: bool, on_back: impl Fn() + 'static) {
    register_discrete(ctx, BackPriority::Overlay, enabled, on_back);
}

#[composable]
fn register_discrete(
    ctx: &mut ComposeCtx,
    priority: BackPriority,
    enabled: bool,
    on_back: impl Fn() + 'static,
) {
    let dispatcher = ctx.back();
    let dispatcher_id = dispatcher.id();
    let callback: Rc<dyn Fn()> = Rc::new(on_back);
    let latest: State<Rc<dyn Fn()>> = remember_updated_state(ctx, callback);
    let registration = ctx.remember(|| {
        let latest = latest.clone();
        dispatcher.register(
            priority,
            enabled,
            BackHandlerSpec::discrete(move || {
                let callback = latest.peek();
                callback();
            }),
        )
    });
    let registration = registration.peek();
    side_effect(ctx, move || {
        set_enabled_for(dispatcher_id, registration, enabled)
    });
    on_forget(ctx, move || remove_for(dispatcher_id, registration));
}

/// Register a predictive back handler.
///
/// It receives `Started` and every `Progressed` frame of the gesture it owns, then exactly one of
/// `Completed` / `Cancelled`. A discrete back (three-button navigation, a hardware key, Escape)
/// arrives as `Started(BackEvent::NONE)` immediately followed by `Completed`, which is how Compose
/// treats a predictive handler that has no gesture to follow.
#[composable]
pub fn predictive_back_handler(
    ctx: &mut ComposeCtx,
    enabled: bool,
    on_back: impl FnMut(BackUpdate) + 'static,
) {
    let dispatcher = ctx.back();
    let dispatcher_id = dispatcher.id();
    let callback: Rc<RefCell<dyn FnMut(BackUpdate)>> = Rc::new(RefCell::new(on_back));
    let latest: State<Rc<RefCell<dyn FnMut(BackUpdate)>>> = remember_updated_state(ctx, callback);
    let registration = ctx.remember(|| {
        let latest = latest.clone();
        dispatcher.register(
            BackPriority::Default,
            enabled,
            BackHandlerSpec::predictive(move |update| {
                let callback = latest.peek();
                (*callback.borrow_mut())(update);
            }),
        )
    });
    let registration = registration.peek();
    side_effect(ctx, move || {
        set_enabled_for(dispatcher_id, registration, enabled)
    });
    on_forget(ctx, move || remove_for(dispatcher_id, registration));
}
