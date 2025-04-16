use crate::dpi::LogicalSize;
use crate::shared::{Gettable, Settable, Shared, SharedAnimationTrait, SharedBool, SharedUnSend, WeakShared, WeakSharedUnSend};
use crate::ui::theme::material_theme;
use crate::ui::{LayoutAnimation, Item, Theme};
use parking_lot::MutexGuard;
use skia_safe::Color;
use skiwin::SkiaWindow;
use std::collections::{BTreeSet, LinkedList};
use std::ops::DerefMut;
use std::time::{Duration, Instant};
use winit::event_loop::EventLoopProxy as WinitEventLoopProxy;
use winit::window::{Window, WindowId};
use proc_macro::AsRef;
use crate::ui::app::WindowAttr;

#[derive(Clone, Debug)]
pub(crate) struct Timer {
    pub id: usize,
    pub start_time: Instant,
    pub duration: Duration,
}

pub struct Event {
    pub window_id: WindowId,
    pub event: EventType,
}

pub enum EventType {
    RequestFocus,
    RequestLayout,
    RequestRedraw,
    StartSharedAnimation(Box<dyn SharedAnimationTrait + Send>),
    StartLayoutAnimation(LayoutAnimation),
    Timer(usize),
    SetWindowAttribute(Box<dyn FnOnce(Option<&Window>) + Send>),
    NewWindow{
        item_generator: Box<dyn FnOnce(&WindowContext, &WindowAttr) -> Item + Send + 'static>,
        window_attr: WindowAttr,
    },
    NewLayer(Box<dyn FnOnce(&WindowContext, &WindowAttr) -> Item + Send + 'static>),
    RemoveLayer(usize),
}

#[derive(Clone, AsRef)]
pub struct EventLoopProxy {
    window_id: WindowId,
    event_loop_proxy: WinitEventLoopProxy<Event>,
}

impl EventLoopProxy {
    pub fn new(window_id: WindowId, event_loop_proxy: WinitEventLoopProxy<Event>) -> Self {
        Self {
            window_id,
            event_loop_proxy,
        }
    }

    fn send_event(&self, event: Event) {
        match self.event_loop_proxy.send_event(event) {
            Ok(()) => {}
            Err(_e) => {
                // panic!("Failed to send user event: {}", e);
            }
        }
    }

    pub fn request_redraw(&self) {
        self.send_event(Event {
            window_id: self.window_id,
            event: EventType::RequestRedraw,
        });
    }

    pub fn request_layout(&self) {
        self.send_event(Event {
            window_id: self.window_id,
            event: EventType::RequestLayout,
        });
    }

    pub fn set_window_attribute(&self, f: impl FnOnce(Option<&Window>) + Send + 'static) {
        self.send_event(Event {
            window_id: self.window_id,
            event: EventType::SetWindowAttribute(Box::new(f)),
        });
    }
    
    pub fn start_layout_animation(&self, animation: LayoutAnimation) {
        self.send_event(Event {
            window_id: self.window_id,
            event: EventType::StartLayoutAnimation(animation),
        });
    }

    pub fn start_shared_animation(&self, animation: Box<dyn SharedAnimationTrait + Send>) {
        self.send_event(Event {
            window_id: self.window_id,
            event: EventType::StartSharedAnimation(animation),
        });
    }
    
    pub fn request_focus(&self) {
        self.send_event(Event {
            window_id: self.window_id,
            event: EventType::RequestFocus,
        });
    }
    
    pub fn new_window(&self, item_generator: impl FnOnce(&WindowContext, &WindowAttr) -> Item + Send + 'static, window_attr: WindowAttr) {
        self.send_event(Event {
            window_id: self.window_id,
            event: EventType::NewWindow{
                item_generator: Box::new(item_generator),
                window_attr,
            },
        });
    }

    pub fn new_layer(&self, item_generator: impl FnOnce(&WindowContext, &WindowAttr) -> Item + Send + 'static) {
        self.send_event(Event {
            window_id: self.window_id,
            event: EventType::NewLayer(Box::new(item_generator))
        });
    }

