/*use std::ops::Deref;
use std::rc::{Rc, Weak};
use std::sync::{Arc, LockResult, Mutex, MutexGuard, RwLockReadGuard, RwLockWriteGuard};
use skia_safe::gpu::SyncCpu::No;

use winit::event_loop::{EventLoopClosed, EventLoopProxy};
use winit::window::Window;

use crate::app::AppProxy;
use crate::app::Theme;
use crate::property::BoolProperty;
use crate::uib::{Item, LayoutDirection, Pointer};

#[derive(Clone, Debug)]
pub(crate) enum UserEvent {
    Empty,
    StartAnimation,
    TimerExpired(usize, String),
}

pub(crate) struct App {
    pub theme: Theme,
    pub request_close: bool,
    pub need_redraw: bool,
    pub need_layout: bool,
    pub need_rebuild: bool,
    pub layout_direction: LayoutDirection,
    pub focused_item_property: Option<BoolProperty>
}

impl App {
    pub fn new(theme: Theme) -> Self {
        Self {
            theme,
            request_close: false,
            need_redraw: false,
            need_layout: false,
            need_rebuild: false,
            layout_direction: LayoutDirection::LeftToRight,
            focused_item_property: None,
        }
    }

    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
    }

    pub fn request_rebuild(&mut self) {
        self.need_rebuild = true;
    }
    
    pub fn layout_direction(&self) -> LayoutDirection {
        self.layout_direction
    }

    pub fn set_layout_direction(&mut self, layout_direction: LayoutDirection) {
        self.layout_direction = layout_direction;
    }
}

pub struct SharedApp {
    pub(crate) app: Rc<Mutex<App>>,
    window: Option<Arc<Mutex<Window>>>,
    pub(crate) event_loop_proxy: EventLoopProxy<UserEvent>,
    pub(crate) ui: Option<Weak<Mutex<Item>>>,
}

impl SharedApp {
    pub(crate) fn new(event_loop_proxy: EventLoopProxy<UserEvent>, theme: Theme) -> Self {
        Self {
            app: Rc::new(Mutex::new(App::new(theme))),
            window: None,
            event_loop_proxy,
            ui: None,
        }
    }


    
    pub fn app(&self) -> Rc<Mutex<App>> {
        self.app.clone()
    }
}

impl Clone for SharedApp {
    fn clone(&self) -> Self {
        Self {
            app: self.app.clone(),
            window: self.window.clone(),
            event_loop_proxy: self.event_loop_proxy.clone(),
            ui: self.ui.clone(),
        }
    }
}

impl SharedApp {
    pub fn set_theme(&self, theme: Theme) {
        self.app.lock().unwrap().set_theme(theme);
    }

    pub(crate) fn set_window(&mut self, window: Window) {
        self.window = Some(Arc::new(Mutex::new(window)));
    }
    
    pub fn window(&self) -> LockResult<MutexGuard<Window>> {
        self.window.as_ref().expect("window is not set").lock()
    }

    pub(crate) fn send_event(&mut self, event: UserEvent)  -> Result<(), EventLoopClosed<UserEvent>> {
        self.event_loop_proxy.send_event(event)
    }

    pub fn request_redraw(&self) {
        if !self.app.lock().unwrap().need_redraw {
            self.app.lock().unwrap().need_redraw = true;
            self.window().unwrap().request_redraw();
        }
    }

    pub fn request_layout(&self) {
        self.app.lock().unwrap().need_layout = true;
        self.request_redraw();
    }

    pub fn request_rebuild(&self) {
        self.app.lock().unwrap().request_rebuild();
        self.request_layout();
    }

    pub fn activate_ime(&mut self) {
        self.window().unwrap().set_ime_allowed(true);
    }

    pub fn deactivate_ime(&mut self) {
        self.window().unwrap().set_ime_allowed(false);
    }

    pub(crate) fn redraw_done(&mut self) {
        self.app.lock().unwrap().need_redraw = false;
    }

    pub fn need_layout(&self) -> bool {
        self.app.lock().unwrap().need_layout
    }
    
    pub(crate) fn layout_done(&mut self) {
        self.app.lock().unwrap().need_layout = false;
    }

    pub fn need_rebuild(&self) -> bool {
        self.app.lock().unwrap().need_rebuild
    }
    
    pub(crate) fn rebuild_done(&mut self) {
        self.app.lock().unwrap().need_rebuild = false;
    }

    pub fn focus(&mut self, item: &mut Item) {
        if let Some(focused_item_property) = self.app.lock().unwrap().focused_item_property.as_mut() {
            focused_item_property.set_value(false);
        }
        let mut focused_item_property = item.get_focused();
        focused_item_property.set_value(true);
        self.app.lock().unwrap().focused_item_property = Some(focused_item_property);
    }

    pub fn content_width(&self) -> f32 {
        let inner_width=if let Some(window) = self.window.as_ref() {
            window.lock().expect("window is not set").inner_size().width as f32
        } else {
            0.0
        };
        inner_width / self.scale_factor()
    }

    pub fn content_height(&self) -> f32 {
        let inner_height=if let Some(window) = self.window.as_ref() {
            window.lock().expect("window is not set").inner_size().height as f32
        } else {
            0.0
        };
        inner_height / self.scale_factor()
    }

    pub fn scale_factor(&self) -> f32 {
        if let Some(window) = self.window.as_ref() {
            window.lock().expect("window is not set").scale_factor() as f32
        } else {
            1.0
        }
    }

    pub fn layout_direction(&self) -> LayoutDirection {
        self.app.lock().unwrap().layout_direction()
    }

    pub fn set_layout_direction(&mut self, layout_direction: LayoutDirection) {
        self.app.lock().unwrap().set_layout_direction(layout_direction);
    }
    
    pub fn proxy(&self) -> AppProxy {
        AppProxy::new(self.window.as_ref().unwrap(), &self.event_loop_proxy)
    }
    
    pub fn request_close(&self) {
        self.app.lock().unwrap().request_close = true;
    }
    
    pub fn need_close(&self) -> bool {
        self.app.lock().unwrap().request_close
    }
    
}*/