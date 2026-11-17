//! The handler table and gesture state machine behind `BackDispatcher`.

use std::cell::RefCell;
use std::rc::Rc;
use std::rc::Weak;
use std::sync::atomic::{AtomicU64, Ordering};

use super::{BackEvent, BackPriority, BackUpdate};

/// Identity for the UI-thread registry that effect cleanups use.
///
/// Effect callbacks must be `Send`, while a dispatcher is deliberately `!Send`: the cleanup
/// therefore captures the numeric id and looks the table up on the thread that runs it. Only the
/// UI thread can have a live entry, and a dead window is pruned on the next lookup.
static NEXT_DISPATCHER_ID: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static LIVE_DISPATCHERS: RefCell<Vec<(u64, Weak<RefCell<State>>)>> = const { RefCell::new(Vec::new()) };
}

/// What a registration does when it is invoked.
enum Handler {
    /// A plain back action: called once on `Completed`.
    Discrete(Box<dyn Fn()>),
    /// A predictive handler: called for every update of the gesture it owns.
    Predictive(Box<dyn FnMut(BackUpdate)>),
}

/// A handler passed to `BackDispatcher::register`.
pub enum BackHandlerSpec {
    /// Run `on_back` when a discrete back completes.
    Discrete(Box<dyn Fn()>),
    /// Follow a predictive gesture; receives start, progress updates and one terminal update.
    Predictive(Box<dyn FnMut(BackUpdate)>),
}

impl BackHandlerSpec {
    /// Build a discrete handler.
    pub fn discrete(on_back: impl Fn() + 'static) -> Self {
        Self::Discrete(Box::new(on_back))
    }

    /// Build a predictive handler.
    pub fn predictive(on_back: impl FnMut(BackUpdate) + 'static) -> Self {
        Self::Predictive(Box::new(on_back))
    }
}

/// A handler's slot in a dispatcher.
///
/// Copy so a composition can keep it in a `State` and hand it back from a cleanup closure without
/// borrowing the dispatcher. It is only meaningful for the dispatcher that created it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BackRegistration {
    pub(crate) id: u64,
}

struct Entry {
    id: u64,
    priority: BackPriority,
    enabled: bool,
    /// `None` while the handler is being invoked: taking it out keeps the dispatcher borrow free
    /// during the callback, so a handler may remove or disable itself without a `RefCell` panic.
    handler: Option<Handler>,
}

struct Gesture {
    owner: u64,
}

struct State {
    entries: Vec<Entry>,
    next_id: u64,
    gesture: Option<Gesture>,
    /// True while a handler is running. Nested dispatches and cancellations are suppressed; the
    /// gesture can still be cleared, it just has nobody left to notify.
    dispatching: bool,
}

impl State {
    fn select_owner(&self) -> Option<u64> {
        self.entries
            .iter()
            .rev()
            .find(|entry| {
                entry.enabled && entry.handler.is_some() && entry.priority == BackPriority::Overlay
            })
            .or_else(|| {
                self.entries
                    .iter()
                    .rev()
                    .find(|entry| entry.enabled && entry.handler.is_some())
            })
            .map(|entry| entry.id)
    }

    fn is_predictive(&self, id: u64) -> bool {
        self.entries
            .iter()
            .find(|entry| entry.id == id)
            .is_some_and(|entry| matches!(entry.handler, Some(Handler::Predictive(_))))
    }

    /// Decide what one dispatch should do. Deliveries are performed after this returns, so no
    /// borrow is held while user code runs.
    fn plan(&mut self, update: BackUpdate) -> Plan {
        let mut plan = Plan::default();
        match update {
            BackUpdate::Started(event) => {
                if self.gesture.is_some() {
                    // The platform does not overlap gestures; ignore a duplicate start rather
                    // than re-seating a handler the user is already dragging.
                    return plan;
                }
                let Some(owner) = self.select_owner() else {
                    return plan;
                };
                self.gesture = Some(Gesture { owner });
                plan.handled = true;
                if self.is_predictive(owner) {
                    plan.deliver(owner, BackUpdate::Started(event));
                }
            }
            BackUpdate::Progressed(event) => {
                let Some(gesture) = self.gesture.as_ref() else {
                    return plan;
                };
                let owner = gesture.owner;
                if !self
                    .entries
                    .iter()
                    .any(|entry| entry.id == owner && entry.enabled && entry.handler.is_some())
                {
                    self.gesture = None;
                    return plan;
                }
                plan.handled = true;
                if self.is_predictive(owner) {
                    plan.deliver(owner, BackUpdate::Progressed(event));
                }
            }
            BackUpdate::Completed => {
                if let Some(gesture) = self.gesture.take() {
                    let owner = gesture.owner;
                    if self
                        .entries
                        .iter()
                        .any(|entry| entry.id == owner && entry.enabled && entry.handler.is_some())
                    {
                        plan.handled = true;
                        plan.deliver(owner, BackUpdate::Completed);
                        return plan;
                    }
                    // The owner vanished mid-gesture; finish against the current table below.
                }
                let Some(owner) = self.select_owner() else {
                    return plan;
                };
                plan.handled = true;
                if self.is_predictive(owner) {
                    // A discrete back with no gesture: give the predictive handler the same
                    // start-then-complete shape Compose uses when a flow is created for it.
                    plan.deliver(owner, BackUpdate::Started(BackEvent::NONE));
                    plan.deliver(owner, BackUpdate::Completed);
                } else {
                    plan.deliver(owner, BackUpdate::Completed);
                }
            }
            BackUpdate::Cancelled => {
                let Some(gesture) = self.gesture.take() else {
                    return plan;
                };
                plan.handled = true;
                if self.is_predictive(gesture.owner) {
                    plan.deliver(gesture.owner, BackUpdate::Cancelled);
                }
            }
        }
        plan
    }
}

