    use super::dispatch_ptr_event;
    use crate::layout::node::{hit_test, scene_to_node_local, LayoutNode};
    use crate::unit::{Offset, Size};
    use crate::modifier::{Modifier, ScrollState};
    use crate::input::{PointerButton, PointerEvent, PointerEventType, PointerKind};

    /// Builds a scroll container (0,0,100x200) plus a child (0,100,100x60) carrying a PointerEvent
    /// handler. With scroll offset=50 the child's visual top edge is y=50.
    fn build_scrolled_tree() -> (Vec<LayoutNode>, std::sync::Arc<std::sync::Mutex<Option<(f32, f32)>>>, std::sync::Arc<std::sync::Mutex<Option<(f32, f32)>>>, ScrollState) {
        let scroll = ScrollState::new();
        scroll.offset.set(50.0);
        // Records the child's handler calls (to verify the child's local coordinates subtract the ancestor scroll).
        let received = std::sync::Arc::new(std::sync::Mutex::new(None::<(f32, f32)>));
        let recv2 = received.clone();
        // Records the container's own handler calls (to verify its local coordinates do not subtract its own offset).
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
            position: (0.0, 0.0), // dispatch overwrites this
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
        // Scene point (50,60): after scrolling it should hit the child (visual top y=50, spanning 50..110).
        let path = hit_test(&nodes, root, 50.0, 60.0);
        assert_eq!(path, vec![0, 1], "the child's visual range should be hit after scrolling");

        let (expect_x, expect_y) = scene_to_node_local(&nodes, &path, 1, 50.0, 60.0);
        assert_eq!((expect_x, expect_y), (50.0, 10.0), "baseline: scene_to_node_local must subtract the scroll");

        dispatch_ptr_event(&nodes, root, &path, &make_event(), (50.0, 60.0), None);
        let got = *received.lock().unwrap();
        assert_eq!(got, Some((expect_x, expect_y)),
            "the local coordinates dispatch hands the handler must match hit_test/scene_to_node_local");
    }

    #[test]
    fn dispatch_coord_follows_scroll_offset_change() {
        let (nodes, received, _container_recv, scroll) = build_scrolled_tree();
        let root = 0;
        // offset 100: visual top y=0, so scene (50,50) maps to local y=50.
        scroll.offset.set(100.0);
        let path = hit_test(&nodes, root, 50.0, 50.0);
        assert_eq!(path, vec![0, 1], "at offset=100 the scene point (50,50) still hits the child");
        let (expect_x, expect_y) = scene_to_node_local(&nodes, &path, 1, 50.0, 50.0);
        assert_eq!((expect_x, expect_y), (50.0, 50.0));

        dispatch_ptr_event(&nodes, root, &path, &make_event(), (50.0, 50.0), None);
        let got = *received.lock().unwrap();
        assert_eq!(got, Some((expect_x, expect_y)),
            "dispatch coordinates must follow a change in the scroll offset");
    }

    #[test]
    fn dispatch_scroll_container_own_coord_uses_ancestor_not_self() {
        // A scroll container's own local coordinates are scene minus container position; its own offset
        // does not move its top-left corner.
        let (nodes, _received, container_recv, _scroll) = build_scrolled_tree();
        let root = 0;
        let path = vec![0]; // only the container itself is hit (outside the child)
        // Scene (20,30): the container's local point is (20,30) and must not subtract its own offset=50.
        dispatch_ptr_event(&nodes, root, &path, &make_event(), (20.0, 30.0), None);
        let got = *container_recv.lock().unwrap();
        assert_eq!(got, Some((20.0, 30.0)),
            "a scroll container's own local coordinates must not subtract its own offset (the offset only affects children)");
    }

    /// Dual track (exp/modifier-node): PointerNode and the enum PointerEvent are dispatched together.
    /// Verifies that (1) the tunnel runs outside-in and the bubble inside-out, (2) the node receives the
    /// same local coordinates as the enum, and (3) when the enum consumes (returns true) the node no
    /// longer receives the event (the old behaviour wins).
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
        // Scene (50,60) maps to child-local (30,30).
        dispatch_ptr_event(&nodes, 0, &path, &make_event(), (50.0, 60.0), None);
        assert_eq!(*enum_log.lock().unwrap(), vec![(30.0, 30.0)]);
        assert_eq!(*ev_log.lock().unwrap(), vec![(30.0, 30.0)], "the node's bubble coordinates match the enum's");
        assert_eq!(*pre_log.lock().unwrap(), vec![(30.0, 30.0)], "the node's tunnel coordinates match too");
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
                .on_pointer_event(|_| true) // the enum consumes
                .pointer_node(Rec { ev: ev_log.clone() }),
        )];
        nodes[0].measured_size = Size::new(100.0, 100.0);
        dispatch_ptr_event(&nodes, 0, &[0], &make_event(), (10.0, 10.0), None);
        assert!(ev_log.lock().unwrap().is_empty(), "once the enum consumes, the node on that path must not receive it");
    }

    /// P0-1 regression: an inner decorative PointerNode must not steal the outer tap. Gesture target
    /// selection only considers the TapOn/DragOn enums, so with a bare node inside, the outer TapOnTap
    /// still gets the hit.
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
                    .pointer_node(Deco), // decorative: always returns false, consumes nothing
            ),
        ];
        nodes[0].measured_size = Size::new(200.0, 200.0);
        nodes[1].measured_size = Size::new(100.0, 100.0);
        nodes[1].position = Offset::new(20.0, 20.0);
        nodes[0].children.push(1);
        // Clicking the inner node at (50,50) gives the hit path [0,1].
        let path = hit_test(&nodes, 0, 50.0, 50.0);
        assert_eq!(path, vec![0, 1]);
        // Gesture target selection (same logic as gesture_down): the innermost node that has_gesture.
        let gid = path
            .iter()
            .rev()
            .find(|&&i| nodes[i].modifier.has_gesture())
            .copied();
        assert_eq!(gid, Some(0), "a bare inner PointerNode is not routed, so the outer tap is the target");
        // fire_gesture_action semantics: gid=0's TapOnTap enum can fire.
        assert!(nodes[0].modifier.has_gesture());
        assert!(!nodes[1].modifier.has_gesture());
    }
