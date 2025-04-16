use crate::ui::animation::interpolator::{EaseOutCirc, Interpolator};
use crate::ui::animation::Target;
use crate::ui::app::{EventLoopProxy, WindowContext};
use skia_safe::Color;
use std::sync::Arc;
use std::time::{Duration, Instant};
use material_colors::blend::cam16_ucs;
use material_colors::color::Argb;
use parking_lot::Mutex;

pub(crate) struct InnerAnimation {
    pub event_loop_proxy: EventLoopProxy,
    pub duration: Duration,
    pub start_time: Instant,
    pub interpolator: Box<dyn Interpolator + Send>,
    pub target: Target,
    pub transformation: Box<dyn FnMut() + Send>,
    pub on_start: Option<Box<dyn Fn() + Send>>,
    pub on_finish: Option<Box<dyn Fn() + Send>>,
}

impl InnerAnimation {
    pub fn new(event_loop_proxy: impl AsRef<EventLoopProxy>, target: Target) -> Self {
        Self {
            event_loop_proxy: event_loop_proxy.as_ref().clone(),
            duration: Duration::from_millis(500),
            start_time: Instant::now(),
            interpolator: Box::new(EaseOutCirc::new()),
            target,
            transformation: Box::new(|| {}),
            on_start: None,
            on_finish: None,
        }
    }

    pub fn interpolate_f32(&self, start: f32, end: f32) -> f32 {
        let time_elapsed = self.start_time.elapsed().as_millis() as f32;
        let progress = (time_elapsed / self.duration.as_millis() as f32).clamp(0.0, 1.0);
        let interpolated = self.interpolator.interpolate(progress);
        start + (end - start) * interpolated
    }

    pub fn interpolate_color(&self, start: &Color, end: &Color) -> Color {
        let time_elapsed = self.start_time.elapsed().as_millis() as f64;
        let progress = (time_elapsed / self.duration.as_millis() as f64).clamp(0.0, 1.0);
        let start_a = start.a() as f64;
        let start_argb = Argb::new(255, start.r(), start.g(), start.b());
        let end_a = end.a() as f64;
        let end_argb = Argb::new(255, end.r(), end.g(), end.b());
        let blend_a = start_a + (end_a - start_a) * progress;
        let blend_argb = cam16_ucs(start_argb, end_argb, progress);
        let a = blend_a as u8;
        let r = blend_argb.red;
        let g = blend_argb.green;
        let b = blend_argb.blue;
        Color::from_argb(a, r, g, b)
    }

    pub fn on_start(&mut self, on_start: impl Fn() + Send + 'static) {
        self.on_start = Some(Box::new(on_start));
    }

    pub fn on_finish(&mut self, on_finish: impl Fn() + Send + 'static) {
        self.on_finish = Some(Box::new(on_finish));
    }

    pub fn is_target(&self, id: usize) -> bool {
        match &self.target {
            Target::Exclusion(targets) => !targets.contains(&id),
            Target::Inclusion(targets) => targets.contains(&id),
        }
    }

    pub fn is_finished(&self) -> bool {
        self.start_time.elapsed() >= self.duration
    }
}

#[derive(Clone)]
pub struct LayoutAnimation {
    pub(crate) inner: Arc<Mutex<InnerAnimation>>,
}

impl LayoutAnimation {
    pub fn new(event_loop_proxy: &EventLoopProxy, target: Target) -> Self {
        Self {
            inner: Arc::new(Mutex::new(InnerAnimation::new(event_loop_proxy, target))),
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
    pub fn interpolator(self, interpolator: Box<dyn Interpolator +Send>) -> Self {
        {
            let mut inner = self.inner.lock();
            inner.interpolator = interpolator;
        }
        self
    }

    /// What you should in the `transformation` closure is
    /// setting the properties of the [`Item`](crate::ui::item::Item) that you want to animate.
    pub fn transformation(self, transformation: impl FnMut() + Send + 'static) -> Self {
        {
            let mut inner = self.inner.lock();
            inner.transformation = Box::new(transformation);
        }
        self
    }

    pub fn on_finished(self, on_finished: impl Fn() + Send + 'static) -> Self {
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
        let event_loop_proxy = self.inner.lock().event_loop_proxy.clone();
        event_loop_proxy.start_layout_animation(self);
    }

    pub fn is_finished(&self) -> bool {
        let mut inner = self.inner.lock();
        let is_finish = inner.is_finished();
        if is_finish {
            if let Some(on_finished) = inner.on_finish.take() {
                on_finished();
            }
        }
        is_finish
    }

    pub fn is_target(&self, id: usize) -> bool {
        let inner = self.inner.lock();
        inner.is_target(id)
    }

    pub fn interpolate_f32(&self, start: f32, end: f32) -> f32 {
        let inner = self.inner.lock();
        inner.interpolate_f32(start, end)
    }

    pub fn interpolate_color(&self, start: &Color, end: &Color) -> Color {
        let inner = self.inner.lock();
        inner.interpolate_color(start, end)
    }
}

pub trait AnimationExt {
    fn animate(&self, target: Target) -> LayoutAnimation;
}

impl AnimationExt for WindowContext {
    fn animate(&self, target: Target) -> LayoutAnimation {
        LayoutAnimation::new(self.event_loop_proxy(), target)
    }
}

impl AnimationExt for EventLoopProxy {
    fn animate(&self, target: Target) -> LayoutAnimation {
        LayoutAnimation::new(self, target)
    }
}
