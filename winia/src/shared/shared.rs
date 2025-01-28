use crate::ui::app::AppContext;
use std::fmt::Display;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::{Duration, Instant};
use crate::core::generate_id;
use crate::shared::SharedF32;
use crate::ui::animation::interpolator::{Interpolator, Linear};

/// A trait for getting the value of a shared.
/// This function will return a specific value instead of `PropertyValue` which needs to be unwrapped.
pub trait Gettable<T: Clone> {
    fn get(&self) -> T;
}

/// A trait for setting the value of a shared.
/// So we can use the same function to set the static value and the dynamic value.
pub trait Settable<T: ?Sized> {
    fn set(&mut self, value: T);
}

/// A trait for observing changes in a shared.
pub trait Observable {
    fn add_observer(&mut self, id: usize, observer: Box<dyn FnMut()>) -> Removal;
}

pub struct Removal {
    removal: Box<dyn FnOnce()>,
}

impl Removal {
    pub fn new(removal: impl FnOnce() + 'static) -> Self {
        Self {
            removal: Box::new(removal),
        }
    }
    pub fn drop(self) {}
    pub fn unwrap(self) -> Box<dyn FnOnce()> {
        self.removal
    }
}

#[macro_export]
macro_rules! shared {
    (|$($observable:ident),*| $value_generator:block) => {
        {
            let observable = [$($observable.clone()),*];
            Shared::from_dynamic(&observable, move || $value_generator)
        }
    }
}

pub struct Shared<T> {
    id: usize,
    /// The value of the shared.
    value: Arc<Mutex<T>>,
    value_generator: Arc<Mutex<Option<Box<dyn Fn() -> T>>>>,
    /// A list of objects that observed by this shared.
    /// The first element of the tuple is the observable object, and the second element is the id of the observer.
    /// The id is used to remove the observer when the observable object is dropped.
    observed_objects: Arc<Mutex<Vec<(Option<Box<dyn Observable>>, Box<dyn FnOnce()>)>>>,
    /// A list of simple observers. The key is the id of the observer.
    simple_observers: Arc<Mutex<Vec<(usize, Box<dyn FnMut()>)>>>,
    /// A list of specific observers. The key is the id of the observer. The value is the observer function.
    /// The observer function takes a mutable reference to the shared value.
    specific_observers: Arc<Mutex<Vec<(usize, Box<dyn FnMut(&mut T)>)>>>,
    animation: Arc<Mutex<Option<SharedAnimation<T>>>>,
}

