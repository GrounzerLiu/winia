#!/usr/bin/env python3
import re

with open('D:/Projects/winia/winia/src/app.rs', 'r', encoding='utf-8') as f:
    text = f.read()

old = '''                    match evt {
                        debug::DebugEvent::Click { x, y } => {
                            if let Some(root) = pw.composer.layout_root() {
                                let nodes = hit_test(root, x, y);
                                let mut click_handled = false;
                                std::mem::drop(root);

                                // 设置焦点 + 光标（DebugEvent 也要有焦点才能输入）
                                if let Some(innermost) = nodes.last() {
                                    if crate::layout::node::has_focusable_modifier(innermost) {
                                        // 焦点（需要可变 root）
                                        if let Some(root_mut) = pw.composer.layout_root_mut() {
                                            let fid = innermost.id;
                                            crate::layout::node::clear_focus(root_mut);
                                            crate::layout::node::set_focus_by_id(root_mut, fid);
                                            pw.focused_id = Some(fid);
                                            pw.focused_slot_key = Some(innermost.slot_key);
                                            if let Some(ref sw) = pw.skia_window { sw.set_ime_allowed(true); }
                                        }
                                        // 光标位置
                                        if let Ok(borrow) = innermost.cached_paragraph.try_borrow() {
                                            if let Some(para) = borrow.as_ref() {
                                                if let Some(root) = pw.composer.layout_root() {
                                                    let (ax, ay) = node_abs_position(root, innermost.id);
                                                    let tl = crate::text::TextLayout::new(para, 0);
                                                    let idx = tl.get_closest_grapheme_cluster_cluster_at(skia_safe::Point::new(x - ax, y - ay));
                                                    innermost.cursor_index.set(idx);
                                                    if let Some(cb) = innermost.cursor_callback.borrow_mut().as_mut() { cb(idx); }
                                                    eprintln!("[cursor] debug-click idx={} xy=({:.0},{:.0}) abs=({:.0},{:.0})", idx, x, y, ax, ay);
                                                }
                                            }
                                        }
                                    }
                                }

                                eprintln!("[debug-click] pos=({:.0},{:.0}) path_len={} sf={}", x, y, nodes.len(), pw.scale_factor);'''

new = '''                    match evt {
                        debug::DebugEvent::Click { x, y } => {
                            let mut click_handled = false;
                            // 先查询焦点信息（不可变root）
                            let (focused_fid, focused_sk) = {
                                let opt = pw.composer.layout_root().and_then(|root| {
                                    let nodes = hit_test(root, x, y);
                                    nodes.last().filter(|n| crate::layout::node::has_focusable_modifier(n))
                                        .map(|n| (n.id, n.slot_key))
                                });
                                opt.unwrap_or((0, 0))
                            };
                            if focused_fid != 0 {
                                // 设置焦点（可变root）
                                if let Some(root_mut) = pw.composer.layout_root_mut() {
                                    crate::layout::node::clear_focus(root_mut);
                                    crate::layout::node::set_focus_by_id(root_mut, focused_fid);
                                }
                                pw.focused_id = Some(focused_fid);
                                pw.focused_slot_key = Some(focused_sk);
                                if let Some(ref sw) = pw.skia_window { sw.set_ime_allowed(true); }
                                // 光标位置
                                if let Some(root) = pw.composer.layout_root() {
                                    if let Some(innermost) = crate::layout::node::find_node_by_id(root, focused_fid) {
                                        if let Ok(borrow) = innermost.cached_paragraph.try_borrow() {
                                            if let Some(para) = borrow.as_ref() {
                                                let (ax, ay) = node_abs_position(root, focused_fid);
                                                let tl = crate::text::TextLayout::new(para, 0);
                                                let idx = tl.get_closest_grapheme_cluster_cluster_at(skia_safe::Point::new(x - ax, y - ay));
                                                innermost.cursor_index.set(idx);
                                                if let Some(cb) = innermost.cursor_callback.borrow_mut().as_mut() { cb(idx); }
                                                eprintln!("[cursor] debug-click idx={} xy=({:.0},{:.0}) abs=({:.0},{:.0})", idx, x, y, ax, ay);
                                            }
                                        }
                                    }
                                }
                            }

                            if let Some(root) = pw.composer.layout_root() {
                                let nodes = hit_test(root, x, y);
                                eprintln!("[debug-click] pos=({:.0},{:.0}) path_len={} sf={}", x, y, nodes.len(), pw.scale_factor);'''

if old in text:
    text = text.replace(old, new)
    with open('D:/Projects/winia/winia/src/app.rs', 'w', encoding='utf-8') as f:
        f.write(text)
    print('replaced successfully')
else:
    print('old NOT FOUND in file')
    idx = text.find('std::mem::drop(root)')
    print(f'Found drop at {idx}')
    print(text[idx-50:idx+100])
