//! produce_state: an async producer that feeds a State.

use crate::composable;
use crate::effect::launched::LaunchedEffect;
use crate::runtime::composer::ComposeCtx;
use crate::runtime::state::State;

/// Run an async producer that writes into a State (Compose's produceState).
///
/// The state starts at initial and is returned to the caller to read in composition. The producer
/// receives a clone of the same handle, so it publishes with state.set(..); a new key cancels the
/// running producer and starts a new one, and leaving the composition cancels it for good.
///
/// This is the bridge for a source that is not a watch channel: an API with a callback, a socket,
/// a query with intermediate results. For a watch::Receiver use observe_watch, which also
/// re-subscribes when the receiver is replaced; for a value that only has to follow a computation
/// use snapshot_flow.
///
/// Timing is LaunchedEffect's: the producer starts during composition, not after the frame
/// commits. Use side_effect for work that must see the finished tree.
#[composable]
pub fn produce_state<T, K, F>(
    ctx: &mut ComposeCtx,
    initial: T,
    key: K,
    producer: impl FnOnce(State<T>) -> F + Send + 'static,
) -> State<T>
where
    T: Clone + PartialEq + Send + Sync + 'static,
    K: PartialEq + Clone + Send + 'static,
    F: std::future::Future<Output = ()> + Send + 'static,
{
    let state = ctx.remember(|| initial);
    let produced = state.clone();
    // LaunchedEffect owns the key/restart/teardown contract. Handing it a closure that captures the
    // State keeps the two halves one unit: the producer's lifetime is this call site's, not the
    // handle's, so a caller that drops the returned State still owns a cancelled producer.
    LaunchedEffect::new(key).build(ctx, move |_| async move {
        producer(produced).await;
    });
    state
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn drive(rt: &tokio::runtime::Runtime) {
        rt.block_on(async {
            for _ in 0..60 {
                tokio::task::yield_now().await;
            }
        });
    }

    /// The producer publishes into the returned state, and a new key restarts it.
    #[test]
    fn the_producer_feeds_the_state_and_a_key_change_restarts_it() {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let _entered = rt.enter();
        let starts: Arc<Mutex<Vec<u32>>> = Arc::new(Mutex::new(Vec::new()));
        let kept: Arc<Mutex<Option<State<u32>>>> = Arc::new(Mutex::new(None));
        let mut composer = crate::runtime::composer::Composer::new();

        let mut frame = |composer: &mut crate::runtime::composer::Composer, key: u32| {
            let starts = starts.clone();
            let kept = kept.clone();
            composer.compose(move |ctx: &mut ComposeCtx| {
                let state = produce_state(ctx, 0u32, key, move |state| async move {
                    starts.lock().unwrap().push(key);
                    state.set(key);
                    // Parked: only a restart or a teardown ends this producer.
                    std::future::pending::<()>().await;
                });
                *kept.lock().unwrap() = Some(state);
            });
            composer.layout(crate::layout::Constraints::new(0.0, 100.0, 0.0, 100.0));
        };

        frame(&mut composer, 1);
        drive(&rt);
        assert_eq!(*starts.lock().unwrap(), vec![1], "the producer ran once");
        assert_eq!(kept.lock().unwrap().as_ref().unwrap().peek(), 1);

        frame(&mut composer, 2);
        drive(&rt);
        assert_eq!(
            *starts.lock().unwrap(),
            vec![1, 2],
            "a key change restarts the producer"
        );
        assert_eq!(kept.lock().unwrap().as_ref().unwrap().peek(), 2);
    }

    /// Leaving the composition drops the producer's future instead of leaving it parked.
    #[test]
    fn leaving_the_composition_cancels_the_producer() {
        struct Marker(Arc<Mutex<Vec<&'static str>>>, &'static str);
        impl Drop for Marker {
            fn drop(&mut self) {
                self.0.lock().unwrap().push(self.1);
            }
        }

        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let _entered = rt.enter();
        let log: Arc<Mutex<Vec<&'static str>>> = Arc::new(Mutex::new(Vec::new()));
        let mut composer = crate::runtime::composer::Composer::new();

        let l = log.clone();
        composer.compose(move |ctx: &mut ComposeCtx| {
            let _ = produce_state(ctx, 0u32, (), move |_state| async move {
                let _guard = Marker(l, "producer dropped");
                std::future::pending::<()>().await;
            });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 100.0, 0.0, 100.0));
        drive(&rt);
        assert!(
            log.lock().unwrap().is_empty(),
            "the producer is parked, nothing dropped yet"
        );

        // A frame that does not mention it: the composition point is gone.
        composer.compose(|_ctx: &mut ComposeCtx| {});
        composer.layout(crate::layout::Constraints::new(0.0, 100.0, 0.0, 100.0));
        drive(&rt);
        assert_eq!(
            *log.lock().unwrap(),
            vec!["producer dropped"],
            "leaving the composition must cancel the producer"
        );
    }
}
