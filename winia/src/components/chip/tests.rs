use super::*;
use crate::components::{Icon, Text};
use crate::layout::components::Stack;
use crate::layout::sizing::{Dimension, SizeValue};
use crate::runtime::composer::Composer;
use crate::text::{FontWeight, TextStyle};
use crate::theme::Typography;
use crate::unit::{Sp, TextUnit};

/// Compose one chip and hand back the composer plus its container node (the chip's own box).
fn chip_container(chip: Chip) -> (Composer, usize) {
    let mut composer = Composer::new();
    composer.compose(|ctx| chip.build(ctx));
    composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
    let root = composer.layout_root_idx().expect("root");
    let nodes = composer.arena_nodes();
    // A single chip is the root itself unless a wrapper was inserted; find the node carrying the outline
    // or the background.
    let inner = if nodes[root]
        .modifier
        .elements()
        .iter()
        .any(|el| matches!(el, crate::modifier::ModifierElement::Background { .. }))
    {
        root
    } else {
        nodes[root]
            .children
            .first()
            .copied()
            .expect("the chip's box")
    };
    (composer, inner)
}

fn outline_of(composer: &Composer, idx: usize) -> Option<(f32, crate::graphics::Color)> {
    composer.arena_nodes()[idx]
        .modifier
        .elements()
        .iter()
        .find_map(|el| match el {
            crate::modifier::ModifierElement::Border { width, color, .. } => {
                Some((width.to_logical_px(), color.eval()))
            }
            _ => None,
        })
}

fn fixed(v: &Dimension) -> Option<f32> {
    match v {
        Dimension::Fixed(dp) => Some(*dp),
        _ => None,
    }
}

/// The container colour the chip paints (its `background` element holds a closure).
fn painted_background(composer: &Composer, idx: usize) -> crate::graphics::Color {
    composer.arena_nodes()[idx]
        .modifier
        .elements()
        .iter()
        .find_map(|el| match el {
            crate::modifier::ModifierElement::Background { color, .. } => Some(color.eval()),
            _ => None,
        })
        .expect("the chip paints a container")
}

/// The horizontal padding the chip asks for, (start, end) — the resolved sums, so it does not
/// depend on which sides an element happens to leave at zero.
fn padding_of(composer: &Composer, idx: usize) -> (Option<f32>, Option<f32>) {
    let (s, _, e, _) = composer.arena_nodes()[idx].modifier.get_padding_sides();
    (Some(s), Some(e))
}

fn label(text: &'static str) -> impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static {
    move |ctx: &mut ComposeCtx| Text::new(text).build(ctx)
}

/// The specs page lists the assist chip's label role as **On surface** and the suggestion chip's as
/// **On surface variant**; winia used the suggestion value for both.
#[test]
fn assist_and_suggestion_labels_are_different_roles() {
    let theme = crate::theme::WiniaTheme::colors();
    assert_eq!(
        ChipDefaults::assist_chip_colors(&theme).label,
        theme.on_surface
    );
    assert_eq!(
        ChipDefaults::suggestion_chip_colors(&theme).label,
        theme.on_surface_variant
    );
}

/// The specs page: assist and suggestion chips outline with `Outline`, filter and input chips with
/// `Outline variant`; a selected chip has no outline at all.
#[test]
fn the_outline_role_follows_the_variant() {
    let theme = crate::theme::WiniaTheme::colors();
    let (assist, a_idx) = chip_container(Chip::assist(label("A"), || {}));
    assert_eq!(outline_of(&assist, a_idx), Some((1.0, theme.outline)));
    let (suggestion, s_idx) = chip_container(Chip::suggestion(label("S"), || {}));
    assert_eq!(outline_of(&suggestion, s_idx), Some((1.0, theme.outline)));
    let (filter, f_idx) = chip_container(Chip::filter(false, label("F"), || {}));
    assert_eq!(
        outline_of(&filter, f_idx),
        Some((1.0, theme.outline_variant))
    );
    let (selected, s2_idx) = chip_container(Chip::filter(true, label("F"), || {}));
    assert_eq!(
        outline_of(&selected, s2_idx),
        None,
        "a selected chip draws no outline"
    );
    let (input, i_idx) = chip_container(Chip::input(false, label("I"), || {}));
    assert_eq!(
        outline_of(&input, i_idx),
        Some((1.0, theme.outline_variant))
    );
}

/// The specs page gives an input chip's leading icon the Primary role, so it stays Primary when the chip
/// is selected; a filter chip's turns On secondary container.
#[test]
fn an_input_chips_selected_leading_icon_stays_primary() {
    let theme = crate::theme::WiniaTheme::colors();
    assert_eq!(
        ChipDefaults::selectable_chip_colors(&theme).selected_leading_icon,
        theme.on_secondary_container
    );
    assert_eq!(
        ChipDefaults::input_chip_colors(&theme).selected_leading_icon,
        theme.primary
    );
}

