    use super::*;
    use crate::input::PointerIcon;
    use crate::layout::node::LayoutNode;
    use crate::modifier::Modifier;

    fn tree(leaf: Modifier, parent: Modifier) -> Vec<LayoutNode> {
        let mut parent_node = LayoutNode::new(parent, Some(0));
        parent_node.children = vec![1];
        vec![parent_node, LayoutNode::leaf(leaf)]
    }

    #[test]
    fn the_deepest_icon_wins() {
        let nodes = tree(
            Modifier::new().pointer_hover_icon(PointerIcon::HAND),
            Modifier::new().pointer_hover_icon(PointerIcon::TEXT),
        );
        assert_eq!(pointer_icon_for_path(&nodes, &[0, 1]), PointerIcon::HAND);
    }

    #[test]
    fn an_override_ancestor_beats_a_descendant() {
        let nodes = tree(
            Modifier::new().pointer_hover_icon(PointerIcon::HAND),
            Modifier::new().pointer_hover_icon_override_descendants(PointerIcon::TEXT),
        );
        assert_eq!(pointer_icon_for_path(&nodes, &[0, 1]), PointerIcon::TEXT);
    }

    #[test]
    fn the_first_override_from_the_leaf_wins() {
        // Three levels: leaf HAND, middle override TEXT, root WAIT. Walking up from the leaf, the
        // middle override is met first and stops the search.
        let mut nodes = vec![
            LayoutNode::new(
                Modifier::new()
                    .pointer_hover_icon(PointerIcon::WAIT)
                    .pointer_hover_icon_override_descendants(PointerIcon::WAIT),
                Some(0),
            ),
            LayoutNode::new(
                Modifier::new().pointer_hover_icon_override_descendants(PointerIcon::TEXT),
                Some(1),
            ),
            LayoutNode::leaf(Modifier::new().pointer_hover_icon(PointerIcon::HAND)),
        ];
        nodes[0].children = vec![1];
        nodes[1].children = vec![2];
        assert_eq!(pointer_icon_for_path(&nodes, &[0, 1, 2]), PointerIcon::TEXT);
    }

    #[test]
    fn no_icon_means_default() {
        let nodes = tree(Modifier::new(), Modifier::new());
        assert_eq!(pointer_icon_for_path(&nodes, &[0, 1]), PointerIcon::DEFAULT);
    }

    #[test]
    fn the_icon_is_part_of_the_element_parameter() {
        assert!(Modifier::new()
            .pointer_hover_icon(PointerIcon::HAND)
            .param_eq(&Modifier::new().pointer_hover_icon(PointerIcon::HAND)));
        assert!(!Modifier::new()
            .pointer_hover_icon(PointerIcon::HAND)
            .param_eq(&Modifier::new().pointer_hover_icon(PointerIcon::TEXT)));
        assert!(!Modifier::new()
            .pointer_hover_icon(PointerIcon::HAND)
            .param_eq(&Modifier::new().pointer_hover_icon_override_descendants(PointerIcon::HAND)));
    }
