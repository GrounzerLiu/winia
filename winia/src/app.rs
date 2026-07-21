//! 应用壳 — run_app + 窗口管理 + 事件循环（多窗口）

use crate::core::composer::{ComposeCtx, Composer};
use crate::debug;
use crate::layout::constraints::Constraints;
use crate::layout::node::{hit_test, focus_next, LayoutNode};
use crate::modifier::ModifierElement;
use crate::render;
pub(crate) type PendingItem = (f32, f32, Option<Box<dyn Fn(&mut ComposeCtx) + Send>>, Option<Box<dyn FnMut() + Send>>, Option<u64>);
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
}

impl PerWindow {
    fn new(content: Box<dyn Fn(&mut ComposeCtx)>, width: f32, height: f32) -> Self {
        PerWindow { composer: Composer::new(), skia_window: None, width, height, scale_factor: 1.0, focused_id: None, content, on_close: None, created_id: None }
    }
    pub(crate) fn created_id(&self) -> Option<u64> { self.created_id }
}

// ── AppState ──

struct AppState<F> where F: Fn(&mut ComposeCtx) + Send + Sync + 'static {
    /// 根 composable（每个窗口独立执行）
    content: F,
    /// 已创建的窗口
    windows: HashMap<WindowId, PerWindow>,
    /// 待创建的窗口（由 Window composable 排队）
    pending_content: Vec<PendingItem>,
    /// 父窗口 ID（用于 is_parent 判断，不依赖 HashMap 顺序）
    parent_window_id: Option<WindowId>,
}

