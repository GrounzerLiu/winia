//! Tests for the effect subsystem.

use super::*;
use crate::runtime::composer::ComposeCtx;
use crate::runtime::state::State;



// Effects are not layout citizens.

/// An effect occupies no layout node, so it does not shift its neighbours.
///
/// This is the regression test for the reason `remember_on_forget` exists. The cleanup used to
/// ride on an implicit LEAF NODE, which was a real child of the enclosing container: an effect
/// sitting between two rows of a `Column(spacing = 10)` pushed the second row from y=29 to y=39,
/// i.e. opened a one-spacing gap that nothing in the layout asked for.
#[test]
fn an_effect_adds_no_layout_node_and_no_spacing_slot() {
    let sample = |with_effect: bool| -> (Vec<f32>, usize) {
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx: &mut ComposeCtx| {
            crate::layout::Column::new().spacing(10.0).build(ctx, |ctx| {
                crate::components::Text::new("first").build(ctx);
                if with_effect {
                    DisposableEffect::new(()).build(ctx, |_| Box::new(|| {}));
                }
                crate::components::Text::new("second").build(ctx);
            });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
        let nodes = composer.arena_nodes();
        let root = composer.layout_root_idx().expect("root");
        let ys = nodes[root].children.iter().map(|&c| nodes[c].position.y).collect();
        (ys, nodes.len())
    };
    let (plain, plain_nodes) = sample(false);
    let (with, with_nodes) = sample(true);
    assert_eq!(
        with, plain,
        "an effect must not move its neighbours: without it {plain:?}, with it {with:?}"
    );
    assert_eq!(
        with_nodes, plain_nodes,
        "and it must not allocate a layout node: {plain_nodes} vs {with_nodes}"
    );
}


/// Dropping the whole `Composer` runs the callbacks too.
///
/// This is the path with no explicit collector, and it used to work by accident: the callback
/// rode on a `LayoutNode`, which has a `Drop` that fires `on_remove`. Moving the callback into
/// the slot dropped that guarantee — measured on `remember_infinite_transition`: its animation id
/// stayed in the global table after the composer went away (1 before the drop, still 1 after).
/// `Slot` has its own `Drop` now, and this pins it.
#[test]
fn dropping_the_composer_runs_the_forget_callbacks() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc as StdArc;
    let ran = StdArc::new(AtomicUsize::new(0));
    {
        let mut composer = crate::runtime::composer::Composer::new();
        let r = ran.clone();
        composer.compose(move |ctx: &mut ComposeCtx| {
            let r2 = r.clone();
            DisposableEffect::new(()).build(ctx, move |_| {
                let r3 = r2.clone();
                Box::new(move || {
                    r3.fetch_add(1, Ordering::SeqCst);
                })
            });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 100.0, 0.0, 100.0));
        assert_eq!(ran.load(Ordering::SeqCst), 0, "nothing has been removed yet");
    }
    assert_eq!(
        ran.load(Ordering::SeqCst),
        1,
        "dropping the composer must run the cleanup, not leak it"
    );
}
// DisposableEffect: the lifecycle it promises.

/// Setup runs once for one key, and a key change runs the OLD cleanup before the new setup.
/// The order matters: a setup that installs a listener and a cleanup that removes it must not
/// overlap, or the new listener is removed by the old cleanup.
#[test]
fn a_key_change_runs_the_old_cleanup_before_the_new_setup() {
    use std::sync::{Arc as StdArc, Mutex as StdMutex};
    let log = StdArc::new(StdMutex::new(Vec::<String>::new()));
    let mut composer = crate::runtime::composer::Composer::new();
    let mut frame = |composer: &mut crate::runtime::composer::Composer, key: u32| {
        let l = log.clone();
        composer.compose(move |ctx: &mut ComposeCtx| {
            let l2 = l.clone();
            DisposableEffect::new(key).build(ctx, move |k| {
                l.lock().unwrap().push(format!("setup {k}"));
                // The effect closure is `Fn`, so it may be called again: clone per call.
                let l3 = l2.clone();
                Box::new(move || l3.lock().unwrap().push(format!("cleanup {k}")))
            });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 100.0, 0.0, 100.0));
    };
    frame(&mut composer, 1);
    assert_eq!(*log.lock().unwrap(), vec!["setup 1".to_string()], "first frame: setup only");
    frame(&mut composer, 1);
    assert_eq!(
        *log.lock().unwrap(),
        vec!["setup 1".to_string()],
        "the same key must not re-run the effect"
    );
    frame(&mut composer, 2);
    assert_eq!(
        *log.lock().unwrap(),
        vec!["setup 1".to_string(), "cleanup 1".to_string(), "setup 2".to_string()],
        "key change: old cleanup, then new setup"
    );
}

