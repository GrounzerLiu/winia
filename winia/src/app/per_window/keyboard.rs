use super::*;

pub(crate) fn dispatch_key_to_focus(pw: &PerWindow, ke: &crate::input::KbEvent) -> bool {
    let Some(fid) = pw.focused_id else { return false };
    let Some(r) = pw.composer.layout_root_idx() else { return false };
    dispatch_key_in_arena(pw.composer.arena_nodes(), r, fid, ke)
}

/// Dispatch a key event to the topmost focused overlay first (overlays own
/// independent composers — the main-tree focus path can never reach them).
/// Falls through (returns false) when no overlay has focus, so callers can
/// continue with the main tree.
pub(crate) fn dispatch_key_to_overlay(pw: &PerWindow, ke: &crate::input::KbEvent) -> bool {
    // Only the arena that OWNS the keyboard gets the key. Walking "the first overlay that has focus"
    // would hand keys to a layer below the modal that is actually on top (an overlay keeps a
    // remembered focus now, see `claim_keyboard_for_overlay`), and falling through to lower surfaces
    // would double-handle a key the owner already acted on.
    let Some(i) = keyboard_scope(pw) else { return false };
    let ov = &pw.overlay.layers[i];
    let Some(fid) = ov.focused_id else { return false };
    let Some(r) = ov.composer.layout_root_idx() else { return false };
    // The owner did not consume — stop here.
    dispatch_key_in_arena(ov.composer.arena_nodes(), r, fid, ke)
}

/// Core key dispatch against one arena: focus path Preview (root→focused) +
/// Bubble (focused→root) + focused-node activation (Enter/Space → onClick).
/// Shared by the main tree and overlay composers (identical semantics).
pub(crate) fn dispatch_key_in_arena(
    nodes: &[crate::layout::node::LayoutNode],
    r: usize,
    fid: u64,
    ke: &crate::input::KbEvent,
) -> bool {
    // Collect the focus path: root → ... → focused.
    let mut path: Vec<usize> = Vec::new();
    if let Some(idx) = crate::layout::node::find_node_by_id(nodes, r, fid) {
        path.push(idx);
        let mut pid = nodes[idx].parent_id;
        while let Some(id) = pid {
            if let Some(anc) = crate::layout::node::find_node_by_id(nodes, r, id) {
                path.push(anc);
                pid = nodes[anc].parent_id;
            } else { break; }
        }
        path.reverse(); // path[0] == root, path[last] == focused
    }
    // Preview: root → focused (matching `onPreviewKeyEvent`).
    // Dual track (exp/modifier-node): the enum `on_pre_key` handlers run first, then the node's own
    // key node on the same node.
    for &ni in &path {
        for el in nodes[ni].modifier.elements() {
            if let crate::modifier::ModifierElement::KbEvent { on_pre_key: Some(handler), .. } = el {
                if handler(ke) { return true; }
            }
        }
        for k in nodes[ni].modifier.key_nodes() {
            if k.on_pre(ke) { return true; }
        }
    }
    // Bubble: focused → root (matching `onKeyEvent`).
    for &ni in path.iter().rev() {
        for el in nodes[ni].modifier.elements().iter().rev() {
            if let crate::modifier::ModifierElement::KbEvent { on_key: Some(handler), .. } = el {
                if handler(ke) { return true; }
            }
        }
        for k in nodes[ni].modifier.key_nodes() {
            if k.on_event(ke) { return true; }
        }
    }
    // Keyboard activation of the focused component (Compose `clickable`: Enter/Space on the
    // focused node triggers onClick — only the focused node's own clickable responds, and it does
    // not bubble to ancestors).
    let is_activate = matches!(&ke.key, winit::keyboard::Key::Named(winit::keyboard::NamedKey::Enter))
        || matches!(&ke.key, winit::keyboard::Key::Character(c) if c == " ");
    if ke.event_type == crate::input::KbEventType::KeyDown && !ke.repeat && is_activate {
        if let Some(idx) = crate::layout::node::find_node_by_id(nodes, r, fid) {
            if let Some(on_click) = nodes[idx].modifier.on_click() {
                on_click();
                return true;
            }
            // Dual track: a node click handler responds to keyboard activation too.
            if let Some(node_cb) = nodes[idx].modifier.node_click() {
                node_cb.on_click();
                return true;
            }
        }
    }
    false
}

