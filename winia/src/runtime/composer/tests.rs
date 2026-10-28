use super::*;
use super::slot_table::Slot;

mod core_tests {
    use super::*;

    #[test]
    fn position_callbacks_add_the_overlay_origin() {
        // An overlay composer is painted under its layer origin; a callback inside it must still
        // report window coordinates, or an anchored menu measures itself against the wrong point.
        // The origin is part of the change gate: when a layer moves, the callback fires again with
        // the new window-space box even though the layer-local box did not change.
        let mut composer = Composer::new();
        let seen: std::sync::Arc<std::sync::Mutex<Vec<(f32, f32)>>> =
            std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let write = seen.clone();
        composer.compose(move |ctx| {
            let key = ctx.next_key();
            let modifier = Modifier::new()
                .size(50.0, 20.0)
                .on_globally_positioned(move |position| {
                    write.lock().unwrap().push((position.x, position.y));
                });
            ctx.start_leaf(key, modifier);
            ctx.end_node();
        });
        let constraints = crate::layout::Constraints::new(0.0, 100.0, 0.0, 100.0);
        composer.screen_origin = (0.0, 0.0);
        composer.layout(constraints);
        composer.screen_origin = (40.0, 25.0);
        composer.layout(constraints);
        assert_eq!(
            *seen.lock().unwrap(),
            vec![(0.0, 0.0), (40.0, 25.0)],
            "the origin change must re-fire with window coordinates"
        );
    }

    /// A group with NO declared parameters and NOTHING read inside its body Skips on the next frame.
    ///
    /// That combination is what froze the SearchBar's results list: the panel body received an
    /// already-built list (the query was read in the PARENT scope), so the body itself had no dependency
    /// that could re-run it, and no declared parameter either. Either one is enough — the container's shape
    /// needs no declaration precisely because it reads `progress` inside the body.
    ///
    /// This pins the "declares nothing and reads nothing" half; the SearchBar case states its fix in code.
    #[test]
    fn group_without_declared_params_reenters_or_skips() {
        let mut composer = Composer::new();
        let runs = std::cell::Cell::new(0);

        let mut frame = |composer: &mut Composer| {
            composer.compose(|ctx| {
                let key = ctx.next_key();
                let r = &runs;
                match ctx.start_restartable_group(key, Modifier::new(), BoxLayout::new()) {
                    GroupStatus::Skip => {}
                    GroupStatus::Enter => {
                        r.set(r.get() + 1);
                        let inner = ctx.next_key();
                        ctx.start_leaf(inner, Modifier::new());
                        ctx.end_node();
                    }
                }
                ctx.end_restartable_group();
            });
            composer.layout(Constraints::new(0.0, 100.0, 0.0, 100.0));
        };

        frame(&mut composer);
        let first = runs.get();
        frame(&mut composer);
        let second = runs.get();
        eprintln!(
            "[probe] group without declared params: runs after frame1={first}, after frame2={second}"
        );
        assert_eq!(first, 1, "frame 1 must Enter (no cache yet)");
        // Record the observed behaviour rather than assert a preference: this is what tells the SearchBar
        // whether it has to declare its inputs.
        assert_eq!(
            second, 1,
            "measured: the group SKIPPED on frame 2 with no declared parameters (runs stayed {second})"
        );
    }

    /// How far an invalidation spreads, pinned. Compose invalidates exactly the scopes that read the
    /// state, and a child re-runs only when its own parameters changed. winia cannot compare a captured
    /// value as a parameter (a `#[composable]` body reads a state and feeds the child through a closure,
    /// not through a declared parameter), so a read landing on a SCOPE re-enters that scope's DIRECT
    /// children — and stops there. It used to re-enter the whole subtree, which meant every descendant
    /// of a component that reads a per-frame animation value re-ran with it.
    ///
    /// Shape: a page container with a clean sibling and a nested component (its own scope reads
    /// `counter`) holding a middle group and, inside that, a grandchild group.
    #[test]
    fn a_scope_dependency_re_enters_one_level_and_no_deeper() {
        let mut composer = Composer::new();
        let counter = State::new(0i32);
        let sibling_runs = std::cell::Cell::new(0);
        let middle_runs = std::cell::Cell::new(0);
        let grandchild_runs = std::cell::Cell::new(0);
        let leaf_group = |ctx: &mut ComposeCtx, runs: &std::cell::Cell<u32>| {
            let key = ctx.next_key();
            match ctx.start_restartable_group(key, Modifier::new(), BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    runs.set(runs.get() + 1);
                    let leaf = ctx.next_key();
                    ctx.start_leaf(leaf, Modifier::new());
                    ctx.end_node();
                }
            }
            ctx.end_restartable_group();
        };
        let build = |ctx: &mut ComposeCtx| {
            let page_key = ctx.next_key();
            match ctx.start_restartable_group(page_key, Modifier::new(), BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    // (a) A clean sibling of the component: not on the invalidated path at all.
                    leaf_group(ctx, &sibling_runs);
                    // (b) The component, whose own scope reads the state.
                    let _inner = ctx.start_scope();
                    let _ = counter.get();
                    // (c) A direct child of the reading scope: re-entered, because a captured value
                    // cannot be compared here (this is the documented deviation).
                    let middle_key = ctx.next_key();
                    match ctx.start_restartable_group(middle_key, Modifier::new(), BoxLayout::new()) {
                        GroupStatus::Skip => {}
                        GroupStatus::Enter => {
                            middle_runs.set(middle_runs.get() + 1);
                            // (d) Its child: nothing invalidated it, so it must skip.
                            leaf_group(ctx, &grandchild_runs);
                        }
                    }
                    ctx.end_restartable_group();
                    ctx.end_scope();
                }
            }
            ctx.end_restartable_group();
        };
        let layout = |composer: &mut Composer| {
            composer.layout(Constraints::new(0.0, 100.0, 0.0, 100.0));
        };
        composer.compose(build);
        layout(&mut composer);
        assert_eq!((sibling_runs.get(), middle_runs.get(), grandchild_runs.get()), (1, 1, 1));
        composer.recompose(build);
        layout(&mut composer);
        assert_eq!(
            (sibling_runs.get(), middle_runs.get(), grandchild_runs.get()),
            (1, 1, 1),
            "an idle frame re-runs nothing"
        );
        counter.set(1);
        assert!(composer.recompose(build), "the write must request a recomposition");
        layout(&mut composer);
        assert_eq!(sibling_runs.get(), 1, "a sibling of the component is untouched");
        assert_eq!(
            middle_runs.get(),
            2,
            "a direct child of the reading scope re-enters — a captured value cannot be compared here"
        );
        assert_eq!(
            grandchild_runs.get(),
            1,
            "and the invalidation STOPS there: 2 here would mean the reader's whole subtree was marked \
             dirty (the old conservative rule)"
        );
    }

    #[test]
    fn test_composer_new() {
        let mut composer = Composer::new();
        let key = composer.next_group_key();
        let key2 = composer.next_group_key();
        // key = fnv(base, 序号)（mix_key 全 64 位混合）——同 base 相邻序号 key 不同
        assert_ne!(key, key2);
        // 序号混合应改变低 32 位（fnv 扩散）——不要求高 32 位相同（旧拼接语义）
        assert_ne!(key as u32, key2 as u32, "序号应扩散到低 32 位（mix_key 全 64 位混合）");
        // mix_key 本身：不同序号 → 不同 key（跨 base 也不碰撞丢熵）
        assert_ne!(mix_key(0x1234, 0), mix_key(0x1234, 1), "同 base 不同序号 key 不同");
        assert_ne!(mix_key(0x1234, 0), mix_key(0x1235, 0), "不同 base 同序号 key 不同");
    }

    /// Phase 4.1：ScaleFactorChanged → request_recomposition → build 重跑并读到
    /// 新 density（TextUnit::Px 组合期转换随 DPI 变化刷新，而非保留旧值）。
    #[test]
    fn test_request_recomposition_reruns_build_with_new_density() {
        let mut composer = Composer::new();
        // build 读 density（模拟 Text font_size Px 组合期转换）存入外部 State
        let seen = State::new(0.0f32);
        let build = |ctx: &mut ComposeCtx| {
            let d = crate::runtime::density::current_density();
            seen.set(d.density);
            let key = ctx.next_key();
            ctx.start_leaf(key, Modifier::new());
            ctx.end_node();
        };
        // 帧 1：density 2.0
        crate::runtime::density::with_density(crate::unit::Density::from_density(2.0), || {
            composer.compose(build);
        });
        assert_eq!(seen.get(), 2.0, "首帧 density 2.0");
        // 模拟 ScaleFactorChanged：request_recomposition + density 变为 1.0
        composer.request_recomposition(0);
        crate::runtime::density::with_density(crate::unit::Density::from_density(1.0), || {
            assert!(composer.recompose(build), "request_recomposition 应驱动重组");
        });
        assert_eq!(seen.get(), 1.0, "重组后 build 读到新 density 1.0");
        // 无 request 时 recompose 应跳过（needs_recomposition 已消费）
        crate::runtime::density::with_density(crate::unit::Density::from_density(1.0), || {
            assert!(!composer.recompose(build), "无变化时 recompose 应跳过");
        });
    }

    #[test]
    fn test_window_lifecycle_isolation_per_composer() {
        let composer_a = Composer::new();
        let composer_b = Composer::new();

        composer_a.lifecycle.set_pending_remove(7);
        assert_eq!(composer_a.pending_window_close_id(), Some(7));
        assert_eq!(composer_b.pending_window_close_id(), None, "window close request must stay scoped to the owning Composer");

        composer_b.lifecycle.set_pending_remove(9);
        composer_b.lifecycle.mark_rebuilt();
        assert_eq!(composer_b.pending_window_close_id(), None, "rebuilt window should not report pending close");
        assert_eq!(composer_a.pending_window_close_id(), Some(7));

        composer_a.reset_pending_window_remove();
        assert_eq!(composer_a.pending_window_close_id(), None);
    }

    #[test]
    fn test_remember_persistence() {
        let mut composer = Composer::new();

        // 首次组合
        composer.compose(|ctx| {
            let state: State<i32> = ctx.remember(|| 42);
            assert_eq!(state.get(), 42);
            state.set(100);
        });

        // 重组：同一个 remember 应返回同一个 State，值保持 100
        composer.recompose(|ctx| {
            let state: State<i32> = ctx.remember(|| 42);
            assert_eq!(state.get(), 100);
        });
    }

    #[test]
    fn test_remember_multiple() {
        let mut composer = Composer::new();

        composer.compose(|ctx| {
            let a: State<i32> = ctx.remember(|| 1);
            let b: State<String> = ctx.remember(|| "hello".to_string());
            a.set(10);
            b.set("world".to_string());
        });

        composer.recompose(|ctx| {
            let a: State<i32> = ctx.remember(|| 99);
            let b: State<String> = ctx.remember(|| "none".to_string());
            assert_eq!(a.get(), 10);
            assert_eq!(b.get(), "world");
        });
    }

    #[test]
    fn test_conditional_remember() {
        let mut composer = Composer::new();
        let show_extra = State::new(false);

        composer.compose(|ctx| {
            let always: State<i32> = ctx.remember(|| 1);
            if show_extra.get() {
                let extra: State<i32> = ctx.remember(|| 2);
                assert_eq!(extra.get(), 2);
            }
            assert_eq!(always.get(), 1);
        });
    }

use crate::layout::BoxLayout;

    /// Popup 锚点：当前作用域最后一个兄弟的 slot key（无兄弟 → None）
    #[test]
    fn test_prev_sibling_slot_key() {
        let mut composer = Composer::new();
        let mut root_key = 0u64;
        let mut leaf1_key = 0u64;
        let mut leaf2_key = 0u64;
        let mut observed: Vec<Option<u64>> = Vec::new();
        composer.compose(|ctx| {
            root_key = ctx.next_key();
            ctx.start_container(root_key, Modifier::new(), BoxLayout::new());
            leaf1_key = ctx.next_key();
            ctx.start_leaf(leaf1_key, Modifier::new());
            ctx.end_node();
            observed.push(ctx.prev_sibling_slot_key());
            leaf2_key = ctx.next_key();
            ctx.start_leaf(leaf2_key, Modifier::new());
            ctx.end_node();
            observed.push(ctx.prev_sibling_slot_key());
            ctx.end_node();
            observed.push(ctx.prev_sibling_slot_key());
        });
        assert_eq!(observed, vec![Some(leaf1_key), Some(leaf2_key), Some(root_key)]);

        // 重组帧：上一帧有 A/B/C 三个兄弟，本帧只重组合 A 后查询——
        // 必须返回 A（而非残留的 C）——复现 Popup 锚点错位到 Dialog 按钮的 bug
        composer.compose(|ctx| {
            for _ in 0..3 {
                let k = ctx.next_key();
                ctx.start_leaf(k, Modifier::new());
                ctx.end_node();
            }
        });
        let mut recomposed: Option<u64> = None;
        let mut a_key = 0u64;
        composer.compose(|ctx| {
            a_key = ctx.next_key();
            ctx.start_leaf(a_key, Modifier::new());
            ctx.end_node();
            recomposed = ctx.prev_sibling_slot_key();
        });
        assert_eq!(recomposed, Some(a_key), "重组帧应锚到本帧刚组合的兄弟 A，而非残留的旧兄弟");
    }

    /// 测试 restartable group 的 skip → replay 路径：
    /// 状态变化只影响某个 leaf slot，兄弟 slot 应被 clean skip 并正确 replay。
    #[test]
    fn test_restartable_skip_replay_tree_integrity() {
        let mut composer = Composer::new();

        // Frame 1: 建立初始树
        composer.compose(|ctx| {
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    // 子 1: state-dependent leaf
                    let text_key = ctx.next_key();
                    {
                        let _count: State<i32> = ctx.remember(|| 0);
                        ctx.start_leaf(text_key, Modifier::new());
                    }
                    ctx.end_node();

                    // 子 2: clean restartable group
                    let btn_key = ctx.next_key();
                    match ctx.start_restartable_group(btn_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                        GroupStatus::Skip => {}
                        GroupStatus::Enter => {
                            let content_key = ctx.next_key();
                            ctx.start_leaf(content_key, Modifier::new());
                            ctx.end_node();
                        }
                    }
                    ctx.end_restartable_group();
                }
            }
            ctx.end_restartable_group();
        });

        // 断言 Frame 1 树结构
        let root = composer.layout_root_idx().expect("root should exist");
        assert_eq!(composer.arena_nodes()[root].children.len(), 2, "root should have 2 children");
        assert_eq!(composer.arena_nodes()[composer.arena_nodes()[root].children[0]].children.len(), 0, "text should be leaf");
        assert_eq!(composer.arena_nodes()[composer.arena_nodes()[root].children[1]].children.len(), 1, "button should have 1 child (content)");

        // Frame 2: recompose（button 应被 skip/replay）
        composer.recompose(|ctx| {
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    let text_key = ctx.next_key();
                    {
                        let _count: State<i32> = ctx.remember(|| 999);
                        ctx.start_leaf(text_key, Modifier::new());
                    }
                    ctx.end_node();

                    let btn_key = ctx.next_key();
                    match ctx.start_restartable_group(btn_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                        GroupStatus::Skip => {}
                        GroupStatus::Enter => {
                            let content_key = ctx.next_key();
                            ctx.start_leaf(content_key, Modifier::new());
                            ctx.end_node();
                        }
                    }
                    ctx.end_restartable_group();
                }
            }
            ctx.end_restartable_group();
        });

        // 断言 Frame 2 树结构仍正确
        let root = composer.layout_root_idx().expect("root should exist");
        assert_eq!(composer.arena_nodes()[root].children.len(), 2, "after recompose: root should have 2 children, got {}", composer.arena_nodes()[root].children.len());
        assert_eq!(composer.arena_nodes()[composer.arena_nodes()[root].children[0]].children.len(), 0, "after recompose: text should still be leaf");
        assert_eq!(composer.arena_nodes()[composer.arena_nodes()[root].children[1]].children.len(), 1, "after recompose: button should still have 1 child");
    }

    /// 3 层嵌套 restartable group: Column → Column → Text，验证深层 replay 正确性
    #[test]
    fn test_deep_nested_skip_replay() {
        let mut composer = Composer::new();

        // Frame 1: compose 3-level tree
        composer.compose(|ctx| {
            let key1 = ctx.next_key();
            match ctx.start_restartable_group(key1, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    // Level 2: nested restartable group
                    let key2 = ctx.next_key();
                    match ctx.start_restartable_group(key2, Modifier::new(), crate::layout::BoxLayout::new()) {
                        GroupStatus::Skip => {}
                        GroupStatus::Enter => {
                            // Level 3: state-dependent leaf
                            let leaf_key = ctx.next_key();
                            {
                                let _count: State<i32> = ctx.remember(|| 0);
                                ctx.start_leaf(leaf_key, Modifier::new());
                            }
                            ctx.end_node();

                            // Sibling: clean leaf
                            let leaf2_key = ctx.next_key();
                            ctx.start_leaf(leaf2_key, Modifier::new());
                            ctx.end_node();
                        }
                    }
                    ctx.end_restartable_group();
                }
            }
            ctx.end_restartable_group();
        });

        // Verify Frame 1: root → [level2 → [leaf1, leaf2]]
        let root = composer.layout_root_idx().unwrap();
        assert_eq!(composer.arena_nodes()[root].children.len(), 1, "Frame1: root has 1 child");
        assert_eq!(composer.arena_nodes()[composer.arena_nodes()[root].children[0]].children.len(), 2, "Frame1: level2 has 2 children");

        // Frame 2: recompose (level2 and leaf2 should be clean → skip/replay)
        composer.recompose(|ctx| {
            let key1 = ctx.next_key();
            match ctx.start_restartable_group(key1, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    let key2 = ctx.next_key();
                    match ctx.start_restartable_group(key2, Modifier::new(), crate::layout::BoxLayout::new()) {
                        GroupStatus::Skip => {}
                        GroupStatus::Enter => {
                            let leaf_key = ctx.next_key();
                            {
                                let _count: State<i32> = ctx.remember(|| 999);
                                ctx.start_leaf(leaf_key, Modifier::new());
                            }
                            ctx.end_node();

                            let leaf2_key = ctx.next_key();
                            ctx.start_leaf(leaf2_key, Modifier::new());
                            ctx.end_node();
                        }
                    }
                    ctx.end_restartable_group();
                }
            }
            ctx.end_restartable_group();
        });

        // Verify Frame 2: structure should be identical
        let root = composer.layout_root_idx().unwrap();
        assert_eq!(composer.arena_nodes()[root].children.len(), 1, "Frame2: root has 1 child");
        assert_eq!(composer.arena_nodes()[composer.arena_nodes()[root].children[0]].children.len(), 2, "Frame2: level2 has 2 children (leaf1 + leaf2)");
    }

    /// 验证 compose 时 slot 计数功能正常（增量重组的前提）
    #[test]
    fn test_compose_counts_clean_and_dirty() {
        let mut composer = Composer::new();
        let count: State<i32> = State::new(0);

        // Frame 1: 初始 compose
        composer.compose(|ctx| {
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    let _ = count.get();
                    { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); }
                    ctx.end_node();
                }
            }
            ctx.end_restartable_group();
        });
        assert_eq!(composer.compose_dirty_count, 2, "initial: root + leaf both new");
        assert_eq!(composer.compose_clean_count, 0, "initial: no clean slots");

        // Frame 2: recompose
        composer.recompose(|ctx| {
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    let _ = count.get();
                    { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); }
                    ctx.end_node();
                }
            }
            ctx.end_restartable_group();
        });
        // Frame 2, with NO layout in between: the root's slot is Clean, its params are unchanged, and
        // the arena still holds a node for its key — so the content does not run, and the leaf's slot
        // is never started. The counts say exactly that: one slot visited, and it was clean.
        //
        // This is a deliberate behaviour change that came with reading the reuse index instead of the
        // frame cache in the Skip decision. The cache is filled by LAYOUT, so a compose with no layout
        // before it (this test's shape, and a fresh composer's second frame) always Entered; the index
        // is repaired at the start of every compose from the tree, so the container can Skip here.
        // Nothing about the resulting tree changes: the skip path re-attaches the node's children from
        // the index, which is what the count below checks.
        assert_eq!(composer.compose_clean_count, 1, "frame 2 visits the root slot only");
        assert_eq!(composer.compose_dirty_count, 0, "…and it is clean, so the content is skipped");
        let root = composer.layout_root_idx().expect("the tree survives the skip");
        assert_eq!(composer.arena_nodes()[root].children.len(), 1,
            "the skipped container's child was re-attached from the index");
    }
}

/// A policy that reads a state AFTER measuring its children — the "parent post-child read" whose slot
/// attribution the doc's open item is about.
#[derive(Debug)]
#[allow(dead_code)] // the tests below are its only users
struct PostChildReadPolicy {
    handle: State<f32>,
}

impl crate::layout::node::MeasurePolicy for PostChildReadPolicy {
    fn measure(
        &self,
        nodes: &mut Vec<crate::layout::node::LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: crate::layout::constraints::Constraints,
    ) -> (crate::unit::Size, Vec<crate::layout::node::Placement>) {
        let mut h = 0.0f32;
        for &c in children {
            let (s, _) = crate::layout::node::measure_node(nodes, policies, c, constraints);
            h += s.height;
        }
        // The read that must be attributed to THIS node, not to the last child measured.
        let extra = self.handle.get();
        (crate::unit::Size::new(extra, h), Vec::new())
    }

    fn place(
        &self,
        _nodes: &mut Vec<crate::layout::node::LayoutNode>,
        _children: &[usize],
        _placements: &[crate::layout::node::Placement],
    ) {
    }
}

