use std::time::{Duration, Instant};
use crate::animation::Interpolator;
use crate::animation::interpolator::Linear;
use crate::shared::{AnimatableValue, Animation, AnimationSpec};

pub struct Keyframe<T: AnimatableValue> {
    pub time: Duration,
    pub value: T,
    pub interpolator: Box<dyn Interpolator>,
}

impl<T: AnimatableValue> Keyframe<T> {
    pub fn new(time: Duration, value: T) -> Self {
        Self {
            time,
            value,
            interpolator: Linear::boxed()
        }
    }

    pub fn interpolator(mut self, interpolator: Box<dyn Interpolator>) -> Self {
        self.interpolator = interpolator;
        self
    }
}

pub struct Keyframes<T: AnimatableValue> {
    pub keyframes: Vec<Keyframe<T>>,
    start_time: Option<Instant>,
    is_finished: bool,
    on_finish_callbacks: Vec<Box<dyn Fn() + Send + Sync>>,
    on_start_callbacks: Vec<Box<dyn Fn() + Send + Sync>>,
}

impl<T: AnimatableValue> Keyframes<T> {
    /// Creates a new Keyframes animation with the given keyframes.
    /// Returns an error if less than two keyframes are provided.
    pub fn try_new(mut keyframes: Vec<Keyframe<T>>) -> Result<Self, crate::error::WiniaError> {
        if keyframes.len() < 2 {
            return Err(crate::error::WiniaError::InsufficientKeyframes(keyframes.len()));
        }
        keyframes.sort_by_key(|k| k.time);
        Ok(Self {
            keyframes,
            start_time: None,
            is_finished: false,
            on_finish_callbacks: vec![],
            on_start_callbacks: vec![],
        })
    }

    /// Creates a new Keyframes animation.
    /// Panics in debug mode if less than two keyframes are provided.
    pub fn new(mut keyframes: Vec<Keyframe<T>>) -> Self {
        #[cfg(debug_assertions)]
        {
            Self::try_new(keyframes).expect("Keyframes animation requires at least two keyframes")
        }
        #[cfg(not(debug_assertions))]
        {
            match Self::try_new(keyframes) {
                Ok(anim) => anim,
                Err(e) => {
                    log::error!("{e}");
                    // Return a minimal valid animation
                    Self {
                        keyframes: vec![],
                        start_time: None,
                        is_finished: true,
                        on_finish_callbacks: vec![],
                        on_start_callbacks: vec![],
                    }
                }
            }
        }
    }

    fn total_duration(&self) -> Duration {
        self.keyframes.last().unwrap().time
    }
}

impl<T: AnimatableValue> Animation<T> for Keyframes<T> {
    fn update(&mut self) -> Option<T> {
        if self.check_finished() {
            return None
        }
        if self.start_time.is_none() {
            return None;
        }
        let elapsed = self.start_time.as_ref().unwrap().elapsed();
        // Find the current keyframe interval
        for i in 0..self.keyframes.len() - 1 {
            let kf_start = &self.keyframes[i];
            let kf_end = &self.keyframes[i + 1];
            if elapsed >= kf_start.time && elapsed <= kf_end.time {
                let interval_duration = kf_end.time - kf_start.time;
                let elapsed_in_interval = elapsed - kf_start.time;
                let t = elapsed_in_interval.as_secs_f32() / interval_duration.as_secs_f32();
                let t_interpolated = kf_end.interpolator.interpolate(t);
                let value = kf_start.value.clone() + (kf_end.value.clone() - kf_start.value.clone()).mul_f32(t_interpolated);
                return Some(value);
            }
        }
        self.is_finished = true;
        self.start_time = None;
        None
    }

    fn check_finished(&mut self) -> bool {
        if self.is_finished {
            return true;
        }
        if self.start_time.is_none() {
            return false;
        }
        let elapsed = self.start_time.unwrap().elapsed();
        if elapsed >= self.total_duration() {
            for callback in &self.on_finish_callbacks {
                callback();
            }
            self.start_time = None;
            self.is_finished = true;
            return true;
        }
        false
    }

    fn animate_to(&mut self, _target: T) {
        self.start_time = Some(Instant::now());
        self.is_finished = false;
        for callback in &self.on_start_callbacks {
            callback();
        }
    }

    fn stop(&mut self) {
        if self.start_time.is_none() || self.is_finished {
            return;
        }
        self.start_time = None;
        self.is_finished = true;
        for callback in &self.on_finish_callbacks {
            callback();
        }
    }
}

pub struct KeyframesSpec<T: AnimatableValue> {
    pub keyframes: Vec<Keyframe<T>>,
    on_start_callbacks: Vec<Box<dyn Fn() + Send + Sync>>,
    on_finish_callbacks: Vec<Box<dyn Fn() + Send + Sync>>,
}
impl<T: AnimatableValue> KeyframesSpec<T> {
    pub fn new(keyframes: Vec<Keyframe<T>>) -> Self {
        Self {
            keyframes,
            on_start_callbacks: vec![],
            on_finish_callbacks: vec![],
        }
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

    pub fn build_keyframes(self) -> Keyframes<T> {
        let mut keyframes = Keyframes::new(self.keyframes);
        keyframes.on_start_callbacks = self.on_start_callbacks;
        keyframes.on_finish_callbacks = self.on_finish_callbacks;
        keyframes.start_time = Some(Instant::now());
        keyframes
    }
}

impl<T: AnimatableValue + 'static> AnimationSpec<T> for KeyframesSpec<T> {
    fn build(self, from: T, to: T) -> Box<dyn Animation<T>> {
        let keyframes = self.build_keyframes();
        Box::new(keyframes)
    }
}