/// The specs page's padding rules: 16 dp with nothing at the edge, 8 dp beside an icon, and for an input
/// chip "left padding for avatar 4 dp, right padding for avatar 8 dp".
#[test]
fn the_padding_is_the_specs_rule() {
    let (plain, p_idx) = chip_container(Chip::assist(label("A"), || {}));
    assert_eq!(padding_of(&plain, p_idx), (Some(16.0), Some(16.0)));
    let (icon, i_idx) = chip_container(Chip::assist(label("A"), || {}).leading_icon(|ctx| {
        Icon::new(crate::graphics::IconSource::svg_path(
            "M12 3v10h8v2H10V3h2z",
        ))
        .build(ctx);
    }));
    assert_eq!(padding_of(&icon, i_idx), (Some(8.0), Some(8.0)));

    let (bare_input, b_idx) = chip_container(Chip::input(false, label("I"), || {}));
    assert_eq!(padding_of(&bare_input, b_idx), (Some(16.0), Some(16.0)));
    let (with_avatar, av_idx) =
        chip_container(Chip::input(false, label("I"), || {}).avatar(|ctx| {
            Stack::new().build(ctx, |_| {});
        }));
    assert_eq!(
        padding_of(&with_avatar, av_idx),
        (Some(4.0), Some(16.0)),
        "an avatar sits 4 dp from the leading edge (the specs page's left padding for avatar); the trailing \
             edge has nothing in it, so it keeps the default 16 dp"
    );
    let (with_close, c_idx) =
        chip_container(Chip::input(false, label("I"), || {}).trailing_icon(|ctx| {
            Icon::new(crate::graphics::IconSource::svg_path(
                "M19 6.41 12 13.4 5 6.41 3.59 8 12 16.4 20.41 8z",
            ))
            .build(ctx);
        }));
    assert_eq!(padding_of(&with_close, c_idx), (Some(16.0), Some(8.0)));
}

/// The elevation sets, in dp: a flat chip is Level0 everywhere except while dragged (`Level4` = 8); a filter
/// chip lifts 1 dp on hover (`FlatSelectedHoverContainerElevation` = `Level1`); the elevated variants are
/// `Level1` at rest and `Level2` (3 dp) hovered.
#[test]
fn the_elevation_sets_are_the_tokens_in_dp() {
    use crate::input::interaction::ComponentState;
    assert_eq!(
        ChipElevation::flat().for_state(&ComponentState::idle()),
        0.0
    );
    assert_eq!(
        ChipElevation::flat().for_state(&ComponentState {
            dragged: true,
            pressed: true,
            ..ComponentState::idle()
        }),
        8.0,
        "a dragged chip rises to Level4, and the drag outranks the press it holds"
    );
    assert_eq!(
        ChipElevation::hover_lifting().for_state(&ComponentState {
            hovered: true,
            ..ComponentState::idle()
        }),
        1.0
    );
    assert_eq!(
        ChipElevation::elevated().for_state(&ComponentState {
            hovered: true,
            ..ComponentState::idle()
        }),
        3.0,
        "Level2 is 3 dp, not 2"
    );
    assert_eq!(
        ChipElevation::elevated().for_state(&ComponentState::disabled()),
        0.0
    );
    assert!(
        !ChipElevation::flat().is_flat(),
        "a flat chip still lifts while dragged"
    );
    assert!(ChipElevation::new(0.0, 0.0, 0.0, 0.0, 0.0, 0.0).is_flat());
}

/// The transparency rule: a chip whose container is transparent shows NO shadow, whatever its state's token
/// value says — a shadow behind a transparent fill reads as a smudge. During the selection fade the alpha
/// decides, so the filled-to-transparent direction drops the shadow as soon as the fill starts leaving and
/// the transparent-to-filled direction shows none until it has arrived.
#[test]
fn a_transparent_container_never_shadows() {
    use crate::input::interaction::ComponentState;
    let hovered = ComponentState {
        hovered: true,
        ..ComponentState::idle()
    };
    let transparent = 0u8;
    let fading_in = 128u8;
    let opaque = 255u8;

    // A flat filter chip hovered: the token says Level1, but nothing may be painted.
    assert_eq!(
        ChipElevation::shadow_for(&hovered, transparent, &ChipElevation::hover_lifting()),
        0.0
    );
    assert_eq!(
        ChipElevation::shadow_for(&hovered, fading_in, &ChipElevation::hover_lifting()),
        0.0,
        "and not while the fill is still on its way in"
    );
    assert_eq!(
        ChipElevation::shadow_for(&hovered, opaque, &ChipElevation::hover_lifting()),
        1.0,
        "once the fill is there, the token's hover lift shows"
    );

    // An assist or suggestion chip has a transparent container in every state.
    let flat = ChipElevation::flat();
    let dragged = ComponentState {
        dragged: true,
        ..ComponentState::idle()
    };
    assert_eq!(ChipElevation::shadow_for(&dragged, transparent, &flat), 0.0);
    assert_eq!(
        ChipElevation::shadow_for(&dragged, opaque, &flat),
        8.0,
        "were its container ever opaque, the drag lift would be Level4"
    );

    // The elevated variants are filled at rest, so they lift by their tokens.
    let elevated = ChipElevation::elevated();
    assert_eq!(
        ChipElevation::shadow_for(&ComponentState::idle(), opaque, &elevated),
        1.0
    );
    assert_eq!(ChipElevation::shadow_for(&hovered, opaque, &elevated), 3.0);
    assert_eq!(
        ChipElevation::shadow_for(&ComponentState::idle(), transparent, &elevated),
        0.0
    );
}

