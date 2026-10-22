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
    // 收集焦点路径：root → ... → focused
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
    // Preview: root → focused（对齐 onPreviewKeyEvent）
    // 双轨（exp/modifier-node）：枚举 on_pre_key 先行，同节点 key node 紧随。
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
    // Bubble: focused → root（对齐 onKeyEvent）
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
    // 聚焦组件的键盘激活（对标 Compose clickable：聚焦时按 Enter/Space
    // 触发 onClick——仅聚焦节点自身的 clickable 响应，不向祖先冒泡）
    let is_activate = matches!(&ke.key, winit::keyboard::Key::Named(winit::keyboard::NamedKey::Enter))
        || matches!(&ke.key, winit::keyboard::Key::Character(c) if c == " ");
    if ke.event_type == crate::input::KbEventType::KeyDown && !ke.repeat && is_activate {
        if let Some(idx) = crate::layout::node::find_node_by_id(nodes, r, fid) {
            if let Some(on_click) = nodes[idx].modifier.on_click() {
                on_click();
                return true;
            }
            // 双轨：node 点击同样响应键盘激活
            if let Some(node_cb) = nodes[idx].modifier.node_click() {
                node_cb.on_click();
                return true;
            }
        }
    }
    false
}

/// 解析 debug `k` 命令的按键字符串为 winit Key（单字符 → Character；
/// 特殊名 → NamedKey）。无法解析返回 None（忽略）。
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

/// 查找 grapheme anchor 定位用的文本节点（有 cached_paragraph 的节点）。
///
/// 策略分两步：
/// 1. **向上**：沿命中路径从 innermost 起找第一个有 paragraph 的节点——
///    命中文本本身（输入 leaf / label / 输出 Text）时直接命中。
/// 2. **向下**（向上失败，且 innermost 是 TextField 容器）：点击容器空白
///    （文本右侧/padding/后缀区）时输入 leaf 不在命中路径——DFS 容器子树
///    找第一个「有 paragraph + 有 registrar」的后代（TextField 输入 leaf；
///    label/placeholder 有 para 无 reg，排除）。
///
/// ⚠ 向下分支必须**限定 TextField 容器**（modifier 含 TextFieldVisual；
/// 输入 leaf 无此标记）：非 TextField 空白（页面空隙/Column 空白/
/// SelectionContainer 空白）绝不向下找——否则 DFS 会误命中子树中任意
/// 「有 para+reg」的文本（如远处 TextField 的输入 leaf），从空白按下的
/// 拖动会错误选中/错绑 anchor。
///
/// 返回 None = 点击处无可定位文本（空区域点击 → 不设光标/不开始拖选）。
pub(crate) fn find_anchor_text_node(nodes: &[LayoutNode], path: &[usize], innermost: usize) -> Option<usize> {
    if let Some(&i) = path.iter().rev().find(|&&i| nodes[i].cached_paragraph.borrow().is_some()) {
        return Some(i);
    }
    let is_tf_container = nodes[innermost].modifier.elements().iter()
        .any(|el| matches!(el, crate::modifier::ModifierElement::TextFieldVisual { .. }));
    if !is_tf_container {
        return None;
    }
    // 向下找：TextField 容器子树中的输入 leaf（para + registrar 双条件）
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

/// 节点或其子树中是否有节点声明 IME 需求（ime_callback 非空）。
///
/// ⚠ TextField 容器化：焦点/键盘在容器（focusable 挂在容器 modifier），而
/// ime_callback 在**输入 leaf**（set_current_node_ime_callback）——只查焦点
/// 节点自身则 TextField 聚焦时 IME 永不开启。DFS 向下找子树（与
/// find_anchor_text_node 同策略）；Button 等无 ime_callback 的 focusable
/// 子树不匹配 → 聚焦不开启输入法。
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

/// 从节点子树中找第一个声明 ime_callback 的节点（TextField 输入 leaf）。
///
/// ⚠ 与 node_or_descendant_wants_ime 同根因：焦点在容器、ime_callback 在
/// 输入 leaf——Preedit 事件须派发到实际持有 ime_callback 的节点。
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

