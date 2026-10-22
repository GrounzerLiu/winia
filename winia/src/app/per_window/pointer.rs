use super::*;

/// 沿命中路径从内到外触发第一个 on_click——返回是否触发。
/// 主树 click 与 overlay 点击共用（消除重复）
/// 双轨（exp/modifier-node）：枚举 Clickable 优先（旧行为），无枚举时回退
/// 开放 ClickNode（第三方自定义点击无需改核心）。
pub(crate) fn fire_click_along_path(nodes: &[crate::layout::node::LayoutNode], path: &[usize]) -> bool {
    for &i in path.iter().rev() {
        if let Some(cb) = nodes[i].modifier.on_click() {
            (cb)();
            return true;
        }
        if let Some(node_cb) = nodes[i].modifier.node_click() {
            node_cb.on_click();
            return true;
        }
    }
    false
}


/// 按下：命中路径最内层 clickable 绑定的交互源 → 发射 Press
/// （对标 Compose clickable 的 PressInteraction.Press）
pub(crate) fn press_interaction_down(pw: &mut PerWindow, path: &[usize], scene_pos: (f32, f32)) {
    let (slot, idx, src) = {
        let nodes = pw.composer.arena_nodes();
        let Some(&idx) = path.iter().rev().find(|&&i| {
            nodes[i].modifier.clickable_interaction().is_some()
                || nodes[i].modifier.node_click_interaction().is_some()
        }) else {
            return;
        };
        let src = match nodes[idx]
            .modifier
            .clickable_interaction()
            .cloned()
            .or_else(|| nodes[idx].modifier.node_click_interaction())
        {
            Some(s) => s,
            None => return,
        };
        (nodes[idx].slot_key, idx, src)
    };
    // 场景坐标 → 节点本地坐标（对标 Compose PressInteraction.Press.pressPosition）。
    // 波纹中心必须存本地坐标：绘制时加回布局原点，滚动/变换后波纹跟随节点；
    // 若存场景坐标，按下后滚动/动画会脱离按钮。
    let local = {
        let nodes = pw.composer.arena_nodes();
        let clickable_local =
            crate::layout::node::scene_to_node_local(nodes, path, idx, scene_pos.0, scene_pos.1);
        // Ripple 与 clickable 不同节点时（如 Switch 的 Handle 容器）：
        // 按压坐标要换算到 Ripple 节点本地空间，否则波纹锚点偏移。
        // 本地坐标差 = 两节点绝对位置差（无滚动/变换时等价于 scene 换算）。
        let ripple_adjust = pw.composer.layout_root_idx().and_then(|r| {
            // 优先在命中路径内定位（同一 source 复用时避免锚点错位）；Switch 的
            // ripple 挂在 Handle 子节点上，点击轨道空白处时不在 path 内——回退
            // 全树搜索。已知限制：同一 source 复用于多个组件时全树搜索可能取到
            // 错误节点（当前 Switch 均为每组件独立 source，不受影响）。
            let ridx = path
                .iter()
                .copied()
                .find(|&i| {
                    nodes[i]
                        .modifier
                        .ripple_interaction()
                        .map(|s| s == &src)
                        .unwrap_or(false)
                })
                .or_else(|| {
                    nodes.iter().position(|n| {
                        n.modifier
                            .ripple_interaction()
                            .map(|s| s == &src)
                            .unwrap_or(false)
                    })
                })?;
            if ridx == idx {
                return None;
            }
            let (ax, ay) = node_abs_position(nodes, r, nodes[idx].id);
            let (rx, ry) = node_abs_position(nodes, r, nodes[ridx].id);
            Some((ax - rx, ay - ry))
        });
        match ripple_adjust {
            Some((dx, dy)) => (clickable_local.0 + dx, clickable_local.1 + dy),
            None => clickable_local,
        }
    };
    src.emit_press_at(local);
    pw.pressed_interaction = Some((slot, src));
}

/// 释放/取消按下交互（Up 或越界 slop）——对标 PressInteraction.Release/Cancel
pub(crate) fn release_pressed_interaction(pw: &mut PerWindow) {
    if let Some((_, src)) = pw.pressed_interaction.take() {
        src.emit_release();
    }
    // overlay 的按下交互同步释放（overlay 独立 composer——独立 pressed 状态）
    for ov in &mut pw.overlay.layers {
        if let Some((_, src)) = ov.pressed_interaction.take() {
            src.emit_release();
        }
    }
}