#[derive(Default)]
struct Plan {
    handled: bool,
    first: Option<Delivery>,
    second: Option<Delivery>,
}

struct Delivery {
    id: u64,
    update: BackUpdate,
}

impl Plan {
    fn deliver(&mut self, id: u64, update: BackUpdate) {
        let delivery = Delivery { id, update };
        if self.first.is_none() {
            self.first = Some(delivery);
        } else {
            self.second = Some(delivery);
        }
    }
}

/// The per-window back handler table.
///
/// Clone the handle to share it; every clone talks to the same table. Not `Send`: see the module
/// documentation for the threading rule.
#[derive(Clone)]
pub struct BackDispatcher {
    id: u64,
    state: Rc<RefCell<State>>,
}

impl Default for BackDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl BackDispatcher {
    /// An empty dispatcher.
    pub fn new() -> Self {
        let id = NEXT_DISPATCHER_ID.fetch_add(1, Ordering::Relaxed);
        let state = Rc::new(RefCell::new(State {
            entries: Vec::new(),
            next_id: 1,
            gesture: None,
            dispatching: false,
        }));
        LIVE_DISPATCHERS.with(|live| {
            let mut live = live.borrow_mut();
            if live.len() > 32 {
                live.retain(|(_, weak)| weak.strong_count() > 0);
            }
            live.push((id, Rc::downgrade(&state)));
        });
        Self { id, state }
    }

    /// Stable identity used by effect cleanup closures, which must be `Send`.
    pub(crate) fn id(&self) -> u64 {
        self.id
    }

    /// Add a handler and return its registration.
    ///
    /// Registration order is dispatch order (newest wins), so callers register in composition
    /// order. `enabled` starts the handler in the requested state; toggling it later uses
    /// `set_enabled`.
    pub fn register(
        &self,
        priority: BackPriority,
        enabled: bool,
        handler: BackHandlerSpec,
    ) -> BackRegistration {
        let mut state = self.state.borrow_mut();
        let id = state.next_id;
        state.next_id += 1;
        let handler = match handler {
            BackHandlerSpec::Discrete(callback) => Handler::Discrete(callback),
            BackHandlerSpec::Predictive(callback) => Handler::Predictive(callback),
        };
        state.entries.push(Entry {
            id,
            priority,
            enabled,
            handler: Some(handler),
        });
        BackRegistration { id }
    }

    /// Enable or disable a registered handler.
    ///
    /// Disabling the handler that owns the current gesture cancels the gesture: a predictive owner
    /// receives `Cancelled`, a discrete one is simply dropped. Disabling a handler that is not the
    /// owner only affects future selection.
    pub fn set_enabled(&self, registration: BackRegistration, enabled: bool) {
        let changed = {
            let mut state = self.state.borrow_mut();
            match state
                .entries
                .iter_mut()
                .find(|entry| entry.id == registration.id)
            {
                Some(entry) if entry.enabled != enabled => {
                    entry.enabled = enabled;
                    true
                }
                _ => false,
            }
        };
        if changed && !enabled {
            self.cancel_owner(registration.id);
        }
    }

    /// Remove a handler from the table.
    ///
    /// Removing the active owner clears the gesture without delivering `Cancelled`: the callback
    /// is gone, so there is nothing left to notify. Returns whether the registration existed.
    pub fn remove(&self, registration: BackRegistration) -> bool {
        let mut state = self.state.borrow_mut();
        let Some(position) = state
            .entries
            .iter()
            .position(|entry| entry.id == registration.id)
        else {
            return false;
        };
        state.entries.remove(position);
        if state
            .gesture
            .as_ref()
            .is_some_and(|gesture| gesture.owner == registration.id)
        {
            state.gesture = None;
        }
        true
    }

