    use super::*;

    /// The node is grown to the 48 dp touch target while the container paints at the token size. Without
    /// `minimum_interactive_component_size` a Small IconToggleButton is 40 dp AND has no touch target at all
    /// — which is what happened when `IconButtonSize::container_size()` went 48 -> 40 and nothing here
    /// compensated. Deleting that line again now fails this test.
    #[test]
    fn the_node_keeps_the_48_dp_touch_target() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                IconToggleButton::new(false)
                    .on_checked_change(|_| {})
                    .build(ctx, |ctx| {
                        crate::components::icon::Icon::svg_path("M12 2L22 12 12 22 2 12Z")
                            .build(ctx);
                    });
            });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        // Both axes: the touch target is a 48 square, and folding only the width would miss a height left at
        // the token container's 40.
        let (widest, tallest) =
            composer
                .arena_nodes()
                .iter()
                .fold((0.0f32, 0.0f32), |(w, h), node| {
                    (
                        w.max(node.measured_size.width),
                        h.max(node.measured_size.height),
                    )
                });
        assert_eq!(
            (widest, tallest),
            (48.0, 48.0),
            "the node is the 48 dp touch target, with the token container painted inside it"
        );
    }

    /// The parity half of `IconButton`'s `.clip(shape)`: without it a toggle button's ripple and children
    /// spill past the container's corners.
    #[test]
    fn the_container_is_clipped_to_its_shape() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                IconToggleButton::new(false)
                    .on_checked_change(|_| {})
                    .build(ctx, |ctx| {
                        crate::components::icon::Icon::svg_path("M12 2L22 12 12 22 2 12Z")
                            .build(ctx);
                    });
            });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        let clipped = composer.arena_nodes().iter().any(|node| {
            node.modifier
                .elements()
                .iter()
                .any(|el| matches!(el, crate::modifier::ModifierElement::Clip { .. }))
        });
        assert!(
            clipped,
            "the container carries a clip, so its children cannot spill past the corners"
        );
    }

    /// The same press morph `IconButton` has. Round is CornerFull over the token container, so at rest the
    /// radius is half the container and a press interpolates down to that size's PressedContainerShape —
    /// deleting the morph leaves the radius at 20 and fails this test.
    #[test]
    fn a_press_morphs_the_container_shape_over_time() {
        // `update_animations` below drives the process-wide table, which `clear_all_animations` (200+ call
        // sites) empties for everyone — so this holds the same lock the animation tests take.
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let source = MutableInteractionSource::new();
        let src = source.clone();
        let mut composer = crate::runtime::composer::Composer::new();
        let mut build = |composer: &mut crate::runtime::composer::Composer| {
            composer.compose(|ctx| {
                WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    IconToggleButton::new(false)
                        .on_checked_change(|_| {})
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
        // Round over the token container: the paint is inset back to 40 dp inside the 48 dp touch target.
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

    /// Switching `checked` fades the container rather than snapping. `Filled` is the style this test has
    /// to use: its two container colours actually differ (`surfaceContainer` -> `primary`), while the
    /// Standard one is transparent in both states. Deleting the `animate_color_as_state` call leaves the
    /// colour at the checked value on the first frame and fails the `assert_ne!` below.
    #[test]
    fn a_checked_switch_fades_the_container_over_time() {
        // `update_animations` drives the process-wide table, so this takes the same lock the other
        // animation tests hold.
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let checked = crate::runtime::state::State::new(false);
        let mut composer = crate::runtime::composer::Composer::new();
        let mut build = |composer: &mut crate::runtime::composer::Composer| {
            let on = checked.get();
            composer.compose(|ctx| {
                WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    IconToggleButton::filled(on)
                        .on_checked_change(|_| {})
                        .build(ctx, |ctx| {
                            crate::components::icon::Icon::svg_path("M12 2L22 12 12 22 2 12Z")
                                .build(ctx);
                        });
                });
            });
            composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        };
        let container = |composer: &crate::runtime::composer::Composer| -> Color {
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
                .expect("the container paints a background")
        };
        // A spring settles within its threshold rather than exactly on the target, so compare per channel.
        let near = |got: Color, want: Color| {
            (got.r as i32 - want.r as i32).abs() <= 2
                && (got.g as i32 - want.g as i32).abs() <= 2
                && (got.b as i32 - want.b as i32).abs() <= 2
        };

        build(&mut composer);
        let colors = IconToggleButtonDefaults::colors_for(&theme, IconButtonStyle::Filled);
        let unchecked = colors.container_color(true, false);
        let target = colors.container_color(true, true);
        assert_ne!(
            unchecked, target,
            "this test needs a style whose two container colours differ"
        );
        assert_eq!(
            container(&composer),
            unchecked,
            "at rest the container is the unchecked colour"
        );

        checked.update(|c| *c = true);
        build(&mut composer);
        // The fade must be REGISTERED and must not have landed yet: `assert_ne!` alone would also pass if
        // no animation had started and the paint were simply stale.
        assert!(
            crate::animation::is_animating(),
            "switching checked registers the colour fade"
        );
        assert!(
            !near(container(&composer), target),
            "the colour does not jump to the checked value, got {:?}",
            container(&composer)
        );

        for _ in 0..600 {
            if !crate::animation::update_animations() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
            build(&mut composer);
        }
        assert!(
            near(container(&composer), target),
            "and the fade arrives at the checked colour, got {:?}",
            container(&composer)
        );
    }

    /// The documented way to fade the ICON as well as the colour: `content` is a `FnOnce`, so the caller
    /// wraps the glyph in `Crossfade` (see `docs/icon.md`, and the four buttons in `icon_demo`). What this
    /// pins is that the two compose: `IconToggleButton` opens a restartable group, and the crossfade has to
    /// stay functional inside it — the outgoing generation must remain on screen next to the incoming one.
    #[test]
    fn the_icon_cross_fades_with_the_checked_state() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let checked = crate::runtime::state::State::new(false);
        let mut composer = crate::runtime::composer::Composer::new();
        let mut build = |composer: &mut crate::runtime::composer::Composer| {
            composer.compose(|ctx| {
                WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    IconToggleButton::new(checked.get())
                        .on_checked_change(|_| {})
                        .build(ctx, |ctx| {
                            crate::components::Crossfade::new(checked.clone())
                                .build(ctx, |ctx, on| {
                                    let path = if on { "M0 0h1v1z" } else { "M0 0h2v2z" };
                                    crate::components::icon::Icon::svg_path(path).build(ctx);
                                });
                        });
                });
            });
            composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        };
        // Each generation draws its own icon, so counting them counts the generations on screen.
        let icons = |composer: &crate::runtime::composer::Composer| -> usize {
            composer
                .arena_nodes()
                .iter()
                .filter(|node| {
                    node.modifier
                        .elements()
                        .iter()
                        .any(|el| matches!(el, crate::modifier::ModifierElement::DrawIcon { .. }))
                })
                .count()
        };

        build(&mut composer);
        assert_eq!(icons(&composer), 1, "one generation while nothing is changing");

        checked.update(|c| *c = true);
        build(&mut composer);
        assert_eq!(
            icons(&composer),
            2,
            "the outgoing icon stays on screen while the incoming one fades in"
        );
    }

    /// The colour must survive the cross-fade wrapper: the content colour this button provides has to reach
    /// a glyph built INSIDE a `Crossfade` generation, or every icon in the fade paints in the wrong colour.
    #[test]
    fn the_icon_keeps_its_content_colour_inside_the_crossfade() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let checked = crate::runtime::state::State::new(true);
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                IconToggleButton::filled(checked.get())
                    .on_checked_change(|_| {})
                    .build(ctx, |ctx| {
                        crate::components::Crossfade::new(checked.clone()).build(ctx, |ctx, on| {
                            let path = if on { "M0 0h1v1z" } else { "M0 0h2v2z" };
                            crate::components::icon::Icon::svg_path(path).build(ctx);
                        });
                    });
            });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        let mut tints = Vec::new();
        for node in composer.arena_nodes() {
            for el in node.modifier.elements() {
                if let crate::modifier::ModifierElement::DrawIcon { spec } = el {
                    tints.push(spec.tint.as_ref().map(|t| t.eval()));
                }
            }
        }
        assert!(
            !tints.is_empty(),
            "the icon should be built inside the crossfade"
        );
        for tint in &tints {
            assert_eq!(
                *tint,
                Some(theme.on_primary),
                "every generation's icon takes the checked content colour"
            );
        }
    }

    /// The checked fade and the shape morph are PAINT-time values: after the switch itself has
    /// recomposed the button once, ticking the animations must not enter a single slot.
    #[test]
    fn the_checked_fade_does_not_recompose_the_button() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let checked = crate::runtime::state::State::new(false);
        let mut composer = crate::runtime::composer::Composer::new();
        let mut build = |composer: &mut crate::runtime::composer::Composer| {
            composer.compose(|ctx| {
                WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    IconToggleButton::filled(checked.get())
                        .on_checked_change(|_| {})
                        .build(ctx, |ctx| {
                            crate::components::icon::Icon::svg_path("M0 0h1v1z").build(ctx);
                        });
                });
            });
            composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        };

        build(&mut composer);
        build(&mut composer);
        let baseline_entered = composer.compose_entered_count;
        assert_eq!(composer.compose_dirty_count, 0, "a settled frame dirties nothing");
        build(&mut composer);
        assert_eq!(
            (composer.compose_entered_count, composer.compose_dirty_count),
            (baseline_entered, 0),
            "the settled baseline must be stable"
        );

        // The switch retargets the container fade, the ripple colour and the shape morph. The BODY runs
        // every build (the parent composes it), and the group may legitimately Skip because those
        // values are carried as Dyns and the new modifier is applied on Skip. The probe is therefore
        // the ticks below: they must start an animation and enter no group beyond the provider baseline.
        checked.update(|c| *c = true);
        build(&mut composer);
        build(&mut composer);
        assert!(
            crate::animation::is_animating(),
            "the switch must leave an animation running, or the ticks below prove nothing"
        );

        let (mut worst_entered, mut worst_dirty, mut saw_animation) = (0, 0, false);
        for _ in 0..30 {
            saw_animation |= crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(2));
            build(&mut composer);
            worst_entered = worst_entered.max(composer.compose_entered_count);
            worst_dirty = worst_dirty.max(composer.compose_dirty_count);
        }
        assert!(saw_animation, "the ticks must actually advance an animation");
        assert_eq!(
            (worst_entered, worst_dirty),
            (baseline_entered, 0),
            "the container fade, the ripple colour and the shape morph are painted, not composed \
             (worst frame: entered {worst_entered}, dirty {worst_dirty})"
        );
        crate::animation::clear_all_animations();
    }

    /// Switching `checked` must move the glyph to the checked colour. This is the case the "starts checked"
    /// test above cannot see: the content colour is ANIMATED, and a `Crossfade` generation only rebuilds its
    /// content when its own slot parameters change — so a generation built on the first frame of that
    /// animation can capture the colour it is leaving and keep it forever.
    #[test]
    fn the_glyph_follows_the_content_colour_after_a_checked_switch() {
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let checked = crate::runtime::state::State::new(false);
        let mut composer = crate::runtime::composer::Composer::new();
        let mut build = |composer: &mut crate::runtime::composer::Composer| {
            composer.compose(|ctx| {
                WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    IconToggleButton::filled(checked.get())
                        .on_checked_change(|_| {})
                        .build(ctx, |ctx| {
                            crate::components::Crossfade::new(checked.clone()).build(
                                ctx,
                                |ctx, on| {
                                    let path = if on { "M0 0h1v1z" } else { "M0 0h2v2z" };
                                    crate::components::icon::Icon::svg_path(path).build(ctx);
                                },
                            );
                        });
                });
            });
            composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        };
        let tints = |composer: &crate::runtime::composer::Composer| -> Vec<Option<Color>> {
            composer
                .arena_nodes()
                .iter()
                .flat_map(|node| {
                    node.modifier.elements().iter().filter_map(|el| match el {
                        crate::modifier::ModifierElement::DrawIcon { spec } => {
                            Some(spec.tint.as_ref().map(|t| t.eval()))
                        }
                        _ => None,
                    })
                })
                .collect()
        };

        let colors = IconToggleButtonDefaults::colors_for(&theme, IconButtonStyle::Filled);
        let unchecked = colors.content_color(true, false);
        let target = colors.content_color(true, true);

        build(&mut composer);
        assert_eq!(
            tints(&composer),
            vec![Some(unchecked)],
            "an unchecked button paints the unchecked content colour"
        );

        checked.update(|c| *c = true);
        build(&mut composer);
        for _ in 0..600 {
            if !crate::animation::update_animations() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
            build(&mut composer);
        }
        assert_eq!(
            tints(&composer),
            vec![Some(target)],
            "once the switch settles only the checked generation is left, in the checked colour"
        );
    }

    #[test]
    fn color_state_resolution() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let colors = IconToggleButtonDefaults::colors_for(&theme, IconButtonStyle::Filled);
        assert_eq!(colors.container_color(true, false), colors.container);
        assert_eq!(colors.content_color(true, false), colors.content);
        assert_eq!(colors.container_color(true, true), theme.primary, "checked 容器 = primary");
        assert_eq!(colors.content_color(true, true), theme.on_primary);
        // disabled 优先于 checked
        assert_eq!(colors.container_color(false, true), colors.disabled_container);
        assert_eq!(colors.content_color(false, true), colors.disabled_content);
        // 标准版 checked 内容 = primary（M3 SelectedColor）
        let s = IconToggleButtonDefaults::colors_for(&theme, IconButtonStyle::Standard);
        assert_eq!(s.content_color(true, true), theme.primary);
        // disabled 色值本身：OnSurface 10%/38%（防与 IconButton 漂移）
        assert_eq!(s.disabled_content.a, (theme.on_surface.a as f32 * 0.38) as u8);
        let f2 = IconToggleButtonDefaults::colors_for(&theme, IconButtonStyle::Filled);
        assert_eq!(f2.disabled_container.a, (theme.on_surface.a as f32 * 0.10) as u8);
        // Filled 未选 = surfaceContainer/onSurfaceVariant
        let f = IconToggleButtonDefaults::colors_for(&theme, IconButtonStyle::Filled);
        assert_eq!(f.container, theme.surface_container);
        assert_eq!(f.content, theme.on_surface_variant);
        // Tonal 选中 = secondary/onSecondary（14_1_0 token）
        let t = IconToggleButtonDefaults::colors_for(&theme, IconButtonStyle::FilledTonal);
        assert_eq!(t.checked_container, theme.secondary);
        assert_eq!(t.checked_content, theme.on_secondary);
        // Outlined 选中 = inverseSurface/inverseOnSurface
        let o = IconToggleButtonDefaults::colors_for(&theme, IconButtonStyle::Outlined);
        assert_eq!(o.checked_container, theme.inverse_surface);
        assert_eq!(o.checked_content, theme.inverse_on_surface);
    }

    #[test]
    fn outlined_border_hidden_when_checked() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        assert!(IconToggleButtonDefaults::outlined_border(&theme, true, false, 1.0).is_some());
        assert_eq!(
            IconToggleButtonDefaults::outlined_border(&theme, true, false, 1.0).unwrap().color,
            theme.outline_variant
        );
        assert!(IconToggleButtonDefaults::outlined_border(&theme, true, true, 1.0).is_none(), "checked 无边框");
        let disabled = IconToggleButtonDefaults::outlined_border(&theme, false, false, 1.0).unwrap();
        assert_eq!(disabled.color.a, (theme.outline_variant.a as f32 * 0.38) as u8);
        assert_eq!(
            IconToggleButtonDefaults::outlined_border(&theme, true, false, IconButtonSize::Large.outline_width())
                .unwrap()
                .width,
            2.0
        );
    }

    #[test]
    fn constructors() {
        assert!(IconToggleButton::new(true).get_checked());
        assert!(!IconToggleButton::new(false).get_checked());
        assert_eq!(IconToggleButton::filled(true).get_style(), IconButtonStyle::Filled);
        assert_eq!(IconToggleButton::filled_tonal(true).get_style(), IconButtonStyle::FilledTonal);
        assert_eq!(IconToggleButton::outlined(true).get_style(), IconButtonStyle::Outlined);
        assert_eq!(IconToggleButton::new(true).checked(false).get_checked(), false);
        assert_eq!(IconToggleButton::new(true).get_size(), IconButtonSize::Small);
        assert_eq!(
            IconToggleButton::new(true).size(IconButtonSize::XLarge).get_size(),
            IconButtonSize::XLarge
        );
        // 尺寸表与 IconButton 共用同一 token 表
        assert_eq!(IconToggleButton::new(true).size(IconButtonSize::XSmall).get_size().container_size(), 32.0);
        assert_eq!(IconToggleButton::new(true).size(IconButtonSize::Large).get_size().icon_size(), 32.0);
    }

    #[test]
    fn checked_content_color_propagates_to_icon() {
        // 集成：Filled checked 内 Icon（tint Auto）解析为 on_primary
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                IconToggleButton::filled(true)
                    .on_checked_change(|_| {})
                    .build(ctx, |ctx| {
                        crate::components::icon::Icon::svg_path("M12 2L22 12 12 22 2 12Z").build(ctx);
                    });
            });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        let mut tint = None;
        for node in composer.arena_nodes() {
            for el in node.modifier.elements() {
                if let crate::modifier::ModifierElement::DrawIcon { spec } = el {
                    tint = spec.tint.as_ref().map(|t| t.eval());
                }
            }
        }
        assert_eq!(tint, Some(theme.on_primary), "checked 内容色下传 Icon tint");
    }

    #[test]
    fn disabled_toggle_has_no_interaction_elements() {
        use crate::modifier::ModifierElement;
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            IconToggleButton::new(false)
                .enabled(false)
                .on_checked_change(|_| {})
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
                    "禁用 Toggle 不应有波纹/可聚焦元素: {el:?}"
                );
            }
        }
    }

    #[test]
    fn outlined_checked_has_no_border_element() {
        use crate::modifier::ModifierElement;
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            IconToggleButton::outlined(true)
                .on_checked_change(|_| {})
                .build(ctx, |ctx| {
                    crate::components::icon::Icon::svg_path("M12 2L22 12 12 22 2 12Z").build(ctx);
                });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        for node in composer.arena_nodes() {
            for el in node.modifier.elements() {
                assert!(
                    !matches!(el, ModifierElement::Border { .. }),
                    "Outlined checked 节点不应有 Border 元素: {el:?}"
                );
            }
        }
    }

    #[test]
    fn toggle_callback_receives_inverted_checked() {
        use crate::modifier::ModifierElement;
        use std::sync::atomic::{AtomicBool, Ordering};
        // 未选中 → 点击回调应收到 true（取反）
        for (checked, expect) in [(false, true), (true, false)] {
            let received = Arc::new(AtomicBool::new(false));
            let cb = received.clone();
            let mut composer = crate::runtime::composer::Composer::new();
            composer.compose(|ctx| {
                IconToggleButton::new(checked)
                    .on_checked_change(move |v| cb.store(v, Ordering::Relaxed))
                    .build(ctx, |ctx| {
                        crate::components::icon::Icon::svg_path("M12 2L22 12 12 22 2 12Z").build(ctx);
                    });
            });
            composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
            let mut clicked = false;
            for node in composer.arena_nodes() {
                for el in node.modifier.elements() {
                    if let ModifierElement::Clickable { on_click, .. } = el {
                        on_click();
                        clicked = true;
                    }
                }
            }
            assert!(clicked, "存在 Clickable");
            assert_eq!(received.load(Ordering::Relaxed), expect, "回调收到 !checked");
        }
    }

    /// A CHECKED toggle is not the resting circle. Upstream's default toggle shapes are
    /// (`IconButtonDefaults.kt:1046-1055`)
    ///
    ///     shape        = SmallIconButtonTokens.ContainerShapeRound         (CornerFull)
    ///     pressedShape = SmallIconButtonTokens.PressedContainerShape       (CornerSmall)
    ///     checkedShape = SmallIconButtonTokens.SelectedContainerShapeRound (CornerMedium)
    ///
    /// and `shapeByInteraction` picks PRESSED > CHECKED > resting (`IconButton.kt:1622-1638`). The
    /// `SelectedContainerShape*` tokens went unused here, so a checked toggle kept its resting
    /// circle; deleting the checked branch fails this test.
    #[test]
    fn a_checked_toggle_morphs_to_the_selected_container_shape() {
        // `update_animations` drives the process-wide table, so this holds the same lock the other
        // animation tests take.
        let _guard = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let checked = crate::runtime::state::State::new(false);
        let mut composer = crate::runtime::composer::Composer::new();
        let mut build = |composer: &mut crate::runtime::composer::Composer| {
            let on = checked.get();
            composer.compose(|ctx| {
                WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    IconToggleButton::new(on)
                        .on_checked_change(|_| {})
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
        assert_eq!(
            radius(&composer),
            20.0,
            "unchecked, Small's container is CornerFull over its 40 dp box"
        );

        checked.set(true);
        build(&mut composer);
        // The morph must be REGISTERED, and the corners must not have switched yet — the same shape
        // the press test above takes, and the reason its `build` sits before this loop: with
        // nothing animating, `update_animations` returns false on the first call and the loop would
        // exit before the build that starts the morph ever ran.
        assert!(
            crate::animation::is_animating(),
            "switching checked registers the shape morph"
        );
        assert_ne!(
            radius(&composer),
            12.0,
            "the checked shape arrives over time instead of switching"
        );
        for _ in 0..600 {
            if !crate::animation::update_animations() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
            build(&mut composer);
        }
        assert!(
            (radius(&composer) - 12.0).abs() < 0.5,
            "checked, it arrives at Small's SelectedContainerShapeRound (CornerMedium = 12), got {}",
            radius(&composer)
        );
    }
