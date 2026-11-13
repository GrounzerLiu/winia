use super::*;

/// The node an interactive checkbox occupies, and the state layer it names.
///
/// `material3/Checkbox.kt:293-302` applies `Modifier.minimumInteractiveComponentSize()` (48 dp) whenever the
/// checkbox takes clicks with its 20 dp box centred inside; a checkbox with no click handler is 24 dp (the
/// box plus `CheckboxDefaultPadding`, `:736`) and takes no minimum. The halo is `StateLayerSize` (40 dp,
/// `:285`) — smaller than the node, so it travels as a NAMED size and the spreading layer still reaches the
/// node's diagonal.
#[test]
fn an_interactive_checkbox_reserves_48_dp_and_names_a_40_dp_state_layer() {
    let geometry = |interactive: bool| -> (f32, bool, usize, Option<f32>) {
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            crate::layout::components::Column::new()
                .modifier(Modifier::new().fill_max_width())
                .build(ctx, |ctx| {
                    let cb = Checkbox::new(false);
                    let cb = if interactive {
                        cb.on_checked_change(|_| {})
                    } else {
                        cb
                    };
                    cb.build(ctx);
                });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        let nodes = composer.arena_nodes();
        let clickables = nodes
            .iter()
            .filter(|n| {
                n.modifier
                    .elements()
                    .iter()
                    .any(|el| matches!(el, crate::modifier::ModifierElement::Clickable { .. }))
            })
            .count();
        let checkout = nodes
            .iter()
            .find(|n| {
                n.measured_size.width == CHECKBOX_TOUCH_TARGET
                    || n.measured_size.width == CHECKBOX_IDLE_SIZE
            })
            .expect("the checkbox node");
        let has_ripple = checkout
            .modifier
            .elements()
            .iter()
            .any(|el| matches!(el, crate::modifier::ModifierElement::Ripple { .. }));
        let state_layer = checkout.modifier.elements().iter().find_map(|el| match el {
            crate::modifier::ModifierElement::Ripple {
                state_layer_diameter,
                ..
            } => state_layer_diameter.as_ref().map(|d| d.eval()),
            _ => None,
        });
        (
            checkout.measured_size.width,
            has_ripple,
            clickables,
            state_layer,
        )
    };

    let (size, has_ripple, clickables, state_layer) = geometry(true);
    assert_eq!(size, 48.0, "an interactive checkbox reserves 48 dp");
    assert_eq!(clickables, 1, "and carries one clickable node");
    assert!(has_ripple, "with an unbounded ripple");
    assert_eq!(
        state_layer,
        Some(CHECKBOX_STATE_LAYER_SIZE),
        "and names a 40 dp state layer, smaller than the node"
    );

    let (size, has_ripple, clickables, _) = geometry(false);
    assert_eq!(
        size, CHECKBOX_IDLE_SIZE,
        "with no click handler the minimum is not applied"
    );
    assert_eq!(clickables, 0, "and nothing is clickable");
    assert!(!has_ripple, "and there is no ripple");
}

#[test]
fn toggleable_state_conversion() {
    assert_eq!(ToggleableState::from_bool(true), ToggleableState::On);
    assert_eq!(ToggleableState::from_bool(false), ToggleableState::Off);
    assert!(ToggleableState::On.is_checked());
    assert!(ToggleableState::Indeterminate.is_checked());
    assert!(!ToggleableState::Off.is_checked());
    assert!(ToggleableState::Indeterminate.is_indeterminate());
}

/// The draw-fraction spec is the SAME spring in both directions — a deliberate departure from M3.
///
/// The reference splits on the direction (`material3/Checkbox.kt:597-613`): filling springs on
/// `DefaultSpatial`, emptying is `snap(delayMillis = SnapAnimationDelay)`. At this component's size
/// that made ticking off read as "no animation" — measured on a frozen clock, the exit held 1.0 for six
/// frames and then jumped to 0, while the entry grew 0.13 → 0.32 → 0.49 → 0.63 → … over ~160 ms.
///
/// Both directions run the same spring so the exit shrinks instead of vanishing. This test pins the
/// symmetry, which `docs/checkbox.md` section 5 records as the divergence; reverting to M3's split
/// (a `Tween` when leaving) has to fail here rather than only changing how it feels.
#[test]
fn the_draw_spec_is_one_spring_in_both_directions() {
    for state in [
        ToggleableState::On,
        ToggleableState::Off,
        ToggleableState::Indeterminate,
    ] {
        match checkbox_draw_spec(state) {
            crate::animation::AnimationSpec::Spring(spec) => {
                assert_eq!(
                    spec,
                    crate::animation::SpringSpec::spatial_default(),
                    "{state:?} draws on DefaultSpatial"
                );
                assert_eq!(spec.damping_ratio, 0.9, "{state:?}");
            }
            other => panic!(
                "{state:?} must draw on the spring — M3's delayed snap for `Off` is exactly what this \
                 component departs from; got {other:?}"
            ),
        }
    }
    // The three states must genuinely be interchangeable here: same spec for every one, so an `if`
    // that reintroduces a direction split cannot survive by comparing equal on two of them.
    assert_eq!(
        format!("{:?}", checkbox_draw_spec(ToggleableState::Off)),
        format!("{:?}", checkbox_draw_spec(ToggleableState::On)),
        "leaving and entering share the spec"
    );
}

