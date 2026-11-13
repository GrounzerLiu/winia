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

/// The draw-fraction spec per state, pinned to M3: filling (and `On` ↔ `Indeterminate`) is the
/// `DefaultSpatial` spring, emptying is a 100 ms-delayed snap rather than either a spring or an
/// immediate jump.
#[test]
fn the_draw_spec_matches_m3s_transition() {
    for state in [ToggleableState::On, ToggleableState::Indeterminate] {
        match checkbox_draw_spec(state) {
            crate::animation::AnimationSpec::Spring(spec) => {
                assert_eq!(
                    spec,
                    crate::animation::SpringSpec::spatial_default(),
                    "{state:?}"
                );
                assert_eq!(spec.damping_ratio, 0.9, "{state:?}");
            }
            other => panic!("{state:?} must draw on a spring, got {other:?}"),
        }
    }
    match checkbox_draw_spec(ToggleableState::Off) {
        crate::animation::AnimationSpec::Tween(spec) => {
            assert!(spec.duration.is_zero(), "the snap itself is instant");
            assert_eq!(
                spec.delay, CHECKBOX_SNAP_DELAY,
                "SnapAnimationDelay = 100 ms"
            );
            assert_eq!(CHECKBOX_SNAP_DELAY, std::time::Duration::from_millis(100));
        }
        other => panic!("emptying must snap after a delay, got {other:?}"),
    }
}

/// The colour spec per state, pinned to M3's tokens: leaving (`Off`) is the faster spring, every
/// other state the slower one. A silent return to a 300 ms tween fails here.
#[test]
fn the_colour_spec_is_m3s_effect_spring_per_state() {
    let leaving = checkbox_color_spec(ToggleableState::Off);
    assert_eq!(leaving, crate::animation::SpringSpec::effects_fast());
    assert_eq!(leaving.stiffness, 3800.0);
    for state in [ToggleableState::On, ToggleableState::Indeterminate] {
        let arriving = checkbox_color_spec(state);
        assert_eq!(
            arriving,
            crate::animation::SpringSpec::effects_default(),
            "{state:?}"
        );
        assert_eq!(arriving.stiffness, 1600.0, "{state:?}");
    }
}

#[test]
fn color_state_resolution() {
    let theme = ThemeColors::light_from_seed(0x6750A4);
    let colors = CheckboxDefaults::checkbox_colors(&theme);
    // On：primary 容器/边框 + onPrimary 勾号
    assert_eq!(colors.box_color(true, ToggleableState::On), theme.primary);
    assert_eq!(
        colors.border_color(true, ToggleableState::On),
        theme.primary
    );
    assert_eq!(
        colors.checkmark_color(ToggleableState::On),
        theme.on_primary
    );
    // Indeterminate 与 On 共用 checked 色组
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
    // Off：透明容器 + OnSurfaceVariant 边框 + 透明勾号
    assert_eq!(colors.box_color(true, ToggleableState::Off).a, 0);
    assert_eq!(
        colors.border_color(true, ToggleableState::Off),
        theme.on_surface_variant
    );
    assert_eq!(colors.checkmark_color(ToggleableState::Off).a, 0);
    // disabled 优先于状态
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
    // disabled 色值：OnSurface @ 38%
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
        assert!(clicked, "存在 Clickable");
        assert_eq!(
            received.load(Ordering::Relaxed),
            expect,
            "回调收到 !checked"
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
    assert!(clicked, "存在 Clickable");
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
                    "禁用 Checkbox 不应有交互元素: {el:?}"
                );
            }
        }
    }
}

