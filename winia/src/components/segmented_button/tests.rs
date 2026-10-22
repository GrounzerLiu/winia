    use super::*;
    use crate::runtime::composer::Composer;
    /// The icon swap's crossfade runs on M3's `DefaultEffects` spring, as
    /// `SegmentedButton.kt:655` passes explicitly — not on the framework's `tween()` default.
    #[test]
    fn the_icon_crossfade_runs_on_m3s_default_effects_spring() {
        match segmented_icon_crossfade_spec() {
            crate::animation::AnimationSpec::Spring(spec) => {
                assert_eq!(spec, crate::animation::SpringSpec::effects_default());
                assert_eq!((spec.damping_ratio, spec.stiffness), (1.0, 1600.0));
            }
            other => panic!("the icon crossfade must be a spring, got {other:?}"),
        }
    }

    use crate::layout::LayoutDirection;
    use crate::components::text::Text;

    /// The slot offset slides on M3's `FastSpatial` spring (0.9/1400); winia's no-bouncy default fails.
    #[test]
    fn the_slot_offset_runs_on_m3s_fast_spatial_spring() {
        match segmented_slot_offset_spec() {
            crate::animation::AnimationSpec::Spring(spec) => {
                assert_eq!(spec, crate::animation::SpringSpec::spatial_fast());
                assert_eq!((spec.damping_ratio, spec.stiffness), (0.9, 1400.0));
            }
            other => panic!("the slot offset must be a spring, got {other:?}"),
        }
    }
    use crate::theme::ThemeColors;

    fn compose_row(count: usize, selected: usize, theme: &ThemeColors, dir: LayoutDirection) -> Composer {
        let mut composer = Composer::new();
        let theme = theme.clone();
        composer.compose(|ctx| {
            WiniaTheme::with_theme_and_direction(theme, dir, ctx, |ctx| {
                SingleChoiceSegmentedButtonRow::new().build(ctx, |ctx| {
                    for i in 0..count {
                        SegmentedButton::new(i == selected, move || {})
                            .shape(SegmentedButtonDefaults::item_shape(i, count))
                            .build(ctx, |ctx| {
                                Text::new(format!("Item {i}")).build(ctx);
                            });
                    }
                });
            });
        });
        composer
    }

    fn row_children(composer: &Composer) -> Vec<usize> {
        let root = composer.layout_root_idx().expect("root");
        composer.arena_nodes()[root].children.clone()
    }

    // ── Pure rules ──

    #[test]
    fn item_shape_follows_index_count_and_direction() {
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_theme_and_direction(
                ThemeColors::light_from_seed(0x6750A4),
                LayoutDirection::Ltr,
                ctx,
                |_ctx| {
                    assert_eq!(
                        SegmentedButtonDefaults::item_shape(0, 1),
                        Shape::Pill,
                        "a lone item is a stadium"
                    );
                    assert_eq!(
                        SegmentedButtonDefaults::item_shape(0, 3),
                        Shape::left_rounded(SegmentedButtonDefaults::OUTER_CORNER_RADIUS)
                    );
                    assert_eq!(
                        SegmentedButtonDefaults::item_shape(2, 3),
                        Shape::right_rounded(SegmentedButtonDefaults::OUTER_CORNER_RADIUS)
                    );
                    assert_eq!(SegmentedButtonDefaults::item_shape(1, 3), Shape::Rectangle);
                },
            );
        });
        // RTL rounds the other two corners, so a caller passes the same index/count either way.
        composer.compose(|ctx| {
            WiniaTheme::with_theme_and_direction(
                ThemeColors::light_from_seed(0x6750A4),
                LayoutDirection::Rtl,
                ctx,
                |_ctx| {
                    assert_eq!(
                        SegmentedButtonDefaults::item_shape(0, 3),
                        Shape::right_rounded(SegmentedButtonDefaults::OUTER_CORNER_RADIUS)
                    );
                    assert_eq!(
                        SegmentedButtonDefaults::item_shape(2, 3),
                        Shape::left_rounded(SegmentedButtonDefaults::OUTER_CORNER_RADIUS)
                    );
                },
            );
        });
    }

    #[test]
    fn colors_resolve_by_state() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let c = SegmentedButtonDefaults::colors(&theme);
        assert_eq!(c.container_color(true, true), theme.secondary_container);
        assert_eq!(c.container_color(true, false), Color::TRANSPARENT);
        assert_eq!(c.content_color(true, true), theme.on_secondary_container);
        assert_eq!(c.content_color(true, false), theme.on_surface);
        assert_eq!(c.border_color(true, true), theme.outline, "the active border is the outline too");
        assert_eq!(c.border_color(true, false), theme.outline);
        // Disabled keeps the active fill but dims content and border, as the tokens say.
        assert_eq!(c.container_color(false, true), theme.secondary_container);
        assert_eq!(c.content_color(false, true).a, 97, "onSurface @ 38%");
        assert_eq!(c.border_color(false, true).a, 31, "outline @ 12%");
        assert_eq!(c.container_color(false, false), Color::TRANSPARENT);
    }

    // ── Layout ──

    #[test]
    fn items_are_equal_width_and_share_their_borders() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = compose_row(3, 0, &theme, LayoutDirection::Ltr);
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
        let root = composer.layout_root_idx().expect("root");
        let items = row_children(&composer);
        let nodes = composer.arena_nodes();
        assert_eq!(items.len(), 3);
        let w = nodes[items[0]].measured_size.width;
        assert_eq!(nodes[root].measured_size.height, SegmentedButtonDefaults::HEIGHT);
        for (i, &it) in items.iter().enumerate() {
            assert_eq!(nodes[it].measured_size.width, w, "item {i} must match the widest item");
            assert!(w >= SegmentedButtonDefaults::MIN_WIDTH, "items keep the 58 px minimum");
            let expected = i as f32 * (w - SegmentedButtonDefaults::BORDER_WIDTH);
            assert!(
                (nodes[it].position.x - expected).abs() < 0.01,
                "item {i} at x={} but the overlap arithmetic says {expected}",
                nodes[it].position.x
            );
        }
        let total = 3.0 * w - 2.0 * SegmentedButtonDefaults::BORDER_WIDTH;
        assert!(
            (nodes[root].measured_size.width - total).abs() < 0.01,
            "the row is n × w − (n − 1) borders wide"
        );
    }

    /// Under RTL the strip is mirrored: item 0 sits at the RIGHT edge, which is the side
    /// [`SegmentedButtonDefaults::item_shape`] rounds for it. Without the mirror both rounded corners
    /// land on the shared inner edges and the outer edges come out square (reported from the demo).
    #[test]
    fn rtl_mirrors_the_strip_so_the_rounded_corners_stay_on_the_outer_edges() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = compose_row(3, 0, &theme, LayoutDirection::Rtl);
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
        let root = composer.layout_root_idx().expect("root");
        let items = row_children(&composer);
        let nodes = composer.arena_nodes();
        let w = nodes[items[0]].measured_size.width;
        let step = w - SegmentedButtonDefaults::BORDER_WIDTH;
        let total = nodes[root].measured_size.width;
        assert_eq!(items.len(), 3);

        // Item 0 is the FIRST composed and the LAST in x: mirrored, it ends at the row's right edge.
        // (The corners `item_shape` gives that item are covered by its own test; what matters here is
        // that the item carrying them ends up on the edge they round.)
        let first = &nodes[items[0]];
        assert!(
            (first.position.x - 2.0 * step).abs() < 0.01,
            "item 0 must sit at the right edge in RTL (x={}, expected {})",
            first.position.x,
            2.0 * step
        );
        assert!(
            (first.position.x + first.measured_size.width - total).abs() < 0.01,
            "item 0's right edge must land on the row's right edge"
        );
        // The last item mirrors to the left edge.
        let last = &nodes[items[2]];
        assert!(last.position.x.abs() < 0.01, "the last item sits at the left edge in RTL");
        // The middle never moves: mirroring swaps the ends, it does not reorder the sequence.
        assert!((nodes[items[1]].position.x - step).abs() < 0.01, "the middle item stays centred");
    }

    /// Equal width means the WIDEST item's width, not the parent's: a narrow row wraps (Compose's
    /// `IntrinsicSize.Min` row), and a parent too narrow to hold the strip shrinks the items instead of
    /// letting them overflow.
    #[test]
    fn the_row_wraps_to_the_widest_item_and_shrinks_when_it_must() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = Composer::new();
        let t = theme.clone();
        composer.compose(|ctx| {
            WiniaTheme::with_theme(t, ctx, |ctx| {
                SingleChoiceSegmentedButtonRow::new().build(ctx, |ctx| {
                    SegmentedButton::new(true, || {})
                        .shape(SegmentedButtonDefaults::item_shape(0, 2))
                        .build(ctx, |ctx| {
                            Text::new("A very long segment label").build(ctx);
                        });
                    SegmentedButton::new(false, || {})
                        .shape(SegmentedButtonDefaults::item_shape(1, 2))
                        .build(ctx, |ctx| {
                            Text::new("B").build(ctx);
                        });
                });
            });
        });
        composer.layout(Constraints::new(0.0, 600.0, 0.0, 300.0));
        let root = composer.layout_root_idx().expect("root");
        let items = row_children(&composer);
        let nodes = composer.arena_nodes();
        let wide = nodes[items[0]].measured_size.width;
        assert_eq!(nodes[items[1]].measured_size.width, wide, "the short item stretches to match");
        assert!(
            wide > 200.0 && nodes[root].measured_size.width < 600.0,
            "the row wraps its content (row {} wide, items {wide})",
            nodes[root].measured_size.width
        );

        // The same row in a 120 px parent: the items shrink to fit rather than overflowing.
        composer.layout(Constraints::new(0.0, 120.0, 0.0, 300.0));
        let nodes = composer.arena_nodes();
        let w = nodes[items[0]].measured_size.width;
        let total = 2.0 * w - SegmentedButtonDefaults::BORDER_WIDTH;
        assert!(total <= 120.01, "the strip fits the parent ({total} <= 120)");
    }

    #[test]
    fn a_checked_item_is_raised_above_its_neighbours() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = compose_row(3, 1, &theme, LayoutDirection::Ltr);
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
        let items = row_children(&composer);
        let nodes = composer.arena_nodes();
        assert_eq!(
            nodes[items[1]].modifier.get_z_index(),
            Some(SegmentedButtonDefaults::CHECKED_Z),
            "the checked item paints above the shared edges"
        );
        assert_eq!(
            nodes[items[0]].modifier.get_z_index(),
            None,
            "an idle unchecked item carries no z, so the renderer keeps its plain loop"
        );
    }

    /// The check's slot is always composed, invisible until the segment is active, and each arrival
    /// starts from 0 — the state of the enter animation as the composition leaves it.
    ///
    /// ⚠ What this canNOT pin, and why: an animation only advances when the app ticks it, and a headless
    /// test never does, so the progress never reaches 1.0 here and the reset on the active transition
    /// (`progress.set(0.0)`, which is what makes a SECOND pick animate too — reported from the demo) is
    /// not observable from here. That one was checked in the running demo instead.
    #[test]
    fn the_check_animates_in_every_time_the_selection_arrives() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let selected = Arc::new(AtomicBool::new(false));
        let mut composer = Composer::new();
        let sel = selected.clone();
        let t = theme.clone();
        let scene = move |ctx: &mut ComposeCtx| {
            let on = sel.load(Ordering::Relaxed);
            let s2 = sel.clone();
            WiniaTheme::with_theme(t.clone(), ctx, |ctx| {
                SingleChoiceSegmentedButtonRow::new().build(ctx, |ctx| {
                    SegmentedButton::new(on, move || s2.store(!on, Ordering::Relaxed))
                        .shape(SegmentedButtonDefaults::item_shape(0, 1))
                        .build(ctx, |ctx| {
                            Text::new("Only").build(ctx);
                        });
                });
            });
        };
        // The icon's alpha, read from the node that carries the enter animation.
        let icon_alpha = |composer: &Composer| -> Option<f32> {
            composer
                .arena_nodes()
                .iter()
                .find_map(|n| n.modifier.graphics_layer_params().map(|p| p.alpha))
        };

        composer.compose(scene.clone());
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 100.0));
        assert_eq!(
            icon_alpha(&composer),
            Some(0.0),
            "the icon slot is always composed, invisible until the segment is active"
        );

        // First arrival: the animation starts from 0 (it has not ticked yet).
        selected.store(true, Ordering::Relaxed);
        composer.recompose(scene.clone());
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 100.0));
        let first = icon_alpha(&composer).expect("an active segment composes its check icon");
        assert!(first < 0.5, "the check enters from 0, got alpha {first}");

        // Leave and arrive again — the second arrival must animate as well.
        selected.store(false, Ordering::Relaxed);
        composer.recompose(scene.clone());
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 100.0));
        selected.store(true, Ordering::Relaxed);
        composer.recompose(scene);
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 100.0));
        let again = icon_alpha(&composer).expect("the check is composed again");
        assert!(
            again < 0.5,
            "the second time a segment is picked its check must animate in too, got alpha {again}"
        );
    }

    /// Under RTL the icon and the label swap sides INSIDE the item: the check moves to the trailing
    /// (right) end and the label to the leading one. Compose's own content policy places both with
    /// absolute `place()` and does not mirror, so this is a deliberate difference — see
    /// [`SegmentedButtonContentPolicy`].
    #[test]
    fn rtl_swaps_the_icon_and_the_label_inside_the_item() {
        /// (icon x, label x) for one active item, measured in `dir`.
        fn content_x(dir: LayoutDirection) -> (f32, f32) {
            let theme = ThemeColors::light_from_seed(0x6750A4);
            let mut composer = Composer::new();
            composer.compose(|ctx| {
                WiniaTheme::with_theme_and_direction(theme.clone(), dir, ctx, |ctx| {
                    SingleChoiceSegmentedButtonRow::new().build(ctx, |ctx| {
                        SegmentedButton::new(true, || {})
                            .shape(SegmentedButtonDefaults::item_shape(0, 1))
                            .build(ctx, |ctx| {
                                Text::new("Only").build(ctx);
                            });
                    });
                });
            });
            composer.layout(Constraints::new(0.0, 200.0, 0.0, 100.0));
            let root = composer.layout_root_idx().expect("root");
            let nodes = composer.arena_nodes();
            let item = nodes[root].children[0];
            let icon = nodes[item].children[0];
            let label = nodes[item].children[1];
            (nodes[icon].position.x, nodes[label].position.x)
        }

        let (icon_ltr, label_ltr) = content_x(LayoutDirection::Ltr);
        assert!(
            icon_ltr < label_ltr,
            "LTR draws the check BEFORE the label (icon {icon_ltr}, label {label_ltr})"
        );
        let (icon_rtl, label_rtl) = content_x(LayoutDirection::Rtl);
        assert!(
            icon_rtl > label_rtl,
            "RTL draws the check AFTER the label (icon {icon_rtl}, label {label_rtl})"
        );
        // The block is centred, so the content keeps its distance from the item's edges: the check's
        // outer edge is `pad_h` from the item's leading edge in LTR and from its trailing edge in RTL.
        let item_w = {
            let theme = ThemeColors::light_from_seed(0x6750A4);
            let mut composer = Composer::new();
            composer.compose(|ctx| {
                WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    SingleChoiceSegmentedButtonRow::new().build(ctx, |ctx| {
                        SegmentedButton::new(true, || {})
                            .shape(SegmentedButtonDefaults::item_shape(0, 1))
                            .build(ctx, |ctx| {
                                Text::new("Only").build(ctx);
                            });
                    });
                });
            });
            composer.layout(Constraints::new(0.0, 200.0, 0.0, 100.0));
            let root = composer.layout_root_idx().expect("root");
            composer.arena_nodes()[composer.arena_nodes()[root].children[0]].measured_size.width
        };
        let icon_outer_ltr = icon_ltr;
        let icon_outer_rtl = item_w - (icon_rtl + SegmentedButtonDefaults::ICON_SIZE);
        assert!(
            (icon_outer_ltr - icon_outer_rtl).abs() < 1.0,
            "the check keeps its distance from the item's outer edge when mirrored \
             (ltr {icon_outer_ltr}, rtl {icon_outer_rtl}, item {item_w})"
        );
    }

    /// With an `inactive_icon` the slot is occupied in BOTH states — that is what makes the pair
    /// crossfade in place (Compose's `Crossfade` branch) and keeps the label where it is, where the
    /// default single check leaves the slot empty, invisible and the label centred.
    #[test]
    fn an_inactive_icon_keeps_the_slot_and_the_label_where_they_are() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = Composer::new();
        let t = theme.clone();
        composer.compose(|ctx| {
            WiniaTheme::with_theme(t, ctx, |ctx| {
                SingleChoiceSegmentedButtonRow::new().build(ctx, |ctx| {
                    SegmentedButton::new(false, || {})
                        .shape(SegmentedButtonDefaults::item_shape(0, 1))
                        .inactive_icon(|ctx| {
                            crate::components::icon::Icon::svg_path(CHECK_ICON_PATH)
                                .size(SegmentedButtonDefaults::ICON_SIZE)
                                .build(ctx);
                        })
                        .build(ctx, |ctx| {
                            Text::new("Only").build(ctx);
                        });
                });
            });
        });
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 100.0));
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        let item = nodes[root].children[0];
        let icon = nodes[item].children[0];
        assert_eq!(nodes[icon].children.len(), 1, "the icon slot holds a node even while the segment is inactive");
        let alpha = nodes[icon]
            .modifier
            .graphics_layer_params()
            .map(|p| p.alpha)
            .expect("the slot carries the animation layer");
        assert_eq!(alpha, 1.0, "an inactive icon of a crossfading pair is VISIBLE — the default single check is invisible (alpha 0) instead, so the pair keeps the slot occupied");
        // ⚠ The label's x is not asserted here: its offset animates from the centred start toward the
        // slot, and a headless test never ticks the animation, so it would still read the initial -13.
        // What matters structurally is above — the slot is occupied and visible while inactive.
    }

    /// The focus ring follows the THEME's color, and an interacting item sits above a checked
    /// neighbour. The second half is a deviation on purpose: Compose's `interactionZIndex` leaves an
    /// interacting unchecked item (1) below a checked one (5), because the M3 ring is drawn inside the
    /// item's bounds — ours is the framework's ring just outside the rect, and the checked neighbour
    /// covered its shared edge (reported from the demo).
    #[test]
    fn a_focused_item_uses_the_theme_focus_color_and_outranks_a_checked_neighbour() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let src = MutableInteractionSource::new();
        let mut composer = Composer::new();
        let (t, s) = (theme.clone(), src.clone());
        let scene = move |ctx: &mut ComposeCtx| {
            let (t, s) = (t.clone(), s.clone());
            WiniaTheme::with_theme(t, ctx, |ctx| {
                SingleChoiceSegmentedButtonRow::new().build(ctx, |ctx| {
                    SegmentedButton::new(true, || {})
                        .shape(SegmentedButtonDefaults::item_shape(0, 2))
                        .build(ctx, |ctx| {
                            Text::new("One").build(ctx);
                        });
                    SegmentedButton::new(false, || {})
                        .shape(SegmentedButtonDefaults::item_shape(1, 2))
                        .interaction_source(s)
                        .build(ctx, |ctx| {
                            Text::new("Two").build(ctx);
                        });
                });
            });
        };
        composer.compose(scene.clone());
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 100.0));
        let items = row_children(&composer);
        assert_eq!(
            composer.arena_nodes()[items[1]].focus_color.get(),
            theme.primary,
            "the ring takes the theme's focus color, not the framework default"
        );
        assert_eq!(
            composer.arena_nodes()[items[0]].modifier.get_z_index(),
            Some(SegmentedButtonDefaults::CHECKED_Z)
        );

        src.emit_focus();
        composer.recompose(scene);
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 100.0));
        let items = row_children(&composer);
        assert_eq!(
            composer.arena_nodes()[items[1]].modifier.get_z_index(),
            Some(SegmentedButtonDefaults::CHECKED_Z + SegmentedButtonDefaults::INTERACTING_Z),
            "a focused item outranks the checked neighbour so its ring is not cut off"
        );
    }

    // ── Pixels ──

    fn render_row(count: usize, selected: usize, theme: &ThemeColors) -> (Vec<[u8; 4]>, usize) {
        use skia_safe::surfaces;
        let mut composer = compose_row(count, selected, theme, LayoutDirection::Ltr);
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
        let mut surface = surfaces::raster_n32_premul((300, 60)).unwrap();
        surface.canvas().clear(skia_safe::Color::WHITE);
        let root = composer.layout_root_idx().expect("root");
        crate::render::render(composer.arena_nodes(), root, surface.canvas());
        let pm = surface.peek_pixels().expect("pixmap");
        (pm.pixels::<[u8; 4]>().expect("pixels").to_vec(), 300)
    }

    fn at(px: &[[u8; 4]], w: usize, x: f32, y: f32) -> (i32, i32, i32) {
        let p = px[(y as usize) * w + (x as usize)];
        (p[2] as i32, p[1] as i32, p[0] as i32)
    }

    fn close(a: (i32, i32, i32), b: (i32, i32, i32)) -> bool {
        (a.0 - b.0).abs() <= 8 && (a.1 - b.1).abs() <= 8 && (a.2 - b.2).abs() <= 8
    }

    #[test]
    fn the_checked_item_is_filled_and_the_rest_are_outlined() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let (px, w) = render_row(3, 1, &theme);
        let sec = (theme.secondary_container.r as i32, theme.secondary_container.g as i32, theme.secondary_container.b as i32);
        let outline = (theme.outline.r as i32, theme.outline.g as i32, theme.outline.b as i32);
        assert!(close(at(&px, w, 150.0, 20.0), sec), "the checked container is secondaryContainer (got {:?})", at(&px, w, 150.0, 20.0));
        // Below the label (the item is 40 px tall and the text is centred), inside the container.
        assert!(close(at(&px, w, 40.0, 35.0), (255, 255, 255)), "an unchecked container is transparent (got {:?})", at(&px, w, 40.0, 35.0));
        assert!(close(at(&px, w, 40.0, 0.0), outline), "…with a 1 px outline on the outer row (got {:?})", at(&px, w, 40.0, 0.0));
        // The strip's own edge carries the outline…
        assert!(close(at(&px, w, 0.0, 20.0), outline), "the strip's outer edge is the outline (got {:?})", at(&px, w, 0.0, 20.0));
        // …while the shared edge next to the CHECKED item is covered by its fill: a checked item paints
        // above its neighbours (Compose's `interactionZIndex`, which is what `z_index` is here for).
        assert!(close(at(&px, w, 99.0, 20.0), sec), "the checked item covers the shared edge (got {:?})", at(&px, w, 99.0, 20.0));
        assert!(close(at(&px, w, 100.0, 20.0), sec), "…and its fill continues past it (got {:?})", at(&px, w, 100.0, 20.0));
    }

    #[test]
    fn a_disabled_item_uses_the_disabled_palette() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = Composer::new();
        let t = theme.clone();
        composer.compose(|ctx| {
            WiniaTheme::with_theme(t, ctx, |ctx| {
                SingleChoiceSegmentedButtonRow::new().build(ctx, |ctx| {
                    SegmentedButton::new(true, || {})
                        .shape(SegmentedButtonDefaults::item_shape(0, 1))
                        .enabled(false)
                        .build(ctx, |ctx| {
                            Text::new("Off").build(ctx);
                        });
                });
            });
        });
        composer.layout(Constraints::new(0.0, 120.0, 0.0, 100.0));
        let mut surface = skia_safe::surfaces::raster_n32_premul((120, 60)).unwrap();
        surface.canvas().clear(skia_safe::Color::WHITE);
        let root = composer.layout_root_idx().expect("root");
        crate::render::render(composer.arena_nodes(), root, surface.canvas());
        let pm = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        // A disabled ACTIVE item keeps secondaryContainer as its fill.
        let sec = (theme.secondary_container.r as i32, theme.secondary_container.g as i32, theme.secondary_container.b as i32);
        assert!(close(at(px, 120, 60.0, 20.0), sec), "a disabled active fill stays secondaryContainer (got {:?})", at(px, 120, 60.0, 20.0));
    }
