use crate::ui::animation::interpolator::{EaseOutCirc, Interpolator};
use crate::ui::animation::Target;
use crate::ui::app::AppContext;
use skia_safe::Color;
use std::sync::Arc;
use std::time::{Duration, Instant};
use material_colors::blend::cam16_ucs;
use material_colors::color::Argb;
use parking_lot::Mutex;

pub(crate) struct InnerAnimation {
    pub app_context: AppContext,
    pub duration: Duration,
    pub start_time: Instant,
    pub interpolator: Box<dyn Interpolator>,
    pub target: Target,
    pub transformation: Box<dyn FnMut()>,
}

impl InnerAnimation {
    pub fn new(app_context: AppContext, target: Target) -> Self {
        Self {
            app_context,
            duration: Duration::from_millis(500),
            start_time: Instant::now(),
            interpolator: Box::new(EaseOutCirc::new()),
            target,
            transformation: Box::new(|| {}),
        }
    }

    pub fn interpolate_f32(&self, start: f32, end: f32) -> f32 {
        let time_elapsed = self.start_time.elapsed().as_millis() as f32;
        let progress = (time_elapsed / self.duration.as_millis() as f32).clamp(0.0, 1.0);
        let interpolated = self.interpolator.interpolate(progress);
        start + (end - start) * interpolated
    }

    pub fn interpolate_color(&self, start: Color, end: Color) -> Color {
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
pub struct Animation {
    pub(crate) inner: Arc<Mutex<InnerAnimation>>,
}

impl Animation {
    pub fn new(app_context: AppContext, target: Target) -> Self {
        Self {
            inner: Arc::new(Mutex::new(InnerAnimation::new(app_context, target))),
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
    pub fn interpolator(self, interpolator: Box<dyn Interpolator>) -> Self {
        {
            let mut inner = self.inner.lock();
            inner.interpolator = interpolator;
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

    pub fn start(self) {
        let app_context = self.inner.lock().app_context.clone();
        app_context
            .starting_animations
            .write(|starting_animations| starting_animations.push_back(self.clone()));
    }

    pub fn is_finished(&self) -> bool {
        let inner = self.inner.lock();
        inner.is_finished()
    }

    pub fn is_target(&self, id: usize) -> bool {
        let inner = self.inner.lock();
        inner.is_target(id)
    }

    pub fn interpolate_f32(&self, start: f32, end: f32) -> f32 {
        let inner = self.inner.lock();
        inner.interpolate_f32(start, end)
    }

    pub fn interpolate_color(&self, start: Color, end: Color) -> Color {
        let inner = self.inner.lock();
        inner.interpolate_color(start, end)
    }
}

pub trait AnimationExt {
    fn animate(&self, target: Target) -> Animation;
}

impl AnimationExt for AppContext {
    fn animate(&self, target: Target) -> Animation {
        Animation::new(self.clone(), target)
    }
}
