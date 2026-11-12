use super::*;
use crate::runtime::composer::Composer;

#[test]
fn test_button_defaults() {
    let btn = Button::new();
    assert!(btn.get_enabled());
    assert_eq!(btn.get_style(), ButtonStyle::Filled);
    assert_eq!(
        btn.get_shape(),
        Shape::pill(),
        "the default shape is the CornerFull pill"
    );
    assert!(
        btn.get_content_padding().is_none(),
        "content_padding defaults per style"
    );
    assert!(
        btn.get_min_size().0.is_none() && btn.get_min_size().1.is_none(),
        "min size defaults to ButtonDefaults"
    );
    assert_eq!(
        btn.get_border(),
        None,
        "border defaults per style (only Outlined has one)"
    );
    assert_eq!(btn.get_modifier().elements().len(), 0);
}

#[test]
fn test_button_defaults_values() {
    assert_eq!(ButtonDefaults::shape(), Shape::pill());
    assert_eq!(ButtonDefaults::min_width(), 58.0);
    assert_eq!(ButtonDefaults::min_height(), 40.0);
    assert_eq!(ButtonDefaults::icon_spacing(), 8.0, "IconLabelSpace");
    // The colour factory agrees with `from_theme` — a thin wrapper, as Compose's `buttonColors()` is.
    let theme = crate::theme::ThemeColors::light_from_seed(0x6750A4);
    assert_eq!(
        ButtonDefaults::button_colors(&theme, ButtonStyle::Filled),
        ButtonColors::from_theme(&theme, ButtonStyle::Filled),
    );
    assert_eq!(
        ButtonDefaults::button_elevation(),
        ButtonElevation::default_elevation()
    );
    assert_eq!(
        ButtonDefaults::elevated_button_elevation(),
        ButtonElevation::elevated()
    );
    let p = ButtonDefaults::content_padding(ButtonStyle::Filled);
    assert!(matches!(p.0, crate::layout::Dimension::Fixed(24.0)));
    assert!(matches!(p.1, crate::layout::Dimension::Fixed(8.0)));
    let p = ButtonDefaults::content_padding(ButtonStyle::Text);
    assert!(matches!(p.0, crate::layout::Dimension::Fixed(12.0)));
    let icon = ButtonDefaults::button_with_icon_content_padding();
    assert!(matches!(icon.0, crate::layout::Dimension::Fixed(16.0)));
    assert!(matches!(icon.2, crate::layout::Dimension::Fixed(24.0)));
    let text_icon = ButtonDefaults::text_button_with_icon_content_padding();
    assert!(matches!(text_icon.0, crate::layout::Dimension::Fixed(12.0)));
    assert!(matches!(text_icon.2, crate::layout::Dimension::Fixed(16.0)));
}

#[test]
fn test_button_border() {
    let b = ButtonBorder::new(2.0, crate::graphics::Color::RED);
    assert_eq!(b.width, 2.0);
    let btn = Button::new().border(b);
    assert_eq!(btn.get_border(), Some(b));
    assert_eq!(btn.get_border().unwrap().color, crate::graphics::Color::RED);
}

#[test]
fn test_outlined_border_uses_theme_outline() {
    // material3's OutlinedButton border is `OutlineVariant` (not `outline` or `primary`).
    let theme = crate::theme::ThemeColors::light_from_seed(0x6750A4);
    let enabled = ButtonBorder::outlined(&theme, true);
    assert_eq!(enabled.width, 1.0);
    assert_eq!(
        enabled.color, theme.outline_variant,
        "an enabled border is OutlineVariant"
    );
    assert_ne!(
        enabled.color, theme.primary,
        "it must not reuse the content colour"
    );
    let disabled = ButtonBorder::outlined(&theme, false);
    assert_eq!(
        disabled.color.a,
        (theme.outline_variant.a as f32 * 0.10) as u8,
        "disabled is OutlineVariant at DisabledContainerOpacity (0.1)"
    );
}

#[test]
fn button_default_colors_match_m3_tokens() {
    let theme = crate::theme::ThemeColors::light_from_seed(0x6750A4);
    let alpha = |c: crate::graphics::Color, a: f32| {
        crate::graphics::Color::from_argb((c.a as f32 * a) as u8, c.r, c.g, c.b)
    };
    let f = ButtonColors::from_theme(&theme, ButtonStyle::Filled);
    assert_eq!(f.container, theme.primary);
    assert_eq!(f.content, theme.on_primary);
    assert_eq!(f.disabled_container, alpha(theme.on_surface, 0.10));
    assert_eq!(f.disabled_content, alpha(theme.on_surface_variant, 0.38));
    let e = ButtonColors::from_theme(&theme, ButtonStyle::Elevated);
    assert_eq!(
        e.container, theme.surface_container_low,
        "Elevated = SurfaceContainerLow"
    );
    assert_eq!(e.content, theme.primary);
    let t = ButtonColors::from_theme(&theme, ButtonStyle::Tonal);
    assert_eq!(t.container, theme.secondary_container);
    assert_eq!(t.content, theme.on_secondary_container);
    assert_eq!(
        t.disabled_container,
        alpha(theme.on_surface, 0.12),
        "Tonal's disabled container is 12%"
    );
    assert_eq!(t.disabled_content, alpha(theme.on_surface, 0.38));
    let o = ButtonColors::from_theme(&theme, ButtonStyle::Outlined);
    assert_eq!(o.container.a, 0, "the Outlined container is transparent");
    assert_eq!(
        o.content, theme.on_surface_variant,
        "Outlined content is OnSurfaceVariant"
    );
    assert_eq!(o.disabled_container.a, 0);
    assert_eq!(o.disabled_content, alpha(theme.on_surface_variant, 0.38));
    let tx = ButtonColors::from_theme(&theme, ButtonStyle::Text);
    assert_eq!(
        tx.content, theme.primary,
        "Text content is Primary (which is what M3 does)"
    );
    assert_eq!(tx.disabled_content, alpha(theme.on_surface_variant, 0.38));
}