/// 悬停更新：**路径上所有 hoverable** 节点进入/离开 → 发射 Hover Enter/Exit
/// （对标 Compose hoverable：每个 hoverable 独立收到 Enter/Exit；修复前只
/// 发射最内层——嵌套 hoverable（如 Tooltip 锚点容器 + 内部 Button）外层
/// 收不到 Enter → Tooltip 不显示）。节点移除时自动补 Exit。
/// The cursor a hit path asks for: the deepest `pointer_hover_icon`, unless an ancestor with
/// `override_descendants` wins — Compose Desktop's `PointerHoverIconModifierNode` rule.
pub(crate) fn pointer_icon_for_path(
    nodes: &[LayoutNode],
    path: &[usize],
) -> crate::input::PointerIcon {
    let mut nearest: Option<crate::input::PointerIcon> = None;
    // Leaf first: the innermost icon is the nearest one, and the first override wins over
    // everything below it.
    for &idx in path.iter().rev() {
        for el in nodes[idx].modifier.elements().iter().rev() {
            if let crate::modifier::ModifierElement::PointerHoverIcon { icon, override_descendants } = el {
                if *override_descendants {
                    return *icon;
                }
                if nearest.is_none() {
                    nearest = Some(*icon);
                }
            }
        }
    }
    nearest.unwrap_or_default()
}

/// Apply a cursor to the window, skipping the platform call when it is already the current one.
pub(crate) fn update_cursor(pw: &mut PerWindow, icon: crate::input::PointerIcon) {
    if pw.cursor_icon == icon {
        return;
    }
    pw.cursor_icon = icon;
    if let Some(sw) = pw.skia_window.as_ref() {
        sw.set_cursor(winit::cursor::Cursor::Icon(icon.cursor_icon()));
    }
}

pub(crate) fn update_hover(pw: &mut PerWindow, scene_pos: (f32, f32)) {
    // overlay 优先：指针在 overlay 内容上 → 只更新 overlay 的 hover，
    // 同时主树 + **其他 overlay** 全部退出 hover（overlay 盖住下层——
    // 对齐真实指针层叠；堆叠 overlay 时下层按钮 hover 不残留）
    if let Some((i, local)) = hit_overlay(pw, scene_pos) {
        let ov = &mut pw.overlay.layers[i];
        overlay_update_hover(ov, local);
        // 主树 hover 全清（指针在 overlay 上——主树不可见）
        let olds: Vec<u64> = pw.hovered_slots.drain().collect();
        for slot in olds {
            exit_hover_at(pw, slot);
        }
        // 其他 overlay（下层）hover 全清
        for j in 0..pw.overlay.layers.len() {
            if j == i { continue; }
            let olds: Vec<u64> = pw.overlay.layers[j].hovered_slots.drain().collect();
            for slot in olds {
                overlay_exit_hover_at(&mut pw.overlay.layers[j], slot);
            }
        }
        let icon = {
            let ov = &pw.overlay.layers[i];
            match ov.composer.layout_root_idx() {
                Some(root) => {
                    let nodes = ov.composer.arena_nodes();
                    let path = hit_test_with_flights(
                        nodes,
                        root,
                        ov.composer.transition_roots(),
                        local.0,
                        local.1,
                    );
                    pointer_icon_for_path(nodes, &path)
                }
                None => crate::input::PointerIcon::DEFAULT,
            }
        };
        update_cursor(pw, icon);
        return;
    }
    // 未命中任何 overlay：主树 hover 更新 + 所有 overlay hover 全清
    // （指针离开 overlay——overlay 的 hover 状态清理）
    for ov in &mut pw.overlay.layers {
        let olds: Vec<u64> = ov.hovered_slots.drain().collect();
        for slot in olds {
            overlay_exit_hover_at(ov, slot);
        }
    }
    // 当前路径上所有 hoverable 的 (slot, interaction)
    let (hit, icon): (Vec<(u64, crate::input::interaction::MutableInteractionSource)>, crate::input::PointerIcon) = {
        let nodes = pw.composer.arena_nodes();
        let Some(r) = pw.composer.layout_root_idx() else { return; };
        let path = hit_test_with_flights(nodes, r, pw.composer.transition_roots(), scene_pos.0, scene_pos.1);
        let hit = path.iter()
            .filter(|&&i| nodes[i].modifier.has_hoverable())
            .filter_map(|&i| nodes[i].modifier.hoverable_interaction().map(|s| (nodes[i].slot_key, s.clone())))
            .collect();
        (hit, pointer_icon_for_path(nodes, &path))
    };
    update_cursor(pw, icon);
    let hit_slots: std::collections::HashSet<u64> = hit.iter().map(|(s, _)| *s).collect();
    // 仍在 hover 的：跳过（已 enter）
    // 新进入：emit enter + 记录
    for (slot, src) in &hit {
        if pw.hovered_slots.insert(*slot) {
            src.emit_hover_enter();
        }
    }
    // 已退出：补 Exit + 移除
    let gone: Vec<u64> = pw.hovered_slots.iter().copied().filter(|s| !hit_slots.contains(s)).collect();
    for slot in gone {
        pw.hovered_slots.remove(&slot);
        exit_hover_at(pw, slot);
    }
}


