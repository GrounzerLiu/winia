    use super::*;


    // ── State::changes: observing from outside a composition ──

    fn poll_once<T>(s: &mut StateChanges<T>, cx: &mut Context<'_>) -> Poll<Option<T>>
    where
        T: Clone + PartialEq,
    {
        use futures_util::Stream as _;
        Pin::new(s).poll_next(cx)
    }

    /// The seed is the current value; then one item per change.
    #[test]
    fn changes_emits_the_current_value_then_each_change() {
        let waker = futures_util::task::noop_waker();
        let mut cx = Context::from_waker(&waker);
        let state = State::new(0u32);
        let mut changes = state.changes();

        assert_eq!(poll_once(&mut changes, &mut cx), Poll::Ready(Some(0)), "seed");
        assert_eq!(poll_once(&mut changes, &mut cx), Poll::Pending, "nothing new yet");

        state.set(1);
        assert_eq!(poll_once(&mut changes, &mut cx), Poll::Ready(Some(1)));

        // A write that changes nothing must not re-emit.
        state.set(1);
        assert_eq!(poll_once(&mut changes, &mut cx), Poll::Pending);
    }

    /// Several writes between two polls collapse to the latest value: the stream reads the cell.
    #[test]
    fn changes_conflates_writes_between_polls() {
        let waker = futures_util::task::noop_waker();
        let mut cx = Context::from_waker(&waker);
        let state = State::new(0u32);
        let mut changes = state.changes();

        assert_eq!(poll_once(&mut changes, &mut cx), Poll::Ready(Some(0)));
        state.set(1);
        state.set(2);
        state.set(3);
        assert_eq!(
            poll_once(&mut changes, &mut cx),
            Poll::Ready(Some(3)),
            "the latest value wins; intermediate ones are not queued"
        );
    }

    /// Dropping the stream removes its observer instead of leaving a dead entry behind.
    #[test]
    fn a_dropped_stream_unsubscribes() {
        let state = State::new(0u32);
        let changes = state.changes();
        {
            let list = state.raw.inner.signal.observers.get().expect("observer list");
            assert_eq!(list.lock().len(), 1);
        }
        drop(changes);
        let list = state.raw.inner.signal.observers.get().expect("observer list");
        assert!(list.lock().is_empty(), "drop must unsubscribe");
    }

    /// Two collectors on one state both receive the change.
    #[test]
    fn every_collector_sees_the_change() {
        let waker = futures_util::task::noop_waker();
        let mut cx = Context::from_waker(&waker);
        let state = State::new(0u32);
        let mut a = state.changes();
        let mut b = state.changes();
        assert_eq!(poll_once(&mut a, &mut cx), Poll::Ready(Some(0)));
        assert_eq!(poll_once(&mut b, &mut cx), Poll::Ready(Some(0)));

        state.set(5);
        assert_eq!(poll_once(&mut a, &mut cx), Poll::Ready(Some(5)));
        assert_eq!(poll_once(&mut b, &mut cx), Poll::Ready(Some(5)));
    }


    #[test]
    fn state_id_is_internal_newtype_with_public_compatibility() {
        let state = State::new(7i32);
        let other = State::new(8i32);
        let internal = state.signal_id();
        assert_eq!(state.state_id(), internal);
        assert_eq!(state.raw.inner.public_id, state.id());
        assert_ne!(state.id(), other.id(), "public compatibility IDs remain distinct");
        assert_ne!(internal, other.signal_id());
        assert_eq!(internal.raw(), state.state_id().raw());
        assert_eq!(StateId::new(u64::MAX).raw(), u64::MAX);
        assert_ne!(internal, StateId::new(internal.raw().wrapping_add(1)));
    }

    #[test]
    fn test_backchannel_write_lands_without_notify() {
        let queue = ComposerSubscription::new();
        let channel = Backchannel::new(0i32);
        channel.get();
        let mut frame = begin_compose_deps_with_queue(Arc::downgrade(&queue));
        channel.get();
        take_deps();
        frame.commit();

        channel.set(42);
        assert_eq!(channel.get(), 42);
        assert!(queue.is_empty(), "Backchannel::set must not enqueue");
    }

    #[test]
    fn unread_state_does_not_notify() {
        let queue = ComposerSubscription::new();
        let state = State::new(0i32);
        state.set(1);
        assert!(queue.is_empty(), "未读取的 State 不应通知 Composer");
    }

    #[test]
    fn revision_handshake_catches_write_after_subscription() {
        let queue = ComposerSubscription::new();
        let state = State::new(0i32);
        let signal = state.raw.inner.signal.clone();
        let observed = queue.subscribe_signal(&signal).unwrap();

        // Simulate a write in the gap between the subscription snapshot and
        // the post-read check. The check must preserve the invalidation even
        // when the caller has not yet finished reading the value.
        state.set(1);
        signal.enqueue_if_changed(&queue, observed);

        assert_eq!(queue.drain(), vec![state.signal_id()]);
    }

    #[test]
    fn revision_handshake_does_not_requeue_after_unsubscribe() {
        let queue = ComposerSubscription::new();
        let state = State::new(0i32);
        let signal = state.raw.inner.signal.clone();
        let observed = queue.subscribe_signal(&signal).unwrap();

        queue.unsubscribe_all();
        state.set(1);
        signal.enqueue_if_changed(&queue, observed);

        assert!(queue.is_empty(), "取消订阅后 handshake 不应重新入队");
    }

    #[test]
    fn closed_subscription_cannot_readd_signal() {
        let queue = ComposerSubscription::new();
        let state = State::new(0i32);
        let signal = state.raw.inner.signal.clone();
        assert!(queue.subscribe_signal(&signal).is_some());
        queue.enqueue(state.signal_id());

        queue.unsubscribe_all();
        assert!(queue.is_empty(), "closed Composer queue must discard stale pending IDs");
        assert!(!queue.enqueue(state.signal_id()));
        assert!(queue.subscribe_signal(&signal).is_none());
        state.set(1);
        assert!(queue.is_empty(), "关闭后的 Composer queue 不应重新收到通知");
        assert!(signal.subscribers.lock().is_empty(), "notify 应清理 closed subscriber");
    }

    /// State 创建无 owner；两个 Composer 读取同一 State 后都订阅，重复读取去重。
    #[test]
    fn drain_non_compose_preserves_mixed_compose_pending() {
        let queue = ComposerSubscription::new();
        let compose_state = State::new(1i32);
        let layout_state = State::new(2i32);
        let unknown = StateId::new(u64::MAX);

        queue.enqueue(compose_state.signal_id());
        queue.enqueue(layout_state.signal_id());
        queue.enqueue(unknown);

        let compose_ids = std::collections::HashSet::from([compose_state.signal_id()]);
        let layout_ids = std::collections::HashSet::from([
            compose_state.signal_id(),
            layout_state.signal_id(),
        ]);
        let layout_pending = queue.drain_non_compose_collect_layout(&compose_ids, &layout_ids);

        assert_eq!(
            layout_pending,
            vec![compose_state.signal_id(), layout_state.signal_id()],
            "mixed compose/layout IDs must mark layout without being consumed from compose"
        );
        assert_eq!(queue.pending_ids(), vec![compose_state.signal_id()]);
    }

    #[test]
    fn cross_composer_subscription_fan_out_and_dedup() {
        let state = State::new(0i32);
        let main = ComposerSubscription::new();
        let overlay = ComposerSubscription::new();

        let mut main_frame = begin_compose_deps_with_queue(Arc::downgrade(&main));
        state.get();
        state.get();
        take_deps();
        main_frame.commit();

        let mut overlay_frame = begin_compose_deps_with_queue(Arc::downgrade(&overlay));
        state.get();
        take_deps();
        overlay_frame.commit();

        state.set(1);
        assert_eq!(main.drain(), vec![state.signal_id()]);
        assert_eq!(overlay.drain(), vec![state.signal_id()]);

        // Re-reading the same signal must not duplicate the subscriber.
        let mut reread_frame = begin_compose_deps_with_queue(Arc::downgrade(&overlay));
        state.get();
        take_deps();
        reread_frame.commit();
        state.set(2);
        assert_eq!(overlay.drain(), vec![state.signal_id()]);

        drop(overlay);
        state.set(3);
        assert_eq!(main.drain(), vec![state.signal_id()]);
    }

    #[test]
    fn explicit_unsubscribe_stops_notifications() {
        let queue = ComposerSubscription::new();
        let state = State::new(0i32);
        let mut frame = begin_compose_deps_with_queue(Arc::downgrade(&queue));
        state.get();
        take_deps();
        frame.commit();

        queue.unsubscribe_all();
        state.set(1);
        assert!(queue.is_empty(), "取消订阅后不应再收到 State 通知");
    }

    /// Re-reading a state must not grow the tracked set: the container is keyed by signal id.
    ///
    /// The shape this pins down is also the fix for a measured defect — the container used to be a
    /// list, and every read scanned it to answer "already tracked?", which made a list of N rows cost
    /// O(N²) per frame (800 rows: 5 ms of a 7.4 ms frame, `docs/benchmarks.md`).
    #[test]
    fn repeated_reads_track_one_entry_per_signal() {
        let queue = ComposerSubscription::new();
        let a = State::new(1i32);
        let b = State::new(2i32);
        let mut frame = begin_compose_deps_with_queue(Arc::downgrade(&queue));
        for _ in 0..5 {
            a.get();
            b.get();
        }
        take_deps();
        frame.commit();
        assert_eq!(queue.signal_ids().len(), 2, "repeat reads must track one entry per signal");
    }

    /// Reading N states must not cost O(N²).
    ///
    /// A ratio rather than an absolute figure, deliberately loose: linear work is ~40x for 40x the
    /// states, while the scan this replaced was ~1600x, so the bound separates the two shapes with a
    /// wide margin on any machine. Written because the defect it guards was invisible to a
    /// correctness test — every assertion still passed while a frame spent 5 ms on 800 reads.
    #[test]
    fn reading_many_states_does_not_rescan_every_tracked_signal() {
        fn measure(states: usize) -> std::time::Duration {
            let queue = ComposerSubscription::new();
            let handles: Vec<State<i64>> = (0..states as i64).map(State::new).collect();
            let mut frame = begin_compose_deps_with_queue(Arc::downgrade(&queue));
            for handle in &handles {
                handle.get(); // warm the tracking, then time steady-state reads
            }
            let mut best = std::time::Duration::MAX;
            for _ in 0..3 {
                let start = std::time::Instant::now();
                for _ in 0..5 {
                    for handle in &handles {
                        handle.get();
                    }
                }
                best = best.min(start.elapsed());
            }
            take_deps();
            frame.commit();
            best
        }

        let small = measure(50);
        let large = measure(2000);
        assert!(
            large < small * 200,
            "40x the states must not cost more than 200x the time: {small:?} vs {large:?}"
        );
    }

    #[test]
    fn retain_signals_removes_stale_reads() {
        let queue = ComposerSubscription::new();
        let state = State::new(0i32);
        let mut frame = begin_compose_deps_with_queue(Arc::downgrade(&queue));
        state.get();
        take_deps();
        frame.commit();

        let live = std::collections::HashSet::new();
        queue.retain_signals(&live);
        state.set(1);
        assert!(queue.is_empty(), "读取集合移除后不应保留旧订阅");
    }

    #[test]
    fn nested_dependency_frames_restore_outer_reads() {
        let outer_a = State::new(1i32);
        let inner_b = State::new(2i32);
        let outer_c = State::new(3i32);
        let outer_queue = ComposerSubscription::new();
        let inner_queue = ComposerSubscription::new();

        let mut outer = begin_compose_deps_with_queue(Arc::downgrade(&outer_queue));
        assert_eq!(outer_a.get(), 1);

        let mut inner = begin_layout_deps_with_queue(Arc::downgrade(&inner_queue));
        assert_eq!(inner_b.get(), 2);
        let inner_deps = take_deps();
        assert_eq!(inner_deps.len(), 1);
        assert_eq!(inner_deps[0].0.id(), inner_b.signal_id());
        inner.commit();

        assert_eq!(outer_c.get(), 3);
        let outer_deps = take_deps();
        assert_eq!(outer_deps.len(), 2);
        assert!(outer_deps.iter().any(|(signal, _)| signal.id() == outer_a.signal_id()));
        assert!(outer_deps.iter().any(|(signal, _)| signal.id() == outer_c.signal_id()));
        outer.commit();

        inner_b.set(4);
        assert_eq!(inner_queue.drain(), vec![inner_b.signal_id()]);
        outer_a.set(5);
        outer_c.set(6);
        assert_eq!(outer_queue.drain(), vec![outer_a.signal_id(), outer_c.signal_id()]);
    }

    #[test]
    fn panic_drops_inner_frame_and_restores_outer_subscription() {
        let outer_state = State::new(1i32);
        let inner_state = State::new(2i32);
        let outer_queue = ComposerSubscription::new();
        let inner_queue = ComposerSubscription::new();

        let mut outer = begin_compose_deps_with_queue(Arc::downgrade(&outer_queue));
        outer_state.get();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _inner = begin_compose_deps_with_queue(Arc::downgrade(&inner_queue));
            inner_state.get();
            panic!("discard inner dependency frame");
        }));
        assert!(result.is_err());

        outer_state.get();
        let deps = take_deps();
        assert_eq!(deps.len(), 2);
        assert!(deps.iter().all(|(signal, _)| signal.id() == outer_state.signal_id()));
        outer.commit();

        inner_state.set(3);
        assert!(inner_queue.is_empty(), "panic frame 的新订阅应回滚");
        outer_state.set(4);
        assert_eq!(outer_queue.drain(), vec![outer_state.signal_id()]);
    }

    /// A read inside a no-deps frame records nothing, and the outer frame keeps recording after.
    #[test]
    fn a_no_deps_frame_records_nothing_and_restores_the_outer_frame() {
        let outer_read = State::new(1i32);
        let inner_read = State::new(2i32);
        let mut outer = begin_tracking_frame();
        let _ = outer_read.get();
        {
            let mut inner = begin_no_deps_frame();
            let _ = inner_read.get();
            inner.commit();
        }
        let _ = outer_read.get();
        let deps = take_deps();
        outer.commit();

        let ids: Vec<_> = deps.iter().map(|(signal, _)| signal.id()).collect();
        // TWO reads of the outer state, one before the inner frame and one after: the second is what
        // proves the outer frame was restored, not just that the inner one stopped recording.
        assert_eq!(
            ids.iter().filter(|id| **id == outer_read.signal_id()).count(),
            2,
            "the outer frame keeps recording after the inner one closes: {ids:?}"
        );
        assert!(
            !ids.contains(&inner_read.signal_id()),
            "a read inside the no-deps frame records nothing: {ids:?}"
        );
    }

    /// A side effect's read does not become the outer composer's layout dependency.
    ///
    /// The drain runs after the compose transactions close. When that compose was started from a
    /// layout pass, committing the inner frame restores the host's Layout frame, so without the
    /// no-deps guard a read in the effect would subscribe the host queue and be filed under a slot
    /// key that does not belong to it.
    #[test]
    fn a_side_effect_read_does_not_become_a_layout_dependency() {
        let host = ComposerSubscription::new();
        let state = State::new(1i32);
        let mut composer = crate::runtime::composer::Composer::new();

        let mut frame = begin_layout_deps_with_queue(Arc::downgrade(&host));
        composer.compose(|ctx| {
            let s = state.clone();
            crate::effect::side_effect(ctx, move || {
                let _ = s.get();
            });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 100.0, 0.0, 100.0));
        frame.commit();

        state.set(2);
        assert!(
            host.drain().is_empty(),
            "the effect's read must not become a layout dependency of the host"
        );
    }

    /// A panicking side effect takes the rest of that pass's queue with it.
    ///
    /// The queue is taken before the drain, so the effects behind the panicking one are dropped
    /// unrun and the next pass does not retry them. Documented in docs/effects.md; this pins it.
    #[test]
    fn a_panicking_side_effect_drops_the_rest_of_its_queue() {
        let ran: Arc<std::sync::Mutex<Vec<&'static str>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut composer = crate::runtime::composer::Composer::new();
        let log = ran.clone();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let first = log.clone();
            let second = log.clone();
            composer.compose(move |ctx| {
                crate::effect::side_effect(ctx, move || {
                    first.lock().unwrap().push("first");
                    panic!("boom");
                });
                crate::effect::side_effect(ctx, move || {
                    second.lock().unwrap().push("second");
                });
            });
        }));

        assert!(outcome.is_err(), "the panic propagates out of compose");
        assert_eq!(
            *ran.lock().unwrap(),
            vec!["first"],
            "the effects queued behind the panicking one are dropped unrun"
        );
        composer.compose(|_ctx| {});
        assert_eq!(
            *ran.lock().unwrap(),
            vec!["first"],
            "and the next pass does not run them either — the queue was already taken"
        );
    }

    /// Each composer drains its own queue: an overlay does not run the page's effects.
    #[test]
    fn each_composer_drains_its_own_side_effects() {
        let ran: Arc<std::sync::Mutex<Vec<&'static str>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut page = crate::runtime::composer::Composer::new();
        let mut overlay = crate::runtime::composer::Composer::new();

        let log = ran.clone();
        page.compose(move |ctx| {
            let log = log.clone();
            crate::effect::side_effect(ctx, move || log.lock().unwrap().push("page"));
        });
        let log = ran.clone();
        overlay.compose(move |ctx| {
            let log = log.clone();
            crate::effect::side_effect(ctx, move || log.lock().unwrap().push("overlay"));
        });

        assert_eq!(
            *ran.lock().unwrap(),
            vec!["page", "overlay"],
            "each composer runs exactly the effects it queued, when it commits"
        );
    }

    /// `ctx.frame_clock()` is the composer's own window, not a global or a default.
    #[test]
    fn a_composer_reports_its_own_window_clock() {
        let mut composer = crate::runtime::composer::Composer::new();
        composer.set_window_id(0xABCD);
        let seen = Arc::new(std::sync::Mutex::new(0u64));
        let s = seen.clone();
        composer.compose(move |ctx| {
            *s.lock().unwrap() = ctx.frame_clock().window();
        });
        assert_eq!(*seen.lock().unwrap(), 0xABCD);
    }

    /// A batch applies eagerly and publishes once, while every write keeps its own version stamp.
    ///
    /// The version has to move per write, not per batch: it orders writes against the values they
    /// published, and a batch that bumped only at commit would let a concurrent reader sample an
    /// unchanged version across a changed pair.
    #[test]
    fn a_write_batch_applies_eagerly_and_publishes_once() {
        struct Counter(Arc<std::sync::atomic::AtomicUsize>);
        impl StateObserver for Counter {
            fn on_change(&self, _id: StateId) {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }

        let a = State::new(0i32);
        let seen = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let observer: Arc<dyn StateObserver> = Arc::new(Counter(seen.clone()));
        a.raw.inner.signal.subscribe_observer(&observer);
        let before_version = global_version();

        write_batch(|| {
            a.set(1);
            assert_eq!(a.peek(), 1, "the writer reads its own write inside the batch");
            a.set(2);
            a.set(3);
            assert_eq!(seen.load(Ordering::Relaxed), 0, "nothing is published while the batch is open");
        });

        assert_eq!(a.peek(), 3);
        assert_eq!(seen.load(Ordering::Relaxed), 1, "one notification for the whole batch");
        assert_eq!(
            global_version(),
            before_version + 3,
            "but three version stamps, one per write"
        );
    }

    /// The point of a batch: a reader woken by it sees the finished pair, not half of it.
    #[test]
    fn a_batch_makes_a_pair_visible_together() {
        struct WatchB {
            b: State<i32>,
            log: Arc<std::sync::Mutex<Vec<i32>>>,
        }
        impl StateObserver for WatchB {
            fn on_change(&self, _id: StateId) {
                self.log.lock().unwrap().push(self.b.peek());
            }
        }

        let a = State::new(0i32);
        let b = State::new(0i32);
        let log: Arc<std::sync::Mutex<Vec<i32>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
        let observer: Arc<dyn StateObserver> = Arc::new(WatchB { b: b.clone(), log: log.clone() });
        a.raw.inner.signal.subscribe_observer(&observer);

        // Without a batch the observer runs before the second write, so it sees the old b.
        a.set(1);
        assert_eq!(*log.lock().unwrap(), vec![0], "an unbatched reader sees half of the pair");

        log.lock().unwrap().clear();
        write_batch(|| {
            a.set(2);
            b.set(2);
        });
        assert_eq!(*log.lock().unwrap(), vec![2], "a batched one sees the whole pair");
    }

    /// Nested batches merge into the outermost one.
    #[test]
    fn a_nested_batch_commits_with_its_parent() {
        let a = State::new(0i32);
        let before = global_version();
        write_batch(|| {
            a.set(1);
            write_batch(|| a.set(2));
        });
        assert_eq!(a.peek(), 2);
        assert_eq!(
            global_version(),
            before + 2,
            "one PUBLISH for the nest, but each write keeps its own version stamp"
        );
    }

    /// A panic inside the batch still publishes what was written: the values are in, and leaving
    /// observers unwoken would be worse than a spurious wake.
    #[test]
    fn a_panicking_batch_still_publishes() {
        let a = State::new(0i32);
        let before = global_version();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            write_batch(|| {
                a.set(7);
                panic!("boom");
            });
        }));
        assert!(outcome.is_err(), "the panic propagates");
        assert_eq!(a.peek(), 7, "the write was applied before the panic");
        assert_eq!(global_version(), before + 1, "and it was published on the way out");
    }

    /// The torn-read counter is process-global, so the tests that assert a delta on it take turns.
    static PASS_COUNTER_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// A read that sees ANOTHER thread's write from mid-pass is counted as a torn read.
    #[test]
    fn a_foreign_mid_pass_write_is_counted_as_a_torn_read() {
        let _serial = PASS_COUNTER_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let state = State::new(0i32);
        let writer = state.clone();
        let before = torn_pass_reads();
        let (read_done, wait_read) = std::sync::mpsc::channel::<()>();
        let (written, wait_write) = std::sync::mpsc::channel::<()>();

        std::thread::scope(|scope| {
            let reader = state.clone();
            scope.spawn(move || {
                let mut composer = crate::runtime::composer::Composer::new();
                composer.compose(move |ctx| {
                    let _ = reader.get();
                    read_done.send(()).unwrap();
                    wait_write.recv().unwrap();
                    // The write landed while this pass was running, from another thread.
                    let _ = reader.get();
                    let _ = ctx;
                });
            });
            wait_read.recv().unwrap();
            writer.set(1);
            written.send(()).unwrap();
        });

        assert_eq!(
            torn_pass_reads(),
            before + 1,
            "the second read observed a mid-pass write from another thread"
        );
    }

    /// A write the pass made itself is the normal pattern, not a torn read.
    #[test]
    fn a_write_from_the_pass_itself_is_not_a_torn_read() {
        let _serial = PASS_COUNTER_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let state = State::new(0i32);
        let handle = state.clone();
        let before = torn_pass_reads();
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(move |ctx| {
            let _ = handle.get();
            handle.set(1);
            let _ = handle.get();
            let _ = ctx;
        });
        assert_eq!(torn_pass_reads(), before, "a component setting state while it builds is expected");
    }

    /// A nested pass on the same thread is part of the same composition, so its writes are not torn.
    #[test]
    fn a_nested_pass_write_is_not_a_torn_read() {
        let _serial = PASS_COUNTER_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let state = State::new(0i32);
        let handle = state.clone();
        let before = torn_pass_reads();
        let mut outer = crate::runtime::composer::Composer::new();
        let mut inner = crate::runtime::composer::Composer::new();
        outer.compose(move |_ctx| {
            let _ = handle.get();
            inner.compose(|_ctx| {
                handle.set(1);
            });
            let _ = handle.get();
        });
        assert_eq!(torn_pass_reads(), before, "same thread, same composition");
    }

    /// A batched write is stamped at the version it committed at, or the instrument goes blind to it.
    #[test]
    fn a_batched_write_stamps_the_version_it_committed_at() {
        let a = State::new(0i32);
        write_batch(|| {
            a.set(1);
            assert!(
                a.raw.inner.signal.last_write_version.load(Ordering::Relaxed) > 0,
                "a batched write is stamped as it happens, not at commit"
            );
        });
        let stamped = a.raw.inner.signal.last_write_version.load(Ordering::Relaxed);
        assert!(stamped > 0 && stamped <= global_version(), "and the stamp is a real version");
    }

    #[test]
    fn a_visual_write_bumps_the_paint_epoch() {
        let state = State::new(1.0f32);
        let visual = state.into_visual();
        let before = visual_epoch();
        visual.set(2.0);
        assert!(
            visual_epoch() > before,
            "a draw-layer write must invalidate the item exit-picture cache"
        );
    }