#[test]
fn test_button_dynamic_padding_and_min_size() {
    use crate::runtime::state::State;
    let pad = State::new(10.0);
    let btn = Button::new()
        .content_padding((&pad, 4.0, &pad, 4.0))
        .min_size(&pad, 40.0);
    let (p, (mw, mh)) = (btn.get_content_padding().unwrap(), btn.get_min_size());
    assert!(
        matches!(&p.0, Dimension::Dynamic(_)),
        "a State reference becomes Dynamic, so an animation can drive it"
    );
    assert!(matches!(&p.1, Dimension::Fixed(_)));
    assert!(matches!(&mw.unwrap(), Dimension::Dynamic(_)));
    assert!(matches!(&mh.unwrap(), Dimension::Fixed(_)));
}

#[test]
fn test_button_builder() {
    let clicked = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let clicked_clone = clicked.clone();

    let btn = Button::new()
        .on_click(move || clicked_clone.store(true, std::sync::atomic::Ordering::SeqCst))
        .enabled(false)
        .style(ButtonStyle::Outlined)
        .modifier(Modifier::new().size(200.0, 48.0));

    assert!(!btn.get_enabled());
    assert_eq!(btn.get_style(), ButtonStyle::Outlined);
    assert_eq!(btn.get_modifier().elements().len(), 1);

    // `on_click` is moved into the Button now, so it drops with the builder.
}

#[test]
fn test_button_variant_constructors() {
    // material3's each-variant-is-a-composable difference, expressed as default parameters.
    assert_eq!(Button::filled().get_style(), ButtonStyle::Filled);
    assert_eq!(Button::elevated().get_style(), ButtonStyle::Elevated);
    assert_eq!(
        Button::elevated().get_elevation(),
        Some(ButtonElevation::elevated())
    );
    assert_eq!(Button::filled_tonal().get_style(), ButtonStyle::Tonal);
    assert_eq!(Button::outlined().get_style(), ButtonStyle::Outlined);
    assert_eq!(Button::text().get_style(), ButtonStyle::Text);
    assert_eq!(Button::new().get_size(), ButtonSize::Small);
    assert_eq!(
        Button::new().size(ButtonSize::XLarge).get_size(),
        ButtonSize::XLarge
    );
}

#[test]
fn button_size_dimensions() {
    assert_eq!(ButtonSize::XSmall.container_height(), 32.0);
    assert_eq!(ButtonSize::Small.container_height(), 40.0);
    assert_eq!(ButtonSize::Medium.container_height(), 56.0);
    assert_eq!(ButtonSize::Large.container_height(), 96.0);
    assert_eq!(ButtonSize::XLarge.container_height(), 136.0);
    assert_eq!(ButtonSize::XSmall.icon_size(), 20.0);
    assert_eq!(ButtonSize::Small.icon_size(), 20.0);
    assert_eq!(ButtonSize::Medium.icon_size(), 24.0);
    assert_eq!(ButtonSize::Large.icon_size(), 32.0);
    assert_eq!(ButtonSize::XLarge.icon_size(), 40.0);
    assert_eq!(ButtonSize::XSmall.icon_label_space(), 8.0);
    assert_eq!(ButtonSize::Large.icon_label_space(), 12.0);
    assert_eq!(ButtonSize::XLarge.icon_label_space(), 16.0);
    assert_eq!(ButtonSize::XSmall.horizontal_padding(), 16.0);
    // `ButtonSmallTokens.LeadingSpace` is 16, not the baseline's 24: the two are different numbers for the
    // same 40 dp height, and only an explicit `.size(Small)` takes this one (see `Button::size`).
    assert_eq!(ButtonSize::Small.horizontal_padding(), 16.0);
    assert_eq!(ButtonSize::XLarge.horizontal_padding(), 64.0);
    assert_eq!(ButtonSize::Large.outline_width(), 2.0);
    assert_eq!(ButtonSize::XLarge.outline_width(), 3.0);
    assert_eq!(ButtonSize::Small.outline_width(), 1.0);
    assert_eq!(ButtonSize::Medium.outline_width(), 1.0);
    assert_eq!(ButtonSize::Medium.horizontal_padding(), 24.0);
    assert_eq!(ButtonSize::XLarge.icon_label_space(), 16.0);
    // `content_padding_for`: horizontal from the size, and Text keeps a fixed 12
    let p = ButtonDefaults::content_padding_for(ButtonSize::Medium, ButtonStyle::Filled);
    assert!(matches!(p.0, crate::layout::Dimension::Fixed(24.0)));
    let pt = ButtonDefaults::content_padding_for(ButtonSize::XLarge, ButtonStyle::Text);
    assert!(matches!(pt.0, crate::layout::Dimension::Fixed(12.0)));

    // Vertical padding is per size and is what keeps the box at `ContainerHeight`: each value
    // leaves exactly that size's icon height in the content box.
    assert_eq!(ButtonSize::XSmall.vertical_padding(), 6.0);
    assert_eq!(ButtonSize::Small.vertical_padding(), 10.0);
    assert_eq!(ButtonSize::Medium.vertical_padding(), 16.0);
    assert_eq!(ButtonSize::Large.vertical_padding(), 32.0);
    assert_eq!(ButtonSize::XLarge.vertical_padding(), 48.0);
    for size in [
        ButtonSize::XSmall,
        ButtonSize::Small,
        ButtonSize::Medium,
        ButtonSize::Large,
        ButtonSize::XLarge,
    ] {
        assert!(
            (size.vertical_padding() * 2.0 + size.icon_size() - size.container_height()).abs()
                < 0.01,
            "{size:?}: 2 * vertical_padding + icon_size must equal ContainerHeight, got \
             {} + {} = {} against {}",
            size.vertical_padding() * 2.0,
            size.icon_size(),
            size.vertical_padding() * 2.0 + size.icon_size(),
            size.container_height(),
        );

        // And `content_padding_for` has to actually USE that value — the assertion above only
        // relates two constants, so it would keep passing if the builder went back to a flat 8.
        // That is exactly the bug: `Button::build` takes the padding from here.
        let (_, top, _, bottom) = ButtonDefaults::content_padding_for(size, ButtonStyle::Filled);
        for (edge, value) in [("top", top), ("bottom", bottom)] {
            assert!(
                matches!(value, crate::layout::Dimension::Fixed(v) if (v - size.vertical_padding()).abs() < 0.01),
                "{size:?}: content_padding_for's {edge} must be vertical_padding() ({}), got {value:?}",
                size.vertical_padding(),
            );
        }
    }
}

