    use super::*;
    use crate::runtime::composer::Composer;
    use crate::theme::Typography;
    use crate::text::{FontWeight, TextStyle};
    use crate::unit::{Sp, TextUnit};
    use crate::layout::sizing::{Dimension, SizeValue};
    use crate::components::{Icon, Text};
    use crate::layout::components::Stack;

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
            nodes[root].children.first().copied().expect("the chip's box")
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
        assert_eq!(ChipDefaults::assist_chip_colors(&theme).label, theme.on_surface);
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
        assert_eq!(outline_of(&filter, f_idx), Some((1.0, theme.outline_variant)));
        let (selected, s2_idx) = chip_container(Chip::filter(true, label("F"), || {}));
        assert_eq!(outline_of(&selected, s2_idx), None, "a selected chip draws no outline");
        let (input, i_idx) = chip_container(Chip::input(false, label("I"), || {}));
        assert_eq!(outline_of(&input, i_idx), Some((1.0, theme.outline_variant)));
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
            Icon::new(crate::graphics::IconSource::svg_path("M12 3v10h8v2H10V3h2z")).build(ctx);
        }));
        assert_eq!(padding_of(&icon, i_idx), (Some(8.0), Some(8.0)));

        let (bare_input, b_idx) = chip_container(Chip::input(false, label("I"), || {}));
        assert_eq!(padding_of(&bare_input, b_idx), (Some(16.0), Some(16.0)));
        let (with_avatar, av_idx) = chip_container(
            Chip::input(false, label("I"), || {}).avatar(|ctx| {
                Stack::new().build(ctx, |_| {});
            }),
        );
        assert_eq!(
            padding_of(&with_avatar, av_idx),
            (Some(4.0), Some(16.0)),
            "an avatar sits 4 dp from the leading edge (the specs page's left padding for avatar); the trailing \
             edge has nothing in it, so it keeps the default 16 dp"
        );
        let (with_close, c_idx) = chip_container(Chip::input(false, label("I"), || {}).trailing_icon(|ctx| {
            Icon::new(crate::graphics::IconSource::svg_path("M19 6.41 12 13.4 5 6.41 3.59 8 12 16.4 20.41 8z")).build(ctx);
        }));
        assert_eq!(padding_of(&with_close, c_idx), (Some(16.0), Some(8.0)));
    }

    /// The elevation sets, in dp: a flat chip is Level0 everywhere except while dragged (`Level4` = 8); a filter
    /// chip lifts 1 dp on hover (`FlatSelectedHoverContainerElevation` = `Level1`); the elevated variants are
    /// `Level1` at rest and `Level2` (3 dp) hovered.
    #[test]
    fn the_elevation_sets_are_the_tokens_in_dp() {
        use crate::input::interaction::ComponentState;
        assert_eq!(ChipElevation::flat().for_state(&ComponentState::idle()), 0.0);
        assert_eq!(
            ChipElevation::flat().for_state(&ComponentState { dragged: true, pressed: true, ..ComponentState::idle() }),
            8.0,
            "a dragged chip rises to Level4, and the drag outranks the press it holds"
        );
        assert_eq!(
            ChipElevation::hover_lifting().for_state(&ComponentState { hovered: true, ..ComponentState::idle() }),
            1.0
        );
        assert_eq!(
            ChipElevation::elevated().for_state(&ComponentState { hovered: true, ..ComponentState::idle() }),
            3.0,
            "Level2 is 3 dp, not 2"
        );
        assert_eq!(ChipElevation::elevated().for_state(&ComponentState::disabled()), 0.0);
        assert!(!ChipElevation::flat().is_flat(), "a flat chip still lifts while dragged");
        assert!(ChipElevation::new(0.0, 0.0, 0.0, 0.0, 0.0, 0.0).is_flat());
    }

    /// The transparency rule: a chip whose container is transparent shows NO shadow, whatever its state's token
    /// value says — a shadow behind a transparent fill reads as a smudge. During the selection fade the alpha
    /// decides, so the filled-to-transparent direction drops the shadow as soon as the fill starts leaving and
    /// the transparent-to-filled direction shows none until it has arrived.
    #[test]
    fn a_transparent_container_never_shadows() {
        use crate::input::interaction::ComponentState;
        let hovered = ComponentState { hovered: true, ..ComponentState::idle() };
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
        let dragged = ComponentState { dragged: true, ..ComponentState::idle() };
        assert_eq!(ChipElevation::shadow_for(&dragged, transparent, &flat), 0.0);
        assert_eq!(
            ChipElevation::shadow_for(&dragged, opaque, &flat),
            8.0,
            "were its container ever opaque, the drag lift would be Level4"
        );

        // The elevated variants are filled at rest, so they lift by their tokens.
        let elevated = ChipElevation::elevated();
        assert_eq!(ChipElevation::shadow_for(&ComponentState::idle(), opaque, &elevated), 1.0);
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
        assert_eq!(painted_background(&elev, e_idx), theme.surface_container_low);
        assert_eq!(outline_of(&elev, e_idx), None, "an elevated chip has no outline");

        let (elev_filter, ef_idx) = chip_container(Chip::elevated_filter(false, label("F"), || {}));
        assert_eq!(painted_background(&elev_filter, ef_idx), theme.surface_container_low);
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

    /// The caller-supplied interaction source drives the chip's state, and the shadow it produces has two
    /// halves that are worth pinning: a hover lift RAMPS IN over the interaction-pair spec, and leaving hover
    /// drops it ON THE FIRST FRAME (`wanted <= shown` uses a Snap) so nothing lingers behind a container on its
    /// way out. The second half is what the falsification for this test removes.
    #[test]
    fn a_hover_lift_ramps_in_and_drops_at_once() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
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
        assert!(
            painted_shadow(&composer) <= 0.001,
            "leaving hover drops the shadow immediately (got {})",
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

    /// A disabled chip's outline is `OnSurface` at 12 % (`FlatDisabledOutlineColor` /
    /// `FlatDisabledOutlineOpacity` and their per-variant names), not the enabled role's colour; an elevated
    /// chip still carries no outline in any state.
    #[test]
    fn a_disabled_chip_outlines_with_on_surface_at_twelve_percent() {
        let theme = crate::theme::WiniaTheme::colors();
        // The expectation is built HERE from the scheme colour and the token's opacity, never from
        // `ChipDefaults::disabled_outline`: an expectation taken from the function under test would accept a
        // wrong base colour, which is exactly the mistake this test exists to catch.
        let expected = crate::graphics::Color::from_argb(31, theme.on_surface.r, theme.on_surface.g, theme.on_surface.b);
        assert_eq!(expected.a, 31, "12 % of 255 rounds to 31");
        assert_ne!(expected, theme.outline, "the disabled role is NOT the enabled outline colour");
        assert_eq!(
            ChipDefaults::disabled_outline(&theme),
            expected,
            "and the helper the component asks agrees with that"
        );

        let (assist, a) = chip_container(Chip::assist(label("A"), || {}).enabled(false));
        assert_eq!(outline_of(&assist, a), Some((1.0, expected)), "assist");
        let (suggestion, s) = chip_container(Chip::suggestion(label("S"), || {}).enabled(false));
        assert_eq!(outline_of(&suggestion, s), Some((1.0, expected)), "suggestion");
        let (filter, f) = chip_container(Chip::filter(false, label("F"), || {}).enabled(false));
        assert_eq!(outline_of(&filter, f), Some((1.0, expected)), "filter");
        let (input, i) = chip_container(Chip::input(false, label("I"), || {}).enabled(false));
        assert_eq!(outline_of(&input, i), Some((1.0, expected)), "input");

        let (enabled, e) = chip_container(Chip::assist(label("A"), || {}));
        assert_eq!(outline_of(&enabled, e), Some((1.0, theme.outline)), "enabled keeps its role colour");
        let (elevated, el) = chip_container(Chip::elevated_assist(label("E"), || {}).enabled(false));
        assert_eq!(outline_of(&elevated, el), None, "an elevated chip never outlines");
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
        assert_eq!(painted_background(&filter, f), expected, "a disabled selected filter chip");
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
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();

        let only_element = |chip: Chip| -> crate::semantics::SemanticsNode {
            let mut composer = Composer::new();
            composer.compose(|ctx| chip.build(ctx));
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
            let root = composer.layout_root_idx().expect("root");
            let tree = crate::semantics::semantics_tree(composer.arena_nodes(), root);
            assert_eq!(tree.len(), 1, "a chip is ONE accessibility element, label and icons included");
            tree.into_iter().next().expect("the chip element")
        };

        for (name, chip) in [
            ("assist", Chip::assist(label("A"), || {})),
            ("elevated assist", Chip::elevated_assist(label("A"), || {})),
            ("suggestion", Chip::suggestion(label("S"), || {})),
            ("elevated suggestion", Chip::elevated_suggestion(label("S"), || {})),
        ] {
            let element = only_element(chip);
            assert_eq!(element.role, Some(SemanticsRole::Button), "{name} is a button");
            assert_eq!(
                element.state.selected_value(),
                None,
                "{name} says nothing about selection — it has none"
            );
        }

        for (name, chip) in [
            ("filter (selected)", Chip::filter(true, label("F"), || {})),
            ("filter (unselected)", Chip::filter(false, label("F"), || {})),
            ("elevated filter", Chip::elevated_filter(true, label("F"), || {})),
            ("input", Chip::input(true, label("I"), || {})),
        ] {
            let element = only_element(chip);
            assert_eq!(element.role, Some(SemanticsRole::Checkbox), "{name} is a checkbox");
            assert!(
                element.state.selected_value().is_some(),
                "{name} must state its selection either way, so a reader can tell checked from unchecked"
            );
        }

        assert_eq!(
            only_element(Chip::filter(true, label("F"), || {})).state.selected_value(),
            Some(true),
            "a selected filter chip reads as selected"
        );
        assert_eq!(
            only_element(Chip::filter(false, label("F"), || {})).state.selected_value(),
            Some(false),
            "and an unselected one as unselected, not as silent"
        );
        assert_eq!(
            only_element(Chip::assist(label("A"), || {}).enabled(false)).state.enabled_value(),
            Some(false),
            "a disabled chip says so (every variant publishes enabled)"
        );
        assert!(SemanticsState::new().is_unspecified(), "the helper above is only a constructor");
    }

    fn find_text_style(nodes: &[crate::layout::node::LayoutNode], idx: usize) -> Option<(f32, FontWeight, f32, Option<f32>)> {
        for el in nodes[idx].modifier.elements() {
            if let crate::modifier::ModifierElement::TextContent { font_size, font_weight, letter_spacing, line_height, .. } = el {
                return Some((*font_size, *font_weight, *letter_spacing, *line_height));
            }
        }
        for &child in &nodes[idx].children {
            if let Some(style) = find_text_style(nodes, child) { return Some(style); }
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
                Chip::assist(|ctx| crate::components::Text::new("Label").build(ctx), || {}).build(ctx);
            });
        });
        let root = composer.layout_root_idx().unwrap();
        let style = find_text_style(composer.arena_nodes(), root).unwrap();
        assert_eq!(style.0, 19.0);
        assert_eq!(style.1, FontWeight::BOLD);
        assert_eq!(style.2, 1.3);
        assert_eq!(style.3, Some(27.0));
    }
