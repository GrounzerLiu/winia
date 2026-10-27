    use super::*;
    use crate::modifier::Modifier;

    /// The `SlotKeyMap` hasher must use the WHOLE key — and must not collapse a real key set.
    ///
    /// Both halves of this are performance properties, not correctness ones (a bad hasher still answers
    /// correctly, just slowly), which is why they need a test: nothing else in the suite would notice a
    /// "simplification" of the hasher, and the maps it serves are filled and read once per node per
    /// frame (`docs/benchmarks.md`).
    ///
    /// The first part is the sharp one, and it is the reason the hash folds the high half down before
    /// multiplying: a multiply propagates bits upward only, so the LOW bits of a product depend only on
    /// the low bits of its input — and the low bits are what hashbrown indexes with. Two keys that
    /// differ only in the high half therefore land in the same bucket under a bare multiply. `slot_key`s
    /// are pre-mixed by the composer, so today's key set rarely looks like that; the test guards the
    /// SHAPE of the hash rather than today's keys.
    ///
    /// The second part is deliberately loose, and the note is worth keeping: 800 keys in 256 buckets
    /// have a worst bucket around 9 even for a perfect hash (sha256 measures 9-10 on these patterns, the
    /// shipped hasher 8-9), so this half can only catch a total collapse — it is here to stop a hasher
    /// that ignores its input, not to rank candidates.
    #[test]
    fn slot_key_hasher_uses_the_whole_key_and_spreads_real_keys() {
        use std::hash::{BuildHasher, Hash, Hasher};

        fn mix_key(base: u64, c: u64) -> u64 {
            let mut h: u64 = 0xcbf29ce484222325;
            h ^= base;
            h = h.wrapping_mul(0x100000001b3);
            h ^= c;
            h = h.wrapping_mul(0x100000001b3);
            h
        }

        // A deterministic stand-in for the source hashes a composition produces.
        let mut seed = 0x243F6A8885A308D3u64;
        let mut next_base = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };

        let hasher = std::hash::BuildHasherDefault::<SlotKeyHasher>::default();
        let bucket = |k: u64| -> usize {
            let mut h = hasher.build_hasher();
            k.hash(&mut h);
            h.finish() as usize
        };

        // Part 1: the high half must reach the LOW bits, which is what a bucket index is made of.
        let base = next_base();
        // 256 pairs, each differing only in bit 40 — high enough that a multiply (which propagates
        // bits upward only) never brings it down, low enough that folding the top half down lands it
        // inside the window compared below.
        let collapsed = (0..256u64).filter(|i| {
            let low = mix_key(base, i + 1) & 0xFFFF_FFFF;
            (bucket(low) & 0xFF_FFFF) == (bucket(low | (1 << 40)) & 0xFF_FFFF)
        }).count();
        assert!(
            collapsed <= 1,
            "{collapsed} of 256 key pairs that differ only in the high half hashed into the same low \
             24 bits — the hasher is dropping the top of the key, which is what the index and the \
             control byte are both taken from (a bare multiply collapses all 256 of these, and a \
             pass-through the same; measured)"
        );

        let mut patterns: Vec<Vec<u64>> = Vec::new();
        // 800 rows, each its own composable call site.
        patterns.push((0..800).map(|_| mix_key(next_base(), 1)).collect());
        // One call site in a loop: one base, 800 sequential counters.
        patterns.push((1..=800).map(|c| mix_key(base, c)).collect());
        // Nested: the bases are themselves mixed keys.
        patterns.push((0..800).map(|i| mix_key(mix_key(next_base(), i), 1 + (i % 3))).collect());

        const BUCKETS: usize = 256;
        for (i, keys) in patterns.iter().enumerate() {
            // Precondition: these must be 800 DISTINCT keys, or the test would be measuring the
            // pattern's own collisions rather than the hasher's spreading (a first version of this
            // test did exactly that, on a pattern that mostly repeated itself).
            let distinct: std::collections::HashSet<u64> = keys.iter().copied().collect();
            assert_eq!(distinct.len(), keys.len(), "pattern {i} must have distinct keys");

            let mut counts = vec![0usize; BUCKETS];
            for k in keys {
                counts[bucket(*k) % BUCKETS] += 1;
            }
            let busy = *counts.iter().max().unwrap();
            assert!(
                busy <= keys.len() / 20,
                "pattern {i}: the busiest bucket holds {busy} of {} keys — the hasher is not \
                 spreading them at all (a perfect hash measures ~9 here, an input-ignoring one 800), \
                 which costs lookups, not correctness",
                keys.len()
            );
        }
    }

    #[test]
    fn test_hit_test_basic() {
        let mut nodes = vec![LayoutNode::leaf(Modifier::new().size(100.0, 100.0))];
        nodes[0].measured_size = Size::new(100.0, 100.0);
        nodes[0].position = Offset::new(0.0, 0.0);

        let path = hit_test(&nodes, 0, 50.0, 50.0);
        assert_eq!(path, vec![0]);
    }

    #[test]
    fn a_hidden_sibling_does_not_take_the_hit() {
        // Modifier.visible(false): not painted, so not hit either — the sibling underneath answers.
        let mut nodes = vec![
            LayoutNode::leaf(Modifier::new().size(200.0, 200.0)),
            LayoutNode::leaf(Modifier::new().size(100.0, 100.0)),
            LayoutNode::leaf(Modifier::new().size(40.0, 40.0).visible(false)),
        ];
        nodes[0].measured_size = Size::new(200.0, 200.0);
        nodes[1].measured_size = Size::new(100.0, 100.0);
        nodes[2].measured_size = Size::new(40.0, 40.0);
        nodes[2].position = Offset::new(30.0, 30.0);
        nodes[0].children = vec![1, 2];

        let path = hit_test(&nodes, 0, 40.0, 40.0);
        assert_eq!(path, vec![0, 1], "the hidden top sibling is skipped: {path:?}");
    }

    #[test]
    fn test_hit_test_z_order_topmost_first() {
        // Siblings paint in children order (later paints above), so the hit test must prefer the
        // later one: leaf1 fills the box, leaf2 is a smaller rectangle on top, and their overlap must
        // hit leaf2.
        let mut nodes = vec![
            LayoutNode::leaf(Modifier::new().size(200.0, 200.0)),
            LayoutNode::leaf(Modifier::new().size(100.0, 100.0)),
            LayoutNode::leaf(Modifier::new().size(40.0, 40.0)),
        ];
        nodes[0].measured_size = Size::new(200.0, 200.0);
        nodes[1].measured_size = Size::new(100.0, 100.0);
        nodes[2].measured_size = Size::new(40.0, 40.0);
        nodes[1].position = Offset::new(0.0, 0.0);
        nodes[2].position = Offset::new(30.0, 30.0);
        nodes[0].children = vec![1, 2];

        // (40, 40) is inside both, and the upper one (leaf2) wins.
        let path = hit_test(&nodes, 0, 40.0, 40.0);
        assert_eq!(path, vec![0, 2], "the upper sibling wins the hit (z-order)");
        // (20, 20) is only inside leaf1 (outside leaf2).
        let path2 = hit_test(&nodes, 0, 20.0, 20.0);
        assert_eq!(path2, vec![0, 1]);
    }

    #[test]
    fn test_hit_test_miss() {
        let mut nodes = vec![LayoutNode::leaf(Modifier::new().size(100.0, 100.0))];
        nodes[0].measured_size = Size::new(100.0, 100.0);

        let path = hit_test(&nodes, 0, 150.0, 50.0);
        assert_eq!(path.len(), 0);
    }

    /// `Modifier::z_index` reorders the paint order, and the hit test walks that order in reverse: a
    /// child raised above its sibling receives the press even though it is composed FIRST.
    #[test]
    fn test_hit_test_follows_z_index() {
        let mut nodes = vec![
            LayoutNode::leaf(Modifier::new().size(200.0, 200.0)),
            // Composed first (bottom by tree order), but raised above its sibling.
            LayoutNode::leaf(Modifier::new().size(100.0, 100.0).z_index(1.0)),
            LayoutNode::leaf(Modifier::new().size(100.0, 100.0)),
        ];
        nodes[0].measured_size = Size::new(200.0, 200.0);
        nodes[1].measured_size = Size::new(100.0, 100.0);
        nodes[2].measured_size = Size::new(100.0, 100.0);
        nodes[1].position = Offset::new(0.0, 0.0);
        nodes[2].position = Offset::new(0.0, 0.0);
        nodes[0].children = vec![1, 2];
        nodes[0].children_have_z = true;

        // (40,40) is inside both; the raised sibling wins over the later-composed one.
        assert_eq!(hit_test(&nodes, 0, 40.0, 40.0), vec![0, 1], "the raised sibling receives the press");
        // ...and outside the raised sibling's own box the other one still gets it.
        nodes[1].measured_size = Size::new(20.0, 20.0);
        assert_eq!(hit_test(&nodes, 0, 40.0, 40.0), vec![0, 2], "outside the raised box, the sibling does");
    }

    /// Without any z the order is plain tree order (the fast path the renderer takes): the LATER child
    /// is the topmost one, exactly as before this feature existed.
    #[test]
    fn test_hit_test_without_z_keeps_tree_order() {
        let mut nodes = vec![
            LayoutNode::leaf(Modifier::new().size(200.0, 200.0)),
            LayoutNode::leaf(Modifier::new().size(100.0, 100.0)),
            LayoutNode::leaf(Modifier::new().size(100.0, 100.0)),
        ];
        nodes[0].measured_size = Size::new(200.0, 200.0);
        nodes[1].measured_size = Size::new(100.0, 100.0);
        nodes[2].measured_size = Size::new(100.0, 100.0);
        nodes[0].children = vec![1, 2];
        assert!(!nodes[0].children_have_z, "nothing sets a z here");
        assert_eq!(hit_test(&nodes, 0, 40.0, 40.0), vec![0, 2]);
    }

    /// `paint_order` is the shared ordering rule: stable (equal z keeps tree order) and total (a NaN z
    /// compares as equal rather than panicking, so a broken value cannot take the frame down).
    #[test]
    fn paint_order_is_stable_and_total() {
        let mut nodes = vec![
            LayoutNode::leaf(Modifier::new()),
            LayoutNode::leaf(Modifier::new().z_index(1.0)),
            LayoutNode::leaf(Modifier::new()),
            LayoutNode::leaf(Modifier::new().z_index(-1.0)),
            LayoutNode::leaf(Modifier::new().z_index(f32::NAN)),
        ];
        assert_eq!(paint_order(&nodes, &[1, 2, 3, 4]), vec![3, 2, 1, 4], "z ascending, ties in tree order");
        nodes[4].modifier = Modifier::new().z_index(1.0);
        assert_eq!(paint_order(&nodes, &[1, 4]), vec![1, 4], "equal z keeps tree order");
    }
    #[test]
    fn test_scroll_offset_rtl_reverse_mirrors() {
        // RTL (scroll_reverse) regression: the hit test must use the render pass's mirrored offset
        // off = content_w - viewport_w - offset, not the raw offset — otherwise clicks on a scrolled
        // RTL tab row land in the wrong place (a bug reported from real use).
        let state = crate::modifier::ScrollState::new();
        let m = Modifier::new().horizontal_scroll(state.clone());
        let mut node = LayoutNode::leaf(m);
        node.scroll_reverse = true;
        node.scroll_content_width = 1000.0;
        node.scroll_viewport_width = 360.0;

        // offset = 0 maps to 1000 - 360 - 0 = 640 (the render pass shows the end of the content).
        state.offset.set(0.0);
        let (dx, dy) = scroll_offset_for_node(&node);
        assert!((dx - 640.0).abs() < 0.01, "RTL offset 0 mirrors to 640, got {dx}");
        assert_eq!(dy, 0.0);

        // offset=100 → 640-100 = 540
        state.offset.set(100.0);
        let (dx, _) = scroll_offset_for_node(&node);
        assert!((dx - 540.0).abs() < 0.01, "RTL offset 100 mirrors to 540, got {dx}");

        // The LTR control (not reversed): the offset is the raw one.
        node.scroll_reverse = false;
        state.offset.set(100.0);
        let (dx, _) = scroll_offset_for_node(&node);
        assert!((dx - 100.0).abs() < 0.01, "LTR uses the raw offset 100, got {dx}");
    }

    #[test]
    fn test_hit_test_child() {
        let mut nodes = vec![
            LayoutNode::leaf(Modifier::new().size(100.0, 100.0)),
            LayoutNode::leaf(Modifier::new().size(50.0, 30.0)),
        ];
        nodes[0].measured_size = Size::new(100.0, 100.0);
        nodes[1].measured_size = Size::new(50.0, 30.0);
        nodes[1].position = Offset::new(10.0, 60.0);
        nodes[0].children.push(1);

        // A click on the child
        let path = hit_test(&nodes, 0, 30.0, 75.0);
        assert_eq!(path, vec![0, 1], "should hit parent and child");
    }

    #[test]
    fn test_hit_test_child_miss() {
        let mut nodes = vec![
            LayoutNode::leaf(Modifier::new().size(100.0, 100.0)),
            LayoutNode::leaf(Modifier::new().size(50.0, 30.0)),
        ];
        nodes[0].measured_size = Size::new(100.0, 100.0);
        nodes[1].measured_size = Size::new(50.0, 30.0);
        nodes[1].position = Offset::new(10.0, 60.0);
        nodes[0].children.push(1);

        // A click on the parent, outside the child's range
        let path = hit_test(&nodes, 0, 80.0, 75.0);
        assert_eq!(path, vec![0], "should only hit parent");
    }

    /// Regression: scene coordinates (the hit test's input) and `node_abs_position` (what app.rs uses
    /// to place a click or drag) must live in the same space — both subtract the ancestors' scroll
    /// offsets. `node_abs_position` used to accumulate the layout position alone, so after a scroll
    /// its local y carried the scroll amount and a multi-line TextField could only be hit on its
    /// first line.
    #[test]
    fn test_scroll_scene_coords_consistent() {
        // A scroll container at y = 0 scrolled by 50 holds a child at y = 100 (content 60 high).
        let scroll = crate::modifier::ScrollState::new();
        scroll.offset.set(50.0);
        let mut nodes = vec![
            LayoutNode::leaf(Modifier::new().vertical_scroll(scroll.clone()).size(100.0, 200.0)),
            LayoutNode::leaf(Modifier::new().size(100.0, 60.0)),
        ];
        nodes[0].measured_size = Size::new(100.0, 200.0);
        nodes[1].measured_size = Size::new(100.0, 60.0);
        nodes[1].position = Offset::new(0.0, 100.0);
        nodes[0].children.push(1);

        // The render pass translates by (0, -50), so the child's visual top is at scene y = 50.
        let path = hit_test(&nodes, 0, 50.0, 60.0);
        assert_eq!(path, vec![0, 1], "the child's visual range (50,50)-(50,110) is hit after the scroll");
        // Local = scene - visual top = (50-0, 60-50) = (50, 10).
        let (x, y) = scene_to_node_local(&nodes, &path, 1, 50.0, 60.0);
        assert_eq!((x, y), (50.0, 10.0), "the local coordinates subtract the scroll offset");

        // An unscrolled point (y = 140 maps to scene 90, still the child's local y = 40).
        let path2 = hit_test(&nodes, 0, 50.0, 90.0);
        assert_eq!(path2, vec![0, 1]);
        let (_, y2) = scene_to_node_local(&nodes, &path2, 1, 50.0, 90.0);
        assert_eq!(y2, 40.0);

        // Changing the scroll offset changes the local coordinates of the same scene point (the
        // offset has been consumed). offset 50: visual top 50, scene (50,50) -> local y 0.
        // offset 100: visual top 0, scene (50,50) -> local y 50, still inside the child.
        scroll.offset.set(100.0);
        let path3 = hit_test(&nodes, 0, 50.0, 50.0);
        assert_eq!(path3, vec![0, 1]);
        let (_, y3) = scene_to_node_local(&nodes, &path3, 1, 50.0, 50.0);
        assert_eq!(y3, 50.0, "the local coordinates follow the scroll offset");
    }

    // ── scene_to_node_local (the ripple press point's local coordinates) ──

    #[test]
    fn scene_to_node_local_plain() {
        // root(10,20) -> child(30,40): the visual origin is (40,60).
        let mut nodes = vec![
            LayoutNode::leaf(Modifier::new().size(100.0, 100.0)),
            LayoutNode::leaf(Modifier::new().size(50.0, 50.0)),
        ];
        nodes[0].position = Offset::new(10.0, 20.0);
        nodes[1].position = Offset::new(30.0, 40.0);
        nodes[0].children.push(1);

        let (lx, ly) = scene_to_node_local(&nodes, &[0, 1], 1, 45.0, 62.0);
        assert_eq!((lx, ly), (5.0, 2.0), "no scroll: local = scene - layout origin");
    }

    #[test]
    fn scene_to_node_local_under_scroll() {
        // root(0,0) -> scroll container(50,100, offset 50) -> child(100,0): the visual origin is
        // (50+100-0, 100+0-50) = (150, 50).
        let scroll = crate::modifier::ScrollState::new();
        scroll.offset.set(50.0);
        let mut nodes = vec![
            LayoutNode::leaf(Modifier::new().size(300.0, 300.0)),
            LayoutNode::leaf(Modifier::new().size(200.0, 200.0).vertical_scroll(scroll)),
            LayoutNode::leaf(Modifier::new().size(50.0, 50.0)),
        ];
        nodes[0].position = Offset::new(0.0, 0.0);
        nodes[1].position = Offset::new(50.0, 100.0);
        nodes[2].position = Offset::new(100.0, 0.0);
        nodes[0].children.push(1);
        nodes[1].children.push(2);

        let (lx, ly) = scene_to_node_local(&nodes, &[0, 1, 2], 2, 160.0, 60.0);
        assert_eq!((lx, ly), (10.0, 10.0), "the scroll offset comes off the visual origin, as in hit_test and render");
    }

    #[test]
    fn collect_focus_candidates_uses_visual_centers() {
        // root(0,0) -> scroll container(50,100, offset 50) -> focusable(100,0, 20x20)
        let scroll = crate::modifier::ScrollState::new();
        scroll.offset.set(50.0);
        let mut nodes = vec![
            LayoutNode::leaf(Modifier::new().size(300.0, 300.0)),
            LayoutNode::leaf(Modifier::new().size(200.0, 200.0).vertical_scroll(scroll)),
            LayoutNode::leaf(Modifier::new().size(20.0, 20.0).focusable()),
        ];
        nodes[0].position = Offset::new(0.0, 0.0);
        nodes[1].position = Offset::new(50.0, 100.0);
        nodes[2].position = Offset::new(100.0, 0.0);
        // These tests do not measure, so the sizes are set by hand (as in the scene_to_node_local series).
        nodes[0].measured_size = Size::new(300.0, 300.0);
        nodes[1].measured_size = Size::new(200.0, 200.0);
        nodes[2].measured_size = Size::new(20.0, 20.0);
        nodes[0].children.push(1);
        nodes[1].children.push(2);

        let cands = collect_focus_candidates(&nodes, 0);
        assert_eq!(cands.len(), 1, "only the focusable child is a candidate");
        // Visual position (150, 50), centre (160, 60) — the scroll offset is already removed.
        assert_eq!(cands[0].0, nodes[2].id);
        assert_eq!((cands[0].1, cands[0].2), (160.0, 60.0));
    }

    // ── aspectRatio / requiredSize ──

    #[test]
    fn required_size_overrides_incoming() {
        // Parent 100x100, requiredSize(200, 50) overflows it (enforceIncoming = false).
        let m = Modifier::new().required_size(200.0, 50.0);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 100.0, 0.0, 100.0));
        assert_eq!((size.width, size.height), (200.0, 50.0), "required ignores the incoming shrink");
    }

    #[test]
    fn required_width_only() {
        let m = Modifier::new().required_width(300.0);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 100.0, 0.0, 100.0));
        assert_eq!(size.width, 300.0, "requiredWidth overflows");
        assert!(size.height <= 100.0, "the height still obeys the incoming constraint");
    }

    #[test]
    fn aspect_ratio_derives_from_max_width() {
        // No size constraint: max 300x300 with ratio 2 gives 300 wide and 150 high.
        let m = Modifier::new().aspect_ratio(2.0, false);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 300.0, 0.0, 300.0));
        assert_eq!((size.width, size.height), (300.0, 150.0), "derived from max width");
    }

    #[test]
    fn aspect_ratio_respects_inner_fixed_size() {
        // The chain's `size(50, 20)` is tight (min == max), so no aspect candidate fits: the
        // unenforced 50x25 is chosen and then clamped back into the tight range, giving 50x20. The
        // NODE size matches Compose; the content differs — Compose measures the child at 50x25 and
        // lets the outer `size` node shrink it to 50x20, where winia hands `fixed(50, 20)` to the
        // child directly (see the differences table in docs/box.md).
        let m = Modifier::new().size(50.0, 20.0).aspect_ratio(2.0, false);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 300.0, 0.0, 300.0));
        assert_eq!((size.width, size.height), (50.0, 20.0), "a tight size leaves aspect no room (as in Compose)");
    }

    #[test]
    fn aspect_ratio_clamps_to_max_height() {
        // No size: max 300x100 with ratio 2 gives h = 150, over the 100 limit, so (200, 100).
        let m = Modifier::new().aspect_ratio(2.0, false);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 300.0, 0.0, 100.0));
        assert_eq!((size.width, size.height), (200.0, 100.0), "the width follows from the height limit");
    }

    #[test]
    fn aspect_ratio_match_height_first() {
        // match_height_first starts from max height: 100x300 with ratio 2.
        // → h = 300，w = 600 > 100 → clamp：(100, 50)
        let m = Modifier::new().aspect_ratio(2.0, true);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 100.0, 0.0, 300.0));
        assert_eq!((size.width, size.height), (100.0, 50.0), "match_height_first derives it");
    }

    /// Compose's `findSize` has FOUR candidates on each pass; with no bounded maximum the
    /// minimum candidates still produce a size (`AspectRatio.kt:196-222`). The old derivation
    /// gave up when both maxima were unbounded and the ratio was ignored.
    #[test]
    fn aspect_ratio_falls_back_to_the_minimum_when_both_maxima_are_unbounded() {
        let m = Modifier::new().min_width(200.0).aspect_ratio(2.0, false);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::UNBOUNDED);
        assert_eq!(
            (size.width, size.height),
            (200.0, 100.0),
            "the minimum-width candidate answers when nothing is bounded"
        );
    }

    /// `wrapContentWidth` outside `aspectRatio`: the wrap node relaxes the incoming minimum
    /// for the aspect node inside it, which then derives from the relaxed range; the wrap node
    /// reports the content coerced back into the incoming range (`Size.kt:1029-1061`).
    #[test]
    fn wrap_content_outside_an_aspect_ratio_relaxes_then_restores_the_range() {
        let m = Modifier::new()
            .wrap_content_width(Alignment::Start, false)
            .aspect_ratio(2.0, false);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 300.0, 0.0, 300.0));
        assert_eq!((size.width, size.height), (300.0, 150.0));
    }

    /// `aspectRatio` outside `wrapContentWidth`: the aspect node fixes the range first, so the
    /// wrap node's recorded range is that fixed one; its relaxed measurement lets the content be
    /// smaller, and the wrapper puts the node back at the aspect size, centring the content.
    #[test]
    fn an_aspect_ratio_outside_wrap_content_keeps_its_box_and_centres_the_content() {
        use crate::runtime::composer::Composer;
        let mut composer = Composer::new();
        composer.compose(crate::compose!(|ctx| {
            crate::layout::Stack::new()
                .modifier(
                    Modifier::new()
                        .aspect_ratio(2.0, false)
                        .wrap_content_width(Alignment::Center, false),
                )
                .build(ctx, |ctx| {
                    crate::components::Text::new("content")
                        .modifier(Modifier::new().size(100.0, 50.0))
                        .build(ctx);
                });
        }));
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
        let root = composer.arena.root.expect("root");
        assert_eq!(
            composer.arena_nodes()[root].measured_size,
            Size::new(300.0, 150.0),
            "the aspect box survives the wrap"
        );
        assert_eq!(
            composer.arena_nodes()[composer.arena_nodes()[root].children[0]].position.x,
            100.0,
            "and the 100 dp content is centred inside the 300 dp box"
        );
    }

    /// Padding is a CHAIN STEP: Compose's padding node deflates the constraints where it stands
    /// (`foundation/layout/Padding.kt`), so a size fixed INSIDE the padding is clamped into the
    /// already-deflated range and the node adds the padding on top — 300 inside 8 dp of padding is
    /// a 316 dp node.
    #[test]
    fn padding_outside_a_size_deflates_before_the_size_is_fixed() {
        let m = Modifier::new().padding(8.0).size(300.0, 120.0);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 400.0, 0.0, 400.0));
        assert_eq!((size.width, size.height), (316.0, 136.0));
    }

    /// The other order absorbs the padding: the size is fixed first, then the padding deflates what
    /// is left, and the 8 dp comes out of the 300 rather than on top of it.
    #[test]
    fn a_size_outside_padding_absorbs_the_padding() {
        let m = Modifier::new().size(300.0, 120.0).padding(8.0);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 400.0, 0.0, 400.0));
        assert_eq!((size.width, size.height), (300.0, 120.0));
    }

    /// The combination the chain-order fix exists for: the aspect ratio now derives from the
    /// DEFLATED range, as Compose's padding node would leave it (284 x 142), and the node adds the
    /// padding back (300 x 158). Before the padding step it derived 300x150 and put the content in
    /// 284x134.
    #[test]
    fn padding_outside_an_aspect_ratio_derives_from_the_deflated_range() {
        let m = Modifier::new().padding(8.0).aspect_ratio(2.0, false);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 300.0, 0.0, 300.0));
        assert_eq!((size.width, size.height), (300.0, 158.0));
    }

    /// An `IntrinsicSize` request and padding on ONE chain count the padding once: the request
    /// answers the PADDED box, and the engine adds the padding to the outer size itself. Measured
    /// before the fix: 104 (content + 2 x padding) instead of 80.
    #[test]
    fn an_intrinsic_width_request_counts_its_padding_once() {
        use crate::layout::IntrinsicSize;
        #[derive(Debug)]
        struct FiftySix;
        impl crate::layout::node::MeasurePolicy for FiftySix {
            fn measure(
                &self,
                _n: &mut Vec<LayoutNode>,
                _p: &[Box<dyn crate::layout::node::MeasurePolicy>],
                _c: &[usize],
                _constraints: Constraints,
            ) -> (Size, Vec<Placement>) {
                (Size::new(56.0, 20.0), Vec::new())
            }
            fn place(&self, _n: &mut Vec<LayoutNode>, _c: &[usize], _p: &[Placement]) {}
        }
        let policies: Vec<Box<dyn crate::layout::node::MeasurePolicy>> = vec![Box::new(FiftySix)];
        let m = Modifier::new().padding_horizontal(12.0).width(IntrinsicSize::Max);
        let mut nodes = vec![LayoutNode::new(m, Some(0))];
        let (size, _) = measure_node(&mut nodes, &policies, 0, Constraints::new(0.0, 400.0, 0.0, 400.0));
        assert_eq!(size.width, 80.0, "56 dp of content plus 24 dp of padding, counted once");
    }

    /// The wrap node sits INSIDE the padding, so its recorded range is the inner one; the wrapper is
    /// clamped in that space and the padding is added back. Clamping the padded size against the
    /// inner range lost the padding: 392 instead of 396.
    #[test]
    fn wrap_content_outside_padding_clamps_the_inner_size() {
        // padding(outer) → wrap → size(inner): the wrap records the deflated range (0..384), the
        // size fixes 380 inside it, and the node reports 380 + 16 = 396.
        let m = Modifier::new()
            .padding(8.0)
            .wrap_content_width(Alignment::Start, false)
            .size(380.0, 20.0);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 400.0, 0.0, 400.0));
        assert_eq!(size.width, 396.0, "380 of content plus the border");

        // A tight parent: the inner range is 276, so the wrapper is 276 and the node stays at 300.
        let m = Modifier::new()
            .padding(12.0)
            .wrap_content_width(Alignment::Start, false)
            .required_size(380.0, 20.0);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::fixed(300.0, 300.0));
        assert_eq!(size.width, 300.0, "the padded node stays at the tight size");
    }

    /// A consume-only element neutralises a padding element AFTER it, because the walk resolves
    /// them in chain order. Before insets became steps, every padding element was resolved before
    /// every consume-only one, so the first order padded by the inset too.
    #[test]
    fn a_consume_before_an_inset_padding_neutralises_it() {
        let _lock = crate::window::insets::INSET_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let controller = crate::window::insets::current();
        struct ResetInsets(crate::window::insets::WindowInsetsController);
        impl Drop for ResetInsets {
            fn drop(&mut self) {
                self.0.set(crate::window::insets::WindowInsets::default());
            }
        }
        let _reset = ResetInsets(controller.clone());
        controller.set(crate::window::insets::WindowInsets {
            status_bars: crate::window::insets::Insets::new(0.0, 24.0, 0.0, 0.0),
            ..Default::default()
        });

        let consume_first = Modifier::new()
            .consume_window_insets(crate::window::insets::InsetsKind::StatusBars)
            .status_bars_padding()
            .size(10.0, 10.0);
        let mut nodes = vec![LayoutNode::leaf(consume_first)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 200.0, 0.0, 200.0));
        assert_eq!(size, Size::new(10.0, 10.0), "the inset was consumed before the padding element");

        let pad_first = Modifier::new()
            .status_bars_padding()
            .consume_window_insets(crate::window::insets::InsetsKind::StatusBars)
            .size(10.0, 10.0);
        let mut nodes = vec![LayoutNode::leaf(pad_first)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 200.0, 0.0, 200.0));
        assert_eq!(size, Size::new(10.0, 34.0), "and the reverse order still pads");
    }

    /// The intrinsic probe's CROSS axis is deflated once: the walk already shrank the range by the
    /// chain's padding, so the engine hands the probe an outer-space value and the probe's own
    /// subtraction cancels. Before this the content was asked at 400 - 2 * 8.
    #[test]
    fn an_intrinsic_height_probe_asks_the_content_at_the_deflated_width() {
        use crate::layout::IntrinsicSize;
        #[derive(Debug)]
        struct WidthEcho;
        impl crate::layout::node::MeasurePolicy for WidthEcho {
            fn measure(
                &self,
                _n: &mut Vec<LayoutNode>,
                _p: &[Box<dyn crate::layout::node::MeasurePolicy>],
                _c: &[usize],
                constraints: Constraints,
            ) -> (Size, Vec<Placement>) {
                let w = constraints.max_width;
                (Size::new(w, w * 0.5), Vec::new())
            }
            fn place(&self, _n: &mut Vec<LayoutNode>, _c: &[usize], _p: &[Placement]) {}
            fn max_intrinsic_height(
                &self,
                _c: &mut crate::layout::node::IntrinsicCtx<'_>,
                _n: &[usize],
                width: f32,
            ) -> f32 {
                width * 0.5
            }
            fn min_intrinsic_height(
                &self,
                _c: &mut crate::layout::node::IntrinsicCtx<'_>,
                _n: &[usize],
                width: f32,
            ) -> f32 {
                width * 0.5
            }
        }
        let policies: Vec<Box<dyn crate::layout::node::MeasurePolicy>> = vec![Box::new(WidthEcho)];
        let m = Modifier::new().padding_horizontal(8.0).height(IntrinsicSize::Max);
        let mut nodes = vec![LayoutNode::new(m, Some(0))];
        let (size, _) = measure_node(&mut nodes, &policies, 0, Constraints::new(0.0, 400.0, 0.0, 400.0));
        assert_eq!(size.height, 192.0, "the content was asked at 384, so half of it is 192");
    }

    /// A contradictory range (`min_width` above `max_width`) resolves by CHAIN ORDER, like every
    /// other conflict: the outer node wins, so the answer is the floor when the floor is outer. Both
    /// paths agree (the walk applies the outer `clamp` last), and neither panics.
    #[test]
    fn a_contradictory_min_and_max_resolves_by_chain_order() {
        #[derive(Debug)]
        struct Nothing;
        impl crate::layout::node::MeasurePolicy for Nothing {
            fn measure(
                &self,
                _n: &mut Vec<LayoutNode>,
                _p: &[Box<dyn crate::layout::node::MeasurePolicy>],
                _c: &[usize],
                _constraints: Constraints,
            ) -> (Size, Vec<Placement>) {
                (Size::new(0.0, 0.0), Vec::new())
            }
            fn place(&self, _n: &mut Vec<LayoutNode>, _c: &[usize], _p: &[Placement]) {}
        }
        let policies: Vec<Box<dyn crate::layout::node::MeasurePolicy>> = vec![Box::new(Nothing)];
        let floor_outer = Modifier::new().min_width(300.0).max_width(200.0);
        let mut nodes = vec![LayoutNode::new(floor_outer, Some(0))];
        assert_eq!(
            crate::layout::node::intrinsic_size_of(
                &mut nodes,
                &policies,
                0,
                crate::layout::IntrinsicQuery::MaxWidth,
                100.0
            ),
            300.0,
            "the floor is the outer node, so it wins"
        );
        let cap_outer = Modifier::new().max_width(200.0).min_width(300.0);
        let mut nodes = vec![LayoutNode::new(cap_outer, Some(0))];
        assert_eq!(
            crate::layout::node::intrinsic_size_of(
                &mut nodes,
                &policies,
                0,
                crate::layout::IntrinsicQuery::MaxWidth,
                100.0
            ),
            200.0,
            "and the cap wins when it is outer"
        );
    }

    /// `defaultMinSize` is a node in the chain, so its floor applies to the box it owns: outside
    /// the padding it is a 58 dp box, inside it the padding is added on top (74).
    #[test]
    fn padding_and_default_min_size_answer_in_chain_order() {
        #[derive(Debug)]
        struct Nothing;
        impl crate::layout::node::MeasurePolicy for Nothing {
            fn measure(
                &self,
                _n: &mut Vec<LayoutNode>,
                _p: &[Box<dyn crate::layout::node::MeasurePolicy>],
                _c: &[usize],
                _constraints: Constraints,
            ) -> (Size, Vec<Placement>) {
                (Size::new(0.0, 0.0), Vec::new())
            }
            fn place(&self, _n: &mut Vec<LayoutNode>, _c: &[usize], _p: &[Placement]) {}
        }
        let policies: Vec<Box<dyn crate::layout::node::MeasurePolicy>> = vec![Box::new(Nothing)];
        let outside = Modifier::new().default_min_size(58.0, 40.0).padding(8.0);
        let mut nodes = vec![LayoutNode::new(outside, Some(0))];
        assert_eq!(
            crate::layout::node::intrinsic_size_of(&mut nodes, &policies, 0, crate::layout::IntrinsicQuery::MaxWidth, 100.0),
            58.0,
            "the floor is the box; the padding is inside it"
        );
        let inside = Modifier::new().padding(8.0).default_min_size(58.0, 40.0);
        let mut nodes = vec![LayoutNode::new(inside, Some(0))];
        assert_eq!(
            crate::layout::node::intrinsic_size_of(&mut nodes, &policies, 0, crate::layout::IntrinsicQuery::MaxWidth, 100.0),
            74.0,
            "the floor applies to the content, so the padding is added on top"
        );
    }

    /// The padding replay past a cap: with the padding OUTSIDE a 100 dp maximum the answer is
    /// 100 + 16; with it inside, the 100 caps the box and the padding comes out of it.
    #[test]
    fn padding_and_max_width_answer_in_chain_order() {
        #[derive(Debug)]
        struct Hundred;
        impl crate::layout::node::MeasurePolicy for Hundred {
            fn measure(
                &self,
                _n: &mut Vec<LayoutNode>,
                _p: &[Box<dyn crate::layout::node::MeasurePolicy>],
                _c: &[usize],
                _constraints: Constraints,
            ) -> (Size, Vec<Placement>) {
                (Size::new(100.0, 20.0), Vec::new())
            }
            fn place(&self, _n: &mut Vec<LayoutNode>, _c: &[usize], _p: &[Placement]) {}
        }
        let policies: Vec<Box<dyn crate::layout::node::MeasurePolicy>> = vec![Box::new(Hundred)];
        let outside = Modifier::new().max_width(100.0).padding(8.0);
        let mut nodes = vec![LayoutNode::new(outside, Some(0))];
        assert_eq!(
            crate::layout::node::intrinsic_size_of(&mut nodes, &policies, 0, crate::layout::IntrinsicQuery::MaxWidth, 100.0),
            100.0,
            "the cap is the box; the padding is inside it"
        );
        let inside = Modifier::new().padding(8.0).max_width(100.0);
        let mut nodes = vec![LayoutNode::new(inside, Some(0))];
        assert_eq!(
            crate::layout::node::intrinsic_size_of(&mut nodes, &policies, 0, crate::layout::IntrinsicQuery::MaxWidth, 100.0),
            116.0,
            "the cap applies to the content, so the padding is added on top"
        );
    }

    /// Window insets are chain steps too: a size fixed INSIDE the inset is clamped into the
    /// deflated range and the node grows by the inset, while a size OUTSIDE it absorbs the inset.
    #[test]
    fn window_inset_padding_is_a_chain_step() {
        let _lock = crate::window::insets::INSET_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let controller = crate::window::insets::current();
        struct ResetInsets(crate::window::insets::WindowInsetsController);
        impl Drop for ResetInsets {
            fn drop(&mut self) {
                self.0.set(crate::window::insets::WindowInsets::default());
            }
        }
        let _reset = ResetInsets(controller.clone());
        controller.set(crate::window::insets::WindowInsets {
            status_bars: crate::window::insets::Insets::new(0.0, 24.0, 0.0, 0.0),
            ..Default::default()
        });

        // inset OUTSIDE the size: the size fixes inside the 24 dp shorter range, the node is 72 tall.
        let m = Modifier::new().status_bars_padding().size(200.0, 48.0);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 400.0, 0.0, 400.0));
        assert_eq!((size.width, size.height), (200.0, 72.0));

        // size OUTSIDE the inset: the 48 dp is fixed first and the inset comes out of it.
        let m = Modifier::new().size(200.0, 48.0).status_bars_padding();
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 400.0, 0.0, 400.0));
        assert_eq!((size.width, size.height), (200.0, 48.0));
    }

    /// The UNENFORCED pass: when no candidate satisfies the range, Compose still returns one
    /// ("the constraints will not be respected", `AspectRatio.kt:43-47`). winia then CLAMPS it
    /// (the derivation feeds `Constraints::fixed`, where Compose has an extra node), so the
    /// height lands on the range instead of on the unenforced 62.5.
    #[test]
    fn aspect_ratio_falls_back_to_an_unsatisfied_candidate_before_giving_up() {
        let m = Modifier::new()
            .width_in(250.0, 250.0)
            .height_in(55.0, 60.0)
            .aspect_ratio(4.0, false);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::UNBOUNDED);
        assert_eq!((size.width, size.height), (250.0, 60.0));
    }

    // ── minWidth / minHeight (Compose's widthIn/heightIn) ──

    #[test]
    fn min_width_height_raises_min_constraints() {
        // min_width(58).min_height(40) with empty content grows to 58x40 (Button's default minimum).
        let m = Modifier::new().min_width(58.0).min_height(40.0);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 100.0, 0.0, 100.0));
        assert_eq!((size.width, size.height), (58.0, 40.0), "the minimum raises the incoming min");
    }

    #[test]
    fn min_width_yields_to_tight_size() {
        // After a tight size(30,20) the min(58) is capped by max = 30, as Compose merges constraints.
        let m = Modifier::new().size(30.0, 20.0).min_width(58.0);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 100.0, 0.0, 100.0));
        assert_eq!((size.width, size.height), (30.0, 20.0), "the minimum must not cross the tight max");
    }

    #[test]
    fn min_width_dynamic_state() {
        // A dynamic min (animation): the measure-time get() registers a layout dependency, so a change re-measures.
        use crate::runtime::state::State;
        let s = State::new(58.0);
        let m = Modifier::new().min_width(&s);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 100.0, 0.0, 100.0));
        assert_eq!(size.width, 58.0);
        s.set(70.0);
        // Simulate the layout invalidation an animation notify causes (composer pending -> layout_dirty).
        nodes[0].layout_dirty = true;
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 100.0, 0.0, 100.0));
        assert_eq!(size.width, 70.0, "the re-measure picks up the changed dynamic min");
    }

    // ── size ranges (Compose's widthIn/heightIn/sizeIn and required*In) ──

    #[test]
    fn width_in_intersects_the_target_with_the_incoming_range() {
        // Target [300, 500] into incoming [0, 200]: the incoming maximum wins.
        let m = Modifier::new().width_in(300.0, 500.0);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 200.0, 0.0, 200.0));
        assert_eq!(size.width, 200.0);
    }

    #[test]
    fn width_in_raises_the_minimum_and_lets_the_child_choose() {
        let m = Modifier::new().width_in(100.0, 300.0);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 1000.0, 0.0, 100.0));
        assert_eq!(size.width, 100.0, "an empty leaf sits at the minimum, not the maximum");
    }

    #[test]
    fn size_in_sets_both_axes_in_one_node() {
        let m = Modifier::new().size_in(80.0, 40.0, 200.0, 90.0);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 1000.0, 0.0, 1000.0));
        assert_eq!((size.width, size.height), (80.0, 40.0));
    }

    #[test]
    fn required_size_in_ignores_the_incoming_maximum() {
        let m = Modifier::new().required_width_in(300.0, 400.0);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 100.0, 0.0, 100.0));
        assert_eq!(size.width, 300.0, "the required minimum wins over the incoming max");
    }

    #[test]
    fn size_ranges_resolve_by_chain_order() {
        let measure = |m: Modifier| {
            let mut nodes = vec![LayoutNode::leaf(m)];
            let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 1000.0, 0.0, 100.0));
            size.width
        };
        assert_eq!(
            measure(Modifier::new().width_in(0.0, 200.0).width_in(300.0, 400.0)),
            200.0,
            "the outer range wins: the inner one is clipped into it"
        );
        assert_eq!(
            measure(Modifier::new().width_in(300.0, 400.0).width_in(0.0, 200.0)),
            300.0,
            "and the reverse is 300"
        );
    }

    #[test]
    fn default_min_size_only_applies_where_the_incoming_minimum_is_zero() {
        let m = Modifier::new().default_min_size(58.0, 40.0);
        let mut nodes = vec![LayoutNode::leaf(m.clone())];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 300.0, 0.0, 300.0));
        assert_eq!((size.width, size.height), (58.0, 40.0), "the default floor applies");

        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(80.0, 300.0, 0.0, 300.0));
        assert_eq!(size.width, 80.0, "a caller's minimum overrides the default");
    }

    #[test]
    fn fill_max_width_fraction_takes_that_share_of_the_maximum() {
        let m = Modifier::new().fill_max_width_fraction(0.5);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 200.0, 0.0, 100.0));
        assert_eq!(size.width, 100.0);
    }

    // ── wrapContent（Compose wrapContentSize/Width/Height）──

    #[test]
    fn wrap_content_size_centres_the_leaf_content_in_the_minimum_box() {
        // Chain order matters: the wrap is OUTER, so it relaxes the range before `size` applies.
        let m = Modifier::new()
            .wrap_content_size(ContentAlignment::CENTER, false)
            .size(20.0, 20.0);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(100.0, 100.0, 100.0, 100.0));
        assert_eq!((size.width, size.height), (100.0, 100.0));
        assert_eq!(nodes[0].content_box_override, Some((40.0, 40.0, 20.0, 20.0)));
    }

    #[test]
    fn wrap_content_size_unbounded_lets_the_content_overflow_the_maximum() {
        let m = Modifier::new()
            .wrap_content_size(ContentAlignment::CENTER, true)
            .size(200.0, 20.0);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 100.0, 0.0, 100.0));
        assert_eq!(size.width, 100.0, "the wrapper is coerced back into the incoming range");
        assert_eq!(
            nodes[0].content_box_override,
            Some((-50.0, 0.0, 200.0, 20.0)),
            "the content overflows symmetrically"
        );
    }

    #[test]
    fn wrap_content_size_centres_a_child() {
        let mut nodes = vec![
            LayoutNode::new(Modifier::new().wrap_content_size(ContentAlignment::CENTER, false), Some(0)),
            LayoutNode::leaf(Modifier::new().size(20.0, 20.0)),
        ];
        nodes[0].children = vec![1];
        let policies: Vec<Box<dyn crate::layout::node::MeasurePolicy>> =
            vec![Box::new(crate::layout::box_layout::BoxLayout::new())];
        let (size, _) = measure_node(&mut nodes, &policies, 0, Constraints::new(100.0, 100.0, 100.0, 100.0));
        assert_eq!((size.width, size.height), (100.0, 100.0));
        assert_eq!((nodes[1].position.x, nodes[1].position.y), (40.0, 40.0));
    }

    #[test]
    fn inset_padding_consumes_for_the_subtree() {
        use crate::window::insets::{Insets, WindowInsets};
        // The shared lock: a function-local one cannot protect against the OTHER tests that mutate
        // the same process-global fallback controller.
        let _guard = crate::window::insets::INSET_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let controller = crate::window::insets::current();
        // Reset on Drop, not at the end of the body: a failing assertion must not leave system
        // insets behind for every later test in this process.
        struct ResetInsets(crate::window::insets::WindowInsetsController);
        impl Drop for ResetInsets {
            fn drop(&mut self) {
                self.0.set(WindowInsets::default());
            }
        }
        let _reset = ResetInsets(controller.clone());
        controller.set(WindowInsets {
            status_bars: Insets::new(0.0, 24.0, 0.0, 0.0),
            navigation_bars: Insets::new(0.0, 0.0, 0.0, 48.0),
            ..Default::default()
        });

        let mut nodes = vec![
            LayoutNode::new(Modifier::new().system_bars_padding(), Some(0)),
            LayoutNode::leaf(Modifier::new().navigation_bars_padding().size(10.0, 10.0)),
        ];
        nodes[0].children = vec![1];
        let policies: Vec<Box<dyn crate::layout::node::MeasurePolicy>> =
            vec![Box::new(crate::layout::box_layout::BoxLayout::new())];
        let (size, _) = measure_node(&mut nodes, &policies, 0, Constraints::new(0.0, 200.0, 0.0, 200.0));
        assert_eq!(
            size,
            Size::new(10.0, 82.0),
            "the outer systemBars pads top 24 + bottom 48; the inner navigationBars is consumed"
        );

        // Consume-only: the inner padding must not reappear.
        let mut nodes = vec![
            LayoutNode::new(
                Modifier::new().consume_window_insets(crate::window::insets::InsetsKind::NavigationBars),
                Some(0),
            ),
            LayoutNode::leaf(Modifier::new().navigation_bars_padding().size(10.0, 10.0)),
        ];
        nodes[0].children = vec![1];
        let (size, _) = measure_node(&mut nodes, &policies, 0, Constraints::new(0.0, 200.0, 0.0, 200.0));
        assert_eq!(size, Size::new(10.0, 10.0), "consumed insets pad by zero");
    }

    // ── position callbacks (Compose onPlaced / onGloballyPositioned) ──

    #[test]
    fn position_callbacks_report_the_global_box_once_per_change() {
        use std::sync::{Arc, Mutex};
        let placed: Arc<Mutex<Vec<GlobalPosition>>> = Arc::new(Mutex::new(Vec::new()));
        let global: Arc<Mutex<Vec<GlobalPosition>>> = Arc::new(Mutex::new(Vec::new()));
        let (p, g) = (placed.clone(), global.clone());
        let mut nodes = vec![
            LayoutNode::new(Modifier::new(), Some(0)),
            LayoutNode::leaf(
                Modifier::new()
                    .on_placed(move |pos| p.lock().unwrap().push(pos))
                    .on_globally_positioned(move |pos| g.lock().unwrap().push(pos)),
            ),
        ];
        nodes[0].children = vec![1];
        nodes[0].measured_size = Size::new(100.0, 100.0);
        nodes[1].position = Offset::new(10.0, 20.0);
        nodes[1].measured_size = Size::new(30.0, 40.0);

        let (placed_cb, global_cb) = collect_position_callbacks(&mut nodes, 0, (0.0, 0.0));
        for (callback, position) in placed_cb {
            callback(position);
        }
        for (callback, position) in global_cb {
            callback(position);
        }
        let expected = GlobalPosition::new(10.0, 20.0, 30.0, 40.0);
        assert_eq!(*placed.lock().unwrap(), vec![expected]);
        assert_eq!(*global.lock().unwrap(), vec![expected]);

        // The box did not change: neither callback fires again.
        let (placed_cb, global_cb) = collect_position_callbacks(&mut nodes, 0, (0.0, 0.0));
        assert!(placed_cb.is_empty() && global_cb.is_empty());

        // A moved child reports its new box.
        nodes[1].position = Offset::new(15.0, 25.0);
        let (placed_cb, _) = collect_position_callbacks(&mut nodes, 0, (0.0, 0.0));
        for (callback, position) in placed_cb {
            callback(position);
        }
        assert_eq!(
            placed.lock().unwrap().last(),
            Some(&GlobalPosition::new(15.0, 25.0, 30.0, 40.0))
        );
    }

    #[test]
    fn a_scroll_offset_is_part_of_the_global_position() {
        use std::sync::{Arc, Mutex};
        let seen: Arc<Mutex<Vec<GlobalPosition>>> = Arc::new(Mutex::new(Vec::new()));
        let s = seen.clone();
        let scroll = crate::modifier::ScrollState::new();
        scroll.offset.set(7.0);
        let mut nodes = vec![
            LayoutNode::new(Modifier::new().vertical_scroll(scroll), Some(0)),
            LayoutNode::leaf(Modifier::new().on_globally_positioned(move |pos| {
                s.lock().unwrap().push(pos)
            })),
        ];
        nodes[0].children = vec![1];
        nodes[0].measured_size = Size::new(100.0, 100.0);
        nodes[1].position = Offset::new(0.0, 30.0);
        nodes[1].measured_size = Size::new(10.0, 10.0);
        let (_, global_cb) = collect_position_callbacks(&mut nodes, 0, (0.0, 0.0));
        for (callback, position) in global_cb {
            callback(position);
        }
        assert_eq!(seen.lock().unwrap().first().map(|p| p.y), Some(23.0), "30 - 7");
    }

    #[test]
    fn layout_id_is_queryable_on_the_chain_and_in_the_arena() {
        let m = Modifier::new().layout_id("field");
        assert_eq!(m.layout_id_value(), Some("field"));
        let mut composer = crate::runtime::composer::Composer::new();
        composer.arena_nodes_mut().push(LayoutNode::leaf(m));
        assert_eq!(composer.find_by_layout_id("field"), Some(0));
        assert_eq!(composer.find_by_layout_id("other"), None);
    }

    // ── maxWidth / maxHeight (the other half of Compose's widthIn/heightIn) ──

    #[test]
    fn max_width_caps_a_wider_child() {
        // The cap lowers the constraints the content sees, so a 400-wide child comes out 200.
        let mut nodes = vec![
            LayoutNode::new(Modifier::new().max_width(200.0), Some(0)),
            LayoutNode::leaf(Modifier::new().size(400.0, 20.0)),
        ];
        nodes[0].children = vec![1];
        let policies: Vec<Box<dyn crate::layout::node::MeasurePolicy>> =
            vec![Box::new(crate::layout::box_layout::BoxLayout::new())];
        let (size, _) = measure_node(&mut nodes, &policies, 0, Constraints::new(0.0, 1000.0, 0.0, 1000.0));
        assert_eq!(size.width, 200.0, "max_width limits the content width");
    }

    /// A conflicting min and max resolve by CHAIN ORDER, which is Compose's rule: each modifier is
    /// its own node and `constrain`s into what the node before it produced, so the outer call's
    /// range is the one that survives (`Size.kt`: `SizeNode.targetConstraints`).
    ///
    /// Both directions are asserted because either single answer is wrong on its own — this used to
    /// be "the min always wins", a documented deviation that matched the second order only.
    #[test]
    fn a_conflicting_min_and_max_resolve_by_chain_order() {
        let measure = |m: Modifier| {
            let mut nodes = vec![LayoutNode::leaf(m)];
            let (size, _) =
                measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 1000.0, 0.0, 100.0));
            size.width
        };
        assert_eq!(
            measure(Modifier::new().max_width(200.0).min_width(300.0)),
            200.0,
            "the max written first is the outer node, so it wins the conflict"
        );
        assert_eq!(
            measure(Modifier::new().min_width(300.0).max_width(200.0)),
            300.0,
            "and the min written first wins it, as in Compose"
        );
    }

    #[test]
    fn max_width_yields_to_a_tight_size() {
        // A tight size(30) raises the min to 30, so the max cannot pull it below that.
        let m = Modifier::new().size(30.0, 20.0).max_width(10.0);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 100.0, 0.0, 100.0));
        assert_eq!((size.width, size.height), (30.0, 20.0), "the maximum must not cross the tight min");
    }

    #[test]
    fn max_height_caps_a_taller_child() {
        let mut nodes = vec![
            LayoutNode::new(Modifier::new().max_height(50.0), Some(0)),
            LayoutNode::leaf(Modifier::new().size(20.0, 400.0)),
        ];
        nodes[0].children = vec![1];
        let policies: Vec<Box<dyn crate::layout::node::MeasurePolicy>> =
            vec![Box::new(crate::layout::box_layout::BoxLayout::new())];
        let (size, _) = measure_node(&mut nodes, &policies, 0, Constraints::new(0.0, 1000.0, 0.0, 1000.0));
        assert_eq!(size.height, 50.0, "max_height limits the content height");
    }

    #[test]
    fn max_width_dynamic_state() {
        // A dynamic cap is re-resolved on re-measure, like the min counterpart's. (The
        // dependency registration happens in the same `Dimension::Dynamic` path; this test pins
        // the re-resolution, not the registration — it marks the nodes dirty itself.)
        use crate::runtime::state::State;
        let s = State::new(200.0);
        let mut nodes = vec![
            LayoutNode::new(Modifier::new().max_width(&s), Some(0)),
            LayoutNode::leaf(Modifier::new().size(400.0, 20.0)),
        ];
        nodes[0].children = vec![1];
        let policies: Vec<Box<dyn crate::layout::node::MeasurePolicy>> =
            vec![Box::new(crate::layout::box_layout::BoxLayout::new())];
        let (size, _) = measure_node(&mut nodes, &policies, 0, Constraints::new(0.0, 1000.0, 0.0, 1000.0));
        assert_eq!(size.width, 200.0);
        s.set(300.0);
        nodes[0].layout_dirty = true;
        nodes[1].layout_dirty = true;
        let (size, _) = measure_node(&mut nodes, &policies, 0, Constraints::new(0.0, 1000.0, 0.0, 1000.0));
        assert_eq!(size.width, 300.0, "the re-measure picks up the changed dynamic max");
    }


    /// A dynamic PADDING is a length like any other: it is resolved during measure, so a state that
    /// changes re-measures the node without a recomposition. (Same mechanism as `max_width`: the
    /// closure's read registers the layout dependency; this pins the re-resolution.)
    #[test]
    fn dynamic_padding_is_re_resolved_on_re_measure() {
        use crate::runtime::state::State;
        let pad = State::new(4.0f32);
        let mut nodes = vec![
            LayoutNode::new(Modifier::new().padding(pad.clone()), Some(0)),
            LayoutNode::leaf(Modifier::new().size(100.0, 20.0)),
        ];
        nodes[0].children = vec![1];
        let policies: Vec<Box<dyn crate::layout::node::MeasurePolicy>> =
            vec![Box::new(crate::layout::box_layout::BoxLayout::new())];
        let (size, _) = measure_node(&mut nodes, &policies, 0, Constraints::new(0.0, 1000.0, 0.0, 1000.0));
        assert_eq!((size.width, size.height), (108.0, 28.0), "4 px on every side");

        pad.set(10.0);
        nodes[0].layout_dirty = true;
        nodes[1].layout_dirty = true;
        let (size, _) = measure_node(&mut nodes, &policies, 0, Constraints::new(0.0, 1000.0, 0.0, 1000.0));
        assert_eq!(
            (size.width, size.height),
            (120.0, 40.0),
            "10 px on every side after the value changed"
        );
    }

    // ── test_tag ──

    #[test]
    fn test_tag_stored_and_queried() {
        let m = Modifier::new().test_tag("btn-submit");
        assert_eq!(m.get_test_tag(), Some("btn-submit"));
        let m2 = Modifier::new();
        assert_eq!(m2.get_test_tag(), None);
    }

    #[test]
    fn test_tag_in_param_eq() {
        let a = Modifier::new().test_tag("a");
        let b = Modifier::new().test_tag("b");
        assert!(!a.param_eq(&b), "a different tag must compare unequal");
        let c = Modifier::new().test_tag("a");
        assert!(a.param_eq(&c), "the same tag must compare equal");
    }

    #[test]
    fn test_leaf_padding_included_in_measured_size() {
        // A leaf (no policy) adds the padding back to its size. It used to lose it: the measured size
        // excluded the padding, so Text + padding collapsed and the content hugged the edge.
        let mut nodes = vec![LayoutNode::leaf(Modifier::new().size(100.0, 50.0).padding(10.0))];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 200.0, 0.0, 200.0));
        // size(100,50) deflated by padding(10) leaves 80x30 of content, which reads 100x50 again.
        assert_eq!(size, Size::new(100.0, 50.0));
        assert_eq!(nodes[0].measured_size, Size::new(100.0, 50.0));
    }

    #[test]
    fn test_leaf_asymmetric_padding_included() {
        // Asymmetric padding, start(10) plus bottom(5), with a fixed size(60,40): the content area is
        // 50x35 and the padding brings the total back to 60x40 (it used to return 50x35, losing it).
        let mut nodes = vec![LayoutNode::leaf(
            Modifier::new().size(60.0, 40.0).padding_start(10.0).padding_bottom(5.0),
        )];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 200.0, 0.0, 200.0));
        assert_eq!(size, Size::new(60.0, 40.0));
        // A padding-only leaf with no fixed size: empty content plus the padding.
        let mut nodes2 = vec![LayoutNode::leaf(Modifier::new().padding_start(10.0).padding_bottom(5.0))];
        let (size2, _) = measure_node(&mut nodes2, &[], 0, Constraints::new(0.0, 200.0, 0.0, 200.0));
        assert_eq!(size2, Size::new(10.0, 5.0));
    }

    // ── Image leaf measurement (intrinsic-size layout: an unspecified dimension falls back to the
    //    intrinsic size, as Compose does) ──

    fn image_modifier() -> Modifier {
        use crate::graphics::{ContentScale, ImageAlignment};
        Modifier::new().image_content(
            crate::graphics::IconSource::svg("<svg viewBox=\"0 0 48 24\"/>"),
            ContentScale::Fit,
            ImageAlignment::Center,
            1.0,
            None,
            crate::graphics::FilterQuality::Low,
        )
    }

    #[test]
    fn test_leaf_image_intrinsic_size() {
        // The intrinsic size comes from the SVG viewBox (in memory, no file).
        let mut nodes = vec![LayoutNode::leaf(image_modifier())];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::UNBOUNDED);
        assert_eq!(size, Size::new(48.0, 24.0), "UNBOUNDED uses the intrinsic size");
        assert_eq!(nodes[0].measured_size, Size::new(48.0, 24.0));
    }

    #[test]
    fn test_leaf_image_tight_constraints_clamped() {
        // A tight 100x50 clamps it when no size modifier is present.
        let mut nodes = vec![LayoutNode::leaf(image_modifier())];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(100.0, 100.0, 50.0, 50.0));
        assert_eq!(size, Size::new(100.0, 50.0));
    }

    #[test]
    fn test_leaf_image_size_modifier_overrides() {
        // A modifier size overrides the intrinsic one (as in Compose).
        let m = Modifier::new().size(64.0, 32.0).image_content(
            crate::graphics::IconSource::svg("<svg viewBox=\"0 0 48 24\"/>"),
            crate::graphics::ContentScale::Fit,
            crate::graphics::ImageAlignment::Center,
            1.0,
            None,
            crate::graphics::FilterQuality::Low,
        );
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::UNBOUNDED);
        assert_eq!(size, Size::new(64.0, 32.0));
    }

    #[test]
    fn test_leaf_image_svg_without_viewbox_fallback() {
        // An SVG with no viewBox or width falls back to 24x24 measured (matching the decode fallback),
        // otherwise an Image would measure 0 while the same Icon draws.
        let m = Modifier::new().image_content(
            crate::graphics::IconSource::svg(
                "<svg xmlns=\"http://www.w3.org/2000/svg\"><path d=\"M0 0h24v24H0z\"/></svg>",
            ),
            crate::graphics::ContentScale::Fit,
            crate::graphics::ImageAlignment::Center,
            1.0,
            None,
            crate::graphics::FilterQuality::Low,
        );
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::UNBOUNDED);
        assert_eq!(size, Size::new(24.0, 24.0));
    }

    #[test]
    fn test_measure_node_rtl_padding() {
        // RTL with padding_start(10): start is on the right, so the content sits 10 in from it.
        use crate::layout::row::RowLayout;
        let mut nodes = vec![
            LayoutNode::new(
                Modifier::new()
                    .size(100.0, 50.0)
                    .layout_direction(LayoutDirection::Rtl)
                    .padding_start(10.0),
                Some(0),
            ),
            LayoutNode::leaf(Modifier::new().size(30.0, 20.0)),
        ];
        nodes[0].children = vec![1];
        nodes[0].layout_direction = LayoutDirection::Rtl; // the materialised snapshot (a modifier override)
        let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(RowLayout::new().direction(LayoutDirection::Rtl))];
        let _ = measure_node(&mut nodes, &policies, 0, Constraints::new(0.0, 100.0, 0.0, 100.0));
        // Content width 100 - 10 = 90; the first child (30 wide) sits at the right end, so x = 60.
        // The padding offset (RTL uses end = 0) leaves it unchanged.
        assert_eq!(nodes[1].position.x, 60.0, "RTL puts start on the right: the content hugs it, 10 of empty space on the right");
        // The gap on the right is 100 - (60+30) = 10, so padding_start lands on the right.
    }

    #[test]
    fn test_measure_node_rtl_padding_end_left() {
        // RTL with padding_end(20): end is on the left, so the content starts from the right and leaves 20 on the left.
        use crate::layout::row::RowLayout;
        let mut nodes = vec![
            LayoutNode::new(
                Modifier::new()
                    .size(100.0, 50.0)
                    .layout_direction(LayoutDirection::Rtl)
                    .padding_end(20.0),
                Some(0),
            ),
            LayoutNode::leaf(Modifier::new().size(30.0, 20.0)),
        ];
        nodes[0].children = vec![1];
        nodes[0].layout_direction = LayoutDirection::Rtl; // the materialised snapshot
        let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(RowLayout::new().direction(LayoutDirection::Rtl))];
        let _ = measure_node(&mut nodes, &policies, 0, Constraints::new(0.0, 100.0, 0.0, 100.0));
        // Content width 100 - 20 = 80; flex puts the first child at x = 80 - 30 = 50.
        // The padding offset (RTL uses end = 20) makes it 50 + 20 = 70.
        assert_eq!(nodes[1].position.x, 70.0, "RTL puts end on the left: the content runs from the right, 20 of space on the left");
    }

    #[test]
    fn test_measure_node_ltr_padding_unaffected() {
        // The LTR control: padding_start(10) puts the child 10 from the left.
        use crate::layout::row::RowLayout;
        let mut nodes = vec![
            LayoutNode::new(
                Modifier::new().size(100.0, 50.0).padding_start(10.0),
                Some(0),
            ),
            LayoutNode::leaf(Modifier::new().size(30.0, 20.0)),
        ];
        nodes[0].children = vec![1];
        let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(RowLayout::new())];
        let _ = measure_node(&mut nodes, &policies, 0, Constraints::new(0.0, 100.0, 0.0, 100.0));
        assert_eq!(nodes[1].position.x, 10.0, "LTR puts start on the left: the child sits 10 in");
    }

    #[test]
    fn test_measure_node_offset_rtl_mirror() {
        // A plain offset mirrors x under RTL as Compose's offset does; absolute_offset is exempt.
        use crate::layout::row::RowLayout;
        // LTR: offset(10, 5) moves the child 10 to the right.
        let mut nodes = vec![
            LayoutNode::new(Modifier::new().size(100.0, 50.0), Some(0)),
            LayoutNode::leaf(Modifier::new().size(30.0, 20.0).offset(10.0, 5.0)),
        ];
        nodes[0].children = vec![1];
        let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(RowLayout::new())];
        let _ = measure_node(&mut nodes, &policies, 0, Constraints::new(0.0, 100.0, 0.0, 100.0));
        assert_eq!(nodes[1].position.x, 10.0, "LTR offset(10) moves right");
        assert_eq!(nodes[1].position.y, 5.0, "the offset's y ignores the direction");

        // RTL: offset(10) mirrors, so x loses 10.
        let mut nodes = vec![
            LayoutNode::new(Modifier::new().size(100.0, 50.0), Some(0)),
            LayoutNode::leaf(Modifier::new().size(30.0, 20.0).offset(10.0, 5.0)),
        ];
        nodes[0].children = vec![1];
        nodes[0].layout_direction = LayoutDirection::Rtl;
        let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(RowLayout::new().direction(LayoutDirection::Rtl))];
        let _ = measure_node(&mut nodes, &policies, 0, Constraints::new(0.0, 100.0, 0.0, 100.0));
        assert_eq!(nodes[1].position.x, 60.0, "RTL offset(10) mirrors x (flex 70 - 10)");

        // RTL with absolute_offset: no mirroring, so x gains 10.
        let mut nodes = vec![
            LayoutNode::new(Modifier::new().size(100.0, 50.0), Some(0)),
            LayoutNode::leaf(Modifier::new().size(30.0, 20.0).absolute_offset(10.0, 0.0)),
        ];
        nodes[0].children = vec![1];
        nodes[0].layout_direction = LayoutDirection::Rtl;
        let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(RowLayout::new().direction(LayoutDirection::Rtl))];
        let _ = measure_node(&mut nodes, &policies, 0, Constraints::new(0.0, 100.0, 0.0, 100.0));
        assert_eq!(nodes[1].position.x, 80.0, "RTL absolute_offset(10) does not mirror (flex 70 + 10)");
    }

    /// The text snapshot compare must be as faithful as the snapshot it reads, and the refresh must
    /// make a node comparable against what it was just materialized with.
    ///
    /// This is the guard behind the re-measure that keeps a folding text node from painting a stale
    /// paragraph (`materialize`'s reuse path). Its sharp edge is that the comparison is written BY
    /// HAND field by field — `text_snapshot` clones the content string on every call, so the tuple
    /// form it replaced allocated per text node per frame — and a field dropped from it is invisible
    /// everywhere else: the node simply never re-measures for that field. Colour is the field with a
    /// test of its own at the composer level (an alpha fade changes no size), so it is asserted here
    /// too; the rest are checked as one set.
    #[test]
    fn text_snapshot_compare_sees_every_field_and_refresh_settles() {
        fn text_node(content: &str, color: crate::graphics::Color) -> LayoutNode {
            LayoutNode::new(
                Modifier::new().push(crate::modifier::ModifierElement::TextContent {
                    content: content.to_string(),
                    font_size: 14.0,
                    color,
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
                }),
                None,
            )
        }

        let black = crate::graphics::Color::from_argb(255, 0, 0, 0);
        let transparent = crate::graphics::Color::from_argb(0, 0, 0, 0);
        let mut node = text_node("hello", black);

        // Never materialized: nothing to compare against, and the caller must be told so.
        assert!(!node.text_snapshot_matches(), "a node with no snapshot cannot claim to match");
        node.refresh_text_snapshot();
        assert!(node.text_snapshot_matches(), "the refresh records the node's own text");
        assert!(node.last_text.is_some(), "…in the snapshot field, not just in the answer");

        // Same text, every field identical: still matching (this is the per-frame path).
        node.modifier = text_node("hello", black).modifier.clone();
        assert!(node.text_snapshot_matches(), "the same text matches");

        // A colour change is a change: the paragraph it paints was shaped in the old colour.
        node.modifier = text_node("hello", transparent).modifier.clone();
        assert!(!node.text_snapshot_matches(), "colour is part of the compared set");

        // …and the refresh is what settles it, so the very next compare is clean.
        node.refresh_text_snapshot();
        assert!(node.text_snapshot_matches(), "after the refresh the new text is the baseline");

        // Content, size and the layout-affecting flags are compared too.
        node.modifier = text_node("hello!", transparent).modifier.clone();
        assert!(!node.text_snapshot_matches(), "content is compared");

        // A node that loses its text has nothing to compare and must not hold a stale snapshot.
        let mut box_node = LayoutNode::new(Modifier::new().size(10.0, 10.0), None);
        box_node.last_text = node.last_text.clone();
        assert!(box_node.text_snapshot_matches(), "a node with no text matches by definition");
        box_node.refresh_text_snapshot();
        assert!(box_node.last_text.is_none(), "and the refresh drops the snapshot it cannot use");
    }