/// The colour spec is ONE spring in both directions, and it is the SLOW `DefaultEffects` one.
///
/// M3 splits here too (`colorAnimationSpecForState`, `Checkbox.kt:1040-1049`): `FastEffects`
/// (stiffness 3800) while the box empties, `DefaultEffects` (1600) otherwise. winia used to follow it,
/// and that split is why unchecking read as "it just becomes a box": measured on the box's own
/// background element, the fill was gone by 120 ms while the glyph still carried a third of its alpha,
/// so the glyph finished its exit painted on the page at almost no contrast.
///
/// Both directions run `DefaultEffects` so the fill, the border and the glyph fade together. This
/// test pins that: reverting to the `FastEffects`-when-leaving branch fails here rather than only
/// changing what the eye reports, and a silent return to a 300 ms tween fails too.
#[test]
fn the_colour_spec_is_one_effect_spring_in_both_directions() {
    for state in [
        ToggleableState::On,
        ToggleableState::Off,
        ToggleableState::Indeterminate,
    ] {
        let spec = checkbox_color_spec(state);
        assert_eq!(
            spec,
            crate::animation::SpringSpec::effects_default(),
            "{state:?} fades on DefaultEffects"
        );
        assert_eq!(spec.stiffness, 1600.0, "{state:?}");
    }
    // The three states must be genuinely interchangeable, so an `if` that reintroduces the direction
    // split cannot survive by happening to compare equal on two of them.
    assert_eq!(
        format!("{:?}", checkbox_color_spec(ToggleableState::Off)),
        format!("{:?}", checkbox_color_spec(ToggleableState::On)),
        "leaving and arriving share the spec"
    );
}

#[test]
fn color_state_resolution() {
    let theme = ThemeColors::light_from_seed(0x6750A4);
    let colors = CheckboxDefaults::checkbox_colors(&theme);
    // On: a Primary container and border, an OnPrimary checkmark.
    assert_eq!(colors.box_color(true, ToggleableState::On), theme.primary);
    assert_eq!(
        colors.border_color(true, ToggleableState::On),
        theme.primary
    );
    assert_eq!(
        colors.checkmark_color(ToggleableState::On),
        theme.on_primary
    );
    // Indeterminate shares the checked colour group with On.
    assert_eq!(
        colors.box_color(true, ToggleableState::Indeterminate),
        theme.primary
    );
    assert_eq!(
        colors.border_color(true, ToggleableState::Indeterminate),
        theme.primary
    );
    assert_eq!(
        colors.checkmark_color(ToggleableState::Indeterminate),
        theme.on_primary
    );
    // Off: a transparent container, an OnSurfaceVariant border, a transparent checkmark.
    assert_eq!(colors.box_color(true, ToggleableState::Off).a, 0);
    assert_eq!(
        colors.border_color(true, ToggleableState::Off),
        theme.on_surface_variant
    );
    assert_eq!(colors.checkmark_color(ToggleableState::Off).a, 0);
    // `disabled` takes precedence over the state.
    assert_eq!(
        colors.box_color(false, ToggleableState::On),
        colors.disabled_checked_box
    );
    assert_eq!(
        colors.border_color(false, ToggleableState::Indeterminate),
        colors.disabled_indeterminate_border
    );
    assert_eq!(
        colors.box_color(false, ToggleableState::Indeterminate),
        colors.disabled_indeterminate_box
    );
    // The disabled values: OnSurface @ 38%.
    assert_eq!(
        colors.disabled_checked_box.a,
        (theme.on_surface.a as f32 * 0.38) as u8
    );
    assert_eq!(
        colors.disabled_unchecked_border.a,
        (theme.on_surface.a as f32 * 0.38) as u8
    );
    assert_eq!(
        colors.disabled_indeterminate_box.a,
        (theme.on_surface.a as f32 * 0.38) as u8
    );
    // winia has no disabled checkmark slot, so a disabled box keeps painting `checked_checkmark`
    // (OnPrimary). M3 DOES have one — `CheckboxColors.disabledCheckmarkColor` (`Checkbox.kt:784`,
    // tokens `SelectedDisabledIconColor` = Surface) — but the flag-off branch never reads it: it
    // calls `checkmarkColor(state)` (`:948`), which splits on the state alone. Since
    // `isCheckboxStylingFixEnabled` defaults to false (`ComposeMaterial3Flags.kt:49`), resolving the
    // disabled checkmark to the checked colour matches M3's default behaviour.
    assert_eq!(
        colors.checkmark_color(ToggleableState::On),
        theme.on_primary
    );
}