#[test]
fn outlined_border_width_follows_size_variant() {
    // End to end: the materialized node of an XLarge Outlined button carries a 3 dp Border.
    let mut composer = crate::runtime::composer::Composer::new();
    composer.compose(|ctx| {
        Button::outlined()
            .size(ButtonSize::XLarge)
            .on_click(|| {})
            .build(ctx, |ctx| {
                crate::components::Text::new("x").build(ctx);
            });
    });
    composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 300.0));
    let mut width = 0.0f32;
    for node in composer.arena_nodes() {
        for el in node.modifier.elements() {
            if let crate::modifier::ModifierElement::Border { width: w, .. } = el {
                width = w.to_logical_px();
            }
        }
    }
    assert_eq!(width, 3.0, "an XLarge Outlined border is 3 dp");
}

#[test]
fn test_button_composition() {
    let mut composer = Composer::new();

    composer.compose(|ctx| {
        let _btn = Button::new().modifier(Modifier::new().size(100.0, 40.0));

        // Simulate the build: Button is the container.
        let key = ctx.next_key();
        ctx.start_container(key, Modifier::new().size(100.0, 40.0), BoxLayout::new());
        // The child content (a stand-in for Text).
        let text_key = ctx.next_key();
        ctx.start_leaf(text_key, Modifier::new());
        ctx.end_node();
        ctx.end_node();
    });

    // The composition tree is built.
    assert!(composer.layout_root().is_some());
}

/// Compose's buttons carry a 48 dp touch target even when the visual box is smaller: the clickable
/// `Surface` they are built on applies `Modifier.minimumInteractiveComponentSize()`
/// (`material3/Surface.kt:219`, the node at `material3/InteractiveComponentSize.kt:98-140`), so the box is
/// `max(content, 48)` with the 40 dp visual centred inside. Both heights are asserted, because the point of
/// the wrapper is that the VARIABLE box (the background, the ripple) stays 40.
#[test]
fn a_button_reserves_a_48_dp_touch_target() {
    let mut composer = Composer::new();
    composer.compose(|ctx| {
        // Under a Column: the composition root is special-cased and does not show the enlargement.
        crate::layout::components::Column::new()
            .modifier(Modifier::new().fill_max_width())
            .build(ctx, |ctx| {
                Button::filled()
                    .on_click(|| {})
                    .build(ctx, |ctx| crate::components::Text::new("Ok").build(ctx));
            });
    });
    composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
    let nodes = composer.arena_nodes();
    let heights: Vec<f32> = nodes.iter().map(|n| n.measured_size.height).collect();
    let widest = nodes
        .iter()
        .map(|n| n.measured_size.width)
        .fold(0.0f32, f32::max);
    assert!(
        heights.contains(&48.0),
        "the button reserves the 48 dp touch target ({heights:?})"
    );
    assert!(
        heights.iter().any(|h| (h - 40.0).abs() < 0.5),
        "and its visual box is still the 40 dp content box ({heights:?})"
    );
    assert!(
        widest >= 58.0,
        "the box is at least `MinWidth` wide ({widest})"
    );
}

/// A button honours `RowScope::weight`, as a plain box of the same content would.
///
/// `weight` is Compose's `ParentDataModifierNode` — `LayoutWeightNode` writes
/// `RowColumnParentData` onto the resulting `Placeable`
/// (`foundation-layout/RowColumnImpl.kt:485-492`) — so a `Row` reads it through any number of nodes
/// nested in between. winia asks the DIRECT child's modifier and a button always inserts its 48 dp
/// wrapper, so without `Modifier::split_parent_data` the weight lands one level too deep and the
/// button falls back to its touch target.
#[test]
fn a_button_honours_a_row_weight() {
    let mut composer = Composer::new();
    composer.compose(|ctx| {
        crate::layout::components::Row::new()
            .spacing(10.0)
            .build(ctx, |row| {
                for label in ["One", "Two"] {
                    // `weight` is a `RowScope` method: it hands back the modifier this child needs,
                    // and the scope derefs to the ctx the button builds with.
                    let share = row.weight(1.0);
                    Button::filled()
                        .modifier(share)
                        .on_click(|| {})
                        .build(row, |ctx| crate::components::Text::new(label).build(ctx));
                }
            });
    });
    composer.layout(crate::layout::Constraints::new(0.0, 410.0, 0.0, 400.0));
    // Two equal shares of 410 minus one 10 dp gap: each is 200.
    let expected = (410.0 - 10.0) / 2.0;
    let widths: Vec<f32> = composer
        .arena_nodes()
        .iter()
        .map(|n| n.measured_size.width)
        .collect();
    let shares = widths.iter().filter(|w| (*w - expected).abs() < 0.5).count();
    assert_eq!(
        shares, 4,
        "each weighted button must measure {expected}, in both its 48 dp wrapper and the visual box \
         inside it (all widths: {widths:?}) — fewer means the weight stayed on the inner node and \
         the wrapper kept its touch target"
    );
    assert!(
        !widths.iter().any(|w| (*w - 48.0).abs() < 0.5),
        "no button may sit at the bare 48 dp touch-target width once it has a weight \
         (all widths: {widths:?})"
    );
}