impl<T: 'static> Shared<T> {
    pub fn new(value: T) -> Self {
        Self::from_static(value)
    }

    fn inner_new(value: T, value_generator: Option<Box<dyn Fn() -> T>>) -> Self {
        let value = Arc::new(Mutex::new(value));
        let value_generator = Arc::new(Mutex::new(value_generator));
        Self {
            id: generate_id(),
            value,
            value_generator,
            observed_objects: Arc::new(Mutex::new(Vec::with_capacity(0))),
            simple_observers: Arc::new(Mutex::new(Vec::with_capacity(0))),
            specific_observers: Arc::new(Mutex::new(Vec::with_capacity(0))),
            animation: Arc::new(Mutex::new(None)),
        }
    }

    pub fn from_static(value: T) -> Self {
        Self::inner_new(value, None)
    }

    pub fn from_dynamic<O: Observable + Clone + 'static>(o: &[O], value_generator: impl Fn() -> T + 'static) -> Self {
        let value_generator: Box<dyn Fn() -> T> = Box::new(value_generator);
        let value = value_generator();
        let value_generator = Some(value_generator);
        let mut shared = Self::inner_new(value, value_generator);
        for observable in o {
            shared.observe(observable.clone());
        }
        shared
    }

    pub fn value(&self) -> MutexGuard<T> {
        self.value.lock().unwrap()
    }

    pub fn read<R>(&self, mut operation: impl FnMut(&T) -> R) -> R {
        let value = self.value.lock().unwrap();
        operation(value.deref())
    }

    pub fn write<R>(&self, mut operation: impl FnMut(&mut T) -> R) -> R {
        let r = {
            let mut value = self.value.lock().unwrap();
            operation(value.deref_mut())
        };
        self.notify();
        r
    }

    pub fn id(&self) -> usize {
        self.id
    }

    pub fn set_static(&mut self, value: T) {
        self.clear_observed_objects();
        *self.value.lock().unwrap() = value;
        *self.value_generator.lock().unwrap() = None;
        self.notify();
    }


    /// Sets a new dynamic value for this object and stores the value generator used to compute it.
    ///
    /// This method will clear the list of observed objects before setting the new value.
    ///
    /// The `value_generator` parameter is a closure that takes no arguments and returns a value of type `T`.
    /// It is wrapped in a `Box` to ensure that it has a static lifetime.
    ///
    /// The new value is computed by calling `value_generator` and stored in `self.value`.
    /// The value generator is stored in `self.value_generator`.
    ///
    /// Finally, the `notify` method is called to notify any observers of the change in value.
    pub fn set_dynamic(&mut self, value_generator: impl Fn() -> T + 'static) {
        self.clear_observed_objects();
        let value_generator = Box::new(value_generator);
        let value = (&value_generator)();
        *self.value.lock().unwrap() = value;
        *self.value_generator.lock().unwrap() = Some(value_generator);
        self.notify();
    }

    fn can_generate(&self) -> bool {
        self.value_generator.lock().unwrap().is_some()
    }

    pub fn notify(&self) {
        if self.can_generate() {
            let value_generator = self.value_generator.lock().unwrap();
            let mut value = self.value.lock().unwrap();
            *value = (&value_generator.as_ref().unwrap())();
        }

        for (_, observer) in self.simple_observers.lock().unwrap().iter_mut() {
            observer();
        }

        let mut value = self.value.lock().unwrap();
        for (_, observer) in self.specific_observers.lock().unwrap().iter_mut() {
            observer(&mut *value);
        }
    }

    fn clear_observed_objects(&mut self) {
        for (_, removal) in self.observed_objects.lock().unwrap().drain(..) {
            removal();
        }
    }

    pub fn add_specific_observer(&mut self, id: usize, observer: impl FnMut(&mut T) + 'static) {
        self.specific_observers.lock().unwrap().push((id, Box::new(observer)));
    }

    fn observe<O: Observable + Clone + 'static>(&mut self, observable: O) {
        let mut observable = Box::new(observable);
        let self_weak = self.weak();
        let removal = observable.add_observer(self.id(), Box::new(move || {
            if let Some(mut property) = self_weak.upgrade() {
                property.notify();
            }
        })).unwrap();
        self.observed_objects.lock().unwrap().push((Some(observable), removal));
    }

    pub fn remove_observer(&mut self, id: usize) {
        self.simple_observers.lock().unwrap().retain(|(i, _)| *i != id);
        self.specific_observers.lock().unwrap().retain(|(i, _)| *i != id);
    }

    pub fn get_animation(&self) -> Option<SharedAnimation<T>> {
        self.animation.lock().unwrap().as_ref().cloned()
    }

    pub fn weak(&self) -> WeakShared<T> {
        WeakShared {
            id: self.id,
            value: Arc::downgrade(&self.value),
            value_generator: Arc::downgrade(&self.value_generator),
            observed_objects: Arc::downgrade(&self.observed_objects),
            simple_observers: Arc::downgrade(&self.simple_observers),
            specific_observers: Arc::downgrade(&self.specific_observers),
            animation: Arc::downgrade(&self.animation),
        }
    }
}

impl<T> Clone for Shared<T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            value: self.value.clone(),
            value_generator: self.value_generator.clone(),
            observed_objects: self.observed_objects.clone(),
            simple_observers: self.simple_observers.clone(),
            specific_observers: self.specific_observers.clone(),
            animation: self.animation.clone(),
        }
    }
}

impl<T> Observable for Shared<T> {
    fn add_observer(&mut self, id: usize, observer: Box<dyn FnMut()>) -> Removal {
        self.simple_observers.lock().unwrap().push((id, observer));
        let simple_observers = self.simple_observers.clone();
        Removal {
            removal: Box::new(move || {
                simple_observers.lock().unwrap().retain(|(i, _)| *i != id);
            })
        }
    }
}

// impl<T> Deref for Shared<T> {
//     type Target = Arc<Mutex<T>>;
//
//     fn deref(&self) -> &Arc<Mutex<T>> {
//         &self.value
//     }
// }
//
// impl<T: 'static> DerefMut for Shared<T> {
//     fn deref_mut(&mut self) -> &mut Arc<Mutex<T>> {
//         &mut self.value
//     }
// }
//
// impl<T: 'static> AsRef<Arc<Mutex<T>>> for Shared<T> {
//     fn as_ref(&self) -> &Arc<Mutex<T>> {
//         &self.value
//     }
// }
//
// impl<T: 'static> AsMut<Arc<Mutex<T>>> for Shared<T> {
//     fn as_mut(&mut self) -> &mut Arc<Mutex<T>> {
//         &mut self.value
//     }
// }


