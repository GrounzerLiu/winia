use crate::core::RefClone;
use crate::property::{Property, PropertyWeak, Settable};
use crate::ui::app::UserEvent;
use crate::ui::{Animation, Item};
use skiwin::SkiaWindow;
use std::collections::LinkedList;
use std::ops::DerefMut;
use std::sync::{Arc, Mutex, Weak};
use winit::event_loop::EventLoopProxy;
use crate::ui::item::ItemEvent;

pub struct AppContext {
    pub(crate) window: Arc<Mutex<Option<Box<dyn SkiaWindow>>>>,
    pub(crate) event_loop_proxy: Arc<Mutex<Option<EventLoopProxy<UserEvent>>>>,
    pub(crate) starting_animations: Arc<Mutex<LinkedList<Animation>>>,
    pub(crate) running_animations: Arc<Mutex<Vec<Animation>>>,
    /// (focused_id, waiting_for_focus_id)
    pub(crate) focused_item: Arc<Mutex<(Option<(usize,bool)>, Option<usize>)>>,
    pub(crate) title: Property<String>,
    pub(crate) min_width: Property<f32>,
    pub(crate) min_height: Property<f32>,
    pub(crate) max_width: Property<f32>,
    pub(crate) max_height: Property<f32>,
}

impl Default for AppContext {
    fn default() -> Self {
        Self::new()
    }
}

impl AppContext {
    pub fn new() -> Self {
        Self {
            window: Arc::new(Mutex::new(None)),
            event_loop_proxy: Arc::new(Mutex::new(None)),
            starting_animations: Arc::new(Mutex::new(LinkedList::new())),
            running_animations: Arc::new(Mutex::new(Vec::new())),
            focused_item: Arc::new(Mutex::new((None, None))),
            title: "Title".to_string().into(),
            min_width: 0.0.into(),
            min_height: 0.0.into(),
            max_width: f32::MAX.into(),
            max_height: f32::MAX.into(),
        }
    }

    pub(crate) fn is_window_created(&self) -> bool {
        let window = self.window.lock().unwrap();
        window.is_some()
    }

    pub(crate) fn window(&self, f: impl FnOnce(&mut dyn SkiaWindow)) {
        let mut window = self.window.lock().unwrap();
        if let Some(window) = window.as_mut() {
            f(window.deref_mut());
        }
    }

    pub(crate) fn event_loop_proxy(&self, f: impl FnOnce(&mut EventLoopProxy<UserEvent>)) {
        let mut event_loop_proxy = self.event_loop_proxy.lock().unwrap();
        if let Some(event_loop_proxy) = event_loop_proxy.as_mut() {
            f(event_loop_proxy);
        }
    }

    pub fn id(&self) -> usize {
        let mut id = 0_u64;
        self.window(|window| {
            id = window.id().into();
        });
        id as usize
    }

    pub fn title(&self) -> Property<String> {
        self.title.ref_clone()
    }

    pub fn min_width(&self) -> Property<f32> {
        self.min_width.ref_clone()
    }

    pub fn min_height(&self) -> Property<f32> {
        self.min_height.ref_clone()
    }

    pub fn max_width(&self) -> Property<f32> {
        self.max_width.ref_clone()
    }

    pub fn max_height(&self) -> Property<f32> {
        self.max_height.ref_clone()
    }

    pub fn start_animation(&mut self, animation: Animation) {
        self.starting_animations.lock().unwrap().push_back(animation);
    }
}

impl AppContext {
    pub fn scale_factor(&self) -> f32 {
        let mut scale_factor = 1.0;
        self.window(|window| {
            scale_factor = window.scale_factor();
        });
        scale_factor as f32
    }

    pub fn request_redraw(&self) {
        self.window(|window| {
            window.request_redraw();
        })
    }

    pub fn request_re_layout(&self) {
        self.event_loop_proxy(|event_loop_proxy| {
            match event_loop_proxy.send_event(UserEvent::ReLayout) {
                Ok(()) => {}
                Err(e) => {
                    panic!("Failed to send re-layout event: {}", e);
                }
            }
        });
    }
    
    pub fn request_focus(&self, id: usize, focused: bool) {
        self.focused_item.lock().unwrap().0 = Some((id, focused));
        self.request_redraw();
    }
}

// impl AppContext{
//     pub(crate) fn ref_clone(&self) -> Self {
//         Self {
//             window: self.window.clone(),
//             event_loop_proxy: self.event_loop_proxy.clone(),
//             starting_animations: self.starting_animations.clone(),
//             running_animations: self.running_animations.clone(),
//             title: self.title.ref_clone(),
//             min_width: self.min_width.ref_clone(),
//             min_height: self.min_height.ref_clone(),
//             max_width: self.max_width.ref_clone(),
//             max_height: self.max_height.ref_clone(),
//         }
//     }
// }

impl RefClone for AppContext {
    fn ref_clone(&self) -> Self {
        Self {
            window: self.window.clone(),
            event_loop_proxy: self.event_loop_proxy.clone(),
            starting_animations: self.starting_animations.clone(),
            running_animations: self.running_animations.clone(),
            focused_item: self.focused_item.clone(),
            title: self.title.ref_clone(),
            min_width: self.min_width.ref_clone(),
            min_height: self.min_height.ref_clone(),
            max_width: self.max_width.ref_clone(),
            max_height: self.max_height.ref_clone(),
        }
    }
}

pub struct AppContextWeak {
    window: Weak<Mutex<Option<Box<dyn SkiaWindow>>>>,
    event_loop_proxy: Weak<Mutex<Option<EventLoopProxy<UserEvent>>>>,
    starting_animations: Weak<Mutex<LinkedList<Animation>>>,
    running_animations: Weak<Mutex<Vec<Animation>>>,
    focused_item: Weak<Mutex<(Option<(usize,bool)>, Option<usize>)>>,
    title: PropertyWeak<String>,
    min_width: PropertyWeak<f32>,
    min_height: PropertyWeak<f32>,
    max_width: PropertyWeak<f32>,
    max_height: PropertyWeak<f32>,
}

impl AppContext {
    pub fn weak_ref(&self) -> AppContextWeak {
        AppContextWeak {
            window: Arc::downgrade(&self.window),
            event_loop_proxy: Arc::downgrade(&self.event_loop_proxy),
            starting_animations: Arc::downgrade(&self.starting_animations),
            running_animations: Arc::downgrade(&self.running_animations),
            focused_item: Arc::downgrade(&self.focused_item),
            title: self.title.weak(),
            min_width: self.min_width.weak(),
            min_height: self.min_height.weak(),
            max_width: self.max_width.weak(),
            max_height: self.max_height.weak(),
        }
    }
}

impl AppContextWeak {
    pub fn upgrade(&self) -> Option<AppContext> {
        Some(AppContext {
            window: self.window.upgrade()?,
            event_loop_proxy: self.event_loop_proxy.upgrade()?,
            starting_animations: self.starting_animations.upgrade()?,
            running_animations: self.running_animations.upgrade()?,
            focused_item: self.focused_item.upgrade()?,
            title: self.title.upgrade()?,
            min_width: self.min_width.upgrade()?,
            min_height: self.min_height.upgrade()?,
            max_width: self.max_width.upgrade()?,
            max_height: self.max_height.upgrade()?,
        })
    }
}