/// The elevated variants fill with `SurfaceContainerLow` (the specs page's "surface container low
/// (optional)" role) and carry no outline; the flat ones stay transparent with their outline.
#[test]
fn elevated_chips_fill_and_drop_the_outline() {
    let theme = crate::theme::WiniaTheme::colors();
    let (flat, f_idx) = chip_container(Chip::assist(label("A"), || {}));
    assert_eq!(
        painted_background(&flat, f_idx),
        crate::graphics::Color::TRANSPARENT,
        "a flat assist chip has no fill, so it can never show a shadow"
    );
    assert!(outline_of(&flat, f_idx).is_some(), "and it is outlined");

    let (elev, e_idx) = chip_container(Chip::elevated_assist(label("E"), || {}));
    assert_eq!(
        painted_background(&elev, e_idx),
        theme.surface_container_low
    );
    assert_eq!(
        outline_of(&elev, e_idx),
        None,
        "an elevated chip has no outline"
    );

    let (elev_filter, ef_idx) = chip_container(Chip::elevated_filter(false, label("F"), || {}));
    assert_eq!(
        painted_background(&elev_filter, ef_idx),
        theme.surface_container_low
    );
    let (elev_filter_on, efo_idx) = chip_container(Chip::elevated_filter(true, label("F"), || {}));
    assert_eq!(
        painted_background(&elev_filter_on, efo_idx),
        theme.secondary_container,
        "a selected elevated filter chip keeps the selected container"
    );
}

/// The shadow the chip's layer would paint right now (the layer's closure reads the animated state).
fn painted_shadow(composer: &Composer) -> f32 {
    composer
        .arena_nodes()
        .iter()
        .find_map(|n| {
            n.modifier.elements().iter().find_map(|el| match el {
                crate::modifier::ModifierElement::GraphicsLayer { params } => {
                    Some(params.eval().shadow_elevation)
                }
                _ => None,
            })
        })
        .expect("the chip keeps its elevation layer")
}

/// Drive frames until the animations settle, re-composing each frame the way a real window does.
fn settle_shadow(
    composer: &mut Composer,
    source: crate::input::interaction::MutableInteractionSource,
    selected: bool,
) {
    for _ in 0..200 {
        if !crate::animation::update_animations() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(3));
        build_filter_chip(composer, source.clone(), selected);
    }
}

fn build_filter_chip(
    composer: &mut Composer,
    source: crate::input::interaction::MutableInteractionSource,
    selected: bool,
) {
    composer.compose(move |ctx: &mut ComposeCtx| {
        Chip::filter(selected, label("F"), || {})
            .interaction_source(source.clone())
            .build(ctx);
    });
    composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
}

/// The caller-supplied interaction source drives the chip's state, and the shadow follows Compose's
/// interaction-pair rule in BOTH directions: a hover lift RAMPS IN over the incoming spec, and leaving hover
/// fades out over the outgoing one — the same 120 ms `Button` and `Card` use. A fall that the container's own
/// transparency causes is the exception and still snaps; `a_container_going_transparent_drops_the_shadow_at_once`
/// pins that half.
#[test]
fn a_hover_lift_ramps_in_and_fades_out() {
    let _g = crate::animation::tests::TEST_SERIAL
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    crate::animation::clear_all_animations();
    // A SELECTED filter chip: its container is filled, so the transparency gate lets the shadow through.
    let source = crate::input::interaction::MutableInteractionSource::new();
    let mut composer = Composer::new();

    source.emit_hover_enter();
    build_filter_chip(&mut composer, source.clone(), true);
    settle_shadow(&mut composer, source.clone(), true);
    assert!(
        painted_shadow(&composer) > 0.5,
        "a hovered selected filter chip lifts to Level1 (got {})",
        painted_shadow(&composer)
    );

    source.emit_hover_exit();
    build_filter_chip(&mut composer, source.clone(), true);
    let after_exit = painted_shadow(&composer);
    assert!(
        after_exit > 0.5,
        "leaving hover fades the lift out over the outgoing spec instead of cutting it \
         (painted {after_exit} on the frame the state changed)"
    );
    settle_shadow(&mut composer, source.clone(), true);
    assert!(
        painted_shadow(&composer) <= 0.001,
        "and it is gone once the spec has run (got {})",
        painted_shadow(&composer)
    );

    source.emit_hover_enter();
    build_filter_chip(&mut composer, source.clone(), true);
    let mid = painted_shadow(&composer);
    assert!(
        mid < 0.5,
        "hovering again ramps the lift in over the pair spec (painted {mid} right after the state change)"
    );
    settle_shadow(&mut composer, source.clone(), true);
    assert!(
        painted_shadow(&composer) > 0.5,
        "and it arrives once the spec has run (got {})",
        painted_shadow(&composer)
    );
}