/// 对指定 slot 的节点补发 Hover Exit（节点已移除则跳过——hover 状态自然清理）
pub(crate) fn exit_hover_at(pw: &mut PerWindow, slot: u64) {
    let nodes = pw.composer.arena_nodes();
    let Some(r) = pw.composer.layout_root_idx() else { return; };
    if let Some(nid) = crate::layout::node::find_node_id_by_slot_key(nodes, r, slot) {
        if let Some(idx) = crate::layout::node::find_node_by_id(nodes, r, nid) {
            if let Some(src) = nodes[idx].modifier.hoverable_interaction() {
                src.emit_hover_exit();
            }
        }
    }
}

/// Compose 风格 click 检测：Down 记录（pointer_down_state）、Up 释放时触发 on_click。
/// 真实 PointerButton Up 与 debug 模拟（d/u）共用——消除平行实现并可模拟验证。
pub(crate) fn detect_click(pw: &mut PerWindow, scene_pos: (f32, f32)) -> bool {
    const CLICK_SLOP: f32 = 18.0;
    const CLICK_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(500);
    let Some(down) = pw.input.down.take() else {
        return false;
    };
    // Down 时所在节点的 slot_key（跨重组稳定——Down 与 Up 之间可能发生重组
    // （如聚焦触发），arena 节点重建 → node_id 变化 → 旧 id 匹配必然失败；
    // slot_key 按组合位置稳定，不受重组影响）
    let down_slot = pw.input.down_slot;
    let dx = scene_pos.0 - down.position.0;
    let dy = scene_pos.1 - down.position.1;
    let dist = (dx * dx + dy * dy).sqrt();
    let in_time = down.time.elapsed() < CLICK_TIMEOUT;
    if dist <= CLICK_SLOP && in_time {
        let nodes = pw.composer.arena_nodes();
        if let Some(r) = pw.composer.layout_root_idx() {
            let path = hit_test_with_flights(nodes, r, pw.composer.transition_roots(), scene_pos.0, scene_pos.1);
            let hit = down_slot
                .map(|slot| path.iter().any(|&i| nodes[i].slot_key == slot))
                .unwrap_or_else(|| path.iter().any(|&i| nodes[i].id == down.node_id));
            if hit {
                // 只在相同节点触发 click（从内到外找第一个 on_click）
                return fire_click_along_path(nodes, &path);
            }
        }
    }
    false
}

/// 键盘事件派发到焦点路径（Preview: root→focused；Bubble: focused→root，
/// 对齐 onPreviewKeyEvent/onKeyEvent）+ 聚焦组件激活（Enter/Space 触发
/// onClick——仅聚焦节点自身的 clickable）。
///
/// 真实 KeyboardInput 与 debug 模拟共用（防行为分叉）：Escape/Tab 等
/// 框架级按键由调用方前置处理（聚焦导航/清焦），不进入本函数。

