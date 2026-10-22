    use super::*;
    use crate::input::PointerButton;
    use crate::input::PointerEventType;
    use crate::input::PointerKind;
    use crate::runtime::composer::Composer;

    /// 试点绘制节点：Background(color, shape) 的 node 等价物（第三方可照抄）。
    #[derive(Debug)]
    struct TestBgNode {
        color: Color,
        shape: Shape,
    }

    impl DrawNode for TestBgNode {
        fn draw(&self, canvas: &skia_safe::Canvas, rect: skia_safe::Rect) {
            crate::render::draw_background_for_node(canvas, rect, &self.color, &self.shape);
        }
        fn node_key(&self) -> String {
            format!("testbg:{:?}:{:?}", self.color, self.shape)
        }
    }

    /// Test wrapping node: paints `before` color at the background layer and
    /// `after` color above content (third parties copy this shape for content-
    /// overlay decor such as selection highlight or debug overlay).
    #[derive(Debug)]
    struct TestWrapNode {
        before: Color,
        after: Color,
    }

    impl DrawWrapNode for TestWrapNode {
        fn draw_before(
            &self,
            canvas: &skia_safe::Canvas,
            rect: skia_safe::Rect,
            _modifier: &Modifier,
        ) {
            crate::render::draw_background_for_node(canvas, rect, &self.before, &Shape::Rectangle);
        }
        fn draw_after(
            &self,
            canvas: &skia_safe::Canvas,
            rect: skia_safe::Rect,
            _modifier: &Modifier,
        ) {
            crate::render::draw_background_for_node(canvas, rect, &self.after, &Shape::Rectangle);
        }
        fn node_key(&self) -> String {
            format!("testwrap:{:?}:{:?}", self.before, self.after)
        }
    }

    /// Render a single leaf and read one pixel back. Returns BGRA bytes.
    fn render_leaf_pixel(modifier: Modifier, x: usize, y: usize) -> [u8; 4] {
        use skia_safe::surfaces;
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            let key = ctx.next_key();
            ctx.start_leaf(key, modifier);
            ctx.end_node();
        });
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
        let mut surface = surfaces::raster_n32_premul((300, 300)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::WHITE);
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        crate::render::render(nodes, root, canvas);
        let pm = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        let w = pm.width() as usize;
        px[y * w + x]
    }

    /// 试点点击节点：Clickable 的 node 等价物。
    #[derive(Debug)]
    struct TestClickNode {
        count: std::sync::Arc<std::sync::atomic::AtomicU32>,
    }

    impl ClickNode for TestClickNode {
        fn on_click(&self) {
            self.count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
        fn node_key(&self) -> String {
            "testclick".to_string()
        }
    }

    #[test]
    fn node_track_draw_renders_background_pixels() {
        use skia_safe::surfaces;
        let theme_color = Color::from_argb(255, 200, 30, 30);
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            let key = ctx.next_key();
            ctx.start_leaf(
                key,
                Modifier::new().size(60.0, 40.0).draw_node(TestBgNode {
                    color: theme_color,
                    shape: Shape::Rectangle,
                }),
            );
            ctx.end_node();
        });
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
        let mut surface = surfaces::raster_n32_premul((300, 300)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::WHITE);
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        crate::render::render(nodes, root, canvas);
        // 中心像素应为节点背景色（node 链绘制生效）
        let pm = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        let w = pm.width() as usize;
        let p = px[20 * w + 30]; // BGRA 内存序
        assert!(
            (p[2] as i16 - 200).abs() <= 6
                && (p[1] as i16 - 30).abs() <= 6
                && (p[0] as i16 - 30).abs() <= 6,
            "node 绘制背景应生效，实际 BGRA={:?}",
            p
        );
    }

    /// P1-1 回归：混用枚举 background + draw_node 时，node 在枚举之后绘制
    /// （与文档“枚举链走完后走 node 链”一致）。此前顺序反了会被枚举盖住。
    #[test]
    fn node_track_draw_after_enum_background() {
        use skia_safe::surfaces;
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            let key = ctx.next_key();
            ctx.start_leaf(
                key,
                Modifier::new()
                    .size(60.0, 40.0)
                    .background(Color::from_argb(255, 200, 30, 30), Shape::Rectangle)
                    .draw_node(TestBgNode {
                        color: Color::from_argb(255, 30, 30, 200),
                        shape: Shape::Rectangle,
                    }),
            );
            ctx.end_node();
        });
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
        let mut surface = surfaces::raster_n32_premul((300, 300)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::WHITE);
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        crate::render::render(nodes, root, canvas);
        let pm = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        let w = pm.width() as usize;
        let p = px[20 * w + 30];
        assert!(
            (p[0] as i16 - 200).abs() <= 6,
            "node 应盖住枚举 background（蓝色在上），实际 BGRA={:?}",
            p
        );
    }

    #[test]
    fn node_track_click_fires_without_enum() {
        let count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            let key = ctx.next_key();
            ctx.start_leaf(
                key,
                Modifier::new()
                    .size(60.0, 40.0)
                    .click_node(TestClickNode { count: count.clone() }),
            );
            ctx.end_node();
        });
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        // 枚举 on_click 应为 None（试点未 push 枚举），node_click 应命中
        assert!(nodes[root].modifier.on_click().is_none());
        let cb = nodes[root]
            .modifier
            .node_click()
            .expect("node_click 应命中");
        cb.on_click();
        cb.on_click();
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[test]
    fn node_track_param_eq_detects_key_change() {
        let a = Modifier::new().draw_node(TestBgNode {
            color: Color::RED,
            shape: Shape::Rectangle,
        });
        let b = Modifier::new().draw_node(TestBgNode {
            color: Color::RED,
            shape: Shape::Rectangle,
        });
        let c = Modifier::new().draw_node(TestBgNode {
            color: Color::BLUE,
            shape: Shape::Rectangle,
        });
        assert!(a.param_eq(&b), "同参 node 应相等（Skip）");
        assert!(!a.param_eq(&c), "颜色变化 node_key 应不等（Enter）");
        // 枚举 vs node 数量不等 → 不等
        let d = Modifier::new().background(Color::RED, Shape::Rectangle);
        assert!(!a.param_eq(&d), "node 轨道与枚举轨道不等长应不等");
    }

    #[test]
    fn wrap_before_paints_at_background_layer() {
        // before half shares the DrawNode background-layer slot: enum blue bg first,
        // then before paints red on top. Uses a before-only node so the after half
        // (default no-op) does not cover it. Center pixel must be red.
        #[derive(Debug)]
        struct BeforeOnly {
            color: Color,
        }
        impl DrawWrapNode for BeforeOnly {
            fn draw_before(
                &self,
                canvas: &skia_safe::Canvas,
                rect: skia_safe::Rect,
                _modifier: &Modifier,
            ) {
                crate::render::draw_background_for_node(canvas, rect, &self.color, &Shape::Rectangle);
            }
            fn node_key(&self) -> String {
                format!("beforeonly:{:?}", self.color)
            }
        }
        let red = Color::from_argb(255, 200, 30, 30);
        let blue = Color::from_argb(255, 30, 30, 200);
        let p = render_leaf_pixel(
            Modifier::new()
                .size(60.0, 40.0)
                .background(blue, Shape::Rectangle)
                .draw_wrap_node(BeforeOnly { color: red }),
            30,
            20,
        );
        assert!(
            (p[2] as i16 - 200).abs() <= 6 && (p[1] as i16 - 30).abs() <= 6,
            "before should cover enum background (red on top), got BGRA={:?}",
            p
        );
    }

    #[test]
    fn wrap_after_covers_background_layer() {
        // after half runs above content: enum red bg first, before paints blue, then
        // after paints opaque yellow on top. Center pixel must be yellow, proving the
        // after slot sits above the background-layer paints.
        let red = Color::from_argb(255, 200, 30, 30);
        let blue = Color::from_argb(255, 30, 30, 200);
        let yellow = Color::from_argb(255, 220, 200, 30);
        let p = render_leaf_pixel(
            Modifier::new()
                .size(60.0, 40.0)
                .background(red, Shape::Rectangle)
                .draw_wrap_node(TestWrapNode { before: blue, after: yellow }),
            30,
            20,
        );
        assert!(
            (p[2] as i16 - 220).abs() <= 8
                && (p[1] as i16 - 200).abs() <= 8
                && (p[0] as i16 - 30).abs() <= 8,
            "after should cover everything below (yellow on top), got BGRA={:?}",
            p
        );
    }

    #[test]
    fn wrap_after_covers_child_text() {
        // Strictest proof of "above content": parent holds the wrap node, child leaf
        // holds black text; parent after paints an opaque overlay rect. Sampled pixels
        // inside the overlay must show the overlay color, not text-darkened pixels —
        // i.e. after runs after the children recursion in render_pass1.
        use crate::runtime::composer::Composer;
        use skia_safe::surfaces;
        let overlay = Color::from_argb(255, 30, 200, 30);
        #[derive(Debug)]
        struct AfterOnly {
            color: Color,
        }
        impl DrawWrapNode for AfterOnly {
            fn draw_after(
                &self,
                canvas: &skia_safe::Canvas,
                rect: skia_safe::Rect,
                _modifier: &Modifier,
            ) {
                // Cover the top text rows with an opaque bar.
                let bar = skia_safe::Rect::new(
                    rect.left,
                    rect.top,
                    rect.right,
                    rect.top + 24.0,
                );
                crate::render::draw_background_for_node(canvas, bar, &self.color, &Shape::Rectangle);
            }
            fn node_key(&self) -> String {
                format!("afteronly:{:?}", self.color)
            }
        }
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            let parent = ctx.next_key();
            ctx.start_container(
                parent,
                Modifier::new()
                    .size(200.0, 60.0)
                    .background(Color::WHITE, Shape::Rectangle)
                    .draw_wrap_node(AfterOnly { color: overlay }),
                crate::layout::BoxLayout::new(),
            );
            let child = ctx.next_key();
            ctx.start_leaf(
                child,
                Modifier::new().size(200.0, 60.0).text_content(
                    "CoverMe".to_string(),
                    14.0,
                    Color::BLACK,
                    crate::text::FontWeight::NORMAL,
                    crate::text::FontSlant::Upright,
                    usize::MAX,
                    crate::text::TextAlign::Left,
                    crate::text::TextOverflow::Clip,
                    true,
                ),
            );
            ctx.end_node();
            ctx.end_node();
        });
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
        let mut surface = surfaces::raster_n32_premul((300, 300)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::WHITE);
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        crate::render::render(nodes, root, canvas);
        let pm = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        let w = pm.width() as usize;
        // Scan the overlay bar rows for any dark (text) pixel: none should survive.
        let mut dark = 0;
        for y in 2..22 {
            for x in (2..198).step_by(2) {
                let p = px[y * w + x];
                if p[0] < 120 && p[1] < 120 && p[2] < 120 {
                    dark += 1;
                }
            }
        }
        assert_eq!(dark, 0, "after overlay must cover child text (no dark pixels in bar)");
    }

    #[test]
    fn wrap_node_key_drives_skip() {
        // node_key covers both halves: same params Skip, either half changed Enters.
        // The render pipeline keys DrawWrap via node_key_of "draw_wrap:" prefix.
        let mk = |b: Color, a: Color| {
            Modifier::new().draw_wrap_node(TestWrapNode { before: b, after: a })
        };
        let red = Color::from_argb(255, 200, 30, 30);
        let blue = Color::from_argb(255, 30, 30, 200);
        assert!(mk(red, blue).param_eq(&mk(red, blue)), "same wrap params Skip");
        assert!(!mk(red, blue).param_eq(&mk(blue, blue)), "before change Enters");
        assert!(!mk(red, blue).param_eq(&mk(red, red)), "after change Enters");
        assert_eq!(
            node_key_of(&Modifier::new().draw_wrap_node(TestWrapNode { before: red, after: blue }).modifier_nodes()[0]),
            format!("draw_wrap:testwrap:{:?}:{:?}", red, blue),
            "debug/tree key prefix"
        );
    }

    #[test]
    fn wrap_after_covers_enum_ripple() {
        // Proves the after slot sits above the enum Ripple path (the exact position a
        // future RippleNode migration needs): enum ripple pressed to mid-expand, after
        // paints an opaque bar over the press point. The press pixel must show the bar
        // color, not the ripple color.
        use crate::runtime::composer::Composer;
        use crate::interaction::MutableInteractionSource;
        use skia_safe::surfaces;
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();
        let source = MutableInteractionSource::new();
        let ripple_color = Color::from_argb(255, 200, 30, 30);
        let bar_color = Color::from_argb(255, 30, 200, 30);
        #[derive(Debug)]
        struct AfterBar {
            color: Color,
        }
        impl DrawWrapNode for AfterBar {
            fn draw_after(
                &self,
                canvas: &skia_safe::Canvas,
                rect: skia_safe::Rect,
                _modifier: &Modifier,
            ) {
                // Opaque bar over the press point (30, 20).
                let bar = skia_safe::Rect::new(20.0, 10.0, 40.0, 30.0);
                let _ = rect;
                crate::render::draw_background_for_node(canvas, bar, &self.color, &Shape::Rectangle);
            }
            fn node_key(&self) -> String {
                format!("afterbar:{:?}", self.color)
            }
        }
        let src = source.clone();
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            let key = ctx.next_key();
            ctx.start_leaf(
                key,
                Modifier::new()
                    .size(60.0, 40.0)
                    .background(Color::WHITE, Shape::Rectangle)
                    .ripple(&src, ripple_color, true)
                    .draw_wrap_node(AfterBar { color: bar_color }),
            );
            ctx.end_node();
        });
        // Press at (30, 20), then force mid-expand so the ripple surely paints
        // (fresh push_animatable only runs its first update — radius ~0).
        source.emit_press_at((30.0, 20.0));
        source.ripple_layers()[0].progress.set(0.5);
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
        let mut surface = surfaces::raster_n32_premul((300, 300)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::WHITE);
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        crate::render::render(nodes, root, canvas);
        crate::animation::clear_all_animations();
        let pm = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        let w = pm.width() as usize;
        let p = px[20 * w + 30];
        assert!(
            (p[1] as i16 - 200).abs() <= 8
                && (p[2] as i16 - 30).abs() <= 8
                && (p[0] as i16 - 30).abs() <= 8,
            "after bar must cover the enum ripple at press point (green), got BGRA={:?}",
            p
        );
        // Counter-proof (no false positive): same tree without the wrap node must show
        // the ripple color at the press point — i.e. the ripple really paints there,
        // so the green above genuinely covers it rather than covering nothing.
        crate::animation::clear_all_animations();
        let source2 = MutableInteractionSource::new();
        let src2 = source2.clone();
        let mut composer2 = Composer::new();
        composer2.compose(|ctx| {
            let key = ctx.next_key();
            ctx.start_leaf(
                key,
                Modifier::new()
                    .size(60.0, 40.0)
                    .background(Color::WHITE, Shape::Rectangle)
                    .ripple(&src2, ripple_color, true),
            );
            ctx.end_node();
        });
        source2.emit_press_at((30.0, 20.0));
        source2.ripple_layers()[0].progress.set(0.5);
        composer2.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
        let mut surface2 = surfaces::raster_n32_premul((300, 300)).unwrap();
        let canvas2 = surface2.canvas();
        canvas2.clear(skia_safe::Color::WHITE);
        let root2 = composer2.layout_root_idx().expect("root");
        let nodes2 = composer2.arena_nodes();
        crate::render::render(nodes2, root2, canvas2);
        crate::animation::clear_all_animations();
        let pm2 = surface2.peek_pixels().expect("pixmap");
        let px2: &[[u8; 4]] = pm2.pixels::<[u8; 4]>().expect("pixels");
        let q = px2[20 * w + 30];
        // Ripple red (200,30,30) at 0.10 opacity over white: R ≈ 200*0.1+255*0.9 = 249,
        // G/B ≈ 30*0.1+255*0.9 = 232. Assert reddish tint, distinct from both white
        // and the green bar.
        assert!(
            q[2] > q[1] + 5 && q[2] > q[0] + 5,
            "counter-proof: ripple must paint reddish at press point without wrap, got BGRA={:?}",
            q
        );
    }

    #[test]
    fn node_track_then_merges_both_tracks() {
        let m = Modifier::new()
            .size(10.0, 10.0)
            .draw_node(TestBgNode { color: Color::RED, shape: Shape::Rectangle })
            .then(Modifier::new().click_node(TestClickNode {
                count: Default::default(),
            }));
        assert_eq!(m.modifier_nodes().len(), 2, "then 应合并 node 轨道");
        assert_eq!(m.elements().len(), 1, "枚举轨道不受影响");
    }

    /// 试点指针节点：记录隧道/冒泡调用（第三方手势识别照此形状实现 PointerNode）。
    #[derive(Debug)]
    struct TestPointerNode {
        pre_log: std::sync::Arc<std::sync::Mutex<Vec<(f32, f32)>>>,
        event_log: std::sync::Arc<std::sync::Mutex<Vec<(f32, f32)>>>,
        consume: bool,
    }

    impl PointerNode for TestPointerNode {
        fn on_pre(&self, ev: &PointerEvent) -> bool {
            self.pre_log.lock().unwrap().push(ev.position);
            self.consume
        }
        fn on_event(&self, ev: &PointerEvent) -> bool {
            self.event_log.lock().unwrap().push(ev.position);
            self.consume
        }
        fn node_key(&self) -> String {
            format!("testptr:{}", self.consume)
        }
    }

    #[test]
    fn node_track_pointer_bubble_receives_local_coords() {
        use crate::layout::node::LayoutNode;
        use crate::unit::{Offset, Size};
        let pre_log = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let event_log = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut nodes = vec![LayoutNode::leaf(
            Modifier::new().size(100.0, 100.0).pointer_node(TestPointerNode {
                pre_log: pre_log.clone(),
                event_log: event_log.clone(),
                consume: false,
            }),
        )];
        nodes[0].measured_size = Size::new(100.0, 100.0);
        nodes[0].position = Offset::new(10.0, 20.0);
        let path = vec![0];
        let ev = PointerEvent {
            event_type: PointerEventType::Move,
            position: (0.0, 0.0),
            scene_position: (50.0, 60.0),
            kind: PointerKind::Mouse { button: PointerButton::Primary },
            is_alt_pressed: false,
            is_ctrl_pressed: false,
            is_shift_pressed: false,
            is_meta_pressed: false,
        };
        // 直接走 app.rs 的分发（同模块外不可见——此处测 Modifier 侧装配；
        // 分发语义由 app.rs pointer_dispatch_coord_tests 风格覆盖，见下）。
        assert_eq!(nodes[0].modifier.pointer_nodes().count(), 1);
        // P0-1 回归：PointerNode 不参与 has_gesture（装饰性 node 不偷外层 tap）
        assert!(
            !nodes[0].modifier.has_gesture(),
            "纯 PointerNode 不应进手势路由（P0-1：偷 tap）"
        );
        assert!(
            !nodes[0].modifier.has_drag_gesture(),
            "纯 PointerNode 不应被当拖拽手势"
        );
        // node_key 指纹：consume 变化 → Enter
        let a = Modifier::new().pointer_node(TestPointerNode {
            pre_log: pre_log.clone(),
            event_log: event_log.clone(),
            consume: false,
        });
        let b = Modifier::new().pointer_node(TestPointerNode {
            pre_log: pre_log.clone(),
            event_log: event_log.clone(),
            consume: true,
        });
        assert!(!a.param_eq(&b), "pointer node_key 变化应 Enter");
        let _ = (path, ev);
    }

    /// 试点键盘节点：记录隧道/冒泡调用（第三方快捷键/输入拦截照此形状实现 KeyNode）。
    #[derive(Debug)]
    struct TestKeyNode {
        pre_log: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
        event_log: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
        consume: bool,
    }

    impl KeyNode for TestKeyNode {
        fn on_pre(&self, ev: &KbEvent) -> bool {
            self.pre_log.lock().unwrap().push(format!("{:?}", ev.key));
            self.consume
        }
        fn on_event(&self, ev: &KbEvent) -> bool {
            self.event_log.lock().unwrap().push(format!("{:?}", ev.key));
            self.consume
        }
        fn node_key(&self) -> String {
            format!("testkey:{}", self.consume)
        }
    }

    #[test]
    fn node_track_key_assembly_and_fingerprint() {
        let pre_log = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let event_log = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let m = Modifier::new().key_node(TestKeyNode {
            pre_log: pre_log.clone(),
            event_log: event_log.clone(),
            consume: false,
        });
        assert_eq!(m.key_nodes().count(), 1);
        assert_eq!(m.modifier_nodes().len(), 1);
        // node_key 指纹：consume 变化 → Enter
        let a = Modifier::new().key_node(TestKeyNode {
            pre_log: pre_log.clone(),
            event_log: event_log.clone(),
            consume: false,
        });
        let b = Modifier::new().key_node(TestKeyNode {
            pre_log: pre_log.clone(),
            event_log: event_log.clone(),
            consume: true,
        });
        assert!(!a.param_eq(&b), "key node_key 变化应 Enter");
        assert!(a.param_eq(&a.clone()), "同参 key node 应相等");
    }

    /// 试点布局节点 A 型：min-width 提升（MinWidth 枚举的 node 等价物）。
    /// 第三方自定义约束（宽高比钳制/内容相关约束等）照此形状实现 LayoutNode。
    #[derive(Debug)]
    struct TestMinWidthNode {
        min_w: f32,
    }

    impl LayoutModifierNode for TestMinWidthNode {
        fn transform(&self, mut inner: crate::layout::Constraints) -> crate::layout::Constraints {
            inner.min_width = inner.min_width.max(self.min_w).min(inner.max_width);
            inner
        }
        fn node_key(&self) -> String {
            format!("testminw:{}", self.min_w)
        }
    }

    #[test]
    fn node_track_layout_transform_applies_and_folds() {
        use crate::runtime::composer::Composer;
        // 约束 max 400：node 提 min_w=200 → 叶子宽应为 200（tighten 生效）
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            let key = ctx.next_key();
            ctx.start_leaf(
                key,
                Modifier::new().layout_node(TestMinWidthNode { min_w: 200.0 }),
            );
            ctx.end_node();
        });
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        let root = composer.layout_root_idx().expect("root");
        let w = composer.arena_nodes()[root].measured_size.width;
        assert_eq!(w, 200.0, "LayoutNode transform 应提升 min_width 生效");

        // 常量折叠：同约束 re-layout 不应重测（node 不破坏折叠语义）
        #[cfg(test)]
        let before = {
            crate::layout::node::MEASURE_COUNT.with(|c| c.get())
        };
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        #[cfg(test)]
        {
            let after = crate::layout::node::MEASURE_COUNT.with(|c| c.get());
            assert_eq!(after, before, "同约束同 node 应命中常量折叠");
        }

        // 指纹：min_w 变化 → param_eq 不等 → Enter
        let a = Modifier::new().layout_node(TestMinWidthNode { min_w: 200.0 });
        let b = Modifier::new().layout_node(TestMinWidthNode { min_w: 300.0 });
        assert!(!a.param_eq(&b), "layout node_key 变化应 Enter");
        assert!(a.param_eq(&a.clone()), "同参 layout node 应相等");
    }

    #[test]
    fn node_track_layout_node_state_driven_remeasures() {
        use crate::runtime::composer::Composer;
        use crate::runtime::state::State;
        // 动态值在 transform 内 get → 注册布局依赖 → set 后重测（与 Dimension::Dynamic 同）
        #[derive(Debug)]
        struct DynMinNode {
            s: State<f32>,
        }
        impl LayoutModifierNode for DynMinNode {
            fn transform(&self, mut inner: crate::layout::Constraints) -> crate::layout::Constraints {
                let v = self.s.get();
                inner.min_width = inner.min_width.max(v).min(inner.max_width);
                inner
            }
            fn node_key(&self) -> String {
                "dynmin".to_string() // 值走 State，不进 key（同 Dynamic 语义）
            }
        }
        let s = State::new(100.0f32);
        let mut composer = Composer::new();
        let s2 = s.clone();
        composer.compose(|ctx| {
            let key = ctx.next_key();
            ctx.start_leaf(key, Modifier::new().layout_node(DynMinNode { s: s2.clone() }));
            ctx.end_node();
        });
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        let root = composer.layout_root_idx().expect("root");
        assert_eq!(composer.arena_nodes()[root].measured_size.width, 100.0);
        // set 后：pending 走 layout 通道 → layout() 直接重测（无需 compose）
        s.set(250.0);
        assert!(composer.has_pending_states(), "set 后应有 pending");
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        assert_eq!(
            composer.arena_nodes()[root].measured_size.width, 250.0,
            "State 变化应经布局依赖重测（无需重组）"
        );
    }

    /// The stateful-draw-node contract: a DrawNode holds a `remember`ed State (an Arc clone) and
    /// evaluates it at paint time - the same rule `Background`'s colour follows. The value change that
    /// repaints comes from an external `Visual`/notification, or from a build-time `get()` when the
    /// node was created.
    ///
    /// The lesson (measured): by render time the dependency frame is gone
    /// (`take_deps`/render_pass1 leaves `DEP_MODE = None`), so a `get()` here registers nothing -
    /// exactly like a dynamic background. Repaint is driven by a build-time `get()` or by the
    /// animation engine writing through `Animating` and requesting a redraw.
    #[derive(Debug)]
    struct TestStatefulBgNode {
        color_state: crate::runtime::state::State<Color>,
    }

    impl DrawNode for TestStatefulBgNode {
        fn draw(&self, canvas: &skia_safe::Canvas, rect: skia_safe::Rect) {
            // Evaluated at paint time (the same rule as `Background`): `peek()` states the intent -
            // a `get()` here would be inert, because there is no dependency frame outside a pass.
            let c = self.color_state.peek();
            crate::render::draw_background_for_node(canvas, rect, &c, &Shape::Rectangle);
        }
        fn node_key(&self) -> String {
            // The value rides the State, not the key (the same rule as `Dimension::Dynamic` or a
            // dynamic background: a rebuilt closure does not force Enter; a value change goes through
            // the dependency or a repaint).
            "teststatefulbg".to_string()
        }
    }

    #[test]
    fn node_track_stateful_draw_follows_state() {
        use crate::runtime::state::State;
        use skia_safe::surfaces;
        let red = Color::from_argb(255, 200, 30, 30);
        let blue = Color::from_argb(255, 30, 30, 200);
        let color_state = State::new(red);
        let mut composer = Composer::new();
        let render_once = |composer: &mut Composer| -> [u8; 4] {
            composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
            let mut surface = surfaces::raster_n32_premul((300, 300)).unwrap();
            let canvas = surface.canvas();
            canvas.clear(skia_safe::Color::WHITE);
            let root = composer.layout_root_idx().expect("root");
            let nodes = composer.arena_nodes();
            crate::render::render(nodes, root, canvas);
            let pm = surface.peek_pixels().expect("pixmap");
            let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
            let w = pm.width() as usize;
            px[20 * w + 30]
        };
        // 首帧：State::new 即 ownerless，clone 进 node——与
        // ctx.remember(...).get() 同 Arc 语义（remember 只是跨帧持久化包装）。
        let cs = color_state.clone();
        composer.compose(|ctx| {
            let key = ctx.next_key();
            // build 期 get 注册 compose 依赖（同 Background 动画闭包在 build 期
            // 捕获 State 的用法——值变化 → 重组 → 新闭包/node → 重绘）。
            let _dep = cs.get();
            ctx.start_leaf(
                key,
                Modifier::new()
                    .size(60.0, 40.0)
                    .draw_node(TestStatefulBgNode { color_state: cs.clone() }),
            );
            ctx.end_node();
        });
        let p1 = render_once(&mut composer);
        assert!(
            (p1[2] as i16 - 200).abs() <= 6,
            "首帧应为红色，实际 BGRA={:?}",
            p1
        );
        // set 后重组（build 期 get 已注册 compose 依赖）→ 新值渲染
        color_state.set(blue);
        assert!(composer.has_pending_states(), "build 期 get 应注册 compose 依赖");
        let cs2 = color_state.clone();
        composer.recompose(|ctx| {
            let key = ctx.next_key();
            let _dep = cs2.get();
            ctx.start_leaf(
                key,
                Modifier::new()
                    .size(60.0, 40.0)
                    .draw_node(TestStatefulBgNode { color_state: cs2.clone() }),
            );
            ctx.end_node();
        });
        let p2 = render_once(&mut composer);
        assert!(
            (p2[0] as i16 - 200).abs() <= 6,
            "set 后重组应为蓝色，实际 BGRA={:?}",
            p2
        );
        // Skip 语义：node_key 与 State 值无关 → 同 key 跨帧 Skip 不误触发 Enter
        let a = Modifier::new().draw_node(TestStatefulBgNode { color_state: color_state.clone() });
        let b = Modifier::new().draw_node(TestStatefulBgNode { color_state: color_state.clone() });
        assert!(a.param_eq(&b), "有状态 node 的 key 与值无关（值走依赖通道）");
    }

    /// `z_index` is an ordinary modifier element: it reads back, and it takes part in the equality the
    /// composer uses to decide whether a group may be skipped — a changed z is a changed modifier.
    #[test]
    fn z_index_reads_back_and_affects_equality() {
        let raised = Modifier::new().size(10.0, 10.0).z_index(1.0);
        assert_eq!(raised.get_z_index(), Some(1.0));
        assert_eq!(Modifier::new().get_z_index(), None, "no element means tree order");
        // `param_eq` is the comparison the composer makes to decide Skip.
        assert!(raised.param_eq(&Modifier::new().size(10.0, 10.0).z_index(1.0)));
        assert!(!raised.param_eq(&Modifier::new().size(10.0, 10.0).z_index(0.0)), "a changed z is a changed modifier");
        assert!(!raised.param_eq(&Modifier::new().size(10.0, 10.0)));
        // The element survives the Debug path (the render trace prints modifiers).
        assert!(format!("{raised:?}").contains("ZIndex"));
    }

    /// The layout pass records whether any child sets a z, so the renderer and the hit test can keep
    /// their plain tree-order loops for every node that does not use the feature.
    #[test]
    fn children_have_z_is_recorded_by_the_layout_pass() {
        use crate::runtime::composer::{Composer, GroupStatus};
        use crate::layout::BoxLayout;
        use crate::layout::constraints::Constraints;

        let layout_root_flag = |raise: bool| {
            let mut composer = Composer::new();
            composer.compose(|ctx| {
                let key = ctx.next_key();
                match ctx.start_restartable_group(key, Modifier::new().size(100.0, 100.0), BoxLayout::new()) {
                    GroupStatus::Skip => {}
                    GroupStatus::Enter => {
                        for i in 0..2 {
                            let child_key = ctx.next_key();
                            let mut m = Modifier::new().size(50.0, 50.0);
                            if raise && i == 1 {
                                m = m.z_index(1.0);
                            }
                            ctx.start_leaf(child_key, m);
                            ctx.end_node();
                        }
                    }
                }
                ctx.end_restartable_group();
            });
            composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
            let root = composer.layout_root_idx().expect("root");
            composer.arena_nodes()[root].children_have_z
        };

        assert!(!layout_root_flag(false), "no child sets a z → the fast path stays available");
        assert!(layout_root_flag(true), "a child sets a z → the passes must order");
    }
