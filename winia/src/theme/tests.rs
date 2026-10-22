    use super::*;

    /// A `surface`-filled 40×40 leaf under a param-less wrapper group — the smallest shape that shows both
    /// whether a theme reached the tree and whether an unmarked wrapper kept the colors it composed with.
    fn surface_scene(ctx: &mut crate::runtime::composer::ComposeCtx) {
        use crate::runtime::composer::GroupStatus;
        use crate::layout::BoxLayout;
        use crate::modifier::{Modifier};
        use crate::graphics::{Shape};
        let key = ctx.next_key();
        match ctx.start_restartable_group(key, Modifier::new(), BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                let leaf = ctx.next_key();
                ctx.start_leaf(
                    leaf,
                    Modifier::new()
                        .size(40.0, 40.0)
                        .background(WiniaTheme::colors().surface, Shape::Rectangle),
                );
                ctx.end_node();
            }
        }
        ctx.end_restartable_group();
    }

    /// B channel of the centre pixel of `surface_scene` drawn into a 40×40 raster (BGRA).
    fn centre_pixel(composer: &mut crate::runtime::composer::Composer) -> i32 {
        use crate::layout::constraints::Constraints;
        composer.layout(Constraints::new(0.0, 40.0, 0.0, 40.0));
        let mut surface = skia_safe::surfaces::raster_n32_premul((40, 40)).unwrap();
        surface.canvas().clear(skia_safe::Color::WHITE);
        let root = composer.layout_root_idx().expect("root");
        crate::render::render(composer.arena_nodes(), root, surface.canvas());
        let pm = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        px[20 * 40 + 20][0] as i32
    }

    /// A theme change carries itself into an idle subtree.
    ///
    /// The provider compares what it is about to provide with the previous frame's and dirties its own
    /// subtree when they differ, so a param-less wrapper between the theme and the content re-runs and
    /// picks the new palette up. Before that, the same shape kept the colors it composed with until
    /// something outside told the composer (`mark_content_dirty`, which `PerWindow::refresh_theme`
    /// performs) — the SearchBar filtering bug had exactly that shape, and this test used to assert the
    /// stale behavior as the contract.
    #[test]
    fn a_theme_change_reaches_an_idle_subtree_by_itself() {
        let _serial = THEME_TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        use crate::runtime::composer::Composer;

        let scene = |ctx: &mut crate::runtime::composer::ComposeCtx| {
            WiniaTheme::auto(ctx, surface_scene);
        };

        set_system_dark_mode(Some(false));
        let mut composer = Composer::new();
        composer.compose(scene);
        let light = centre_pixel(&mut composer);

        // The system mode moves. Nothing external tells the tree about it, and the subtree is idle: the
        // provider itself has to carry the new palette in.
        set_system_dark_mode(Some(true));
        composer.recompose(scene);
        assert_ne!(
            centre_pixel(&mut composer),
            light,
            "the provider must reach an idle subtree on its own"
        );

        // The window-level step is still what SCHEDULES the frame (`needs_recomposition`); it must stay
        // harmless now that the provider propagates too.
        composer.mark_content_dirty();
        composer.recompose(scene);
        assert_ne!(centre_pixel(&mut composer), light, "and the external dirtying still works");

        // Hand the mode back: it is process-global.
        set_system_dark_mode(None);
    }

    /// A reader that changed nothing about ITSELF still has to pick up a new local value.
    ///
    /// This is the gap `CompositionLocal::provides` documents: the local is a stack, and a reader only
    /// re-reads when its own group is re-entered. A param-less wrapper declares nothing, so once the
    /// provider resolves a NEW direction/theme it would otherwise keep the old one — the components
    /// inside compiled their padding, alignment and colors from it. The provider closes that by
    /// dirtying its own subtree when the resolved values differ from the previous frame's.
    #[test]
    fn a_local_change_reaches_a_reader_that_declared_nothing() {
        let _serial = THEME_TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        use crate::runtime::composer::{Composer, ComposeCtx, GroupStatus};
        use crate::layout::BoxLayout;
        use crate::layout::LayoutDirection;
        use crate::modifier::Modifier;
        use crate::components::text::Text;

        // Records what the reader actually built with, on every run — and every SKIP, so a test can
        // tell "the reader re-read the local" from "the reader never skipped in the first place".
        fn reader(
            ctx: &mut ComposeCtx,
            seen: &std::rc::Rc<std::cell::RefCell<Vec<LayoutDirection>>>,
            skips: &std::rc::Rc<std::cell::Cell<usize>>,
        ) {
            let key = ctx.next_key();
            // A param-less wrapper: it declares nothing, so nothing about IT changes between frames.
            match ctx.start_restartable_group(key, Modifier::new(), BoxLayout::new()) {
                GroupStatus::Skip => { skips.set(skips.get() + 1); }
                GroupStatus::Enter => {
                    seen.borrow_mut().push(crate::layout::direction::current());
                    let leaf = ctx.next_key();
                    let dir = if crate::layout::direction::current() == LayoutDirection::Rtl { "rtl" } else { "ltr" };
                    Text::new(dir).build(ctx);
                    let _ = leaf;
                }
            }
            ctx.end_restartable_group();
        }

        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let seen_in = seen.clone();
        let skips = std::rc::Rc::new(std::cell::Cell::new(0usize));
        let skips_in = skips.clone();
        let direction = crate::State::new(LayoutDirection::Ltr);
        let direction_in = direction.clone();

        let scene: Box<dyn Fn(&mut ComposeCtx)> = Box::new(move |ctx: &mut ComposeCtx| {
            let dir = direction_in.get();
            WiniaTheme::with_theme_typography_and_direction(
                ThemeColors::default_light(),
                Typography::default(),
                dir,
                ctx,
                |ctx| reader(ctx, &seen_in, &skips_in),
            );
        });

        // A frame is compose → layout, and this helper is what keeps the two halves in that order. A
        // group can only Skip when the reuse index holds a node for its key — and while the index is
        // rebuilt by `layout`, a compose that finds it empty repairs it from the tree first, so a test
        // that composes twice in a row now Skips too (it restores nodes that were never measured,
        // which is not the shape this test is about). The CONTROL below is what catches a vacuous
        // "the reader kept the old value".
        let mut frame = |composer: &mut Composer, scene: &dyn Fn(&mut ComposeCtx)| {
            composer.compose(|ctx| scene(ctx));
            composer.layout(crate::layout::constraints::Constraints::new(0.0, 200.0, 0.0, 200.0));
        };
        let mut composer = Composer::new();
        frame(&mut composer, &scene);
        assert_eq!(seen.borrow().as_slice(), [LayoutDirection::Ltr], "first frame reads the provided value");

        // CONTROL: does this group skip AT ALL when nothing changed? If it never skips, a test built on
        // it cannot see the local-invalidation bug and proves nothing.
        frame(&mut composer, &scene);
        assert_eq!(seen.borrow().len(), 1, "CONTROL: an unchanged frame must Skip the reader");
        assert!(skips.get() >= 1, "CONTROL: the group skipped");

        // Swap the DIRECTION only. The reader itself declares nothing, so its group is Skippable.
        direction.set(LayoutDirection::Rtl);
        frame(&mut composer, &scene);
        assert_eq!(
            seen.borrow().as_slice(),
            [LayoutDirection::Ltr, LayoutDirection::Rtl],
            "a reader inside the provider must re-read after the provided value changed"
        );
    }


    /// The shape of a WINDOW, end to end at the composer level: the declaring tree samples the intent and
    /// PUBLISHES it into the window's cell, the window's own composer composes under the cell's palette,
    /// and a change reaches the tree by two independent routes.
    ///
    /// Both routes are load-bearing. A palette captured as a value pinned the window to its startup colors
    /// (measured on the demo: an `auto` window stayed dark while the surface behind it flipped to light),
    /// and an intent sampled once pinned it to its startup NODE (a sub-window whose declaring tree switched
    /// from light to dark kept its palette forever).
    #[test]
    fn a_window_content_follows_its_cell() {
        let _serial = THEME_TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        use crate::runtime::composer::Composer;

        set_system_dark_mode(Some(false));
        // What `Window::build` samples, while the application's own theme node is in scope.
        let sampled = std::cell::RefCell::new(None);
        let mut init = Composer::new();
        init.compose(|ctx| {
            WiniaTheme::auto(ctx, |_| {
                *sampled.borrow_mut() = Some(WindowTheme::new(current_theme_spec()));
            })
        });
        let cell = sampled.into_inner().expect("the window samples a theme");
        assert_eq!(cell.spec(), ThemeSpec::Auto, "a window inside `auto` must keep following the system");
        let mut drawn = cell.applied();

        // Each frame after that: the window's own composer runs the content under the cell's palette.
        let mut composer = Composer::new();
        let mut frame = |composer: &mut Composer| {
            composer.compose(|ctx| cell.provide(ctx, surface_scene));
        };
        frame(&mut composer);
        let light = centre_pixel(&mut composer);

        // (1) The system mode moves, and the loop refreshes this window's palette.
        set_system_dark_mode(Some(true));
        assert!(cell.refresh(&mut drawn), "the system theme moved");
        composer.mark_content_dirty();
        frame(&mut composer);
        assert_ne!(centre_pixel(&mut composer), light, "the window's tree must follow the system");
        let dark = centre_pixel(&mut composer);

        // (2) The DECLARING tree publishes another intent (it switched its own theme node). The window
        // composes in a different composer, so without the cell it would never hear about it.
        let plain = ThemeSpec::Fixed(ThemeColors::default_light());
        assert!(cell.publish(plain, Typography::default(), LayoutDirection::Ltr), "the intent moved");
        assert!(cell.refresh(&mut drawn), "the published palette is not what is drawn");
        composer.mark_content_dirty();
        frame(&mut composer);
        let after = centre_pixel(&mut composer);
        assert_ne!(after, dark, "a published intent has to reach the window too");
        assert_eq!(after, light, "it is the light palette the tree published");

        set_system_dark_mode(None);
    }

    /// A window carries the TYPOGRAPHY and DIRECTION in force where it was declared, not just the palette.
    ///
    /// The per-frame wrapper used to provide `Typography::default()` and LTR unconditionally, so an
    /// application that set a type scale (or an RTL direction) around its `Window` node kept it for exactly
    /// one frame — and nothing would have reported it: the window looks fine, just default.
    #[test]
    fn a_window_content_follows_published_typography_and_direction() {
        let _serial = THEME_TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        use crate::runtime::composer::Composer;

        let custom = Typography { body_large: TextStyle::new().font_size(23.0), ..Typography::default() };
        // What `Window::build` samples, from inside the theme nodes that wrap it.
        let sampled = std::cell::RefCell::new(None);
        let mut init = Composer::new();
        init.compose(|ctx| {
            WiniaTheme::with_theme_and_direction(ThemeColors::default_light(), LayoutDirection::Rtl, ctx, |ctx| {
                WiniaTheme::with_typography(custom.clone(), ctx, |ctx| {
                    *sampled.borrow_mut() = Some(WindowTheme::new(current_theme_spec()));
                    let cell = sampled.borrow();
                    cell.as_ref()
                        .unwrap()
                        .publish(current_theme_spec(), WiniaTheme::typography(), crate::layout::direction::current());
                })
            })
        });
        let cell = sampled.into_inner().expect("the window samples a theme");
        let mut drawn = cell.applied();

        // Each frame: the window's own composer composes under the cell's theme. What a component would read
        // while building has to be the published type scale and direction.
        let seen = std::cell::RefCell::new(Vec::new());
        let mut composer = Composer::new();
        let mut frame = |composer: &mut Composer| {
            composer.compose(|ctx| {
                cell.provide(ctx, |_| {
                    seen.borrow_mut().push((WiniaTheme::typography(), crate::layout::direction::current()));
                })
            });
        };
        frame(&mut composer);
        let (typography, direction) = seen.borrow()[0].clone();
        assert_eq!(typography, custom, "the declared type scale has to reach the window's content");
        assert_eq!(direction, LayoutDirection::Rtl, "and so does the direction");

        // Published changes are what the frame step exists for: they have to read as "what was drawn is now
        // wrong", even though the palette did not move.
        let other = Typography { body_large: TextStyle::new().font_size(31.0), ..Typography::default() };
        assert!(cell.publish(current_theme_spec(), other.clone(), LayoutDirection::Ltr), "the type scale moved");
        assert!(cell.refresh(&mut drawn), "a type-scale change has to re-run the tree");
        frame(&mut composer);
        let (typography, direction) = seen.borrow()[1].clone();
        assert_eq!(typography, other);
        assert_eq!(direction, LayoutDirection::Ltr);
        assert!(!cell.refresh(&mut drawn), "and then there is nothing left to do");

        // A DIRECTION-only change is the other half of that comparison: same palette, same type scale, and
        // the tree still has to run again (a Row mirrors, text aligns the other way). Without it, dropping
        // the direction from the comparison would pass every other test in this file.
        assert!(cell.publish(current_theme_spec(), other.clone(), LayoutDirection::Rtl), "the direction moved");
        assert!(cell.refresh(&mut drawn), "a direction change has to re-run the tree");
        frame(&mut composer);
        assert_eq!(seen.borrow()[2].1, LayoutDirection::Rtl, "and the content sees it");
    }

    /// `Auto` resolves on every call; `Fixed` answers with the palette it was built from, whatever the mode
    /// says — an application that pinned a theme must not be dragged by the system.
    #[test]
    fn theme_spec_resolves_auto_per_call_and_fixed_never() {
        let _serial = THEME_TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let fixed = ThemeSpec::Fixed(ThemeColors::default_light());
        // Outside any theme node a caller gets the palette in force, never a follow-the-system spec.
        assert!(matches!(current_theme_spec(), ThemeSpec::Fixed(_)));

        set_system_dark_mode(Some(true));
        assert_eq!(ThemeSpec::Auto.colors().background, ThemeColors::default_dark().background);
        assert_eq!(fixed.colors().background, ThemeColors::default_light().background);

        set_system_dark_mode(Some(false));
        assert_eq!(ThemeSpec::Auto.colors().background, ThemeColors::default_light().background);
        assert_eq!(fixed.colors().background, ThemeColors::default_light().background);
        set_system_dark_mode(None);
    }

    /// A theme provide records its spec for the duration of its content only — a window that samples it
    /// deeper in the tree gets the INNERMOST one, and the enclosing composition's spec comes back when the
    /// provide ends.
    #[test]
    fn a_theme_provide_records_its_spec_for_its_content_only() {
        let mut composer = crate::runtime::composer::Composer::new();
        let inside = std::cell::RefCell::new(None);
        let after = std::cell::RefCell::new(None);
        composer.compose(|ctx| {
            WiniaTheme::auto(ctx, |ctx| {
                // `dark`, not `light`: outside every theme node the answer is already `Fixed(default_light)`,
                // so a lighter inner theme could not tell "the inner provide was recorded" from "no
                // thread-local was ever written at all".
                WiniaTheme::dark(ctx, |_| {
                    *inside.borrow_mut() = Some(current_theme_spec());
                });
                *after.borrow_mut() = Some(current_theme_spec());
            });
        });
        let inside = inside.into_inner().expect("the inner provide recorded its spec");
        assert_eq!(inside, ThemeSpec::Fixed(ThemeColors::default_dark()), "the INNERMOST theme wins");
        assert_eq!(after.into_inner(), Some(ThemeSpec::Auto), "and the enclosing one comes back");
    }

    use crate::components::text::Text;

    #[test]
    fn material3_typography_matches_list_item_tokens() {
        let typography = Typography::default();
        assert_eq!(typography.body_large.font_size, Some(TextUnit::Sp(Sp(16.0))));
        assert_eq!(typography.body_large.line_height, Some(TextUnit::Sp(crate::unit::Sp(24.0))));
        assert_eq!(typography.body_large.letter_spacing, Some(0.5));
        assert_eq!(typography.body_medium.font_size, Some(TextUnit::Sp(Sp(14.0))));
        assert_eq!(typography.label_small.font_size, Some(TextUnit::Sp(Sp(11.0))));
        assert_eq!(typography.label_small.font_weight, Some(FontWeight::MEDIUM));
    }

    #[test]
    fn typography_local_nests_and_restores() {
        let custom = Typography {
            body_large: TextStyle::new().font_size(20.0),
            ..Typography::default()
        };
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            let default = WiniaTheme::typography();
            WiniaTheme::with_typography(custom.clone(), ctx, |ctx| {
                assert_eq!(WiniaTheme::typography(), custom);
            });
            assert_eq!(WiniaTheme::typography(), default);
        });
    }

    /// The two schemes are Compose's own token files, number for number: `StandardMotionTokens.kt:20-31`
    /// and `ExpressiveMotionTokens.kt:22-33`. The effect springs are the same in both — only the spatial
    /// tokens differ, which is why switching the scheme cannot change a colour animation.
    #[test]
    fn the_motion_schemes_are_comp_oses_two_token_sets() {
        let standard = MotionScheme::standard();
        for (token, damping, stiffness) in [
            (&standard.default_spatial, 0.9, 700.0),
            (&standard.fast_spatial, 0.9, 1400.0),
            (&standard.slow_spatial, 0.9, 300.0),
            (&standard.default_effects, 1.0, 1600.0),
            (&standard.fast_effects, 1.0, 3800.0),
            (&standard.slow_effects, 1.0, 800.0),
        ] {
            assert_eq!(
                (token.damping_ratio, token.stiffness),
                (damping, stiffness),
                "StandardMotionTokens"
            );
            assert_eq!(token.mass, 1.0);
        }

        let expressive = MotionScheme::expressive();
        for (token, damping, stiffness) in [
            (&expressive.default_spatial, 0.8, 380.0),
            (&expressive.fast_spatial, 0.6, 800.0),
            (&expressive.slow_spatial, 0.8, 200.0),
            (&expressive.default_effects, 1.0, 1600.0),
            (&expressive.fast_effects, 1.0, 3800.0),
            (&expressive.slow_effects, 1.0, 800.0),
        ] {
            assert_eq!(
                (token.damping_ratio, token.stiffness),
                (damping, stiffness),
                "ExpressiveMotionTokens"
            );
        }

        for (a, b) in [
            (&expressive.default_effects, &standard.default_effects),
            (&expressive.fast_effects, &standard.fast_effects),
            (&expressive.slow_effects, &standard.slow_effects),
        ] {
            assert_eq!(
                (a.damping_ratio, a.stiffness),
                (b.damping_ratio, b.stiffness),
                "the effect springs do not move between the two schemes"
            );
        }
        assert_eq!(MotionScheme::default(), MotionScheme::standard());
    }

    /// A component asks for a ROLE, and the theme decides which spring that is: under expressive motion
    /// `SpringSpec::spatial_fast()` (the spec a radio dot, a switch thumb and a segmented slot use) is the
    /// expressive token, and outside the override it is the standard one. Before the scheme existed both
    /// were the same constant, so this test could not have two different answers.
    #[test]
    fn a_component_takes_its_spring_from_the_motion_scheme() {
        use crate::animation::SpringSpec;
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            let standard = SpringSpec::spatial_fast();
            assert_eq!(
                (standard.damping_ratio, standard.stiffness),
                (0.9, 1400.0),
                "the theme carries standard motion"
            );
            WiniaTheme::with_motion_scheme(MotionScheme::expressive(), ctx, |_ctx| {
                let expressive = SpringSpec::spatial_fast();
                assert_eq!(
                    (expressive.damping_ratio, expressive.stiffness),
                    (0.6, 800.0),
                    "expressive motion reaches the component through the same call"
                );
                assert_eq!(
                    SpringSpec::effects_default(),
                    MotionScheme::expressive().default_effects,
                    "the effect springs resolve through the scheme too (and are the same numbers)"
                );
            });
            assert_eq!(
                SpringSpec::spatial_fast(),
                standard,
                "the override ends with its subtree"
            );
        });
    }

    #[test]
    fn legacy_theme_keeps_bare_text_default_size() {
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::light(ctx, |ctx| {
                Text::new("unchanged").build(ctx);
            });
        });
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let size = nodes[root].modifier.elements().iter().find_map(|el| {
            if let crate::modifier::ModifierElement::TextContent { font_size, .. } = el {
                Some(*font_size)
            } else {
                None
            }
        }).unwrap();
        assert_eq!(size, 14.0, "旧主题入口不得将裸 Text 自动改为 BodyLarge");
    }
