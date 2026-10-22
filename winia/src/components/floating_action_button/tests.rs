    use super::*;
    use crate::runtime::composer::Composer;
    use crate::components::Text;

    /// The extended FAB's size animation is M3's `FastSpatial` spring (0.9/1400), not the no-bouncy
    /// 400 the comment used to call an approximation.
    #[test]
    fn the_size_spec_is_m3s_fast_spatial_spring() {
        match fab_size_spec() {
            crate::animation::AnimationSpec::Spring(spec) => {
                assert_eq!(spec, crate::animation::SpringSpec::spatial_fast());
                assert_eq!((spec.damping_ratio, spec.stiffness), (0.9, 1400.0));
            }
            other => panic!("the size spec must be a spring, got {other:?}"),
        }
    }

    fn child<'a>(nodes: &'a [LayoutNode], parent: usize, index: usize) -> &'a LayoutNode {
        &nodes[nodes[parent].children[index]]
    }

    #[test]
    fn size_tokens_match_m3_fab_variants() {
        assert_eq!(FloatingActionButtonSize::Small.container_size(), 40.0);
        assert_eq!(FloatingActionButtonSize::Regular.container_size(), 56.0);
        assert_eq!(FloatingActionButtonSize::Medium.container_size(), 80.0);
        assert_eq!(FloatingActionButtonSize::Large.container_size(), 96.0);
        assert_eq!(FloatingActionButtonSize::Regular.icon_size(), 24.0);
        assert_eq!(FloatingActionButtonSize::Medium.icon_size(), 28.0);
        assert_eq!(FloatingActionButtonSize::Large.icon_size(), 36.0);
    }

    #[test]
    fn default_elevation_matches_material3_tokens() {
        // `FabPrimaryContainerTokens`: `ContainerElevation`, `PressedContainerElevation` and
        // `FocusedContainerElevation` are all `ElevationTokens.Level3` (6 dp); only the hovered one is
        // `Level4` (8 dp). (This assertion used to read `(6, 12, 8, 8, 0)`, which the tokens never said.)
        assert_eq!(
            FloatingActionButtonElevation::default_elevation(),
            FloatingActionButtonElevation::new(6.0, 6.0, 6.0, 8.0, 0.0)
        );
    }

    #[test]
    fn defaults_use_regular_primary_container_fab() {
        let fab = FloatingActionButton::new();
        assert_eq!(fab.get_size(), FloatingActionButtonSize::Regular);
        assert_eq!(fab.get_shape(), Shape::rounded(16.0));
        assert_eq!(
            fab.get_elevation(),
            Some(FloatingActionButtonElevation::default_elevation())
        );
        assert!(fab.get_enabled());
    }

    /// `FloatingActionButtonDefaults.extendedFabShape` (`material3/FloatingActionButton.kt:525-526`)
    /// is CornerLarge, and it is the shape an extended FAB with no explicit shape resolves to.
    #[test]
    fn the_extended_fab_default_shape_is_corner_large() {
        let def = FloatingActionButtonDefaults::extended_fab_shape();
        assert_eq!(
            def,
            Shape::rounded(16.0),
            "ExtendedFabPrimaryTokens.ContainerShape"
        );

        let mut composer = Composer::new();
        composer.compose(|ctx| {
            ExtendedFloatingActionButton::content(|ctx| Text::new("Create").build(ctx))
                .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let painted_shape = nodes[root]
            .modifier
            .elements()
            .iter()
            .find_map(|el| match el {
                crate::modifier::ModifierElement::Background { shape, .. } => Some(shape.eval()),
                _ => None,
            })
            .expect("the extended FAB paints a background");
        assert_eq!(
            painted_shape, def,
            "the component defaults to the shared accessor, not a second literal"
        );
    }

    #[test]
    fn size_constructors_select_expected_shapes() {
        assert_eq!(
            FloatingActionButton::small().get_shape(),
            Shape::rounded(12.0)
        );
        assert_eq!(
            FloatingActionButton::medium().get_shape(),
            Shape::rounded(20.0)
        );
        assert_eq!(
            FloatingActionButton::large().get_shape(),
            Shape::rounded(28.0)
        );
        assert_eq!(
            FloatingActionButton::medium()
                .shape(Shape::Circle)
                .get_shape(),
            Shape::Circle
        );
    }

    #[test]
    fn default_colors_use_primary_container_tokens() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let colors = FloatingActionButtonDefaults::colors(&theme);
        assert_eq!(colors.container, theme.primary_container);
        assert_eq!(colors.content, theme.on_primary_container);
        assert_eq!(
            colors.container_color(false).a,
            (theme.on_surface.a as f32 * 0.12) as u8
        );
        assert_eq!(
            colors.content_color(false).a,
            (theme.on_surface.a as f32 * 0.38) as u8
        );

        let primary = FloatingActionButtonDefaults::primary_colors(&theme);
        assert_eq!(primary.container, theme.primary);
        assert_eq!(primary.content, theme.on_primary);
    }

    #[test]
    fn ripple_color_follows_enabled_content_color() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let colors = FloatingActionButtonDefaults::colors(&theme);
        assert_eq!(colors.ripple_color(true), theme.on_primary_container);
        assert_eq!(colors.ripple_color(false), colors.disabled_content);

        let primary = FloatingActionButtonDefaults::primary_colors(&theme);
        assert_eq!(primary.ripple_color(true), theme.on_primary);
    }

    /// `contentColorFor(containerColor)` is Compose's default (`FloatingActionButton.kt:114`): a
    /// paired role derives its "on" colour, and anything else falls back to `on_surface`.
    #[test]
    fn from_container_derives_the_content_colour() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let paired = FloatingActionButtonColors::from_container(&theme, theme.secondary_container);
        assert_eq!(paired.container, theme.secondary_container);
        assert_eq!(paired.content, theme.on_secondary_container);

        let custom = Color::from_argb(255, 1, 2, 3);
        let unpaired = FloatingActionButtonColors::from_container(&theme, custom);
        assert_eq!(unpaired.container, custom);
        assert_eq!(
            unpaired.content, theme.on_surface,
            "a colour with no paired \"on\" role falls back to on_surface"
        );
        // The disabled pair still comes from the theme, exactly as `from_pair` builds it.
        assert_eq!(
            unpaired.disabled_container.a,
            (theme.on_surface.a as f32 * 0.12) as u8
        );
    }

    #[test]
    fn elevation_state_priority_matches_interaction_order() {
        let elevation = FloatingActionButtonElevation::new(1.0, 3.0, 2.0, 4.0, 0.0);
        let mut state = ComponentState::idle();
        assert_eq!(elevation.for_state(&state), 1.0);
        state.hovered = true;
        assert_eq!(elevation.for_state(&state), 4.0);
        state.focused = true;
        assert_eq!(elevation.for_state(&state), 4.0);
        state.pressed = true;
        assert_eq!(elevation.for_state(&state), 3.0);
        state.enabled = false;
        assert_eq!(elevation.for_state(&state), 0.0);
    }

    /// A DRAGGED FAB takes the default — Compose's `FloatingActionButtonElevationAnimatable`
    /// matches only Press/Hover/Focus and falls through to the default for a drag
    /// (`FloatingActionButton.kt:713-719`), and the Button follows the same rule. A drag always
    /// holds a press, so without the dragged branch the value would be the pressed one.
    #[test]
    fn a_dragged_fab_takes_the_default_elevation() {
        let elevation = FloatingActionButtonElevation::new(1.0, 3.0, 2.0, 4.0, 0.0);
        let mut state = ComponentState::idle();
        state.dragged = true;
        state.pressed = true;
        assert_eq!(elevation.for_state(&state), 1.0);
        assert_eq!(
            state.elevation_interaction(),
            Some(crate::animation::ElevationInteraction::Drag)
        );
    }

    /// The short-circuit both FABs share: a token set whose every state is 0 gets no animated
    /// layer at all (`FloatingActionButtonElevation::bottom_app_bar`).
    #[test]
    fn a_shadow_free_elevation_is_recognised() {
        assert!(FloatingActionButtonElevation::bottom_app_bar().is_shadow_free());
        assert!(!FloatingActionButtonElevation::new(1.0, 3.0, 2.0, 4.0, 0.0).is_shadow_free());
        assert!(!FloatingActionButtonElevation::new(0.0, 0.0, 0.0, 0.0, 1.0).is_shadow_free());
    }

    /// Compose paints the FAB through a `Surface(tonalElevation = elevation.tonalElevation())`
    /// (`FloatingActionButton.kt:155`), and `tonalElevation()` IS the default elevation
    /// (`:624-626`). So a container that resolves to `surface` is tinted by that value — and only
    /// a `surface` container is.
    #[test]
    fn a_surface_coloured_fab_is_tinted_by_its_tonal_elevation() {
        let theme = ThemeColors::default_light();
        let painted = |colors: FloatingActionButtonColors| -> Color {
            let mut composer = Composer::new();
            composer.compose(|ctx| {
                WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    FloatingActionButton::new().colors(colors).build(ctx, |_| {});
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 300.0));
            composer
                .arena_nodes()
                .iter()
                .find_map(|node| {
                    node.modifier.elements().iter().find_map(|el| match el {
                        crate::modifier::ModifierElement::Background { color, .. } => {
                            Some(color.eval())
                        }
                        _ => None,
                    })
                })
                .expect("the FAB paints a background")
        };

        // The token set's default is Level3 = 6 dp, so the tint's alpha is (4.5·ln 7 + 2)/100.
        let alpha = ((4.5 * (6.0f32 + 1.0).ln()) + 2.0) / 100.0;
        let on_surface =
            FloatingActionButtonColors::from_pair(&theme, theme.surface, theme.on_surface);
        assert_eq!(
            painted(on_surface),
            theme.surface.overlay(theme.surface_tint, alpha),
            "a surface-coloured FAB is tinted by its default elevation"
        );

        // Any other container is left alone — the gate `applyTonalElevation` has, and the reason
        // the usual `primaryContainer` FAB never shows a tint.
        assert_eq!(
            painted(FloatingActionButtonDefaults::primary_colors(&theme)),
            theme.primary,
            "a non-surface container is untouched"
        );
    }

    #[test]
    fn content_color_is_provided_to_fab_content() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                FloatingActionButton::new().build(ctx, |_ctx| {
                    assert_eq!(WiniaTheme::content_color(), theme.on_primary_container);
                });
            });
        });
    }

    #[test]
    fn builder_configuration_is_retained() {
        let custom =
            FloatingActionButtonColors::new(Color::RED, Color::WHITE, Color::BLACK, Color::GREEN);
        let fab = FloatingActionButton::small()
            .enabled(false)
            .colors(custom)
            .shape(Shape::pill())
            .elevation(FloatingActionButtonElevation::bottom_app_bar());
        assert!(!fab.get_enabled());
        assert_eq!(fab.get_colors(), Some(custom));
        assert_eq!(fab.get_shape(), Shape::Pill);
        assert_eq!(
            fab.get_elevation(),
            Some(FloatingActionButtonElevation::bottom_app_bar())
        );
    }
    // ── ExtendedFloatingActionButton ──

    fn icon_leaf(ctx: &mut ComposeCtx, size: f32) {
        let key = ctx.next_key();
        ctx.start_leaf(key, Modifier::new().size(size, size));
        ctx.end_node();
    }

    #[test]
    fn extended_fab_tokens_match_androidx_main() {
        assert_eq!(EXTENDED_FAB_HEIGHT, 56.0, "ExtendedFabPrimaryTokens.ContainerHeight");
        assert_eq!(EXTENDED_FAB_COLLAPSED_WIDTH, 56.0, "FabBaselineTokens.ContainerWidth");
        assert_eq!(EXTENDED_FAB_MIN_EXPANDED_WIDTH, 80.0, "ExtendedFabMinimumWidth");
    }

    /// Each tier's metric set, against `*ExtendedFabTokens` — including the three values upstream
    /// does NOT take from the token file. The Medium and Large icon-label gaps are overridden in
    /// the source (its own TODO says the tokens are wrong: 16 and 20 against the code's 12 and 16),
    /// the medium shape is `ShapeDefaults.LargeIncreased` because that token line is commented out
    /// upstream, and the collapsed width is the tier's `ContainerHeight`, not a shared constant.
    #[test]
    fn the_extended_fab_size_tiers_are_the_tokens() {
        for (size, height, padding, gap, radius, icon) in [
            (ExtendedFabSize::Small, 56.0, 16.0, 8.0, 16.0, 24.0),
            (ExtendedFabSize::Medium, 80.0, 26.0, 12.0, 20.0, 28.0),
            (ExtendedFabSize::Large, 96.0, 28.0, 16.0, 28.0, 32.0),
        ] {
            assert_eq!(size.container_height(), height, "{size:?} ContainerHeight");
            assert_eq!(size.horizontal_padding(), padding, "{size:?} Leading/TrailingSpace");
            assert_eq!(size.icon_label_space(), gap, "{size:?} IconLabelSpace");
            assert_eq!(size.shape(), Shape::rounded(radius), "{size:?} extendedFabShape");
            assert_eq!(size.icon_size(), icon, "{size:?} IconSize");
            assert_eq!(
                size.metrics().collapsed_width,
                height,
                "{size:?}: *ExtendedFabMinimumWidth IS the tier's ContainerHeight"
            );
        }
        // The label style is the one table the loop above does not carry.
        let typography = crate::theme::Typography::default();
        for (size, style, name) in [
            (ExtendedFabSize::Small, &typography.title_medium, "TitleMedium"),
            (ExtendedFabSize::Medium, &typography.title_large, "TitleLarge"),
            (ExtendedFabSize::Large, &typography.headline_small, "HeadlineSmall"),
        ] {
            assert_eq!(
                size.text_style(&typography).font_size,
                style.font_size,
                "{size:?} takes {name}"
            );
        }
        // And they are three different styles, not one size repeated.
        assert_ne!(typography.title_medium.font_size, typography.title_large.font_size);
        assert_ne!(typography.title_large.font_size, typography.headline_small.font_size);

        // The three tiers are three distinct heights — nothing is shared between them.
        assert_ne!(
            ExtendedFabSize::Small.container_height(),
            ExtendedFabSize::Medium.container_height()
        );
        assert_ne!(
            ExtendedFabSize::Medium.container_height(),
            ExtendedFabSize::Large.container_height()
        );
    }

    /// A named tier moves the whole metric set at once. The collapsed box is the tier's own square,
    /// and the expanded one is `start + icon + gap + text + end` with the icon at the start padding
    /// and the label one icon-plus-gap further in (upstream's private overload, which takes all four
    /// numbers as parameters: `FloatingActionButton.kt:917-986`).
    #[test]
    fn a_named_tier_moves_the_collapsed_and_expanded_geometry() {
        let geom = |size: ExtendedFabSize, expanded: bool| {
            let mut composer = Composer::new();
            // `move` because the icon slot captures `size`, and a composable closure has to own
            // what it captures.
            composer.compose(move |ctx| {
                ExtendedFloatingActionButton::new(
                    |ctx| Text::new("Create").build(ctx),
                    move |ctx| icon_leaf(ctx, size.icon_size()),
                    State::new(expanded),
                )
                .size(size)
                .on_click(|| {})
                .build(ctx);
            });
            composer.layout(Constraints::new(0.0, 600.0, 0.0, 400.0));
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let icon = child(&nodes, root, 0);
            let text = child(&nodes, root, 1);
            (
                nodes[root].measured_size,
                icon.position.x,
                text.position.x,
                text.measured_size.width,
            )
        };

        for size in [
            ExtendedFabSize::Small,
            ExtendedFabSize::Medium,
            ExtendedFabSize::Large,
        ] {
            let h = size.container_height();
            let (collapsed, _, _, _) = geom(size, false);
            assert_eq!(
                (collapsed.width, collapsed.height),
                (h, h),
                "{size:?} collapses to its own square"
            );

            let (expanded, icon_x, text_x, text_w) = geom(size, true);
            let padding = size.horizontal_padding();
            assert_eq!(expanded.height, h, "{size:?} height");
            assert_eq!(
                expanded.width,
                padding + size.icon_size() + size.icon_label_space() + text_w + padding,
                "{size:?}: start + icon + gap + text + end"
            );
            assert_eq!(icon_x, padding, "{size:?} icon sits at the start padding");
            assert_eq!(
                text_x,
                padding + size.icon_size() + size.icon_label_space(),
                "{size:?} label sits one icon plus the gap further in"
            );
        }
    }

    #[test]
    fn extended_fab_collapsed_is_56_square_icon_centered() {
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            ExtendedFloatingActionButton::new(
                |ctx| Text::new("Create").build(ctx),
                |ctx| icon_leaf(ctx, 24.0),
                State::new(false),
            )
            .on_click(|| {})
            .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        // 收起态：56×56（同普通 FAB），图标居中
        assert_eq!(nodes[root].measured_size.width, EXTENDED_FAB_COLLAPSED_WIDTH);
        assert_eq!(nodes[root].measured_size.height, EXTENDED_FAB_HEIGHT);
        let icon = child(&nodes, root, 0);
        assert_eq!(
            icon.position,
            Offset::new(
                (EXTENDED_FAB_COLLAPSED_WIDTH - 24.0) / 2.0,
                (EXTENDED_FAB_HEIGHT - 24.0) / 2.0,
            ),
            "收起态图标居中"
        );
    }

    #[test]
    fn extended_fab_expanded_shows_icon_then_text_with_spec_paddings() {
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            ExtendedFloatingActionButton::new(
                |ctx| Text::new("Create").build(ctx),
                |ctx| icon_leaf(ctx, 24.0),
                State::new(true),
            )
            .on_click(|| {})
            .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let icon = child(&nodes, root, 0);
        let text_slot = child(&nodes, root, 1);
        // 展开态：宽 = 16 + 24 + 12 + textW + 20（≥80）；高 56
        let expected_w = EXT_START_ICON_PADDING
            + 24.0
            + EXT_END_ICON_PADDING
            + text_slot.measured_size.width
            + EXT_TEXT_PADDING;
        assert_eq!(nodes[root].measured_size.height, EXTENDED_FAB_HEIGHT);
        assert_eq!(nodes[root].measured_size.width, expected_w);
        // 图标 x=16（start padding）垂直居中
        assert_eq!(icon.position.x, EXT_START_ICON_PADDING);
        assert_eq!(icon.position.y, (EXTENDED_FAB_HEIGHT - 24.0) / 2.0);
        // 文本槽紧跟 icon+12dp 间距，垂直居中
        assert_eq!(
            text_slot.position.x,
            EXT_START_ICON_PADDING + 24.0 + EXT_END_ICON_PADDING,
        );
        assert_eq!(
            text_slot.position.y,
            (EXTENDED_FAB_HEIGHT - text_slot.measured_size.height) / 2.0,
        );
    }

    #[test]
    fn extended_fab_rtl_mirrors_icon_and_text_positions() {
        // RTL 展开态：图标贴右缘（width-16-iconW），文本在图标左侧（gap 12）
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            ExtendedFloatingActionButton::new(
                |ctx| Text::new("Create").build(ctx),
                |ctx| icon_leaf(ctx, 24.0),
                State::new(true),
            )
            .modifier(Modifier::new().layout_direction(LayoutDirection::Rtl))
            .on_click(|| {})
            .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let width = nodes[root].measured_size.width;
        let icon = child(&nodes, root, 0);
        let text_slot = child(&nodes, root, 1);
        // 图标右缘距容器右缘 16（LTR 时是左缘距左缘 16）
        assert_eq!(
            icon.position.x + icon.measured_size.width,
            width - EXT_START_ICON_PADDING,
            "RTL 下图标应镜像到右侧"
        );
        // 文本右缘 = 图标左缘 - 12dp 间距
        assert_eq!(
            text_slot.position.x + text_slot.measured_size.width,
            icon.position.x - EXT_END_ICON_PADDING,
        );
        // LTR 对照：同结构方向相反
        let mut ltr = Composer::new();
        ltr.compose(|ctx| {
            ExtendedFloatingActionButton::new(
                |ctx| Text::new("Create").build(ctx),
                |ctx| icon_leaf(ctx, 24.0),
                State::new(true),
            )
            .on_click(|| {})
            .build(ctx);
        });
        ltr.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        let nodes2 = ltr.arena_nodes();
        let icon_ltr = child(&nodes2, ltr.layout_root_idx().unwrap(), 0);
        assert_eq!(icon_ltr.position.x, EXT_START_ICON_PADDING, "LTR 图标在左侧");
    }

    /// The content-only overload (`FloatingActionButton.kt:312`): one row, 20 dp horizontal
    /// padding, an 80 dp floor, 56 dp tall, with the content centred.
    #[test]
    fn extended_fab_content_only_pads_20_and_floors_at_80() {
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            ExtendedFloatingActionButton::content(|ctx| Text::new("Create").build(ctx))
                .on_click(|| {})
                .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let content = child(&nodes, root, 0);
        let size = nodes[root].measured_size;
        assert_eq!(size.height, EXTENDED_FAB_HEIGHT);
        assert!(
            size.width >= EXTENDED_FAB_MIN_EXPANDED_WIDTH,
            "a short label still floors at 80 dp, got {size:?}"
        );
        assert_eq!(content.position.x, (size.width - content.measured_size.width) / 2.0);
        assert_eq!(
            content.position.y,
            (size.height - content.measured_size.height) / 2.0
        );
    }

    /// Content wider than the floor widens the bar: width = 20 + content + 20.
    #[test]
    fn extended_fab_content_only_widens_for_wide_content() {
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            ExtendedFloatingActionButton::content(|ctx| {
                let key = ctx.next_key();
                ctx.start_leaf(key, Modifier::new().size(200.0, 24.0));
                ctx.end_node();
            })
            .build(ctx);
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        assert_eq!(
            nodes[root].measured_size.width,
            200.0 + 2.0 * EXT_TEXT_PADDING,
        );
        assert_eq!(nodes[root].measured_size.height, EXTENDED_FAB_HEIGHT);
    }
