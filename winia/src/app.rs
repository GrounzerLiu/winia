//! 应用壳 — run_app + 窗口管理 + 事件循环（多窗口）

use crate::core::composer::{ComposeCtx, Composer};
use crate::debug;
use crate::layout::constraints::Constraints;
use crate::layout::node::{hit_test, focus_next, LayoutNode};
use crate::modifier::ModifierElement;
use crate::render;
use skiwin::{SkiaWindowTrait, vulkan::VulkanSkiaWindow};
use std::collections::HashMap;
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::{StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::WindowId;

// ── PerWindow ──

struct PerWindow {
    composer: Composer,
    skia_window: Option<VulkanSkiaWindow>,
    width: f32, height: f32,
    scale_factor: f64,
    focused_id: Option<u64>,
    content: Box<dyn Fn(&mut ComposeCtx)>,
}

impl PerWindow {
    fn new(content: Box<dyn Fn(&mut ComposeCtx)>, width: f32, height: f32) -> Self {
        PerWindow { composer: Composer::new(), skia_window: None, width, height, scale_factor: 1.0, focused_id: None, content }
    }
}

// ── AppState ──

struct AppState<F> where F: Fn(&mut ComposeCtx) {
    /// 根 composable（每个窗口独立执行）
    content: F,
    /// 已创建的窗口
    windows: HashMap<WindowId, PerWindow>,
    /// 待创建的窗口（由 Window composable 排队）
    pending_content: Vec<(f32, f32, Option<Box<dyn Fn(&mut ComposeCtx) + Send>>)>,
}

impl<F> ApplicationHandler for AppState<F> where F: Fn(&mut ComposeCtx) + Sync {
    fn new_events(&mut self, event_loop: &dyn ActiveEventLoop, _cause: StartCause) {
        event_loop.set_control_flow(if debug::has_pending() { ControlFlow::Poll } else { ControlFlow::Wait });
    }

    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.windows.is_empty() {
            // 初始窗口，直接传 None，让 open_window 使用 PerWindow.content
            self.open_window(event_loop, 400.0, 300.0, None);
        }
        for (w, h, c_opt) in take_pending_windows() {
            self.pending_content.push((w, h, c_opt));
        }
        while let Some((w, h, c_opt)) = self.pending_content.pop() {
            self.open_window(event_loop, w, h, c_opt);
        }
    }

    fn resumed(&mut self, _event_loop: &dyn ActiveEventLoop) {}

    fn proxy_wake_up(&mut self, _event_loop: &dyn ActiveEventLoop) {
        for pw in self.windows.values() {
            if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
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
                self.windows.remove(&window_id);
                // 主动态窗口已全部关闭
                if self.windows.is_empty() { event_loop.exit(); }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                pw.scale_factor = scale_factor;
                if let Some(ref sw) = pw.skia_window { sw.request_redraw(); }
            }
            WindowEvent::PointerButton { position, state, .. } if state.is_pressed() => {
                let lp = position.to_logical::<f32>(pw.scale_factor);
                if let Some(root) = pw.composer.layout_root() {
                    for node in hit_test(root, lp.x, lp.y).iter().rev() {
                        for el in node.modifier.elements() {
                            if let ModifierElement::Clickable { on_click } = el { on_click(); }
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
                // 原生模拟点击
                if let Some((cx, cy)) = debug::take_native_click() {
                    if let Some(root) = pw.composer.layout_root() {
                        for node in hit_test(root, cx, cy).iter().rev() {
                            for el in node.modifier.elements() {
                                if let ModifierElement::Clickable { on_click } = el { on_click(); }
                            }
                        }
                    }
                }
                // compose → layout → render
                pw.composer.compose(|ctx| (pw.content)(ctx));
                if let Some(fid) = pw.focused_id {
                    if let Some(r) = pw.composer.layout_root_mut() { crate::layout::node::focus_by_id(r, fid); }
                }
                pw.composer.layout(Constraints::new(0.0, pw.width, 0.0, pw.height));
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
                // DevTools 事件
                let mut handled = false;
                for evt in debug::take_queued_events() {
                    match evt {
                        debug::DebugEvent::Click { x, y } => {
                            let sf = pw.scale_factor as f32;
                            if let Some(root) = pw.composer.layout_root() {
                                for node in hit_test(root, x / sf, y / sf).iter().rev() {
                                    for el in node.modifier.elements() {
                                        if let ModifierElement::Clickable { on_click } = el { on_click(); handled = true; }
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

impl<F> AppState<F> where F: Fn(&mut ComposeCtx) {
    fn open_window(&mut self, event_loop: &dyn ActiveEventLoop, width: f32, height: f32, content: Option<Box<dyn Fn(&mut ComposeCtx) + Send>>) {
        let mut a = winit::window::WindowAttributes::default();
        a.title = "Winia".into();
        a.surface_size = Some(winit::dpi::Size::Logical(winit::dpi::LogicalSize::new(width as f64, height as f64)));
        a.visible = false;
        let w = Arc::new(event_loop.create_window(a).expect("window"));
        let window_id = w.id();
        let sf = w.scale_factor();
        let skia_window = VulkanSkiaWindow::new(event_loop, w);
        // 首次 compose+render
        let content = content.unwrap_or_else(|| Box::new(|_| {}));
        let mut pw = PerWindow::new(content, width, height);
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
    }
}

// ── 公共 API ──

// ── 公共 API ──

static GLOBAL_PENDING: std::sync::Mutex<Vec<(f32, f32, Option<Box<dyn Fn(&mut ComposeCtx) + Send>>)>> = std::sync::Mutex::new(Vec::new());

pub fn open_window(width: f32, height: f32, content: Option<Box<dyn Fn(&mut ComposeCtx) + Send>>) {
    GLOBAL_PENDING.lock().unwrap().push((width, height, content));
}

pub fn take_pending_windows() -> Vec<(f32, f32, Option<Box<dyn Fn(&mut ComposeCtx) + Send>>)> {
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

pub fn run_app(content: impl Fn(&mut ComposeCtx) + Send + Sync + 'static, width: f32, height: f32) {
    let event_loop = EventLoop::new().expect("event loop");
    let proxy = event_loop.create_proxy();
    debug::set_wake_callback(move || { let _ = proxy.wake_up(); });
    debug::start_server();
    let state: &'static mut AppState<_> = Box::leak(Box::new(AppState {
        content,
        windows: HashMap::new(),
        pending_content: vec![(width, height, None)],
    }));
    event_loop.run_app(state).expect("run_app");
}
