//! Tests for the effect subsystem.

use super::*;
use crate::runtime::composer::ComposeCtx;
use crate::runtime::state::State;



// ── Effects are not layout citizens ──

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
// ── DisposableEffect: the lifecycle it promises ──

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

// ── observe_watch: which channel is it actually listening to ──

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

// ── LaunchedEffect: replacement order ──

/// A key change ABORTS the old body (a parked future is dropped, it does not linger) and runs the
/// replacement exactly once.
///
/// KNOWN GAP, recorded rather than hidden: this does NOT prove the replacement waits for the old
/// body to stop. The mutation that removes that join — spawning the new body directly — still
/// passes here, because on a `current_thread` runtime the abort lands before the new task is
/// polled either way. The join matters for the multi-threaded runtime `run_app` builds, where the
/// two tasks really do run on different workers, and that interleaving is not reproducible with a
/// deterministic test on this harness. The ordering is therefore a code-level guarantee with a
/// comment saying why, not a pinned behaviour.
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
/// frame_tick 广播后，with_frame_nanos 返回新时间戳（非初始 0）
#[test]
fn frame_clock_ticks_and_waits() {
    let rt = tokio::runtime::Builder::new_current_thread().enable_time().build().unwrap();
    rt.block_on(async {
        let fut = with_frame_nanos();
        frame_tick();
        let v = fut.await;
        assert!(v > 0, "帧时间戳必须 > 0（实际 {v}）");
        // 第二次 tick 返回更新的时间戳
        let fut2 = with_frame_nanos();
        std::thread::sleep(std::time::Duration::from_millis(2));
        frame_tick();
        let v2 = fut2.await;
        assert!(v2 > v, "第二次 tick 时间戳必须递增（{v} -> {v2}）");
    });
}

// ── observe_watch: leaving the composition stops the subscription ──

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

// ── remember_updated_state ──

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

// ── CoroutineScope: cancelled by the composition point ──

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
