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

    // ── IntervalList: locate / key mapping ──
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
        // Keys: a section WITHOUT a factory uses its global index; a section WITH one is handed the
        // index inside that section (Compose's `items(count, key = { index -> … })`, and what every
        // data-driven factory assumes), so the second section answers 100 and 101 for its two items.
        assert_eq!(il.key_of(2), Some(2));
        assert_eq!(il.key_of(3), Some(100));
        assert_eq!(il.key_of(4), Some(101));
        assert_eq!(il.index_of_key(100), Some(3));
        assert_eq!(il.index_of_key(2), Some(2));
        assert_eq!(il.index_of_key(999), None);
    }

    // ── The height cache ──
    #[test]
    fn height_cache_estimated_then_recorded() {
        let mut c = ItemHeightCache::default();
        assert_eq!(c.height(0), LAZY_ITEM_ESTIMATED_HEIGHT, "an unmeasured item uses the estimate");
        c.record(2, 30.0);
        assert_eq!(c.height(2), 30.0);
        assert_eq!(c.height(1), LAZY_ITEM_ESTIMATED_HEIGHT);
        // Recording beyond the end extends the cache.
        c.record(5, 20.0);
        assert_eq!(c.height(5), 20.0);
    }

    // ── Height cache: migrate by item key (a data change must not misplace heights by index) ──
    #[test]
    fn height_cache_keyed_rebase_follows_identity() {
        // Setup: data 0..6 (key = itself); index 2 (key=2) measured 30, index 5 (key=5) measured 20.
        // Then one item (key=100) is inserted at the head, so key 2 moves to index 3 and key 5 to
        // index 6. After the rebase the heights must follow the KEY, not the old index.
        let mut c = ItemHeightCache::default();
        c.record(2, 30.0);
        c.record_keyed(2, 30.0);
        c.record(5, 20.0);
        c.record_keyed(5, 20.0);
        // New data: the key sequence is [100, 0, 1, 2, 3, 4, 5].
        let new_keys = [100u64, 0, 1, 2, 3, 4, 5];
        c.rebase(&new_keys);
        // key=2 is now index 3, so the height 30 migrates there.
        assert_eq!(c.height(3), 30.0, "the key=2 item migrates to index 3");
        // key=5 is now index 6, so the height 20 migrates there.
        assert_eq!(c.height(6), 20.0, "the key=5 item migrates to index 6");
        // The inserted key=100 (index 0) has no record, so it is estimated.
        assert_eq!(c.height(0), LAZY_ITEM_ESTIMATED_HEIGHT, "a new item is estimated");
        // Index 2 now holds key=1 (no record), so it is estimated — the old 30 must not linger.
        assert_eq!(c.height(2), LAZY_ITEM_ESTIMATED_HEIGHT, "no stale height is left at the old index");
        // Shorter data: heights past the end are dropped.
        c.record_keyed(9, 55.0); // simulate a height recorded for key=9 (no longer in the data)
        c.rebase(&[100u64, 0, 1, 2, 3, 4]); // the new total is 6
        assert_eq!(c.heights.len(), 6, "shorter data clears the out-of-range heights");
        assert_eq!(c.height_keyed(9), None, "the record of a vanished key is dropped");
    }

    // A data change with the SAME total — the case that motivates the key view: a reorder, a
    // mid-list removal or a replacement changes the key sequence while the total stays put, and the
    // heights must still follow the key.
    #[test]
    fn height_cache_keyed_rebase_same_total_reorder_and_delete() {
        // Original data [a=30, b=40, c=50] (keys a/b/c).
        let mut c = ItemHeightCache::default();
        c.record_keyed(10, 30.0); // a
        c.record_keyed(20, 40.0); // b
        c.record_keyed(30, 50.0); // c
        // Case 1: same total, reordered [a,b,c] → [c,a,b] (key sequence [30,10,20], total 3).
        c.rebase(&[30u64, 10, 20]);
        assert_eq!(c.heights.len(), 3, "a same-total reorder does not change the length");
        assert_eq!(c.height(0), 50.0, "index 0 is now c (key 30), height 50");
        assert_eq!(c.height(1), 30.0, "index 1 is now a (key 10), height 30");
        assert_eq!(c.height(2), 40.0, "index 2 is now b (key 20), height 40");
        // Case 2: a mid-list removal, [a,b,c] → [a,c] (total 3→2, key sequence [10,30]).
        c.rebase(&[10u64, 30]);
        assert_eq!(c.heights.len(), 2, "the removal shrinks the cache");
        assert_eq!(c.height(0), 30.0, "index 0 is a (key 10) and keeps height 30");
        assert_eq!(c.height(1), 50.0, "index 1 is c (key 30), migrated with height 50");
        assert_eq!(c.height_keyed(20), None, "the removed key's height record is dropped");
        // Case 3: a replacement. A section whose content changes but whose KEYS do not is the
        // documented limitation (a stable key keeps its height); here the key changes, so the
        // migration is exact. [a,c] → [a,x,c] with x inserted before c.
        c.rebase(&[10u64, 99, 30]);
        assert_eq!(c.height(0), 30.0, "a keeps its height");
        assert_eq!(c.height(1), LAZY_ITEM_ESTIMATED_HEIGHT, "the new key 99 is estimated");
        assert_eq!(c.height(2), 50.0, "c moves to index 2");
    }

    // ── Anchor conversion ──
    #[test]
    fn anchor_from_offset_matches() {
        let mut c = ItemHeightCache::default();
        for i in 0..10 { c.record(i, 50.0); }
        // Every item is 50 tall with no spacing: offset 0..50 → item 0, 50..100 → item 1.
        assert_eq!(anchor_from_offset(&c, 0.0, 0.0, 0.0), (0, 0.0));
        assert_eq!(anchor_from_offset(&c, 49.0, 0.0, 0.0), (0, 49.0));
        assert_eq!(anchor_from_offset(&c, 50.0, 0.0, 0.0), (1, 0.0));
        assert_eq!(anchor_from_offset(&c, 120.0, 0.0, 0.0), (2, 20.0));
        // With `before` padding the content starts lower: offset 50 (which includes `before`) is
        // still item 0, at offset 0 inside it.
        assert_eq!(anchor_from_offset(&c, 50.0, 0.0, 50.0), (0, 0.0));
        assert_eq!(anchor_from_offset(&c, 99.0, 0.0, 50.0), (0, 49.0));
        assert_eq!(anchor_from_offset(&c, 100.0, 0.0, 50.0), (1, 0.0));
        // Past the recorded range (10 items) the estimate extrapolates: 99999 / 48 ≈ 2082.
        let (far_idx, far_off) = anchor_from_offset(&c, 99999.0, 0.0, 0.0);
        assert!(far_idx > 2000, "the estimate extrapolates; got {far_idx}");
        // The 10 recorded items take 50 each; the rest follows the 48 estimate:
        // 99999 - 500 - (idx - 10) * 48.
        let expected = 99999.0 - 500.0 - (far_idx - 10) as f32 * 48.0;
        assert!((far_off - expected).abs() < 0.001);
    }

    // ── The visible range ──
    #[test]
    fn visible_range_includes_beyond_bounds() {
        let mut c = ItemHeightCache::default();
        for i in 0..100 { c.record(i, 50.0); }
        // Anchor item 5 at offset 0, viewport 200 → 4 visible (200/50) plus 4 prefetched, so the
        // end lands near 9.
        let (s, e) = visible_range(&c, 5, 0.0, 200.0, 0.0, 100, LAZY_BEYOND_BOUNDS);
        assert_eq!(s, 1, "start = 5 minus the 4 prefetched items");
        assert!(e >= 9 && e <= 13, "the end covers the viewport plus the prefetch; got {e}");
        // A huge viewport runs the end up to the total.
        let (_, e2) = visible_range(&c, 5, 0.0, 99999.0, 0.0, 100, LAZY_BEYOND_BOUNDS);
        assert_eq!(e2, 100);
    }

    // ── Component level: a data change under stable keys keeps the scroll position ──
    #[test]
    fn key_preserves_scroll_position_on_data_change() {
        // Setup: scroll to item 50 (key=50), then insert 10 items at the head (the old item 50 is now
        // index 60). `last_known_first_key = 50` corrects the offset on the next `build`.
        let state = LazyListState::new();
        // Simulate the state after that scroll: offset corresponds to item 50 (48 per item, no
        // spacing).
        state.offset.set(50.0 * 48.0);
        let mut il = IntervalList::new();
        // Data 1: 0..100, key = itself.
        let c: Arc<dyn Fn(&mut ComposeCtx, usize) + Send + Sync> = Arc::new(|_, _| {});
        il.add(100, Some(Arc::new(|g| g as u64)), c.clone(), false);
        il.rebuild();
        assert_eq!(il.total(), 100);
        // Record the anchor key the way `build` does: offset 2400 → item 50.
        // Note that `anchor_from_offset` can only anchor inside the RECORDED range (an empty cache
        // degrades to 0) while a real `build` has the previous frame's cache, so the first 60 heights
        // are recorded explicitly to stand in for a measured state.
        let mut cache = ItemHeightCache::default();
        for i in 0..60 { cache.record(i, 48.0); }
        let (first, _) = anchor_from_offset(&cache, 2400.0, 0.0, 0.0);
        assert_eq!(first, 50, "offset 2400 / 48 is item 50");
        let key50 = il.key_of(50).unwrap();
        assert_eq!(key50, 50);
        // Data 2: 10 items inserted at the head. The keys stay unique; the new ones take 100..109
        // while the original items keep their keys and shift by +10.
        let mut il2 = IntervalList::new();
        // 10 items (keys 100..109) go in front, so the original 100 (keys 0..99) now occupy the
        // global indices 10..109 — the item whose key is 50 sits at index 60.
        il2.add(10, Some(Arc::new(|g| (100 + g) as u64)), c.clone(), false);
        // The second section's factory gets ITS OWN index 0..99, which is the original key.
        il2.add(100, Some(Arc::new(|g| g as u64)), c.clone(), false);
        il2.rebuild();
        assert_eq!(il2.total(), 110);
        // The key=50 item is now at index 60.
        assert_eq!(il2.index_of_key(50), Some(60));
        // The correction moves the offset to the new index's prefix (60 x 48).
        let corrected = prefix_height(&cache, 60, 0.0);
        assert_eq!(corrected, 60.0 * 48.0);
        // Verify the key-keeping semantics: last_key = 50 → the new offset is prefix(60).
        state.last_known_first_key.set(Some(50));
        assert_eq!(state.last_known_first_key.get(), Some(50));
    }

    // ── Component level, pixels: lazy rendering and content changes after a scroll ──
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
        // Cluster the rows that contain dark text pixels.
        let mut rows = vec![false; h];
        for y in 0..h {
            let mut n = 0;
            for x in 0..w {
                let p = px[y * w + x];
                // BGRA → (b, g, r) (raster_n32_premul is little-endian).
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

    /// `Modifier::padding` on the list itself deflates the VIEWPORT (the padding is outside the
    /// lazy node, Compose's rule), while the items are shifted into the padded area by the engine.
    /// The node still fills its slot exactly: the viewport is the inner size, and the padding is
    /// added on top of it — not the other way round.
    #[test]
    fn a_lazy_column_with_modifier_padding_measures_the_deflated_viewport() {
        let state = LazyListState::new();
        let mut composer = Composer::new();
        composer.compose(crate::compose!(|ctx| {
            LazyColumn::new()
                .state(state.clone())
                .modifier(Modifier::new().padding(10.0))
                .items(
                    6,
                    |i| i as u64,
                    |ctx, i| {
                        crate::components::Text::new(format!("Item {i}"))
                            .modifier(Modifier::new().size(50.0, 40.0))
                            .build(ctx);
                    },
                )
                .build(ctx);
        }));
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        let root = composer.arena.root.expect("root");
        let nodes = composer.arena_nodes();
        assert_eq!(
            nodes[root].measured_size,
            crate::unit::Size::new(200.0, 200.0),
            "the node fills its 200 dp slot: viewport 180 + padding 20"
        );
        let first = nodes[root].children[0];
        assert_eq!(
            nodes[first].position,
            crate::unit::Offset::new(10.0, 10.0),
            "and the items start inside the padding"
        );
    }

    /// Modifier padding and `contentPadding` are different things and ADD UP: the modifier
    /// padding deflates the viewport, the content padding insets the items inside it (Compose's
    /// rule). A list with both starts its first item at modifier + content padding.
    #[test]
    fn modifier_padding_and_content_padding_add_up() {
        let state = LazyListState::new();
        let mut composer = Composer::new();
        composer.compose(crate::compose!(|ctx| {
            LazyColumn::new()
                .state(state.clone())
                .content_padding(6.0, 4.0)
                .modifier(Modifier::new().padding(10.0))
                .items(
                    4,
                    |i| i as u64,
                    |ctx, i| {
                        crate::components::Text::new(format!("Item {i}"))
                            .modifier(Modifier::new().size(50.0, 40.0))
                            .build(ctx);
                    },
                )
                .build(ctx);
        }));
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        let root = composer.arena.root.expect("root");
        let nodes = composer.arena_nodes();
        assert_eq!(nodes[root].measured_size, crate::unit::Size::new(200.0, 200.0));
        let first = nodes[root].children[0];
        assert_eq!(
            nodes[first].position,
            crate::unit::Offset::new(10.0, 16.0),
            "10 dp of modifier padding plus 6 dp of content padding"
        );
    }

    #[test]
    fn lazy_renders_only_visible_items() {
        // A 1000-item list renders only the ~12 items in the viewport (600 px / 40 px), not all of them.
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
        // A 600 px viewport at ~38 px per item shows ~15; the registered window prefetches more, and only
        // the visible ones paint.
        assert!(clusters >= 8 && clusters <= 30, "only the visible items paint; clusters={clusters}");
        // The composition count is far below 1000 — the core lazy assertion.
        let n = composed.load(Ordering::Relaxed);
        assert!(n <= 60, "far fewer items composed than the 1000 that exist; got {n}");
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
        // Scroll: simulate the effect of `apply_scroll_delta`.
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
        // The two frames must differ (a scroll brings other items into view): compare, row by row,
        // whether text pixels appear. An offset of 2000 at ~38 px is ~52 items, so the text shifts by
        // roughly 2000 px and hundreds of rows should differ.
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
        assert!(diff_rows > 10, "content changes after the scroll; diff_rows={diff_rows}");
        // The derived anchor is the exact post-scroll value (~2000/38 ≈ 52).
        assert!(state.first_visible() >= 40, "first_visible={}", state.first_visible());
    }

    // ── LazyRow: the same machinery on the other axis ──
    fn count_text_columns(px: &[[u8; 4]], w: usize, h: usize) -> usize {
        // Cluster the COLUMNS that contain dark text (for the horizontal list: the text lines are packed
        // into a few rows, so items are counted by column).
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
        // A 1000-item horizontal list renders only the ~9 in the viewport (600 px / ~64 px), not all.
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
        // A 600 px viewport at ~64 px per item is ~9 visible (prefetch and gaps included).
        assert!(clusters >= 5 && clusters <= 20, "only the visible items paint; clusters={clusters}");
        let n = composed.load(Ordering::Relaxed);
        // The composition window covers 2000 px, the same machinery LazyColumn uses: 2000/56 ≈ 36 items
        // plus the 8 prefetched.
        assert!(n <= 60, "far fewer items composed than the 1000 that exist; got {n}");
    }

    #[test]
    fn lazy_row_scroll_changes_visible_items() {
        // After a horizontal scroll to offset 2000 the rendered item texts change (different items).
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
        // The frames differ (a scroll brings other items into view): compare, column by column, whether
        // text pixels appear.
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
        assert!(diff_cols > 10, "content changes after the scroll; diff_cols={diff_cols}");
        // The derived anchor is the exact post-scroll value (~2000/64 ≈ 31).
        assert!(state.first_visible() >= 25, "first_visible={}", state.first_visible());
    }

    // ── Programmatic jumps: the anchor is authoritative, so the landing is exact ──
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
        // Regression: a jump to 500 must land at 500. The old implementation computed the pixel offset
        // from an empty cache at 48 px per item, the measured height was ~47.5, the drift accumulated
        // and it landed at 505 (measured by the user). Now the anchor is authoritative: the measure
        // derives the pixel from the written-back cache, so the round trip is exact.
        let state = LazyListState::new();
        let items: Arc<Vec<u64>> = Arc::new((0..1000).collect());
        let (px0, w0) = render_lazy_state(&state, &items);
        state.scroll_to_item(500, 0.0);
        let (px1, w1) = render_lazy_state(&state, &items);
        assert_eq!(w0, w1);
        assert_eq!(
            state.first_visible(),
            500,
            "a jump to 500 must land at exactly 500 (the old implementation landed at 505)"
        );
        // The pixel offset is a mixed prefix (unmeasured stretch priced at the 48 estimate), which does
        // not affect the landing: placement and the translate share the same `prefix_height`, so item 500
        // sits exactly at the top. As scrolling measures more items the offset converges to the true sum,
        // so only a plausible range is asserted.
        assert!(
            state.offset() > 20000.0 && state.offset() < 26000.0,
            "the offset sits near the 500-item prefix; got {}",
            state.offset()
        );
        // The content really changed (different items).
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
        assert!(diff_rows > 10, "content changes after the jump; diff_rows={diff_rows}");
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
    fn scroll_to_item_clamps_to_end() {
        // An out-of-range jump is clamped to the end (Compose clamps the scroll position during measure).
        let state = LazyListState::new();
        let items: Arc<Vec<u64>> = Arc::new((0..1000).collect());
        let (_px, _w) = render_lazy_state(&state, &items);
        state.scroll_to_item(99999, 0.0);
        let (_px1, _w1) = render_lazy_state(&state, &items);
        // At the bottom the first item is `total - items that fit` (a 600 px viewport at ~43 px ≈ 987).
        assert!(
            state.first_visible() >= 980,
            "an out-of-range jump is clamped to the end; got {}",
            state.first_visible()
        );
        // `offset` sits near the content's end. The lower bound here uses the REAL content height:
        // 1000 x ~43 px minus a 600 px viewport ≈ 42400. The old bound (>45000) came from the flat 48 px
        // estimate (1000 x 48 - 600 = 47400); the content height is now priced with the median of the
        // measured rows (`coverage_height`), which removed that 12 % over-estimate.
        assert!(
            state.offset() > 41000.0,
            "the offset sits near the content end; got {}",
            state.offset()
        );
        // Jump back to the top.
        state.scroll_to_item(0, 0.0);
        let (_px2, _w2) = render_lazy_state(&state, &items);
        assert_eq!(state.first_visible(), 0, "back at the top");
    }

    #[test]
    fn jump_to_end_shows_last_item_with_real_viewport() {
        // Regression (measured by the user: jumping to the end left Item 999 off screen).
        // `measure_node` used to rewrite every scroll container's `max_height` to f32::MAX, so the
        // policy never saw the real viewport and fell back to vh = 600 against a real 400 — the clamp's
        // `max_offset = content_h - 600` was 100 too large and the jump overshot, pushing Item 999 out
        // of the bottom. Fixed by keeping a finite `max_height` on a lazy container (the policy owns the
        // child constraints explicitly).
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
        // There must be text within 60 px of the viewport bottom (Item 999 fully visible).
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
            "the last item is visible at the viewport bottom; bottom_text_rows={bottom_text_rows}"
        );
        // The first item sits near the end: a 400 px viewport at ~47.5 px is 8.4 items, so
        // first_visible ≈ 991.
        assert!(
            state.first_visible() >= 985,
            "first_visible={}",
            state.first_visible()
        );
    }

    // ── Fling: the offset advances and stops at the limit ──
    /// Pump the global animations until the offset settles (two reads less than 0.5 apart).
    ///
    /// Do NOT loop on `has_animation_for_state`: without the serial lock a parallel test's
    /// `update_animations` takes the whole animation table away for a moment (a brief empty window),
    /// which reads as "finished" (measured under a parallel run: the fling stopped mid-way at 836).
    /// A caller that already holds `TEST_SERIAL` can use `step_animations_until_target` below instead —
    /// its two conditions (table empty AND near the target) have no such window.
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

    /// Pump the global animations until the offset is within 5.0 of the target AND the animation table
    /// is empty — for scenarios with an EXACT target (e.g. `animate_scroll_to_item`, whose target is
    /// derived from the same cache as `scroll_to_item`, so the round trip is exact).
    ///
    /// Background: a spring's per-frame travel is naturally < 0.5 near the end (threshold 0.01, a 50x
    /// margin), so `step_animations_until_done` fires before the engine is really done (before it writes
    /// the exact target) and leaves ~13 px behind — which under full-suite jitter occasionally exceeded
    /// the 5 px threshold of the final assertion. This function requires the caller to hold `TEST_SERIAL`
    /// (serially nothing else takes or clears the table, so `has_animation_for_state` cannot see a false
    /// empty window) and settles on both conditions: table empty AND near the target.
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
        // The animation registry is process-global, so hold the serial lock: a parallel `clear` would wipe
        // the fling in flight.
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        // Render once so the policy writes `fling_limit` back (content height minus the 600 px viewport).
        let state = LazyListState::new();
        let items: Arc<Vec<u64>> = Arc::new((0..1000).collect());
        let (_px, _w) = render_lazy_state(&state, &items);
        let limit = state.fling_limit.get();
        assert!(limit > 10000.0, "1000 items of content minus the viewport is large; got {limit}");

        // A natural stop: v0 = 5000 decays to a limit of 5000/4.2 ≈ 1190, below `limit`.
        state.fling(5000.0);
        let frames = step_animations_until_done(&state, 300);
        assert!(frames < 300, "the fling converges ({frames} frames)");
        let end = state.offset();
        assert!(
            end > 1000.0 && end < 1400.0,
            "the fling advances the offset near its decay limit; got {end}"
        );

        // Hitting the limit: an enormous velocity stops at `fling_limit` (Compose stops when the fling is
        // spent).
        state.fling(1_000_000.0);
        let frames = step_animations_until_done(&state, 300);
        assert!(frames < 300, "the limit-hitting fling converges ({frames} frames)");
        let end2 = state.offset();
        assert!(
            (end2 - limit).abs() < 1.0,
            "it stops at {limit}; got {end2}"
        );

        // The reverse direction: a negative velocity flings up and clamps back to 0.
        state.fling(-1_000_000.0);
        let frames = step_animations_until_done(&state, 300);
        assert!(frames < 300, "the reverse fling converges ({frames} frames)");
        assert_eq!(state.offset(), 0.0, "the reverse fling rests at the top");
    }

    // ═══════════════════════════════════════════════════
    // Sticky headers (Compose's `stickyHeader`)
    // ═══════════════════════════════════════════════════

    /// Five sections: a sticky header (red background, "HEAD n" text) followed by 19 ordinary items.
    /// Header global index = n*20; an ordinary item's key is 200 + n*19 + i (unique across the list).
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

    /// A 1000-item list with content padding (matching `render_lazy`'s 400x600 viewport).
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

    /// The five-section red-header sticky list with content padding (header global index = n*20).
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

    /// How many red header BANDS touch the top: counts runs of red rows (the header background).
    fn red_bands(px: &[[u8; 4]], w: usize, h: usize, limit: usize) -> usize {
        let mut bands = 0;
        let mut in_band = false;
        for y in 0..limit.min(h) {
            let mut red = false;
            for x in (0..w).step_by(2) {
                let p = px[y * w + x];
                // The raster is BGRA8888 (n32_premul, little-endian), so p[2] is the RED channel.
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

    /// The number of consecutive red rows at the viewport top (the pinned header's visible height; BGRA
    /// byte order).
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
        // After a deep scroll the pinned header becomes the anchor (`first_visible` = the header index),
        // the red band still sits at the viewport top, and the ordinary items slide underneath it (their
        // text pixels are still below).
        let state = LazyListState::new();
        let (px0, w0) = render_lazy(|ctx| sticky_build(ctx, state.clone()));
        assert_eq!(state.first_visible(), 0, "the initial anchor is header 0");
        assert_eq!(state.first_visible_offset.get(), 0.0);

        // Scroll into section 3 (offset 2000 with C(40) ≈ 1900, so header 2 pins).
        state.offset.set(2000.0);
        let (px1, w1) = render_lazy(|ctx| sticky_build(ctx, state.clone()));
        assert_eq!(w0, w1);
        assert_eq!(
            state.first_visible(),
            40,
            "the pinned header becomes the anchor (Compose's firstVisibleItemIndex)"
        );
        assert_eq!(state.first_visible_offset.get(), 0.0);
        // Exactly one red band, and it sits at the viewport top because it is pinned.
        assert_eq!(red_bands(&px1, w1, 600, 300), 1, "exactly one header at the viewport top while pinned");
        let mut top_red = false;
        for x in (0..w1).step_by(2) {
            let p = px1[x];
            if p[2] > 150 && p[0] < 100 && p[1] < 100 {
                top_red = true;
            }
        }
        assert!(top_red, "the pinned header occupies the viewport's top row");
        // The content slides under the header: ordinary item text is still below the band.
        let mut below_text = false;
        for y in (70..600).step_by(2) {
            for x in (0..w1).step_by(2) {
                let p = px1[y * w1 + x];
                if p[0] < 120 && p[1] < 120 && p[2] < 120 {
                    below_text = true;
                }
            }
        }
        assert!(below_text, "ordinary item content keeps rendering under the pinned header");
        // Different content from the first frame (a different section is on screen).
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
        assert!(diff_rows > 10, "content changes after the scroll; diff_rows={diff_rows}");

        // A deep scroll (out of range → clamped to 4150, near the end) pins header 4 — this exercises the
        // unbounded walk-back.
        state.offset.set(100_000.0);
        let (_px2, _w2) = render_lazy(|ctx| sticky_build(ctx, state.clone()));
        assert!(
            state.offset() < 4500.0,
            "the out-of-range offset is clamped; got {}",
            state.offset()
        );
        assert_eq!(
            state.first_visible(),
            80,
            "after the deep scroll header 4 (global index 80) is pinned — the unbounded walk-back",
        );
    }

    #[test]
    fn sticky_header_pushes_previous_on_approach() {
        // While the next header is within h(prev) of the top the previous one starts sliding out (the two
        // are adjacent, so visually it is one continuous red band), which is why the assertion is that the
        // TOP BAND GROWS; once it has arrived the previous header is gone entirely.
        let state = LazyListState::new();
        let (px0, w0) = render_lazy(|ctx| sticky_build(ctx, state.clone()));
        let h0_full = top_band_height(&px0, w0, 600);
        assert!(h0_full > 40, "header 0 is fully pinned at the top; band height {h0_full}");

        // The header top C(20) depends on the height measured at runtime (font loading order gives 40 or
        // 47.5), so the transition window [C(20) - h(0), C(20)] drifts with it — hence a scan that asserts
        // INVARIANTS over an interval:
        // 1) during the transition the pushed-out header 0 and header 1 are adjacent, so the top band
        //    GROWS (band in (h, 2h): the two headers merge);
        // 2) past the transition window header 1 pins (anchor = 20, a single band, back to height h).
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
        assert!(saw_merge, "the top band grows during the transition (the two headers merge, band > one header)");
        assert!(saw_pinned, "past the transition window header 1 is pinned (anchor 20, a single band)");
        let final_diff = if final_band > h0_full { final_band - h0_full } else { h0_full - final_band };
        assert!(final_diff < 6, "header 1 is fully pinned (band {final_band} ≈ {h0_full})");
    }

    #[test]
    fn sticky_header_composes_only_pinned_and_visible() {
        // Composition count: a 600 px viewport is ~12 items, plus 8 prefetched, plus the pinned header —
        // far below the total of 100 (the sticky walk-back does not pull in the whole list).
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
        assert!(n0 <= 40, "the first compose covers only the visible items plus the prefetch; got {n0}");
        // A deep scroll brings the pinned header into the window without composing the whole list.
        state.offset.set(2000.0);
        let (px, w) = render_lazy(|ctx| sticky_build(ctx, state.clone()));
        let n1 = composed.load(std::sync::atomic::Ordering::Relaxed);
        assert!(n1 <= 60, "the deep-scroll compose is still far below 100; got {n1}");
        assert_eq!(red_bands(&px, w, 600, 200), 1);
    }

    // ═══════════════════════════════════════════════════
    // contentPadding (Compose's `contentPadding`)
    // ═══════════════════════════════════════════════════

    #[test]
    fn content_padding_starts_content_after_padding() {
        // pad_before = 50: at offset 0 the first item's content starts at y ≈ 68 (50 + 12 of item padding +
        // the glyph), so rows 20..30 have no text; at offset 50 (the content has scrolled into the padding)
        // the first item touches the top and its text reaches 18..30.
        let state = LazyListState::new();
        let (px0, w0) = render_lazy(|ctx| plain_build_padded(ctx, state.clone(), (50.0, 0.0), (0.0, 0.0)));
        let mut top_empty = true;
        for y in 20..30usize {
            for x in (0..w0).step_by(2) {
                let p = px0[y * w0 + x];
                if p[0] < 120 && p[1] < 120 && p[2] < 120 { top_empty = false; }
            }
        }
        assert!(top_empty, "at offset 0 rows 20..30 are blank (the content starts after the 50 dp padding)");

        state.offset.set(50.0);
        let (px1, w1) = render_lazy(|ctx| plain_build_padded(ctx, state.clone(), (50.0, 0.0), (0.0, 0.0)));
        let mut top_text = false;
        for y in 18..30usize {
            for x in (0..w1).step_by(2) {
                let p = px1[y * w1 + x];
                if p[0] < 120 && p[1] < 120 && p[2] < 120 { top_text = true; }
            }
        }
        assert!(top_text, "after offset 50 the first item touches the top (text reaches y 18..30)");
        // Before scrolling the item top is at y = 50: at its natural boundary the content stops at the
        // padding instead of touching the viewport edge.
        assert_eq!(state.first_visible(), 0, "pad 50 plus offset 50 keeps the anchor at index 0");
    }

    #[test]
    fn content_padding_max_offset_includes_after() {
        // pad_after = 40: at the end `max_offset = pad_before + Σ items + pad_after - viewport`, so the last
        // item's bottom stops at `viewport - 40` and the bottom 35 rows have no text.
        let state = LazyListState::new();
        state.scroll_to_item(99999, 0.0);
        let (px, w) = render_lazy(|ctx| plain_build_padded(ctx, state.clone(), (0.0, 40.0), (0.0, 0.0)));
        assert!(state.offset() > 20_000.0, "a jump to the end scrolls near the end; got {}", state.offset());
        let first = state.first_visible();
        assert!(first >= 970, "first_visible sits near the end; got {first}");
        let mut bottom_empty = true;
        for y in (600 - 35)..600usize {
            for x in (0..w).step_by(2) {
                let p = px[y * w + x];
                if p[0] < 120 && p[1] < 120 && p[2] < 120 { bottom_empty = false; }
            }
        }
        assert!(bottom_empty, "pad_after 40: the bottom 35 rows are blank (the last item stops at the padding)");
    }

    #[test]
    fn content_padding_sticky_pins_at_padding() {
        // pad = 50: after a deep scroll header 4 (global index 80) pins at y = 50, not 0 — the top 45 rows
        // have no red and the band sits at y 50..98.
        let state = LazyListState::new();
        state.offset.set(3900.0); // C(80) = 3800 at h = 47.5, so header 4 pins; below max_off ≈ 4200, no clamp
        let (px, w) = render_lazy(|ctx| sticky_build_padded(ctx, state.clone(), 50.0));
        let mut top_clear = true;
        for y in 0..45usize {
            for x in (0..w).step_by(2) {
                let p = px[y * w + x];
                if p[2] > 150 && p[0] < 100 && p[1] < 100 { top_clear = false; }
            }
        }
        assert!(top_clear, "pad 50: the top 45 rows are blank (the header pins at y = 50)");
        let mut band_red = false;
        for y in 50..98usize {
            for x in (0..w).step_by(2) {
                let p = px[y * w + x];
                if p[2] > 150 && p[0] < 100 && p[1] < 100 { band_red = true; }
            }
        }
        assert!(band_red, "y 50..98 is the red pinned header");
        assert_eq!(state.first_visible(), 80, "the pinned header is the anchor (global index 80)");
        // The band no longer covers the viewport top (rows 0..40).
        assert_eq!(red_bands(&px, w, 600, 200), 1, "exactly one red band (the pinned header)");
    }

    #[test]
    fn content_padding_cross_axis_shrinks_items() {
        // Cross padding 40/40: an item is placed from x = 40 and its width is limited to 400 - 80 = 320, so
        // there is no text before x = 36 and the text starts around x 52..92.
        let state = LazyListState::new();
        let (px, w) = render_lazy(|ctx| plain_build_padded(ctx, state.clone(), (0.0, 0.0), (40.0, 40.0)));
        let mut left_empty = true;
        for y in (0..600).step_by(2) {
            for x in 0..36usize {
                let p = px[y * w + x];
                if p[0] < 120 && p[1] < 120 && p[2] < 120 { left_empty = false; }
            }
        }
        assert!(left_empty, "cross padding 40: no text before x = 36");
        let mut text_after_pad = false;
        for y in (0..600).step_by(2) {
            for x in 56..70usize {
                let p = px[y * w + x];
                if p[0] < 120 && p[1] < 120 && p[2] < 120 { text_after_pad = true; }
            }
        }
        assert!(text_after_pad, "the text starts around x = 54 (40 of cross padding plus 12 of item padding)");
    }

    // ═══════════════════════════════════════════════════════
    // reverseLayout (Compose's `reverseLayout`: index 0 sits at the bottom)
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

    /// Whether the given row range contains dark text pixels.
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
        // offset 0: item 0 is at the bottom of the screen (`first_visible` = 0), so there is text at the
        // bottom AND at the top (the viewport shows the list's first ~12 items, with index 0 lowest).
        let state = LazyListState::new();
        let (px0, w0) = render_lazy(|ctx| reverse_build(ctx, state.clone()));
        assert_eq!(state.first_visible(), 0, "at offset 0 the anchor is item 0 (the screen bottom)");
        assert!(rows_have_text(&px0, w0, 570, 600), "item 0 is visible at the screen bottom");
        assert!(rows_have_text(&px0, w0, 0, 30), "the screen top also shows the start of the list (~item 12)");

        // A growing offset still moves toward the list's END (a growing index) — the scroll direction is
        // unchanged.
        state.offset.set(5000.0);
        let (px1, w1) = render_lazy(|ctx| reverse_build(ctx, state.clone()));
        let first = state.first_visible();
        assert!(first >= 90, "at offset 5000 the anchor advances to ~105; got {first}");
        // The bottom item is the anchor (~105); the top is ~117.
        assert!(rows_have_text(&px1, w1, 0, 30), "content is still at the screen top after the reverse scroll");
        // The frames differ (a scroll brings other items into view).
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
        assert!(diff_rows > 10, "content changes after the reverse scroll; diff_rows={diff_rows}");
    }

    #[test]
    fn reverse_layout_scroll_to_item_lands_at_bottom() {
        // Jump to 500: item 500's BOTTOM touches the viewport bottom (the mirror of the forward "top to
        // top"), so the bottom rows have text; the anchor is 500, the item at the screen bottom.
        let state = LazyListState::new();
        let items: Arc<Vec<u64>> = Arc::new((0..1000).collect());
        // The forward control: item 500 touches the screen TOP.
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
        assert_eq!(state.first_visible(), 500, "reverse jump to 500: the anchor is 500 (the item at the screen bottom)");
        assert!(rows_have_text(&px, w, 570, 600), "item 500's bottom touches the viewport bottom (text on the bottom rows)");
        // There is content at the top too (the ~12 items above item 500).
        assert!(rows_have_text(&px, w, 0, 40), "the items above item 500 are visible at the screen top");
        // offset = prefix(500) ≈ 23750 (no spacing, no padding).
        assert!(
            state.offset() > 20_000.0 && state.offset() < 26_000.0,
            "the reverse jump offset is prefix(500); got {}",
            state.offset()
        );
    }

    #[test]
    fn reverse_layout_clamps_at_both_ends() {
        // Out of range (offset 100000) clamps to max_off = content_h - vh ≈ 46900, so the anchor is ≈ 987.
        let state = LazyListState::new();
        state.offset.set(100_000.0);
        // First frame: `build` anchors on the un-clamped offset (a too-narrow window); by the second frame
        // the policy has clamped the offset and the window converges (a real app converges after the first
        // frame, so the test asserts on the converged frame).
        let (_px0, _w0) = render_lazy(|ctx| reverse_build(ctx, state.clone()));
        let (px, w) = render_lazy(|ctx| reverse_build(ctx, state.clone()));
        let first = state.first_visible();
        assert!(first >= 970, "after the clamp the index is near the list end; got {first}");
        assert!(rows_have_text(&px, w, 570, 600), "the last item is visible at the screen bottom");
        // Back to the start.
        state.offset.set(0.0);
        let (_px2, _w2) = render_lazy(|ctx| reverse_build(ctx, state.clone()));
        assert_eq!(state.first_visible(), 0, "offset 0 is back at item 0");
    }

    // ── animateScrollToItem (Compose's `animateScrollToItem` — a spring) ──
    #[test]
    fn animate_scroll_to_item_animates_offset() {
        // The animation registry is process-global, so hold the serial lock: `step_animations_until_target`
        // relies on `has_animation_for_state` never seeing a false empty window, which needs serial runs
        // (see the helper's docs).
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let state = LazyListState::new();
        let items: Arc<Vec<u64>> = Arc::new((0..1000).collect());
        let (_px, _w) = render_lazy_state(&state, &items);
        let before = state.offset();
        assert_eq!(before, 0.0);

        // An immediate jump measures the exact target (prefix(500) over the real heights — the font height
        // differs at runtime, so a constant 23750 cannot be asserted).
        state.scroll_to_item(500, 0.0);
        let (_px, _w) = render_lazy_state(&state, &items);
        let target = state.offset();
        assert!(target > 15_000.0 && target < 30_000.0, "the target is ≈ prefix(500); got {target}");

        // Back to 0, then animate to the same target.
        state.offset.set(0.0);
        let (_px, _w) = render_lazy_state(&state, &items);
        state.animate_scroll_to_item(500, 0.0);
        let (_px, _w) = render_lazy_state(&state, &items);
        // The measure pass consumes the request and registers a spring (`push_animatable` updates once
        // immediately — elapsed ≈ 0, travel ≈ 0 — and the step loop advances it afterwards). The target is
        // derived from the same cache as `scroll_to_item`, so the round trip is exact, and the settle uses
        // both conditions (table empty AND near the target) because a travel-only criterion fires before the
        // engine is done, leaving ~13 px (see the helper's docs).
        let frames = step_animations_until_target(&state, target, 400);
        assert!(frames < 400, "the spring converges ({frames} frames)");
        let end = state.offset();
        assert!(
            (end - target).abs() < 5.0,
            "the animation settles on the same target as scroll_to_item ({target}); got {end}"
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

    // ── RTL, the unbounded cross axis, and the defensive normalisation of the knobs ──

    fn sized_item(ctx: &mut ComposeCtx, i: usize, w: f32, h: f32) {
        crate::components::Text::new(format!("item {i}"))
            .modifier(Modifier::new().size(w, h))
            .build(ctx);
    }

    fn children_positions(composer: &crate::runtime::composer::Composer, root: usize) -> Vec<(f32, f32)> {
        composer.arena_nodes()[root]
            .children
            .iter()
            .map(|&c| {
                let p = composer.arena_nodes()[c].position;
                (p.x, p.y)
            })
            .collect()
    }

    /// Compose lays a LazyRow out from the RIGHT in RTL, so index 0 must sit at the right edge and
    /// the items must run leftward. winia keeps one content coordinate system and mirrors it (the
    /// policy mirrors the placement, the scroll modifier mirrors the translate), which is the same
    /// pair of steps `reverseLayout` already used — `TabRow`'s scrollable does it with
    /// `horizontal_scroll_reverse(is_rtl)`.
    #[test]
    fn an_rtl_row_runs_right_to_left() {
        let mut composer = Composer::new();
        composer.compose(crate::compose!(|ctx| {
            LazyRow::new()
                .modifier(Modifier::new().layout_direction(crate::layout::LayoutDirection::Rtl))
                .items(4, |i| i as u64, |ctx, i| sized_item(ctx, i, 50.0, 40.0))
                .build(ctx);
        }));
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        let root = composer.arena.root.expect("root");
        assert_eq!(
            children_positions(&composer, root),
            vec![(150.0, 0.0), (100.0, 0.0), (50.0, 0.0), (0.0, 0.0)],
            "item 0 at the right edge, running leftward, and the cross axis untouched"
        );
    }

    /// The LTR control: nothing about the change above may move an LTR list.
    #[test]
    fn an_ltr_row_is_unchanged() {
        let mut composer = Composer::new();
        composer.compose(crate::compose!(|ctx| {
            LazyRow::new()
                .items(4, |i| i as u64, |ctx, i| sized_item(ctx, i, 50.0, 40.0))
                .build(ctx);
        }));
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        let root = composer.arena.root.expect("root");
        assert_eq!(
            children_positions(&composer, root),
            vec![(0.0, 0.0), (50.0, 0.0), (100.0, 0.0), (150.0, 0.0)]
        );
    }

    /// `reverseLayout` alone still puts index 0 at the far end, and `reverseLayout` + RTL cancel —
    /// the same XOR the scroll modifier gets, because both mirrorings have to agree or the content is
    /// drawn a viewport away from where it was measured.
    #[test]
    fn a_reversed_row_mirrors_and_cancels_under_rtl() {
        for (rtl, reverse, expected_first_x) in [(false, true, 150.0), (true, true, 0.0)] {
            let direction = if rtl {
                crate::layout::LayoutDirection::Rtl
            } else {
                crate::layout::LayoutDirection::Ltr
            };
            let mut composer = Composer::new();
            composer.compose(crate::compose!(|ctx| {
                LazyRow::new()
                    .reverse_layout(reverse)
                    .modifier(Modifier::new().layout_direction(direction))
                    .items(4, |i| i as u64, |ctx, i| sized_item(ctx, i, 50.0, 40.0))
                    .build(ctx);
            }));
            composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
            let root = composer.arena.root.expect("root");
            let first = children_positions(&composer, root)[0];
            assert_eq!(
                first.0, expected_first_x,
                "rtl={rtl} reverse={reverse}: item 0 at x={expected_first_x}"
            );
        }
    }

    /// A vertical list does not mirror its MAIN axis under RTL (the items still run downward), but
    /// its CROSS axis is horizontal, so `content_padding_cross`'s "before" is the right edge and a
    /// narrow item hugs it.
    #[test]
    fn an_rtl_column_right_aligns_its_items() {
        let mut composer = Composer::new();
        composer.compose(crate::compose!(|ctx| {
            LazyColumn::new()
                .content_padding_cross(10.0, 4.0)
                .modifier(Modifier::new().layout_direction(crate::layout::LayoutDirection::Rtl))
                .items(3, |i| i as u64, |ctx, i| sized_item(ctx, i, 50.0, 40.0))
                .build(ctx);
        }));
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        let root = composer.arena.root.expect("root");
        assert_eq!(
            children_positions(&composer, root),
            vec![(140.0, 0.0), (140.0, 40.0), (140.0, 80.0)],
            "x = 200 - 10 (the start padding, now on the right) - 50 (the item)"
        );
    }

    /// A filling item covers the padded box on either side, so under RTL its x is the AFTER padding —
    /// the mirrored form of the same box, not a different one.
    #[test]
    fn an_rtl_column_mirrors_the_cross_padding_for_a_filling_item() {
        let mut composer = Composer::new();
        composer.compose(crate::compose!(|ctx| {
            LazyColumn::new()
                .content_padding_cross(10.0, 4.0)
                .modifier(Modifier::new().layout_direction(crate::layout::LayoutDirection::Rtl))
                .items(2, |i| i as u64, |ctx, i| {
                    crate::components::Text::new(format!("item {i}"))
                        .modifier(Modifier::new().fill_max_width().height(40.0))
                        .build(ctx);
                })
                .build(ctx);
        }));
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        let root = composer.arena.root.expect("root");
        let positions = children_positions(&composer, root);
        assert_eq!(positions[0], (4.0, 0.0), "the item spans [4, 190]: the padded box mirrored");
        let widest = composer.arena_nodes()[composer.arena_nodes()[root].children[0]].measured_size;
        assert_eq!(widest.width, 186.0, "200 - 10 - 4");
    }

    /// The unbounded cross axis must not leak the `f32::MAX` sentinel: a lazy column inside a
    /// horizontal scroll reports its WIDEST item plus the cross content padding. Reporting 3.4e38
    /// poisoned the scroll container's content width, hit testing and any scrollbar built on it.
    #[test]
    fn an_unbounded_cross_axis_reports_the_widest_item() {
        let mut composer = Composer::new();
        composer.compose(crate::compose!(|ctx| {
            crate::layout::Row::new()
                .modifier(Modifier::new().horizontal_scroll(crate::modifier::ScrollState::new()))
                .build(ctx, |ctx| {
                    LazyColumn::new()
                        .content_padding_cross(6.0, 4.0)
                        .items(3, |i| i as u64, |ctx, i| sized_item(ctx, i, 50.0, 40.0))
                        .build(ctx);
                });
        }));
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        let row = composer.arena.root.expect("root");
        let column = composer.arena.root().expect("root").children[0];
        let size = composer.arena_nodes()[column].measured_size;
        assert_eq!(size.width, 60.0, "50 dp of item plus 6 + 4 of cross padding, not 3.4e38");
        assert_eq!(size.height, 200.0, "the main axis still reports the viewport");
        assert!(size.width.is_finite() && composer.arena_nodes()[row].measured_size.width.is_finite());
    }

    /// The mirror image of the case above: a LazyRow inside a vertical scroll has an unbounded
    /// HEIGHT, and must report the tallest item instead of the sentinel.
    #[test]
    fn a_lazy_row_in_a_vertical_scroll_keeps_a_finite_height() {
        let mut composer = Composer::new();
        composer.compose(crate::compose!(|ctx| {
            crate::layout::Column::new()
                .modifier(Modifier::new().vertical_scroll(crate::modifier::ScrollState::new()))
                .build(ctx, |ctx| {
                    LazyRow::new()
                        .items(3, |i| i as u64, |ctx, i| sized_item(ctx, i, 50.0, 40.0))
                        .build(ctx);
                });
        }));
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        let column = composer.arena.root.expect("root");
        let row = composer.arena_nodes()[column].children[0];
        let size = composer.arena_nodes()[row].measured_size;
        assert_eq!(size.height, 40.0, "the tallest item, not 3.4e38");
        assert!(size.height.is_finite());
    }

    /// `visible_range` adds `beyond_bounds` to the end index; the knob is public, and a big enough
    /// value overflowed `usize` (a debug panic). It saturates now.
    #[test]
    fn an_enormous_beyond_bounds_does_not_overflow() {
        let cache = ItemHeightCache::default();
        assert_eq!(
            visible_range(&cache, 0, 0.0, 200.0, 0.0, 10, usize::MAX),
            (0, 10),
            "saturating, not wrapping"
        );
        let mut composer = Composer::new();
        composer.compose(crate::compose!(|ctx| {
            LazyColumn::new()
                .beyond_bounds(usize::MAX)
                .items(3, |i| i as u64, |ctx, i| sized_item(ctx, i, 50.0, 40.0))
                .build(ctx);
        }));
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        let root = composer.arena.root.expect("root");
        assert!(!composer.arena_nodes()[root].children.is_empty());
    }

    /// A negative spacing would make `prefix_height` non-monotonic (the anchor recovery would land
    /// on the wrong item), so the builder clamps it; the items then stack with no gap.
    #[test]
    fn a_negative_spacing_is_clamped_to_zero() {
        let mut composer = Composer::new();
        composer.compose(crate::compose!(|ctx| {
            LazyColumn::new()
                .spacing(-10.0)
                .items(3, |i| i as u64, |ctx, i| sized_item(ctx, i, 50.0, 40.0))
                .build(ctx);
        }));
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        let root = composer.arena.root.expect("root");
        assert_eq!(
            children_positions(&composer, root),
            vec![(0.0, 0.0), (0.0, 40.0), (0.0, 80.0)]
        );
    }

    /// Negative content padding would let items grow outside the viewport and make the scroll extent
    /// negative; it is clamped where it enters.
    #[test]
    fn negative_content_padding_is_clamped_to_zero() {
        let mut composer = Composer::new();
        composer.compose(crate::compose!(|ctx| {
            LazyColumn::new()
                .content_padding(-20.0, -20.0)
                .items(2, |i| i as u64, |ctx, i| sized_item(ctx, i, 50.0, 40.0))
                .build(ctx);
        }));
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        let root = composer.arena.root.expect("root");
        assert_eq!(
            children_positions(&composer, root),
            vec![(0.0, 0.0), (0.0, 40.0)],
            "the first item starts at the viewport edge, not 20 dp above it"
        );
    }

    /// A NaN page size would be pushed straight into the item constraints and poison every
    /// measurement; a non-finite size is ignored instead, and the items keep their own size.
    #[test]
    fn a_non_finite_item_main_size_is_ignored() {
        let mut composer = Composer::new();
        composer.compose(crate::compose!(|ctx| {
            LazyColumn::new()
                .item_main_size(f32::NAN)
                .items(2, |i| i as u64, |ctx, i| sized_item(ctx, i, 50.0, 40.0))
                .build(ctx);
        }));
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        let root = composer.arena.root.expect("root");
        let positions = children_positions(&composer, root);
        assert_eq!(positions, vec![(0.0, 0.0), (0.0, 40.0)], "no NaN reached a placement");
    }

    /// An empty list has no items to place and must not panic or divide by zero.
    #[test]
    fn an_empty_lazy_list_lays_out_with_no_items() {
        let mut composer = Composer::new();
        composer.compose(crate::compose!(|ctx| {
            LazyColumn::new()
                .items(0, |i| i as u64, |ctx, i| sized_item(ctx, i, 50.0, 40.0))
                .build(ctx);
        }));
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        let root = composer.arena.root.expect("root");
        assert!(composer.arena_nodes()[root].children.is_empty());
        assert_eq!(composer.arena_nodes()[root].measured_size, crate::unit::Size::new(200.0, 200.0));
    }

    /// `scroll_to_item` with a negative in-item offset must not write a negative pixel offset: the
    /// animated path always clamped, the immediate one did not, and every later frame derives the
    /// anchor from that offset.
    #[test]
    fn an_immediate_jump_is_clamped_like_the_animated_one() {
        let state = LazyListState::new();
        let mut composer = Composer::new();
        composer.compose(crate::compose!(|ctx| {
            LazyColumn::new()
                .state(state.clone())
                .items(30, |i| i as u64, |ctx, i| sized_item(ctx, i, 50.0, 40.0))
                .build(ctx);
        }));
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        state.scroll_to_item(5, -500.0);
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        assert_eq!(state.offset(), 0.0, "a negative in-item offset cannot scroll above the start");

        state.scroll_to_item(usize::MAX, 0.0);
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        let max_off = state.fling_limit.peek();
        assert!(max_off.is_finite() && max_off > 0.0);
        assert_eq!(state.offset(), max_off, "an out-of-range index lands at the content's end");
    }

    /// content padding larger than the viewport is legal (the list can scroll into it) and must not
    /// panic or invert the scroll extent.
    #[test]
    fn content_padding_taller_than_the_viewport_stays_scrollable() {
        let state = LazyListState::new();
        let mut composer = Composer::new();
        composer.compose(crate::compose!(|ctx| {
            LazyColumn::new()
                .state(state.clone())
                .content_padding(300.0, 300.0)
                .items(3, |i| i as u64, |ctx, i| sized_item(ctx, i, 50.0, 40.0))
                .build(ctx);
        }));
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        let root = composer.arena.root.expect("root");
        let positions = children_positions(&composer, root);
        assert_eq!(positions[0], (0.0, 300.0), "the first item starts below the 300 dp padding");
        let max_off = state.fling_limit.peek();
        assert_eq!(max_off, 300.0 + 120.0 + 300.0 - 200.0, "content minus the viewport");
    }

    /// Main-axis `content_padding` under RTL: "before" is the start of the content, which is the
    /// RIGHT edge in an RTL row, so the first item is offset from the right.
    #[test]
    fn main_axis_content_padding_mirrors_with_an_rtl_row() {
        let mut composer = Composer::new();
        composer.compose(crate::compose!(|ctx| {
            LazyRow::new()
                // 20 + 3 x 50 + 30 == the 200 dp viewport, so the render mirror is the identity and
                // the arena position IS the screen position.
                .content_padding(20.0, 30.0)
                .modifier(Modifier::new().layout_direction(crate::layout::LayoutDirection::Rtl))
                .items(3, |i| i as u64, |ctx, i| sized_item(ctx, i, 50.0, 40.0))
                .build(ctx);
        }));
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        let root = composer.arena.root.expect("root");
        assert_eq!(
            children_positions(&composer, root)[0],
            (130.0, 0.0),
            "item 0 sits 20 dp in from the RIGHT edge"
        );
    }

    /// `reverseLayout` plus `content_padding`: the padding stays the content's start, and the
    /// mirrored content puts item 0 that far from the far end of the content.
    #[test]
    fn reverse_layout_keeps_the_content_padding_at_the_content_start() {
        let mut composer = Composer::new();
        composer.compose(crate::compose!(|ctx| {
            LazyColumn::new()
                .reverse_layout(true)
                .content_padding(20.0, 0.0)
                .items(3, |i| i as u64, |ctx, i| sized_item(ctx, i, 50.0, 40.0))
                .build(ctx);
        }));
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        let root = composer.arena.root.expect("root");
        // content = 20 + 3*40 = 140; item 0 sits at 140 - 20 - 40 = 80.
        assert_eq!(children_positions(&composer, root)[0], (0.0, 80.0));
    }

    // ── Layout info (Compose's LazyListLayoutInfo) ──

    fn ten_item_column(ctx: &mut ComposeCtx) {
        LazyColumn::new()
            .items(10, |i| i as u64, |ctx, i| sized_item(ctx, i, 50.0, 40.0))
            .build(ctx);
    }

    /// The snapshot a caller reads: which items are visible, where they start relative to the list,
    /// how big they are, and the viewport bounds the visibility is measured against.
    #[test]
    fn layout_info_reports_the_visible_items() {
        let state = LazyListState::new();
        let mut composer = Composer::new();
        composer.compose(crate::compose!(|ctx| {
            LazyColumn::new()
                .state(state.clone())
                .items(10, |i| i as u64, |ctx, i| sized_item(ctx, i, 50.0, 40.0))
                .build(ctx);
        }));
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));

        let info = state.layout_info();
        assert_eq!(info.total_items, 10);
        assert_eq!((info.viewport_size.0, info.viewport_size.1), (200.0, 200.0));
        assert!(info.vertical && !info.reverse_layout && !info.rtl && !info.mirrored());
        assert_eq!(info.viewport_start_offset, 0.0);
        assert_eq!(info.viewport_end_offset, 200.0);
        let items: Vec<(usize, f32, f32)> =
            info.visible_items.iter().map(|i| (i.index, i.offset, i.size)).collect();
        assert_eq!(
            items,
            vec![(0, 0.0, 40.0), (1, 40.0, 40.0), (2, 80.0, 40.0), (3, 120.0, 40.0), (4, 160.0, 40.0)],
            "five items fit, and the sixth starts exactly at the viewport's end"
        );
        assert!(info.is_fully_visible(160.0, 40.0), "160 + 40 is the viewport's end");
        assert!(!info.is_fully_visible(200.0, 40.0), "an item starting at the end is not visible");
    }

    /// Scrolled past an item, its offset is NEGATIVE — that is Compose's contract, and it is what
    /// makes "is the first item fully visible" answerable.
    #[test]
    fn layout_info_offsets_go_negative_once_scrolled() {
        let state = LazyListState::new();
        let mut composer = Composer::new();
        composer.compose(crate::compose!(|ctx| {
            LazyColumn::new()
                .state(state.clone())
                .items(10, |i| i as u64, |ctx, i| sized_item(ctx, i, 50.0, 40.0))
                .build(ctx);
        }));
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        state.offset.set(60.0);
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));

        let info = state.layout_info();
        // At offset 60 item 0 spans -60..-20, entirely above the viewport, so it is NOT visible:
        // item 1 is the first one that still intersects, and it starts at -20.
        assert_eq!(info.visible_items[0].index, 1);
        assert_eq!(info.visible_items[0].offset, -20.0);
        assert_eq!(info.visible_items[1].index, 2);
        assert_eq!(info.visible_items[1].offset, 20.0);
        assert!(
            !info.is_fully_visible(-20.0, 40.0),
            "a partially visible first item is not fully visible"
        );
        assert!(!info.visible_items.iter().any(|item| item.index == 0), "item 0 scrolled out");
    }

    /// `contentPadding` shifts the items AND the viewport bounds: Compose reports
    /// `viewportStartOffset = -beforeContentPadding` so the padding area still counts as visible.
    #[test]
    fn layout_info_folds_in_the_content_padding() {
        let state = LazyListState::new();
        let mut composer = Composer::new();
        composer.compose(crate::compose!(|ctx| {
            LazyColumn::new()
                .state(state.clone())
                .content_padding(20.0, 10.0)
                .items(10, |i| i as u64, |ctx, i| sized_item(ctx, i, 50.0, 40.0))
                .build(ctx);
        }));
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));

        let info = state.layout_info();
        assert_eq!(info.viewport_start_offset, -20.0);
        assert_eq!(info.viewport_end_offset, 210.0);
        assert_eq!(info.before_content_padding, 20.0);
        assert_eq!(info.after_content_padding, 10.0);
        assert_eq!(info.visible_items[0].offset, 20.0, "the first item starts after the padding");
    }

    /// The composition window covers `beyond_bounds` extra items on each side; those are composed but
    /// NOT visible, and the snapshot must not list them.
    #[test]
    fn layout_info_excludes_the_prefetch_window() {
        let state = LazyListState::new();
        let mut composer = Composer::new();
        composer.compose(crate::compose!(|ctx| {
            LazyColumn::new()
                .state(state.clone())
                .beyond_bounds(8)
                .items(200, |i| i as u64, |ctx, i| sized_item(ctx, i, 50.0, 40.0))
                .build(ctx);
        }));
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));

        let info = state.layout_info();
        assert_eq!(
            info.visible_items.len(),
            5,
            "five 40 dp items in a 200 dp viewport, prefetched neighbours excluded: {:?}",
            info.visible_items.iter().map(|i| i.index).collect::<Vec<_>>()
        );
        assert_eq!(info.visible_items.last().map(|i| i.index), Some(4));
    }

    /// Keys come from the item's registration, so a caller can match a visible item back to its data.
    #[test]
    fn layout_info_carries_the_item_keys() {
        let state = LazyListState::new();
        let mut composer = Composer::new();
        composer.compose(crate::compose!(|ctx| {
            LazyColumn::new()
                .state(state.clone())
                .items(10, |i| 1000 + i as u64, |ctx, i| sized_item(ctx, i, 50.0, 40.0))
                .build(ctx);
        }));
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        let info = state.layout_info();
        assert_eq!(
            info.visible_items.iter().map(|i| i.key).collect::<Vec<_>>(),
            vec![1000, 1001, 1002, 1003, 1004]
        );
    }

    /// An RTL row is mirrored, so offsets are measured from the right: item 0 starts at
    /// `viewport - width` and the LAST visible item starts at 0.
    #[test]
    fn an_rtl_row_reports_mirrored_offsets() {
        let state = LazyListState::new();
        let mut composer = Composer::new();
        composer.compose(crate::compose!(|ctx| {
            LazyRow::new()
                .state(state.clone())
                .modifier(Modifier::new().layout_direction(crate::layout::LayoutDirection::Rtl))
                .items(4, |i| i as u64, |ctx, i| sized_item(ctx, i, 50.0, 40.0))
                .build(ctx);
        }));
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));

        let info = state.layout_info();
        assert!(!info.vertical && info.rtl && info.mirrored());
        assert_eq!(
            info.visible_items.iter().map(|i| (i.index, i.offset)).collect::<Vec<_>>(),
            vec![(0, 150.0), (1, 100.0), (2, 50.0), (3, 0.0)],
            "item 0 at the right edge, running leftward"
        );
    }

    /// A reverse column lays its content out from the far end, so the LAST item sits at offset 0 and
    /// item 0 near the viewport's end.
    #[test]
    fn a_reverse_column_reports_offsets_from_the_far_end() {
        let state = LazyListState::new();
        let mut composer = Composer::new();
        composer.compose(crate::compose!(|ctx| {
            LazyColumn::new()
                .state(state.clone())
                .reverse_layout(true)
                .items(3, |i| i as u64, |ctx, i| sized_item(ctx, i, 50.0, 40.0))
                .build(ctx);
        }));
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));

        let info = state.layout_info();
        assert!(info.reverse_layout && info.mirrored());
        assert_eq!(
            info.visible_items.iter().map(|i| (i.index, i.offset)).collect::<Vec<_>>(),
            vec![(0, 160.0), (1, 120.0), (2, 80.0)],
            "the 120 dp of content sits against the far end of the 200 dp viewport"
        );
    }

    /// Before the first measure there is nothing to report — an empty snapshot, not a panic.
    #[test]
    fn layout_info_is_empty_before_the_first_measure() {
        let state = LazyListState::new();
        let info = state.layout_info();
        assert!(info.visible_items.is_empty());
        assert_eq!(info.total_items, 0);
        assert_eq!(info.viewport_size, (0.0, 0.0));
    }

    /// Scrolling re-writes the snapshot: the first visible index moves with the offset.
    #[test]
    fn layout_info_follows_the_scroll_offset() {
        let state = LazyListState::new();
        let mut composer = Composer::new();
        composer.compose(crate::compose!(|ctx| {
            LazyColumn::new()
                .state(state.clone())
                .items(200, |i| i as u64, |ctx, i| sized_item(ctx, i, 50.0, 40.0))
                .build(ctx);
        }));
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        assert_eq!(state.layout_info().visible_items[0].index, 0);
        state.offset.set(400.0);
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        let info = state.layout_info();
        assert_eq!(info.visible_items[0].index, 10, "offset 400 / 40 is item 10, at offset 0");
        assert_eq!(info.visible_items[0].offset, 0.0);
    }

    /// A pinned sticky header is placed at the viewport's start, so it appears in the snapshot at
    /// offset 0 even though its data position is far behind the anchor.
    #[test]
    fn a_pinned_sticky_header_appears_in_the_layout_info() {
        let state = LazyListState::new();
        let mut composer = Composer::new();
        composer.compose(crate::compose!(|ctx| {
            let mut list = LazyColumn::new()
                .state(state.clone())
                .sticky_header(0, |ctx| sized_item(ctx, 0, 50.0, 40.0));
            list = list.items(40, |i| 100 + i as u64, |ctx, i| sized_item(ctx, i, 50.0, 40.0));
            list.build(ctx);
        }));
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        state.offset.set(200.0);
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));

        let info = state.layout_info();
        let header = info
            .visible_items
            .iter()
            .find(|item| item.key == 0)
            .expect("the pinned header is visible");
        assert_eq!(header.offset, 0.0, "pinned at the viewport start");
        assert_eq!(header.index, 0, "and it keeps its data index");
    }

    // ── Accessibility: the list is a collection, each row an item ──

    fn semantics_of(composer: &crate::runtime::composer::Composer, root: usize) -> Vec<crate::semantics::SemanticsNode> {
        crate::semantics::semantics_tree(composer.arena_nodes(), root)
    }

    /// A `LazyColumn` declares a collection of `total` rows, and each composed row declares its own
    /// index — the pair a reader turns into "row 3 of 200".
    #[test]
    fn a_lazy_column_declares_a_collection_and_its_item_positions() {
        let state = LazyListState::new();
        let mut composer = Composer::new();
        composer.compose(crate::compose!(|ctx| {
            LazyColumn::new()
                .state(state.clone())
                .items(200, |i| i as u64, |ctx, i| sized_item(ctx, i, 50.0, 40.0))
                .build(ctx);
        }));
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        let root = composer.arena.root.expect("root");
        let tree = semantics_of(&composer, root);
        let list = tree
            .iter()
            .find(|node| node.collection.is_some())
            .expect("the list is a collection");
        assert_eq!(
            list.collection,
            Some(crate::semantics::CollectionInfo::rows(200)),
            "a column counts ROWS"
        );
        let positions: Vec<usize> = list
            .children
            .iter()
            .filter_map(|node| node.collection_item.map(|info| info.index))
            .collect();
        // The tree covers the whole COMPOSED window (the visible rows plus the beyond-bounds
        // prefetch), so the assertion is: the positions are contiguous from 0, and every VISIBLE
        // item is among them — a reader filters the rest by bounds.
        assert_eq!(positions, (0..positions.len()).collect::<Vec<_>>(), "contiguous from the head");
        let visible: Vec<usize> = state.layout_info().visible_items.iter().map(|i| i.index).collect();
        assert_eq!(visible, vec![0, 1, 2, 3, 4], "five rows fit");
        assert!(positions.len() > visible.len(), "the prefetch is composed too: {positions:?}");
        assert!(visible.iter().all(|index| positions.contains(index)));
    }

    /// A `LazyRow` counts COLUMNS — same declaration, the other axis.
    #[test]
    fn a_lazy_row_declares_columns() {
        let state = LazyListState::new();
        let mut composer = Composer::new();
        composer.compose(crate::compose!(|ctx| {
            LazyRow::new()
                .state(state.clone())
                .items(40, |i| i as u64, |ctx, i| sized_item(ctx, i, 50.0, 40.0))
                .build(ctx);
        }));
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        let root = composer.arena.root.expect("root");
        let tree = semantics_of(&composer, root);
        let list = tree.iter().find(|node| node.collection.is_some()).expect("collection");
        assert_eq!(list.collection, Some(crate::semantics::CollectionInfo::columns(40)));
        let positions: Vec<usize> = list
            .children
            .iter()
            .filter_map(|node| node.collection_item.map(|info| info.index))
            .collect();
        assert_eq!(positions, (0..positions.len()).collect::<Vec<_>>(), "contiguous from the head");
        let visible: Vec<usize> = state.layout_info().visible_items.iter().map(|i| i.index).collect();
        assert_eq!(visible, vec![0, 1, 2, 3], "four 50 dp tiles fit in 200 dp");
    }

    /// The positions are DATA indices, so they follow the scroll: after scrolling to offset 400 the
    /// first composed row is 10, and an RTL row still counts from 0 at its data start.
    #[test]
    fn the_item_positions_follow_the_data_index() {
        let state = LazyListState::new();
        let mut composer = Composer::new();
        composer.compose(crate::compose!(|ctx| {
            LazyColumn::new()
                .state(state.clone())
                .items(200, |i| i as u64, |ctx, i| sized_item(ctx, i, 50.0, 40.0))
                .build(ctx);
        }));
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        // A real frame: the window is chosen during COMPOSITION, so scrolling needs a compose as
        // well as a layout — a layout alone keeps the rows the previous compose registered.
        state.offset.set(400.0);
        composer.compose(crate::compose!(|ctx| {
            LazyColumn::new()
                .state(state.clone())
                .items(200, |i| i as u64, |ctx, i| sized_item(ctx, i, 50.0, 40.0))
                .build(ctx);
        }));
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        let root = composer.arena.root.expect("root");
        let tree = semantics_of(&composer, root);
        let list = tree.iter().find(|node| node.collection.is_some()).expect("collection");
        let positions: Vec<usize> = list
            .children
            .iter()
            .filter_map(|node| node.collection_item.map(|info| info.index))
            .collect();
        let visible: Vec<usize> = state.layout_info().visible_items.iter().map(|i| i.index).collect();
        assert_eq!(visible, vec![10, 11, 12, 13, 14], "offset 400 at 40 dp per row");
        assert!(positions.contains(&10), "and the window starts at the anchor: {positions:?}");
    }

    /// An empty list still declares its (zero-length) collection: a reader should hear "list, empty"
    /// rather than nothing at all.
    #[test]
    fn an_empty_lazy_list_still_declares_its_collection() {
        let state = LazyListState::new();
        let mut composer = Composer::new();
        composer.compose(crate::compose!(|ctx| {
            LazyColumn::new()
                .state(state.clone())
                .items(0, |i| i as u64, |ctx, i| sized_item(ctx, i, 50.0, 40.0))
                .build(ctx);
        }));
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        let root = composer.arena.root.expect("root");
        let tree = semantics_of(&composer, root);
        let list = tree.iter().find(|node| node.collection.is_some()).expect("collection");
        assert_eq!(list.collection, Some(crate::semantics::CollectionInfo::rows(0)));
        assert!(list.children.is_empty());
    }

    /// The item wrapper keeps its content as a child, so a row's text is still readable ("row 3" with
    /// its label) instead of being swallowed by the position declaration.
    #[test]
    fn a_row_keeps_its_content_under_its_position() {
        let state = LazyListState::new();
        let mut composer = Composer::new();
        composer.compose(crate::compose!(|ctx| {
            LazyColumn::new()
                .state(state.clone())
                .items(5, |i| i as u64, |ctx, i| {
                    crate::components::Text::new(format!("row {i}"))
                        .modifier(Modifier::new().size(50.0, 40.0))
                        .build(ctx);
                })
                .build(ctx);
        }));
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        let root = composer.arena.root.expect("root");
        let tree = semantics_of(&composer, root);
        let list = tree.iter().find(|node| node.collection.is_some()).expect("collection");
        let first = list.children.first().expect("a row element");
        assert_eq!(first.collection_item.map(|info| info.index), Some(0));
        assert!(
            first.find_by_name("row 0").is_some(),
            "the row's text is still in the tree under its position"
        );
    }

    /// Compose's `ScrollToIndex`: the list answers a reader's "bring item N into view" with the
    /// same anchored jump `scroll_to_item` performs, and the item really is visible on the next frame.
    #[test]
    fn the_list_offers_scroll_to_index_and_it_jumps() {
        let state = LazyListState::new();
        let mut composer = Composer::new();
        composer.compose(crate::compose!(|ctx| {
            LazyColumn::new()
                .state(state.clone())
                .items(200, |i| i as u64, |ctx, i| sized_item(ctx, i, 50.0, 40.0))
                .build(ctx);
        }));
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));

        let root = composer.arena.root.expect("root");
        let tree = semantics_of(&composer, root);
        let list = tree.iter().find(|node| node.collection.is_some()).expect("collection");
        let action = list
            .scroll_to_index
            .as_ref()
            .expect("the list offers ScrollToIndex")
            .clone();

        // Invoke it the way a platform bridge would, then run a real frame.
        action.invoke(50);
        composer.compose(crate::compose!(|ctx| {
            LazyColumn::new()
                .state(state.clone())
                .items(200, |i| i as u64, |ctx, i| sized_item(ctx, i, 50.0, 40.0))
                .build(ctx);
        }));
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));

        assert_eq!(state.first_visible(), 50, "the jump lands on the requested index");
        let info = state.layout_info();
        assert_eq!(
            info.visible_items.first().map(|item| item.index),
            Some(50),
            "and the first visible item is the one asked for"
        );
        assert!(info.is_fully_visible(info.visible_items[0].offset, info.visible_items[0].size));
    }

    /// An out-of-range request is clamped like any other jump, not dropped: a reader asking for an
    /// index past the end should land at the end.
    #[test]
    fn scroll_to_index_past_the_end_lands_at_the_end() {
        let state = LazyListState::new();
        let mut composer = Composer::new();
        composer.compose(crate::compose!(|ctx| {
            LazyColumn::new()
                .state(state.clone())
                .items(20, |i| i as u64, |ctx, i| sized_item(ctx, i, 50.0, 40.0))
                .build(ctx);
        }));
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        let root = composer.arena.root.expect("root");
        let tree = semantics_of(&composer, root);
        let action = tree
            .iter()
            .find(|node| node.collection.is_some())
            .and_then(|node| node.scroll_to_index.clone())
            .expect("the list offers ScrollToIndex");
        action.invoke(9999);
        composer.compose(crate::compose!(|ctx| {
            LazyColumn::new()
                .state(state.clone())
                .items(20, |i| i as u64, |ctx, i| sized_item(ctx, i, 50.0, 40.0))
                .build(ctx);
        }));
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        let info = state.layout_info();
        assert_eq!(
            info.visible_items.last().map(|item| item.index),
            Some(19),
            "the request is clamped to the last item: {:?}",
            info.visible_items.iter().map(|i| i.index).collect::<Vec<_>>()
        );
    }

    // ── Compose's IndexForKey ──

    /// Compose's `IndexForKey`: a reader keeps its place by key, so the collection has to say which
    /// index a key has now — including after items were inserted in front of it.
    #[test]
    fn index_for_key_maps_a_key_to_its_current_index() {
        let state = LazyListState::new();
        let mut composer = Composer::new();
        let keys: Arc<Vec<u64>> = Arc::new((0..50).map(|i| 1000 + i as u64).collect());
        let build = |composer: &mut Composer, keys: Arc<Vec<u64>>| {
            composer.compose(crate::compose!(|ctx| {
                LazyColumn::new()
                    .state(state.clone())
                    .items_from(keys, |k| *k, |ctx, i, _k| sized_item(ctx, i, 50.0, 40.0))
                    .build(ctx);
            }));
            composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        };
        build(&mut composer, keys.clone());

        let root = composer.arena.root.expect("root");
        let action = semantics_of(&composer, root)
            .iter()
            .find_map(|node| node.index_for_key.clone())
            .expect("the list offers IndexForKey");
        assert_eq!(action.invoke(1000), Some(0));
        assert_eq!(action.invoke(1042), Some(42));
        assert_eq!(action.invoke(999), None, "a key that is not in the data has no index");

        // Insert two items in front: every old key moves by two, and the lookup follows.
        let shifted: Arc<Vec<u64>> =
            Arc::new((0..2).map(|i| 5000 + i as u64).chain(keys.iter().copied()).collect());
        build(&mut composer, shifted);
        let root = composer.arena.root.expect("root");
        let action = semantics_of(&composer, root)
            .iter()
            .find_map(|node| node.index_for_key.clone())
            .expect("the rebuilt list offers it too");
        assert_eq!(action.invoke(1000), Some(2), "the key kept its identity, the index moved");
        assert_eq!(action.invoke(1042), Some(44));
        assert_eq!(action.invoke(5000), Some(0));
    }

    /// A section without a key factory uses the index as its key, so the lookup answers the index
    /// itself — the same rule `keys` follows everywhere else in the module.
    #[test]
    fn index_for_key_answers_the_index_for_unkeyed_sections() {
        let state = LazyListState::new();
        let mut composer = Composer::new();
        composer.compose(crate::compose!(|ctx| {
            LazyColumn::new()
                .state(state.clone())
                .items_plain(20, |ctx, i| sized_item(ctx, i, 50.0, 40.0))
                .build(ctx);
        }));
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        let root = composer.arena.root.expect("root");
        let action = semantics_of(&composer, root)
            .iter()
            .find_map(|node| node.index_for_key.clone())
            .expect("the list offers IndexForKey");
        assert_eq!(action.invoke(7), Some(7));
        assert_eq!(action.invoke(20), None, "past the end");
    }

    /// A key FACTORY gets the index inside its own section, and so does the content closure — Compose
    /// hands `items(count, key = { index -> … })` a section index. A sticky header registered first
    /// shifts every GLOBAL index by one, which used to make `items_from` index its data out of bounds
    /// ("the len is 50 but the index is 50", measured in the demo).
    #[test]
    fn a_section_after_a_sticky_header_still_sees_its_own_indices() {
        let state = LazyListState::new();
        let data: Arc<Vec<u64>> = Arc::new((100..110).collect());
        let mut composer = Composer::new();
        composer.compose(crate::compose!(|ctx| {
            LazyColumn::new()
                .state(state.clone())
                .sticky_header(u64::MAX, |ctx| sized_item(ctx, 0, 50.0, 20.0))
                .items_from(data.clone(), |key| *key, |ctx, index, key| {
                    assert_eq!(
                        *key,
                        100 +
                            index as u64,
                        "the content closure gets the section-local index"
                    );
                    sized_item(ctx, index, 50.0, 40.0);
                })
                .build(ctx);
        }));
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));

        let info = state.layout_info();
        let keys: Vec<u64> = info.visible_items.iter().map(|item| item.key).collect();
        assert_eq!(
            keys.first().copied(),
            Some(u64::MAX),
            "the header keeps its own key: {keys:?}"
        );
        assert_eq!(keys.get(1).copied(), Some(100), "and the first item its data key: {keys:?}");

        // The global position is still what the collection reports: the header occupies index 0.
        let root = composer.arena.root.expect("root");
        let action = semantics_of(&composer, root)
            .iter()
            .find_map(|node| node.index_for_key.clone())
            .expect("the list offers IndexForKey");
        assert_eq!(action.invoke(100), Some(1));
        assert_eq!(action.invoke(109), Some(10));
    }

    /// `user_scroll(false)` takes the GESTURES away, not the container: the list keeps its clip (or it
    /// would paint its items over the next card — measured in the demo) and keeps answering a
    /// programmatic jump.
    #[test]
    fn a_locked_list_keeps_its_container_and_loses_only_its_gestures() {
        let state = LazyListState::new();
        let mut composer = Composer::new();
        composer.compose(crate::compose!(|ctx| {
            LazyColumn::new()
                .state(state.clone())
                .user_scroll(false)
                .items(30, |i| i as u64, |ctx, i| sized_item(ctx, i, 50.0, 40.0))
                .build(ctx);
        }));
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        let root = composer.arena.root.expect("root");
        let modifier = &composer.arena_nodes()[root].modifier;
        assert!(
            modifier.vertical_scroll_state().is_some(),
            "the scroll container stays: it owns the clip and the viewport"
        );
        assert!(
            modifier.is_scroll_input_disabled(),
            "but the gesture paths skip it"
        );

        // And a programmatic jump still works, because it goes through the measure pass.
        state.scroll_to_item(20, 0.0);
        composer.compose(crate::compose!(|ctx| {
            LazyColumn::new()
                .state(state.clone())
                .user_scroll(false)
                .items(30, |i| i as u64, |ctx, i| sized_item(ctx, i, 50.0, 40.0))
                .build(ctx);
        }));
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        assert_eq!(
            state.layout_info().visible_items.first().map(|item| item.index),
            Some(20),
            "a locked list still jumps"
        );
    }

    /// A key that is new to the composed WINDOW is not a new item. Scrolling into fresh territory
    /// composes keys that were always in the data; fading them in makes a scroll look like a strobe
    /// (measured before the fix: a jump to offset 4000 composed keys 35..39 at alpha 0).
    #[test]
    fn a_scroll_does_not_fade_items_in() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let state = LazyListState::new();
        let mut composer = Composer::new();
        let items: Arc<Vec<u64>> = Arc::new((0..40).collect());
        let mut build = |composer: &mut Composer, items: Arc<Vec<u64>>| {
            let s = state.clone();
            composer.compose(crate::compose!(|ctx| {
                LazyColumn::new()
                    .state(s)
                    .modifier(Modifier::new().fill_max_width().height(200.0))
                    .items_from(items, |k| *k, |ctx, _i, k| {
                        crate::layout::Column::new()
                            .modifier(Modifier::new().animate_item().fill_max_width().height(56.0))
                            .build(ctx, |ctx| {
                                crate::components::text::Text::new(format!("Item {k}")).build(ctx);
                            });
                    })
                    .build(ctx);
            }));
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 200.0));
        };
        let alphas = |composer: &Composer| -> Vec<(u64, f32)> {
            let mut rows: Vec<(u64, f32)> = composer
                .arena_nodes()
                .iter()
                .filter_map(|n| n.item_paint.as_ref().map(|p| (p.key, p.alpha.peek())))
                .collect();
            rows.sort_by(|a, b| a.0.cmp(&b.0));
            rows
        };

        build(&mut composer, items.clone());
        assert!(
            alphas(&composer).iter().all(|(_, alpha)| *alpha == 1.0),
            "the first frame shows every item at full alpha"
        );

        // Scroll far enough that keys the window never held are composed for the first time.
        state.offset.set(4000.0);
        build(&mut composer, items);
        let after = alphas(&composer);
        assert!(!after.is_empty(), "the far window is composed: {after:?}");
        assert!(
            after.iter().all(|(_, alpha)| *alpha == 1.0),
            "a scroll must not fade the items it reveals in: {after:?}"
        );
        crate::animation::clear_all_animations();
    }

    /// The other half of the rule: when the DATA changed, the new key still fades in.
    #[test]
    fn an_insert_fades_the_new_item_in() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let state = LazyListState::new();
        let mut composer = Composer::new();
        let mut build = |composer: &mut Composer, items: Arc<Vec<u64>>| {
            let s = state.clone();
            composer.compose(crate::compose!(|ctx| {
                LazyColumn::new()
                    .state(s)
                    .modifier(Modifier::new().fill_max_width().height(200.0))
                    .items_from(items, |k| *k, |ctx, _i, k| {
                        crate::layout::Column::new()
                            .modifier(Modifier::new().animate_item().fill_max_width().height(56.0))
                            .build(ctx, |ctx| {
                                crate::components::text::Text::new(format!("Item {k}")).build(ctx);
                            });
                    })
                    .build(ctx);
            }));
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 200.0));
        };

        build(&mut composer, Arc::new((0..8).collect()));
        let mut items: Vec<u64> = vec![999];
        items.extend(0..8);
        build(&mut composer, Arc::new(items));
        let inserted = composer
            .arena_nodes()
            .iter()
            .filter_map(|n| n.item_paint.as_ref())
            .find(|paint| paint.key == 999)
            .expect("the inserted item is painted");
        assert!(
            inserted.alpha.peek() < 0.001,
            "an inserted key fades in from transparent, got {}",
            inserted.alpha.peek()
        );
        crate::animation::clear_all_animations();
    }
    /// A re-layout that does not move the item must not restart its placement animation: a scroll
    /// re-lays out on every frame, and restarting from the in-flight offset throws the spring's
    /// velocity away — the item then crawls towards its slot instead of settling (the "item
    /// animation gets slow while scrolling" report).
    #[test]
    fn a_relayout_that_does_not_move_the_item_keeps_its_placement_animation() {
        let (_guard, clock) = crate::animation::tests::serial_clock();
        crate::animation::clear_all_animations();
        let state = LazyListState::new();
        let mut composer = Composer::new();
        let mut build = |composer: &mut Composer, items: Arc<Vec<u64>>| {
            let s = state.clone();
            composer.compose(crate::compose!(|ctx| {
                LazyColumn::new()
                    .state(s)
                    .modifier(Modifier::new().fill_max_width().fill_max_height())
                    .items_from(items, |k| *k, |ctx, _i, k| {
                        crate::layout::Column::new()
                            .modifier(Modifier::new().animate_item().fill_max_width().height(48.0))
                            .build(ctx, |ctx| {
                                crate::components::text::Text::new(format!("Item {k}")).build(ctx);
                            });
                    })
                    .build(ctx);
            }));
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 600.0));
        };
        let offset_of = |composer: &Composer, key: u64| -> f32 {
            composer
                .arena_nodes()
                .iter()
                .filter_map(|n| n.item_paint.as_ref())
                .find(|paint| paint.key == key)
                .map(|paint| paint.offset.peek().y)
                .unwrap_or(0.0)
        };

        build(&mut composer, Arc::new((0..20).collect()));
        // Insert in the middle: everything below moves down one row and starts a placement animation.
        let mut items: Vec<u64> = (0..10).collect();
        items.push(999);
        items.extend(10..20);
        let items = Arc::new(items);
        build(&mut composer, items.clone());
        let start = offset_of(&composer, 15);
        assert!(start.abs() > 10.0, "the shift started an animation: {start}");

        // Ten frames of a SCROLL: the offset changes (so the list re-lays out every frame) while the
        // item's slot in the content does not move at all.
        let mut scrolled = 0.0f32;
        for _ in 0..10 {
            scrolled += 1.0;
            state.offset.set(scrolled);
            clock.advance(std::time::Duration::from_millis(16));
            crate::animation::update_animations();
            build(&mut composer, items.clone());
        }
        let after = offset_of(&composer, 15);
        assert!(
            after.abs() < start.abs() * 0.6,
            "the animation keeps progressing across scroll re-layouts: {start} -> {after}"
        );
        crate::animation::clear_all_animations();
    }
    /// A list whose data starts EMPTY must still fade its first real content in. "First layout" is a
    /// FLAG, not "the key set is empty": an async load or a filter that transiently matches nothing
    /// would otherwise suppress the fade for its first real data — and again after every empty frame.
    #[test]
    fn content_that_arrives_after_an_empty_frame_fades_in() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let state = LazyListState::new();
        let mut composer = Composer::new();
        let mut build = |composer: &mut Composer, items: Arc<Vec<u64>>| {
            let s = state.clone();
            composer.compose(crate::compose!(|ctx| {
                LazyColumn::new()
                    .state(s)
                    .modifier(Modifier::new().fill_max_width().fill_max_height())
                    .items_from(items, |k| *k, |ctx, _i, k| {
                        crate::layout::Column::new()
                            .modifier(Modifier::new().animate_item().fill_max_width().height(48.0))
                            .build(ctx, |ctx| {
                                crate::components::text::Text::new(format!("Item {k}")).build(ctx);
                            });
                    })
                    .build(ctx);
            }));
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 600.0));
        };

        build(&mut composer, Arc::new(Vec::new()));
        build(&mut composer, Arc::new((0..8).collect()));
        let alpha = composer
            .arena_nodes()
            .iter()
            .filter_map(|n| n.item_paint.as_ref())
            .find(|paint| paint.key == 0)
            .map(|paint| paint.alpha.peek())
            .expect("the first real item is painted");
        assert!(
            alpha < 0.001,
            "content that arrives after an empty frame fades in, got {alpha}"
        );
        crate::animation::clear_all_animations();
    }
    /// A data change must not move the VIEWPORT: the anchor key keeps its pixel position. The
    /// correction used to snap the anchor to the top of the viewport, so at launch (offset 0, content
    /// padding 4) the first insert scrolled the list up 4 px, and mid-scroll it jumped by whatever
    /// fraction of an item the anchor was into the viewport.
    #[test]
    fn a_data_change_keeps_the_anchor_at_its_pixel_position() {
        let state = LazyListState::new();
        let mut composer = Composer::new();
        let mut build = |composer: &mut Composer, items: Arc<Vec<u64>>| {
            let s = state.clone();
            composer.compose(crate::compose!(|ctx| {
                LazyColumn::new()
                    .state(s)
                    .modifier(Modifier::new().fill_max_width().height(300.0))
                    .spacing(8.0)
                    .content_padding(4.0, 4.0)
                    .items_from(items, |k| *k, |ctx, _i, k| {
                        crate::components::text::Text::new(format!("Item {k}"))
                            .modifier(Modifier::new().height(48.0))
                            .build(ctx);
                    })
                    .build(ctx);
            }));
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 300.0));
        };

        build(&mut composer, Arc::new((0..20).collect()));
        assert_eq!(state.offset(), 0.0, "a fresh list sits at the top");

        // Insert in the MIDDLE: nothing above the anchor moved, so the offset must not move either.
        let mut items: Vec<u64> = (0..10).collect();
        items.push(999);
        items.extend(10..20);
        build(&mut composer, Arc::new(items));
        assert_eq!(
            state.offset(),
            0.0,
            "the first insert must not scroll the list (it used to snap to the content padding)"
        );

        // Now scroll to a non-boundary position (20 dp INTO item 3) and insert at the HEAD: the anchor
        // key is what has to stay put, and its pixel position has to survive too.
        state.scroll_to_item(3, 20.0);
        build(&mut composer, Arc::new((0..20).collect()));
        build(&mut composer, Arc::new((0..20).collect()));
        let before = state.offset();
        assert!(before > 100.0, "the list really scrolled: {before}");
        let mut items: Vec<u64> = vec![77];
        items.extend(0..20);
        build(&mut composer, Arc::new(items));
        // The anchor item's VISUAL position is what must not move, so the offset has to grow by exactly
        // the inserted row (48 + 8 spacing) — not stay put (that would move the content up) and not
        // snap to a row boundary (that would move it by the 20 dp the anchor was into its slot).
        assert_eq!(
            state.offset(),
            before + 56.0,
            "the anchor item keeps its pixel position when a row is inserted above it"
        );
    }
    /// Inserting at the head while the list is SCROLLED pushes every row down and moves the viewport
    /// down by the same amount, so nothing moves on screen and nothing may animate. Without handing
    /// the correction's shift to the item host, the correction and the placement animation compensate
    /// the same shift twice and every row flashes one row up before springing back.
    #[test]
    fn an_insert_at_the_head_while_scrolled_does_not_animate_the_rows() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let state = LazyListState::new();
        let mut composer = Composer::new();
        let mut build = |composer: &mut Composer, items: Arc<Vec<u64>>| {
            let s = state.clone();
            composer.compose(crate::compose!(|ctx| {
                LazyColumn::new()
                    .state(s)
                    .modifier(Modifier::new().fill_max_width().height(300.0))
                    .spacing(8.0)
                    .content_padding(4.0, 4.0)
                    .items_from(items, |k| *k, |ctx, _i, k| {
                        crate::layout::Column::new()
                            .modifier(Modifier::new().animate_item().fill_max_width().height(48.0))
                            .build(ctx, |ctx| {
                                crate::components::text::Text::new(format!("Item {k}")).build(ctx);
                            });
                    })
                    .build(ctx);
            }));
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 300.0));
        };

        build(&mut composer, Arc::new((0..20).collect()));
        // Scroll first: the anchor has something to preserve only then.
        state.scroll_to_item(3, 20.0);
        build(&mut composer, Arc::new((0..20).collect()));
        build(&mut composer, Arc::new((0..20).collect()));
        let mut items: Vec<u64> = vec![99];
        items.extend(0..20);
        build(&mut composer, Arc::new(items));

        assert!(
            state.offset() >= 55.0,
            "the correction moved the viewport by one row: {}",
            state.offset()
        );
        let offsets: Vec<(u64, f32)> = composer
            .arena_nodes()
            .iter()
            .filter_map(|n| n.item_paint.as_ref().map(|p| (p.key, p.offset.peek().y)))
            .collect();
        assert!(offsets.len() >= 5, "the rows carry paint channels: {offsets:?}");
        assert!(
            offsets.iter().all(|(_, y)| y.abs() < 0.5),
            "no row may animate when the whole list only scrolled: {offsets:?}"
        );
        crate::animation::clear_all_animations();
    }
    /// At the very TOP there is no reading position to preserve, so an insert at the head shows its
    /// item (the viewport stays at 0) and the rows below ANIMATE down into their new slots. Compose
    /// anchors by key here and silently scrolls instead; this is a deliberate deviation.
    #[test]
    fn an_insert_at_the_head_while_at_the_top_reveals_the_new_item() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let state = LazyListState::new();
        let mut composer = Composer::new();
        let mut build = |composer: &mut Composer, items: Arc<Vec<u64>>| {
            let s = state.clone();
            composer.compose(crate::compose!(|ctx| {
                LazyColumn::new()
                    .state(s)
                    .modifier(Modifier::new().fill_max_width().height(300.0))
                    .spacing(8.0)
                    .content_padding(4.0, 4.0)
                    .items_from(items, |k| *k, |ctx, _i, k| {
                        crate::layout::Column::new()
                            .modifier(Modifier::new().animate_item().fill_max_width().height(48.0))
                            .build(ctx, |ctx| {
                                crate::components::text::Text::new(format!("Item {k}")).build(ctx);
                            });
                    })
                    .build(ctx);
            }));
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 300.0));
        };

        build(&mut composer, Arc::new((0..20).collect()));
        let mut items: Vec<u64> = vec![99];
        items.extend(0..20);
        build(&mut composer, Arc::new(items));

        assert_eq!(state.offset(), 0.0, "the viewport stays at the top, so the new row is visible");
        let offsets: Vec<(u64, f32)> = composer
            .arena_nodes()
            .iter()
            .filter_map(|n| n.item_paint.as_ref().map(|p| (p.key, p.offset.peek().y)))
            .collect();
        assert!(
            offsets.iter().any(|(key, y)| *key != 99 && *y < -10.0),
            "the rows below start at their old place and spring down: {offsets:?}"
        );
        crate::animation::clear_all_animations();
    }
    /// The same case one row above the bottom, with the demo's geometry: the anchor correction's
    /// build-time estimate of the inserted row's height is off by the row spacing, so a shift derived
    /// from it left every row 8 dp out (drawn 8 dp lower, then springing back). Rebasing by the
    /// anchor's ACTUAL delta fixes it.
    #[test]
    fn an_insert_at_the_head_near_the_bottom_does_not_flash_the_rows() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let state = LazyListState::new();
        let mut composer = Composer::new();
        let mut build = |composer: &mut Composer, items: Arc<Vec<u64>>| {
            let s = state.clone();
            composer.compose(crate::compose!(|ctx| {
                LazyColumn::new()
                    .state(s)
                    .modifier(Modifier::new().fill_max_width().height(483.0))
                    .spacing(8.0)
                    .content_padding(4.0, 4.0)
                    .beyond_bounds(2)
                    .items_from(items, |k| *k, |ctx, _i, k| {
                        crate::layout::Column::new()
                            .modifier(Modifier::new().animate_item().fill_max_width().height(56.0))
                            .build(ctx, |ctx| {
                                crate::components::text::Text::new(format!("Item {k}")).build(ctx);
                            });
                    })
                    .build(ctx);
            }));
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 483.0));
        };

        build(&mut composer, Arc::new((0..19).collect()));
        // One row above the very bottom (the demo's reading position when it reported the flash).
        state.offset.set(733.0);
        build(&mut composer, Arc::new((0..19).collect()));
        build(&mut composer, Arc::new((0..19).collect()));
        let mut items: Vec<u64> = vec![99];
        items.extend(0..19);
        build(&mut composer, Arc::new(items));

        let offsets: Vec<(u64, f32)> = composer
            .arena_nodes()
            .iter()
            .filter_map(|n| n.item_paint.as_ref().map(|p| (p.key, p.offset.peek().y)))
            .collect();
        assert!(offsets.len() >= 5, "the rows carry paint channels: {offsets:?}");
        assert!(
            offsets.iter().all(|(_, y)| y.abs() < 0.5),
            "no row may flash when the whole list only scrolled: {offsets:?}"
        );
        crate::animation::clear_all_animations();
    }
    /// The APP composes until stable and only then lays out (`recompose_until_stable` in
    /// `app/per_window/core.rs`), so a correction that writes the offset produces a second compose
    /// pass before the layout that consumes the anchor hand-off. Clearing that hand-off in `build`
    /// therefore lost it on the real path while every single-compose test still passed — this is the
    /// regression test for exactly that.
    #[test]
    fn an_insert_at_the_head_while_scrolled_survives_a_second_compose_pass() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let state = LazyListState::new();
        let mut composer = Composer::new();
        let scene = |state: &LazyListState, items: Arc<Vec<u64>>| {
            let s = state.clone();
            move |ctx: &mut crate::runtime::composer::ComposeCtx| {
                let s = s.clone();
                let items = items.clone();
                LazyColumn::new()
                    .state(s)
                    .modifier(Modifier::new().fill_max_width().height(300.0))
                    .spacing(8.0)
                    .content_padding(4.0, 4.0)
                    .items_from(items, |k| *k, |ctx, _i, k| {
                        crate::layout::Column::new()
                            .modifier(Modifier::new().animate_item().fill_max_width().height(48.0))
                            .build(ctx, |ctx| {
                                crate::components::text::Text::new(format!("Item {k}")).build(ctx);
                            });
                    })
                    .build(ctx);
            }
        };

        composer.compose(scene(&state, Arc::new((0..20).collect())));
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 300.0));
        state.scroll_to_item(3, 20.0);
        composer.compose(scene(&state, Arc::new((0..20).collect())));
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 300.0));

        // Insert at the head, and compose TWICE before laying out — what the app does.
        let mut items: Vec<u64> = vec![99];
        items.extend(0..20);
        let items = Arc::new(items);
        composer.compose(scene(&state, items.clone()));
        composer.compose(scene(&state, items));
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 300.0));

        let offsets: Vec<(u64, f32)> = composer
            .arena_nodes()
            .iter()
            .filter_map(|n| n.item_paint.as_ref().map(|p| (p.key, p.offset.peek().y)))
            .collect();
        // The rows really do have paint channels (so "the host did nothing" cannot pass this), and
        // none of them is out of place.
        assert!(offsets.len() >= 5, "the rows carry paint channels: {offsets:?}");
        assert!(offsets.len() >= 5, "the rows carry paint channels: {offsets:?}");
        assert!(
            offsets.iter().all(|(_, y)| y.abs() < 0.5),
            "no row may animate when the whole list only scrolled: {offsets:?}"
        );
        crate::animation::clear_all_animations();
    }

    /// The anchor row does not have to carry the marker: a list that marks most rows but not the one
    /// that happens to be first visible (a section header, say) has no remembered position for it, and
    /// the correction's own delta is used instead. Without that fallback the whole correction was
    /// silently dropped and the rows flashed.
    #[test]
    fn the_anchor_without_its_own_marker_still_rebases_the_rows() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let state = LazyListState::new();
        let mut composer = Composer::new();
        let mut build = |composer: &mut Composer, items: Arc<Vec<u64>>| {
            let s = state.clone();
            composer.compose(crate::compose!(|ctx| {
                LazyColumn::new()
                    .state(s)
                    .modifier(Modifier::new().fill_max_width().height(300.0))
                    .spacing(8.0)
                    .content_padding(4.0, 4.0)
                    .items_from(items, |k| *k, |ctx, _i, k| {
                        // Key 3 is the row the scroll below leaves first: no marker on purpose.
                        let modifier = if *k == 3 {
                            Modifier::new().fill_max_width().height(48.0)
                        } else {
                            Modifier::new().animate_item().fill_max_width().height(48.0)
                        };
                        crate::layout::Column::new().modifier(modifier).build(ctx, |ctx| {
                            crate::components::text::Text::new(format!("Item {k}")).build(ctx);
                        });
                    })
                    .build(ctx);
            }));
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 300.0));
        };

        build(&mut composer, Arc::new((0..20).collect()));
        state.scroll_to_item(3, 20.0);
        build(&mut composer, Arc::new((0..20).collect()));
        build(&mut composer, Arc::new((0..20).collect()));

        let mut items: Vec<u64> = vec![99];
        items.extend(0..20);
        build(&mut composer, Arc::new(items));

        let offsets: Vec<(u64, f32)> = composer
            .arena_nodes()
            .iter()
            .filter_map(|n| n.item_paint.as_ref().map(|p| (p.key, p.offset.peek().y)))
            .collect();
        assert!(offsets.len() >= 5, "the marked rows have paint channels: {offsets:?}");
        assert!(
            offsets.iter().all(|(_, y)| y.abs() < 0.5),
            "the fallback delta keeps the rows still: {offsets:?}"
        );
        crate::animation::clear_all_animations();
    }
    // ── Duplicate-key detection (Compose throws on a key conflict) ──
    #[test]
    #[should_panic(expected = "duplicate key")]
    fn duplicate_key_detected_in_rebuild() {
        let mut il = IntervalList::new();
        let c1: Arc<dyn Fn(&mut ComposeCtx, usize) + Send + Sync> = Arc::new(|_, _| {});
        let c2: Arc<dyn Fn(&mut ComposeCtx, usize) + Send + Sync> = Arc::new(|_, _| {});
        il.add(1, Some(Arc::new(|_| 42u64)), c1, false);
        il.add(1, Some(Arc::new(|_| 42u64)), c2, false);
        il.rebuild(); // must panic
    }

    // ── measure/build convergence (audit Phase 3.2): after the window grows, does `build`
    //    converge the visible range onto the REAL viewport? ──
    /// Render across several frames on ONE composer (the convergence of a list after a window resize).
    /// Each frame composes and lays out with `heights[i]`; the last frame is rendered and the return value
    /// says whether there is text within 60 px of the viewport bottom — i.e. whether `build` composed the
    /// items the bottom needs.
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
        // Look for text rows within 60 px of the viewport bottom: composed items leave text there.
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
        // The viewport grows from 400 to 2500 (past the old fixed 2000 estimate window). With the
        // cross-frame viewport signal in place, `build` sees the real extent and the bottom items are
        // composed after a few frames. Frame order: 400 (initial) → 2500 (resize) → 2500 (converged).
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
        eprintln!("bottom of the viewport covered after resizing to 2500 and converging: {covered}");
        // With the cross-frame viewport signal, the bottom items are composed once it converges.
        assert!(covered, "after a resize the enlarged viewport converges with its bottom items composed (covered={covered})");
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
    /// empty until the next frame (documented in `docs/architecture-audit.md` Phase 3.2, "cross-frame
    /// convergence").
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
        eprintln!("resize frame (400 -> 2500, no same-frame convergence), bottom covered: {without}");
        assert!(
            !without,
            "control: without same-frame convergence the resize frame must miss the new bottom \
             (covered={without})"
        );
        let covered = resize_frame_bottom_covered(true);
        eprintln!("resize frame (400 -> 2500, same-frame convergence), bottom covered: {covered}");
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
