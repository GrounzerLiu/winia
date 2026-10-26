use super::*;

// ═══════════════════════════════════════════════════════════
// Building the tree
// ═══════════════════════════════════════════════════════════

/// Resolve the semantics of one composer's arena into elements, top level first.
///
/// `root` is the arena index of the layout root, as `Composer::layout_root_idx` returns it.
pub fn semantics_tree(nodes: &[LayoutNode], root: usize) -> Vec<SemanticsNode> {
    let mut out = Vec::new();
    collect(nodes, root, 0.0, 0.0, &mut out);
    out
}

pub(crate) fn collect(nodes: &[LayoutNode], idx: usize, parent_x: f32, parent_y: f32, out: &mut Vec<SemanticsNode>) {
    let node = &nodes[idx];
    // Compose's `Modifier.visible(false)`: "invisible to accessibility services" (`Visible.kt:36-38`),
    // so the node and everything below it stays out of the tree a screen reader descends.
    if !node.modifier.is_visible() {
        return;
    }
    let config = node.modifier.semantics_config();
    let (x, y) = (parent_x + node.position.x, parent_y + node.position.y);
    let bounds = (x, y, node.measured_size.width, node.measured_size.height);

    let clickable = node.modifier.on_click().is_some() || node.modifier.node_click().is_some();

    // `clearAndSetSemantics`: the block REPLACES this node's declaration and cuts the whole subtree
    // out of the tree. A block that declares nothing leaves the node out too (Compose's
    // "invisible to the user"), which is why the name comes only from the block — never from the
    // text the clear was meant to suppress.
    if let Some(clear) = node.modifier.clear_and_set_semantics_config() {
        let declared = clear.role_value().is_some()
            || clear.content_description_value().is_some()
            || !clear.state_value().is_unspecified()
            || clear.merges_descendants()
            || clear.is_selectable_group()
            || clear.live_region_mode() != LiveRegionMode::Off
            || !clear.custom_actions().is_empty()
            || clear.collection_info_value().is_some()
            || clear.collection_item_info_value().is_some()
            || clear.scroll_to_index_action_value().is_some()
            || clear.index_for_key_action_value().is_some();
        if declared {
            out.push(SemanticsNode {
                node_id: node.id,
                role: clear.role_value(),
                name: clear.content_description_value().map(str::to_string),
                state: clear.state_value(),
                actions: clear.custom_actions().to_vec(),
                clickable,
                focused: node.focused,
                live_region: clear.live_region_mode(),
                selectable_group: clear.is_selectable_group(),
                collection: clear.collection_info_value(),
                collection_item: clear.collection_item_info_value(),
                scroll_to_index: clear.scroll_to_index_action_value().cloned(),
                index_for_key: clear.index_for_key_action_value().cloned(),
                bounds,
                children: Vec::new(),
            });
        }
        return;
    }

    let declared_name = config
        .content_description_value()
        .map(str::to_string)
        .or_else(|| own_name(node));

    // Where this node's children start: below any scroll offset, the same way the focus walk and hit
    // testing do it, so bounds stay in one coordinate space.
    let (scroll_x, scroll_y) = scroll_offset_for_node(node);
    let (child_x, child_y) = (x - scroll_x, y - scroll_y);
    let children_of = node.children.clone();

    if clickable || config.merges_descendants() {
        // Claimed subtree: fold every descendant into this one element and drop them from the tree.
        let mut absorbed = Vec::new();
        for child in children_of {
            collect(nodes, child, child_x, child_y, &mut absorbed);
        }
        // Every absorbed descendant's state is folded in, not just the first one that says
        // something. Taking only the first loses whatever the rest declared: a dialog holding a
        // toggle (`enabled`) and an entry field in error would report the toggle's state and drop
        // the error, and the reader would never hear what is wrong with the value.
        let absorbed_state = absorbed
            .iter()
            .map(|child| child.state.clone())
            .reduce(|merged, next| next.or(merged))
            .unwrap_or_default();
        let absorbed_role = absorbed.iter().find_map(|child| child.role);
        // Actions accumulate across the claimed subtree, deduped by label (same rule as `or`).
        let mut actions = config.custom_actions().to_vec();
        for child in &absorbed {
            for action in &child.actions {
                if !actions.iter().any(|a| a.label() == action.label()) {
                    actions.push(action.clone());
                }
            }
        }
        out.push(SemanticsNode {
            node_id: node.id,
            role: config.role_value().or(absorbed_role),
            name: declared_name.or_else(|| joined_name(&absorbed)),
            state: config.state_value().or(absorbed_state),
            actions,
            clickable: clickable || absorbed.iter().any(|child| child.clickable),
            focused: node.focused,
            // An absorbed live region stays live: a snackbar wrapping its message is one element, and
            // the mode came from the wrapper.
            live_region: config
                .live_region_mode()
                .max(absorbed.iter().map(|child| child.live_region).max().unwrap_or_default()),
            selectable_group: config.is_selectable_group(),
            collection: config.collection_info_value(),
            collection_item: config.collection_item_info_value(),
            scroll_to_index: config.scroll_to_index_action_value().cloned(),
            index_for_key: config.index_for_key_action_value().cloned(),
            bounds,
            children: Vec::new(),
        });
        return;
    }

    let mut children = Vec::new();
    for child in children_of {
        collect(nodes, child, child_x, child_y, &mut children);
    }

    let role = config.role_value();
    if role.is_some()
        || declared_name.is_some()
        || !config.state_value().is_unspecified()
        // A group flag is a declaration too: without this the group node would be transparent and the tree
        // would lose the one thing that says its children are a collection.
        || config.is_selectable_group()
        // Same for a collection: a lazy list declares the item count and (as an item of its own list)
        // the position, and both have to survive into the tree or the reader sees an anonymous stack.
        || config.collection_info_value().is_some()
        || config.collection_item_info_value().is_some()
        // A scroll action is a declaration too: a list that only offers "bring item N into view"
        // must not be transparent.
        || config.scroll_to_index_action_value().is_some()
        || config.index_for_key_action_value().is_some()
    {
        // A declared node keeps its place and its children (nothing was claimed).
        out.push(SemanticsNode {
            node_id: node.id,
            role,
            name: declared_name,
            state: config.state_value(),
            actions: config.custom_actions().to_vec(),
            clickable,
            focused: node.focused,
            live_region: config.live_region_mode(),
            selectable_group: config.is_selectable_group(),
            collection: config.collection_info_value(),
            collection_item: config.collection_item_info_value(),
            scroll_to_index: config.scroll_to_index_action_value().cloned(),
            index_for_key: config.index_for_key_action_value().cloned(),
            bounds,
            children,
        });
    } else {
        // Declares nothing: transparent. Its descendants stand in its place, so an ordinary `Column`
        // never appears between a panel and its buttons.
        out.extend(children);
    }
}