/// The transparency gate outranks the outgoing spec: once the fill starts leaving, the shadow is gone on that
/// frame, because a lift with nothing under it reads as a smudge. Deselecting a chip whose container has just
/// been carrying a hover lift therefore steps to zero — no in-between value, which is what tells a snap apart
/// from the fade the test above pins.
#[test]
fn a_container_going_transparent_drops_the_shadow_at_once() {
    let _g = crate::animation::tests::TEST_SERIAL
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    crate::animation::clear_all_animations();
    let source = crate::input::interaction::MutableInteractionSource::new();
    let mut composer = Composer::new();

    source.emit_hover_enter();
    build_filter_chip(&mut composer, source.clone(), true);
    settle_shadow(&mut composer, source.clone(), true);
    assert!(
        painted_shadow(&composer) > 0.5,
        "the chip starts out lifted (got {})",
        painted_shadow(&composer)
    );

    // Same hovered state, now drifting out of selection: only the container is on its way to transparent.
    // The first build is what starts that fade, so it has to come before the frame loop — reading
    // `update_animations` first would find nothing running and stop on the spot.
    build_filter_chip(&mut composer, source.clone(), false);
    let mut seen = vec![painted_shadow(&composer)];
    for _ in 0..200 {
        if !crate::animation::update_animations() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(3));
        build_filter_chip(&mut composer, source.clone(), false);
        seen.push(painted_shadow(&composer));
        if seen.last().is_some_and(|s| *s <= 0.001) {
            break;
        }
    }
    assert!(
        seen.last().is_some_and(|s| *s <= 0.001),
        "the shadow is gone by the time the fill has left (trail {:?})",
        &seen[seen.len().saturating_sub(6)..]
    );
    assert!(
        seen.iter().all(|s| *s <= 0.001 || *s >= 0.5),
        "and it steps straight to zero rather than easing down behind the leaving fill (trail {:?})",
        &seen[seen.len().saturating_sub(6)..]
    );
}

/// A disabled chip's outline is `OnSurface` at 12 % (`FlatDisabledOutlineColor` /
/// `FlatDisabledOutlineOpacity` and their per-variant names), not the enabled role's colour; an elevated
/// chip still carries no outline in any state.
#[test]
fn a_disabled_chip_outlines_with_on_surface_at_twelve_percent() {
    let theme = crate::theme::WiniaTheme::colors();
    // The expectation is built HERE from the scheme colour and the token's opacity, never from
    // `ChipDefaults::disabled_outline`: an expectation taken from the function under test would accept a
    // wrong base colour, which is exactly the mistake this test exists to catch.
    let expected = crate::graphics::Color::from_argb(
        31,
        theme.on_surface.r,
        theme.on_surface.g,
        theme.on_surface.b,
    );
    assert_eq!(expected.a, 31, "12 % of 255 rounds to 31");
    assert_ne!(
        expected, theme.outline,
        "the disabled role is NOT the enabled outline colour"
    );
    assert_eq!(
        ChipDefaults::disabled_outline(&theme),
        expected,
        "and the helper the component asks agrees with that"
    );

    let (assist, a) = chip_container(Chip::assist(label("A"), || {}).enabled(false));
    assert_eq!(outline_of(&assist, a), Some((1.0, expected)), "assist");
    let (suggestion, s) = chip_container(Chip::suggestion(label("S"), || {}).enabled(false));
    assert_eq!(
        outline_of(&suggestion, s),
        Some((1.0, expected)),
        "suggestion"
    );
    let (filter, f) = chip_container(Chip::filter(false, label("F"), || {}).enabled(false));
    assert_eq!(outline_of(&filter, f), Some((1.0, expected)), "filter");
    let (input, i) = chip_container(Chip::input(false, label("I"), || {}).enabled(false));
    assert_eq!(outline_of(&input, i), Some((1.0, expected)), "input");

    let (enabled, e) = chip_container(Chip::assist(label("A"), || {}));
    assert_eq!(
        outline_of(&enabled, e),
        Some((1.0, theme.outline)),
        "enabled keeps its role colour"
    );
    let (elevated, el) = chip_container(Chip::elevated_assist(label("E"), || {}).enabled(false));
    assert_eq!(
        outline_of(&elevated, el),
        None,
        "an elevated chip never outlines"
    );
}

/// A disabled SELECTED chip's container is `OnSurface` at 12 % (`FlatDisabledSelectedContainerColor` /
/// `FlatDisabledSelectedContainerOpacity`, `DisabledSelectedContainerColor` / `DisabledSelectedContainerOpacity`),
/// where winia painted 38 %. The enabled selected container stays `secondaryContainer` as the control, and a
/// disabled UNSELECTED chip stays transparent.
#[test]
fn a_disabled_selected_chip_fills_with_on_surface_at_twelve_percent() {
    let theme = crate::theme::WiniaTheme::colors();
    let expected = crate::graphics::Color::from_argb(
        (255.0 * ChipDefaults::DISABLED_SELECTED_CONTAINER_OPACITY).round() as u8,
        theme.on_surface.r,
        theme.on_surface.g,
        theme.on_surface.b,
    );
    assert_eq!(expected.a, 31, "12 % of 255 rounds to 31");

    let (filter, f) = chip_container(Chip::filter(true, label("F"), || {}).enabled(false));
    assert_eq!(
        painted_background(&filter, f),
        expected,
        "a disabled selected filter chip"
    );
    let (input, i) = chip_container(Chip::input(true, label("I"), || {}).enabled(false));
    assert_eq!(painted_background(&input, i), expected, "and an input chip");

    let (on, o) = chip_container(Chip::filter(true, label("F"), || {}));
    assert_eq!(
        painted_background(&on, o),
        theme.secondary_container,
        "enabled keeps the selected container"
    );
    let (off, u) = chip_container(Chip::filter(false, label("F"), || {}).enabled(false));
    assert_eq!(
        painted_background(&off, u),
        crate::graphics::Color::TRANSPARENT,
        "a disabled UNSELECTED chip has no fill, so its disabled outline carries the state"
    );
}

