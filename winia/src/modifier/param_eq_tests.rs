    use super::*;

    /// A flex item's own config is a parameter of the FlexBox that measured it: a changed grow,
    /// basis or align-self must re-enter, or a Clean line keeps its old geometry.
    #[test]
    fn param_eq_compares_the_flex_item_config() {
        use crate::layout::{FlexAlignSelf, FlexBasis, FlexItemConfig};

        let a = Modifier::new().flex(FlexItemConfig::new().grow(1.0).basis_dp(80.0));
        let b = Modifier::new().flex(FlexItemConfig::new().grow(1.0).basis_dp(80.0));
        assert!(a.param_eq(&b), "two identical flex configs are the same parameter");

        let grown = Modifier::new().flex(FlexItemConfig::new().grow(2.0).basis_dp(80.0));
        assert!(!a.param_eq(&grown), "a different grow must re-enter");

        let percent =
            Modifier::new().flex(FlexItemConfig::new().grow(1.0).basis(FlexBasis::Percent(0.5)));
        assert!(!a.param_eq(&percent), "a different basis must re-enter");

        let aligned = Modifier::new().flex(
            FlexItemConfig::new()
                .grow(1.0)
                .basis_dp(80.0)
                .align_self(FlexAlignSelf::End),
        );
        assert!(!a.param_eq(&aligned), "a different align-self must re-enter");
    }

    /// A changed `content_align_self` must re-enter the box: the parameter is part of the Skip
    /// contract (`BoxLayout` places children with it), and an element missing from `param_eq` is a
    /// change a Clean slot skips past forever.
    #[test]
    fn param_eq_compares_the_box_child_alignment() {
        use crate::layout::ContentAlignment;
        let a = Modifier::new().content_align_self(ContentAlignment::TOP_END);
        let b = Modifier::new().content_align_self(ContentAlignment::BOTTOM_END);
        let a2 = Modifier::new().content_align_self(ContentAlignment::TOP_END);
        assert!(!a.param_eq(&b), "a different corner must compare unequal");
        assert!(a.param_eq(&a2), "the same corner must compare equal");

        // `match_parent_size` is a declaration like any other: it must compare unequal to its absence
        // (a Clean slot would otherwise keep a stale box size), and equal to itself.
        let matching = Modifier::new().match_parent_size();
        let plain = Modifier::new();
        assert!(!matching.param_eq(&plain), "adding match_parent_size must re-enter");
        assert!(matching.param_eq(&Modifier::new().match_parent_size()));

        // The one-axis carrier too, and the two kinds are not interchangeable.
        let one = Modifier::new().align_self(crate::layout::Alignment::Center);
        let two = Modifier::new().content_align_self(ContentAlignment::both(crate::layout::Alignment::Center));
        assert!(!one.param_eq(&two), "a 1-D and a 2-D alignment are different declarations");
    }

    /// The item-animation contract is a parameter like any other: two specs with the same curves
    /// must compare equal (the comparison runs per item per frame), and a changed curve must re-enter.
    #[test]
    fn param_eq_compares_the_item_animation_spec_by_value() {
        use crate::animation::item::ItemAnimationSpec;
        use crate::animation::interpolator::Linear;
        use crate::animation::{SpringSpec, TweenSpec};
        use std::time::Duration;

        let build = || {
            Modifier::new().animate_item_with(
                ItemAnimationSpec::new()
                    .placement(SpringSpec::default())
                    .fade_in(TweenSpec::new(Duration::from_millis(300), Linear::new())),
            )
        };
        let a = build();
        let b = build();
        assert!(a.param_eq(&b), "a rebuilt spec with the same curves must compare equal");

        let slower = Modifier::new().animate_item_with(
            ItemAnimationSpec::new()
                .placement(SpringSpec::default())
                .fade_in(TweenSpec::new(Duration::from_millis(400), Linear::new())),
        );
        assert!(!a.param_eq(&slower), "a changed fade curve must re-enter");

        // The default marker and a spec-less modifier are different declarations.
        assert!(!Modifier::new().animate_item().param_eq(&Modifier::new()));
        assert!(Modifier::new().animate_item().param_eq(&Modifier::new().animate_item()));
    }

    #[test]
    fn param_eq_compares_the_visibility() {
        let hidden = Modifier::new().visible(false);
        assert!(
            !hidden.param_eq(&Modifier::new().visible(true)),
            "a different visibility must compare unequal, or the node would stay hidden"
        );
        assert!(hidden.param_eq(&Modifier::new().visible(false)));
        // The comparison is structural, so a chain that GAINS a `visible(true)` is not equal to a
        // chain without one — the container enters once and the node settles from then on.
        assert!(!Modifier::new().param_eq(&Modifier::new().visible(true)));
    }

    #[test]
    fn param_eq_detects_size_change() {
        let a = Modifier::new().width(200.0);
        let b = Modifier::new().width(360.0);
        assert!(!a.param_eq(&b), "width 200 vs 360 必须不等");
        let c = Modifier::new().width(200.0);
        assert!(a.param_eq(&c), "width 相同必须相等");
    }


    /// A brush is a `Dyn<Brush>`: a changed STATIC gradient re-enters (otherwise the old gradient
    /// keeps being painted), two dynamic ones are the same request, and the shape is still compared.
    #[test]
    fn param_eq_compares_a_static_brush() {
        use crate::graphics::Brush;
        use crate::runtime::state::State;
        let red = Color::from_argb(255, 255, 0, 0);
        let blue = Color::from_argb(255, 0, 0, 255);

        let a = Modifier::new().background_brush(Brush::solid(red), Shape::rounded(8.0));
        let b = Modifier::new().background_brush(Brush::solid(blue), Shape::rounded(8.0));
        assert!(!a.param_eq(&b), "a changed static brush must re-enter");
        let same = Modifier::new().background_brush(Brush::solid(red), Shape::rounded(8.0));
        assert!(a.param_eq(&same), "the same brush is the same parameter");

        let s1 = State::new(Brush::solid(red));
        let s2 = State::new(Brush::solid(blue));
        let d1 = Modifier::new().background_brush(&s1, Shape::rounded(8.0));
        let d2 = Modifier::new().background_brush(&s2, Shape::rounded(8.0));
        assert!(d1.param_eq(&d2), "two dynamic brushes are the same request");
        assert!(!a.param_eq(&d1), "a literal turning into a state is a change");
    }


    /// A shape is a `Dyn<Shape>`: a changed STATIC shape re-enters, two dynamic shapes are the same
    /// request, and a literal turning into a state is a change.
    #[test]
    fn param_eq_compares_a_static_shape() {
        use crate::runtime::state::State;
        let red = Color::from_argb(255, 255, 0, 0);
        let a = Modifier::new().background(red, Shape::rounded(4.0));
        let b = Modifier::new().background(red, Shape::rounded(8.0));
        assert!(!a.param_eq(&b), "a changed static shape must re-enter");
        let same = Modifier::new().background(red, Shape::rounded(4.0));
        assert!(a.param_eq(&same));

        let s1 = State::new(Shape::rounded(4.0));
        let s2 = State::new(Shape::rounded(8.0));
        let d1 = Modifier::new().background(red, &s1);
        let d2 = Modifier::new().background(red, &s2);
        assert!(d1.param_eq(&d2), "two dynamic shapes are the same request");
        assert!(!a.param_eq(&d1), "a literal turning into a state is a change");
    }

    /// A background colour is now a `Dyn`: two DYNAMIC colours are the same parameter (the value is
    /// read at paint time), while a changed STATIC colour has to re-enter — otherwise the node keeps
    /// painting the old colour.
    /// One `Border` element now carries a `Dyn<Color>`: a state-driven colour is read per paint, and
    /// the parameter comparison keeps the static/dynamic distinction the old two variants encoded.
    /// The ripple colour is a carrier: a state-driven one is read per paint, and the parameter
    /// comparison keeps "changed static" and "two dynamics" apart.
    #[test]
    fn param_eq_compares_the_size_range_elements() {
        let a = Modifier::new().width_in(10.0, 20.0);
        assert!(a.param_eq(&Modifier::new().width_in(10.0, 20.0)));
        assert!(!a.param_eq(&Modifier::new().width_in(10.0, 21.0)));
        assert!(
            !Modifier::new().required_width_in(10.0, 20.0).param_eq(&a),
            "enforce_incoming is part of the parameter"
        );
        let wrap = Modifier::new().wrap_content_size(crate::layout::ContentAlignment::CENTER, false);
        assert!(wrap.param_eq(&Modifier::new().wrap_content_size(
            crate::layout::ContentAlignment::CENTER,
            false
        )));
        assert!(!wrap.param_eq(&Modifier::new().wrap_content_size(
            crate::layout::ContentAlignment::CENTER,
            true
        )));
        assert!(
            !Modifier::new().fill_max_width().param_eq(&Modifier::new().fill_max_width_fraction(0.5)),
            "a fraction change must re-enter"
        );
        assert!(Modifier::new()
            .default_min_size(10.0, 20.0)
            .param_eq(&Modifier::new().default_min_size(10.0, 20.0)));
    }

    #[test]
    fn clip_scrollable_container_and_overscroll_are_parameters() {
        use crate::overscroll::{Orientation, OverscrollEffect};
        let vertical = Modifier::new().clip_scrollable_container(Orientation::Vertical);
        assert_eq!(vertical.clip_scrollable_vertical(), Some(true));
        let horizontal = Modifier::new().clip_scrollable_container(Orientation::Horizontal);
        assert_eq!(horizontal.clip_scrollable_vertical(), Some(false));
        assert!(!vertical.param_eq(&horizontal));

        let a = OverscrollEffect::new();
        let b = OverscrollEffect::new();
        assert!(Modifier::new().overscroll(&a).param_eq(&Modifier::new().overscroll(&a)));
        assert!(!Modifier::new().overscroll(&a).param_eq(&Modifier::new().overscroll(&b)));
    }

    #[test]
    fn draw_with_cache_rebuilds_only_when_the_size_changes() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let builds = std::sync::Arc::new(AtomicUsize::new(0));
        let draws = std::sync::Arc::new(AtomicUsize::new(0));
        let (b, d) = (builds.clone(), draws.clone());
        let m = Modifier::new().draw_with_cache(
            move |scope| {
                b.fetch_add(1, Ordering::Relaxed);
                DrawCacheScope {
                    width: scope.width,
                    height: scope.height,
                    density: scope.density,
                }
            },
            move |cached, _canvas, _rect| {
                d.fetch_add(1, Ordering::Relaxed);
                assert_eq!(cached.width + cached.height, cached.width + cached.height);
            },
        );
        let node = m.draw_nodes().next().expect("one draw node").clone();
        let mut surface = skia_safe::surfaces::raster_n32_premul((100, 100)).unwrap();
        let canvas = surface.canvas();
        let rect = skia_safe::Rect::from_xywh(0.0, 0.0, 60.0, 40.0);
        node.draw(canvas, rect);
        node.draw(canvas, rect);
        assert_eq!(builds.load(Ordering::Relaxed), 1, "same size reuses the cache");
        assert_eq!(draws.load(Ordering::Relaxed), 2, "draw runs every paint");
        node.draw(canvas, skia_safe::Rect::from_xywh(0.0, 0.0, 30.0, 30.0));
        assert_eq!(builds.load(Ordering::Relaxed), 2, "a size change rebuilds");
    }

    #[test]
    fn a_ripple_colour_can_be_dynamic() {
        use crate::runtime::state::State;
        let red = Color::from_argb(255, 255, 0, 0);
        let green = Color::from_argb(255, 0, 128, 0);
        let state = State::new(red);
        let source = MutableInteractionSource::new();
        let m = Modifier::new().ripple(&source, &state, true);
        let ModifierElement::Ripple { color, state_layer_diameter, .. } = &m.elements()[0] else {
            panic!("a ripple is one element");
        };
        assert!(color.is_dynamic());
        assert_eq!(color.eval(), red);
        state.set(green);
        assert_eq!(color.eval(), green);
        assert!(state_layer_diameter.is_none());

        let a = Modifier::new().ripple(&source, red, true);
        let b = Modifier::new().ripple(&source, green, true);
        assert!(!a.param_eq(&b), "a changed static ripple colour must re-enter");
        let other = State::new(red);
        assert!(
            Modifier::new().ripple(&source, &state, true)
                .param_eq(&Modifier::new().ripple(&source, &other, true)),
            "two dynamic ripple colours are the same request"
        );
        assert!(!a.param_eq(&m), "a literal turning into a state is a change");
    }

    /// The paint-time carriers added on top of Shape/Border: image alpha, blur radius, drop-shadow
    /// params and the layer convenience methods all read their state at eval, not at build.
    #[test]
    fn paint_time_carriers_read_their_states() {
        use crate::runtime::state::State;

        let alpha = State::new(0.5f32);
        let m = Modifier::new().image_content(
            crate::graphics::IconSource::svg_path("M0 0h1z"),
            crate::graphics::ContentScale::Fit,
            crate::graphics::ImageAlignment::Center,
            &alpha,
            None,
            FilterQuality::Low,
        );
        let ModifierElement::ImageContent { alpha: a, .. } = &m.elements()[0] else {
            panic!("an image is one element");
        };
        assert!(a.is_dynamic());
        assert_eq!(a.eval(), 0.5);
        alpha.set(0.25);
        assert_eq!(a.eval(), 0.25);

        let blur = State::new(4.0f32);
        let m = Modifier::new().blur(&blur);
        let ModifierElement::Blur { radius } = &m.elements()[0] else {
            panic!("blur is one element");
        };
        assert!(radius.is_dynamic());
        blur.set(8.0);
        assert_eq!(radius.eval(), 8.0);

        let shadow = State::new(ShadowParams::new(2.0, 0.0, 1.0, Color::BLACK, 0.3));
        let m = Modifier::new().drop_shadow(Shape::Rectangle, &shadow);
        let ModifierElement::Shadow { params, .. } = &m.elements()[0] else {
            panic!("a drop shadow is one element");
        };
        assert!(params.is_dynamic());
        shadow.set(ShadowParams::new(4.0, 0.0, 2.0, Color::BLACK, 0.2));
        assert_eq!(params.eval().radius, 4.0);

        let layer = State::new(0.5f32);
        let m = Modifier::new().alpha(&layer);
        let ModifierElement::GraphicsLayer { params } = &m.elements()[0] else {
            panic!("alpha is a layer");
        };
        assert!(params.is_dynamic());
        assert_eq!(params.eval().alpha, 0.5);
        assert!(params.eval().clip, "alpha < 1 clips to bounds");
        layer.set(0.25);
        assert_eq!(params.eval().alpha, 0.25);
        assert!(!m.param_eq(&Modifier::new().alpha(0.5f32)), "a state is not a literal");
    }

    #[test]
    fn a_border_colour_can_be_dynamic() {
        use crate::runtime::state::State;
        let red = Color::from_argb(255, 255, 0, 0);
        let green = Color::from_argb(255, 0, 128, 0);
        let state = State::new(red);
        let m = Modifier::new().border(2.0, &state, Shape::rounded(8.0));
        let ModifierElement::Border { color, .. } = &m.elements()[0] else {
            panic!("a border is one element");
        };
        assert!(color.is_dynamic());
        assert_eq!(color.eval(), red);
        state.set(green);
        assert_eq!(color.eval(), green);

        let a = Modifier::new().border(2.0, red, Shape::rounded(8.0));
        let b = Modifier::new().border(2.0, green, Shape::rounded(8.0));
        assert!(!a.param_eq(&b), "a changed static border colour must re-enter");
        assert!(a.param_eq(&Modifier::new().border(2.0, red, Shape::rounded(8.0))));
        assert!(
            m.param_eq(&Modifier::new().border(2.0, &state, Shape::rounded(8.0))),
            "two dynamic borders are the same request"
        );
        assert!(!a.param_eq(&m), "a literal turning into a state is a change");
    }

    #[test]
    fn param_eq_compares_a_static_background_colour() {
        use crate::runtime::state::State;
        let blue = Color::from_argb(255, 66, 133, 244);
        let green = Color::from_argb(255, 76, 175, 80);
        let a = Modifier::new().width(200.0).background(blue, Shape::rounded(8.0));
        let b = Modifier::new().width(200.0).background(green, Shape::rounded(8.0));
        assert!(!a.param_eq(&b), "a changed static colour must re-enter");

        let same = Modifier::new().width(200.0).background(blue, Shape::rounded(8.0));
        assert!(a.param_eq(&same), "the same colour is the same parameter");

        let s1 = State::new(blue);
        let s2 = State::new(green);
        let d1 = Modifier::new().width(200.0).background(&s1, Shape::rounded(8.0));
        let d2 = Modifier::new().width(200.0).background(&s2, Shape::rounded(8.0));
        assert!(d1.param_eq(&d2), "two dynamic colours are the same request");
        assert!(!a.param_eq(&d1), "a literal turning into a state is a change");
    }

    #[test]
    fn param_eq_element_count_mismatch() {
        let a = Modifier::new().width(200.0);
        let b = Modifier::new().width(200.0).padding(4.0);
        assert!(!a.param_eq(&b), "元素数不同必须不等");
    }

    /// Every lazy container attaches a `LazyScroll` element, so without an arm here the element fell
    /// through to the catch-all `false` and no lazy list could ever Skip - the parameter declarations
    /// in `LazyList::build` were inert. The element compares by the content-height channel it carries
    /// (same list, same channel) plus the direction flag.
    #[test]
    fn param_eq_pairs_lazy_scroll_elements() {
        let height = crate::runtime::state::Backchannel::new(0.0);
        let same_channel = height.clone();
        let another_list = crate::runtime::state::Backchannel::new(0.0);

        let forward = Modifier::new().lazy_scroll(height.clone());
        assert!(
            forward.param_eq(&Modifier::new().lazy_scroll(same_channel)),
            "the same content-height channel is the same element"
        );
        assert!(
            !forward.param_eq(&Modifier::new().lazy_scroll(another_list)),
            "another list's channel is another element"
        );

        // The same list, mirrored: only the flag differs, and the flag decides the render mirror.
        let reversed = Modifier::new().lazy_scroll(height).lazy_scroll_reverse(true);
        assert!(
            !forward.param_eq(&reversed),
            "the direction flag is part of the element"
        );
    }

    // ── GraphicsLayer 便捷包装（alpha/rotate/scale + transformOrigin）──

    #[test]
    fn default_transform_origin_is_center() {
        // 对标 Compose：transformOrigin 默认 Center（0.5, 0.5）
        let p = GraphicsLayerParams::default();
        assert_eq!(p.transform_origin, TransformOrigin::CENTER);
        assert!(!p.clip, "graphics_layer 本身默认不 clip（alpha 便捷版才 clip）");
        assert_eq!(p.ambient_shadow_color, DEFAULT_AMBIENT_SHADOW_COLOR);
        assert_eq!(p.spot_shadow_color, DEFAULT_SPOT_SHADOW_COLOR);
    }

    #[test]
    fn graphics_layers_fold_and_preserve_user_colors() {
        let custom = GraphicsLayerParams {
            alpha: 0.5,
            translation_x: 4.0,
            ambient_shadow_color: Color::RED,
            spot_shadow_color: Color::BLUE,
            ..Default::default()
        };
        let p = Modifier::new()
            .graphics_layer(GraphicsLayerParams {
                shadow_elevation: 6.0,
                ..Default::default()
            })
            .graphics_layer(custom)
            .graphics_layer(GraphicsLayerParams {
                translation_y: 3.0,
                ..Default::default()
            })
            .graphics_layer_params()
            .unwrap();
        assert_eq!(p.shadow_elevation, 6.0);
        assert_eq!(p.alpha, 0.5);
        assert_eq!(p.translation_x, 4.0);
        assert_eq!(p.translation_y, 3.0);
        assert_eq!(p.ambient_shadow_color, Color::RED);
        assert_eq!(p.spot_shadow_color, Color::BLUE);
    }

    #[test]
    fn alpha_creates_layer_with_clip() {
        // 对标源码：`alpha(a) = graphicsLayer(alpha=a, clip=true)`
        let m = Modifier::new().alpha(0.5);
        let p = m.graphics_layer_params().unwrap();
        assert_eq!(p.alpha, 0.5);
        assert!(p.clip, "alpha<1 必须隐式 clip（Compose 语义）");
    }

    #[test]
    fn alpha_identity_no_op() {
        let m = Modifier::new().alpha(1.0);
        assert!(m.graphics_layer_params().is_none(), "alpha=1 不应用（源码 early-return）");
        let m = Modifier::new().rotate(0.0).scale(1.0, 1.0);
        assert!(m.graphics_layer_params().is_none(), "rotate=0/scale=1 不应用");
    }

    #[test]
    fn alpha_merges_into_existing_layer() {
        // 已存在 GraphicsLayer → 合并（alpha 相乘 + clip）而非嵌套层
        let m = Modifier::new()
            .graphics_layer(GraphicsLayerParams { alpha: 0.8, ..Default::default() })
            .alpha(0.5);
        let p = m.graphics_layer_params().unwrap();
        assert!((p.alpha - 0.4).abs() < 1e-6, "alpha 相乘（0.8*0.5）");
        assert!(p.clip);
    }

    #[test]
    fn rotate_scale_merge_with_origin() {
        let m = Modifier::new().rotate(45.0).scale(2.0, 3.0);
        let p = m.graphics_layer_params().unwrap();
        assert_eq!(p.rotation_z, 45.0);
        assert_eq!(p.scale_x, 2.0);
        assert_eq!(p.scale_y, 3.0);
        // 便捷包装不动 transform_origin（默认 Center——绕中心，Compose 语义）
        assert_eq!(p.transform_origin, TransformOrigin::CENTER);
    }

    #[test]
    fn chain_alpha_rotate_scale_merge() {
        let m = Modifier::new().alpha(0.5).rotate(90.0).scale(2.0, 2.0);
        let p = m.graphics_layer_params().unwrap();
        assert_eq!(p.alpha, 0.5);
        assert_eq!(p.rotation_z, 90.0);
        assert_eq!(p.scale_x, 2.0);
        assert!(p.clip);
    }

    // ── shadow / drop_shadow ──

    #[test]
    fn shadow_goes_to_the_graphics_layer() {
        // Compose's `Modifier.shadow` is a graphicsLayer: the elevation reaches Skia as a Z height,
        // and the one colour serves as both the ambient and the spot colour.
        let ink = Color::from_argb(40, 0, 0, 0);
        let m = Modifier::new().shadow(8.0, Shape::rounded(4.0), true, ink);
        let p = m.graphics_layer_params().expect("shadow() adds a graphics layer");
        assert_eq!(p.shadow_elevation, 8.0);
        assert_eq!(p.shadow_shape, Some(Shape::rounded(4.0)));
        assert!(p.clip);
        assert_eq!(p.ambient_shadow_color, ink);
        assert_eq!(p.spot_shadow_color, ink);
        assert!(
            m.elements().iter().all(|el| !matches!(el, ModifierElement::Shadow { .. })),
            "nothing is expanded into an ambient/spot pair any more"
        );
    }

    #[test]
    fn shadow_with_nothing_to_draw_returns_self() {
        // Compose's early return: no elevation and no clip means no layer at all.
        let m = Modifier::new().shadow(0.0, Shape::Rectangle, false, Color::from_argb(40, 0, 0, 0));
        assert!(m.elements().is_empty(), "elevation 0 without clip adds nothing");
    }

    #[test]
    fn drop_shadow_custom_params() {
        let m = Modifier::new().drop_shadow(
            Shape::rounded(8.0),
            ShadowParams { radius: 6.0, spread: 2.0, offset_x: 3.0, offset_y: 5.0, color: Color::from_argb(255, 50, 50, 50), alpha: 0.6 },
        );
        let p = m.elements().iter().find_map(|el| {
            if let ModifierElement::Shadow { params, .. } = el { Some(params.eval()) } else { None }
        }).unwrap();
        assert_eq!(p.radius, 6.0);
        assert_eq!(p.spread, 2.0);
        assert_eq!(p.offset_x, 3.0);
        assert_eq!(p.offset_y, 5.0);
        assert_eq!(p.alpha, 0.6);
    }

    #[test]
    fn multiple_drop_shadows_stack() {
        let m = Modifier::new()
            .drop_shadow(Shape::Rectangle, ShadowParams::new(2.0, 0.0, 1.0, Color::BLACK, 0.3))
            .drop_shadow(Shape::Rectangle, ShadowParams::new(4.0, 0.0, 2.0, Color::BLACK, 0.2));
        let n = m.elements().iter().filter(|el| matches!(el, ModifierElement::Shadow { .. })).count();
        assert_eq!(n, 2, "多个 drop_shadow 叠加（Compose vararg 语义）");
    }

    /// on_size_changed（对标 Compose onSizeChanged）：测量后上报逻辑尺寸；
    /// 尺寸未变化不重复回调，约束变化引发新尺寸时再次回调
    #[test]
    fn on_size_changed_reports_and_dedups() {
        use crate::runtime::composer::Composer;
        let reported: std::sync::Arc<std::sync::Mutex<Vec<(f32, f32)>>> = Default::default();
        let mut composer = Composer::new();
        {
            let rep = reported.clone();
            composer.compose(|ctx| {
                crate::layout::components::Column::new()
                    .modifier(Modifier::new().fill_max_width().on_size_changed(move |w, h| {
                        rep.lock().unwrap().push((w, h));
                    }))
                    .build(ctx, |ctx| {
                        crate::components::Text::new("hello").build(ctx);
                    });
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        }
        {
            let reps = reported.lock().unwrap();
            assert_eq!(reps.len(), 1, "测量完成后应上报一次");
            assert_eq!(reps[0].0, 400.0, "fill_max_width 宽度应等于约束 max");
        }
        // 同约束 re-layout：尺寸未变（clean-skip 折叠）——不得重复回调
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        assert_eq!(reported.lock().unwrap().len(), 1, "尺寸未变化不应重复回调");
        // 约束变化 → 宽度变化 → 再次回调
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 400.0));
        let reps = reported.lock().unwrap();
        assert_eq!(reps.len(), 2, "宽度变化应再次回调");
        assert_eq!(reps[1].0, 300.0, "第二次上报应反映新宽度");
    }

    /// A span-only edit has to re-enter: the resolved spans are the parameter, not a cache of it.
    ///
    /// Before this they were compared as "same" on purpose, so flipping `.bold(..)` between frames
    /// produced an equal payload and the leaf kept its old paragraph (measured: identical width and
    /// fonts across frames).
    #[test]
    fn param_eq_compares_the_rich_text_spans() {
        use crate::text::{FontSlant, FontWeight, RichSpanStyle};
        use crate::graphics::Color;
        let span = |weight: FontWeight| RichSpanStyle {
            start: 0,
            end: 3,
            font_size: 16.0,
            color: Color::BLACK,
            font_weight: weight,
            font_style: FontSlant::Upright,
            underline: false,
            overline: false,
            strikethrough: false,
            decoration_color: None,
            decoration_style: None,
            decoration_mode: None,
            baseline_shift: 0.0,
            letter_spacing: 0.0,
            word_spacing: 0.0,
            height_multiple: 0.0,
            half_leading: false,
            font_families: Vec::new(),
            font_width: 5,
            font_edging: None,
            font_hinting: None,
            subpixel: false,
            foreground_color: None,
            background: None,
            locale: None,
        };
        let rich = |spans: Vec<RichSpanStyle>| {
            Modifier::new().push(ModifierElement::RichTextContent {
                content: "abc".to_string(),
                drawables: Vec::new(),
                drawable_ranges: Vec::new(),
                spans,
                align: crate::text::TextAlign::Left,
                max_lines: usize::MAX,
                overflow: crate::text::TextOverflow::Clip,
                soft_wrap: true,
                line_height: None,
                min_lines: 1,
                line_height_style: None,
            })
        };
        let normal = rich(vec![span(FontWeight::NORMAL)]);
        let same = rich(vec![span(FontWeight::NORMAL)]);
        let bold = rich(vec![span(FontWeight::BOLD)]);
        assert!(normal.param_eq(&same), "the same span list is the same parameter");
        assert!(
            !normal.param_eq(&bold),
            "a weight change inside a span must re-enter the leaf"
        );
    }