#[test]
fn checkbox_callback_receives_inverted_checked() {
    use crate::modifier::ModifierElement;
    use std::sync::atomic::{AtomicBool, Ordering};
    for (checked, expect) in [(false, true), (true, false)] {
        let received = Arc::new(AtomicBool::new(false));
        let cb = received.clone();
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            Checkbox::new(checked)
                .on_checked_change(move |v| cb.store(v, Ordering::Relaxed))
                .build(ctx);
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
        assert!(clicked, "a Clickable is present");
        assert_eq!(
            received.load(Ordering::Relaxed),
            expect,
            "the callback receives !checked"
        );
    }
}

#[test]
fn tri_state_on_click_fires() {
    use crate::modifier::ModifierElement;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let clicks = Arc::new(AtomicUsize::new(0));
    let c = clicks.clone();
    let mut composer = crate::runtime::composer::Composer::new();
    composer.compose(|ctx| {
        TriStateCheckbox::new(ToggleableState::Indeterminate)
            .on_click(move || {
                c.fetch_add(1, Ordering::Relaxed);
            })
            .build(ctx);
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
    assert!(clicked, "a Clickable is present");
    assert_eq!(clicks.load(Ordering::Relaxed), 1);
}

#[test]
fn disabled_checkbox_has_no_interaction_elements() {
    use crate::modifier::ModifierElement;
    for state in [
        ToggleableState::Off,
        ToggleableState::On,
        ToggleableState::Indeterminate,
    ] {
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            TriStateCheckbox::new(state)
                .enabled(false)
                .on_click(|| {})
                .build(ctx);
        });
        composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        for node in composer.arena_nodes() {
            for el in node.modifier.elements() {
                assert!(
                    !matches!(el, ModifierElement::Ripple { .. })
                        && !matches!(el, ModifierElement::Focusable { .. })
                        && !matches!(el, ModifierElement::Clickable { .. }),
                    "a disabled Checkbox must register no interaction element: {el:?}"
                );
            }
        }
    }
}

#[test]
fn checked_checkmark_icon_tint_resolves_to_on_primary() {
    // Integration: in the On state both the check and the dash icon are tinted on_primary.
    let theme = ThemeColors::light_from_seed(0x6750A4);
    let mut composer = crate::runtime::composer::Composer::new();
    composer.compose(|ctx| {
        WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
            Checkbox::new(true).on_checked_change(|_| {}).build(ctx);
        });
    });
    composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
    let mut count = 0;
    for node in composer.arena_nodes() {
        for el in node.modifier.elements() {
            if let crate::modifier::ModifierElement::DrawIcon { spec } = el {
                assert_eq!(
                    spec.tint.as_ref().map(|t| t.eval()),
                    Some(theme.on_primary),
                    "the checkmark tint is OnPrimary"
                );
                count += 1;
            }
        }
    }
    assert_eq!(
        count, 2,
        "the On state carries BOTH the check and the dash icon (the animation pair)"
    );
}

#[test]
fn unchecked_has_transparent_checkmark_and_no_fill() {
    let theme = ThemeColors::light_from_seed(0x6750A4);
    let colors = CheckboxColors::from_theme(&theme);
    assert_eq!(colors.unchecked_box.a, 0);
    assert_eq!(colors.checkmark_color(ToggleableState::Off).a, 0);
    assert_eq!(colors.checked_box, theme.primary);
}