/// The elevation animation is directional, as Compose's is (`material3/internal/Elevation.kt:101-118`):
/// 120 ms `FastOutSlowInEasing` towards an interaction, 150 ms of
/// `CubicBezierEasing(0.40, 0.00, 0.60, 1.00)` away from one, and 120 ms of that same curve when what is
/// being left is hover. winia animated every direction with one 180 ms `EaseOutCubic`.
#[test]
fn the_elevation_spec_keys_on_the_interaction_pair() {
    use crate::animation::ElevationInteraction as I;
    let millis = |from: Option<I>, to: Option<I>| match ButtonElevation::spec_for(from, to) {
        Some(crate::animation::AnimationSpec::Tween(t)) => t.duration.as_millis(),
        Some(other) => panic!("an elevation animation is a tween, got {other:?}"),
        None => 0,
    };
    assert_eq!(
        millis(None, Some(I::Hover)),
        120,
        "entering hover is the incoming 120 ms"
    );
    assert_eq!(
        millis(Some(I::Hover), Some(I::Press)),
        120,
        "hover to press is incoming too"
    );
    assert_eq!(
        millis(Some(I::Press), Some(I::Hover)),
        120,
        "releasing into a hover is still an incoming interaction"
    );
    assert_eq!(
        millis(Some(I::Press), None),
        150,
        "back to rest is the outgoing 150 ms"
    );
    assert_eq!(millis(Some(I::Drag), None), 150, "and so is leaving a drag");
    assert_eq!(
        millis(Some(I::Hover), None),
        120,
        "leaving hover is shorter"
    );
    assert_eq!(millis(None, None), 0, "idle to idle snaps, as Compose does");
}