/// A parent's State::get AFTER its children measured must register on the PARENT.
///
/// `measure_node_inner` arms `ACTIVE_SLOT_KEY` with a node's slot at its top, and each child's measure
/// re-arms it with the child's slot. Nothing re-arms the PARENT's slot after the children are done, so
/// a read in the parent's post-child work (a policy's own bookkeeping, a dynamic size resolved after
/// laying children out) is recorded against the LAST CHILD's slot. The consequence is not a crash: the
/// dependency lives on a slot the parent does not own, so it is dropped the moment that child goes away
/// (a lazy list recycling it, an `if` branch closing), and the parent's layout silently stops tracking
/// the state — the failure mode this file's other layout-dependency tests exist for.
#[test]
fn test_layout_dep_of_a_parent_post_child_read_lands_on_the_parent() {
    let mut composer = Composer::new();
    let constraints = crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0);
    let width = State::new(30.0f32);

    let handle = width.clone();
    composer.compose(|ctx| {
        let root_key = ctx.next_key();
        ctx.start_container(root_key, Modifier::new(), PostChildReadPolicy { handle });
        for _ in 0..2 {
            let leaf_key = ctx.next_key();
            ctx.start_leaf(leaf_key, Modifier::new().size(10.0, 10.0));
            ctx.end_node();
        }
        ctx.end_node();
    });
    composer.layout(constraints);

    let root = composer.layout_root_idx().unwrap();
    let parent_key = composer.arena_nodes()[root].slot_key;
    let last_child = *composer.arena_nodes()[root].children.last().unwrap();
    let last_child_key = composer.arena_nodes()[last_child].slot_key;
    let width_id = width.signal_id();
    let registered = composer
        .layout_deps
        .get(&width_id)
        .expect("the post-child read must register SOMETHING");

    assert!(
        registered.contains(&parent_key),
        "the parent's post-child read must be attributed to the parent (slot {parent_key:#x}); \
         registered instead on {registered:?} (the last child is {last_child_key:#x})"
    );
    assert!(
        !registered.contains(&last_child_key),
        "…and NOT to the last child it happened to measure (slot {last_child_key:#x})"
    );

    // The consequence, which is what the misattribution actually costs: the dependency must survive
    // the child it was wrongly attributed to. Here the children are removed entirely (the composer
    // re-composes the parent with none), and the state must still re-measure the parent.
    let handle = width.clone();
    composer.compose(|ctx| {
        let root_key = ctx.next_key();
        ctx.start_container(root_key, Modifier::new(), PostChildReadPolicy { handle });
        ctx.end_node();
    });
    composer.layout(constraints);
    assert!(
        composer.layout_deps.get(&width_id).is_some_and(|keys| keys.contains(&parent_key)),
        "the parent's dependency must survive its children going away — a dependency parked on a \
         child's slot is dropped with the child"
    );
}

/// The parameter buffer recycles its boxes instead of allocating one per declaration per frame.
///
/// A behavioural test cannot see this — reusing a box and allocating a fresh one produce identical
/// state, which is why the round that introduced the recycling needed a counter to prove it did
/// anything (`param_alloc`). The shape asserted here is what the win looks like: the FIRST frame
/// allocates one box per declaring group (that is the storage), and every frame after it allocates a
/// handful — the buffer from the previous frame's last group — because the rest circulate down the rows.
#[test]
fn test_parameter_boxes_are_recycled_across_frames() {
    let mut composer = Composer::new();
    let constraints = crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0);
    let rows = 40usize;

    let mut frame = |composer: &mut Composer, tick: i64| {
        composer.compose(move |ctx| {
            let root_key = ctx.next_key();
            ctx.start_container(root_key, Modifier::new(), crate::layout::BoxLayout::new());
            for row in 0..rows {
                let key = ctx.next_key();
                // One declared parameter per row, the shape a list has.
                let _ = ctx.changed(&(tick + row as i64));
                match ctx.start_restartable_group(key, Modifier::new(), crate::layout::BoxLayout::new()) {
                    GroupStatus::Skip => {}
                    GroupStatus::Enter => {
                        let leaf = ctx.next_key();
                        ctx.start_leaf(leaf, Modifier::new().size(10.0, 10.0));
                        ctx.end_node();
                    }
                }
                ctx.end_restartable_group();
            }
            ctx.end_node();
        });
        composer.layout(constraints);
    };

    param_alloc::reset();
    frame(&mut composer, 0);
    let first = param_alloc::count();
    assert!(
        first >= rows as u64,
        "the first frame allocates the boxes it stores (one per declaring group): got {first} for {rows} rows"
    );

    param_alloc::reset();
    frame(&mut composer, 1);
    let second = param_alloc::count();
    assert!(
        second <= 2,
        "the second frame must REUSE the boxes the first frame handed back — got {second} allocations for \
         {rows} rows (without recycling this is one per row)"
    );

    param_alloc::reset();
    frame(&mut composer, 2);
    let third = param_alloc::count();
    assert!(third <= 2, "and it keeps recycling: got {third} allocations on the third frame");
}

/// 阶段4 键修复验证：复用索引（slot_key → 节点）改用 slot_key 后，
/// 无变化帧的 clean group 应真正 Skip（键 miss 时恒 Enter）。
/// 帧1 组合 root+leaf（无 scope）→ 帧2 无状态变化 → root 应返回 Skip。
#[test]
fn test_is_skip_after_clean_frame() {
    let mut composer = Composer::new();
    let count: State<i32> = State::new(0);

    composer.compose(|ctx| {
        let root_key = ctx.next_key();
        match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                let _ = count.get();
                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); }
                ctx.end_node();
            }
        }
        ctx.end_restartable_group();
    });

    // layout 一次：填充复用索引（真实流程：compose → layout → 下帧 compose）
    composer.layout(crate::layout::constraints::Constraints::new(0.0, 100.0, 0.0, 100.0));

    // 帧2：无状态变化 → root slot Clean → 索引按 slot_key 命中 → Skip
    let mut skip_happened = false;
    composer.compose(|ctx| {
        let root_key = ctx.next_key();
        match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => skip_happened = true,
            GroupStatus::Enter => {
                let _ = count.get();
                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); }
                ctx.end_node();
            }
        }
        ctx.end_restartable_group();
    });
    assert!(skip_happened,
        "无变化帧的 clean group 应 Skip（复用索引按 slot_key 命中）——若 Enter 说明 is_skip 键 miss");
}

/// 回归：同帧二次 compose（recompose 循环）——首次物化后 drain 了 prev_node_by_key，
/// 第二次 materialize 必须保留现有树（守卫：prev 空 + root 有）——否则空 prev 重建
/// → Skip 恢复全失败 → 树塌缩（动画启动瞬间坐标错乱 bug）。
#[test]
fn test_same_frame_second_compose_retains_tree() {
    let mut composer = Composer::new();

    // 帧 1：初始 compose + layout（prev 构建）
    composer.compose(|ctx| {
        let root_key = ctx.next_key();
        match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); }
                ctx.end_node();
            }
        }
        ctx.end_restartable_group();
    });

    // 帧 2 同帧两次 compose（模拟 recompose 循环——中间无 layout）：
    // 第一次 compose 物化并 drain prev_node_by_key；第二次 materialize
    // 不再命中守卫（8548718 守卫已移除——守卫会错误阻断同帧二次 compose 的
    // 真实 Enter 内容，如 AnimatedContent 切换帧）——走防御降级重建：
    // Skip 无缓存 → 按 Enter 重建（dirty 重测）——不塌缩、内容正确
    composer.compose(|ctx| {
        let root_key = ctx.next_key();
        match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); }
                ctx.end_node();
            }
        }
        ctx.end_restartable_group();
    });
    composer.compose(|ctx| {
        let root_key = ctx.next_key();
        match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); }
                ctx.end_node();
            }
        }
        ctx.end_restartable_group();
    });

    let root2 = composer.layout_root_idx().expect("帧2 root");
    // 树完整（root 有效 + 内容保留——降级重建不塌缩）
    let nodes = composer.arena_nodes();
    assert!(nodes[root2].children.len() == 1, "root 仍有一个 leaf 子节点（未塌缩）");

    // 帧 3：prev 空（帧2 无 layout——collect 未跑）→ 降级重建（不塌缩、内容正确）
    let nodes2 = composer.arena_nodes().len();
    composer.compose(|ctx| {
        let root_key = ctx.next_key();
        match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); }
                ctx.end_node();
            }
        }
        ctx.end_restartable_group();
    });
    let nodes3 = composer.arena_nodes().len();
    let root3 = composer.layout_root_idx().expect("帧3 root");
    assert!(composer.arena_nodes()[root3].children.len() == 1, "帧3 重建后树完整");

    // 帧 4：帧3 已 collect → 恢复 Skip 复用（节点数不再增长）
    composer.compose(|ctx| {
        let root_key = ctx.next_key();
        match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); }
                ctx.end_node();
            }
        }
        ctx.end_restartable_group();
    });
    let nodes4 = composer.arena_nodes().len();
    assert_eq!(nodes3, nodes4, "帧4 应恢复 Skip 复用（节点数不变）");
}

/// 阶段4 键修复的**关键回归用例**：scope 层存在时（scope 是 slot 树中 group 的父，
/// LayoutNode 树无 scope 层——两棵树路径不一致），复用索引按 slot_key 仍应命中。
/// path 键实现下此场景 miss → 恒 Enter；slot_key 键修复后应 Skip。
#[test]
fn test_is_skip_with_scope_layer() {
    let mut composer = Composer::new();
    let count: State<i32> = State::new(0);

    composer.compose(|ctx| {
        ctx.start_scope();
        let root_key = ctx.next_key();
        match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                let _ = count.get();
                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); }
                ctx.end_node();
            }
        }
        ctx.end_restartable_group();
        ctx.end_scope();
    });

    // layout 一次：填充复用索引（真实流程：compose → layout → 下帧 compose）
    composer.layout(crate::layout::constraints::Constraints::new(0.0, 100.0, 0.0, 100.0));

    // 帧2：无状态变化 → group slot Clean → 索引按 slot_key 命中 → Skip（尽管 scope 层在 slot 树中）
    let mut skip_happened = false;
    composer.compose(|ctx| {
        ctx.start_scope();
        let root_key = ctx.next_key();
        match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => skip_happened = true,
            GroupStatus::Enter => {
                let _ = count.get();
                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); }
                ctx.end_node();
            }
        }
        ctx.end_restartable_group();
        ctx.end_scope();
    });
    assert!(skip_happened,
        "含 scope 层的 clean group 应 Skip（slot_key 键修复后两棵树路径错位不再导致 miss）——若 Enter 说明回归");
    // 自清洁：帧2 后 layout（复位依赖记录模式——take_deps 等价）
}

/// 阶段5：changed 参数比较机制——帧1 参数写入 slot.params，帧2 同参数比较返回 false（未变）。
/// 参数变化时返回 true（触发重建）。
#[test]
fn test_changed_param_comparison() {
    let mut composer = Composer::new();

    // 帧1：参数 "hello"（首次 → changed=true）
    let mut p1 = None;
    composer.compose(|ctx| {
        p1 = Some(ctx.changed(&"hello".to_string()));
        let k = ctx.next_key();
        ctx.start_leaf(k, Modifier::new());
        ctx.end_node();
    });
    assert_eq!(p1, Some(true), "首次 changed 应返回 true");

    // 帧2：同参数 → changed=false（未变）
    let mut p2 = None;
    composer.compose(|ctx| {
        p2 = Some(ctx.changed(&"hello".to_string()));
        let k = ctx.next_key();
        ctx.start_leaf(k, Modifier::new());
        ctx.end_node();
    });
    assert_eq!(p2, Some(false), "同参数 changed 应返回 false（未变）");

    // 帧3：参数变化 → changed=true
    let mut p3 = None;
    composer.compose(|ctx| {
        p3 = Some(ctx.changed(&"world".to_string()));
        let k = ctx.next_key();
        ctx.start_leaf(k, Modifier::new());
        ctx.end_node();
    });
    assert_eq!(p3, Some(true), "参数变化 changed 应返回 true");

    // 帧4：回到帧2 的参数（同位置 → 与上帧比较——上帧是 "world" → 变化 → true）
    let mut p4 = None;
    composer.compose(|ctx| {
        p4 = Some(ctx.changed(&"hello".to_string()));
        let k = ctx.next_key();
        ctx.start_leaf(k, Modifier::new());
        ctx.end_node();
    });
    assert_eq!(p4, Some(true), "与上帧比较（hello vs world）→ 变化 → true");
}

/// 阶段5：参数相等跳过集成测试——`#[composable]` 组件用 ctx.changed 声明参数，
/// 参数未变 + slot clean → group Skip；参数变化 → Enter。
#[test]
fn test_param_equal_skip_integration() {
    let mut composer = Composer::new();
    let mut title = "hello".to_string();
    let mut last_status = None;

    let compose_once = |composer: &mut Composer, title: &str, out: &mut Option<GroupStatus>| {
        composer.compose(|ctx| {
            // 组件（模拟 `#[composable]`）：参数声明（start group 前）
            let _changed = ctx.changed(&title.to_string());
            let key = ctx.next_key();
            let status = ctx.start_restartable_group(key, Modifier::new(), crate::layout::BoxLayout::new());
            *out = Some(status);
            if let GroupStatus::Enter = status {
                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); }
                ctx.end_node();
            }
            ctx.end_restartable_group();
        });
    };

    // 帧1：参数 "hello"（首次 → Enter）
    compose_once(&mut composer, &title, &mut last_status);
    assert_eq!(last_status, Some(GroupStatus::Enter), "首次应 Enter");
    // layout（填充复用索引）
    composer.layout(crate::layout::constraints::Constraints::new(0.0, 100.0, 0.0, 100.0));

    // 帧2：参数未变 → Skip（参数相等 + slot clean）
    compose_once(&mut composer, &title, &mut last_status);
    assert_eq!(last_status, Some(GroupStatus::Skip), "参数未变 + clean 应 Skip");
    composer.layout(crate::layout::constraints::Constraints::new(0.0, 100.0, 0.0, 100.0));

    // 帧3：参数变化 → Enter（即使 slot clean）
    title = "world".to_string();
    compose_once(&mut composer, &title, &mut last_status);
    assert_eq!(last_status, Some(GroupStatus::Enter), "参数变化应 Enter（绕过 clean-skip）");
}

/// 阶段5 边界：changed 调用次数变化（上帧 2 参，本帧 1 参）→ 保守 Enter（不等）。
#[test]
fn test_changed_count_change_enters() {
    let mut composer = Composer::new();
    let mut last_status = None;

    // 帧1：2 个参数
    composer.compose(|ctx| {
        ctx.changed(&"a".to_string());
        ctx.changed(&1u32);
        let k = ctx.next_key();
        let s = ctx.start_restartable_group(k, Modifier::new(), crate::layout::BoxLayout::new());
        last_status = Some(s);
        if let GroupStatus::Enter = s { ctx.end_node(); }
        ctx.end_restartable_group();
    });

    // 帧2：1 个参数（次数变化）→ 与上帧 2 参比较 → 数量不等 → Enter（保守）
    composer.compose(|ctx| {
        ctx.changed(&"a".to_string());
        let k = ctx.next_key();
        let s = ctx.start_restartable_group(k, Modifier::new(), crate::layout::BoxLayout::new());
        last_status = Some(s);
        if let GroupStatus::Enter = s { ctx.end_node(); }
        ctx.end_restartable_group();
    });
    assert_eq!(last_status, Some(GroupStatus::Enter),
        "changed 次数变化（2→1）应保守 Enter");

    // 帧3：恢复 2 参（与帧2 的 1 参比较 → 数量不等 → Enter）
    composer.compose(|ctx| {
        ctx.changed(&"a".to_string());
        ctx.changed(&1u32);
        let k = ctx.next_key();
        let s = ctx.start_restartable_group(k, Modifier::new(), crate::layout::BoxLayout::new());
        last_status = Some(s);
        if let GroupStatus::Enter = s { ctx.end_node(); }
        ctx.end_restartable_group();
    });
    assert_eq!(last_status, Some(GroupStatus::Enter), "次数恢复 1→2 应保守 Enter");
}

/// 阶段5 边界：组件结构切换（参数组件 → 无参数容器）→ 保守 Enter，不错 Skip。
#[test]
fn test_param_to_plain_switch_enters() {
    let mut composer = Composer::new();
    let mut last_status = None;

    // 帧1：声明参数的组件（changed 1 次）
    composer.compose(|ctx| {
        ctx.changed(&"x".to_string());
        let k = ctx.next_key();
        let s = ctx.start_restartable_group(k, Modifier::new(), crate::layout::BoxLayout::new());
        last_status = Some(s);
        if let GroupStatus::Enter = s { ctx.end_node(); }
        ctx.end_restartable_group();
    });

    // 帧2：同位置换成无参数容器（不调 changed）→ pending 空 vs 上帧 params 非空 → 不等 → Enter
    composer.compose(|ctx| {
        let k = ctx.next_key();
        let s = ctx.start_restartable_group(k, Modifier::new(), crate::layout::BoxLayout::new());
        last_status = Some(s);
        if let GroupStatus::Enter = s { ctx.end_node(); }
        ctx.end_restartable_group();
    });
    assert_eq!(last_status, Some(GroupStatus::Enter),
        "参数组件→无参数容器切换应保守 Enter");
}

/// 量化：无变化帧的重组粒度——clean（可跳过/复用缓存）vs dirty（需重建）占比。
/// 若 clean 占比高 → 增量重组已达成（对象复用收益边际）；dirty 高 → 重建是主开销。
#[test]
fn test_quantify_reuse_ratio() {
    let mut composer = Composer::new();
    let count: State<i32> = State::new(0);

    // 模拟 demo 结构：root Column(scope) → 7 节 × (标题 Text + 动画 box Column + Text)
    let build_tree = |composer: &mut Composer| {
        composer.compose(|ctx| {
            ctx.start_scope();
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    for _ in 0..7 {
                        { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); } // 标题
                        let g = ctx.next_key();
                        match ctx.start_restartable_group(g, Modifier::new(), crate::layout::BoxLayout::new()) {
                            GroupStatus::Skip => {}
                            GroupStatus::Enter => {
                                let _ = count.get();
                                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); }
                            }
                        }
                        ctx.end_restartable_group();
                    }
                }
            }
            ctx.end_restartable_group();
            ctx.end_scope();
        });
        };

    build_tree(&mut composer);
    let clean1 = composer.compose_clean_count;
    let dirty1 = composer.compose_dirty_count;
    eprintln!("[quant] 帧1: clean={} dirty={}", clean1, dirty1);

    // 帧2：无变化 → 应全 clean（Skip）
    build_tree(&mut composer);
    let clean2 = composer.compose_clean_count;
    let dirty2 = composer.compose_dirty_count;
    eprintln!("[quant] 帧2(无变化): clean={} dirty={}  → clean占比 {:.0}%",
        clean2, dirty2, if clean2+dirty2>0 { clean2*100/(clean2+dirty2) } else { 0 });
    assert!(clean2 >= dirty2, "无变化帧 clean 应 ≥ dirty（增量重组生效）");
}

/// 阶段D：节点复用生效验证——连续 compose+layout 多帧，arena 节点数应稳定
/// （复用 = 不新增 alloc；无复用 = 每帧全新建 → 持续增长）。
#[test]
fn test_arena_reuse_stabilizes() {
    let mut composer = Composer::new();
    let count: State<i32> = State::new(0);

    let build = |composer: &mut Composer| {
        composer.compose(|ctx| {
            ctx.start_scope();
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    for _ in 0..5 {
                        { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); }
                        let g = ctx.next_key();
                        match ctx.start_restartable_group(g, Modifier::new(), crate::layout::BoxLayout::new()) {
                            GroupStatus::Skip => {}
                            GroupStatus::Enter => {
                                let _ = count.get();
                                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); }
                            }
                        }
                        ctx.end_restartable_group();
                    }
                }
            }
            ctx.end_restartable_group();
            ctx.end_scope();
        });
        };

    build(&mut composer);
    let n1 = composer.arena.nodes.len();
    eprintln!("[reuse-check] frame1 nodes={}", n1);
    for _ in 0..10 {
        build(&mut composer);
    }
    eprintln!("[key-stable] 帧2 clean={} dirty={}", composer.compose_clean_count, composer.compose_dirty_count);
    let n2 = composer.arena.nodes.len();
    eprintln!("[reuse-check] frame11 nodes={}", n2);
    assert!(n2 <= n1 + 2,
        "节点复用应使 arena 稳定：frame1={} frame11={}（无复用会持续增长）", n1, n2);
}

/// 遗留问题验证：policy 池是否每帧增长（节点复用但 policy 每帧 alloc → 内存泄漏）
#[test]
fn test_policy_pool_growth() {
    let mut composer = Composer::new();
    let count: State<i32> = State::new(0);
    let build = |composer: &mut Composer| {
        composer.compose(|ctx| {
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    let _ = count.get();
                    { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); }
                }
            }
            ctx.end_restartable_group();
        });
        };
    build(&mut composer);
    let p1 = composer.arena.policies.len();
    for _ in 0..20 { build(&mut composer); }
    let p2 = composer.arena.policies.len();
    eprintln!("[policy-growth] frame1={} frame21={}", p1, p2);
    assert!(p2 <= p1 + 1, "policy 池应稳定（复用 policy）——frame1={} frame21={}", p1, p2);
}