/// Leaving the composition runs the last cleanup.
#[test]
fn leaving_the_composition_runs_the_cleanup() {
    use std::sync::{Arc as StdArc, Mutex as StdMutex};
    let log = StdArc::new(StdMutex::new(Vec::<String>::new()));
    let mut composer = crate::runtime::composer::Composer::new();
    let show = crate::runtime::state::State::new(true);
    let mut frame = |composer: &mut crate::runtime::composer::Composer| {
        let l = log.clone();
        let s = show.clone();
        composer.compose(move |ctx: &mut ComposeCtx| {
            if s.get() {
                let l2 = l.clone();
                DisposableEffect::new(()).build(ctx, move |_| {
                    l.lock().unwrap().push("setup".to_string());
                    let l3 = l2.clone();
                    Box::new(move || l3.lock().unwrap().push("cleanup".to_string()))
                });
            }
        });
        composer.layout(crate::layout::Constraints::new(0.0, 100.0, 0.0, 100.0));
    };
    frame(&mut composer);
    assert_eq!(*log.lock().unwrap(), vec!["setup".to_string()]);
    show.set(false);
    frame(&mut composer);
    assert_eq!(
        *log.lock().unwrap(),
        vec!["setup".to_string(), "cleanup".to_string()],
        "the branch is gone, so the cleanup must have run"
    );
}

/// A PANICKING cleanup must not poison the effect's lock. The cleanup used to run with the
/// mutex guard alive, so a panic left it poisoned and every later `lock().unwrap()` in that
/// effect panicked — the instance was dead for the rest of the run. The cleanup is called
/// outside the guard now, so the next key change still works.
#[test]
fn a_panicking_cleanup_does_not_kill_the_effect() {
    use std::sync::{Arc as StdArc, Mutex as StdMutex};
    let setups = StdArc::new(StdMutex::new(0usize));
    let mut composer = crate::runtime::composer::Composer::new();
    let mut frame = |composer: &mut crate::runtime::composer::Composer, key: u32| {
        let n = setups.clone();
        composer.compose(move |ctx: &mut ComposeCtx| {
            let n2 = n.clone();
            DisposableEffect::new(key).build(ctx, move |k| {
                *n.lock().unwrap() += 1;
                let cleanup: Box<dyn FnOnce() + Send> = if k == 1 {
                    // The FIRST effect's cleanup panics; the second's is well behaved.
                    Box::new(|| panic!("cleanup blew up"))
                } else {
                    let _ = n2.clone();
                    Box::new(|| {})
                };
                cleanup
            });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 100.0, 0.0, 100.0));
    };
    frame(&mut composer, 1);
    // The key change runs the panicking cleanup. Swallow it: this test is about what the lock
    // looks like afterwards, not about propagating a caller's panic.
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| frame(&mut composer, 2)));
    assert!(panicked.is_err(), "the caller's cleanup really did panic");
    assert_eq!(*setups.lock().unwrap(), 1, "setup 2 had not run yet when the panic escaped");

    // THE POINT: the effect still works. With the old code the cleanup ran under the guard, so
    // this second key change would panic on `lock().unwrap()` with a PoisonError instead of
    // running setup 3.
    frame(&mut composer, 3);
    assert_eq!(
        *setups.lock().unwrap(),
        2,
        "a panicking cleanup must not poison the effect's lock: setup 3 has to run"
    );
}

// observe_watch: which channel is it actually listening to.