/// A Filled button is flat at rest and carries 1 dp while hovered — `FilledButtonTokens.HoveredContainerElevation`
/// (`FilledTonalButtonTokens` the same) — which winia could never show: `Button::filled()` stored no
/// elevation, the build skipped the layer, and hovering painted nothing. The Outlined style declares no
/// elevation entries at all and stays shadow-free as the control.
///
/// This drives the button's own interaction source and reads the elevation its graphics layer would paint.
#[test]
fn a_hovered_filled_button_raises_the_shadow_it_paints() {
    let _g = crate::animation::tests::TEST_SERIAL
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    crate::animation::clear_all_animations();

    let painted = |button: Button, source: &MutableInteractionSource| -> Option<f32> {
        let src = source.clone();
        let mut composer = Composer::new();
        let button = button.interaction_source(src);
        composer.compose(|ctx| {
            button.on_click(|| {}).build(ctx, |ctx| {
                crate::components::Text::new("Ok").build(ctx);
            });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        composer.arena_nodes().iter().find_map(|node| {
            node.modifier.elements().iter().find_map(|el| match el {
                crate::modifier::ModifierElement::GraphicsLayer { params } => {
                    Some(params.eval().shadow_elevation)
                }
                _ => None,
            })
        })
    };

    let filled_source = MutableInteractionSource::new();
    assert_eq!(
        painted(Button::filled(), &filled_source),
        Some(0.0),
        "a Filled button is flat at rest, but it still carries the layer that will lift it"
    );
    filled_source.emit_hover_enter();
    let mut composer = Composer::new();
    let src = filled_source.clone();
    composer.compose(|ctx| {
        Button::filled()
            .interaction_source(src)
            .on_click(|| {})
            .build(ctx, |ctx| crate::components::Text::new("Ok").build(ctx));
    });
    composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
    // Assert that the animation is registered rather than counting real frames (see the card
    // test): under load a couple of long frames can reach the target.
    assert!(
        crate::animation::is_animating(),
        "hovering registers an elevation animation"
    );
    for _ in 0..600 {
        if !crate::animation::update_animations() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let hovered = composer
        .arena_nodes()
        .iter()
        .find_map(|node| {
            node.modifier.elements().iter().find_map(|el| match el {
                crate::modifier::ModifierElement::GraphicsLayer { params } => {
                    Some(params.eval().shadow_elevation)
                }
                _ => None,
            })
        })
        .expect("still there");
    assert!(
        (hovered - 1.0).abs() < 0.01,
        "hovering raises the shadow to `HoveredContainerElevation` (1), got {hovered}"
    );

    let outlined = MutableInteractionSource::new();
    outlined.emit_hover_enter();
    assert_eq!(
        painted(Button::outlined(), &outlined),
        None,
        "an Outlined button declares no elevation in any state, so it carries no layer"
    );
}

/// A FilledTonal button has NO shadow at rest and 1 dp while hovered — `FilledTonalButtonTokens`:
/// `ContainerElevation`/`PressedContainerElevation`/`FocusContainerElevation` are `ElevationTokens.Level0`
/// and `HoverContainerElevation` is `Level1` (`Button.kt:862-875` wires exactly those into
/// `ButtonDefaults.filledTonalButtonElevation()`).
#[test]
fn a_filled_tonal_button_is_flat_at_rest_and_lifts_on_hover() {
    let _g = crate::animation::tests::TEST_SERIAL
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    crate::animation::clear_all_animations();

    let painted = |source: &MutableInteractionSource| -> Option<f32> {
        let src = source.clone();
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            Button::filled_tonal()
                .interaction_source(src)
                .on_click(|| {})
                .build(ctx, |ctx| {
                    crate::components::Text::new("Ok").build(ctx);
                });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        composer.arena_nodes().iter().find_map(|node| {
            node.modifier.elements().iter().find_map(|el| match el {
                crate::modifier::ModifierElement::GraphicsLayer { params } => {
                    Some(params.eval().shadow_elevation)
                }
                _ => None,
            })
        })
    };

    let at_rest = MutableInteractionSource::new();
    assert_eq!(
        painted(&at_rest),
        Some(0.0),
        "a tonal button paints no shadow at rest"
    );

    // The token sets agree in value; they are still two sets in material3, so keep both named.
    assert_eq!(
        ButtonDefaults::filled_tonal_button_elevation(),
        ButtonDefaults::button_elevation(),
        "FilledTonalButtonTokens and FilledButtonTokens have the same elevation numbers"
    );

    let hovered = MutableInteractionSource::new();
    hovered.emit_hover_enter();
    let src = hovered.clone();
    let mut composer = Composer::new();
    composer.compose(|ctx| {
        Button::filled_tonal()
            .interaction_source(src)
            .on_click(|| {})
            .build(ctx, |ctx| crate::components::Text::new("Ok").build(ctx));
    });
    composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
    for _ in 0..120 {
        if !crate::animation::update_animations() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(8));
    }
    let lifted = composer
        .arena_nodes()
        .iter()
        .find_map(|node| {
            node.modifier.elements().iter().find_map(|el| match el {
                crate::modifier::ModifierElement::GraphicsLayer { params } => {
                    Some(params.eval().shadow_elevation)
                }
                _ => None,
            })
        })
        .expect("the layer stays");
    assert!(
        (lifted - 1.0).abs() < 0.01,
        "hover leans the tonal button up to `HoverContainerElevation` (1 dp), got {lifted}"
    );
}

/// No size named is M3's default button, whose horizontal padding is the BASELINE 24
/// (`BaselineButtonTokens.LeadingSpace`); `.size(Small)` is the expressive size, whose own token is 16
/// (`ButtonSmallTokens.LeadingSpace`). Same 40 dp height, different padding, which is why the builder has
/// to remember whether a size was named.
#[test]
fn a_named_size_takes_its_own_padding_and_the_default_takes_the_baseline() {
    let padding_of = |button: Button| -> f32 {
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            button.on_click(|| {}).build(ctx, |ctx| {
                crate::components::Text::new("Ok").build(ctx);
            });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        let nodes = composer.arena_nodes();
        let root = composer.layout_root_idx().expect("laid out");
        let hit = std::iter::once(root)
            .chain(nodes[root].children.iter().copied())
            .find(|i| nodes[*i].modifier.get_padding_horizontal().0 > 0.0);
        nodes[hit.expect("the button's own chain")]
            .modifier
            .get_padding_horizontal()
            .0
    };
    assert_eq!(
        padding_of(Button::filled()),
        24.0,
        "the default keeps the baseline"
    );
    assert_eq!(
        padding_of(Button::filled().size(ButtonSize::Small)),
        16.0,
        "a named Small takes ButtonSmallTokens.LeadingSpace"
    );
}

#[test]
fn test_button_colors_state_resolution() {
    use crate::graphics::Color;
    let colors = ButtonColors::new(
        Color::RED,
        Color::WHITE,
        Color::from_argb(100, 100, 100, 100),
        Color::from_argb(80, 200, 200, 200),
    );
    // disabled wins
    assert_eq!(
        colors.container_color_for(&ComponentState::disabled()),
        colors.disabled_container
    );
    // The container colour distinguishes enabled/disabled only, as material3's `ButtonColors` does —
    // the hover/focus/press state layer is painted by the ripple indication instead, never stacked
    // on the container colour.
    assert_eq!(
        colors.container_color_for(&ComponentState::idle()),
        colors.container
    );
    assert_eq!(
        colors.container_color_for(&ComponentState {
            hovered: true,
            pressed: true,
            ..ComponentState::idle()
        }),
        colors.container,
    );
    // The content colour distinguishes enabled/disabled only, as material3's `ButtonColors` does.
    assert_eq!(
        colors.content_color_for(&ComponentState::idle()),
        colors.content
    );
    assert_eq!(
        colors.content_color_for(&ComponentState::disabled()),
        colors.disabled_content
    );
}

#[test]
fn test_button_elevation_priority() {
    let e = ButtonElevation::new(1.0, 8.0, 2.0, 2.0, 0.0);
    assert_eq!(e.for_state(&ComponentState::disabled()), 0.0);
    assert_eq!(e.for_state(&ComponentState::idle()), 1.0);
    assert_eq!(
        e.for_state(&ComponentState {
            pressed: true,
            ..ComponentState::idle()
        }),
        8.0
    );
    assert_eq!(
        e.for_state(&ComponentState {
            hovered: true,
            ..ComponentState::idle()
        }),
        2.0
    );
    assert_eq!(
        e.for_state(&ComponentState {
            focused: true,
            ..ComponentState::idle()
        }),
        2.0
    );
    // Order: disabled > dragged > pressed > hovered > focused > default (latest interaction wins)
    // A drag is the latest interaction and Compose's ButtonElevation has no arm for it (`Button.kt:978-988`),
    // so a dragged button sits at its DEFAULT elevation — and its own press is ignored even though every
    // drag holds one.
    assert_eq!(
        e.for_state(&ComponentState {
            dragged: true,
            pressed: true,
            ..ComponentState::idle()
        }),
        1.0,
        "a dragged button takes the default elevation, as Compose's fall-through does"
    );
    assert_eq!(
        e.for_state(&ComponentState {
            pressed: true,
            hovered: true,
            ..ComponentState::idle()
        }),
        8.0
    );
    // When pressed < hovered, a press takes `pressed` per the spec instead of climbing to hover.
    let low_press = ButtonElevation::new(1.0, 1.0, 1.0, 3.0, 0.0);
    assert_eq!(
        low_press.for_state(&ComponentState {
            pressed: true,
            hovered: true,
            ..ComponentState::idle()
        }),
        1.0,
        "a press uses the pressed elevation"
    );
    // ElevatedButton tokens, in dp: rest 1 / pressed 1 / focused 1 / hovered Level2 = 3 / disabled 0
    assert_eq!(
        ButtonElevation::elevated(),
        ButtonElevation::new(1.0, 1.0, 1.0, 3.0, 0.0)
    );
    assert_eq!(
        ButtonElevation::elevated().for_state(&ComponentState::idle()),
        1.0
    );
    assert_eq!(
        ButtonElevation::elevated().for_state(&ComponentState {
            hovered: true,
            ..ComponentState::idle()
        }),
        3.0,
        "`ElevatedButtonTokens.HoveredContainerElevation` is Level2, i.e. 3 dp"
    );
    assert_eq!(
        ButtonElevation::elevated().for_state(&ComponentState {
            pressed: true,
            hovered: true,
            ..ComponentState::idle()
        }),
        1.0
    );
    // Touch screens: a press-ONLY state matches the official token.
    assert_eq!(
        ButtonElevation::elevated().for_state(&ComponentState {
            pressed: true,
            ..ComponentState::idle()
        }),
        1.0
    );
    assert_eq!(
        ButtonElevation::default_elevation().for_state(&ComponentState::idle()),
        0.0
    );
}

/// `PressedContainerShape` per size — the shape a pressed container morphs to. The two ends of
/// the default button are therefore CornerFull (pill) -> CornerSmall (8), which is a visible
/// move on a 40 dp button.
#[test]
fn the_pressed_container_radii_are_the_tokens() {
    for (size, radius) in [
        (ButtonSize::XSmall, 8.0),
        (ButtonSize::Small, 8.0),
        (ButtonSize::Medium, 12.0),
        (ButtonSize::Large, 16.0),
        (ButtonSize::XLarge, 16.0),
    ] {
        assert_eq!(
            size.pressed_container_radius(),
            radius,
            "{size:?}'s PressedContainerShape"
        );
    }
}

/// A press morphs the container's SHAPE, the way upstream's `ButtonShapes(shape, pressedShape)`
/// does through `shapeByInteraction` (`material3/Button.kt:1922-1939`). The default pair is the
/// pill (CornerFull, a percent corner that resolves against the box) and Small's
/// `PressedContainerShape` (CornerSmall, 8). The transition carries both ends because the
/// percent one cannot be resolved at compose time. Deleting the morph leaves `Shape::Pill` on
/// every frame and fails this test.
#[test]
fn a_press_morphs_the_container_shape_over_time() {
    // `update_animations` drives the process-wide table, so this takes the same lock the other
    // animation tests hold.
    let _guard = crate::animation::tests::TEST_SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let theme = crate::theme::ThemeColors::light_from_seed(0x6750A4);
    let source = MutableInteractionSource::new();
    let src = source.clone();
    let mut composer = crate::runtime::composer::Composer::new();
    let mut build = |composer: &mut crate::runtime::composer::Composer| {
        composer.compose(|ctx| {
            crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                Button::new()
                    .interaction_source(src.clone())
                    .on_click(|| {})
                    .build(ctx, |ctx| {
                        crate::components::text::Text::new("Save").build(ctx);
                    });
            });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
    };
    let painted = |composer: &crate::runtime::composer::Composer| -> Shape {
        composer
            .arena_nodes()
            .iter()
            .find_map(|node| {
                node.modifier.elements().iter().find_map(|el| match el {
                    crate::modifier::ModifierElement::Background { shape, .. } => {
                        Some(shape.eval())
                    }
                    _ => None,
                })
            })
            .expect("the container paints a background")
    };

    build(&mut composer);
    assert_eq!(
        painted(&composer),
        Shape::Pill,
        "at rest the default button is the CornerFull pill"
    );

    source.emit_press();
    build(&mut composer);
    // The morph must be REGISTERED and must not have landed yet. `assert_ne!` alone would also
    // pass if nothing had started: the resting shape is a `Pill`, not a `PillToRounded`.
    assert!(
        crate::animation::is_animating(),
        "pressing registers the shape morph"
    );
    // The morph is registered but has not moved yet — the frame that starts a spring still
    // reads its start value, so the shape is the pill at t = 0 here.
    assert_eq!(
        painted(&composer),
        Shape::Pill,
        "the press is registered, not applied in one frame"
    );

    // What makes it a MORPH and not a switch: the corners are seen part-way through. A snap
    // could never produce a `t` strictly between the ends.
    let mut saw_intermediate = false;
    for _ in 0..600 {
        if !crate::animation::update_animations() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
        build(&mut composer);
        if let Shape::PillToRounded { t, .. } = painted(&composer) {
            if (0.01..=0.99).contains(&t) {
                saw_intermediate = true;
            }
        }
    }
    assert!(
        saw_intermediate,
        "the corners travel through intermediate values instead of snapping"
    );
    // Resolved against a box, so this holds whether the settled shape is still the transition
    // or the plain rounded rect its t = 1 end resolves to.
    assert_eq!(
        painted(&composer).resolved_for_box(58.0, 40.0),
        Shape::rounded(8.0),
        "and it arrives at Small's PressedContainerShape (CornerSmall = 8)"
    );
}

/// The press morph is PAINTED, not composed: an animation tick must not dirty a single slot of
/// the button. Before the shape became a paint-time value, the shape closure read the progress
/// with `get()` during composition, so every frame of the spring re-entered the button.
#[test]
fn the_press_morph_does_not_recompose_the_button() {
    let _guard = crate::animation::tests::TEST_SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let theme = crate::theme::ThemeColors::light_from_seed(0x6750A4);
    let source = MutableInteractionSource::new();
    let src = source.clone();
    let mut composer = crate::runtime::composer::Composer::new();
    let mut build = |composer: &mut crate::runtime::composer::Composer| {
        let src = src.clone();
        let theme = theme.clone();
        composer.compose(move |ctx| {
            crate::theme::WiniaTheme::with_theme(theme, ctx, |ctx| {
                Button::new()
                    .interaction_source(src.clone())
                    .on_click(|| {})
                    .build(ctx, |ctx| {
                        crate::components::text::Text::new("Save").build(ctx);
                    });
            });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
    };

    // Settle. The theme provider re-enters on every pass (it has no comparable parameters and no
    // reusable node), so the basline enter count is not zero - what this test pins is that a tick
    // adds no enter BEYOND that baseline and dirties no slot.
    build(&mut composer);
    build(&mut composer);
    let baseline_entered = composer.compose_entered_count;
    assert_eq!(
        composer.compose_dirty_count, 0,
        "a settled frame dirties nothing"
    );
    build(&mut composer);
    assert_eq!(
        (composer.compose_entered_count, composer.compose_dirty_count),
        (baseline_entered, 0),
        "the settled baseline must be stable"
    );

    // The press retargets the animated values. The BODY runs every build (the parent composes it),
    // but the group itself may legitimately Skip: the morph is carried as a Dyn and the new
    // modifier is applied on Skip. What must hold is that the press leaves an animation running;
    // the ticks below then prove it is painted, not composed.
    source.emit_press();
    build(&mut composer);
    build(&mut composer);
    assert!(
        crate::animation::is_animating(),
        "the press must leave an animation running, or the ticks below prove nothing"
    );

    // Tick the spring. A composition-time reader of the morph would enter the group here; a
    // paint-time one leaves the composition alone.
    let (mut worst_entered, mut worst_dirty, mut saw_animation) = (0, 0, false);
    for _ in 0..30 {
        saw_animation |= crate::animation::update_animations();
        std::thread::sleep(std::time::Duration::from_millis(2));
        build(&mut composer);
        worst_entered = worst_entered.max(composer.compose_entered_count);
        worst_dirty = worst_dirty.max(composer.compose_dirty_count);
    }
    assert!(
        saw_animation,
        "the ticks must actually advance an animation"
    );
    assert_eq!(
        (worst_entered, worst_dirty),
        (baseline_entered, 0),
        "no animation tick may re-enter the button beyond the provider baseline \
             (worst frame: entered {worst_entered}, dirty {worst_dirty})"
    );
    crate::animation::clear_all_animations();
}

// ── component stylings ──

fn green() -> crate::graphics::Color {
    crate::graphics::Color::from_argb(255, 0, 128, 0)
}
fn red() -> crate::graphics::Color {
    crate::graphics::Color::from_argb(255, 200, 0, 0)
}

/// The point of the styling layer: one styling, every call site, nothing to change per button.
#[test]
fn a_button_styling_reaches_the_derivation_every_button_uses() {
    let theme = crate::theme::ThemeColors::default_light();
    let plain = button_colors_with_styles(&theme, ButtonStyle::Filled);
    assert_eq!(
        plain.container, theme.primary,
        "the default still comes from the role colour"
    );

    let styled = crate::components::component_styles::with_component_styles(
        crate::components::component_styles::ComponentStyles::new()
            .with(ButtonStyling::new(|scope| scope.container(green()))),
        || button_colors_with_styles(&theme, ButtonStyle::Filled),
    );
    assert_eq!(styled.container, green());
    assert_eq!(
        styled.content, plain.content,
        "only what the styling set changed"
    );
}

#[test]
fn a_button_styling_can_derive_from_the_theme() {
    let theme = crate::theme::ThemeColors::default_light();
    let styled = crate::components::component_styles::with_component_styles(
        crate::components::component_styles::ComponentStyles::new().with(ButtonStyling::new(
            |scope| scope.container(scope.theme.tertiary),
        )),
        || button_colors_with_styles(&theme, ButtonStyle::Filled),
    );
    assert_eq!(styled.container, theme.tertiary);
}

#[test]
fn the_button_styling_ends_with_its_block() {
    let theme = crate::theme::ThemeColors::default_light();
    crate::components::component_styles::with_component_styles(
        crate::components::component_styles::ComponentStyles::new()
            .with(ButtonStyling::new(|scope| scope.container(green()))),
        || {
            assert_eq!(
                button_colors_with_styles(&theme, ButtonStyle::Filled).container,
                green()
            )
        },
    );
    assert_eq!(
        button_colors_with_styles(&theme, ButtonStyle::Filled).container,
        theme.primary,
        "outside the block the theme derivation is back"
    );
}

/// With no styling in scope the resolution has to be byte-identical to the old derivation.
#[test]
fn no_button_styling_means_the_old_derivation() {
    let theme = crate::theme::ThemeColors::default_light();
    for variant in [
        ButtonStyle::Filled,
        ButtonStyle::Elevated,
        ButtonStyle::Tonal,
        ButtonStyle::Outlined,
        ButtonStyle::Text,
    ] {
        let resolved = button_colors_with_styles(&theme, variant);
        let old = ButtonColors::from_theme(&theme, variant);
        assert_eq!(resolved.container, old.container, "{variant:?}");
        assert_eq!(resolved.content, old.content, "{variant:?}");
        assert_eq!(
            resolved.disabled_container, old.disabled_container,
            "{variant:?}"
        );
        assert_eq!(
            resolved.disabled_content, old.disabled_content,
            "{variant:?}"
        );
    }
}

/// Broader then narrower: the registered styling is one function, and later setters win.
#[test]
fn a_narrower_button_styling_wins_over_a_broader_one() {
    let theme = crate::theme::ThemeColors::default_light();
    let broad = ButtonStyling::new(|scope| scope.container(red()));
    let narrow = ButtonStyling::new(|scope| scope.container(green()));
    let styled = crate::components::component_styles::with_component_styles(
        crate::components::component_styles::ComponentStyles::new().with(broad.then(narrow)),
        || button_colors_with_styles(&theme, ButtonStyle::Filled),
    );
    assert_eq!(styled.container, green());
}

/// Light and dark palettes answer `is_dark` differently, so one styling can branch on it.
#[test]
fn a_button_styling_can_style_light_and_dark_differently() {
    let light = crate::theme::ThemeColors::default_light();
    let dark = crate::theme::ThemeColors::default_dark();
    assert!(!crate::components::component_styles::palette_is_dark(
        &light
    ));
    assert!(crate::components::component_styles::palette_is_dark(&dark));

    let styling = ButtonStyling::new(|scope| {
        scope.container(if scope.is_dark { red() } else { green() });
    });
    let styles = crate::components::component_styles::ComponentStyles::new().with(styling);
    let on_light =
        crate::components::component_styles::with_component_styles(styles.clone(), || {
            button_colors_with_styles(&light, ButtonStyle::Filled)
        });
    let on_dark = crate::components::component_styles::with_component_styles(styles, || {
        button_colors_with_styles(&dark, ButtonStyle::Filled)
    });
    assert_eq!(on_light.container, green());
    assert_eq!(on_dark.container, red());
}

/// The build path really uses the styling - a resolver-only test would pass if build reverted to
/// `ButtonColors::from_theme`.
#[test]
fn a_composed_button_paints_the_class_styling() {
    let theme = crate::theme::ThemeColors::default_light();
    let mut composer = crate::runtime::composer::Composer::new();
    let styled = green();
    crate::components::component_styles::with_component_styles(
        crate::components::component_styles::ComponentStyles::new()
            .with(ButtonStyling::new(move |scope| scope.container(styled))),
        || {
            composer.compose(|ctx| {
                crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    Button::filled().build(ctx, |ctx| {
                        crate::components::text::Text::new("Save").build(ctx);
                    });
                });
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        },
    );

    let painted = composer
        .arena_nodes()
        .iter()
        .find_map(|node| {
            node.modifier.elements().iter().find_map(|el| match el {
                crate::modifier::ModifierElement::Background { color, .. } => Some(color.eval()),
                _ => None,
            })
        })
        .expect("the container paints a background");
    assert_eq!(
        painted,
        green(),
        "build must resolve through the styling layer"
    );
}

/// An explicit parameter still beats the class styling.
#[test]
fn an_explicit_colour_beats_the_class_styling() {
    let theme = crate::theme::ThemeColors::default_light();
    let explicit = crate::graphics::Color::from_argb(255, 0, 0, 255);
    let mut composer = crate::runtime::composer::Composer::new();
    crate::components::component_styles::with_component_styles(
        crate::components::component_styles::ComponentStyles::new()
            .with(ButtonStyling::new(|scope| scope.container(green()))),
        || {
            composer.compose(|ctx| {
                crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    Button::filled()
                        .colors(ButtonColors::new(explicit, explicit, explicit, explicit))
                        .build(ctx, |ctx| {
                            crate::components::text::Text::new("Save").build(ctx);
                        });
                });
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        },
    );

    let painted = composer
        .arena_nodes()
        .iter()
        .find_map(|node| {
            node.modifier.elements().iter().find_map(|el| match el {
                crate::modifier::ModifierElement::Background { color, .. } => Some(color.eval()),
                _ => None,
            })
        })
        .expect("the container paints a background");
    assert_eq!(
        painted, explicit,
        "explicit parameter > class styling > theme"
    );
}

/// A page set layers on top of the application set without copying it.
#[test]
fn a_merged_set_lets_the_page_win_per_class() {
    let theme = crate::theme::ThemeColors::default_light();
    let base = crate::components::component_styles::ComponentStyles::new()
        .with(ButtonStyling::new(|scope| scope.container(red())));
    let page = crate::components::component_styles::ComponentStyles::new()
        .with(ButtonStyling::new(|scope| scope.container(green())));
    let resolved =
        crate::components::component_styles::with_component_styles(base.merge(page), || {
            button_colors_with_styles(&theme, ButtonStyle::Filled)
        });
    assert_eq!(resolved.container, green());
}

/// A Button repaints its label when the CONTENT colour changes — the container moved and the text
/// stayed behind (measured on a checked/unchecked toggle: a dark container kept the old light
/// label and vice versa).
///
/// `WiniaTheme::with_content_color` and `ProvideTextStyle` are `CompositionLocal`s. A local does not
/// invalidate its readers, so the inner `Row` — which declares nothing that changed — Skips, the
/// providers never re-run, and the label keeps painting the colour it was composed with while the
/// background (computed in `build` itself) moves on. That is the defect `badge.rs` records, and this
/// pins the same fix for `Button`: the provided entries carry a signal, reading one subscribes, and
/// the label is re-entered because it read the value.
#[test]
fn a_button_repaints_its_label_when_the_content_colour_changes() {
    use crate::graphics::Color;
    let theme = crate::theme::ThemeColors::light_from_seed(0x6750A4);
    let disabled_container = Color::from_argb(128, 0, 0, 0);
    let disabled_content = Color::from_argb(97, 0, 0, 0);
    let dark_container = Color::from_argb(255, 20, 24, 30);
    let light_label = Color::from_argb(255, 245, 246, 250);
    let light_container = Color::from_argb(255, 210, 200, 240);
    let dark_label = Color::from_argb(255, 6, 8, 12);

    let colours = std::cell::Cell::new(ButtonColors::new(
        dark_container,
        light_label,
        disabled_container,
        disabled_content,
    ));
    let mut composer = crate::runtime::composer::Composer::new();
    let scene = |ctx: &mut ComposeCtx| {
        let colours = colours.get();
        crate::theme::WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
            Button::filled()
                .colors(colours)
                .on_click(|| {})
                .build(ctx, |ctx| crate::components::text::Text::new("Day").build(ctx));
        });
    };
    composer.compose(scene);
    composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
    // Flip BOTH halves, the way a toggle moves between checked and unchecked.
    colours.set(ButtonColors::new(
        light_container,
        dark_label,
        disabled_container,
        disabled_content,
    ));
    // A frame's own path: the content colour reaches the label through a provider, whose notify
    // lands in the queue during this pass and dirties the reader on the next one.
    composer.recompose_to_stable(|| scene);
    composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));

    let nodes = composer.arena_nodes();
    let mut painted_container = None;
    let mut painted_label = None;
    for el in nodes.iter().flat_map(|node| node.modifier.elements()) {
        match el {
            crate::modifier::ModifierElement::Background { color, .. }
                if painted_container.is_none() =>
            {
                painted_container = Some(color.eval())
            }
            crate::modifier::ModifierElement::TextContent { content, color, .. }
                if content.as_str() == "Day" =>
            {
                painted_label = Some(*color)
            }
            _ => {}
        }
    }
    assert_eq!(
        painted_container,
        Some(light_container),
        "the container moved to the new colour"
    );
    assert_eq!(
        painted_label,
        Some(dark_label),
        "the label repaints in the NEW content colour instead of the one it was composed with"
    );
}