    pub fn remove_layer(&self, id: usize) {
        self.send_event(Event {
            window_id: self.window_id,
            event: EventType::RemoveLayer(id),
        });
    }
}

#[derive(Clone)]
pub struct WindowContext {
    pub(crate) theme: Shared<Theme>,
    pub(crate) window: SharedUnSend<Box<dyn SkiaWindow>>,
    pub(crate) event_loop_proxy: EventLoopProxy,
    pub(crate) request_layout: Shared<bool>,
    pub(crate) request_redraw: Shared<bool>,
    pub(crate) layout_animations: SharedUnSend<Vec<LayoutAnimation>>,
    pub(crate) shared_animations: SharedUnSend<Vec<Box<dyn SharedAnimationTrait + Send>>>,
    pub(crate) focused_property: Shared<Option<(SharedBool, usize)>>,
    pub(crate) focus_changed_items: Shared<BTreeSet<usize>>,
    pub(crate) timers: Shared<Vec<Timer>>,
    pub(crate) cursor_position: Shared<(f32, f32)>,
    pub(crate) title: Shared<String>,
    pub(crate) min_width: Shared<f32>,
    pub(crate) min_height: Shared<f32>,
    pub(crate) max_width: Shared<f32>,
    pub(crate) max_height: Shared<f32>,
}

impl WindowContext {
    pub(crate) fn new(window: impl SkiaWindow + 'static, event_loop_proxy: winit::event_loop::EventLoopProxy<Event>) -> Self {
        let window_id = window.id();
        Self {
            theme: material_theme(Color::from_rgb(255, 0, 0), dark_light::detect().map_or(false,|mode|{
                mode == dark_light::Mode::Dark
            })).into(),
            window: SharedUnSend::from_static(Box::new(window)),
            event_loop_proxy: EventLoopProxy::new(window_id, event_loop_proxy),
            request_layout: false.into(),
            request_redraw: false.into(),
            layout_animations: Vec::new().into(),
            shared_animations: Vec::new().into(),
            focused_property: None.into(),
            focus_changed_items: BTreeSet::new().into(),
            timers: Vec::new().into(),
            cursor_position: (0.0, 0.0).into(),
            title: "Title".to_string().into(),
            min_width: 0.0.into(),
            min_height: 0.0.into(),
            max_width: f32::MAX.into(),
            max_height: f32::MAX.into(),
        }
    }

    pub(crate) fn window(&self) -> MutexGuard<'_, Box<dyn SkiaWindow>> {
        self.window.lock()
    }

    pub fn window_size(&self) -> (f32, f32) {
        let scale_factor = self.scale_factor();
        let window = self.window();
        let size = window.inner_size();
        (
            size.width as f32 / scale_factor,
            size.height as f32 / scale_factor,
        )
    }

    pub fn window_id(&self) -> WindowId {
        self.window().id()
    }

    pub fn get_cursor_position(&self) -> (f32, f32) {
        self.cursor_position.lock().clone()
    }

    pub fn title(&self) -> &Shared<String> {
        &self.title
    }

    pub fn min_width(&self) -> &Shared<f32> {
        &self.min_width
    }

    pub fn min_height(&self) -> &Shared<f32> {
        &self.min_height
    }

    pub fn max_width(&self) -> &Shared<f32> {
        &self.max_width
    }

    pub fn max_height(&self) -> &Shared<f32> {
        &self.max_height
    }

    pub fn create_timer(&self, id: usize, duration: impl Into<Duration>) {
        let timer = Timer {
            id,
            start_time: Instant::now(),
            duration: duration.into(),
        };
        self.timers.write(|timers| timers.push(timer.clone()));
    }

    // pub fn send_event(&self, event: Event) {
    //     self.event_loop_proxy.send_event(event);
    // }

    pub fn event_loop_proxy(&self) -> &EventLoopProxy {
        &self.event_loop_proxy
    }

    pub fn theme(&self) -> &Shared<Theme> {
        &self.theme
    }
}

impl WindowContext {
    pub fn scale_factor(&self) -> f32 {
        self.window().scale_factor() as f32
        // 1.0
    }

