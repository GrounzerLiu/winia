    use super::*;

    #[test]
    fn default_colors_per_style() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        // The standard and outlined styles both hand the icon `LocalContentColor.current`, exactly as
        // `IconButtonDefaults` does, so they have to resolve while composing; only the filled styles use
        // fixed roles. The VIBRANT sets are what pin `OnSurfaceVariant`, so the plain outlined button
        // must not read it — that collision made the plain and vibrant buttons indistinguishable.
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |_ctx| {
                let local = WiniaTheme::content_color();
                let s = IconButtonDefaults::icon_button_colors(&theme);
                assert_eq!(s.container.a, 0, "standard container is transparent");
                assert_eq!(s.content, local, "standard content follows LocalContentColor");
                assert_eq!(s.disabled_content.a, (local.a as f32 * 0.38) as u8);

                let o = IconButtonDefaults::outlined_icon_button_colors(&theme);
                assert_eq!(o.container.a, 0, "outlined container is transparent");
                assert_eq!(o.content, local, "outlined content follows LocalContentColor");
            });
        });
        let f = IconButtonDefaults::filled_icon_button_colors(&theme);
        assert_eq!(f.container, theme.primary);
        assert_eq!(f.content, theme.on_primary);
        let t = IconButtonDefaults::filled_tonal_icon_button_colors(&theme);
        assert_eq!(t.container, theme.secondary_container);
        assert_eq!(t.content, theme.on_secondary_container);
    }

    #[test]
    fn a_size_change_reaches_the_restartable_group() {
        // `size_variant` is read by `build` (container size and outlined border width), so it must be
        // declared with `ctx.changed`: the Skip contract is a whitelist, and an undeclared parameter
        // that changes leaves a Clean slot skipping straight past the new value.
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let size = crate::runtime::state::State::new(IconButtonSize::Small);
        let mut composer = crate::runtime::composer::Composer::new();
        // `build` reserves a 48 dp touch target around the token container, so the widest node is the
        // target: 48 for Small (whose container is 40), and the container itself for Large (96).
        let widest = |composer: &crate::runtime::composer::Composer| -> f32 {
            composer
                .arena_nodes()
                .iter()
                .map(|node| node.measured_size.width)
                .fold(0.0f32, f32::max)
        };
        let mut compose = |composer: &mut crate::runtime::composer::Composer| {
            let chosen = size.get();
            composer.compose(|ctx| {
                WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    IconButton::new()
                        .size(chosen)
                        .on_click(|| {})
                        .build(ctx, |ctx| {
                            crate::components::icon::Icon::svg_path("M12 2L22 12 12 22 2 12Z")
                                .build(ctx);
                        });
                });
            });
        };
        let constraints = crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0);
        compose(&mut composer);
        composer.layout(constraints);
        assert_eq!(
            widest(&composer),
            48.0,
            "Small's 40 dp container sits inside a 48 dp touch target"
        );

        size.update(|s| *s = IconButtonSize::Large);
        compose(&mut composer);
        composer.layout(constraints);
        assert_eq!(
            widest(&composer),
            96.0,
            "the new size reaches the slot instead of being skipped"
        );
    }

    #[test]
    fn outlined_border_follows_the_content_colour() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |_ctx| {
                let local = WiniaTheme::content_color();
                let enabled = IconButtonDefaults::outlined_border(&theme, true, 1.0);
                assert_eq!(enabled.color, local, "the plain outline takes LocalContentColor");
                let disabled = IconButtonDefaults::outlined_border(&theme, false, 1.0);
                assert_eq!(disabled.color.a, (local.a as f32 * 0.38) as u8);
                assert_eq!(
                    IconButtonDefaults::outlined_border(
                        &theme,
                        true,
                        IconButtonSize::Large.outline_width()
                    )
                    .width,
                    2.0,
                    "Large outline is 2 dp"
                );
                // The VIBRANT border is the one that pins the token's OutlineVariant — the plain one
                // must differ from it, or the two buttons look the same.
                let vibrant =
                    IconButtonDefaults::outlined_icon_button_vibrant_border(&theme, true, 1.0);
                assert_eq!(vibrant.color, theme.outline_variant);
                assert_ne!(
                    enabled.color, vibrant.color,
                    "plain and vibrant outlines must not be the same colour"
                );
            });
        });
    }

    #[test]
    fn vibrant_colors_use_the_token_roles() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        // Standard: the vibrant set pins `StandardIconButtonTokens.Color` (OnSurfaceVariant), where the
        // plain set hands the icon `LocalContentColor`.
        let standard = IconButtonDefaults::icon_button_vibrant_colors(&theme);
        assert_eq!(
            standard.container.a, 0,
            "vibrant standard keeps a transparent container"
        );
        assert_eq!(standard.content, theme.on_surface_variant);
        assert_eq!(
            standard.disabled_content.a,
            (theme.on_surface.a as f32 * 0.38) as u8,
            "disabled content is OnSurface at DisabledOpacity"
        );

        let outlined = IconButtonDefaults::outlined_icon_button_vibrant_colors(&theme);
        assert_eq!(outlined.container.a, 0);
        assert_eq!(outlined.content, theme.on_surface_variant);

        // Vibrant border: the token's OutlineColor, not the LocalContentColor the plain border uses.
        let on = IconButtonDefaults::outlined_icon_button_vibrant_border(&theme, true, 1.0);
        assert_eq!(on.color, theme.outline_variant);
        assert_eq!(on.width, 1.0);
        let off = IconButtonDefaults::outlined_icon_button_vibrant_border(&theme, false, 1.0);
        assert_eq!(off.color.a, (theme.outline_variant.a as f32 * 0.38) as u8);
    }

    #[test]
    fn color_state_resolution() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let colors = IconButtonDefaults::filled_icon_button_colors(&theme);
        assert_eq!(colors.container_color(true), theme.primary);
        assert_eq!(colors.container_color(false), colors.disabled_container);
        assert_eq!(colors.content_color(true), theme.on_primary);
        assert_eq!(colors.content_color(false), colors.disabled_content);
        // M3 token：禁用容器 OnSurface 10%、禁用内容 OnSurface 38%
        assert_eq!(colors.disabled_container.a, (theme.on_surface.a as f32 * 0.10) as u8);
        assert_eq!(colors.disabled_content.a, (theme.on_surface.a as f32 * 0.38) as u8);
        let standard = IconButtonDefaults::icon_button_colors(&theme);
        assert_eq!(standard.disabled_container.a, 0, "标准版禁用容器仍透明");
    }

    #[test]
    fn variant_constructors() {
        assert_eq!(IconButton::new().get_style(), IconButtonStyle::Standard);
        assert_eq!(IconButton::filled().get_style(), IconButtonStyle::Filled);
        assert_eq!(IconButton::filled_tonal().get_style(), IconButtonStyle::FilledTonal);
        assert_eq!(IconButton::outlined().get_style(), IconButtonStyle::Outlined);
        assert_eq!(IconButton::new().get_shape(), Shape::Circle);
        assert_eq!(IconButton::new().get_size(), IconButtonSize::Small);
        assert_eq!(
            IconButton::new().size(IconButtonSize::Large).get_size(),
            IconButtonSize::Large
        );
    }

    #[test]
    fn size_variant_dimensions() {
        assert_eq!(IconButtonSize::XSmall.container_size(), 32.0);
        assert_eq!(IconButtonSize::Small.container_size(), 40.0);
        assert_eq!(IconButtonSize::Medium.container_size(), 56.0);
        assert_eq!(IconButtonSize::Large.container_size(), 96.0);
        assert_eq!(IconButtonSize::XLarge.container_size(), 136.0);
        assert_eq!(IconButtonSize::XSmall.icon_size(), 20.0);
        assert_eq!(IconButtonSize::Small.icon_size(), 24.0);
        assert_eq!(IconButtonSize::Medium.icon_size(), 24.0);
        assert_eq!(IconButtonSize::Large.icon_size(), 32.0);
        assert_eq!(IconButtonSize::XLarge.icon_size(), 40.0);
        assert_eq!(IconButtonSize::XSmall.outline_width(), 1.0);
        assert_eq!(IconButtonSize::Medium.outline_width(), 1.0);
        assert_eq!(IconButtonSize::Large.outline_width(), 2.0);
        assert_eq!(IconButtonSize::XLarge.outline_width(), 3.0);
    }

    #[test]
    fn width_options_match_the_tokens() {
        use IconButtonWidthOption::{Narrow, Uniform, Wide};
        // Uniform is the square container, so its width IS that size's `ContainerHeight` token.
        for (size, height) in [
            (IconButtonSize::XSmall, 32.0),
            (IconButtonSize::Small, 40.0),
            (IconButtonSize::Medium, 56.0),
            (IconButtonSize::Large, 96.0),
            (IconButtonSize::XLarge, 136.0),
        ] {
            assert_eq!(
                size.container_width(Uniform),
                height,
                "Uniform reproduces the square container"
            );
        }
        // Narrow and Wide are the token spaces around the icon: Small 4+4 / 14+14, Medium 12+12 /
        // 24+24, XLarge 32+32 / 72+72.
        assert_eq!(IconButtonSize::Small.container_width(Narrow), 32.0);
        assert_eq!(IconButtonSize::Small.container_width(Wide), 52.0);
        assert_eq!(IconButtonSize::Medium.container_width(Narrow), 48.0);
        assert_eq!(IconButtonSize::Medium.container_width(Wide), 72.0);
        assert_eq!(IconButtonSize::XLarge.container_width(Wide), 184.0);
        assert_eq!(IconButtonSize::Small.horizontal_space(Narrow), 8.0);
        assert_eq!(IconButtonSize::Large.horizontal_space(Uniform), 64.0);
    }

    #[test]
    fn shape_families_and_pressed_shapes_match_the_tokens() {
        for size in [
            IconButtonSize::XSmall,
            IconButtonSize::Small,
            IconButtonSize::Medium,
            IconButtonSize::Large,
            IconButtonSize::XLarge,
        ] {
            assert_eq!(
                size.container_shape(IconButtonShape::Round),
                Shape::Circle,
                "ContainerShapeRound is CornerFull at every size"
            );
        }
        assert_eq!(
            IconButtonSize::Small.container_shape(IconButtonShape::Square),
            Shape::rounded(12.0),
            "ContainerShapeSquare is CornerMedium for Small"
        );
        assert_eq!(
            IconButtonSize::Medium.container_shape(IconButtonShape::Square),
            Shape::rounded(16.0)
        );
        assert_eq!(
            IconButtonSize::XLarge.container_shape(IconButtonShape::Square),
            Shape::rounded(28.0)
        );
        // PressedContainerShape: CornerSmall / CornerMedium / CornerLarge.
        assert_eq!(
            IconButtonSize::Small.pressed_container_shape(),
            Shape::rounded(8.0)
        );
        assert_eq!(
            IconButtonSize::Medium.pressed_container_shape(),
            Shape::rounded(12.0)
        );
        assert_eq!(
            IconButtonSize::XLarge.pressed_container_shape(),
            Shape::rounded(16.0)
        );
    }

    #[test]
    fn a_press_morphs_the_container_shape_over_time() {
        // `update_animations` below drives the process-wide table, which `clear_all_animations` (200+ call
        // sites) empties for everyone — so this holds the same lock the animation tests take.
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // The container paints through `background(color, shape)`, so the shape its node carries is
        // wherever the morph has reached. The animation lives in the composer's slot, so this reuses
        // one composer instead of rebuilding per sample.
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let source = MutableInteractionSource::new();
        let src = source.clone();
        let mut composer = crate::runtime::composer::Composer::new();
        let mut build = |composer: &mut crate::runtime::composer::Composer| {
            composer.compose(|ctx| {
                WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    IconButton::new()
                        .on_click(|| {})
                        .interaction_source(src.clone())
                        .build(ctx, |ctx| {
                            crate::components::icon::Icon::svg_path("M12 2L22 12 12 22 2 12Z")
                                .build(ctx);
                        });
                });
            });
            composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        };
        let radius = |composer: &crate::runtime::composer::Composer| -> f32 {
            let painted = composer.arena_nodes().iter().find_map(|node| {
                node.modifier.elements().iter().find_map(|el| match el {
                    crate::modifier::ModifierElement::Background { shape, .. } => Some(shape.eval()),
                    _ => None,
                })
            });
            match painted {
                Some(Shape::RoundedRect { corner_radius }) => corner_radius,
                other => panic!("the container should be a rounded rect, got {other:?}"),
            }
        };

        build(&mut composer);
        // Round is CornerFull over the token container: the paint is inset back to 40 dp inside the
        // 48 dp touch target, which is where `Shape::Circle` used to paint.
        assert_eq!(radius(&composer), 20.0, "at rest the container is Round");

        source.emit_press();
        build(&mut composer);
        // The morph must be REGISTERED, and the shape must not have switched yet. `assert_ne!` on its own
        // would also pass if no animation had started at all: the resting radius is 20, not 8.
        assert!(
            crate::animation::is_animating(),
            "pressing registers the shape morph"
        );
        assert_ne!(
            radius(&composer),
            8.0,
            "a press starts a morph instead of switching the shape"
        );

        for _ in 0..600 {
            if !crate::animation::update_animations() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
            build(&mut composer);
        }
        assert!(
            (radius(&composer) - 8.0).abs() < 0.5,
            "and it arrives at Small's PressedContainerShape, got {}",
            radius(&composer)
        );
    }

    #[test]
    fn content_color_propagates_to_subtree() {
        // with_content_color 覆盖子树内容色（Icon tint Auto 的取值来源）
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_content_color(Color::RED, ctx, |ctx| {
                assert_eq!(WiniaTheme::content_color(), Color::RED);
                WiniaTheme::with_content_color(Color::BLUE, ctx, |ctx| {
                    assert_eq!(WiniaTheme::content_color(), Color::BLUE, "嵌套覆盖生效");
                });
                assert_eq!(WiniaTheme::content_color(), Color::RED, "闭包退出后恢复");
            });
        });
    }

    #[test]
    fn theme_default_content_color_is_on_surface() {
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::light(ctx, |ctx| {
                let theme = WiniaTheme::colors();
                assert_eq!(WiniaTheme::content_color(), theme.on_surface);
            });
        });
    }

    #[test]
    fn filled_button_icon_tint_resolves_to_content_color() {
        // 集成：Filled IconButton 内容中的 Icon（tint Auto）应解析为 on_primary
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                IconButton::filled()
                    .on_click(|| {})
                    .build(ctx, |ctx| {
                        crate::components::icon::Icon::svg_path("M12 2L22 12 12 22 2 12Z").build(ctx);
                    });
            });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        let mut resolved_tint = None;
        for node in composer.arena_nodes() {
            for el in node.modifier.elements() {
                if let crate::modifier::ModifierElement::DrawIcon { spec } = el {
                    resolved_tint = spec.tint.as_ref().map(|t| t.eval());
                }
            }
        }
        assert_eq!(resolved_tint, Some(theme.on_primary), "Filled 内容色下传 Icon tint");
    }

    #[test]
    fn disabled_icon_button_has_no_interaction_elements() {
        use crate::modifier::ModifierElement;
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            IconButton::new()
                .enabled(false)
                .on_click(|| {})
                .build(ctx, |ctx| {
                    crate::components::icon::Icon::svg_path("M12 2L22 12 12 22 2 12Z").build(ctx);
                });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        for node in composer.arena_nodes() {
            for el in node.modifier.elements() {
                assert!(
                    !matches!(el, ModifierElement::Ripple { .. })
                        && !matches!(el, ModifierElement::Focusable { .. }),
                    "禁用 IconButton 不应有波纹/可聚焦元素: {el:?}"
                );
            }
        }
    }

    /// The `SelectedContainerShape*` table, size by size. Every size has
    /// `ContainerShapeRound = CornerFull` and a rounded `SelectedContainerShapeRound` —
    /// CornerMedium (12) for XS/S, CornerLarge (16) for M, CornerExtraLarge (28) for L/XL — while
    /// the SQUARE pair is the reverse: `ContainerShapeSquare` is that rectangle and
    /// `SelectedContainerShapeSquare` is `CornerFull`. A toggle therefore INVERTS its family when
    /// it becomes checked.
    #[test]
    fn the_selected_container_shapes_are_the_tokens() {
        for (size, selected_round) in [
            (IconButtonSize::XSmall, 12.0),
            (IconButtonSize::Small, 12.0),
            (IconButtonSize::Medium, 16.0),
            (IconButtonSize::Large, 28.0),
            (IconButtonSize::XLarge, 28.0),
        ] {
            assert_eq!(
                size.selected_container_radius(IconButtonShape::Round, 99.0),
                selected_round,
                "{size:?}'s SelectedContainerShapeRound"
            );
            assert_eq!(
                size.selected_container_radius(IconButtonShape::Square, 99.0),
                99.0,
                "{size:?}'s SelectedContainerShapeSquare is CornerFull, i.e. the round radius"
            );
        }
    }

    /// A focused IconButton carries `.clip(shape)` (the container must clip its children) and the focus
    /// ring is drawn OUTSIDE the container by `draw_focus`. Before the ring moved after the clip
    /// restore, the clip swallowed the whole ring and this sample came back pure white.
    #[test]
    fn a_focused_icon_button_paints_its_ring_outside_the_container_clip() {
        let (_guard, clock) = crate::animation::tests::serial_clock();
        crate::animation::clear_all_animations();
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let source = MutableInteractionSource::new();
        let mut composer = crate::runtime::composer::Composer::new();
        let src = source.clone();
        composer.compose(move |ctx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                crate::layout::Column::new()
                    .modifier(Modifier::new().fill_max_size().padding(20.0))
                    .build(ctx, |ctx| {
                        IconButton::new()
                            .interaction_source(src.clone())
                            .on_click(|| {})
                            .build(ctx, |ctx| {
                                crate::components::icon::Icon::svg_path("M12 2L22 12 12 22 2 12Z")
                                    .build(ctx);
                            });
                    });
            });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 100.0, 0.0, 100.0));
        // The button node is the 48 dp touch target at (20,20); its container is the 40 dp box inside
        // it, so the ring (2 px gap + a 3 px stroke) lands on x = 19..22 at the vertical centre.
        source.emit_focus();
        clock.advance(std::time::Duration::from_millis(250));
        crate::animation::update_animations();

        let mut surface = skia_safe::surfaces::raster_n32_premul((100, 100)).expect("surface");
        surface.canvas().clear(skia_safe::Color::WHITE);
        let root = composer.layout_root_idx().expect("laid out");
        crate::render::render(composer.arena_nodes(), root, surface.canvas());
        let pm = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        let at = |x: usize, y: usize| px[y * 100 + x];

        let ring = at(20, 44);
        let inside = at(28, 44);
        eprintln!("probe icon-button focus ring: outside {ring:?}, inside {inside:?}");
        // A clipped ring leaves pure WHITE here; the ring is the theme's primary, antialiased
        // against the background, so pin only that it is substantially darker than white.
        assert!(
            ring[0] < 230 && ring[1] < 230 && ring[2] < 230,
            "the focus ring is painted outside the container at x = 20: {ring:?}"
        );
        assert!(
            (inside[0] as i32 - inside[1] as i32).abs() <= 2
                && (inside[1] as i32 - inside[2] as i32).abs() <= 2,
            "inside the container is the neutral focus state layer, not the ring: {inside:?}"
        );
    }

    /// The shape morph runs on `MotionSchemeKeyTokens.DefaultEffects`, which upstream comments is
    /// "intentional here to prevent any bounce in this component" (`IconButton.kt:1558-1568`). A
    /// spatial spring overshoots by design, and at the expressive scheme's `fast_spatial`
    /// (damping 0.6, stiffness 800) that overshoot is large — this test is what stops the morph
    /// drifting back onto one.
    #[test]
    fn the_shape_morph_runs_on_default_effects() {
        match shape_morph_spec() {
            crate::animation::AnimationSpec::Spring(spec) => {
                assert_eq!(spec, crate::animation::SpringSpec::effects_default());
                assert_eq!(
                    (spec.damping_ratio, spec.stiffness),
                    (1.0, 1600.0),
                    "DefaultEffects is damping 1.0 / stiffness 1600 \
                     (tokens/ExpressiveMotionTokens.kt), so it cannot overshoot"
                );
            }
            other => panic!("the shape morph must be a spring, got {other:?}"),
        }
    }