/// 指针按下核心（真实 PointerButton 与 debug 模拟共用——防行为分叉）。
///
/// 统一：hit_test → 清除旧选区 → grapheme anchor 定位 → PtrDownState 记录 →
/// 光标设置/自动聚焦（仅真实路径 `with_focus`）→ on_pointer_event 分发。
/// 返回 true 表示命中节点并已处理。
pub(crate) fn handle_pointer_down(
    pw: &mut PerWindow,
    scene_pos: (f32, f32),
    kind: crate::input::PointerKind,
    modifiers: &winit::keyboard::ModifiersState,
    with_focus: bool,
) -> bool {
    // 顶层弹出层优先：命中 overlay → 记录点击目标（事件不进主树）；
    // 外部点击 → dismiss（模态/可关闭）
    if overlay_down(pw, scene_pos, kind.clone()) {
        return true;
    }
    let nodes = pw.composer.arena_nodes();
    let Some(r) = pw.composer.layout_root_idx() else { return false; };
    let path = hit_test_with_flights(nodes, r, pw.composer.transition_roots(), scene_pos.0, scene_pos.1);
    let Some(&innermost) = path.last() else { return false; };

    // 清除旧的选区（新点击开始）
    {
        if let Some(reg) = nodes[innermost].registrar.borrow().as_ref().cloned() {
            reg.clear_selection();
        }
    }
    // grapheme anchor 定位。
    // ⚠ TextField 容器化后：paragraph 只缓存在**输入 leaf**，容器节点无
    // cached_paragraph——点击容器空白命中容器 → innermost 无 para。由
    // find_anchor_text_node 处理：向上找有 paragraph 的祖先（命中文本本身），
    // 失败且命中 TextField 容器时向下找子树中的输入 leaf。
    let anchor_node = find_anchor_text_node(nodes, &path, innermost);
    let anchor = anchor_node.and_then(|ai| {
        let borrow = nodes[ai].cached_paragraph.try_borrow().ok()?;
        borrow.as_ref().map(|para| {
            let (ax, ay) = node_abs_position(nodes, r, nodes[ai].id);
            // 段落局部坐标须扣除节点 padding（渲染侧文本画在 content 区 =
            // 节点原点 + padding——不扣则点击位置整体偏 padding 偏移，
            // 光标跳错位置）
            let (pad_s, pad_t, pad_e, _) = nodes[ai].modifier.get_padding_sides();
            let pad_x = if nodes[ai].layout_direction == crate::layout::LayoutDirection::Rtl { pad_e } else { pad_s };
            let tl = crate::text::TextLayout::new(para, 0);
            let hit = tl.get_closest_grapheme_cluster_cluster_at(skia_safe::Point::new(scene_pos.0 - ax - pad_x, scene_pos.1 - ay - pad_t));
            // 定位结果是显示文本偏移（paragraph = 显示文本）——经 OffsetMapping
            // 转回编辑偏移（密码掩码/格式化输入）。
            // ⚠ TextFieldVisual 挂在**容器**——须向上找（offset_mapping_for_node）
            crate::text::field::offset_mapping_for_node(nodes, r, ai)
                .map(|m| m.transformed_to_original(hit))
                .unwrap_or(hit)
        })
    });
    // reg 只用 anchor 节点自己的 registrar（不 fallback active_registrar）——
    // 不可选节点（输出 Text 等未注册）的 node.registrar 为 None，
    // fallback 会取到全局残留（如 Container B 的）→ anchor_registrar
    // 错绑 B → 拖动到 B 时 same_reg=true → 混合偏移 → B 被选
    let own_reg = anchor_node.and_then(|ai| nodes[ai].registrar.borrow().as_ref().cloned());
    let anchor_global = anchor.and_then(|a| {
        anchor_node.and_then(|ai| {
            own_reg.as_ref()
                .and_then(|reg| reg.segment_info(nodes[ai].slot_key).map(|(off, _)| off + a))
        })
    });
    pw.input.down = Some(PtrDownState {
        node_id: nodes[innermost].id,
        position: scene_pos,
        time: std::time::Instant::now(),
        selection_anchor: anchor_global,
        anchor_registrar: own_reg, // None（不可选节点）→ 无 anchor 容器
    });
    pw.input.down_slot = Some(nodes[innermost].slot_key);

    if with_focus {
        // 设置 TextField 光标位置 + selection 更新回调（仅真实路径）。
        // ⚠ 点击聚焦由组件自己决定（TextField 内部 requestFocus）——
        // 框架层不自动聚焦（对标 Compose：clickable/focusable 点击不请求焦点，
        // 焦点由 Tab 导航或显式 requestFocus 获得）。
        // 光标设置在 anchor 节点（输入 leaf）上——光标绘制/回调都在 leaf
        if let (Some(a), Some(ai)) = (anchor, anchor_node) {
            nodes[ai].cursor_index.set(a);
            if let Some(cb) = nodes[ai].cursor_callback.borrow_mut().as_mut() {
                cb(a);
            }
        }
    }

    // 按下交互（clickable 绑定源——Compose Press 语义；置于 nodes 借用结束后）
    press_interaction_down(pw, &path, scene_pos);

    // 手势入口（on_press 立即触发；后续 move/up 由 gesture_node 路由）——
    // 置于 with_focus 块后（nodes 借用结束，避免与 pw mut 冲突）
    gesture_down(pw, scene_pos);

    // 拖拽滚动目标：按下点向上找最近滚动容器。文本选择/组件 drag 手势优先
    // （拖选文本/组件拖拽不滚动——对齐 Compose 最内层 pointerInput 消费）
    pw.input.drag_scroll = (|| {
        let nodes = pw.composer.arena_nodes();
        let Some(r) = pw.composer.layout_root_idx() else { return None };
        let path = hit_test_with_flights(nodes, r, pw.composer.transition_roots(), scene_pos.0, scene_pos.1);
        #[cfg(debug_assertions)]
        if drag_trace_enabled() {
            eprintln!("[drag-down] scene=({},{}) path_len={} path={:?}",
                scene_pos.0, scene_pos.1, path.len(),
                path.iter().map(|&i| (i, nodes[i].slot_key, nodes[i].modifier.vertical_scroll_state().is_some())).collect::<Vec<_>>());
            for &i in &path {
                let n = &nodes[i];
                eprintln!("  [drag-node] idx={} pos=({},{}) size=({},{}) vp_h={} scroll={}",
                    i, n.position.x, n.position.y, n.measured_size.width, n.measured_size.height,
                    n.scroll_viewport_height,
                    n.modifier.vertical_scroll_state().is_some());
            }
        }
        let Some(&_innermost) = path.last() else { return None };
        let selecting = pw.input.down.as_ref()
            .map(|s| s.selection_anchor.is_some())
            .unwrap_or(false);
        // 内层手势优先，但**滚动优先于外层面板拖拽**（对齐 Compose：BottomSheet 面板
        // 整面板可拖，但列表区按下时内容滚动优先——面板 on_drag 是外层 fallback）。
        // child_drag 仅在「drag 节点比 scroll 节点更深」（内层组件如 slider/switch）
        // 时才成立；drag 是 scroll 祖先（面板）→ 滚动优先，不放行 child_drag。
        let drag_idx = path_drag_idx(nodes, &path);
        let scroll_idx = path_scroll_idx(nodes, &path);
        let child_drag = inner_component_drag(drag_idx, scroll_idx);
        // An inner drag component and the scroll it sits in both want this gesture; which one gets it
        // is decided by the finger's dominant axis in `gesture_move`, so record the scroll as the
        // fallback owner here. `None` when the component stands alone (it owns every direction), and
        // `None` while a text selection is in progress: the selection keeps the pointer (it is
        // extended from every move below), so the list must not start scrolling under the finger.
        pw.input.scroll_slot = if child_drag && !selecting {
            scroll_idx.map(|i| nodes[i].slot_key)
        } else {
            None
        };
        if selecting || child_drag {
            None
        } else {
            scroll_idx.map(|i| DragScroll::new(nodes[i].slot_key, scene_pos))
        }
    })();

    // 分发 on_pointer_event（Down）
    let nodes = pw.composer.arena_nodes();
    let ptr_ev = crate::input::PointerEvent {
        event_type: crate::input::PointerEventType::Down,
        position: (0.0, 0.0),
        scene_position: scene_pos,
        kind,
        is_alt_pressed: modifiers.alt_key(),
        is_ctrl_pressed: modifiers.control_key(),
        is_shift_pressed: modifiers.shift_key(),
        is_meta_pressed: modifiers.meta_key(),
    };
    pw.input.last_kind = ptr_ev.kind.clone();
    dispatch_ptr_event(nodes, r, &path, &ptr_ev, scene_pos, pw.input.down_slot);
    true
}

