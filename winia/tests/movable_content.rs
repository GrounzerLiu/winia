//! Production-key regression tests for movable content identity, updates and lifetime.
use std::sync::{Arc, atomic::{AtomicBool, AtomicUsize, Ordering}};
use winia::{Composer, MovableContent};
use winia::layout::{BoxLayout, Constraints};
use winia::prelude::*;

fn layout(composer: &mut Composer) {
    composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
}
fn host(ctx: &mut ComposeCtx) {
    ctx.start_container(0x7777, Modifier::new(), BoxLayout::new());
}
fn leaf(ctx: &mut ComposeCtx, width: f32) {
    let key = ctx.next_key();
    ctx.start_leaf(key, Modifier::new().size(width, 5.0));
    ctx.end_node();
}
fn leaves(composer: &Composer) -> Vec<(u64, f32)> {
    fn visit(nodes: &[winia::layout::node::LayoutNode], index: usize, out: &mut Vec<(u64, f32)>) {
        if nodes[index].children.is_empty() { out.push((nodes[index].id, nodes[index].measured_size.width)); }
        for &child in &nodes[index].children { visit(nodes, child, out); }
    }
    let mut out = Vec::new();
    if let Some(root) = composer.layout_root_idx() { visit(composer.arena_nodes(), root, &mut out); }
    out
}
fn widths(composer: &Composer) -> Vec<f32> { leaves(composer).into_iter().map(|(_, w)| w).collect() }

#[test]
fn nested_state_read_remains_subscribed_and_updates() {
    let value = State::new(10.0_f32);
    let mut composer = Composer::new();
    let frame = |composer: &mut Composer| {
        composer.compose(|ctx| {
            let source = value.clone();
            let handle = ctx.key("owner", |ctx| ctx.remember_movable_content(move |ctx| {
                ctx.key("body", |ctx| Column::new().build(ctx, |ctx| leaf(ctx, source.get())));
            }));
            ctx.key("host", |ctx| Column::new().build(ctx, |ctx| {
                ctx.key("ref", |ctx| handle.compose(ctx));
            }));
        });
        layout(composer);
    };
    frame(&mut composer);
    assert_eq!(widths(&composer), vec![10.0]);
    value.set(20.0);
    assert!(composer.has_pending_states(), "a live stored reader must keep its subscription");
    frame(&mut composer);
    assert_eq!(widths(&composer), vec![20.0], "invalidation must enter the current host and stored reader");
}

#[test]
fn fresh_captured_payload_reenters_nested_content() {
    let title = State::new(10.0_f32);
    let mut composer = Composer::new();
    let frame = |composer: &mut Composer| {
        composer.compose(|ctx| ctx.key("owner-scope", |ctx| {
            ctx.start_scope();
            let current = title.get();
            let source = ctx.key("source", |ctx| ctx.remember_backchannel(|| 10.0_f32));
            source.set(current);
            let handle = ctx.key("handle", |ctx| ctx.remember_movable_content(move |ctx| {
                let captured = source.get();
                ctx.key("body", |ctx| Column::new().build(ctx, |ctx| leaf(ctx, captured)));
            }));
            host(ctx);
            ctx.key("ref", |ctx| handle.compose(ctx));
            ctx.end_node();
            ctx.end_scope();
        }));
        layout(composer);
    };
    frame(&mut composer);
    title.set(20.0);
    assert!(composer.has_pending_states());
    frame(&mut composer);
    assert_eq!(widths(&composer), vec![20.0]);
}