    /// Whether any handler would be selected right now.
    pub fn has_enabled(&self) -> bool {
        self.state
            .borrow()
            .entries
            .iter()
            .any(|entry| entry.enabled && entry.handler.is_some())
    }

    /// Dispatch one update. Returns whether a handler consumed it.
    ///
    /// A nested call from inside a handler returns `false`; the table is already busy.
    pub fn dispatch(&self, update: BackUpdate) -> bool {
        let plan = {
            let mut state = self.state.borrow_mut();
            if state.dispatching {
                return false;
            }
            state.dispatching = true;
            state.plan(update)
        };
        let _guard = ClearDispatching(&self.state);
        if let Some(delivery) = plan.first {
            self.invoke(delivery.id, delivery.update);
        }
        if let Some(delivery) = plan.second {
            self.invoke(delivery.id, delivery.update);
        }
        plan.handled
    }

    /// Cancel the gesture in flight, if any. Used when the window is suspended or the activity is
    /// destroyed. Returns whether a gesture was active.
    pub fn cancel_gesture(&self) -> bool {
        let (owner, dispatching) = {
            let mut state = self.state.borrow_mut();
            let owner = state.gesture.take().map(|gesture| gesture.owner);
            (owner, state.dispatching)
        };
        match owner {
            Some(id) => {
                if !dispatching {
                    self.invoke(id, BackUpdate::Cancelled);
                }
                true
            }
            None => false,
        }
    }

    /// Take the handler out of the table and run it, then put it back if it is still registered.
    ///
    /// Running user code while no borrow is held is what lets a handler remove or disable itself
    /// from inside its own callback without a `RefCell` panic.
    fn invoke(&self, id: u64, update: BackUpdate) {
        let Some(mut handler) = self.take_handler(id) else {
            return;
        };
        match &mut handler {
            Handler::Discrete(callback) => {
                if matches!(update, BackUpdate::Completed) {
                    callback();
                }
            }
            Handler::Predictive(callback) => callback(update),
        }
        self.restore_handler(id, handler);
    }

    fn take_handler(&self, id: u64) -> Option<Handler> {
        let mut state = self.state.borrow_mut();
        state
            .entries
            .iter_mut()
            .find(|entry| entry.id == id)
            .and_then(|entry| entry.handler.take())
    }

    fn restore_handler(&self, id: u64, handler: Handler) {
        let mut state = self.state.borrow_mut();
        if let Some(entry) = state.entries.iter_mut().find(|entry| entry.id == id)
            && entry.handler.is_none()
        {
            entry.handler = Some(handler);
        }
    }

    /// Cancel the gesture when its owner is disabled. Suppressed while a callback is running; the
    /// gesture is still cleared, because the handler that would receive `Cancelled` is the one
    /// currently executing.
    fn cancel_owner(&self, id: u64) {
        let (owned, dispatching) = {
            let mut state = self.state.borrow_mut();
            let owned = state
                .gesture
                .as_ref()
                .is_some_and(|gesture| gesture.owner == id);
            if owned {
                state.gesture = None;
            }
            (owned, state.dispatching)
        };
        if owned && !dispatching {
            self.invoke(id, BackUpdate::Cancelled);
        }
    }
}

/// Run `f` against the dispatcher registered under `id` on this thread, if it is still alive.
///
/// Effect cleanups capture only the id, so this is the bridge between a `Send` callback and the
/// `!Send` table it belongs to. A lookup on another thread finds nothing and does nothing.
pub(crate) fn with_dispatcher(id: u64, f: impl FnOnce(&BackDispatcher)) {
    let state = LIVE_DISPATCHERS.with(|live| {
        let mut live = live.borrow_mut();
        let mut found = None;
        live.retain(|(entry_id, weak)| {
            if *entry_id == id {
                found = weak.upgrade();
                found.is_some()
            } else {
                weak.strong_count() > 0
            }
        });
        found
    });
    if let Some(state) = state {
        f(&BackDispatcher { id, state });
    }
}

/// Effect-side enable toggle: capture `id`, never the table.
pub(crate) fn set_enabled_for(id: u64, registration: BackRegistration, enabled: bool) {
    with_dispatcher(id, |dispatcher| {
        dispatcher.set_enabled(registration, enabled)
    });
}

/// Effect-side removal: capture `id`, never the table.
pub(crate) fn remove_for(id: u64, registration: BackRegistration) {
    with_dispatcher(id, |dispatcher| {
        dispatcher.remove(registration);
    });
}

/// Clears the `dispatching` flag even if a handler panics, so the dispatcher is not left wedged.
struct ClearDispatching<'a>(&'a RefCell<State>);

impl Drop for ClearDispatching<'_> {
    fn drop(&mut self) {
        self.0.borrow_mut().dispatching = false;
    }
}
