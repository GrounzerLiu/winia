# Apply all keyboard changes to app.rs in one clean pass
with open('winia/src/app.rs', 'r') as f:
    content = f.read()

# All changes as simple text replacements

# 1. focus_prev import
content = content.replace(
    'use crate::layout::node::{hit_test, focus_next, LayoutNode};',
    'use crate::layout::node::{hit_test, focus_next, focus_prev, LayoutNode};'
)

# 2. focused_slot_key field
content = content.replace(
    '    theme: crate::ui::theme::ThemeColors,\n}',
    '    theme: crate::ui::theme::ThemeColors,\n    /// 焦点节点的 slot_key\n    pub(crate) focused_slot_key: Option<u64>,\n}'
)

# 3. Constructor
content = content.replace(
    'focused_id: None, content, on_close: None, created_id: None, theme }',
    'focused_id: None, content, on_close: None, created_id: None, theme, focused_slot_key: None }'
)

# 4. Focus restoration in recompose
content = content.replace(
    '            if let Some(fid) = self.focused_id {\n                if let Some(r) = self.composer.layout_root_mut() {\n                    crate::layout::node::focus_by_id(r, fid);\n                }\n            }',
    '            if let Some(slot_key) = self.focused_slot_key {\n                if let Some(r) = self.composer.layout_root_mut() {\n                    if let Some(new_id) = crate::layout::node::find_node_id_by_slot_key(r, slot_key) {\n                        crate::layout::node::set_focus_by_id(r, new_id);\n                        self.focused_id = Some(new_id);\n                    } else {\n                        self.focused_id = None;\n                    }\n                }\n            }'
)

# 5. modifiers field
content = content.replace(
    '    parent_window_id: Option<WindowId>,\n    /// 初始化回调',
    '    parent_window_id: Option<WindowId>,\n    /// 窗口全局修饰键状态\n    pub(crate) modifiers: winit::keyboard::ModifiersState,\n    /// 初始化回调'
)

# 6. modifiers init
content = content.replace(
    '        parent_window_id: None,\n    };\n    event_loop.run_app',
    '        parent_window_id: None,\n        modifiers: Default::default(),\n    };\n    event_loop.run_app'
)

# 7. Keyboard handler — replace the old simple Tab handler
old_kb = "                if let Some(ref proxy) = *APP_PROXY.lock().unwrap() { let _ = proxy.wake_up(); }\n            }\n            WindowEvent::KeyboardInput { event, .. } if event.state.is_pressed() => {\n                if matches!(&event.logical_key, Key::Named(NamedKey::Tab)) {\n                    if let Some(root) = pw.composer.layout_root_mut() {\n                        focus_next(root);\n                        pw.focused_id = crate::layout::node::get_focus_id(root);\n                    }\n                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }\n                    event_loop.set_control_flow(ControlFlow::Poll);\n                }\n            }"

content = content.replace(old_kb,
    "                if let Some(ref proxy) = *APP_PROXY.lock().unwrap() { let _ = proxy.wake_up(); }\n            }\n"
    "            WindowEvent::ModifiersChanged(m) => {\n"
    "                self.modifiers = m.state();\n"
    "            }\n"
    "            WindowEvent::KeyboardInput { event, .. } if event.state.is_pressed() => {\n"
    "                let event_type = if event.state.is_pressed() {\n"
    "                    crate::modifier::KbEventType::KeyDown\n"
    "                } else {\n"
    "                    crate::modifier::KbEventType::KeyUp\n"
    "                };\n"
    "                let ke = crate::modifier::KbEvent {\n"
    "                    key: event.logical_key.clone(),\n"
    "                    event_type,\n"
    "                    is_alt_pressed: self.modifiers.alt_key(),\n"
    "                    is_ctrl_pressed: self.modifiers.control_key(),\n"
    "                    is_shift_pressed: self.modifiers.shift_key(),\n"
    "                    is_meta_pressed: self.modifiers.meta_key(),\n"
    "                    repeat: event.repeat,\n"
    "                };\n"
    "                let mut consumed = false;\n"
    "                if matches!(&event.logical_key, Key::Named(NamedKey::Escape)) {\n"
    "                    if pw.focused_id.is_some() {\n"
    "                        pw.composer.layout_root_mut().map(|root| crate::layout::node::clear_focus(root));\n"
    "                        pw.focused_id = None;\n"
    "                        pw.focused_slot_key = None;\n"
    "                        consumed = true;\n"
    "                    }\n"
    "                }\n"
    "                if matches!(&event.logical_key, Key::Named(NamedKey::Tab)) {\n"
    "                    let shift = self.modifiers.shift_key();\n"
    "                    let (new_id, new_slot) = pw.composer.layout_root_mut().map(|root| {\n"
    "                        if shift { focus_prev(root); } else { focus_next(root); }\n"
    "                        let id = crate::layout::node::get_focus_id(root);\n"
    "                        let slot = id.and_then(|fid| crate::layout::node::find_node_by_id(root, fid).map(|n| n.slot_key));\n"
    "                        (id, slot)\n"
    "                    }).unwrap_or((None, None));\n"
    "                    pw.focused_id = new_id;\n"
    "                    pw.focused_slot_key = new_slot;\n"
    "                    consumed = true;\n"
    "                }\n"
    "                if !consumed {\n"
    "                    if let Some(fid) = pw.focused_id {\n"
    "                        if let Some(root) = pw.composer.layout_root() {\n"
    "                            // Collect ancestors from root to focused\n"
    "                            fn collect_path<'a>(node: &'a LayoutNode, target: u64, out: &mut Vec<&'a LayoutNode>) -> bool {\n"
    "                                if node.id == target { out.push(node); return true; }\n"
    "                                for child in &node.children {\n"
    "                                    if collect_path(child, target, out) {\n"
    "                                        out.push(node);\n"
    "                                        return true;\n"
    "                                    }\n"
    "                                }\n"
    "                                false\n"
    "                            }\n"
    "                            let mut path = Vec::new();\n"
    "                            collect_path(root, fid, &mut path);\n"
    "                            path.reverse();\n"
    "                            // Preview: root -> focused\n"
    "                            'preview: for &node in &path {\n"
    "                                for el in node.modifier.elements() {\n"
    "                                    if let crate::modifier::ModifierElement::KbEvent { on_pre_key: Some(handler), .. } = el {\n"
    "                                        if handler(&ke) { consumed = true; break 'preview; }\n"
    "                                    }\n"
    "                                }\n"
    "                            }\n"
    "                            if !consumed {\n"
    "                                // Bubble: focused -> root\n"
    "                                'bubble: for &node in path.iter().rev() {\n"
    "                                    for el in node.modifier.elements() {\n"
    "                                        if let crate::modifier::ModifierElement::KbEvent { on_key: Some(handler), .. } = el {\n"
    "                                            if handler(&ke) { consumed = true; break 'bubble; }\n"
    "                                        }\n"
    "                                    }\n"
    "                                }\n"
    "                            }\n"
    "                        }\n"
    "                    }\n"
    "                }\n"
    "                if consumed {\n"
    "                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }\n"
    "                    event_loop.set_control_flow(ControlFlow::Poll);\n"
    "                }\n"
    "            }"
)

with open('winia/src/app.rs', 'w') as f:
    f.write(content)
print('done')