#[test]
fn dropping_owner_and_all_handles_releases_store_resources() {
    let resource = Arc::new(());
    let weak = Arc::downgrade(&resource);
    let source = weak.clone();
    let mut composer = Composer::new();
    composer.compose(|ctx| ctx.key("owner", |ctx| {
        ctx.start_scope();
        let handle = ctx.remember_movable_content(move |ctx| ctx.key("body", |ctx| {
            let held = ctx.remember(|| source.upgrade().expect("initial resource"));
            let _ = held.peek();
            leaf(ctx, 10.0);
        }));
        host(ctx);
        ctx.key("ref", |ctx| handle.compose(ctx));
        ctx.end_node();
        ctx.end_scope();
    }));
    layout(&mut composer);
    drop(resource);
    assert!(weak.upgrade().is_some());
    composer.compose(|ctx| { host(ctx); ctx.end_node(); });
    layout(&mut composer);
    assert!(weak.upgrade().is_none(), "removed owners must not keep unreachable stores alive");
}

#[test]
fn uncomposed_trailing_child_disappears() {
    let show = Arc::new(AtomicBool::new(true));
    let mut composer = Composer::new();
    let frame = |composer: &mut Composer| {
        composer.compose(|ctx| {
            let show = show.clone();
            let handle = ctx.key("handle", |ctx| ctx.remember_movable_content(move |ctx| {
                ctx.key("first", |ctx| leaf(ctx, 10.0));
                if show.load(Ordering::SeqCst) { ctx.key("second", |ctx| leaf(ctx, 20.0)); }
            }));
            host(ctx);
            ctx.key("ref", |ctx| handle.compose(ctx));
            ctx.end_node();
        });
        layout(composer);
    };
    frame(&mut composer);
    assert_eq!(widths(&composer), vec![10.0, 20.0]);
    show.store(false, Ordering::SeqCst);
    frame(&mut composer);
    assert_eq!(widths(&composer), vec![10.0]);
    show.store(true, Ordering::SeqCst);
    frame(&mut composer);
    assert_eq!(widths(&composer), vec![10.0, 20.0]);
}

fn shared_payload(ctx: &mut ComposeCtx, inits: &Arc<AtomicUsize>) {
    ctx.key("shared-payload", |ctx| {
        let marker = ctx.remember(|| inits.fetch_add(1, Ordering::SeqCst));
        leaf(ctx, 10.0 + marker.peek() as f32);
    });
}
#[test]
fn distinct_handles_keep_state_and_nodes_after_reordering() {
    let inits = Arc::new(AtomicUsize::new(0));
    let mut composer = Composer::new();
    let frame = |composer: &mut Composer, reverse: bool| {
        composer.compose(|ctx| {
            let a = inits.clone();
            let first = ctx.key("owner-a", |ctx| ctx.remember_movable_content(move |ctx| shared_payload(ctx, &a)));
            let b = inits.clone();
            let second = ctx.key("owner-b", |ctx| ctx.remember_movable_content(move |ctx| shared_payload(ctx, &b)));
            host(ctx);
            if reverse {
                ctx.key("ref-b", |ctx| second.compose(ctx));
                ctx.key("ref-a", |ctx| first.compose(ctx));
            } else {
                ctx.key("ref-a", |ctx| first.compose(ctx));
                ctx.key("ref-b", |ctx| second.compose(ctx));
            }
            ctx.end_node();
        });
        layout(composer);
    };
    frame(&mut composer, false);
    let original = leaves(&composer);
    assert_eq!(widths(&composer), vec![10.0, 11.0]);
    frame(&mut composer, true);
    assert_eq!(inits.load(Ordering::SeqCst), 2);
    assert_eq!(leaves(&composer), vec![original[1], original[0]]);
}

#[test]
fn caller_explicit_keys_do_not_replace_payload_identity() {
    let inits = Arc::new(AtomicUsize::new(0));
    let mut composer = Composer::new();
    let frame = |composer: &mut Composer, site: &str| {
        composer.compose(|ctx| {
            let source = inits.clone();
            let handle = ctx.key("owner", |ctx| ctx.remember_movable_content(move |ctx| {
                let marker = keyed_stmt!({ ctx.remember(|| source.fetch_add(1, Ordering::SeqCst)) });
                keyed_stmt!({ leaf(ctx, 10.0 + marker.peek() as f32); });
            }));
            host(ctx);
            ctx.key(site, |ctx| handle.compose(ctx));
            ctx.end_node();
        });
        layout(composer);
    };
    frame(&mut composer, "site-a");
    let original = leaves(&composer);
    frame(&mut composer, "site-b");
    assert_eq!(inits.load(Ordering::SeqCst), 1);
    assert_eq!(leaves(&composer), original);
}

