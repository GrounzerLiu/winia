    use super::*;

    // ── 纯函数：刻度/吸附/换算 ──
    #[test]
    fn tick_fractions_matches_compose() {
        // steps=4 → 6 点（含两端）：[0, .2, .4, .6, .8, 1]（Compose steps+2）
        let tf = tick_fractions(4);
        assert_eq!(tf.len(), 6);
        for (i, f) in tf.iter().enumerate() {
            assert!(((f - i as f32 / 5.0).abs()) < 1e-6, "tick[{i}]={f}");
        }
        assert!(tick_fractions(0).is_empty(), "连续滑块无刻度");
    }

    #[test]
    fn snap_value_to_nearest_tick() {
        // 0..10 4 步 → 允许 2/4/6/8（Compose 语义）
        assert_eq!(snap_value(0.0, 4, 0.0, 10.0), 0.0);
        assert_eq!(snap_value(10.0, 4, 0.0, 10.0), 10.0);
        assert_eq!(snap_value(2.0, 4, 0.0, 10.0), 2.0);
        assert_eq!(snap_value(1.2, 4, 0.0, 10.0), 2.0, "1.2 → 最近刻度 2");
        assert_eq!(snap_value(2.9, 4, 0.0, 10.0), 2.0, "2.9 → 最近刻度 2");
        assert_eq!(snap_value(3.1, 4, 0.0, 10.0), 4.0, "3.1 → 最近刻度 4");
        // 连续：clamp 不吸附
        assert_eq!(snap_value(3.3, 0, 0.0, 10.0), 3.3);
        assert_eq!(snap_value(11.0, 0, 0.0, 10.0), 10.0, "越界 clamp");
    }

    #[test]
    fn value_at_x_and_fraction() {
        assert_eq!(value_at_x(0.0, 100.0, 0.0, 10.0, 0), 0.0);
        assert_eq!(value_at_x(100.0, 100.0, 0.0, 10.0, 0), 10.0);
        assert_eq!(value_at_x(50.0, 100.0, 0.0, 10.0, 0), 5.0);
        assert_eq!(value_at_x(-5.0, 100.0, 0.0, 10.0, 0), 0.0, "负坐标 clamp");
        assert_eq!(fraction_from_value(5.0, 0.0, 10.0), 0.5);
        assert_eq!(fraction_from_value(20.0, 0.0, 10.0), 1.0, "越界 clamp");
        assert_eq!(fraction_from_value(-1.0, 0.0, 10.0), 0.0);
    }

    // ── 颜色解析 ──
    #[test]
    fn slider_colors_resolution() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let c = SliderDefaults::slider_colors(&theme);
        // enabled：thumb/active = primary、inactive = secondary_container
        assert_eq!(c.thumb_color(true), theme.primary);
        assert_eq!(c.track_color(true, true), theme.primary);
        assert_eq!(c.track_color(true, false), theme.secondary_container);
        // tick 交叉（Compose defaultSliderColors）：active 区 = inactive track 色
        assert_eq!(c.tick_color(true, true), theme.secondary_container);
        assert_eq!(c.tick_color(true, false), theme.primary);
        // disabled：38%/38%/12%
        assert_eq!(c.thumb_color(false).a, (theme.on_surface.a as f32 * 0.38) as u8);
        assert_eq!(c.track_color(false, true).a, (theme.on_surface.a as f32 * 0.38) as u8);
        assert_eq!(c.track_color(false, false).a, (theme.on_surface.a as f32 * 0.12) as u8);
    }

    // ── 键盘步进（对齐 slideOnKeyEvents）──
    #[test]
    fn keyboard_steps_and_pages() {
        use std::sync::atomic::{AtomicU32, Ordering};
        use winit::keyboard::{Key, NamedKey};
        let mk = |k: Key, t: KbEventType| KbEvent {
            key: k,
            event_type: t,
            is_alt_pressed: false, is_ctrl_pressed: false, is_shift_pressed: false, is_meta_pressed: false,
            repeat: false,
        };
        let last = Arc::new(AtomicU32::new(0));
        let last_cb = last.clone();
        let set: Arc<dyn Fn(f32) + Send + Sync> = Arc::new(move |v: f32| { last_cb.store((v * 100.0) as u32, Ordering::Relaxed); });
        let none: Option<Arc<dyn Fn() + Send + Sync>> = None;
        // 连续：方向键 = 1% 值域
        assert!(handle_key(&mk(Key::Named(NamedKey::ArrowRight), KbEventType::KeyDown), 5.0, 0.0, 10.0, 0, &Some(set.clone()), &none));
        assert_eq!(last.load(Ordering::Relaxed), 510, "5 + 1%*10 = 5.1");
        assert!(handle_key(&mk(Key::Named(NamedKey::ArrowLeft), KbEventType::KeyDown), 5.0, 0.0, 10.0, 0, &Some(set.clone()), &none));
        assert_eq!(last.load(Ordering::Relaxed), 490, "5 - 0.1 = 4.9");
        // 离散 4 步：步长 = 10/5 = 2
        assert!(handle_key(&mk(Key::Named(NamedKey::ArrowRight), KbEventType::KeyDown), 4.0, 0.0, 10.0, 4, &Some(set.clone()), &none));
        assert_eq!(last.load(Ordering::Relaxed), 600, "4 + 2 = 6");
        // Home/End 端点
        assert!(handle_key(&mk(Key::Named(NamedKey::Home), KbEventType::KeyDown), 5.0, 0.0, 10.0, 0, &Some(set.clone()), &none));
        assert_eq!(last.load(Ordering::Relaxed), 0);
        assert!(handle_key(&mk(Key::Named(NamedKey::End), KbEventType::KeyDown), 5.0, 0.0, 10.0, 0, &Some(set.clone()), &none));
        assert_eq!(last.load(Ordering::Relaxed), 1000);
        // 无关键不消费
        assert!(!handle_key(&mk(Key::Named(NamedKey::Escape), KbEventType::KeyDown), 5.0, 0.0, 10.0, 0, &Some(set.clone()), &none));
    }

    // ── 像素测试：track/thumb 渲染 ──
    fn render_slider_px(build: impl FnOnce(&mut ComposeCtx)) -> (Vec<[u8; 4]>, usize) {
        use skia_safe::{Color as SkColor, surfaces};
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = crate::runtime::composer::Composer::new();
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

    #[test]
    fn slider_renders_track_and_thumb() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let (px, w) = render_slider_px(|ctx| {
            Slider::new(0.5)
                .value_range(0.0, 1.0)
                .on_value_change(|_| {})
                .build(ctx);
        });
        let prim = (theme.primary.r as i32, theme.primary.g as i32, theme.primary.b as i32);
        let sec = (theme.secondary_container.r as i32, theme.secondary_container.g as i32, theme.secondary_container.b as i32);
        // 节点 48 高 → 轨道中心 y=24；值 0.5 → active 0..150
        assert!(close(at(&px, w, 75.0, 24.0), prim), "active 轨道应为 primary");
        assert!(close(at(&px, w, 225.0, 24.0), sec), "inactive 轨道应为 secondary_container");
        // thumb 中心 x=150（宽 4 高 44）
        assert!(close(at(&px, w, 150.0, 24.0), prim), "thumb 应为 primary");
        assert!(close(at(&px, w, 150.0, 10.0), prim), "thumb 上段应 primary");
        assert!(close(at(&px, w, 250.0, 24.0), sec), "inactive 轨道右侧");
        // M3 轨道两段独立：active 到 thumb 左侧 gap=6（end_gap=8，thumb 半宽 2）
        // → active 终点 142、inactive 起点 158——间隙区（145, 155）应为白
        assert!(close(at(&px, w, 145.0, 24.0), (255, 255, 255)), "active 与 thumb 间隙应为白（实际 {:?}）", at(&px, w, 145.0, 24.0));
        assert!(close(at(&px, w, 155.0, 24.0), (255, 255, 255)), "inactive 与 thumb 间隙应为白（实际 {:?}）", at(&px, w, 155.0, 24.0));
        // stop indicator：inactive 段右端头中心 = track_right - corner = 300-8 = 292——
        // primary 点画在 secondary 轨道端头上（可见）；stop 不贴边（距边 corner）
        assert!(close(at(&px, w, 292.0, 24.0), prim), "inactive 端头 stop indicator 应为 primary（实际 {:?}）", at(&px, w, 292.0, 24.0));
        // 连续时 active 段左端 stop（8,24）与 active track 同色（primary）——
        // 用户规范（连续时开始的 stop 与 active track 同色）
        assert!(close(at(&px, w, 8.0, 24.0), prim), "连续时 active 端头 stop 应与 active track 同色（primary，实际 {:?}）", at(&px, w, 8.0, 24.0));
    }

    #[test]
    fn slider_zero_value_full_inactive() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let (px, w) = render_slider_px(|ctx| {
            Slider::new(0.0).value_range(0.0, 1.0).on_value_change(|_| {}).build(ctx);
        });
        let sec = (theme.secondary_container.r as i32, theme.secondary_container.g as i32, theme.secondary_container.b as i32);
        let prim = (theme.primary.r as i32, theme.primary.g as i32, theme.primary.b as i32);
        assert!(close(at(&px, w, 75.0, 24.0), sec), "value=0 全 inactive");
        assert!(close(at(&px, w, 150.0, 24.0), sec), "轨道中部 inactive（无 active）");
        // 端点几何：thumb 中心 = corner = 8（轨道端头圆心 = stop 位置）——
        // handle 端点正好停在 stop 上（stop 不贴边，thumb 左缘 6）
        assert!(close(at(&px, w, 8.0, 24.0), prim), "value=0 时 thumb 中心应在 x=8（stop 上，实际 {:?}）", at(&px, w, 8.0, 24.0));
    }

    /// The inactive tail needs to be longer than the round end it owns — `gap + cornerSize`, with or
    /// without ticks (Compose's `Track` uses its default `enableCornerShrinking = false` for both
    /// sliders, so the corner is always required). Near max the tail is shorter than that and must NOT
    /// be drawn; a clear stretch of track past the thumb is the correct look, not a sliver.
    #[test]
    fn slider_tail_near_max_is_left_clear() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let sec = (theme.secondary_container.r as i32, theme.secondary_container.g as i32, theme.secondary_container.b as i32);
        let prim = (theme.primary.r as i32, theme.primary.g as i32, theme.primary.b as i32);
        let white = (255, 255, 255);
        // value 0.9 → thumb centre 263.6, tail [271.6, 292] — longer than the 8 px corner → drawn,
        // and its stop dot at 292.
        let (px, w) = render_slider_px(|ctx| {
            Slider::new(0.9).value_range(0.0, 1.0).on_value_change(|_| {}).build(ctx);
        });
        assert!(close(at(&px, w, 280.0, 24.0), sec), "a long tail is drawn (got {:?})", at(&px, w, 280.0, 24.0));
        assert!(close(at(&px, w, 292.0, 24.0), prim), "with its stop dot (got {:?})", at(&px, w, 292.0, 24.0));
        // value 0.99 → tail [297.2, 300], 2.8 px: shorter than the corner, so nothing is drawn past
        // the thumb and no stop dot appears (a slider pinned near max otherwise shows a stray sliver).
        let (px2, w2) = render_slider_px(|ctx| {
            Slider::new(0.99).value_range(0.0, 1.0).on_value_change(|_| {}).build(ctx);
        });
        assert!(close(at(&px2, w2, 299.0, 24.0), white), "a sub-corner tail is left clear (got {:?})", at(&px2, w2, 299.0, 24.0));
    }

    /// A discrete slider only ever shows an on-tick value, INCLUDING one the caller passed in:
    /// Compose snaps in `SliderState`'s setter. Value 0.3 with steps = 4 (ticks 0, .2, .4, .6, .8, 1)
    /// must therefore draw at 0.4 — thumb centre 8 + 284 × 0.4 = 121.6 — and not between the ticks.
    #[test]
    fn discrete_slider_snaps_a_caller_supplied_value() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let (px, w) = render_slider_px(|ctx| {
            Slider::new(0.3).value_range(0.0, 1.0).steps(4).on_value_change(|_| {}).build(ctx);
        });
        let prim = (theme.primary.r as i32, theme.primary.g as i32, theme.primary.b as i32);
        let white = (255, 255, 255);
        assert!(close(at(&px, w, 121.6, 24.0), prim), "the thumb sits on the 0.4 tick (got {:?})", at(&px, w, 121.6, 24.0));
        // The active track reaches to one gap before the SNAPPED thumb (113.6): at 110 it is still
        // active, whereas an unsnapped 0.3 would have ended its fill at 85.2 and left this clear.
        assert!(close(at(&px, w, 110.0, 24.0), prim), "the fill follows the snapped value (got {:?})", at(&px, w, 110.0, 24.0));
        assert!(close(at(&px, w, 125.0, 24.0), white), "the gap after the snapped thumb is clear (got {:?})", at(&px, w, 125.0, 24.0));
    }

    #[test]
    fn slider_steps_renders_ticks() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let (px, w) = render_slider_px(|ctx| {
            Slider::new(0.5)
                .value_range(0.0, 1.0)
                .steps(4)
                .on_value_change(|_| {})
                .build(ctx);
        });
        let prim = (theme.primary.r as i32, theme.primary.g as i32, theme.primary.b as i32);
        let sec = (theme.secondary_container.r as i32, theme.secondary_container.g as i32, theme.secondary_container.b as i32);
        // tick 位置沿轨道内缩 corner：x = 8 + (300-16)*f = 8+284f
        // f=0.2 → 64.8、f=0.8 → 235.2；active_end = 150-8 = 142
        //（≤142 用 active 色 secondary_container，>142 用 inactive 色 primary）
        assert!(close(at(&px, w, 64.8, 24.0), sec), "active 区 tick = secondary_container（在 primary 轨道上）");
        assert!(close(at(&px, w, 235.2, 24.0), prim), "inactive 区 tick = primary（在 secondary 轨道上）");
        // 离散时 active 段左端 stop（8,24，与 f=0 的 tick 同位）应与同区 tick 同色
        //（secondary_container）——用户规范（离散与其它 stop 一致）
        assert!(close(at(&px, w, 8.0, 24.0), sec), "离散时 active 端头 stop 应与同区 tick 同色（secondary_container，实际 {:?}）", at(&px, w, 8.0, 24.0));
    }

    // ── 交互测试：按下立即跳转（用户规范——不等 tap/drag）──
    #[test]
    fn press_callback_jumps_immediately() {
        use std::sync::atomic::{AtomicI32, Ordering};
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let got = Arc::new(AtomicI32::new(-1));
        let mut composer = crate::runtime::composer::Composer::new();
        let scene = |ctx: &mut ComposeCtx| {
            let g = got.clone();
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                Slider::new(0.0)
                    .value_range(0.0, 10.0)
                    .on_value_change(move |v| g.store((v * 100.0) as i32, Ordering::Relaxed))
                    .build(ctx);
            });
        };
        composer.compose(scene);
        composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 300.0));
        // 渲染一次让 draw 闭包写入轨道宽度（200）
        let mut surface = skia_safe::surfaces::raster_n32_premul((200, 300)).unwrap();
        let canvas = surface.canvas();
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        crate::render::render(nodes, root, canvas);
        // 找 TapOnPress 回调并模拟按下 x=100（值应 = 10 * (100-8)/(200-16) = 5）
        let mut press: Option<Arc<dyn Fn((f32, f32)) + Send + Sync>> = None;
        for n in composer.arena_nodes() {
            for el in n.modifier.elements() {
                if let crate::modifier::ModifierElement::TapOnPress { cb } = el {
                    press = Some(cb.clone());
                }
            }
        }
        let press = press.expect("应有 TapOnPress");
        press((100.0, 24.0));
        assert_eq!(got.load(Ordering::Relaxed), 500, "按下 x=100/200 → value=5（立即跳转）");
        // 按下 x=150 → (150-8)/(200-16) = 0.7717 → 7.717
        press((150.0, 24.0));
        assert_eq!(got.load(Ordering::Relaxed), 771, "按下 x=150 → value=7.717");
    }

    // ── 防回归：非根节点（rect.left ≠ 0）时 thumb 必须相对轨道居中 ──
    // 2026-08 debug server 实测：thumb 绘制曾双重加 rect.left（value_pos 已是
    // 绝对坐标）——根节点测试（left=0）无法暴露，真实窗口（left=24）偏右 24px
    #[test]
    fn thumb_centered_when_embedded_offsets() {
        use skia_safe::{Color as SkColor, surfaces};
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = crate::runtime::composer::Composer::new();
        let scene = |ctx: &mut ComposeCtx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                // 外层容器：size(300, 300) 把 slider 推到 (50, 100)
                let k = ctx.next_key();
                ctx.start_container(k, Modifier::new().size(300.0, 300.0), crate::layout::BoxLayout::new());
                let sk = ctx.next_key();
                match ctx.start_restartable_group(
                    sk,
                    Modifier::new().offset(50.0, 100.0),
                    crate::layout::BoxLayout::new().alignment(crate::layout::Alignment::Start),
                ) {
                    crate::runtime::composer::GroupStatus::Skip => {}
                    crate::runtime::composer::GroupStatus::Enter => {
                        Slider::new(0.5)
                            .value_range(0.0, 1.0)
                            .on_value_change(|_| {})
                            .build(ctx);
                    }
                }
                ctx.end_restartable_group();
                ctx.end_node();
            });
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
        let at = |x: f32, y: f32| {
            let p = px[(y as usize) * 300 + (x as usize)];
            (p[2] as i32, p[1] as i32, p[0] as i32)
        };
        let prim = (theme.primary.r as i32, theme.primary.g as i32, theme.primary.b as i32);
        // slider fill_max_width → 宽 300，在 (50,100)；轨道中心 y=100+24=124
        // value=0.5 → thumb 中心 = 50+2+296*0.5 = 200（fix 前右偏 50 → 250）
        let thumb = at(200.0, 124.0);
        assert!(close(thumb, prim), "thumb 应居中于轨道（x=200，实际 {thumb:?}）");
        // thumb 两侧间隙（end_gap=8：thumb [198,202]，间隙 [190,197]/[203,210]）
        let gap_l = at(193.0, 124.0);
        let gap_r = at(207.0, 124.0);
        assert!(gap_l.0 > 245 && gap_l.1 > 245 && gap_l.2 > 245, "thumb 左侧应为间隙（实际 {gap_l:?}）");
        assert!(gap_r.0 > 245 && gap_r.1 > 245 && gap_r.2 > 245, "thumb 右侧应为间隙（实际 {gap_r:?}）");
    }

    // ── 防回归：拖动时 thumb 宽度减半（4→2）──
    // 手势回调（on_drag_start）必须发射 interaction.dragged——否则 thumb_active
    // 恒 false（2026-08 实测：拖动时 thumb 不变窄）
    #[test]
    fn drag_interaction_narrows_thumb() {
        use skia_safe::{Color as SkColor, surfaces};
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = crate::runtime::composer::Composer::new();
        let src = MutableInteractionSource::new();
        let scene = |ctx: &mut ComposeCtx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                Slider::new(0.5)
                    .value_range(0.0, 1.0)
                    .interaction_source(src.clone())
                    .on_value_change(|_| {})
                    .build(ctx);
            });
        };
        let render = |composer: &mut crate::runtime::composer::Composer| -> usize {
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
            // 中心行 y=24（slider 根节点）；thumb 在 148-152（value=0.5）——
            // 扫描 x=144..156 避开 active 轨道终点 142 与 inactive 起点 158
            let prim = (theme.primary.r as i32, theme.primary.g as i32, theme.primary.b as i32);
            let mut w = 0usize;
            for x in 144..156usize {
                let p = px[24 * 300 + x];
                let c = (p[2] as i32, p[1] as i32, p[0] as i32);
                if (c.0 - prim.0).abs() <= 8 && (c.1 - prim.1).abs() <= 8 && (c.2 - prim.2).abs() <= 8 { w += 1; }
            }
            w
        };
        // 静止：thumb 宽 4
        let w0 = render(&mut composer);
        assert_eq!(w0, 4, "静止时 thumb 宽应为 4（实际 {w0}）");
        // 拖动开始：dragged=true → 重组 → thumb 宽 2
        src.emit_drag_start();
        let w1 = render(&mut composer);
        assert_eq!(w1, 2, "拖动时 thumb 宽应减半为 2（实际 {w1}）——on_drag_start 必须 emit_drag_start");
        // 拖动结束：恢复 4
        src.emit_drag_end();
        let w2 = render(&mut composer);
        assert_eq!(w2, 4, "拖动结束 thumb 宽应恢复 4（实际 {w2}）");
    }

    // ── 防回归：与 thumb 重合的 stop 不显示 ──
    // steps=4、value=0.4：thumb 中心 121.6 恰在 f=0.4 的 tick 上——该 tick 不画
    //（121.6 处为轨道色 primary）；f=0.2 的 tick（64.8）不重合仍显示
    #[test]
    fn tick_under_thumb_is_hidden() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let (px, w) = render_slider_px(|ctx| {
            Slider::new(0.4)
                .value_range(0.0, 1.0)
                .steps(4)
                .on_value_change(|_| {})
                .build(ctx);
        });
        let prim = (theme.primary.r as i32, theme.primary.g as i32, theme.primary.b as i32);
        let sec = (theme.secondary_container.r as i32, theme.secondary_container.g as i32, theme.secondary_container.b as i32);
        // thumb 中心 = 8+284×0.4 = 121.6——重合 tick 不画：121.6 处是轨道色（primary）
        let under = at(&px, w, 121.6, 24.0);
        assert!(close(under, prim), "thumb 下的 tick 不应绘制（应为轨道 primary，实际 {under:?}）");
        // 不重合的 tick（f=0.2 → 64.8）仍显示（secondary_container）
        let other = at(&px, w, 64.8, 24.0);
        assert!(close(other, sec), "不重合 tick 应正常显示（secondary_container，实际 {other:?}）");
    }

    // ── 防回归：焦点环包围 thumb 胶囊（而非整个组件 rect）──
    #[test]
    fn focus_ring_wraps_thumb_not_component() {
        use skia_safe::{Color as SkColor, surfaces};
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = crate::runtime::composer::Composer::new();
        let src = MutableInteractionSource::new();
        let scene = |ctx: &mut ComposeCtx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                Slider::new(0.5)
                    .value_range(0.0, 1.0)
                    .interaction_source(src.clone())
                    .on_value_change(|_| {})
                    .build(ctx);
            });
        };
        let render = |composer: &mut crate::runtime::composer::Composer| -> Vec<[u8; 4]> {
            composer.compose(scene);
            composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
            let mut surface = surfaces::raster_n32_premul((300, 300)).unwrap();
            let canvas = surface.canvas();
            canvas.clear(SkColor::WHITE);
            let root = composer.layout_root_idx().expect("root");
            let nodes = composer.arena_nodes();
            crate::render::render(nodes, root, canvas);
            let pm = surface.peek_pixels().expect("pixmap");
            pm.pixels::<[u8; 4]>().expect("pixels").to_vec()
        };
        let at = |px: &[[u8; 4]], x: f32, y: f32| {
            let p = px[(y as usize) * 300 + (x as usize)];
            (p[2] as i32, p[1] as i32, p[0] as i32)
        };
        let prim = (theme.primary.r as i32, theme.primary.g as i32, theme.primary.b as i32);
        // 无焦点：(145,24) 是 thumb 左侧间隙（白）
        let px0 = render(&mut composer);
        let gap0 = at(&px0, 145.0, 24.0);
        assert!(gap0.0 > 245 && gap0.1 > 245 && gap0.2 > 245, "未聚焦时 thumb 左侧应为间隙（实际 {gap0:?}）");
        // 聚焦 + 推进焦点环淡入动画
        src.emit_focus();
        for _ in 0..60 {
            if !crate::animation::update_animations() { break; }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let px1 = render(&mut composer);
        // 焦点环环带中心线 = track 端头（value_pos ± end_gap = 142/158）——
        // 环带间距与两个轨道端头距离一致（16dp）
        // inactive 侧环带（中心线 158 = track 端头）画在 secondary 轨道上——primary 可见
        let ring_r = at(&px1, 158.0, 24.0);
        assert!(close(ring_r, prim), "焦点环环带应对齐 inactive 轨道端头（x=158，实际 {ring_r:?}）");
        // thumb 紧邻处（145）无环——环带已外移到轨道端头
        let near = at(&px1, 145.0, 24.0);
        assert!(near.0 > 245 && near.1 > 245 && near.2 > 245, "thumb 紧邻处不应有环带（环带在轨道端头，实际 {near:?}）");
        // 整组件焦点环会在组件顶部 y=0 全宽分布（gap2+宽3 外圈）；thumb 环只在
        // thumb 附近（x 144.5..155.5）——验证 y=0 行远离 thumb 处无环色
        let far_top = at(&px1, 50.0, 0.0);
        assert!(far_top.0 > 245 && far_top.1 > 245 && far_top.2 > 245, "y=0 远离 thumb 处不应有焦点环（整组件环会全宽分布，实际 {far_top:?}）");
        // 环顶/底水平段在组件外（sr 顶 -4，环带 -5.5..-2.5）——y=0 无环色
        let top = at(&px1, 150.0, 0.0);
        assert!(top.0 > 245 && top.1 > 245 && top.2 > 245, "y=0 不应有环（环顶段在组件外，实际 {top:?}）");
    }

    // ── exp/modifier-node 首个真实迁移验证 ──
    #[test]
    fn slider_track_node_key_covers_all_visual_params() {
        use crate::runtime::state::State;
        use crate::modifier::DrawNode;
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let colors = SliderDefaults::slider_colors(&theme);
        let mut colors2 = colors;
        colors2.thumb_color = Color::from_argb(255, 1, 2, 3);
        // 同一 interaction 源（换源单独测——每次 new 源 id 不同）
        let shared_src = MutableInteractionSource::new();
        let shared_tw = crate::runtime::state::Backchannel::new(300.0);
        #[allow(clippy::too_many_arguments)]
        let mk = |value: f32, enabled: bool, thumb_active: bool, colors: SliderColors,
                  min: f32, max: f32, steps: i32| {
            SliderTrackNode {
                track_width: shared_tw.clone(),
                interaction: shared_src.clone(),
                colors,
                enabled,
                value,
                min,
                max,
                steps,
                thumb_active,
            }
        };
        let base = mk(0.5, true, false, colors, 0.0, 1.0, 0);
        // 同参 → 相等（Skip）
        assert_eq!(base.node_key(), mk(0.5, true, false, colors, 0.0, 1.0, 0).node_key());
        // 值/开关/状态任一变化 → 不等（Enter）
        assert_ne!(base.node_key(), mk(0.6, true, false, colors, 0.0, 1.0, 0).node_key(), "value 应进 key");
        assert_ne!(base.node_key(), mk(0.5, false, false, colors, 0.0, 1.0, 0).node_key(), "enabled 应进 key");
        assert_ne!(base.node_key(), mk(0.5, true, true, colors, 0.0, 1.0, 0).node_key(), "thumb_active 应进 key");
        // P0-2 补：colors/min/max/steps 任一变化 → 不等
        assert_ne!(base.node_key(), mk(0.5, true, false, colors2, 0.0, 1.0, 0).node_key(), "colors 应进 key");
        assert_ne!(base.node_key(), mk(0.5, true, false, colors, 0.5, 1.0, 0).node_key(), "min 应进 key");
        assert_ne!(base.node_key(), mk(0.5, true, false, colors, 0.0, 2.0, 0).node_key(), "max 应进 key");
        assert_ne!(base.node_key(), mk(0.5, true, false, colors, 0.0, 1.0, 4).node_key(), "steps 应进 key");
        // P0-2 补：track_width 回写通道不进 key（值变化 → key 相等，走依赖通道）
        shared_tw.set(999.0);
        assert_eq!(
            base.node_key(), mk(0.5, true, false, colors, 0.0, 1.0, 0).node_key(),
            "track_width 回写值变化不应进 key"
        );
        // 换源 → 不等（重建绑定）
        let other = SliderTrackNode {
            track_width: crate::runtime::state::Backchannel::new(300.0),
            interaction: MutableInteractionSource::new(), // 新源 id 不同
            colors,
            enabled: true,
            value: 0.5,
            min: 0.0,
            max: 1.0,
            steps: 0,
            thumb_active: false,
        };
        assert_ne!(base.node_key(), other.node_key(), "换 interaction 源应进 key");
    }

    #[cfg(feature = "debug-server")]
    #[test]
    fn slider_track_node_visible_in_debug_tree() {
        // 调试树应含 node(slidertrack:...) 条目（具名可观测——匿名闭包无此能力）。
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = crate::runtime::composer::Composer::new();
        let scene = |ctx: &mut ComposeCtx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                Slider::new(0.5).on_value_change(|_| {}).build(ctx);
            });
        };
        composer.compose(scene);
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        let json = crate::debug::build_tree_json(nodes, root);
        assert!(
            json.contains("node(draw:slidertrack:"),
            "调试树应含 SliderTrackNode 条目，实际: {}",
            &json[..json.len().min(500)]
        );
    }