impl<T: 'static> From<T> for Shared<T> {
    fn from(value: T) -> Self {
        Self::from_static(value)
    }
}

impl<T: 'static> Settable<T> for Shared<T> {
    fn set(&mut self, value: T) {
        self.set_static(value);
    }
}

impl<T: 'static> Settable<Box<dyn Fn() -> T>> for Shared<T> {
    fn set(&mut self, value: Box<dyn Fn() -> T>) {
        self.set_dynamic(value);
    }
}

impl<T: Clone> Gettable<T> for Shared<T> {
    fn get(&self) -> T {
        self.value.lock().unwrap().clone()
    }
}

impl<T: Clone> Shared<T> {
    fn into(self) -> T {
        self.get()
    }
}

impl<T: 'static> AsRef<Shared<T>> for Shared<T> {
    fn as_ref(&self) -> &Shared<T> {
        self
    }
}

impl<T: 'static> From<&Shared<T>> for Shared<T> {
    fn from(value: &Shared<T>) -> Self {
        value.clone()
    }
}

impl<T: 'static + PartialEq> PartialEq for Shared<T> {
    fn eq(&self, other: &Self) -> bool {
        self.read(|value| other.read(|other_value| value == other_value))
    }
}

impl<T: 'static + Display> Display for Shared<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.read(|value| value.fmt(f))
    }
}

impl<T: 'static + Default> Default for Shared<T> {
    fn default() -> Self {
        Self::from_static(Default::default())
    }
}

#[derive(Clone)]
pub struct WeakShared<T> {
    id: usize,
    value: Weak<Mutex<T>>,
    value_generator: Weak<Mutex<Option<Box<dyn Fn() -> T>>>>,
    observed_objects: Weak<Mutex<Vec<(Option<Box<dyn Observable>>, Box<dyn FnOnce()>)>>>,
    simple_observers: Weak<Mutex<Vec<(usize, Box<dyn FnMut()>)>>>,
    specific_observers: Weak<Mutex<Vec<(usize, Box<dyn FnMut(&mut T)>)>>>,
    animation: Weak<Mutex<Option<SharedAnimation<T>>>>,
}

impl<T: 'static> WeakShared<T> {
    pub fn upgrade(&self) -> Option<Shared<T>> {
        let value = self.value.upgrade()?;
        let value_generator = self.value_generator.upgrade()?;
        let observed_objects = self.observed_objects.upgrade()?;
        let simple_observers = self.simple_observers.upgrade()?;
        let specific_observers = self.specific_observers.upgrade()?;
        let animation = self.animation.upgrade()?;
        Some(Shared {
            id: self.id,
            value,
            value_generator,
            observed_objects,
            simple_observers,
            specific_observers,
            animation,
        })
    }

    pub fn id(&self) -> usize {
        self.id
    }

    pub fn is_alive(&self) -> bool {
        self.value.upgrade().is_some()
    }

    pub fn read<R>(&self, operation: impl Fn(&T) -> R) -> Option<R> {
        Some(self.upgrade()?.read(operation))
    }

    pub fn write<R>(&self, operation: impl Fn(&mut T) -> R) -> Option<R> {
        Some(self.upgrade()?.write(operation))
    }
}


struct InnerSharedAnimation<T> {
    id: usize,
    is_finished: bool,
    shared: WeakShared<T>,
    from: T,
    to: T,
    value_generator: Box<dyn Fn(&T, &T, f32) -> T>,
    duration: Duration,
    start_time: Instant,
    interpolator: Box<dyn Interpolator>,
    on_start: Option<Box<dyn FnMut()>>,
    on_finish: Option<Box<dyn FnMut()>>,
}