#[composable]
fn suite_with_captured_label(ctx: &mut ComposeCtx, input: &State<f32>) {
    use winia::components::navigation_suite::{NavigationSuiteScaffold, NavigationSuiteType};
    let captured_width = input.get();
    NavigationSuiteScaffold::new(
        |items| items.item(true, |ctx| leaf(ctx, 5.0), move |ctx| {
            Column::new().build(ctx, |ctx| leaf(ctx, captured_width));
        }, || {}),
        |ctx| { let key = ctx.next_key(); ctx.start_leaf(key, Modifier::new()); ctx.end_node(); },
    ).layout_type(NavigationSuiteType::ShortNavigationBarCompact).transition(false).build(ctx);
}
#[test]
fn navigation_suite_refreshes_current_captured_label() {
    let input = State::new(31.0_f32);
    let mut composer = Composer::new();
    let frame = |composer: &mut Composer| {
        composer.compose(|ctx| suite_with_captured_label(ctx, &input));
        layout(composer);
    };
    frame(&mut composer);
    assert!(widths(&composer).contains(&31.0));
    input.set(47.0);
    assert!(composer.has_pending_states());
    frame(&mut composer);
    assert!(widths(&composer).contains(&47.0));
    assert!(!widths(&composer).contains(&31.0));
}

#[test]
fn skipped_idle_host_keeps_payload_visible() {
    let mut composer = Composer::new();
    let frame = |composer: &mut Composer| {
        composer.compose(|ctx| {
            let handle = ctx.key("owner", |ctx| ctx.remember_movable_content(move |ctx| ctx.key("body", |ctx| leaf(ctx, 10.0))));
            ctx.key("host", |ctx| Column::new().build(ctx, |ctx| ctx.key("ref", |ctx| handle.compose(ctx))));
        });
        layout(composer);
    };
    frame(&mut composer);
    let original = leaves(&composer);
    frame(&mut composer);
    frame(&mut composer);
    assert_eq!(leaves(&composer), original);
}

#[test]
fn replacing_a_reference_with_an_ordinary_node_disposes_old_payload() {
    let mut composer = Composer::new();
    let resource = Arc::new(());
    let source = Arc::downgrade(&resource);
    composer.compose(|ctx| {
        let payload = ctx.key("owner", |ctx| ctx.remember_movable_content(move |ctx| {
            let _held = ctx.remember(|| source.upgrade().unwrap());
            leaf(ctx, 10.0);
        }));
        host(ctx);
        ctx.key("same-site", |ctx| payload.compose(ctx));
        ctx.end_node();
    });
    layout(&mut composer);
    let weak = Arc::downgrade(&resource);
    drop(resource);
    composer.compose(|ctx| {
        host(ctx);
        ctx.key("same-site", |ctx| leaf(ctx, 25.0));
        ctx.end_node();
    });
    layout(&mut composer);
    assert_eq!(widths(&composer), vec![25.0]);
    assert!(weak.upgrade().is_none());
}

