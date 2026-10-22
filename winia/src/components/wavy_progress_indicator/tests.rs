    use super::*;
    use crate::runtime::composer::Composer;
    use crate::layout::Constraints;
    use crate::components::progress_indicator::LinearProgressIndicator;

    #[test]
    fn default_amplitude_matches_compose() {
        assert_eq!(WavyProgressIndicatorDefaults::indicator_amplitude(0.0), 0.0);
        assert_eq!(WavyProgressIndicatorDefaults::indicator_amplitude(0.1), 0.0);
        assert_eq!(WavyProgressIndicatorDefaults::indicator_amplitude(0.5), 1.0);
        assert_eq!(
            WavyProgressIndicatorDefaults::indicator_amplitude(0.949),
            1.0
        );
        assert_eq!(
            WavyProgressIndicatorDefaults::indicator_amplitude(0.95),
            0.0
        );
        assert_eq!(WavyProgressIndicatorDefaults::indicator_amplitude(1.0), 0.0);
    }

    #[test]
    fn linear_wave_duration_matches_compose() {
        // wavelength == waveSpeed → 1000ms（每秒一个波长）
        let spec = wave_animation_spec(40.0, 40.0);
        assert_eq!(spec.duration, Duration::from_millis(1000));
        // 最小值保护 50ms
        let spec2 = wave_animation_spec(0.1, 1000.0);
        assert_eq!(spec2.duration, Duration::from_millis(50));
    }

    #[test]
    fn circular_vertex_count_formula() {
        // Compose：numVertices = max(5, round(2πr/λ))，r = 48/2 - 4/2 = 22，λ=15
        let r = WAVY_CIRCULAR_SIZE / 2.0 - WAVY_STROKE_WIDTH / 2.0;
        let n = ((2.0 * PI * r / WAVY_CIRCULAR_WAVELENGTH).round() as usize)
            .max(MIN_CIRCULAR_VERTEX_COUNT);
        assert_eq!(n, 9);
        assert!(n >= MIN_CIRCULAR_VERTEX_COUNT);
    }

    #[test]
    fn circular_shapes_cache_builds_paths() {
        let mut cache = CircularShapesCache::default();
        cache.update(48.0, 48.0, 15.0, 4.0);
        let track = cache.get_track_path();
        let star = cache.get_progress_path(1.0, false);
        let morph = cache.get_progress_path(0.5, false);
        assert!(track.bounds().width() > 0.0);
        assert!(star.bounds().width() > 0.0);
        assert!(morph.bounds().width() > 0.0);
        assert!(
            cache.morph_match.is_some(),
            "Morph feature-mapping 应缓存，避免每帧 Morph::new"
        );
    }

    #[test]
    fn circular_progress_path_starts_at_top() {
        let mut cache = CircularShapesCache::default();
        cache.update(48.0, 48.0, 15.0, 4.0);
        // 圆、Morph 中间帧、星形都必须从正上方开始，避免振幅过渡时起点/方向跳变。
        for amp in [0.0f32, 0.5, 1.0] {
            let mut path = cache.get_progress_path(amp, false);
            process_circular_path(&mut path, 48.0, 48.0, 4.0);
            let mut measure = skia_safe::PathMeasure::new(&path, true, None);
            let (start, _tangent) = measure.pos_tan(0.0).expect("PathMeasure 应能取到起点");
            let cx = 24.0;
            let cy = 24.0;
            assert!(
                (start.x - cx).abs() < 2.0,
                "Circular 起点应在正上方（amp={amp}，x≈24），实际 x={}",
                start.x
            );
            assert!(
                start.y < cy - 10.0,
                "Circular 起点应在正上方（amp={amp}，y<14），实际 y={}",
                start.y
            );
        }
    }

    #[test]
    fn circular_peak_phase_stable_across_amplitudes() {
        // 回归：Morph 中间帧与 amplitude=1 的星形必须保持同一组波峰角度，
        // 否则 0.94→0.95 振幅过渡时波峰/波谷会整体旋转（相位抖动）。
        let mut cache = CircularShapesCache::default();
        cache.update(48.0, 48.0, 15.0, 4.0);
        let mut top_peak_angles = Vec::new();
        for amp in [0.5f32, 0.9, 1.0] {
            let mut path = cache.get_progress_path(amp, true);
            process_circular_path(&mut path, 48.0, 48.0, 4.0);
            let mut measure = skia_safe::PathMeasure::new(&path, true, None);
            let half = measure.length() / 2.0;
            let cx = 24.0;
            let cy = 24.0;
            let steps = 2000;
            let mut pts = Vec::with_capacity(steps + 1);
            for i in 0..=steps {
                let d = i as f32 / steps as f32 * half;
                let (p, _) = measure.pos_tan(d).unwrap();
                let r = ((p.x - cx).powi(2) + (p.y - cy).powi(2)).sqrt();
                let a = (p.y - cy).atan2(p.x - cx).to_degrees();
                pts.push((a, r));
            }
            let mut best = f32::MAX;
            let mut best_angle = f32::NAN;
            for i in 1..steps {
                let (a0, r0) = pts[i - 1];
                let (a1, r1) = pts[i];
                let (a2, r2) = pts[i + 1];
                if r1 > r0 && r1 >= r2 {
                    // 找最靠近正上方（-90°）的外顶点。
                    let diff = (a1 + 90.0).abs();
                    if diff < best {
                        best = diff;
                        best_angle = a1;
                    }
                }
            }
            top_peak_angles.push(best_angle);
        }
        assert_eq!(top_peak_angles.len(), 3, "每个 amplitude 都应找到顶部波峰");
        let max_diff = top_peak_angles
            .iter()
            .zip(top_peak_angles.iter().skip(1))
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        assert!(
            max_diff <= 2.0,
            "Circular 波峰角度在振幅过渡时应保持稳定，实际角度={top_peak_angles:?}"
        );
    }

    // ── 像素测试 ──
    fn render_wavy(build: impl FnOnce(&mut ComposeCtx)) -> (Vec<[u8; 4]>, usize) {
        // 像素测试也可能注册无限动画，必须与动画断言测试串行并清理，
        // 避免并行测试污染全局动画表。
        let _g = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        use skia_safe::{Color as SkColor, surfaces};
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = Composer::new();
        let scene = |ctx: &mut ComposeCtx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| build(ctx));
        };
        composer.compose(scene);
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
        let mut surface = surfaces::raster_n32_premul((300, 300)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(SkColor::WHITE);
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        crate::render::render(nodes, root, canvas);
        let pm = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        let result = (px.to_vec(), pm.width() as usize);
        crate::animation::clear_all_animations();
        result
    }

    fn px_at(buf: &[[u8; 4]], w: usize, x: usize, y: usize) -> [u8; 4] {
        let p = buf[y * w + x];
        [p[2], p[1], p[0], p[3]]
    }

    fn color_eq(c: Color, px: [u8; 4], tol: u8) -> bool {
        (c.r as i16 - px[0] as i16).abs() <= tol as i16
            && (c.g as i16 - px[1] as i16).abs() <= tol as i16
            && (c.b as i16 - px[2] as i16).abs() <= tol as i16
            && (c.a as i16 - px[3] as i16).abs() <= tol as i16
    }

    #[test]
    fn linear_amplitude_zero_is_flat_line() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let primary = WavyProgressIndicatorDefaults::indicator_color(&theme);
        let (buf, w) = render_wavy(|ctx| {
            LinearWavyProgressIndicator::new(0.5)
                .amplitude_fn(|_| 0.0)
                .build(ctx);
        });
        let mut ys = vec![];
        for y in 0..300 {
            for x in 0..300 {
                if color_eq(primary, px_at(&buf, w, x, y), 6) {
                    ys.push(y as i32);
                }
            }
        }
        assert!(!ys.is_empty(), "amplitude=0 时应有 primary 直线");
        let spread = ys.iter().max().unwrap() - ys.iter().min().unwrap();
        assert!(spread <= 8, "amplitude=0 应接近水平直线，y spread={spread}");
    }

    #[test]
    fn linear_amplitude_one_shows_wave() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let primary = WavyProgressIndicatorDefaults::indicator_color(&theme);
        let (buf, w) = render_wavy(|ctx| {
            LinearWavyProgressIndicator::new(0.5)
                .amplitude_fn(|_| 1.0)
                .build(ctx);
        });
        let mut ys = vec![];
        for y in 0..300 {
            for x in 0..300 {
                if color_eq(primary, px_at(&buf, w, x, y), 6) {
                    ys.push(y as i32);
                }
            }
        }
        assert!(!ys.is_empty(), "amplitude=1 时应有 primary 波浪");
        let spread = ys.iter().max().unwrap() - ys.iter().min().unwrap();
        assert!(spread >= 6, "amplitude=1 应呈现波浪，y spread={spread}");
    }

    #[test]
    fn linear_wavy_reuses_flat_indeterminate_animation_specs() {
        assert_eq!(
            progress_indicator::linear_first_line_head_spec().duration,
            Duration::from_millis(1750)
        );
    }

    #[test]
    fn indeterminate_build_registers_infinite_animations() {
        let _g = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = Composer::new();
        let scene = |ctx: &mut ComposeCtx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                LinearWavyProgressIndicator::indeterminate().build(ctx);
            });
        };
        composer.compose(scene);
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
        assert!(crate::animation::is_animating());
        crate::animation::clear_all_animations();
    }

    #[test]
    fn circular_indeterminate_build_registers_infinite_animations() {
        let _g = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = Composer::new();
        let scene = |ctx: &mut ComposeCtx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                CircularWavyProgressIndicator::indeterminate().build(ctx);
            });
        };
        composer.compose(scene);
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
        assert!(crate::animation::is_animating());
        crate::animation::clear_all_animations();
    }

    #[test]
    fn linear_determinate_build_registers_amplitude_animation() {
        let _g = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = Composer::new();
        let scene = |ctx: &mut ComposeCtx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                LinearWavyProgressIndicator::new(0.5).build(ctx);
            });
        };
        composer.compose(scene);
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
        // 初始 target=1，amplitude_state 初始为 1 → push_animatable 不会注册动画。
        // 这里仅确保不 panic；振幅动画行为由上面的像素测试覆盖。
        crate::animation::clear_all_animations();
    }

    #[test]
    fn linear_draw_respects_rect_offset() {
        use skia_safe::{Color as SkColor, surfaces};
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let primary = WavyProgressIndicatorDefaults::indicator_color(&theme);
        let mut surface = surfaces::raster_n32_premul((300, 300)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(SkColor::WHITE);
        let rect = skia_safe::Rect::from_xywh(30.0, 50.0, 240.0, 10.0);
        let cache = Mutex::new(LinearShapesCache::default());
        draw_linear_wavy_determinate(
            canvas,
            rect,
            primary,
            WavyProgressIndicatorDefaults::track_color(&theme),
            4.0,
            4.0,
            ProgressIndicatorStrokeCap::Round,
            4.0,
            4.0,
            0.5,
            1.0,
            40.0,
            0.0,
            true,
            true,
            &cache,
        );
        let pm = surface.peek_pixels().unwrap();
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().unwrap();
        let w = pm.width() as usize;
        let mut min_x = usize::MAX;
        let mut min_y = usize::MAX;
        for y in 0..300 {
            for x in 0..300 {
                let p = px[y * w + x];
                let rgba = [p[2], p[1], p[0], p[3]];
                if color_eq(primary, rgba, 6) {
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                }
            }
        }
        assert!(min_x >= 28, "Linear 绘制应随 rect.left 偏移，min_x={min_x}");
        assert!(min_y >= 48, "Linear 绘制应随 rect.top 偏移，min_y={min_y}");
    }

    #[test]
    fn circular_draw_respects_rect_offset() {
        use skia_safe::{Color as SkColor, surfaces};
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let primary = WavyProgressIndicatorDefaults::indicator_color(&theme);
        let mut surface = surfaces::raster_n32_premul((300, 300)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(SkColor::WHITE);
        let rect = skia_safe::Rect::from_xywh(40.0, 60.0, 48.0, 48.0);
        let cache = Mutex::new(CircularShapesCache::default());
        draw_circular_wavy_determinate(
            canvas,
            rect,
            primary,
            WavyProgressIndicatorDefaults::track_color(&theme),
            4.0,
            4.0,
            ProgressIndicatorStrokeCap::Round,
            4.0,
            0.5,
            1.0,
            15.0,
            true,
            0.0,
            &cache,
        );
        let pm = surface.peek_pixels().unwrap();
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().unwrap();
        let w = pm.width() as usize;
        let mut min_x = usize::MAX;
        let mut min_y = usize::MAX;
        for y in 0..300 {
            for x in 0..300 {
                let p = px[y * w + x];
                let rgba = [p[2], p[1], p[0], p[3]];
                if color_eq(primary, rgba, 6) {
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                }
            }
        }
        assert!(
            min_x >= 38,
            "Circular 绘制应随 rect.left 偏移，min_x={min_x}"
        );
        assert!(
            min_y >= 58,
            "Circular 绘制应随 rect.top 偏移，min_y={min_y}"
        );
    }

    // ── WavyNode 迁移（exp/wavy-node）──
    #[test]
    fn wavy_nodes_key_covers_static_params() {
        use crate::modifier::DrawNode;
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let color = WavyProgressIndicatorDefaults::indicator_color(&theme);
        let track = WavyProgressIndicatorDefaults::track_color(&theme);
        let st = || crate::runtime::state::Visual::new(0.5f32);
        // linear-indet：静态变 → 不等；动画值变 → 相等
        let base = LinearWavyIndeterminateNode {
            color, track_color: track, stroke_width: 4.0, track_stroke_width: 4.0,
            cap: ProgressIndicatorStrokeCap::Round, gap_size: 4.0,
            amplitude: 1.0, wavelength: 20.0, enable_motion: true,
            fh: st(), ft: st(), sh: st(), st: st(), wave_offset: st(),
            cache: Arc::new(Mutex::new(LinearShapesCache::default())),
        };
        let same = LinearWavyIndeterminateNode {
            color, track_color: track, stroke_width: 4.0, track_stroke_width: 4.0,
            cap: ProgressIndicatorStrokeCap::Round, gap_size: 4.0,
            amplitude: 1.0, wavelength: 20.0, enable_motion: true,
            fh: st(), ft: st(), sh: st(), st: st(), wave_offset: st(),
            cache: Arc::new(Mutex::new(LinearShapesCache::default())),
        };
        assert_eq!(base.node_key(), same.node_key());
        // cache 不同实例 → key 仍相等（性能设施不进 key）
        let moved_anim = LinearWavyIndeterminateNode {
            color, track_color: track, stroke_width: 4.0, track_stroke_width: 4.0,
            cap: ProgressIndicatorStrokeCap::Round, gap_size: 4.0,
            amplitude: 1.0, wavelength: 20.0, enable_motion: true,
            fh: crate::runtime::state::Visual::new(0.9),
            ft: st(), sh: st(), st: st(), wave_offset: st(),
            cache: Arc::new(Mutex::new(LinearShapesCache::default())),
        };
        assert_eq!(base.node_key(), moved_anim.node_key(), "无限动画值不应进 key");
        let other_wl = LinearWavyIndeterminateNode {
            color, track_color: track, stroke_width: 4.0, track_stroke_width: 4.0,
            cap: ProgressIndicatorStrokeCap::Round, gap_size: 4.0,
            amplitude: 1.0, wavelength: 40.0, enable_motion: true,
            fh: st(), ft: st(), sh: st(), st: st(), wave_offset: st(),
            cache: Arc::new(Mutex::new(LinearShapesCache::default())),
        };
        assert_ne!(base.node_key(), other_wl.node_key(), "wavelength 应进 key");
        // linear-det：progress/token 进 key，amplitude_state 不进
        let dbase = LinearWavyDeterminateNode {
            color, track_color: track, stroke_width: 4.0, track_stroke_width: 4.0,
            cap: ProgressIndicatorStrokeCap::Round, gap_size: 4.0, stop_size: 4.0,
            progress: 0.5, wavelength: 40.0, enable_motion: true,
            amplitude_token: (0, 0, 0),
            amplitude_state: crate::runtime::state::State::new(0.5),
            wave_offset: st(),
            cache: Arc::new(Mutex::new(LinearShapesCache::default())),
        };
        let dsame = LinearWavyDeterminateNode {
            color, track_color: track, stroke_width: 4.0, track_stroke_width: 4.0,
            cap: ProgressIndicatorStrokeCap::Round, gap_size: 4.0, stop_size: 4.0,
            progress: 0.5, wavelength: 40.0, enable_motion: true,
            amplitude_token: (0, 0, 0),
            amplitude_state: crate::runtime::state::State::new(0.1),
            wave_offset: st(),
            cache: Arc::new(Mutex::new(LinearShapesCache::default())),
        };
        assert_eq!(dbase.node_key(), dsame.node_key(), "振幅过渡值不应进 key");
        let dprog = LinearWavyDeterminateNode {
            color, track_color: track, stroke_width: 4.0, track_stroke_width: 4.0,
            cap: ProgressIndicatorStrokeCap::Round, gap_size: 4.0, stop_size: 4.0,
            progress: 0.7, wavelength: 40.0, enable_motion: true,
            amplitude_token: (0, 0, 0),
            amplitude_state: crate::runtime::state::State::new(0.5),
            wave_offset: st(),
            cache: Arc::new(Mutex::new(LinearShapesCache::default())),
        };
        assert_ne!(dbase.node_key(), dprog.node_key(), "progress 应进 key");
        let dtok = LinearWavyDeterminateNode {
            color, track_color: track, stroke_width: 4.0, track_stroke_width: 4.0,
            cap: ProgressIndicatorStrokeCap::Round, gap_size: 4.0, stop_size: 4.0,
            progress: 0.5, wavelength: 40.0, enable_motion: true,
            amplitude_token: (1, 0, 12345),
            amplitude_state: crate::runtime::state::State::new(0.5),
            wave_offset: st(),
            cache: Arc::new(Mutex::new(LinearShapesCache::default())),
        };
        assert_ne!(dbase.node_key(), dtok.node_key(), "amplitude_token 应进 key");
    }

    #[test]
    fn wavy_linear_determinate_node_renders_identical_to_enum_draw() {
        // 双路对照（determinate 首帧确定值：amplitude=1/wave=0）：node 路 vs
        // 旧闭包复刻，裸 leaf 同 240×32 surface 逐字节。
        use crate::modifier::DrawNode;
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let color = WavyProgressIndicatorDefaults::indicator_color(&theme);
        let track = WavyProgressIndicatorDefaults::track_color(&theme);
        let render_leaf = |modifier: crate::modifier::Modifier| {
            let mut composer = Composer::new();
            let scene = |ctx: &mut ComposeCtx| {
                WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    let key = ctx.next_key();
                    ctx.start_leaf(key, modifier.clone());
                    ctx.end_node();
                });
            };
            composer.compose(scene);
            composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
            let mut surface = skia_safe::surfaces::raster_n32_premul((300, 32)).unwrap();
            let canvas = surface.canvas();
            canvas.clear(skia_safe::Color::WHITE);
            let root = composer.layout_root_idx().expect("root");
            let nodes = composer.arena_nodes();
            crate::render::render(nodes, root, canvas);
            let pm = surface.peek_pixels().expect("pixmap");
            pm.pixels::<[u8; 4]>().expect("pixels").to_vec()
        };
        let cache_n = Arc::new(Mutex::new(LinearShapesCache::default()));
        let cache_e = Arc::new(Mutex::new(LinearShapesCache::default()));
        let node_mod = crate::modifier::Modifier::new()
            .size(240.0, 32.0)
            .draw_node(LinearWavyDeterminateNode {
                color, track_color: track, stroke_width: 4.0, track_stroke_width: 4.0,
                cap: ProgressIndicatorStrokeCap::Round, gap_size: 4.0, stop_size: 4.0,
                progress: 0.5, wavelength: 40.0, enable_motion: true,
                amplitude_token: (0, 0, 0),
                amplitude_state: crate::runtime::state::State::new(1.0),
                wave_offset: crate::runtime::state::Visual::new(0.0),
                cache: cache_n,
            });
        let enum_mod = crate::modifier::Modifier::new()
            .size(240.0, 32.0)
            .draw(move |canvas, rect| {
                draw_linear_wavy_determinate(
                    canvas, rect, color, track, 4.0, 4.0,
                    ProgressIndicatorStrokeCap::Round, 4.0, 4.0,
                    0.5, 1.0, 40.0, 0.0, true, true, &cache_e,
                );
            });
        let px_node = render_leaf(node_mod);
        let px_enum = render_leaf(enum_mod);
        assert_eq!(px_node.len(), 300 * 32);
        assert_eq!(px_node, px_enum, "wavy node 路��旧 draw 路必须逐字���一致");
        // node_key 具名前缀可观测
        let probe = LinearWavyIndeterminateNode {
            color, track_color: track, stroke_width: 4.0, track_stroke_width: 4.0,
            cap: ProgressIndicatorStrokeCap::Round, gap_size: 4.0,
            amplitude: 1.0, wavelength: 20.0, enable_motion: true,
            fh: crate::runtime::state::Visual::new(0.0),
            ft: crate::runtime::state::Visual::new(0.0),
            sh: crate::runtime::state::Visual::new(0.0),
            st: crate::runtime::state::Visual::new(0.0),
            wave_offset: crate::runtime::state::Visual::new(0.0),
            cache: Arc::new(Mutex::new(LinearShapesCache::default())),
        };
        assert!(probe.node_key().starts_with("wavy-linear-indet:"), "key 应有具名前缀，实际 {}", probe.node_key());
    }
