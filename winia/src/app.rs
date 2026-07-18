//! 应用壳 — run_app + 窗口管理 + 事件循环

use crate::core::composer::{ComposeCtx, Composer};
use crate::debug;
use crate::layout::constraints::Constraints;
use crate::layout::node::{hit_test, focus_next};
use crate::modifier::ModifierElement;
use crate::render;
use skiwin::{SkiaWindowTrait, vulkan::VulkanSkiaWindow};
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::{StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::WindowId;

struct AppState<F> where F: Fn(&mut ComposeCtx) {
    content: F,
    composer: Composer,
    skia_window: Option<VulkanSkiaWindow>,
    width: f32, height: f32,
    scale_factor: f64,
    /// 当前焦点节点 ID（通过 FocusRequester），跨 compose 保持
    focused_id: Option<u64>,
}

impl<F> ApplicationHandler for AppState<F> where F: Fn(&mut ComposeCtx) {
    fn new_events(&mut self, event_loop: &dyn ActiveEventLoop, _cause: StartCause) {
        event_loop.set_control_flow(if debug::has_pending() { ControlFlow::Poll } else { ControlFlow::Wait });
    }
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.skia_window.is_none() {
            let mut a = winit::window::WindowAttributes::default();
            a.title = "Winia".into();
            a.surface_size = Some(winit::dpi::Size::Logical(winit::dpi::LogicalSize::new(self.width as f64, self.height as f64)));
            a.visible = false;
            let w = Arc::new(event_loop.create_window(a).expect("window"));
            self.scale_factor = w.scale_factor();
            self.skia_window = Some(VulkanSkiaWindow::new(event_loop, w));
            // 立刻 compose+layout+render，然后显示窗口
            self.composer.compose(|ctx| (self.content)(ctx));
            self.composer.layout(Constraints::new(0.0, self.width, 0.0, self.height));
            if let Some(ref mut sw) = self.skia_window {
                if let Some(root) = self.composer.layout_root() {
                    let sf = self.scale_factor as f32;
                    sw.draw(|surface| {
                        let canvas = surface.canvas(); canvas.clear(skia_safe::Color::WHITE);
                        canvas.save(); canvas.scale((sf, sf)); render::render(root, canvas); canvas.restore();
                    });
                }
                sw.set_visible(true);
            }
        } else if let Some(ref mut sw) = self.skia_window { sw.destroy_surface(); sw.recreate_surface(); }
    }
    fn resumed(&mut self, _: &dyn ActiveEventLoop) {}
    fn proxy_wake_up(&mut self, _: &dyn ActiveEventLoop) { if let Some(ref sw) = self.skia_window { sw.request_redraw(); } }

    fn window_event(&mut self, event_loop: &dyn ActiveEventLoop, _wid: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => { self.scale_factor = scale_factor; if let Some(ref sw) = self.skia_window { sw.request_redraw(); } }
            WindowEvent::PointerButton { position, state, .. } if state.is_pressed() => {
                let lp = position.to_logical::<f32>(self.scale_factor);
                if let Some(root) = self.composer.layout_root() {
                    for node in hit_test(root, lp.x, lp.y).iter().rev() {
                        for el in node.modifier.elements() {
                            if let ModifierElement::Clickable { on_click } = el { on_click(); }
                        }
                    }
                }
                if let Some(ref sw) = self.skia_window { sw.request_redraw(); }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state.is_pressed() => {
                if matches!(&event.logical_key, Key::Named(NamedKey::Tab)) {
                    if let Some(root) = self.composer.layout_root_mut() {
                        focus_next(root);
                        // 找到当前 Focusable 节点的 FocusRequesterId 并持久化
                        self.focused_id = crate::layout::node::get_focus_id(root);
                    }
                    if let Some(ref sw) = self.skia_window { sw.request_redraw(); }
                }
            }
            WindowEvent::SurfaceResized(s) => {
                let l = s.to_logical::<f32>(self.scale_factor);
                println!("[app] SurfaceResized logical=({:.0},{:.0}) current=({:.0},{:.0})", l.width, l.height, self.width, self.height);
                // winit bug #2094：启动时先发超大尺寸再发正确尺寸
                // 忽略与设定值差距过大的中间状态
                if (l.width - self.width).abs() > self.width * 0.5
                    || (l.height - self.height).abs() > self.height * 0.5
                {
                    println!("[app]   → 中间态，跳过");
                } else {
                    self.width = l.width;
                    self.height = l.height;
                    println!("[app]   → 采用");
                }
                if let Some(ref mut sw) = self.skia_window { sw.resize(); }
            }
            WindowEvent::RedrawRequested => {
                // 原生模拟点击
                if let Some((cx, cy)) = debug::take_native_click() {
                    // /click 使用逻辑坐标，直接 hit_test
                    if let Some(root) = self.composer.layout_root() {
                        for node in hit_test(root, cx, cy).iter().rev() {
                            for el in node.modifier.elements() {
                                if let ModifierElement::Clickable { on_click } = el { on_click(); }
                            }
                        }
                    }
                }
                // compose → layout → render
                self.composer.compose(|ctx| (self.content)(ctx));
                // compose 后恢复持久焦点
                if let Some(fid) = self.focused_id {
                    if let Some(r) = self.composer.layout_root_mut() {
                        crate::layout::node::focus_by_id(r, fid);
                    }
                }
                self.composer.layout(Constraints::new(0.0, self.width, 0.0, self.height));
                if let Some(ref mut sw) = self.skia_window {
                    if let Some(root) = self.composer.layout_root() {
                        let sf = self.scale_factor as f32;
                        sw.draw(|surface| {
                            let canvas = surface.canvas(); canvas.clear(skia_safe::Color::WHITE);
                            canvas.save(); canvas.scale((sf, sf)); render::render(root, canvas); canvas.restore();
                            debug::update_tree(&debug::build_tree_json(root));
                            if debug::screenshot_requested() {
                                let (pw, ph) = ((self.width*sf) as i32, (self.height*sf) as i32);
                                let info = skia_safe::ImageInfo::new((pw,ph), skia_safe::ColorType::RGBA8888, skia_safe::AlphaType::Premul, None);
                                let mut pixels = vec![0u8; (pw*ph*4) as usize];
                                if surface.read_pixels(&info, &mut pixels, pw as usize * 4, (0,0)) { debug::update_pixels(&pixels, pw as u32, ph as u32); }
                                debug::screenshot_done();
                            }
                        });
                    }
                }
                // 渲染后处理 DevTools 事件——此时树是最新的
                let mut handled = false;
                for evt in debug::take_queued_events() {
                    match evt {
                        debug::DebugEvent::Click { x, y } => {
                            let sf = self.scale_factor as f32;
                            if let Some(root) = self.composer.layout_root() {
                                for node in hit_test(root, x / sf, y / sf).iter().rev() {
                                    for el in node.modifier.elements() {
                                        if let ModifierElement::Clickable { on_click } = el { on_click(); handled = true; }
                                    }
                                }
                            }
                        }
                        debug::DebugEvent::Key { key } => { if key == "Tab" { if let Some(r) = self.composer.layout_root_mut() { focus_next(r); handled = true; self.focused_id = None; } } }
                        debug::DebugEvent::FocusNext => { if let Some(r) = self.composer.layout_root_mut() { focus_next(r); handled = true; self.focused_id = None; } }
                        debug::DebugEvent::RequestFocus { id } => {
                            if let Some(r) = self.composer.layout_root_mut() {
                                if crate::layout::node::focus_by_id(r, id) {
                                    self.focused_id = Some(id);
                                    handled = true;
                                }
                            }
                        }
                        debug::DebugEvent::Resize { w, h } => { self.width = w; self.height = h; handled = true; }
                        _ => {}
                    }
                }
                // 事件导致状态变化 → 再请求一帧显示新状态
                if handled { if let Some(ref sw) = self.skia_window { sw.request_redraw(); } }
                // 动画 + pending
                if crate::animation::tick() { if let Some(ref sw) = self.skia_window { sw.request_redraw(); } }
                if debug::has_pending() { if let Some(ref sw) = self.skia_window { sw.request_redraw(); } }
            }
            _ => {}
        }
    }
}

pub fn run_app(content: impl Fn(&mut ComposeCtx) + 'static, width: f32, height: f32) {
    let el = EventLoop::new().expect("event loop");
    let p = el.create_proxy();
    debug::set_wake_callback(move || { let _ = p.wake_up(); });
    debug::start_server();
    let state: &'static mut AppState<_> = Box::leak(Box::new(AppState { content, composer: Composer::new(), skia_window: None, width, height, scale_factor: 1.0, focused_id: None }));
    el.run_app(state).expect("run_app");
}