impl<F> ApplicationHandler for AppState<F> where F: Fn(&mut ComposeCtx) + Send + Sync + Clone + 'static {
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
                if closing_last {
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
            WindowEvent::PointerButton { position, state, .. } if state.is_pressed() => {
                let lp = position.to_logical::<f32>(pw.scale_factor);
                if let Some(root) = pw.composer.layout_root() {
                    let mut handled = false;
                    for node in hit_test(root, lp.x, lp.y).iter().rev() {
                        if handled { break; }
                        for el in node.modifier.elements() {
                            if let ModifierElement::Clickable { on_click } = el { on_click(); handled = true; break; }
                        }
                    }
                }
                if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state.is_pressed() => {
                if matches!(&event.logical_key, Key::Named(NamedKey::Tab)) {
                    if let Some(root) = pw.composer.layout_root_mut() {
                        focus_next(root);
                        pw.focused_id = crate::layout::node::get_focus_id(root);
                    }
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
            }
            WindowEvent::RedrawRequested => {
                // compose → layout → render
                pw.composer.compose(|ctx| (pw.content)(ctx));
                if let Some(fid) = pw.focused_id {
                    if let Some(r) = pw.composer.layout_root_mut() { crate::layout::node::focus_by_id(r, fid); }
                }
                pw.composer.layout(Constraints::new(0.0, pw.width, 0.0, pw.height));
                // 检查 compose 后是否有待关闭窗口，有则唤醒 proxy 消费
                if crate::ui::window::Window::has_pending_close() { debug::wake(); }
                if let Some(ref mut sw) = pw.skia_window {
                    if let Some(root) = pw.composer.layout_root() {
                        let sf = pw.scale_factor as f32;
                        sw.draw(|surface| {
                            let canvas = surface.canvas(); canvas.clear(skia_safe::Color::WHITE);
                            canvas.save(); canvas.scale((sf, sf)); render::render(root, canvas); canvas.restore();
                            debug::update_tree(&debug::build_tree_json(root));
                            if debug::screenshot_requested() {
                                let (pw2, ph2) = ((pw.width*sf) as i32, (pw.height*sf) as i32);
                                let info = skia_safe::ImageInfo::new((pw2,ph2), skia_safe::ColorType::RGBA8888, skia_safe::AlphaType::Premul, None);
                                let mut pixels = vec![0u8; (pw2*ph2*4) as usize];
                                if surface.read_pixels(&info, &mut pixels, pw2 as usize * 4, (0,0)) { debug::update_pixels(&pixels, pw2 as u32, ph2 as u32); }
                                debug::screenshot_done();
                            }
                        });
                    }
                }
                // DevTools 事件（仅父窗口消费，防止多窗口抢）
                if !is_parent { return; }
                let mut handled = false;
                for evt in debug::take_queued_events() {
                    match evt {
                        debug::DebugEvent::Click { x, y } => {
                            let sf = pw.scale_factor as f32;
                            if let Some(root) = pw.composer.layout_root() {
                                let nodes = hit_test(root, x / sf, y / sf);
                                let mut click_handled = false;
                                for node in nodes.iter().rev() {
                                    if click_handled { break; }
                                    let mod_strs: Vec<String> = node.modifier.elements().iter().map(|el| format!("{:?}", el)).collect();
                                    for el in node.modifier.elements() {
                                        if let ModifierElement::Clickable { on_click } = el { on_click(); handled = true; click_handled = true; break; }
                                    }
                                }
                            }
                        }
                        debug::DebugEvent::Key { key } => {
                            if key == "Tab" {
                                if let Some(r) = pw.composer.layout_root_mut() {
                                    focus_next(r);
                                    pw.focused_id = crate::layout::node::get_focus_id(r);
                                    handled = true;
                                }
                            }
                        }
                        debug::DebugEvent::FocusNext => {
                            if let Some(r) = pw.composer.layout_root_mut() { focus_next(r); pw.focused_id = crate::layout::node::get_focus_id(r); handled = true; }
                        }
                        debug::DebugEvent::RequestFocus { id } => {
                            if let Some(r) = pw.composer.layout_root_mut() {
                                if crate::layout::node::focus_by_id(r, id) { pw.focused_id = Some(id); handled = true; }
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

impl<F> AppState<F> where F: Fn(&mut ComposeCtx) + Send + Sync + Clone {
    /// 消费 `app::open_window` 排队的窗口请求 + 初始窗口创建
    fn process_pending_windows(&mut self, event_loop: &dyn ActiveEventLoop) {
        let initial: Option<PendingItem> = if self.windows.is_empty() {
            Some((400.0, 300.0, Some(Box::new(self.content.clone()) as Box<dyn Fn(&mut ComposeCtx) + Send>), None, None))
        } else { None };

        for item in take_pending_windows() {
            self.pending_content.push(item);
        }

        if let Some(item) = initial { self.pending_content.insert(0, item); }

        while let Some((w, h, c_opt, on_close, created_id)) = self.pending_content.pop() {
            let content = c_opt.unwrap_or_else(|| Box::new(|_| {}));
            self.open_window(event_loop, w, h, content, on_close, created_id);
        }
    }

    fn open_window(&mut self, event_loop: &dyn ActiveEventLoop, width: f32, height: f32, content: Box<dyn Fn(&mut ComposeCtx) + Send>, on_close: Option<Box<dyn FnMut() + Send>>, created_id: Option<u64>) {
        let mut a = winit::window::WindowAttributes::default();
        a.title = "Winia".into();
        a.surface_size = Some(winit::dpi::Size::Logical(winit::dpi::LogicalSize::new(width as f64, height as f64)));
        a.visible = false;
        let w = Arc::new(event_loop.create_window(a).expect("window"));
        let window_id = w.id();
        let sf = w.scale_factor();
        let skia_window = VulkanSkiaWindow::new(event_loop, w);
        // 首次 compose+render
        let mut pw = PerWindow::new(content, width, height);
        pw.on_close = on_close;
        pw.created_id = created_id;
        pw.scale_factor = sf;
        pw.skia_window = Some(skia_window);
        pw.composer.compose(|ctx| (pw.content)(ctx));
        pw.composer.layout(Constraints::new(0.0, width, 0.0, height));
        if let Some(ref mut sw) = pw.skia_window {
            if let Some(root) = pw.composer.layout_root() {
                let sf2 = sf as f32;
                sw.draw(|surface| {
                    let canvas = surface.canvas(); canvas.clear(skia_safe::Color::WHITE);
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

static GLOBAL_PENDING: std::sync::Mutex<Vec<PendingItem>> = std::sync::Mutex::new(Vec::new());
static APP_PROXY: Mutex<Option<winit::event_loop::EventLoopProxy>> = Mutex::new(None);

pub fn open_window(width: f32, height: f32, content: Option<Box<dyn Fn(&mut ComposeCtx) + Send>>) {
    GLOBAL_PENDING.lock().unwrap().push((width, height, content, None, None));
    wake_impl();
}

pub fn open_window_with_close(width: f32, height: f32, content: Option<Box<dyn Fn(&mut ComposeCtx) + Send>>, on_close: Option<Box<dyn FnMut() + Send>>, _created_id: Option<u64>) {
    GLOBAL_PENDING.lock().unwrap().push((width, height, content, on_close, _created_id));
    wake_impl();
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
    // 优先使用 APP_PROXY（独立于 debug-server feature）
    if let Some(ref proxy) = *APP_PROXY.lock().unwrap() {
        let _ = proxy.wake_up();
        return;
    }
    // 回退到 debug::wake()（当 debug-server 启用时）
    debug::wake();
}

pub fn take_pending_windows() -> Vec<PendingItem> {
    std::mem::take(&mut *GLOBAL_PENDING.lock().unwrap())
}
use crate::modifier::Dimension;

fn apply_scroll_delta(node: &mut LayoutNode, dy: f32) -> bool {
    for el in node.modifier.elements() {
        if let ModifierElement::VerticalScroll { state } = el {
            let current = state.get();
            let content_h = node.children.iter().map(|c| c.position.y + c.measured_size.height).fold(0.0, f32::max);
            let visible_h = node.modifier.elements().iter().find_map(|el| match el {
                ModifierElement::Size { height: Dimension::Fixed(h), .. } => Some(*h),
                _ => None,
            }).unwrap_or(0.0);
            let new = (current - dy).clamp(0.0, (content_h - visible_h).max(0.0));
            state.set(new);
            return true;
        }
    }
    for child in &mut node.children {
        if apply_scroll_delta(child, dy) { return true; }
    }
    false
}

pub fn run_app(content: impl Fn(&mut ComposeCtx) + Clone + Send + Sync + 'static, _width: f32, _height: f32) {
    let event_loop = EventLoop::new().expect("event loop");
    let proxy = event_loop.create_proxy();
    debug::set_event_loop_proxy(proxy.clone());
    *APP_PROXY.lock().unwrap() = Some(proxy);
    debug::start_server();
    let state: &'static mut AppState<_> = Box::leak(Box::new(AppState {
        content,
        windows: HashMap::new(),
        pending_content: Vec::new(),
        parent_window_id: None,
    }));
    event_loop.run_app(state).expect("run_app");
    // run_app 返回 = 事件循环已退出
    // 停止 debug server 线程后进程自然退出
    debug::force_shutdown();
}
