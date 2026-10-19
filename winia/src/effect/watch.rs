//! `observe_watch`: bridge a `watch::Receiver` into a `State`.

use crate::composable;
use crate::effect::scope::remember_coroutine_scope;
use crate::runtime::composer::ComposeCtx;
use crate::runtime::state::State;
use std::sync::Arc;
use std::sync::Mutex;

/// What `observe_watch` remembers across recompositions: the receiver it is currently subscribed to
/// and the task following it.
///
/// The task is a `JoinHandle`, not an `AbortHandle`, for the same reason
/// `LaunchedEffectState::task` is one: `abort` only asks, so a replacement that started at once
/// would overlap the old task until it unwinds — and both of them write the same `State`.
struct WatchSubscription<T> {
    rx: Option<tokio::sync::watch::Receiver<T>>,
    task: Option<tokio::task::JoinHandle<()>>,
}

/// Bridge a `watch::Receiver` straight into a `State` (no `WatchStream` hop: `send` is always
/// reliable). `#[composable]`: the two remembers and the coroutine scope take a stable base from
/// the call site.
///
/// A REPLACED receiver is re-subscribed. The first version gated the spawn on a one-shot `started`
/// flag, so passing a different channel at the same call site silently kept listening to the old one
/// — the `rx` argument was honoured exactly once and then ignored for the lifetime of the slot.
/// Compose's `produceState(key = rx)` restarts for the same reason.
#[composable]
pub fn observe_watch<T: Clone + Send + Sync + PartialEq + 'static>(
    ctx: &mut ComposeCtx,
    rx: tokio::sync::watch::Receiver<T>,
    initial: T,
) -> State<T> {
    // `remember` already returns a `State<T>`, so the slot holds the value itself. The `State::new`
    // wrapper it replaced only added a layer: the outer handle was written once at init and read once
    // per frame, so it could never invalidate anything the inner one did not.
    let state: State<T> = ctx.remember(|| initial.clone());
    let slot: Arc<Mutex<WatchSubscription<T>>> =
        ctx.remember_handle(|| Arc::new(Mutex::new(WatchSubscription { rx: None, task: None })));
    let slot_arc = Arc::clone(&slot);

    // `same_channel` is the receiver's identity test: two clones of one channel answer true, two
    // channels answer false. That is the key this effect restarts on.
    let needs_subscribe = {
        let s = slot_arc.lock().unwrap();
        match &s.rx {
            Some(current) => !current.same_channel(&rx),
            None => true,
        }
    };

    if needs_subscribe {
        let scope = remember_coroutine_scope(ctx);
        // Take the old handle out of the slot OUTSIDE the lock, then abort it: the abort itself is
        // not user code, but this file keeps every task-handle call off the shared guard.
        let previous = {
            let mut s = slot_arc.lock().unwrap();
            s.task.take()
        };
        if let Some(old) = &previous {
            old.abort();
        }
        let s_state = state.clone();
        // Keep one clone for the slot's identity check; the task owns the other.
        let rx_keep = rx.clone();
        // The task owns the receiver instead of sharing it behind a lock: `changed()` borrows the
        // receiver for the length of the wait, so a guard alive across that await would make the
        // future `!Send` — and it would be held for the whole, indefinite wait besides.
        //
        // The replacement waits for the aborted task to stop before following the new channel.
        // Without the join the two overlap until the abort unwinds, and the old task can still run
        // the `set` that follows its last `changed()` — writing the PREVIOUS channel's value over
        // the new one. `LaunchedEffect` joins for exactly this reason.
        let handle = scope.rt.spawn(async move {
            if let Some(old) = previous {
                let _ = old.await;
            }
            follow_watch(rx, s_state).await;
        });
        {
            let mut s = slot_arc.lock().unwrap();
            s.rx = Some(rx_keep);
            s.task = Some(handle);
        }

        // Leaving the composition stops the subscription too. The task is spawned on the runtime
        // directly (`rt.spawn`, not `CoroutineScope::spawn`), so `ScopeState::drop` does not know
        // about it, and dropping an `AbortHandle` cancels nothing: without this the task outlived
        // the slot and kept following a channel whose `State` had no readers left. Registered once
        // per subscription — the callback lives in this slot's map and only reads the stable `Arc`.
        let slot_for_remove = Arc::clone(&slot_arc);
        ctx.remember_on_forget(move || {
            let taken = {
                let mut s = slot_for_remove.lock().unwrap();
                s.task.take()
            };
            if let Some(task) = taken {
                task.abort();
            }
        });
    }

    state
}

/// Follow `rx` into `state` until the channel closes or the task is aborted.
async fn follow_watch<T>(mut rx: tokio::sync::watch::Receiver<T>, state: State<T>)
where
    T: Clone + Send + Sync + PartialEq + 'static,
{
    // Seed from the channel's CURRENT value, so a re-subscription lands on the new source instead of
    // keeping whatever the previous receiver had left in the State.
    state.set(rx.borrow_and_update().clone());
    loop {
        if rx.changed().await.is_err() {
            break;
        }
        state.set(rx.borrow_and_update().clone());
    }
}
