use crate::dpi::LogicalSize;
use crate::shared::{Shared, SharedAnimation, SharedBool, WeakShared};
use crate::ui::app::UserEvent;
use crate::ui::theme::Style;
use crate::ui::Animation;
use skia_safe::Color;
use skiwin::SkiaWindow;
use std::collections::{BTreeSet, LinkedList};
use std::ops::DerefMut;
use std::time::{Duration, Instant};
use winit::event_loop::EventLoopProxy;

#[derive(Clone, Debug)]
pub(crate) struct Timer {
    pub id: usize,
    pub start_time: Instant,
    pub duration: Duration,
}

#[derive(Clone)]
pub struct AppContext {
    pub(crate) theme: Shared<Style>,
    pub(crate) window: Shared<Option<Box<dyn SkiaWindow>>>,
    pub(crate) event_loop_proxy: Shared<Option<EventLoopProxy<UserEvent>>>,
    pub(crate) request_re_layout: Shared<bool>,
    pub(crate) starting_animations: Shared<LinkedList<Animation>>,
    pub(crate) running_animations: Shared<Vec<Animation>>,
    pub(crate) shared_animations: Shared<Vec<Box<dyn SharedAnimation>>>,
    pub(crate) focused_property: Shared<Option<(SharedBool, usize)>>,
    pub(crate) focus_changed_items: Shared<BTreeSet<usize>>,
    pub(crate) timers: Shared<Vec<Timer>>,
    pub(crate) title: Shared<String>,
    pub(crate) min_width: Shared<f32>,
    pub(crate) min_height: Shared<f32>,
    pub(crate) max_width: Shared<f32>,
    pub(crate) max_height: Shared<f32>,
}

impl Default for AppContext {
    fn default() -> Self {
        Self::new()
    }
}

impl AppContext {
    pub fn new() -> Self {
        Self {
            theme: Style::new(Color::RED, true).into(),
            window: None.into(),
            event_loop_proxy: None.into(),
            request_re_layout: false.into(),
            starting_animations: LinkedList::new().into(),
            running_animations: Vec::new().into(),
            shared_animations: Vec::new().into(),
            focused_property: None.into(),
            focus_changed_items: BTreeSet::new().into(),
            timers: Vec::new().into(),
            title: "Title".to_string().into(),
            min_width: 0.0.into(),
            min_height: 0.0.into(),
            max_width: f32::MAX.into(),
            max_height: f32::MAX.into(),
        }
    }

    pub(crate) fn is_window_created(&self) -> bool {
        self.window.read(|window| window.is_some())
    }

    pub(crate) fn window(&self, f: impl FnOnce(&mut dyn SkiaWindow)) {
        let mut window = self.window.value();
        if let Some(window) = window.deref_mut() {
            f(window.deref_mut());
        }
    }

    pub(crate) fn window_size(&self) -> Option<LogicalSize<f32>> {
        self.window.read(|window| {
            window.as_ref().map(|window| {
                let size = window.inner_size();
                let scale_factor = window.scale_factor() as f32;
                LogicalSize::new(
                    size.width as f32 / scale_factor,
                    size.height as f32 / scale_factor,
                )
            })
        })
    }

    pub(crate) fn event_loop_proxy(&self, f: impl FnOnce(&mut EventLoopProxy<UserEvent>)) {
        let mut event_loop_proxy = self.event_loop_proxy.value();
        if let Some(event_loop_proxy) = event_loop_proxy.deref_mut() {
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

    pub fn title(&self) -> Shared<String> {
        self.title.clone()
    }

    pub fn min_width(&self) -> Shared<f32> {
        self.min_width.clone()
    }

    pub fn min_height(&self) -> Shared<f32> {
        self.min_height.clone()
    }

    pub fn max_width(&self) -> Shared<f32> {
        self.max_width.clone()
    }

    pub fn max_height(&self) -> Shared<f32> {
        self.max_height.clone()
    }

    pub fn start_animation(&mut self, animation: Animation) {
        self.starting_animations
            .write(|starting_animations| starting_animations.push_back(animation.clone()));
    }

    pub fn create_timer(&self, id: usize, duration: impl Into<Duration>) {
        let timer = Timer {
            id,
            start_time: Instant::now(),
            duration: duration.into(),
        };
        self.timers.write(|timers| timers.push(timer.clone()));
    }

    pub fn send_user_event(&self, event: UserEvent) {
        self.event_loop_proxy(|event_loop_proxy| {
            match event_loop_proxy.send_event(event) {
                Ok(()) => {}
                Err(e) => {
                    // panic!("Failed to send user event: {}", e);
                }
            }
        });
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
        });
    }

    pub fn request_re_layout(&self) {
        self.request_re_layout.write(|request_re_layout| *request_re_layout = true);
        self.window.value().as_ref().map(|window| {
            window.request_redraw();
        });
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

pub struct AppContextWeak {
    theme: WeakShared<Style>,
    window: WeakShared<Option<Box<dyn SkiaWindow>>>,
    event_loop_proxy: WeakShared<Option<EventLoopProxy<UserEvent>>>,
    request_re_layout: WeakShared<bool>,
    starting_animations: WeakShared<LinkedList<Animation>>,
    running_animations: WeakShared<Vec<Animation>>,
    shared_animations: WeakShared<Vec<Box<dyn SharedAnimation>>>,
    focused_property: WeakShared<Option<(SharedBool, usize)>>,
    focus_changed_items: WeakShared<BTreeSet<usize>>,
    timer: WeakShared<Vec<Timer>>,
    title: WeakShared<String>,
    min_width: WeakShared<f32>,
    min_height: WeakShared<f32>,
    max_width: WeakShared<f32>,
    max_height: WeakShared<f32>,
}

impl AppContext {
    pub fn weak_ref(&self) -> AppContextWeak {
        AppContextWeak {
            theme: self.theme.weak(),
            window: self.window.weak(),
            event_loop_proxy: self.event_loop_proxy.weak(),
            request_re_layout: self.request_re_layout.weak(),
            starting_animations: self.starting_animations.weak(),
            running_animations: self.running_animations.weak(),
            shared_animations: self.shared_animations.weak(),
            focused_property: self.focused_property.weak(),
            focus_changed_items: self.focus_changed_items.weak(),
            timer: self.timers.weak(),
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
            theme: self.theme.upgrade()?,
            window: self.window.upgrade()?,
            event_loop_proxy: self.event_loop_proxy.upgrade()?,
            request_re_layout: self.request_re_layout.upgrade()?,
            starting_animations: self.starting_animations.upgrade()?,
            running_animations: self.running_animations.upgrade()?,
            shared_animations: self.shared_animations.upgrade()?,
            focused_property: self.focused_property.upgrade()?,
            focus_changed_items: self.focus_changed_items.upgrade()?,
            timers: self.timer.upgrade()?,
            title: self.title.upgrade()?,
            min_width: self.min_width.upgrade()?,
            min_height: self.min_height.upgrade()?,
            max_width: self.max_width.upgrade()?,
            max_height: self.max_height.upgrade()?,
        })
    }
}