/// Compose's `windowInsets*Width` / `windowInsets*Height` (`WindowInsetsSize.kt`): the node's ONE axis
/// IS the unconsumed inset on that side and the content is measured inside it.
#[test]
fn an_inset_sized_node_takes_its_axis_from_the_inset() {
    let _lock = crate::window::insets::INSET_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let controller = crate::window::insets::current();
    struct ResetInsets(crate::window::insets::WindowInsetsController);
    impl Drop for ResetInsets {
        fn drop(&mut self) {
            self.0.set(crate::window::insets::WindowInsets::default());
        }
    }
    let _reset = ResetInsets(controller.clone());
    controller.set(crate::window::insets::WindowInsets {
        status_bars: crate::window::insets::Insets::new(20.0, 24.0, 5.0, 8.0),
        ..Default::default()
    });

    // The top inset owns the height; the width comes from the content.
    let top = Modifier::new()
        .window_insets_top_height(crate::window::insets::InsetsKind::StatusBars)
        .size(10.0, 10.0);
    let mut nodes = vec![LayoutNode::leaf(top)];
    let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 200.0, 0.0, 200.0));
    assert_eq!(size, Size::new(10.0, 24.0), "24 dp of status bar is the height");

    // The bottom inset owns the height too.
    let bottom = Modifier::new()
        .window_insets_bottom_height(crate::window::insets::InsetsKind::StatusBars)
        .size(10.0, 10.0);
    let mut nodes = vec![LayoutNode::leaf(bottom)];
    let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 200.0, 0.0, 200.0));
    assert_eq!(size, Size::new(10.0, 8.0), "8 dp of bottom inset is the height");

    // The inline start is left in LTR (20) and the right in RTL (5).
    for (direction, expected) in [
        (crate::layout::LayoutDirection::Ltr, 20.0),
        (crate::layout::LayoutDirection::Rtl, 5.0),
    ] {
        let start = Modifier::new()
            .window_insets_start_width(crate::window::insets::InsetsKind::StatusBars)
            .size(10.0, 10.0);
        let mut nodes = vec![LayoutNode::leaf(start)];
        // A hand-built node bypasses materialization, which is what usually stamps the direction.
        nodes[0].layout_direction = direction;
        let (size, _) =
            measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 200.0, 0.0, 200.0));
        assert_eq!(
            size,
            Size::new(expected, 10.0),
            "the start width follows the layout direction ({direction:?})"
        );
    }

    // An inset that has been consumed contributes nothing.
    controller.set(crate::window::insets::WindowInsets::default());
    let zero = Modifier::new()
        .window_insets_start_width(crate::window::insets::InsetsKind::StatusBars)
        .size(10.0, 10.0);
    let mut nodes = vec![LayoutNode::leaf(zero)];
    let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 200.0, 0.0, 200.0));
    // Compose returns layout(0, 0) for a zero inset and places nothing.
    assert_eq!(size, Size::new(0.0, 0.0), "no inset, no size");
}

