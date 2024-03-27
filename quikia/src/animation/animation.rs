use std::collections::{HashMap, LinkedList};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use material_color_utilities::blend_cam16ucs;
use skia_safe::Color;

use crate::animation::animation_set::AnimationSet;
use crate::app::SharedApp;
use crate::ui::{Item, DisplayParameter};

#[derive(Clone)]
pub struct AnimationController {
    is_finished: Arc<Mutex<bool>>,
}

impl AnimationController {
    pub fn new() -> Self {
        Self {
            is_finished: Arc::new(Mutex::new(false)),
        }
    }

    pub fn is_finished(&self) -> bool {
        *self.is_finished.lock().unwrap()
    }

    pub fn finish(&self) {
        *self.is_finished.lock().unwrap() = true;
    }
}

pub(crate) struct LayoutTransition {
    pub layout_transition: Box<dyn FnMut()>,
}

impl LayoutTransition {
    pub fn new(layout_transition: impl FnMut() + 'static) -> Self {
        Self {
            layout_transition: Box::new(layout_transition),
        }
    }

    pub fn run(&mut self) {
        (self.layout_transition)();
    }
}

unsafe impl Send for LayoutTransition {}

pub struct Animation {
    app: SharedApp,
    animation_controller: AnimationController,
    start_time: Instant,
    duration: Duration,
    pub(crate) layout_transition: LayoutTransition,
    pub(crate) from: Option<HashMap<usize, DisplayParameter>>,
    pub(crate) to: Option<HashMap<usize, DisplayParameter>>,
    on_start: LinkedList<Box<dyn FnMut()>>,
    on_finish: LinkedList<Box<dyn FnMut()>>,
}

impl Animation {
    pub fn new(app: SharedApp, layout_transition: impl FnMut() + 'static) -> Self {
        Self {
            app,
            animation_controller: AnimationController::new(),
            start_time: Instant::now(),
            duration: Duration::from_millis(2000),
            layout_transition: LayoutTransition::new(layout_transition),
            from: None,
            to: None,
            on_start: LinkedList::new(),
            on_finish: LinkedList::new(),
        }
    }

    pub fn with(self, animation: Animation) -> AnimationSet {
        AnimationSet::new().with(self).with(animation)
    }

    pub fn start(mut self) -> AnimationController {
        self.start_time = Instant::now();
        let animation_controller = self.animation_controller.clone();
        self.on_start.iter_mut().for_each(|on_start| {
            on_start();
        });
        let app = self.app.clone();
        app.lock().unwrap().animations.lock().unwrap().push(self);
        return animation_controller;
    }

    fn color_to_argb(color: &Color) -> u32 {
        let color = color.clone();
        let a = color.a();
        let r = color.r();
        let g = color.g();
        let b = color.b();
        (a as u32) << 24 | (r as u32) << 16 | (g as u32) << 8 | b as u32
    }

    pub fn update(&mut self, item: &mut Item, now: Instant) {
        // let elapsed = now - self.start_time;
        // let mut progress = elapsed.as_secs_f32() / self.duration.as_secs_f32();
        // let mut is_finished = false;
        // if progress >= 1.0 {
        //     progress = 1.0;
        //     is_finished = true;
        // } else if progress < 0.0 {
        //     progress = 0.0;
        // } else if self.animation_controller.is_finished() {
        //     self.on_finish.iter_mut().for_each(|on_finish| {
        //         on_finish();
        //     });
        //     return;
        // }
        // 
        // let from_map = self.from.as_mut().unwrap();
        // let to_map = self.to.as_mut().unwrap();
        // let mut stack = LinkedList::new();
        // stack.push_back(item);
        // while let Some(item) = stack.pop_back() {
        //     if let Some(from) = from_map.get(&item.get_id()) {
        //         let to = to_map.get(&item.get_id()).unwrap();
        //         if from != to {
        //             let mut layout_params = item.get_layout_params().clone();
        //             layout_params.relative_x = from.relative_x + (to.relative_x - from.relative_x) * progress;
        //             layout_params.relative_y = from.relative_y + (to.relative_y - from.relative_y) * progress;
        //             layout_params.width = from.width + (to.width - from.width) * progress;
        //             layout_params.height = from.height + (to.height - from.height) * progress;
        //             layout_params.margin_start = from.margin_start + (to.margin_start - from.margin_start) * progress;
        //             layout_params.margin_top = from.margin_top + (to.margin_top - from.margin_top) * progress;
        //             layout_params.margin_end = from.margin_end + (to.margin_end - from.margin_end) * progress;
        //             layout_params.margin_bottom = from.margin_bottom + (to.margin_bottom - from.margin_bottom) * progress;
        //             layout_params.padding_start = from.padding_start + (to.padding_start - from.padding_start) * progress;
        //             layout_params.padding_top = from.padding_top + (to.padding_top - from.padding_top) * progress;
        //             layout_params.padding_end = from.padding_end + (to.padding_end - from.padding_end) * progress;
        //             layout_params.padding_bottom = from.padding_bottom + (to.padding_bottom - from.padding_bottom) * progress;
        //             from.float_params.iter().for_each(|(key, value)| {
        //                 let to_value = to.float_params.get(key).unwrap();
        //                 layout_params.float_params.insert(key.clone(), value + (to_value - value) * progress);
        //             });
        //             from.color_params.iter().for_each(|(key, value)| {
        //                 let to_value = to.color_params.get(key).unwrap();
        //                 let from_argb = Self::color_to_argb(value);
        //                 let to_argb = Self::color_to_argb(to_value);
        //                 let argb = blend_cam16ucs(from_argb, to_argb, progress as f64);
        //                 layout_params.color_params.insert(key.clone(), Color::from(argb));
        //             });
        //             item.set_layout_params(&layout_params);
        //         } else {
        //             from_map.remove(&item.get_id());
        //             to_map.remove(&item.get_id());
        //         }
        //     }
        // 
        //     stack.extend(item.get_children_mut().iter_mut());
        // }
        // if is_finished {
        //     self.animation_controller.finish();
        //     self.on_finish.iter_mut().for_each(|on_finish| {
        //         on_finish();
        //     });
        // }
    }

    pub fn is_finished(&self) -> bool {
        self.animation_controller.is_finished()
    }

    pub fn duration(mut self, duration: impl Into<Duration>) -> Self {
        self.duration = duration.into();
        self
    }

    pub fn on_start(mut self, on_start: impl FnMut() + 'static) -> Self {
        self.on_start.push_back(Box::new(on_start));
        self
    }

    pub fn on_finish(mut self, on_finish: impl FnMut() + 'static) -> Self {
        self.on_finish.push_back(Box::new(on_finish));
        self
    }

    pub(crate) fn item_to_layout_params(item: &Item) -> HashMap<usize, DisplayParameter> {
        let mut map = HashMap::new();
        // let mut stack = LinkedList::new();
        // stack.push_back(item);
        // while let Some(item) = stack.pop_back() {
        //     map.insert(item.get_id(), item.get_layout_params().clone());
        //     stack.extend(item.get_children().iter());
        // }
        map
    }
}

pub trait AnimationExt {
    fn animation(&self, layout_transition: impl FnMut() + 'static) -> Animation;
}

impl AnimationExt for SharedApp {
    fn animation(&self, layout_transition: impl FnMut() + 'static) -> Animation {
        Animation::new(self.clone(), layout_transition)
    }
}