/// material3 gives each chip family its own role: the non-selectable ones are `Role.Button`
/// (`Chip.kt:1963`, the `Surface(onClick = …)` behind assist/suggestion and their elevated forms) and the
/// selectable ones `Role.Checkbox` (`:2010`), whose `Surface(selected = …)` publishes the selected state
/// beside the role. winia declared no role at all, so a reader saw an unnamed box.
#[test]
fn the_variants_declare_the_roles_material3_gives_them() {
    use crate::semantics::{SemanticsRole, SemanticsState};
    // Composing a chip drives its container-colour animation; the registry is global, so hold the lock.
    let _g = crate::animation::tests::TEST_SERIAL
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    crate::animation::clear_all_animations();

    let only_element = |chip: Chip| -> crate::semantics::SemanticsNode {
        let mut composer = Composer::new();
        composer.compose(|ctx| chip.build(ctx));
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        let root = composer.layout_root_idx().expect("root");
        let tree = crate::semantics::semantics_tree(composer.arena_nodes(), root);
        assert_eq!(
            tree.len(),
            1,
            "a chip is ONE accessibility element, label and icons included"
        );
        tree.into_iter().next().expect("the chip element")
    };

    for (name, chip) in [
        ("assist", Chip::assist(label("A"), || {})),
        ("elevated assist", Chip::elevated_assist(label("A"), || {})),
        ("suggestion", Chip::suggestion(label("S"), || {})),
        (
            "elevated suggestion",
            Chip::elevated_suggestion(label("S"), || {}),
        ),
    ] {
        let element = only_element(chip);
        assert_eq!(
            element.role,
            Some(SemanticsRole::Button),
            "{name} is a button"
        );
        assert_eq!(
            element.state.selected_value(),
            None,
            "{name} says nothing about selection — it has none"
        );
    }

    for (name, chip) in [
        ("filter (selected)", Chip::filter(true, label("F"), || {})),
        (
            "filter (unselected)",
            Chip::filter(false, label("F"), || {}),
        ),
        (
            "elevated filter",
            Chip::elevated_filter(true, label("F"), || {}),
        ),
        ("input", Chip::input(true, label("I"), || {})),
    ] {
        let element = only_element(chip);
        assert_eq!(
            element.role,
            Some(SemanticsRole::Checkbox),
            "{name} is a checkbox"
        );
        assert!(
            element.state.selected_value().is_some(),
            "{name} must state its selection either way, so a reader can tell checked from unchecked"
        );
    }

    assert_eq!(
        only_element(Chip::filter(true, label("F"), || {}))
            .state
            .selected_value(),
        Some(true),
        "a selected filter chip reads as selected"
    );
    assert_eq!(
        only_element(Chip::filter(false, label("F"), || {}))
            .state
            .selected_value(),
        Some(false),
        "and an unselected one as unselected, not as silent"
    );
    assert_eq!(
        only_element(Chip::assist(label("A"), || {}).enabled(false))
            .state
            .enabled_value(),
        Some(false),
        "a disabled chip says so (every variant publishes enabled)"
    );
    assert!(
        SemanticsState::new().is_unspecified(),
        "the helper above is only a constructor"
    );
}

fn find_text_style(
    nodes: &[crate::layout::node::LayoutNode],
    idx: usize,
) -> Option<(f32, FontWeight, f32, Option<f32>)> {
    for el in nodes[idx].modifier.elements() {
        if let crate::modifier::ModifierElement::TextContent {
            font_size,
            font_weight,
            letter_spacing,
            line_height,
            ..
        } = el
        {
            return Some((*font_size, *font_weight, *letter_spacing, *line_height));
        }
    }
    for &child in &nodes[idx].children {
        if let Some(style) = find_text_style(nodes, child) {
            return Some(style);
        }
    }
    None
}

#[test]
fn label_uses_typography_label_large() {
    let custom = Typography {
        label_large: TextStyle::new()
            .font_size(TextUnit::Sp(Sp(19.0)))
            .line_height(27.0)
            .letter_spacing(1.3)
            .font_weight(FontWeight::BOLD),
        ..Typography::default()
    };
    let mut composer = Composer::new();
    composer.compose(|ctx| {
        crate::theme::WiniaTheme::with_typography(custom, ctx, |ctx| {
            Chip::assist(
                |ctx| crate::components::Text::new("Label").build(ctx),
                || {},
            )
            .build(ctx);
        });
    });
    let root = composer.layout_root_idx().unwrap();
    let style = find_text_style(composer.arena_nodes(), root).unwrap();
    assert_eq!(style.0, 19.0);
    assert_eq!(style.1, FontWeight::BOLD);
    assert_eq!(style.2, 1.3);
    assert_eq!(style.3, Some(27.0));
}

// ── The behaviour fixes (recorded in `docs/chip.md`, "Behaviour") ──

/// Every `DrawIcon` tint the chip paints, in document order.
fn icon_tints(composer: &Composer) -> Vec<Option<crate::graphics::Color>> {
    let mut out = Vec::new();
    for node in composer.arena_nodes() {
        for el in node.modifier.elements() {
            if let crate::modifier::ModifierElement::DrawIcon { spec } = el {
                out.push(spec.tint.as_ref().map(|t| t.eval()));
            }
        }
    }
    out
}

fn an_icon(path: &'static str) -> impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static {
    move |ctx: &mut ComposeCtx| {
        Icon::new(crate::graphics::IconSource::svg_path(path)).build(ctx);
    }
}