#[test]
fn stored_previous_sibling_lookup_uses_the_payload_tree() {
    let mut composer = Composer::new();
    let observed = Arc::new(std::sync::Mutex::new(None));
    let expected = Arc::new(std::sync::Mutex::new(None));
    composer.compose(|ctx| {
        let observed = observed.clone();
        let expected = expected.clone();
        let payload = ctx.key("owner", |ctx| ctx.remember_movable_content(move |ctx| {
            Column::new().build(ctx, |ctx| {
                let key = ctx.next_key();
                ctx.start_leaf(key, Modifier::new().size(18.0, 5.0));
                ctx.end_node();
                *expected.lock().unwrap() = Some(key);
                *observed.lock().unwrap() = ctx.prev_sibling_slot_key();
            });
        }));
        host(ctx);
        ctx.key("invoke", |ctx| payload.compose(ctx));
        ctx.end_node();
    });
    layout(&mut composer);
    assert_eq!(*observed.lock().unwrap(), *expected.lock().unwrap());
    assert!(expected.lock().unwrap().is_some());
}

#[test]
fn catching_a_payload_panic_restores_the_callers_runtime() {
    let mut composer = Composer::new();
    for _ in 0..2 {
        composer.compose(|ctx| {
            let payload = ctx.key("owner", |ctx| ctx.remember_movable_content(|ctx| {
                ctx.key("body", |ctx| Column::new().build(ctx, |ctx| {
                    leaf(ctx, 10.0);
                    panic!("intentional payload failure");
                }));
            }));
            host(ctx);
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                ctx.key("invoke", |ctx| payload.compose(ctx));
            }));
            assert!(result.is_err());
            ctx.key("after", |ctx| leaf(ctx, 25.0));
            ctx.end_node();
        });
        layout(&mut composer);
        assert_eq!(widths(&composer), vec![25.0], "failed store products must not corrupt caller siblings");
    }
}

#[test]
fn explicit_duplicate_placement_is_rejected_without_corrupting_first() {
    let mut composer = Composer::new();
    composer.compose(|ctx| {
        let payload = ctx.key("owner", |ctx| ctx.remember_movable_content(|ctx| leaf(ctx, 10.0)));
        host(ctx);
        ctx.key("first", |ctx| payload.compose(ctx));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ctx.key("second", |ctx| payload.compose(ctx));
        }));
        assert!(result.is_err(), "the minimal API must never silently omit unsupported duplicates");
        ctx.key("after", |ctx| leaf(ctx, 25.0));
        ctx.end_node();
    });
    layout(&mut composer);
    assert_eq!(widths(&composer), vec![10.0, 25.0]);
}

#[test]
fn same_composition_retry_keeps_its_state_and_discards_failed_reads() {
    let first = State::new(10.0_f32);
    let committed = State::new(20.0_f32);
    let attempts = Arc::new(AtomicUsize::new(0));
    let inits = Arc::new(AtomicUsize::new(0));
    let mut composer = Composer::new();
    let frame = |composer: &mut Composer, retry: bool| {
        composer.compose(|ctx| {
            let first = first.clone();
            let committed = committed.clone();
            let attempts = attempts.clone();
            let inits = inits.clone();
            let payload = ctx.key("owner", |ctx| ctx.remember_movable_content(move |ctx| {
                let marker = ctx.remember(|| inits.fetch_add(1, Ordering::SeqCst));
                if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                    let _ = first.get();
                    ctx.changed(&123_u32);
                    panic!("first attempt fails after reading state");
                }
                leaf(ctx, committed.get() + marker.peek() as f32);
            }));
            host(ctx);
            if retry {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ctx.key("invoke", |ctx| payload.compose(ctx))));
                assert!(result.is_err());
            }
            ctx.key("invoke", |ctx| payload.compose(ctx));
            ctx.end_node();
        });
        layout(composer);
    };
    frame(&mut composer, true);
    let original = leaves(&composer);
    assert_eq!(widths(&composer), vec![21.0]);
    first.set(11.0);
    assert!(!composer.has_pending_states(), "failed attempt must not leave a live subscription");
    frame(&mut composer, false);
    assert_eq!(inits.load(Ordering::SeqCst), 2, "only failed initialization and first committed initialization run");
    assert_eq!(leaves(&composer), original);
    committed.set(30.0);
    assert!(composer.has_pending_states());
    frame(&mut composer, false);
    assert_eq!(widths(&composer), vec![31.0]);
}

