    use super::*;
    use crate::runtime::composer::Composer;
    use crate::layout::Constraints;

    /// The "not measured yet" sentinel is `f32::MAX`, which `is_finite()` happily accepts — so the
    /// obvious test for it answers "yes, you can scroll forward" on a list nobody has laid out. That
    /// is the trap the ⚠ on [`LazyListState::can_scroll_forward`] warns about, and it is worth pinning:
    /// Compose's `canScrollForward` starts at `false` (`LazyListState.kt:474`).
    ///
    /// A REAL measured limit of `0` is the opposite case and must stay distinguishable — it means the
    /// content fits and the list genuinely cannot move.
    #[test]
    fn an_unmeasured_list_reports_no_room_ahead_but_a_measured_one_does() {
        let fresh = LazyListState::new();
        assert_eq!(fresh.fling_limit.peek(), f32::MAX, "the sentinel is f32::MAX, not infinity");
        assert!(
            !fresh.can_scroll_forward(),
            "before the first measure there is no measured limit, so this must not claim room ahead"
        );
        assert!(!fresh.can_scroll_backward(), "and it has certainly not been scrolled");

        let measured = LazyListState::new();
        measured.fling_limit.set(500.0);
        measured.offset.set(0.0);
        assert!(measured.can_scroll_forward(), "a measured limit below the offset leaves room ahead");

        measured.offset.set(500.0);
        assert!(!measured.can_scroll_forward(), "resting at the limit is the end of the list");
        assert!(measured.can_scroll_backward(), "and there is history behind it");

        let frozen = LazyListState::new();
        frozen.fling_limit.set(0.0);
        assert!(
            !frozen.can_scroll_forward(),
            "a real limit of 0 means the content fits — not that nothing has been measured yet"
        );
    }

    // ── IntervalList：定位 / key 映射 ──
    #[test]
    fn interval_list_locate() {
        let mut il = IntervalList::new();
        let c1: Arc<dyn Fn(&mut ComposeCtx, usize) + Send + Sync> = Arc::new(|_, _| {});
        let c2: Arc<dyn Fn(&mut ComposeCtx, usize) + Send + Sync> = Arc::new(|_, _| {});
        il.add(3, None, c1, false);
        il.add(2, Some(Arc::new(|g| 100 + g as u64)), c2, false);
        il.rebuild();
        assert_eq!(il.total(), 5);
        assert_eq!(il.locate(0), Some((0, 0)));
        assert_eq!(il.locate(2), Some((0, 2)));
        assert_eq!(il.locate(3), Some((1, 0)));
        assert_eq!(il.locate(4), Some((1, 1)));
        assert_eq!(il.locate(5), None);
        // key：无工厂段用索引；有工厂段用工厂
        assert_eq!(il.key_of(2), Some(2));
        assert_eq!(il.key_of(3), Some(103));
        assert_eq!(il.index_of_key(103), Some(3));
        assert_eq!(il.index_of_key(2), Some(2));
        assert_eq!(il.index_of_key(999), None);
    }

    // ── 高度缓存 ──
    #[test]
    fn height_cache_estimated_then_recorded() {
        let mut c = ItemHeightCache::default();
        assert_eq!(c.height(0), LAZY_ITEM_ESTIMATED_HEIGHT, "未测项用预估");
        c.record(2, 30.0);
        assert_eq!(c.height(2), 30.0);
        assert_eq!(c.height(1), LAZY_ITEM_ESTIMATED_HEIGHT);
        // 记录越界自动扩展
        c.record(5, 20.0);
        assert_eq!(c.height(5), 20.0);
    }

    // ── 高度缓存：按 item key 迁移（方案 A——数据变化不因 index 平移错位）──
    #[test]
    fn height_cache_keyed_rebase_follows_identity() {
        // 场景：原数据 0..6（key = 自身），index 2（key=2）实测高 30、index 5（key=5）
        // 实测高 20。随后数据前部插入 1 项（key=100）→ 原 key 2 移到 index 3、key 5
        // 移到 index 6。rebase 后高度应跟随 key 而非旧 index。
        let mut c = ItemHeightCache::default();
        c.record(2, 30.0);
        c.record_keyed(2, 30.0);
        c.record(5, 20.0);
        c.record_keyed(5, 20.0);
        // 新数据：key 序列 = [100, 0, 1, 2, 3, 4, 5]
        let new_keys = [100u64, 0, 1, 2, 3, 4, 5];
        c.rebase(&new_keys);
        // key=2 现在 index 3 → 高度 30 迁移过去
        assert_eq!(c.height(3), 30.0, "key=2 项应迁到 index 3");
        // key=5 现在 index 6 → 高度 20 迁移过去
        assert_eq!(c.height(6), 20.0, "key=5 项应迁到 index 6");
        // 新插入项 key=100（index 0）无记录 → 预估
        assert_eq!(c.height(0), LAZY_ITEM_ESTIMATED_HEIGHT, "新项走预估");
        // 原 index 2 处现在对应 key=1（无记录）→ 预估（不再残留 30）
        assert_eq!(c.height(2), LAZY_ITEM_ESTIMATED_HEIGHT, "旧 index 处不残留旧高度");
        // 数据变短：截断后越界高度清除
        c.record_keyed(9, 55.0); // 模拟曾测过 key=9（现不在数据中）
        c.rebase(&[100u64, 0, 1, 2, 3, 4]); // 新 total 6
        assert_eq!(c.heights.len(), 6, "数据变短清空越界高度");
        assert_eq!(c.height_keyed(9), None, "消失的 key 记录被清理");
    }

    // 同 total 数据变化（方案 A 核心动机）：total 不变但 key 序列变化（重排/
    // 中段删除/替换）时，高度仍按 key 跟随而非 index。
    #[test]
    fn height_cache_keyed_rebase_same_total_reorder_and_delete() {
        // 原数据 [a=30, b=40, c=50]（key = a/b/c 映射）
        let mut c = ItemHeightCache::default();
        c.record_keyed(10, 30.0); // a
        c.record_keyed(20, 40.0); // b
        c.record_keyed(30, 50.0); // c
        // 场景 1：同 total 重排 [a,b,c] → [c,a,b]（key 序列 [30,10,20]，total 均 3）
        c.rebase(&[30u64, 10, 20]);
        assert_eq!(c.heights.len(), 3, "同 total 重排不改变长度");
        assert_eq!(c.height(0), 50.0, "index 0 现在是 c(30) 高 50");
        assert_eq!(c.height(1), 30.0, "index 1 现在是 a(10) 高 30");
        assert_eq!(c.height(2), 40.0, "index 2 现在是 b(20) 高 40");
        // 场景 2：中段删除 [a,b,c] → [a,c]（total 3→2，key 序列 [10,30]）
        c.rebase(&[10u64, 30]);
        assert_eq!(c.heights.len(), 2, "删除后长度收缩");
        assert_eq!(c.height(0), 30.0, "index 0 是 a(10) 高 30 保持");
        assert_eq!(c.height(1), 50.0, "index 1 是 c(30) 高 50 迁移到位");
        assert_eq!(c.height_keyed(20), None, "被删的 b 高度记录清除");
        // 场景 3：替换（同 key 段内容变化——文档承认的局限：key 不变则不失效，
        // 此处 key 变才正确迁移）。[a,c] → [a,x,c]（前部插入 x）
        c.rebase(&[10u64, 99, 30]);
        assert_eq!(c.height(0), 30.0, "a 保持");
        assert_eq!(c.height(1), LAZY_ITEM_ESTIMATED_HEIGHT, "新 key 99 走预估");
        assert_eq!(c.height(2), 50.0, "c 迁移到 index 2");
    }

    // ── 锚点转换 ──
    #[test]
    fn anchor_from_offset_matches() {
        let mut c = ItemHeightCache::default();
        for i in 0..10 { c.record(i, 50.0); }
        // 每项 50+0 间距：offset 0..50 → 项 0；50..100 → 项 1
        assert_eq!(anchor_from_offset(&c, 0.0, 0.0, 0.0), (0, 0.0));
        assert_eq!(anchor_from_offset(&c, 49.0, 0.0, 0.0), (0, 49.0));
        assert_eq!(anchor_from_offset(&c, 50.0, 0.0, 0.0), (1, 0.0));
        assert_eq!(anchor_from_offset(&c, 120.0, 0.0, 0.0), (2, 20.0));
        // before padding：内容起点下移（offset=50 含 before → 锚点仍在项 0，偏移 0）
        assert_eq!(anchor_from_offset(&c, 50.0, 0.0, 50.0), (0, 0.0));
        assert_eq!(anchor_from_offset(&c, 99.0, 0.0, 50.0), (0, 49.0));
        assert_eq!(anchor_from_offset(&c, 100.0, 0.0, 50.0), (1, 0.0));
        // 超出已知范围（10 项已知）→ 按预估 48 继续扩展：99999 / 48 ≈ 2082
        let (far_idx, far_off) = anchor_from_offset(&c, 99999.0, 0.0, 0.0);
        assert!(far_idx > 2000, "预估扩展：实际 {far_idx}");
        // 已知 10 项 ×50 先扣掉，剩余按预估 48 继续：99999 - 500 - (idx-10)*48
        let expected = 99999.0 - 500.0 - (far_idx - 10) as f32 * 48.0;
        assert!((far_off - expected).abs() < 0.001);
    }

    // ── 可见范围 ──
    #[test]
    fn visible_range_includes_beyond_bounds() {
        let mut c = ItemHeightCache::default();
        for i in 0..100 { c.record(i, 50.0); }
        // 锚点项 5，偏移 0，视口 200 → 可见 4 项（200/50）+ 预取 4 = end≈9
        let (s, e) = visible_range(&c, 5, 0.0, 200.0, 0.0, 100, LAZY_BEYOND_BOUNDS);
        assert_eq!(s, 1, "start = 5 - 预取 4");
        assert!(e >= 9 && e <= 13, "end 覆盖视口 + 预取，实际 {e}");
        // 视口很大时 end 到 total
        let (_, e2) = visible_range(&c, 5, 0.0, 99999.0, 0.0, 100, LAZY_BEYOND_BOUNDS);
        assert_eq!(e2, 100);
    }

    // ── 组件级：稳定 key 数据变化保持滚动位置 ──
    #[test]
    fn key_preserves_scroll_position_on_data_change() {
        // 场景：滚动到项 50（key=50），然后列表前部插入 10 项（原项 50 → 新 index 60）。
        // last_known_first_key=50 应在下次 build 时校正 offset 到新位置。
        let state = LazyListState::new();
        // 模拟：滚动后 offset 对应项 50（每项高 48+0）
        state.offset.set(50.0 * 48.0);
        let mut il = IntervalList::new();
        // 数据 1：0..100（key = 自身）
        let c: Arc<dyn Fn(&mut ComposeCtx, usize) + Send + Sync> = Arc::new(|_, _| {});
        il.add(100, Some(Arc::new(|g| g as u64)), c.clone(), false);
        il.rebuild();
        assert_eq!(il.total(), 100);
        // 记录锚点 key（模拟 build 中的行为）：offset=2400 → 项 50。
        // 注意：anchor_from_offset 只锚定到已知高度范围（空缓存退化为 0）；
        // 真实场景 build 有上一帧的高度缓存。这里显式记录前 60 项高度模拟已测状态。
        let mut cache = ItemHeightCache::default();
        for i in 0..60 { cache.record(i, 48.0); }
        let (first, _) = anchor_from_offset(&cache, 2400.0, 0.0, 0.0);
        assert_eq!(first, 50, "offset 2400 / 48 = 项 50");
        let key50 = il.key_of(50).unwrap();
        assert_eq!(key50, 50);
        // 数据 2：前部插入 10 项（key 仍唯一：新项 key 90..99？不——新项用 key 0..9，
        // 原项 key 不变，但 index 平移 +10。模拟：key 0..99 + 10 个新 key 100..109）
        let mut il2 = IntervalList::new();
        // 前插 10 项（key 100..109），原 100 项后移（key = 数据 id = 0..99，
        // 全局 index 10..109）——原 key=50 的项现在全局 index 60
        il2.add(10, Some(Arc::new(|g| (100 + g) as u64)), c.clone(), false);
        il2.add(100, Some(Arc::new(|g| (g - 10) as u64)), c.clone(), false);
        il2.rebuild();
        assert_eq!(il2.total(), 110);
        // 原 key=50 的项现在在 index 60
        assert_eq!(il2.index_of_key(50), Some(60));
        // 校正：offset 应更新到新 index 的 prefix（60 × 48）
        let corrected = prefix_height(&cache, 60, 0.0);
        assert_eq!(corrected, 60.0 * 48.0);
        // 验证 key 保持语义：last_key=50 → 新 offset = prefix(60)
        state.last_known_first_key.set(Some(50));
        assert_eq!(state.last_known_first_key.get(), Some(50));
    }

    // ── 组件级像素：懒加载渲染 + 滚动后内容变化 ──
    fn render_lazy_sized(build: impl FnOnce(&mut ComposeCtx), w: f32, h: f32) -> (Vec<[u8; 4]>, usize) {
        use skia_safe::{Color as SkColor, surfaces};
        let theme = crate::theme::ThemeColors::light_from_seed(0x6750A4);
        let mut composer = Composer::new();
        let scene = |ctx: &mut ComposeCtx| {
            crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| build(ctx));
        };
        composer.compose(scene);
        composer.layout(Constraints::new(0.0, w, 0.0, h));
        let mut surface = surfaces::raster_n32_premul((w as i32, h as i32)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(SkColor::WHITE);
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        crate::render::render(nodes, root, canvas);
        let pm = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        (px.to_vec(), pm.width() as usize)
    }

    /// One frame, the way the app runs one: compose + layout, and again while the measure asks for it
    /// (`take_compose_after_layout`, `docs/lazy-column.md` 2.9). A single compose+layout is NOT the frame
    /// a user sees when the measure finds the window short — the app converges it within the frame — so a
    /// test rendering one pass would assert on a frame that never reaches a screen.
    fn render_lazy(build: impl Fn(&mut ComposeCtx)) -> (Vec<[u8; 4]>, usize) {
        use skia_safe::{Color as SkColor, surfaces};
        let theme = crate::theme::ThemeColors::light_from_seed(0x6750A4);
        let mut composer = Composer::new();
        let scene = |ctx: &mut ComposeCtx| {
            crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| build(ctx));
        };
        for _ in 0..8 {
            composer.compose(&scene);
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 600.0));
            if !crate::runtime::composer::take_compose_after_layout() {
                break;
            }
        }
        let mut surface = surfaces::raster_n32_premul((400, 600)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(SkColor::WHITE);
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        crate::render::render(nodes, root, canvas);
        let pm = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        (px.to_vec(), pm.width() as usize)
    }

    fn count_text_clusters(px: &[[u8; 4]], w: usize, h: usize) -> usize {
        // 深色文本像素行聚类
        let mut rows = vec![false; h];
        for y in 0..h {
            let mut n = 0;
            for x in 0..w {
                let p = px[y * w + x];
                // BGRA → (b,g,r) 语义（raster_n32_premul 小端）
                if p[0] < 120 && p[1] < 120 && p[2] < 120 { n += 1; }
            }
            rows[y] = n > 3;
        }
        let mut clusters = 0;
        let mut prev = -100;
        for (y, &r) in rows.iter().enumerate() {
            if r {
                if y as i32 - prev > 20 { clusters += 1; }
                prev = y as i32;
            }
        }
        clusters
    }

    #[test]
    fn lazy_renders_only_visible_items() {
        // 1000 项列表：只渲染视口内的 ~12 项（600px / 40px），而非全部
        use std::sync::atomic::{AtomicUsize, Ordering};
        let composed = Arc::new(AtomicUsize::new(0));
        let composed2 = composed.clone();
        let items: Arc<Vec<u64>> = Arc::new((0..1000).collect());
        let items2 = items.clone();
        let (px, w) = render_lazy(move |ctx| {
            let c2 = composed2.clone();
            LazyColumn::new()
                .modifier(Modifier::new().fill_max_width().fill_max_height())
                .items_from(
                    items2.clone(),
                    |v: &u64| *v,
                    move |ctx, _i, v| {
                        c2.fetch_add(1, Ordering::Relaxed);
                        crate::components::text::Text::new(format!("Item {}", v))
                            .font_size(14.0)
                            .modifier(Modifier::new().padding(12.0))
                            .build(ctx);
                    },
                )
                .build(ctx);
        });
        let h = 600usize;
        let clusters = count_text_clusters(&px, w, h);
        // 视口 600 / 项高 ~38 = ~15 项可见；注册窗含预取但渲染仅可见
        assert!(clusters >= 8 && clusters <= 30, "只渲染可见项，clusters={clusters}");
        // 组合次数 << 1000（懒加载核心断言）
        let n = composed.load(Ordering::Relaxed);
        assert!(n <= 60, "组合项数应远小于总数 1000，实际 {n}");
    }

    /// `fill_items` gives every item the viewport's main size — `fillParentMaxWidth` — and that is
    /// also the precondition the paging snap measures: the step is only written back when the items
    /// really do fill the viewport. Without the flag the items wrap to their text and the snap stands
    /// down.
    #[test]
    fn fill_items_widens_each_item_to_the_viewport_and_enables_the_snap_step() {
        const VIEWPORT: f32 = 300.0;
        let state = LazyListState::new();
        let state_paged = LazyListState::new();

        let rows = |state: &LazyListState, fill: bool| {
            render_lazy_sized(
                move |ctx| {
                    LazyRow::new()
                        .state(state.clone())
                        .fill_items(fill)
                        .snap_paging(true)
                        .modifier(Modifier::new().fill_max_width().fill_max_height())
                        .items(40, |i: usize| i as u64, |ctx, i| {
                            crate::components::text::Text::new(format!("M{i}"))
                                .font_size(14.0)
                                .build(ctx);
                        })
                        .build(ctx);
                },
                VIEWPORT,
                100.0,
            )
        };

        // Wrapping items measure at their text's width, so the snap step stays unset.
        rows(&state, false);
        assert_eq!(
            state.snap.peek(),
            None,
            "items that wrap to their content have no snap positions"
        );

        // Filling items measure at the viewport, and the config comes back with that as the step.
        rows(&state_paged, true);
        let snap = state_paged.snap.peek().expect(
            "fill_items makes every item one viewport wide, which IS the snap step",
        );
        assert_eq!(snap.step, VIEWPORT);
        assert!(
            snap.min_fling_velocity > 0.0,
            "and the velocity threshold comes with it ({} was passed)",
            snap.min_fling_velocity
        );
    }

    #[test]
    fn lazy_scroll_changes_visible_items() {
        let state = LazyListState::new();
        let items: Arc<Vec<u64>> = Arc::new((0..1000).collect());
        let (px0, w0) = render_lazy(|ctx| {
            LazyColumn::new()
                .state(state.clone())
                .modifier(Modifier::new().fill_max_width().fill_max_height())
                .items_from(items.clone(), |v: &u64| *v, |ctx, _i, v| {
                    crate::components::text::Text::new(format!("Item {}", v))
                        .font_size(14.0)
                        .modifier(Modifier::new().padding(12.0))
                        .build(ctx);
                })
                .build(ctx);
        });
        // 滚动：模拟 apply_scroll_delta 效果
        state.offset.set(2000.0);
        let (px1, w1) = render_lazy(|ctx| {
            LazyColumn::new()
                .state(state.clone())
                .modifier(Modifier::new().fill_max_width().fill_max_height())
                .items_from(items.clone(), |v: &u64| *v, |ctx, _i, v| {
                    crate::components::text::Text::new(format!("Item {}", v))
                        .font_size(14.0)
                        .modifier(Modifier::new().padding(12.0))
                        .build(ctx);
                })
                .build(ctx);
        });
        assert_eq!(w0, w1);
        // 帧间内容应不同（滚动后渲染不同项）：按行比较文本像素出现与否。
        // 偏移 2000/38 ≈ 52 项 → 文本行整体平移 ~2000px，diff_rows 应为数百。
        let h = 600usize;
        let mut diff_rows = 0;
        for y in 0..h {
            let mut has0 = false;
            let mut has1 = false;
            for x in (0..w0).step_by(8) {
                let p0 = px0[y * w0 + x];
                let p1 = px1[y * w0 + x];
                has0 |= p0[0] < 120 && p0[1] < 120 && p0[2] < 120;
                has1 |= p1[0] < 120 && p1[1] < 120 && p1[2] < 120;
            }
            if has0 != has1 { diff_rows += 1; }
        }
        assert!(diff_rows > 10, "滚动后内容变化，diff_rows={diff_rows}");
        // 派生锚点应为滚动后的精确值（约 2000/38 ≈ 52）
        assert!(state.first_visible() >= 40, "first_visible={}", state.first_visible());
    }

    // ── LazyRow 横向：同机制换轴 ──
    fn count_text_columns(px: &[[u8; 4]], w: usize, h: usize) -> usize {
        // 深色文本像素列聚类（横向列表用——文本行都挤在少数行，按列数项）
        let mut cols = vec![false; w];
        for x in 0..w {
            let mut n = 0;
            for y in (0..h).step_by(2) {
                let p = px[y * w + x];
                if p[0] < 120 && p[1] < 120 && p[2] < 120 { n += 1; }
            }
            cols[x] = n > 3;
        }
        let mut clusters = 0;
        let mut prev = -100;
        for (x, &c) in cols.iter().enumerate() {
            if c {
                if x as i32 - prev > 20 { clusters += 1; }
                prev = x as i32;
            }
        }
        clusters
    }

    #[test]
    fn lazy_row_renders_only_visible_items() {
        // 1000 项横向列表：只渲染视口内 ~9 项（600px / ~64px），而非全部
        use std::sync::atomic::{AtomicUsize, Ordering};
        let composed = Arc::new(AtomicUsize::new(0));
        let composed2 = composed.clone();
        let items: Arc<Vec<u64>> = Arc::new((0..1000).collect());
        let items2 = items.clone();
        let (px, w) = render_lazy_sized(
            move |ctx| {
                let c2 = composed2.clone();
                LazyRow::new()
                    .modifier(Modifier::new().fill_max_width().fill_max_height())
                    .items_from(
                        items2,
                        |v: &u64| *v,
                        move |ctx, _i, v| {
                            c2.fetch_add(1, Ordering::Relaxed);
                            crate::components::text::Text::new(format!("Item {}", v))
                                .font_size(14.0)
                                .modifier(Modifier::new().padding(12.0))
                                .build(ctx);
                        },
                    )
                    .build(ctx);
            },
            600.0,
            400.0,
        );
        let h = 400usize;
        let clusters = count_text_columns(&px, w, h);
        // 视口 600 / 项宽 ~64 = ~9 项可见（含预取与间隙）
        assert!(clusters >= 5 && clusters <= 20, "只渲染可见项，clusters={clusters}");
        let n = composed.load(Ordering::Relaxed);
        // 组合期预取窗固定 2000px（与 LazyColumn 同机制）：2000/56 ≈ 36 + 预取 8
        assert!(n <= 60, "组合项数应远小于总数 1000，实际 {n}");
    }

    #[test]
    fn lazy_row_scroll_changes_visible_items() {
        // 横向滚动 offset=2000 后，渲染的 Item 文本应变化（不同项）
        let state = LazyListState::new();
        let items: Arc<Vec<u64>> = Arc::new((0..1000).collect());
        let build_row = |ctx: &mut ComposeCtx, s: LazyListState| {
            LazyRow::new()
                .state(s)
                .modifier(Modifier::new().fill_max_width().fill_max_height())
                .items_from(items.clone(), |v: &u64| *v, |ctx, _i, v| {
                    crate::components::text::Text::new(format!("Item {}", v))
                        .font_size(14.0)
                        .modifier(Modifier::new().padding(12.0))
                        .build(ctx);
                })
                .build(ctx);
        };
        let (px0, w0) = render_lazy_sized(|ctx| build_row(ctx, state.clone()), 600.0, 400.0);
        state.offset.set(2000.0);
        let (px1, w1) = render_lazy_sized(|ctx| build_row(ctx, state.clone()), 600.0, 400.0);
        assert_eq!(w0, w1);
        // 帧间内容应不同（滚动后渲染不同项）：按列比较文本像素出现与否
        let h = 400usize;
        let mut diff_cols = 0;
        for x in 0..w0 {
            let mut has0 = false;
            let mut has1 = false;
            for y in (0..h).step_by(8) {
                let p0 = px0[y * w0 + x];
                let p1 = px1[y * w0 + x];
                has0 |= p0[0] < 120 && p0[1] < 120 && p0[2] < 120;
                has1 |= p1[0] < 120 && p1[1] < 120 && p1[2] < 120;
            }
            if has0 != has1 { diff_cols += 1; }
        }
        assert!(diff_cols > 10, "滚动后内容变化，diff_cols={diff_cols}");
        // 派生锚点应为滚动后的精确值（约 2000/64 ≈ 31）
        assert!(state.first_visible() >= 25, "first_visible={}", state.first_visible());
    }

    // ── 程序化跳转：锚点权威，落点精确 ──
    fn render_lazy_state(
        state: &LazyListState,
        items: &Arc<Vec<u64>>,
    ) -> (Vec<[u8; 4]>, usize) {
        let s = state.clone();
        let items = items.clone();
        render_lazy(move |ctx| {
            LazyColumn::new()
                .state(s.clone())
                .modifier(Modifier::new().fill_max_width().fill_max_height())
                .items_from(items.clone(), |v: &u64| *v, |ctx, _i, v| {
                    crate::components::text::Text::new(format!("Item {}", v))
                        .font_size(14.0)
                        .modifier(Modifier::new().padding(12.0))
                        .build(ctx);
                })
                .build(ctx);
        })
    }

    #[test]
    fn scroll_to_item_lands_exactly_on_index() {
        // 回归：跳 500 必须落 500——旧实现用空缓存预估 48px/项算像素 offset，
        // 实测高度 ~47.5 累积偏差 → 落到 505（用户实测）。新实现锚点权威：
        // 测量期从写回后的缓存推导像素，round-trip 精确。
        let state = LazyListState::new();
        let items: Arc<Vec<u64>> = Arc::new((0..1000).collect());
        let (px0, w0) = render_lazy_state(&state, &items);
        state.scroll_to_item(500, 0.0);
        let (px1, w1) = render_lazy_state(&state, &items);
        assert_eq!(w0, w1);
        assert_eq!(
            state.first_visible(),
            500,
            "跳转 500 必须精确落 500（旧实现 505）"
        );
        // 像素 offset 是预估混合前缀（未测中段按 48 预估）——但这不影响落点：
        // 放置与 translate 共用同一 prefix 函数，item 500 视觉上精确在顶部；
        // 随滚动测量推进，offset 逐步收敛到真实和。只断言合理范围。
        assert!(
            state.offset() > 20000.0 && state.offset() < 26000.0,
            "offset 应在 500 项前缀附近，实际 {}",
            state.offset()
        );
        // 内容确实变化（不同项）
        let h = 600usize;
        let mut diff_rows = 0;
        for y in 0..h {
            let mut has0 = false;
            let mut has1 = false;
            for x in (0..w0).step_by(8) {
                let p0 = px0[y * w0 + x];
                let p1 = px1[y * w0 + x];
                has0 |= p0[0] < 120 && p0[1] < 120 && p0[2] < 120;
                has1 |= p1[0] < 120 && p1[1] < 120 && p1[2] < 120;
            }
            if has0 != has1 { diff_rows += 1; }
        }
        assert!(diff_rows > 10, "跳转后内容变化，diff_rows={diff_rows}");
    }

    /// The landing precision that the jump documentation leans on, in the state a jump actually has to
    /// reason in: the cache is MIXED — real heights for rows that have been seen, an estimate for the
    /// rest — and every jump after the first one runs with a cache like that. `scroll_to_item(i)` must
    /// land with `firstVisibleItemIndex == i` for every row that can BE first, whatever the estimate says.
    /// The last rows cannot (there is no content below them to fill the viewport), so they are the clamp
    /// case, asserted separately below.
    #[test]
    fn jumps_land_exactly_on_their_index_with_a_partially_measured_cache() {
        let state = LazyListState::new();
        let items: Arc<Vec<u64>> = Arc::new((0..1000).collect());
        let _ = render_lazy_state(&state, &items);
        for target in [500usize, 0, 400, 50, 700, 200, 950] {
            state.scroll_to_item(target, 0.0);
            let _ = render_lazy_state(&state, &items);
            assert_eq!(
                state.first_visible(),
                target,
                "a jump to {target} must land on it"
            );
        }
        // The last row is out of reach: the clamp takes the list to its end, which is a DIFFERENT (lower)
        // first index — the case whose window has to be opened clamped rather than at the request.
        state.scroll_to_item(999, 0.0);
        let _ = render_lazy_state(&state, &items);
        let clamped = state.first_visible();
        assert!(
            clamped < 999 && clamped > 900,
            "a jump to the last row clamps to the end, actual {clamped}"
        );
    }

    #[test]
    fn scroll_to_item_clamps_to_end() {        // 越界跳转 clamp 到末尾（对齐 Compose：scroll position 在 measure 期 clamp）
        let state = LazyListState::new();
        let items: Arc<Vec<u64>> = Arc::new((0..1000).collect());
        let (_px, _w) = render_lazy_state(&state, &items);
        state.scroll_to_item(99999, 0.0);
        let (_px1, _w1) = render_lazy_state(&state, &items);
        // 滚动到底部时首项 = total - 视口容纳项数（600px 视口、~43px 项 → ≈987）
        assert!(
            state.first_visible() >= 980,
            "越界跳转应 clamp 到末尾，实际 {}",
            state.first_visible()
        );
        // `offset` 在内容底部附近。这里的下界按**真实**内容高算：1000 项 × ~43px - 600px 视口
        // ≈ 42400。原断言是 >45000——那是 48px 平坦估算（1000 × 48 - 600 = 47400）的产物；
        // 现在内容高按已测行高的中位数计价（`coverage_height`），估算偏高 12% 的偏差被去掉。
        assert!(
            state.offset() > 41000.0,
            "offset 应接近内容底部，实际 {}",
            state.offset()
        );
        // 顶部跳转
        state.scroll_to_item(0, 0.0);
        let (_px2, _w2) = render_lazy_state(&state, &items);
        assert_eq!(state.first_visible(), 0, "跳回顶部");
    }

    #[test]
    fn jump_to_end_shows_last_item_with_real_viewport() {
        // 回归（用户实测：跳末尾看不到 Item 999）：measure_node 对 scroll 容器
        // 一律把 max_height 改写成 f32::MAX → policy 永远拿不到真实视口 → 回退
        // vh=600，而真实视口 400 → clamp 的 max_offset = content_h - 600 偏大
        // 100px → 跳末尾滚过头，Item 999 被推出视口底部。
        // 修复：lazy 容器保留有限 max_height（policy 显式控制子约束）。
        let state = LazyListState::new();
        let items: Arc<Vec<u64>> = Arc::new((0..1000).collect());
        let mk = |state: &LazyListState, items: &Arc<Vec<u64>>| {
            let s = state.clone();
            let items = items.clone();
            render_lazy_sized(
                move |ctx| {
                    LazyColumn::new()
                        .state(s)
                        .modifier(Modifier::new().fill_max_width().fill_max_height())
                        .items_from(items, |v: &u64| *v, |ctx, _i, v| {
                            crate::components::text::Text::new(format!("Item {}", v))
                                .font_size(14.0)
                                .modifier(Modifier::new().padding(12.0))
                                .build(ctx);
                        })
                        .build(ctx);
                },
                400.0,
                400.0,
            )
        };
        let (px0, w0) = mk(&state, &items);
        state.scroll_to_item(999, 0.0);
        let (px1, w1) = mk(&state, &items);
        assert_eq!(w0, w1);
        // 视口底部 60px 内应有文本（Item 999 完整可见）
        let h = 400usize;
        let mut bottom_text_rows = 0;
        for y in (h - 60)..h {
            let mut has = false;
            for x in (0..w0).step_by(4) {
                let p = px1[y * w0 + x];
                has |= p[0] < 120 && p[1] < 120 && p[2] < 120;
            }
            if has { bottom_text_rows += 1; }
        }
        assert!(
            bottom_text_rows > 10,
            "末尾项应在视口底部可见，bottom_text_rows={bottom_text_rows}"
        );
        // 首项应接近底部：视口 400 / ~47.5 ≈ 8.4 项 → first_visible ≈ 991
        assert!(
            state.first_visible() >= 985,
            "first_visible={}",
            state.first_visible()
        );
    }

    // ── fling 惯性滚动：偏移推进 + 撞极限停止 ──
    /// 泵全局动画直到 offset 收敛（两次读数差 < 0.5）。
    /// ⚠ 不用 `has_animation_for_state` 做循环条件：未持串行锁时，并行测试的
    /// update_animations 会整表取走动画（短暂空窗）→ 误判结束（实测并行下
    /// fling 停在中途 836）。调用方若已持 TEST_SERIAL，可用下方的
    /// `step_animations_until_target`（表空 + 距目标双条件，无空窗问题）。
    fn step_animations_until_done(state: &LazyListState, max_frames: usize) -> usize {
        use std::time::Duration;
        let mut frames = 0;
        let mut last = state.offset();
        loop {
            crate::animation::update_animations();
            std::thread::sleep(Duration::from_millis(20));
            let v = state.offset();
            frames += 1;
            let settled = (v - last).abs() < 0.5 && frames > 5;
            if settled || frames >= max_frames {
                break;
            }
            last = v;
        }
        frames
    }

    /// 泵全局动画直到 offset 距目标 < 5.0 且动画表排空（精确目标场景专用——
    /// 如 animate_scroll_to_item，目标由 scroll_to_item 同缓存推导，round-trip 精确）。
    ///
    /// 背景：spring 尾段每帧位移天然 < 0.5（threshold 0.01，50 倍间隙）——
    /// `step_animations_until_done` 的位移判据会在引擎真正 done（写精确目标）
    /// 之前提前触发，残余 ~13px 没收完（全量负载抖动时偶发超最终断言 5px 阈）。
    /// 本函数要求调用方已持 TEST_SERIAL（串行下无跨测试取表/清表干扰，
    /// `has_animation_for_state` 无空窗误判），以"表空 + 距目标"双条件收敛。
    fn step_animations_until_target(
        state: &LazyListState,
        target: f32,
        max_frames: usize,
    ) -> usize {
        use std::time::Duration;
        let mut frames = 0;
        loop {
            crate::animation::update_animations();
            std::thread::sleep(Duration::from_millis(20));
            let v = state.offset();
            frames += 1;
            let sid = state.offset.state_id();
            let drained = !crate::animation::has_animation_for_state(sid);
            if (drained && (v - target).abs() < 5.0) || frames >= max_frames {
                break;
            }
        }
        frames
    }

    #[test]
    fn fling_animates_offset_and_stops_at_limit() {
        // 动画注册表全局共享——持串行锁防并行 clear/竞态抹掉在飞 fling
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        // 渲染一次让 policy 回写 fling_limit（内容高 - 视口 600）
        let state = LazyListState::new();
        let items: Arc<Vec<u64>> = Arc::new((0..1000).collect());
        let (_px, _w) = render_lazy_state(&state, &items);
        let limit = state.fling_limit.get();
        assert!(limit > 10000.0, "1000 项内容高 - 视口应很大，实际 {limit}");

        // 自然停：v0=5000 → 衰减极限 5000/4.2 ≈ 1190 < limit
        state.fling(5000.0);
        let frames = step_animations_until_done(&state, 300);
        assert!(frames < 300, "fling 应收敛（{frames} 帧）");
        let end = state.offset();
        assert!(
            end > 1000.0 && end < 1400.0,
            "fling 应推进 offset 到衰减极限附近，实际 {end}"
        );

        // 撞极限：超大速度 → 停在 fling_limit（对齐 Compose：fling 消耗完即停）
        state.fling(1_000_000.0);
        let frames = step_animations_until_done(&state, 300);
        assert!(frames < 300, "撞极限 fling 应收敛（{frames} 帧）");
        let end2 = state.offset();
        assert!(
            (end2 - limit).abs() < 1.0,
            "撞极限应停在 {limit}，实际 {end2}"
        );

        // 反向：负速度向下甩 → clamp 回 0
        state.fling(-1_000_000.0);
        let frames = step_animations_until_done(&state, 300);
        assert!(frames < 300, "反向 fling 应收敛（{frames} 帧）");
        assert_eq!(state.offset(), 0.0, "反向 fling 应停在顶部");
    }

    // ═══════════════════════════════════════════════════
    // sticky header（对齐 Compose stickyHeader）
    // ═══════════════════════════════════════════════════

    /// 5 个 section：sticky header（红色背景 + HEAD n 文本）后跟 19 个普通项。
    /// header 全局 index = n*20；普通项 key = 200 + n*19 + i（全列表唯一）。
    fn sticky_build(ctx: &mut ComposeCtx, state: LazyListState) {
        use crate::graphics::{Color, Shape};
        let mut lb = LazyColumn::new()
            .state(state)
            .modifier(Modifier::new().fill_max_width().fill_max_height());
        for s in 0..5u64 {
            lb = lb.sticky_header(s, move |ctx| {
                crate::components::text::Text::new(format!("HEAD {s}"))
                    .font_size(18.0)
                    .modifier(Modifier::new().padding(12.0).background(
                        Color::from_argb(255, 0xC6, 0x28, 0x28),
                        Shape::rounded(2.0),
                    ))
                    .build(ctx);
            });
            lb = lb.items(
                19,
                move |i| 200 + s * 19 + i as u64,
                move |ctx, i| {
                    crate::components::text::Text::new(format!("Item {}", s * 19 + i as u64))
                        .font_size(14.0)
                        .modifier(Modifier::new().padding(12.0))
                        .build(ctx);
                },
            );
        }
        lb.build(ctx);
    }

    /// 带 content_padding 的 1000 项列表（对齐 render_lazy 的 400x600 视口）
    fn plain_build_padded(ctx: &mut ComposeCtx, state: LazyListState, pad: (f32, f32), cross: (f32, f32)) {
        let items: Arc<Vec<u64>> = Arc::new((0..1000).collect());
        LazyColumn::new()
            .state(state)
            .content_padding(pad.0, pad.1)
            .content_padding_cross(cross.0, cross.1)
            .modifier(Modifier::new().fill_max_width().fill_max_height())
            .items_from(items, |v: &u64| *v, |ctx, _i, v| {
                crate::components::text::Text::new(format!("Item {v}"))
                    .font_size(14.0)
                    .modifier(Modifier::new().padding(12.0))
                    .build(ctx);
            })
            .build(ctx);
    }

    /// 带 content_padding 的 5 sections 红底 sticky 列表（header 全局 index = n*20）
    fn sticky_build_padded(ctx: &mut ComposeCtx, state: LazyListState, pad: f32) {
        use crate::graphics::{Color, Shape};
        let mut lb = LazyColumn::new()
            .state(state)
            .content_padding(pad, 0.0)
            .modifier(Modifier::new().fill_max_width().fill_max_height());
        for s in 0..5u64 {
            lb = lb.sticky_header(s, move |ctx| {
                crate::components::text::Text::new(format!("HEAD {s}"))
                    .font_size(18.0)
                    .modifier(Modifier::new().padding(12.0).background(
                        Color::from_argb(255, 0xC6, 0x28, 0x28),
                        Shape::rounded(2.0),
                    ))
                    .build(ctx);
            });
            lb = lb.items(
                19,
                move |i| 200 + s * 19 + i as u64,
                move |ctx, i| {
                    crate::components::text::Text::new(format!("Item {}", s * 19 + i as u64))
                        .font_size(14.0)
                        .modifier(Modifier::new().padding(12.0))
                        .build(ctx);
                },
            );
        }
        lb.build(ctx);
    }

    /// 顶部红色 header 带数量：连续红色行段（header 背景）计数
    fn red_bands(px: &[[u8; 4]], w: usize, h: usize, limit: usize) -> usize {
        let mut bands = 0;
        let mut in_band = false;
        for y in 0..limit.min(h) {
            let mut red = false;
            for x in (0..w).step_by(2) {
                let p = px[y * w + x];
                // ⚠ raster 是 BGRA8888（n32_premul 小端）：p[2] 才是红通道
                if p[2] > 150 && p[0] < 100 && p[1] < 100 {
                    red = true;
                    break;
                }
            }
            if red && !in_band {
                bands += 1;
                in_band = true;
            } else if !red {
                in_band = false;
            }
        }
        bands
    }

    /// 视口顶连续红色行数（钉住 header 的可见高度；BGRA 字节序）
    fn top_band_height(px: &[[u8; 4]], w: usize, h: usize) -> usize {
        let mut rows = 0;
        for y in 0..h {
            let mut red = false;
            for x in (0..w).step_by(2) {
                let p = px[y * w + x];
                if p[2] > 150 && p[0] < 100 && p[1] < 100 {
                    red = true;
                    break;
                }
            }
            if red {
                rows += 1;
            } else {
                break;
            }
        }
        rows
    }

    #[test]
    fn sticky_header_pins_to_top_while_scrolling() {
        // 深滚动后：钉住的 header 成为锚点（first_visible = header index），
        // 红色带仍在视口顶，普通项内容从 header 下面滑过（下方仍有文本像素）
        let state = LazyListState::new();
        let (px0, w0) = render_lazy(|ctx| sticky_build(ctx, state.clone()));
        assert_eq!(state.first_visible(), 0, "初始锚点 = header 0");
        assert_eq!(state.first_visible_offset.get(), 0.0);

        // 滚动进 section 3（offset 2000：C(40)≈1900 ≤ 2000 → header 2 钉住）
        state.offset.set(2000.0);
        let (px1, w1) = render_lazy(|ctx| sticky_build(ctx, state.clone()));
        assert_eq!(w0, w1);
        assert_eq!(
            state.first_visible(),
            40,
            "钉住的 header 成为锚点（对齐 Compose firstVisibleItemIndex）"
        );
        assert_eq!(state.first_visible_offset.get(), 0.0);
        // 红色带：恰一个，且在视口顶（钉住）
        assert_eq!(red_bands(&px1, w1, 600, 300), 1, "钉住时视口顶恰一个 header");
        let mut top_red = false;
        for x in (0..w1).step_by(2) {
            let p = px1[x];
            if p[2] > 150 && p[0] < 100 && p[1] < 100 {
                top_red = true;
            }
        }
        assert!(top_red, "钉住的 header 在视口最顶行");
        // 内容从 header 下滑过：header 带下方仍有普通项文本
        let mut below_text = false;
        for y in (70..600).step_by(2) {
            for x in (0..w1).step_by(2) {
                let p = px1[y * w1 + x];
                if p[0] < 120 && p[1] < 120 && p[2] < 120 {
                    below_text = true;
                }
            }
        }
        assert!(below_text, "普通项内容应继续渲染在钉住 header 之下");
        // 与初始帧内容不同（滚动到不同 section）
        let mut diff_rows = 0;
        for y in 0..600usize {
            let mut has0 = false;
            let mut has1 = false;
            for x in (0..w0).step_by(8) {
                let p0 = px0[y * w0 + x];
                let p1 = px1[y * w0 + x];
                has0 |= p0[0] < 120 && p0[1] < 120 && p0[2] < 120;
                has1 |= p1[0] < 120 && p1[1] < 120 && p1[2] < 120;
            }
            if has0 != has1 {
                diff_rows += 1;
            }
        }
        assert!(diff_rows > 10, "滚动后内容变化，diff_rows={diff_rows}");

        // 深滚动（超界 → clamp 到 4150 ≈ 末尾）：header 4 钉住——验证无界回溯
        state.offset.set(100_000.0);
        let (_px2, _w2) = render_lazy(|ctx| sticky_build(ctx, state.clone()));
        assert!(
            state.offset() < 4500.0,
            "越界应 clamp，实际 {}",
            state.offset()
        );
        assert_eq!(
            state.first_visible(),
            80,
            "深滚动后 header 4（全局 80）钉住——无界回溯",
        );
    }

    #[test]
    fn sticky_header_pushes_previous_on_approach() {
        // 下一个 header 距顶 h(prev) 内时，前一个开始滑出（两 header 无缝相邻，
        // 视觉上是一条连续红带）→ 断言**顶带高度变矮**；到位后前一个完全推出
        let state = LazyListState::new();
        let (px0, w0) = render_lazy(|ctx| sticky_build(ctx, state.clone()));
        let h0_full = top_band_height(&px0, w0, 600);
        assert!(h0_full > 40, "初始 header 0 完整钉在顶，顶带高 {h0_full}");

        // ⚠ header 顶 C(20) 依赖运行时实测高度（字体加载时序不同 → 40 或 47.5），
        // 过渡窗口 [C(20)-h(0), C(20)] 也随之漂移——用扫描区间断言不变量：
        // 1) 过渡期：被推的 header 0 与 header 1 无缝相邻 → 顶带"变高"
        //    （band ∈ (h, 2h)，两 header 合并）
        // 2) 越过过渡区后 header 1 钉顶（锚点 = 20、单带、顶带恢复 h）
        let mut saw_merge = false;
        let mut saw_pinned = false;
        let mut final_band = 0usize;
        for off in (800..1010).step_by(5) {
            state.offset.set(off as f32);
            let (px, w) = render_lazy(|ctx| sticky_build(ctx, state.clone()));
            let band = top_band_height(&px, w, 600);
            if band > h0_full + 8 && band < 2 * h0_full - 8 {
                saw_merge = true;
            }
            if off >= 985 {
                if state.first_visible() == 20 && red_bands(&px, w, 600, 200) == 1 {
                    saw_pinned = true;
                    final_band = band;
                }
            }
        }
        assert!(saw_merge, "过渡期顶带应变高（两 header 合并，band > 单 header）");
        assert!(saw_pinned, "越过过渡区后 header 1 应钉顶（锚点 20 + 单带）");
        let final_diff = if final_band > h0_full { final_band - h0_full } else { h0_full - final_band };
        assert!(final_diff < 6, "header 1 完整钉顶（顶带 {final_band} ≈ {h0_full}）");
    }

    #[test]
    fn sticky_header_composes_only_pinned_and_visible() {
        // 组合项数：视口 600px ≈ 12 项 + 预取 8 + 钉住 header —— 远小于 total 100
        // （sticky 回溯不拉全列表）
        let composed = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let state = LazyListState::new();
        let c0 = composed.clone();
        let s0 = state.clone();
        render_lazy(move |ctx| {
            let mut lb = LazyColumn::new()
                .state(s0.clone())
                .modifier(Modifier::new().fill_max_width().fill_max_height());
            for s in 0..5u64 {
                let c = c0.clone();
                lb = lb.sticky_header(s, move |ctx| {
                    c.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    crate::components::text::Text::new(format!("HEAD {s}"))
                        .font_size(18.0)
                        .modifier(Modifier::new().padding(12.0))
                        .build(ctx);
                });
                lb = lb.items(
                    19,
                    move |i| 200 + s * 19 + i as u64,
                    |ctx, i| {
                        crate::components::text::Text::new(format!("Item {i}"))
                            .font_size(14.0)
                            .modifier(Modifier::new().padding(12.0))
                            .build(ctx);
                    },
                );
            }
            lb.build(ctx);
        });
        let n0 = composed.load(std::sync::atomic::Ordering::Relaxed);
        assert!(n0 <= 40, "初始组合应只含可见项+预取，实际 {n0}");
        // 深滚动：钉住的 header 加入组合但列表不整体组合
        state.offset.set(2000.0);
        let (px, w) = render_lazy(|ctx| sticky_build(ctx, state.clone()));
        let n1 = composed.load(std::sync::atomic::Ordering::Relaxed);
        assert!(n1 <= 60, "深滚动组合项数应仍远小于 100，实际 {n1}");
        assert_eq!(red_bands(&px, w, 600, 200), 1);
    }

    // ═══════════════════════════════════════════════════
    // contentPadding（对齐 Compose contentPadding）
    // ═══════════════════════════════════════════════════

    #[test]
    fn content_padding_starts_content_after_padding() {
        // pad_before=50：offset 0 时首项内容从 y≈68 开始（50 + padding 12 + glyph），
        // 顶部 20..30 行无文本；offset=50（内容滚进 padding）后首项贴顶，文本进入 18..30
        let state = LazyListState::new();
        let (px0, w0) = render_lazy(|ctx| plain_build_padded(ctx, state.clone(), (50.0, 0.0), (0.0, 0.0)));
        let mut top_empty = true;
        for y in 20..30usize {
            for x in (0..w0).step_by(2) {
                let p = px0[y * w0 + x];
                if p[0] < 120 && p[1] < 120 && p[2] < 120 { top_empty = false; }
            }
        }
        assert!(top_empty, "offset 0 时顶部 20..30 行应为空白（内容从 pad 50 后开始）");

        state.offset.set(50.0);
        let (px1, w1) = render_lazy(|ctx| plain_build_padded(ctx, state.clone(), (50.0, 0.0), (0.0, 0.0)));
        let mut top_text = false;
        for y in 18..30usize {
            for x in (0..w1).step_by(2) {
                let p = px1[y * w1 + x];
                if p[0] < 120 && p[1] < 120 && p[2] < 120 { top_text = true; }
            }
        }
        assert!(top_text, "offset=50 后首项应贴顶（文本进入 y 18..30）");
        // 滚动前项顶在 y=50：自然边界处内容停在内边距处，不贴视口边
        assert_eq!(state.first_visible(), 0, "pad 50 + offset 50 → 锚点 index 0");
    }

    #[test]
    fn content_padding_max_offset_includes_after() {
        // pad_after=40：跳末尾时 max_offset = pad_before + Σ项 + pad_after - viewport，
        // 末项底停在 viewport-40 处 → 底部 35 行无文本
        let state = LazyListState::new();
        state.scroll_to_item(99999, 0.0);
        let (px, w) = render_lazy(|ctx| plain_build_padded(ctx, state.clone(), (0.0, 40.0), (0.0, 0.0)));
        assert!(state.offset() > 20_000.0, "跳末尾应滚到接近末尾，实际 {}", state.offset());
        let first = state.first_visible();
        assert!(first >= 970, "first_visible 应接近末尾，实际 {first}");
        let mut bottom_empty = true;
        for y in (600 - 35)..600usize {
            for x in (0..w).step_by(2) {
                let p = px[y * w + x];
                if p[0] < 120 && p[1] < 120 && p[2] < 120 { bottom_empty = false; }
            }
        }
        assert!(bottom_empty, "pad_after=40：底部 35 行应为空白（末项停在 padding 处）");
    }

    #[test]
    fn content_padding_sticky_pins_at_padding() {
        // pad=50：深滚动后 header 4（全局 80）钉在视口 y=50（不是 0）：
        // 顶部 45 行无红，红带在 y 50..98
        let state = LazyListState::new();
        state.offset.set(3900.0); // C(80)=3800（h=47.5）→ header 4 钉住；且 < max_off≈4200 不 clamp
        let (px, w) = render_lazy(|ctx| sticky_build_padded(ctx, state.clone(), 50.0));
        let mut top_clear = true;
        for y in 0..45usize {
            for x in (0..w).step_by(2) {
                let p = px[y * w + x];
                if p[2] > 150 && p[0] < 100 && p[1] < 100 { top_clear = false; }
            }
        }
        assert!(top_clear, "pad=50：顶部 45 行应为空白（header 钉在 y=50）");
        let mut band_red = false;
        for y in 50..98usize {
            for x in (0..w).step_by(2) {
                let p = px[y * w + x];
                if p[2] > 150 && p[0] < 100 && p[1] < 100 { band_red = true; }
            }
        }
        assert!(band_red, "y 50..98 应为红色钉住 header");
        assert_eq!(state.first_visible(), 80, "钉住时锚点 = 钉住的 header（全局 80）");
        // 红带不再覆盖视口顶 0..40
        assert_eq!(red_bands(&px, w, 600, 200), 1, "仅一条红带（钉住的 header）");
    }

    #[test]
    fn content_padding_cross_axis_shrinks_items() {
        // cross pad 40/40：item 从 x=40 开始放置且宽被限制在 400-80=320：
        // x<36 无文本；文本起点 ≈ x 52..92
        let state = LazyListState::new();
        let (px, w) = render_lazy(|ctx| plain_build_padded(ctx, state.clone(), (0.0, 0.0), (40.0, 40.0)));
        let mut left_empty = true;
        for y in (0..600).step_by(2) {
            for x in 0..36usize {
                let p = px[y * w + x];
                if p[0] < 120 && p[1] < 120 && p[2] < 120 { left_empty = false; }
            }
        }
        assert!(left_empty, "交叉轴 padding 40：x<36 应无文本");
        let mut text_after_pad = false;
        for y in (0..600).step_by(2) {
            for x in 56..70usize {
                let p = px[y * w + x];
                if p[0] < 120 && p[1] < 120 && p[2] < 120 { text_after_pad = true; }
            }
        }
        assert!(text_after_pad, "文本应从 x≈54 开始（40 cross + 12 padding）");
    }

    // ═══════════════════════════════════════════════════════
    // reverseLayout（对齐 Compose reverseLayout：index 0 在底部）
    // ═══════════════════════════════════════════════════════

    fn reverse_build(ctx: &mut ComposeCtx, state: LazyListState) {
        let items: Arc<Vec<u64>> = Arc::new((0..1000).collect());
        LazyColumn::new()
            .state(state)
            .reverse_layout(true)
            .modifier(Modifier::new().fill_max_width().fill_max_height())
            .items_from(items, |v: &u64| *v, |ctx, _i, v| {
                crate::components::text::Text::new(format!("Item {}", v))
                    .font_size(14.0)
                    .modifier(Modifier::new().padding(12.0))
                    .build(ctx);
            })
            .build(ctx);
    }

    /// 指定行区间内是否有暗色文本像素
    fn rows_have_text(px: &[[u8; 4]], w: usize, y0: usize, y1: usize) -> bool {
        for y in y0..y1 {
            for x in (0..w).step_by(2) {
                let p = px[y * w + x];
                if p[0] < 120 && p[1] < 120 && p[2] < 120 { return true; }
            }
        }
        false
    }

    #[test]
    fn reverse_layout_starts_at_bottom_and_scrolls_toward_end() {
        // offset=0：项 0 在屏幕底（first_visible = 0），底部有文本、顶部也有
        // （视口显示列表开头 ~12 项，index 0 在最下）
        let state = LazyListState::new();
        let (px0, w0) = render_lazy(|ctx| reverse_build(ctx, state.clone()));
        assert_eq!(state.first_visible(), 0, "offset=0 锚点 = 项 0（屏幕底）");
        assert!(rows_have_text(&px0, w0, 570, 600), "项 0 应在屏幕底部可见");
        assert!(rows_have_text(&px0, w0, 0, 30), "屏幕顶也应显示列表开头内容（~项 12）");

        // offset 增 = 向列表末尾（index 增）——与正向滚动方向一致
        state.offset.set(5000.0);
        let (px1, w1) = render_lazy(|ctx| reverse_build(ctx, state.clone()));
        let first = state.first_visible();
        assert!(first >= 90, "offset 5000 后锚点应推进到 ~105，实际 {first}");
        // 底部项 = 锚点项（屏幕底显示 index ~105），顶部 = index ~117
        assert!(rows_have_text(&px1, w1, 0, 30), "反向滚动后屏幕顶仍有内容");
        // 帧间内容变化（滚到不同项）
        let mut diff_rows = 0;
        for y in 0..600usize {
            let mut h0 = false; let mut h1 = false;
            for x in (0..w0).step_by(8) {
                let p0 = px0[y * w0 + x];
                let p1 = px1[y * w0 + x];
                h0 |= p0[0] < 120 && p0[1] < 120 && p0[2] < 120;
                h1 |= p1[0] < 120 && p1[1] < 120 && p1[2] < 120;
            }
            if h0 != h1 { diff_rows += 1; }
        }
        assert!(diff_rows > 10, "反向滚动后内容变化，diff_rows={diff_rows}");
    }

    #[test]
    fn reverse_layout_scroll_to_item_lands_at_bottom() {
        // 跳 500：项 500 **底**贴视口底（与正向"顶贴顶"镜像）→ 底部行有文本；
        // 锚点 = 500（屏幕底项）
        let state = LazyListState::new();
        let items: Arc<Vec<u64>> = Arc::new((0..1000).collect());
        // 正向对照：项 500 贴屏幕**顶**
        let fwd = LazyListState::new();
        let (pf, wf) = render_lazy(|ctx| {
            let it = items.clone();
            let s = fwd.clone();
            LazyColumn::new()
                .state(s)
                .modifier(Modifier::new().fill_max_width().fill_max_height())
                .items_from(it, |v: &u64| *v, |ctx, _i, v| {
                    crate::components::text::Text::new(format!("Item {}", v))
                        .font_size(14.0)
                        .modifier(Modifier::new().padding(12.0))
                        .build(ctx);
                })
                .build(ctx);
        });
        fwd.scroll_to_item(500, 0.0);
        let (pf2, wf2) = render_lazy(|ctx| {
            let it = items.clone();
            let s = fwd.clone();
            LazyColumn::new()
                .state(s)
                .modifier(Modifier::new().fill_max_width().fill_max_height())
                .items_from(it, |v: &u64| *v, |ctx, _i, v| {
                    crate::components::text::Text::new(format!("Item {}", v))
                        .font_size(14.0)
                        .modifier(Modifier::new().padding(12.0))
                        .build(ctx);
                })
                .build(ctx);
        });
        let _ = (pf, wf, pf2, wf2);
        assert_eq!(fwd.first_visible(), 500);

        state.scroll_to_item(500, 0.0);
        let (px, w) = render_lazy(|ctx| reverse_build(ctx, state.clone()));
        assert_eq!(state.first_visible(), 500, "反向跳 500：锚点 = 500（屏幕底项）");
        assert!(rows_have_text(&px, w, 570, 600), "项 500 底应贴视口底（底部行有文本）");
        // 顶部也有内容（项 500 上方 ~12 项）
        assert!(rows_have_text(&px, w, 0, 40), "项 500 上方的项应在屏幕顶可见");
        // offset = prefix(500) ≈ 23750（0 spacing + 无 padding）
        assert!(
            state.offset() > 20_000.0 && state.offset() < 26_000.0,
            "反向 jump offset = prefix(500)，实际 {}",
            state.offset()
        );
    }

    #[test]
    fn reverse_layout_clamps_at_both_ends() {
        // 越界（offset=100000）→ clamp 到 max_off = content_h - vh ≈ 46900 → 锚点 ≈ 987
        let state = LazyListState::new();
        state.offset.set(100_000.0);
        // 首帧：build 用未 clamp 的 offset 锚定（组合窗偏窄）；第二帧 offset 已被
        // policy clamp → 组合窗收敛（真实 app 首帧后自然收敛，测试断言收敛帧）
        let (_px0, _w0) = render_lazy(|ctx| reverse_build(ctx, state.clone()));
        let (px, w) = render_lazy(|ctx| reverse_build(ctx, state.clone()));
        let first = state.first_visible();
        assert!(first >= 970, "越界 clamp 后应接近列表末尾（index 大），实际 {first}");
        assert!(rows_have_text(&px, w, 570, 600), "末尾项应在屏幕底可见");
        // 回到开头
        state.offset.set(0.0);
        let (_px2, _w2) = render_lazy(|ctx| reverse_build(ctx, state.clone()));
        assert_eq!(state.first_visible(), 0, "offset=0 回到项 0");
    }

    // ── animateScrollToItem（对齐 Compose animateScrollToItem——spring 动画）──
    #[test]
    fn animate_scroll_to_item_animates_offset() {
        // 动画注册表全局共享——持串行锁（step_animations_until_target 的
        // has_animation_for_state 无空窗误判依赖串行，见 helper 注释）
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let state = LazyListState::new();
        let items: Arc<Vec<u64>> = Arc::new((0..1000).collect());
        let (_px, _w) = render_lazy_state(&state, &items);
        let before = state.offset();
        assert_eq!(before, 0.0);

        // 立即跳转测出精确目标（实测高度：prefix(500)——运行时字体高度不同，
        // 不能用常数 23750 断言）
        state.scroll_to_item(500, 0.0);
        let (_px, _w) = render_lazy_state(&state, &items);
        let target = state.offset();
        assert!(target > 15_000.0 && target < 30_000.0, "目标应 ≈ prefix(500)，实际 {target}");

        // 回到 0 再动画滚动到同一目标
        state.offset.set(0.0);
        let (_px, _w) = render_lazy_state(&state, &items);
        state.animate_scroll_to_item(500, 0.0);
        let (_px, _w) = render_lazy_state(&state, &items);
        // 测量期消费：注册 spring 动画（push_animatable 立即 update 一帧——
        // elapsed≈0 位移≈0，之后由 step 循环推进）。目标由 scroll_to_item
        // 同缓存推导、round-trip 精确——用表空+距目标双条件收敛（位移判据
        // 会在引擎 done 前提前触发，残余 ~13px，见 helper 注释）。
        let frames = step_animations_until_target(&state, target, 400);
        assert!(frames < 400, "spring 应收敛（{frames} 帧）");
        let end = state.offset();
        assert!(
            (end - target).abs() < 5.0,
            "动画应收敛到 scroll_to_item 同一目标 {target}，实际 {end}"
        );
    }

    /// A programmatic animated jump RAISES `is_scroll_in_progress` for the length of its spring, rather
    /// than clearing it as it used to.
    ///
    /// Both halves are load-bearing, and each pins a different decision in the measure's `animate`
    /// branch:
    ///
    /// * **Up while it runs.** That flag is the same `State` the drag and fling paths write (`build`
    ///   hands this clone to the list's `ScrollState`), and it is what `sync_month_pages` reads through
    ///   `is_scrolling()` for Compose's `!isScrollInProgress` guard (`DatePicker.kt:1548-1553`). Clearing
    ///   it here left that guard inert: an outside change landing mid-animation would `scroll_to_item`
    ///   and `cancel_animation` straight through the motion the user had just asked for. Drop the
    ///   `in_progress.set(true)` and the mid-spring assertion below goes red.
    /// * **Down again afterwards.** The clearing rides on the animation's completion callback, so a
    ///   variant that does not always call it would latch the flag on with nothing left to clear it —
    ///   and a latched flag freezes the month sync (see the bound in `date_picker.rs`). The
    ///   already-at-target case is the one that distinguishes the variants: plain
    ///   `push_animatable_with_done` returns WITHOUT firing `done` when the offset already equals the
    ///   target (`animation.rs:475-480`) and also when an identical motion is already running
    ///   (`:491-494`), while the velocity variant reports completion in both (`:195-198`, `:212-215`).
    #[test]
    fn an_animated_jump_holds_is_scroll_in_progress_for_the_length_of_its_spring() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let state = LazyListState::new();
        let items: Arc<Vec<u64>> = Arc::new((0..1000).collect());
        let (_px, _w) = render_lazy_state(&state, &items);
        assert!(!state.is_scrolling(), "a list nobody has touched is not scrolling");

        // The target is derived the same way the jump derives it, so the assertion is about the flag
        // and not about a guessed offset.
        state.scroll_to_item(500, 0.0);
        let (_px, _w) = render_lazy_state(&state, &items);
        let target = state.offset();
        state.offset.set(0.0);
        let (_px, _w) = render_lazy_state(&state, &items);
        assert!(!state.is_scrolling(), "an instantaneous jump has no motion to report");

        state.animate_scroll_to_item(500, 0.0);
        let (_px, _w) = render_lazy_state(&state, &items);
        // The measure consumed the request and registered the spring; one `update` has already run, so
        // the motion is genuinely in flight rather than merely queued.
        assert!(
            crate::animation::has_animation_for_state(state.offset.state_id()),
            "the animated jump must have registered a spring, or the rest of this test is vacuous"
        );
        assert!(
            (state.offset() - target).abs() > 100.0,
            "and it must not have arrived yet (offset {} of {target}) — otherwise there is no window \
             for the flag to be up in",
            state.offset()
        );
        assert!(
            state.is_scrolling(),
            "an animated jump is a scroll in progress: the flag has to be UP for the length of the \
             spring, or the `!isScrollInProgress` guard it feeds cannot see the motion it protects"
        );

        step_animations_until_target(&state, target, 400);
        assert!(
            !state.is_scrolling(),
            "and the flag comes back down when the spring lands — a latched `true` would freeze every \
             sync that waits on it"
        );

        // The variant choice, pinned where it is observable: a request for the page the list is already
        // on runs no animation, so the completion callback fires immediately and the flag is never left
        // up. With the plain `push_animatable_with_done` the `set(true)` above would have nothing to
        // clear it and this assertion would hang on `true` forever.
        let settled = LazyListState::new();
        let (_px, _w) = render_lazy_state(&settled, &items);
        settled.animate_scroll_to_item(0, 0.0);
        let (_px, _w) = render_lazy_state(&settled, &items);
        assert!(
            !crate::animation::has_animation_for_state(settled.offset.state_id()),
            "jumping to the page already on screen registers no motion"
        );
        assert!(
            !settled.is_scrolling(),
            "and it must not leave the flag up with no animation to bring it down"
        );
    }

    // ── 重复 key 检测（对齐 Compose：key 冲突抛异常）──
    #[test]
    #[should_panic(expected = "duplicate key")]
    fn duplicate_key_detected_in_rebuild() {
        let mut il = IntervalList::new();
        let c1: Arc<dyn Fn(&mut ComposeCtx, usize) + Send + Sync> = Arc::new(|_, _| {});
        let c2: Arc<dyn Fn(&mut ComposeCtx, usize) + Send + Sync> = Arc::new(|_, _| {});
        il.add(1, Some(Arc::new(|_| 42u64)), c1, false);
        il.add(1, Some(Arc::new(|_| 42u64)), c2, false);
        il.rebuild(); // 应 panic
    }

    // ── measure/build convergence（审计 Phase 3.2）：窗口 resize 放大后，build
    //    是否按真实视口收敛可见范围？──
    /// 同一 Composer 跨多帧渲染（模拟窗口 resize 后同一列表的收敛）。
    /// 每帧对 `heights[i]` 做 compose+layout；渲染最后帧，返回视口底部 60px
    /// 内是否有文本（判断可见项是否被 build 组合覆盖）。
    fn render_lazy_frames_bottom_text(build: impl Fn(&mut ComposeCtx, &LazyListState) + Send, heights: &[f32]) -> bool {
        use skia_safe::{Color as SkColor, surfaces};
        let theme = crate::theme::ThemeColors::light_from_seed(0x6750A4);
        let state = LazyListState::new();
        let mut composer = Composer::new();
        let last_h = *heights.last().unwrap_or(&400.0);
        for &h in heights {
            composer.compose(|ctx| crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| build(ctx, &state)));
            composer.layout(Constraints::new(0.0, 400.0, 0.0, h));
        }
        let mut surface = surfaces::raster_n32_premul((400, last_h as i32)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(SkColor::WHITE);
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        crate::render::render(nodes, root, canvas);
        let pm = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        let w = 400usize;
        let h = last_h as usize;
        // 检查视口底部 60px 是否有文本行（可见项若被 build 组合覆盖则有文本）
        let mut text_rows = 0;
        for y in (h.saturating_sub(60))..h {
            let mut has = false;
            for x in (0..w).step_by(4) {
                let p = px[y * w + x];
                has |= p[0] < 120 && p[1] < 120 && p[2] < 120;
            }
            if has { text_rows += 1; }
        }
        text_rows > 5
    }

    #[test]
    fn resize_to_larger_viewport_converges_to_cover_visible_items() {
        // 视口从 400 放大到 2500（超过原固定 2000 估算窗）。方向一后 build 感知
        // 真实视口（跨帧信号），经多帧收敛后底部可见项应被组合覆盖。
        // 帧序：400（初始）→ 2500（resize）→ 2500（收敛帧——build 读到 2500）。
        let items: Arc<Vec<u64>> = Arc::new((0..200).collect());
        let covered = render_lazy_frames_bottom_text(
            |ctx, _state| {
                LazyColumn::new()
                    .modifier(Modifier::new().fill_max_width().fill_max_height())
                    .items_from(items.clone(), |v: &u64| *v, |ctx, _i, v| {
                        crate::components::text::Text::new(format!("Item {}", v))
                            .font_size(14.0)
                            .modifier(Modifier::new().padding(12.0))
                            .build(ctx);
                    })
                    .build(ctx);
            },
            &[400.0, 2500.0, 2500.0],
        );
        eprintln!("resize 到 2500 并收敛后视口底部是否被可见项覆盖: {covered}");
        // 方向一实现后：build 感知真实视口，跨帧收敛，底部项应被组合覆盖。
        assert!(covered, "方向一后放大视口应收敛覆盖底部可见项（covered={covered}）");
    }

    /// The trigger the frame handler's same-frame convergence consumes, and what makes it affordable:
    /// a measure that finds its composed window too small for the viewport it is being measured in asks
    /// for one more compose (only a compose can add the missing items), and a measure whose window
    /// covers the viewport asks for nothing.
    ///
    /// Both halves are load-bearing. If the request were never raised the resize lag would come back
    /// with no test turning red; if it were raised on every frame the handler would re-compose the whole
    /// tree per frame, which is why the scrolling frames are asserted too — that is the interaction the
    /// request exists to keep at one pass.
    #[test]
    fn the_window_that_misses_its_viewport_asks_for_a_same_frame_compose() {
        use crate::runtime::composer::take_compose_after_layout;
        let items: Arc<Vec<u64>> = Arc::new((0..200).collect());
        // A `Fn` (not the `FnOnce` `compose` takes) so the same content can run on both passes.
        let build = |ctx: &mut ComposeCtx| {
            LazyColumn::new()
                .modifier(Modifier::new().fill_max_width().fill_max_height())
                .items_from(items.clone(), |v: &u64| *v, |ctx, _i, v| {
                    crate::components::text::Text::new(format!("Item {}", v))
                        .font_size(14.0)
                        .modifier(Modifier::new().padding(12.0))
                        .build(ctx);
                })
                .build(ctx);
        };
        let mut composer = Composer::new();
        // The frame handler's loop, on the trigger the handler actually consumes.
        let converge = |composer: &mut Composer, h: f32| -> usize {
            let mut passes = 0;
            loop {
                composer.compose(&build);
                composer.layout(Constraints::new(0.0, 400.0, 0.0, h));
                passes += 1;
                if !take_compose_after_layout() {
                    return passes;
                }
                assert!(passes < 8, "a short window must converge, not spin");
            }
        };

        // Frame 1 measures for the first time and learns the real viewport, so it is short too.
        let first = converge(&mut composer, 400.0);
        eprintln!("compose+layout passes: first frame {first}");

        // STEADY STATE: a frame that changes nothing must ask for nothing. This is the property that
        // keeps the convergence loop from re-composing the whole tree every frame.
        let steady = converge(&mut composer, 400.0);
        eprintln!("compose+layout passes: settled frame {steady}");
        assert_eq!(
            steady, 1,
            "a settled frame must not ask for another pass, took {steady}"
        );

        // Resize: the window `build` composed was sized for 400 and 2500 needs more items, which only a
        // compose can add. The measure must ASK for that compose — a silent request would leave the loop
        // doing nothing and the resize lag would come back with no test turning red.
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 2500.0));
        assert!(
            take_compose_after_layout(),
            "the resize measure found its window short and must ask for a same-frame compose"
        );
        let after = converge(&mut composer, 2500.0);
        eprintln!("compose+layout passes: resize 400->2500 {after}");
        assert!(
            after <= 4,
            "the resize must converge in a bounded number of passes, took {after}"
        );
        assert_eq!(
            converge(&mut composer, 2500.0),
            1,
            "the frame after the resize must be a single pass again"
        );

        // SCROLL is what decides whether this is affordable: it happens every frame while the user
        // drags, so a second pass there would double the frame's compose+layout. The window covers the
        // viewport on those frames, so the measure must ask for nothing.
        let scroll_state = LazyListState::new();
        let items2: Arc<Vec<u64>> = Arc::new((0..200).collect());
        let build_scroll = {
            let scroll_state = scroll_state.clone();
            move |ctx: &mut ComposeCtx| {
                LazyColumn::new()
                    .state(scroll_state.clone())
                    .modifier(Modifier::new().fill_max_width().fill_max_height())
                    .items_from(items2.clone(), |v: &u64| *v, |ctx, _i, v| {
                        crate::components::text::Text::new(format!("Item {}", v))
                            .font_size(14.0)
                            .modifier(Modifier::new().padding(12.0))
                            .build(ctx);
                    })
                    .build(ctx);
            }
        };
        let mut composer2 = Composer::new();
        let mut converge_scroll = |composer: &mut Composer| {
            let mut n = 0;
            loop {
                composer.compose(&build_scroll);
                composer.layout(Constraints::new(0.0, 400.0, 0.0, 2500.0));
                n += 1;
                if !take_compose_after_layout() || n >= 8 {
                    return n;
                }
            }
        };
        // A fresh composer starts from the state's default viewport (600) while the real one is 2500, so
        // its FIRST frame is the "window is short" case again — measured separately, not part of the
        // scrolling claim.
        let prime = converge_scroll(&mut composer2);
        eprintln!("compose+layout passes: fresh composer first frame {prime}");

        let mut passes_per_frame = Vec::new();
        for _ in 0..5 {
            // One row's worth: 12 padding + 14 text + 12 padding + 10 default spacing.
            let off = scroll_state.offset.get() + 48.0;
            scroll_state.offset.set(off);
            passes_per_frame.push(converge_scroll(&mut composer2));
        }
        eprintln!("compose+layout passes while scrolling one row per frame: {passes_per_frame:?}");
        assert!(
            passes_per_frame.iter().all(|&n| n == 1),
            "a scrolling frame must stay at one compose+layout pass, got {passes_per_frame:?}"
        );
    }

    /// The affordability of the same-frame convergence rests on the window `build` composes being the
    /// window the viewport needs. A never-measured item used to be assumed 48px (`LAZY_ITEM_ESTIMATED_HEIGHT`),
    /// so rows shorter than that made the window come up short and the frame asked for a second compose —
    /// measured on 24px rows (half the estimate) at 2 passes per frame for 10 rows/frame and 3 for 25,
    /// and ~1.5 ms of extra passes on a cold jump in `lazy_column_demo`. `coverage_height` now walks on the
    /// median of the measured heights, and every one of those cases is a single pass again.
    ///
    /// What the test pins is that NO scrolling shape pays for the estimate: the old figures are in the
    /// comment because a regression would come back as exactly those numbers.
    #[test]
    fn the_coverage_estimate_keeps_every_scroll_frame_at_one_pass() {
        use crate::runtime::composer::take_compose_after_layout;
        let state = LazyListState::new();
        let items: Arc<Vec<u64>> = Arc::new((0..400).collect());
        let build = {
            let state = state.clone();
            move |ctx: &mut ComposeCtx| {
                LazyColumn::new()
                    .state(state.clone())
                    .modifier(Modifier::new().fill_max_width().fill_max_height())
                    .items_from(items.clone(), |v: &u64| *v, |ctx, _i, v| {
                        crate::components::text::Text::new(format!("Item {}", v))
                            .font_size(14.0)
                            .modifier(Modifier::new().fill_max_width().height(24.0))
                            .build(ctx);
                    })
                    .build(ctx);
            }
        };
        let mut composer = Composer::new();
        let mut converge = |composer: &mut Composer| {
            let mut n = 0;
            loop {
                composer.compose(&build);
                composer.layout(Constraints::new(0.0, 400.0, 0.0, 600.0));
                n += 1;
                if !take_compose_after_layout() || n >= 8 {
                    return n;
                }
            }
        };
        // The first frame composes from the state's default viewport and converges like any other.
        let first = converge(&mut composer);
        eprintln!("compose+layout passes: 24px rows, first frame {first}");

        // Slow scrolling, wheel-sized steps, and the fast ends that used to cost 2 and 3 passes.
        for step_rows in [1.0f32, 4.0, 10.0, 25.0] {
            let mut passes = Vec::new();
            for _ in 0..6 {
                let off = state.offset.get() + 24.0 * step_rows;
                state.offset.set(off);
                passes.push(converge(&mut composer));
            }
            eprintln!("compose+layout passes at {step_rows} rows/frame (24px rows): {passes:?}");
            assert!(
                passes.iter().all(|&n| n == 1),
                "scrolling {step_rows} rows/frame must stay at one compose+layout pass, got {passes:?}"
            );
        }

        // A JUMP into rows nothing has measured yet is the other shape that used to pay: the whole window
        // is unmeasured, so the estimate governed all of it.
        for jump_to in [200usize, 0, 350, 120] {
            state.scroll_to_item(jump_to, 0.0);
            let passes = converge(&mut composer);
            eprintln!("compose+layout passes jumping to {jump_to}: {passes}");
            assert_eq!(
                passes, 1,
                "a jump to {jump_to} must compose what it needs in one pass, took {passes}"
            );
        }

        // A jump PAST the end is the case the measure clamps: `build` now clamps the request first, with
        // the `max_off` the last measure wrote, so the window opens where the measure will leave the list
        // instead of at a position the measure moves it away from. Measured pass counts for this shape:
        // 3 here (and 4 with the clamp disabled), 2 in `lazy_column_demo` across runs (3 without it,
        // including a cold jump straight to the end) — one full compose+layout round removed, which is
        // the whole justification; the frame's µs move by ~100-200, i.e. within this demo's run-to-run
        // spread (`docs/frame-cost-probe-round.md` 4b). It is not down to one pass: what is left over is
        // the anchor conversion below using the FLAT estimate where the window walk uses the median's,
        // so measuring still moves the anchor once. That is a consistency question for both, not another
        // clamp, and it is recorded rather than guessed at.
        for jump_to in [399usize, 5000, 399] {
            state.scroll_to_item(jump_to, 0.0);
            let passes = converge(&mut composer);
            eprintln!("compose+layout passes jumping past the end to {jump_to}: {passes}");
            assert!(
                passes <= 4,
                "a jump past the end ({jump_to}) must converge in a bounded number of passes, took {passes}"
            );
        }
    }

    /// `coverage_height` is what the window walk uses for items nothing has measured: the median of the
    /// measured ones, clamped to `[estimate/2, estimate]`. `height` (the anchor math and the placements)
    /// must NOT move with it — those two read the flat estimate and their tests below pin that.
    #[test]
    fn the_coverage_height_follows_the_measured_rows_within_bounds() {
        let mut c = ItemHeightCache::default();
        // Nothing measured: the flat estimate, as before.
        assert_eq!(c.coverage_height(0), LAZY_ITEM_ESTIMATED_HEIGHT);

        // Rows shorter than the estimate: coverage follows them, so windows stop coming up short.
        for i in 0..8 {
            c.record(i, 24.0);
        }
        assert_eq!(c.coverage_height(50), 24.0, "an unmeasured item is assumed as tall as its neighbours");
        assert_eq!(c.height(50), LAZY_ITEM_ESTIMATED_HEIGHT, "the anchor math keeps the flat estimate");

        // Rows TALLER than the estimate: coverage stays on the estimate — a taller assumption would only
        // make windows shorter than they already are, which is the direction that costs a second pass.
        for i in 8..16 {
            c.record(i, 96.0);
        }
        assert_eq!(c.coverage_height(80), LAZY_ITEM_ESTIMATED_HEIGHT);

        // Stray tiny items cannot blow a window up by an unbounded factor: the median is clamped at half
        // the estimate, so the window can at most double.
        for i in 16..32 {
            c.record(i, 1.0);
        }
        assert_eq!(c.coverage_height(200), LAZY_ITEM_ESTIMATED_HEIGHT * 0.5);
    }

    /// The gap this round is about: the frame the viewport GROWS on must already cover what the new
    /// viewport shows. Today `LazyList::build` reads the viewport the PREVIOUS frame's measure wrote, so
    /// a resize to a taller viewport composes the old, smaller window and the bottom of the new one is
    /// empty until the next frame (documented in `docs/architecture-audit.md` §Phase 3.2, "跨帧收敛").
    ///
    /// The harness renders the given frame heights and inspects the LAST one only, so passing a single
    /// resize frame (`[400, 2500]`) measures exactly that frame — no convergence frame is allowed.
    /// `stable` selects the frame handler's semantics: consume the measure's same-frame compose request
    /// (`take_compose_after_layout`, the trigger `PerWindow::recompose_layout_render` loops on), which is
    /// the fix.
    fn resize_frame_bottom_covered(stable: bool) -> bool {
        use skia_safe::{Color as SkColor, surfaces};
        let theme = crate::theme::ThemeColors::light_from_seed(0x6750A4);
        let state = LazyListState::new();
        let items: Arc<Vec<u64>> = Arc::new((0..200).collect());
        let build = move |ctx: &mut ComposeCtx, _s: &LazyListState| {
            LazyColumn::new()
                .modifier(Modifier::new().fill_max_width().fill_max_height())
                .items_from(items.clone(), |v: &u64| *v, |ctx, _i, v| {
                    crate::components::text::Text::new(format!("Item {}", v))
                        .font_size(14.0)
                        .modifier(Modifier::new().padding(12.0))
                        .build(ctx);
                })
                .build(ctx);
        };
        let mut composer = Composer::new();
        let heights = [400.0f32, 2500.0];
        for (i, &h) in heights.iter().enumerate() {
            let last = i + 1 == heights.len();
            if last {
                // The resize frame: first the compose+layout the handler does today...
                composer.compose(|ctx| {
                    crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| build(ctx, &state))
                });
                composer.layout(Constraints::new(0.0, 400.0, 0.0, h));
                if stable {
                    // ...then the same-frame convergence the fix adds: consume the measure's request and
                    // compose again, exactly as the frame handler does. Bounded, and it stops as soon as
                    // the measure stops asking.
                    for _ in 0..8 {
                        if !crate::runtime::composer::take_compose_after_layout() {
                            break;
                        }
                        composer.compose(|ctx| {
                            crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| build(ctx, &state))
                        });
                        composer.layout(Constraints::new(0.0, 400.0, 0.0, h));
                    }
                }
            } else {
                composer.compose(|ctx| {
                    crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| build(ctx, &state))
                });
                composer.layout(Constraints::new(0.0, 400.0, 0.0, h));
            }
        }
        let last_h = 2500.0f32;
        let mut surface = surfaces::raster_n32_premul((400, last_h as i32)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(SkColor::WHITE);
        let root = composer.layout_root_idx().expect("root");
        crate::render::render(composer.arena_nodes(), root, canvas);
        let pm = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        let (w, h) = (400usize, last_h as usize);
        let mut text_rows = 0;
        for y in (h.saturating_sub(60))..h {
            let mut has = false;
            for x in (0..w).step_by(4) {
                let p = px[y * w + x];
                has |= p[0] < 120 && p[1] < 120 && p[2] < 120;
            }
            if has {
                text_rows += 1;
            }
        }
        text_rows > 5
    }

    #[test]
    fn the_frame_the_viewport_grows_on_already_covers_the_new_bottom() {
        // CONTROL first: without the same-frame convergence the resize frame is one window behind,
        // which is the defect `docs/architecture-audit.md` §Phase 3.2 recorded. If this ever stops
        // failing, the assertion below proves nothing.
        let without = resize_frame_bottom_covered(false);
        eprintln!("resize 帧（400 -> 2500，无同帧收敛）视口底部是否被覆盖: {without}");
        assert!(
            !without,
            "control: without same-frame convergence the resize frame must miss the new bottom \
             (covered={without})"
        );
        let covered = resize_frame_bottom_covered(true);
        eprintln!("resize 帧（400 -> 2500，同帧收敛）视口底部是否被覆盖: {covered}");
        assert!(
            covered,
            "the resize frame itself must cover the new viewport's bottom, not the frame after it \
             (covered={covered})"
        );
    }

    /// The derived scroll flags follow the boundary synchronously, and only that.
    #[test]
    fn the_derived_scroll_flags_flip_at_the_boundary() {
        let list = LazyListState::new();
        let mut composer = crate::runtime::composer::Composer::new();
        let slots: std::sync::Arc<std::sync::Mutex<Option<(crate::runtime::state::State<bool>, crate::runtime::state::State<bool>)>>> =
            std::sync::Arc::new(std::sync::Mutex::new(None));
        let l = list.clone();
        let out = slots.clone();
        composer.compose(move |ctx| {
            let back = l.remember_can_scroll_backward(ctx);
            let forward = l.remember_can_scroll_forward(ctx);
            *out.lock().unwrap() = Some((back, forward));
        });
        let (back, forward) = slots.lock().unwrap().clone().unwrap();
        assert!(!back.peek(), "at the top: nothing to go back to");
        assert!(!forward.peek(), "and nothing laid out past the end yet");

        list.offset.set(10.0);
        assert!(back.peek(), "a write past the boundary flips it with no executor involved");
    }

    /// A DIRECT fling on a paging list (no component, no gesture) goes through the same strategy as
    /// the gesture path: it settles on a page boundary instead of decaying across it.
    #[test]
    fn a_direct_fling_on_a_paging_list_settles_on_a_page() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        const STEP: f32 = 200.0;

        let state = LazyListState::new();
        state.fling_limit.set(2000.0);
        state.snap.set(Some(crate::fling::SnapInfo {
            step: STEP,
            ..Default::default()
        }));
        state.offset.set(50.0);
        state.fling(4000.0);

        for _ in 0..800 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(2));
            if !state.is_scrolling.peek() {
                break;
            }
        }
        let end = state.offset.get();
        // The built-in plan for 4000 px/s from 50 px at a 200 px step: the page the release starts in
        // (0) plus one page, which is the default `max_pages_per_fling`. A decay would stop at
        // 50 + 4000/4.2 ≈ 1002 px, i.e. ~2 px off a boundary - which is why this pins the VALUE and
        // not merely "a boundary".
        assert!(
            (end - STEP).abs() < 0.5,
            "a paging list flings to page 1 (200), got {end}"
        );
        crate::animation::clear_all_animations();
    }

    /// A same-count data REPLACEMENT reaches the rows: the key sequence decides which item groups
    /// exist, so keys 1..3 -> 7..9 composes fresh rows while an unchanged frame composes none.
    ///
    /// The key sequence is also declared as a parameter of the container (`ctx.changed(&sig)`), which
    /// is what keeps `intervals.rebuild()` and the height-cache rebase - both inside the body -
    /// running for a reorder that keeps the count. This test cannot see that half on its own: a list
    /// body that re-enters still skips the unchanged item groups, so the row counter looks the same
    /// either way. It pins the user-visible half (rows follow the data), not the declaration.
    #[test]
    fn a_same_count_data_swap_rebuilds_the_rows() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let state = LazyListState::new();
        let runs = std::sync::Arc::new(AtomicUsize::new(0));
        let mut composer = Composer::new();
        let build = |composer: &mut Composer, items: std::sync::Arc<Vec<u64>>| {
            let s = state.clone();
            let r = runs.clone();
            composer.compose(move |ctx| {
                LazyColumn::new()
                    .state(s)
                    .modifier(Modifier::new().fill_max_width().fill_max_height())
                    .items_from(items, |v| *v, move |ctx, _i, v| {
                        r.fetch_add(1, Ordering::Relaxed);
                        crate::components::text::Text::new(format!("Item {v}")).build(ctx);
                    })
                    .build(ctx);
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 600.0));
        };

        build(&mut composer, std::sync::Arc::new(vec![1, 2, 3]));
        let first = runs.load(Ordering::Relaxed);
        assert!(first > 0, "the first frame composed rows");
        // A second frame on the SAME data settles the group: the first frame wrote the data signature
        // and the anchor back, which dirties it exactly once.
        build(&mut composer, std::sync::Arc::new(vec![1, 2, 3]));
        let settled = runs.load(Ordering::Relaxed);
        assert_eq!(
            settled, first,
            "an unchanged frame must SKIP the rows, not recompose them"
        );
        // Same count, different keys: the key sequence is a declared parameter, so this re-enters.
        build(&mut composer, std::sync::Arc::new(vec![7, 8, 9]));
        assert!(
            runs.load(Ordering::Relaxed) > settled,
            "a same-count swap (keys 1..3 -> 7..9) must re-run the rows, not skip the list"
        );
    }

    /// `reverseLayout` flipped at ONE call site reaches the list: the direction is carried both by
    /// the `LazyScroll` element (so the container's modifier comparison sees a change) and by the
    /// parameter `build` declares, and the measure mirrors every placement because of it.
    ///
    /// The assertion is on the LAYOUT, not on the skip decision: an undeclared parameter is only
    /// caught by the modifier comparison here, and the mirror itself is what a caller sees.
    #[test]
    fn flipping_reverse_at_one_call_site_mirrors_the_layout() {
        let state = LazyListState::new();
        let mut composer = Composer::new();
        let build = |composer: &mut Composer, reverse: bool| {
            let s = state.clone();
            composer.compose(move |ctx| {
                LazyColumn::new()
                    .state(s)
                    .reverse_layout(reverse)
                    .modifier(Modifier::new().fill_max_width().fill_max_height())
                    .items_plain(40, |ctx, i| {
                        crate::components::text::Text::new(format!("Item {i}"))
                            .font_size(14.0)
                            .modifier(Modifier::new().padding(12.0))
                            .build(ctx);
                    })
                    .build(ctx);
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 600.0));
        };
        // The list node is the one carrying the LazyScroll element; its first child is item 0, whose
        // content-coordinate y is what the mirrored placement moves.
        let first_item_y = |composer: &Composer| {
            let nodes = composer.arena_nodes();
            let list = nodes
                .iter()
                .position(|node| {
                    node.modifier.elements().iter().any(|el| {
                        matches!(el, crate::modifier::ModifierElement::LazyScroll { .. })
                    })
                })
                .expect("the list attaches a LazyScroll element");
            let child = *nodes[list].children.first().expect("the list composed items");
            nodes[child].position.y
        };

        build(&mut composer, false);
        let forward = first_item_y(&composer);
        assert!(
            forward.abs() < 1.0,
            "a forward list starts its first item at the top, got {forward}"
        );

        build(&mut composer, true);
        let reversed = first_item_y(&composer);
        assert!(
            reversed > forward + 100.0,
            "flipping reverseLayout must mirror the placement - the first item belongs at the bottom \
             (forward {forward}, reversed {reversed})"
        );
    }

    #[test]
    fn animate_item_springs_a_shifted_item_from_its_old_slot() {
        let (_guard, clock) = crate::animation::tests::serial_clock();
        crate::animation::clear_all_animations();
        let theme = crate::theme::ThemeColors::light_from_seed(0x6750A4);
        let state = LazyListState::new();
        let mut composer = Composer::new();
        let mut build = |composer: &mut Composer, items: Arc<Vec<u64>>| {
            let s = state.clone();
            let scene = move |ctx: &mut crate::runtime::composer::ComposeCtx| {
                let s = s.clone();
                let items = items.clone();
                let theme = theme.clone();
                crate::theme::WiniaTheme::with_theme(theme, ctx, move |ctx| {
                    LazyColumn::new()
                        .state(s.clone())
                        .modifier(Modifier::new().fill_max_width().fill_max_height())
                        .items_from(items, |v: &u64| *v, |ctx, _i, v| {
                            crate::components::text::Text::new(format!("Item {v}"))
                                .modifier(Modifier::new().animate_item().height(48.0))
                                .build(ctx);
                        })
                        .build(ctx);
                });
            };
            for _ in 0..6 {
                composer.compose(&scene);
                composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 600.0));
                if !crate::runtime::composer::take_compose_after_layout() {
                    break;
                }
            }
        };

        build(&mut composer, Arc::new((0..20).collect()));
        assert!(
            composer
                .arena_nodes()
                .iter()
                .filter_map(|n| n.item_paint.as_ref())
                .all(|paint| paint.offset.peek().y.abs() < 0.5),
            "nothing has moved yet, so nothing animates"
        );

        // Insert a key in the middle: the items below it move down by one row, and their placement
        // animation starts at the slot they used to occupy.
        let mut items: Vec<u64> = (0..10).collect();
        items.push(999);
        items.extend(10..20);
        build(&mut composer, Arc::new(items));
        let shifted: Vec<f32> = composer
            .arena_nodes()
            .iter()
            .filter_map(|n| n.item_paint.as_ref().map(|paint| paint.offset.peek().y))
            .filter(|value| value.abs() > 0.5)
            .collect();
        assert!(
            !shifted.is_empty(),
            "items below the insert start their animation from the old slot"
        );
        assert!(
            shifted.iter().any(|value| *value < 0.0),
            "an item pushed down starts above its new slot"
        );
        // The offset must SPRING back to 0; without the animation registration it would stay at
        // the old slot forever, and the earlier assertions cannot tell the two apart.
        for _ in 0..40 {
            clock.advance(std::time::Duration::from_millis(16));
            crate::animation::update_animations();
        }
        assert!(
            composer
                .arena_nodes()
                .iter()
                .filter_map(|n| n.item_paint.as_ref())
                .all(|paint| paint.offset.peek().y.abs() < 0.5),
            "the placement spring settles on the new slot"
        );
        crate::animation::clear_all_animations();
    }
