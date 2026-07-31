//! 应用壳 — run_app + 窗口管理 + 事件循环（多窗口）

use std::time::Instant;

use crate::core::composer::{ComposeCtx, Composer};
use crate::debug;
use crate::layout::constraints::Constraints;
use crate::layout::node::{hit_test, focus_next, focus_prev, LayoutNode};
use crate::modifier::Dimension;
use crate::render;
pub(crate) struct PendingWindow {
    pub width: f32,
    pub height: f32,
    pub title: String,
    pub content: Option<Box<dyn Fn(&mut ComposeCtx) + Send>>,
    pub on_close: Option<Box<dyn FnMut() + Send>>,
    pub created_id: Option<u64>,
    pub theme: Option<crate::ui::theme::ThemeColors>,
}
use skiwin::{SkiaWindowTrait, vulkan::VulkanSkiaWindow};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use winit::application::ApplicationHandler;use winit::event::{StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::WindowId;

// ── PerWindow ──

pub(crate) struct PerWindow {
    pub(crate) composer: Composer,
    pub(crate) skia_window: Option<VulkanSkiaWindow>,
    width: f32, height: f32,
    pub(crate) scale_factor: f64,
    pub(crate) focused_id: Option<u64>,
    pub(crate) content: Box<dyn Fn(&mut ComposeCtx)>,
    pub(crate) on_close: Option<Box<dyn FnMut() + Send>>,
    pub(crate) created_id: Option<u64>,
    theme: crate::ui::theme::ThemeColors,
    /// 焦点节点的 slot_key
    pub(crate) focused_slot_key: Option<u64>,
    /// 指针按下态（Compose 风格 click 检测）
    pointer_down_state: Option<PtrDownState>,
    /// 最近的 PointerKind（Move 事件继承自上一个 Down）
    last_pointer_kind: crate::modifier::PointerKind,
    /// Down 时的最内层节点 ID（后续 Move/Up 优先发给此节点，而非 hit_test）
    pointer_down_slot: Option<u64>,
}

/// Compose 风格的 click 检测中间状态
struct PtrDownState {
    node_id: u64,
    position: (f32, f32),
    time: Instant,
    /// 文本选区的起始字符位置（Down 时记录）
    selection_anchor: Option<usize>,
}

impl PerWindow {
    fn new(content: Box<dyn Fn(&mut ComposeCtx)>, width: f32, height: f32, theme: crate::ui::theme::ThemeColors) -> Self {
        PerWindow { composer: Composer::new(), skia_window: None, width, height, scale_factor: 1.0, focused_id: None, content, on_close: None, created_id: None, theme, focused_slot_key: None, pointer_down_state: None, last_pointer_kind: crate::modifier::PointerKind::Mouse { button: crate::modifier::PointerButton::Primary }, pointer_down_slot: None }
    }
    pub(crate) fn created_id(&self) -> Option<u64> { self.created_id }

    /// 清除焦点 + 更新缓存
    fn clear_focus(&mut self, root: &mut LayoutNode) {
        crate::layout::node::clear_focus(root);
        self.focused_id = None;
        self.focused_slot_key = None;
        if let Some(ref sw) = self.skia_window { sw.set_ime_allowed(false); }
    }

    /// 从当前焦点节点刷新 cached 字段
    fn refresh_focus(&mut self, root: &LayoutNode) {
        // 如果树中已有焦点节点，直接读取
        if let Some(fid) = crate::layout::node::get_focus_id(root) {
            self.focused_id = Some(fid);
            self.focused_slot_key = crate::layout::node::find_node_by_id(root, fid).map(|n| n.slot_key);
        }
        // 否则：如果之前有关焦点但树中丢失了（重组后新节点 focus=false），保留 focused_id
        // （由后续触发的 set_focus_by_id 或 pointer 事件补上树的焦点标记）
    }

    /// 增量重组 → 恢复焦点 → 布局 → 渲染（供 RedrawRequested 使用）
    /// 循环消费 notify 队列直到稳定，避免 tokio task 的并发通知丢失。
    fn recompose_layout_render(&mut self, after_draw: impl FnOnce(&LayoutNode, &mut skia_safe::Surface)) {
        // 清除待关闭标志——只捕获本次重组的 on_remove，防止跨窗口污染
        crate::ui::window::reset_pending_remove();
        // 循环 compose 直到没有新的 pending state——处理并发 task 在 compose 期间
        // 完成的 case（第二个 notify 的 state 在第一次 compose 之后才入队）
        // 循环 compose 直到没有新的 pending state
        loop {
            let did_compose = self.composer.recompose(|ctx| (self.content)(ctx));
            if let Some(slot_key) = self.focused_slot_key {
                if let Some(r) = self.composer.layout_root_mut() {
                    if let Some(new_id) = crate::layout::node::find_node_id_by_slot_key(r, slot_key) {
                        crate::layout::node::clear_focus(r);
                        crate::layout::node::set_focus_by_id(r, new_id);
                        self.focused_id = Some(new_id);
                            eprintln!("[focus] restored by slot_key id={}", new_id);
                    } else {
                        self.focused_id = None;
                        self.focused_slot_key = None;
                    }
                }
            }
            // 如果在 compose 期间又有新 notify 入队，需要再处理一次
            if !did_compose && !self.composer.has_pending_states() {
                break;
            }
        }
        self.composer.layout(Constraints::new(0.0, self.width, 0.0, self.height));

        let bg = self.theme.background;

        if let Some(ref mut sw) = self.skia_window {
            if let Some(root) = self.composer.layout_root() {
                let sf = self.scale_factor as f32;
                sw.draw(|surface| {
                    let canvas = surface.canvas();
                    canvas.clear(skia_safe::Color::from_argb(bg.a, bg.r, bg.g, bg.b));
                    canvas.save();
                    canvas.scale((sf, sf));
                    render::render(root, canvas);
                    canvas.restore();
                    after_draw(root, surface);
                });
            }
        }
    }
}

// ── AppState ──

struct AppState {
    /// 已创建的窗口
    windows: HashMap<WindowId, PerWindow>,
    /// 待创建的窗口（由 Window composable 排队）
    pending_content: Vec<PendingWindow>,
    /// 父窗口 ID（用于 is_parent 判断，不依赖 HashMap 顺序）
    parent_window_id: Option<WindowId>,
    /// 窗口全局修饰键状态
    pub(crate) modifiers: winit::keyboard::ModifiersState,
    /// 初始化回调（仅首次调用，用于声明式创建主窗口）
    init: Option<Box<dyn FnOnce(&mut ComposeCtx)>>,
}

impl ApplicationHandler for AppState {
        fn new_events(&mut self, event_loop: &dyn ActiveEventLoop, _cause: StartCause) {
        // 动画运行时用 Poll 保持连续渲染，空闲时 Wait 省电
        if crate::animation::is_animating() {
            event_loop.set_control_flow(ControlFlow::Poll);
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }

    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        AppState::process_pending_windows(self, event_loop);
    }

    fn resumed(&mut self, _event_loop: &dyn ActiveEventLoop) {}

    fn proxy_wake_up(&mut self, event_loop: &dyn ActiveEventLoop) {
        // 如果 debug server 请求关闭，退出事件循环
        if debug::is_shutdown() {
            event_loop.exit();
            return;
        }
        AppState::process_pending_windows(self, event_loop);
        // 消费 pending close（on_remove 推入，compose 末尾也消费一次）
        crate::ui::window::Window::process_detached(&mut self.windows, event_loop, &|| debug::force_shutdown());
        // 处理 close_window_by_id 请求（先 drain 再处理，避免持锁调用 cb）
        let queue = std::mem::take(&mut *CLOSE_QUEUED.lock().unwrap());
        for cid in queue {
            let to_close: Vec<WindowId> = self.windows.iter()
                .filter(|(_, pw)| pw.created_id() == Some(cid))
                .map(|(wid, _)| *wid)
                .collect();
            for wid in to_close {
                if let Some(mut pw) = self.windows.remove(&wid) {
                    if let Some(ref mut cb) = pw.on_close { cb(); }
                    for pw2 in self.windows.values() {
                        if let Some(ref sw) = pw2.skia_window { sw.request_redraw(); }
                    }
                    if self.windows.is_empty() { debug::force_shutdown(); event_loop.exit(); }
                }
            }
        }
        // 调试工具有 pending 请求时唤醒窗口（截图/模拟事件需要 RedrawRequested）
        if debug::has_pending() {
            for pw in self.windows.values() {
                if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
            }
        }
        // 异步 State 变更唤醒事件循环后需要 request_redraw
        for pw in self.windows.values() {
            if pw.composer.has_pending_states() {
                if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let is_parent = self.parent_window_id.map(|p| p == window_id).unwrap_or(false);
        let closing_last = self.windows.len() == 1;
        let Some(pw) = self.windows.get_mut(&window_id) else { return };

        match event {
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, y) => y * 20.0,
                    winit::event::MouseScrollDelta::PixelDelta(p) => p.y as f32,
                };
                if dy != 0.0 {
                    if let Some(root) = pw.composer.layout_root_mut() { apply_scroll_delta(root, dy); }
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                }
            }
            WindowEvent::CloseRequested => {
                if let Some(ref mut cb) = pw.on_close { cb(); }
                self.windows.remove(&window_id);
                // 通知其他窗口重绘（状态可能已变化）
                for pw in self.windows.values() {
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                }
                if closing_last || is_parent {
                    debug::force_shutdown();
                    event_loop.exit();
                }
            }
            WindowEvent::Destroyed => {
                if let Some(cid) = pw.created_id { crate::ui::window::CREATED.lock().unwrap().remove(&cid); }
                self.windows.remove(&window_id);
                for pw in self.windows.values() {
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                }
                if self.windows.is_empty() {
                    event_loop.exit();
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                pw.scale_factor = scale_factor;
                if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
            }
            WindowEvent::PointerButton { position, state, button, .. } => {
                let lp = position.to_logical::<f32>(pw.scale_factor);
                let scene_pos = (lp.x, lp.y);
                let event_type = if state.is_pressed() {
                    crate::modifier::PointerEventType::Down
                } else {
                    crate::modifier::PointerEventType::Up
                };
                if state.is_pressed() {
                    // ── Down：记录按下态 + 清除旧选区 ──
                    let (focusable_id, is_focusable) = if let Some(root) = pw.composer.layout_root() {
                        let path = hit_test(root, scene_pos.0, scene_pos.1);
                        if let Some(innermost) = path.last() {
                            let fid = innermost.id;
                            let f = crate::layout::node::has_focusable_modifier(innermost);
                            // 清除旧的选区（新点击开始）
                            {
                                let reg = innermost.registrar.borrow().as_ref().cloned()
                                    .unwrap_or_else(|| crate::ui::selection_container::active_registrar());
                                reg.clear_selection();
                            }
                            let anchor = if let Ok(borrow) = innermost.cached_paragraph.try_borrow() {
                                if let Some(para) = borrow.as_ref() {
                                    let (ax, ay) = node_abs_position(root, innermost.id);
                                    let tl = crate::text::TextLayout::new(para, 0);
                                    let closest = tl.get_closest_grapheme_cluster_cluster_at(skia_safe::Point::new(scene_pos.0 - ax, scene_pos.1 - ay));
                                    eprintln!("[cursor] click scene=({:.0},{:.0}) abs=({:.0},{:.0}) local=({:.0},{:.0}) anchor={:?}",
                                        scene_pos.0, scene_pos.1, ax, ay, scene_pos.0 - ax, scene_pos.1 - ay, closest);
                                    Some(closest)
                                } else { eprintln!("[cursor] para None"); None }
                            } else { eprintln!("[cursor] try_borrow failed"); None };
                            pw.pointer_down_state = Some(PtrDownState {
                                node_id: innermost.id, position: scene_pos, time: Instant::now(), selection_anchor: None });
                            pw.pointer_down_slot = Some(innermost.slot_key);
                            // 将 anchor 转为全局索引再存入
                            if let Some(a) = anchor {
                                let reg = innermost.registrar.borrow().as_ref().cloned()
                                    .unwrap_or_else(|| crate::ui::selection_container::active_registrar());
                                let seg = reg.segment_info(innermost.slot_key);
                                let global_a = seg.map(|(off,_)| off + a).unwrap_or(a);
                                pw.pointer_down_state.as_mut().map(|s| s.selection_anchor = Some(global_a));
                                // 设置 TextField 光标位置
                                innermost.cursor_index.set(a);
                                // 触发 TextField 的 selection 更新回调
                                if let Some(cb) = innermost.cursor_callback.borrow_mut().as_mut() {
                                    cb(a);
                                }
                            }
                            (Some(fid), f)
                        } else { (None, false) }
                    } else { (None, false) };
                    // 点击自动聚焦
                    if is_focusable { if let Some(id) = focusable_id { if let Some(root) = pw.composer.layout_root_mut() { crate::layout::node::clear_focus(root); crate::layout::node::set_focus_by_id(root, id); } if let Some(ref sw) = pw.skia_window { sw.set_ime_allowed(true); } } }
                    // ── Up：Compose 风格 click 检测 ──
                    const CLICK_SLOP: f32 = 18.0;
                    const CLICK_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(500);
                    if let Some(down) = pw.pointer_down_state.take() {
                        let dx = scene_pos.0 - down.position.0;
                        let dy = scene_pos.1 - down.position.1;
                        let dist = (dx * dx + dy * dy).sqrt();
                        let in_time = down.time.elapsed() < CLICK_TIMEOUT;
                        if dist <= CLICK_SLOP && in_time {
                            if let Some(root) = pw.composer.layout_root() {
                                let path = hit_test(root, scene_pos.0, scene_pos.1);
                                if path.iter().any(|n| n.id == down.node_id) {
                                    // 只在相同节点触发 click
                                    for node in path.iter().rev() {
                                        if let Some(on_click) = node.modifier.on_click() {
                                            on_click();
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                // ── 指针事件分发（Up 时先分发后清除 capture）──
                if let Some(root) = pw.composer.layout_root() {
                    let path = hit_test(root, scene_pos.0, scene_pos.1);
                    let ptr_ev = crate::modifier::PointerEvent {
                        event_type,
                        position: (0.0, 0.0),
                        scene_position: scene_pos,
                        kind: crate::modifier::PointerKind::from_button_source(&button),
                        is_alt_pressed: self.modifiers.alt_key(),
                        is_ctrl_pressed: self.modifiers.control_key(),
                        is_shift_pressed: self.modifiers.shift_key(),
                        is_meta_pressed: self.modifiers.meta_key(),
                    };
                    pw.last_pointer_kind = ptr_ev.kind.clone();
                    dispatch_ptr_event(root, &path, &ptr_ev, scene_pos, pw.pointer_down_slot);
                }
                // Up 后清除 capture + 通知选区变化
                if !state.is_pressed() {
                    // Compose 方式：从拖拽节点 slot_key 取 registrar 直接 fire
                    if let Some(slot) = pw.pointer_down_slot {
                        if let Some(root) = pw.composer.layout_root() {
                            if let Some(nid) = crate::layout::node::find_node_id_by_slot_key(root, slot) {
                                if let Some(node) = crate::layout::node::find_node_by_id(root, nid) {
                                    if let Some(reg) = node.registrar.borrow().as_ref() {
                                        reg.fire_on_change();
                                    }
                                }
                            }
                        }
                    }
                    pw.pointer_down_slot = None;
                }
                if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                event_loop.set_control_flow(ControlFlow::Poll);
                if let Some(root) = pw.composer.layout_root_mut() {
                    let fid = crate::layout::node::get_focus_id(root);
                    let slot = fid.and_then(|id| crate::layout::node::find_node_by_id(root, id).map(|n| n.slot_key));
                    pw.focused_id = fid;
                    pw.focused_slot_key = slot;
                }
                if let Some(ref proxy) = *APP_PROXY.lock().unwrap() { let _ = proxy.wake_up(); }
            }
            WindowEvent::PointerMoved { position, .. } => {
                let lp = position.to_logical::<f32>(pw.scale_factor);
                let scene_pos = (lp.x, lp.y);
                if let Some(root) = pw.composer.layout_root() {
                    let path = hit_test(root, scene_pos.0, scene_pos.1);
                    // 拖拽选中文本
                    if pw.pointer_down_state.is_some() {
                        if let Some(innermost) = path.last() {
                            let down = pw.pointer_down_state.as_ref().unwrap();
                            let dx = scene_pos.0 - down.position.0;
                            let dy = scene_pos.1 - down.position.1;
                            const CLICK_SLOP: f32 = 18.0;
                            if (dx*dx + dy*dy).sqrt() > CLICK_SLOP {
                                let (abs_x, abs_y) = node_abs_position(root, innermost.id);
                                if let Ok(borrow) = innermost.cached_paragraph.try_borrow() {
                                    if let Some(para) = borrow.as_ref() {
                                        // 对齐偏移（匹配渲染侧 x_off）
                                        let node_w = innermost.measured_size.width;
                                        let align = innermost.modifier.align().unwrap_or(crate::ui::TextAlign::Left);
                                        let x_off = match align {
                                            crate::ui::TextAlign::Center => abs_x + (node_w - para.max_intrinsic_width()).max(0.0) / 2.0,
                                            crate::ui::TextAlign::Right => abs_x + (node_w - para.max_intrinsic_width()).max(0.0),
                                            _ => abs_x,
                                        };
                                        let tl = crate::text::TextLayout::new(para, 0);
                                        {
                                            let reg = innermost.registrar.borrow().as_ref().cloned()
                                                .unwrap_or_else(|| crate::ui::selection_container::active_registrar());
                                            // 只更新注册到 SelectionContainer 的节点
                                            if let Some((global_off, _)) = reg.segment_info(innermost.slot_key) {
                                                let current_index = tl.get_closest_grapheme_cluster_cluster_at(skia_safe::Point::new(scene_pos.0 - x_off, scene_pos.1 - abs_y));
                                                let current_global = global_off + current_index;
                                                let anchor_global = pw.pointer_down_state.as_ref().and_then(|d| d.selection_anchor);
                                                let s = anchor_global.map(|a| a.min(current_global)).unwrap_or(current_global);
                                                let e = anchor_global.map(|a| a.max(current_global)).unwrap_or(current_global + 1);
                                                eprintln!("[selection] set global range={}..{}", s, e);
                                                reg.set_selection(s, e);
                                            } else if let Some(anchor_global) = pw.pointer_down_state.as_ref().and_then(|d| d.selection_anchor) {
                                                // 鼠标超出 SelectionContainer：扩展到边界
                                                let total_len = reg.total_text_len();
                                                let edge = if scene_pos.1 < abs_y { 0 } else { total_len };
                                                let s = anchor_global.min(edge);
                                                let e = anchor_global.max(edge);
                                                eprintln!("[selection] edge snap range={}..{}", s, e);
                                                reg.set_selection(s, e);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    let ptr_ev = crate::modifier::PointerEvent {
                        event_type: crate::modifier::PointerEventType::Move,
                        position: (0.0, 0.0),
                        scene_position: scene_pos,
                        kind: pw.last_pointer_kind.clone(),
                        is_alt_pressed: self.modifiers.alt_key(),
                        is_ctrl_pressed: self.modifiers.control_key(),
                        is_shift_pressed: self.modifiers.shift_key(),
                        is_meta_pressed: self.modifiers.meta_key(),
                    };
                    dispatch_ptr_event(root, &path, &ptr_ev, scene_pos, pw.pointer_down_slot);
                    // 拖拽选区后请求重绘
                    if pw.pointer_down_state.is_some() {
                        if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    }
                }
            }
            WindowEvent::ModifiersChanged(m) => {
                self.modifiers = m.state();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let event_type = if event.state.is_pressed() {
                    crate::modifier::KbEventType::KeyDown
                } else {
                    crate::modifier::KbEventType::KeyUp
                };
                let ke = crate::modifier::KbEvent {
                    key: event.logical_key.clone(),
                    event_type,
                    is_alt_pressed: self.modifiers.alt_key(),
                    is_ctrl_pressed: self.modifiers.control_key(),
                    is_shift_pressed: self.modifiers.shift_key(),
                    is_meta_pressed: self.modifiers.meta_key(),
                    repeat: event.repeat,
                };
                let mut consumed = false;
                if event.state.is_pressed() && matches!(&event.logical_key, Key::Named(NamedKey::Escape)) {
                    if pw.focused_id.is_some() {
                        if let Some(root) = pw.composer.layout_root_mut() {
                            crate::layout::node::clear_focus(root);
                        }
                        pw.focused_id = None;
                        pw.focused_slot_key = None;
                        consumed = true;
                    }
                }
                if event.state.is_pressed() && matches!(&event.logical_key, Key::Named(NamedKey::Tab)) {
                    let shift = self.modifiers.shift_key();
                    let (new_id, new_slot) = pw.composer.layout_root_mut().map(|root| {
                        if shift { focus_prev(root); } else { focus_next(root); }
                        let id = crate::layout::node::get_focus_id(root);
                        let slot = id.and_then(|fid| crate::layout::node::find_node_by_id(root, fid).map(|n| n.slot_key));
                        (id, slot)
                    }).unwrap_or((None, None));
                    pw.focused_id = new_id;
                    pw.focused_slot_key = new_slot;
                    consumed = true;
                }
                if !consumed {
                    if let Some(fid) = pw.focused_id {
                        if let Some(root) = pw.composer.layout_root() {
                            // 收集焦点路径：root → ... → focused
                            let mut path: Vec<&LayoutNode> = Vec::new();
                            if let Some(node) = crate::layout::node::find_node_by_id(root, fid) {
                                path.push(node);
                                // 向上收集父链
                                let mut pid = node.parent_id;
                                while let Some(id) = pid {
                                    if let Some(anc) = crate::layout::node::find_node_by_id(root, id) {
                                        path.push(anc);
                                        pid = anc.parent_id;
                                    } else { break; }
                                }
                                path.reverse(); // 现在 path[0] == root, path[last] == focused
                            }

                            // Preview: root → focused（对齐 onPreviewKeyEvent）
                            for node in &path {
                                for el in node.modifier.elements() {
                                    if let crate::modifier::ModifierElement::KbEvent { on_pre_key: Some(handler), .. } = el {
                                        if handler(&ke) { consumed = true; break; }
                                    }
                                }
                                if consumed { break; }
                            }
                            if !consumed {
                                // Bubble: focused → root（对齐 onKeyEvent）
                                for node in path.iter().rev() {
                                    for el in node.modifier.elements().iter().rev() {
                                        if let crate::modifier::ModifierElement::KbEvent { on_key: Some(handler), .. } = el {
                                            if handler(&ke) { consumed = true; break; }
                                        }
                                    }
                                    if consumed { break; }
                                }
                            }
                        }
                    }
                }
                if consumed {
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    event_loop.set_control_flow(ControlFlow::Poll);
                }
            }
            WindowEvent::Ime(ime) => {
                use winit::event::Ime;
                match ime {
                    Ime::Preedit(text, cursor) => {
                        // 通过 focused node 的 ime_callback 通知 TextField
                        if let Some(fid) = pw.focused_id {
                            if let Some(root) = pw.composer.layout_root() {
                                if let Some(node) = crate::layout::node::find_node_by_id(root, fid) {
                                    if let Some(cb) = node.ime_callback.borrow_mut().as_mut() {
                                        cb(&text, cursor);
                                    }
                                }
                            }
                        }
                        if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                    }
                    Ime::Commit(text) => {
                        // IME 提交文本——派发给聚焦节点的 on_key_event 以 Character 形式
                        if let Some(fid) = pw.focused_id {
                            if let Some(root) = pw.composer.layout_root_mut() {
                                // 逐字符发送
                                for ch in text.chars() {
                                    let s = ch.to_string();
                                    let ke = crate::modifier::KbEvent {
                                        key: winit::keyboard::Key::Character(s.clone().into()),
                                        event_type: crate::modifier::KbEventType::KeyDown,
                                        is_alt_pressed: false, is_ctrl_pressed: false,
                                        is_shift_pressed: false, is_meta_pressed: false,
                                        repeat: false,
                                    };
                                    let mut path = Vec::new();
                                    if let Some(node) = crate::layout::node::find_node_by_id(root, fid) {
                                        path.push(node);
                                        let mut pid = node.parent_id;
                                        while let Some(id) = pid {
                                            if let Some(anc) = crate::layout::node::find_node_by_id(root, id) {
                                                path.push(anc); pid = anc.parent_id;
                                            } else { break; }
                                        }
                                        path.reverse();
                                    }
                                    for node in path.iter().rev() {
                                        let mut consumed = false;
                                        for el in node.modifier.elements().iter().rev() {
                                            if let crate::modifier::ModifierElement::KbEvent { on_key: Some(handler), .. } = el {
                                                if handler(&ke) { consumed = true; break; }
                                            }
                                        }
                                        if consumed { break; }
                                    }
                                }
                                if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                            }
                        }
                    }
                    Ime::Enabled | Ime::Disabled => {}
                    _ => {}
                }
            }
            WindowEvent::SurfaceResized(s) => {
                let l = s.to_logical::<f32>(pw.scale_factor);
                if (l.width - pw.width).abs() > pw.width * 0.5
                    || (l.height - pw.height).abs() > pw.height * 0.5 { /* winit bug #2094 */ }
                else { pw.width = l.width; pw.height = l.height; }
                if let Some(ref mut sw) = pw.skia_window { sw.resize(); }
                // 确保下一帧以新尺寸重新布局（有些平台 resize 后不自动触发 RedrawRequested）
                if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
            }
            WindowEvent::RedrawRequested => {
                // 在 compose 前更新动画（确保渲染使用最新值，消除一帧滞后抖动）
                if crate::animation::update_animations() {
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                }
                // 消费焦点请求（在 compose 前处理，避免丢失）
                for id in crate::modifier::take_focus_requests() {
                    if let Some(root) = pw.composer.layout_root_mut() {
                        if crate::layout::node::focus_by_id(root, id) {
                            pw.focused_id = crate::layout::node::get_focus_id(root);
                        }
                    }
                    // 单独查 slot_key（避免与 root 的 borrow 冲突）
                    if let Some(fid) = pw.focused_id {
                        if let Some(root) = pw.composer.layout_root() {
                            if let Some(found) = crate::layout::node::find_node_by_id(root, fid) {
                                pw.focused_slot_key = Some(found.slot_key);
                            }
                        }
                    }
                    if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
                }
                // 增量重组 → 布局 → 渲染
                let w = pw.width;
                let h = pw.height;
                let sf = pw.scale_factor as f32;
                pw.recompose_layout_render(|root, surface| {
                    debug::update_tree(&debug::build_tree_json(root));
                    if debug::screenshot_requested() {
                        let (pw2, ph2) = ((w * sf) as i32, (h * sf) as i32);
                        let info = skia_safe::ImageInfo::new((pw2, ph2), skia_safe::ColorType::RGBA8888, skia_safe::AlphaType::Premul, None);
                        let mut pixels = vec![0u8; (pw2 * ph2 * 4) as usize];
                        if surface.read_pixels(&info, &mut pixels, pw2 as usize * 4, (0, 0)) {
                            debug::update_pixels(&pixels, pw2 as u32, ph2 as u32);
                        }
                        debug::screenshot_done();
                    }
                });
                // IME 光标区域更新（输入法候选框跟随光标位置）
                if let Some(ref sw) = pw.skia_window {
                    if let Some(fid) = pw.focused_id {
                        if let Some(root) = pw.composer.layout_root() {
                            if let Some(para) = crate::layout::node::find_node_by_id(root, fid) {
                                if let Ok(borrow) = para.cached_paragraph.try_borrow() {
                                    if let Some(p) = borrow.as_ref() {
                                        let tl = crate::text::TextLayout::new(
                                            p,
                                            p.paragraph_byte_to_real_indices.len(),
                                        );
                                        if let Some((cx, cy, ch)) = tl.get_cursor_position(para.cursor_index.get()) {
                                            let abs = node_abs_position(root, fid);
                                            let align = para.modifier.align().unwrap_or(crate::ui::TextAlign::Left);
                                            let node_w = para.measured_size.width;
                                            let intrinsic_w = p.max_intrinsic_width();
                                            let x_off = match align {
                                                crate::ui::TextAlign::Left | crate::ui::TextAlign::Justify => abs.0,
                                                crate::ui::TextAlign::Center => abs.0 + (node_w - intrinsic_w).max(0.0) / 2.0,
                                                crate::ui::TextAlign::Right => abs.0 + (node_w - intrinsic_w).max(0.0),
                                            };
                                            let x = (x_off + cx) as f64;
                                            let y = (abs.1 + cy) as f64;
                                            let _ = sw.request_ime_update(
                                                winit::window::ImeRequest::Update(
                                                    winit::window::ImeRequestData::default()
                                                        .with_cursor_area(
                                                            winit::dpi::Position::Logical(winit::dpi::LogicalPosition::new(x, y)),
                                                            winit::dpi::Size::Logical(winit::dpi::LogicalSize::new(2.0, ch as f64)),
                                                        )
                                                )
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                // 检查 compose 后是否有待关闭窗口
                if crate::ui::window::Window::has_pending_close() {
                    if let Some(ref proxy) = *APP_PROXY.lock().unwrap() { let _ = proxy.wake_up(); }
                }
                // DevTools 事件（仅父窗口消费，防止多窗口抢）
                if !is_parent { return; }
                let mut handled = false;
                for evt in debug::take_queued_events() {
                    match evt {
                        debug::DebugEvent::Click { x, y } => {
                            if let Some(root) = pw.composer.layout_root() {
                                let nodes = hit_test(root, x, y);
                                let mut click_handled = false;
                                let (fid, sk) = nodes.last()
                                    .filter(|n| crate::layout::node::has_focusable_modifier(n))
                                    .map(|n| (n.id, n.slot_key))
                                    .unwrap_or((0, 0));
                                let path_len = nodes.len();
                                // Click 检测（消耗 nodes/root 前做）
                                for node in nodes.iter().rev() {
                                    if click_handled { break; }
                                    if let Some(on_click) = node.modifier.on_click() {
                                        on_click();
                                        handled = true;
                                        click_handled = true;
                                    }
                                }
                                std::mem::drop(nodes);
                                std::mem::drop(root);
                                if fid != 0 {
                                    if let Some(root_mut) = pw.composer.layout_root_mut() {
                                        crate::layout::node::clear_focus(root_mut);
                                        crate::layout::node::set_focus_by_id(root_mut, fid);
                                    }
                                    pw.focused_id = Some(fid);
                                    pw.focused_slot_key = Some(sk);
                                    if let Some(ref sw) = pw.skia_window { sw.set_ime_allowed(true); }
                                }
                                eprintln!("[debug-click] pos=({:.0},{:.0}) path_len={} sf={}", x, y, path_len, pw.scale_factor);
                                eprintln!("[debug-click] handled={} pos=({:.0},{:.0})", click_handled, x, y);
                            }
                        }
                        debug::DebugEvent::Key { key } => {
                            if key == "Tab" {
                                if let Some(r) = pw.composer.layout_root_mut() {
                                    focus_next(r);
                                    pw.focused_id = crate::layout::node::get_focus_id(r);
                                    pw.focused_slot_key = pw.focused_id.and_then(|id| crate::layout::node::find_node_by_id(r, id).map(|n| n.slot_key));
                                    handled = true;
                                }
                            }
                        }
                        debug::DebugEvent::FocusNext => {
                            if let Some(r) = pw.composer.layout_root_mut() { focus_next(r); pw.focused_id = crate::layout::node::get_focus_id(r); pw.focused_slot_key = pw.focused_id.and_then(|id| crate::layout::node::find_node_by_id(r, id).map(|n| n.slot_key)); handled = true; }
                        }
                        debug::DebugEvent::RequestFocus { id } => {
                            if let Some(r) = pw.composer.layout_root_mut() {
                                if crate::layout::node::focus_by_id(r, id) {
                                    pw.focused_id = crate::layout::node::get_focus_id(r);
                                    pw.focused_slot_key = pw.focused_id.and_then(|fid| crate::layout::node::find_node_by_id(r, fid).map(|n| n.slot_key));
                                    handled = true;
                                }
                            }
                        }
                        debug::DebugEvent::Scroll { dy, .. } => {
                            if let Some(root) = pw.composer.layout_root_mut() { apply_scroll_delta(root, dy); handled = true; }
                        }
                        debug::DebugEvent::Resize { w, h } => { pw.width = w; pw.height = h; handled = true; }
                        _ => {}
                    }
                }
                if handled { if let Some(ref sw) = pw.skia_window { sw.request_redraw(); } }
                if debug::has_pending() { if let Some(ref sw) = pw.skia_window { sw.request_redraw(); } }
            }
            _ => {}
        }
    }
}

impl AppState {
    /// 消费 `app::open_window` 排队的窗口请求 + 首次初始化
    fn process_pending_windows(&mut self, event_loop: &dyn ActiveEventLoop) {
        // 首次：运行 init 回调收集主窗口创建请求
        if self.windows.is_empty() {
            if let Some(init) = self.init.take() {
                let mut composer = Composer::new();
                composer.compose(|ctx| init(ctx));
                // 临时 composer 被 drop，其 on_remove 可能设置 PENDING_REMOVE_ID
                // 清除副作用，防止主窗口被错误关闭
                crate::ui::window::reset_lifecycle_flags();
            }
        }

        for item in take_pending_windows() {
            self.pending_content.push(item);
        }

        // 消费 pending：创建窗口
        while self.pending_content.len() > 0 {
            // drain-like: pop from front
            let pending = self.pending_content.remove(0);
            self.open_window(event_loop, pending);
        }
    }

    fn open_window(&mut self, event_loop: &dyn ActiveEventLoop, pending: PendingWindow) {
        let mut a = winit::window::WindowAttributes::default();
        a.title = if pending.title.is_empty() { "Winia".into() } else { pending.title.clone() };
        a.surface_size = Some(winit::dpi::Size::Logical(winit::dpi::LogicalSize::new(pending.width as f64, pending.height as f64)));
        a.visible = false;
        let w = Arc::new(event_loop.create_window(a).expect("window"));
        let window_id = w.id();
        let sf = w.scale_factor();
        let skia_window = VulkanSkiaWindow::new(event_loop, w);
        let content = pending.content.unwrap_or_else(|| Box::new(|_| {}));
        let theme = pending.theme.unwrap_or_else(|| crate::ui::theme::ThemeColors::default_light());
        let mut pw = PerWindow::new(content, pending.width, pending.height, theme);
        pw.on_close = pending.on_close;
        pw.created_id = pending.created_id;
        pw.scale_factor = sf;
        pw.skia_window = Some(skia_window);
        pw.composer.compose(|ctx| (pw.content)(ctx));
        pw.composer.layout(Constraints::new(0.0, pending.width, 0.0, pending.height));
        let bg = pw.theme.background;
        if let Some(ref mut sw) = pw.skia_window {
            if let Some(root) = pw.composer.layout_root() {
                let sf2 = sf as f32;
                sw.draw(|surface| {
                    let canvas = surface.canvas(); canvas.clear(skia_safe::Color::from_argb(bg.a, bg.r, bg.g, bg.b));
                    canvas.save(); canvas.scale((sf2, sf2)); render::render(root, canvas); canvas.restore();
                });
                sw.set_visible(true);
            }
        }
        self.windows.insert(window_id, pw);
        if self.parent_window_id.is_none() {
            self.parent_window_id = Some(window_id);
        }
    }
}

// ── 公共 API ──

// ── 公共 API ──

static GLOBAL_PENDING: std::sync::Mutex<Vec<PendingWindow>> = std::sync::Mutex::new(Vec::new());
static APP_PROXY: Mutex<Option<winit::event_loop::EventLoopProxy>> = Mutex::new(None);

pub fn open_window(width: f32, height: f32, content: Option<Box<dyn Fn(&mut ComposeCtx) + Send>>) {
    open_window_with_title(width, height, String::new(), content, None, None, None);
}

pub fn open_window_with_title(width: f32, height: f32, title: String, content: Option<Box<dyn Fn(&mut ComposeCtx) + Send>>, on_close: Option<Box<dyn FnMut() + Send>>, created_id: Option<u64>, theme: Option<crate::ui::theme::ThemeColors>) {
    GLOBAL_PENDING.lock().unwrap().push(PendingWindow { width, height, title, content, on_close, created_id, theme });
    wake_impl();
}

pub fn open_window_with_close(width: f32, height: f32, content: Option<Box<dyn Fn(&mut ComposeCtx) + Send>>, on_close: Option<Box<dyn FnMut() + Send>>, _created_id: Option<u64>) {
    open_window_with_title(width, height, String::new(), content, on_close, _created_id, None);
}

/// 通过声明式 id 请求关闭窗口
pub fn close_window_by_id(created_id: u64) {
    CLOSE_QUEUED.lock().unwrap().push(created_id);
    wake_impl();
}

/// 取消关闭请求（Window::build 重建窗口时调用，抵消旧节点 on_remove 的推送）
pub fn cancel_close(created_id: u64) {
    CLOSE_QUEUED.lock().unwrap().retain(|&x| x != created_id);
}

static CLOSE_QUEUED: std::sync::Mutex<Vec<u64>> = std::sync::Mutex::new(Vec::new());

pub(crate) fn wake_impl() {
    if let Some(ref proxy) = *APP_PROXY.lock().unwrap() {
        let _ = proxy.wake_up();
        return;
    }
    debug::wake();
}

pub(crate) fn take_pending_windows() -> Vec<PendingWindow> {
    std::mem::take(&mut *GLOBAL_PENDING.lock().unwrap())
}

fn apply_scroll_delta(node: &mut LayoutNode, dy: f32) -> bool {
    if let Some(state) = node.modifier.vertical_scroll_state() {
        let current = state.get();
        // 滚动极限 = 内容总高度 - 可视区域高度
        // viewport 高度优先用 scroll_viewport_height（fill_max_height 场景），
        // 降级到 fixed_size()（固定高度场景），再降级到 0（无限制）。
        let visible_h = if node.scroll_viewport_height > 0.0 {
            node.scroll_viewport_height
        } else {
            node.modifier.fixed_size()
                .and_then(|(_, h)| match h {
                    Dimension::Fixed(h) => Some(h),
                    _ => None,
                })
                .unwrap_or(0.0)
        };
        let max_offset = (node.measured_size.height - visible_h).max(0.0);
        let new = (current - dy).clamp(0.0, max_offset);
        state.set(new);
        return true;
    }
    for child in &mut node.children {
        if apply_scroll_delta(child, dy) { return true; }
    }
    false
}

/// 分发指针事件到 hit_test 路径（pre: outer→inner, bubble: inner→outer）
/// 计算节点在布局树中的绝对位置（从根累加 position）
fn node_abs_position(root: &LayoutNode, id: u64) -> (f32, f32) {
    fn walk(node: &LayoutNode, target: u64, abs_x: f32, abs_y: f32) -> Option<(f32, f32)> {
        let nx = abs_x + node.position.x;
        let ny = abs_y + node.position.y;
        if node.id == target { return Some((nx, ny)); }
        for child in &node.children {
            if let Some(r) = walk(child, target, nx, ny) { return Some(r); }
        }
        None
    }
    walk(root, id, 0.0, 0.0).unwrap_or((0.0, 0.0))
}

fn dispatch_ptr_event(
    root: &LayoutNode,
    path: &[&LayoutNode],
    event: &crate::modifier::PointerEvent,
    scene_pos: (f32, f32),
    captured_id: Option<u64>,
) -> bool {
    if captured_id.is_some() {
    }
    // 如果指针被一个节点捕获（Down 后未释放），用 root 查找节点并构建祖先链
    let captured_path: Vec<&LayoutNode> = if let Some(cid) = captured_id {
        let mut ancestors: Vec<&LayoutNode> = Vec::new();
        if let Some(node) = crate::layout::node::find_node_id_by_slot_key(root, cid).and_then(|nid| crate::layout::node::find_node_by_id(root, nid)) {
            ancestors.push(node);
            let mut pid = node.parent_id;
            while let Some(id) = pid {
                if let Some(anc) = crate::layout::node::find_node_by_id(root, id) {
                    ancestors.push(anc);
                    pid = anc.parent_id;
                } else { break; }
            }
            ancestors.reverse(); // root → ... → captured
            ancestors
        } else {
            path.to_vec()
        }
    } else { path.to_vec() };
    let use_path = &captured_path;
    // 计算路径累积偏移（每个节点的 position 是相对于父节点的偏移）
    let mut abs_x = 0.0f32;
    let mut abs_y = 0.0f32;
    let abs_positions: Vec<(f32, f32)> = use_path.iter().map(|n| {
        abs_x += n.position.x;
        abs_y += n.position.y;
        (abs_x, abs_y)
    }).collect();

    // on_pre_ptr: outer → inner
    for (i, node) in use_path.iter().enumerate() {
        let local_x = scene_pos.0 - abs_positions[i].0;
        let local_y = scene_pos.1 - abs_positions[i].1;
        let mut ev = event.clone();
        ev.position = (local_x, local_y);
        for el in node.modifier.elements() {
            if let crate::modifier::ModifierElement::PointerEvent { on_pre_ptr: Some(handler), .. } = el {
            }
        }
    }

    // on_ptr: inner → outer
    for (i, node) in use_path.iter().enumerate().rev() {
        let local_x = scene_pos.0 - abs_positions[i].0;
        let local_y = scene_pos.1 - abs_positions[i].1;
        let mut ev = event.clone();
        ev.position = (local_x, local_y);
        for el in node.modifier.elements() {
            if let crate::modifier::ModifierElement::PointerEvent { on_ptr: Some(handler), .. } = el {
            }
        }
    }
    false
}

pub fn run_app(app: impl FnOnce(&mut ComposeCtx) + 'static) {
    let event_loop = EventLoop::new().expect("event loop");
    let proxy = event_loop.create_proxy();
    debug::set_event_loop_proxy(proxy.clone());
    let proxy2 = proxy.clone();
    *APP_PROXY.lock().unwrap() = Some(proxy);
    crate::core::state::set_wake_fn(move || { let _ = proxy2.wake_up(); });
    debug::start_stdin_channel();
    debug::start_ws_server();
    let state = AppState {
        init: Some(Box::new(app)),
        windows: HashMap::new(),
        pending_content: Vec::new(),
        parent_window_id: None,
        modifiers: Default::default(),
    };
    event_loop.run_app(state).expect("run_app");
    debug::force_shutdown();
}