#[test]
fn uncheck_keeps_primary_pixels_during_transition() {
    // Regression: unchecking must fade the container and the checkmark out of Primary — still
    // visible on the FIRST rendered frame after the state flips, not switched to transparent at
    // once. Otherwise the glyph's scale-out is swallowed and unchecking reads as "no animation".
    use skia_safe::{Color, surfaces};
    use std::time::Duration;
    let _g = crate::animation::tests::TEST_SERIAL
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let theme = ThemeColors::light_from_seed(0x6750A4);
    let mut composer = crate::runtime::composer::Composer::new();
    let holder = std::cell::RefCell::new(None::<crate::runtime::state::State<bool>>);
    let build_scene = |ctx: &mut ComposeCtx| {
        WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
            let c = ctx.remember(|| true);
            holder.replace(Some(c.clone()));
            Checkbox::new(c.get()).on_checked_change(|_| {}).build(ctx);
        });
    };
    let mut render = |composer: &mut crate::runtime::composer::Composer| -> Vec<u8> {
        composer.compose(build_scene);
        composer.compose(build_scene);
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
        let mut surface = surfaces::raster_n32_premul((300, 300)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(Color::WHITE);
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        crate::render::render(nodes, root, canvas);
        let pm = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        px.iter()
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect::<Vec<_>>()
    };
    let count_primary = |px: &[u8]| {
        px.chunks_exact(3)
            // Skia's N32 premul memory order is BGRA, so the first 3 bytes from flat_map are B,G,R.
            .filter(|rgb| rgb[2] > 80 && rgb[0] as i32 > rgb[2] as i32 + 20)
            .count()
    };
    composer.compose(build_scene);
    composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
    let c = holder.borrow().clone().expect("state");
    let checked_px = render(&mut composer);
    assert!(
        count_primary(&checked_px) > 50,
        "the checked state must paint Primary container pixels"
    );
    // Uncheck: the state flips to Off at once, but the colour and glyph animations have not advanced
    // yet — the transition frame must still carry Primary pixels (before the fix the container and
    // the glyph went transparent instantly, so this was 0).
    c.update(|s| *s = false);
    let transition_px = render(&mut composer);
    let transition_primary = count_primary(&transition_px);
    assert!(
        transition_primary > 50,
        "the unchecking transition frame must keep Primary pixels (got {transition_primary}) — \
         switching the colour to transparent swallows the exit animation"
    );
    // Once the animations finish: the settled unchecked state has no Primary pixels left.
    for _ in 0..400 {
        if !crate::animation::update_animations() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let settled_px = render(&mut composer);
    assert!(
        count_primary(&settled_px) < 5,
        "the settled unchecked state must have no Primary pixels left"
    );
}

#[test]
fn disabled_checked_border_merges_with_fill() {
    // Regression: with a disabled CHECKED checkbox the border colour equals the container colour
    // (semi-transparent), so the renderer must merge them into a plain fill. Stroking another layer
    // of border blends twice and leaves the edge visibly darker than the interior.
    use skia_safe::{Color as SkColor, surfaces};
    let _g = crate::animation::tests::TEST_SERIAL
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let fill = crate::graphics::Color::from_argb(96, 40, 80, 220);
    let mut composer = crate::runtime::composer::Composer::new();
    let scene = |ctx: &mut ComposeCtx| {
        Checkbox::new(true)
            .enabled(false)
            .colors(CheckboxColors::new(
                fill, fill, fill, fill, fill, fill, fill, fill, fill, fill, fill, fill,
            ))
            .on_checked_change(|_| {})
            .build(ctx);
    };
    composer.compose(scene);
    composer.compose(scene);
    composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
    let mut surface = surfaces::raster_n32_premul((300, 300)).unwrap();
    let canvas = surface.canvas();
    canvas.clear(SkColor::WHITE);
    let root = composer.layout_root_idx().expect("root");
    let nodes = composer.arena_nodes();
    crate::render::render(nodes, root, canvas);
    let pm = surface.peek_pixels().expect("pixmap");
    let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
    // The 20×20 visual box, centred inside the 40×40 touch target.
    let (mut bx, mut by) = (0.0f32, 0.0f32);
    let mut found = false;
    for node in nodes {
        if node.measured_size.width == CHECKBOX_TOUCH_TARGET
            && node.measured_size.height == CHECKBOX_TOUCH_TARGET
        {
            bx = node.position.x + (CHECKBOX_TOUCH_TARGET - CHECKBOX_SIZE) / 2.0;
            by = node.position.y + (CHECKBOX_TOUCH_TARGET - CHECKBOX_SIZE) / 2.0;
            found = true;
            break;
        }
    }
    assert!(found, "a 40×40 touch target must be present");
    // This scene is a single-level layout, so `node.position` IS the global pixel coordinate — the
    // same convention the switch pixel tests use. A nested node would need accumulated offsets.
    let at = |x: f32, y: f32| {
        let p = px[(y as usize) * 300 + (x as usize)];
        (p[0] as i32, p[1] as i32, p[2] as i32)
    };
    // The interior reference point sits above the checkmark's path (y+3) so the glyph cannot cover it.
    let interior = at(bx + 10.0, by + 3.0);
    let edges = [
        at(bx + 1.0, by + 10.0),
        at(bx + 18.0, by + 10.0),
        at(bx + 10.0, by + 1.0),
        at(bx + 10.0, by + 18.0),
    ];
    for (i, e) in edges.iter().enumerate() {
        let d = (e.0 - interior.0).abs() + (e.1 - interior.1).abs() + (e.2 - interior.2).abs();
        assert!(
            d < 40,
            "the border must merge into a plain fill (edge[{i}]={e:?} interior={interior:?} diff={d})"
        );
    }
}

/// A new `colors(..)` reaches the CHECKMARK, not only the box.
///
/// The two are carried by different channels, and only one of them is automatic. The box and border
/// take `Dyn` closures (`background(move || anim.peek())`), read at paint time, so they follow
/// anything. The checkmark's colour goes into `Icon::tint`, which is consumed when the icon is
/// COMPOSED — and the group holding those two icons can only re-enter if it compares a parameter.
/// Its own modifier cannot supply one: `Dyn::same_parameter` calls any two dynamics equal
/// (`graphics/dyn_value.rs:43-49`), which is what lets an animating container Skip at all. Hence the
/// `ctx.changed(&check_color)` in `checkbox_impl`; deleting it leaves both glyphs painting the colour
/// they were built with (measured: (255, 255, 255) after the palette moved to (1, 2, 3)).
///
/// A theme switch does not show this — the theme provider re-enters the whole subtree — so the
/// regression only appears for a caller that passes changed `CheckboxColors`.
#[test]
fn a_colors_change_reaches_the_checkmark_tint() {
    // The container colour is animated, so this registers with the process-wide table the other
    // animation tests share — and the frozen clock lets the assertion step the fade to rest.
    let (_g, clock) = crate::animation::tests::serial_clock();
    crate::animation::clear_all_animations();

    let theme = ThemeColors::light_from_seed(0x6750A4);
    let before = CheckboxColors::from_theme(&theme);
    let mut after = before;
    after.checked_checkmark = Color::from_argb(255, 1, 2, 3);
    assert_ne!(
        before.checked_checkmark, after.checked_checkmark,
        "this test needs two different checkmark colours"
    );

    let current = std::cell::Cell::new(before);
    let mut composer = crate::runtime::composer::Composer::new();
    let scene = |ctx: &mut ComposeCtx| {
        let colours = current.get();
        WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
            Checkbox::new(true)
                .colors(colours)
                .on_checked_change(|_| {})
                .build(ctx);
        });
    };
    let tints = |composer: &crate::runtime::composer::Composer| -> Vec<Option<Color>> {
        composer
            .arena_nodes()
            .iter()
            .flat_map(|node| node.modifier.elements())
            .filter_map(|el| match el {
                crate::modifier::ModifierElement::DrawIcon { spec } => {
                    Some(spec.tint.as_ref().map(|t| t.eval()))
                }
                _ => None,
            })
            .collect()
    };

    composer.compose(scene);
    composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
    let painted = tints(&composer);
    assert_eq!(
        painted.len(),
        2,
        "On state carries both the check and the dash"
    );
    assert!(
        painted.iter().all(|t| *t == Some(before.checked_checkmark)),
        "at rest both glyphs take the supplied checkmark colour, got {painted:?}"
    );

    current.set(after);
    // The tint is the color of an `animateColorAsState` call now, not a constant, so the glyphs reach
    // the new colour over the spring instead of on the next pass. Push the motion once, then step the
    // clock to rest before reading — asserting straight after the recomposition measures the START of
    // the fade, which is the colour they are leaving (measured: (251, 255, 255)).
    composer.recompose_to_stable(|| scene);
    for _ in 0..120 {
        clock.advance(std::time::Duration::from_millis(17));
        if !crate::animation::update_animations() {
            break;
        }
        composer.recompose_to_stable(|| scene);
    }
    composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
    let repainted = tints(&composer);
    assert!(
        repainted
            .iter()
            .all(|t| *t == Some(after.checked_checkmark)),
        "the glyphs repaint in the NEW checkmark colour instead of the one they were composed \
         with; got {repainted:?}, want {:?}",
        after.checked_checkmark
    );
}