#[test]
fn reference_to_container_replacement_survives_idle_skip() {
    let mut composer = Composer::new();
    composer.compose(|ctx| {
        let payload = ctx.key("owner", |ctx| ctx.remember_movable_content(|ctx| leaf(ctx, 10.0)));
        host(ctx);
        ctx.key("position", |ctx| payload.compose(ctx));
        ctx.end_node();
    });
    layout(&mut composer);
    for _ in 0..3 {
        composer.compose(|ctx| {
            host(ctx);
            ctx.key("position", |ctx| Column::new().modifier(Modifier::new().size(50.0, 20.0)).build(ctx, |ctx| {
                leaf(ctx, 25.0);
            }));
            ctx.end_node();
        });
        layout(&mut composer);
        let nodes = composer.arena_nodes();
        let root = composer.layout_root_idx().unwrap();
        let replacement = nodes[root].children[0];
        assert_eq!(nodes[replacement].measured_size.width, 50.0);
        assert_eq!(nodes[replacement].children.len(), 1);
        assert_eq!(widths(&composer), vec![25.0]);
    }
}

#[composable]
fn first_raw_component(ctx: &mut ComposeCtx) {
    Column::new().build(ctx, |ctx| {
        let marker = ctx.remember(|| 10.0_f32);
        leaf(ctx, marker.get());
    });
}

#[composable]
fn second_raw_component(ctx: &mut ComposeCtx) {
    Column::new().build(ctx, |ctx| {
        let marker = ctx.remember(|| 20.0_f32);
        leaf(ctx, marker.get());
    });
}

#[test]
fn different_components_in_raw_payload_keep_their_function_identity() {
    let show_first = Arc::new(AtomicBool::new(true));
    let mut composer = Composer::new();
    let frame = |composer: &mut Composer| {
        composer.compose(|ctx| {
            let show_first = show_first.clone();
            let payload = ctx.key("owner", |ctx| ctx.remember_movable_content(move |ctx| {
                if show_first.load(Ordering::SeqCst) { first_raw_component(ctx); }
                second_raw_component(ctx);
            }));
            host(ctx);
            ctx.key("invoke", |ctx| payload.compose(ctx));
            ctx.end_node();
        });
        layout(composer);
    };
    frame(&mut composer);
    let original = leaves(&composer);
    assert_eq!(widths(&composer), vec![10.0, 20.0]);
    show_first.store(false, Ordering::SeqCst);
    frame(&mut composer);
    assert_eq!(leaves(&composer), vec![original[1]]);
}

#[test]
fn placement_locals_come_from_current_host_without_reinitialization() {
    use winia::runtime::composition_local::CompositionLocal;
    static LOCAL_WIDTH: std::sync::LazyLock<CompositionLocal<f32>> =
        std::sync::LazyLock::new(|| CompositionLocal::new(|| 5.0));
    let inits = Arc::new(AtomicUsize::new(0));
    let mut composer = Composer::new();
    let frame = |composer: &mut Composer, supplied: f32, site: &str| {
        composer.compose(|ctx| {
            let source = inits.clone();
            let payload = LOCAL_WIDTH.provides(13.0, || ctx.key("owner", |ctx| {
                ctx.remember_movable_content(move |ctx| {
                    Column::new().build(ctx, |ctx| {
                        let marker = ctx.remember(|| source.fetch_add(1, Ordering::SeqCst));
                        assert_eq!(marker.peek(), 0);
                        leaf(ctx, LOCAL_WIDTH.current());
                    });
                })
            }));
            host(ctx);
            LOCAL_WIDTH.provides(supplied, || ctx.key(site, |ctx| payload.compose(ctx)));
            ctx.end_node();
        });
        layout(composer);
    };
    frame(&mut composer, 21.0, "first-host");
    let id = leaves(&composer)[0].0;
    frame(&mut composer, 31.0, "second-host");
    assert_eq!(leaves(&composer), vec![(id, 31.0)]);
    assert_eq!(inits.load(Ordering::SeqCst), 1);
}

