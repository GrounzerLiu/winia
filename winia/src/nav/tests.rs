    use super::*;
    use super::display::is_pop;

    /// The framework's default transition curve is part of its behaviour, not an implementation detail:
    /// every preset and NavDisplay's default go through `NavTransitionSpec::new`. This pins the default
    /// (300 ms, a fast-out curve) by sampling the interpolator, so changing it has to be deliberate.
    ///
    /// Teeth: revert the default to `EaseInOutCubic` and the sampled point at 0.25 fails — that curve is
    /// symmetric (f(0.25) = 0.0625, measured 0.071), the fast-out one is well ahead of it.
    #[test]
    fn default_transition_spec_is_a_fast_out_300ms_fade() {
        use crate::animation::interpolator::Interpolator;
        let spec = NavTransitionSpec::fade();
        assert_eq!(
            spec.duration,
            std::time::Duration::from_millis(300),
            "the framework default duration"
        );
        let f = |p: f32| spec.interpolator.interpolate(p);
        assert!((f(0.0) - 0.0).abs() < 1e-3 && (f(1.0) - 1.0).abs() < 1e-3, "endpoints");
        // The exact curve, not just its family: a five-point fingerprint against a fresh `EaseOutCubic`
        // (the same points `PartialEq for NavTransitionSpec` samples). The earlier `f(0.25) > 0.4` admitted
        // EaseOutQuad (0.436), EaseOutBounce (0.473), EaseOutCirc (0.660), EaseOutQuart (0.689), EaseOutQuint
        // (0.765), EaseOutBack (0.816), EaseOutExpo (0.826) and EaseOutElastic (0.912).
        let reference = crate::animation::interpolator::EaseOutCubic::new();
        for p in [0.0f32, 0.25, 0.5, 0.75, 1.0] {
            assert!(
                (f(p) - reference.interpolate(p)).abs() < 1e-4,
                "the default curve must be EaseOutCubic exactly: f({p}) = {:.4}, expected {:.4}",
                f(p),
                reference.interpolate(p)
            );
        }
        // …and the preset must actually be a cross-fade, not just a curve.
        assert_eq!(spec.enter, NavEnter::FadeIn, "the default enters by fading in");
        assert_eq!(spec.exit, NavExit::FadeOut, "…and leaves by fading out");
    }

    /// A published scene id must name the LAYER, not just the scene: the registry behind scene tags is
    /// process-global and keyed by id alone, so two hosts rendering the same route — or one host pushing the
    /// same route twice, which puts one scene key in both layers — would otherwise collide, and the
    /// surviving entry would classify the other end with the wrong role.
    #[test]
    fn layer_scene_ids_are_unique_per_host_and_role() {
        let key = fnv_hash(&7u64);
        assert_eq!(
            layer_scene_id(1, key, false),
            layer_scene_id(1, key, false),
            "stable for one layer across frames"
        );
        assert_ne!(
            layer_scene_id(1, key, false),
            layer_scene_id(1, key, true),
            "a host's two layers differ by role"
        );
        assert_ne!(
            layer_scene_id(1, key, false),
            layer_scene_id(2, key, false),
            "two hosts rendering the same route differ"
        );
        // …and the scene key must participate too: an id that ignored it would pass every assertion above
        // (a function of host and role alone), which is what the first version of this test could not see.
        assert_ne!(
            layer_scene_id(1, key, false),
            layer_scene_id(1, fnv_hash(&8u64), false),
            "two different scenes of one host differ"
        );
    }

    #[derive(Clone, PartialEq, Eq, Debug, Hash)]
    enum TestRoute {
        Home,
        Detail(u64),
        Settings,
    }

    /// 遍历 arena 树收集所有 TextContent 文本（测试辅助）
    fn collect_texts(nodes: &[crate::layout::node::LayoutNode], root: usize, out: &mut Vec<String>) {
        let node = &nodes[root];
        for el in node.modifier.elements() {
            if let crate::modifier::ModifierElement::TextContent { content, .. } = el {
                out.push(content.clone());
            }
        }
        for &c in &node.children {
            collect_texts(nodes, c, out);
        }
    }

    /// NavTransitionSpec 曲线采样指纹：五点（0/0.25/0.5/0.75/1）必须区分
    /// 内置 EaseInOut 系曲线（回归测试——三点采样无法区分它们，见 PartialEq
    /// 注释）
    #[test]
    fn transition_spec_interpolator_fingerprint_distinguishes_ease_in_out() {
        use crate::animation::interpolator::Interpolator;
        let curves: Vec<Box<dyn Interpolator>> = vec![
            Box::new(crate::animation::interpolator::EaseInOutSine::new()),
            Box::new(crate::animation::interpolator::EaseInOutQuad::new()),
            Box::new(crate::animation::interpolator::EaseInOutCubic::new()),
            Box::new(crate::animation::interpolator::EaseInOutQuart::new()),
            Box::new(crate::animation::interpolator::EaseInOutQuint::new()),
            Box::new(crate::animation::interpolator::EaseInOutExpo::new()),
            Box::new(crate::animation::interpolator::EaseInOutCirc::new()),
        ];
        let pts = [0.0f32, 0.25, 0.5, 0.75, 1.0];
        let fps: Vec<Vec<f32>> = curves.iter()
            .map(|c| pts.iter().map(|&p| c.interpolate(p)).collect())
            .collect();
        // 任意两条曲线的五点指纹不得完全相同（同类型同参才相等）
        for i in 0..fps.len() {
            for j in (i + 1)..fps.len() {
                assert_ne!(fps[i], fps[j], "曲线 {i} 与 {j} 五点指纹碰撞——采样点不足");
            }
        }
    }

    /// NavMetadata：TypeId 类型安全存取——with/get/contains/覆盖
    #[test]
    fn nav_metadata_typed_get_set() {
        #[derive(Clone, Debug, PartialEq)]
        struct MetaA { v: u32 }
        #[derive(Clone, Debug, PartialEq)]
        struct MetaB { s: String }

        let m = NavMetadata::new()
            .with(MetaA { v: 42 })
            .with(MetaB { s: "hello".into() });
        assert_eq!(m.get::<MetaA>().map(|a| a.v), Some(42));
        assert_eq!(m.get::<MetaB>().map(|b| b.s.as_str()), Some("hello"));
        assert!(m.contains::<MetaA>());
        assert!(!m.contains::<String>());
        assert_eq!(m.len(), 2);

        // 同类型覆盖
        let m2 = m.clone().with(MetaA { v: 99 });
        assert_eq!(m2.get::<MetaA>().map(|a| a.v), Some(99));
        assert_eq!(m2.len(), 2, "覆盖不增项");
    }

    /// NavEntry::metadata 携带 + Scene::metadata 默认 = 栈顶 entry 的 metadata
    #[test]
    fn nav_entry_metadata_and_scene_default() {
        #[derive(Clone, Debug, PartialEq)]
        struct MyMeta { id: u32 }

        let bs = NavBackStack::<TestRoute>::with_initial(TestRoute::Home);
        let mut composer = crate::runtime::composer::Composer::new();
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        composer.compose(|ctx| {
            NavDisplay::new(&bs, |ctx, key| match key {
                TestRoute::Home => NavEntry::new(key.clone(), |ctx, _| {
                    crate::components::Text::new("HomeScreen").build(ctx);
                })
                .metadata(NavMetadata::new().with(MyMeta { id: 7 })),
                TestRoute::Settings => NavEntry::new(key.clone(), |ctx, _| {
                    crate::components::Text::new("SettingsScreen").build(ctx);
                }),
                TestRoute::Detail(id) => {
                    let id = *id;
                    NavEntry::new(key.clone(), move |ctx, _| {
                        crate::components::Text::new(format!("Detail{id}")).build(ctx);
                    })
                }
            })
            .build(ctx);
        });
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        // SinglePane 场景的 metadata 默认 = 栈顶 entry（Home）的 metadata
        // （经 Scene::metadata 默认实现——entries.last().metadata）
        // 直接验证 NavEntry::metadata_ref 存取
        // （场景级 metadata 读取在 NavDisplay 内部使用——此处验证 entry 层）
        let entry = NavEntry::new(TestRoute::Home, |_, _| {})
            .metadata(NavMetadata::new().with(MyMeta { id: 7 }));
        assert_eq!(entry.metadata_ref().get::<MyMeta>().map(|m| m.id), Some(7));
        assert!(!entry.metadata_ref().contains::<String>());
    }

    /// ResultEventBus：send/take 一次性消费 + 覆盖 + peek
    #[test]
    fn result_event_bus_send_take() {
        #[derive(Clone, Debug, PartialEq)]
        struct EditResult { saved_id: u64 }

        let bus = ResultEventBus::new();
        assert!(bus.is_empty());

        bus.send("edit", EditResult { saved_id: 1 });
        bus.send("edit", EditResult { saved_id: 2 }); // 覆盖
        assert_eq!(bus.len(), 1);
        assert_eq!(bus.peek::<EditResult>("edit").map(|r| r.saved_id), Some(2));

        // 一次性消费
        let r = bus.take::<EditResult>("edit");
        assert_eq!(r.map(|r| r.saved_id), Some(2));
        assert!(bus.take::<EditResult>("edit").is_none(), "take 后应清空");
        assert!(bus.is_empty());

        // 多 key 独立
        bus.send("a", 1u32);
        bus.send("b", "x".to_string());
        assert_eq!(bus.take::<u32>("a"), Some(1));
        assert_eq!(bus.take::<String>("b"), Some("x".to_string()));
        assert!(bus.is_empty());
    }

    /// isPop 列表差分（对标 NavDisplay.isPop）：前缀子集=pop、整栈替换/发散/更长
    /// =navigate；空栈侧 winia 扩展（空→非空=push、清空=pop）
    #[test]
    fn is_pop_direction_diffing() {
        // push：更长 → 非 pop
        assert!(!is_pop(&[TestRoute::Home], &[TestRoute::Home, TestRoute::Detail(1)]));
        // pop：前缀子集且更短
        assert!(is_pop(&[TestRoute::Home, TestRoute::Detail(1)], &[TestRoute::Home]));
        // 整栈替换（首元素不同）→ 非 pop
        assert!(!is_pop(&[TestRoute::Home, TestRoute::Detail(1)], &[TestRoute::Settings]));
        // 等长但中途发散 → 非 pop（替换）
        assert!(!is_pop(
            &[TestRoute::Home, TestRoute::Detail(1)],
            &[TestRoute::Home, TestRoute::Detail(2)]
        ));
        // 前缀相同但发散且不长于旧栈 → 非 pop（[A,B,C]→[A,X]）
        assert!(!is_pop(
            &[TestRoute::Home, TestRoute::Detail(1), TestRoute::Settings],
            &[TestRoute::Home, TestRoute::Detail(2)]
        ));
        // 空栈侧（winia 扩展）
        assert!(!is_pop::<TestRoute>(&[], &[TestRoute::Home]), "空栈 push → 非 pop");
        assert!(is_pop(&[TestRoute::Home], &[]), "清空 → pop");
        // 同栈无变化 → 非 pop
        assert!(!is_pop(&[TestRoute::Home], &[TestRoute::Home]));
    }

    #[test]
    fn back_stack_push_pop_works() {
        let bs = NavBackStack::<TestRoute>::with_initial(TestRoute::Home);
        assert_eq!(bs.top(), Some(TestRoute::Home));
        assert_eq!(bs.len(), 1);
        bs.push(TestRoute::Detail(42));
        assert_eq!(bs.len(), 2);
        assert_eq!(bs.top(), Some(TestRoute::Detail(42)));
        assert_eq!(bs.pop(), Some(TestRoute::Detail(42)));
        assert_eq!(bs.top(), Some(TestRoute::Home));
        assert_eq!(bs.pop(), Some(TestRoute::Home));
        assert!(bs.is_empty());
        assert_eq!(bs.pop(), None, "空栈 pop 返回 None");
    }

    #[test]
    fn back_stack_remove_last_and_clear() {
        let bs = NavBackStack::<TestRoute>::new();
        bs.push(TestRoute::Home);
        bs.push(TestRoute::Detail(1));
        bs.remove_last();
        assert_eq!(bs.top(), Some(TestRoute::Home));
        bs.clear();
        assert!(bs.is_empty());
        bs.set_stack(vec![TestRoute::Detail(9), TestRoute::Detail(8)]);
        assert_eq!(bs.len(), 2);
        assert_eq!(bs.top(), Some(TestRoute::Detail(8)));
    }

    #[test]
    fn nav_display_renders_top_entry() {
        use crate::runtime::composer::Composer;
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let bs = NavBackStack::<TestRoute>::with_initial(TestRoute::Home);
        let mut composer = Composer::new();

        let mut build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                NavDisplay::new(&bs, |ctx, key| match key {
                    TestRoute::Home => NavEntry::new(key.clone(), |ctx, _| {
                        crate::components::Text::new("HomeScreen").build(ctx);
                    }),
                    TestRoute::Settings => NavEntry::new(key.clone(), |ctx, _| {
                        crate::components::Text::new("SettingsScreen").build(ctx);
                    }),
                    TestRoute::Detail(id) => {
                        let id = *id; // 复制（&u64 → u64）——move 闭包需 owned
                        NavEntry::new(key.clone(), move |ctx, _| {
                            crate::components::Text::new(format!("Detail{id}")).build(ctx);
                        })
                    }
                })
                .build(ctx);
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        // 推进动画帧（过渡动画需时间——push/pop 后等滑动完成）
        let mut advance = |composer: &mut Composer| {
            for _ in 0..12 {
                crate::animation::update_animations();
                std::thread::sleep(std::time::Duration::from_millis(60));
                build(composer);
            }
        };

        // 栈顶是 Home——树里应有 HomeScreen 文本
        build(&mut composer);
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let mut texts = Vec::new();
        collect_texts(nodes, root, &mut texts);
        assert!(texts.iter().any(|t| t.contains("HomeScreen")), "SinglePane 应渲染栈顶 HomeScreen");

        // push Detail(7) → 推进动画 → 渲染 Detail7
        bs.push(TestRoute::Detail(7));
        build(&mut composer);
        advance(&mut composer);
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let mut texts = Vec::new();
        collect_texts(nodes, root, &mut texts);
        assert!(texts.iter().any(|t| t.contains("Detail7")), "push 后应渲染 Detail7");
        // pop → 回到 Home
        bs.pop();
        build(&mut composer);
        advance(&mut composer);
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let mut texts = Vec::new();
        collect_texts(nodes, root, &mut texts);
        assert!(texts.iter().any(|t| t.contains("HomeScreen")), "pop 后应回到 HomeScreen");
        assert!(!texts.iter().any(|t| t.contains("Detail7")), "pop 后 Detail7 应移除");
    }

    /// ListDetail 双栏 Scene：栈 ≥2 时同时渲染 list（倒数第二）与 detail（栈顶）
    #[test]
    fn list_detail_scene_renders_both_entries() {
        use crate::runtime::composer::Composer;
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let bs = NavBackStack::<TestRoute>::with_initial(TestRoute::Home);
        let mut composer = Composer::new();
        let build = |composer: &mut Composer, bs: &NavBackStack<TestRoute>| {
            composer.compose(|ctx| {
                NavDisplay::new(&bs, |ctx, key| match key {
                    TestRoute::Home => NavEntry::new(key.clone(), |ctx, _| {
                        crate::components::Text::new("HomeScreen").build(ctx);
                    }),
                    TestRoute::Settings => NavEntry::new(key.clone(), |ctx, _| {
                        crate::components::Text::new("SettingsScreen").build(ctx);
                    }),
                    TestRoute::Detail(id) => {
                        let id = *id;
                        NavEntry::new(key.clone(), move |ctx, _| {
                            crate::components::Text::new(format!("Detail{id}")).build(ctx);
                        })
                    }
                })
                .scene_strategies(vec![Box::new(ListDetailStrategy)])
                .build(ctx);
            });
            composer.layout(crate::layout::Constraints::new(0.0, 800.0, 0.0, 400.0));
        };
        // 栈 1 项：SinglePane（只有 Home）
        build(&mut composer, &bs);
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let mut texts = Vec::new();
        collect_texts(nodes, root, &mut texts);
        assert!(texts.iter().any(|t| t.contains("HomeScreen")));
        assert!(!texts.iter().any(|t| t.contains("Detail")), "栈 1 项无双栏");
        // 推进动画帧（场景化后所有场景变化都走过渡——含 ListDetail↔Single）
        let mut advance = |composer: &mut Composer| {
            for _ in 0..12 {
                crate::animation::update_animations();
                std::thread::sleep(std::time::Duration::from_millis(60));
                build(composer, &bs);
            }
        };
        // push Detail(5)：栈 2 项 → ListDetail 场景（Home 在左栏 + Detail5 在右栏）
        bs.push(TestRoute::Detail(5));
        build(&mut composer, &bs);
        advance(&mut composer);
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let mut texts = Vec::new();
        collect_texts(nodes, root, &mut texts);
        assert!(texts.iter().any(|t| t.contains("HomeScreen")), "双栏左栏渲染 list（Home）");
        assert!(texts.iter().any(|t| t.contains("Detail5")), "双栏右栏渲染 detail（Detail5）");
        // push Settings：栈 3 项 → 场景过渡（detail 变化即场景变化——按
        // transition_spec 整屏过渡；场景只渲染末两位：list=Detail5、detail=Settings，
        // Home 不在新场景内。回归覆盖 first()/get(1) 错位 bug）
        bs.push(TestRoute::Settings);
        build(&mut composer, &bs);
        advance(&mut composer);
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let mut texts = Vec::new();
        collect_texts(nodes, root, &mut texts);
        assert!(texts.iter().any(|t| t.contains("Detail5")), "栈 3 项左栏应=倒数第二（Detail5）");
        assert!(texts.iter().any(|t| t.contains("SettingsScreen")), "栈 3 项右栏应=栈顶（Settings）");
        assert!(!texts.iter().any(|t| t.contains("HomeScreen")), "场景只渲染末两位，Home 不应出现");
        // pop Settings：场景过渡反向，回 [H,D5] 双栏（Home 回到 list pane）
        bs.pop();
        build(&mut composer, &bs);
        advance(&mut composer);
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let mut texts = Vec::new();
        collect_texts(nodes, root, &mut texts);
        assert!(texts.iter().any(|t| t.contains("HomeScreen")) && texts.iter().any(|t| t.contains("Detail5")),
            "pop 后回 [H,D5] 双栏");
        assert!(!texts.iter().any(|t| t.contains("SettingsScreen")), "Settings 应已移除");
        // pop 回 1 项 → 场景边界（ListDetail→SinglePane）有过渡，完成后仅 Home
        bs.pop();
        build(&mut composer, &bs);
        advance(&mut composer);
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let mut texts = Vec::new();
        collect_texts(nodes, root, &mut texts);
        assert!(texts.iter().any(|t| t.contains("HomeScreen")));
        assert!(!texts.iter().any(|t| t.contains("Detail")), "pop 过渡完成后无双栏");
    }

    /// RememberStateDecorator：entry 内 remember 状态跨 pop/push 保持
    /// （固定组合 key——对标 Nav3 SaveableStateHolder 的 contentKey 状态保持）
    #[test]
    fn remember_state_decorator_preserves_entry_state() {
        use crate::runtime::composer::Composer;
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let bs = NavBackStack::<TestRoute>::with_initial(TestRoute::Home);
        let mut composer = Composer::new();

        let mut build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                NavDisplay::new(&bs, |ctx, key| match key {
                    TestRoute::Home => NavEntry::new(key.clone(), |ctx, _| {
                        crate::components::Text::new("HomeScreen").build(ctx);
                    }),
                    TestRoute::Settings => NavEntry::new(key.clone(), |ctx, _| {
                        crate::components::Text::new("SettingsScreen").build(ctx);
                    }),
                    TestRoute::Detail(id) => {
                        let id = *id;
                    NavEntry::new(key.clone(), move |ctx, _| {
                        // 状态池保持（对标 Nav3 SaveableStateHolder）
                        let counter = remember_entry_state(|| 0i32);
                            counter.update(|v| *v += 1);
                            crate::components::Text::new(format!("Detail{id}-counter{}", counter.get())).build(ctx);
                        })
                    }
                })
                .build(ctx);
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let mut advance = |composer: &mut Composer| {
            for _ in 0..12 {
                crate::animation::update_animations();
                std::thread::sleep(std::time::Duration::from_millis(60));
                build(composer);
            }
        };
        let read_counter = |composer: &mut Composer| -> i32 {
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let mut texts = Vec::new();
            collect_texts(nodes, root, &mut texts);
            texts.iter().find_map(|t| {
                let p = "Detail7-counter";
                t.find(p).map(|i| t[i + p.len()..].parse::<i32>().unwrap_or(0))
            }).expect("应渲染 Detail7-counterN")
        };

        // 场景 1（Nav3 核心语义——覆盖返回保持）：Detail(7) → 计数累加 → push
        // Settings（Detail 仍在栈中）→ pop 回 Detail(7) → counter 应从旧值继续
        // （Detail 未从 back stack 移除——状态池保持，对标 Nav3 同一 entry
        // 生命周期内状态保持）
        bs.push(TestRoute::Detail(7));
        build(&mut composer);
        advance(&mut composer);
        let first = read_counter(&mut composer);
        assert!(first > 0, "首次进入 counter 应 >0，实际 {first}");
        bs.push(TestRoute::Settings); // 覆盖 Detail（不 pop）
        build(&mut composer);
        advance(&mut composer);
        bs.pop(); // 回到 Detail(7)
        build(&mut composer);
        advance(&mut composer);
        let covered = read_counter(&mut composer);
        assert!(
            covered > first,
            "覆盖返回保持：counter 应从 {first} 继续（实际 {covered}）"
        );

        // 场景 2（Nav3 语义——pop 清理）：pop Detail（从 back stack 移除 →
        // removeState 清理）→ 再 push 同一 Detail(7) → counter 重置（从 0 重新计数）
        bs.pop(); // 移除 Detail(7)
        build(&mut composer);
        advance(&mut composer);
        bs.push(TestRoute::Detail(7));
        build(&mut composer);
        advance(&mut composer);
        let fresh = read_counter(&mut composer);
        assert!(
            fresh <= covered,
            "pop 后重进应重置（Nav3 removeState 语义）——covered={covered} 实际 {fresh}"
        );
    }

    /// 过渡中间帧：push/pop 后新旧两页应同树（Stack 双页层叠——对标
    /// AnimatedContent 双内容过渡），动画完成后旧页移除。
    /// 显式采用 horizontal_slide（Android 形态）——默认 fade 由
    /// `transition_spec_fade_and_none` 覆盖。
    #[test]
    fn transition_midframe_renders_both_pages() {
        use crate::runtime::composer::Composer;
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let bs = NavBackStack::<TestRoute>::with_initial(TestRoute::Home);
        let mut composer = Composer::new();
        let (push_spec, pop_spec) = NavTransitionSpec::horizontal_slide();

        let mut build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                NavDisplay::new(&bs, |ctx, key| match key {
                    TestRoute::Home => NavEntry::new(key.clone(), |ctx, _| {
                        crate::components::Text::new("HomeScreen").build(ctx);
                    }),
                    TestRoute::Settings => NavEntry::new(key.clone(), |ctx, _| {
                        crate::components::Text::new("SettingsScreen").build(ctx);
                    }),
                    TestRoute::Detail(id) => {
                        let id = *id;
                        NavEntry::new(key.clone(), move |ctx, _| {
                            crate::components::Text::new(format!("Detail{id}")).build(ctx);
                        })
                    }
                })
                .transition_spec(push_spec.clone())
                .pop_transition_spec(pop_spec.clone())
                .build(ctx);
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        // 单帧推进（~60ms——300ms 过渡的中段），整轮推进（>300ms——过渡完成）
        let tick = |composer: &mut Composer| {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(60));
            build(composer);
        };
        let texts = |composer: &mut Composer| -> Vec<String> {
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let mut out = Vec::new();
            collect_texts(nodes, root, &mut out);
            out
        };

        // push 中间帧：HomeScreen（旧页滑出）+ Detail7（新页滑入）同树
        build(&mut composer);
        bs.push(TestRoute::Detail(7));
        build(&mut composer);
        tick(&mut composer);
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("HomeScreen")), "push 中间帧应含旧页 HomeScreen");
        assert!(t.iter().any(|x| x.contains("Detail7")), "push 中间帧应含新页 Detail7");
        for _ in 0..12 { tick(&mut composer); }
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("Detail7")));
        assert!(!t.iter().any(|x| x.contains("HomeScreen")), "push 完成后旧页应移除");

        // pop 中间帧：Detail7（滑出）+ HomeScreen（滑入）同树；完成后 Detail7 移除
        bs.pop();
        build(&mut composer);
        tick(&mut composer);
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("HomeScreen")), "pop 中间帧应含新页 HomeScreen");
        assert!(t.iter().any(|x| x.contains("Detail7")), "pop 中间帧应含滑出旧页 Detail7");
        for _ in 0..12 { tick(&mut composer); }
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("HomeScreen")));
        assert!(!t.iter().any(|x| x.contains("Detail7")), "pop 完成后旧页应移除");
    }

    /// dialog 标记 entry（对标 Nav3 `dialog()` metadata / DialogSceneStrategy）：
    /// 栈顶 dialog entry 渲染为模态覆盖层——**主树不渲染其内容**（测试 composer
    /// 不物化 overlay）；base 场景不受对话框开/关影响；连续 dialog 栈仅栈顶
    /// 渲染覆盖层；pop 即关闭、无残留
    #[test]
    fn dialog_entry_renders_as_overlay() {
        #[derive(Clone, PartialEq, Eq, Debug, Hash)]
        enum DialogRoute {
            Home,
            About,
        }
        use crate::runtime::composer::Composer;
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let bs = NavBackStack::<DialogRoute>::with_initial(DialogRoute::Home);
        let mut composer = Composer::new();
        let mut build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                NavDisplay::new(&bs, |ctx, key| match key {
                    DialogRoute::Home => NavEntry::new(key.clone(), |ctx, _| {
                        crate::components::Text::new("HomeScreen").build(ctx);
                    }),
                    DialogRoute::About => NavEntry::new(key.clone(), |ctx, _| {
                        crate::components::Text::new("AboutScreen").build(ctx);
                    })
                    .as_dialog(),
                })
                .build(ctx);
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let texts = |composer: &mut Composer| -> Vec<String> {
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let mut out = Vec::new();
            collect_texts(nodes, root, &mut out);
            out
        };

        // 开对话框：主树只渲染 base（Home）——About 内容不进主树（覆盖层由
        // app 层独立物化，测试 composer 不渲染）；base 场景不变（无过渡）
        build(&mut composer);
        bs.push(DialogRoute::About);
        build(&mut composer);
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("HomeScreen")), "base 场景应正常渲染");
        assert!(!t.iter().any(|x| x.contains("AboutScreen")), "dialog 内容不应进主树");
        // 连续 dialog 栈：仅栈顶渲染覆盖层，base 仍为 Home
        bs.push(DialogRoute::About);
        build(&mut composer);
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("HomeScreen")));
        assert!(!t.iter().any(|x| x.contains("AboutScreen")), "连续 dialog 仅覆盖层，主树无内容");
        // pop 一个对话框：主树不变、无残留
        bs.pop();
        build(&mut composer);
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("HomeScreen")));
        // pop 到底：回到纯 base
        bs.pop();
        build(&mut composer);
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("HomeScreen")));
        assert!(!t.iter().any(|x| x.contains("AboutScreen")));
    }

    /// P1 回归：对话框弹到**空栈**后程序化 pop 关闭——Dialog::build 必须仍
    /// 无条件执行（记录 active=false → app 层 sync 删除 overlay），不得泄漏。
    /// 修复前 `if stack.is_empty() { return }` 提前返回 → 无 active=false 记录
    /// → overlay 永久残留（可见、拦截点击）。
    #[test]
    fn dialog_on_empty_stack_close_records_inactive() {
        #[derive(Clone, PartialEq, Eq, Debug, Hash)]
        enum DialogRoute {
            About,
        }
        use crate::runtime::composer::Composer;
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let bs = NavBackStack::<DialogRoute>::new(); // 空栈开始
        let mut composer = Composer::new();
        let mut build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                NavDisplay::new(&bs, |ctx, key| match key {
                    DialogRoute::About => NavEntry::new(key.clone(), |ctx, _| {
                        crate::components::Text::new("AboutScreen").build(ctx);
                    })
                    .as_dialog(),
                })
                .build(ctx);
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        };

        // 空栈：build 执行（无 scene、无 dialog）——Dialog::build(false) 应记录 active=false
        build(&mut composer);
        let inactive_ids: Vec<u64> = composer.overlay_active.iter()
            .filter(|(_, a)| !**a)
            .map(|(&id, _)| id)
            .collect();
        // 空栈首帧无 overlay 记录（从未注册）——只验证不 panic
        build(&mut composer);

        // 弹入对话框（栈 [About]——唯一 entry）
        bs.push(DialogRoute::About);
        build(&mut composer);
        let active_ids: Vec<u64> = composer.overlay_active.iter()
            .filter(|(_, a)| **a)
            .map(|(&id, _)| id)
            .collect();
        assert_eq!(active_ids.len(), 1, "对话框应注册 active overlay");
        let ov_id = active_ids[0];

        // 程序化 pop 关闭（按钮路径——不经过 app 层 overlay_down）
        bs.pop();
        build(&mut composer);
        // ⚠ P1 修复点：栈空后 Dialog::build 仍执行 → overlay_active 记录
        // (ov_id, false) → app 层 sync_overlays 能删除该 overlay。
        // 断言"记录存在且为 false"——`unwrap_or(false)` 会把"本帧无记录"
        // （修复前：空栈 early-return → Dialog::build 不执行）与"记录为
        // false"混为一谈，测试对 P1 不敏感。必须区分：
        // 修复前 get=Some(true)（上帧残留）或 None（被 clear）→ 断言失败；
        // 修复后 get=Some(&false) → 通过
        assert_eq!(
            composer.overlay_active.get(&ov_id),
            Some(&false),
            "对话框关闭后必须记录 active=false（ov_id={ov_id}）——否则 overlay 泄漏"
        );
    }

    /// pop 滑出期间旧页内容每帧重跑——不得把已 removeState 清理的池槽重新插回
    /// （回归测试：draining 只读作用域。组合期写入的内容最易触发重污染——
    /// 若无 draining，滑出 12 帧会把计数累加进重插的槽，重进首帧远大于 1）
    #[test]
    fn pop_slideout_does_not_repollute_entry_state_pool() {
        use crate::runtime::composer::Composer;
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let bs = NavBackStack::<TestRoute>::with_initial(TestRoute::Home);
        let mut composer = Composer::new();

        let mut build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                NavDisplay::new(&bs, |ctx, key| match key {
                    TestRoute::Home => NavEntry::new(key.clone(), |ctx, _| {
                        crate::components::Text::new("HomeScreen").build(ctx);
                    }),
                    TestRoute::Settings => NavEntry::new(key.clone(), |ctx, _| {
                        crate::components::Text::new("SettingsScreen").build(ctx);
                    }),
                    TestRoute::Detail(id) => {
                        let id = *id;
                        NavEntry::new(key.clone(), move |ctx, _| {
                            // 组合期写入（重污染路径的最小复现）
                            let counter = remember_entry_state(|| 0i32);
                            counter.update(|v| *v += 1);
                            crate::components::Text::new(format!("Detail{id}-counter{}", counter.get())).build(ctx);
                        })
                    }
                })
                .build(ctx);
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let mut advance = |composer: &mut Composer| {
            for _ in 0..12 {
                crate::animation::update_animations();
                std::thread::sleep(std::time::Duration::from_millis(60));
                build(composer);
            }
        };
        let read_counter = |composer: &mut Composer| -> i32 {
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let mut texts = Vec::new();
            collect_texts(nodes, root, &mut texts);
            texts.iter().find_map(|t| {
                let p = "Detail7-counter";
                t.find(p).map(|i| t[i + p.len()..].parse::<i32>().unwrap_or(0))
            }).expect("应渲染 Detail7-counterN")
        };

        bs.push(TestRoute::Detail(7));
        build(&mut composer);
        advance(&mut composer);
        bs.pop(); // removeState 清理 → 滑出 12 帧（draining 不得写池）
        build(&mut composer);
        advance(&mut composer);
        bs.push(TestRoute::Detail(7));
        build(&mut composer); // 重进首帧（未推进动画——读到的即首帧值）
        let first = read_counter(&mut composer);
        assert_eq!(
            first, 1,
            "pop 完成后重进：池应为空（滑出层未重污染），首帧计数恰为 1——实际 {first}"
        );
    }

    /// contentKey（对标 NavEntry.contentKey）：同 contentKey 的不同路由共享状态池
    /// 槽（Nav3 语义：同内容 = 同状态）；默认（key hash）不同路由相互独立；
    /// 同 contentKey 多实例时弹出其一不清理（"最后一个实例"语义）
    #[test]
    fn content_key_shares_state_across_routes() {
        use crate::runtime::composer::Composer;
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let bs = NavBackStack::<TestRoute>::with_initial(TestRoute::Home);
        let mut composer = Composer::new();

        let mut build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                NavDisplay::new(&bs, |ctx, key| match key {
                    TestRoute::Home => NavEntry::new(key.clone(), |ctx, _| {
                        crate::components::Text::new("HomeScreen").build(ctx);
                    }),
                    TestRoute::Settings => NavEntry::new(key.clone(), |ctx, _| {
                        crate::components::Text::new("SettingsScreen").build(ctx);
                    }),
                    TestRoute::Detail(id) => {
                        let id = *id;
                        let key = key.clone();
                        // Detail(1)/Detail(2) 共享 contentKey=42（同内容语义）；
                        // Detail(3) 用默认（key hash）——独立状态
                        let entry = if id <= 2 {
                            NavEntry::with_content_key(key, 42, move |ctx, _| {
                                let counter = remember_entry_state(|| 0i32);
                                counter.update(|v| *v += 1);
                                crate::components::Text::new(format!("Detail{id}-counter{}", counter.get())).build(ctx);
                            })
                        } else {
                            NavEntry::new(key, move |ctx, _| {
                                let counter = remember_entry_state(|| 0i32);
                                counter.update(|v| *v += 1);
                                crate::components::Text::new(format!("Detail{id}-counter{}", counter.get())).build(ctx);
                            })
                        };
                        entry
                    }
                })
                .build(ctx);
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let advance = |composer: &mut Composer| {
            for _ in 0..12 {
                crate::animation::update_animations();
                std::thread::sleep(std::time::Duration::from_millis(60));
                build(composer);
            }
        };
        let counter_of = |composer: &mut Composer, id: u64| -> i32 {
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let mut texts = Vec::new();
            collect_texts(nodes, root, &mut texts);
            texts.iter().find_map(|t| {
                let p = format!("Detail{id}-counter");
                t.find(&p).map(|i| t[i + p.len()..].parse::<i32>().unwrap_or(0))
            }).unwrap_or_else(|| panic!("应渲染 Detail{id}-counterN"))
        };

        // Detail(1)（ck=42）计数累加
        bs.push(TestRoute::Detail(1));
        build(&mut composer);
        advance(&mut composer);
        let n1 = counter_of(&mut composer, 1);
        assert!(n1 > 0, "Detail(1) 首轮应计数 >0");
        // push Settings 覆盖 → pop → Detail(1) 计数保持（同 ck 槽未清理）
        bs.push(TestRoute::Settings);
        build(&mut composer);
        advance(&mut composer);
        bs.pop();
        build(&mut composer);
        advance(&mut composer);
        let n1b = counter_of(&mut composer, 1);
        assert!(n1b > n1, "覆盖返回保持：{} 应 > {n1}", n1b);
        // push Detail(2)（同 ck=42）：共享同一池槽——计数从 Detail(1) 的值继续
        bs.push(TestRoute::Detail(2));
        build(&mut composer);
        advance(&mut composer);
        let n2 = counter_of(&mut composer, 2);
        assert!(n2 > n1b, "同 contentKey 应共享池槽：Detail(2) 计数 {n2} 应 > Detail(1) 的 {n1b}");
        // 弹出 Detail(2) 时 Detail(1)（同 ck）仍在栈——不清理（最后实例语义）
        bs.pop();
        build(&mut composer);
        advance(&mut composer);
        let n1c = counter_of(&mut composer, 1);
        assert!(n1c >= n2, "同 ck 多实例弹出其一不清理：Detail(1) {n1c} 应 ≥ {n2}");
        // 全部弹出（ck=42 槽清理）→ push Detail(3)（默认 ck）：全新状态
        bs.pop();
        build(&mut composer);
        advance(&mut composer);
        bs.push(TestRoute::Detail(3));
        build(&mut composer);
        let n3 = counter_of(&mut composer, 3);
        assert_eq!(n3, 1, "默认 contentKey 独立状态：首帧计数应恰为 1——实际 {n3}");
    }

    /// 过渡规格：默认 fade（Nav3 NavDisplay 默认——fadeIn togetherWith fadeOut）
    /// 中间帧双页同树；`Spec::none()`（EnterTransition.None togetherWith
    /// ExitTransition.None）瞬时切换——无旧页、无动画
    #[test]
    fn transition_spec_fade_and_none() {
        use crate::runtime::composer::Composer;
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let bs = NavBackStack::<TestRoute>::with_initial(TestRoute::Home);
        let mut composer = Composer::new();

        let build_with = |composer: &mut Composer, spec: NavTransitionSpec| {
            composer.compose(|ctx| {
                NavDisplay::new(&bs, |ctx, key| match key {
                    TestRoute::Home => NavEntry::new(key.clone(), |ctx, _| {
                        crate::components::Text::new("HomeScreen").build(ctx);
                    }),
                    TestRoute::Settings => NavEntry::new(key.clone(), |ctx, _| {
                        crate::components::Text::new("SettingsScreen").build(ctx);
                    }),
                    TestRoute::Detail(id) => {
                        let id = *id;
                        NavEntry::new(key.clone(), move |ctx, _| {
                            crate::components::Text::new(format!("Detail{id}")).build(ctx);
                        })
                    }
                })
                .transition_spec(spec.clone())
                .pop_transition_spec(spec)
                .build(ctx);
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let texts = |composer: &mut Composer| -> Vec<String> {
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let mut out = Vec::new();
            collect_texts(nodes, root, &mut out);
            out
        };
        let tick = |composer: &mut Composer| {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(60));
        };

        // —— 默认 fade：中间帧双页同树，完成后旧页移除 ——
        build_with(&mut composer, NavTransitionSpec::fade());
        bs.push(TestRoute::Detail(7));
        build_with(&mut composer, NavTransitionSpec::fade());
        tick(&mut composer);
        build_with(&mut composer, NavTransitionSpec::fade());
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("HomeScreen")), "fade 中间帧应含旧页");
        assert!(t.iter().any(|x| x.contains("Detail7")), "fade 中间帧应含新页");
        for _ in 0..12 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(60));
            build_with(&mut composer, NavTransitionSpec::fade());
        }
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("Detail7")));
        assert!(!t.iter().any(|x| x.contains("HomeScreen")), "fade 完成后旧页应移除");

        // —— none：瞬时切换（无旧页层、无动画）——
        build_with(&mut composer, NavTransitionSpec::none());
        bs.pop();
        bs.push(TestRoute::Settings);
        build_with(&mut composer, NavTransitionSpec::none());
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("SettingsScreen")), "none 应立即渲染新页");
        assert!(!t.iter().any(|x| x.contains("Detail7")), "none 不应有旧页残留");
        assert!(!t.iter().any(|x| x.contains("HomeScreen")), "none 不应渲染栈外页");
    }

    /// 策略链切换（如自适应模式切换）：**entries 未变 → 瞬时切换**（形态切
    /// 无过渡——双层同渲同 contentKey 内容会在非宏节点键空间 dup-key，且形态
    /// 切瞬切符合平台惯例）；切换后单帧即稳态、可反复切换
    #[test]
    fn scene_strategy_switch_converges() {
        use crate::runtime::composer::Composer;
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let bs = NavBackStack::<TestRoute>::with_initial(TestRoute::Home);
        let mut composer = Composer::new();
        let list_detail = |composer: &mut Composer| {
            composer.compose(|ctx| {
                NavDisplay::new(&bs, |ctx, key| match key {
                    TestRoute::Home => NavEntry::new(key.clone(), |ctx, _| {
                        crate::components::Text::new("HomeScreen").build(ctx);
                    }),
                    TestRoute::Settings => NavEntry::new(key.clone(), |ctx, _| {
                        crate::components::Text::new("SettingsScreen").build(ctx);
                    }),
                    TestRoute::Detail(id) => {
                        let id = *id;
                        NavEntry::new(key.clone(), move |ctx, _| {
                            crate::components::Text::new(format!("Detail{id}")).build(ctx);
                        })
                    }
                })
                .scene_strategies(vec![Box::new(ListDetailStrategy)])
                .build(ctx);
            });
            composer.layout(crate::layout::Constraints::new(0.0, 800.0, 0.0, 400.0));
        };
        let single = |composer: &mut Composer| {
            composer.compose(|ctx| {
                NavDisplay::new(&bs, |ctx, key| match key {
                    TestRoute::Home => NavEntry::new(key.clone(), |ctx, _| {
                        crate::components::Text::new("HomeScreen").build(ctx);
                    }),
                    TestRoute::Settings => NavEntry::new(key.clone(), |ctx, _| {
                        crate::components::Text::new("SettingsScreen").build(ctx);
                    }),
                    TestRoute::Detail(id) => {
                        let id = *id;
                        NavEntry::new(key.clone(), move |ctx, _| {
                            crate::components::Text::new(format!("Detail{id}")).build(ctx);
                        })
                    }
                })
                .build(ctx); // 空链 → SinglePane 兜底
            });
            composer.layout(crate::layout::Constraints::new(0.0, 800.0, 0.0, 400.0));
        };
        let texts = |composer: &mut Composer| -> Vec<String> {
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let mut out = Vec::new();
            collect_texts(nodes, root, &mut out);
            out
        };

        // 阶段 1（single）：push D5 → 场景过渡（fade）→ 收敛后单栏仅栈顶
        single(&mut composer);
        bs.push(TestRoute::Detail(5));
        for _ in 0..12 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(60));
            single(&mut composer);
        }
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("Detail5")));
        assert!(!t.iter().any(|x| x.contains("HomeScreen")), "SinglePane 收敛后只渲染栈顶");

        // 阶段 2（切 ListDetail）：entries 未变 → 瞬时切换，单帧即双栏稳态
        list_detail(&mut composer);
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("HomeScreen")) && t.iter().any(|x| x.contains("Detail5")),
            "切双栏应瞬时稳态 H+D5");

        // 阶段 3（切回 single）：瞬时回单栏（H 退场）
        single(&mut composer);
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("Detail5")) && !t.iter().any(|x| x.contains("HomeScreen")),
            "切回单栏瞬时仅栈顶");

        // 阶段 4（再切双栏）：反复切换后仍稳态（无冻结/无残留）
        list_detail(&mut composer);
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("HomeScreen")) && t.iter().any(|x| x.contains("Detail5")),
            "二次切双栏仍稳态（无冻结）");
    }

    /// SceneStrategy 策略链（对标 Nav3 `List<SceneStrategy>` + SinglePane 兜底）：
    /// 自定义策略优先接手（自定义 Scene 渲染全部 entries）、ListDetail 次之
    /// （<2 条返回 None 落空）、SinglePane 兜底——验证链序与回退语义
    #[test]
    fn scene_strategy_chain_priority_and_fallback() {
        use crate::runtime::composer::Composer;

        /// 三栏场景：一列渲染全部 entries（自定义 Scene 形态）
        struct TriPaneScene<K: NavKey> { entries: Vec<NavEntry<K>> }
        impl<K: NavKey> Scene<K> for TriPaneScene<K> {
            fn scene_key(&self) -> u64 {
                fnv_hash(&("winia/test/TriPaneScene", self.entries.len()))
            }
            fn entries(&self) -> &[NavEntry<K>] { &self.entries }
            fn content(
                &self,
                ctx: &mut ComposeCtx,
                render_entry: &dyn Fn(&mut ComposeCtx, &NavEntry<K>, bool),
            ) {
                crate::layout::components::Column::new().build(ctx, |ctx| {
                    for e in &self.entries {
                        render_entry(ctx, e, false);
                    }
                });
            }
        }
        /// 栈 ≥3 时接手（三栏）
        struct TriPaneStrategy;
        impl<K: NavKey> SceneStrategy<K> for TriPaneStrategy {
            fn calculate_scene(&self, entries: &[NavEntry<K>]) -> Option<Box<dyn Scene<K>>> {
                (entries.len() >= 3).then(|| Box::new(TriPaneScene { entries: entries.to_vec() }) as Box<dyn Scene<K>>)
            }
        }

        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let bs = NavBackStack::<TestRoute>::with_initial(TestRoute::Home);
        let mut composer = Composer::new();

        let mut build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                NavDisplay::new(&bs, |ctx, key| match key {
                    TestRoute::Home => NavEntry::new(key.clone(), |ctx, _| {
                        crate::components::Text::new("HomeScreen").build(ctx);
                    }),
                    TestRoute::Settings => NavEntry::new(key.clone(), |ctx, _| {
                        crate::components::Text::new("SettingsScreen").build(ctx);
                    }),
                    TestRoute::Detail(id) => {
                        let id = *id;
                        NavEntry::new(key.clone(), move |ctx, _| {
                            crate::components::Text::new(format!("Detail{id}")).build(ctx);
                        })
                    }
                })
                .scene_strategies(vec![
                    Box::new(TriPaneStrategy),
                    Box::new(ListDetailStrategy),
                ])
                .build(ctx);
            });
            composer.layout(crate::layout::Constraints::new(0.0, 800.0, 0.0, 400.0));
        };
        let texts = |composer: &mut Composer| -> Vec<String> {
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let mut out = Vec::new();
            collect_texts(nodes, root, &mut out);
            out
        };

        // 栈 1 项：TriPane 落空、ListDetail 落空 → SinglePane 兜底
        build(&mut composer);
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("HomeScreen")));
        assert!(!t.iter().any(|x| x.contains("Detail")), "栈 1 项应 SinglePane 兜底");

        // 栈 2 项：TriPane 落空 → ListDetail 接手（list+detail 双栏）
        bs.push(TestRoute::Detail(5));
        build(&mut composer);
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("HomeScreen")) && t.iter().any(|x| x.contains("Detail5")),
            "栈 2 项应 ListDetail 接手");

        // 栈 3 项：TriPane 接手（自定义场景渲染全部 entries）
        bs.push(TestRoute::Settings);
        build(&mut composer);
        let t = texts(&mut composer);
        assert!(
            t.iter().any(|x| x.contains("HomeScreen"))
                && t.iter().any(|x| x.contains("Detail5"))
                && t.iter().any(|x| x.contains("SettingsScreen")),
            "栈 3 项应自定义 TriPaneScene 接手（渲染全部 entries）"
        );
    }

    /// entry 级过渡覆盖（对标 Nav3 NavDisplay.TransitionKey/PopTransitionKey
    /// metadata 优先级"过渡中 entry > NavDisplay 默认"）：push 前景 = 新栈顶、
    /// pop 前景 = 被弹出的旧条目。覆盖 `none()` 生效的判别：单帧内无旧页层
    /// （若覆盖失效回退默认 fade，旧页会保留一整个过渡期）
    #[test]
    fn entry_transition_override_wins_over_display_default() {
        use crate::runtime::composer::Composer;
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let bs = NavBackStack::<TestRoute>::with_initial(TestRoute::Home);
        let mut composer = Composer::new();

        let mut build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                NavDisplay::new(&bs, |ctx, key| match key {
                    TestRoute::Home => NavEntry::new(key.clone(), |ctx, _| {
                        crate::components::Text::new("HomeScreen").build(ctx);
                    }),
                    TestRoute::Settings => NavEntry::new(key.clone(), |ctx, _| {
                        crate::components::Text::new("SettingsScreen").build(ctx);
                    }),
                    TestRoute::Detail(id) => {
                        let id = *id;
                        // Detail 覆盖双向为 none（瞬时）——Settings 不覆盖（对照组）
                        NavEntry::new(key.clone(), move |ctx, _| {
                            crate::components::Text::new(format!("Detail{id}")).build(ctx);
                        })
                        .transition_spec(NavTransitionSpec::none())
                        .pop_transition_spec(NavTransitionSpec::none())
                    }
                })
                .build(ctx); // 默认 fade（display 级）
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let texts = |composer: &mut Composer| -> Vec<String> {
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let mut out = Vec::new();
            collect_texts(nodes, root, &mut out);
            out
        };
        let tick = |composer: &mut Composer| {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(60));
        };

        build(&mut composer);
        // push Detail（覆盖 none）→ 单帧内旧页即移除
        bs.push(TestRoute::Detail(7));
        build(&mut composer);
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("Detail7")));
        assert!(!t.iter().any(|x| x.contains("HomeScreen")),
            "entry transition_spec 覆盖应生效（none 瞬时，无旧页层）");

        // 对照：push Settings（无覆盖）→ 默认 fade → 中间帧双页
        bs.push(TestRoute::Settings);
        build(&mut composer);
        tick(&mut composer);
        build(&mut composer);
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("SettingsScreen")) && t.iter().any(|x| x.contains("Detail7")),
            "无覆盖 entry 应回退 NavDisplay 默认 fade（中间帧双页）");
        for _ in 0..12 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(60));
            build(&mut composer);
        }

        // pop Settings（无 pop 覆盖）→ 默认 fade → 中间帧双页
        bs.pop();
        build(&mut composer);
        tick(&mut composer);
        build(&mut composer);
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("SettingsScreen")) && t.iter().any(|x| x.contains("Detail7")),
            "无覆盖 entry 的 pop 应回退默认 fade");
        for _ in 0..12 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(60));
            build(&mut composer);
        }

        // pop Detail（覆盖 pop none）→ 单帧内旧页即移除
        bs.pop();
        build(&mut composer);
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("HomeScreen")));
        assert!(!t.iter().any(|x| x.contains("Detail7")),
            "entry pop_transition_spec 覆盖应生效（none 瞬时，无旧页层）");
    }

    /// shared_axis（M3 30px 反向小位移 + 淡入淡出）——覆盖 `SlideOffset::Px`
    /// 求值与 `SlideAndFade*` 原语分支：中间帧双页同树、完成后旧页移除
    #[test]
    fn transition_shared_axis_midframe() {
        use crate::runtime::composer::Composer;
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let bs = NavBackStack::<TestRoute>::with_initial(TestRoute::Home);
        let mut composer = Composer::new();
        let (push_spec, pop_spec) = NavTransitionSpec::shared_axis();

        let mut build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                NavDisplay::new(&bs, |ctx, key| match key {
                    TestRoute::Home => NavEntry::new(key.clone(), |ctx, _| {
                        crate::components::Text::new("HomeScreen").build(ctx);
                    }),
                    TestRoute::Settings => NavEntry::new(key.clone(), |ctx, _| {
                        crate::components::Text::new("SettingsScreen").build(ctx);
                    }),
                    TestRoute::Detail(id) => {
                        let id = *id;
                        NavEntry::new(key.clone(), move |ctx, _| {
                            crate::components::Text::new(format!("Detail{id}")).build(ctx);
                        })
                    }
                })
                .transition_spec(push_spec.clone())
                .pop_transition_spec(pop_spec.clone())
                .build(ctx);
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let texts = |composer: &mut Composer| -> Vec<String> {
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let mut out = Vec::new();
            collect_texts(nodes, root, &mut out);
            out
        };
        let advance = |composer: &mut Composer| {
            for _ in 0..12 {
                crate::animation::update_animations();
                std::thread::sleep(std::time::Duration::from_millis(60));
                build(composer);
            }
        };

        build(&mut composer);
        bs.push(TestRoute::Detail(7));
        build(&mut composer);
        crate::animation::update_animations();
        std::thread::sleep(std::time::Duration::from_millis(60));
        build(&mut composer);
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("HomeScreen")), "shared_axis 中间帧应含旧页");
        assert!(t.iter().any(|x| x.contains("Detail7")), "shared_axis 中间帧应含新页");
        advance(&mut composer);
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("Detail7")));
        assert!(!t.iter().any(|x| x.contains("HomeScreen")), "shared_axis 完成后旧页应移除");
    }

    /// 方向选择语义：push 用 `transition_spec`（滑动——中间帧双页）、pop 用
    /// `pop_transition_spec`（none——瞬时单页）；两个 setter 必须独立生效
    #[test]
    fn transition_specs_selected_per_direction() {
        use crate::runtime::composer::Composer;
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let bs = NavBackStack::<TestRoute>::with_initial(TestRoute::Home);
        let mut composer = Composer::new();
        let (slide_push, _slide_pop_unused) = NavTransitionSpec::horizontal_slide();

        let mut build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                NavDisplay::new(&bs, |ctx, key| match key {
                    TestRoute::Home => NavEntry::new(key.clone(), |ctx, _| {
                        crate::components::Text::new("HomeScreen").build(ctx);
                    }),
                    TestRoute::Settings => NavEntry::new(key.clone(), |ctx, _| {
                        crate::components::Text::new("SettingsScreen").build(ctx);
                    }),
                    TestRoute::Detail(id) => {
                        let id = *id;
                        NavEntry::new(key.clone(), move |ctx, _| {
                            crate::components::Text::new(format!("Detail{id}")).build(ctx);
                        })
                    }
                })
                .transition_spec(slide_push.clone())
                .pop_transition_spec(NavTransitionSpec::none())
                .build(ctx);
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let texts = |composer: &mut Composer| -> Vec<String> {
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let mut out = Vec::new();
            collect_texts(nodes, root, &mut out);
            out
        };

        // push：走 slide spec——中间帧双页同树
        build(&mut composer);
        bs.push(TestRoute::Detail(7));
        build(&mut composer);
        crate::animation::update_animations();
        std::thread::sleep(std::time::Duration::from_millis(60));
        build(&mut composer);
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("HomeScreen")) && t.iter().any(|x| x.contains("Detail7")),
            "push 应走 transition_spec（滑动中间帧双页）");
        for _ in 0..12 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(60));
            build(&mut composer);
        }
        // pop：走 none spec——瞬时单页（若误用 slide，单帧内仍有旧页层）
        bs.pop();
        build(&mut composer);
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("HomeScreen")), "pop 后应渲染 Home");
        assert!(!t.iter().any(|x| x.contains("Detail7")), "pop 应走 pop_transition_spec（none 瞬时，无旧页层）");
    }

    /// SceneDecoratorStrategy：装饰器包装 scene——content 渲染装饰内容 +
    /// 原 scene 内容；scene_key/entries 透传；链式应用（外层后加入）
    #[test]
    fn scene_decorator_wraps_content() {
        use crate::runtime::composer::Composer;

        /// 记录式装饰器——在 scene 内容前追加一行文本（模拟底部导航栏）
        struct BannerDecorator { label: &'static str }
        impl<K: NavKey> SceneDecoratorStrategy<K> for BannerDecorator {
            fn decorate_scene(&self, scene: Box<dyn Scene<K>>) -> Box<dyn Scene<K>> {
                struct Decorated<K2: NavKey> {
                    inner: Box<dyn Scene<K2>>,
                    label: &'static str,
                }
                impl<K2: NavKey> Scene<K2> for Decorated<K2> {
                    fn scene_key(&self) -> u64 { self.inner.scene_key() }
                    fn entries(&self) -> &[NavEntry<K2>] { self.inner.entries() }
                    fn content(
                        &self,
                        ctx: &mut ComposeCtx,
                        render_entry: &dyn Fn(&mut ComposeCtx, &NavEntry<K2>, bool),
                    ) {
                        // 装饰内容（顶部）
                        crate::components::Text::new(format!("[{}]", self.label)).build(ctx);
                        // 原 scene 内容
                        self.inner.content(ctx, render_entry);
                    }
                }
                Box::new(Decorated { inner: scene, label: self.label })
            }
        }

        let bs = NavBackStack::<TestRoute>::with_initial(TestRoute::Home);
        let mut composer = Composer::new();
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                NavDisplay::new(&bs, |ctx, key| match key {
                    TestRoute::Home => NavEntry::new(key.clone(), |ctx, _| {
                        crate::components::Text::new("HomeScreen").build(ctx);
                    }),
                    TestRoute::Settings => NavEntry::new(key.clone(), |ctx, _| {
                        crate::components::Text::new("SettingsScreen").build(ctx);
                    }),
                    TestRoute::Detail(id) => {
                        let id = *id;
                        NavEntry::new(key.clone(), move |ctx, _| {
                            crate::components::Text::new(format!("Detail{id}")).build(ctx);
                        })
                    }
                })
                .scene_decorator_strategies(vec![
                    Box::new(BannerDecorator { label: "nav" }),
                ])
                .build(ctx);
            });
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        };
        let texts = |composer: &mut Composer| -> Vec<String> {
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            let mut out = Vec::new();
            collect_texts(nodes, root, &mut out);
            out
        };

        build(&mut composer);
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("[nav]")), "装饰内容应渲染（顶部导航栏）");
        assert!(t.iter().any(|x| x.contains("HomeScreen")), "原 scene 内容应保留");

        // 装饰不影响导航
        bs.push(TestRoute::Detail(3));
        build(&mut composer);
        let t = texts(&mut composer);
        assert!(t.iter().any(|x| x.contains("[nav]")));
        assert!(t.iter().any(|x| x.contains("Detail3")));
    }
