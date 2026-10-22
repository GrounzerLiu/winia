    use super::*;

    #[test]
    fn a_menu_item_carries_a_ripple_and_no_focus_ring() {
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            DropdownMenuItem::new("A").build(ctx);
        });
        composer.layout(crate::layout::constraints::Constraints::new(0.0, 400.0, 0.0, 600.0));
        let root = composer.layout_root_idx().expect("the item's node");
        let nodes = composer.arena_nodes();
        let els = nodes[root].modifier.elements();
        assert!(
            els.iter().any(|e| matches!(e, crate::modifier::ModifierElement::Clickable { .. })),
            "the item is clickable: {:?}",
            els.len()
        );
        assert!(
            els.iter().any(|e| matches!(e, crate::modifier::ModifierElement::Ripple { .. })),
            "the item carries a ripple (material3's indication)"
        );
        assert!(
            els.iter().any(|e| matches!(e, crate::modifier::ModifierElement::NoFocusRing)),
            "…and no focus ring: material3 marks menu-item focus with a state layer"
        );
    }

    #[test]
    fn dropdown_menu_position_follows_the_material3_candidates() {
        use crate::layout::LayoutDirection::{Ltr, Rtl};
        let window = (420.0, 520.0);
        let size = (120.0, 100.0);
        // 1. Below the anchor, when the whole menu fits under it.
        let below = dropdown_menu_position((16.0, 100.0, 80.0, 32.0), size, window, Ltr);
        assert_eq!(below, (16.0, 132.0), "below the anchor, start-aligned");
        // 2. Above it, when below would overflow: the anchor sits low, the menu is too tall for the room left.
        let above = dropdown_menu_position((16.0, 400.0, 80.0, 32.0), (120.0, 200.0), window, Ltr);
        assert_eq!(above, (16.0, 200.0), "above the anchor");
        // 3. Centred on the anchor's TOP edge — both of the first two fail, this one fits.
        let centred = dropdown_menu_position((16.0, 260.0, 80.0, 32.0), (120.0, 400.0), window, Ltr);
        assert_eq!(centred.1, 60.0, "centred on the anchor's top edge: 260 - 400/2");
        // 4. Pinned to the nearer window edge: nothing fits, so the anchor's half decides which edge. The
        //    menu has to be no taller than the margin band (520 - 96), or the case below takes over.
        let pinned_top = dropdown_menu_position((16.0, 100.0, 80.0, 32.0), (120.0, 400.0), window, Ltr);
        assert_eq!(pinned_top.1, MENU_VERTICAL_MARGIN, "anchor in the top half pins to the top margin");
        let pinned_bottom = dropdown_menu_position((16.0, 420.0, 80.0, 32.0), (120.0, 400.0), window, Ltr);
        assert_eq!(
            pinned_bottom.1,
            520.0 - MENU_VERTICAL_MARGIN - 400.0,
            "anchor in the bottom half pins to the bottom margin"
        );
        // 5. Taller than the margin band: centred, not pushed out of the window.
        let too_tall = dropdown_menu_position((16.0, 100.0, 80.0, 32.0), (120.0, 520.0), window, Ltr);
        assert_eq!(too_tall.1, 0.0, "a window-sized menu is centred: (520 - 520) / 2");

        // Horizontal candidates, with M3's zero margin.
        assert_eq!(
            dropdown_menu_position((16.0, 100.0, 80.0, 32.0), (120.0, 100.0), window, Ltr).0,
            16.0,
            "start-aligned when it fits"
        );
        let end = dropdown_menu_position((380.0, 100.0, 40.0, 32.0), (120.0, 100.0), window, Ltr);
        assert_eq!(end.0, 380.0 + 40.0 - 120.0, "end-aligned: right edges meet");
        // Neither alignment fits, so the anchor's half decides the edge. The menu has to be wide enough that
        // both start- and end-alignment overflow, or one of them takes the case.
        let pinned_left = dropdown_menu_position((30.0, 100.0, 40.0, 32.0), (400.0, 100.0), window, Ltr);
        assert_eq!(pinned_left.0, 0.0, "neither alignment fits: the anchor's half picks the edge");
        let pinned_right = dropdown_menu_position((300.0, 100.0, 40.0, 32.0), (400.0, 100.0), window, Ltr);
        assert_eq!(pinned_right.0, 420.0 - 400.0, "and the other half picks the other edge");

        // …and the same geometry under RTL picks the MIRRORED edge, because the two candidates are the
        // anchor's start and end, not its left and right (`internal/MenuPosition.kt`'s direction-resolved
        // horizontal positions). The anchor and the menu both sit fully inside the window, so this is about
        // which candidate wins, not about a fallback.
        let anchor = (100.0, 100.0, 60.0, 32.0);
        let fits = (80.0, 100.0);
        assert_eq!(
            dropdown_menu_position(anchor, fits, window, Ltr).0,
            100.0,
            "LTR: the popup's left edge on the anchor's left edge"
        );
        assert_eq!(
            dropdown_menu_position(anchor, fits, window, Rtl).0,
            80.0,
            "RTL: its right edge on the anchor's right edge (100 + 60 - 80)"
        );
    }

    #[test]
    fn menu_defaults_are_material3s_tokens() {
        assert_eq!(
            MenuDefaults::tonal_elevation(),
            0.0,
            "MenuDefaults.TonalElevation = ElevationTokens.Level0"
        );
        assert_eq!(
            MenuDefaults::shadow_elevation(),
            3.0,
            "MenuDefaults.ShadowElevation = MenuTokens.ContainerElevation = ElevationTokens.Level2"
        );
        assert!(
            matches!(
                MenuDefaults::shape(),
                crate::graphics::Shape::RoundedRect { corner_radius } if corner_radius == 4.0
            ),
            "MenuDefaults.shape = MenuTokens.ContainerShape = CornerExtraSmall (4dp), got {:?}",
            MenuDefaults::shape()
        );
        assert_eq!(
            MenuDefaults::dropdown_menu_item_content_padding(),
            (12.0, 0.0),
            "MenuDefaults.DropdownMenuItemContentPadding = PaddingValues(horizontal = 12.dp, vertical = 0)"
        );
        assert_eq!(
            MenuDefaults::dropdown_menu_item_content_padding(),
            (DROPDOWN_ITEM_HORIZONTAL_PADDING, 0.0),
            "the published padding and the constant the item applies must be the same value"
        );
    }

    /// material3 splits the anchor's semantics by type (`ExposedDropdownMenu.kt:1464-1476`): a PRIMARY anchor
    /// is `Role.DropdownList` and nothing else — the field it heads is already an editable node — while a
    /// SECONDARY anchor is a `Role.Button` that reports whether the list is showing. winia published only the
    /// secondary half, so a primary anchor reached a reader as an unnamed node.
    #[test]
    fn a_primary_anchor_is_a_dropdown_list_and_a_secondary_one_is_a_button() {
        use crate::semantics::{SemanticsNode, SemanticsRole};

        let element = |anchor_type: ExposedDropdownMenuAnchorType, element_anchor: bool| -> SemanticsNode {
            let expanded = crate::runtime::state::State::new(false);
            let mut composer = crate::runtime::composer::Composer::new();
            let box_ = ExposedDropdownMenuBox::new(expanded).anchor_type(anchor_type);
            composer.compose(move |ctx| {
                if element_anchor {
                    box_.build_with_anchor_modifier(
                        ctx,
                        |ctx, m: crate::modifier::Modifier| {
                            crate::components::text::Text::new("pick").modifier(m).build(ctx);
                        },
                        |_ctx| {},
                    );
                } else {
                    box_.build(
                        ctx,
                        |ctx| {
                            crate::components::text::Text::new("pick").build(ctx);
                        },
                        |_ctx| {},
                    );
                }
            });
            composer.layout(crate::layout::constraints::Constraints::new(0.0, 400.0, 0.0, 600.0));
            let root = composer.layout_root_idx().expect("the anchor's node");
            let tree = crate::semantics::semantics_tree(composer.arena_nodes(), root);
            tree.into_iter()
                .find(|n| n.role.is_some())
                .expect("the anchor declares a role")
        };

        let primary = element(ExposedDropdownMenuAnchorType::PrimaryNotEditable, false);
        assert_eq!(
            primary.role,
            Some(SemanticsRole::DropdownList),
            "a primary anchor is a dropdown list (and says nothing else)"
        );
        assert_eq!(
            primary.state.expanded_value(),
            None,
            "the expanded state belongs to the secondary branch only"
        );

        let editable = element(ExposedDropdownMenuAnchorType::PrimaryEditable, false);
        assert_eq!(
            editable.role,
            Some(SemanticsRole::DropdownList),
            "…and the editable primary anchor is one too"
        );

        let secondary = element(ExposedDropdownMenuAnchorType::SecondaryEditable, true);
        assert_eq!(
            secondary.role,
            Some(SemanticsRole::Button),
            "a secondary anchor is a button"
        );
        assert_eq!(
            secondary.state.expanded_value(),
            Some(false),
            "…that reports the list as collapsed"
        );
    }

    /// The user-visible half of the intrinsic fix: a menu item whose label sits between a leading and a
    /// trailing icon must keep that label on ONE line inside the menu's own chain (`width(IntrinsicSize.Max)`
    /// on the column, the label weighted in the row). Before the fix the label was handed its fractional
    /// intrinsic width (28.2 for "复制") and wrapped into two stacked glyphs, which is exactly what the demo
    /// screenshot showed.
    ///
    /// Teeth: removing the `ceil` from the text leaf's intrinsic answer (`layout/node.rs`, `leaf_intrinsic`)
    /// fails the last assertion with the label two lines tall.
    #[test]
    fn a_menu_items_label_stays_on_one_line() {
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            crate::layout::components::Column::new()
                .modifier(crate::modifier::Modifier::new().width(crate::layout::IntrinsicSize::Max))
                .build(ctx, |ctx| {
                    DropdownMenuItem::new("复制")
                        .leading_icon(|ctx| {
                            crate::components::text::Text::new("⧉").font_size(16.0).build(ctx);
                        })
                        .trailing_icon(|ctx| {
                            crate::components::text::Text::new("Ctrl+C").font_size(12.0).build(ctx);
                        })
                        .build(ctx);
                });
        });
        composer.layout(crate::layout::constraints::Constraints::new(0.0, 400.0, 0.0, 600.0));
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        // The item is the column's only child; its label is the weighted child of the item's row.
        let item = nodes[root].children[0];
        let label_column = nodes[item].children[1];
        let label = nodes[label_column].children[0];
        let label_size = nodes[label].measured_size;
        assert!(
            label_size.height < 2.0 * 19.0,
            "the label must be one line tall, got {label_size:?} (item {:?})",
            nodes[item].measured_size
        );
        // …and the item is wide enough for icon + padding + label + shortcut.
        assert!(
            nodes[item].measured_size.width >= 24.0 + 12.0 + label_size.width + 12.0 + 35.0 - 1.0,
            "the item spans its label and the shortcut: item {:?}, label {:?}",
            nodes[item].measured_size,
            label_size
        );
    }

    #[test]
    fn menu_defaults_are_what_the_components_resolve_to() {
        let theme = crate::theme::WiniaTheme::colors();
        assert_eq!(
            MenuDefaults::container_color(),
            theme.surface_container,
            "MenuDefaults.containerColor = MenuTokens.ContainerColor = surfaceContainer"
        );
        let colors = MenuDefaults::item_colors();
        assert_eq!(colors.text, theme.on_surface, "MenuDefaults.itemColors() text = onSurface");
        assert_eq!(
            colors.leading_icon, theme.on_surface_variant,
            "MenuDefaults.itemColors() leading icon = onSurfaceVariant"
        );
        assert_eq!(
            colors.disabled_text,
            with_alpha_factor(theme.on_surface, DISABLED_ALPHA),
            "MenuDefaults.itemColors() disabled text = onSurface at the disabled opacity"
        );
        // The item's own fallback goes through the same accessor, so an unset `colors` and
        // `MenuDefaults.itemColors()` cannot drift.
        assert_eq!(
            crate::components::dropdown_menu::MenuItemColors::defaults().text,
            colors.text,
            "MenuItemColors::defaults (what an item resolves to) must equal the published default"
        );
    }

    /// The published `arrow_drop_down` data must draw the arrow, at the icon's own coordinates, and draw
    /// the SAME arrow as the absolute form of the same triangle (what this constant used to carry).
    ///
    /// Skia's SVG parser is what carries the relative commands, so this is the measurement that makes it
    /// safe to keep the icon's own data here instead of a re-derived form; the bounds assertion is what
    /// says the triangle is the real one (it spans x 7..17, y 10..15 in the 24 dp box).
    #[test]
    fn the_published_arrow_data_draws_the_same_arrow() {
        let published = render_arrow(ExposedDropdownMenuDefaults::ARROW_DROP_DOWN_PATH);
        let absolute = render_arrow("M7 10L12 15L17 10z");
        let ink = published.iter().filter(|on| **on).count();
        // Measured: 30 of the 576 pixels in the box. The triangle's area is 25 (½ · 10 · 5) and its
        // bounding box is 50, so the coverage has to land in between, and near the area.
        assert!(
            (20..=45).contains(&ink),
            "the arrow covers {ink} of the 576 pixels in its 24 dp box, not the 25 dp² triangle"
        );
        let diff = published.iter().zip(absolute.iter()).filter(|(a, b)| a != b).count();
        assert!(diff <= 2, "the published data draws the same arrow ({diff} pixels differ)");
        assert_eq!(
            ink_bounds(&published),
            (7, 10, 17, 15),
            "the triangle sits where the official asset puts it"
        );
    }

    /// The 24x24 ink mask of an arrow drawn through the real `Icon` pipeline (node -> render -> pixels).
    fn render_arrow(data: &str) -> Vec<bool> {
        let mut composer = crate::runtime::composer::Composer::new();
        composer.compose(|ctx| {
            crate::components::icon::Icon::svg_path(data)
                .tint(crate::graphics::Color::BLACK)
                .size(24.0)
                .build(ctx);
        });
        composer.layout(crate::layout::Constraints::new(0.0, 24.0, 0.0, 24.0));
        let mut surface = skia_safe::surfaces::raster_n32_premul((24, 24)).expect("surface");
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::WHITE);
        let root = composer.layout_root_idx().expect("root");
        crate::render::render(composer.arena_nodes(), root, canvas);
        let pixels = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pixels.pixels::<[u8; 4]>().expect("pixels");
        px.iter().map(|p| p[0] < 128).collect()
    }

    /// The half-open pixel box the mask covers, `(left, top, right, bottom)`, or `(0, 0, 0, 0)` when
    /// nothing was drawn.
    fn ink_bounds(mask: &[bool]) -> (usize, usize, usize, usize) {
        let (mut left, mut top, mut right, mut bottom) = (usize::MAX, usize::MAX, 0usize, 0usize);
        for (i, on) in mask.iter().enumerate() {
            if !on {
                continue;
            }
            let (x, y) = (i % 24, i / 24);
            left = left.min(x);
            top = top.min(y);
            right = right.max(x + 1);
            bottom = bottom.max(y + 1);
        }
        if left == usize::MAX { (0, 0, 0, 0) } else { (left, top, right, bottom) }
    }
