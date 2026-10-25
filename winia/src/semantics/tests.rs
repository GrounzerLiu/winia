    use super::*;
    use crate::runtime::composer::Composer;
    use crate::layout::Constraints;
    use crate::components::button::Button;
    use crate::components::checkbox::TriStateCheckbox;
    use crate::layout::components::{Column, Row};
    use crate::components::radio_button::RadioButton;
    use crate::components::switch::Switch;
    use crate::components::text::Text;
    use crate::theme::WiniaTheme;
    use crate::modifier::Modifier;

    /// Compose a tree and return its semantics elements.
    fn tree_of(build: impl FnOnce(&mut crate::runtime::composer::ComposeCtx)) -> Vec<SemanticsNode> {
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::light(ctx, |ctx| build(ctx));
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        let root = composer.layout_root_idx().expect("layout root");
        semantics_tree(composer.arena_nodes(), root)
    }

    /// The one element named `name`.
    fn named<'a>(tree: &'a [SemanticsNode], name: &str) -> &'a SemanticsNode {
        let mut found = None;
        for node in tree {
            if let Some(hit) = node.find_by_name(name) {
                found = Some(hit);
                break;
            }
        }
        found.unwrap_or_else(|| panic!("no semantics element named {name:?} in {:?}", names(tree)))
    }

    fn names(tree: &[SemanticsNode]) -> Vec<String> {
        let mut out = Vec::new();
        for node in tree {
            node.walk(&mut |n| {
                out.push(format!("{:?}/{:?}", n.role.map(|r| r.name()), n.name));
            });
        }
        out
    }

    /// A one-window snapshot around `main` — what the platform bridge's change detection reads.
    fn snapshot_of(main: Vec<SemanticsNode>) -> WindowSemantics {
        WindowSemantics {
            main,
            overlays: Vec::new(),
            window_size: (100.0, 100.0),
            scale_factor: 1.0,
        }
    }

    // ── Collections (Compose's CollectionInfo / CollectionItemInfo) ──

    /// A declared collection reaches the tree with its count AND keeps its children: the items are
    /// the collection's children, not something folded into it.
    #[test]
    fn a_declared_collection_reaches_the_tree_and_the_json() {
        let tree = tree_of(|ctx| {
            Column::new()
                .modifier(Modifier::new().semantics(
                    SemanticsConfig::new().collection_info(CollectionInfo::rows(200)),
                ))
                .build(ctx, |ctx| {
                    Text::new("one").build(ctx);
                    Text::new("two").build(ctx);
                });
        });
        let list = tree
            .iter()
            .find(|node| node.collection.is_some())
            .expect("the collection element is in the tree");
        assert_eq!(list.collection, Some(CollectionInfo::rows(200)));
        assert_eq!(list.children.len(), 2, "the items stay children of the collection");
        assert!(list.collection_item.is_none());
        let json = semantics_json(&tree);
        assert!(
            json.contains("\"collection\":{\"rows\":200,\"columns\":0}"),
            "the debug tree reports the count: {json}"
        );
    }

    /// An item position is a declaration of its own: a node that only says "I am row 1 of the
    /// enclosing list" is an element, not something transparent.
    #[test]
    fn a_collection_item_is_an_element_of_its_own() {
        let tree = tree_of(|ctx| {
            Column::new()
                .modifier(Modifier::new().semantics(
                    SemanticsConfig::new().collection_info(CollectionInfo::rows(3)),
                ))
                .build(ctx, |ctx| {
                    for i in 0..3usize {
                        Column::new()
                            .modifier(Modifier::new().semantics(
                                SemanticsConfig::new()
                                    .collection_item_info(CollectionItemInfo::new(i)),
                            ))
                            .build(ctx, |ctx| {
                                Text::new(format!("row {i}")).build(ctx);
                            });
                    }
                });
        });
        let list = tree
            .iter()
            .find(|node| node.collection.is_some())
            .expect("the collection element");
        let positions: Vec<usize> = list
            .children
            .iter()
            .filter_map(|node| node.collection_item.map(|info| info.index))
            .collect();
        assert_eq!(positions, vec![0, 1, 2], "each item reports its own position");
        assert!(
            semantics_json(&tree).contains("\"collectionItem\":{\"index\":1,\"span\":1}"),
            "and the position is in the debug tree"
        );
    }

    /// A caller's declaration wins field by field, and an undeclared one keeps the component's.
    #[test]
    fn a_caller_collection_wins_over_the_components() {
        let component = SemanticsConfig::new().collection_info(CollectionInfo::rows(200));
        let caller = SemanticsConfig::new().collection_info(CollectionInfo::columns(7));
        assert_eq!(
            caller.or(component.clone()).collection_info_value(),
            Some(CollectionInfo::columns(7)),
            "a filtered/restated collection replaces the component's"
        );
        assert_eq!(
            SemanticsConfig::new().or(component).collection_info_value(),
            Some(CollectionInfo::rows(200)),
            "and saying nothing keeps it"
        );
        // The item position merges the same way.
        let with_item = SemanticsConfig::new().collection_item_info(CollectionItemInfo::spanning(3, 2));
        assert_eq!(
            SemanticsConfig::new().or(with_item).collection_item_info_value(),
            Some(CollectionItemInfo::spanning(3, 2))
        );
    }

    #[test]
    fn collection_info_lengths_and_spans_are_sane() {
        assert_eq!(CollectionInfo::rows(200).len(), 200);
        assert!(CollectionInfo::rows(200).is_vertical());
        assert!(!CollectionInfo::rows(0).is_vertical(), "an empty collection has no axis");
        assert!(CollectionInfo::rows(0).is_empty());
        assert_eq!(CollectionInfo::columns(7).len(), 7);
        assert!(!CollectionInfo::columns(7).is_vertical());
        assert_eq!(CollectionItemInfo::new(3).span, 1);
        assert_eq!(CollectionItemInfo::spanning(3, 0).span, 1, "a span is at least one");
    }

    #[test]
    fn the_structure_fingerprint_tracks_shape_not_content() {
        // Frames of ONE composer, which is what a bridge ever compares (a different composer means a
        // different window, and node ids come from a process-wide counter).
        let mut composer = Composer::new();
        let label = crate::State::new("a".to_string());
        let extra = crate::State::new(false);

        let mut frame = |composer: &mut Composer| {
            let label = label.clone();
            let extra = extra.clone();
            composer.compose(|ctx| {
                WiniaTheme::light(ctx, |ctx| {
                    Column::new().build(ctx, |ctx| {
                        Text::new(label.get()).build(ctx);
                        if extra.get() {
                            Text::new("second".to_string()).build(ctx);
                        }
                    });
                });
            });
            composer.layout(Constraints::new(0.0, 100.0, 0.0, 100.0));
            let root = composer.layout_root_idx().expect("layout root");
            snapshot_of(semantics_tree(composer.arena_nodes(), root))
        };

        let first = frame(&mut composer);
        // The CONTROL the fingerprint has to pass: nothing changed, so nothing is reported. An id that
        // drifted every frame would make this fail, and with it every provider cache in the bridge.
        let again = frame(&mut composer);
        assert_eq!(
            first.structure_fingerprint(),
            again.structure_fingerprint(),
            "an unchanged tree has the same fingerprint — node ids included, which is the invariant the \
             bridge's cached providers rely on"
        );
        let ids_of = |snapshot: &WindowSemantics| -> Vec<u64> {
            snapshot.flatten().into_iter().map(|(node, _)| node.node_id).collect()
        };
        assert_eq!(ids_of(&first), ids_of(&again), "and the ids themselves are stable across frames");

        // A renamed label is a property change, not a structural one.
        label.set("b".to_string());
        let renamed = frame(&mut composer);
        assert_eq!(
            first.structure_fingerprint(),
            renamed.structure_fingerprint(),
            "a different label is not structure — reporting it as such would make every counter tick \
             look like a rebuilt tree"
        );

        // An element appearing IS structure.
        extra.set(true);
        let added = frame(&mut composer);
        assert_ne!(
            first.structure_fingerprint(),
            added.structure_fingerprint(),
            "an element appearing is a structural change"
        );
    }

    #[test]
    fn the_structure_fingerprint_notices_an_overlay() {
        let base = snapshot_of(tree_of(|ctx| {
            Text::new("page").build(ctx);
        }));
        let mut with_overlay = base.clone();
        with_overlay.overlays.push(OverlaySemantics {
            id: 7,
            origin: (10.0, 10.0),
            nodes: vec![SemanticsNode {
                node_id: 99,
                role: Some(SemanticsRole::Dialog),
                name: Some("Confirm".into()),
                state: SemanticsState::new(),
                actions: Vec::new(),
                clickable: false,
                focused: false,
                live_region: LiveRegionMode::Off,
                selectable_group: false,
                collection: None,
                collection_item: None,
                bounds: (0.0, 0.0, 50.0, 50.0),
                children: Vec::new(),
            }],
        });
        assert_ne!(
            base.structure_fingerprint(),
            with_overlay.structure_fingerprint(),
            "a dialog appearing is the structural change a client most needs to hear about"
        );
    }

    #[test]
    fn the_focused_path_addresses_the_focused_element() {
        // Focus is placed on a click target, which is what Tab reaches; the path has to address it so
        // a bridge can hand UIA a provider for exactly that element.
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::light(ctx, |ctx| {
                Column::new().build(ctx, |ctx| {
                    Text::new("First").build(ctx);
                    Button::filled().on_click(|| {}).build(ctx, |ctx| {
                        Text::new("Second").build(ctx);
                    });
                });
            });
        });
        composer.layout(Constraints::new(0.0, 200.0, 0.0, 200.0));
        let root = composer.layout_root_idx().unwrap();

        // Nothing is focused before anything moves the ring.
        assert_eq!(
            snapshot_of(semantics_tree(composer.arena_nodes(), root)).focused_path(),
            None
        );

        // `focus_next` writes the flags in place (the nodes are not copyable), so it borrows the arena.
        assert!(
            crate::layout::node::focus_next(composer.arena_nodes_mut(), root),
            "the button is focusable"
        );
        let after = snapshot_of(semantics_tree(composer.arena_nodes(), root));
        let path = after.focused_path().expect("a focusable took the ring");
        let (node, _) = after.resolve(&path).expect("the path addresses an element");
        assert!(node.clickable, "and it leads to the element that owns the focus");
    }

    #[test]
    fn text_appears_as_a_named_element() {
        let tree = tree_of(|ctx| {
            Text::new("Hello semantics").build(ctx);
        });
        let node = named(&tree, "Hello semantics");
        assert_eq!(node.role, None, "plain text carries a name and no role");
        assert!(!node.clickable);
    }

    #[test]
    fn a_container_that_declares_nothing_stays_out_of_the_tree() {
        // A Column between the text and the root must not become an element: a screen reader should
        // hear the text, not "column".
        let tree = tree_of(|ctx| {
            Column::new().build(ctx, |ctx| {
                Text::new("inside").build(ctx);
            });
        });
        assert_eq!(tree.len(), 1, "only the text: {:?}", names(&tree));
        assert_eq!(tree[0].name.as_deref(), Some("inside"));
    }

    #[test]
    fn a_button_is_one_element_named_by_the_text_it_contains() {
        // The claim rule: the click target absorbs its subtree, so the label becomes the button's
        // name instead of hanging under it as an unnamed child.
        let tree = tree_of(|ctx| {
            Button::filled().on_click(|| {}).build(ctx, |ctx| {
                Text::new("Submit").build(ctx);
            });
        });
        let button = named(&tree, "Submit");
        assert_eq!(button.role, Some(SemanticsRole::Button));
        assert!(button.clickable);
        assert!(button.children.is_empty(), "the text was absorbed: {:?}", names(&tree));
    }

    #[test]
    fn a_click_target_publishes_its_enabled_state() {
        let tree = tree_of(|ctx| {
            Button::filled().on_click(|| {}).enabled(false).build(ctx, |ctx| {
                Text::new("Nope").build(ctx);
            });
        });
        assert_eq!(named(&tree, "Nope").state.enabled_value(), Some(false));
    }

    #[test]
    fn a_checkbox_reports_its_tri_state() {
        for (value, expect) in [
            (ToggleableState::Off, "off"),
            (ToggleableState::On, "on"),
            (ToggleableState::Indeterminate, "indeterminate"),
        ] {
            let tree = tree_of(|ctx| {
                TriStateCheckbox::new(value).on_click(|| {}).build(ctx);
            });
            let node = &tree[0];
            assert_eq!(node.role, Some(SemanticsRole::Checkbox));
            assert_eq!(node.state.checked_value(), Some(value), "case {expect}");
        }
    }

    #[test]
    fn a_switch_reports_checked_and_a_radio_reports_selected() {
        // Compose draws this line: a switch/checkbox is checked, a radio is selected, and the
        // difference is what a screen reader announces.
        let tree = tree_of(|ctx| {
            Switch::new(true).on_checked_change(|_| {}).build(ctx, |_| {});
        });
        assert_eq!(tree[0].role, Some(SemanticsRole::Switch));
        assert_eq!(tree[0].state.checked_value(), Some(ToggleableState::On));
        assert_eq!(tree[0].state.selected_value(), None);

        let tree = tree_of(|ctx| {
            RadioButton::new(true).on_click(|| {}).build(ctx);
        });
        assert_eq!(tree[0].role, Some(SemanticsRole::RadioButton));
        assert_eq!(tree[0].state.selected_value(), Some(true));
        assert_eq!(tree[0].state.checked_value(), None);
    }

    #[test]
    fn a_caller_declaration_wins_per_field_without_wiping_the_components_role() {
        let tree = tree_of(|ctx| {
            Button::filled()
                .on_click(|| {})
                .modifier(Modifier::new().semantics(
                    SemanticsConfig::new().content_description("Save the document"),
                ))
                .build(ctx, |ctx| {
                    Text::new("Save").build(ctx);
                });
        });
        let button = named(&tree, "Save the document");
        assert_eq!(
            button.role,
            Some(SemanticsRole::Button),
            "the description must not have erased the role"
        );
    }

    #[test]
    fn a_decorative_icon_stays_out_and_a_described_one_is_an_image() {
        let tree = tree_of(|ctx| {
            Column::new().build(ctx, |ctx| {
                crate::components::icon::Icon::svg_path("M0 0 L24 24").build(ctx);
                crate::components::icon::Icon::svg_path("M0 0 L24 24")
                    .content_description("Close")
                    .build(ctx);
            });
        });
        assert_eq!(tree.len(), 1, "only the described icon: {:?}", names(&tree));
        assert_eq!(tree[0].role, Some(SemanticsRole::Image));
        assert_eq!(tree[0].name.as_deref(), Some("Close"));
    }

    #[test]
    fn bounds_are_absolute_and_include_the_containers_offset() {
        // A press or an OS query needs real screen coordinates, not parent-relative ones.
        let tree = tree_of(|ctx| {
            Column::new()
                .modifier(Modifier::new().padding(24.0))
                .build(ctx, |ctx| {
                    Text::new("offset").build(ctx);
                });
        });
        let (x, y, w, h) = tree[0].bounds;
        assert_eq!((x, y), (24.0, 24.0), "the padding must be folded into the position");
        assert!(w > 0.0 && h > 0.0);
    }

    #[test]
    fn click_node_by_id_fires_the_action_the_element_reports() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let hits = Arc::new(AtomicUsize::new(0));
        let counter = hits.clone();
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::light(ctx, |ctx| {
                Button::filled()
                    .on_click(move || {
                        counter.fetch_add(1, Ordering::SeqCst);
                    })
                    .build(ctx, |ctx| {
                        Text::new("Press me").build(ctx);
                    });
            });
        });
        composer.layout(Constraints::new(0.0, 400.0, 0.0, 400.0));
        let root = composer.layout_root_idx().unwrap();
        let tree = semantics_tree(composer.arena_nodes(), root);
        let button = named(&tree, "Press me");

        assert!(click_node_by_id(composer.arena_nodes(), button.node_id));
        assert_eq!(hits.load(Ordering::SeqCst), 1);
        // An id that names no node reports "nothing happened" rather than panicking.
        assert!(!click_node_by_id(composer.arena_nodes(), u64::MAX));
    }

    #[test]
    fn merging_is_implied_by_a_click_target_and_available_explicitly() {
        // An explicit claim on a non-interactive node: a card that reads as one label.
        let tree = tree_of(|ctx| {
            Row::new()
                .modifier(Modifier::new().semantics(
                    SemanticsConfig::new()
                        .role(SemanticsRole::ProgressBar)
                        .merge_descendants(true),
                ))
                .build(ctx, |ctx| {
                    Text::new("Loading").build(ctx);
                    Text::new("50%").build(ctx);
                });
        });
        assert_eq!(tree.len(), 1, "{:?}", names(&tree));
        assert_eq!(tree[0].name.as_deref(), Some("Loading 50%"));
        assert_eq!(tree[0].role, Some(SemanticsRole::ProgressBar));
        assert!(tree[0].children.is_empty());
    }

    #[test]
    fn an_absorbed_state_survives_the_merge() {
        // A clickable wrapper around a switch: the wrapper answers the click, the switch answers the
        // state, and the element has to carry both.
        let tree = tree_of(|ctx| {
            Row::new()
                .modifier(Modifier::new().clickable(|| {}))
                .build(ctx, |ctx| {
                    Switch::new(true).on_checked_change(|_| {}).build(ctx, |_| {});
                });
        });
        assert_eq!(tree.len(), 1, "one clickable element: {:?}", names(&tree));
        assert!(tree[0].clickable);
        assert_eq!(tree[0].state.checked_value(), Some(ToggleableState::On));
    }

    #[test]
    fn a_live_region_keeps_its_mode_through_a_merge() {
        // The shape a snackbar has: a claimed subtree, announced unprompted. The mode has to survive
        // the claim — the element a client is told about is the wrapper, not the text inside it.
        let tree = tree_of(|ctx| {
            Row::new()
                .modifier(Modifier::new().semantics(
                    SemanticsConfig::new()
                        .merge_descendants(true)
                        .live_region(LiveRegionMode::Polite),
                ))
                .build(ctx, |ctx| {
                    Text::new("Message sent").build(ctx);
                });
        });
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].live_region, LiveRegionMode::Polite);
        assert_eq!(tree[0].effective_name().as_deref(), Some("Message sent"), "and it is named");
    }

    #[test]
    fn an_ordinary_element_is_not_a_live_region() {
        // The default matters as much as the setting: if everything announced itself unprompted, a
        // screen reader would be unusable. Only what asks for it is spoken without focus.
        let tree = tree_of(|ctx| {
            Text::new("Just text").build(ctx);
        });
        assert_eq!(tree[0].live_region, LiveRegionMode::Off);
    }

    #[test]
    fn the_loudest_live_region_declaration_wins() {
        // A component declares `Polite` (a snackbar is information); a caller may raise it to
        // `Assertive`. The other direction does not lower it — the same rule the claim follows.
        let raised = SemanticsConfig::new()
            .live_region(LiveRegionMode::Assertive)
            .or(SemanticsConfig::new().live_region(LiveRegionMode::Polite));
        assert_eq!(raised.live_region_mode(), LiveRegionMode::Assertive);

        let not_lowered = SemanticsConfig::new()
            .live_region(LiveRegionMode::Off)
            .or(SemanticsConfig::new().live_region(LiveRegionMode::Assertive));
        assert_eq!(not_lowered.live_region_mode(), LiveRegionMode::Assertive);
    }

    #[test]
    fn the_json_carries_the_live_region_mode() {
        let tree = tree_of(|ctx| {
            Row::new()
                .modifier(Modifier::new().semantics(
                    SemanticsConfig::new()
                        .merge_descendants(true)
                        .live_region(LiveRegionMode::Polite),
                ))
                .build(ctx, |ctx| {
                    Text::new("Saved").build(ctx);
                });
        });
        let json = semantics_json(&tree);
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(parsed[0]["liveRegion"], "polite");
        assert_eq!(parsed[0]["name"], "Saved");

        // And an ordinary element reports null, not a mode.
        let plain = tree_of(|ctx| {
            Text::new("plain").build(ctx);
        });
        let parsed: serde_json::Value =
            serde_json::from_str(&semantics_json(&plain)).expect("valid JSON");
        assert_eq!(parsed[0]["liveRegion"], serde_json::Value::Null);
    }

    #[test]
    fn the_json_is_parseable_and_names_the_state() {
        let tree = tree_of(|ctx| {
            Column::new().build(ctx, |ctx| {
                TriStateCheckbox::new(ToggleableState::On).on_click(|| {}).build(ctx);
                Button::filled().on_click(|| {}).build(ctx, |ctx| {
                    Text::new("Go").build(ctx);
                });
            });
        });
        let json = semantics_json(&tree);
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        let items = parsed.as_array().expect("an array");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["role"], "checkbox");
        assert_eq!(items[0]["state"]["checked"], "on");
        assert_eq!(items[1]["role"], "button");
        assert_eq!(items[1]["name"], "Go");
        assert_eq!(items[1]["clickable"], true);
    }

    #[test]
    fn json_escapes_a_name_that_would_break_the_document() {
        let tree = tree_of(|ctx| {
            Text::new("say \"hi\"\nnow\ttab").build(ctx);
        });
        let json = semantics_json(&tree);
        serde_json::from_str::<serde_json::Value>(&json)
            .expect("a quote or a newline in a label must not produce invalid JSON");
    }

    #[test]
    fn role_names_round_trip() {
        for role in [
            SemanticsRole::Button,
            SemanticsRole::Checkbox,
            SemanticsRole::Switch,
            SemanticsRole::RadioButton,
            SemanticsRole::Tab,
            SemanticsRole::Image,
            SemanticsRole::ProgressBar,
            SemanticsRole::Dialog,
        ] {
            assert_eq!(SemanticsRole::from_name(role.name()), Some(role));
        }
        assert_eq!(SemanticsRole::from_name("nope"), None);
    }

    #[test]
    fn an_unspecified_state_is_not_the_same_as_a_false_one() {
        assert!(SemanticsState::new().is_unspecified());
        // Merging fills gaps only: a declared `false` must not be overwritten by a fallback `true`.
        let merged = SemanticsState::new()
            .selected(false)
            .or(SemanticsState::new().selected(true).expanded(true));
        assert_eq!(merged.selected_value(), Some(false));
        assert_eq!(merged.expanded_value(), Some(true));
    }

    // ── clearAndSetSemantics and custom actions ──

    #[test]
    fn clear_and_set_semantics_replaces_the_declaration_and_hides_the_subtree() {
        let mut nodes = vec![
            LayoutNode::new(
                crate::modifier::Modifier::new().clear_and_set_semantics(
                    SemanticsConfig::new()
                        .role(SemanticsRole::Button)
                        .content_description("Save"),
                ),
                Some(0),
            ),
            LayoutNode::leaf(crate::modifier::Modifier::new().semantics(
                SemanticsConfig::new().content_description("Child"),
            )),
        ];
        nodes[0].children = vec![1];
        nodes[0].measured_size = crate::unit::Size::new(50.0, 20.0);
        let tree = semantics_tree(&nodes, 0);
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].name.as_deref(), Some("Save"));
        assert_eq!(tree[0].role, Some(SemanticsRole::Button));
        assert!(tree[0].children.is_empty(), "the subtree is hidden");
        assert!(tree[0].actions.is_empty());
    }

    #[test]
    fn an_empty_clear_and_set_semantics_hides_the_node() {
        let mut nodes = vec![
            LayoutNode::new(
                crate::modifier::Modifier::new()
                    .clear_and_set_semantics(SemanticsConfig::new()),
                Some(0),
            ),
            LayoutNode::leaf(crate::modifier::Modifier::new().semantics(
                SemanticsConfig::new().content_description("Child"),
            )),
        ];
        nodes[0].children = vec![1];
        assert!(
            semantics_tree(&nodes, 0).is_empty(),
            "a clear block that declares nothing makes the node invisible"
        );
    }

    #[test]
    fn custom_actions_are_listed_and_invokable() {
        let hits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let h = hits.clone();
        let mut nodes = vec![LayoutNode::leaf(crate::modifier::Modifier::new().semantics(
            SemanticsConfig::new()
                .content_description("Item")
                .custom_action("Delete", move || {
                    h.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }),
        ))];
        nodes[0].measured_size = crate::unit::Size::new(10.0, 10.0);
        let tree = semantics_tree(&nodes, 0);
        let labels: Vec<&str> = tree[0].actions.iter().map(|a| a.label()).collect();
        assert_eq!(labels, vec!["Delete"]);
        tree[0].actions[0].invoke();
        assert_eq!(hits.load(std::sync::atomic::Ordering::Relaxed), 1);
    }