/// 回归测试（review 发现的 Blocking bug）：复用节点必须按 slot_status 设 dirty——
/// State 变化只标记 slot，与 LayoutNode.dirty 无桥接；复用节点不设 dirty 会被
/// 常量折叠返回旧尺寸（动画/文本不更新）。本测试：帧2 set State → 节点应重测。
#[test]
fn test_reused_node_remeasures_on_state_change() {
    let mut composer = Composer::new();
    let count: State<f32> = State::new(50.0);
    let holder = std::cell::RefCell::new(None::<State<f32>>);

    let build = |composer: &mut Composer| {
        composer.compose(|ctx| {
            let s = ctx.remember(|| 0.0f32);
            *holder.borrow_mut() = Some(s.clone());
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    let _ = s.get();
                    // 尺寸由 State 驱动（动态 size——测量时求值 + 注册依赖）
                    let k = ctx.next_key();
                    ctx.start_leaf(k, Modifier::new().size(&s, 10.0));
                    ctx.end_node();
                }
            }
            ctx.end_restartable_group();
        });
        composer.layout(crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0));
    };

    build(&mut composer);
    let root_idx = composer.layout_root_idx().unwrap();
    let w1 = composer.arena_nodes()[root_idx].children[0];
    let size1 = composer.arena_nodes()[w1].measured_size.width;
    eprintln!("[remeasure] frame1 w={}", size1);

    // 帧2：State 变化（尺寸目标变）→ 节点复用 → 必须重测（dirty 桥接）
    let s = holder.borrow().clone().unwrap();
    s.set(300.0);
    build(&mut composer);
    let root_idx = composer.layout_root_idx().unwrap();
    let w1 = composer.arena_nodes()[root_idx].children[0];
    let size2 = composer.arena_nodes()[w1].measured_size.width;
    eprintln!("[remeasure] frame2 w={}", size2);
    assert!(size2 > size1 + 10.0,
        "State 变化后复用节点应重测：frame1 w={} frame2 w={}（冻结则 bug 复发）", size1, size2);
}

// ═══════════════════════════════════════════════════════════
// 两段式依赖测试（P2-1：布局期读动画值只重测不重组）
// ═══════════════════════════════════════════════════════════

#[test]
fn test_dependency_reverse_graph_rebuilds_from_forward_reads() {
    let mut composer = Composer::new();
    let first = StateId::new(1);
    let second = StateId::new(2);

    composer.compose_slot_reads.insert(10, HashSet::from([first, second]));
    composer.compose_slot_reads.insert(20, HashSet::from([first]));
    composer.rebuild_compose_reverse_deps();
    assert_eq!(composer.slot_deps.get(&first), Some(&HashSet::from([10, 20])));
    assert_eq!(composer.slot_deps.get(&second), Some(&HashSet::from([10])));

    composer.layout_slot_reads.insert(30, HashSet::from([second]));
    composer.layout_slot_reads.insert(40, HashSet::from([first, second]));
    composer.rebuild_layout_reverse_deps();
    assert_eq!(composer.layout_deps.get(&first), Some(&HashSet::from([40])));
    assert_eq!(composer.layout_deps.get(&second), Some(&HashSet::from([30, 40])));
    composer.debug_assert_dependency_graphs();
}

/// T1 记录分流：组合期 get() 进 slot_deps；布局期（measure 中 SizeDynamic）get() 进 layout_deps
#[test]
fn test_layout_dep_recording_split() {
    let mut composer = Composer::new();
    let holder = std::cell::RefCell::new(None::<State<f32>>);

    composer.compose(|ctx| {
        let s = ctx.remember(|| 0.0f32);
        *holder.borrow_mut() = Some(s.clone());
        let root_key = ctx.next_key();
        match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                let _ = s.get(); // 组合期读 → slot_deps
                let k = ctx.next_key();
                ctx.start_leaf(k, Modifier::new().size(&s, 10.0)); // 布局期读 → layout_deps
                ctx.end_node();
            }
        }
        ctx.end_restartable_group();
    });
    composer.layout(crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0));

    let s = holder.borrow().clone().unwrap();
    let sid = s.signal_id();
    assert!(composer.slot_deps.contains_key(&sid),
        "组合期 get() 应注册进 slot_deps");
    assert!(composer.layout_deps.contains_key(&sid),
        "布局期（SizeDynamic）get() 应注册进 layout_deps");
    // 同一 State 双通道（组合+布局）各自记录
    let keys_layout = composer.layout_deps.get(&sid).unwrap();
    assert_eq!(keys_layout.len(), 1, "layout_deps 应含叶子节点 key");
    let leaf_key = *keys_layout.iter().next().unwrap();
    assert_eq!(composer.layout_slot_reads.get(&leaf_key).unwrap(), &std::collections::HashSet::from([sid]));
}

/// A measured slot with no dynamic read must replace its previous forward reads.
#[test]
fn test_layout_slot_reads_remove_empty_remeasure() {
    let mut composer = Composer::new();
    let holder = std::cell::RefCell::new(None::<State<f32>>);
    let use_dynamic = std::cell::Cell::new(true);

    let build = |composer: &mut Composer| {
        composer.compose(|ctx| {
            let state = ctx.remember(|| 10.0f32);
            *holder.borrow_mut() = Some(state.clone());
            let key = ctx.next_key();
            ctx.start_leaf(key, if use_dynamic.get() {
                Modifier::new().size(&state, 10.0)
            } else {
                Modifier::new().size(20.0, 10.0)
            });
            ctx.end_node();
        });
        composer.layout(crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0));
    };

    build(&mut composer);
    let state_id = holder.borrow().as_ref().unwrap().signal_id();
    assert!(composer.layout_deps.contains_key(&state_id));
    assert!(!composer.layout_slot_reads.is_empty());

    use_dynamic.set(false);
    // The external mode switch itself is not reactive. Queue a layout-only
    // invalidation so the next layout pass really measures this slot.
    holder.borrow().as_ref().unwrap().as_raw().set_animating(11.0);
    build(&mut composer);
    assert!(!composer.layout_deps.contains_key(&state_id), "空读取重测应移除 reverse edge");
    assert!(composer.layout_slot_reads.values().all(|reads| !reads.contains(&state_id)));

    holder.borrow().as_ref().unwrap().as_raw().set_animating(300.0);
    assert!(!composer.has_pending_states(), "移除布局读取后 State 不应继续入队");
}

/// A cached sibling that is not remeasured must retain its forward layout reads.
#[test]
fn test_layout_slot_reads_preserve_untouched_cached_slot() {
    let mut composer = Composer::new();
    let first_holder = std::cell::RefCell::new(None::<State<f32>>);
    let second_holder = std::cell::RefCell::new(None::<State<f32>>);

    let build = |composer: &mut Composer| {
        composer.compose(|ctx| {
            let first = ctx.remember(|| 10.0f32);
            let second = ctx.remember(|| 20.0f32);
            *first_holder.borrow_mut() = Some(first.clone());
            *second_holder.borrow_mut() = Some(second.clone());
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    let first_key = ctx.next_key();
                    ctx.start_leaf(first_key, Modifier::new().width(&first).height(10.0));
                    ctx.end_node();
                    let second_key = ctx.next_key();
                    ctx.start_leaf(second_key, Modifier::new().width(&second).height(10.0));
                    ctx.end_node();
                }
            }
            ctx.end_restartable_group();
        });
        composer.layout(crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0));
    };

    build(&mut composer);
    let first_id = first_holder.borrow().as_ref().unwrap().signal_id();
    let second_id = second_holder.borrow().as_ref().unwrap().signal_id();
    let second_key = *composer.layout_deps.get(&second_id).unwrap().iter().next().unwrap();
    let second_reads = composer.layout_slot_reads.get(&second_key).unwrap().clone();

    // Only the first leaf is dirty. The second leaf hits the measure cache and
    // must keep its previous forward edge instead of being treated as empty.
    first_holder.borrow().as_ref().unwrap().as_raw().set_animating(300.0);
    build(&mut composer);

    assert_eq!(composer.layout_slot_reads.get(&second_key), Some(&second_reads));
    assert!(composer.layout_deps.get(&second_id).unwrap().contains(&second_key));
    assert!(composer.layout_deps.get(&first_id).is_some_and(|keys| !keys.is_empty()));
}

/// Removing a composed node must remove its layout forward and reverse edges.
#[test]
fn test_layout_slot_reads_remove_removed_slot() {
    let mut composer = Composer::new();
    let show_holder = std::cell::RefCell::new(None::<State<bool>>);
    let first_holder = std::cell::RefCell::new(None::<State<f32>>);
    let removed_holder = std::cell::RefCell::new(None::<State<f32>>);

    let build = |composer: &mut Composer| {
        composer.compose(|ctx| {
            let show = ctx.remember(|| true);
            let first = ctx.remember(|| 10.0f32);
            let removed = ctx.remember(|| 20.0f32);
            *show_holder.borrow_mut() = Some(show.clone());
            *first_holder.borrow_mut() = Some(first.clone());
            *removed_holder.borrow_mut() = Some(removed.clone());
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    if show.get() {
                        let first_key = ctx.next_key();
                        ctx.start_leaf(first_key, Modifier::new().width(&first).height(10.0));
                        ctx.end_node();
                        let removed_key = ctx.next_key();
                        ctx.start_leaf(removed_key, Modifier::new().width(&removed).height(10.0));
                        ctx.end_node();
                    }
                }
            }
            ctx.end_restartable_group();
        });
        composer.layout(crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0));
    };

    build(&mut composer);
    let removed_id = removed_holder.borrow().as_ref().unwrap().signal_id();
    let removed_key = *composer.layout_deps.get(&removed_id).unwrap().iter().next().unwrap();
    assert!(composer.layout_slot_reads.contains_key(&removed_key));

    show_holder.borrow().as_ref().unwrap().set(false);
    build(&mut composer);

    assert!(!composer.layout_slot_reads.contains_key(&removed_key));
    assert!(!composer.layout_deps.contains_key(&removed_id));
    removed_holder.borrow().as_ref().unwrap().as_raw().set_animating(300.0);
    assert!(!composer.has_pending_states(), "removed layout signal must be unsubscribed");
    assert!(composer.layout_deps.contains_key(&first_holder.borrow().as_ref().unwrap().signal_id()) == false,
        "the branch removes both dynamic leaves");
}

/// An empty composition clears the forward graph and all reverse subscriptions.
#[test]
fn test_layout_slot_reads_clear_when_root_removed() {
    let mut composer = Composer::new();
    let watched = State::new(10.0f32);

    composer.compose(|ctx| {
        let key = ctx.next_key();
        ctx.start_leaf(key, Modifier::new().width(&watched).height(10.0));
        ctx.end_node();
    });
    composer.layout(crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0));
    assert!(!composer.layout_slot_reads.is_empty());

    composer.compose(|_ctx| {});
    watched.as_raw().set_animating(200.0);
    assert!(composer.has_pending_states(), "empty root test must start with a queued layout notification");
    composer.layout(crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0));

    assert!(composer.layout_slot_reads.is_empty());
    assert!(composer.layout_deps.is_empty());
    assert!(composer.layout_signal_handles.is_empty());
    assert!(!composer.has_pending_states(), "empty root must consume stale layout notification");
}

/// A panic while measuring must roll back subscriptions learned by that layout frame.
#[test]
fn test_layout_dependency_panic_rolls_back_new_subscription() {
    use std::panic::AssertUnwindSafe;
    use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};

    let mut composer = Composer::new();
    let stable = State::new(10.0f32);
    let panic_state = State::new(20.0f32);
    let should_panic = Arc::new(AtomicBool::new(true));
    let constraints = crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0);

    composer.compose(|ctx| {
        let key = ctx.next_key();
        ctx.start_leaf(key, Modifier::new().width(&stable).height(10.0));
        ctx.end_node();
    });
    composer.layout(constraints);
    let stable_id = stable.signal_id();
    let stable_key = *composer.layout_deps.get(&stable_id).unwrap().iter().next().unwrap();

    let panic_signal = panic_state.clone();
    let panic_flag = should_panic.clone();
    composer.compose(|ctx| {
        let key = ctx.next_key();
        ctx.start_leaf(key, Modifier::new().width(move || {
            let _ = panic_signal.get();
            if panic_flag.load(AtomicOrdering::Relaxed) {
                panic!("layout dependency panic");
            }
            20.0
        }).height(10.0));
        ctx.end_node();
    });
    stable.as_raw().set_animating(11.0);
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| composer.layout(constraints)));
    assert!(result.is_err());

    assert!(composer.layout_deps.get(&stable_id).is_some_and(|keys| keys.contains(&stable_key)),
        "panic must preserve the last committed layout graph");
    assert!(!composer.layout_signal_handles.contains_key(&panic_state.signal_id()),
        "panic-only signal handle must not survive the failed layout");
    assert!(!composer.layout_deps.contains_key(&panic_state.signal_id()),
        "panic-only signal must not remain subscribed");
    assert!(composer.pending_states.pending_ids().contains(&stable_id),
        "the consumed layout invalidation must be retained for retry");
    panic_state.as_raw().set_animating(21.0);
    assert!(!composer.pending_states.pending_ids().contains(&panic_state.signal_id()),
        "rolled-back signal must not enqueue Composer");

    should_panic.store(false, AtomicOrdering::Relaxed);
    composer.layout(constraints);
    assert!(!composer.has_pending_states(), "retry should consume the retained invalidation");
}

/// A panic while measuring must leave the reuse index as it was.
///
/// `LayoutTransaction` MOVES `prev_node_by_key` into its snapshot before layout rebuilds it (it used to
/// deep-clone the maps, which cost 149 µs of an idle 800-row layout). The move is only correct because
/// a rollback puts it back — and nothing else in the suite would notice if it did not: the index is
/// read by the NEXT compose's Skip decision and by materialize, where a missing entry does not panic,
/// it silently fails the Skip and rebuilds the subtree. So this checks the index directly, and then
/// the observable consequence, that the next idle compose can still Skip.
#[test]
fn test_layout_panic_restores_reuse_index() {
    use std::panic::AssertUnwindSafe;
    use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};

    let mut composer = Composer::new();
    let constraints = crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0);
    let layout_state = State::new(20.0f32);
    let should_panic = Arc::new(AtomicBool::new(false));
    let mut leaf_key = 0u64;

    // The panicking closure is a MEASURE-time width, and it reads a State so the node has a layout
    // dependency — that is what `set_animating` below uses to force a re-measure without recomposing
    // (a leaf whose slot is clean and whose layout is not dirty keeps its cached measurement).
    let build = |composer: &mut Composer,
                 layout_state: &State<f32>,
                 should_panic: &Arc<AtomicBool>,
                 leaf_key: &mut u64| {
        let flag = should_panic.clone();
        let width_state = layout_state.clone();
        composer.compose(|ctx| {
            let key = ctx.next_key();
            match ctx.start_restartable_group(key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    let leaf = ctx.next_key();
                    *leaf_key = leaf;
                    ctx.start_leaf(leaf, Modifier::new().width(move || {
                        let value = width_state.get();
                        if flag.load(AtomicOrdering::Relaxed) {
                            panic!("measure panic");
                        }
                        value
                    }).height(10.0));
                    ctx.end_node();
                }
            }
            ctx.end_restartable_group();
        });
    };

    build(&mut composer, &layout_state, &should_panic, &mut leaf_key);
    composer.layout(constraints);
    assert!(composer.prev_node_by_key.contains_key(&leaf_key), "layout must index the leaf");

    // A DIRECT layout that panics, with no compose in between: compose drains `prev_node_by_key` as it
    // claims nodes, so this is the frame shape in which the index is non-empty when layout takes it.
    should_panic.store(true, AtomicOrdering::Relaxed);
    layout_state.as_raw().set_animating(21.0);
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| composer.layout(constraints)));
    assert!(result.is_err(), "the measure panic must reach the caller");

    // The index layout took is back: the failed frame must not have consumed the previous frame's one.
    assert!(composer.prev_node_by_key.contains_key(&leaf_key),
        "rollback must restore the node index the failed layout took");
    assert!(composer.arena.root.is_some(), "rollback must restore the tree");

    // And the restored cache is USABLE: the retry lays out, and the frame after it Skips the container.
    should_panic.store(false, AtomicOrdering::Relaxed);
    composer.layout(constraints);
    let mut skipped = false;
    composer.compose(|ctx| {
        let key = ctx.next_key();
        match ctx.start_restartable_group(key, Modifier::new(), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => skipped = true,
            GroupStatus::Enter => {}
        }
        ctx.end_restartable_group();
    });
    assert!(skipped, "the frame after a rolled-back layout must be able to Skip from the restored cache");
}

/// T2 布局失效传播：layout_dirty_keys 命中的节点 + 祖先链全部标 layout_dirty
#[test]
fn test_layout_dirty_propagates_to_ancestors() {
    let mut composer = Composer::new();
    let holder = std::cell::RefCell::new(None::<State<f32>>);

    composer.compose(|ctx| {
        let s = ctx.remember(|| 0.0f32);
        *holder.borrow_mut() = Some(s.clone());
        let root_key = ctx.next_key();
        match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                let k = ctx.next_key();
                ctx.start_leaf(k, Modifier::new().size(&s, 10.0));
                ctx.end_node();
            }
        }
        ctx.end_restartable_group();
    });
    composer.layout(crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0));

    let root_idx = composer.layout_root_idx().unwrap();
    let leaf_idx = composer.arena_nodes()[root_idx].children[0];
    let leaf_key = composer.arena_nodes()[leaf_idx].slot_key;

    // 模拟 pending 消费收集 → 直接测 apply_layout_dirty（裸函数：DFS + 祖先传播）
    composer.layout_dirty_keys.insert(leaf_key);
    let mut nodes = std::mem::take(&mut composer.arena.nodes);
    crate::layout::node::apply_layout_dirty(&mut nodes, root_idx, &composer.layout_dirty_keys);
    assert!(nodes[root_idx].layout_dirty, "祖先（root）应被传播标脏");
    assert!(nodes[leaf_idx].layout_dirty, "命中节点应标脏");
    composer.arena.nodes = nodes;
}

/// T3 动画尺寸只重测不重组：布局依赖 State 变化 → 组合不重跑（clean 计数）+ 尺寸更新
#[test]
fn test_layout_dep_remesures_without_recompose() {
    let mut composer = Composer::new();
    let holder = std::cell::RefCell::new(None::<State<f32>>);

    let build = |composer: &mut Composer| {
        composer.compose(|ctx| {
            let s = ctx.remember(|| 0.0f32);
            *holder.borrow_mut() = Some(s.clone());
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    let k = ctx.next_key();
                    ctx.start_leaf(k, Modifier::new().size(&s, 10.0));
                    ctx.end_node();
                }
            }
            ctx.end_restartable_group();
        });
        composer.layout(crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0));
    };

    build(&mut composer);
    let root_idx = composer.layout_root_idx().unwrap();
    let leaf_idx = composer.arena_nodes()[root_idx].children[0];
    let size1 = composer.arena_nodes()[leaf_idx].measured_size.width;

    // 布局动画值变化（Animating 写——动画推进语义）
    let s = holder.borrow().clone().unwrap();
    s.as_raw().set_animating(300.0);
    build(&mut composer);
    let root_idx = composer.layout_root_idx().unwrap();
    let leaf_idx = composer.arena_nodes()[root_idx].children[0];
    let size2 = composer.arena_nodes()[leaf_idx].measured_size.width;

    assert!(size2 > size1 + 10.0, "布局依赖 State 变化应重测：frame1 w={} frame2 w={}", size1, size2);
    // 关键断言：不重组——leaf slot 应为 Clean（layout_dirty 不触发组合级 dirty）
    assert_eq!(composer.compose_dirty_count, 0,
        "布局动画值变化不应触发组合级 dirty（只重测不重组）——dirty_count={}", composer.compose_dirty_count);
}

/// T4 常量折叠保留布局依赖：折叠帧不重新注册 → layout_deps 旧项保留；notify 后重测
#[test]
fn test_layout_dep_survives_const_fold() {
    let mut composer = Composer::new();
    let holder = std::cell::RefCell::new(None::<State<f32>>);

    let build = |composer: &mut Composer| {
        composer.compose(|ctx| {
            let s = ctx.remember(|| 0.0f32);
            *holder.borrow_mut() = Some(s.clone());
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    let k = ctx.next_key();
                    ctx.start_leaf(k, Modifier::new().size(&s, 10.0));
                    ctx.end_node();
                }
            }
            ctx.end_restartable_group();
        });
        composer.layout(crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0));
    };

    build(&mut composer);
    let sid = holder.borrow().clone().unwrap().signal_id();
    assert!(composer.layout_deps.contains_key(&sid), "帧1 应注册布局依赖");

    // 帧2：无 notify 的重复 build——compose 全 Skip、measure 常量折叠命中
    use crate::layout::node::MEASURE_COUNT;
    let m1 = MEASURE_COUNT.with(|c| c.get());
    build(&mut composer);
    let m2 = MEASURE_COUNT.with(|c| c.get());
    assert_eq!(m1, m2,
        "折叠帧不应重新 measure（m1={} m2={}——若重测则依赖续期而非折叠保留，T4 语义失效）", m1, m2);
    assert!(composer.layout_deps.contains_key(&sid),
        "常量折叠帧（未重新 measure）应保留旧布局依赖——丢失则布局动画冻结");
    let leaf_key = {
        let root_idx = composer.layout_root_idx().unwrap();
        let leaf_idx = composer.arena_nodes()[root_idx].children[0];
        composer.arena_nodes()[leaf_idx].slot_key
    };
    assert!(composer.layout_deps.get(&sid).unwrap().contains(&leaf_key),
        "旧依赖项（state→leaf key）应保留");

    // 帧3：notify → 重测并维持依赖
    let s = holder.borrow().clone().unwrap();
    s.as_raw().set_animating(123.0);
    build(&mut composer);
    let m3 = MEASURE_COUNT.with(|c| c.get());
    assert!(m3 > m2, "notify 后应重新 measure（布局失效生效）");
    assert!(composer.layout_deps.contains_key(&sid), "重测后依赖应续期");
}