/// 指针移动核心（真实 PointerMoved 与 debug 模拟共用——防行为分叉）。
///
/// 统一：拖拽选区（18px slop + x_off 对齐偏移 + grapheme 定位 + compute_selection）
/// + on_pointer_event 分发。返回是否消费（选择更新或 handler 消费）。
pub(crate) fn handle_pointer_move(
    pw: &mut PerWindow,
    scene_pos: (f32, f32),
    kind: crate::input::PointerKind,
    modifiers: &winit::keyboard::ModifiersState,
) -> bool {
    // 手势驱动（drag capture：tracker 存在即路由——指针移出组件仍接收）
    // ⚠ 必须在 nodes 借用之前（gesture_move 内部自取 nodes/pw mut）
    let mut handled = false;
    if pw.input.node.is_some() && gesture_move(pw, scene_pos) {
        handled = true;
    }
    // overlay 点击 slop 取消：按下 overlay 后拖出 18px → 取消 click（对齐
    // 主树 detect_click 的 slop 判定；down 位置 = overlay screen_pos + local）
    if pw.overlay.click.is_some() {
        let cancel = match pw.overlay.click {
            Some((idx, local, _)) => {
                match pw.overlay.layers.get(idx) {
                    Some(ov) => {
                        let down_screen = (ov.screen_pos.0 + local.0, ov.screen_pos.1 + local.1);
                        let dx = scene_pos.0 - down_screen.0;
                        let dy = scene_pos.1 - down_screen.1;
                        (dx * dx + dy * dy).sqrt() > 18.0
                    }
                    None => false,
                }
            }
            None => false,
        };
        if cancel {
            pw.overlay.click = None;
            release_pressed_interaction(pw);
        }
    }
    if let Some((idx, slot, down_pos)) = pw.overlay.drag {
        let (dx, dy) = (scene_pos.0 - down_pos.0, scene_pos.1 - down_pos.1);
        if !pw.overlay.drag_started {
            if dx * dx + dy * dy >= 8.0 * 8.0 {
                pw.overlay.drag_started = true;
                pw.overlay.drag_last = Some(scene_pos);
                if let Some(ov) = pw.overlay.layers.get(idx) {
                    let nodes = ov.composer.arena_nodes();
                    if let Some(r) = ov.composer.layout_root_idx() {
                        // ⚠ The overlay arena is LAYER-local, and a gesture callback receives
                        // node-local coordinates (`fire_gesture_action` subtracts the node's
                        // position inside the arena it was given). So the absolute position has to
                        // be converted here — passing `scene_pos` shifted it by the popup's screen
                        // origin, which is why dragging a Slider inside a popup landed on the wrong
                        // value (deltas are unaffected: the layer offset cancels in a difference).
                        let local = (
                            scene_pos.0 - pw.overlay.drag_origin.0,
                            scene_pos.1 - pw.overlay.drag_origin.1,
                        );
                        fire_gesture_action(nodes, r, slot,
                            crate::input::gesture::GestureAction::DragStart(local));
                        fire_gesture_action(nodes, r, slot,
                            crate::input::gesture::GestureAction::DragMove(local, (dx, dy)));
                    }
                }
                handled = true;
            }
        } else {
            let last = pw.overlay.drag_last.unwrap_or(down_pos);
            let inc = (scene_pos.0 - last.0, scene_pos.1 - last.1);
            pw.overlay.drag_last = Some(scene_pos);
            if let Some(ov) = pw.overlay.layers.get(idx) {
                let nodes = ov.composer.arena_nodes();
                if let Some(r) = ov.composer.layout_root_idx() {
                    let local = (
                        scene_pos.0 - pw.overlay.drag_origin.0,
                        scene_pos.1 - pw.overlay.drag_origin.1,
                    );
                    fire_gesture_action(nodes, r, slot,
                        crate::input::gesture::GestureAction::DragMove(local, inc));
                }
            }
            handled = true;
        }
    }
    // overlay 内拖拽滚动（独立 Composer——BottomSheet 内 LazyColumn）
    // 列表滚动走 overlay Composer 的 dispatch_nested_scroll_delta，Sheet 通过 panel 上的
    // SheetNested（nested_scroll）吃剩余 available。此分支仅消费列表，能否拖 Sheet 取决于 nested。
    if pw.overlay.drag_scroll.is_some() {
        let target: Option<usize> = (|| {
            let ov_idx = pw.overlay.layers.len().checked_sub(1).unwrap_or(0);
            let slot = pw.overlay.drag_scroll.as_ref().unwrap().slot;
            let ov = pw.overlay.layers.get(ov_idx)?;
            let r = ov.composer.layout_root_idx()?;
            let nodes = ov.composer.arena_nodes();
            let id = crate::layout::node::find_node_id_by_slot_key(nodes, r, slot)?;
            crate::layout::node::find_node_by_id(nodes, r, id)
        })();
        let (dx, dy) = {
            let ds = pw.overlay.drag_scroll.as_mut().unwrap();
            let dx = scene_pos.0 - ds.last_x;
            let dy = scene_pos.1 - ds.last_y;
            ds.last_x = scene_pos.0;
            ds.last_y = scene_pos.1;
            let now = std::time::Instant::now();
            ds.samples.push((now, scene_pos.0, scene_pos.1));
            let cutoff = now - std::time::Duration::from_millis(200);
            ds.samples.retain(|(t, _, _)| *t >= cutoff);
            (dx, dy)
        };
        if let Some(idx) = target {
            let (ax, ay) = {
                let ov_idx = pw.overlay.layers.len().checked_sub(1).unwrap_or(0);
                let ov = &pw.overlay.layers[ov_idx];
                let nodes = ov.composer.arena_nodes();
                if nodes[idx].modifier.vertical_scroll_state().is_some() { (0.0, dy) }
                else if nodes[idx].modifier.horizontal_scroll_state().is_some() { (dx, 0.0) }
                else { (0.0, 0.0) }
            };
            if ax != 0.0 || ay != 0.0 {
                let ov_idx = pw.overlay.layers.len().checked_sub(1).unwrap_or(0);
                if let Some(ov) = pw.overlay.layers.get_mut(ov_idx) {
                    if let Some(root) = ov.composer.layout_root_idx() {
                        let density = crate::unit::Density::from_density(pw.scale_factor as f32);
                        let consumed = dispatch_nested_scroll_delta(ov.composer.arena_nodes_mut(), root, idx, crate::nested_scroll::ScrollDelta::new(ax, ay), crate::nested_scroll::NestedScrollSource::Drag, density);
                        if consumed.x != 0.0 || consumed.y != 0.0 {
                            handled = true;
                        }
                    }
                }
            }
            let ov_idx = pw.overlay.layers.len().checked_sub(1).unwrap_or(0);
            if let Some(ov) = pw.overlay.layers.get(ov_idx) {
                let nodes = ov.composer.arena_nodes();
                if let Some(ss) = nodes[idx].modifier.vertical_scroll_state() { ss.is_scroll_in_progress.set(true); }
                else if let Some(ss) = nodes[idx].modifier.horizontal_scroll_state() { ss.is_scroll_in_progress.set(true); }
            }
        }
    }
    // 拖拽滚动：内容跟随指针 + 记录速度样本（松手 fling 用）。放在手势/文本
    // 选择之前——但按下时已排除组件 drag 手势与文本选择，此处无冲突
    if pw.input.drag_scroll.is_some() {
        let target: Option<usize> = (|| {
            let nodes = pw.composer.arena_nodes();
            let Some(r) = pw.composer.layout_root_idx() else { return None };
            let slot = pw.input.drag_scroll.as_ref().unwrap().slot;
            let id = crate::layout::node::find_node_id_by_slot_key(nodes, r, slot)?;
            crate::layout::node::find_node_by_id(nodes, r, id)
        })();
        let (dx, dy) = {
            let ds = pw.input.drag_scroll.as_mut().unwrap();
            let dx = scene_pos.0 - ds.last_x;
            let dy = scene_pos.1 - ds.last_y;
            ds.last_x = scene_pos.0;
            ds.last_y = scene_pos.1;
            let now = std::time::Instant::now();
            ds.samples.push((now, scene_pos.0, scene_pos.1));
            let cutoff = now - std::time::Duration::from_millis(200);
            ds.samples.retain(|(t, _, _)| *t >= cutoff);
            (dx, dy)
        };
        if let Some(idx) = target {
            #[cfg(debug_assertions)]
            if drag_trace_enabled() {
                let nodes = pw.composer.arena_nodes();
                eprintln!("[drag-move] target_idx={} scroll_v={} scroll_h={} slot={:?}",
                    idx,
                    nodes[idx].modifier.vertical_scroll_state().is_some(),
                    nodes[idx].modifier.horizontal_scroll_state().is_some(),
                    pw.input.drag_scroll.as_ref().map(|d| d.slot));
            }
            // 轴感知：垂直容器吃 dy，水平容器吃 dx（apply_scroll_delta 按节点轴取）
            let (ax, ay) = {
                let nodes = pw.composer.arena_nodes();
                if nodes[idx].modifier.vertical_scroll_state().is_some() {
                    (0.0, dy)
                } else if nodes[idx].modifier.horizontal_scroll_state().is_some() {
                    (dx, 0.0)
                } else {
                    (0.0, 0.0)
                }
            };
            if ax != 0.0 || ay != 0.0 {
                let root = pw.composer.layout_root_idx();
                let consumed = root.map(|root| dispatch_nested_scroll_delta(
                    pw.composer.arena_nodes_mut(),
                    root,
                    idx,
                    crate::nested_scroll::ScrollDelta::new(ax, ay),
                    crate::nested_scroll::NestedScrollSource::Drag,
                    crate::unit::Density::from_density(pw.scale_factor as f32),
                )).unwrap_or(crate::nested_scroll::ScrollDelta::ZERO);
                handled = consumed.x != 0.0 || consumed.y != 0.0;
            }
            // 拖拽中标记（apply_scroll_delta 内部取消 fling 时置 false——这里覆盖）
            let nodes = pw.composer.arena_nodes();
            if let Some(ss) = nodes[idx].modifier.vertical_scroll_state() {
                ss.is_scroll_in_progress.set(true);
            } else if let Some(ss) = nodes[idx].modifier.horizontal_scroll_state() {
                ss.is_scroll_in_progress.set(true);
            }
        }
    }
    // 悬停更新（自身 hit test——不依赖下方 nodes 借用）
    update_hover(pw, scene_pos);
    // 越界 slop：按下交互取消（Compose：press 超过 touch slop → Cancel）
    if pw.input.down.is_some() {
        let down = pw.input.down.as_ref().unwrap();
        let dx = scene_pos.0 - down.position.0;
        let dy = scene_pos.1 - down.position.1;
        if (dx * dx + dy * dy).sqrt() > 18.0 {
            release_pressed_interaction(pw);
            // overlay 点击同步取消（对齐主树 detect_click 的 slop 判定——
            // 拖出 slop 后 up 不应 fire overlay click）
            pw.overlay.click = None;
        }
    }
    let nodes = pw.composer.arena_nodes();
    let Some(r) = pw.composer.layout_root_idx() else { return false; };
    let path = hit_test_with_flights(nodes, r, pw.composer.transition_roots(), scene_pos.0, scene_pos.1);

    // 拖拽选中文本
    if pw.input.down.is_some() {
        if let Some(&innermost) = path.last() {
            let down = pw.input.down.as_ref().unwrap();
            let dx = scene_pos.0 - down.position.0;
            let dy = scene_pos.1 - down.position.1;
            const CLICK_SLOP: f32 = 18.0;
            // 文本选择：按下在可选中文本（anchor 有值）→ **任意移动即更新
            // 选区**（单字母 ~7px < CLICK_SLOP——slop 门槛导致无法选中单
            // 字母/小范围文本；对齐 Compose：按下即进入拖选）。其余场景
            // 保持 slop 判定（clickable/拖拽手势）
            let text_selection = down.selection_anchor.is_some();
            if text_selection || (dx * dx + dy * dy).sqrt() > CLICK_SLOP {
                let (abs_x, abs_y) = node_abs_position(nodes, r, nodes[innermost].id);
                if let Ok(borrow) = nodes[innermost].cached_paragraph.try_borrow() {
                    if let Some(para) = borrow.as_ref() {
                        // 对齐偏移（匹配渲染侧 x_off）——绘制用内容盒，选区高亮须一致
                        //（飞行中 measured_size 是父级被告知的尺寸，review R3-F6）
                        let node_w = nodes[innermost].content_box().width;
                        let align = nodes[innermost].modifier.align().unwrap_or(crate::text::TextAlign::Left);
                        // The slack rule has to be the PAINT paths' rule, not a copy of the old
                        // one: a WRAPPED paragraph was laid out inside its box, so a negative
                        // slack would push it out of it — but an UNWRAPPED one
                        // (`soft_wrap(false)`, laid out at its intrinsic width) can be WIDER than
                        // the box, and then centring or right-aligning has to shift it the other
                        // way. With the clamped form here, a selection on an overflowing label
                        // was skewed by half the overflow (`render.rs` uses the conditional one
                        // at both of its sites).
                        let soft_wrap = nodes[innermost]
                            .modifier
                            .elements()
                            .iter()
                            .find_map(|el| match el {
                                crate::modifier::ModifierElement::TextContent { soft_wrap, .. } => Some(*soft_wrap),
                                _ => None,
                            })
                            .unwrap_or(true);
                        let slack = if soft_wrap {
                            (node_w - para.max_intrinsic_width()).max(0.0)
                        } else {
                            node_w - para.max_intrinsic_width()
                        };
                        let x_off = match align {
                            crate::text::TextAlign::Center => abs_x + slack / 2.0,
                            crate::text::TextAlign::Right => abs_x + slack,
                            _ => abs_x,
                        };
                        // 段落局部坐标：扣节点 padding（与渲染侧 content 区一致——
                        // 否则拖动选区整体偏 padding 偏移）
                        let (pad_s, pad_t, pad_e, _) = nodes[innermost].modifier.get_padding_sides();
                        let pad_x = if nodes[innermost].layout_direction == crate::layout::LayoutDirection::Rtl { pad_e } else { pad_s };
                        let tl = crate::text::TextLayout::new(para, 0);
                        {
                            let down = pw.input.down.as_ref().unwrap();
                            // 不可选节点（未注册到任何 SelectionContainer）→ 不更新选择
                            // （用 if let 包裹而非 else return——return 会跳过 dispatch_ptr_event/request_redraw）
                            if let Some(reg) = nodes[innermost].registrar.borrow().as_ref().cloned() {
                                let current_index = tl.get_closest_grapheme_cluster_cluster_at(
                                    skia_safe::Point::new(scene_pos.0 - x_off - pad_x, scene_pos.1 - abs_y - pad_t));
                                // 显示偏移 → 编辑偏移（密码掩码/格式化输入；
                                // TextFieldVisual 在容器——向上找）
                                let current_index = crate::text::field::offset_mapping_for_node(nodes, r, innermost)
                                    .map(|m| m.transformed_to_original(current_index))
                                    .unwrap_or(current_index);
                                let cur_off = reg.segment_info(nodes[innermost].slot_key).map(|(off, _)| off);
                                if let Some((target, s, e)) = crate::components::selection_container::compute_selection(
                                    down.anchor_registrar.as_ref(), down.selection_anchor,
                                    &reg, cur_off, current_index,
                                    scene_pos.1, down.position.1, abs_y,
                                ) {
                                    // 范围是编辑偏移（anchor/current 已转回）——
                                    // reg 空间 = 显示偏移，写入选区前转换
                                    let (ts, te) = crate::text::field::offset_mapping_for_node(nodes, r, innermost)
                                        .map(|m| (m.original_to_transformed(s), m.original_to_transformed(e)))
                                        .unwrap_or((s, e));
                                    target.set_selection(ts, te);
                                    handled = true;
                                }
                            } // end if let Some(reg)
                        }
                    }
                }
            }
        }
    }

    // 分发 on_pointer_event（悬停 Move 也可能更新 State）
    let nodes = pw.composer.arena_nodes();
    let ptr_ev = crate::input::PointerEvent {
        event_type: crate::input::PointerEventType::Move,
        position: (0.0, 0.0),
        scene_position: scene_pos,
        kind,
        is_alt_pressed: modifiers.alt_key(),
        is_ctrl_pressed: modifiers.control_key(),
        is_shift_pressed: modifiers.shift_key(),
        is_meta_pressed: modifiers.meta_key(),
    };
    pw.input.last_kind = ptr_ev.kind.clone();
    let consumed = dispatch_ptr_event(nodes, r, &path, &ptr_ev, scene_pos, pw.input.down_slot);
    handled || consumed
}