/// This node's own name source, without looking at descendants: its text, or an image/icon
/// description.
pub(crate) fn own_name(node: &LayoutNode) -> Option<String> {
    for element in node.modifier.elements() {
        match element {
            ModifierElement::TextContent { content, .. } => return Some(content.clone()),
            ModifierElement::RichTextContent { content, .. } => return Some(content.clone()),
            ModifierElement::Semantics(config) => {
                if let Some(description) = config.content_description_value() {
                    return Some(description.to_string());
                }
            }
            _ => {}
        }
    }
    None
}

/// Depth-first walk handing each element its index path and the origin its coordinates are relative
/// to. `crate::accessibility` has its own copy for the provider tree; this one is here because
/// [`WindowSemantics::focused_path`] needs it on every platform.
pub(crate) fn walk_with_paths(
    node: &SemanticsNode,
    path: Vec<usize>,
    origin: (f32, f32),
    f: &mut impl FnMut(&SemanticsNode, (f32, f32), Vec<usize>),
) {
    f(node, origin, path.clone());
    for (index, child) in node.children.iter().enumerate() {
        let mut child_path = path.clone();
        child_path.push(index);
        walk_with_paths(child, child_path, origin, f);
    }
}

/// The names an absorbed subtree contributes, in tree order.
pub(crate) fn joined_name(absorbed: &[SemanticsNode]) -> Option<String> {
    let names: Vec<String> = absorbed.iter().filter_map(|node| node.effective_name()).collect();
    if names.is_empty() {
        None
    } else {
        Some(names.join(" "))
    }
}

/// Fire the click action of the element with this `LayoutNode` id — what a platform bridge's Invoke
/// pattern needs, and the reason [`SemanticsNode::node_id`] is part of the tree.
///
/// Returns whether a callback ran; an element without an action (a label) reports `false`.
pub fn click_node_by_id(nodes: &[LayoutNode], node_id: u64) -> bool {
    let Some(node) = nodes.iter().find(|node| node.id == node_id) else {
        return false;
    };
    if let Some(callback) = node.modifier.on_click() {
        callback();
        return true;
    }
    if let Some(node_callback) = node.modifier.node_click() {
        node_callback.on_click();
        return true;
    }
    false
}