/// 跨 Composer 订阅（端到端）：overlay 独立 Composer 组合期读主树 State →
/// 主树 set 后 overlay Composer 的 pending 队列收到失效 → recompose 读到新值。
/// 这是 Popup / 浮层内容响应主树变化的通路。
#[test]
fn test_overlay_composer_invalidated_by_main_tree_state() {
    let mut main = Composer::new();
    let holder = std::cell::RefCell::new(None::<crate::runtime::state::State<Vec<String>>>);
    main.compose(|ctx| {
        let items = ctx.remember(|| vec!["a".to_string()]);
        *holder.borrow_mut() = Some(items.clone());
        // Ownerless State only notifies Composer instances that actually read it.
        let _ = items.get();
        let k = ctx.next_key();
        ctx.start_leaf(k, Modifier::new());
        ctx.end_node();
    });
    let items = holder.borrow().clone().unwrap();

    // overlay：独立 Composer，content 读主树 State
    let mut ov = Composer::new();
    let seen = std::cell::RefCell::new(None::<usize>);
    {
        let seen_ref = &seen;
        ov.compose(|ctx| {
            *seen_ref.borrow_mut() = Some(items.get().len());
            let k = ctx.next_key();
            ctx.start_leaf(k, Modifier::new());
            ctx.end_node();
        });
    }
    assert_eq!(seen.borrow().as_ref(), Some(&1), "首帧读到初值");

    // 主树 set：两个队列都应入队（fan-out）
    items.set(vec!["x".into(), "y".into(), "z".into()]);
    assert!(main.has_pending_states());
    assert!(ov.has_pending_states(), "跨 Composer 读取应建立订阅——否则 overlay 永不更新");

    // overlay recompose：消费 pending、读到新值、依赖续期
    let ran = {
        let seen_ref = &seen;
        ov.recompose(|ctx| {
            *seen_ref.borrow_mut() = Some(items.get().len());
            let k = ctx.next_key();
            ctx.start_leaf(k, Modifier::new());
            ctx.end_node();
        })
    };
    assert!(ran, "overlay 应因订阅通知执行重组");
    assert_eq!(seen.borrow().as_ref(), Some(&3));
    assert!(!ov.has_pending_states(), "recompose 后 pending 清空");

    // 再次 set：订阅仍在（重组时 record_dep 续订）→ 持续响应
    items.set(Vec::new());
    assert!(ov.has_pending_states(), "重组后依赖续期——持续响应后续变化");
}

/// Compose dependency edges are replaced when an Entered scope stops reading a State.
#[test]
fn test_compose_read_removal_unsubscribes_stale_state() {
    let mut composer = Composer::new();
    let gate = State::new(true);
    let watched = State::new(0i32);
    let constraints = crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0);

    let build = |composer: &mut Composer| {
        composer.compose(|ctx| {
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    if gate.get() {
                        let _ = watched.get();
                    }
                    let leaf_key = ctx.next_key();
                    ctx.start_leaf(leaf_key, Modifier::new());
                    ctx.end_node();
                }
            }
            ctx.end_restartable_group();
        });
        composer.layout(constraints);
    };

    build(&mut composer);
    let watched_id = watched.signal_id();
    assert!(composer.slot_deps.contains_key(&watched_id));

    gate.set(false);
    build(&mut composer);
    assert!(!composer.slot_deps.contains_key(&watched_id), "旧读取边应被移除");

    watched.set(1);
    assert!(!composer.has_pending_states(), "移除读取后 State 不应再使 Composer 入队");
}

/// Composer drop removes its queue from every StateSignal it read.
#[test]
fn test_composer_drop_unsubscribes_state_signals() {
    let watched = State::new(0i32);
    let pending = {
        let mut composer = Composer::new();
        let pending = composer.pending_states.clone();
        composer.compose(|ctx| {
            let _ = watched.get();
            let key = ctx.next_key();
            ctx.start_leaf(key, Modifier::new());
            ctx.end_node();
        });
        pending
    };

    watched.set(1);
    assert!(pending.is_empty(), "Composer drop 后不应收到 State 通知");
}

#[test]
fn test_runtime_frame_restores_tls_after_panic_and_nested_drop() {
    let baseline_active = ACTIVE_SLOT_KEY.with(|slot| slot.get());
    let baseline_groups = GROUP_STACK.with(|groups| groups.borrow().clone());
    let baseline_stmts = STMT_STACK.with(|stmts| stmts.borrow().clone());

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _frame = begin_runtime_frame();
        assert_eq!(ACTIVE_SLOT_KEY.with(|slot| slot.get()), 0);
        assert!(GROUP_STACK.with(|groups| groups.borrow().is_empty()));
        assert!(STMT_STACK.with(|stmts| stmts.borrow().is_empty()));
        set_active_slot_key(99);
        GROUP_STACK.with(|groups| groups.borrow_mut().push(99));
        STMT_STACK.with(|stmts| stmts.borrow_mut().push((99, 99)));
        panic!("runtime frame rollback");
    }));
    assert!(result.is_err());
    assert_eq!(ACTIVE_SLOT_KEY.with(|slot| slot.get()), baseline_active);
    assert_eq!(GROUP_STACK.with(|groups| groups.borrow().clone()), baseline_groups);
    assert_eq!(STMT_STACK.with(|stmts| stmts.borrow().clone()), baseline_stmts);

    let outer = begin_runtime_frame();
    set_active_slot_key(21);
    GROUP_STACK.with(|groups| groups.borrow_mut().push(21));
    STMT_STACK.with(|stmts| stmts.borrow_mut().push((21, 21)));
    {
        let _inner = begin_runtime_frame();
        set_active_slot_key(22);
        GROUP_STACK.with(|groups| groups.borrow_mut().push(22));
        STMT_STACK.with(|stmts| stmts.borrow_mut().push((22, 22)));
    }
    assert_eq!(ACTIVE_SLOT_KEY.with(|slot| slot.get()), 21);
    assert_eq!(GROUP_STACK.with(|groups| groups.borrow().clone()), vec![21]);
    assert_eq!(STMT_STACK.with(|stmts| stmts.borrow().clone()), vec![(21, 21)]);
    drop(outer);
    assert_eq!(ACTIVE_SLOT_KEY.with(|slot| slot.get()), baseline_active);
    assert_eq!(GROUP_STACK.with(|groups| groups.borrow().clone()), baseline_groups);
    assert_eq!(STMT_STACK.with(|stmts| stmts.borrow().clone()), baseline_stmts);
}

#[test]
fn test_nested_composer_compose_preserves_outer_dependencies() {
    let outer_state = State::new(1i32);
    let inner_state = State::new(2i32);
    let mut outer = Composer::new();
    let mut inner = Composer::new();

    outer.compose(|_ctx| {
        assert_eq!(outer_state.get(), 1);
        inner.compose(|_ctx| {
            assert_eq!(inner_state.get(), 2);
        });
        assert_eq!(outer_state.get(), 1);
    });

    assert!(outer.slot_deps.contains_key(&outer_state.signal_id()));
    assert!(!outer.slot_deps.contains_key(&inner_state.signal_id()));
    assert!(inner.slot_deps.contains_key(&inner_state.signal_id()));
    assert!(!inner.slot_deps.contains_key(&outer_state.signal_id()));

    outer_state.set(3);
    assert!(outer.has_pending_states());
    assert!(!inner.has_pending_states());
    inner_state.set(4);
    assert!(inner.has_pending_states());
}

/// Layout-only invalidation can be consumed directly by layout() without compose().
#[test]
fn test_layout_only_pending_consumed_without_recompose() {
    let mut composer = Composer::new();
    let size = State::new(10.0f32);
    let constraints = crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0);

    composer.compose(|ctx| {
        let key = ctx.next_key();
        ctx.start_leaf(key, Modifier::new().size(&size, 10.0));
        ctx.end_node();
    });
    composer.layout(constraints);
    let root = composer.layout_root_idx().unwrap();
    let before = composer.arena_nodes()[root].measured_size.width;

    size.as_raw().set_animating(300.0);
    assert!(composer.has_pending_states());
    composer.layout(constraints);
    let root = composer.layout_root_idx().unwrap();
    let after = composer.arena_nodes()[root].measured_size.width;
    assert!(after > before + 10.0, "layout-only State 应直接触发重测：{} -> {}", before, after);
    assert!(!composer.has_pending_states());
}

/// A State read by both composition and layout must remain queued for compose
/// after layout-only classification marks its layout path dirty.
#[test]
fn test_mixed_compose_layout_pending_reaches_recompose() {
    let mut composer = Composer::new();
    let size = State::new(10.0f32);
    let constraints = crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0);

    composer.compose(|ctx| {
        assert_eq!(size.get(), 10.0);
        let key = ctx.next_key();
        ctx.start_leaf(key, Modifier::new().size(&size, 10.0));
        ctx.end_node();
    });
    composer.layout(constraints);
    let root = composer.layout_root_idx().unwrap();
    let before = composer.arena_nodes()[root].measured_size.width;

    size.as_raw().set_animating(300.0);
    assert!(composer.has_pending_states());
    assert!(composer.recompose(|ctx| {
        assert_eq!(size.get(), 300.0);
        let key = ctx.next_key();
        ctx.start_leaf(key, Modifier::new().size(&size, 10.0));
        ctx.end_node();
    }));
    assert!(!composer.has_pending_states(), "mixed invalidation should be consumed by recompose");

    composer.layout(constraints);
    let root = composer.layout_root_idx().unwrap();
    let after = composer.arena_nodes()[root].measured_size.width;
    assert!(after > before + 10.0, "mixed State should still drive layout: {before} -> {after}");
}

/// 回归测试（review 发现）：register_modifier_deps_recursive（scroll 等 modifier 内
/// State::get）必须在 take_deps 之前执行——否则依赖被静默丢弃、滚动不刷新。
#[test]
fn test_modifier_scroll_dep_registered() {
    let mut composer = Composer::new();
    let holder = std::cell::RefCell::new(None::<crate::modifier::ScrollState>);

    let build = |composer: &mut Composer| {
        composer.compose(|ctx| {
            let ss = ctx.remember(|| crate::modifier::ScrollState::new()).get();
            *holder.borrow_mut() = Some(ss.clone());
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    let k = ctx.next_key();
                    ctx.start_leaf(k, Modifier::new().vertical_scroll(ss.clone()));
                    ctx.end_node();
                }
            }
            ctx.end_restartable_group();
        });
        composer.layout(crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0));
    };

    build(&mut composer);
    let sid = holder.borrow().as_ref().unwrap().offset.signal_id();
    assert!(composer.slot_deps.contains_key(&sid),
        "scroll offset 应注册组合依赖（register_modifier_deps_recursive）——丢失则滚动不刷新");

    // offset 变化 → 下帧该 slot 组合级 dirty（滚动触发重组）
    holder.borrow().as_ref().unwrap().offset.set(50.0);
    build(&mut composer);
    assert!(composer.compose_dirty_count > 0,
        "scroll 变化应触发组合级 dirty（dirty_count={}）", composer.compose_dirty_count);
}

/// A panic from late node cleanup must restore the last committed dependency graph.
/// This is narrower than a SlotTable transaction: only dependency maps and signal
/// subscriptions are rolled back here.
#[test]
fn test_compose_late_cleanup_panic_restores_dependency_graph() {
    let mut composer = Composer::new();
    let committed = State::new(1i32);
    let failed = State::new(2i32);
    let committed_for_remove = committed.clone();

    composer.compose(|ctx| {
        let _ = committed.get();
        ctx.start_leaf_with_remove(
            1,
            Modifier::new(),
            Box::new(move || {
                committed_for_remove.as_raw().set_animating(3);
                panic!("late compose cleanup panic");
            }),
        );
        ctx.end_node();
    });
    composer.layout(crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0));
    assert!(composer.slot_deps.contains_key(&committed.signal_id()));

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        composer.compose(|ctx| {
            let _ = failed.get();
            ctx.start_leaf(2, Modifier::new());
            ctx.end_node();
        });
    }));
    assert!(result.is_err(), "late cleanup should panic");
    assert!(composer.slot_deps.contains_key(&committed.signal_id()),
        "panic must restore the committed compose dependency");
    assert!(!composer.slot_deps.contains_key(&failed.signal_id()),
        "failed compose dependency must not remain committed");
    assert!(!composer.has_pending_states(),
        "failed-frame notifications must be removed during dependency rollback");

    committed.as_raw().set_animating(4);
    assert!(composer.has_pending_states(), "restored signal must remain subscribed");
    composer.pending_states.drain();
    failed.as_raw().set_animating(4);
    assert!(!composer.has_pending_states(), "failed signal must be unsubscribed after rollback");
}

/// 崩溃边界（P3-3）前提验证：content panic 后（catch_unwind 捕获），
/// 下帧恢复正常内容应自愈——slot 表/依赖缓冲（DEP_MODE 残留由 begin 清空）
/// 从半状态重建，不残留垃圾。
#[test]
fn test_compose_runtime_snapshot_restores_key_context_after_panic() {
    let mut composer = Composer::new();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        composer.compose(|ctx| {
            ctx.key("failed-key", |ctx| {
                let _ = ctx.next_key();
                panic!("runtime snapshot panic");
            });
        });
    }));
    assert!(result.is_err());
    assert!(composer.scope_source_stack.is_empty());
    assert!(composer.key_override_stack.is_empty());
    assert!(composer.slot_table.path.is_empty());
    assert!(composer.slot_table.child_counters.len() == 1);

    composer.compose(|ctx| {
        let key = ctx.key("recovered-key", |ctx| ctx.next_key());
        ctx.start_leaf(key, Modifier::new());
        ctx.end_node();
    });
    assert!(composer.layout_root_idx().is_some(), "next compose should rebuild after rollback");
}

#[test]
fn test_compose_panic_recovers_next_frame() {
    let mut composer = Composer::new();
    let should_panic = std::cell::Cell::new(true);

    let content = |ctx: &mut ComposeCtx| {
        if should_panic.get() {
            panic!("模拟用户 content panic");
        }
        let root_key = ctx.next_key();
        match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                let k = ctx.next_key();
                ctx.start_leaf(k, Modifier::new());
                ctx.end_node();
            }
        }
        ctx.end_restartable_group();
    };

    // 帧1：panic（上层 catch_unwind 捕获——此处直接验证 panic 确实发生）
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        composer.compose(content);
    }));
    assert!(r.is_err(), "帧1 应 panic（模拟渲染路径崩溃边界触发）");

    // 帧2：恢复正常内容 → 自愈
    should_panic.set(false);
    composer.compose(content);
    composer.layout(crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0));
    assert!(composer.layout_root_idx().is_some(), "panic 后下帧应自愈（树重建）");
    let root_idx = composer.layout_root_idx().unwrap();
    assert_eq!(composer.arena_nodes()[root_idx].children.len(), 1, "自愈后结构正确");
}


/// The batch boundary, clause 1: a write during a compose PASS is not acted on by that pass, and is
/// not lost either — the next pass re-enters the group that reads it.
///
/// The existing test above proves the notification stays queued. This one proves the other half, which
/// is the one a user can see: the queued notification still DRIVES a recomposition, on the next pass.
#[test]
fn test_a_write_during_content_recomposes_the_next_pass() {
    let mut composer = Composer::new();
    let state = State::new(0i32);
    let runs = std::rc::Rc::new(std::cell::Cell::new(0usize));
    let write_once = std::cell::Cell::new(true);

    let mut pass = |composer: &mut Composer, write: bool| {
        let runs = runs.clone();
        let state = state.clone();
        let write = write && write_once.replace(false);
        composer.compose(move |ctx| {
            let value = state.get();
            runs.set(runs.get() + 1);
            if write {
                // A write during content — a user callback, or a task completing on another thread.
                state.as_raw().set_animating(value + 1);
            }
            let key = ctx.next_key();
            ctx.start_leaf(key, Modifier::new());
            ctx.end_node();
        });
    };

    pass(&mut composer, true);
    assert_eq!(runs.get(), 1, "the writing pass runs the group once");
    assert!(
        composer.has_pending_states(),
        "a write during content must remain queued, not be consumed by the pass that made it"
    );

    // The next pass sees the queued notification and re-enters the group.
    let runs_next = runs.clone();
    let state_next = state.clone();
    assert!(composer.recompose(move |ctx| {
        let value = state_next.get();
        runs_next.set(runs_next.get() + 1);
        let key = ctx.next_key();
        ctx.start_leaf(key, Modifier::new());
        ctx.end_node();
        let _ = value;
    }));
    assert_eq!(runs.get(), 2, "the next pass re-enters the group — the notification was not lost");
    assert!(!composer.has_pending_states(), "and the next pass consumes it");
}

/// The batch boundary, clause 2: a write DURING MEASURE is not consumed by the layout pass that
/// produced it — the layout pass drains before it measures.
///
/// This is the clause that keeps a frame from acting on its own output: a measure callback that writes
/// a state (an animation tick, a scroll correction) must not extend the layout it is already inside.
///
/// The write has to be provoked by forcing a re-measure, and that is the whole difficulty of this test:
/// with unchanged constraints every node FOLDS (returns its cached size without running its closure), so
/// a first version of this test asserted on a frame where the callback never ran at all. The measured
/// value is asserted too — "something is pending" is not evidence that the write happened.
#[test]
fn test_a_write_during_measure_is_next_layout_batch() {
    let mut composer = Composer::new();
    let constraints = crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0);
    // A constraint that differs by one pixel invalidates every node's fold, so this frame really
    // measures (the same trick `layout_forced` uses in the bench).
    let forced = crate::layout::constraints::Constraints::new(0.0, 501.0, 0.0, 500.0);
    // `width_source` is read during measure (a layout dependency); `written_by_measure` is written by
    // that same measure callback, and has a layout dependency of its own through a second node.
    let width_source = State::new(20.0f32);
    let written_by_measure = State::new(0.0f32);
    // `AtomicBool` rather than `Cell`: a measure closure has to be `Send`.
    let write_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

    let width_flag = write_flag.clone();
    let written = written_by_measure.clone();
    let width_for_measure = width_source.clone();
    let measure_calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let calls_for_closure = measure_calls.clone();
    // Both leaves go inside ONE container: at the top level each desc overwrites `arena.root`, so a
    // second top-level node is materialized but never measured (its callbacks never run). A first
    // version of this test had the two leaves at top level and asserted on a closure that never ran.
    composer.compose(|ctx| {
        let root_key = ctx.next_key();
        ctx.start_container(root_key, Modifier::new(), crate::layout::BoxLayout::new());
        let key = ctx.next_key();
        let effect = written.clone();
        let flag = width_flag.clone();
        let calls = calls_for_closure.clone();
        ctx.start_leaf(key, Modifier::new().width(move || {
            calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let w = width_for_measure.get();
            if flag.load(std::sync::atomic::Ordering::Relaxed) {
                effect.as_raw().set_animating(w + 1.0);
            }
            w
        }).height(10.0));
        ctx.end_node();
        // A second node reads the written state during measure, giving it a layout dependency.
        let read_key = ctx.next_key();
        let source = written_by_measure.clone();
        ctx.start_leaf(read_key, Modifier::new().width(move || {
            source.get().max(1.0)
        }).height(10.0));
        ctx.end_node();
        ctx.end_node();
    });
    composer.layout(constraints);
    let written_id = written_by_measure.signal_id();
    assert!(
        composer.layout_deps.contains_key(&written_id),
        "precondition: the written state must have a layout dependency, or the tail drop would eat it"
    );

    // Frame 2: the write happens during this frame's measure.
    write_flag.store(true, std::sync::atomic::Ordering::Relaxed);
    let source_a = width_source.clone();
    let source_b = written_by_measure.clone();
    let calls_frame2 = measure_calls.clone();
    let write_flag_frame2 = write_flag.clone();
    let effect_frame2 = written_by_measure.clone();
    composer.compose(|ctx| {
        let root_key = ctx.next_key();
        ctx.start_container(root_key, Modifier::new(), crate::layout::BoxLayout::new());
        let key = ctx.next_key();
        let calls = calls_frame2.clone();
        let flag = write_flag_frame2.clone();
        let effect = effect_frame2.clone();
        ctx.start_leaf(key, Modifier::new().width(move || {
            calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let w = source_a.get();
            if flag.load(std::sync::atomic::Ordering::Relaxed) {
                effect.as_raw().set_animating(w + 1.0);
            }
            w
        }).height(10.0));
        ctx.end_node();
        let read_key = ctx.next_key();
        ctx.start_leaf(read_key, Modifier::new().width(move || {
            source_b.get().max(1.0)
        }).height(10.0));
        ctx.end_node();
        ctx.end_node();
    });
    composer.layout(forced);
    assert_eq!(
        measure_calls.load(std::sync::atomic::Ordering::Relaxed),
        2,
        "the measure closure must have run once per layout pass (a fold would skip it)"
    );
    assert_eq!(
        written_by_measure.get(),
        21.0,
        "precondition: the measure callback ran and wrote (without a forced re-measure everything \
         folds and the write never happens)"
    );
    assert!(
        composer.has_pending_states(),
        "a write during measure must remain queued for the next batch, not extend this layout"
    );
    assert_eq!(
        composer.pending_state_count(),
        1,
        "and it must be queued exactly once"
    );

    // And the next layout pass consumes it.
    write_flag.store(false, std::sync::atomic::Ordering::Relaxed);
    composer.layout(forced);
    assert!(!composer.has_pending_states(), "the next layout pass consumes the queued write");
}

