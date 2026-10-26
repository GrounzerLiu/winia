use super::*;

// ═══════════════════════════════════════════════════════════
// Debug JSON
// ═══════════════════════════════════════════════════════════

/// The tree as JSON — what the debug server's `sem` command returns and UI tests assert on.
///
/// Hand-written rather than serde: `serde_json` is a dev-dependency, and this is a handful of fields.
pub fn semantics_json(tree: &[SemanticsNode]) -> String {
    let mut out = String::from("[");
    for (i, node) in tree.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        node_json(node, &mut out);
    }
    out.push(']');
    out
}

pub(crate) fn node_json(node: &SemanticsNode, out: &mut String) {
    let (x, y, w, h) = node.bounds;
    out.push_str(&format!(
        r#"{{"id":{},"role":{},"name":{},"state":"#,
        node.node_id,
        match node.role {
            Some(role) => format!("\"{}\"", role.name()),
            None => "null".to_string(),
        },
        match node.name.as_deref() {
            Some(name) => json_string(name),
            None => "null".to_string(),
        },
    ));
    // State: only the fields that say something, so a consumer can tell "unchecked" from "silent".
    let mut state = Vec::new();
    if let Some(selected) = node.state.selected_value() {
        state.push(format!("\"selected\":{selected}"));
    }
    if let Some(checked) = node.state.checked_value() {
        let name = match checked {
            ToggleableState::Off => "off",
            ToggleableState::On => "on",
            ToggleableState::Indeterminate => "indeterminate",
        };
        state.push(format!("\"checked\":\"{name}\""));
    }
    if let Some(expanded) = node.state.expanded_value() {
        state.push(format!("\"expanded\":{expanded}"));
    }
    if let Some(enabled) = node.state.enabled_value() {
        state.push(format!("\"enabled\":{enabled}"));
    }
    if let Some((current, min, max)) = node.state.progress_value() {
        state.push(format!("\"progress\":{{\"value\":{current},\"min\":{min},\"max\":{max}}}"));
    }
    if let Some(message) = node.state.error_value() {
        // `json_string` already quotes and escapes; quoting it again would make the whole
        // snapshot unparseable, which reads from the debug channel as no answer at all.
        state.push(format!("\"error\":{}", json_string(message)));
    }
    let actions = node
        .actions
        .iter()
        .map(|action| json_string(action.label()))
        .collect::<Vec<_>>()
        .join(",");
    // A collection and an item position are separate keys: a list declares the count, its items
    // declare where they sit, and a consumer may read either without the other.
    let collection = match node.collection {
        Some(info) => format!("{{\"rows\":{},\"columns\":{}}}", info.rows, info.columns),
        None => "null".to_string(),
    };
    let collection_item = match node.collection_item {
        Some(info) => format!("{{\"index\":{},\"span\":{}}}", info.index, info.span),
        None => "null".to_string(),
    };
    // A capability flag, not an id: the closure is runtime state, and what a client needs to know is
    // whether asking this element to scroll an item into view would do anything.
    let scroll_to_index = node.scroll_to_index.is_some();
    out.push_str(&format!(
        "{{{}}},\"actions\":[{actions}],\"clickable\":{},\"focused\":{},\"liveRegion\":{},\"selectableGroup\":{},\"collection\":{collection},\"collectionItem\":{collection_item},\"scrollToIndex\":{scroll_to_index},\"bounds\":[{x:.0},{y:.0},{w:.0},{h:.0}],\"children\":[",
        state.join(","),
        node.clickable,
        node.focused,
        match node.live_region {
            LiveRegionMode::Off => "null",
            LiveRegionMode::Polite => "\"polite\"",
            LiveRegionMode::Assertive => "\"assertive\"",
        },
        node.selectable_group,
    ));
    for (i, child) in node.children.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        node_json(child, out);
    }
    out.push_str("]}");
}

pub(crate) fn json_string(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
        .replace('\u{8}', "\\b")
        .replace('\u{c}', "\\f");
    format!("\"{escaped}\"")
}


