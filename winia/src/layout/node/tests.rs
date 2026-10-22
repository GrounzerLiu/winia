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
    fn test_hit_test_z_order_topmost_first() {
        // 兄弟节点：绘制按 children 正序（后画在上层）——命中必须倒序优先
        // 底层铺满（leaf1）+ 上层局部矩形（leaf2）重叠点 → 应命中 leaf2
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

        // (40,40) 同时落在 leaf1 与 leaf2 内——上层（后画 leaf2）优先
        let path = hit_test(&nodes, 0, 40.0, 40.0);
        assert_eq!(path, vec![0, 2], "上层兄弟优先命中（z-order）");
        // (20,20) 只落 leaf1（leaf2 范围外）
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
        // RTL（scroll_reverse）回归：hit_test 坐标转换必须用 render 的镜像
        // 偏移量 off = content_w - viewport_w - offset，而非裸 offset——
        // 否则 RTL 滚动 tab 的点击位置错位（用户实测 bug）
        let state = crate::modifier::ScrollState::new();
        let m = Modifier::new().horizontal_scroll(state.clone());
        let mut node = LayoutNode::leaf(m);
        node.scroll_reverse = true;
        node.scroll_content_width = 1000.0;
        node.scroll_viewport_width = 360.0;

        // offset=0 → 镜像偏移 = 1000-360-0 = 640（render 显示内容末端）
        state.offset.set(0.0);
        let (dx, dy) = scroll_offset_for_node(&node);
        assert!((dx - 640.0).abs() < 0.01, "RTL offset=0 应镜像为 640，实际 {dx}");
        assert_eq!(dy, 0.0);

        // offset=100 → 640-100 = 540
        state.offset.set(100.0);
        let (dx, _) = scroll_offset_for_node(&node);
        assert!((dx - 540.0).abs() < 0.01, "RTL offset=100 应镜像为 540，实际 {dx}");

        // LTR（非 reverse）对照：偏移 = 裸 offset
        node.scroll_reverse = false;
        state.offset.set(100.0);
        let (dx, _) = scroll_offset_for_node(&node);
        assert!((dx - 100.0).abs() < 0.01, "LTR 应裸 offset=100，实际 {dx}");
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

        // 点击子节点
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

        // 点击父节点但不在子节点范围内
        let path = hit_test(&nodes, 0, 80.0, 75.0);
        assert_eq!(path, vec![0], "should only hit parent");
    }

    /// 回归：滚动容器内 scene 坐标（hit_test 输入）与 node_abs_position
    /// （app.rs 点击/拖拽定位用）必须同一空间——都减祖先 scroll 偏移。
    /// 此前 node_abs_position 纯累加 layout position，滚动后局部 y 被
    /// 滚动量污染 → 多行 TextField 点击/拖拽只能定位到第一行。
    #[test]
    fn test_scroll_scene_coords_consistent() {
        // 滚动容器（y=0，滚动 50px）内含子节点（y=100，内容高 60）
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

        // 渲染时画布 translate(0, -50) → 子节点视觉顶边在场景 y=50
        let path = hit_test(&nodes, 0, 50.0, 60.0);
        assert_eq!(path, vec![0, 1], "滚动后子节点视觉范围 (50,50)-(50,110) 应命中");
        // 子节点本地坐标 = 场景 - 视觉顶边 = (50-0, 60-50) = (50, 10)
        let (x, y) = scene_to_node_local(&nodes, &path, 1, 50.0, 60.0);
        assert_eq!((x, y), (50.0, 10.0), "本地坐标须扣除滚动偏移");

        // 未滚动点（y=140 → 场景 140-50=90，仍命中子节点本地 y=40）
        let path2 = hit_test(&nodes, 0, 50.0, 90.0);
        assert_eq!(path2, vec![0, 1]);
        let (_, y2) = scene_to_node_local(&nodes, &path2, 1, 50.0, 90.0);
        assert_eq!(y2, 40.0);

        // 滚动偏移变化 → 同一场景点对应不同本地坐标（偏移已消耗）。
        // offset=50：视觉顶边 y=50，场景 (50,50) → 本地 y=0；
        // offset=100：视觉顶边 y=0，场景 (50,50) → 本地 y=50（仍在可视范围）
        scroll.offset.set(100.0);
        let path3 = hit_test(&nodes, 0, 50.0, 50.0);
        assert_eq!(path3, vec![0, 1]);
        let (_, y3) = scene_to_node_local(&nodes, &path3, 1, 50.0, 50.0);
        assert_eq!(y3, 50.0, "本地坐标随滚动偏移同步变化");
    }

    // ── scene_to_node_local（波纹按压点本地坐标）──

    #[test]
    fn scene_to_node_local_plain() {
        // root(10,20) → child(30,40)：视觉原点 (40,60)
        let mut nodes = vec![
            LayoutNode::leaf(Modifier::new().size(100.0, 100.0)),
            LayoutNode::leaf(Modifier::new().size(50.0, 50.0)),
        ];
        nodes[0].position = Offset::new(10.0, 20.0);
        nodes[1].position = Offset::new(30.0, 40.0);
        nodes[0].children.push(1);

        let (lx, ly) = scene_to_node_local(&nodes, &[0, 1], 1, 45.0, 62.0);
        assert_eq!((lx, ly), (5.0, 2.0), "无滚动：本地 = 场景 - 布局原点");
    }

    #[test]
    fn scene_to_node_local_under_scroll() {
        // root(0,0) → scroll 容器(50,100, offset=50) → child(100,0)：
        // 视觉原点 = (50+100-0, 100+0-50) = (150, 50)
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
        assert_eq!((lx, ly), (10.0, 10.0), "滚动偏移从视觉原点扣除（与 hit_test/渲染一致）");
    }

    #[test]
    fn collect_focus_candidates_uses_visual_centers() {
        // root(0,0) → scroll 容器(50,100, offset=50) → focusable(100,0, 20x20)
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
        // 测试不执行 measure——显式设置测量尺寸（与 scene_to_node_local 系列一致）
        nodes[0].measured_size = Size::new(300.0, 300.0);
        nodes[1].measured_size = Size::new(200.0, 200.0);
        nodes[2].measured_size = Size::new(20.0, 20.0);
        nodes[0].children.push(1);
        nodes[1].children.push(2);

        let cands = collect_focus_candidates(&nodes, 0);
        assert_eq!(cands.len(), 1, "仅 focusable 子节点进入候选");
        // 视觉位置 = (150, 50)，中心 = (160, 60)（scroll offset 已扣除）
        assert_eq!(cands[0].0, nodes[2].id);
        assert_eq!((cands[0].1, cands[0].2), (160.0, 60.0));
    }

    // ── aspectRatio / requiredSize ──

    #[test]
    fn required_size_overrides_incoming() {
        // 父约束 100x100，requiredSize(200, 50) → 溢出（enforceIncoming=false）
        let m = Modifier::new().required_size(200.0, 50.0);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 100.0, 0.0, 100.0));
        assert_eq!((size.width, size.height), (200.0, 50.0), "required 忽略 incoming 收缩");
    }

    #[test]
    fn required_width_only() {
        let m = Modifier::new().required_width(300.0);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 100.0, 0.0, 100.0));
        assert_eq!(size.width, 300.0, "requiredWidth 溢出");
        assert!(size.height <= 100.0, "高度仍受 incoming 约束");
    }

    #[test]
    fn aspect_ratio_derives_from_max_width() {
        // 无 size 约束：外层 max 300x300，ratio 2 → 宽 300 高 150
        let m = Modifier::new().aspect_ratio(2.0, false);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 300.0, 0.0, 300.0));
        assert_eq!((size.width, size.height), (300.0, 150.0), "以 max_w 为基准推导");
    }

    #[test]
    fn aspect_ratio_respects_inner_fixed_size() {
        // 链内 size(50,20) 是 tight（min=max）——aspect 推导 40x20 被
        // clamp 回 min——Compose 同（tight size 下 aspect 无法改变尺寸）
        let m = Modifier::new().size(50.0, 20.0).aspect_ratio(2.0, false);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 300.0, 0.0, 300.0));
        assert_eq!((size.width, size.height), (50.0, 20.0), "tight size 约束 aspect 不可改变（Compose 一致）");
    }

    #[test]
    fn aspect_ratio_clamps_to_max_height() {
        // 无 size：外层 max 300x100，ratio 2 → h = 300/2 = 150 > 100 → clamp：(200, 100)
        let m = Modifier::new().aspect_ratio(2.0, false);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 300.0, 0.0, 100.0));
        assert_eq!((size.width, size.height), (200.0, 100.0), "高度超界时反推宽度");
    }

    #[test]
    fn aspect_ratio_match_height_first() {
        // match_height_first：以 max_h 为基准——无 size 时外层 100x300，ratio 2
        // → h = 300，w = 600 > 100 → clamp：(100, 50)
        let m = Modifier::new().aspect_ratio(2.0, true);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 100.0, 0.0, 300.0));
        assert_eq!((size.width, size.height), (100.0, 50.0), "match_height_first 反推");
    }

    // ── minWidth / minHeight（对标 Compose widthIn/heightIn）──

    #[test]
    fn min_width_height_raises_min_constraints() {
        // min_width(58).min_height(40) + 空内容 → 撑到 58x40（Button 默认最小尺寸）
        let m = Modifier::new().min_width(58.0).min_height(40.0);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 100.0, 0.0, 100.0));
        assert_eq!((size.width, size.height), (58.0, 40.0), "min 约束提升 incoming min");
    }

    #[test]
    fn min_width_yields_to_tight_size() {
        // tight size(30,20) 之后 min(58) 被 max=30 夹住（Compose constraints 合并语义）
        let m = Modifier::new().size(30.0, 20.0).min_width(58.0);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 100.0, 0.0, 100.0));
        assert_eq!((size.width, size.height), (30.0, 20.0), "min 不得越过 tight max");
    }

    #[test]
    fn min_width_dynamic_state() {
        // 动态 min（动画）：measure 期 get() 注册布局依赖——值变化重测生效
        use crate::runtime::state::State;
        let s = State::new(58.0);
        let m = Modifier::new().min_width(&s);
        let mut nodes = vec![LayoutNode::leaf(m)];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 100.0, 0.0, 100.0));
        assert_eq!(size.width, 58.0);
        s.set(70.0);
        // 模拟动画值 notify 后的布局失效（真实链路：composer pending → layout_dirty）
        nodes[0].layout_dirty = true;
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 100.0, 0.0, 100.0));
        assert_eq!(size.width, 70.0, "动态 min 值变化后重测取新值");
    }

    // ── size ranges（Compose widthIn/heightIn/sizeIn 与 required*In）──

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
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
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

    // ── maxWidth / maxHeight（Compose widthIn/heightIn 的另一半）──

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
        assert_eq!(size.width, 200.0, "max_width 限制内容宽度");
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
        assert_eq!((size.width, size.height), (30.0, 20.0), "max 不得越过 tight min");
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
        assert_eq!(size.height, 50.0, "max_height 限制内容高度");
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
        assert_eq!(size.width, 300.0, "动态 max 值变化后重测取新值");
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
        assert!(!a.param_eq(&b), "tag 不同必须不等");
        let c = Modifier::new().test_tag("a");
        assert!(a.param_eq(&c), "tag 相同必须相等");
    }

    #[test]
    fn test_leaf_padding_included_in_measured_size() {
        // 叶子节点（无 policy）尺寸回加 padding（此前丢失：测量不含内边距，
        // Text+padding 高度塌陷、内容贴边）
        let mut nodes = vec![LayoutNode::leaf(Modifier::new().size(100.0, 50.0).padding(10.0))];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 200.0, 0.0, 200.0));
        // size(100,50) 经 padding(10) 内缩 → 内容 80x30，回加后 100x50
        assert_eq!(size, Size::new(100.0, 50.0));
        assert_eq!(nodes[0].measured_size, Size::new(100.0, 50.0));
    }

    #[test]
    fn test_leaf_asymmetric_padding_included() {
        // 非对称 padding：start(10)+bottom(5) + 固定 size(60,40)
        // → 内容区 50x35 + padding 回加 = 总尺寸 60x40（修改前返回 50x35 丢失 padding）
        let mut nodes = vec![LayoutNode::leaf(
            Modifier::new().size(60.0, 40.0).padding_start(10.0).padding_bottom(5.0),
        )];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(0.0, 200.0, 0.0, 200.0));
        assert_eq!(size, Size::new(60.0, 40.0));
        // 无固定尺寸的纯 padding 叶子：内容 0 + padding 回加
        let mut nodes2 = vec![LayoutNode::leaf(Modifier::new().padding_start(10.0).padding_bottom(5.0))];
        let (size2, _) = measure_node(&mut nodes2, &[], 0, Constraints::new(0.0, 200.0, 0.0, 200.0));
        assert_eq!(size2, Size::new(10.0, 5.0));
    }

    // ── Image 叶子测量（固有尺寸布局——对齐 Compose：未指定维度以固有尺寸为基准）──

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
        // 固有尺寸来自 SVG viewBox（纯内存，无需文件）
        let mut nodes = vec![LayoutNode::leaf(image_modifier())];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::UNBOUNDED);
        assert_eq!(size, Size::new(48.0, 24.0), "UNBOUNDED 下按固有尺寸");
        assert_eq!(nodes[0].measured_size, Size::new(48.0, 24.0));
    }

    #[test]
    fn test_leaf_image_tight_constraints_clamped() {
        // tight 100x50：无 size modifier 时被约束钳制
        let mut nodes = vec![LayoutNode::leaf(image_modifier())];
        let (size, _) = measure_node(&mut nodes, &[], 0, Constraints::new(100.0, 100.0, 50.0, 50.0));
        assert_eq!(size, Size::new(100.0, 50.0));
    }

    #[test]
    fn test_leaf_image_size_modifier_overrides() {
        // modifier size 覆盖固有尺寸（Compose 语义：size 指定即以此为准）
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
        // 无 viewBox/width 的 SVG：测量回退 24×24（与解码回退一致——
        // 否则 Image 测量 0 尺寸空白而 Icon 正常显示的不一致）
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
        // RTL + padding_start(10)：start 在右——子内容靠右 10（左侧空隙 0）
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
        nodes[0].layout_direction = LayoutDirection::Rtl; // 模拟物化快照（modifier 覆盖）
        let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(RowLayout::new().direction(LayoutDirection::Rtl))];
        let _ = measure_node(&mut nodes, &policies, 0, Constraints::new(0.0, 100.0, 0.0, 100.0));
        // 内容区宽 = 100-10 = 90；RTL 第一个子（30 宽）在右端 → x = 60
        // padding 偏移（Rtl 用 end=0）→ 不变
        assert_eq!(nodes[1].position.x, 60.0, "RTL 下 start 在右：子内容靠右，右侧空隙 10");
        // 右侧空隙 = 100 - (60+30) = 10 ✓（padding_start 生效在右）
    }

    #[test]
    fn test_measure_node_rtl_padding_end_left() {
        // RTL + padding_end(20)：end 在左——子内容从右侧排，左侧空隙 20
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
        nodes[0].layout_direction = LayoutDirection::Rtl; // 模拟物化快照
        let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(RowLayout::new().direction(LayoutDirection::Rtl))];
        let _ = measure_node(&mut nodes, &policies, 0, Constraints::new(0.0, 100.0, 0.0, 100.0));
        // 内容区宽 = 100-20 = 80；flex RTL 第一个子 x = 80-30 = 50
        // padding 偏移（Rtl 用 end=20）→ 50+20 = 70
        assert_eq!(nodes[1].position.x, 70.0, "RTL 下 end 在左：子靠右排，左侧空隙 20");
    }

    #[test]
    fn test_measure_node_ltr_padding_unaffected() {
        // LTR 对照组：padding_start(10) → 子靠左 10
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
        assert_eq!(nodes[1].position.x, 10.0, "LTR 下 start 在左：子靠左 10");
    }

    #[test]
    fn test_measure_node_offset_rtl_mirror() {
        // 普通 offset 在 RTL 下 x 镜像（对标 Compose offset）；absolute_offset 豁免
        use crate::layout::row::RowLayout;
        // LTR：offset(10, 5) → 子 x 加 10
        let mut nodes = vec![
            LayoutNode::new(Modifier::new().size(100.0, 50.0), Some(0)),
            LayoutNode::leaf(Modifier::new().size(30.0, 20.0).offset(10.0, 5.0)),
        ];
        nodes[0].children = vec![1];
        let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(RowLayout::new())];
        let _ = measure_node(&mut nodes, &policies, 0, Constraints::new(0.0, 100.0, 0.0, 100.0));
        assert_eq!(nodes[1].position.x, 10.0, "LTR offset(10) 向右");
        assert_eq!(nodes[1].position.y, 5.0, "offset y 不受方向影响");

        // RTL：offset(10) 镜像 → x 减 10
        let mut nodes = vec![
            LayoutNode::new(Modifier::new().size(100.0, 50.0), Some(0)),
            LayoutNode::leaf(Modifier::new().size(30.0, 20.0).offset(10.0, 5.0)),
        ];
        nodes[0].children = vec![1];
        nodes[0].layout_direction = LayoutDirection::Rtl;
        let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(RowLayout::new().direction(LayoutDirection::Rtl))];
        let _ = measure_node(&mut nodes, &policies, 0, Constraints::new(0.0, 100.0, 0.0, 100.0));
        assert_eq!(nodes[1].position.x, 60.0, "RTL offset(10) x 镜像（flex 70 - 10）");

        // RTL + absolute_offset：不镜像 → x 加 10
        let mut nodes = vec![
            LayoutNode::new(Modifier::new().size(100.0, 50.0), Some(0)),
            LayoutNode::leaf(Modifier::new().size(30.0, 20.0).absolute_offset(10.0, 0.0)),
        ];
        nodes[0].children = vec![1];
        nodes[0].layout_direction = LayoutDirection::Rtl;
        let policies: Vec<Box<dyn MeasurePolicy>> = vec![Box::new(RowLayout::new().direction(LayoutDirection::Rtl))];
        let _ = measure_node(&mut nodes, &policies, 0, Constraints::new(0.0, 100.0, 0.0, 100.0));
        assert_eq!(nodes[1].position.x, 80.0, "RTL absolute_offset(10) 不镜像（flex 70 + 10）");
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
