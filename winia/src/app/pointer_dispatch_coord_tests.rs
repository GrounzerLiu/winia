    use super::dispatch_ptr_event;
    use crate::layout::node::{hit_test, scene_to_node_local, LayoutNode};
    use crate::unit::{Offset, Size};
    use crate::modifier::{Modifier, ScrollState};
    use crate::input::{PointerButton, PointerEvent, PointerEventType, PointerKind};

    /// 构造 scroll 容器(0,0,100×200) + 子节点(0,100,100×60，带 PointerEvent handler)。
    /// scroll offset=50 → 子节点视觉顶边 y=50。
    fn build_scrolled_tree() -> (Vec<LayoutNode>, std::sync::Arc<std::sync::Mutex<Option<(f32, f32)>>>, std::sync::Arc<std::sync::Mutex<Option<(f32, f32)>>>, ScrollState) {
        let scroll = ScrollState::new();
        scroll.offset.set(50.0);
        // 子节点 handler 记录（验证子节点局部坐标扣除祖先 scroll）
        let received = std::sync::Arc::new(std::sync::Mutex::new(None::<(f32, f32)>));
        let recv2 = received.clone();
        // 容器自身 handler 记录（验证自身局部坐标不减自身 offset）
        let container_recv = std::sync::Arc::new(std::sync::Mutex::new(None::<(f32, f32)>));
        let crecv2 = container_recv.clone();
        let mut nodes = vec![
            LayoutNode::leaf(Modifier::new().vertical_scroll(scroll.clone()).size(100.0, 200.0).on_pointer_event(move |ev| {
                *crecv2.lock().unwrap() = Some(ev.position);
                false
            })),
            LayoutNode::leaf(Modifier::new().size(100.0, 60.0).on_pointer_event(move |ev| {
                *recv2.lock().unwrap() = Some(ev.position);
                false
            })),
        ];
        nodes[0].measured_size = Size::new(100.0, 200.0);
        nodes[1].measured_size = Size::new(100.0, 60.0);
        nodes[1].position = Offset::new(0.0, 100.0);
        nodes[0].children.push(1);
        (nodes, received, container_recv, scroll)
    }

    fn make_event() -> PointerEvent {
        PointerEvent {
            event_type: PointerEventType::Move,
            position: (0.0, 0.0), // dispatch 会覆盖
            scene_position: (0.0, 0.0),
            kind: PointerKind::Mouse { button: PointerButton::Primary },
            is_alt_pressed: false,
            is_ctrl_pressed: false,
            is_shift_pressed: false,
            is_meta_pressed: false,
        }
    }

    #[test]
    fn dispatch_local_coord_matches_scene_to_node_local_in_scroll() {
        let (nodes, received, _container_recv, _scroll) = build_scrolled_tree();
        let root = 0;
        // 场景点 (50,60)：滚动后应命中子节点（视觉顶边 y=50，范围 50..110）
        let path = hit_test(&nodes, root, 50.0, 60.0);
        assert_eq!(path, vec![0, 1], "滚动后子节点视觉范围应命中");

        let (expect_x, expect_y) = scene_to_node_local(&nodes, &path, 1, 50.0, 60.0);
        assert_eq!((expect_x, expect_y), (50.0, 10.0), "基线：scene_to_node_local 须扣除滚动");

        dispatch_ptr_event(&nodes, root, &path, &make_event(), (50.0, 60.0), None);
        let got = *received.lock().unwrap();
        assert_eq!(got, Some((expect_x, expect_y)),
            "dispatch 传给 handler 的局部坐标必须与 hit_test/scene_to_node_local 一致");
    }

    #[test]
    fn dispatch_coord_follows_scroll_offset_change() {
        let (nodes, received, _container_recv, scroll) = build_scrolled_tree();
        let root = 0;
        // offset 100：视觉顶边 y=0，场景 (50,50) → 本地 y=50
        scroll.offset.set(100.0);
        let path = hit_test(&nodes, root, 50.0, 50.0);
        assert_eq!(path, vec![0, 1], "offset=100 时场景 (50,50) 仍命中子节点");
        let (expect_x, expect_y) = scene_to_node_local(&nodes, &path, 1, 50.0, 50.0);
        assert_eq!((expect_x, expect_y), (50.0, 50.0));

        dispatch_ptr_event(&nodes, root, &path, &make_event(), (50.0, 50.0), None);
        let got = *received.lock().unwrap();
        assert_eq!(got, Some((expect_x, expect_y)),
            "滚动偏移变化后 dispatch 坐标须同步");
    }

    #[test]
    fn dispatch_scroll_container_own_coord_uses_ancestor_not_self() {
        // scroll 容器自身的局部坐标 = 场景 - 容器位置（自身 offset 不影响自身左上角）
        let (nodes, _received, container_recv, _scroll) = build_scrolled_tree();
        let root = 0;
        let path = vec![0]; // 只命中容器自身（子节点外区域）
        // 场景 (20,30)：容器本地 (20,30)，不应减自身 offset=50
        dispatch_ptr_event(&nodes, root, &path, &make_event(), (20.0, 30.0), None);
        let got = *container_recv.lock().unwrap();
        assert_eq!(got, Some((20.0, 30.0)),
            "scroll 容器自身局部坐标不应减自身 offset（offset 只影响子节点）");
    }

    /// 双轨（exp/modifier-node）：PointerNode 与枚举 PointerEvent 同场分发。
    /// 验证：①隧道外→内、冒泡内→外顺序；②node 收到与枚举相同的本地坐标；
    /// ③枚举消费（true）时 node 不再收到（旧行为优先）。
    #[test]
    fn dispatch_pointer_node_dual_track_order_and_coords() {
        use crate::modifier::PointerNode;
        use std::sync::{Arc, Mutex};
        #[derive(Debug)]
        struct Rec {
            pre: Arc<Mutex<Vec<(f32, f32)>>>,
            ev: Arc<Mutex<Vec<(f32, f32)>>>,
        }
        impl PointerNode for Rec {
            fn on_pre(&self, e: &PointerEvent) -> bool {
                self.pre.lock().unwrap().push(e.position);
                false
            }
            fn on_event(&self, e: &PointerEvent) -> bool {
                self.ev.lock().unwrap().push(e.position);
                false
            }
        }
        let pre_log = Arc::new(Mutex::new(Vec::new()));
        let ev_log = Arc::new(Mutex::new(Vec::new()));
        let enum_log = Arc::new(Mutex::new(Vec::new()));
        let enum_log2 = enum_log.clone();
        let mut nodes = vec![
            LayoutNode::leaf(Modifier::new().size(200.0, 200.0)),
            LayoutNode::leaf(
                Modifier::new()
                    .size(100.0, 100.0)
                    .on_pointer_event(move |e| {
                        enum_log2.lock().unwrap().push(e.position);
                        false
                    })
                    .pointer_node(Rec { pre: pre_log.clone(), ev: ev_log.clone() }),
            ),
        ];
        nodes[0].measured_size = Size::new(200.0, 200.0);
        nodes[1].measured_size = Size::new(100.0, 100.0);
        nodes[1].position = Offset::new(20.0, 30.0);
        nodes[0].children.push(1);
        let path = vec![0, 1];
        // 场景 (50,60) → 子本地 (30,30)
        dispatch_ptr_event(&nodes, 0, &path, &make_event(), (50.0, 60.0), None);
        assert_eq!(*enum_log.lock().unwrap(), vec![(30.0, 30.0)]);
        assert_eq!(*ev_log.lock().unwrap(), vec![(30.0, 30.0)], "node 冒泡坐标与枚举一致");
        assert_eq!(*pre_log.lock().unwrap(), vec![(30.0, 30.0)], "node 隧道坐标一致");
    }

    #[test]
    fn dispatch_pointer_node_enum_consume_blocks_node() {
        use crate::modifier::PointerNode;
        use std::sync::{Arc, Mutex};
        #[derive(Debug)]
        struct Rec {
            ev: Arc<Mutex<Vec<(f32, f32)>>>,
        }
        impl PointerNode for Rec {
            fn on_event(&self, e: &PointerEvent) -> bool {
                self.ev.lock().unwrap().push(e.position);
                false
            }
        }
        let ev_log = Arc::new(Mutex::new(Vec::new()));
        let mut nodes = vec![LayoutNode::leaf(
            Modifier::new()
                .size(100.0, 100.0)
                .on_pointer_event(|_| true) // 枚举消费
                .pointer_node(Rec { ev: ev_log.clone() }),
        )];
        nodes[0].measured_size = Size::new(100.0, 100.0);
        dispatch_ptr_event(&nodes, 0, &[0], &make_event(), (10.0, 10.0), None);
        assert!(ev_log.lock().unwrap().is_empty(), "枚举消费后同节点 node 不应收到");
    }

    /// P0-1 回归：内层装饰性 PointerNode 不偷外层 tap。
    /// gesture 目标选择只认 TapOn/DragOn 枚举——内层纯 node 时外层 TapOnTap 命中。
    #[test]
    fn pointer_node_does_not_steal_outer_tap_target() {
        use crate::layout::node::hit_test;
        use crate::modifier::PointerNode;
        use std::sync::{Arc, Mutex};
        #[derive(Debug)]
        struct Deco;
        impl PointerNode for Deco {}
        let log = Arc::new(Mutex::new(Vec::new()));
        let log2 = log.clone();
        let mut nodes = vec![
            LayoutNode::leaf(Modifier::new().size(200.0, 200.0).on_tap(move |_| {
                log2.lock().unwrap().push("outer-tap".to_string());
            })),
            LayoutNode::leaf(
                Modifier::new()
                    .size(100.0, 100.0)
                    .pointer_node(Deco), // 装饰性：恒返 false，不消费
            ),
        ];
        nodes[0].measured_size = Size::new(200.0, 200.0);
        nodes[1].measured_size = Size::new(100.0, 100.0);
        nodes[1].position = Offset::new(20.0, 20.0);
        nodes[0].children.push(1);
        // 点内层 (50,50)：命中路径 [0,1]
        let path = hit_test(&nodes, 0, 50.0, 50.0);
        assert_eq!(path, vec![0, 1]);
        // gesture 目标选择（与 gesture_down 同逻辑）：最内层 has_gesture 者
        let gid = path
            .iter()
            .rev()
            .find(|&&i| nodes[i].modifier.has_gesture())
            .copied();
        assert_eq!(gid, Some(0), "内层纯 PointerNode 不进路由，外层 tap 命中");
        // fire_gesture_action 语义：gid=0 的枚举 TapOnTap 可触发
        assert!(nodes[0].modifier.has_gesture());
        assert!(!nodes[1].modifier.has_gesture());
    }