/// `copy(..)` replaces exactly the slots it is given and keeps every other one.
///
/// `None` is the "keep" role: M3 spells it `Color.Unspecified` and routes it through `takeOrElse`
/// (`Checkbox.kt:888-909`). Carrying the absence in the `Option` rather than in a sentinel colour is
/// what makes an all-`None` copy an identity — a sentinel would break that the moment a caller wanted
/// to paint the exact sentinel value.
#[test]
fn copy_replaces_exactly_the_slots_it_is_given() {
    let theme = ThemeColors::light_from_seed(0x6750A4);
    let base = CheckboxColors::from_theme(&theme);
    let changed = Color::from_argb(255, 1, 2, 3);
    let c = base.copy(
        Some(changed), // checked_checkmark
        None,          // unchecked_checkmark
        Some(changed), // checked_box
        None,          // unchecked_box
        None,          // disabled_checked_box
        None,          // disabled_unchecked_box
        None,          // disabled_indeterminate_box
        None,          // checked_border
        Some(changed), // unchecked_border
        None,          // disabled_border
        None,          // disabled_unchecked_border
        None,          // disabled_indeterminate_border
    );
    // The three supplied slots moved...
    assert_eq!(c.checked_checkmark, changed);
    assert_eq!(c.checked_box, changed);
    assert_eq!(c.unchecked_border, changed);
    // ...and every `None` kept the source value, field by field.
    assert_eq!(c.unchecked_checkmark, base.unchecked_checkmark);
    assert_eq!(c.unchecked_box, base.unchecked_box);
    assert_eq!(c.disabled_checked_box, base.disabled_checked_box);
    assert_eq!(c.disabled_unchecked_box, base.disabled_unchecked_box);
    assert_eq!(
        c.disabled_indeterminate_box,
        base.disabled_indeterminate_box
    );
    assert_eq!(c.checked_border, base.checked_border);
    assert_eq!(c.disabled_border, base.disabled_border);
    assert_eq!(c.disabled_unchecked_border, base.disabled_unchecked_border);
    assert_eq!(
        c.disabled_indeterminate_border,
        base.disabled_indeterminate_border
    );
    assert_eq!(
        base.copy(
            None, None, None, None, None, None, None, None, None, None, None, None
        ),
        base,
        "an all-`None` copy is the identity"
    );
}