/// The LEADING and TRAILING icons take their OWN colour slots.
///
/// Both were fed one `icon_color`, so the three `trailing_icon` slots on the two colour structs were
/// written by `ChipDefaults` and never read. Measured before the fix on the default light theme: the
/// trailing glyph painted (101, 85, 143) — the leading role — where the specs page gives it
/// `On surface variant` (73, 69, 78).
#[test]
fn a_trailing_icon_takes_the_trailing_role() {
    let theme = crate::theme::WiniaTheme::colors();
    let (composer, _) = chip_container(
        Chip::assist(label("A"), || {})
            .leading_icon(an_icon("M0 0h1v1z"))
            .trailing_icon(an_icon("M0 0h2v2z")),
    );
    let tints = icon_tints(&composer);
    assert_eq!(tints.len(), 2, "both icons paint");
    assert_eq!(
        tints[0],
        Some(theme.primary),
        "the leading icon takes Primary"
    );
    assert_eq!(
        tints[1],
        Some(theme.on_surface_variant),
        "the trailing icon takes On surface variant — feeding it the leading colour is the bug this \
         pins"
    );
}

/// A colour that MOVES has to reach the glyph — and it takes a declaration in BOTH groups.
///
/// The chip's outer group and the inner `Row` each compare a whitelist, and neither carries a colour
/// in its modifier chain. Measured before the fix with a probe that counted the leading-icon closure
/// across three frames while only `leading_icon` moved: it ran ONCE and kept painting the old tint
/// (`seen=["101,143"]` on every frame). Declaring the colour on the chip's group alone was still not
/// enough — the `Row` then Skipped on its own unchanged parameters — so the value is declared before
/// the `Row` starts as well.
#[test]
fn moving_only_an_icon_colour_recolours_the_glyph() {
    use std::cell::RefCell;
    let holder = RefCell::new(None::<crate::runtime::state::State<bool>>);
    let mut composer = Composer::new();
    let scene = |ctx: &mut ComposeCtx| {
        let flip = ctx.remember(|| false);
        holder.replace(Some(flip.clone()));
        let base = ChipDefaults::assist_chip_colors(&crate::theme::WiniaTheme::colors());
        // ONLY the leading icon colour moves. The label, the container and the border stay put, so a
        // stale glyph cannot be excused by "something else re-entered the group".
        let colors = if flip.get() {
            ChipColors {
                leading_icon: crate::graphics::Color::from_argb(255, 200, 10, 20),
                ..base
            }
        } else {
            base
        };
        Chip::assist(label("A"), || {})
            .leading_icon(an_icon("M0 0h1v1z"))
            .colors(colors)
            .build(ctx);
    };

    composer.compose(&scene);
    assert_eq!(
        icon_tints(&composer)[0],
        Some(crate::theme::WiniaTheme::colors().primary),
        "at rest the leading icon is Primary"
    );
    let flip = holder.borrow().clone().expect("the flip state");
    flip.set(true);
    composer.compose(&scene);
    composer.compose(&scene);
    assert_eq!(
        icon_tints(&composer)[0],
        Some(crate::graphics::Color::from_argb(255, 200, 10, 20)),
        "a moved leading-icon colour must reach the glyph"
    );
}

/// Two `.modifier(..)` calls both survive.
///
/// The setter ASSIGNED (`self.modifier = m`) where every sibling component appends
/// (`self.modifier.then(m)`), so the second call silently discarded the first. Measured before the
/// fix with `fill_max_width` then `fill_max_height`: `fill_w=false fill_h=true`.
#[test]
fn two_modifier_calls_append_to_the_chain() {
    let (composer, idx) = chip_container(
        Chip::assist(label("A"), || {})
            .modifier(Modifier::new().fill_max_width())
            .modifier(Modifier::new().fill_max_height()),
    );
    let els = composer.arena_nodes()[idx].modifier.elements();
    assert!(
        els.iter()
            .any(|el| matches!(el, crate::modifier::ModifierElement::FillMaxWidth { .. })),
        "the FIRST .modifier(..) call keeps its effect"
    );
    assert!(
        els.iter()
            .any(|el| matches!(el, crate::modifier::ModifierElement::FillMaxHeight { .. })),
        "and so does the second"
    );
}

/// An avatar WINS over a leading icon — one leading slot, not two.
///
/// material3's `leadingContent` is `avatar != null -> avatar // An avatar takes precedence`
/// (`Chip.kt:3528`). winia rendered both when a caller set them together, side by side.
#[test]
fn an_avatar_wins_over_a_leading_icon() {
    let (composer, _) = chip_container(
        Chip::input(false, label("I"), || {})
            .avatar(an_icon("M0 0h9v9z"))
            .leading_icon(an_icon("M0 0h5v5z")),
    );
    assert_eq!(
        icon_tints(&composer).len(),
        1,
        "exactly one leading slot paints — the avatar"
    );
}

// ── The 48 dp touch target (`docs/chip.md` §3) ──

