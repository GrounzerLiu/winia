use std::clone::UseCloned;
use std::collections::BTreeSet;
use std::ops::{Deref, DerefMut};
use crate::animation::LayoutAnimation;
use crate::app::WindowAttributes;
use crate::shared::{Shared, SharedAnimationTrait, SharedBool, SharedDerived, SharedSource};
use crate::theme::material_theme;
use crate::ui::{Color, Item};
use crate::{depend, Theme};
use proc_macro::AsRef;
use std::sync::Arc;
use std::time::{Duration, Instant};
use clonelet::clone;
use crossbeam_channel::{Receiver, Sender};
use getset::Getters;
use winit::event_loop::EventLoopProxy as WinitEventLoopProxy;
use winit::window::{Window, WindowId};

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

#[derive(Clone)]
pub struct LayerController {
    id: SharedSource<Option<u32>>,
    event_loop_proxy: EventLoopProxy,
}

impl LayerController {
    pub(crate) fn new(event_loop_proxy: EventLoopProxy) -> Self {
        Self {
            id: Shared::from(None),
            event_loop_proxy,
        }
    }

    pub(crate) fn set_id(&self, id: u32) {
        self.id.set(Some(id));
    }

    pub fn remove(&self) {
        let id = self.id.get();
        if let Some(id) = id {
            self.event_loop_proxy.remove_layer(id);
        }
    }
}

pub enum EventType {
    RequestFocus(u32),
    RequestUpdateLayout,
    StartSharedAnimation(Box<dyn SharedAnimationTrait + Send>),
    StartLayoutAnimation(LayoutAnimation),
    Timer(usize),
    SetWindowAttribute(Box<dyn FnOnce(&Box<dyn Window>) + Send>),
    NewWindow {
        item_generator: Box<dyn FnOnce(&WindowContext) -> Item + Send + 'static>,
        window_attributes: WindowAttributes,
    },
    AddLayer(Box<dyn FnOnce(&WindowContext, LayerController) -> Item + Send + 'static>),
    RemoveLayer(u32),
}

#[derive(Clone, AsRef)]
pub struct EventLoopProxy {
    window_id: WindowId,
    event_loop_proxy: WinitEventLoopProxy,
    sender: Sender<Event>
}

impl EventLoopProxy {
    pub fn new(window_id: WindowId, event_loop_proxy: WinitEventLoopProxy, sender: Sender<Event>) -> Self {
        Self {
            window_id,
            event_loop_proxy,
            sender
        }
    }

    fn send_event(&self, event: Event) {
        self.sender.send(event).unwrap();
        self.event_loop_proxy.wake_up()
    }

    pub fn request_update_layout(&self) {
        self.send_event(Event {
            window_id: self.window_id,
            event: EventType::RequestUpdateLayout,
        });
    }