/// The six-argument `CheckboxDefaults::colors(..)` (`Checkbox.kt:368`): which slot each argument feeds.
///
/// Two of the six do double duty — `checkedColor` is both the checked FILL and the checked border,
/// `disabledCheckedColor` both the disabled checked fill and that border — and three slots are forced
/// transparent rather than exposed, exactly as upstream passes `Color.Transparent` for them.
#[test]
fn defaults_colors_feeds_six_arguments_and_forces_three_transparent() {
    let theme = ThemeColors::light_from_seed(0x6750A4);
    let base = CheckboxDefaults::checkbox_colors(&theme);
    let checked = Color::from_argb(255, 10, 0, 0);
    let unchecked = Color::from_argb(255, 0, 10, 0);
    let mark = Color::from_argb(255, 0, 0, 10);
    let disabled_checked = Color::from_argb(255, 20, 0, 0);
    let disabled_unchecked = Color::from_argb(255, 0, 20, 0);
    let disabled_indeterminate = Color::from_argb(255, 0, 0, 20);
    let c = CheckboxDefaults::colors(
        &theme,
        Some(checked),
        Some(unchecked),
        Some(mark),
        Some(disabled_checked),
        Some(disabled_unchecked),
        Some(disabled_indeterminate),
    );
    // `checkmarkColor` feeds the checked slot. winia has no disabled checkmark slot, so unlike
    // upstream this argument does not also seed one (`Checkbox.kt:379`).
    assert_eq!(c.checked_checkmark, mark);
    // Box: the checked fill, and the two disabled forms with a box argument of their own.
    assert_eq!(c.checked_box, checked);
    assert_eq!(c.disabled_checked_box, disabled_checked);
    assert_eq!(c.disabled_indeterminate_box, disabled_indeterminate);
    // Border: mirrors the checked fill, takes the caller's outline colours when not checked.
    assert_eq!(c.checked_border, checked);
    assert_eq!(c.unchecked_border, unchecked);
    assert_eq!(c.disabled_border, disabled_checked);
    assert_eq!(c.disabled_unchecked_border, disabled_unchecked);
    assert_eq!(c.disabled_indeterminate_border, disabled_indeterminate);
    // The three forced ones: an unchecked box is an OUTLINE, never a fill, so these cannot be named.
    for (name, value) in [
        ("unchecked_box", c.unchecked_box),
        ("disabled_unchecked_box", c.disabled_unchecked_box),
        ("unchecked_checkmark", c.unchecked_checkmark),
    ] {
        assert_eq!(
            value.a, 0,
            "{name} is transparent by construction in the six-argument form"
        );
    }
    // The theme already derives those three transparent, so an all-`None` call is the theme's set.
    assert_eq!(
        CheckboxDefaults::colors(&theme, None, None, None, None, None, None),
        base,
        "every argument is optional — omitting all six must return the theme's derivation"
    );
}

