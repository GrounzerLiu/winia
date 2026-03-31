use std::ops::{Add, Div, Mul, Sub};
use std::time::{Duration, Instant};
use crate::animation::Interpolator;
use crate::animation::interpolator::Linear;
use crate::shared::animation::shared_animation::{AnimatableValue, Animation};
use crate::shared::{AnimationSpec, AnyAnimation};

pub struct Tween<T> {
    pub from: T,
    pub to: T,
    start_time: Option<Instant>,
    duration: Duration,
    interpolator: Box<dyn Interpolator>,
    is_finished: bool,
    on_start_callbacks: Vec<Box<dyn Fn() + Send + Sync>>,
    on_finish_callbacks: Vec<Box<dyn Fn() + Send + Sync>>,
    on_restart_callbacks: Vec<Box<dyn Fn() + Send + Sync>>,
}

impl<T> Tween<T>
where
    T: AnimatableValue,
{
    pub fn new(
        from: T,
        to: T,
    ) -> Self {
        Self {
            from,
            to,
            start_time: None,
            duration: Duration::from_secs(1),
            interpolator: Linear::boxed(),
            is_finished: false,
            on_start_callbacks: vec![],
            on_finish_callbacks: vec![],
            on_restart_callbacks: vec![],
        }
    }

    pub fn duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    pub fn interpolator(mut self, interpolator: Box<dyn Interpolator>) -> Self {
        self.interpolator = interpolator;
        self
    }

    pub fn on_start<F>(mut self, callback: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.on_start_callbacks.push(Box::new(callback));
        self
    }
    pub fn on_finish<F>(mut self, callback: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.on_finish_callbacks.push(Box::new(callback));
        self
    }

    pub fn on_restart<F>(mut self, callback: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.on_restart_callbacks.push(Box::new(callback));
        self
    }

    pub fn restart(&mut self) {
        self.is_finished = false;
        self.start_time = Some(Instant::now());
        for callback in &self.on_restart_callbacks {
            callback();
        }
    }
}

impl<T> Animation<T> for Tween<T>
where
    T: AnimatableValue,
{
    fn update(&mut self) -> Option<T> {
        if self.check_finished() {
            return None;
        }
        if let Some(start_time) = self.start_time {
            let elapsed = start_time.elapsed();
            let t = (elapsed.as_secs_f32() / self.duration.as_secs_f32()).min(1.0);
            let t = self.interpolator.interpolate(t);
            let value = self.from.clone() + (self.to.clone() - self.from.clone()).mul_f32(t);
            Some(value)
        } else {
            None
        }
    }

    fn check_finished(&mut self) -> bool {
        if self.is_finished {
            return true;
        }
        if let Some(start_time) = self.start_time {
            if start_time.elapsed() >= self.duration {
                for callback in &self.on_finish_callbacks {
                    callback();
                }
                self.is_finished = true;
                return true;
            }
        }
        false
    }

    fn animate_to(&mut self, target: T) {
        self.to = target;
        self.is_finished = false;
        self.start_time = Some(Instant::now());
        for callback in &self.on_start_callbacks {
            callback();
        }
    }

    fn stop(&mut self) {
        if self.is_finished || self.start_time.is_none() {
            return;
        }
        self.is_finished = true;
        self.start_time = None;
        for callback in &self.on_finish_callbacks {
            callback();
        }
    }
}

pub struct TweenSpec<T: AnimatableValue> {
    phantom: std::marker::PhantomData<T>,
    duration: Duration,
    interpolator: Box<dyn Interpolator>,
    on_start_callbacks: Vec<Box<dyn Fn() + Send + Sync>>,
    on_finish_callbacks: Vec<Box<dyn Fn() + Send + Sync>>,
    on_restart_callbacks: Vec<Box<dyn Fn() + Send + Sync>>,
}

impl<T> TweenSpec<T>
where
    T: AnimatableValue,
{
    pub fn new() -> Self {
        Self {
            phantom: std::marker::PhantomData,
            duration: Duration::from_secs(1),
            interpolator: Linear::boxed(),
            on_start_callbacks: vec![],
            on_finish_callbacks: vec![],
            on_restart_callbacks: vec![],
        }
    }

    pub fn duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    pub fn interpolator(mut self, interpolator: Box<dyn Interpolator>) -> Self {
        self.interpolator = interpolator;
        self
    }

    pub fn on_start<F>(mut self, callback: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.on_start_callbacks.push(Box::new(callback));
        self
    }
    pub fn on_finish<F>(mut self, callback: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.on_finish_callbacks.push(Box::new(callback));
        self
    }

    pub fn on_restart<F>(mut self, callback: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.on_restart_callbacks.push(Box::new(callback));
        self
    }

    pub fn build_tween(self, from: T, to: T) -> Tween<T> {
        Tween {
            from,
            to,
            start_time: Some(Instant::now()),
            duration: self.duration,
            interpolator: self.interpolator,
            is_finished: false,
            on_start_callbacks: self.on_start_callbacks,
            on_finish_callbacks: self.on_finish_callbacks,
            on_restart_callbacks: self.on_restart_callbacks,
        }
    }
}

impl<T> AnimationSpec<T> for TweenSpec<T>
where
    T: AnimatableValue + 'static,
{
    fn build(self, from: T, to: T) -> Box<dyn Animation<T>> {
        let tween = self.build_tween(from, to);
        Box::new(tween)
    }
}