/// Parse a key string from the debug `k` command into a winit Key (a single character →
/// `Character`, a special name → `NamedKey`). Returns None when it cannot be parsed (ignored).
pub(crate) fn parse_debug_key(s: &str) -> Option<winit::keyboard::Key> {
    use winit::keyboard::{Key, NamedKey};
    match s {
        "Backspace" => Some(Key::Named(NamedKey::Backspace)),
        "Delete" => Some(Key::Named(NamedKey::Delete)),
        "Enter" => Some(Key::Named(NamedKey::Enter)),
        "Escape" => Some(Key::Named(NamedKey::Escape)),
        "ArrowLeft" => Some(Key::Named(NamedKey::ArrowLeft)),
        "ArrowRight" => Some(Key::Named(NamedKey::ArrowRight)),
        "ArrowUp" => Some(Key::Named(NamedKey::ArrowUp)),
        "ArrowDown" => Some(Key::Named(NamedKey::ArrowDown)),
        // The spacebar. winit 0.31 (the version in Cargo.lock) has no `NamedKey::Space`, so a spacebar
        // arrives as `Key::Character(" ")` — the same spelling winia's own activation path checks
        // (`app.rs:4552`), and the one components reading it as activation accept (`overlay.rs`, an
        // editable menu anchor). This name exists so a test can press it at all.
        "Space" => Some(Key::Character(" ".into())),
        "Home" => Some(Key::Named(NamedKey::Home)),
        "End" => Some(Key::Named(NamedKey::End)),
        _ if s.chars().count() == 1 => Some(Key::Character(s.to_string().into())),
        _ => None,
    }
}

/// Find the text node used for grapheme anchor placement (a node with a `cached_paragraph`).
///
/// Two steps:
/// 1. **Upward**: walk the hit path from the innermost node and take the first one with a
///    paragraph — when the hit is the text itself (an input leaf / label / output Text) this
///    matches directly.
/// 2. **Downward** (upward failed and the innermost node is a TextField container): clicking the
///    container's blank area (right of the text / padding / suffix) leaves the input leaf off the
///    hit path, so DFS the container subtree for the first descendant with both a paragraph and a
///    registrar (the TextField input leaf; a label/placeholder has a para but no reg, so it is
///    excluded).
///
/// ⚠ The downward branch must be **restricted to TextField containers** (the modifier holds a
/// `TextFieldVisual`; an input leaf has no such marker): never search downward from blank space
/// that is not a TextField (a page gap / Column gap / SelectionContainer gap) — otherwise the DFS
/// would wrongly match any "para+reg" text in the subtree (e.g. the input leaf of a distant
/// TextField), and a drag started on blank space would select the wrong text or bind the wrong
/// anchor.
///
/// None means there is no locatable text at the click point (a click on empty space → no cursor,
/// no drag-selection).
pub(crate) fn find_anchor_text_node(nodes: &[LayoutNode], path: &[usize], innermost: usize) -> Option<usize> {
    if let Some(&i) = path.iter().rev().find(|&&i| nodes[i].cached_paragraph.borrow().is_some()) {
        return Some(i);
    }
    let is_tf_container = nodes[innermost].modifier.elements().iter()
        .any(|el| matches!(el, crate::modifier::ModifierElement::TextFieldVisual { .. }));
    if !is_tf_container {
        return None;
    }
    // Search downward: the input leaf inside the TextField container's subtree (both para and registrar).
    pub(crate) fn dfs(nodes: &[LayoutNode], idx: usize) -> Option<usize> {
        if nodes[idx].cached_paragraph.borrow().is_some() && nodes[idx].registrar.borrow().is_some() {
            return Some(idx);
        }
        for &c in &nodes[idx].children {
            if let Some(f) = dfs(nodes, c) { return Some(f); }
        }
        None
    }
    dfs(nodes, innermost)
}

/// Does the node or its subtree declare an IME need (a non-empty `ime_callback`)?
///
/// ⚠ TextField containment: focus and keyboard live on the container (focusable is on the
/// container's modifier), while `ime_callback` is on the **input leaf**
/// (`set_current_node_ime_callback`). Checking only the focused node itself would mean the IME
/// never opens when a TextField is focused. So DFS the subtree (the same strategy as
/// `find_anchor_text_node`); a focusable subtree without an `ime_callback` (a Button and the like)
/// does not match → focusing it does not open the IME.
pub(crate) fn node_or_descendant_wants_ime(nodes: &[LayoutNode], idx: usize) -> bool {
    if nodes[idx].ime_callback.borrow().is_some() {
        return true;
    }
    for &c in &nodes[idx].children {
        if node_or_descendant_wants_ime(nodes, c) {
            return true;
        }
    }
    false
}

/// Find the first node in the subtree that declares an `ime_callback` (the TextField input leaf).
///
/// ⚠ Same root cause as `node_or_descendant_wants_ime`: focus is on the container while
/// `ime_callback` is on the input leaf, so a Preedit event has to be dispatched to the node that
/// actually holds the callback.
pub(crate) fn find_descendant_ime_callback(nodes: &[LayoutNode], idx: usize) -> Option<usize> {
    if nodes[idx].ime_callback.borrow().is_some() {
        return Some(idx);
    }
    for &c in &nodes[idx].children {
        if let Some(f) = find_descendant_ime_callback(nodes, c) {
            return Some(f);
        }
    }
    None
}