/// Unchecking FADES and SHRINKS the glyph — and the fade must stay VISIBLE while it happens.
///
/// Three separate things made the exit unreadable, each found by measuring the rendered frame rather
/// than the component's properties:
///
/// 1. the tint was pinned to `checked_checkmark` for every state, so the colour never moved at all;
/// 2. the draw fraction held at 1.0 for `SnapAnimationDelay` (100 ms) then jumped to 0 in one frame,
///    because M3 snaps the emptying direction (`Checkbox.kt:603`). winia now runs the exit on the same
///    `DefaultSpatial` spring as the entry — `the_draw_spec_is_one_spring_in_both_directions`;
/// 3. the fade target was `unchecked_checkmark`, which is `Color.Transparent` — BLACK. `Color`
///    interpolates in Oklab, lightness included, so white → black-at-zero-alpha dimmed the glyph almost
///    at once: measured peak-vs-interior lightness inside the box went 206 → 63 in the first 17 ms frame
///    and then barely moved, i.e. the tick did not fade, it just became a bare box. The target is now
///    the checked colour at alpha 0, so only opacity moves.
///
/// This test pins (2) and (3) together, on the pixels this time: the scale must be mid-shrink AND the
/// glyph must still be visible one frame in. A regression to the black target re-collapses the contrast
/// even though every property-level assertion above still passes.
#[test]
fn unchecking_fades_and_shrinks_the_glyph_visibly() {
    use skia_safe::{Color as SkColor, surfaces};
    let (_g, clock) = crate::animation::tests::serial_clock();
    crate::animation::clear_all_animations();

    let theme = ThemeColors::light_from_seed(0x6750A4);
    let mut composer = crate::runtime::composer::Composer::new();
    let holder = std::cell::RefCell::new(None::<crate::runtime::state::State<bool>>);
    let scene = |ctx: &mut ComposeCtx| {
        WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
            let c = ctx.remember(|| true);
            holder.replace(Some(c.clone()));
            Checkbox::new(c.get()).on_checked_change(|_| {}).build(ctx);
        });
    };
    // The two glyphs' tint alphas, and the largest layer scale (the check sits at 1 while the dash
    // rests at 0 in the On state, so `max` is the one that moves).
    let glyphs = |composer: &crate::runtime::composer::Composer| -> (Vec<u8>, f32) {
        let mut alphas = Vec::new();
        let mut scale = 0.0f32;
        for node in composer.arena_nodes() {
            for el in node.modifier.elements() {
                match el {
                    crate::modifier::ModifierElement::DrawIcon { spec } => {
                        alphas.push(spec.tint.as_ref().map(|t| t.eval().a).unwrap_or(0));
                    }
                    crate::modifier::ModifierElement::GraphicsLayer { params } => {
                        scale = scale.max(params.eval().scale_x);
                    }
                    _ => {}
                }
            }
        }
        (alphas, scale)
    };
    // How far the brightest pixel inside the 20 dp box stands out from its interior: the glyph's
    // legibility, in lightness units. Read from the rendered frame, because that is where the fade the
    // eye complains about actually happens.
    let contrast = |composer: &crate::runtime::composer::Composer| -> i32 {
        let mut surface = surfaces::raster_n32_premul((120, 120)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(SkColor::from_rgb(28, 27, 31));
        let root = composer.layout_root_idx().expect("root");
        crate::render::render(composer.arena_nodes(), root, canvas);
        let pm = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        let (mut hi, mut lo) = (0i32, i32::MAX);
        for y in 14..34usize {
            for x in 14..34usize {
                let p = px[y * 120 + x];
                // N32 premul reads BGRA.
                let lum = (p[2] as i32 * 3 + p[1] as i32 * 6 + p[0] as i32) / 10;
                hi = hi.max(lum);
                lo = lo.min(lum);
            }
        }
        hi - lo
    };
    let frame = |composer: &mut crate::runtime::composer::Composer| {
        composer.recompose_to_stable(|| scene);
        composer.layout(crate::layout::Constraints::new(0.0, 120.0, 0.0, 120.0));
    };

    composer.compose(scene);
    composer.layout(crate::layout::Constraints::new(0.0, 120.0, 0.0, 120.0));
    let c = holder.borrow().clone().expect("the checkbox state");
    let (at_rest, scale_rest) = glyphs(&composer);
    assert_eq!(at_rest, vec![255, 255], "On paints both glyphs opaque");
    assert!((scale_rest - 1.0).abs() < 1e-4, "and the check at full size");
    let contrast_rest = contrast(&composer);

    c.set(false);
    frame(&mut composer);
    clock.advance(std::time::Duration::from_millis(17));
    crate::animation::update_animations();
    frame(&mut composer);

    let (fading, scale_early) = glyphs(&composer);
    let contrast_early = contrast(&composer);
    assert!(
        fading.iter().all(|a| *a > 0 && *a < 255),
        "one frame into the exit the glyphs are MID-FADE, not switched off and not still opaque; \
         got {fading:?} — a pinned tint leaves this at 255 for every frame"
    );
    assert!(
        scale_early > 0.0 && scale_early < 1.0,
        "and the scale is MID-SHRINK: the exit springs down instead of holding at full size; got \
         {scale_early} — a `snap(delayMillis = 100)` spec parks it at 1.0 for six frames"
    );
    // The one the eye actually reports. Half the resting contrast is the floor: a fade to BLACK keeps
    // the alpha curve identical but lands far below this, because Oklab moves lightness too.
    assert!(
        contrast_early * 2 > contrast_rest,
        "the glyph is still LEGIBLE one frame in — fading to black-at-zero-alpha collapses this to a \
         third of the resting value while every alpha assertion above still passes; got \
         {contrast_early} against {contrast_rest} at rest"
    );

    // Both channels finish at the same place: no glyph painted, no scale left.
    for _ in 0..120 {
        clock.advance(std::time::Duration::from_millis(17));
        if !crate::animation::update_animations() {
            break;
        }
        frame(&mut composer);
    }
    let (settled, scale_late) = glyphs(&composer);
    assert!(
        settled.iter().all(|a| *a == 0),
        "settled Off paints no glyph; got {settled:?}"
    );
    assert!(
        scale_late.abs() < 1e-4,
        "and the scale has shrunk to 0 by then; got {scale_late}"
    );
}

