    use super::dispatch_key_to_focus;
    use super::PerWindow;
    use crate::layout::Constraints;
    use crate::modifier::{KeyNode, Modifier};
    use crate::input::{KbEvent, KbEventType};
    use crate::theme::ThemeColors;
    use std::sync::{Arc, Mutex};

    #[derive(Debug)]
    struct Rec {
        pre: Arc<Mutex<Vec<String>>>,
        ev: Arc<Mutex<Vec<String>>>,
    }
    impl KeyNode for Rec {
        fn on_pre(&self, _e: &KbEvent) -> bool {
            self.pre.lock().unwrap().push("pre".into());
            false
        }
        fn on_event(&self, _e: &KbEvent) -> bool {
            self.ev.lock().unwrap().push("event".into());
            false
        }
    }

    fn make_key() -> KbEvent {
        KbEvent {
            key: winit::keyboard::Key::Character("a".into()),
            event_type: KbEventType::KeyDown,
            is_alt_pressed: false,
            is_ctrl_pressed: false,
            is_shift_pressed: false,
            is_meta_pressed: false,
            repeat: false,
        }
    }

    #[test]
    fn focus_by_node_id_updates_the_window_bookkeeping_and_refuses_the_unfocusable() {
        let theme = ThemeColors::default_light();
        let mut pw = PerWindow::new(Box::new(|_| {}), 300.0, 200.0, theme);
        pw.content = Box::new(|ctx| {
            // A container groups the three Texts into one tree the layout root can reach.
            crate::layout::Column::new().build(ctx, |ctx| {
                for label in ["A", "B"] {
                    crate::components::Text::new(label)
                        .modifier(Modifier::new().focusable())
                        .build(ctx);
                }
                crate::components::Text::new("C")
                    .modifier(
                        Modifier::new()
                            .focusable()
                            .focus_properties_can_focus(false),
                    )
                    .build(ctx);
            });
        });
        let content = std::mem::replace(&mut pw.content, Box::new(|_| {}));
        pw.composer.recompose(|ctx| content(ctx));
        pw.content = content;
        pw.composer.layout(Constraints::new(0.0, 300.0, 0.0, 200.0));

        let (a_id, b_id, c_id, a_slot) = {
            let root = pw.composer.layout_root_idx().expect("layout root");
            let nodes = pw.composer.arena_nodes();
            let mut focusable = Vec::new();
            crate::layout::node::collect_focusable_ids(nodes, root, &mut focusable);
            assert_eq!(focusable.len(), 2, "canFocus = false keeps C out of the walk");
            let c = nodes
                .iter()
                .find(|node| node.modifier.focus_can_focus() == Some(false))
                .expect("C exists")
                .id;
            let a_slot = nodes
                .iter()
                .find(|node| node.id == focusable[0])
                .map(|node| node.slot_key);
            (focusable[0], focusable[1], c, a_slot)
        };

        assert!(pw.focus_by_node_id(a_id), "A is focusable");
        assert_eq!(pw.focused_id, Some(a_id));
        assert_eq!(pw.focused_slot_key, a_slot, "the slot key follows the id");

        // The UIA path and the debug protocol both land here: a refused target must not disturb
        // the focus the user already has.
        assert!(!pw.focus_by_node_id(c_id), "C is not focusable");
        assert_eq!(pw.focused_id, Some(a_id));
        assert!(!pw.focus_by_node_id(999_999), "a missing id is refused");

        assert!(pw.focus_by_node_id(b_id));
        assert_eq!(pw.focused_id, Some(b_id));
        let root = pw.composer.layout_root_idx().unwrap();
        assert_eq!(
            crate::layout::node::get_focus_id(pw.composer.arena_nodes(), root),
            Some(b_id),
            "the arena focus agrees with the bookkeeping"
        );
    }

    #[test]
    fn a_refused_slot_restore_clears_the_stale_focus_flags() {
        let theme = ThemeColors::default_light();
        let mut pw = PerWindow::new(Box::new(|_| {}), 300.0, 200.0, theme);
        pw.content = Box::new(|ctx| {
            crate::layout::Column::new().build(ctx, |ctx| {
                for unfocusable in [false, true] {
                    let key = ctx.next_key();
                    let modifier = if unfocusable {
                        Modifier::new()
                            .focusable()
                            .focus_properties_can_focus(false)
                    } else {
                        Modifier::new().focusable()
                    };
                    ctx.start_leaf(key, modifier);
                    ctx.end_node();
                }
            });
        });
        let content = std::mem::replace(&mut pw.content, Box::new(|_| {}));
        pw.composer.recompose(|ctx| content(ctx));
        pw.content = content;
        pw.composer.layout(Constraints::new(0.0, 300.0, 0.0, 200.0));

        let (a_id, refused_slot) = {
            let root = pw.composer.layout_root_idx().expect("layout root");
            let nodes = pw.composer.arena_nodes();
            let mut focusable = Vec::new();
            crate::layout::node::collect_focusable_ids(nodes, root, &mut focusable);
            let a_id = focusable[0];
            let refused = nodes
                .iter()
                .find(|node| node.modifier.focus_can_focus() == Some(false))
                .expect("the canFocus = false leaf exists");
            (a_id, refused.slot_key)
        };

        // The slot still resolves, but its node no longer accepts focus. The failure branch must
        // clear the tree flags, not only the window fields.
        assert!(pw.focus_by_node_id(a_id));
        pw.focused_slot_key = Some(refused_slot);
        pw.restore_focus_by_slot_key();
        assert_eq!(pw.focused_id, None);
        assert_eq!(pw.focused_slot_key, None);
        let root = pw.composer.layout_root_idx().unwrap();
        assert_eq!(
            crate::layout::node::get_focus_id(pw.composer.arena_nodes(), root),
            None,
            "the arena must not keep a focused node the window has forgotten"
        );

        // The other failure path: the slot is gone entirely.
        assert!(pw.focus_by_node_id(a_id));
        pw.focused_slot_key = Some(u64::MAX);
        pw.restore_focus_by_slot_key();
        assert_eq!(pw.focused_id, None);
        assert_eq!(pw.focused_slot_key, None);
        assert_eq!(
            crate::layout::node::get_focus_id(pw.composer.arena_nodes(), root),
            None
        );
    }

    #[test]
    fn focus_by_node_id_can_reach_an_overlay_arena() {
        let theme = ThemeColors::default_light();
        let mut pw = PerWindow::new(Box::new(|_| {}), 300.0, 200.0, theme);
        pw.content = Box::new(|ctx| {
            crate::layout::Column::new().build(ctx, |ctx| {
                let key = ctx.next_key();
                ctx.start_leaf(key, Modifier::new().focusable());
                ctx.end_node();
            });
        });
        let content = std::mem::replace(&mut pw.content, Box::new(|_| {}));
        pw.composer.recompose(|ctx| content(ctx));
        pw.content = content;
        pw.composer.layout(Constraints::new(0.0, 300.0, 0.0, 200.0));
        let main_id = {
            let root = pw.composer.layout_root_idx().unwrap();
            let nodes = pw.composer.arena_nodes();
            let mut focusable = Vec::new();
            crate::layout::node::collect_focusable_ids(nodes, root, &mut focusable);
            focusable[0]
        };
        assert!(pw.focus_by_node_id(main_id));

        // A second composer stands in for an overlay layer: UIA hands the node id of the focused
        // semantics tree, which may live there.
        let mut overlay = crate::runtime::composer::Composer::new();
        overlay.compose(|ctx| {
            crate::layout::Column::new().build(ctx, |ctx| {
                let key = ctx.next_key();
                ctx.start_leaf(key, Modifier::new().focusable());
                ctx.end_node();
            });
        });
        overlay.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        let overlay_id = {
            let root = overlay.layout_root_idx().unwrap();
            let nodes = overlay.arena_nodes();
            let mut focusable = Vec::new();
            crate::layout::node::collect_focusable_ids(nodes, root, &mut focusable);
            focusable[0]
        };
        let desc = crate::overlay::OverlayDesc {
            id: crate::overlay::next_overlay_id(),
            anchor_slot: None,
            direction: None,
            position: crate::overlay::PopupPosition::Center,
            offset: (0.0, 0.0),
            anchor_slide: None,
            modal: false,
            focus_scope: false,
            dismiss_on_outside: false,
            dismiss_on_back_press: false,
            dismiss_defers_to_content: false,
            click_passthrough: false,
            on_dismiss: None,
            fit_around_anchor: false,
            match_anchor_width: false,
            enter_anim: None,
            exit_anim: None,
            content: Box::new(|_| {}),
            local_snapshot: Vec::new(),
        };
        pw.overlay.layers.push(
            crate::app::overlay_host::OverlayWindow::new_with_composer(desc, overlay),
        );

        assert!(
            pw.focus_by_node_id(overlay_id),
            "the overlay arena must be searched too"
        );
        assert_eq!(pw.focused_id, None, "overlay focus clears the page");
        assert_eq!(pw.focused_slot_key, None);
        assert_eq!(pw.overlay.layers[0].focused_id, Some(overlay_id));
        assert!(pw.overlay.layers[0].focused_slot_key.is_some());
        let root = pw.composer.layout_root_idx().unwrap();
        assert_eq!(
            crate::layout::node::get_focus_id(pw.composer.arena_nodes(), root),
            None,
            "the page arena loses focus when the overlay takes it"
        );
    }

    #[test]
    fn key_node_receives_preview_and_bubble() {
        let pre_log = Arc::new(Mutex::new(Vec::new()));
        let ev_log = Arc::new(Mutex::new(Vec::new()));
        let enum_log = Arc::new(Mutex::new(Vec::new()));
        let enum_log2 = enum_log.clone();
        let theme = ThemeColors::default_light();
        let mut pw = PerWindow::new(Box::new(|_| {}), 400.0, 300.0, theme);
        let (pre_c, ev_c) = (pre_log.clone(), ev_log.clone());
        pw.content = Box::new(move |ctx| {
            let enum_log3 = enum_log2.clone();
            crate::components::Text::new("k")
                .modifier(
                    Modifier::new()
                        .focusable()
                        .on_key_event(move |_| {
                            enum_log3.lock().unwrap().push("enum".to_string());
                            false
                        })
                        .key_node(Rec { pre: pre_c.clone(), ev: ev_c.clone() }),
                )
                .build(ctx);
        });
        // 跑一帧组合+布局（借 recompose_layout_render 的组合部分逻辑）
        let content = std::mem::replace(&mut pw.content, Box::new(|_| {}));
        pw.composer.recompose(|ctx| content(ctx));
        pw.content = content;
        pw.composer.layout(Constraints::new(0.0, 400.0, 0.0, 300.0));
        // 聚焦唯一节点
        if let Some(r) = pw.composer.layout_root_idx() {
            let nodes = pw.composer.arena_nodes_mut();
            crate::layout::node::clear_focus(nodes, r);
            // 首个 focusable 即 Text 节点（arena 序遍历）
            let arena = pw.composer.arena_nodes();
            let mut found = None;
            let mut stack = vec![r];
            while let Some(idx) = stack.pop() {
                if found.is_none()
                    && arena[idx].modifier.elements().iter().any(|el| {
                        matches!(el, crate::modifier::ModifierElement::Focusable { .. })
                    })
                {
                    found = Some(arena[idx].id);
                }
                for &c in arena[idx].children.iter().rev() {
                    stack.push(c);
                }
            }
            drop(arena);
            if let Some(id) = found {
                let nodes = pw.composer.arena_nodes_mut();
                crate::layout::node::set_focus_by_id(nodes, r, id);
                pw.focused_id = Some(id);
            }
        }
        assert!(pw.focused_id.is_some(), "应聚焦到 Text 节点");
        let handled = dispatch_key_to_focus(&pw, &make_key());
        assert!(!handled, "都不消费应返回 false");
        assert_eq!(pre_log.lock().unwrap().len(), 1, "隧道应到 node 一次");
        assert_eq!(ev_log.lock().unwrap().len(), 1, "冒泡应到 node 一次");
        assert_eq!(enum_log.lock().unwrap().len(), 1, "枚举应收到一次");
    }