#[test]
fn omitted_scope_reference_cannot_revive_under_a_later_skipped_host() {
    let mut composer = Composer::new();
    let frame = |composer: &mut Composer, mode: usize| {
        composer.compose(|ctx| {
            let payload = ctx.key("owner", |ctx| ctx.remember_movable_content(|ctx| leaf(ctx, 10.0)));
            host(ctx);
            ctx.key("old-host", |ctx| {
                ctx.changed(&(mode == 0));
                Column::new().build(ctx, |ctx| {
                    ctx.start_scope_keyed(0x9988);
                    if mode == 0 { ctx.key("old-ref", |ctx| payload.compose(ctx)); }
                    ctx.end_scope();
                });
            });
            if mode == 2 { ctx.key("new-host", |ctx| payload.compose(ctx)); }
            ctx.end_node();
        });
        layout(composer);
    };
    frame(&mut composer, 0);
    frame(&mut composer, 1);
    frame(&mut composer, 2);
    assert_eq!(widths(&composer).iter().filter(|&&width| width == 10.0).count(), 1);
}

#[test]
fn rejected_nested_invocation_does_not_shift_following_node_identity() {
    let attempt_nested = Arc::new(AtomicBool::new(true));
    let mut composer = Composer::new();
    let frame = |composer: &mut Composer| {
        composer.compose(|ctx| {
            let nested = ctx.key("nested-owner", |ctx| ctx.remember_movable_content(|ctx| leaf(ctx, 99.0)));
            let attempt_nested = attempt_nested.clone();
            let outer = ctx.key("outer-owner", |ctx| ctx.remember_movable_content(move |ctx| {
                leaf(ctx, 10.0);
                if attempt_nested.load(Ordering::SeqCst) {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| nested.compose(ctx)));
                    assert!(result.is_err());
                }
                leaf(ctx, 20.0);
            }));
            host(ctx);
            ctx.key("outer-ref", |ctx| outer.compose(ctx));
            ctx.end_node();
        });
        layout(composer);
    };
    frame(&mut composer);
    let original = leaves(&composer);
    attempt_nested.store(false, Ordering::SeqCst);
    frame(&mut composer);
    assert_eq!(leaves(&composer), original);
}

#[test]
fn repeated_raw_component_calls_have_independent_scopes_and_readers() {
    let value = State::new(10.0_f32);
    let inits = Arc::new(AtomicUsize::new(0));
    let mut composer = Composer::new();
    let frame = |composer: &mut Composer| {
        composer.compose(|ctx| {
            let value = value.clone();
            let inits = inits.clone();
            let payload = ctx.key("owner", |ctx| ctx.remember_movable_content(move |ctx| {
                repeated_raw_component(ctx, &value, &inits);
                repeated_raw_component(ctx, &value, &inits);
            }));
            ctx.key("host", |ctx| Column::new().build(ctx, |ctx| payload.compose(ctx)));
        });
        layout(composer);
    };
    frame(&mut composer);
    let original = leaves(&composer);
    assert_eq!(widths(&composer), vec![10.0, 11.0]);
    value.set(20.0);
    assert!(composer.has_pending_states());
    frame(&mut composer);
    assert_eq!(widths(&composer), vec![20.0, 21.0]);
    assert_eq!(leaves(&composer).iter().map(|(id, _)| id).collect::<Vec<_>>(), original.iter().map(|(id, _)| id).collect::<Vec<_>>());
    assert_eq!(inits.load(Ordering::SeqCst), 2);
}