/// A REPLACED receiver is re-subscribed. The first version spawned once behind a `started`
/// flag, so a different channel at the same call site was silently ignored and the state kept
/// following the old one forever.
#[test]
fn observe_watch_follows_a_replaced_receiver() {
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let _e = rt.enter();
    let (tx1, rx1) = tokio::sync::watch::channel(1u32);
    let (tx2, rx2) = tokio::sync::watch::channel(2u32);
    let mut composer = crate::runtime::composer::Composer::new();
    let mut compose_with = |composer: &mut crate::runtime::composer::Composer,
                            rx: tokio::sync::watch::Receiver<u32>| {
        composer.compose(move |ctx: &mut ComposeCtx| {
            let v = observe_watch(ctx, rx.clone(), 0);
            let _ = v.get();
        });
        composer.layout(crate::layout::Constraints::new(0.0, 100.0, 0.0, 100.0));
    };
    compose_with(&mut composer, rx1.clone());
    // Same call site, a DIFFERENT channel: this is the replacement.
    compose_with(&mut composer, rx2.clone());
    // Drive the runtime so the spawned subscription tasks reach their first await.
    rt.block_on(async { for _ in 0..50 { tokio::task::yield_now().await; } });
    // Send on both channels; only the CURRENT one may reach the state.
    tx1.send(11).unwrap();
    tx2.send(22).unwrap();
    rt.block_on(async { for _ in 0..50 { tokio::task::yield_now().await; } });
    // Read the slot's value by composing once more with the same receiver. The value has to come
    // back through an AtomicU32: `compose` takes a `move` closure, so a plain `let mut seen`
    // would be captured BY VALUE and the assignment inside would never leave the closure.
    let seen = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let seen_out = seen.clone();
    let r2 = rx2.clone();
    composer.compose(move |ctx: &mut ComposeCtx| {
        let v = observe_watch(ctx, r2.clone(), 0);
        seen_out.store(v.get(), std::sync::atomic::Ordering::SeqCst);
    });
    assert_eq!(
        seen.load(std::sync::atomic::Ordering::SeqCst),
        22,
        "the state must follow the REPLACED receiver (22), not the one it started on (11)"
    );
}

// LaunchedEffect: replacement order.