#[test]
fn checked_checkmark_icon_tint_resolves_to_on_primary() {
    // 集成：On 时勾号/横线图标 tint = on_primary
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
                    "勾号 tint = OnPrimary"
                );
                count += 1;
            }
        }
    }
    assert_eq!(count, 2, "On 状态同时挂 check 与 dash 两个图标（动画节点）");
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
    // 回归：取消选中时容器/勾号必须从 Primary 渐变消失（状态切换后的
    // 第一个渲染帧仍可见），而不是瞬间切透明——否则勾号缩放退出动画
    // 被吞掉，表现为“取消选中没有动画”。
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
            // skia N32 premul 内存序为 BGRA：flat_map 取前 3 字节 = B,G,R
            .filter(|rgb| rgb[2] > 80 && rgb[0] as i32 > rgb[2] as i32 + 20)
            .count()
    };
    composer.compose(build_scene);
    composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
    let c = holder.borrow().clone().expect("state");
    let checked_px = render(&mut composer);
    assert!(
        count_primary(&checked_px) > 50,
        "选中态应有 Primary 容器像素"
    );
    // 取消选中：状态立即切 Off，但颜色/勾号动画尚未推进——过渡帧必须
    // 仍保留 Primary 像素（修复前容器/勾号瞬切透明 → 这里为 0）
    c.update(|s| *s = false);
    let transition_px = render(&mut composer);
    let transition_primary = count_primary(&transition_px);
    assert!(
        transition_primary > 50,
        "取消选中过渡帧应保留 Primary 像素（实际 {transition_primary}）——颜色瞬切透明吞掉退出动画"
    );
    // 动画推完 → 回到未选中静止态：不再有 Primary 像素
    for _ in 0..400 {
        if !crate::animation::update_animations() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let settled_px = render(&mut composer);
    assert!(
        count_primary(&settled_px) < 5,
        "未选中静止态不应再有 Primary 像素"
    );
}

#[test]
fn disabled_checked_border_merges_with_fill() {
    // 回归：禁用已选中时边框色 = 容器色（半透明），渲染层应合并为
    // 纯填充；若边框再叠一层 stroke 会双重混合，边框带明显深于内部。
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
    // 40×40 触摸目标内居中 20×20 视觉盒
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
    assert!(found, "应有 40×40 触摸目标");
    // 当前场景是单层布局，node.position 即全局像素坐标（与 switch 像素
    // 测试同一约定）；若测试引入嵌套节点需改为递归累计偏移。
    let at = |x: f32, y: f32| {
        let p = px[(y as usize) * 300 + (x as usize)];
        (p[0] as i32, p[1] as i32, p[2] as i32)
    };
    // 内部参考点选在勾号路径上方（y+3），避免勾号覆盖
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
            "边框应合并为纯填充（edge[{i}]={e:?} interior={interior:?} diff={d}）"
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

/// Unchecking FADES the glyph out; the checkmark's colour has an animation of its own.
///
/// The tint used to be pinned to `checked_checkmark` for every state, so the only exit a glyph had was
/// the snap of its scale at `SnapAnimationDelay` (100 ms) — and 100 ms of nothing followed by an
/// instant disappearance reads as "unchecking has no animation". M3 animates the colour:
/// `animateColorAsState(checkmarkColor, colorAnimationSpec)` (`Checkbox.kt:634`), with no `enabled`
/// branch, over `checkmarkColor(state)` (`:948`), which returns `uncheckedCheckmarkColor` —
/// `Color.Transparent` by default (`:451`) — when `Off`. Emptying therefore fades on `FastEffects`
/// while the scale waits out its delay.
///
/// Both halves are asserted: the fade is PART WAY through one frame in (neither the old colour nor
/// gone), and the scale has not moved yet. Measured trajectory at 17 ms frames after the flip:
/// 255 → 162 → 85 → 43 → 21 → 11 → 5 → 0, scale holding 1.0 until 102 ms.
#[test]
fn unchecking_fades_the_glyph_colour_before_the_scale_snaps() {
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
    let frame = |composer: &mut crate::runtime::composer::Composer| {
        composer.recompose_to_stable(|| scene);
        composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
    };

    composer.compose(scene);
    composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
    let c = holder.borrow().clone().expect("the checkbox state");
    let (at_rest, _) = glyphs(&composer);
    assert_eq!(at_rest, vec![255, 255], "On paints both glyphs opaque");

    c.set(false);
    frame(&mut composer);
    clock.advance(std::time::Duration::from_millis(17));
    crate::animation::update_animations();
    frame(&mut composer);

    let (fading, scale_early) = glyphs(&composer);
    assert!(
        fading.iter().all(|a| *a > 0 && *a < 255),
        "one frame into the exit the glyphs are MID-FADE, not switched off and not still opaque; \
         got {fading:?} — a pinned tint leaves this at 255 for every frame"
    );
    assert!(
        (scale_early - 1.0).abs() < 1e-4,
        "and the SCALE has not moved yet: M3 waits out SnapAnimationDelay before snapping it, so the \
         fade is the only thing carrying the exit; got {scale_early}"
    );

    // Past the delay the colour has arrived at `unchecked_checkmark` (transparent) and the scale snaps.
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
        "and the scale has snapped to 0 by then; got {scale_late}"
    );
}