/// The batch boundary, clause 3: it is per PHASE, not per frame — a LAYOUT-ONLY write made during
/// compose IS consumed by that same frame's layout, because layout has not measured yet.
///
/// This is deliberate (`consume_layout_pending` runs at the top of layout precisely so a layout-only
/// state does not need a whole extra frame), and it is the asymmetry that makes "only the next batch"
/// the wrong way to state the rule: what is guaranteed is that a phase never consumes what was
/// enqueued while it was running.
///
/// The write has to happen on a state that ALREADY has a subscriber, which is why this test runs a
/// quiet frame first: a notification reaches the queue only through a subscription, so a write to a
/// state nobody has read yet enqueues nothing at all (there is no one to notify — the value is simply
/// read fresh by the layout that follows). A first version of this test wrote on the very first frame
/// and asserted the write was queued; it was not, and not because anything consumed it.
#[test]
fn test_a_layout_only_write_during_compose_is_consumed_by_that_frames_layout() {
    let mut composer = Composer::new();
    let constraints = crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0);
    let width = State::new(20.0f32);

    // The state is read only during measure → a layout-only dependency.
    let build = |composer: &mut Composer, write: Option<f32>| {
        let w = width.clone();
        let write_target = width.clone();
        composer.compose(move |ctx| {
            let root_key = ctx.next_key();
            ctx.start_container(root_key, Modifier::new(), crate::layout::BoxLayout::new());
            let leaf_key = ctx.next_key();
            ctx.start_leaf(leaf_key, Modifier::new().width(move || w.get()).height(10.0));
            ctx.end_node();
            ctx.end_node();
            if let Some(value) = write {
                // A write from inside content, now that the state has a subscriber.
                write_target.set(value);
            }
        });
    };

    // Frame 1 (quiet): subscribe the composer to `width` through the measure that reads it.
    build(&mut composer, None);
    composer.layout(constraints);
    assert!(
        !composer.has_pending_states(),
        "precondition: a quiet frame leaves nothing queued"
    );

    // Frame 2: write it from content.
    build(&mut composer, Some(99.0));
    assert!(
        composer.has_pending_states(),
        "a layout-only write during compose is queued (compose does not consume it)"
    );
    let root = composer.layout_root_idx().unwrap();
    let leaf = composer.arena_nodes()[root].children[0];
    composer.layout(constraints);
    let measured = composer.arena_nodes()[leaf].measured_size.width;
    assert_eq!(
        measured, 99.0,
        "the same frame's layout consumed the layout-only write and measured the NEW value — \
         layout runs after compose, so there is nothing stale to protect"
    );
    assert!(!composer.has_pending_states(), "and the layout pass consumed it");
}

/// Two top-level nodes in one compose: only ONE of them becomes the tree's root.
///
/// `materialize_node` assigns `arena.root = Some(index)` for every desc whose parent is `None`, so a
/// second top-level desc silently REPLACES the first as the root — the first is materialized into the
/// arena and then never measured, never painted, and never collected by the layout maps. A real
/// application never hits this (the root comes from `app_root!`/`run_app!`, one container), but a
/// hand-written test or an overlay composer can, and the symptom is a node whose callbacks never run:
/// this was found by a boundary test whose measure closure never fired.
///
/// The behaviour is pinned rather than changed: making the root a synthetic container would move every
/// node's index and id, which the node-reuse, shared-element and semantics paths all index by. What
/// this test buys is that the next person meets the rule in a test name instead of in a mystery.
#[test]
fn test_only_one_top_level_node_becomes_the_root() {
    let mut composer = Composer::new();
    let constraints = crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0);
    composer.compose(|ctx| {
        let first = ctx.next_key();
        ctx.start_leaf(first, Modifier::new().size(10.0, 10.0));
        ctx.end_node();
        let second = ctx.next_key();
        ctx.start_leaf(second, Modifier::new().size(20.0, 20.0));
        ctx.end_node();
    });
    composer.layout(constraints);

    let root = composer.layout_root_idx().expect("a root");
    let nodes = composer.arena_nodes();
    assert_eq!(nodes.len(), 2, "both nodes were materialized");
    assert_eq!(
        nodes[root].measured_size.width,
        20.0,
        "the LAST top-level desc is the root (the second leaf here)"
    );
    assert_eq!(
        composer.prev_node_by_key.len(),
        1,
        "and the first leaf is not in the layout index either — it is unreachable from the root"
    );
}

/// A notification generated after compose drains its batch remains queued for the
/// next batch instead of being consumed by the current frame.
#[test]
fn test_compose_notification_during_frame_is_next_batch() {
    let mut composer = Composer::new();
    let state = State::new(0i32);
    let notify_once = std::cell::Cell::new(true);

    composer.compose(|_ctx| {
        let _ = state.get();
        if notify_once.replace(false) {
            state.as_raw().set_animating(1);
        }
    });

    assert!(composer.has_pending_states(), "in-frame notification must remain pending");
    assert!(composer.recompose(|_ctx| {
        let _ = state.get();
    }));
    assert!(!composer.has_pending_states(), "the next batch should consume the notification");
}

/// A State read at the very root of a composer's content — no scope, no node — must still
/// subscribe: the read registers against the root fallback key.
#[test]
fn test_root_level_read_subscribes_the_composer() {
    let mut composer = Composer::new();
    let state = State::new(0i32);
    composer.compose(|_ctx| {
        let _ = state.get();
    });
    state.set(1);
    assert!(composer.has_pending_states(), "a root read must leave the composer subscribed");
}

/// A panic after compose consumes its pending batch must restore that batch for
/// a retry; this guard does not attempt the larger SlotTable transaction.
#[test]
fn test_compose_pending_batch_restored_after_panic() {
    use std::panic::AssertUnwindSafe;

    let mut composer = Composer::new();
    let state = State::new(0i32);
    composer.compose(|_ctx| {
        let _ = state.get();
    });
    state.as_raw().set_animating(1);
    let state_id = state.signal_id();
    assert!(composer.pending_states.pending_ids().contains(&state_id));

    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        composer.compose(|_ctx| {
            let _ = state.get();
            panic!("compose batch panic");
        });
    }));
    assert!(result.is_err());
    assert!(composer.pending_states.pending_ids().contains(&state_id),
        "panic must restore the consumed compose batch");

    assert!(composer.recompose(|_ctx| {
        let _ = state.get();
    }));
    assert!(!composer.has_pending_states(), "retry should consume the restored batch");
}

/// 组合场景：drain 后帧内新到的通知与已消费批在 panic 恢复时同时保留——
/// 已消费的 ID 恢复（去重），新通知留在队列等下一批（不丢不重）。
#[test]
fn test_panic_restore_keeps_both_consumed_batch_and_in_frame_notification() {
    use std::panic::AssertUnwindSafe;

    let mut composer = Composer::new();
    let state_a = State::new(0i32);
    let state_b = State::new(0i32);
    // 建立依赖：state_a 和 state_b 都被 compose 读取
    composer.compose(|_ctx| {
        let _ = state_a.get();
        let _ = state_b.get();
    });

    // 进入下一帧前：激活两个 state
    state_a.as_raw().set_animating(1);
    state_b.as_raw().set_animating(1);
    let id_a = state_a.signal_id();
    let id_b = state_b.signal_id();
    let pending = composer.pending_states.pending_ids();
    assert!(pending.contains(&id_a) && pending.contains(&id_b));

    // 帧内：drain 消费 id_a（假设 id_a 先被消费），随后帧中新通知 id_b；
    // 帧尾 panic → Drop 恢复已消费的 id_a，同时 id_b 仍在队列
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        composer.compose(|_ctx| {
            let _ = state_a.get();
            let _ = state_b.get();
            // 模拟帧内新通知：在 drain 后、panic 前 set state_b
            // （真实帧内 set 由用户回调触发，这里直接模拟）
            if std::cell::Cell::new(true).take() {
                state_b.as_raw().set_animating(2);
            }
            panic!("compose panic after in-frame notification");
        });
    }));
    assert!(result.is_err());

    // 恢复后：id_a（已消费的批）被 restore，id_b（帧内新通知）保留——
    // 两者都在队列中（去重后各一次）
    let recovered = composer.pending_states.pending_ids();
    assert!(recovered.contains(&id_a), "consumed batch must be restored");
    assert!(recovered.contains(&id_b), "in-frame notification must remain queued");
    assert_eq!(
        recovered.iter().filter(|&&id| id == id_a).count(),
        1,
        "restore must not duplicate restored IDs"
    );
}

/// Helper: 深度优先检查 Slot 树上是否有残留 desc（panic 帧产物）。The test below is its only
/// caller, so the lib build cannot see the use.
#[allow(dead_code)]
fn slot_tree_has_residual_desc(slot: &Slot) -> bool {
    if slot.desc.is_some() {
        return true;
    }
    slot.children.iter().any(slot_tree_has_residual_desc)
}

/// A panic after a slot wrote its desc (Enter path) but before collect_desc_tree
/// must not let the residual desc leak into the next compose frame: reset() at the
/// start of compose discards unconsumed descs, so a Clean-reused slot cannot be
/// materialized from a stale desc.
#[test]
fn test_panic_residual_desc_cleared_by_next_compose_reset() {
    use std::panic::AssertUnwindSafe;

    let mut composer = Composer::new();

    // 帧1：start_restartable_group Enter 路径写入 desc 后 panic（模拟渲染路径崩溃边界）
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        composer.compose(|ctx| {
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    let k = ctx.next_key();
                    ctx.start_leaf(k, Modifier::new());
                    ctx.end_node();
                    panic!("panic after desc write");
                }
            }
            ctx.end_restartable_group();
        });
    }));
    assert!(result.is_err(), "帧1 应 panic");

    // panic 后：Enter 已写入的 desc 残留在 slot 树上（collect_desc_tree 未执行）
    assert!(
        slot_tree_has_residual_desc(&composer.slot_table.root_slot),
        "panic 后应存在残留 desc（本修复的触发条件）"
    );

    // 帧2：compose 开头 reset() 清空残留 desc → Clean 复用不再误收集
    composer.compose(|ctx| {
        let root_key = ctx.next_key();
        match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                let k = ctx.next_key();
                ctx.start_leaf(k, Modifier::new());
                ctx.end_node();
            }
        }
        ctx.end_restartable_group();
    });

    // 成功帧结束后所有 desc 被 collect take 或 reset 清空——树上不应再有残留
    assert!(
        !slot_tree_has_residual_desc(&composer.slot_table.root_slot),
        "成功帧后不应残留 desc"
    );
    assert!(composer.layout_root_idx().is_some(), "帧2 应成功物化");
}

/// A notification for a read removed during compose must not leave a stale
/// pending ID that keeps the Composer awake forever.
#[test]
fn test_removed_read_notification_during_compose_is_dropped() {
    let mut composer = Composer::new();
    let watched = State::new(0i32);
    let trigger = State::new(false);

    composer.compose(|_ctx| {
        let _ = watched.get();
        let _ = trigger.get();
    });

    trigger.as_raw().set_animating(true);
    composer.compose(|_ctx| {
        let _ = trigger.get();
        watched.as_raw().set_animating(1);
    });

    assert!(!composer.has_pending_states(), "removed read must not leave a stale pending ID");
}

/// 数据驱动的结构变化：State 变 → root Enter → 新增 leaf 生效。
/// （源码级结构变化在 Skip 语义下不触发——对标 Compose：结构变化必须由数据驱动）
#[test]
fn test_data_driven_structure_change() {
    let mut composer = Composer::new();
    let holder = std::cell::RefCell::new(None::<State<i32>>);

    let build = |composer: &mut Composer, two_leaves: bool| {
        composer.compose(|ctx| {
            let s = ctx.remember(|| 0i32);
            *holder.borrow_mut() = Some(s.clone());
            ctx.start_scope();
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    let _ = s.get();
                    { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); }
                    if two_leaves {
                        { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); }
                    }
                }
            }
            ctx.end_restartable_group();
            ctx.end_scope();
        });
        };

    // 帧1：1 leaf（Enter）
    build(&mut composer, false);
    let root = composer.layout_root_idx().unwrap();
    assert_eq!(composer.arena_nodes()[root].children.len(), 1, "帧1 应有 1 leaf");

    // 帧2：s 变化 → Enter → 2 leaf（数据驱动结构变化）
    let s = holder.borrow().clone().unwrap();
    s.set(1);
    build(&mut composer, true);
    let root = composer.layout_root_idx().unwrap();
    assert_eq!(composer.arena_nodes()[root].children.len(), 2,
        "数据驱动结构变化：s.set → Enter → 新增 leaf 应生效");
    eprintln!("[struct-change] ok: 1 leaf → 2 leaf（数据驱动）");

    // 帧3：s 再变 → 结构回退 1 leaf（未复用节点回收）
    let s = holder.borrow().clone().unwrap();
    s.set(2);
    build(&mut composer, false);
    let root = composer.layout_root_idx().unwrap();
    assert_eq!(composer.arena_nodes()[root].children.len(), 1,
        "结构回退：移除 leaf 应生效（未复用节点回收）");
}

/// 回归测试（23ad6a7）：Skip/Enter 交替时 key 稳定性——
/// 模拟 demo 场景：帧1 全 Enter → 帧2 部分 Skip（Row 的 content 不执行，
/// 其内 next_key 消失——全局 counter 会平移，每路径 counter 不漂移）→
/// 帧3 全 Enter——关键：帧3 的 scroll 内节点 key 与帧1 相同（复用命中）。
#[test]
fn test_key_stable_across_skip_enter() {
    let mut composer = Composer::new();
    let count_holder = std::cell::RefCell::new(None::<State<i32>>);
    let row_holder = std::cell::RefCell::new(None::<State<i32>>);

    // 模拟：root Column → [Row(Text+spacer+Button), scroll Column(2 节 × (标题+box))]
    // row_state 驱动 Row（帧2 不变 → Row Skip → 其内 3 个 next_key 消失——全局
    // counter 会平移 scroll 的 key；每路径 counter 不漂移）
    let build = |composer: &mut Composer| {
        composer.compose(|ctx| {
            let count = ctx.remember(|| 0i32);
            *count_holder.borrow_mut() = Some(count.clone());
            let row_state = ctx.remember(|| 0i32);
            *row_holder.borrow_mut() = Some(row_state.clone());
            ctx.start_scope();
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    let _ = count.get(); // root 依赖（root scope 内注册——帧2 count 变 → root Enter）
                    // Row（帧2 依赖 row_state 未变 → Clean → Skip——content 不执行）
                    let row_key = ctx.next_key();
                    match ctx.start_restartable_group(row_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                        GroupStatus::Skip => {}
                        GroupStatus::Enter => {
                            let _ = row_state.get(); // Row 依赖
                            { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); }
                            { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); }
                            { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); }
                        }
                    }
                    ctx.end_restartable_group();
                    // scroll Column（依赖 count——帧2 Enter：count 变 → scroll 重跑）
                    let scroll_key = ctx.next_key();
                    match ctx.start_restartable_group(scroll_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                        GroupStatus::Skip => {}
                        GroupStatus::Enter => {
                            let _ = count.get(); // scroll 依赖（帧2 count 变 → scroll Enter）
                            for _ in 0..2 {
                                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); } // 标题
                                let g = ctx.next_key();
                                match ctx.start_restartable_group(g, Modifier::new(), crate::layout::BoxLayout::new()) {
                                    GroupStatus::Skip => {}
                                    GroupStatus::Enter => { { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); } }
                                }
                                ctx.end_restartable_group();
                            }
                        }
                    }
                    ctx.end_restartable_group();
                }
            }
            ctx.end_restartable_group();
            ctx.end_scope();
        });
        };

    // 帧1：全 Enter（count + row_state 首次 → dirty）
    build(&mut composer);
    let n1 = composer.arena.nodes.len();
    eprintln!("[key-stable] 帧1 nodes={}", n1);

    // 帧2：count 变（root/scroll Enter）但 row_state 不变 → **Row Skip**（其内
    // 3 个 next_key 消失——旧全局 counter 会平移 scroll 的 key → 不复用 → arena 增长；
    // 每路径 counter 恒定 → scroll 节点 key 同帧1 → 全复用）
    let s = count_holder.borrow().clone().unwrap();
    s.set(1);
    build(&mut composer);
    eprintln!("[key-stable] 帧2 clean={} dirty={}", composer.compose_clean_count, composer.compose_dirty_count);
    let n2 = composer.arena.nodes.len();
    eprintln!("[key-stable] 帧2 nodes={}", n2);
    assert_eq!(n1, n2, "Row Skip 时 scroll 节点 key 应稳定（每路径 counter）——旧全局 counter 会平移 → arena 增长");
}

/// 回归测试（TextField 输入不显示 bug）：文本内容变化（依赖注册在父容器 →
/// leaf slot Clean）→ 复用节点必须重测（节点的 last_text 快照比对）——
/// 否则常量折叠 + cached_paragraph 旧内容 → 渲染画旧文本。
#[test]
fn test_text_content_change_remeasures() {
    let mut composer = Composer::new();
    let holder = std::cell::RefCell::new(None::<crate::runtime::state::State<String>>);

    // 模拟 TextField：外部 value State（依赖注册在容器 scope）+ TextContent leaf
    let build = |composer: &mut Composer, text: &str| {
        composer.compose(|ctx| {
            let value = ctx.remember(|| "".to_string());
            *holder.borrow_mut() = Some(value.clone());
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    let _ = value.get(); // 依赖注册到容器 scope（对标 TextField）
                    let k = ctx.next_key();
                    // TextContent leaf（content 来自 value——但内容在 build 时快照）
                    let modifier = Modifier::new().push(crate::modifier::ModifierElement::TextContent {
                        content: text.to_string(),
                        font_size: 14.0,
                        color: crate::graphics::Color::from_argb(255, 0, 0, 0),
                        font_weight: crate::text::FontWeight::NORMAL,
                        font_style: crate::text::FontSlant::Upright,
                        max_lines: usize::MAX,
                        align: crate::text::TextAlign::Left,
                        overflow: crate::text::TextOverflow::Clip,
                        soft_wrap: true,
                        letter_spacing: 0.0,
                        line_height: None,
                        min_lines: 1,
                        underline: false,
                        line_through: false,
                        font_family: None,
                        on_text_layout: None,
                        auto_size: None,
                    });
                    ctx.start_leaf(k, modifier);
                    ctx.end_node();
                }
            }
            ctx.end_restartable_group();
        });
        composer.layout(crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0));
    };

    build(&mut composer, "");
    let root = composer.layout_root_idx().unwrap();
    let leaf = composer.arena_nodes()[root].children[0];
    let w1 = composer.arena_nodes()[leaf].measured_size.width;
    eprintln!("[text-change] frame1 w={}", w1);

    // 帧2：value 变（容器 Enter）+ content 新（"hello"）→ leaf 复用但内容变 → 必须重测
    let s = holder.borrow().clone().unwrap();
    s.set("hello".to_string());
    build(&mut composer, "hello");
    let root = composer.layout_root_idx().unwrap();
    let leaf = composer.arena_nodes()[root].children[0];
    let w2 = composer.arena_nodes()[leaf].measured_size.width;
    eprintln!("[text-change] frame2 w={}", w2);
    assert!(w2 > w1, "文本内容变化后复用 leaf 应重测（宽度变）：frame1 w={} frame2 w={}（冻结则 bug 复发）", w1, w2);
}

/// A text STYLE change must re-measure too, not just a content change.
///
/// The comparison used to run against a node's whole cached `Modifier`, so every field the text carries
/// was compared; it now runs against that node's [`crate::layout::node::TextSnapshot`], which is cheaper
/// but only as faithful as the fields the snapshot copies. The one that bites hardest is the colour: an alpha
/// 0→1 fade changes nothing about the size, so a folded measurement would keep painting the cached
/// paragraph with its TRANSPARENT colour and the text would never appear — the reason the original
/// check compares colour at all. This test changes nothing but the colour, and requires the leaf to be
/// dirty after compose (i.e. before layout gets a chance to fold it).
#[test]
fn test_text_style_change_remeasures() {
    let mut composer = Composer::new();
    let holder = std::cell::RefCell::new(None::<crate::runtime::state::State<String>>);

    let build = |composer: &mut Composer, alpha: u8| {
        composer.compose(|ctx| {
            let value = ctx.remember(|| "".to_string());
            *holder.borrow_mut() = Some(value.clone());
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    let _ = value.get(); // 依赖注册到容器 scope（对标 TextField）
                    let k = ctx.next_key();
                    let modifier = Modifier::new().push(crate::modifier::ModifierElement::TextContent {
                        content: "hello".to_string(),
                        font_size: 14.0,
                        // The only thing that differs between the two frames.
                        color: crate::graphics::Color::from_argb(alpha, 0, 0, 0),
                        font_weight: crate::text::FontWeight::NORMAL,
                        font_style: crate::text::FontSlant::Upright,
                        max_lines: usize::MAX,
                        align: crate::text::TextAlign::Left,
                        overflow: crate::text::TextOverflow::Clip,
                        soft_wrap: true,
                        letter_spacing: 0.0,
                        line_height: None,
                        min_lines: 1,
                        underline: false,
                        line_through: false,
                        font_family: None,
                        on_text_layout: None,
                        auto_size: None,
                    });
                    ctx.start_leaf(k, modifier);
                    ctx.end_node();
                }
            }
            ctx.end_restartable_group();
        });
    };

    build(&mut composer, 0); // invisible
    composer.layout(crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0));
    let root = composer.layout_root_idx().unwrap();
    let leaf = composer.arena_nodes()[root].children[0];
    assert!(!composer.arena_nodes()[leaf].dirty, "precondition: frame 1 measured and folded");

    // 帧2：alpha 0 → 255（容器 Enter，leaf slot 仍 Clean）→ 必须判定为变化
    let s = holder.borrow().clone().unwrap();
    s.set("trigger".to_string());
    build(&mut composer, 255);
    let root = composer.layout_root_idx().unwrap();
    let leaf = composer.arena_nodes()[root].children[0];
    assert!(
        composer.arena_nodes()[leaf].dirty,
        "a colour change must re-measure the reused leaf (otherwise the paragraph keeps the old paint)"
    );
}

