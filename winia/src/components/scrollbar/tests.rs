    use super::*;

    #[test]
    fn geometry_hidden_when_content_fits() {
        // content <= viewport -> None (M3: nothing is drawn when the content fits)
        assert!(scrollbar_geometry(600.0, 600.0, 0.0, 24.0, 0.9, 2.0).is_none());
        assert!(scrollbar_geometry(600.0, 400.0, 0.0, 24.0, 0.9, 2.0).is_none());
        assert!(scrollbar_geometry(0.0, 1000.0, 0.0, 24.0, 0.9, 2.0).is_none());
    }

    #[test]
    fn geometry_thumb_proportional() {
        // viewport 600 / content 2000 → thumb = track * 0.3
        // track = 600-4 = 596；thumb = 596*0.3 = 178.8（< max 596*0.9）
        let (off, len) = scrollbar_geometry(600.0, 2000.0, 0.0, 24.0, 0.9, 2.0).unwrap();
        assert!((len - 178.8).abs() < 0.01, "thumb={len}");
        assert_eq!(off, 0.0, "the thumb sits at the top when scroll=0");
        // scrolled to the end: offset = (1400/1400) * (596-178.8) = 417.2
        let (off2, _) = scrollbar_geometry(600.0, 2000.0, 1400.0, 24.0, 0.9, 2.0).unwrap();
        assert!((off2 - 417.2).abs() < 0.01, "off2={off2}");
        // an out-of-range scroll clamps
        let (off3, _) = scrollbar_geometry(600.0, 2000.0, 9999.0, 24.0, 0.9, 2.0).unwrap();
        assert!((off3 - 417.2).abs() < 0.01, "an out-of-range scroll clamps to the end");
    }

    #[test]
    fn geometry_thumb_clamped_min_max() {
        // huge content -> the thumb clamps to the minimum
        let (_, len) = scrollbar_geometry(600.0, 100000.0, 0.0, 24.0, 0.9, 2.0).unwrap();
        assert_eq!(len, 24.0, "the thumb takes the minimum length");
        // content slightly over -> the thumb clamps to the maximum (track*0.9)
        let (_, len2) = scrollbar_geometry(600.0, 610.0, 0.0, 24.0, 0.9, 2.0).unwrap();
        assert!((len2 - 596.0 * 0.9).abs() < 0.01, "the thumb takes the maximum, track*0.9, len2={len2}");
    }

    #[test]
    fn geometry_small_track_no_panic() {
        // P0-1 regression: with track=26 < max(24/0.9)=26.67, max_thumb(23.4) < thumb_min(24),
        // where the old clamp(min>max) panicked - it must return None (no thumb fits the track).
        assert!(scrollbar_geometry(30.0, 1000.0, 0.0, 24.0, 0.9, 2.0).is_none());
        // thumb_min > track draws nothing either (the track < thumb_min branch already pins that).
        assert!(scrollbar_geometry(30.0, 1000.0, 0.0, 30.0, 0.9, 2.0).is_none());
    }

    #[test]
    fn geometry_rejects_nonfinite() {
        // P2-2: NaN/Inf input does not panic and produces no NaN geometry.
        assert!(scrollbar_geometry(f32::NAN, 1000.0, 0.0, 24.0, 0.9, 2.0).is_none());
        assert!(scrollbar_geometry(600.0, f32::INFINITY, 0.0, 24.0, 0.9, 2.0).is_none());
        assert!(scrollbar_geometry(600.0, 2000.0, f32::NAN, 24.0, 0.9, 2.0).is_none());
        assert_eq!(scrollbar_offset_for_thumb_pos(f32::NAN, 596.0, 178.8, 1400.0), 0.0);
        assert_eq!(scrollbar_offset_for_thumb_pos(100.0, f32::NAN, 178.8, 1400.0), 0.0);
    }

    #[test]
    fn offset_for_thumb_pos_degenerate_travel() {
        // travel<=0 (track<=thumb) divides by nothing and produces no NaN (the .max(1.0) guard).
        let v = scrollbar_offset_for_thumb_pos(10.0, 100.0, 100.0, 500.0);
        assert!(v.is_finite(), "v={v}");
        let v2 = scrollbar_offset_for_thumb_pos(10.0, 50.0, 100.0, 500.0);
        assert!(v2.is_finite(), "v2={v2}");
    }

    #[test]
    fn offset_for_thumb_pos_round_trip() {
        // dragging the thumb to the middle of the track -> the middle of the scroll range
        let track = 596.0;
        let thumb = 178.8;
        let max = 1400.0;
        let mid = scrollbar_offset_for_thumb_pos((track - thumb) / 2.0, track, thumb, max);
        assert!((mid - 700.0).abs() < 1.0, "mid={mid}");
        assert_eq!(scrollbar_offset_for_thumb_pos(-10.0, track, thumb, max), 0.0);
        assert_eq!(scrollbar_offset_for_thumb_pos(9999.0, track, thumb, max), max);
    }

    #[test]
    fn drag_preserves_grab_offset() {
        // P1-1 regression: the grab offset is kept (no centre alignment). thumb_offset=100, thumb=178.8,
        // press at pos=150 (inside the thumb, grab=50), then move to 160:
        // grab semantics give target=160-50=110; the old centre alignment gave 160-89.4=70.6.
        // This asserts the grab path (within 1 px), pinning "the thumb follows the pointer without jumping".
        let track = 596.0;
        let thumb = 178.8;
        let max = 1400.0;
        let thumb_offset = 100.0f32;
        let inset = 2.0f32;
        // Simulate the grab computation of on_drag_start (clamp included): grab=(150-2-100)=48
        let grab = ((150.0 - inset - thumb_offset).clamp(0.0, thumb));
        assert!((grab - 48.0).abs() < 0.01, "grab={grab}");
        // Simulate on_drag: pos=160 -> target=160-2-48=110
        let target = (160.0 - inset) - grab;
        let off_grab = scrollbar_offset_for_thumb_pos(target, track, thumb, max);
        let off_center =
            scrollbar_offset_for_thumb_pos((160.0 - inset) - thumb / 2.0, track, thumb, max);
        assert!((off_grab - off_center).abs() > 50.0, "the two semantics must differ clearly (grab={off_grab} center={off_center})");
        // The grab path advances by (160-150)/travel*max, about 10/417.2*1400 = 33.5
        let travel = track - thumb;
        assert!((off_grab - scrollbar_offset_for_thumb_pos(150.0 - inset - grab, track, thumb, max) - 10.0 / travel * max).abs() < 1.0);
        // Press outside the thumb (empty track, pos=400): grab clamps to the thumb (178.8),
        // so the thumb edge follows the pointer instead of jumping to its centre.
        let grab_clamped = (400.0 - inset - thumb_offset).clamp(0.0, thumb);
        assert!((grab_clamped - thumb).abs() < 0.01, "a press outside the thumb clamps to its edge, grab={grab_clamped}");
    }

    #[test]
    fn reverse_visual_round_trip() {
        // P2-3 regression: under reverse, visual = max - raw, and the mirror round-trips exactly.
        // max=1400: raw=0 -> visual=1400 (at the start, the end of the content); raw=1400 -> visual=0.
        let max = 1400.0f32;
        let to_visual = |raw: f32| (max - raw).clamp(0.0, max.max(0.0));
        assert_eq!(to_visual(0.0), 1400.0);
        assert_eq!(to_visual(1400.0), 0.0);
        assert_eq!(to_visual(300.0), 1100.0);
        // Write-back mirrors with the same formula (an involution), so a drag round-trip does not drift.
        let back = |visual: f32| (max - visual).clamp(0.0, max.max(0.0));
        assert_eq!(back(to_visual(300.0)), 300.0);
        // Out-of-range clamp: a raw value outside [0, max] clamps into it.
        assert_eq!(to_visual(-50.0), 1400.0);
        assert_eq!(to_visual(9999.0), 0.0);
    }

    #[test]
    fn scrollbar_node_key_covers_params() {
        use crate::modifier::DrawNode;
        let mk = |off: f32, len: f32| ScrollbarNode {
            thickness_now: 6.0,
            vertical: true,
            thumb_color: Color::from_argb(255, 1, 2, 3),
            track_color: Color::TRANSPARENT,
            thickness: 6.0,
            inset: 2.0,
            thumb_offset: off,
            thumb_len: len,
            has_thumb: true,
            fade: State::new(1.0),
        };
        assert_eq!(mk(0.0, 100.0).node_key(), mk(0.0, 100.0).node_key());
        assert_ne!(mk(0.0, 100.0).node_key(), mk(10.0, 100.0).node_key(), "the offset belongs in the key");
        assert_ne!(mk(0.0, 100.0).node_key(), mk(0.0, 120.0).node_key(), "the length belongs in the key");
        // A different fade value keeps the same key (fade is per-frame animation state; keying it would
        // re-enter every frame - the engine drives redraws and the paint pass peeks the latest value).
        let mut a = mk(0.0, 100.0);
        a.fade = State::new(0.0);
        let b = mk(0.0, 100.0);
        assert_eq!(a.node_key(), b.node_key(), "fade must not be part of the key");
    }

    /// End to end: a scrolling column and a scrollbar in a Row; the thumb key follows offset.set.
    /// The viewport is written back by on_size_changed (one frame later), so this runs two compose+layout passes.
    /// The fade LaunchedEffect needs a tokio context; the test builds a Runtime (as the loading tests do).
    #[test]
    fn vertical_scrollbar_follows_scroll_state() {
        use crate::runtime::composer::Composer;
        use crate::layout::Constraints;
        use crate::theme::{ThemeColors, WiniaTheme};
        use crate::modifier::ScrollState;
        let rt = tokio::runtime::Runtime::new().unwrap();
        let _guard = rt.enter();
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let scroll = ScrollState::new();
        // Simulate the layout write-back: content 1000 - viewport 400 = limit 600
        scroll.fling_limit.set(600.0);
        let mut composer = Composer::new();
        let build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    crate::layout::components::Row::new().build(ctx, |ctx| {
                        crate::layout::components::Column::new()
                            .modifier(
                                Modifier::new()
                                    .fill_max_height()
                                    .vertical_scroll(scroll.clone()),
                            )
                            .build(ctx, |ctx| {
                                let k = ctx.next_key();
                                ctx.start_leaf(k, Modifier::new().size(100.0, 1000.0));
                                ctx.end_node();
                            });
                        VerticalScrollbar::new(scroll.clone())
                            .always_show(true)
                            .build(ctx);
                    });
                });
            });
            composer.layout(Constraints::new(0.0, 200.0, 0.0, 400.0));
        };
        // Find the scrollbar leaf's node_key (it carries the "scrollbar:" prefix)
        fn scrollbar_key(nodes: &[crate::layout::LayoutNode], idx: usize) -> Option<String> {
            let m = &nodes[idx].modifier;
            for n in m.modifier_nodes() {
                let key = crate::modifier::node_key_of(n);
                if key.starts_with("draw:scrollbar:") {
                    return Some(key);
                }
            }
            for &c in &nodes[idx].children {
                if let Some(k) = scrollbar_key(nodes, c) { return Some(k); }
            }
            None
        }
        build(&mut composer);
        build(&mut composer); // second frame: the viewport write-back (400) is in effect
        let root = composer.layout_root_idx().expect("root");
        let key1 = scrollbar_key(composer.arena_nodes(), root)
            .expect("a scrollbar node must exist after the viewport write-back");
        // offset 0 -> the thumb is at the top; after set(300) the key changes (the thumb follows)
        scroll.offset.set(300.0);
        build(&mut composer);
        let key2 = scrollbar_key(composer.arena_nodes(), root).expect("still present after scrolling");
        assert_ne!(key1, key2, "the thumb key must follow an offset change (key1={key1} key2={key2})");
        assert!(key2.ends_with(":true"), "always_show ends the key with has_thumb=true, key2={key2}");
    }

    /// The lazy adapter end to end: a LazyColumn and LazyScrollbar sharing a LazyListState;
    /// the thumb appears once fling_limit is written back, and its key follows offset.set.
    #[test]
    fn lazy_scrollbar_follows_lazy_list_state() {
        use crate::runtime::composer::Composer;
        use crate::layout::Constraints;
        use crate::layout::lazy::LazyListState;
        use crate::theme::{ThemeColors, WiniaTheme};
        let rt = tokio::runtime::Runtime::new().unwrap();
        let _guard = rt.enter();
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let list_state = LazyListState::new();
        // Simulate the measure write-back: content 2000 - viewport 400 = limit 1600
        list_state.fling_limit.set(1600.0);
        let mut composer = Composer::new();
        let build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    crate::layout::components::Row::new().build(ctx, |ctx| {
                        crate::layout::lazy::LazyColumn::new()
                            .state(list_state.clone())
                            .modifier(Modifier::new().fill_max_height().layout_weight(1.0))
                            .items_plain(30, |ctx, _i| {
                                let k = ctx.next_key();
                                ctx.start_leaf(k, Modifier::new().size(100.0, 60.0));
                                ctx.end_node();
                            })
                            .build(ctx);
                        LazyScrollbar::new(list_state.clone())
                            .always_show(true)
                            .build(ctx);
                    });
                });
            });
            composer.layout(Constraints::new(0.0, 200.0, 0.0, 400.0));
        };
        fn scrollbar_key(nodes: &[crate::layout::LayoutNode], idx: usize) -> Option<String> {
            let m = &nodes[idx].modifier;
            for n in m.modifier_nodes() {
                let key = crate::modifier::node_key_of(n);
                if key.starts_with("draw:scrollbar:") {
                    return Some(key);
                }
            }
            for &c in &nodes[idx].children {
                if let Some(k) = scrollbar_key(nodes, c) { return Some(k); }
            }
            None
        }
        build(&mut composer);
        build(&mut composer); // second frame: the viewport write-back is in effect
        let root = composer.layout_root_idx().expect("root");
        let key1 = scrollbar_key(composer.arena_nodes(), root)
            .expect("a lazy scrollbar node must exist after the viewport write-back");
        list_state.offset.set(500.0);
        build(&mut composer);
        let key2 = scrollbar_key(composer.arena_nodes(), root).expect("still present after scrolling");
        assert_ne!(key1, key2, "the thumb key must follow an offset change (key1={key1} key2={key2})");
        assert!(key2.ends_with(":true"), "always_show ends the key with has_thumb=true, key2={key2}");
    }

    /// T8 (P1-1 pin): a reversed lazy list with LazyScrollbar(reverse=Some(true));
    /// at offset=0 the thumb sits at the start (same key as a forward bar at offset=max); without
    /// reverse it mirrors (same key as forward offset=0) - the bar follows the content.
    #[test]
    fn lazy_reverse_mirror() {
        use crate::runtime::composer::Composer;
        use crate::layout::Constraints;
        use crate::layout::lazy::LazyListState;
        use crate::theme::{ThemeColors, WiniaTheme};
        let rt = tokio::runtime::Runtime::new().unwrap();
        let _guard = rt.enter();
        let theme = ThemeColors::light_from_seed(0x6750A4);
        fn build_lazy(
            composer: &mut Composer,
            theme: &ThemeColors,
            list_state: &LazyListState,
            reverse_bar: bool,
        ) {
            composer.compose(|ctx| {
                WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    crate::layout::components::Row::new().build(ctx, |ctx| {
                        crate::layout::lazy::LazyColumn::new()
                            .state(list_state.clone())
                            .reverse_layout(true)
                            .modifier(Modifier::new().fill_max_height().layout_weight(1.0))
                            .items_plain(30, |ctx, _i| {
                                let k = ctx.next_key();
                                ctx.start_leaf(k, Modifier::new().size(100.0, 60.0));
                                ctx.end_node();
                            })
                            .build(ctx);
                        let bar = LazyScrollbar::new(list_state.clone()).always_show(true);
                        let bar = if reverse_bar {
                            bar.scroll_reverse(Some(true))
                        } else {
                            bar
                        };
                        bar.build(ctx);
                    });
                });
            });
            composer.layout(Constraints::new(0.0, 200.0, 0.0, 400.0));
        }
        fn scrollbar_key(nodes: &[crate::layout::LayoutNode], idx: usize) -> Option<String> {
            let m = &nodes[idx].modifier;
            for n in m.modifier_nodes() {
                let key = crate::modifier::node_key_of(n);
                if key.starts_with("draw:scrollbar:") {
                    return Some(key);
                }
            }
            for &c in &nodes[idx].children {
                if let Some(k) = scrollbar_key(nodes, c) { return Some(k); }
            }
            None
        }
        // The forward build helper (same shape as build_lazy, without reverse)
        fn build_fwd(
            composer: &mut Composer,
            theme: &ThemeColors,
            list_state: &LazyListState,
        ) {
            composer.compose(|ctx| {
                WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    crate::layout::components::Row::new().build(ctx, |ctx| {
                        crate::layout::lazy::LazyColumn::new()
                            .state(list_state.clone())
                            .modifier(Modifier::new().fill_max_height().layout_weight(1.0))
                            .items_plain(30, |ctx, _i| {
                                let k = ctx.next_key();
                                ctx.start_leaf(k, Modifier::new().size(100.0, 60.0));
                                ctx.end_node();
                            })
                            .build(ctx);
                        LazyScrollbar::new(list_state.clone())
                            .always_show(true)
                            .build(ctx);
                    });
                });
            });
            composer.layout(Constraints::new(0.0, 200.0, 0.0, 400.0));
        }
        // reverse + mirroring: offset=0 -> visual=max -> the thumb at the end (the same place as offset=max)
        let list_state = LazyListState::new();
        let mut composer = Composer::new();
        build_lazy(&mut composer, &theme, &list_state, true);
        build_lazy(&mut composer, &theme, &list_state, true);
        // Read back the limit the measure pass actually wrote (set(1600) is overwritten there)
        let root = composer.layout_root_idx().expect("root");
        let real_limit = list_state.fling_limit.get();
        let key_rev0 = scrollbar_key(composer.arena_nodes(), root).expect("the reversed bar must exist");
        // Forward offset=limit -> thumb at the end: the same key as reverse offset=0
        // (same visual -> same geometry; a different fade state does not matter - fade is not in the key).
        let list_state2 = LazyListState::new();
        let mut composer2 = Composer::new();
        // Run two frames so measure writes the limit, then set offset=limit (the same max as the reverse side)
        build_fwd(&mut composer2, &theme, &list_state2);
        build_fwd(&mut composer2, &theme, &list_state2);
        list_state2.offset.set(list_state2.fling_limit.get());
        build_fwd(&mut composer2, &theme, &list_state2);
        let root2 = composer2.layout_root_idx().expect("root");
        let key_fwd_max = scrollbar_key(composer2.arena_nodes(), root2).expect("the forward bar must exist");
        assert_eq!(key_rev0, key_fwd_max, "reverse offset=0 must have the same geometry as forward offset=max");
        // Without reverse: offset=0 -> thumb at the top (the forward position, i.e. the bug).
        let list_state3 = LazyListState::new();
        let mut composer3 = Composer::new();
        build_lazy(&mut composer3, &theme, &list_state3, false);
        build_lazy(&mut composer3, &theme, &list_state3, false);
        let root3 = composer3.layout_root_idx().expect("root");
        let key_nobar = scrollbar_key(composer3.arena_nodes(), root3).expect("the bar must exist");
        assert_ne!(key_nobar, key_rev0, "without reverse the bar must differ from the mirrored position (the bug reproduced)");
    }

    /// The horizontal lazy bar end to end: a LazyRow and HorizontalLazyScrollbar sharing a LazyListState;
    /// the thumb appears once fling_limit is written back, and its key follows offset.set.
    #[test]
    fn horizontal_lazy_scrollbar_follows_lazy_list_state() {
        use crate::runtime::composer::Composer;
        use crate::layout::Constraints;
        use crate::layout::lazy::LazyListState;
        use crate::theme::{ThemeColors, WiniaTheme};
        let rt = tokio::runtime::Runtime::new().unwrap();
        let _guard = rt.enter();
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let list_state = LazyListState::new();
        let mut composer = Composer::new();
        let build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    crate::layout::components::Column::new().build(ctx, |ctx| {
                        crate::layout::lazy::LazyRow::new()
                            .state(list_state.clone())
                            .modifier(Modifier::new().fill_max_width().layout_weight(1.0))
                            .items_plain(20, |ctx, _i| {
                                let k = ctx.next_key();
                                ctx.start_leaf(k, Modifier::new().size(80.0, 60.0));
                                ctx.end_node();
                            })
                            .build(ctx);
                        HorizontalLazyScrollbar::new(list_state.clone())
                            .always_show(true)
                            .build(ctx);
                    });
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 200.0));
        };
        fn scrollbar_key(nodes: &[crate::layout::LayoutNode], idx: usize) -> Option<String> {
            let m = &nodes[idx].modifier;
            for n in m.modifier_nodes() {
                let key = crate::modifier::node_key_of(n);
                if key.starts_with("draw:scrollbar:") {
                    return Some(key);
                }
            }
            for &c in &nodes[idx].children {
                if let Some(k) = scrollbar_key(nodes, c) { return Some(k); }
            }
            None
        }
        build(&mut composer);
        build(&mut composer); // second frame: the viewport write-back is in effect
        let root = composer.layout_root_idx().expect("root");
        let key1 = scrollbar_key(composer.arena_nodes(), root)
            .expect("a horizontal lazy bar node must exist after the viewport write-back");
        assert!(key1.starts_with("draw:scrollbar:false:"), "a horizontal bar has vertical=false, key1={key1}");
        list_state.offset.set(300.0);
        build(&mut composer);
        let key2 = scrollbar_key(composer.arena_nodes(), root).expect("still present after scrolling");
        assert_ne!(key1, key2, "the thumb key must follow an offset change (key1={key1} key2={key2})");
        assert!(key2.ends_with(":true"), "always_show ends the key with has_thumb=true, key2={key2}");
    }

    /// P1-1 (third review pin): a reversed LazyRow with HorizontalLazyScrollbar
    /// (reverse=Some(true)); at offset=0 the thumb sits at the end (the same key as forward
    /// offset=max); without reverse it mirrors. The framework side first: the horizontal branch of
    /// `node.rs` must claim `is_lazy_scroll_reverse`, or the render has no mirror and the bar runs backwards.
    #[test]
    fn horizontal_lazy_reverse_mirror() {
        use crate::runtime::composer::Composer;
        use crate::layout::Constraints;
        use crate::layout::lazy::LazyListState;
        use crate::theme::{ThemeColors, WiniaTheme};
        let rt = tokio::runtime::Runtime::new().unwrap();
        let _guard = rt.enter();
        let theme = ThemeColors::light_from_seed(0x6750A4);
        #[allow(clippy::too_many_arguments)]
        fn build_hlazy(
            composer: &mut Composer,
            theme: &ThemeColors,
            list_state: &LazyListState,
            reverse_list: bool,
            reverse_bar: bool,
        ) {
            composer.compose(|ctx| {
                WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    crate::layout::components::Column::new().build(ctx, |ctx| {
                        let mut row = crate::layout::lazy::LazyRow::new()
                            .state(list_state.clone());
                        if reverse_list {
                            row = row.reverse_layout(true);
                        }
                        row.modifier(Modifier::new().fill_max_width().layout_weight(1.0))
                            .items_plain(20, |ctx, _i| {
                                let k = ctx.next_key();
                                ctx.start_leaf(k, Modifier::new().size(80.0, 60.0));
                                ctx.end_node();
                            })
                            .build(ctx);
                        let bar = HorizontalLazyScrollbar::new(list_state.clone())
                            .always_show(true);
                        let bar = if reverse_bar {
                            bar.scroll_reverse(Some(true))
                        } else {
                            bar
                        };
                        bar.build(ctx);
                    });
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 200.0));
        }
        fn scrollbar_key(nodes: &[crate::layout::LayoutNode], idx: usize) -> Option<String> {
            let m = &nodes[idx].modifier;
            for n in m.modifier_nodes() {
                let key = crate::modifier::node_key_of(n);
                if key.starts_with("draw:scrollbar:") {
                    return Some(key);
                }
            }
            for &c in &nodes[idx].children {
                if let Some(k) = scrollbar_key(nodes, c) { return Some(k); }
            }
            None
        }
        // The framework side first: a reversed LazyRow node must carry scroll_reverse (the render mirror needs it).
        let probe_state = LazyListState::new();
        let mut probe = Composer::new();
        build_hlazy(&mut probe, &theme, &probe_state, true, true);
        build_hlazy(&mut probe, &theme, &probe_state, true, true);
        let probe_root = probe.layout_root_idx().expect("root");
        let rev_flag = probe.arena_nodes().iter().any(|n| {
            n.modifier.horizontal_scroll_state().is_some() && n.scroll_reverse
        });
        assert!(rev_flag, "a reversed LazyRow must carry scroll_reverse (the horizontal branch of node.rs)");
        // reverse + mirroring: offset=0 -> visual=max -> the thumb at the end.
        let list_state = LazyListState::new();
        let mut composer = Composer::new();
        build_hlazy(&mut composer, &theme, &list_state, true, true);
        build_hlazy(&mut composer, &theme, &list_state, true, true);
        let root = composer.layout_root_idx().expect("root");
        let key_rev0 = scrollbar_key(composer.arena_nodes(), root).expect("the reversed bar must exist");
        // Forward offset=max -> thumb at the end: the same key as reverse offset=0.
        let list_state2 = LazyListState::new();
        let mut composer2 = Composer::new();
        build_hlazy(&mut composer2, &theme, &list_state2, false, false);
        build_hlazy(&mut composer2, &theme, &list_state2, false, false);
        list_state2.offset.set(list_state2.fling_limit.get());
        build_hlazy(&mut composer2, &theme, &list_state2, false, false);
        let root2 = composer2.layout_root_idx().expect("root");
        let key_fwd_max = scrollbar_key(composer2.arena_nodes(), root2).expect("the forward bar must exist");
        assert_eq!(key_rev0, key_fwd_max, "reverse offset=0 must have the same geometry as forward offset=max");
        // Without reverse: offset=0 -> thumb at the top (the bug reproduced).
        let list_state3 = LazyListState::new();
        let mut composer3 = Composer::new();
        build_hlazy(&mut composer3, &theme, &list_state3, true, false);
        build_hlazy(&mut composer3, &theme, &list_state3, true, false);
        let root3 = composer3.layout_root_idx().expect("root");
        let key_nobar = scrollbar_key(composer3.arena_nodes(), root3).expect("the bar must exist");
        assert_ne!(key_nobar, key_rev0, "without reverse the bar must differ from the mirrored position (the bug reproduced)");
    }

    /// Where a row puts the bar: at the END edge, which mirrors with the layout direction. The bar
    /// itself is direction-neutral (its inset is symmetric), so the placement is the row's job - this
    /// pins that a caller who puts it LAST gets the M3 position in both directions.
    #[test]
    fn a_bar_placed_last_sits_at_the_end_edge_in_both_directions() {
        // The bar's fade runs through a LaunchedEffect, which needs a runtime even in a test.
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let _guard = rt.enter();
        fn child_xs(rtl: bool) -> Vec<f32> {
            let mut composer = crate::runtime::composer::Composer::new();
            let scroll = crate::modifier::ScrollState::new();
            composer.compose(crate::compose!(|ctx| {
                let mut m = crate::modifier::Modifier::new().size(200.0, 100.0);
                if rtl {
                    m = m.layout_direction(crate::layout::LayoutDirection::Rtl);
                }
                crate::layout::Row::new().modifier(m).build(ctx, |ctx| {
                    crate::components::Text::new("content")
                        .modifier(crate::modifier::Modifier::new().width(100.0))
                        .build(ctx);
                    VerticalScrollbar::new(scroll.clone())
                        .modifier(crate::modifier::Modifier::new().width(10.0))
                        .build(ctx);
                });
            }));
            composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 100.0));
            let nodes = composer.arena_nodes();
            let root = composer.arena.root.unwrap();
            nodes[root].children.iter().map(|c| nodes[*c].position.x).collect()
        }

        let ltr = child_xs(false);
        let rtl = child_xs(true);
        assert!(ltr.len() >= 2, "a content child and a bar: {ltr:?}");
        assert!(rtl.len() >= 2, "{rtl:?}");
        assert!(ltr[1] > ltr[0], "LTR: the bar is after the content: {ltr:?}");
        assert!(rtl[1] < rtl[0], "RTL: it lands on the other side: {rtl:?}");
    }
    /// Drive the bar through its own callbacks (the pointer path only forwards to them).
    fn drag_callbacks(node: &crate::layout::node::LayoutNode) -> (Option<std::sync::Arc<dyn Fn((f32, f32)) + Send + Sync>>, Option<std::sync::Arc<dyn Fn((f32, f32), (f32, f32)) + Send + Sync>>) {
        let mut start = None;
        let mut mv = None;
        for el in node.modifier.elements() {
            match el {
                crate::modifier::ModifierElement::DragOnStart { cb } => start = Some(cb.clone()),
                crate::modifier::ModifierElement::DragOnMove { cb } => mv = Some(cb.clone()),
                _ => {},
            }
        }
        (start, mv)
    }

    /// The drag reads geometry LIVE. A build-time capture is stale whenever the bar was built before
    /// its viewport write-back (track 0, thumb 0, travel floored at 1 px): a 50 px move then jumped
    /// the list to its end (measured: 200 of 200), and a captured pre-measure limit is f32::MAX.
    /// With live values, a press 3 px into the thumb and a 50 px move give 50/64 of the range.
    #[test]
    fn a_thumb_drag_uses_the_live_geometry() {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let _guard = rt.enter();
        let scroll = crate::modifier::ScrollState::new();
        let s = scroll.clone();
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(crate::compose!(|ctx| {
            crate::layout::Row::new().modifier(crate::modifier::Modifier::new().size(106.0, 100.0)).build(ctx, |ctx| {
                crate::layout::Column::new()
                    .modifier(crate::modifier::Modifier::new().size(100.0, 100.0).vertical_scroll(s.clone()))
                    .build(ctx, |ctx| {
                        crate::components::Text::new("x").modifier(crate::modifier::Modifier::new().size(100.0, 300.0)).build(ctx);
                    });
                VerticalScrollbar::new(s.clone()).build(ctx);
            });
        }));
        composer.layout(crate::layout::Constraints::new(0.0, 106.0, 0.0, 100.0));
        scroll.fling_limit.set(200.0);
        let (start, mv) = {
            let nodes = composer.arena_nodes();
            let bar = nodes.iter().find(|n| n.modifier.elements().iter().any(|e| matches!(e, crate::modifier::ModifierElement::DragOnStart { .. }))).expect("the bar node");
            drag_callbacks(bar)
        };
        (start.expect("start"))((3.0, 5.0));
        (mv.expect("move"))((3.0, 55.0), (0.0, 50.0));
        assert!((scroll.offset.peek() - 156.25).abs() < 0.5, "50 of the 64 px travel across a 200 px range, got {}", scroll.offset.peek());
    }

    /// A bar with nothing to drag (the content fits) must not take over the gesture: the drag start
    /// used to cancel a running fling and mark the container as scrolling, and the move callback then
    /// did nothing - the flag stayed set until release.
    #[test]
    fn a_drag_start_without_a_thumb_leaves_the_container_alone() {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let _guard = rt.enter();
        let scroll = crate::modifier::ScrollState::new();
        scroll.fling_limit.set(0.0);
        let s = scroll.clone();
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(crate::compose!(|ctx| {
            crate::layout::Row::new().modifier(crate::modifier::Modifier::new().size(106.0, 100.0)).build(ctx, |ctx| {
                crate::layout::Column::new()
                    .modifier(crate::modifier::Modifier::new().size(100.0, 100.0).vertical_scroll(s.clone()))
                    .build(ctx, |ctx| {
                        crate::components::Text::new("x").modifier(crate::modifier::Modifier::new().size(100.0, 60.0)).build(ctx);
                    });
                VerticalScrollbar::new(s.clone()).build(ctx);
            });
        }));
        composer.layout(crate::layout::Constraints::new(0.0, 106.0, 0.0, 100.0));
        let (start, _mv) = {
            let nodes = composer.arena_nodes();
            let bar = nodes.iter().find(|n| n.modifier.elements().iter().any(|e| matches!(e, crate::modifier::ModifierElement::DragOnStart { .. }))).expect("the bar node");
            drag_callbacks(bar)
        };
        (start.expect("start"))((3.0, 50.0));
        assert!(!scroll.is_scroll_in_progress.peek(), "no thumb, so the gesture is not taken over");
    }

    /// WinUI/macOS grow the bar on hover (CMP only recolours it): the thumb's cross-axis size is
    /// chosen per interaction state and animated, while the layout width stays put - hovering must
    /// never reflow the content.
    #[test]
    fn the_thumb_thickness_follows_the_interaction() {
        assert_eq!(scrollbar_thumb_thickness(6.0, 10.0, false), 6.0, "at rest");
        assert_eq!(scrollbar_thumb_thickness(6.0, 10.0, true), 10.0, "hovered or dragged");
        assert_eq!(SCROLLBAR_HOVER_THICKNESS, 10.0, "the default hover thickness");
    }

    /// CMP's `ScrollbarStyle.hoverColor` / `unhoverColor`: the thumb colour is chosen per interaction
    /// state; `animate_color_as_state` smooths the change (300 ms), and the node only sees the result.
    #[test]
    fn the_thumb_tint_follows_the_interaction() {
        let base = crate::graphics::Color::from_argb(179, 120, 120, 120);
        let hover = crate::graphics::Color::from_argb(255, 120, 120, 120);
        assert_eq!(scrollbar_thumb_tint(base, hover, false), base, "at rest");
        assert_eq!(scrollbar_thumb_tint(base, hover, true), hover, "hovered or dragged");
        // The default hover colour is the same outline token at full alpha, so the bar darkens
        // instead of changing hue (a caller can still pass its own through the config).
        let theme = crate::theme::WiniaTheme::colors();
        let default_base = scrollbar_thumb_color(&theme);
        let default_hover = scrollbar_thumb_hover_color(&theme);
        assert_eq!((default_base.r, default_base.g, default_base.b), (default_hover.r, default_hover.g, default_hover.b));
        assert!(default_hover.a > default_base.a, "hovering raises the alpha: {} vs {}", default_hover.a, default_base.a);
    }
    /// Pressing the empty track centres the thumb under the pointer (WinUI/macOS), then the drag keeps
    /// that centre there. A press on the thumb keeps its grab offset instead, so nothing jumps.
    #[test]
    fn pressing_the_track_centres_the_thumb() {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let _guard = rt.enter();
        let scroll = crate::modifier::ScrollState::new();
        let s = scroll.clone();
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(crate::compose!(|ctx| {
            crate::layout::Row::new().modifier(crate::modifier::Modifier::new().size(106.0, 100.0)).build(ctx, |ctx| {
                crate::layout::Column::new()
                    .modifier(crate::modifier::Modifier::new().size(100.0, 100.0).vertical_scroll(s.clone()))
                    .build(ctx, |ctx| {
                        crate::components::Text::new("x").modifier(crate::modifier::Modifier::new().size(100.0, 300.0)).build(ctx);
                    });
                VerticalScrollbar::new(s.clone()).build(ctx);
            });
        }));
        composer.layout(crate::layout::Constraints::new(0.0, 106.0, 0.0, 100.0));
        scroll.fling_limit.set(200.0);
        let start = {
            let nodes = composer.arena_nodes();
            let bar = nodes.iter().find(|n| n.modifier.elements().iter().any(|e| matches!(e, crate::modifier::ModifierElement::DragOnStart { .. }))).expect("the bar node");
            drag_callbacks(bar).0.expect("start")
        };
        // Track is 96 px with a 32 px thumb: a press at 70 is below the thumb (0..32), so the thumb's
        // centre (16) moves to 70, i.e. its top to 54 of the 64 px travel: 54/64 of 200 = 168.75.
        start((3.0, 72.0));
        assert!((scroll.offset.peek() - 168.75).abs() < 0.5, "the thumb centred on the press, got {}", scroll.offset.peek());
    }
    /// The bar's drag IS the scroll axis, so it has to outrank a surrounding scroll container -
    /// without the marker the dominant-axis arbitration hands every vertical move to the page and the
    /// thumb never moves (measured in the demo: dragging the bar scrolled the page).
    #[test]
    fn the_bar_declares_that_its_drag_outranks_the_scroll() {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let _guard = rt.enter();
        let scroll = crate::modifier::ScrollState::new();
        let s = scroll.clone();
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(crate::compose!(|ctx| {
            crate::layout::Row::new().modifier(crate::modifier::Modifier::new().size(106.0, 100.0)).build(ctx, |ctx| {
                crate::layout::Column::new()
                    .modifier(crate::modifier::Modifier::new().size(100.0, 100.0).vertical_scroll(s.clone()))
                    .build(ctx, |ctx| {
                        crate::components::Text::new("x").modifier(crate::modifier::Modifier::new().size(100.0, 300.0)).build(ctx);
                    });
                VerticalScrollbar::new(s.clone()).build(ctx);
            });
        }));
        composer.layout(crate::layout::Constraints::new(0.0, 106.0, 0.0, 100.0));
        let nodes = composer.arena_nodes();
        let bar = nodes.iter().find(|n| n.modifier.elements().iter().any(|e| matches!(e, crate::modifier::ModifierElement::DragOnStart { .. }))).expect("the bar node");
        assert!(bar.modifier.drag_preempts_scroll(), "the bar's drag outranks the page's scroll");
    }
    /// A plain CLICK on the empty track jumps at once. The jump used to hang off `on_drag_start`,
    /// which only fires after the touch slop - so a click did nothing and the user had to drag a few
    /// pixels first (reported).
    #[test]
    fn a_press_on_the_track_jumps_without_a_drag() {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let _guard = rt.enter();
        let scroll = crate::modifier::ScrollState::new();
        let s = scroll.clone();
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(crate::compose!(|ctx| {
            crate::layout::Row::new().modifier(crate::modifier::Modifier::new().size(106.0, 100.0)).build(ctx, |ctx| {
                crate::layout::Column::new()
                    .modifier(crate::modifier::Modifier::new().size(100.0, 100.0).vertical_scroll(s.clone()))
                    .build(ctx, |ctx| {
                        crate::components::Text::new("x").modifier(crate::modifier::Modifier::new().size(100.0, 300.0)).build(ctx);
                    });
                VerticalScrollbar::new(s.clone()).build(ctx);
            });
        }));
        composer.layout(crate::layout::Constraints::new(0.0, 106.0, 0.0, 100.0));
        scroll.fling_limit.set(200.0);
        let press = {
            let nodes = composer.arena_nodes();
            let bar = nodes.iter().find(|n| n.modifier.elements().iter().any(|e| matches!(e, crate::modifier::ModifierElement::TapOnPress { .. }))).expect("the bar node");
            let mut cb = None;
            for el in bar.modifier.elements() {
                if let crate::modifier::ModifierElement::TapOnPress { cb: f } = el { cb = Some(f.clone()); }
            }
            cb.expect("a press callback")
        };
        // Same numbers as the drag test: a press at 70 (below the thumb) centres the thumb there.
        press((3.0, 72.0));
        assert!((scroll.offset.peek() - 168.75).abs() < 0.5, "the click alone jumped, got {}", scroll.offset.peek());
    }
    /// A fresh `ScrollState` reports `f32::MAX` as its limit. Using that as a real range turned a
    /// press on the track into an offset of ~2.6e38 and the content vanished (measured).
    #[test]
    fn a_press_before_the_first_measure_does_not_write_a_garbage_offset() {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let _guard = rt.enter();
        let scroll = crate::modifier::ScrollState::new();
        assert!(scroll.fling_limit.peek() >= f32::MAX, "the sentinel this test guards against");
        let s = scroll.clone();
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(crate::compose!(|ctx| {
            crate::layout::Row::new().modifier(crate::modifier::Modifier::new().size(106.0, 100.0)).build(ctx, |ctx| {
                VerticalScrollbar::new(s.clone()).build(ctx);
            });
        }));
        composer.layout(crate::layout::Constraints::new(0.0, 106.0, 0.0, 100.0));
        let press = {
            let nodes = composer.arena_nodes();
            let bar = nodes.iter().find(|n| n.modifier.elements().iter().any(|e| matches!(e, crate::modifier::ModifierElement::TapOnPress { .. }))).expect("bar");
            let mut cb = None;
            for el in bar.modifier.elements() {
                if let crate::modifier::ModifierElement::TapOnPress { cb: f } = el { cb = Some(f.clone()); }
            }
            cb.expect("press")
        };
        press((3.0, 70.0));
        assert!(scroll.offset.peek() < 1.0e6, "the press wrote a garbage offset: {}", scroll.offset.peek());
    }

    /// Every appearance default lives in `ScrollbarDefaults` (docs/component-authoring.md section 2.2),
    /// and a bar built with `new()` really uses them.
    #[test]
    fn the_defaults_type_matches_the_token_constants_and_the_builder() {
        use crate::components::ScrollbarDefaults;
        assert_eq!(ScrollbarDefaults::thickness(), SCROLLBAR_THICKNESS);
        assert_eq!(ScrollbarDefaults::thumb_min_length(), SCROLLBAR_THUMB_MIN_LENGTH);
        assert_eq!(ScrollbarDefaults::hover_thickness(), SCROLLBAR_HOVER_THICKNESS);
        assert_eq!(ScrollbarDefaults::track_color(), crate::graphics::Color::TRANSPARENT);
        let theme = crate::theme::ThemeColors::dark_from_seed(0x6750A4);
        assert_eq!(ScrollbarDefaults::thumb_color(&theme), scrollbar_thumb_color(&theme));
        assert_eq!(ScrollbarDefaults::hover_color(&theme), scrollbar_thumb_hover_color(&theme));

        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let _guard = rt.enter();
        let scroll = crate::modifier::ScrollState::new();
        let s = scroll.clone();
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(crate::compose!(|ctx| {
            crate::layout::Row::new().modifier(crate::modifier::Modifier::new().size(106.0, 100.0)).build(ctx, |ctx| {
                VerticalScrollbar::new(s.clone()).build(ctx);
            });
        }));
        composer.layout(crate::layout::Constraints::new(0.0, 106.0, 0.0, 100.0));
        let nodes = composer.arena_nodes();
        let bar = nodes.iter().find(|n| n.modifier.elements().iter().any(|e| matches!(e, crate::modifier::ModifierElement::TapOnPress { .. }))).expect("the bar node");
        assert_eq!(bar.measured_size.width, ScrollbarDefaults::thickness(), "the builder uses the default width");
    }