    pub fn request_redraw(&self) {
        if self.request_redraw.get() {
            return;
        }
        self.request_redraw.set(true);
        self.window().request_redraw();
    }

    pub fn request_layout(&self) {
        if self.request_layout.get() {
            return;
        }
        self.request_layout.set(true);
        self.request_redraw();
    }

    // pub fn request_focus(&self, id: usize, focused: bool) {
    //     self.request_focus_item.lock().unwrap().replace((id, focused));
    //     self.request_redraw();
    // }
}

// impl AppContext{
//     pub(crate) fn clone(&self) -> Self {
//         Self {
//             window: self.window.clone(),
//             event_loop_proxy: self.event_loop_proxy.clone(),
//             starting_animations: self.starting_animations.clone(),
//             running_animations: self.running_animations.clone(),
//             title: self.title.clone(),
//             min_width: self.min_width.clone(),
//             min_height: self.min_height.clone(),
//             max_width: self.max_width.clone(),
//             max_height: self.max_height.clone(),
//         }
//     }
// }

// pub struct AppContextWeak {
//     theme: WeakShared<Theme>,
//     window: WeakSharedUnSend<Box<dyn SkiaWindow>>,
//     event_loop_proxy: EventLoopProxy,
//     request_re_layout: WeakShared<bool>,
//     starting_animations: WeakSharedUnSend<LinkedList<Animation>>,
//     running_animations: WeakSharedUnSend<Vec<Animation>>,
//     shared_animations: WeakSharedUnSend<Vec<Box<dyn SharedAnimationTrait + Send>>>,
//     focused_property: WeakShared<Option<(SharedBool, usize)>>,
//     focus_changed_items: WeakShared<BTreeSet<usize>>,
//     timer: WeakShared<Vec<Timer>>,
//     cursor_position: WeakShared<(f32, f32)>,
//     title: WeakShared<String>,
//     min_width: WeakShared<f32>,
//     min_height: WeakShared<f32>,
//     max_width: WeakShared<f32>,
//     max_height: WeakShared<f32>,
// }
// 
// impl WindowContext {
//     pub fn weak_ref(&self) -> AppContextWeak {
//         AppContextWeak {
//             theme: self.theme.weak(),
//             window: self.window.weak(),
//             event_loop_proxy: self.event_loop_proxy.weak(),
//             request_re_layout: self.request_layout.weak(),
//             starting_animations: self.starting_animations.weak(),
//             running_animations: self.running_animations.weak(),
//             shared_animations: self.shared_animations.weak(),
//             focused_property: self.focused_property.weak(),
//             focus_changed_items: self.focus_changed_items.weak(),
//             timer: self.timers.weak(),
//             cursor_position: self.cursor_position.weak(),
//             title: self.title.weak(),
//             min_width: self.min_width.weak(),
//             min_height: self.min_height.weak(),
//             max_width: self.max_width.weak(),
//             max_height: self.max_height.weak(),
//         }
//     }
// }
// 
// impl AppContextWeak {
//     pub fn upgrade(&self) -> Option<WindowContext> {
//         Some(WindowContext {
//             theme: self.theme.upgrade()?,
//             window: self.window.upgrade()?,
//             event_loop_proxy: self.event_loop_proxy.upgrade()?,
//             request_layout: self.request_re_layout.upgrade()?,
//             starting_animations: self.starting_animations.upgrade()?,
//             running_animations: self.running_animations.upgrade()?,
//             shared_animations: self.shared_animations.upgrade()?,
//             focused_property: self.focused_property.upgrade()?,
//             focus_changed_items: self.focus_changed_items.upgrade()?,
//             timers: self.timer.upgrade()?,
//             cursor_position: self.cursor_position.upgrade()?,
//             title: self.title.upgrade()?,
//             min_width: self.min_width.upgrade()?,
//             min_height: self.min_height.upgrade()?,
//             max_width: self.max_width.upgrade()?,
//             max_height: self.max_height.upgrade()?,
//         })
//     }
// }