#[test]
fn test_stmt_key_stable_across_structure_change() {
    let mut composer = Composer::new();
    // 模拟 `#[composable]` 宏注入：帧 1 语句 5 内组合两个节点；帧 2 前插入一条语句
    // （运行时结构变化——但语句 id 是源码位置，不受影响）→ key 应稳定。
    let mut keys_frame1 = Vec::new();
    composer.compose(|ctx| {
        let _ = ctx.start_scope_keyed(0xABCD);
        ctx.push_stmt(5);
        let k1 = ctx.next_key();
        ctx.start_leaf(k1, Modifier::new());
        ctx.end_node();
        let k2 = ctx.next_key();
        ctx.start_leaf(k2, Modifier::new());
        ctx.end_node();
        ctx.pop_stmt();
        ctx.end_scope();
        keys_frame1.push((k1, k2));
    });
    let mut keys_frame2 = Vec::new();
    composer.compose(|ctx| {
        let _ = ctx.start_scope_keyed(0xABCD);
        ctx.push_stmt(3); // 模拟"前面插入的新语句"（源码里在语句 5 前新增）
        ctx.pop_stmt();
        ctx.push_stmt(5); // 原语句 5——id 不变
        let k1 = ctx.next_key();
        ctx.start_leaf(k1, Modifier::new());
        ctx.end_node();
        let k2 = ctx.next_key();
        ctx.start_leaf(k2, Modifier::new());
        ctx.end_node();
        ctx.pop_stmt();
        ctx.end_scope();
        keys_frame2.push((k1, k2));
    });
    assert_eq!(keys_frame1[0], keys_frame2[0], "同语句 id 跨帧 key 应稳定（结构变化不漂移）");
}

#[test]
fn test_stmt_key_differs_by_stmt_id() {
    let mut composer = Composer::new();
    let mut keys = Vec::new();
    composer.compose(|ctx| {
        let _ = ctx.start_scope_keyed(0xABCD);
        ctx.push_stmt(1);
        let k1 = ctx.next_key();
        ctx.start_leaf(k1, Modifier::new());
        ctx.end_node();
        ctx.pop_stmt();
        ctx.push_stmt(2);
        let k2 = ctx.next_key();
        ctx.start_leaf(k2, Modifier::new());
        ctx.end_node();
        ctx.pop_stmt();
        ctx.end_scope();
        keys.push((k1, k2));
    });
    assert_ne!(keys[0].0, keys[0].1, "不同语句 id → 不同 key");
}

#[test]
fn test_key_override_stable() {
    let mut composer = Composer::new();
    let mut keys = Vec::new();
    for _ in 0..2 {
        composer.compose(|ctx| {
            let _ = ctx.start_scope_keyed(0xABCD);
            let k = ctx.key("scroll_list", |ctx| {
                let k = ctx.next_key();
                ctx.start_leaf(k, Modifier::new());
                ctx.end_node();
                k
            });
            ctx.end_scope();
            keys.push(k);
        });
        }
    assert_eq!(keys[0], keys[1], "显式 key() 跨帧稳定");
}

#[test]
fn test_key_override_differs_from_stmt() {
    let mut composer = Composer::new();
    let mut keys = Vec::new();
    composer.compose(|ctx| {
        let _ = ctx.start_scope_keyed(0xABCD);
        ctx.push_stmt(1);
        let k1 = ctx.next_key();
        ctx.start_leaf(k1, Modifier::new());
        ctx.end_node();
        ctx.pop_stmt();
        let k2 = ctx.key("other", |ctx| {
            let k = ctx.next_key();
            ctx.start_leaf(k, Modifier::new());
            ctx.end_node();
            k
        });
        ctx.end_scope();
        keys.push((k1, k2));
    });
    assert_ne!(keys[0].0, keys[0].1, "显式 key() 与语句 key 不同空间");
}

#[test]
fn test_stmt_guard_drops_on_scope_exit() {
    let mut composer = Composer::new();
    composer.compose(|ctx| {
        let _ = ctx.start_scope_keyed(0xABCD);
        // 块内 enter_stmt——块尾（模拟 return/break 提前退出）guard drop 自动 pop
        {
            let _g = ctx.enter_stmt(7);
            // seq = 链哈希（位置分量）——只断言 id 与"非零 seq"（值不固定）
            let (id, seq) = STMT_STACK.with(|s| s.borrow().last().copied()).unwrap();
            assert_eq!(id, 7, "guard 生效：栈顶 id=7");
            let _ = seq;
        } // 块退出——guard drop
        assert!(STMT_STACK.with(|s| s.borrow().is_empty()), "提前退出后栈应自动恢复（无泄漏）");
        // guard 存活期间显式 pop 配对（guard 仍持有——drop 时再 pop 一次无害）
        let _g8 = ctx.enter_stmt(8);
        ctx.pop_stmt(); // 显式配对也正常
        drop(_g8);
        ctx.end_scope();
    });
}

/// for 循环迭代 key 回归（seq 位置化后）：行容器语句的 seq = slot 树兄弟
/// index——start_slot 每帧无条件执行（Skip 帧也执行）→ index 跨帧稳定 =
/// 迭代位置。content 闭包内语句（只在 Enter 执行）继承外层行语句的迭代
/// 位置（outer_seq）。滚动后部分行 Skip → 行容器的 index 不变 → key 稳定
/// （防 text29 撞 text0——旧执行计数机制在 Skip 帧漂移的根因）。
#[test]
fn test_stmt_seq_inherits_outer_iteration_position() {
    use crate::layout::components::Column;
    use std::cell::RefCell;
    let mut composer = Composer::new();
    let keys_first = RefCell::new(Vec::new());
    let keys_recompose = RefCell::new(Vec::new());
    // 记录行容器 key（enter_stmt(6) 后 next_key——真实组合场景）
    let mut scene = |composer: &mut Composer, skip_before: usize| {
        // Column 每帧传变化的 spacing → 强制 Enter（content 重跑）——
        // 模拟滚动触发 Column 重跑；行容器本身 clean + 参数未变 → content Skip
        composer.compose(crate::compose!(|ctx| {
            Column::new().spacing(skip_before as f32).build(ctx, |ctx| {
                for i in 0..30u32 {
                    // 行容器语句（每迭代执行）——seq = 兄弟 index = 迭代位置
                    let _g = ctx.enter_stmt(6);
                    let k = ctx.next_key();
                    match ctx.start_restartable_group(k, Modifier::new(), crate::layout::BoxLayout::new()) {
                        crate::runtime::composer::GroupStatus::Skip => {}
                        crate::runtime::composer::GroupStatus::Enter => {
                            // content 内语句（仅 Enter 执行）——继承外层迭代位置
                            let _g2 = ctx.enter_stmt(7);
                            let _ = ctx.next_key();
                            drop(_g2);
                        }
                    }
                    ctx.end_restartable_group();
                    drop(_g);
                    if i >= skip_before as u32 {
                        keys_recompose.borrow_mut().push(k);
                    }
                }
            });
        }));
        composer.layout(crate::layout::constraints::Constraints::new(0.0, 400.0, 0.0, 600.0));
    };
    // 首帧：30 行全 Enter（全部执行）
    scene(&mut composer, 0);
    keys_first.borrow_mut().extend(keys_recompose.borrow().iter().copied());
    keys_recompose.borrow_mut().clear();
    // 重组：前 29 行 content Skip（只 start 不 Enter），第 30 行 Enter——
    // 行容器 index 仍 0..29（start_slot 无条件）→ 第 30 行 key 与首帧一致
    scene(&mut composer, 29);
    assert_eq!(
        keys_first.borrow()[29], keys_recompose.borrow()[0],
        "content 内语句继承外层迭代位置——滚动后 key 与首帧一致（防 text29 撞 text0）"
    );
    assert_ne!(
        keys_first.borrow()[0], keys_first.borrow()[29],
        "迭代 key 互异——30 行 key 全同（无 seq 分量时代）会让槽树错乱"
    );
}

/// 跨函数 seq 隔离回归：不同 `#[composable]` 函数的语句 id 各自从 0 开始——
/// seq 取 slot 树兄弟位置（next_sibling_index）——函数 A/B 在不同 scope 槽位，
/// 各自 child_counters 独立 → seq 天然隔离（函数 B 行数变化不影响 A 的 key）。
#[test]
fn test_stmt_seq_isolated_across_functions() {
    let mut composer = Composer::new();
    let mut keys_a = Vec::new();
    let mut keys_b = Vec::new();
    composer.compose(|ctx| {
        // 函数 A（scope 0xAAAA）：3 次迭代
        let _ = ctx.start_scope_keyed(0xAAAA);
        for _ in 0..3 {
            ctx.push_stmt(1);
            keys_a.push(ctx.next_key());
            ctx.pop_stmt();
        }
        ctx.end_scope();
        // 函数 B（scope 0xBBBB）：语句 id 也从 1 开始——位置独立于 A
        let _ = ctx.start_scope_keyed(0xBBBB);
        ctx.push_stmt(1);
        keys_b.push(ctx.next_key());
        ctx.pop_stmt();
        ctx.end_scope();
    });
    // 新语义（seq = 链哈希位置）：A 的 3 次迭代位置互异（行实例区分）；
    // B 的位置含"A 在其前"的兄弟序号——与 B_alone（无 A）位置不同是正确
    // 行为（位置不同 = 不同 key = 各自独立，非"基数泄漏"）
    assert_ne!(
        keys_a[0], keys_a[1],
        "函数 A 内迭代位置互异（行实例区分）"
    );
    assert_ne!(
        keys_a[1], keys_a[2],
        "函数 A 内迭代位置互异（行实例区分）"
    );
}

/// 串位 bug 回归：两个不同 scope（不同源码哈希）内**相同的语句 id**（同 push_stmt(5)）
/// 必须生成不同 key——旧 bug（start_scope_keyed 双重 push None → scope=0）下
/// 同 stmt id 跨函数碰撞 → 节点复用串位（nest_demo row 0 ↔ [extra] button）
#[test]
fn test_scope_isolates_stmt_key_across_functions() {
    let mut composer = Composer::new();
    let mut keys = Vec::new();
    composer.compose(|ctx| {
        // 函数 A（hash A）
        let _ = ctx.start_scope_keyed(0xAAAA);
        ctx.push_stmt(5);
        let k_a = ctx.next_key();
        ctx.start_leaf(k_a, Modifier::new());
        ctx.end_node();
        ctx.pop_stmt();
        ctx.end_scope();
        // 函数 B（hash B——不同）
        let _ = ctx.start_scope_keyed(0xBBBB);
        ctx.push_stmt(5); // 相同语句 id
        let k_b = ctx.next_key();
        ctx.start_leaf(k_b, Modifier::new());
        ctx.end_node();
        ctx.pop_stmt();
        ctx.end_scope();
        keys.push((k_a, k_b));
    });
    // 断言 key 基（高位）不同——旧 bug（scope=0）下两函数 base 相同（仅 counter 区分）
    let base_a = keys[0].0 >> 32;
    let base_b = keys[0].1 >> 32;
    assert_ne!(base_a, base_b, "不同 scope 的 key 基必须不同（scope 隔离）——旧 bug 下 base 相同（scope=0）跨函数碰撞");
}

/// 手动 start_scope（push None）与宏 start_scope_keyed（push Some）混用——配对正确
#[test]
fn test_mixed_manual_and_keyed_scope_pairing() {
    let mut composer = Composer::new();
    composer.compose(|ctx| {
        let _ = ctx.start_scope_keyed(0xAAAA);
        {
            // 手动 scope（None——不覆盖外层 Some）
            let _ = ctx.start_scope();
            ctx.end_scope();
        }
        ctx.push_stmt(7);
        let k = ctx.next_key();
        ctx.start_leaf(k, Modifier::new());
        ctx.end_node();
        ctx.pop_stmt();
        ctx.end_scope();
        // 外层 keyed scope 的 source 应保留（未被手动 scope 的 None 破坏）
        assert_eq!(STMT_STACK.with(|s| s.borrow().len()), 0, "stmt 栈应清空");
    });
}

/// 阶段 5 集成测试：容器组件参数（spacing）相等跳过——
/// 参数变化 → Enter（content 重跑）；参数未变 + slot clean → Skip（content 不跑）
#[test]
fn test_component_param_change_forces_reenter() {
    use crate::layout::components::Column;
    let mut composer = Composer::new();
    let mut run_count = std::cell::Cell::new(0);

    // 帧 1：spacing 0（首帧——Enter）
    composer.compose(|ctx| {
        Column::new().spacing(0.0).build(ctx, |ctx| {
            run_count.set(run_count.get() + 1);
            let _ = ctx;
        });
    });
    composer.layout(crate::layout::constraints::Constraints::new(0.0, 800.0, 0.0, 600.0)); // 复用索引在 layout 更新
    assert_eq!(run_count.get(), 1, "首帧 content 执行");

    // 帧 2：spacing 10（参数变化 → Enter——content 重跑）
    run_count.set(0);
    composer.compose(|ctx| {
        Column::new().spacing(10.0).build(ctx, |ctx| {
            run_count.set(run_count.get() + 1);
            let _ = ctx;
        });
    });
    composer.layout(crate::layout::constraints::Constraints::new(0.0, 800.0, 0.0, 600.0));
    assert_eq!(run_count.get(), 1, "参数变化 → content 重跑");

    // 帧 3：spacing 10（参数未变 + clean → Skip——content 不跑）
    run_count.set(0);
    composer.compose(|ctx| {
        Column::new().spacing(10.0).build(ctx, |ctx| {
            run_count.set(run_count.get() + 1);
            let _ = ctx;
        });
    });
    composer.layout(crate::layout::constraints::Constraints::new(0.0, 800.0, 0.0, 600.0));
    assert_eq!(run_count.get(), 0, "参数未变 → Skip（content 不执行）");
}

/// The container Skip decision's modifier comparison, pinned from the outside: a REBUILT modifier with
/// the same numbers still Skips (the content does not re-run), and a changed number still Enters.
///
/// This is the safety net for the change that made the comparison read the arena node's modifier instead
/// of a copy parked in the slot: both halves must survive, and a version of this test that only checked
/// the Enter half would pass with the comparison removed entirely.
#[test]
fn test_container_modifier_comparison_skips_rebuilt_modifiers_and_enters_on_change() {
    let mut composer = Composer::new();
    let constraints = crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0);
    let runs = std::rc::Rc::new(std::cell::Cell::new(0usize));
    let padding = std::cell::Cell::new(8.0f32);

    let mut frame = |composer: &mut Composer| {
        let runs = runs.clone();
        let pad = padding.get();
        composer.compose(move |ctx| {
            let key = ctx.next_key();
            // A modifier with a COMPARABLE parameter (padding is a number) and a rebuilt-every-frame
            // closure (the content), which is what makes "same numbers" the interesting case.
            match ctx.start_restartable_group(
                key,
                Modifier::new().padding(pad),
                crate::layout::BoxLayout::new(),
            ) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    runs.set(runs.get() + 1);
                    let leaf = ctx.next_key();
                    ctx.start_leaf(leaf, Modifier::new().size(10.0, 10.0));
                    ctx.end_node();
                }
            }
            ctx.end_restartable_group();
        });
        composer.layout(constraints);
    };

    frame(&mut composer);
    assert_eq!(runs.get(), 1, "frame 1 builds the content");
    let root = composer.layout_root_idx().unwrap();
    let key = composer.arena_nodes()[root].slot_key;

    // Rebuilt modifier, same number → Skip (content does NOT re-run).
    frame(&mut composer);
    assert_eq!(runs.get(), 1, "a rebuilt-but-numerically-equal modifier must still Skip");

    // Changed number → Enter (content re-runs).
    padding.set(16.0);
    frame(&mut composer);
    assert_eq!(runs.get(), 2, "a changed modifier parameter must re-run the content");

    // And the node carries the new value, which is what the next frame's comparison reads.
    let root = composer.layout_root_idx().unwrap();
    let node_pad = composer.arena_nodes()[root]
        .modifier
        .get_padding_horizontal()
        .0;
    assert_eq!(node_pad, 16.0, "the applied modifier must hold the new padding");
    assert_ne!(key, 0, "sanity: the container has a real slot key");
}

/// A second compose in the SAME frame still Skips an unchanged container.
///
/// This is what `compose`'s refill of `prev_node_by_key` is for. The map is drained by the frame's first
/// materialize, so without the refill the container's Skip decision would find no node to compare its
/// modifier against, and every group would Enter on the second pass — a full re-run of the tree on frames
/// where a notification arrived mid-compose, which the app's frame loop produces precisely so such
/// notifications are not lost. With the refill the second pass behaves like the first: unchanged
/// containers keep what they built.
#[test]
fn test_second_compose_in_a_frame_still_skips_unchanged_containers() {
    let mut composer = Composer::new();
    let constraints = crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0);
    let runs = std::rc::Rc::new(std::cell::Cell::new(0usize));

    let mut build = |composer: &mut Composer| {
        let runs = runs.clone();
        composer.compose(move |ctx| {
            let key = ctx.next_key();
            match ctx.start_restartable_group(key, Modifier::new().padding(8.0), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    runs.set(runs.get() + 1);
                    let leaf = ctx.next_key();
                    ctx.start_leaf(leaf, Modifier::new().size(10.0, 10.0));
                    ctx.end_node();
                }
            }
            ctx.end_restartable_group();
        });
    };

    build(&mut composer);
    composer.layout(constraints);
    assert_eq!(runs.get(), 1, "frame 1 builds the content");

    // Second compose in the SAME frame (no layout between them): the index was drained by the first
    // materialize, and the refill is what lets this pass compare modifiers at all.
    build(&mut composer);
    assert_eq!(
        runs.get(),
        1,
        "the second compose in a frame must Skip an unchanged container — if this reads 2, the index \
         was empty and every group re-entered"
    );

    // …and a THIRD pass still sees the same thing (the refill is not a one-shot).
    build(&mut composer);
    assert_eq!(runs.get(), 1, "later passes in the same frame behave the same way");
}

/// 参数变化后布局层必须更新（should-fix 回归：Enter 时置 dirty——
/// 否则 measure_node 常量折叠返回上帧尺寸/子位置）
#[test]
fn test_param_change_updates_layout() {
    use crate::layout::components::Column;
    use crate::layout::node::find_node_by_id;
    let mut composer = Composer::new();
    let root_id = std::cell::Cell::new(0u64);

    // 帧 1：spacing 0，两个子 Text
    composer.compose(crate::compose!(|ctx| {
        Column::new().spacing(0.0).build(ctx, |ctx| {
            crate::components::text::Text::new("a").build(ctx);
            crate::components::text::Text::new("b").build(ctx);
        });
    }));
    let c = crate::layout::constraints::Constraints::new(0.0, 800.0, 0.0, 600.0);
    composer.layout(c);

    // 帧 2：spacing 10——参数变化 → Enter + dirty → 布局更新
    composer.compose(crate::compose!(|ctx| {
        Column::new().spacing(10.0).build(ctx, |ctx| {
            crate::components::text::Text::new("a").build(ctx);
            crate::components::text::Text::new("b").build(ctx);
        });
    }));
    composer.layout(c);
    let nodes = &composer.arena.nodes;
    // b 的 position（相对 Column——spacing 10 后应 > spacing 0 时）
    // 子 Text 的 position（相对 Column）：a 在 0，b 在 a 高 + spacing 之后
    let positions: Vec<f32> = nodes.iter().filter(|n| n.slot_key != 0 && n.measured_size.height > 0.0)
        .map(|n| n.position.y).collect();
    assert!(positions.len() >= 2, "两个子节点应有 position");
    let max_y = positions.iter().cloned().fold(0.0_f32, f32::max);
    assert!(max_y >= 19.0 + 10.0 - 0.5,
        "参数变化后子位置应反映 spacing=10（b 应在 a 高 19 + spacing 10 之后——实际 {positions:?}）");
}

/// ③ 布局树独立缓存边界：结构变化删除的节点槽位应回收复用（free 池）——
/// arena 容量不随结构变化持续增长
#[test]
fn test_arena_recycles_freed_slots() {
    use crate::layout::components::Column;
    use crate::components::text::Text;
    let mut composer = Composer::new();
    let show = crate::runtime::state::State::new(true);
    let c = crate::layout::constraints::Constraints::new(0.0, 800.0, 0.0, 600.0);

    // 帧 1：show=true——含 extra 分支（3 个 Text）
    let mut cap1 = 0;
    composer.compose(crate::compose!(|ctx| {
        Column::new().build(ctx, |ctx| {
            Text::new("a").build(ctx);
            if show.get() {
                Text::new("b").build(ctx);
                Text::new("c").build(ctx);
            }
        });
    }));
    composer.layout(c);
    cap1 = composer.arena.nodes.len();
    assert!(cap1 >= 4, "帧1 应有 4+ 节点（根+3 Text）");

    // 帧 2：show=false——extra 分支删除（2 节点 free）
    show.set(false);
    composer.compose(crate::compose!(|ctx| {
        Column::new().build(ctx, |ctx| {
            Text::new("a").build(ctx);
            if show.get() {
                Text::new("b").build(ctx);
                Text::new("c").build(ctx);
            }
        });
    }));
    composer.layout(c);

    // 帧 3：show=true——重新创建分支——槽位应复用（容量不持续增长）
    show.set(true);
    composer.compose(crate::compose!(|ctx| {
        Column::new().build(ctx, |ctx| {
            Text::new("a").build(ctx);
            if show.get() {
                Text::new("b").build(ctx);
                Text::new("c").build(ctx);
            }
        });
    }));
    composer.layout(c);
    let cap3 = composer.arena.nodes.len();
    assert!(cap3 <= cap1 + 2, "槽位应复用（帧3 容量 {cap3} 不应远超帧1 {cap1}——free 池回收）");

}

