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
}

/// Compose 风格的 click 检测中间状态
struct PtrDownState {
    node_id: u64,
    position: (f32, f32),
    time: Instant,
}

impl PerWindow {
    fn new(content: Box<dyn Fn(&mut ComposeCtx)>, width: f32, height: f32, theme: crate::ui::theme::ThemeColors) -> Self {
        PerWindow { composer: Composer::new(), skia_window: None, width, height, scale_factor: 1.0, focused_id: None, content, on_close: None, created_id: None, theme, focused_slot_key: None, pointer_down_state: None }
    }
    pub(crate) fn created_id(&self) -> Option<u64> { self.created_id }

    /// 清除焦点 + 更新缓存
    fn clear_focus(&mut self, root: &mut LayoutNode) {
        crate::layout::node::clear_focus(root);
        self.focused_id = None;
        self.focused_slot_key = None;
    }

    /// 从当前焦点节点刷新 cached 字段
    fn refresh_focus(&mut self, root: &LayoutNode) {
        self.focused_id = crate::layout::node::get_focus_id(root);
        self.focused_slot_key = self.focused_id
            .and_then(|id| crate::layout::node::find_node_by_id(root, id).map(|n| n.slot_key));
    }

    /// 增量重组 → 恢复焦点 → 布局 → 渲染（供 RedrawRequested 使用）
    /// 循环消费 notify 队列直到稳定，避免 tokio task 的并发通知丢失。
    fn recompose_layout_render(&mut self, after_draw: impl FnOnce(&LayoutNode, &mut skia_safe::Surface)) {
        // 循环 compose 直到没有新的 pending state——处理并发 task 在 compose 期间
        // 完成的 case（第二个 notify 的 state 在第一次 compose 之后才入队）
        loop {
            let did_compose = self.composer.recompose(|ctx| (self.content)(ctx));
            if let Some(slot_key) = self.focused_slot_key {
                if let Some(r) = self.composer.layout_root_mut() {
                    if let Some(new_id) = crate::layout::node::find_node_id_by_slot_key(r, slot_key) {
                        crate::layout::node::clear_focus(r);
                        crate::layout::node::set_focus_by_id(r, new_id);
                        self.focused_id = Some(new_id);
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
        event_loop.set_control_flow(ControlFlow::Wait);
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
                    // ── Down：记录按下态 ──
                    if let Some(root) = pw.composer.layout_root() {
                        let path = hit_test(root, scene_pos.0, scene_pos.1);
                        if let Some(innermost) = path.last() {
                            pw.pointer_down_state = Some(PtrDownState {
                                node_id: innermost.id,
                                position: scene_pos,
                                time: Instant::now(),
                            });
                        }
                    }
                } else {
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
                // pointer 事件分发（不受 click 影响）
                if let Some(root) = pw.composer.layout_root() {
                    let path = hit_test(root, scene_pos.0, scene_pos.1);
                    let ptr_ev = crate::modifier::PointerEvent {
                        event_type,
                        position: (0.0, 0.0),
                        scene_position: scene_pos,
                        kind: crate::modifier::PointerButton::from_winit(&button),
                        is_alt_pressed: self.modifiers.alt_key(),
                        is_ctrl_pressed: self.modifiers.control_key(),
                        is_shift_pressed: self.modifiers.shift_key(),
                        is_meta_pressed: self.modifiers.meta_key(),
                    };
                    dispatch_ptr_event(root, &path, &ptr_ev, scene_pos);
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
                    let ptr_ev = crate::modifier::PointerEvent {
                        event_type: crate::modifier::PointerEventType::Move,
                        position: (0.0, 0.0),
                        scene_position: scene_pos,
                        kind: crate::modifier::PointerKind::Mouse { button: crate::modifier::PointerButton::Primary },
                        is_alt_pressed: self.modifiers.alt_key(),
                        is_ctrl_pressed: self.modifiers.control_key(),
                        is_shift_pressed: self.modifiers.shift_key(),
                        is_meta_pressed: self.modifiers.meta_key(),
                    };
                    dispatch_ptr_event(root, &path, &ptr_ev, scene_pos);
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
                            if let Some(node) = crate::layout::node::find_node_by_id(root, fid) {
                                // on_pre_key 优先（对齐 Compose onPreviewKeyEvent）
                                for el in node.modifier.elements() {
                                    if let crate::modifier::ModifierElement::KbEvent { on_pre_key: Some(handler), .. } = el {
                                        if handler(&ke) { consumed = true; break; }
                                    }
                                }
                                if !consumed {
                                    // on_key（Compose onKeyEvent: inner→outer 冒泡）
                                    for el in node.modifier.elements().iter().rev() {
                                        if let crate::modifier::ModifierElement::KbEvent { on_key: Some(handler), .. } = el {
                                            if handler(&ke) { consumed = true; break; }
                                        }
                                    }
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
                // 消费焦点请求（在 compose 前处理，避免丢失）
                for id in crate::modifier::take_focus_requests() {
                    if let Some(root) = pw.composer.layout_root_mut() {
                        if crate::layout::node::focus_by_id(root, id) {
                            pw.focused_id = Some(id);
                        }
                    }
                    // 单独查 slot_key（避免与 root 的 borrow 冲突）
                    if let Some(root) = pw.composer.layout_root() {
                        if let Some(found) = crate::layout::node::find_node_by_id(root, id) {
                            pw.focused_slot_key = Some(found.slot_key);
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
                                eprintln!("[debug-click] pos=({:.0},{:.0}) path_len={} sf={}", x, y, nodes.len(), pw.scale_factor);
                                for node in nodes.iter().rev() {
                                    if click_handled { break; }
                                    if let Some(on_click) = node.modifier.on_click() {
                                        on_click();
                                        handled = true;
                                        click_handled = true;
                                    }
                                }
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
                if crate::animation::tick() { if let Some(ref sw) = pw.skia_window { sw.request_redraw(); } }
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
fn dispatch_ptr_event(
    _root: &LayoutNode,
    path: &[&LayoutNode],
    event: &crate::modifier::PointerEvent,
    scene_pos: (f32, f32),
) -> bool {
    // 计算路径累积偏移（每个节点的 position 是相对于父节点的偏移）
    let mut abs_x = 0.0f32;
    let mut abs_y = 0.0f32;
    let abs_positions: Vec<(f32, f32)> = path.iter().map(|n| {
        abs_x += n.position.x;
        abs_y += n.position.y;
        (abs_x, abs_y)
    }).collect();

    // on_pre_ptr: outer → inner
    for (i, node) in path.iter().enumerate() {
        let local_x = scene_pos.0 - abs_positions[i].0;
        let local_y = scene_pos.1 - abs_positions[i].1;
        let mut ev = event.clone();
        ev.position = (local_x, local_y);
        for el in node.modifier.elements() {
            if let crate::modifier::ModifierElement::PointerEvent { on_pre_ptr: Some(handler), .. } = el {
                if handler(&ev) { return true; }
            }
        }
    }

    // on_ptr: inner → outer
    for (i, node) in path.iter().enumerate().rev() {
        let local_x = scene_pos.0 - abs_positions[i].0;
        let local_y = scene_pos.1 - abs_positions[i].1;
        let mut ev = event.clone();
        ev.position = (local_x, local_y);
        for el in node.modifier.elements() {
            if let crate::modifier::ModifierElement::PointerEvent { on_ptr: Some(handler), .. } = el {
                if handler(&ev) { return true; }
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