/// Compose's `TextDelegate.layoutText` (TextDelegate.kt:160-181): a nowrap ELLIPSIS still lays the
/// paragraph out at the box width (and as ONE line), so the ellipsis has something to truncate
/// against. Before this the nowrap branch used `f32::MAX`, the line fit, and no ellipsis was ever
/// produced — measured: layout width 3.4e38, longest line 251.86 in a 60 dp box.
#[test]
fn a_nowrap_ellipsis_lays_the_paragraph_out_at_the_box_width() {
    use crate::text::{FontSlant, FontWeight, TextAlign, TextOverflow};
    let layout = |overflow: TextOverflow, soft_wrap: bool| {
        let modifier = Modifier::new().text_content_full(
            "hello world this is a long line".to_string(),
            20.0,
            crate::graphics::Color::BLACK,
            FontWeight::NORMAL,
            FontSlant::Upright,
            usize::MAX,
            TextAlign::Left,
            overflow,
            soft_wrap,
            0.0,
            None,
            crate::modifier::TextExtras::default(),
        );
        let mut nodes = vec![LayoutNode::leaf(modifier)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 60.0, 0.0, 500.0));
        let paragraph = nodes[0].cached_paragraph.borrow();
        let paragraph = paragraph.as_ref().expect("a paragraph");
        (
            size.width,
            paragraph.max_width(),
            paragraph.longest_line(),
            paragraph.did_exceed_max_lines(),
        )
    };

    let (width, layout_width, longest, exceeded) = layout(TextOverflow::Ellipsis, false);
    assert_eq!(width, 60.0, "the node is as wide as the box");
    assert_eq!(layout_width, 60.0, "the nowrap ellipsis lays out AT the box width");
    assert!(
        longest <= 60.5,
        "the line is truncated to the box (measured {longest}, layout width {layout_width})"
    );
    assert!(exceeded, "the paragraph reports the truncation");

    // A nowrap CLIP still lays out at an infinite width (Compose's widthMatters is false there), so
    // nothing is truncated by the paragraph itself; an ancestor does the clipping.
    let (_, clip_width, clip_longest, _) = layout(TextOverflow::Clip, false);
    assert!(
        clip_width > f32::MAX / 2.0 && clip_longest > 100.0,
        "a nowrap clip keeps the full line (layout {clip_width}, longest {clip_longest})"
    );
}