/// A parent DERIVED from its three children follows them — the select-all shape, on the pixels the
/// semantics tree publishes.
///
/// `examples/checkbox_demo.rs` builds this with a `labelled(..)` helper, and that helper is where the
/// bug lived: it took `state`, built a `Row`, and passed the value to the `TriStateCheckbox` inside
/// WITHOUT declaring it as a parameter of the Row. `Row` compares its own
/// spacing / arrangement / alignment / direction, none of which moves when a child is ticked, so it
/// Skipped: content not run, checkbox not rebuilt, dash left on screen.
///
/// The derivation was never the problem. A counter in the enclosing closure recorded
/// `Indeterminate`, `On`, `Indeterminate` — the right values, computed every pass — while the box
/// stayed on the dash. This is the caller-side half of the rule in `docs/component-authoring.md`
/// section 3.2: a value read in the parent and captured into a container's content is invisible to
/// that container's Skip decision unless the caller hands it over as a parameter.
///
/// Deleting `ctx.changed(&parent)` below reproduces it exactly: the middle assertion fails with the
/// parent stuck on `Indeterminate` after all three children are on.
#[test]
fn a_derived_parent_follows_its_children() {
    use crate::components::Text;
    use crate::layout::components::{Column, Row};
    use crate::runtime::state::State;
    use crate::semantics::{SemanticsConfig, semantics_tree};

    let mut composer = crate::runtime::composer::Composer::new();
    let handles = std::cell::RefCell::new(None::<(State<bool>, State<bool>, State<bool>)>);
    let scene = |ctx: &mut ComposeCtx| {
        WiniaTheme::light(ctx, |ctx| {
            let first = ctx.remember(|| true);
            let second = ctx.remember(|| true);
            let third = ctx.remember(|| false);
            handles.replace(Some((first.clone(), second.clone(), third.clone())));
            Column::new().spacing(2.0).build(ctx, |ctx| {
                // The derivation, in the closure that has to re-run — as the demo does it.
                let all = first.get() && second.get() && third.get();
                let none = !first.get() && !second.get() && !third.get();
                let parent = if all {
                    ToggleableState::On
                } else if none {
                    ToggleableState::Off
                } else {
                    ToggleableState::Indeterminate
                };
                // The line the whole test is about: the derived value is a parameter of the ROW that
                // holds the checkbox. Without it the Row sees unchanged parameters and Skips.
                ctx.changed(&parent);
                Row::new().spacing(8.0).build(ctx, |ctx| {
                    TriStateCheckbox::new(parent)
                        .on_click(|| {})
                        .modifier(Modifier::new().semantics(
                            SemanticsConfig::new().content_description("All"),
                        ))
                        .build(ctx);
                    Text::new("All").font_size(14.0).build(ctx);
                });
                for (label, value) in [
                    ("First", first.clone()),
                    ("Second", second.clone()),
                    ("Third", third.clone()),
                ] {
                    ctx.key(label, |ctx| {
                        Row::new().spacing(8.0).build(ctx, |ctx| {
                            TriStateCheckbox::new(ToggleableState::from_bool(value.get()))
                                .on_click({
                                    let value = value.clone();
                                    move || value.set(!value.get())
                                })
                                .modifier(Modifier::new().semantics(
                                    SemanticsConfig::new().content_description(label),
                                ))
                                .build(ctx);
                            Text::new(label).font_size(14.0).build(ctx);
                        });
                    });
                }
            });
        });
    };
    let parent_state = |composer: &crate::runtime::composer::Composer| -> ToggleableState {
        let root = composer.layout_root_idx().expect("layout root");
        let tree = semantics_tree(composer.arena_nodes(), root);
        for node in &tree {
            if let Some(hit) = node.find_by_name("All") {
                return hit.state.checked.expect("the parent publishes its tri-state value");
            }
        }
        panic!("no semantics element named \"All\"");
    };
    let frame = |composer: &mut crate::runtime::composer::Composer| {
        composer.compose(scene);
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
    };

    frame(&mut composer);
    frame(&mut composer);
    assert_eq!(
        parent_state(&composer),
        ToggleableState::Indeterminate,
        "two of three ticked is Indeterminate"
    );

    let (first, second, third) = handles.borrow().clone().expect("the three handles");
    // 3/3 — the parent must turn On. This is the assertion the demo failed: the value was derived
    // correctly and then never reached the node.
    third.set(true);
    frame(&mut composer);
    frame(&mut composer);
    assert_eq!(
        parent_state(&composer),
        ToggleableState::On,
        "all three ticked must read On — a parent whose value reaches no node stays on the dash \
         (declare it with `ctx.changed` on the container that holds the checkbox)"
    );

    // 2/3 again — back to Indeterminate, so the following is not one-way.
    first.set(false);
    frame(&mut composer);
    frame(&mut composer);
    assert_eq!(
        parent_state(&composer),
        ToggleableState::Indeterminate,
        "unticking one must bring the dash back"
    );

    // 0/3 — Off. (first is already off from the step above; second and third still have to go.)
    second.set(false);
    third.set(false);
    frame(&mut composer);
    frame(&mut composer);
    assert_eq!(
        parent_state(&composer),
        ToggleableState::Off,
        "none ticked is Off"
    );
}