    pub fn set_window_attribute(&self, f: impl FnOnce(&Box<dyn Window>) + Send + 'static) {
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

    pub fn request_focus(&self, item_id: u32) {
        self.send_event(Event {
            window_id: self.window_id,
            event: EventType::RequestFocus(item_id),
        });
    }

    pub fn new_window(
        &self,
        item_generator: impl FnOnce(&WindowContext) -> Item + Send + 'static,
        window_attributes: WindowAttributes,
    ) {
        self.send_event(Event {
            window_id: self.window_id,
            event: EventType::NewWindow {
                item_generator: Box::new(item_generator),
                window_attributes,
            },
        });
    }

    pub fn add_layer(
        &self,
        item_generator: impl FnOnce(&WindowContext, LayerController) -> Item + Send + 'static,
    ) {
        self.send_event(Event {
            window_id: self.window_id,
            event: EventType::AddLayer(Box::new(item_generator)),
        });
    }

    pub fn remove_layer(&self, id: u32) {
        self.send_event(Event {
            window_id: self.window_id,
            event: EventType::RemoveLayer(id),
        });
    }
}

#[derive(Clone, Getters)]
pub struct WindowContext {
    #[get = "pub"]
    window: Arc<Box<dyn Window>>,
    #[get = "pub"]
    window_attributes: WindowAttributes,
    theme: SharedSource<Theme>,
    #[get = "pub"]
    event_loop_proxy: EventLoopProxy,
    #[get = "pub"]
    need_layout: SharedBool,
    #[get = "pub"]
    layout_animations: SharedSource<Vec<LayoutAnimation>>,
    // pub(crate) starting_local_animations: LocalShared<LinkedList<LocalLayoutAnimation>>,
    #[get = "pub(crate)"]
    shared_animations: SharedSource<Vec<Box<dyn SharedAnimationTrait + Send>>>,
    /// ((last focused item, id), (new focused item, id))
    // pub(crate) item_focused: Shared<(Option<(SharedBool, usize)>, Option<(SharedBool, usize)>)>,
    ime_allowed: SharedSource<BTreeSet<u32>>,
    // pub(crate) timers: Shared<Vec<Timer>>,
    #[get = "pub"]
    cursor_position: SharedSource<(f32, f32)>,
    title: SharedDerived<String>,
    min_width: SharedDerived<f32>,
    min_height: SharedDerived<f32>,
    max_width: SharedDerived<f32>,
    max_height: SharedDerived<f32>,
    #[get = "pub"]
    background_color: SharedDerived<Color>,
}

impl WindowContext {
    pub(crate) fn new(
        window: Arc<Box<dyn Window>>,
        window_attributes: &WindowAttributes,
        event_loop_proxy: winit::event_loop::EventLoopProxy,
        sender: Sender<Event>,
    ) -> Self {
        let window_id = window.id();
        let theme = SharedSource::new(
            material_theme(
                Color::RED,
                dark_light::detect().is_ok_and(|mode|{
                    mode == dark_light::Mode::Dark
                })
            )
        );
        let background_color = SharedDerived::from_fn(
            depend!(theme),
            {
                clone!(theme);
                move || {
                    let theme = theme.lock();
                    theme.get_color("background").cloned().unwrap_or(Color::WHITE)
                }
            }
        );
        Self {
            // theme: material_theme(Color::from_rgb(255, 0, 0), dark_light::detect().map_or(false,|mode|{
            //     mode != dark_light::Mode::Dark
            // })).into(),
            window: window.clone(),
            window_attributes: window_attributes.clone(),
            theme,
            event_loop_proxy: EventLoopProxy::new(window_id, event_loop_proxy, sender),
            need_layout: SharedBool::new(false),
            layout_animations: Vec::new().into(),
            // starting_local_animations: LinkedList::new().into(),
            // shared_animations: Vec::new().into(),
            // item_focused: (None, None).into(),
            ime_allowed: BTreeSet::new().into(),
            // timers: Vec::new().into(),
            shared_animations: Vec::new().into(),
            cursor_position: (0.0, 0.0).into(),
            title: "Title".to_string().into(),
            min_width: 0.0.into(),
            min_height: 0.0.into(),
            max_width: f32::MAX.into(),
            max_height: f32::MAX.into(),
            background_color,
        }
    }
    
    pub fn window_size(&self) -> (f32, f32) {
        let scale_factor = self.scale_factor();
        let size = self.window.surface_size();
        (
            size.width as f32 / scale_factor,
            size.height as f32 / scale_factor,
        )
    }

    pub fn window_id(&self) -> WindowId {
        self.window.id()
    }

    pub fn set_ime_allowed(&self, id: u32, allowed: bool) {
        if allowed {
            self.ime_allowed.lock().insert(id);
        } else {
            self.ime_allowed.lock().remove(&id);
        }
        if self.ime_allowed.lock().is_empty() {
            self.window().set_ime_allowed(false);
        } else {
            self.window().set_ime_allowed(true);
        }
    }

    pub fn get_cursor_position(&self) -> (f32, f32) {
        *self.cursor_position.read()
    }

    pub fn title(&self) -> &SharedDerived<String> {
        &self.title
    }

    pub fn min_width(&self) -> &SharedDerived<f32> {
        &self.min_width
    }

    pub fn min_height(&self) -> &SharedDerived<f32> {
        &self.min_height
    }

    pub fn max_width(&self) -> &SharedDerived<f32> {
        &self.max_width
    }

    pub fn max_height(&self) -> &SharedDerived<f32> {
        &self.max_height
    }

    // pub fn create_timer(&self, id: usize, duration: impl Into<Duration>) {
    //     let timer = Timer {
    //         id,
    //         start_time: Instant::now(),
    //         duration: duration.into(),
    //     };
    //     self.timers.write(|timers| timers.push(timer.clone()));
    // }

    // pub fn send_event(&self, event: Event) {
    //     self.event_loop_proxy.send_devent(event);
    // }

    pub fn theme(&self) -> &SharedSource<Theme> {
        &self.theme
    }
}

impl WindowContext {
    pub fn scale_factor(&self) -> f32 {
        self.window.scale_factor() as f32
        // 1.0
    }

    pub fn request_update_layout(&self) {
        if self.need_layout.get() {
            return;
        }
        self.need_layout.set(true);
        self.window.request_redraw();
    }
}

/*impl Deref for WindowContext {
    type Target = WindowContext;
    fn deref(&self) -> &Self::Target {
        self
    }
}

impl DerefMut for WindowContext {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self
    }
}*/

impl AsRef<WindowContext> for WindowContext {
    fn as_ref(&self) -> &WindowContext {
        self
    }
}

impl UseCloned for WindowContext {}