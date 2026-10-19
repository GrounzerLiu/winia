//! Cleanup registration for component authors: `on_forget` and `remember_updated_state`.

use crate::composable;
use crate::runtime::composer::ComposeCtx;
use crate::runtime::state::State;

/// Follow the latest value of `value` across recompositions.
///
/// The returned handle always holds the value from the most recent composition, which is what a
/// long-lived effect needs: a `LaunchedEffect::new(())` body would otherwise close over the value
/// it captured when it started and keep reading that forever.
///
/// ```ignore
/// let latest = remember_updated_state(ctx, on_event);
/// LaunchedEffect::new(()).build(ctx, move |_| async move {
///     loop { wait().await; latest.get()(); }
/// });
/// ```
///
/// The write is SILENT (it goes through the slot's `Backchannel` semantics): the value changed
/// because this composition ran, so scheduling another recompose for it would only buy a spurious
/// frame. A read later in the same pass already sees the new value; a reader in a later pass that
/// skipped this call site keeps the last value it was given.
#[composable]
pub fn remember_updated_state<T: Clone + 'static>(ctx: &mut ComposeCtx, value: T) -> State<T> {
    let state = ctx.remember(|| value.clone());
    state.as_backchannel().set(value);
    state
}

/// Register `cleanup` for the composition point being built here: it runs when that point is
/// removed, and never on a plain recomposition.
///
/// The `#[composable]` is load-bearing — it gives the call its own scope slot, which is what dies
/// when the branch stops composing. [`ComposeCtx::remember_on_forget`] is the same hook without
/// that per-call slot: calling the method directly attaches the callback to whatever slot is
/// current, so a component that stops using a resource while staying composed keeps the callback.
/// Prefer this function unless the callback belongs to the whole enclosing slot.
///
/// No layout node is created — see `remember_on_forget`'s doc for the bug that used to.
#[composable]
pub fn on_forget(ctx: &mut ComposeCtx, cleanup: impl FnOnce() + Send + 'static) {
    ctx.remember_on_forget(cleanup);
}
