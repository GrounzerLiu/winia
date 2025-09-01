use crate::ui::animation::interpolator::{EaseOutCirc, Interpolator};
use crate::ui::animation::{interpolate_color, interpolate_f32, Animation, Target};
use crate::ui::app::{EventLoopProxy, WindowContext};
use parking_lot::Mutex;
use skia_safe::Color;
use std::ops::Deref;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub(crate) struct InnerLocalAnimation {
    pub window_context: WindowContext,
    pub duration: Duration,
    pub start_time: Instant,
    pub interpolator: Box<dyn Interpolator>,
    pub target: Target,
    pub transformation: Box<dyn FnMut()>,
    pub is_finished: bool,
    pub on_start: Option<Box<dyn Fn()>>,
    pub on_finish: Option<Box<dyn Fn()>>,
}

impl InnerLocalAnimation {
    pub fn new(window_context: &WindowContext, target: Target) -> Self {
        Self {
            window_context: window_context.clone(),
            duration: Duration::from_millis(500),
            start_time: Instant::now(),
            interpolator: Box::new(EaseOutCirc::new()),
            target,
            transformation: Box::new(|| {}),
            is_finished: false,
            on_start: None,
            on_finish: None,
        }
    }

    fn progress(&self) -> f32 {
        let time_elapsed = self.start_time.elapsed().as_millis() as f32;
        (time_elapsed / self.duration.as_millis() as f32).clamp(0.0, 1.0)
    }

    fn interpolate_f32(&self, start: f32, end: f32) -> f32 {
        interpolate_f32(
            start,
            end,
            self.progress(),
            self.interpolator.deref(),
        )
    }

    fn interpolate_color(&self, start: &Color, end: &Color) -> Color {
        interpolate_color(
            start,
            end,
            self.progress(),
        )
    }

    pub fn on_start(&mut self, on_start: impl Fn() + 'static) {
        self.on_start = Some(Box::new(on_start));
    }

    pub fn on_finish(&mut self, on_finish: impl Fn() + 'static) {
        self.on_finish = Some(Box::new(on_finish));
    }

    pub fn is_target(&self, id: usize) -> bool {
        match &self.target {
            Target::Exclusion(targets) => !targets.contains(&id),
            Target::Inclusion(targets) => targets.contains(&id),
        }
    }

    pub fn is_finished(&self) -> bool {
        self.start_time.elapsed() >= self.duration || self.is_finished
    }
}

#[derive(Clone)]
pub struct LocalLayoutAnimation {
    pub(crate) inner: Arc<Mutex<InnerLocalAnimation>>,
}

impl LocalLayoutAnimation {
    pub fn new(window_context: &WindowContext, target: Target) -> Self {
        Self {
            inner: Arc::new(Mutex::new(InnerLocalAnimation::new(window_context, target))),
        }
    }

    pub fn duration(self, duration: Duration) -> Self {
        {
            let mut inner = self.inner.lock();
            inner.duration = duration;
        }
        self
    }

    /// Set the interpolator function.
    pub fn interpolator(self, interpolator: impl Interpolator + 'static) -> Self {
        {
            let mut inner = self.inner.lock();
            inner.interpolator = Box::new(interpolator);
        }
        self
    }

    /// What you should in the `transformation` closure is
    /// setting the properties of the [`Item`](crate::ui::item::Item) that you want to animate.
    pub fn transformation(self, transformation: impl FnMut() + 'static) -> Self {
        {
            let mut inner = self.inner.lock();
            inner.transformation = Box::new(transformation);
        }
        self
    }

    pub fn on_finished(self, on_finished: impl Fn() + 'static) -> Self {
        {
            let mut inner = self.inner.lock();
            inner.on_finish(on_finished);
        }
        self
    }

    pub fn start(self) {
        {
            let mut inner = self.inner.lock();
            if let Some(on_start) = inner.on_start.take() {
                on_start();
            }
        }
        let window_context = &self.inner.lock().window_context;
        window_context.starting_local_animations.lock().push_back(self.clone());
        window_context.request_redraw()
    }


    pub fn is_target(&self, id: usize) -> bool {
        let inner = self.inner.lock();
        inner.is_target(id)
    }
}

impl Animation for LocalLayoutAnimation {
    fn interpolate_f32(&self, start: f32, end: f32) -> f32 {
        let inner = self.inner.lock();
        inner.interpolate_f32(start, end)
    }

    fn interpolate_color(&self, start: &Color, end: &Color) -> Color {
        let inner = self.inner.lock();
        inner.interpolate_color(start, end)
    }

    fn clone_boxed(&self) -> Box<dyn Animation> {
        Box::new(self.clone())
    }
    fn is_finished(&self) -> bool {
        let inner = self.inner.lock();
        inner.is_finished()
    }

    fn animatable(&self, id: usize, forced: bool) -> (bool, bool) {
        let animation_inner = self.inner.lock();
        match &animation_inner.target {
            Target::Exclusion(targets) => {
                let is_excluded = targets.contains(&id);
                (!is_excluded && !forced, is_excluded)
            }
            Target::Inclusion(targets) => {
                let is_included = targets.contains(&id);
                (is_included || forced, is_included || forced)
            }
        }
    }

    fn finish(&mut self) {
        let mut inner = self.inner.lock();
        if let Some(on_finish) = inner.on_finish.take() {
            on_finish();
        }
        inner.is_finished = true;
    }
}

pub trait LocalAnimationExt {
    fn local_animate(&self, target: Target) -> LocalLayoutAnimation;
}

impl LocalAnimationExt for WindowContext {
    fn local_animate(&self, target: Target) -> LocalLayoutAnimation {
        LocalLayoutAnimation::new(self, target)
    }
}
