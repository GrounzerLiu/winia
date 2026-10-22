    use super::*;

    #[test]
    fn the_gap_is_material3s_two_dp() {
        assert_eq!(SplitButtonDefaults::SPACING, 2.0);
    }

    #[test]
    fn inner_corners_come_from_the_token_files() {
        // SplitButton{XSmall,Small,Medium,Large,XLarge}Tokens.InnerCornerCornerSize
        assert_eq!(SplitButtonDefaults::inner_corner_size(ButtonSize::XSmall), 4.0);
        assert_eq!(SplitButtonDefaults::inner_corner_size(ButtonSize::Small), 4.0);
        assert_eq!(SplitButtonDefaults::inner_corner_size(ButtonSize::Medium), 4.0);
        assert_eq!(SplitButtonDefaults::inner_corner_size(ButtonSize::Large), 8.0);
        assert_eq!(SplitButtonDefaults::inner_corner_size(ButtonSize::XLarge), 12.0);
        // …Pressed: 8/12/12/20/20
        assert_eq!(SplitButtonDefaults::inner_corner_size_pressed(ButtonSize::XSmall), 8.0);
        assert_eq!(SplitButtonDefaults::inner_corner_size_pressed(ButtonSize::Small), 12.0);
        assert_eq!(SplitButtonDefaults::inner_corner_size_pressed(ButtonSize::Medium), 12.0);
        assert_eq!(SplitButtonDefaults::inner_corner_size_pressed(ButtonSize::Large), 20.0);
        assert_eq!(SplitButtonDefaults::inner_corner_size_pressed(ButtonSize::XLarge), 20.0);
    }

    #[test]
    fn paddings_and_icon_sizes_come_from_the_token_files() {
        assert_eq!(SplitButtonDefaults::leading_content_padding(ButtonSize::XSmall), (12.0, 10.0));
        assert_eq!(SplitButtonDefaults::leading_content_padding(ButtonSize::Small), (16.0, 12.0));
        assert_eq!(SplitButtonDefaults::leading_content_padding(ButtonSize::Medium), (24.0, 24.0));
        assert_eq!(SplitButtonDefaults::leading_content_padding(ButtonSize::Large), (48.0, 48.0));
        assert_eq!(SplitButtonDefaults::leading_content_padding(ButtonSize::XLarge), (64.0, 64.0));
        assert_eq!(SplitButtonDefaults::trailing_content_padding(ButtonSize::Small), (13.0, 13.0));
        assert_eq!(SplitButtonDefaults::trailing_content_padding(ButtonSize::Medium), (15.0, 15.0));
        assert_eq!(SplitButtonDefaults::trailing_content_padding(ButtonSize::Large), (29.0, 29.0));
        assert_eq!(SplitButtonDefaults::trailing_content_padding(ButtonSize::XLarge), (43.0, 43.0));
        assert_eq!(SplitButtonDefaults::trailing_icon_size(ButtonSize::XSmall), 22.0);
        assert_eq!(SplitButtonDefaults::trailing_icon_size(ButtonSize::Small), 22.0);
        assert_eq!(SplitButtonDefaults::trailing_icon_size(ButtonSize::Medium), 26.0);
        assert_eq!(SplitButtonDefaults::trailing_icon_size(ButtonSize::Large), 38.0);
        assert_eq!(SplitButtonDefaults::trailing_icon_size(ButtonSize::XLarge), 50.0);
        assert_eq!(
            SplitButtonDefaults::leading_icon_size(ButtonSize::Small),
            20.0,
            "the leading glyph is `ButtonSmallTokens.IconSize`"
        );
    }

    #[test]
    fn the_outer_corners_are_half_the_short_side() {
        // CornerFull is a percent-50 corner; for a button the short side is the height.
        assert_eq!(SplitButtonDefaults::outer_corner_size(32.0), 16.0);
        assert_eq!(SplitButtonDefaults::outer_corner_size(136.0), 68.0);
    }

    #[test]
    fn the_optical_shift_matches_the_specs_menu_icon_offsets() {
        // The spec lists the trailing button's unselected menu-icon offset as -1/-1/-2/-3/-6 dp for
        // XS/S/M/L/XL. The shift below is Compose's own formula, and it lands on those numbers (the
        // spec's are rounded): 0.11 * (inner - outer).
        for size in [
            ButtonSize::XSmall,
            ButtonSize::Small,
            ButtonSize::Medium,
            ButtonSize::Large,
            ButtonSize::XLarge,
        ] {
            let outer = SplitButtonDefaults::outer_corner_size(
                SplitButtonDefaults::container_height(size),
            );
            let inner = SplitButtonDefaults::inner_corner_size(size);
            let (start, _) = SplitButtonDefaults::trailing_content_padding(size);
            let shift = SplitButtonDefaults::optical_shift(outer, inner, start);
            let spec = match size {
                ButtonSize::XSmall => 1.0,
                ButtonSize::Small => 1.0,
                ButtonSize::Medium => 2.0,
                ButtonSize::Large => 3.0,
                ButtonSize::XLarge => 6.0,
            };
            assert!(
                (shift - spec).abs() <= 1.5,
                "{size:?}: the optical shift {shift} should be near the spec's {spec} dp"
            );
        }
    }

    #[test]
    fn the_optical_shift_cannot_push_the_content_out_of_its_padding() {
        // A tiny padding must clamp the shift: material3 coerces the correction into the padding,
        // precisely so a narrow button's content is never clipped.
        assert_eq!(SplitButtonDefaults::optical_shift(20.0, 4.0, 1.0), 1.0);
        assert!(SplitButtonDefaults::optical_shift(4.0, 20.0, 10.0) == 0.0);
    }

    #[test]
    fn the_shapes_put_the_full_corners_outside_and_the_inner_corner_in() {
        let leading = SplitButtonDefaults::leading_shapes(ButtonSize::Small);
        assert_eq!(leading.shape, Shape::corners(20.0, 4.0, 4.0, 20.0));
        assert_eq!(leading.pressed_shape, Shape::corners(20.0, 12.0, 12.0, 20.0));
        assert_eq!(leading.checked_shape, None);

        let trailing = SplitButtonDefaults::trailing_shapes(ButtonSize::Small);
        assert_eq!(trailing.shape, Shape::corners(4.0, 20.0, 20.0, 4.0));
        assert_eq!(trailing.checked_shape, Some(Shape::Pill));
    }

    // ── Layout (the measure policy material3 writes) ──

    use crate::runtime::composer::Composer;
    use crate::components::text::Text;
    use crate::theme::ThemeColors;

    /// A split button of the default size (40 dp) with a leading label and a trailing "v" glyph, laid
    /// out inside a theme of the given direction. `trailing_height` grows the trailing button's
    /// content, which is how the shared-height rule gets something to equalise (a fixed height on the
    /// content leaf, not a font size: the button's own text style wins over the content's font size).
    fn compose_split(
        label: &str,
        trailing_height: Option<f32>,
        dir: LayoutDirection,
    ) -> Composer {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let label = label.to_string();
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_theme_and_direction(theme.clone(), dir, ctx, |ctx| {
                SplitButtonLayout::new().build(
                    ctx,
                    |ctx| {
                        SplitButtonDefaults::leading_button(|| {}).build(ctx, |ctx| {
                            Text::new(label.clone()).build(ctx);
                        });
                    },
                    |ctx| {
                        SplitButtonDefaults::trailing_button()
                            .on_click(|| {})
                            .build(ctx, |ctx| {
                                let text = Text::new("v");
                                let text = match trailing_height {
                                    Some(height) => {
                                        text.modifier(Modifier::new().height(height))
                                    }
                                    None => text,
                                };
                                text.build(ctx);
                            });
                    },
                );
            });
        });
        composer
    }

    fn measure_split(
        composer: &mut Composer,
        width: f32,
        height: f32,
    ) -> (usize, Vec<usize>, Size, f32, f32) {
        composer.layout(Constraints::new(0.0, width, 0.0, height));
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        let children = nodes[root].children.clone();
        let root_size = nodes[root].measured_size;
        let leading = nodes[children[0]].measured_size.width;
        let trailing = nodes[children[1]].measured_size.width;
        (root, children, root_size, leading, trailing)
    }

    #[test]
    fn the_pair_is_a_leading_button_a_two_dp_gap_and_a_trailing_button() {
        let mut composer = compose_split("Add", None, LayoutDirection::Ltr);
        let (_, children, root_size, leading, trailing) = measure_split(&mut composer, 400.0, 200.0);
        let nodes = composer.arena_nodes();
        assert!(
            (leading + trailing + SplitButtonDefaults::SPACING - root_size.width).abs() < 0.01,
            "the layout is leading + gap + trailing"
        );
        assert!(
            (nodes[children[1]].position.x - (leading + SplitButtonDefaults::SPACING)).abs() < 0.01,
            "the gap sits between the two buttons"
        );
        // A pair that fits its parent wraps its content instead of filling it.
        assert!(root_size.width < 400.0, "the layout hugs its content");
        assert_eq!(nodes[children[0]].measured_size.height, 40.0);
        assert_eq!(nodes[children[1]].measured_size.height, 40.0);
    }

    #[test]
    fn the_trailing_button_is_measured_first_and_the_leading_one_gets_what_is_left() {
        // The label wants far more than the parent can give: material3 measures the trailing button
        // against the full width first, then hands the leading button the remainder.
        let mut composer = compose_split(
            "A split button label far too long for the space",
            None,
            LayoutDirection::Ltr,
        );
        let (_, _, root_size, leading, trailing) = measure_split(&mut composer, 200.0, 200.0);
        assert!(
            trailing >= SplitButtonDefaults::MIN_BUTTON_WIDTH,
            "the trailing button keeps its own width ({trailing})"
        );
        assert!(
            (leading - (200.0 - trailing - SplitButtonDefaults::SPACING)).abs() < 0.01,
            "the leading button takes the remainder ({leading} vs {})",
            200.0 - trailing - SplitButtonDefaults::SPACING
        );
        assert_eq!(root_size.width, 200.0, "the pair fills the parent it cannot fit in");
    }

    #[test]
    fn both_buttons_are_forced_to_one_height() {
        // The trailing button's content asks for a taller box; material3 gives the pair the taller of
        // the two intrinsic heights and measures BOTH at it, so they read as one control.
        let mut composer = compose_split("Add", Some(72.0), LayoutDirection::Ltr);
        let (_, children, _, _, _) = measure_split(&mut composer, 400.0, 400.0);
        let nodes = composer.arena_nodes();
        let height = nodes[children[1]].measured_size.height;
        assert!(
            (height - 72.0).abs() < 0.01,
            "the taller content decides the shared height (got {height})"
        );
        assert_eq!(
            nodes[children[0]].measured_size.height, height,
            "the leading button is stretched to the trailing button's height"
        );
    }

    #[test]
    fn rtl_puts_the_leading_button_on_the_right() {
        let mut composer = compose_split("Add", None, LayoutDirection::Rtl);
        let (_, children, root_size, leading, _) = measure_split(&mut composer, 400.0, 200.0);
        let nodes = composer.arena_nodes();
        let first = &nodes[children[0]];
        let last = &nodes[children[1]];
        assert!(
            (first.position.x + first.measured_size.width - root_size.width).abs() < 0.01,
            "the leading button ends at the right edge in RTL (x={}, width={}, root={})",
            first.position.x,
            leading,
            root_size.width
        );
        assert!(last.position.x.abs() < 0.01, "the trailing button starts at the left edge in RTL");
    }

    // ── The morph ──

    /// The shapes a node actually paints, read off its modifier chain the way the renderer (and the
    /// ripple's clip inference) reads them.
    fn drawn_backgrounds(composer: &Composer, node: usize) -> Vec<Shape> {
        composer.arena_nodes()[node]
            .modifier
            .elements()
            .iter()
            .filter_map(|el| match el {
                crate::modifier::ModifierElement::Background { shape, .. } => Some(shape.eval()),
                _ => None,
            })
            .collect()
    }

    /// A split button whose buttons take a hoisted interaction source, so a test can put a state in
    /// place BEFORE the first composition — the state read is a composition dependency, so the first
    /// pass already sees it (this is what material3's `interactionSource` parameter is for).
    fn compose_split_with_source(
        interaction: &MutableInteractionSource,
        checked: Option<State<bool>>,
        dir: LayoutDirection,
    ) -> Composer {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let source = interaction.clone();
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_theme_and_direction(theme.clone(), dir, ctx, |ctx| {
                SplitButtonLayout::new().build(
                    ctx,
                    |ctx| {
                        SplitButtonDefaults::leading_button(|| {})
                            .interaction_source(source.clone())
                            .build(ctx, |ctx| {
                                Text::new("Add").build(ctx);
                            });
                    },
                    |ctx| {
                        let trailing = match checked {
                            Some(state) => TrailingButton::checked(state),
                            None => SplitButtonDefaults::trailing_button().on_click(|| {}),
                        };
                        trailing.build(ctx, |ctx| {
                            Text::new("v").build(ctx);
                        });
                    },
                );
            });
        });
        composer
    }

    fn split_children(composer: &mut Composer) -> Vec<usize> {
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 200.0));
        let root = composer.layout_root_idx().expect("root");
        composer.arena_nodes()[root].children.clone()
    }

    /// A half's VISUAL node: since `Button` reserves its 48 dp touch target in a wrapper node of its own
    /// (`ModifierElement::MinimumInteractiveSize`), the row's children are targets whose visual box is their only
    /// child. Tests that ask what a half PAINTS or how its content sits walk through this; tests about the pair's
    /// geometry keep using the row's children, whose x is unchanged (the wrapper only shifts itself up by 4 dp to
    /// centre a 40 dp visual inside the taller target).
    fn half_visual(nodes: &[LayoutNode], node: usize) -> usize {
        let is_target = nodes[node]
            .modifier
            .elements()
            .iter()
            .any(|el| matches!(el, crate::modifier::ModifierElement::MinimumInteractiveSize { .. }));
        if is_target {
            nodes[node].children.first().copied().unwrap_or(node)
        } else {
            node
        }
    }

    /// A node's absolute position: the arena stores each position relative to its parent, so a test that wants
    /// to sample the pixels of a nested node has to walk down from the root adding them up.
    fn absolute_position(nodes: &[LayoutNode], root: usize, target: usize) -> Option<(f32, f32)> {
        if root == target {
            return Some((0.0, 0.0));
        }
        for &child in &nodes[root].children {
            if let Some((x, y)) = absolute_position(nodes, child, target) {
                return Some((nodes[child].position.x + x, nodes[child].position.y + y));
            }
        }
        None
    }

    /// The two halves' visual nodes, measured and ready.
    fn split_visuals(composer: &mut Composer) -> Vec<usize> {
        let children = split_children(composer);
        let nodes = composer.arena_nodes();
        children.iter().map(|&c| half_visual(nodes, c)).collect()
    }

    #[test]
    fn shape_for_state_is_material3s_shape_by_interaction() {
        let leading = SplitButtonDefaults::leading_shapes(ButtonSize::Small);
        let trailing = SplitButtonDefaults::trailing_shapes(ButtonSize::Small);
        assert_eq!(SplitButtonDefaults::shape_for_state(&leading, false, false), leading.shape);
        assert_eq!(
            SplitButtonDefaults::shape_for_state(&leading, true, false),
            leading.pressed_shape,
            "pressed wins over the resting shape"
        );
        assert_eq!(
            SplitButtonDefaults::shape_for_state(&leading, false, true),
            leading.shape,
            "the leading button has no checked shape, so it keeps its own"
        );
        assert_eq!(
            SplitButtonDefaults::shape_for_state(&trailing, false, true),
            Shape::Pill,
            "the checked trailing button morphs to the stadium"
        );
        assert_eq!(
            SplitButtonDefaults::shape_for_state(&trailing, true, true),
            trailing.pressed_shape,
            "pressed wins over checked, as `shapeByInteraction` orders them"
        );
    }

    #[test]
    fn a_resting_button_draws_its_resting_shape_and_no_state_layer() {
        let interaction = MutableInteractionSource::new();
        let mut composer = compose_split_with_source(&interaction, None, LayoutDirection::Ltr);
        let children = split_visuals(&mut composer);
        let leading = SplitButtonDefaults::leading_shapes(ButtonSize::Small);
        let trailing = SplitButtonDefaults::trailing_shapes(ButtonSize::Small);
        assert_eq!(
            drawn_backgrounds(&composer, children[0]),
            vec![leading.shape],
            "the leading button paints its container once, in the resting shape"
        );
        assert_eq!(drawn_backgrounds(&composer, children[1]), vec![trailing.shape]);
    }

    #[test]
    fn a_pressed_button_draws_the_pressed_shape() {
        let interaction = MutableInteractionSource::new();
        // Pressed BEFORE the first composition: the state read is a dependency, so the first pass
        // already picks the pressed shape, and the animation starts at its target.
        interaction.emit_press();
        let mut composer = compose_split_with_source(&interaction, None, LayoutDirection::Ltr);
        let children = split_visuals(&mut composer);
        let expected = SplitButtonDefaults::leading_shapes(ButtonSize::Small).pressed_shape;
        assert_eq!(
            drawn_backgrounds(&composer, children[0]),
            vec![expected],
            "a pressed leading button paints the pressed shape (inner corner 12 instead of 4)"
        );
        assert_eq!(
            drawn_backgrounds(&composer, children[1]),
            vec![SplitButtonDefaults::trailing_shapes(ButtonSize::Small).shape],
            "the trailing button is untouched by its neighbour's press"
        );
    }

    /// material3 paints the checked state layer with `drawWithContent { drawContent(); drawOutline(...) }`
    /// — over the content, not behind it under the container. The order is checked structurally rather
    /// than by pixels, because the layer's colour IS the content colour and the content paints in that
    /// same colour: blending a colour over itself leaves every glyph pixel unchanged, so no pixel reading
    /// can separate the two orders. What can be measured is where the node asks for the layer.
    #[test]
    fn the_checked_state_layer_paints_after_the_content() {
        let read = |checked: bool| {
            let interaction = MutableInteractionSource::new();
            let state = if checked { Some(State::new(true)) } else { None };
            let mut composer = compose_split_with_source(&interaction, state, LayoutDirection::Ltr);
            let children = split_visuals(&mut composer);
            let backgrounds = drawn_backgrounds(&composer, children[1]);
            let layers = composer.arena_nodes()[children[1]].modifier.draw_wrap_nodes().count();
            (backgrounds, layers)
        };
        let (resting_backgrounds, resting_layers) = read(false);
        let (checked_backgrounds, checked_layers) = read(true);
        eprintln!(
            "state layer: resting {resting_backgrounds:?}/{resting_layers} checked {checked_backgrounds:?}/{checked_layers}"
        );
        assert_eq!(
            checked_layers,
            resting_layers + 1,
            "the checked half asks for one more after-content layer"
        );
        assert_eq!(
            checked_backgrounds.len(),
            resting_backgrounds.len(),
            "and not for one more background under the content ({checked_backgrounds:?} vs {resting_backgrounds:?})"
        );
        assert_eq!(
            checked_backgrounds.first(),
            Some(&Shape::Pill),
            "the checked container is still the token's stadium"
        );
    }

    // ── What the pair actually paints ──

    /// The inset of the top row of a rounded rectangle IS that corner's radius (for `RoundedCornerShape`
    /// the row spans `left + topStart .. right - topEnd`), so measuring the painted pixels gives the four
    /// radii of each button, not just the shape the modifier chain asked for.
    ///
    /// Measured, because "the two parts' corners look different" is exactly the kind of claim a modifier
    /// chain cannot settle: the pair must mirror — the leading button rounds its LEFT corners fully and
    /// its right ones by the inner-corner token, the trailing button the other way round.
    #[test]
    fn the_pair_paints_the_token_radii_on_every_corner() {
        let interaction = MutableInteractionSource::new();
        let mut composer = compose_split_with_source(&interaction, None, LayoutDirection::Ltr);
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 200.0));
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        let rects: Vec<(f32, f32, f32, f32)> = nodes[root]
            .children
            .iter()
            .map(|i| {
                let visual = half_visual(nodes, *i);
                let (x, y) = absolute_position(nodes, root, visual).expect("the half's visual box");
                let node = &nodes[visual];
                (x, y, node.measured_size.width, node.measured_size.height)
            })
            .collect();

        let (w, h) = (400, 200);
        let mut surface = skia_safe::surfaces::raster_n32_premul((w, h)).expect("surface");
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::WHITE);
        crate::render::render(composer.arena_nodes(), root, canvas);
        let pixels = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pixels.pixels::<[u8; 4]>().expect("pixels");
        let painted = |x: usize, y: usize| px[y * w as usize + x][0] < 200;

        let mut measured = Vec::new();
        for (bx, by, bw, bh) in &rects {
            let x0 = bx.round() as usize;
            let y0 = by.round() as usize;
            let x1 = (bx + bw).round() as usize - 1;
            let y1 = (by + bh).round() as usize - 1;
            // The corner arc spans exactly `radius` rows/columns: the first row whose leftmost painted
            // pixel reaches the button's left edge sits at `top + top_left`. Measuring the EXTENT of the
            // arc, not the inset of one antialiased row, is what makes this readable off a raster.
            let first = |y: usize| (x0..=x1).find(|x| painted(*x, y));
            let last = |y: usize| (x0..=x1).rev().find(|x| painted(*x, y));
            let left_edge = (y0..=y1).filter_map(first).min().expect("a painted button");
            let right_edge = (y0..=y1).filter_map(last).max().expect("a painted button");
            let top_edge = (y0..=y1).find(|y| painted(x0 + (x1 - x0) / 2, *y)).expect("painted");
            let bottom_edge =
                (y0..=y1).rev().find(|y| painted(x0 + (x1 - x0) / 2, *y)).expect("painted");
            let row_left = |y: usize| first(y).expect("painted row");
            let row_right = |y: usize| last(y).expect("painted row");
            let col_top = |x: usize| (y0..=y1).find(|y| painted(x, *y)).expect("painted column");
            let col_bottom = |x: usize| (y0..=y1).rev().find(|y| painted(x, *y)).expect("painted");
            let tl = (top_edge..=bottom_edge).find(|y| row_left(*y) <= left_edge + 1).unwrap()
                - top_edge;
            let tr = (top_edge..=bottom_edge)
                .find(|y| row_right(*y) >= right_edge.saturating_sub(1))
                .unwrap()
                - top_edge;
            let bl = bottom_edge
                - (top_edge..=bottom_edge)
                    .rev()
                    .find(|y| row_left(*y) <= left_edge + 1)
                    .unwrap();
            let br = bottom_edge
                - (top_edge..=bottom_edge)
                    .rev()
                    .find(|y| row_right(*y) >= right_edge.saturating_sub(1))
                    .unwrap();
            let _ = (col_top, col_bottom);
            measured.push((tl, tr, bl, br));
        }
        eprintln!("split button corners (top-left, top-right, bottom-left, bottom-right): {measured:?}");

        let inner = SplitButtonDefaults::inner_corner_size(ButtonSize::Small);
        let outer = rects[0].3 / 2.0;
        // Measured through a coverage threshold, so the arc reads a few pixels short of its real radius
        // (a true 20 dp corner measures ~12 here and a true 4 dp one ~1). What must hold exactly is the MIRROR — the same two radii,
        // swapped — and that the outer corner is the round one.
        for (i, corners) in measured.iter().enumerate() {
            let (far, near) = if i == 0 { (corners.0, corners.1) } else { (corners.3, corners.2) };
            assert!(
                far > near + 4,
                "button {i} must round its outer corner more than its inner one, measured {corners:?}"
            );
            let (other_far, other_near) =
                if i == 0 { (measured[1].3, measured[1].2) } else { (measured[0].0, measured[0].1) };
            assert!(
                far.abs_diff(other_far) <= 2 && near.abs_diff(other_near) <= 2,
                "button {i} must mirror the other half: {corners:?} against the other's two radii"
            );
            assert!(
                (outer * 0.5..=outer + 1.0).contains(&(far as f32)),
                "button {i}'s outer corner is CornerFull ({outer}), measured {far}"
            );
            assert!(
                (near as f32) <= inner + 1.5,
                "button {i}'s inner corner is the tier's {inner}, measured {near}"
            );
        }
    }

    /// The checked stadium is APPROACHED, not jumped to: while the morph is still running the half draws
    /// the animated corners, and only a shape whose four radii have reached their targets is the resolved
    /// shape material3's `shapeByInteraction` names (the stadium, for the checked trailing half).
    ///
    /// This is the regression guard for the transition itself — reading the shape straight off the state
    /// (as it used to) makes the `mid` case below return the stadium immediately, i.e. a snap.
    #[test]
    fn the_checked_stadium_is_reached_through_the_morph() {
        let outer = SplitButtonDefaults::outer_corner_size(SplitButtonDefaults::container_height(
            ButtonSize::Small,
        ));
        let inner = SplitButtonDefaults::inner_corner_size(ButtonSize::Small);
        let stadium = (outer, outer, outer, outer);
        assert_eq!(
            drawn_shape(&Shape::Pill, stadium, stadium),
            Shape::Pill,
            "a settled trailing half draws material3's own checked shape"
        );
        assert_eq!(
            drawn_shape(&Shape::Pill, stadium, (inner, outer, outer, inner)),
            Shape::corners(inner, outer, outer, inner),
            "mid-morph the half draws the animated corners, so the stadium is approached"
        );
        let nearly = drawn_shape(&Shape::Pill, stadium, (outer - 1.0, outer, outer, outer));
        assert_ne!(
            nearly,
            Shape::Pill,
            "a corner one dp short of `outer` is not the stadium yet: {nearly:?}"
        );
    }

    /// The checked trailing half is a stadium in the PAINTED pixels, not only in the modifier chain:
    /// material3's `TrailingCheckedShape = CircleShape` (`SplitButton.kt:427`), which for a
    /// wider-than-tall button is every corner at `outer`.
    #[test]
    fn a_checked_trailing_paints_the_stadium() {
        let interaction = MutableInteractionSource::new();
        let expanded = State::new(true);
        let mut composer =
            compose_split_with_source(&interaction, Some(expanded), LayoutDirection::Ltr);
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 200.0));
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        let trailing = half_visual(nodes, nodes[root].children[1]);
        let (ax, ay) = absolute_position(nodes, root, trailing).expect("the half's visual box");
        let (x, y, w) = (
            ax.round() as usize,
            ay.round() as usize,
            nodes[trailing].measured_size.width.round() as usize,
        );
        let mut surface = skia_safe::surfaces::raster_n32_premul((400, 200)).expect("surface");
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::WHITE);
        crate::render::render(composer.arena_nodes(), root, canvas);
        let pixels = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pixels.pixels::<[u8; 4]>().expect("pixels");
        let painted = |x: usize, y: usize| px[y * 400 + x][0] < 200;
        let row = y + 1;
        let left = (x..x + w).find(|x| painted(*x, row)).expect("painted") - x;
        let right = (x + w - 1) - (x..x + w).rev().find(|x| painted(*x, row)).expect("painted");
        eprintln!("checked trailing painted corners (left, right): ({left}, {right})");
        assert!(
            left.abs_diff(right) <= 2,
            "a checked trailing half is a stadium, so both of its top corners round the same: \
             measured ({left}, {right})"
        );
        assert!(
            left > 8,
            "and its gap-side corner has grown to the outer radius, measured {left}"
        );
    }

    /// A press rounds the inner corner — and does NOT round it all the way. material3's pressed inner
    /// corner is `SmallInnerCornerSizePressed = 12.dp` against a 40 dp container (full would be 20), so
    /// the pressed half is asymmetric on purpose: the outer corner stays `CornerFull`.
    ///
    /// The inset of the top row of the painted rect is that corner's radius (measured with the raster's
    /// bias, which under-reads both the same way, so the comparison between the two states holds).
    #[test]
    fn a_press_rounds_the_inner_corner_without_making_it_full() {
        let measure = |pressed: bool| {
            let interaction = MutableInteractionSource::new();
            if pressed {
                interaction.emit_press();
            }
            let mut composer = compose_split_with_source(&interaction, None, LayoutDirection::Ltr);
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 200.0));
            let root = composer.layout_root_idx().expect("root");
            let nodes = composer.arena_nodes();
            let leading = nodes[root].children[0];
            let (x, y, w) = (
                nodes[leading].position.x.round() as usize,
                nodes[leading].position.y.round() as usize,
                nodes[leading].measured_size.width.round() as usize,
            );
            let mut surface =
                skia_safe::surfaces::raster_n32_premul((400, 200)).expect("surface");
            let canvas = surface.canvas();
            canvas.clear(skia_safe::Color::WHITE);
            crate::render::render(composer.arena_nodes(), root, canvas);
            let pixels = surface.peek_pixels().expect("pixmap");
            let px: &[[u8; 4]] = pixels.pixels::<[u8; 4]>().expect("pixels");
            let painted = |x: usize, y: usize| px[y * 400 + x][0] < 200;
            let row = y + 1;
            let first = (x..x + w).find(|x| painted(*x, row)).expect("painted") - x;
            let last = (x..x + w).rev().find(|x| painted(*x, row)).expect("painted");
            (first, (x + w - 1) - last)
        };
        let (resting_outer, resting_inner) = measure(false);
        let (pressed_outer, pressed_inner) = measure(true);
        eprintln!(
            "split button leading, painted corners (outer, inner): resting ({resting_outer}, \
             {resting_inner}) pressed ({pressed_outer}, {pressed_inner})"
        );
        assert!(
            pressed_inner > resting_inner + 4,
            "a press rounds the inner corner (resting {resting_inner} -> pressed {pressed_inner})"
        );
        assert!(
            pressed_inner < pressed_outer - 4,
            "and it does not round it all the way: material3's pressed inner corner is 12 dp against \
             the 20 dp CornerFull outer (pressed ({pressed_outer}, {pressed_inner}))"
        );
        assert!(
            pressed_outer >= resting_outer - 2,
            "the outer corner stays CornerFull under the press ({resting_outer} -> {pressed_outer})"
        );
    }

    /// A press must change the SHAPE and not move the content: the optical offset reads the settled
    /// radius, not the animated one. The press has to show up somewhere or the assertion proves nothing,
    /// so the same state is also asked for the shape it paints.
    #[test]
    fn a_press_does_not_move_the_content() {
        let measure = |pressed: bool| {
            let interaction = MutableInteractionSource::new();
            if pressed {
                interaction.emit_press();
            }
            let mut composer = compose_split_with_source(&interaction, None, LayoutDirection::Ltr);
            let buttons = split_visuals(&mut composer);
            let nodes = composer.arena_nodes();
            let offsets: Vec<f32> = buttons
                .iter()
                .map(|b| {
                    // Two levels down: the Button holds its content in a centring box, and the offset
                    // lives on the Row inside it. Reading the box itself measures the centring, which
                    // does not move with the offset — that is how this test kept passing while the
                    // content did move (measured: swapping the offset back to the pressed-aware radius
                    // left every library test green).
                    let panel = nodes[*b].children.first().expect("the button's content box");
                    let wrapper = nodes[*panel].children.first().expect("the offset wrapper");
                    nodes[*wrapper].position.x
                })
                .collect();
            let shapes: Vec<Option<Shape>> = buttons
                .iter()
                .map(|b| drawn_backgrounds(&composer, *b).into_iter().next())
                .collect();
            (offsets, shapes)
        };
        let (resting_offsets, resting_shapes) = measure(false);
        let (pressed_offsets, pressed_shapes) = measure(true);
        eprintln!("split button content offsets: resting {resting_offsets:?} pressed {pressed_offsets:?}");
        assert_eq!(
            resting_offsets, pressed_offsets,
            "a press must not move the content (the optical offset reads the settled radius)"
        );
        assert_ne!(
            resting_shapes, pressed_shapes,
            "the press must still morph the shape, or this test proves nothing"
        );
    }

    /// The trailing half's content is optically offset while it is unselected and centred once the menu
    /// is open: material3's per-size "menu icon offset when unselected" and "the icon becomes centered
    /// when selected". This is the layout half of the checked transition — the painted shape is guarded
    /// by `a_checked_trailing_paints_the_stadium`.
    #[test]
    fn a_checked_trailing_centres_its_content() {
        let content_offset = |checked: bool| {
            let interaction = MutableInteractionSource::new();
            let state = if checked { Some(State::new(true)) } else { None };
            let mut composer = compose_split_with_source(&interaction, state, LayoutDirection::Ltr);
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 200.0));
            let root = composer.layout_root_idx().expect("root");
            let nodes = composer.arena_nodes();
            let trailing = half_visual(nodes, nodes[root].children[1]);
            let panel = nodes[trailing].children.first().copied().expect("the content box");
            let wrapper = nodes[panel].children.first().copied().expect("the offset wrapper");
            nodes[wrapper].position.x
        };
        let unselected = content_offset(false);
        let selected = content_offset(true);
        eprintln!("trailing content offset: unselected {unselected} selected {selected}");
        assert_eq!(selected, 0.0, "an open menu centres the trailing half's content");
        assert!(
            unselected < -1.0,
            "and an unselected one sits offset toward the gap, measured {unselected}"
        );
    }

    /// The optical shift is a geometric direction, and RTL does not mirror the answer: the trailing half
    /// sits on the LEFT there with the gap to its right, so the correction has to come out positive where
    /// LTR wants it negative. `Modifier::absolute_offset` is what carries it, because a plain `offset`
    /// has its x mirrored a second time by the parent's direction and sent both halves AWAY from the gap.
    ///
    /// Measured against the same composition with the correction switched off, so the reading needs no
    /// assumption about where the content would otherwise sit.
    #[test]
    fn the_optical_shift_points_at_the_gap_in_both_directions() {
        let wrapper_x = |dir: LayoutDirection, optical: bool| -> f32 {
            let theme = ThemeColors::light_from_seed(0x6750A4);
            let interaction = MutableInteractionSource::new();
            let mut composer = Composer::new();
            composer.compose(|ctx| {
                WiniaTheme::with_theme_and_direction(theme.clone(), dir, ctx, |ctx| {
                    SplitButtonLayout::new().build(
                        ctx,
                        |ctx| {
                            SplitButtonDefaults::leading_button(|| {}).build(ctx, |ctx| {
                                Text::new("Add").build(ctx);
                            });
                        },
                        |ctx| {
                            let trailing = SplitButtonDefaults::trailing_button()
                                .on_click(|| {})
                                .interaction_source(interaction.clone());
                            let trailing = if optical {
                                trailing
                            } else {
                                trailing.without_optical_shift()
                            };
                            trailing.build(ctx, |ctx| {
                                Text::new("v").build(ctx);
                            });
                        },
                    );
                });
            });
            composer.layout(Constraints::new(0.0, 400.0, 0.0, 200.0));
            let root = composer.layout_root_idx().expect("root");
            let nodes = composer.arena_nodes();
            let trailing = half_visual(nodes, nodes[root].children[1]);
            let panel = nodes[trailing].children.first().copied().expect("the content box");
            let wrapper = nodes[panel].children.first().copied().expect("the offset wrapper");
            nodes[wrapper].position.x
        };
        let ltr = wrapper_x(LayoutDirection::Ltr, false) - wrapper_x(LayoutDirection::Ltr, true);
        let rtl = wrapper_x(LayoutDirection::Rtl, false) - wrapper_x(LayoutDirection::Rtl, true);
        eprintln!("trailing content correction (unshifted minus shifted): ltr {ltr} rtl {rtl}");
        assert!(
            ltr > 1.0,
            "LTR: the gap is on the content's left, so it moves that way (measured {ltr})"
        );
        assert!(
            rtl < -1.0,
            "RTL: the gap is on its right, so the correction points the other way (measured {rtl})"
        );
    }

    /// The optical offset follows the radii of the shape the half DRAWS, not the token defaults: material3
    /// computes it from the shape it paints (`SplitButton.kt:807-813`). A caller's own set therefore steers
    /// it — a symmetric one has nothing to compensate, and an asymmetric one moves by ITS corners rather
    /// than by the token's 4 dp / 20 dp pair.
    #[test]
    fn the_offset_follows_a_custom_shape_set() {
        let correction = |shapes: SplitButtonShapes| -> f32 {
            let wrapper_x = |optical: bool| -> f32 {
                let theme = ThemeColors::light_from_seed(0x6750A4);
                let interaction = MutableInteractionSource::new();
                let mut composer = Composer::new();
                composer.compose(|ctx| {
                    WiniaTheme::with_theme_and_direction(
                        theme.clone(),
                        LayoutDirection::Ltr,
                        ctx,
                        |ctx| {
                            SplitButtonLayout::new().build(
                                ctx,
                                |ctx| {
                                    SplitButtonDefaults::leading_button(|| {}).build(ctx, |ctx| {
                                        Text::new("Add").build(ctx);
                                    });
                                },
                                |ctx| {
                                    let trailing = SplitButtonDefaults::trailing_button()
                                        .on_click(|| {})
                                        .interaction_source(interaction.clone())
                                        .shapes(shapes.clone());
                                    let trailing = if optical {
                                        trailing
                                    } else {
                                        trailing.without_optical_shift()
                                    };
                                    trailing.build(ctx, |ctx| {
                                        Text::new("v").build(ctx);
                                    });
                                },
                            );
                        },
                    );
                });
                composer.layout(Constraints::new(0.0, 400.0, 0.0, 200.0));
                let root = composer.layout_root_idx().expect("root");
                let nodes = composer.arena_nodes();
                let trailing = half_visual(nodes, nodes[root].children[1]);
                let panel = nodes[trailing].children.first().copied().expect("the content box");
                let wrapper = nodes[panel].children.first().copied().expect("the offset wrapper");
                nodes[wrapper].position.x
            };
            wrapper_x(false) - wrapper_x(true)
        };
        let symmetric = correction(SplitButtonShapes::flat(Shape::rounded(8.0)));
        let asymmetric = correction(SplitButtonShapes::flat(Shape::corners(0.0, 12.0, 12.0, 0.0)));
        eprintln!("custom shape corrections: symmetric {symmetric} asymmetric {asymmetric}");
        assert!(
            symmetric.abs() < 0.01,
            "a symmetric set leaves nothing to compensate (measured {symmetric})"
        );
        assert!(
            (asymmetric - 1.32).abs() < 0.01,
            "an asymmetric set moves by its own corners, 0.11 * 12 dp (measured {asymmetric})"
        );
    }

    /// One source of direction: a node-level `layout_direction` pinned on the pair used to place it one way
    /// and shape it the other, because the policy read the modifier while the halves read the theme. The
    /// layout now hands its resolution down, so pinning the direction turns both.
    #[test]
    fn a_node_level_direction_steers_the_placement_and_the_shapes() {
        let trailing_backgrounds = |pinned: Option<LayoutDirection>| -> Vec<Shape> {
            let theme = ThemeColors::light_from_seed(0x6750A4);
            let interaction = MutableInteractionSource::new();
            let mut composer = Composer::new();
            composer.compose(|ctx| {
                WiniaTheme::with_theme_and_direction(
                    theme.clone(),
                    LayoutDirection::Ltr,
                    ctx,
                    |ctx| {
                        let mut modifier = Modifier::new();
                        if let Some(direction) = pinned {
                            modifier = modifier.layout_direction(direction);
                        }
                        SplitButtonLayout::new().modifier(modifier).build(
                            ctx,
                            |ctx| {
                                SplitButtonDefaults::leading_button(|| {}).build(ctx, |ctx| {
                                    Text::new("Add").build(ctx);
                                });
                            },
                            |ctx| {
                                SplitButtonDefaults::trailing_button()
                                    .interaction_source(interaction.clone())
                                    .on_click(|| {})
                                    .build(ctx, |ctx| {
                                        Text::new("v").build(ctx);
                                    });
                            },
                        );
                    },
                );
            });
            let children = split_visuals(&mut composer);
            drawn_backgrounds(&composer, children[1])
        };
        let inner = SplitButtonDefaults::inner_corner_size(ButtonSize::Small);
        let outer = SplitButtonDefaults::outer_corner_size(SplitButtonDefaults::container_height(
            ButtonSize::Small,
        ));
        let theme_ltr = trailing_backgrounds(None);
        let pinned_rtl = trailing_backgrounds(Some(LayoutDirection::Rtl));
        eprintln!("trailing shape: theme LTR {theme_ltr:?} pinned RTL {pinned_rtl:?}");
        assert_eq!(
            theme_ltr,
            vec![Shape::corners(inner, outer, outer, inner)],
            "LTR: the inner corners face the gap on the left"
        );
        assert_eq!(
            pinned_rtl,
            vec![Shape::corners(outer, inner, inner, outer)],
            "RTL: the same set, mirrored by the pinned direction"
        );
    }

    /// The pair is an invariant, not a case to handle: material3's `SplitButtonLayout` takes exactly two
    /// composables and indexes them, and this layout composes exactly those two. Composing anything else
    /// into it is a caller bug, and it is asserted rather than degraded in silence.
    #[test]
    #[should_panic(expected = "a split button arranges exactly two buttons")]
    fn a_composition_that_is_not_the_pair_is_rejected() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_theme_and_direction(theme.clone(), LayoutDirection::Ltr, ctx, |ctx| {
                let key = ctx.next_key();
                let policy = SplitButtonPolicy { spacing: 2.0, direction: LayoutDirection::Ltr };
                match ctx.start_restartable_group(key, Modifier::new(), policy) {
                    crate::runtime::composer::GroupStatus::Skip => {}
                    crate::runtime::composer::GroupStatus::Enter => {
                        SplitButtonDefaults::leading_button(|| {}).build(ctx, |ctx| {
                            Text::new("Add").build(ctx);
                        });
                    }
                }
                ctx.end_restartable_group();
            });
        });
        // Laying it out is what runs the policy, and the count check with it.
        let _ = split_children(&mut composer);
    }

    /// material3's split halves compose the caller's content straight into their own `Row` with
    /// `Arrangement.Center` and NO spacing (`SplitButton.kt:715-728`) — unlike its `Button`, which wraps
    /// content in `Row(spacedBy(ButtonDefaults.IconSpacing))`. winia matches the split button: the caller's
    /// children touch, and a caller who wants the button's gap passes that spacing in its own content. The
    /// optical offset's wrapper Row is what would quietly change that, so this pins it.
    #[test]
    fn the_halves_do_not_space_the_callers_content() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_theme_and_direction(theme.clone(), LayoutDirection::Ltr, ctx, |ctx| {
                SplitButtonLayout::new().build(
                    ctx,
                    |ctx| {
                        SplitButtonDefaults::leading_button(|| {}).build(ctx, |ctx| {
                            Text::new("Add").build(ctx);
                            Text::new("v").build(ctx);
                        });
                    },
                    |ctx| {
                        SplitButtonDefaults::trailing_button().on_click(|| {}).build(ctx, |ctx| {
                            Text::new("x").build(ctx);
                        });
                    },
                );
            });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 200.0));
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        let leading = half_visual(nodes, nodes[root].children[0]);
        let panel = nodes[leading].children.first().copied().expect("the content box");
        let wrapper = nodes[panel].children.first().copied().expect("the offset wrapper");
        let kids = nodes[wrapper].children.clone();
        assert_eq!(kids.len(), 2, "the caller's two children are the wrapper's two children");
        let first = &nodes[kids[0]];
        let second = &nodes[kids[1]];
        let gap = second.position.x - (first.position.x + first.measured_size.width);
        eprintln!("caller content gap: {gap}");
        assert!(
            gap.abs() < 0.01,
            "no spacing is added between the caller's children (measured {gap})"
        );
    }

    /// Every corner morph runs on `MotionSchemeKeyTokens.DefaultEffects` (`SplitButton.kt:695`,
    /// `:775`, `:887`, applied through `shapeByInteraction` at `:1316-1334`) — damping 1.0, so
    /// the corners settle without overshoot. This used to be a 180 ms `EaseOutCubic` tween of
    /// winia's own, kept only because the token's numbers were missing from the extracted sources;
    /// they are in `tokens/ExpressiveMotionTokens.kt`.
    #[test]
    fn the_shape_morph_runs_on_default_effects() {
        match shape_morph_spec() {
            crate::animation::AnimationSpec::Spring(spec) => {
                assert_eq!(spec, crate::animation::SpringSpec::effects_default());
                assert_eq!(
                    (spec.damping_ratio, spec.stiffness),
                    (1.0, 1600.0),
                    "DefaultEffects is damping 1.0 / stiffness 1600 \
                     (tokens/ExpressiveMotionTokens.kt)"
                );
            }
            other => panic!("the shape morph must be a spring, got {other:?}"),
        }
    }