impl<T: 'static> InnerSharedAnimation<T> {
    pub fn new(f32: Shared<T>, from: T, to: T, value_generator: impl Fn(&T, &T, f32) -> T + 'static) -> Self {
        Self {
            id: generate_id(),
            is_finished: false,
            shared: f32.weak(),
            from,
            to,
            value_generator: Box::new(value_generator),
            duration: Duration::from_secs(500),
            start_time: Instant::now(),
            interpolator: Box::new(Linear::new()),
            on_start: None,
            on_finish: None,
        }
    }

    pub fn duration(&mut self, duration: Duration) {
        self.duration = duration;
    }

    pub fn interpolator(&mut self, interpolator: impl Interpolator + 'static) {
        self.interpolator = Box::new(interpolator);
    }

    pub fn on_start(&mut self, on_start: impl FnMut() + 'static) {
        self.on_start = Some(Box::new(on_start));
    }

    /// Set the function to be called when the animation is finished or stopped.
    pub fn on_finish(&mut self, on_finish: impl FnMut() + 'static) {
        self.on_finish = Some(Box::new(on_finish));
    }

    // pub fn start(mut self, app_context: &AppContext){
    //     self.start_time = Instant::now();
    //     app_context.shared_animations.value().push(Box::new(self));
    //     app_context.request_redraw();
    //     if let Some(on_start) = self.on_start.take(){
    //         on_start();
    //     }
    // }

    pub fn stop(&mut self) {
        self.is_finished = true;
        // if let Some(on_finish) = self.on_finish.as_mut(){
        //     on_finish();
        // }
    }

    pub fn is_finished(&self) -> bool {
        self.is_finished || self.start_time.elapsed() >= self.duration
    }

    pub fn update(&mut self) {
        if self.is_finished() {
            if let Some(on_finish) = self.on_finish.as_mut() {
                on_finish();
            }
            return;
        }
        let time_elapsed = self.start_time.elapsed().as_millis() as f32;
        let progress = (time_elapsed / self.duration.as_millis() as f32).clamp(0.0, 1.0);
        let interpolated = self.interpolator.interpolate(progress);
        let new_value = (self.value_generator)(&self.from, &self.to, interpolated);
        self.shared.upgrade().map(move |mut shared| {
            shared.set(new_value);
        });
    }
}

pub struct SharedAnimation<T> {
    inner: Arc<Mutex<InnerSharedAnimation<T>>>,
}

impl<T: 'static> SharedAnimation<T> {
    pub fn new(f32: Shared<T>, from: T, to: T, value_generator: impl Fn(&T, &T, f32) -> T + 'static) -> Self {
        let inner = InnerSharedAnimation::new(f32, from, to, value_generator);
        Self {
            inner: Arc::new(Mutex::new(inner)),
        }
    }

    pub fn duration(self, duration: Duration) -> Self {
        self.inner.lock().unwrap().duration = duration;
        self
    }

    pub fn interpolator(self, interpolator: impl Interpolator + 'static) -> Self {
        self.inner.lock().unwrap().interpolator(interpolator);
        self
    }

    pub fn on_start(self, on_start: impl FnMut() + 'static) -> Self {
        self.inner.lock().unwrap().on_start(on_start);
        self
    }

    pub fn on_finish(self, on_finish: impl FnMut() + 'static) -> Self {
        self.inner.lock().unwrap().on_finish(on_finish);
        self
    }

    pub fn start(self, app_context: &AppContext) -> Self {
        {
            let mut inner = self.inner.lock().unwrap();
            inner.start_time = Instant::now();
            app_context.shared_animations.write(|shared_animations| {
                shared_animations.push(Box::new(self.clone()));
            });
            let cloned = self.clone();
            // inner.shared.animation.lock().unwrap().replace(cloned);
            inner.shared.upgrade().map(move |mut shared| {
                shared.animation.lock().unwrap().replace(cloned);
            });
            app_context.request_redraw();
            if let Some(mut on_start) = inner.on_start.take() {
                on_start();
            }
        }
        self
    }

    pub fn stop(self) -> Self {
        self.inner.lock().unwrap().stop();
        self
    }

    // pub fn is_finished(&self) -> bool {
    //     self.inner.read(|inner| inner.is_finished())
    // }
    //
    // pub fn update(self, app_context: &AppContext) -> Self {
    //     self.inner.write(|inner| inner.update());
    //     app_context.request_redraw();
    //     self
    // }

    pub fn id(&self) -> usize {
        self.inner.lock().unwrap().id
    }
}

impl<T> Clone for SharedAnimation<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

pub(crate) trait SharedAnimationTrait {
    fn is_finished(&self) -> bool;
    fn update(&self);
}

impl<T: 'static> SharedAnimationTrait for SharedAnimation<T> {
    fn is_finished(&self) -> bool {
        self.inner.lock().unwrap().is_finished()
    }

    fn update(&self) {
        self.inner.lock().unwrap().update();
    }
}