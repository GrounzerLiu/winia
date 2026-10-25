    use super::*;

    #[test]
    fn geometry_hidden_when_content_fits() {
        // content <= viewport → None（M3：内容放得下不画）
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
        assert_eq!(off, 0.0, "scroll=0 时 thumb 在顶");
        // scroll 到底：offset = (1400/1400) * (596-178.8) = 417.2
        let (off2, _) = scrollbar_geometry(600.0, 2000.0, 1400.0, 24.0, 0.9, 2.0).unwrap();
        assert!((off2 - 417.2).abs() < 0.01, "off2={off2}");
        // scroll 超界 clamp
        let (off3, _) = scrollbar_geometry(600.0, 2000.0, 9999.0, 24.0, 0.9, 2.0).unwrap();
        assert!((off3 - 417.2).abs() < 0.01, "超界 clamp 到底");
    }

    #[test]
    fn geometry_thumb_clamped_min_max() {
        // 内容巨大 → thumb 按 min 钳
        let (_, len) = scrollbar_geometry(600.0, 100000.0, 0.0, 24.0, 0.9, 2.0).unwrap();
        assert_eq!(len, 24.0, "thumb 下限 min");
        // 内容略超 → thumb 按 max 钳（track*0.9）
        let (_, len2) = scrollbar_geometry(600.0, 610.0, 0.0, 24.0, 0.9, 2.0).unwrap();
        assert!((len2 - 596.0 * 0.9).abs() < 0.01, "thumb 上限 track*0.9，len2={len2}");
    }

    #[test]
    fn geometry_small_track_no_panic() {
        // P0-1 回归：track=26 < max(24/0.9)=26.67 时 max_thumb(23.4) < thumb_min(24)，
        // 旧 clamp(min>max) 直接 panic——现应返回 None（track 放不下最小 thumb 则不画）。
        assert!(scrollbar_geometry(30.0, 1000.0, 0.0, 24.0, 0.9, 2.0).is_none());
        // thumb_min > track 同样不画（已有 track < thumb_min 分支锁定）。
        assert!(scrollbar_geometry(30.0, 1000.0, 0.0, 30.0, 0.9, 2.0).is_none());
    }

    #[test]
    fn geometry_rejects_nonfinite() {
        // P2-2：NaN/Inf 输入不 panic、不产出 NaN 几何。
        assert!(scrollbar_geometry(f32::NAN, 1000.0, 0.0, 24.0, 0.9, 2.0).is_none());
        assert!(scrollbar_geometry(600.0, f32::INFINITY, 0.0, 24.0, 0.9, 2.0).is_none());
        assert!(scrollbar_geometry(600.0, 2000.0, f32::NAN, 24.0, 0.9, 2.0).is_none());
        assert_eq!(scrollbar_offset_for_thumb_pos(f32::NAN, 596.0, 178.8, 1400.0), 0.0);
        assert_eq!(scrollbar_offset_for_thumb_pos(100.0, f32::NAN, 178.8, 1400.0), 0.0);
    }

    #[test]
    fn offset_for_thumb_pos_degenerate_travel() {
        // travel<=0（track<=thumb）时不除零、不 NaN（.max(1.0) 保护锁定）。
        let v = scrollbar_offset_for_thumb_pos(10.0, 100.0, 100.0, 500.0);
        assert!(v.is_finite(), "v={v}");
        let v2 = scrollbar_offset_for_thumb_pos(10.0, 50.0, 100.0, 500.0);
        assert!(v2.is_finite(), "v2={v2}");
    }

    #[test]
    fn offset_for_thumb_pos_round_trip() {
        // thumb 拖到 track 中点 → scroll 中点
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
        // P1-1 回归：grab 保持（非中心对齐）。thumb_offset=100、thumb=178.8，
        // 按下点 pos=150（thumb 内，grab=50）后微移到 160：
        // grab 语义 target=160-50=110；旧中心对齐 target=160-89.4=70.6。
        // 断言走 grab 路径（差值应 <1px），锁定"跟手不跳"。
        let track = 596.0;
        let thumb = 178.8;
        let max = 1400.0;
        let thumb_offset = 100.0f32;
        let inset = 2.0f32;
        // 模拟 on_drag_start 的 grab 计算（含 clamp）：grab=(150-2-100)=48
        let grab = ((150.0 - inset - thumb_offset).clamp(0.0, thumb));
        assert!((grab - 48.0).abs() < 0.01, "grab={grab}");
        // 模拟 on_drag：pos=160 → target=160-2-48=110
        let target = (160.0 - inset) - grab;
        let off_grab = scrollbar_offset_for_thumb_pos(target, track, thumb, max);
        let off_center =
            scrollbar_offset_for_thumb_pos((160.0 - inset) - thumb / 2.0, track, thumb, max);
        assert!((off_grab - off_center).abs() > 50.0, "两语义应显著不同（grab={off_grab} center={off_center}）");
        // grab 路径增量 = (160-150)/travel*max ≈ 10/417.2*1400 ≈ 33.5
        let travel = track - thumb;
        assert!((off_grab - scrollbar_offset_for_thumb_pos(150.0 - inset - grab, track, thumb, max) - 10.0 / travel * max).abs() < 1.0);
        // 按下点在 thumb 外（track 空白处 pos=400）：grab 钳到 thumb（178.8），
        //  thumb 边缘跟到光标，不大跳到中心。
        let grab_clamped = (400.0 - inset - thumb_offset).clamp(0.0, thumb);
        assert!((grab_clamped - thumb).abs() < 0.01, "thumb 外按下应钳到边缘，grab={grab_clamped}");
    }

    #[test]
    fn reverse_visual_round_trip() {
        // P2-3 回归：reverse 下 visual = max - raw，来回镜像精确还原。
        // max=1400：raw=0 → visual=1400（条在起点，内容末端）；raw=1400 → visual=0。
        let max = 1400.0f32;
        let to_visual = |raw: f32| (max - raw).clamp(0.0, max.max(0.0));
        assert_eq!(to_visual(0.0), 1400.0);
        assert_eq!(to_visual(1400.0), 0.0);
        assert_eq!(to_visual(300.0), 1100.0);
        // 写回镜像：visual→raw 同公式（对合），拖拽往返不漂移。
        let back = |visual: f32| (max - visual).clamp(0.0, max.max(0.0));
        assert_eq!(back(to_visual(300.0)), 300.0);
        // 超界钳：raw 越界时 visual 钳到 [0, max]。
        assert_eq!(to_visual(-50.0), 1400.0);
        assert_eq!(to_visual(9999.0), 0.0);
    }

    #[test]
    fn scrollbar_node_key_covers_params() {
        use crate::modifier::DrawNode;
        let mk = |off: f32, len: f32| ScrollbarNode {
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
        assert_ne!(mk(0.0, 100.0).node_key(), mk(10.0, 100.0).node_key(), "offset 应进 key");
        assert_ne!(mk(0.0, 100.0).node_key(), mk(0.0, 120.0).node_key(), "len 应进 key");
        // fade 值不同但 key 相同（fade 不进 key——逐帧动画值，进则每帧 Enter；
        // 绘制由引擎 request_redraw 驱动，draw 期 peek 读最新值）。
        let mut a = mk(0.0, 100.0);
        a.fade = State::new(0.0);
        let b = mk(0.0, 100.0);
        assert_eq!(a.node_key(), b.node_key(), "fade 不应进 key");
    }

    /// 组件联动：Row 内滚动列 + scrollbar；offset.set 后 thumb key 跟随。
    /// viewport 靠 on_size_changed 回写（次帧），故跑两帧 compose+layout。
    /// LaunchedEffect（fade）需 tokio 上下文——测试内建 Runtime（对标 loading）。
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
        // 模拟布局回写：内容 1000 - 视口 400 = 极限 600
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
        // 找 scrollbar 叶的 node_key（含 "scrollbar:" 前缀）
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
        build(&mut composer); // 次帧：viewport 回写（400）生效
        let root = composer.layout_root_idx().expect("root");
        let key1 = scrollbar_key(composer.arena_nodes(), root)
            .expect("viewport 回写后应有 scrollbar 节点");
        // offset 0 → thumb 在顶；set(300) 后 key 变化（thumb 跟随）
        scroll.offset.set(300.0);
        build(&mut composer);
        let key2 = scrollbar_key(composer.arena_nodes(), root).expect("滚动后仍在");
        assert_ne!(key1, key2, "offset 变化后 thumb key 应跟随（key1={key1} key2={key2}）");
        assert!(key2.ends_with(":true"), "常显模式 key 末尾 has_thumb=true，key2={key2}");
    }

    /// Lazy 适配联动：LazyColumn + LazyScrollbar 共享 LazyListState；
    /// fling_limit 回写后 thumb 出现，offset.set 后 thumb key 跟随。
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
        // 模拟测量回写：内容 2000 - 视口 400 = 极限 1600
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
        build(&mut composer); // 次帧：viewport 回写生效
        let root = composer.layout_root_idx().expect("root");
        let key1 = scrollbar_key(composer.arena_nodes(), root)
            .expect("viewport 回写后应有 lazy scrollbar 节点");
        list_state.offset.set(500.0);
        build(&mut composer);
        let key2 = scrollbar_key(composer.arena_nodes(), root).expect("滚动后仍在");
        assert_ne!(key1, key2, "offset 变化后 thumb key 应跟随（key1={key1} key2={key2}）");
        assert!(key2.ends_with(":true"), "常显模式 key 末尾 has_thumb=true，key2={key2}");
    }

    /// T8（P1-1 锁定）：反向懒列表 + LazyScrollbar(reverse=Some(true))，
    /// offset=0 时 thumb 应在起点（与正向 offset=max 同 key）；不传 reverse
    /// 则反向（与正向 offset=0 同 key）——锁定"条跟内容走"。
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
        // 正向 build helper（与 build_lazy 同结构，不带 reverse）
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
        // reverse + 镜像：offset=0 → visual=max → thumb 在末端（offset 最大同位）
        let list_state = LazyListState::new();
        let mut composer = Composer::new();
        build_lazy(&mut composer, &theme, &list_state, true);
        build_lazy(&mut composer, &theme, &list_state, true);
        // 读回测量期真实回写的 limit（set(1600) 会被 measure 覆盖为真实值）
        let root = composer.layout_root_idx().expect("root");
        let real_limit = list_state.fling_limit.get();
        let key_rev0 = scrollbar_key(composer.arena_nodes(), root).expect("reverse 条应存在");
        // 正向 offset=limit → thumb 在末端：与 reverse offset=0 同 key
        // （visual 相同 → 几何相同；fade 状态差异不影响 key——fade 不进 key）。
        let list_state2 = LazyListState::new();
        let mut composer2 = Composer::new();
        // 先跑两帧让测量回写 limit，再设 offset=limit（与 reverse 侧同 max）
        build_fwd(&mut composer2, &theme, &list_state2);
        build_fwd(&mut composer2, &theme, &list_state2);
        list_state2.offset.set(list_state2.fling_limit.get());
        build_fwd(&mut composer2, &theme, &list_state2);
        let root2 = composer2.layout_root_idx().expect("root");
        let key_fwd_max = scrollbar_key(composer2.arena_nodes(), root2).expect("正向条应存在");
        assert_eq!(key_rev0, key_fwd_max, "reverse offset=0 应与正向 offset=max 同几何");
        // 不传 reverse：offset=0 → thumb 在顶（与正向 offset=0 同位，反向错误）。
        let list_state3 = LazyListState::new();
        let mut composer3 = Composer::new();
        build_lazy(&mut composer3, &theme, &list_state3, false);
        build_lazy(&mut composer3, &theme, &list_state3, false);
        let root3 = composer3.layout_root_idx().expect("root");
        let key_nobar = scrollbar_key(composer3.arena_nodes(), root3).expect("条应存在");
        assert_ne!(key_nobar, key_rev0, "不传 reverse 应与镜像位不同（反向错误复现）");
    }

    /// 水平懒条联动：LazyRow + HorizontalLazyScrollbar 共享 LazyListState；
    /// fling_limit 回写后 thumb 出现，offset.set 后 thumb key 跟随。
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
        build(&mut composer); // 次帧：viewport 回写生效
        let root = composer.layout_root_idx().expect("root");
        let key1 = scrollbar_key(composer.arena_nodes(), root)
            .expect("viewport 回写后应有水平懒条节点");
        assert!(key1.starts_with("draw:scrollbar:false:"), "水平条 vertical=false，key1={key1}");
        list_state.offset.set(300.0);
        build(&mut composer);
        let key2 = scrollbar_key(composer.arena_nodes(), root).expect("滚动后仍在");
        assert_ne!(key1, key2, "offset 变化后 thumb key 应跟随（key1={key1} key2={key2}）");
        assert!(key2.ends_with(":true"), "常显模式 key 末尾 has_thumb=true，key2={key2}");
    }

    /// P1-1（第三轮 review 锁定）：反向 LazyRow + HorizontalLazyScrollbar
    /// (reverse=Some(true))，offset=0 时 thumb 应在末端（与正向 offset=max
    /// 同 key）；不传 reverse 则反向。先验框架侧：`node.rs` 水平分支须认领
    /// `is_lazy_scroll_reverse`（否则 render 无镜像，条与内容反走）。
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
        // 框架侧先验：反向 LazyRow 节点须标记 scroll_reverse（render 镜像前提）。
        let probe_state = LazyListState::new();
        let mut probe = Composer::new();
        build_hlazy(&mut probe, &theme, &probe_state, true, true);
        build_hlazy(&mut probe, &theme, &probe_state, true, true);
        let probe_root = probe.layout_root_idx().expect("root");
        let rev_flag = probe.arena_nodes().iter().any(|n| {
            n.modifier.horizontal_scroll_state().is_some() && n.scroll_reverse
        });
        assert!(rev_flag, "反向 LazyRow 应标记 scroll_reverse（node.rs 水平分支）");
        // reverse + 镜像：offset=0 → visual=max → thumb 在末端。
        let list_state = LazyListState::new();
        let mut composer = Composer::new();
        build_hlazy(&mut composer, &theme, &list_state, true, true);
        build_hlazy(&mut composer, &theme, &list_state, true, true);
        let root = composer.layout_root_idx().expect("root");
        let key_rev0 = scrollbar_key(composer.arena_nodes(), root).expect("reverse 条应存在");
        // 正向 offset=max → thumb 在末端：与 reverse offset=0 同 key。
        let list_state2 = LazyListState::new();
        let mut composer2 = Composer::new();
        build_hlazy(&mut composer2, &theme, &list_state2, false, false);
        build_hlazy(&mut composer2, &theme, &list_state2, false, false);
        list_state2.offset.set(list_state2.fling_limit.get());
        build_hlazy(&mut composer2, &theme, &list_state2, false, false);
        let root2 = composer2.layout_root_idx().expect("root");
        let key_fwd_max = scrollbar_key(composer2.arena_nodes(), root2).expect("正向条应存在");
        assert_eq!(key_rev0, key_fwd_max, "reverse offset=0 应与正向 offset=max 同几何");
        // 不传 reverse：offset=0 → thumb 在顶（反向错误复现）。
        let list_state3 = LazyListState::new();
        let mut composer3 = Composer::new();
        build_hlazy(&mut composer3, &theme, &list_state3, true, false);
        build_hlazy(&mut composer3, &theme, &list_state3, true, false);
        let root3 = composer3.layout_root_idx().expect("root");
        let key_nobar = scrollbar_key(composer3.arena_nodes(), root3).expect("条应存在");
        assert_ne!(key_nobar, key_rev0, "不传 reverse 应与镜像位不同（反向错误复现）");
    }