/// A chip reserves Compose's 48 dp touch target and keeps its 32 dp container: both sizes are asserted,
/// because the point of the wrapper node is that the VISUAL box stays 32 while the node a caller's parent
/// measures is 48.
///
/// Compose gets this from the `Surface(onClick = …)` every variant is built on (`Chip.kt:3209`, `:3258`,
/// `:3317`), whose `Box` applies `Modifier.minimumInteractiveComponentSize()` unconditionally on the
/// `onClick` overloads (`material3/Surface.kt:225`) — so a disabled chip reserves it too. winia's chip node
/// used to BE the 32 dp box, which left a 32 dp click region where Compose has 48.
#[test]
fn a_chip_reserves_a_48_dp_touch_target() {
    let heights = |chip: Chip| -> (Vec<f32>, Option<f32>) {
        let mut composer = Composer::new();
        composer.compose(|ctx: &mut ComposeCtx| {
            // Under a Column: the composition root is special-cased and does not show the enlargement.
            crate::layout::components::Column::new()
                .modifier(Modifier::new().fill_max_width())
                .build(ctx, |ctx| chip.build(ctx));
        });
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        let nodes = composer.arena_nodes();
        let all = nodes
            .iter()
            .map(|n| n.measured_size.height)
            .collect::<Vec<_>>();
        // The box that paints the container is the one carrying the background.
        let painted = nodes.iter().find_map(|n| {
            n.modifier
                .elements()
                .iter()
                .any(|el| matches!(el, crate::modifier::ModifierElement::Background { .. }))
                .then_some(n.measured_size.height)
        });
        (all, painted)
    };

    for (name, chip) in [
        ("assist", Chip::assist(label("A"), || {})),
        ("filter (selected)", Chip::filter(true, label("F"), || {})),
        ("suggestion", Chip::suggestion(label("S"), || {})),
        ("input", Chip::input(true, label("I"), || {})),
    ] {
        let (heights, painted) = heights(chip);
        assert!(
            heights.iter().any(|h| (*h - 48.0).abs() < 0.5),
            "{name}: the chip reserves the 48 dp touch target ({heights:?})"
        );
        assert_eq!(
            painted,
            Some(ChipDefaults::HEIGHT),
            "{name}: and the box that paints the container is still {} dp",
            ChipDefaults::HEIGHT
        );
    }

    // A DISABLED chip keeps the target: `Surface` applies the modifier to the `onClick` overload whatever
    // `enabled` says, so the size does not depend on it.
    let (heights, painted) = heights(Chip::assist(label("A"), || {}).enabled(false));
    assert!(
        heights.iter().any(|h| (*h - 48.0).abs() < 0.5),
        "a disabled chip reserves it too ({heights:?})"
    );
    assert_eq!(
        painted,
        Some(ChipDefaults::HEIGHT),
        "with its container still at {} dp",
        ChipDefaults::HEIGHT
    );
}

// ── The API gaps closed against material3 (`docs/chip.md` §2, §4) ──

/// The two disabled icon slots exist and are INDEPENDENT of the disabled label.
///
/// Compose's `ChipColors` carries `disabledLeadingIconContentColor` / `disabledTrailingIconContentColor`
/// separately from `disabledLabelColor`; winia had neither, so a disabled chip could only paint both
/// icons in the label's colour and a caller could not override them at all.
#[test]
fn the_disabled_icon_slots_are_separate_from_the_label() {
    let (composer, _) = chip_container(
        Chip::assist(label("A"), || {})
            .enabled(false)
            .leading_icon(an_icon("M0 0h1v1z"))
            .trailing_icon(an_icon("M0 0h2v2z"))
            .colors(ChipColors {
                disabled_leading_icon: crate::graphics::Color::from_argb(255, 200, 10, 20),
                disabled_trailing_icon: crate::graphics::Color::from_argb(255, 20, 10, 200),
                ..ChipDefaults::assist_chip_colors(&crate::theme::WiniaTheme::colors())
            }),
    );
    let tints = icon_tints(&composer);
    assert_eq!(
        tints[0],
        Some(crate::graphics::Color::from_argb(255, 200, 10, 20)),
        "the disabled leading icon takes its own slot"
    );
    assert_eq!(
        tints[1],
        Some(crate::graphics::Color::from_argb(255, 20, 10, 200)),
        "and the trailing icon takes a different one"
    );
}

/// The disabled icon defaults match M3's `OnSurface` role — the pixels a chip already painted.
#[test]
fn the_disabled_icons_default_to_the_on_surface_role() {
    let theme = crate::theme::WiniaTheme::colors();
    let flat = ChipDefaults::assist_chip_colors(&theme);
    assert_eq!(
        flat.disabled_leading_icon, flat.disabled_label,
        "an assist chip's disabled leading icon is the label's disabled colour"
    );
    assert_eq!(
        flat.disabled_trailing_icon, flat.disabled_label,
        "and so is the trailing one"
    );
    let sel = ChipDefaults::selectable_chip_colors(&theme);
    assert!(sel.disabled_leading_icon.a > 0, "the selectable set has one too");
    assert_eq!(sel.disabled_trailing_icon, sel.disabled_leading_icon);
}

/// `copy(..)` replaces exactly the slots it is given and keeps every `None`.
#[test]
fn copy_replaces_exactly_the_slots_it_is_given() {
    let theme = crate::theme::WiniaTheme::colors();
    let base = ChipDefaults::assist_chip_colors(&theme);
    let red = crate::graphics::Color::from_argb(255, 200, 10, 20);

    let all_none = base.copy(None, None, None, None, None, None, None, None);
    assert_eq!(all_none, base, "an all-`None` copy is the identity");

    let one = base.copy(None, Some(red), None, None, None, None, None, None);
    assert_eq!(one.label, red, "the supplied slot is replaced");
    assert_eq!(one.container, base.container, "and the rest is untouched");
    assert_eq!(one.leading_icon, base.leading_icon);
    assert_eq!(one.trailing_icon, base.trailing_icon);
    assert_eq!(one.disabled_label, base.disabled_label);
}

