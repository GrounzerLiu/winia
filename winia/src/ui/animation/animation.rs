use crate::core::{get_id_with_str, RefClone};
use material_color_utilities::blend_cam16ucs;
use material_color_utilities::utils::argb_from_rgb;
use skia_safe::Color;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use crate::ui::animation::ParameterOption;

pub(crate) struct InnerAnimation {
    pub duration: Duration,
    pub start_time: Instant,
    pub interpolator: Box<dyn Fn(f32) -> f32>,
    pub targets: Vec<(usize, Option<ParameterOption>, bool)>,
    pub transformation: Box<dyn FnMut()>,
}

impl InnerAnimation {
    pub fn new() -> Self {
        Self {
            duration: Duration::from_millis(500),
            start_time: Instant::now(),
            interpolator: Box::new(|t| t),
            targets: vec![],
            transformation: Box::new(|| {}),
        }
    }

    pub fn interpolate_f32(&self, start: f32, end: f32) -> f32 {
        let time_elapsed = self.start_time.elapsed().as_millis() as f32;
        let progress = time_elapsed / self.duration.as_millis() as f32;
        let interpolated = (self.interpolator)(progress);
        start + (end - start) * interpolated
    }

    pub fn interpolate_color(&self, start: Color, end: Color) -> Color {
        let time_elapsed = self.start_time.elapsed().as_millis() as f64;
        let progress = time_elapsed / self.duration.as_millis() as f64;
        let start_a = start.a() as f64;
        let start_u32 = argb_from_rgb(start.r(), start.g(), start.b());
        let end_a = end.a() as f64;
        let end_u32 = argb_from_rgb(end.r(), end.g(), end.b());
        let blend_a = start_a + (end_a - start_a) * progress;
        let blend_u32 = blend_cam16ucs(start_u32, end_u32, progress);
        let a = blend_a as u8;
        let r = (blend_u32 >> 16) as u8;
        let g = (blend_u32 >> 8) as u8;
        let b = blend_u32 as u8;
        Color::from_argb(a, r, g, b)
    }
}

pub struct Animation {
    pub(crate) inner: Arc<Mutex<InnerAnimation>>,
}

impl Animation {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(InnerAnimation::new())),
        }
    }

    /// Add a target to the animation.
    /// ``name``: The name of the [`Item`](crate::ui::item::Item) to animate.
    /// ``apply_to_children``: Whether to apply the animation to the children that not included in the target.
    pub fn add_target(self, name: &str, custom_transformation: Option<ParameterOption>, apply_to_children: bool) -> Self {
        {
            let mut inner = self.inner.lock().unwrap();
            inner.targets.push((
                get_id_with_str(name).expect(format!("Invalid name: {}", name).as_str()),
                custom_transformation,
                apply_to_children
            ));
        }
        self
    }

    pub fn duration(self, duration: Duration) -> Self {
        {
            let mut inner = self.inner.lock().unwrap();
            inner.duration = duration;
        }
        self
    }

    /// Set the interpolator function.
    pub fn interpolator(self, interpolator: Box<dyn Fn(f32) -> f32>) -> Self {
        {
            let mut inner = self.inner.lock().unwrap();
            inner.interpolator = interpolator;
        }
        self
    }

    /// What you should in the `transformation` closure is 
    /// setting the properties of the [`Item`](crate::ui::item::Item) that you want to animate.
    pub fn transformation(self, transformation: impl FnMut() + 'static) -> Self {
        {
            let mut inner = self.inner.lock().unwrap();
            inner.transformation = Box::new(transformation);
        }
        self
    }

    pub fn interpolate_f32(self, start: f32, end: f32) -> f32 {
        let inner = self.inner.lock().unwrap();
        inner.interpolate_f32(start, end)
    }

    pub fn interpolate_color(self, start: Color, end: Color) -> Color {
        let inner = self.inner.lock().unwrap();
        inner.interpolate_color(start, end)
    }
}

impl RefClone for Animation {
    fn ref_clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}