/// §3.2 of `docs/component-authoring.md`: a parameter that changes the CONTENT must re-enter the
/// leaf. `Text` declares the content with `ctx.changed`; every other parameter rides the
/// `TextContent` element, whose `param_eq` compares all sixteen fields, so a state-driven font size
/// re-measures even though the string is unchanged (measured 27 -> 53 dp).
#[test]
fn a_text_parameter_change_re_measures_without_a_content_change() {
    use crate::runtime::composer::{ComposeCtx, Composer};
    use crate::runtime::state::State;
    let size = State::new(20.0f32);
    let mut composer = Composer::new();
    let mut build = |composer: &mut Composer, size: State<f32>| {
        composer.compose(crate::compose!(|ctx: &mut ComposeCtx| {
            let s = size.get();
            crate::components::Text::new("Name").font_size(s).build(ctx);
        }));
        composer.layout(Constraints::new(0.0, 500.0, 0.0, 500.0));
        let root = composer.layout_root_idx().unwrap();
        composer.arena_nodes()[root].measured_size.height
    };
    let h20 = build(&mut composer, size.clone());
    size.set(40.0);
    let h40 = build(&mut composer, size.clone());
    assert!(
        h40 > h20 + 20.0,
        "the leaf re-entered and re-measured: h20={h20} h40={h40}"
    );
}