/// The same contract on the selectable set.
#[test]
fn selectable_copy_replaces_exactly_the_slots_it_is_given() {
    let theme = crate::theme::WiniaTheme::colors();
    let base = ChipDefaults::selectable_chip_colors(&theme);
    let red = crate::graphics::Color::from_argb(255, 200, 10, 20);

    let all_none = base.copy(
        None, None, None, None, None, None, None, None, None, None, None, None, None,
    );
    assert_eq!(all_none, base, "an all-`None` copy is the identity");

    let one = base.copy(
        None,
        None,
        None,
        None,
        Some(red),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    );
    assert_eq!(one.selected_container, red);
    assert_eq!(one.container, base.container);
    assert_eq!(one.selected_label, base.selected_label);
}

/// `border(..)` overrides what the variant resolved — and `ChipBorder::none()` removes it.
///
/// material3 gives every one of the seven entry points a `border` parameter; winia computed the border
/// from the variant and gave the caller no way in.
#[test]
fn a_callers_border_overrides_the_variants_own() {
    let red = crate::graphics::Color::from_argb(255, 200, 10, 20);

    let (base, b_idx) = chip_container(Chip::assist(label("A"), || {}));
    let (w0, _) = outline_of(&base, b_idx).expect("an assist chip outlines by default");
    assert_eq!(w0, 1.0, "the variant's own border is 1 dp");

    let (over, o_idx) = chip_container(
        Chip::assist(label("A"), || {}).border(ChipBorder::new(2.0, red)),
    );
    assert_eq!(
        outline_of(&over, o_idx),
        Some((2.0, red)),
        "the caller's width and colour win"
    );

    let (none, n_idx) =
        chip_container(Chip::assist(label("A"), || {}).border(ChipBorder::none()));
    assert_eq!(
        outline_of(&none, n_idx),
        None,
        "`none()` detaches the border (Compose's `border = null`)"
    );
}

/// A caller can also ADD a border where the variant has none — an elevated chip.
#[test]
fn a_border_can_be_added_to_a_variant_that_has_none() {
    let red = crate::graphics::Color::from_argb(255, 200, 10, 20);
    let (base, b_idx) = chip_container(Chip::elevated_assist(label("A"), || {}));
    assert_eq!(
        outline_of(&base, b_idx),
        None,
        "an elevated chip carries no outline by default"
    );
    let (with, w_idx) = chip_container(
        Chip::elevated_assist(label("A"), || {}).border(ChipBorder::new(3.0, red)),
    );
    assert_eq!(outline_of(&with, w_idx), Some((3.0, red)));
}

/// `horizontal_arrangement(..)` changes the gap between the chip's children.
///
/// Measured through the laid-out width rather than the builder field, because that is what the caller
/// sees: with one icon and one label the width moves by exactly the gap difference.
#[test]
fn horizontal_arrangement_changes_the_laid_out_gap() {
    let width_of = |gap: Option<f32>| -> f32 {
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            let mut c = Chip::assist(label("A"), || {}).leading_icon(an_icon("M0 0h1v1z"));
            if let Some(g) = gap {
                c = c.horizontal_arrangement(g);
            }
            c.build(ctx);
        });
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        let root = composer.layout_root_idx().expect("root");
        composer.arena_nodes()[root].measured_size.width
    };
    let default = width_of(None);
    let zero = width_of(Some(0.0));
    let wide = width_of(Some(30.0));
    assert_eq!(
        default - zero,
        ChipDefaults::ELEMENT_GAP,
        "the default gap is ELEMENT_GAP ({})",
        ChipDefaults::ELEMENT_GAP
    );
    assert_eq!(wide - default, 30.0 - ChipDefaults::ELEMENT_GAP);
}

/// A disabled input chip's avatar drops to `DisabledAvatarOpacity` (0.38).
///
/// Compose wraps the avatar in `graphicsLayer { alpha = … }` (`Chip.kt:1205-1215`); winia painted it at
/// full opacity, so a disabled chip's avatar looked enabled.
#[test]
fn a_disabled_input_chips_avatar_is_dimmed() {
    let alpha_of = |enabled: bool| -> Vec<f32> {
        let (composer, _) = chip_container(
            Chip::input(false, label("I"), || {})
                .avatar(an_icon("M0 0h9v9z"))
                .enabled(enabled),
        );
        composer
            .arena_nodes()
            .iter()
            .flat_map(|n| n.modifier.elements().iter())
            .filter_map(|el| match el {
                crate::modifier::ModifierElement::GraphicsLayer { params } => {
                    Some(params.eval().alpha)
                }
                _ => None,
            })
            .collect()
    };
    assert_eq!(
        alpha_of(true),
        vec![1.0],
        "an enabled chip's avatar is fully opaque"
    );
    let dimmed = alpha_of(false);
    assert!(
        dimmed.contains(&ChipDefaults::DISABLED_AVATAR_OPACITY),
        "a disabled chip's avatar is drawn at {} — got {dimmed:?}",
        ChipDefaults::DISABLED_AVATAR_OPACITY
    );
}
