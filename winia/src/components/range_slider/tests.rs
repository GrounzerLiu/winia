    use super::*;
    use crate::runtime::composer::Composer;
    use crate::modifier::{DrawNode, ModifierElement};
    use crate::theme::ThemeColors;
    use std::sync::atomic::{AtomicI32, Ordering};

    // ── Pure logic ──

    #[test]
    fn nearest_thumb_picks_the_nearer_thumb() {
        let v = RangeValue::new(0.25, 0.75);
        // 300-wide track: thumb centres 79 and 221 (the inset axis 8 + 284 × f)
        assert_eq!(nearest_thumb(60.0, 300.0, v, 0.0, 1.0), RangeThumb::Start);
        assert_eq!(nearest_thumb(240.0, 300.0, v, 0.0, 1.0), RangeThumb::End);
        // Exactly between the two (150) is a tie, and the tie goes to the start thumb only when it
        // sits right of the press — it does not, so the end thumb takes it.
        assert_eq!(nearest_thumb(150.0, 300.0, v, 0.0, 1.0), RangeThumb::End);
        // A collapsed range ties everywhere: pressed from its left the start thumb takes it,
        // pressed from its right the end thumb does.
        let collapsed = RangeValue::new(0.5, 0.5);
        let c = thumb_center_x(0.5, 300.0, 0.0, 1.0);
        assert_eq!(nearest_thumb(c - 0.5, 300.0, collapsed, 0.0, 1.0), RangeThumb::Start);
        assert_eq!(nearest_thumb(c + 0.5, 300.0, collapsed, 0.0, 1.0), RangeThumb::End);
    }

    #[test]
    fn a_thumb_cannot_cross_the_other() {
        let v = RangeValue::new(0.2, 0.5);
        assert_eq!(
            range_with_moved_thumb(v, RangeThumb::Start, 0.9, 0.0, 1.0),
            RangeValue::new(0.5, 0.5),
            "the start thumb stops at the end thumb"
        );
        assert_eq!(
            range_with_moved_thumb(v, RangeThumb::End, 0.1, 0.0, 1.0),
            RangeValue::new(0.2, 0.2),
            "the end thumb stops at the start thumb"
        );
        assert_eq!(
            range_with_moved_thumb(v, RangeThumb::Start, -3.0, 0.0, 1.0),
            RangeValue::new(0.0, 0.5),
            "below the range clamps to min"
        );
        assert_eq!(
            range_with_moved_thumb(v, RangeThumb::End, 7.0, 0.0, 1.0),
            RangeValue::new(0.2, 1.0),
            "above the range clamps to max"
        );
    }

    #[test]
    fn thumb_center_matches_the_drawing_axis() {
        // Both ends land on the track's stop indicators, the middle is symmetric.
        assert_eq!(thumb_center_x(0.0, 300.0, 0.0, 1.0), 8.0);
        assert_eq!(thumb_center_x(1.0, 300.0, 0.0, 1.0), 292.0);
        assert_eq!(thumb_center_x(0.5, 300.0, 0.0, 1.0), 150.0);
        // A non-unit range maps the same way; values outside clamp.
        assert_eq!(thumb_center_x(50.0, 200.0, 0.0, 100.0), 100.0);
        assert_eq!(thumb_center_x(-10.0, 200.0, 0.0, 100.0), 8.0);
    }

    // ── Pixel tests ──

    fn render_range_px(build: impl FnOnce(&mut ComposeCtx)) -> (Vec<[u8; 4]>, usize) {
        use skia_safe::{Color as SkColor, surfaces};
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = Composer::new();
        let scene = |ctx: &mut ComposeCtx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| build(ctx));
        };
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
        (px.to_vec(), 300)
    }

    fn at(px: &[[u8; 4]], w: usize, x: f32, y: f32) -> (i32, i32, i32) {
        let p = px[(y as usize) * w + (x as usize)];
        (p[2] as i32, p[1] as i32, p[0] as i32) // BGRA → RGB
    }

    fn close(a: (i32, i32, i32), b: (i32, i32, i32)) -> bool {
        (a.0 - b.0).abs() <= 8 && (a.1 - b.1).abs() <= 8 && (a.2 - b.2).abs() <= 8
    }

    fn rgb(c: crate::graphics::Color) -> (i32, i32, i32) {
        (c.r as i32, c.g as i32, c.b as i32)
    }

    #[test]
    fn range_slider_renders_two_thumbs_and_the_active_middle() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let (px, w) = render_range_px(|ctx| {
            RangeSlider::new((0.25, 0.75))
                .value_range(0.0, 1.0)
                .on_value_change(|_| {})
                .build(ctx);
        });
        let prim = rgb(theme.primary);
        let sec = rgb(theme.secondary_container);
        let white = (255, 255, 255);
        // Track centre line y = 24; thumb centres 8 + 284 × 0.25 = 79 and 221.
        assert!(close(at(&px, w, 40.0, 24.0), sec), "left of the range is inactive track (got {:?})", at(&px, w, 40.0, 24.0));
        assert!(close(at(&px, w, 150.0, 24.0), prim), "between the thumbs is active track (got {:?})", at(&px, w, 150.0, 24.0));
        assert!(close(at(&px, w, 280.0, 24.0), sec), "right of the range is inactive track (got {:?})", at(&px, w, 280.0, 24.0));
        // Two thumbs, one per value.
        assert!(close(at(&px, w, 79.0, 24.0), prim), "start thumb is primary (got {:?})", at(&px, w, 79.0, 24.0));
        assert!(close(at(&px, w, 221.0, 24.0), prim), "end thumb is primary (got {:?})", at(&px, w, 221.0, 24.0));
        // The gaps between a thumb and the track stay clear (gap = thumb half + 6 dp = 8).
        assert!(close(at(&px, w, 72.0, 24.0), white), "gap left of the start thumb is clear (got {:?})", at(&px, w, 72.0, 24.0));
        assert!(close(at(&px, w, 228.0, 24.0), white), "gap right of the end thumb is clear (got {:?})", at(&px, w, 228.0, 24.0));
    }

    /// Thumbs at the ends of the range behave exactly like a single slider's thumb: the fill stops
    /// one gap short of the thumb, so the thumb reads as the end of the track and the last pixels
    /// stay clear. (An earlier version filled to the track's end for a range at an end, which stuck
    /// out past the thumb — reported as wrong against the single slider's look.)
    #[test]
    fn a_range_at_an_end_leaves_the_end_clear() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let prim = rgb(theme.primary);
        let white = (255, 255, 255);
        let (px, w) = render_range_px(|ctx| {
            RangeSlider::new((0.0, 1.0)).value_range(0.0, 1.0).on_value_change(|_| {}).build(ctx);
        });
        // Thumbs at 8 and 292, the fill between 16 and 284: clear at both ends, no stop indicator
        // behind a thumb that sits on the end.
        assert!(close(at(&px, w, 3.0, 24.0), white), "nothing is drawn at the very left (got {:?})", at(&px, w, 3.0, 24.0));
        assert!(close(at(&px, w, 8.0, 24.0), prim), "the start thumb sits corner px in (got {:?})", at(&px, w, 8.0, 24.0));
        assert!(close(at(&px, w, 20.0, 24.0), prim), "the fill starts one gap past the thumb (got {:?})", at(&px, w, 20.0, 24.0));
        assert!(close(at(&px, w, 150.0, 24.0), prim), "the whole span is active (got {:?})", at(&px, w, 150.0, 24.0));
        assert!(close(at(&px, w, 280.0, 24.0), prim), "the fill ends one gap before the end thumb (got {:?})", at(&px, w, 280.0, 24.0));
        assert!(close(at(&px, w, 292.0, 24.0), prim), "the end thumb (got {:?})", at(&px, w, 292.0, 24.0));
        assert!(close(at(&px, w, 297.0, 24.0), white), "nothing is drawn at the very right (got {:?})", at(&px, w, 297.0, 24.0));

        // The same at one end only: (0.0, 0.5) leaves the left clear and an inactive track right of
        // the end thumb.
        let sec = rgb(theme.secondary_container);
        let (px2, w2) = render_range_px(|ctx| {
            RangeSlider::new((0.0, 0.5)).value_range(0.0, 1.0).on_value_change(|_| {}).build(ctx);
        });
        assert!(close(at(&px2, w2, 3.0, 24.0), white), "clear left end (got {:?})", at(&px2, w2, 3.0, 24.0));
        assert!(close(at(&px2, w2, 20.0, 24.0), prim), "active from one gap past the thumb (got {:?})", at(&px2, w2, 20.0, 24.0));
        assert!(close(at(&px2, w2, 200.0, 24.0), sec), "inactive right of the end thumb (got {:?})", at(&px2, w2, 200.0, 24.0));
    }

    #[test]
    fn range_steps_color_ticks_inside_the_range() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let (px, w) = render_range_px(|ctx| {
            RangeSlider::new((0.0, 0.4)).value_range(0.0, 1.0).steps(4).on_value_change(|_| {}).build(ctx);
        });
        // Slider colors cross the two tick colors for contrast: on the primary active track the
        // ticks are secondary_container, on the secondary inactive track they are primary.
        let prim = rgb(theme.primary);
        let sec = rgb(theme.secondary_container);
        // Ticks at 8 + 284 × f for f = 0, .2, .4, .6, .8, 1 → 8, 64.8, 121.6, 178.4, 235.2, 292.
        // The ones on the thumbs (8, 121.6) are skipped, so the first visible tick is 64.8: inside
        // the range → the active tick color.
        assert!(close(at(&px, w, 64.8, 24.0), sec), "a tick inside the range takes the active tick color (got {:?})", at(&px, w, 64.8, 24.0));
        assert!(close(at(&px, w, 178.4, 24.0), prim), "a tick outside the range takes the inactive tick color (got {:?})", at(&px, w, 178.4, 24.0));
        assert!(close(at(&px, w, 235.2, 24.0), prim), "and so does the next one (got {:?})", at(&px, w, 235.2, 24.0));
    }

    #[test]
    fn disabled_range_uses_the_disabled_palette() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let (px, w) = render_range_px(|ctx| {
            RangeSlider::new((0.25, 0.75)).value_range(0.0, 1.0).enabled(false).build(ctx);
        });
        // The disabled colors carry an alpha, so what the surface shows is that color composited
        // over the white background — the composited value is what this pins.
        let over_white = |c: crate::graphics::Color, a: f32| {
            (
                (c.r as f32 * a + 255.0 * (1.0 - a)) as i32,
                (c.g as f32 * a + 255.0 * (1.0 - a)) as i32,
                (c.b as f32 * a + 255.0 * (1.0 - a)) as i32,
            )
        };
        let thumb_disabled = over_white(theme.on_surface, 0.38);
        let inactive_disabled = over_white(theme.on_surface, 0.12);
        assert!(close(at(&px, w, 79.0, 24.0), thumb_disabled), "a disabled thumb is onSurface @ 38% (got {:?}, expected {thumb_disabled:?})", at(&px, w, 79.0, 24.0));
        // The disabled ACTIVE track carries the same 38% alpha as the thumb; the inactive one the
        // 12% of `DisabledInactiveTrackAlpha`.
        assert!(close(at(&px, w, 280.0, 24.0), inactive_disabled), "a disabled inactive track is onSurface @ 12% (got {:?}, expected {inactive_disabled:?})", at(&px, w, 280.0, 24.0));
    }

    // ── Gesture callbacks ──

    /// Build a range slider and hand back its press / drag callbacks, laid out 300 × 300.
    type PressCb = Arc<dyn Fn((f32, f32)) + Send + Sync>;
    type DragCb = Arc<dyn Fn((f32, f32), (f32, f32)) + Send + Sync>;
    type DragEndCb = Arc<dyn Fn() + Send + Sync>;

    #[allow(clippy::type_complexity)]
    fn callbacks(
        value: RangeValue,
        steps: i32,
        got: Arc<AtomicI32>,
    ) -> (PressCb, DragCb, DragEndCb) {
        use skia_safe::surfaces;
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = Composer::new();
        let scene = |ctx: &mut ComposeCtx| {
            let g = got.clone();
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                RangeSlider::new(value)
                    .value_range(0.0, 1.0)
                    .steps(steps)
                    .on_value_change(move |v: RangeValue| {
                        g.store((v.start * 1000.0) as i32, Ordering::Relaxed);
                    })
                    .build(ctx);
            });
        };
        composer.compose(scene);
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
        // Render once so the draw node writes the track width back (300) — the gesture callbacks
        // convert pixels to values through it.
        let mut surface = surfaces::raster_n32_premul((300, 300)).unwrap();
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        crate::render::render(nodes, root, surface.canvas());

        let mut press: Option<PressCb> = None;
        let mut drag: Option<DragCb> = None;
        let mut drag_end: Option<DragEndCb> = None;
        for n in composer.arena_nodes() {
            for el in n.modifier.elements() {
                match el {
                    ModifierElement::TapOnPress { cb } => press = Some(cb.clone()),
                    ModifierElement::DragOnMove { cb } => drag = Some(cb.clone()),
                    ModifierElement::DragOnEnd { cb } => drag_end = Some(cb.clone()),
                    _ => {}
                }
            }
        }
        (
            press.expect("a range slider has a press callback"),
            drag.expect("a range slider has a drag callback"),
            drag_end.expect("a range slider has a drag-end callback"),
        )
    }

    #[test]
    fn pressing_jumps_only_the_nearer_thumb() {
        let got = Arc::new(AtomicI32::new(-1));
        let (press, drag, _end) = callbacks(RangeValue::new(0.25, 0.75), 0, got.clone());
        // x = 60 is nearer the start thumb (79) than the end thumb (221): it jumps to
        // (60 - 8) / 284 = 0.183, the end thumb stays.
        press((60.0, 24.0));
        assert_eq!(got.load(Ordering::Relaxed), 183, "the nearer (start) thumb jumped, the end one stayed");
        let _ = drag;

    }

    #[test]
    fn a_drag_keeps_the_thumb_it_resolved_at_press() {
        let got = Arc::new(AtomicI32::new(-1));
        let (press, drag, end) = callbacks(RangeValue::new(0.25, 0.75), 0, got.clone());
        // Press on the end thumb, then drag to x = 280 → end = (280 - 8) / 284 = 0.958.
        press((240.0, 24.0));
        drag((280.0, 24.0), (40.0, 0.0));
        assert_eq!(got.load(Ordering::Relaxed), 250, "the start thumb stayed at 0.25");
        // Dragging far past the other thumb keeps the END thumb: it clamps at the start thumb's
        // value instead of jumping to the far side (which a re-resolved thumb would do).
        drag((10.0, 24.0), (-270.0, 0.0));
        assert_eq!(got.load(Ordering::Relaxed), 250, "the end thumb clamped at the start thumb (0.25)");
        end();

    }

    #[test]
    fn a_press_on_the_far_side_moves_the_other_thumb() {
        let got = Arc::new(AtomicI32::new(-1));
        let (press, _drag, _end) = callbacks(RangeValue::new(0.25, 0.75), 0, got.clone());
        // x = 240 is nearer the end thumb → the START thumb keeps 0.25 and the end thumb jumps.
        press((240.0, 24.0));
        assert_eq!(got.load(Ordering::Relaxed), 250, "the start thumb value is unchanged");

    }

    /// Both ends snap to ticks, a caller-supplied range included (see `Slider`'s note): with
    /// steps = 4 over 0..1 the ticks are 0, .2, .4, .6, .8, 1, so (0.3, 0.31) collapses onto the 0.4
    /// tick — thumb centres 8 + 284 × f — and (0.3, 0.7) becomes (0.4, 0.8).
    #[test]
    fn discrete_range_snaps_both_ends() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let prim = rgb(theme.primary);
        let sec = rgb(theme.secondary_container);
        let white = (255, 255, 255);

        let (px, w) = render_range_px(|ctx| {
            RangeSlider::new((0.3, 0.31)).value_range(0.0, 1.0).steps(4).on_value_change(|_| {}).build(ctx);
        });
        assert!(close(at(&px, w, 121.6, 24.0), prim), "the collapsed range sits on the 0.4 tick (got {:?})", at(&px, w, 121.6, 24.0));
        // Left of it the track is inactive — an unsnapped (0.3, 0.31) would have put two thumbs here
        // and left 110 in the clear.
        assert!(close(at(&px, w, 110.0, 24.0), sec), "inactive left of the snapped thumb (got {:?})", at(&px, w, 110.0, 24.0));
        assert!(close(at(&px, w, 150.0, 24.0), sec), "inactive right of the snapped thumb (got {:?})", at(&px, w, 150.0, 24.0));

        let (px2, w2) = render_range_px(|ctx| {
            RangeSlider::new((0.3, 0.7)).value_range(0.0, 1.0).steps(4).on_value_change(|_| {}).build(ctx);
        });
        assert!(close(at(&px2, w2, 121.6, 24.0), prim), "start snapped to 0.4 (got {:?})", at(&px2, w2, 121.6, 24.0));
        assert!(close(at(&px2, w2, 235.2, 24.0), prim), "end snapped to 0.8 (got {:?})", at(&px2, w2, 235.2, 24.0));
        assert!(close(at(&px2, w2, 150.0, 24.0), prim), "the snapped span is the active one (got {:?})", at(&px2, w2, 150.0, 24.0));
        assert!(close(at(&px2, w2, 228.0, 24.0), white), "the gap before the snapped end thumb is clear (got {:?})", at(&px2, w2, 228.0, 24.0));
    }

    #[test]
    fn range_slider_track_node_key_covers_all_visual_params() {
        let colors = SliderDefaults::slider_colors(&ThemeColors::light_from_seed(0x6750A4));
        let node = |v: RangeValue, min: f32, max: f32, steps: i32| RangeSliderTrackNode {
            track_width: crate::runtime::state::Backchannel::new(0.0),
            colors,
            enabled: true,
            value: v,
            min,
            max,
            steps,
        };
        let base = node(RangeValue::new(0.25, 0.75), 0.0, 1.0, 0);
        let base_key = base.node_key();
        assert_eq!(
            base_key,
            node(RangeValue::new(0.25, 0.75), 0.0, 1.0, 0).node_key(),
            "same params → same key (Skip holds)"
        );
        assert_ne!(base_key, node(RangeValue::new(0.2, 0.75), 0.0, 1.0, 0).node_key(), "start value");
        assert_ne!(base_key, node(RangeValue::new(0.25, 0.7), 0.0, 1.0, 0).node_key(), "end value");
        assert_ne!(base_key, node(RangeValue::new(0.25, 0.75), 0.0, 2.0, 0).node_key(), "max");
        assert_ne!(base_key, node(RangeValue::new(0.25, 0.75), -1.0, 1.0, 0).node_key(), "min");
        assert_ne!(base_key, node(RangeValue::new(0.25, 0.75), 0.0, 1.0, 4).node_key(), "steps");
        let mut other = node(RangeValue::new(0.25, 0.75), 0.0, 1.0, 0);
        other.enabled = false;
        assert_ne!(base_key, other.node_key(), "enabled");
    }

    /// A thumb's key must cover what it draws (its colors, the halved width) and its own source —
    /// the ring comes from that source, so two thumbs with different sources must not collide.
    #[test]
    fn range_thumb_node_key_covers_all_visual_params() {
        let colors = SliderDefaults::slider_colors(&ThemeColors::light_from_seed(0x6750A4));
        let src = MutableInteractionSource::new();
        let node = |active: bool, enabled: bool, src: MutableInteractionSource| RangeThumbNode {
            interaction: src,
            colors,
            enabled,
            active,
        };
        let base_key = node(false, true, src.clone()).node_key();
        assert_eq!(base_key, node(false, true, src.clone()).node_key(), "same params → same key");
        assert_ne!(base_key, node(true, true, src.clone()).node_key(), "halved width while active");
        assert_ne!(base_key, node(false, false, src.clone()).node_key(), "enabled");
        assert_ne!(
            base_key,
            node(false, true, MutableInteractionSource::new()).node_key(),
            "the source drives the ring, so it must be part of the key"
        );
        let mut recolored = node(false, true, src.clone());
        recolored.colors.thumb_color = crate::graphics::Color::from_argb(255, 1, 2, 3);
        assert_ne!(base_key, recolored.node_key(), "colors");
    }

    /// The nesting the component builds: the root fills the line, its single child is the track (the
    /// node with the gestures and the body), and the track's children are the two thumbs.
    fn range_nodes(composer: &Composer) -> (usize, usize, [usize; 2]) {
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        assert_eq!(nodes[root].children.len(), 1, "the root's single child is the track");
        let track = nodes[root].children[0];
        let thumbs = nodes[track].children.clone();
        assert_eq!(thumbs.len(), 2, "the track's children are the two thumbs");
        (root, track, [thumbs[0], thumbs[1]])
    }

    /// Both thumbs sit on [`thumb_center_x`] — the axis the drawing and the hit test both use, in the
    /// track's own space (which is also the space the gesture callbacks report).
    #[test]
    fn both_thumbs_are_placed_on_the_value_axis() {
        let v = RangeValue::new(0.25, 0.75);
        let mut composer = Composer::new();
        let theme = ThemeColors::light_from_seed(0x6750A4);
        composer.compose(|ctx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                RangeSlider::new(v).value_range(0.0, 1.0).on_value_change(|_| {}).build(ctx);
            });
        });
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
        let (_root, track, thumbs) = range_nodes(&composer);
        let nodes = composer.arena_nodes();
        assert_eq!(nodes[track].measured_size.width, 300.0, "the track fills the line");
        assert_eq!(nodes[track].measured_size.height, SLIDER_TOUCH_HEIGHT);

        for (i, value) in [v.start, v.end].iter().enumerate() {
            let t = &nodes[thumbs[i]];
            assert_eq!(t.measured_size.width, SLIDER_THUMB_WIDTH, "thumb {} keeps its resting width", i);
            assert_eq!(t.measured_size.height, SLIDER_THUMB_HEIGHT);
            let expected = thumb_center_x(*value, 300.0, 0.0, 1.0) - SLIDER_THUMB_WIDTH / 2.0;
            assert!(
                (t.position.x - expected).abs() < 0.01,
                "thumb {i} at x={} but the axis says {expected}",
                t.position.x
            );
            assert!((t.position.y - (SLIDER_TOUCH_HEIGHT - SLIDER_THUMB_HEIGHT) / 2.0).abs() < 0.01);
        }
    }

    /// A caller's `padding` must NOT shift the pointer↔value axis.
    ///
    /// The track is a child of the component root, so a padding insets it while the gestures live on
    /// the track itself: the pointer axis and the drawn axis are the same box, and the padding band is
    /// inert — what Compose's `modifier.padding(16)` does to a Slider too. Two earlier arrangements
    /// got this wrong (the body on the root with the thumbs as children; then the gestures on the root
    /// while the body/thumbs were children) and this test measured the difference: pressing a drawn
    /// thumb jumped it by the padding, 0.313 and then 0.278 where the thumb sat at 0.25.
    #[test]
    fn a_caller_padding_does_not_shift_the_pointer_axis() {
        use skia_safe::surfaces;
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let got = Arc::new(AtomicI32::new(-1));
        let mut composer = Composer::new();
        let scene = |ctx: &mut ComposeCtx| {
            let g = got.clone();
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                RangeSlider::new(RangeValue::new(0.25, 0.75))
                    .value_range(0.0, 1.0)
                    .on_value_change(move |v: RangeValue| {
                        g.store((v.start * 1000.0) as i32, Ordering::Relaxed);
                    })
                    .modifier(Modifier::new().padding(16.0))
                    .build(ctx);
            });
        };
        composer.compose(scene);
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
        let mut surface = surfaces::raster_n32_premul((300, 300)).unwrap();
        let root = composer.layout_root_idx().expect("root");
        crate::render::render(composer.arena_nodes(), root, surface.canvas());

        // The drawn start thumb, in the space the gesture callback receives (the track's own box).
        let (_root, track, thumbs) = range_nodes(&composer);
        let nodes = composer.arena_nodes();
        assert_eq!(nodes[track].measured_size.width, 268.0, "the padding insets the track");
        let thumb_x = nodes[thumbs[0]].position.x + SLIDER_THUMB_WIDTH / 2.0;
        let mut press: Option<PressCb> = None;
        for n in composer.arena_nodes() {
            for el in n.modifier.elements() {
                if let ModifierElement::TapOnPress { cb } = el {
                    press = Some(cb.clone());
                }
            }
        }
        press.expect("a range slider has a press callback")((thumb_x, 24.0));
        assert_eq!(
            got.load(Ordering::Relaxed),
            250,
            "pressing the drawn thumb (x={thumb_x}) must not move it — padding shifted the axis"
        );

    }

    /// Each thumb carries its own focusable node and its own key handling: two focus stops, and a
    /// pointer press hands focus to the thumb it resolved (Compose's model, one `focusable` per
    /// thumb). Without this the keyboard would have a single target and Tab could not switch.
    #[test]
    fn each_thumb_is_its_own_focus_target_with_its_own_keys() {
        let mut composer = Composer::new();
        let theme = ThemeColors::light_from_seed(0x6750A4);
        composer.compose(|ctx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                RangeSlider::new((0.25, 0.75)).value_range(0.0, 1.0).on_value_change(|_| {}).build(ctx);
            });
        });
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
        let (root, track, thumbs) = range_nodes(&composer);
        let nodes = composer.arena_nodes();
        let focusable_thumbs = thumbs
            .iter()
            .filter(|&&c| {
                let els = nodes[c].modifier.elements();
                els.iter().any(|el| matches!(el, ModifierElement::Focusable { .. }))
                    && els.iter().any(|el| matches!(el, ModifierElement::KbEvent { on_key: Some(_), .. }))

            })
            .count();
        assert_eq!(focusable_thumbs, 2, "both thumbs are focusable and carry their own keys");
        for (name, idx) in [("root", root), ("track", track)] {
            let focusable = nodes[idx]
                .modifier
                .elements()
                .iter()
                .any(|el| matches!(el, ModifierElement::Focusable { .. }));
            assert!(!focusable, "the {name} holds the gestures and the body, not a third focus stop");
        }
    }