#[test]
fn test_materialize_structure_change_window_insert() {
    // 结构变化回归：if 分支插入（Window 场景）——slot 树重建——物化树应完整
    let mut composer = Composer::new();
    // show 必须经 remember 创建（owner queue 绑定——notify 定向推送本 Composer）
    let mut show: Option<crate::runtime::state::State<bool>> = None;

    // 帧1：false（无 Window 分支）
    composer.compose(|ctx| {
        let root_key = ctx.next_key();
        match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                show = Some(ctx.remember(|| false));
                let _ = show.as_ref().unwrap().get();
                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); }
                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); }
            }
        }
        ctx.end_restartable_group();
    });
    let r = composer.layout_root_idx().unwrap();
    assert_eq!(composer.arena_nodes()[r].children.len(), 2, "帧1 应 2 leaf");

    // 帧2：true（Window 分支插入——结构变化）
    show.as_ref().unwrap().set(true);
    composer.compose(|ctx| {
        let root_key = ctx.next_key();
        match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                let _ = show.as_ref().unwrap().get();
                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); }
                if show.as_ref().unwrap().get() {
                    { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); }
                }
            }
        }
        ctx.end_restartable_group();
    });
    let r = composer.layout_root_idx().unwrap();
    assert_eq!(composer.arena_nodes()[r].children.len(), 2, "帧2 应 2 leaf（if 插入后）");

    // 帧3：false（结构回退）
    show.as_ref().unwrap().set(false);
    composer.compose(|ctx| {
        let root_key = ctx.next_key();
        match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                let _ = show.as_ref().unwrap().get();
                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); }
            }
        }
        ctx.end_restartable_group();
    });
    let r = composer.layout_root_idx().unwrap();
    eprintln!("[t3] root children={} nodes={}", composer.arena_nodes()[r].children.len(), composer.arena_nodes().len());
    for c in composer.arena_nodes()[r].children.clone() {
        eprintln!("[t3] child key={} dirty={}", composer.arena_nodes()[c].slot_key >> 32, composer.arena_nodes()[c].dirty);
    }
    assert_eq!(composer.arena_nodes()[r].children.len(), 1, "帧3 应 1 leaf（回退）");
}

#[test]
fn test_materialize_skip_restores_subtree() {
    // Skip 恢复（物化核心路径）：帧2 无 State 变化/无参数变化 → 容器 Skip →
    // content 不执行 → 物化从 prev_node_by_key 恢复整个子树（children 重新挂接）
    let mut composer = Composer::new();
    let c = crate::layout::constraints::Constraints::new(0.0, 100.0, 0.0, 100.0);

    // 帧1：Enter——容器 + 2 leaf
    composer.compose(|ctx| {
        let key = ctx.next_key();
        match ctx.start_restartable_group(key, Modifier::new(), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); }
                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); }
            }
        }
        ctx.end_restartable_group();
    });
    composer.layout(c);
    let r = composer.layout_root_idx().unwrap();
    assert_eq!(composer.arena_nodes()[r].children.len(), 2, "帧1 应 2 leaf");

    // 帧2：无变化 → 容器 Skip（content 不跑）→ 物化恢复上帧子树
    composer.compose(|ctx| {
        let key = ctx.next_key();
        match ctx.start_restartable_group(key, Modifier::new(), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                panic!("帧2 应 Skip（content 不应执行）");
            }
        }
        ctx.end_restartable_group();
    });
    composer.layout(c);
    let r = composer.layout_root_idx().unwrap();
    assert_eq!(composer.arena_nodes()[r].children.len(), 2, "Skip 应恢复上帧 2 leaf");
    for ci in composer.arena_nodes()[r].children.clone() {
        let n = &composer.arena_nodes()[ci];
        assert!(!n.dirty, "恢复的 leaf 不应 dirty（缓存测量保留）");
        assert!(n.cached_constraints.is_some(), "恢复的 leaf 应保留测量缓存（cached_constraints）");
        assert_eq!(n.slot_key, composer.arena_nodes()[r].children.iter().find(|&&x| x == ci).map(|_| composer.arena_nodes()[ci].slot_key).unwrap(), "恢复的 leaf slot_key 保留");
    }
}

/// review 修复回归：复用路径必须刷新 layout_direction 快照。
/// 帧1 无方向元素 → Ltr；帧2 modifier 加 layout_direction(Rtl) →
/// param_eq 变化 → Enter 复用旧节点 → 快照必须更新为 Rtl
/// （修复前只有新建节点路径设置快照，复用节点保持旧 Ltr → padding 镜像失效）
#[test]
fn test_materialize_reuse_refreshes_layout_direction() {
    use crate::layout::LayoutDirection;
    let mut composer = Composer::new();
    let c = crate::layout::constraints::Constraints::new(0.0, 100.0, 0.0, 100.0);

    composer.compose(|ctx| {
        let key = ctx.next_key();
        match ctx.start_restartable_group(key, Modifier::new(), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); }
            }
        }
        ctx.end_restartable_group();
    });
    composer.layout(c);
    let r = composer.layout_root_idx().unwrap();
    assert_eq!(
        composer.arena_nodes()[r].layout_direction,
        LayoutDirection::Ltr,
        "帧1 无方向元素 → Ltr"
    );

    // 帧2：modifier 加 layout_direction(Rtl) → param_eq 不等 → Enter 复用旧节点
    composer.compose(|ctx| {
        let key = ctx.next_key();
        match ctx.start_restartable_group(
            key,
            Modifier::new().layout_direction(LayoutDirection::Rtl),
            crate::layout::BoxLayout::new(),
        ) {
            GroupStatus::Skip => panic!("方向变化应 Enter（param_eq 不等）"),
            GroupStatus::Enter => {
                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); }
            }
        }
        ctx.end_restartable_group();
    });
    composer.layout(c);
    let r = composer.layout_root_idx().unwrap();
    assert_eq!(
        composer.arena_nodes()[r].layout_direction,
        LayoutDirection::Rtl,
        "复用路径必须刷新方向快照（modifier 覆盖 > theme）"
    );
}

/// 语义角色切换回归（audit §3.5 残留场景）：TextField 输入叶子（有 IME/cursor
/// 回调）→ 普通 Text（无回调）切换时，残留的 IME/cursor 状态必须被清理——
/// 否则新 Text 节点仍持有旧 IME 回调，渲染/输入路径污染。
#[test]
fn test_materialize_reuse_clears_stale_textfield_state_on_role_switch() {
    let mut composer = Composer::new();
    let c = crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0);

    // 帧1：TextField 输入叶子——挂 IME 回调（对标 text_field.rs 的 set_current_node_ime_callback）
    let mut leaf_key = 0u64;
    composer.compose(|ctx| {
        let root_key = ctx.next_key();
        match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                let k = ctx.next_key();
                leaf_key = k;
                ctx.start_leaf(k, Modifier::new());
                ctx.set_current_node_ime_callback(Box::new(|_text: &str, _c: Option<(usize, usize)>| {}));
                ctx.set_current_node_cursor_and_callback(0, false, Box::new(|_i: usize| {}));
                ctx.end_node();
            }
        }
        ctx.end_restartable_group();
    });
    composer.layout(c);
    let root = composer.layout_root_idx().unwrap();
    let leaf = composer.arena_nodes()[root].children[0];
    assert!(composer.arena_nodes()[leaf].ime_callback.borrow().is_some(),
        "帧1 应有 IME 回调（TextField 场景）");

    // 帧2：同位置变成普通 Text——无 IME/cursor 回调 → 复用节点应清理残留
    // 根容器加 size 参数强制 Enter（否则参数/结构不变 → Skip 导致 build 不执行）
    composer.compose(|ctx| {
        let root_key = ctx.next_key();
        match ctx.start_restartable_group(root_key, Modifier::new().size(100.0, 100.0), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                let k = ctx.next_key();
                assert_eq!(k, leaf_key, "同位置 leaf key 应稳定");
                // 普通 Text 叶子：仅 TextContent，无 IME/cursor 回调
                let modifier = Modifier::new().push(crate::modifier::ModifierElement::TextContent {
                    content: "plain".to_string(),
                    font_size: 14.0,
                    color: crate::graphics::Color::from_argb(255, 0, 0, 0),
                    font_weight: crate::text::FontWeight::NORMAL,
                    font_style: crate::text::FontSlant::Upright,
                    max_lines: usize::MAX,
                    align: crate::text::TextAlign::Left,
                    overflow: crate::text::TextOverflow::Clip,
                    soft_wrap: true,
                    letter_spacing: 0.0,
                    line_height: None,
                    min_lines: 1,
                    underline: false,
                    line_through: false,
                    font_family: None,
                    on_text_layout: None,
                    auto_size: None,
                });
                ctx.start_leaf(k, modifier);
                ctx.end_node();
            }
        }
        ctx.end_restartable_group();
    });
    composer.layout(c);

    let root = composer.layout_root_idx().unwrap();
    let leaf = composer.arena_nodes()[root].children[0];
    let has_text = composer.arena_nodes()[leaf].has_text_content;
    assert!(has_text, "帧2 leaf 应有 has_text_content=true（modifier 含 TextContent）");
    assert!(
        composer.arena_nodes()[leaf].ime_callback.borrow().is_none(),
        "角色切换后 IME 回调应被清理（残留会污染普通 Text）"
    );
    assert!(
        composer.arena_nodes()[leaf].cursor_callback.borrow().is_none(),
        "角色切换后 cursor 回调应被清理"
    );
}

/// RTL 全局切换修复回归：组合期 provides 作用域内捕获方向到 desc——
/// 物化在组合回调后执行（主题的 provider 已退出作用域），
/// 修复前物化期读 theme 恒 Ltr → offset/padding 镜像全部失效（用户实测
/// "都是同向运动"根因）。此测试验证 desc 携带方向。
#[test]
fn test_compose_captures_direction_in_provides_scope() {
    use crate::theme::WiniaTheme;
    let mut composer = Composer::new();
    let c = crate::layout::constraints::Constraints::new(0.0, 100.0, 0.0, 100.0);

    // 模拟 demo 全局切换：with_theme_and_direction(Rtl) 包住组合回调
    composer.compose(|ctx| {
        WiniaTheme::with_theme_and_direction(
            WiniaTheme::colors(),
            crate::layout::LayoutDirection::Rtl,
            ctx,
            |ctx| {
                let key = ctx.next_key();
                ctx.start_leaf(key, Modifier::new());
                ctx.end_node();
            },
        );
    });
    composer.layout(c);
    let r = composer.layout_root_idx().unwrap();
    assert_eq!(
        composer.arena_nodes()[r].layout_direction,
        crate::layout::LayoutDirection::Rtl,
        "组合期 provides 作用域内必须捕获 Rtl（物化期读 theme 会退回 Ltr）"
    );

    // 对照组：无 provides → Ltr
    let mut composer = Composer::new();
    composer.compose(|ctx| {
        let key = ctx.next_key();
        ctx.start_leaf(key, Modifier::new());
        ctx.end_node();
    });
    composer.layout(c);
    let r = composer.layout_root_idx().unwrap();
    assert_eq!(
        composer.arena_nodes()[r].layout_direction,
        crate::layout::LayoutDirection::Ltr,
        "无 provides 作用域默认 Ltr"
    );
}

// ═══════════════════════════════════════════════════════════
// P3-1 Skip 恢复健壮性测试（结构签名）
// ═══════════════════════════════════════════════════════════

/// T2：State 驱动结构变化回归——if 分支增删（show_b State）→ root Enter 重建，
/// A 位置不复用 B 缓存，B 移除后无残留。
#[test]
fn test_skip_recovery_structure_change_by_state() {
    let mut composer = Composer::new();
    let holder = std::cell::RefCell::new(None::<State<bool>>);

    let build = |composer: &mut Composer, holder: &std::cell::RefCell<Option<State<bool>>>| {
        composer.compose(|ctx| {
            let show_b = ctx.remember(|| true);
            *holder.borrow_mut() = Some(show_b.clone());
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    // A 叶子（始终在）
                    { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); }
                    // B 分支（show_b 控制）
                    if show_b.get() {
                        let k = ctx.next_key();
                        ctx.start_leaf(k, Modifier::new().size(100.0, 50.0));
                        ctx.end_node();
                    }
                }
            }
            ctx.end_restartable_group();
        });
        composer.layout(crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0));
    };

    build(&mut composer, &holder);
    let r = composer.layout_root_idx().unwrap();
    assert_eq!(composer.arena_nodes()[r].children.len(), 2, "帧1 应有 A+B 两个 leaf");

    // 帧2：show_b=false（State 驱动 → root Enter → content 重跑 → B 分支不建）
    holder.borrow().as_ref().unwrap().set(false);
    build(&mut composer, &holder);
    let r = composer.layout_root_idx().unwrap();
    assert_eq!(composer.arena_nodes()[r].children.len(), 1, "帧2 应只剩 A（B 移除）");
    // A 正常保留；不应复用 B 的缓存（B 的 size 100x50）
    let a = composer.arena_nodes()[r].children[0];
    assert!(composer.arena_nodes()[a].measured_size.width < 100.0,
        "A 不应复用 B 的缓存（B 的 width=100 不应出现在 A）——width={}", composer.arena_nodes()[a].measured_size.width);

    // 帧3：B 恢复
    holder.borrow().as_ref().unwrap().set(true);
    build(&mut composer, &holder);
    let r = composer.layout_root_idx().unwrap();
    assert_eq!(composer.arena_nodes()[r].children.len(), 2, "帧3 应恢复 A+B");
}

/// `SlotTable::slot_count_bound` must be an UPPER bound on what materialize builds.
///
/// Two `Vec::reserve` calls are sized from it — the layout arena in `materialize` and the live-key set
/// in `collect_live_keys` — and both are only worth anything if the number is at least as large as what
/// is about to be built. The first version of this test's target read `root_slot.children_count`, which
/// `end_slot` maintains for every slot EXCEPT the root (nothing ever ends the root), so it held its
/// constructor value of 1: both reserves asked for two elements and neither ever did anything, while
/// the hasher that shipped with them took the credit. This asserts the bound against the tree that
/// materialize actually produced, which is the property the callers rely on.
#[test]
fn test_slot_count_bound_is_an_upper_bound_on_materialized_nodes() {
    let mut composer = Composer::new();
    let constraints = crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0);
    for rows in [1usize, 8, 64] {
        composer.compose(|ctx| {
            let key = ctx.next_key();
            // The container carries a size that CHANGES with `rows`, which is what makes it Enter: with
            // an unchanged parameter the group Skips and the leaves below are never composed — a first
            // version of this test built 2 nodes for all three shapes and therefore asserted nothing.
            match ctx.start_restartable_group(
                key,
                Modifier::new().size(100.0 + rows as f32, 100.0),
                crate::layout::BoxLayout::new(),
            ) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    for _ in 0..rows {
                        let leaf = ctx.next_key();
                        ctx.start_leaf(leaf, Modifier::new().size(10.0, 10.0));
                        ctx.end_node();
                    }
                }
            }
            ctx.end_restartable_group();
        });
        composer.layout(constraints);
        let bound = composer.slot_table.slot_count_bound();
        let built = composer.arena.nodes.len();
        assert!(
            built > rows,
            "{rows} rows: materialize built only {built} nodes — the frame skipped, so this test would \
             assert nothing (its first version did exactly that)"
        );
        assert!(
            bound >= built,
            "{rows} rows: the bound says {bound} nodes but materialize built {built} — a reserve sized \
             from it asks for less than the frame needs, which is how both of this round's reserves came \
             out as no-ops"
        );
        // And it must still be a BOUND, not a constant: the slot tree for N leaves holds the container,
        // the leaves, and the group's own slot.
        assert!(
            bound <= built * 2 + 4,
            "{rows} rows: the bound ({bound}) is far above what was built ({built}) — it is meant to be \
             the slot tree's node count, not a constant"
        );
    }
}

/// A slot whose key moved to a different index among its siblings keeps its slots — the reorder case.
///
/// `start_slot` used to look for a matching key only AT the current index (or a sibling whose path-hash
/// prefix matched there) and otherwise truncated the parent's children from that index. Statement
/// order cannot change on its own, so the only way a key arrives at an index it did not occupy is an
/// explicit `ctx.key` list that reordered — and there the truncation threw away exactly the slots the
/// key exists to keep. Measured with this shape, two keyed subtrees swapped: both were recreated on
/// every flip.
///
/// Teeth: drop the keyed lookup (keep only the index match) and `created` counts 4 instead of 2.
#[test]
fn a_keyed_sibling_that_moves_keeps_its_slots() {
    let mut composer = Composer::new();
    let constraints = crate::layout::constraints::Constraints::new(0.0, 400.0, 0.0, 400.0);
    let reversed = State::new(false);
    let flag = reversed.clone();
    let created = std::cell::Cell::new(0u32);

    let mut recompose = |composer: &mut Composer, created: &std::cell::Cell<u32>| {
        composer.compose(|ctx| {
            let flip = flag.get();
            let order: [u64; 2] = if flip { [2, 1] } else { [1, 2] };
            for id in order {
                ctx.key(id, |ctx| {
                    let key = ctx.next_key();
                    match ctx.start_restartable_group(
                        key,
                        Modifier::new(),
                        crate::layout::box_layout::BoxLayout::new(),
                    ) {
                        GroupStatus::Skip => {}
                        GroupStatus::Enter => {
                            let _ = ctx.remember(|| created.set(created.get() + 1));
                        }
                    }
                    ctx.end_restartable_group();
                });
            }
        });
        composer.layout(constraints);
    };

    recompose(&mut composer, &created);
    assert_eq!(created.get(), 2, "two keyed subtrees, one slot each");
    reversed.set(true);
    recompose(&mut composer, &created);
    assert_eq!(created.get(), 2, "the order flipped and both kept their slots");
    reversed.set(false);
    recompose(&mut composer, &created);
    assert_eq!(created.get(), 2, "…and back");
}

/// `keyed_range` is the loop form of `ctx.key`: the same prefix/index pair keeps each child's slots
/// across frames instead of assigning them by position.
#[test]
fn keyed_range_keeps_each_indexed_child_s_slots() {
    let mut composer = Composer::new();
    let constraints = crate::layout::constraints::Constraints::new(0.0, 400.0, 0.0, 400.0);
    let created = std::cell::Cell::new(0u32);

    let mut frame = |composer: &mut Composer| {
        composer.compose(|ctx| {
            ctx.keyed_range("range", 3, |ctx, index| {
                let key = ctx.next_key();
                match ctx.start_restartable_group(
                    key,
                    Modifier::new(),
                    crate::layout::box_layout::BoxLayout::new(),
                ) {
                    GroupStatus::Skip => {}
                    GroupStatus::Enter => {
                        let _ = ctx.remember(|| {
                            assert!(index < 3);
                            created.set(created.get() + 1);
                        });
                    }
                }
                ctx.end_restartable_group();
            });
        });
        composer.layout(constraints);
    };

    frame(&mut composer);
    assert_eq!(created.get(), 3, "one slot for each index");
    frame(&mut composer);
    assert_eq!(created.get(), 3, "the same prefix/index pairs keep their slots");
}

/// The generic iterator form keys by item identity; reversing the iterator must not recreate
/// the children.
#[test]
fn keyed_for_each_keeps_item_identity_when_the_order_changes() {
    let mut composer = Composer::new();
    let constraints = crate::layout::constraints::Constraints::new(0.0, 400.0, 0.0, 400.0);
    let reversed = State::new(false);
    let flag = reversed.clone();
    let created = std::cell::Cell::new(0u32);

    let mut frame = |composer: &mut Composer| {
        composer.compose(|ctx| {
            let mut items = vec![("a", 1), ("b", 2)];
            if flag.get() {
                items.reverse();
            }
            ctx.keyed_for_each(
                items,
                |item| item.0,
                |ctx, item| {
                    let key = ctx.next_key();
                    match ctx.start_restartable_group(
                        key,
                        Modifier::new(),
                        crate::layout::box_layout::BoxLayout::new(),
                    ) {
                        GroupStatus::Skip => {}
                        GroupStatus::Enter => {
                            let _ = ctx.remember(|| {
                                assert!(item.1 > 0);
                                created.set(created.get() + 1);
                            });
                        }
                    }
                    ctx.end_restartable_group();
                },
            );
        });
        composer.layout(constraints);
    };

    frame(&mut composer);
    assert_eq!(created.get(), 2, "one slot for each item");
    reversed.set(true);
    frame(&mut composer);
    assert_eq!(created.get(), 2, "the reversed items keep their keys");
}

