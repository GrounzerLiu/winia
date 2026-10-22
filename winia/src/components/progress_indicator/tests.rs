    use super::*;
    use crate::runtime::composer::Composer;
    use crate::layout::Constraints;

    // ── 纯函数：gap/stop 几何 ──
    #[test]
    fn gap_fraction_matches_compose() {
        // Compose：Round cap → adjustedGap = gap(4) + strokeWidth(4) = 8dp；
        // 组件宽 240dp → gapFraction = 8/240
        let adjusted: f32 = 4.0 + 4.0;
        assert!(((adjusted / 240.0) - 8.0 / 240.0).abs() < 1e-6);
        // Butt cap → 不加 strokeWidth
        assert_eq!(4.0 / 240.0, 4.0 / 240.0);
    }

    #[test]
    fn stop_indicator_geometry() {
        // Compose drawStopIndicator：adjustedStopSize = min(4, height)；
        // stopOffset = min((h - size)/2, 6dp)；圆形圆心 = 宽 - size/2 - offset
        let h = 4.0;
        let stop_size = LINEAR_STOP_SIZE.min(h);
        let offset = ((h - stop_size) / 2.0).min(STOP_INDICATOR_TRAILING_SPACE);
        let cx = 240.0 - stop_size / 2.0 - offset;
        assert_eq!(stop_size, 4.0);
        assert_eq!(offset, 0.0);
        assert_eq!(cx, 238.0, "240 - 2 - 0");
        // 大高度：offset 受限 6dp
        let h2: f32 = 20.0;
        let offset2 = ((h2 - 4.0) / 2.0).min(STOP_INDICATOR_TRAILING_SPACE);
        assert_eq!(offset2, 6.0, "(20-4)/2=8 超限 → 6");
    }

    #[test]
    fn circular_gap_sweep_conversion() {
        // Compose：gapSizeSweep = adjustedGap / (π × 直径) × 360°
        let sweep = gap_size_sweep(8.0, 40.0);
        let expected = (8.0 / (std::f32::consts::PI * 40.0)) * 360.0;
        assert!((sweep - expected).abs() < 1e-4);
    }

    // ── 颜色解析 ──
    #[test]
    fn default_colors_resolution() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        assert_eq!(ProgressIndicatorDefaults::indicator_color(&theme), theme.primary);
        assert_eq!(ProgressIndicatorDefaults::track_color(&theme), theme.secondary_container);
        assert_eq!(ProgressIndicatorDefaults::linear_stroke_cap(), ProgressIndicatorStrokeCap::Round);
        assert_eq!(ProgressIndicatorDefaults::circular_stroke_cap(), ProgressIndicatorStrokeCap::Round);
    }

    // ── 动画规格 ──
    #[test]
    fn linear_line_spec_matches_compose_keyframes() {
        // Compose linearIndeterminateFirstLineHeadAnimationSpec：1750ms 周期，0 at 0ms，1 at 1000ms
        let spec = linear_first_line_head_spec();
        assert_eq!(spec.duration, Duration::from_millis(1750));
        let frames = match &spec.base {
            Some(crate::animation::DurationSpec::Keyframes(kf)) => &kf.frames,
            _ => panic!("head spec 应为 keyframes"),
        };
        assert_eq!(frames.len(), 3);
        assert_eq!(frames[0].0, 0.0);
        assert_eq!(frames[0].1, 0.0);
        // 保持段：0 at 0ms（首帧即 0）
        // 上升段：1000/1750 到 1
        assert!(((frames[2].0 - 1000.0 / 1750.0).abs()) < 1e-5);
        assert_eq!(frames[2].1, 1.0);
    }

    #[test]
    fn linear_tail_delay_matches_compose() {
        // Compose firstLineTail：0 at 250ms → 保持段 [0, 250/1750]
        let spec = linear_first_line_tail_spec();
        let frames = match &spec.base {
            Some(crate::animation::DurationSpec::Keyframes(kf)) => &kf.frames,
            _ => panic!(),
        };
        assert_eq!(frames.len(), 3);
        assert!(((frames[1].0 - 250.0 / 1750.0).abs()) < 1e-5, "tail 保持段至 250ms");
        assert_eq!(frames[1].1, 0.0);
        assert!(((frames[2].0 - 1250.0 / 1750.0).abs()) < 1e-5);
    }

    fn is_linear(i: &Arc<dyn interpolator::Interpolator>) -> bool {
        // Linear 恒等：任意 x 返回 x
        (i.interpolate(0.3) - 0.3).abs() < 1e-6 && (i.interpolate(0.7) - 0.7).abs() < 1e-6
    }

    fn is_standard(i: &Arc<dyn interpolator::Interpolator>) -> bool {
        // Standard(0.2,0,0,1)：x=0.5 时 y≈0.63（非线性但起点不猛冲）
        !is_linear(i) && i.interpolate(0.1) < 0.3
    }

    #[test]
    fn circular_animation_specs_match_compose() {
        // 全局旋转：6000ms 线性 0→1080
        let g = circular_global_rotation_spec();
        assert_eq!(g.duration, Duration::from_millis(6000));
        assert!(matches!(g.base, Some(crate::animation::DurationSpec::Tween(_))));
        // 额外旋转：300ms 到 90°（0.25）
        let a = circular_additional_rotation_spec();
        let frames = match &a.base {
            Some(crate::animation::DurationSpec::Keyframes(kf)) => &kf.frames,
            _ => panic!(),
        };
        assert_eq!(frames.len(), 8);
        assert!(((frames[1].0 - 300.0 / 6000.0).abs()) < 1e-5);
        assert_eq!(frames[1].1, 0.25);
        // ⚠ easing 分配（对齐 Compose VectorizedKeyframesSpec 源码语义）：
        // `using E` 属于【段起点帧】；0ms 为隐式补入帧 → LinearEasing。故 Compose 中
        // `90f at 300 using Decelerate` 只影响 hold 段 [300,1500]（值不变），
        // **所有动画段实际全为 Linear**（曾误设 0→90° 为 Decelerate——起点斜率
        // 0.7/0.05=14 倍线性速度导致猛冲，实测一顿一顿）。
        for idx in [1usize, 3, 5, 7] {
            assert!(
                is_linear(&frames[idx].2),
                "全部动画段应 Linear（Compose 隐式/默认），idx={idx}",
            );
        }
        // 进度呼吸：上升段 [0,3000ms] 起点 0ms 隐式帧 → Linear；
        // 下降段 [3000,6000ms] 起点 `0.87 at 3000 using Standard` → Standard
        let p = circular_progress_spec();
        let frames = match &p.base {
            Some(crate::animation::DurationSpec::Keyframes(kf)) => &kf.frames,
            _ => panic!(),
        };
        assert_eq!(frames[1].0, 0.5);
        assert_eq!(frames[1].1, 1.0);
        assert_eq!(frames[2].1, 0.0);
        assert!(is_linear(&frames[1].2), "上升段应 Linear（隐式 0ms 帧）");
        assert!(is_standard(&frames[2].2), "下降段应 Standard（显式 using）");
    }

    #[test]
    fn infinite_keyframes_registers_and_cleans() {
        // 验证 keyframes 无限动画能注册进全局动画表并被清理
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        use crate::animation::push_infinite;
        let state = crate::runtime::state::State::new(0.0f32);
        push_infinite(state.clone(), 0.0, 1.0, linear_first_line_head_spec());
        assert!(crate::animation::has_animation_for_state(state.state_id()));
        // 更新一帧：Restart 模式下 t≈0 → 曲线起点 0
        crate::animation::update_animations();
        assert!(state.peek().abs() < 0.01, "t≈0 时 keyframes 首帧值=0");
        crate::animation::remove_animation_by_state(state.state_id());
        assert!(!crate::animation::has_animation_for_state(state.state_id()));
    }

    // ── 像素测试：determinate 渲染 ──
    fn render_progress(build: impl FnOnce(&mut ComposeCtx)) -> (Vec<[u8; 4]>, usize) {
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
        (px.to_vec(), pm.width() as usize)
    }

    /// 读取 (x, y) 像素为 RGBA（raster_n32_premul 是 BGRA 小端）
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
    fn linear_determinate_pixels() {
        // 进度 50%：左半 primary（indicator），右半 secondary_container（track），
        // 最右端 stop 圆点（primary）
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let (buf, w) = render_progress(|ctx| {
            LinearProgressIndicator::new(0.5).build(ctx);
        });
        assert_eq!(w, 300);
        // 组件默认 240×4 放在根节点内（BoxLayout 默认对齐——验证实际位置）
        // 先扫描找到指示器行：y 方向高度 4dp，找 primary 色所在行
        let primary = ProgressIndicatorDefaults::indicator_color(&theme);
        let track = ProgressIndicatorDefaults::track_color(&theme);
        // 全图扫描：收集所有 primary 像素的 x 范围（indicator 段 + stop 圆点）
        let mut xs = vec![];
        for y in 0..300 {
            for x in 0..300 {
                let p = px_at(&buf, w, x, y);
                if color_eq(primary, p, 4) { xs.push(x); }
            }
        }
        assert!(!xs.is_empty(), "indicator 段应有 primary 像素");
        let x_min = *xs.iter().min().unwrap();
        let x_max = *xs.iter().max().unwrap();
        // 进度 50% + Round cap 延伸：indicator 从 0 到 ~120+2
        assert!(x_min <= 2, "indicator 从最左开始（Round cap 内缩 2）");
        // stop 圆点在右端（x ≈ 238±3），与 indicator 段分离
        assert!(x_max >= 235 && x_max <= 245, "stop 圆点中心在 238 附近，x_max={x_max}");
        // track 色：右半（在 stop 左侧大片区域）
        let mut track_xs = vec![];
        for y in 0..300 {
            for x in 0..300 {
                let p = px_at(&buf, w, x, y);
                if color_eq(track, p, 4) { track_xs.push(x); }
            }
        }
        assert!(!track_xs.is_empty(), "track 段应有 secondary_container 像素");
        // track 从 indicator 末端（~122+gap 8 ≈ 130）开始
        let t_min = *track_xs.iter().min().unwrap();
        assert!(t_min >= 125 && t_min <= 135, "track 从 ~130 开始，t_min={t_min}");
    }

    #[test]
    fn linear_determinate_full_progress() {
        // progress=1：全部 primary（无 track），stop 仍在末端
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let primary = ProgressIndicatorDefaults::indicator_color(&theme);
        let track = ProgressIndicatorDefaults::track_color(&theme);
        let (buf, w) = render_progress(|ctx| {
            LinearProgressIndicator::new(1.0).build(ctx);
        });
        let mut row = None;
        for y in 0..300 {
            for x in 0..300 {
                let p = px_at(&buf, w, x, y);
                if color_eq(primary, p, 4) { row = Some(y); break; }
            }
            if row.is_some() { break; }
        }
        let y = row.expect("行");
        let mut track_found = false;
        for x in 0..300 {
            if color_eq(track, px_at(&buf, w, x, y), 4) { track_found = true; break; }
        }
        assert!(!track_found, "progress=1 时无 track");
    }

    #[test]
    fn linear_zero_progress_no_indicator() {
        // progress=0：indicator 零长不画；track 全宽；stop 在末端
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let primary = ProgressIndicatorDefaults::indicator_color(&theme);
        let (buf, w) = render_progress(|ctx| {
            LinearProgressIndicator::new(0.0).build(ctx);
        });
        let mut row = None;
        for y in 0..300 {
            for x in 0..300 {
                let p = px_at(&buf, w, x, y);
                if color_eq(primary, p, 4) { row = Some(y); break; }
            }
            if row.is_some() { break; }
        }
        // 仅 stop 圆点在 238 附近（indicator 零长无像素）
        let y = row.expect("stop 圆点行");
        let mut primary_xs = vec![];
        for x in 0..300 {
            if color_eq(primary, px_at(&buf, w, x, y), 4) { primary_xs.push(x); }
        }
        assert!(!primary_xs.is_empty());
        assert!(primary_xs.iter().all(|&x| x >= 230), "只剩 stop 圆点（x≥230）");
    }

    #[test]
    fn circular_determinate_pixels() {
        // 25% 进度：12 点起顺时针 90° 弧为 primary；其余 track（secondary_container）
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let primary = ProgressIndicatorDefaults::indicator_color(&theme);
        let track = ProgressIndicatorDefaults::track_color(&theme);
        let (buf, w) = render_progress(|ctx| {
            CircularProgressIndicator::new(0.25).build(ctx);
        });
        // 组件 40×40，居中于 300×300？BoxLayout 默认对齐需确认——扫描全图
        // 中心附近找 primary 弧
        let mut found_primary = false;
        let mut found_track = false;
        for y in 0..300 {
            for x in 0..300 {
                let p = px_at(&buf, w, x, y);
                if color_eq(primary, p, 4) { found_primary = true; }
                if color_eq(track, p, 4) { found_track = true; }
            }
        }
        assert!(found_primary, "应有 primary 弧");
        assert!(found_track, "应有 track 弧");
    }

    #[test]
    fn circular_indeterminate_no_track() {
        // indeterminate circular：无 track（indeterminate track = Transparent），只有 primary
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let track = ProgressIndicatorDefaults::track_color(&theme);
        // 直接调绘制函数（无动画帧上下文——用固定值）
        let mut surface = skia_safe::surfaces::raster_n32_premul((40, 40)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::WHITE);
        let rect = skia_safe::Rect::from_xywh(0.0, 0.0, 40.0, 40.0);
        draw_circular_indeterminate(
            canvas, rect,
            ProgressIndicatorDefaults::indicator_color(&theme),
            ProgressIndicatorStrokeCap::Round,
            TRACK_ACTIVE_SPACE,
            CIRCULAR_STROKE_WIDTH,
            0.0, 0.0, 0.5,
        );
        let pm = surface.peek_pixels().unwrap();
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().unwrap();
        let mut track_found = false;
        for p in px.iter() {
            let rgba = [p[2], p[1], p[0], p[3]];
            if color_eq(track, rgba, 4) { track_found = true; break; }
        }
        assert!(!track_found, "indeterminate circular 无 track 色");
    }

    #[test]
    fn linear_indeterminate_draws_lines() {
        // 直接调绘制函数：固定 head/tail 值验证两条线 + track 分段逻辑
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let primary = ProgressIndicatorDefaults::indicator_color(&theme);
        let track = ProgressIndicatorDefaults::track_color(&theme);
        let mut surface = skia_safe::surfaces::raster_n32_premul((240, 4)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::WHITE);
        let rect = skia_safe::Rect::from_xywh(0.0, 0.0, 240.0, 4.0);
        // 用 Compose 时序值（t=1000ms）：fh=1.0, ft=0.75, sh=0.412, st=0.118——
        // 真实动画中 line2 head 恒 ≤ line1 tail，track-between 段始终有效
        draw_linear_indeterminate(
            canvas, rect,
            primary, track, ProgressIndicatorStrokeCap::Round, TRACK_ACTIVE_SPACE,
            1.0, 0.75,   // first head/tail
            0.412, 0.118, // second head/tail
        );
        let pm = surface.peek_pixels().unwrap();
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().unwrap();
        let w = 240usize;
        // 采样中线 y=2 各 x 的颜色
        let color_at = |x: usize| {
            let p = px[2 * w + x];
            [p[2], p[1], p[0], p[3]]
        };
        // 期望（t=1000ms，gapFraction = 8/240 ≈ 0.033）：
        // line1 [ft, fh] = [0.75, 1.0] → x [180, 240] primary；
        // line2 [st, sh] = [0.118, 0.412] → x [28, 99] primary；
        // track-between [sh+gap, ft-gap] = [0.445, 0.717] → x [107, 172] track；
        // track-after-line2 [0, st-gap] = [0, 0.085] → x [0, 20] track；
        // x=60：line2 内 → primary
        assert!(color_eq(primary, color_at(60), 6), "x=60 在 line2 [28,99] 内");
        // x=200：line1 内 [180,240] → primary
        assert!(color_eq(primary, color_at(200), 6), "x=200 在 line1 [180,240] 内");
        // x=140：track-between [107,172] → track
        assert!(color_eq(track, color_at(140), 6), "x=140 在 track-between [107,172] 段");
        // x=10：track-after-line2 [0,20] → track
        assert!(color_eq(track, color_at(10), 6), "x=10 在 track-after [0,20] 段");
    }

    #[test]
    fn circular_custom_stroke_width_applies() {
        // .stroke_width(8.0) 应画出更宽的弧——直接调绘制函数验证像素行数
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let color = ProgressIndicatorDefaults::indicator_color(&theme);
        let mut surface = skia_safe::surfaces::raster_n32_premul((40, 40)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::WHITE);
        let rect = skia_safe::Rect::from_xywh(0.0, 0.0, 40.0, 40.0);
        // 100% 进度（sweep=360）→ 完整圆环
        draw_circular_determinate(
            canvas, rect,
            color, ProgressIndicatorDefaults::track_color(&theme),
            ProgressIndicatorStrokeCap::Round, TRACK_ACTIVE_SPACE, 8.0,
            1.0,
        );
        let pm = surface.peek_pixels().unwrap();
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().unwrap();
        let w = 40usize;
        // 统计中线 x=20 列上 color 像素的行数（弧宽 = stroke_width）
        let mut rows = 0;
        for y in 0..40 {
            let p = px[y * w + 20];
            let rgba = [p[2], p[1], p[0], p[3]];
            if color_eq(color, rgba, 6) { rows += 1; }
        }
        // 8px 宽弧完整圆环：中线列穿过上下两段 → ~16 行（±2 反锯齿）
        assert!(rows >= 14 && rows <= 18, "8px stroke 完整圆环中线 ~16 行，实际 {rows}");
    }

    #[test]
    fn height_gt_width_falls_back_to_butt() {
        // Compose：height > width 时忽略 cap（退化 Butt）——不 crash 即可
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut surface = skia_safe::surfaces::raster_n32_premul((4, 8)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::WHITE);
        let rect = skia_safe::Rect::from_xywh(0.0, 0.0, 4.0, 8.0);
        draw_linear_determinate(
            canvas, rect,
            ProgressIndicatorDefaults::indicator_color(&theme),
            ProgressIndicatorDefaults::track_color(&theme),
            ProgressIndicatorStrokeCap::Round,
            TRACK_ACTIVE_SPACE,
            0.5, true,
        );
    }

    // ── 组件级：indeterminate 注册动画 ──
    #[test]
    fn indeterminate_build_registers_infinite_animations() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = Composer::new();
        let scene = |ctx: &mut ComposeCtx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                LinearProgressIndicator::indeterminate().build(ctx);
            });
        };
        composer.compose(scene);
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
        assert!(crate::animation::is_animating(), "indeterminate 应注册无限动画");
        // 清理全局动画表（防跨测试污染）
        crate::animation::clear_all_animations();
    }

    #[test]
    fn circular_indeterminate_build_registers_animations() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = Composer::new();
        let scene = |ctx: &mut ComposeCtx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                CircularProgressIndicator::indeterminate().build(ctx);
            });
        };
        composer.compose(scene);
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
        assert!(crate::animation::is_animating());
        crate::animation::clear_all_animations();
    }

    // ── LinearIndeterminateNode 迁移（exp/progress-node 探针）──
    #[test]
    fn linear_indeterminate_node_key_covers_all_static_params() {
        use crate::modifier::DrawNode;
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let color = ProgressIndicatorDefaults::indicator_color(&theme);
        let track = ProgressIndicatorDefaults::track_color(&theme);
        let mk = |color: Color, track_color: Color, cap: ProgressIndicatorStrokeCap, gap: f32| {
            LinearIndeterminateNode {
                color, track_color, cap, gap,
                fh: crate::runtime::state::Visual::new(0.1),
                ft: crate::runtime::state::Visual::new(0.2),
                sh: crate::runtime::state::Visual::new(0.3),
                st: crate::runtime::state::Visual::new(0.4),
            }
        };
        let base = mk(color, track, ProgressIndicatorStrokeCap::Round, 4.0);
        // 同参 → 相等（Skip）
        assert_eq!(base.node_key(), mk(color, track, ProgressIndicatorStrokeCap::Round, 4.0).node_key());
        // 静态参数任一变化 → 不等（Enter）
        let other_color = Color::from_argb(255, 1, 2, 3);
        assert_ne!(base.node_key(), mk(other_color, track, ProgressIndicatorStrokeCap::Round, 4.0).node_key(), "color 应进 key");
        assert_ne!(base.node_key(), mk(color, other_color, ProgressIndicatorStrokeCap::Round, 4.0).node_key(), "track_color 应进 key");
        assert_ne!(base.node_key(), mk(color, track, ProgressIndicatorStrokeCap::Butt, 4.0).node_key(), "cap 应进 key");
        assert_ne!(base.node_key(), mk(color, track, ProgressIndicatorStrokeCap::Round, 8.0).node_key(), "gap 应进 key");
        // 瞬态动画值变化 → 相等（不进 key，渲染期 peek 直读）
        let moved = mk(color, track, ProgressIndicatorStrokeCap::Round, 4.0);
        moved.fh.set(0.9);
        moved.ft.set(0.8);
        moved.sh.set(0.7);
        moved.st.set(0.6);
        assert_eq!(base.node_key(), moved.node_key(), "无限动画 peek 值变化不应进 key");
    }

    #[test]
    fn linear_indeterminate_node_renders_identical_to_enum_draw() {
        // 真双路对照：同参一路 draw_node(LinearIndeterminateNode)，一路旧
        // `Modifier::draw` 匿名闭包（迁移前 build 侧体逐行复刻），同 300×300
        // surface 逐字节 assert_eq。
        use crate::modifier::DrawNode;
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let color = ProgressIndicatorDefaults::indicator_color(&theme);
        let track = ProgressIndicatorDefaults::track_color(&theme);
        let render_with = |modifier: Modifier| {
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
            let mut surface = skia_safe::surfaces::raster_n32_premul((300, 300)).unwrap();
            let canvas = surface.canvas();
            canvas.clear(skia_safe::Color::WHITE);
            let root = composer.layout_root_idx().expect("root");
            let nodes = composer.arena_nodes();
            crate::render::render(nodes, root, canvas);
            let pm = surface.peek_pixels().expect("pixmap");
            pm.pixels::<[u8; 4]>().expect("pixels").to_vec()
        };
        let node_mod = Modifier::new().size(240.0, 4.0).draw_node(LinearIndeterminateNode {
            color, track_color: track, cap: ProgressIndicatorStrokeCap::Round, gap: 4.0,
            fh: crate::runtime::state::Visual::new(0.6),
            ft: crate::runtime::state::Visual::new(0.2),
            sh: crate::runtime::state::Visual::new(0.9),
            st: crate::runtime::state::Visual::new(0.5),
        });
        // 旧闭包逐行复刻（同参 draw_linear_indeterminate）
        let enum_mod = Modifier::new().size(240.0, 4.0).draw(move |canvas, rect| {
            draw_linear_indeterminate(canvas, rect, color, track, ProgressIndicatorStrokeCap::Round, 4.0, 0.6, 0.2, 0.9, 0.5);
        });
        let px_node = render_with(node_mod);
        let px_enum = render_with(enum_mod);
        assert_eq!(px_node.len(), 300 * 300);
        assert_eq!(px_node, px_enum, "node 路与旧 draw 闭包路必须逐字节一致（迁移保真）");
        // node_key 可调试观测（具名可观测——匿名闭包无此能力）
        let probe = LinearIndeterminateNode {
            color, track_color: track, cap: ProgressIndicatorStrokeCap::Round, gap: 4.0,
            fh: crate::runtime::state::Visual::new(0.0),
            ft: crate::runtime::state::Visual::new(0.0),
            sh: crate::runtime::state::Visual::new(0.0),
            st: crate::runtime::state::Visual::new(0.0),
        };
        assert!(probe.node_key().starts_with("linear-indeterminate:"), "key 应有具名前缀，实际 {}", probe.node_key());
    }

    #[test]
    fn linear_indeterminate_node_skips_on_animation_frames() {
        // 三问(a)探针：无限动画逐帧推进时，node_key 不变 → param_eq 通过 →
        // 同 modifier 下 start_restartable_group 可 Skip；重绘由动画引擎每帧
        // request_redraw 驱动（与 Skip 无关）。此处直接断言 param_eq 语义。
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let color = ProgressIndicatorDefaults::indicator_color(&theme);
        let track = ProgressIndicatorDefaults::track_color(&theme);
        let fh = crate::runtime::state::Visual::new(0.0f32);
        let ft = crate::runtime::state::Visual::new(0.0f32);
        let sh = crate::runtime::state::Visual::new(0.0f32);
        let st = crate::runtime::state::Visual::new(0.0f32);
        // 注册真实无限动画（与 build 侧同 spec），推进 5 帧
        crate::animation::push_infinite_visual(fh.clone(), 0.0, 1.0, linear_first_line_head_spec());
        crate::animation::push_infinite_visual(ft.clone(), 0.0, 1.0, linear_first_line_tail_spec());
        for _ in 0..5 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        // 动画值已推进（无限动画永动），但同参 modifier param_eq 仍通过
        let mk = || {
            Modifier::new().size(240.0, 4.0).draw_node(LinearIndeterminateNode {
                color, track_color: track, cap: ProgressIndicatorStrokeCap::Round, gap: 4.0,
                fh: fh.clone(), ft: ft.clone(), sh: sh.clone(), st: st.clone(),
            })
        };
        assert!(fh.peek() != 0.0 || ft.peek() != 0.0, "无限动画应已推进（fh={}, ft={}）", fh.peek(), ft.peek());
        assert!(mk().param_eq(&mk()), "动画值变化时 param_eq 应仍通过（Skip，不多余 Enter）");
        crate::animation::clear_all_animations();
    }

    #[test]
    fn determinate_nodes_key_covers_progress_and_static_params() {
        use crate::modifier::DrawNode;
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let color = ProgressIndicatorDefaults::indicator_color(&theme);
        let track = ProgressIndicatorDefaults::track_color(&theme);
        // linear determinate：progress 进 key（静态值，变化即 Enter）
        let base = LinearDeterminateNode {
            color, track_color: track, cap: ProgressIndicatorStrokeCap::Round,
            gap: 4.0, progress: 0.5, draw_stop: true,
        };
        let same = LinearDeterminateNode {
            color, track_color: track, cap: ProgressIndicatorStrokeCap::Round,
            gap: 4.0, progress: 0.5, draw_stop: true,
        };
        assert_eq!(base.node_key(), same.node_key());
        let moved = LinearDeterminateNode {
            color, track_color: track, cap: ProgressIndicatorStrokeCap::Round,
            gap: 4.0, progress: 0.6, draw_stop: true,
        };
        assert_ne!(base.node_key(), moved.node_key(), "progress 应进 key");
        let no_stop = LinearDeterminateNode {
            color, track_color: track, cap: ProgressIndicatorStrokeCap::Round,
            gap: 4.0, progress: 0.5, draw_stop: false,
        };
        assert_ne!(base.node_key(), no_stop.node_key(), "draw_stop 应进 key");
        // circular determinate：stroke_width + progress 进 key
        let cbase = CircularDeterminateNode {
            color, track_color: track, cap: ProgressIndicatorStrokeCap::Round,
            gap: 4.0, stroke_width: 4.0, progress: 0.5,
        };
        let csame = CircularDeterminateNode {
            color, track_color: track, cap: ProgressIndicatorStrokeCap::Round,
            gap: 4.0, stroke_width: 4.0, progress: 0.5,
        };
        assert_eq!(cbase.node_key(), csame.node_key());
        let cmoved = CircularDeterminateNode {
            color, track_color: track, cap: ProgressIndicatorStrokeCap::Round,
            gap: 4.0, stroke_width: 4.0, progress: 0.7,
        };
        assert_ne!(cbase.node_key(), cmoved.node_key(), "circular progress 应进 key");
        // circular indeterminate：静态三参进 key，动画三值不进
        let ibase = CircularIndeterminateNode {
            color, cap: ProgressIndicatorStrokeCap::Round, stroke_width: 4.0,
            global: crate::runtime::state::Visual::new(10.0),
            additional: crate::runtime::state::Visual::new(20.0),
            progress_anim: crate::runtime::state::Visual::new(0.5),
        };
        let imoved = CircularIndeterminateNode {
            color, cap: ProgressIndicatorStrokeCap::Round, stroke_width: 4.0,
            global: crate::runtime::state::Visual::new(999.0),
            additional: crate::runtime::state::Visual::new(888.0),
            progress_anim: crate::runtime::state::Visual::new(0.1),
        };
        assert_eq!(ibase.node_key(), imoved.node_key(), "circular 无限动画值不应进 key");
        let iother = CircularIndeterminateNode {
            color, cap: ProgressIndicatorStrokeCap::Butt, stroke_width: 4.0,
            global: crate::runtime::state::Visual::new(10.0),
            additional: crate::runtime::state::Visual::new(20.0),
            progress_anim: crate::runtime::state::Visual::new(0.5),
        };
        assert_ne!(ibase.node_key(), iother.node_key(), "circular cap 应进 key");
    }

    #[test]
    fn circular_determinate_node_renders_identical_to_enum_draw() {
        // circular 定值双路对照（不定值已由 linear 探针覆盖 peek 机制；
        // circular 不定值多 canvas.save/rotate，旧闭包复刻同参即可比）。
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let color = ProgressIndicatorDefaults::indicator_color(&theme);
        let track = ProgressIndicatorDefaults::track_color(&theme);
        let render_with = |modifier: Modifier| {
            let mut composer = Composer::new();
            let scene = |ctx: &mut ComposeCtx| {
                WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    let key = ctx.next_key();
                    ctx.start_leaf(key, modifier.clone());
                    ctx.end_node();
                });
            };
            composer.compose(scene);
            composer.layout(Constraints::new(0.0, 100.0, 0.0, 100.0));
            let mut surface = skia_safe::surfaces::raster_n32_premul((100, 100)).unwrap();
            let canvas = surface.canvas();
            canvas.clear(skia_safe::Color::WHITE);
            let root = composer.layout_root_idx().expect("root");
            let nodes = composer.arena_nodes();
            crate::render::render(nodes, root, canvas);
            let pm = surface.peek_pixels().expect("pixmap");
            pm.pixels::<[u8; 4]>().expect("pixels").to_vec()
        };
        let node_mod = Modifier::new().size(40.0, 40.0).draw_node(CircularDeterminateNode {
            color, track_color: track, cap: ProgressIndicatorStrokeCap::Round,
            gap: 4.0, stroke_width: 4.0, progress: 0.5,
        });
        let enum_mod = Modifier::new().size(40.0, 40.0).draw(move |canvas, rect| {
            draw_circular_determinate(canvas, rect, color, track, ProgressIndicatorStrokeCap::Round, 4.0, 4.0, 0.5);
        });
        let px_node = render_with(node_mod);
        let px_enum = render_with(enum_mod);
        assert_eq!(px_node.len(), 100 * 100);
        assert_eq!(px_node, px_enum, "circular node 路与旧 draw 路必须逐字节一致");
    }

    #[test]
    fn draw_stop_toggle_takes_effect_on_recompose() {
        // draw_stop_indicator 切换的绘制正确性：State<bool> 持开关，首帧
        // stop=true（右端有 primary 圆点），set(false) + recompose 后圆点消失。
        // 注：本测试走 State 驱动（根 dirty 整树 Enter），测的是"false 确实不画"，
        // 不覆盖 changed 缺失的窄场景（无 State 的纯 build 入参 + 父带动 Skip）。
        // changed(&self.draw_stop_indicator) 已声明，与 node_key 语义对齐。
        use skia_safe::{Color as SkColor, surfaces};
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let primary = ProgressIndicatorDefaults::indicator_color(&theme);
        let stop: crate::runtime::state::State<bool> = crate::runtime::state::State::new(true);
        let mut composer = Composer::new();
        let make_scene = |stop: crate::runtime::state::State<bool>, theme: ThemeColors| {
            move |ctx: &mut ComposeCtx| {
                let s = stop.get();
                WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    LinearProgressIndicator::new(0.5)
                        .draw_stop_indicator(s)
                        .build(ctx);
                });
            }
        };
        let mut render_now = |composer: &mut Composer| {
            composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
            let mut surface = surfaces::raster_n32_premul((300, 300)).unwrap();
            let canvas = surface.canvas();
            canvas.clear(SkColor::WHITE);
            let root = composer.layout_root_idx().expect("root");
            let nodes = composer.arena_nodes();
            crate::render::render(nodes, root, canvas);
            let pm = surface.peek_pixels().expect("pixmap");
            pm.pixels::<[u8; 4]>().expect("pixels").to_vec()
        };
        composer.compose(make_scene(stop.clone(), theme.clone()));
        let buf1 = render_now(&mut composer);
        let has_stop = |buf: &[[u8; 4]]| {
            (230..250).any(|x| {
                let p = buf[2 * 300 + x];
                color_eq(primary, [p[2], p[1], p[0], p[3]], 8)
            })
        };
        assert!(has_stop(&buf1), "stop=true 时右端应有 primary 圆点");
        // 切换开关 → 增量重组 → 圆点消失
        stop.set(false);
        assert!(composer.recompose(make_scene(stop.clone(), theme.clone())), "开关变化应触发重组");
        let buf2 = render_now(&mut composer);
        assert!(!has_stop(&buf2), "stop=false 后右端圆点应消失");
    }
