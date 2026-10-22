    use crate::layout::node::LayoutNode;
    use crate::modifier::Modifier;

    /// A field container with an input leaf, plus an element beside the leaf (the icon). The paths are
    /// the arena paths a hit test produces, innermost last.
    fn field_with_icon(icon: Modifier) -> (Vec<LayoutNode>, u64, u64, u64) {
        let nodes = vec![
            LayoutNode::new(Modifier::new(), None), // 0 root
            LayoutNode::new(Modifier::new().on_press(|_| {}), None), // 1 the field container
            LayoutNode::new(Modifier::new().on_press(|_| {}), None), // 2 the input leaf
            LayoutNode::new(icon, None),            // 3 the icon inside the field
        ];
        let (container, leaf, icon) = (nodes[1].id, nodes[2].id, nodes[3].id);
        (nodes, container, leaf, icon)
    }

    fn target(nodes: &[LayoutNode], path: &[usize]) -> Option<u64> {
        super::press_gesture_target(nodes, path).map(|(id, _, _)| id)
    }

    #[test]
    fn the_innermost_press_declaration_takes_the_press() {
        let (nodes, container, leaf, _) = field_with_icon(Modifier::new());
        assert_eq!(
            target(&nodes, &[0, 1, 2]),
            Some(leaf),
            "the input leaf's on_press wins over the container's"
        );
        assert_eq!(
            target(&nodes, &[0, 1]),
            Some(container),
            "with no inner declaration the container takes the press"
        );
    }

    #[test]
    fn a_clickable_alone_does_not_take_the_press() {
        let (nodes, container, _, _) = field_with_icon(Modifier::new().clickable(|| {}));
        assert_eq!(
            target(&nodes, &[0, 1, 3]),
            Some(container),
            "a Clickable is not a gesture, so an icon carrying only `clickable` still lets the \
             field's own press (the caret placement) fire: a menu anchor element must register a \
             press as well"
        );
    }

    #[test]
    fn an_on_press_on_the_icon_takes_the_press_from_the_field() {
        let (nodes, _, _, icon) =
            field_with_icon(Modifier::new().clickable(|| {}).on_press(|_| {}));
        assert_eq!(
            target(&nodes, &[0, 1, 3]),
            Some(icon),
            "with a press registered the icon is the innermost target, so the field's caret press \
             does not fire — winia's equivalent of material3's `downEvent.consume()`"
        );
    }