/// A slot can survive a compose UNVISITED and outside a skipped subtree, so `collect_live_keys` is
/// not equivalent to "every slot in the tree".
///
/// That equivalence is tempting: the live-key set costs a ~4000-insert walk per frame at 800 rows, all
/// to answer a handful of lookups, and if the tree only ever held live slots the set could be replaced
/// by recording the keys that LEAVE the tree (a rare, small event). This test is the reason that
/// replacement is not safe, and its shape is the point: the tree below shrinks an INNER group's
/// contents while the outer group re-enters. The outer group's own pruning only drops its direct
/// children, and the inner group's slot was visited, so the extra leaves stay in the tree with nothing
/// to prune them.
///
/// The number has to be read DURING the frame (`Composer::live_residue` is captured by `compose`
/// itself): materialize consumes the `desc`/`skip_modifier` markers that identify a skipped subtree, so
/// the same tree counted afterwards looks like it has far more residue than it did while composing —
/// which is exactly how a first version of this test came out with 9 slots that were all live.
#[test]
fn test_live_keys_exclude_unvisited_slots() {
    let mut composer = Composer::new();
    let constraints = crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0);
    let tick = State::new(0i32);
    let holder = std::cell::RefCell::new((0u32, 0u32));

    // `outer` re-enters every frame (it reads `tick`); inside it a SCOPE emits a variable number of
    // leaves. A scope is the interesting container: it is visited and never skips, and unlike a
    // restartable group it does not prune its children either (`end_scope` is `end_slot`), so nothing
    // on the path drops the leaves that stopped being composed.
    let build = |composer: &mut Composer, holder: &std::cell::RefCell<(u32, u32)>| {
        let (groups, leaves) = *holder.borrow();
        composer.compose(|ctx| {
            let key = ctx.next_key();
            match ctx.start_restartable_group(key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    let _ = tick.get();
                    for _ in 0..groups {
                        let _scope = ctx.start_scope_keyed(0x5C0FE0DE);
                        for _ in 0..leaves {
                            let leaf = ctx.next_key();
                            ctx.start_leaf(leaf, Modifier::new().size(10.0, 10.0));
                            ctx.end_node();
                        }
                        ctx.end_scope();
                    }
                }
            }
            ctx.end_restartable_group();
        });
        composer.layout(constraints);
    };

    // 3 groups x 3 leaves, then the interiors shrink to 1: the outer group re-enters, so nothing
    // skips its way out of being counted.
    *holder.borrow_mut() = (3, 3);
    tick.set(tick.get() + 1);
    build(&mut composer, &holder);
    assert_eq!(composer.live_residue, 0, "a frame with no leftovers has no residue");

    *holder.borrow_mut() = (3, 1);
    tick.set(tick.get() + 1);
    build(&mut composer, &holder);
    assert!(
        composer.live_residue > 0,
        "shrinking the interior of a visited group must leave unvisited slots behind — if this is 0, \
         the tree prunes them somewhere and `collect_live_keys` could be replaced by 'the keys in the \
         tree' (in which case that replacement FIXES this test, it does not break it)"
    );
}

/// T3：结构签名直接验证——手动构造"缓存 children 数 != desc children 数"，
/// materialize_node Skip 恢复应放弃（重建 + key 保留待回收，防张冠李戴与泄漏）。
#[test]
fn test_skip_recovery_sig_mismatch_direct() {
    let mut composer = Composer::new();
    // 帧1：root + 2 leaf
    composer.compose(|ctx| {
        let root_key = ctx.next_key();
        match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); }
                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); }
            }
        }
        ctx.end_restartable_group();
    });
    composer.layout(crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0));
    let old_root = composer.layout_root_idx().unwrap();
    let old_root_key = composer.arena_nodes()[old_root].slot_key;
    let leaf0_key = composer.arena_nodes()[composer.arena_nodes()[old_root].children[0]].slot_key;

    // 手动构造 Skip desc：root 只有 1 子（缓存 2 子——签名不等）
    let desc = crate::runtime::materialize::DescNode {
        key: old_root_key,
        skip: true,
        claimed: None,
        modifier: Modifier::new(),
        preserve_modifier: true,
        policy: None,
        on_remove: None,
        dirty: false,
        registrar: None,
        focus_color: None,
        composing_color: None,
            cursor_index: None,
            cursor_visible: None,
            cursor_callback: None,
            display_focused: None,
            ime_callback: None,
        composing_range: None,
        direction: crate::layout::LayoutDirection::Ltr,
        children: vec![crate::runtime::materialize::DescNode {
            key: leaf0_key,
            skip: true,
        claimed: None,
            modifier: Modifier::new(),
            preserve_modifier: true,
            policy: None,
            on_remove: None,
            dirty: false,
            registrar: None,
            focus_color: None,
            composing_color: None,
            cursor_index: None,
            cursor_visible: None,
            cursor_callback: None,
            display_focused: None,
            ime_callback: None,
            composing_range: None,
            direction: crate::layout::LayoutDirection::Ltr,
            children: vec![],
        }],
    };
    composer.arena.root = None; // 模拟新帧物化开始
    let new_root = crate::runtime::materialize::materialize_node(&mut composer, desc, None).unwrap();

    // 断言：签名不等 → 重建（new_root != old_root）而非恢复缓存
    assert_ne!(new_root, old_root, "签名不等应重建而非恢复缓存");
    // 旧 root 未被复用（不在 reused_nodes）；key 保留在 prev_node_by_key（待 compose 末尾回收 free）
    assert!(!composer.reused_nodes.contains(old_root), "旧节点不应标记复用（待回收）");
    assert!(composer.prev_node_by_key.contains_key(&old_root_key),
        "key 应保留待回收（否则旧节点 arena 泄漏）");
}

/// A skipped subtree whose slots are all inert is claimed in place, and the shape is verified.
///
/// The claim is what makes an idle frame cheap (`docs/benchmarks.md`), and its whole safety rests on
/// `try_claim_skipped_subtree`'s walk: the arena must hold exactly this subtree, node for node. These
/// two tests pin both halves — that the claim fires on an idle frame, and that a stale arena shape
/// makes it BAIL rather than keep a tree that no longer matches the slots.
#[test]
fn test_skipped_subtree_is_claimed_in_place() {
    let mut composer = Composer::new();
    let constraints = crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0);
    let build = |composer: &mut Composer| {
        composer.compose(|ctx| {
            let key = ctx.next_key();
            match ctx.start_restartable_group(key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    for _ in 0..3 {
                        let leaf = ctx.next_key();
                        ctx.start_leaf(leaf, Modifier::new().size(20.0, 10.0));
                        ctx.end_node();
                    }
                }
            }
            ctx.end_restartable_group();
        });
        composer.layout(constraints);
    };

    build(&mut composer);
    let root = composer.layout_root_idx().unwrap();
    let kids: Vec<usize> = composer.arena_nodes()[root].children.clone();
    assert_eq!(kids.len(), 3, "precondition: three leaves under the container");
    assert_eq!(composer.skip_claims, 0, "frame 1 entered: nothing to claim");

    // Frame 2: the container's slot is clean and nothing reads state → it Skips with every slot inert.
    build(&mut composer);
    assert_eq!(composer.skip_claims, 1, "the skipped subtree must be claimed in place");
    let root2 = composer.layout_root_idx().unwrap();
    assert_eq!(composer.arena_nodes()[root2].children, kids,
        "the claimed subtree is the SAME nodes in the SAME order (nothing was rebuilt)");

    // Frame 3: still idle → claimed again (the claim leaves the slots in the state it found them).
    build(&mut composer);
    assert_eq!(composer.skip_claims, 1, "a second idle frame claims again");
}

/// A stale arena shape must make the claim bail — and the fallback must repair the tree.
///
/// This is the failure the whole verification exists for: if the arena no longer matches the slot
/// tree, keeping it in place would render a structure that the slots do not describe. Corrupting the
/// arena by hand is the only way to reach that state deliberately.
#[test]
fn test_stale_arena_shape_makes_the_claim_bail() {
    let mut composer = Composer::new();
    let constraints = crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0);
    let build = |composer: &mut Composer| {
        composer.compose(|ctx| {
            let key = ctx.next_key();
            match ctx.start_restartable_group(key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    for i in 0..2 {
                        let leaf = ctx.next_key();
                        // Distinct keys of the same shape, so a reorder is the only difference.
                        ctx.start_leaf(leaf, Modifier::new().size(20.0 + i as f32, 10.0));
                        ctx.end_node();
                    }
                }
            }
            ctx.end_restartable_group();
        });
        composer.layout(constraints);
    };

    build(&mut composer);
    build(&mut composer);
    assert_eq!(composer.skip_claims, 1, "precondition: the idle frame claims");

    // Corrupt the arena: swap the container's two children. The slots still say [a, b].
    let root = composer.layout_root_idx().unwrap();
    let before: Vec<usize> = composer.arena_nodes()[root].children.clone();
    assert_eq!(before.len(), 2);
    {
        let nodes = composer.arena_nodes_mut();
        nodes[root].children.swap(0, 1);
    }

    // The next idle frame must notice and rebuild from the slots instead of trusting the arena.
    build(&mut composer);
    assert!(composer.skip_claim_bails >= 1, "a shape mismatch must reject the container's claim");
    let root = composer.layout_root_idx().unwrap();
    assert_eq!(composer.arena_nodes()[root].children, before,
        "the fallback rebuilds the children in SLOT order (the corruption is repaired)");
    // The two leaves are inert subtrees in their own right and are still claimed — the rejection is
    // per subtree, which is what keeps a single stale subtree from costing the whole frame.
    assert_eq!(composer.skip_claims, 2, "the leaves are claimed even though their parent bailed");
}

/// T4：数量相同内容不同（A→B 同位置）——保持恢复（Compose 位置复用语义，不强制 Enter）
#[test]
fn test_skip_recovery_same_count_different_content() {
    let mut composer = Composer::new();
    let content = std::cell::Cell::new(0u32);

    let build = |composer: &mut Composer, content: &std::cell::Cell<u32>| {
        composer.compose(|ctx| {
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    // 同位置单个 leaf（数量恒 1）——内容由 cell 控制（无 state 驱动）
                    let _ = content.get();
                    let k = ctx.next_key();
                    ctx.start_leaf(k, Modifier::new());
                    ctx.end_node();
                }
            }
            ctx.end_restartable_group();
        });
        composer.layout(crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0));
    };

    build(&mut composer, &content);
    // 帧2：内容 cell 变化但无 state notify → slot clean + 参数相同 → Skip（恢复）
    content.set(1);
    build(&mut composer, &content);
    // 关键断言：帧2 全 Skip（clean 计数 > 0）——同数量同位置保持恢复（Compose 语义）
    assert!(composer.compose_clean_count > 0, "数量相同内容不同应保持 Skip（clean_count={}）", composer.compose_clean_count);
}

// ═══════════════════════════════════════════════════════════
// app_root! 根入口宏——稳定 key 测试（T7）
// ═══════════════════════════════════════════════════════════

/// T7：app_root! 覆盖下，根闭包内组件调用点获得语句级稳定 key——
/// if 分支结构增删后同位置组件 key 不变、remember 状态保留。
#[test]
fn test_app_root_stable_keys_across_structure_change() {
    let mut composer = Composer::new();
    let show_holder = std::cell::RefCell::new(None::<State<bool>>);
    // B 组件的 key 记录（跨帧断言）
    let b_key = std::cell::Cell::new(None::<u64>);

    // 根入口用 app_root!（宏注入语句级 key——根闭包内调用点稳定）
    let root = crate::app_root!(|ctx: &mut ComposeCtx| {
        let show = ctx.remember(|| true);
        *show_holder.borrow_mut() = Some(show.clone());
        let root_key = ctx.next_key();
        match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
            crate::runtime::composer::GroupStatus::Skip => {}
            crate::runtime::composer::GroupStatus::Enter => {
                // A 组件（if 分支包裹——结构变化场景）
                if show.get() {
                    let k = ctx.next_key();
                    ctx.start_leaf(k, Modifier::new());
                    ctx.end_node();
                }
                // B 组件（始终存在——key 应跨结构变化稳定）
                let k2 = ctx.next_key();
                ctx.start_leaf(k2, Modifier::new().size(50.0, 20.0));
                ctx.end_node();
            }
        }
        ctx.end_restartable_group();
    });

    let build = |composer: &mut Composer| {
        composer.compose(|ctx| { root(ctx); });
        composer.layout(crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0));
        // 记录 B 组件（最后一个 leaf）的 slot_key
        let r = composer.layout_root_idx().unwrap();
        let children = composer.arena_nodes()[r].children.clone();
        let b = children[children.len() - 1];
        b_key.set(Some(composer.arena_nodes()[b].slot_key));
    };

    // 帧1：[A, B]
    build(&mut composer);
    let k1 = b_key.get().unwrap();
    // show 状态 id（跨帧保留断言）
    let show_id = show_holder.borrow().as_ref().unwrap().id();

    // 帧2：show=false → [B]（A 移除——结构变化）。
    // ⚠ seq 位置化（方案 B）：B 的 seq = 兄弟 index——A 移除后 B 从 index 1 变 0
    // → key 变 → B leaf 重建。这是结构变化（start 本身被跳过）的已知边界
    // （Compose 组栈位置同语义：结构变化 = 位置记忆重置）；B 无状态，重建无
    // 视觉影响。show 的 remember 在 if 外（根闭包首语句，index 恒 0）→ 仍稳定。
    show_holder.borrow().as_ref().unwrap().set(false);
    build(&mut composer);
    let k2 = b_key.get().unwrap();
    // B 节点仍存在（重建后 leaf）
    assert!(b_key.get().is_some(), "B 组件应存在（重建）");

    // 帧3：show=true → [A, B]（A 恢复——B index 回到 1）
    show_holder.borrow().as_ref().unwrap().set(true);
    build(&mut composer);
    let k3 = b_key.get().unwrap();
    assert_eq!(k1, k3, "A 恢复后 B 的 key 应回到帧1 值（index 回到 1）——key 由兄弟位置决定");

    // remember 状态（show）跨结构变化保留（同一 State id——根闭包首语句 index 稳定）
    assert_eq!(show_holder.borrow().as_ref().unwrap().id(), show_id,
        "remember 状态应跨结构变化保留（语句级 key 稳定）");
}

/// 回归（文档 1.3 原始 bug 的直接验证）：**条件分支内的 remember 增删不影响
/// 分支外语句的 remember**——语句 id 编译期固定 + per-base 独立计数：
/// if 分支（语句 1）的 remember 在自己的 base 下计数，a（语句 0）/c（语句 2）
/// 的 base 独立——show 切换平移 if 内序号，不触碰 a/c 的 key。
#[test]
fn test_conditional_branch_remember_does_not_drift_siblings() {
    let mut composer = Composer::new();
    let show = crate::runtime::state::State::new(true);
    let a_holder = std::cell::RefCell::new(None::<crate::runtime::state::State<i32>>);
    let c_holder = std::cell::RefCell::new(None::<crate::runtime::state::State<i32>>);

    let build = |composer: &mut Composer| {
        composer.compose(crate::compose!(|ctx| {
            let a = ctx.remember(|| 0i32); // 语句 0
            *a_holder.borrow_mut() = Some(a.clone());
            if show.get() {
                // 语句 1（if 注入）内 remember——show 切换时整个分支增删
                let _b = ctx.remember(|| 1i32);
            }
            let c = ctx.remember(|| 2i32); // 语句 2——独立 base
            *c_holder.borrow_mut() = Some(c.clone());
        }));
        };

    // 帧1：show=true——a/c + if 内 b 全部创建
    build(&mut composer);
    let a1 = a_holder.borrow().clone().unwrap().id();
    let c1 = c_holder.borrow().clone().unwrap().id();

    // 帧2：show=false——if 分支 remember 消失（结构变化）→ 平移 if 内序号
    show.set(false);
    build(&mut composer);
    let a2 = a_holder.borrow().clone().unwrap().id();
    let c2 = c_holder.borrow().clone().unwrap().id();
    assert_eq!(a1, a2, "a（语句 0）的 State 应跨 if 分支增删保留——跨语句不漂移");
    assert_eq!(c1, c2, "c（语句 2）的 State 应跨 if 分支增删保留——跨语句不漂移");

    // 帧3：show=true——b 恢复（新 State——结构变化 = 重置，符合语义）
    show.set(true);
    build(&mut composer);
    let a3 = a_holder.borrow().clone().unwrap().id();
    let c3 = c_holder.borrow().clone().unwrap().id();
    assert_eq!(a1, a3, "a 恢复帧仍应保留");
    assert_eq!(c1, c3, "c 恢复帧仍应保留");
}

/// 语义边界验证（非 bug——Compose 同语义）：**同语句内** remember 数量变化
/// → 序号平移 → 状态重置（可预测）。明确记录该行为，防止被误当漂移 bug 报。
#[test]
fn test_same_stmt_remember_count_change_resets() {
    let mut composer = Composer::new();
    let show_extra = crate::runtime::state::State::new(true);
    let first_holder = std::cell::RefCell::new(None::<crate::runtime::state::State<i32>>);

    let build = |composer: &mut Composer| {
        composer.compose(crate::compose!(|ctx| {
            if show_extra.get() {
                // 同语句内两个 remember——数量随 show_extra 变化
                let _x = ctx.remember(|| 1i32); // seq 0
                let y = ctx.remember(|| 2i32);  // seq 1
                *first_holder.borrow_mut() = Some(y.clone());
            } else {
                let y = ctx.remember(|| 2i32);  // 只剩一个——seq 0（原 x 的槽）
                *first_holder.borrow_mut() = Some(y.clone());
            }
        }));
        };

    build(&mut composer);
    let id1 = first_holder.borrow().clone().unwrap().id();
    // 同语句内 remember 数量 2→1 → 序号平移 → y 拿到原 x 的 key → State 重置
    show_extra.set(false);
    build(&mut composer);
    let id2 = first_holder.borrow().clone().unwrap().id();
    assert_ne!(id1, id2,
        "同语句内 remember 数量变化 = 序号平移 = 重置（Compose 语义，非漂移 bug——明确记录）");

}

mod derived_expression_tests {
    use super::*;
    use crate::layout::BoxLayout;
    use crate::layout::constraints::Constraints;
    use crate::modifier::Modifier;
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn a_derived_value_reenters_its_reader_only_when_it_changes() {
        let source = crate::runtime::state::State::new(0.0f32);
        let owner_enters = Rc::new(Cell::new(0usize));
        let reader_enters = Rc::new(Cell::new(0usize));
        let source_in = source.clone();
        let owner_in = owner_enters.clone();
        let reader_in = reader_enters.clone();

        let mut composer = Composer::new();
        let mut frame = |composer: &mut Composer| {
            let source = source_in.clone();
            let owner = owner_in.clone();
            let reader = reader_in.clone();
            composer.compose(move |ctx: &mut ComposeCtx| {
                let owner_key = ctx.next_key();
                match ctx.start_restartable_group(owner_key, Modifier::new(), BoxLayout::new()) {
                    GroupStatus::Skip => {}
                    GroupStatus::Enter => {
                        owner.set(owner.get() + 1);
                        // The derived expression, read in the owning scope.
                        let past = source.get() > 300.0;

                        // The reader: its only declared param is the DERIVED value.
                        let reader_key = ctx.next_key();
                        ctx.changed(&past);
                        match ctx.start_restartable_group(reader_key, Modifier::new(), BoxLayout::new()) {
                            GroupStatus::Skip => {}
                            GroupStatus::Enter => {
                                reader.set(reader.get() + 1);
                            }
                        }
                        ctx.end_restartable_group();
                    }
                }
                ctx.end_restartable_group();
            });
            // A group can only Skip when the previous frame left a cached subtree to restore.
            composer.layout(Constraints::new(0.0, 100.0, 0.0, 100.0));
        };

        frame(&mut composer);
        assert_eq!((owner_enters.get(), reader_enters.get()), (1, 1), "the first frame enters both");

        // CONTROL: an idle frame Skips both, so the counts below mean something.
        frame(&mut composer);
        assert_eq!((owner_enters.get(), reader_enters.get()), (1, 1), "CONTROL: an idle frame Skips");

        // The source moves three times and the derived value stays false.
        for value in [10.0f32, 120.0, 299.0] {
            source.set(value);
            frame(&mut composer);
        }
        assert_eq!(
            owner_enters.get(),
            4,
            "the scope that owns the READ re-runs for every source change — that is the cost of a \
             derived expression, and what a `derivedStateOf`-style state object would avoid"
        );
        assert_eq!(
            reader_enters.get(),
            1,
            "but the reader must NOT re-run: its declared param never changed"
        );

        // The derived value flips.
        source.set(301.0);
        frame(&mut composer);
        assert_eq!(reader_enters.get(), 2, "and it re-runs when the value it declared changes");
        assert_eq!(owner_enters.get(), 5);
    }

    /// The flip side of the same rule, and why every component declares its params: a group that
    /// declares nothing and reads nothing is NEVER re-entered, so it keeps what it composed with even
    /// when the value it was built from changed in the scope above it.
    #[test]
    fn a_group_that_declares_nothing_keeps_what_it_was_built_with() {
        let source = crate::runtime::state::State::new(0.0f32);
        let enters = Rc::new(Cell::new(0usize));
        let seen = Rc::new(std::cell::RefCell::new(Vec::new()));
        let source_in = source.clone();
        let enters_in = enters.clone();
        let seen_in = seen.clone();

        let mut composer = Composer::new();
        let mut frame = |composer: &mut Composer| {
            let source = source_in.clone();
            let enters = enters_in.clone();
            let seen = seen_in.clone();
            composer.compose(move |ctx: &mut ComposeCtx| {
                let value = source.get();
                let key = ctx.next_key();
                // NOTE: no `ctx.changed(&value)` — this is the mistake this test records.
                match ctx.start_restartable_group(key, Modifier::new(), BoxLayout::new()) {
                    GroupStatus::Skip => {}
                    GroupStatus::Enter => {
                        enters.set(enters.get() + 1);
                        seen.borrow_mut().push(value);
                    }
                }
                ctx.end_restartable_group();
            });
            composer.layout(Constraints::new(0.0, 100.0, 0.0, 100.0));
        };

        frame(&mut composer);
        source.set(42.0);
        frame(&mut composer);

        assert_eq!(
            seen.borrow().as_slice(),
            &[0.0],
            "a group with no declared params is not re-entered, so it keeps the value it composed \
             with — declaring what you read is what makes a component update"
        );
        assert_eq!(enters.get(), 1);
    }
}
