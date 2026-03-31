use std::fmt::Display;
use std::ops::{Add, Mul, Sub};
use std::sync::Arc;
use parking_lot::Mutex;
use crate::app::{EventLoopProxy, WindowContext};
use crate::shared::{SharedSource, SharedSourceWeak, SharedWeak};

pub trait Animation<T: AnimatableValue>: Send + Sync {
    fn update(&mut self) -> Option<T>;
    fn check_finished(&mut self) -> bool;
    fn animate_to(&mut self, target: T);
    fn stop(&mut self);
}

pub trait AnimatableValue:
Send
+ Add<Output=Self>
+ Sub<Output=Self>
+ Mul<Output=Self>
+ Sync
+ Clone
{
    fn from_f32(f: f32) -> Self;
    fn mul_f32(&self, factor: f32) -> Self;
    fn clamp(&self, min: Self, max: Self) -> Self;
}

impl AnimatableValue for f32 {
    fn from_f32(f: f32) -> Self {
        f
    }

    fn mul_f32(&self, factor: f32) -> Self {
        *self * factor
    }

    fn clamp(&self, min: Self, max: Self) -> Self {
        if *self < min {
            min
        } else if *self > max {
            max
        } else {
            *self
        }
    }
}

impl AnimatableValue for f64 {
    fn from_f32(f: f32) -> Self {
        f as f64
    }

    fn mul_f32(&self, factor: f32) -> Self {
        *self * (factor as f64)
    }

    fn clamp(&self, min: Self, max: Self) -> Self {
        if *self < min {
            min
        } else if *self > max {
            max
        } else {
            *self
        }
    }
}

macro_rules! impl_animatable_value {
    ($($t:ty),*) => {
        $(
            impl AnimatableValue for $t {
                fn from_f32(f: f32) -> Self {
                    f as $t
                }

                fn mul_f32(&self, factor: f32) -> Self {
                    (*self as f32 * factor) as $t
                }

                fn clamp(&self, min: Self, max: Self) -> Self {
                    if *self < min {
                        min
                    } else if *self > max {
                        max
                    } else {
                        *self
                    }
                }
            }
        )*
    };
}

impl_animatable_value!(i8, i16, i32, i64, isize, u8, u16, u32, u64, usize);

#[derive(Clone)]
pub struct SharedAnimation<T> {
    inner: Arc<Mutex<Box<dyn Animation<T>>>>,
    shared: SharedSourceWeak<T>,
    done: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
}

impl<T> SharedAnimation<T>
where
    T: AnimatableValue + 'static,
{
    pub fn new(animation: Box<dyn Animation<T>>, shared: SharedSource<T>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(animation)),
            shared: shared.weak(),
            done: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn wait_finished(&self) {
        let (tx, rx) = tokio::sync::oneshot::channel();
        {
            let mut done_guard = self.done.lock();
            *done_guard = Some(tx);
        }
        let _ = rx.await;
    }

    pub fn update(&self) -> Option<T> {
        self.inner.lock().update()
    }

    pub fn check_finished(&self) -> bool {
        let finished = self.inner.lock().check_finished();
        if finished && let Some(tx) = self.done.lock().take() {
            let _ = tx.send(());
        }
        finished
    }

    pub fn animate_to(&self, target: T) {
        self.inner.lock().animate_to(target);
    }

    pub fn stop(&self) {
        self.inner.lock().stop();
    }
}

pub trait AnyAnimation: Send + Sync {
    fn update(&self);
    fn is_finished(&self) -> bool;
}

impl<T> AnyAnimation for SharedAnimation<T>
where
    T: AnimatableValue + 'static,
{
    fn update(&self) {
        let mut guard = self.inner.lock();
        if let Some(value) = guard.update() && let Some(shared) = self.shared.upgrade() {
            shared.set(value);
        }
    }

    fn is_finished(&self) -> bool {
        self.check_finished()
    }
}


pub trait AnimationSpec<T>
where
    T: AnimatableValue,
{
    fn build(self, from: T, to: T) -> Box<dyn Animation<T>>;
}