/// A key change ABORTS the old body (a parked future is dropped, it does not linger) and runs the
/// replacement exactly once.
///
/// This arm pins the ABORT, not the join: on a `current_thread` runtime the abort lands before the
/// replacement is polled either way, so deleting the join still passes here. The join is pinned
/// separately by `a_replacement_waits_for_the_aborted_body_to_stop`, which runs on a two-worker
/// runtime and gives the old body's Drop enough time to be observably still running.
#[test]
fn a_launched_effect_key_change_aborts_the_old_body() {
    use std::sync::{Arc as StdArc, Mutex as StdMutex};
    struct Marker(StdArc<StdMutex<Vec<&'static str>>>, &'static str);
    impl Drop for Marker {
        fn drop(&mut self) { self.0.lock().unwrap().push(self.1); }
    }
    let log = StdArc::new(StdMutex::new(Vec::<&'static str>::new()));
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let _e = rt.enter();
    let mut composer = crate::runtime::composer::Composer::new();
    let mut frame = |composer: &mut crate::runtime::composer::Composer, key: u32| {
        let l = log.clone();
        composer.compose(move |ctx: &mut ComposeCtx| {
            let l2 = l.clone();
            LaunchedEffect::new(key).build(ctx, move |_scope| {
                let l3 = l2.clone();
                async move {
                    // Held across an await, so it is dropped only when this future is dropped
                    // (aborted) or completes.
                    let _guard = Marker(l3.clone(), "old dropped");
                    l3.lock().unwrap().push("old start");
                    // Park forever: only the abort ends this body.
                    std::future::pending::<()>().await;
                }
            });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 100.0, 0.0, 100.0));
    };
    frame(&mut composer, 1);
    rt.block_on(async { for _ in 0..10 { tokio::task::yield_now().await; } });
    assert_eq!(*log.lock().unwrap(), vec!["old start"], "the first body started and parked");

    // Replace it with a body that logs when it starts.
    let l = log.clone();
    composer.compose(move |ctx: &mut ComposeCtx| {
        let l2 = l.clone();
        LaunchedEffect::new(2u32).build(ctx, move |_scope| {
            let l3 = l2.clone();
            async move { l3.lock().unwrap().push("new start"); }
        });
    });
    composer.layout(crate::layout::Constraints::new(0.0, 100.0, 0.0, 100.0));
    rt.block_on(async { for _ in 0..40 { tokio::task::yield_now().await; } });

    let seen = log.lock().unwrap().clone();
    let dropped_at = seen.iter().position(|e| *e == "old dropped");
    let started_at = seen.iter().position(|e| *e == "new start");
    assert!(dropped_at.is_some(), "the aborted body was dropped, not left parked: {seen:?}");
    assert!(started_at.is_some(), "the replacement body ran: {seen:?}");
    assert_eq!(
        seen.iter().filter(|e| **e == "new start").count(),
        1,
        "and it ran exactly once: {seen:?}"
    );
}
/// After a frame_tick broadcast, with_frame_nanos returns a fresh timestamp (not the initial 0).
#[test]
fn frame_clock_ticks_and_waits() {
    let rt = tokio::runtime::Builder::new_current_thread().enable_time().build().unwrap();
    rt.block_on(async {
        let fut = with_frame_nanos();
        frame_tick();
        let v = fut.await;
        assert!(v > 0, "the frame timestamp must be > 0 (got {v})");
        // The second tick returns a newer timestamp.
        let fut2 = with_frame_nanos();
        std::thread::sleep(std::time::Duration::from_millis(2));
        frame_tick();
        let v2 = fut2.await;
        assert!(v2 > v, "the second tick's timestamp must increase ({v} -> {v2})");
    });
}

// observe_watch: leaving the composition stops the subscription.

/// Removing the composition point stops the watch task.
///
/// The task is spawned on the runtime directly (`rt.spawn`, not `CoroutineScope::spawn`), so
/// `ScopeState::drop` does not know about it, and dropping an `AbortHandle` cancels nothing.
/// Before the `on_forget` hook was added the task outlived the slot and kept writing the
/// `State` after the composition point (and every reader of it) was gone.
#[test]
fn observe_watch_stops_when_the_composition_point_is_removed() {
    use std::sync::{Arc as StdArc, Mutex as StdMutex};
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let _e = rt.enter();
    let (tx, rx) = tokio::sync::watch::channel(1u32);
    let mut composer = crate::runtime::composer::Composer::new();
    let kept: StdArc<StdMutex<Option<State<u32>>>> = StdArc::new(StdMutex::new(None));

    let k = kept.clone();
    composer.compose(move |ctx: &mut ComposeCtx| {
        let v = observe_watch(ctx, rx.clone(), 0);
        *k.lock().unwrap() = Some(v);
    });
    composer.layout(crate::layout::Constraints::new(0.0, 100.0, 0.0, 100.0));
    rt.block_on(async { for _ in 0..50 { tokio::task::yield_now().await; } });

    let state = kept.lock().unwrap().clone().expect("the effect returned a state");
    assert_eq!(state.peek(), 1, "the subscription seeds the channel's current value");

    // A frame that does not mention the effect: the slot is dropped, so its forget callback
    // runs and has to abort the task.
    composer.compose(|_ctx: &mut ComposeCtx| {});
    composer.layout(crate::layout::Constraints::new(0.0, 100.0, 0.0, 100.0));
    rt.block_on(async { for _ in 0..50 { tokio::task::yield_now().await; } });

    let _ = tx.send(2);
    rt.block_on(async { for _ in 0..50 { tokio::task::yield_now().await; } });
    assert_eq!(
        state.peek(),
        1,
        "a removed composition point must stop listening: the task outlived its slot"
    );
}

// remember_updated_state.

/// The handle follows the latest value, and the write is silent.
///
/// A `LaunchedEffect::new(())` body closes over what it captured when it started; this is the
/// handle it reads instead. The write must not schedule a recompose: the value changed because
/// this composition ran, so a second frame would be pure waste.
#[test]
fn remember_updated_state_follows_the_latest_value_without_scheduling_a_recompose() {
    let mut composer = crate::runtime::composer::Composer::new();
    let kept: std::sync::Arc<std::sync::Mutex<Option<State<u32>>>> =
        std::sync::Arc::new(std::sync::Mutex::new(None));

    let k = kept.clone();
    composer.compose(move |ctx: &mut ComposeCtx| {
        let latest = remember_updated_state(ctx, 1u32);
        *k.lock().unwrap() = Some(latest.clone());
        let _ = latest.get();
    });
    composer.layout(crate::layout::Constraints::new(0.0, 100.0, 0.0, 100.0));
    assert_eq!(
        kept.lock().unwrap().as_ref().unwrap().peek(),
        1,
        "the first composition seeds the handle"
    );

    // Same call site, a new value.
    composer.compose(|ctx: &mut ComposeCtx| {
        let _ = remember_updated_state(ctx, 2u32);
    });
    composer.layout(crate::layout::Constraints::new(0.0, 100.0, 0.0, 100.0));
    assert_eq!(
        kept.lock().unwrap().as_ref().unwrap().peek(),
        2,
        "the handle must follow the latest value across recompositions"
    );
    assert!(
        !composer.has_pending_states(),
        "the silent write must not schedule another recompose"
    );
}

// CoroutineScope: cancelled by the composition point.

/// The scope cancels with its composition point, even while a clone is still alive.
///
/// Teardown used to hang off `ScopeState::drop`, i.e. off the refcount: any caller that kept a
/// clone (a struct field, a task spawned directly on the runtime) kept every task running past the
/// point that owned it. The cancellation is registered as an `on_forget` callback now.
#[test]
fn a_scope_is_cancelled_by_its_composition_point_not_by_refcount() {
    use std::sync::{Arc as StdArc, Mutex as StdMutex};

    struct Marker(StdArc<StdMutex<Vec<&'static str>>>, &'static str);
    impl Drop for Marker {
        fn drop(&mut self) {
            self.0.lock().unwrap().push(self.1);
        }
    }

    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let _e = rt.enter();
    let log: StdArc<StdMutex<Vec<&'static str>>> = StdArc::new(StdMutex::new(Vec::new()));
    let kept: StdArc<StdMutex<Option<CoroutineScope>>> = StdArc::new(StdMutex::new(None));
    let mut composer = crate::runtime::composer::Composer::new();

    let k = kept.clone();
    let l = log.clone();
    composer.compose(move |ctx: &mut ComposeCtx| {
        let scope = remember_coroutine_scope(ctx);
        let l2 = l.clone();
        scope.spawn(async move {
            let _guard = Marker(l2, "task dropped");
            // Park forever: only the scope's teardown ends this.
            std::future::pending::<()>().await;
        });
        // Keep a clone OUTSIDE the composition: refcount teardown would never run.
        *k.lock().unwrap() = Some(scope);
    });
    composer.layout(crate::layout::Constraints::new(0.0, 100.0, 0.0, 100.0));
    rt.block_on(async { for _ in 0..20 { tokio::task::yield_now().await; } });
    assert!(log.lock().unwrap().is_empty(), "the task is parked, nothing dropped yet");

    // The composition point goes away; the clone in `kept` does not.
    composer.compose(|_ctx: &mut ComposeCtx| {});
    composer.layout(crate::layout::Constraints::new(0.0, 100.0, 0.0, 100.0));
    rt.block_on(async { for _ in 0..20 { tokio::task::yield_now().await; } });

    assert_eq!(
        *log.lock().unwrap(),
        vec!["task dropped"],
        "the scope must cancel with its composition point, even while a clone is still alive"
    );
}

// side_effect: after the composition commits.

/// A side effect is deferred past the rest of the composition.
///
/// It is registered FIRST here and still runs LAST, which is the whole contract: the callback runs
/// at the end of the compose pass, after the tree is materialized, not at its call position.
#[test]
fn a_side_effect_runs_after_the_whole_composition_is_built() {
    use std::sync::{Arc as StdArc, Mutex as StdMutex};
    let log: StdArc<StdMutex<Vec<&'static str>>> = StdArc::new(StdMutex::new(Vec::new()));
    let mut composer = crate::runtime::composer::Composer::new();

    let l1 = log.clone();
    let l2 = log.clone();
    composer.compose(move |ctx: &mut ComposeCtx| {
        let l = l1.clone();
        side_effect(ctx, move || l.lock().unwrap().push("side effect"));
        DisposableEffect::new(1u32).build(ctx, move |_| {
            l2.lock().unwrap().push("setup");
            Box::new(|| {})
        });
    });
    composer.layout(crate::layout::Constraints::new(0.0, 100.0, 0.0, 100.0));

    assert_eq!(
        *log.lock().unwrap(),
        vec!["setup", "side effect"],
        "the side effect is deferred past the rest of the composition"
    );
}

/// It runs once per composition, and not at all on a pass that does not happen.
#[test]
fn a_side_effect_does_not_run_when_the_composition_does_not() {
    use std::sync::{Arc as StdArc, Mutex as StdMutex};
    let runs: StdArc<StdMutex<usize>> = StdArc::new(StdMutex::new(0));
    let mut composer = crate::runtime::composer::Composer::new();

    let r = runs.clone();
    assert!(
        composer.recompose(move |ctx: &mut ComposeCtx| {
            side_effect(ctx, move || *r.lock().unwrap() += 1);
        }),
        "a fresh composer has work to do"
    );
    assert_eq!(*runs.lock().unwrap(), 1, "one composition, one run");

    // Nothing is pending: `recompose` short-circuits and the content never runs.
    assert!(!composer.recompose(|_ctx: &mut ComposeCtx| {}));
    assert_eq!(*runs.lock().unwrap(), 1, "an idle frame must not re-run the side effect");
}

    /// The replacement waits for the aborted body to STOP, not just to be told to stop.
    ///
    /// The single-threaded arm above cannot show this: on a current-thread runtime the abort lands
    /// before the replacement is polled either way, so deleting the join still passes. Here the old
    /// body's Drop blocks for a moment, which turns "did the replacement start before the old one
    /// was gone" into a fact instead of a race — without the join the new body runs immediately on
    /// the other worker and sees the flag still unset.
    #[test]
    fn a_replacement_waits_for_the_aborted_body_to_stop() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::{mpsc, Arc as StdArc, Mutex as StdMutex};
        use std::time::Duration;

        /// The old body's Drop blocks until the test releases it, so "the replacement has not
        /// started yet" is a fact the test controls rather than a race it hopes to win.
        struct GatedStop {
            gate: StdMutex<Option<mpsc::Receiver<()>>>,
            entered: StdArc<AtomicBool>,
            stopped: StdArc<AtomicBool>,
        }
        impl Drop for GatedStop {
            fn drop(&mut self) {
                self.entered.store(true, Ordering::SeqCst);
                if let Some(rx) = self.gate.lock().unwrap().take() {
                    let _ = rx.recv_timeout(Duration::from_secs(5));
                }
                self.stopped.store(true, Ordering::SeqCst);
            }
        }

        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let _entered = rt.enter();

        let (release, wait) = mpsc::channel::<()>();
        let old_started = StdArc::new(AtomicBool::new(false));
        let drop_entered = StdArc::new(AtomicBool::new(false));
        let old_stopped = StdArc::new(AtomicBool::new(false));
        let replacement_ran = StdArc::new(AtomicBool::new(false));
        let mut composer = crate::runtime::composer::Composer::new();

        let started = old_started.clone();
        let gate = StdMutex::new(Some(wait));
        let entered = drop_entered.clone();
        let stopped = old_stopped.clone();
        composer.compose(move |ctx: &mut ComposeCtx| {
            LaunchedEffect::new(1u32).build(ctx, move |_| {
                let started = started.clone();
                let gate = gate.lock().unwrap().take();
                let entered = entered.clone();
                let stopped = stopped.clone();
                async move {
                    started.store(true, Ordering::SeqCst);
                    let _guard = GatedStop {
                        gate: StdMutex::new(gate),
                        entered,
                        stopped,
                    };
                    std::future::pending::<()>().await;
                }
            });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 100.0, 0.0, 100.0));

        // Wait until the old body is really running; a spawn that has not been polled holds no guard.
        rt.block_on(async {
            for _ in 0..500 {
                if old_started.load(Ordering::SeqCst) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        });
        assert!(old_started.load(Ordering::SeqCst), "the old body started");

        let ran = replacement_ran.clone();
        composer.compose(move |ctx: &mut ComposeCtx| {
            LaunchedEffect::new(2u32).build(ctx, move |_| {
                let ran = ran.clone();
                async move {
                    ran.store(true, Ordering::SeqCst);
                }
            });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 100.0, 0.0, 100.0));

        // The aborted body is now inside its Drop, blocked on the gate. With the join the
        // replacement CANNOT have started: it is parked on the old task finishing.
        rt.block_on(async {
            for _ in 0..500 {
                if drop_entered.load(Ordering::SeqCst) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        });
        assert!(
            drop_entered.load(Ordering::SeqCst),
            "the aborted body reached its Drop"
        );
        assert!(
            !replacement_ran.load(Ordering::SeqCst),
            "the replacement started while the aborted body was still being dropped: the join is missing"
        );

        release.send(()).expect("the gate is still open");
        rt.block_on(async {
            for _ in 0..2000 {
                if replacement_ran.load(Ordering::SeqCst) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        });
        // `old_stopped` is set by the Drop that released the gate, so asserting it here would be
        // vacuous; the gate having been released and the replacement running is the real evidence.
        assert!(replacement_ran.load(Ordering::SeqCst), "and then the replacement ran");
    }
    /// A task spawned through the effect body's scope dies with the composition point.
    ///
    /// scope.spawn puts the task in ScopeState.handles, and that list is only aborted by the
    /// scope's on_forget (or by ScopeState::drop). A task that captured a scope clone keeps the Arc
    /// alive, so drop never runs: without the registration the task outlives its slot forever.
    #[test]
    fn a_scope_spawned_task_dies_with_the_effect() {
        use std::sync::{Arc as StdArc, Mutex as StdMutex};

        struct Marker(StdArc<StdMutex<Vec<&'static str>>>, &'static str);
        impl Drop for Marker {
            fn drop(&mut self) {
                self.0.lock().unwrap().push(self.1);
            }
        }

        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let _entered = rt.enter();
        let log: StdArc<StdMutex<Vec<&'static str>>> = StdArc::new(StdMutex::new(Vec::new()));
        let mut composer = crate::runtime::composer::Composer::new();

        let l = log.clone();
        composer.compose(move |ctx: &mut ComposeCtx| {
            LaunchedEffect::new(1u32).build(ctx, move |scope| {
                // The task holds a scope clone, which is what keeps the Arc alive after the body
                // finishes: refcount teardown alone can never abort it.
                let keep = scope.clone();
                scope.spawn(async move {
                    let _guard = Marker(l, "spawned task dropped");
                    let _keep = keep;
                    std::future::pending::<()>().await;
                });
                async {}
            });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 100.0, 0.0, 100.0));
        rt.block_on(async { for _ in 0..30 { tokio::task::yield_now().await; } });
        assert!(log.lock().unwrap().is_empty(), "the spawned task is parked, nothing dropped yet");

        // The composition point goes away; the spawned task must go with it.
        composer.compose(|_ctx: &mut ComposeCtx| {});
        composer.layout(crate::layout::Constraints::new(0.0, 100.0, 0.0, 100.0));
        rt.block_on(async { for _ in 0..30 { tokio::task::yield_now().await; } });

        assert_eq!(
            *log.lock().unwrap(),
            vec!["spawned task dropped"],
            "a task spawned through the effect's scope must die with the composition point"
        );
    }
    /// A key change cancels the tasks the OLD body spawned, not just the body itself.
    ///
    /// The body is handed the composition scope, so `scope.spawn` children are part of that body: a
    /// restart has to take them down with it, the way a Compose coroutine cancels its children.
    #[test]
    fn a_key_change_cancels_the_tasks_the_old_body_spawned() {
        use std::sync::{Arc as StdArc, Mutex as StdMutex};

        struct Marker(StdArc<StdMutex<Vec<&'static str>>>, &'static str);
        impl Drop for Marker {
            fn drop(&mut self) {
                self.0.lock().unwrap().push(self.1);
            }
        }

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _entered = rt.enter();
        let log: StdArc<StdMutex<Vec<&'static str>>> = StdArc::new(StdMutex::new(Vec::new()));
        let mut composer = crate::runtime::composer::Composer::new();

        let l = log.clone();
        composer.compose(move |ctx: &mut ComposeCtx| {
            LaunchedEffect::new(1u32).build(ctx, move |scope| {
                scope.spawn(async move {
                    let _guard = Marker(l, "child of key 1");
                    std::future::pending::<()>().await;
                });
                async {}
            });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 100.0, 0.0, 100.0));
        rt.block_on(async { for _ in 0..30 { tokio::task::yield_now().await; } });
        assert!(log.lock().unwrap().is_empty(), "the child task is parked");

        // Same slot, new key: the body restarts, and its children go with it.
        composer.compose(move |ctx: &mut ComposeCtx| {
            LaunchedEffect::new(2u32).build(ctx, |_| async {});
        });
        composer.layout(crate::layout::Constraints::new(0.0, 100.0, 0.0, 100.0));
        rt.block_on(async { for _ in 0..30 { tokio::task::yield_now().await; } });

        assert_eq!(
            *log.lock().unwrap(),
            vec!["child of key 1"],
            "the old body's child must be cancelled by the restart"
        );
    }