#[composable]
fn repeated_raw_component(ctx: &mut ComposeCtx, value: &State<f32>, inits: &Arc<AtomicUsize>) {
    let marker = ctx.remember(|| inits.fetch_add(1, Ordering::SeqCst));
    let width = value.get() + marker.get() as f32;
    Column::new().build(ctx, |ctx| leaf(ctx, width));
}

#[composable]
fn compiler_only_parent(ctx: &mut ComposeCtx, show: &Arc<AtomicBool>, inits: &Arc<AtomicUsize>) {
    if show.load(Ordering::SeqCst) { compiler_only_child(ctx, inits); }
}

#[composable]
fn compiler_only_child(ctx: &mut ComposeCtx, inits: &Arc<AtomicUsize>) {
    let marker = ctx.remember(|| inits.fetch_add(1, Ordering::SeqCst));
    leaf(ctx, 10.0 + marker.peek() as f32);
}

#[test]
fn removing_child_from_compiler_only_scope_disposes_its_remembered_state() {
    let show = Arc::new(AtomicBool::new(true));
    let inits = Arc::new(AtomicUsize::new(0));
    let mut composer = Composer::new();
    let frame = |composer: &mut Composer| {
        composer.compose(|ctx| {
            let show = show.clone();
            let inits = inits.clone();
            let payload = ctx.key("owner", |ctx| ctx.remember_movable_content(move |ctx| {
                compiler_only_parent(ctx, &show, &inits);
            }));
            host(ctx);
            ctx.key("invoke", |ctx| payload.compose(ctx));
            ctx.end_node();
        });
        layout(composer);
    };
    frame(&mut composer);
    assert_eq!(widths(&composer), vec![10.0]);
    show.store(false, Ordering::SeqCst);
    frame(&mut composer);
    assert!(!widths(&composer).contains(&10.0));
    show.store(true, Ordering::SeqCst);
    frame(&mut composer);
    assert_eq!(inits.load(Ordering::SeqCst), 2);
    assert_eq!(widths(&composer), vec![11.0]);
}

#[test]
fn completed_absence_disposes_placement_even_when_handle_is_retained() {
    let inits = Arc::new(AtomicUsize::new(0));
    let removes = Arc::new(AtomicUsize::new(0));
    let mut composer = Composer::new();
    let saved = Arc::new(std::sync::Mutex::new(None::<MovableContent>));
    let frame = |composer: &mut Composer, shown: bool| {
        composer.compose(|ctx| {
            let source = inits.clone();
            let removes = removes.clone();
            let handle = ctx.key("owner", |ctx| ctx.remember_movable_content(move |ctx| {
                ctx.key("body", |ctx| {
                    let marker = ctx.remember(|| source.fetch_add(1, Ordering::SeqCst));
                    let key = ctx.next_key();
                    let removes = removes.clone();
                    ctx.start_leaf_with_remove(key, Modifier::new().size(10.0 + marker.peek() as f32, 5.0), Box::new(move || { removes.fetch_add(1, Ordering::SeqCst); }));
                    ctx.end_node();
                });
            }));
            *saved.lock().unwrap() = Some(handle.clone());
            host(ctx);
            if shown { ctx.key("ref", |ctx| handle.compose(ctx)); }
            ctx.end_node();
        });
        layout(composer);
    };
    frame(&mut composer, true);
    let original = leaves(&composer);
    frame(&mut composer, false);
    frame(&mut composer, false);
    assert_eq!(removes.load(Ordering::SeqCst), 1, "a completed absent composition disposes the placement");
    frame(&mut composer, true);
    assert_eq!(inits.load(Ordering::SeqCst), 2, "retaining a callable does not retain an absent composition");
    assert_ne!(leaves(&composer), original);
    *saved.lock().unwrap() = None;
    composer.compose(|ctx| { host(ctx); ctx.end_node(); });
    layout(&mut composer);
    assert_eq!(removes.load(Ordering::SeqCst), 2, "each disposed placement runs cleanup exactly once");
}
