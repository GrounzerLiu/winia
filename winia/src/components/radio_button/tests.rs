    use super::*;

    /// M3's `FastSpatial` spring (0.9/1400) for the dot, pinned — a no-bouncy 400 approximation fails.
    #[test]
    fn the_dot_spec_is_m3s_fast_spatial_spring() {
        let spec = radio_dot_spec();
        assert_eq!(spec, crate::animation::SpringSpec::spatial_fast());
        assert_eq!(spec.damping_ratio, 0.9);
        assert_eq!(spec.stiffness, 1400.0);
    }

    /// M3's `DefaultEffects` (spring 1.0/1600) and nothing else: a silent return to a tween fails here.
    #[test]
    fn the_colour_spec_is_m3s_default_effects_spring() {
        let spec = radio_colour_spec();
        assert_eq!(spec, crate::animation::SpringSpec::effects_default());
        assert_eq!(spec.damping_ratio, 1.0);
        assert_eq!(spec.stiffness, 1600.0);
    }

    #[test]
    fn color_state_resolution() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let colors = RadioButtonDefaults::radio_button_colors(&theme);
        // Selected：Primary；Unselected：OnSurfaceVariant
        assert_eq!(colors.radio_color(true, true), theme.primary);
        assert_eq!(colors.radio_color(true, false), theme.on_surface_variant);
        // Disabled：OnSurface @ 38%（两态同值）
        assert_eq!(colors.radio_color(false, true), colors.disabled_selected_color);
        assert_eq!(colors.radio_color(false, false), colors.disabled_unselected_color);
        assert_eq!(colors.disabled_selected_color, colors.disabled_unselected_color);
        assert_eq!(colors.disabled_selected_color.a, (theme.on_surface.a as f32 * 0.38) as u8);
        // 自定义色覆盖
        let custom = RadioButtonColors::new(
            Color::RED, Color::BLUE, Color::from_argb(255, 128, 128, 128), Color::from_argb(255, 128, 128, 128),
        );
        assert_eq!(custom.radio_color(true, true), Color::RED);
        assert_eq!(custom.radio_color(true, false), Color::BLUE);
        assert_eq!(custom.radio_color(false, true), Color::from_argb(255, 128, 128, 128));
    }

    /// `Modifier.selectable_group()` (Compose's `foundation/selection/SelectableGroup.kt:30`, property at
    /// `ui/semantics/SemanticsProperties.kt:67`): the row around a set of radio buttons becomes one element that
    /// says its children are a collection, INSTEAD of being folded away as a transparent container. Without the
    /// modifier there is no such element — that is this test's control.
    #[test]
    fn a_selectable_group_keeps_its_place_and_says_so() {
        // Composing a radio starts its colour/halo animations, and the animation registry is global: without
        // this guard the pixel tests in this module (which read a hover halo mid-fade) see a half-grown halo.
        let _g = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let group = |with_modifier: bool| -> Vec<crate::semantics::SemanticsNode> {
            let mut composer = crate::runtime::composer::Composer::new();
            composer.compose(|ctx: &mut ComposeCtx| {
                let modifier = if with_modifier {
                    Modifier::new().selectable_group()
                } else {
                    Modifier::new()
                };
                crate::layout::Row::new().modifier(modifier).build(ctx, |ctx| {
                    RadioButton::new(true).on_click(|| {}).build(ctx);
                    RadioButton::new(false).on_click(|| {}).build(ctx);
                });
            });
            composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
            let root = composer.layout_root_idx().expect("root");
            crate::semantics::semantics_tree(composer.arena_nodes(), root)
        };

        let tree = group(true);
        let json = crate::semantics::semantics_json(&tree);
        let node = tree
            .iter()
            .find(|n| n.selectable_group)
            .expect("the group element survives in the tree");
        assert_eq!(node.children.len(), 2, "and keeps both radio buttons as its children");
        assert!(
            node.children.iter().all(|c| c.role == Some(crate::semantics::SemanticsRole::RadioButton)),
            "the children are the selectable items"
        );
        // A reader is told about the collection through the same channel the debug server reads.
        assert!(json.contains("\"selectableGroup\":true"), "the flag reaches the semantics JSON: {json}");

        let plain = group(false);
        assert!(
            plain.iter().all(|n| !n.selectable_group),
            "without the modifier nothing claims to be a group"
        );
        assert!(
            crate::semantics::semantics_json(&plain).contains("\"selectableGroup\":false"),
            "the field is present either way, so a consumer can tell the difference"
        );
        // Leave the registry as it was found: this test only wanted the semantics tree, and a parallel pixel
        // test in this module would otherwise read a halo mid-fade.
        crate::animation::clear_all_animations();
    }

    /// `RadioButtonColors::copy` is Compose's partial override: the colours it is not given stay whatever the
    /// source said, which is why a caller can change one state's colour without restating the other three.
    #[test]
    fn colors_copy_overrides_only_what_it_is_given() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let tokens = RadioButtonDefaults::radio_button_colors(&theme);
        let custom = Color::RED;

        let partial = tokens.copy(Some(custom), None, None, None);
        assert_eq!(partial.selected_color, custom, "the one colour it was given");
        assert_eq!(partial.unselected_color, tokens.unselected_color, "unselected keeps its token");
        assert_eq!(
            partial.disabled_selected_color, tokens.disabled_selected_color,
            "disabled selected keeps its token"
        );
        assert_eq!(
            partial.disabled_unselected_color, tokens.disabled_unselected_color,
            "disabled unselected keeps its token"
        );
        // And the resolved colour follows, so the override reaches the component's own lookup.
        assert_eq!(partial.radio_color(true, true), custom);
        assert_eq!(partial.radio_color(true, false), tokens.unselected_color);

        assert_eq!(tokens.copy(None, None, None, None), tokens, "giving nothing keeps everything");
        let all = RadioButtonColors::new(Color::RED, Color::GREEN, Color::BLUE, Color::from_argb(255, 1, 2, 3));
        assert_eq!(
            tokens.copy(Some(Color::RED), Some(Color::GREEN), Some(Color::BLUE), Some(Color::from_argb(255, 1, 2, 3))),
            all,
            "and giving everything reproduces the whole-set form"
        );
    }

    // ── 渲染级像素测试（对齐 checkbox 模式：raster surface + 像素断言）──
    // `origin` is the touch target's OWN top-left corner: its 20 dp visual is centred inside, so the samples
    // below (the visual's centre at +20,+20, the ring's mid-stroke at radius 9 from that centre) hold for any
    // node size — 40 dp before, 48 dp now.
    struct RenderScene {
        px: Vec<[u8; 4]>,
        w: usize,
        origin: (f32, f32), // the touch target's top-left corner
    }

    fn render_radio(selected: bool, enabled: bool, colors: RadioButtonColors) -> RenderScene {
        use skia_safe::{Color as SkColor, surfaces};
        let mut composer = crate::runtime::composer::Composer::new();
        let scene = |ctx: &mut ComposeCtx| {
            RadioButton::new(selected)
                .enabled(enabled)
                .colors(colors)
                .on_click(|| {})
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
        let origin = radio_frame_origin(&composer).expect("the 48x48 touch target is in the tree");
        RenderScene { px: px.to_vec(), w: 300, origin }
    }

    /// The frame every sample offset in these tests is written against: the node's CENTRE mapped onto the old
    /// 40 dp frame, so `+20,+20` is the visual's centre and `±9` its ring's mid-stroke, whatever the node size
    /// is now. Positions in the arena are parent-relative, so they are accumulated from the root — the tests
    /// that used to add a bare `+20` sampled the wrong pixel once the node became 48 dp.
    fn radio_frame_origin(composer: &crate::runtime::composer::Composer) -> Option<(f32, f32)> {
        fn walk(
            nodes: &[crate::layout::node::LayoutNode],
            idx: usize,
            ox: f32,
            oy: f32,
        ) -> Option<(f32, f32)> {
            let node = &nodes[idx];
            let x = ox + node.position.x;
            let y = oy + node.position.y;
            if node.measured_size.width == RADIO_TOUCH_TARGET {
                // The sample offsets below are written against the OLD 40 dp frame (its centre at +20,+20, the
                // ring's mid-stroke 9 dp from it), so rebase the node's centre onto that frame.
                let half = 20.0;
                return Some((
                    x + node.measured_size.width / 2.0 - half,
                    y + node.measured_size.height / 2.0 - half,
                ));
            }
            for &child in &node.children {
                if let Some(hit) = walk(nodes, child, x, y) {
                    return Some(hit);
                }
            }
            None
        }
        let root = composer.layout_root_idx()?;
        walk(composer.arena_nodes(), root, 0.0, 0.0)
    }

    impl RenderScene {
        /// 像素读取：raster_n32_premul 小端为 BGRA 布局——按 (R,G,B) 语义读
        fn at(&self, x: f32, y: f32) -> (i32, i32, i32) {
            let p = self.px[(y as usize) * self.w + (x as usize)];
            (p[2] as i32, p[1] as i32, p[0] as i32)
        }
        fn is_white(&self, x: f32, y: f32) -> bool {
            let p = self.at(x, y);
            p.0 > 245 && p.1 > 245 && p.2 > 245
        }
    }

    fn is_close(a: (i32, i32, i32), b: (i32, i32, i32)) -> bool {
        (a.0 - b.0).abs() <= 6 && (a.1 - b.1).abs() <= 6 && (a.2 - b.2).abs() <= 6
    }


    /// M3 SNAPS the colour when the radio becomes disabled — `RadioButton.kt:146-154` returns the
    /// disabled colour through `rememberUpdatedState` and says why: "there should be no animations
    /// between enabled / disabled". This pins the FIRST frame after the flip: with the old
    /// unconditional spring the ring was still painted in the enabled colour there (and faded over
    /// the following frames); with `Snap` it is the disabled colour immediately.
    #[test]
    fn a_radio_snaps_to_its_disabled_colour_instead_of_fading_to_it() {
        use skia_safe::{Color as SkColor, surfaces};
        let _g = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let colors = RadioButtonDefaults::radio_button_colors(&theme);
        // Selected, so the 10 dp dot fills the centre SOLIDLY: sampling the ring instead lands on its
        // anti-aliased edge (measured 90,86,94 against the solid 73,69,78 — 90% coverage blended with
        // white), which would make the test depend on sub-pixel coverage rather than on the colour.
        let enabled_colour = colors.selected_color;
        let disabled_colour = colors.disabled_selected_color;
        assert_ne!(
            (enabled_colour.r, enabled_colour.g, enabled_colour.b),
            (disabled_colour.r, disabled_colour.g, disabled_colour.b),
            "the two colours must differ for this test to mean anything"
        );

        // Sampled at the visual's CENTRE: the node's top-left plus 20,20 puts us on the dot.
        let ring = |composer: &crate::runtime::composer::Composer| -> (i32, i32, i32) {
            let mut surface = surfaces::raster_n32_premul((300, 300)).unwrap();
            let canvas = surface.canvas();
            canvas.clear(SkColor::WHITE);
            let root = composer.layout_root_idx().expect("root");
            crate::render::render(composer.arena_nodes(), root, canvas);
            let pm = surface.peek_pixels().expect("pixmap");
            let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
            let (ox, oy) = radio_frame_origin(composer).expect("the touch target is in the tree");
            let x = (ox + 20.0) as usize;
            let y = (oy + 20.0) as usize;
            let p = px[y * 300 + x];
            (p[2] as i32, p[1] as i32, p[0] as i32)
        };
        // Compare against what the pixel SHOULD be: the dot colour composited over the white canvas.
        // The disabled colours carry alpha 0.38 (`RadioButtonTokens.*IconOpacity`), so the raw RGB is not
        // what a pixel reads — measured 170,169,171 for (29,27,32) at 0.38 over white.
        let over_white = |c: Color| -> (i32, i32, i32) {
            let a = c.a as f32 / 255.0;
            (
                (c.r as f32 * a + 255.0 * (1.0 - a)) as i32,
                (c.g as f32 * a + 255.0 * (1.0 - a)) as i32,
                (c.b as f32 * a + 255.0 * (1.0 - a)) as i32,
            )
        };
        let near = |a: (i32, i32, i32), b: (i32, i32, i32)| -> bool {
            (a.0 - b.0).abs() <= 8 && (a.1 - b.1).abs() <= 8 && (a.2 - b.2).abs() <= 8
        };

        let enabled = crate::runtime::state::State::new(true);
        let mut composer = crate::runtime::composer::Composer::new();
        let mut build = |composer: &mut crate::runtime::composer::Composer| {
            let e = enabled.clone();
            let c = colors;
            composer.compose(move |ctx: &mut ComposeCtx| {
                RadioButton::new(true).enabled(e.get()).colors(c).on_click(|| {}).build(ctx);
            });
            composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
        };
        build(&mut composer);
        // Settle the enter animation so the enabled colour is fully there.
        for _ in 0..40 {
            if !crate::animation::update_animations() {
                break;
            }
        }
        build(&mut composer);
        assert!(
            near(ring(&composer), over_white(enabled_colour)),
            "enabled: the dot is the enabled colour, got {:?} against {:?}",
            ring(&composer),
            over_white(enabled_colour)
        );

        // Flip to disabled and take ONE frame: no `update_animations` in between.
        enabled.set(false);
        build(&mut composer);
        assert!(
            near(ring(&composer), over_white(disabled_colour)),
            "disabled SNAPS: the FIRST frame is already the disabled colour, got {:?} against {:?}",
            ring(&composer),
            over_white(disabled_colour)
        );
        crate::animation::clear_all_animations();
    }
    /// The node an interactive radio occupies, and the state layer it names.
    ///
    /// `material3/RadioButton.kt:100-115` applies `Modifier.minimumInteractiveComponentSize()` (48 dp) whenever
    /// the radio takes clicks, with its 20 dp icon centred; a radio with no click handler is 24 dp (no minimum).
    /// The halo is `StateLayerSize` (40 dp, `:97`) — smaller than the node, so it travels as a NAMED size while
    /// the spreading layer still reaches the node's diagonal.
    #[test]
    fn an_interactive_radio_reserves_48_dp_and_names_a_40_dp_state_layer() {
        let geometry = |interactive: bool| -> (f32, bool, usize, Option<f32>) {
            let mut composer = crate::runtime::composer::Composer::new();
            composer.compose(|ctx| {
                crate::layout::components::Column::new()
                    .modifier(Modifier::new().fill_max_width())
                    .build(ctx, |ctx| {
                        let rb = RadioButton::new(false);
                        let rb = if interactive { rb.on_click(|| {}) } else { rb };
                        rb.build(ctx);
                    });
            });
            composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
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
            let node = nodes
                .iter()
                .find(|n| {
                    n.measured_size.width == RADIO_TOUCH_TARGET
                        || n.measured_size.width == RADIO_IDLE_SIZE
                })
                .expect("the radio node");
            let has_ripple = node
                .modifier
                .elements()
                .iter()
                .any(|el| matches!(el, crate::modifier::ModifierElement::Ripple { .. }));
            let state_layer = node.modifier.elements().iter().find_map(|el| match el {
                crate::modifier::ModifierElement::Ripple { state_layer_diameter, .. } => {
                    state_layer_diameter.as_ref().map(|d| d.eval())
                }
                _ => None,
            });
            (node.measured_size.width, has_ripple, clickables, state_layer)
        };

        let (size, has_ripple, clickables, state_layer) = geometry(true);
        assert_eq!(size, 48.0, "an interactive radio reserves 48 dp");
        assert_eq!(clickables, 1, "and carries one clickable node");
        assert!(has_ripple, "with an unbounded ripple");
        assert_eq!(
            state_layer,
            Some(RADIO_STATE_LAYER_SIZE),
            "and names a 40 dp state layer, smaller than the node"
        );

        let (size, has_ripple, clickables, _) = geometry(false);
        assert_eq!(size, RADIO_IDLE_SIZE, "with no click handler the minimum is not applied");
        assert_eq!(clickables, 0, "and nothing is clickable");
        assert!(!has_ripple, "and there is no ripple");
    }

    /// A focused radio paints its focus ring OUTSIDE its box, and an unfocused one does not.
    ///
    /// The ring is drawn 3 dp wide with its centre line `gap (2) + 1.5` outside the node's rect, and the node
    /// used to carry `clip(Circle)` — which the renderer sets up BEFORE the ring, so it cut the ring away. That
    /// is the "the focus ring is gone" report: the shape now travels as a zero-width transparent border, which
    /// states the shape without clipping (measured here at the ring's mid-stroke radius).
    #[test]
    fn a_focused_radio_paints_its_ring_outside_the_node() {
        use skia_safe::{Color as SkColor, surfaces};
        let pixels = |focused: bool| -> Vec<[u8; 4]> {
            let mut composer = crate::runtime::composer::Composer::new();
            let source = MutableInteractionSource::new();
            let src = source.clone();
            composer.compose(move |ctx: &mut ComposeCtx| {
                RadioButton::new(false)
                    .on_click(|| {})
                    .interaction_source(src.clone())
                    .build(ctx);
            });
            composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
            if focused {
                source.emit_focus();
            }
            composer.compose(move |ctx: &mut ComposeCtx| {
                RadioButton::new(false)
                    .on_click(|| {})
                    .interaction_source(source.clone())
                    .build(ctx);
            });
            composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
            let mut surface = surfaces::raster_n32_premul((300, 300)).unwrap();
            let canvas = surface.canvas();
            canvas.clear(SkColor::WHITE);
            let root = composer.layout_root_idx().expect("root");
            crate::render::render(composer.arena_nodes(), root, canvas);
            // Real frames so the ring's fade-in is complete.
            for _ in 0..40 {
                if !crate::animation::update_animations() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            let mut surface2 = surfaces::raster_n32_premul((300, 300)).unwrap();
            let canvas2 = surface2.canvas();
            canvas2.clear(SkColor::WHITE);
            crate::render::render(composer.arena_nodes(), root, canvas2);
            let pm = surface2.peek_pixels().expect("pixmap");
            pm.pixels::<[u8; 4]>().expect("pixels").to_vec()
        };

        // The ring's centre line: half the node plus gap + half the 3 dp stroke.
        let ring_radius = RADIO_TOUCH_TARGET / 2.0 + 2.0 + 1.5;
        let centre = RADIO_TOUCH_TARGET / 2.0;
        let ring_pixel = |px: &[[u8; 4]], angle_deg: f32| -> (i32, i32, i32) {
            let a = angle_deg.to_radians();
            let x = (centre + ring_radius * a.cos()) as usize;
            let y = (centre + ring_radius * a.sin()) as usize;
            let p = px[y * 300 + x];
            (p[2] as i32, p[1] as i32, p[0] as i32)
        };
        let is_white = |p: (i32, i32, i32)| p.0 > 245 && p.1 > 245 && p.2 > 245;

        let unfocused = pixels(false);
        assert!(
            is_white(ring_pixel(&unfocused, 0.0)) && is_white(ring_pixel(&unfocused, 180.0)),
            "no ring while the radio is not focused"
        );

        let focused = pixels(true);
        let on_ring = [ring_pixel(&focused, 0.0), ring_pixel(&focused, 180.0), ring_pixel(&focused, 90.0)];
        assert!(
            on_ring.iter().any(|p| !is_white(*p)),
            "the focused radio paints its ring at radius {ring_radius} (sampled {on_ring:?})"
        );
    }

    /// A hovered radio paints exactly ONE halo, and its radius is `StateLayerSize / 2` (20 dp).
    ///
    /// The halo's size is the thing the user kept correcting, so it is measured here by walking a ray out of the
    /// node's centre and reporting the radius at which the colour stops differing from the background — on four
    /// rays, so a stray second layer of any size shows up as a disagreement. `RADIO_STATE_LAYER_SIZE` is the only
    /// number involved; if a bigger disc appears again, this test names it.
    #[test]
    fn a_hovered_radio_paints_one_halo_of_its_state_layer_size() {
        use skia_safe::{Color as SkColor, surfaces};
        // The halo grows over frames, and the animation registry is global: without the guard a test running in
        // parallel can clear it mid-growth and this one then measures a half-grown disc (radius 10 of 20).
        let _g = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let mut composer = crate::runtime::composer::Composer::new();
        let source = MutableInteractionSource::new();
        let src = source.clone();
        composer.compose(move |ctx: &mut ComposeCtx| {
            RadioButton::new(false)
                .on_click(|| {})
                .interaction_source(src.clone())
                .build(ctx);
        });
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
        source.emit_hover_enter();
        composer.compose(move |ctx: &mut ComposeCtx| {
            RadioButton::new(false)
                .on_click(|| {})
                .interaction_source(source.clone())
                .build(ctx);
        });
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
        for _ in 0..40 {
            if !crate::animation::update_animations() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let mut surface = surfaces::raster_n32_premul((300, 300)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(SkColor::WHITE);
        let root = composer.layout_root_idx().expect("root");
        crate::render::render(composer.arena_nodes(), root, canvas);
        let pm = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        let at = |x: f32, y: f32| -> (i32, i32, i32) {
            let p = px[(y as usize) * 300 + (x as usize)];
            (p[2] as i32, p[1] as i32, p[0] as i32)
        };

        // The node sits at the surface's origin (a single-node scene), so its centre is half the target size.
        let centre = RADIO_TOUCH_TARGET / 2.0;
        let background = at(2.0, 2.0);
        let mut outermost = 0.0f32;
        for step in 0..40 {
            let r = step as f32;
            let mut painted = false;
            for angle_deg in [0.0f32, 90.0, 180.0, 270.0] {
                let a = angle_deg.to_radians();
                let p = at(centre + r * a.cos(), centre + r * a.sin());
                if p != background {
                    painted = true;
                }
            }
            if painted {
                outermost = r;
            }
        }
        assert!(
            (outermost - RADIO_STATE_LAYER_SIZE / 2.0).abs() <= 1.5,
            "the halo's outermost painted row is radius {outermost}, expected {} (StateLayerSize / 2)",
            RADIO_STATE_LAYER_SIZE / 2.0
        );
    }

    /// The press ripple is CLIPPED to the halo, and it still finishes.
    ///
    /// The missing clip is what made a wider disc appear around a pressed radio: the foreground spread to its
    /// own maximum instead of being held inside the background circle. This measures the outermost painted
    /// radius while the press is live (it must be the halo and no more) and after the animation is done (it must
    /// fall back to nothing outside the halo).
    #[test]
    fn the_press_ripple_is_clipped_to_the_halo_and_still_finishes() {
        use skia_safe::{Color as SkColor, surfaces};
        let mut composer = crate::runtime::composer::Composer::new();
        let source = MutableInteractionSource::new();
        let src = source.clone();
        composer.compose(move |ctx: &mut ComposeCtx| {
            RadioButton::new(false)
                .on_click(|| {})
                .interaction_source(src.clone())
                .build(ctx);
        });
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
        source.emit_press_at((60.0, 60.0));
        let src2 = source.clone();
        composer.compose(move |ctx: &mut ComposeCtx| {
            RadioButton::new(false)
                .on_click(|| {})
                .interaction_source(src2.clone())
                .build(ctx);
        });
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
        // Enough frames for the foreground to reach the halo: the ripple expands over its own duration, so a
        // short run samples a half-grown disc (10 dp after 80 frames, 19 dp once it is done).
        for _ in 0..300 {
            if !crate::animation::update_animations() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(3));
        }
        source.emit_release();

        let sample = |composer: &crate::runtime::composer::Composer| -> f32 {
            let mut surface = surfaces::raster_n32_premul((300, 300)).unwrap();
            let canvas = surface.canvas();
            canvas.clear(SkColor::WHITE);
            let root = composer.layout_root_idx().expect("root");
            crate::render::render(composer.arena_nodes(), root, canvas);
            let pm = surface.peek_pixels().expect("pixmap");
            let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
            let at = |x: f32, y: f32| -> (i32, i32, i32) {
                let p = px[(y as usize) * 300 + (x as usize)];
                (p[2] as i32, p[1] as i32, p[0] as i32)
            };
            let centre = RADIO_TOUCH_TARGET / 2.0;
            let background = at(2.0, 2.0);
            let mut outermost = 0.0f32;
            for step in 0..60 {
                let r = step as f32;
                for angle_deg in [0.0f32, 45.0, 90.0, 135.0, 180.0, 225.0, 270.0, 315.0] {
                    let a = angle_deg.to_radians();
                    if at(centre + r * a.cos(), centre + r * a.sin()) != background {
                        outermost = r;
                    }
                }
            }
            outermost
        };

        let during = sample(&composer);
        assert!(
            during <= RADIO_STATE_LAYER_SIZE / 2.0 + 1.5,
            "the clip holds the foreground inside the halo — a wider disc is the missing clip (measured {during})"
        );

        for _ in 0..400 {
            if !crate::animation::update_animations() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(3));
        }
        let after = sample(&composer);
        assert!(
            after <= RADIO_STATE_LAYER_SIZE / 2.0 + 1.5,
            "after the ripple has faded nothing may remain outside the halo (measured radius {after})"
        );
    }

    #[test]
    fn selected_renders_ring_and_dot() {
        // 选中：圆环 + 内点都是 selected 色（红），圆环外角为白
        let scene = render_radio(
            true,
            true,
            RadioButtonColors::new(Color::RED, Color::BLUE, Color::from_argb(255, 128, 128, 128), Color::from_argb(255, 128, 128, 128)),
        );
        let (ox, oy) = scene.origin;
        // 中心 (20,20)：内点红
        assert!(
            is_close(scene.at(ox + 20.0, oy + 20.0), (255, 0, 0)),
            "选中态中心应为内点色（红）"
        );
        // 圆环顶部中径 (20,11)（距离圆心 9）：红
        assert!(
            is_close(scene.at(ox + 20.0, oy + 11.0), (255, 0, 0)),
            "选中态圆环顶部应为红（实际 {:?}）",
            scene.at(ox + 20.0, oy + 11.0)
        );
        // 圆环左侧中径 (11,20)：红
        assert!(
            is_close(scene.at(ox + 11.0, oy + 20.0), (255, 0, 0)),
            "选中态圆环左侧应为红"
        );
        // 视觉层外角 (11,11)：白（圆环外）
        assert!(scene.is_white(ox + 11.0, oy + 11.0), "圆环外角应为白");
    }

    #[test]
    fn unselected_renders_ring_only() {
        // 未选中：圆环 unselected 色（蓝），中心白（无内点）
        let scene = render_radio(
            false,
            true,
            RadioButtonColors::new(Color::RED, Color::BLUE, Color::from_argb(255, 128, 128, 128), Color::from_argb(255, 128, 128, 128)),
        );
        let (ox, oy) = scene.origin;
        // 圆环中径半径 9：顶部 (20,11) / 左侧 (11,20) 距离圆心 9——真圆环点
        assert!(
            is_close(scene.at(ox + 20.0, oy + 11.0), (0, 0, 255)),
            "未选中态圆环顶部应为 unselected 色（蓝）（实际 {:?}）",
            scene.at(ox + 20.0, oy + 11.0)
        );
        assert!(
            is_close(scene.at(ox + 11.0, oy + 20.0), (0, 0, 255)),
            "未选中态圆环左侧应为 unselected 色（蓝）"
        );
        assert!(scene.is_white(ox + 20.0, oy + 20.0), "未选中态中心应为白（无内点）");
    }

    #[test]
    fn disabled_uses_disabled_color() {
        // 禁用：圆环/内点都用 disabled 色（绿）
        let scene = render_radio(
            true,
            false,
            RadioButtonColors::new(Color::RED, Color::BLUE, Color::GREEN, Color::from_argb(255, 128, 128, 128)),
        );
        let (ox, oy) = scene.origin;
        let dc = scene.at(ox + 20.0, oy + 20.0);
        assert!(
            is_close(dc, (0, 255, 0)),
            "禁用选中态中心应为 disabled 色（绿）（实际 {dc:?}）"
        );
        assert!(
            is_close(scene.at(ox + 20.0, oy + 11.0), (0, 255, 0)),
            "禁用选中态圆环顶部应为 disabled 色（绿）（实际 {:?}）",
            scene.at(ox + 20.0, oy + 11.0)
        );
    }

    #[test]
    fn deselect_transition_keeps_selected_color() {
        let _g = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        // 状态切换驱动：点击回调里翻转 selected
        let sel = crate::runtime::state::State::new(true);
        let mut composer = crate::runtime::composer::Composer::new();
        let scene = |ctx: &mut ComposeCtx| {
            let s = sel.clone();
            RadioButton::new(sel.get())
                .colors(RadioButtonColors::new(Color::RED, Color::BLUE, Color::from_argb(255, 128, 128, 128), Color::from_argb(255, 128, 128, 128)))
                .on_click(move || s.update(|v| *v = !*v))
                .build(ctx);
        };
        composer.compose(scene);
        composer.compose(scene);
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
        // 触发取消选中（状态立即切 false，但颜色动画尚未推进）
        sel.update(|v| *v = false);
        composer.compose(scene);
        composer.compose(scene);
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
        let mut surface = skia_safe::surfaces::raster_n32_premul((300, 300)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::WHITE);
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        crate::render::render(nodes, root, canvas);
        let pm = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        // The touch target centre, in the same 40 dp reference frame (`radio_frame_origin`).
        let frame = radio_frame_origin(&composer).expect("the touch target is in the tree");
        let center = (frame.0 + 20.0, frame.1 + 20.0);
        let at = |x: f32, y: f32| {
            let p = px[(y as usize) * 300 + (x as usize)];
            (p[2] as i32, p[1] as i32, p[0] as i32) // BGRA → RGB
        };
        // 过渡帧：内点必须仍保留 selected 色（红）——颜色动画在状态切换后
        // 保持旧值直到动画推进（animate_color_as_state 从旧色过渡），防未来
        // 改动把颜色瞬切（吞掉退出动画）
        assert!(
            is_close(at(center.0, center.1), (255, 0, 0)),
            "取消选中过渡帧内点应保留 selected 色（实际 {:?}）——颜色瞬切吞掉退出动画",
            at(center.0, center.1)
        );
        // 动画推完 → 未选中静止态：中心白
        for _ in 0..400 {
            if !crate::animation::update_animations() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        composer.compose(scene);
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
        let mut surface = skia_safe::surfaces::raster_n32_premul((300, 300)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::WHITE);
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        crate::render::render(nodes, root, canvas);
        let pm = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        let at2 = |x: f32, y: f32| {
            let p = px[(y as usize) * 300 + (x as usize)];
            (p[2] as i32, p[1] as i32, p[0] as i32) // BGRA → RGB
        };
        let c = at2(center.0, center.1);
        assert!(
            c.0 > 245 && c.1 > 245 && c.2 > 245,
            "未选中静止态中心应为白（实际 {c:?}）"
        );
    }

