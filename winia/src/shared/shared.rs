use crate::core::next_id;
use crate::ui::animation::interpolator::{Interpolator, Linear};
use crate::ui::app::EventLoopProxy;
use parking_lot::{ArcMutexGuard, Mutex, MutexGuard, RawMutex};
use std::fmt::Display;
use std::future::Future;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

/// A trait for getting the value of a shared.
/// This function will return a specific value instead of `PropertyValue` which needs to be unwrapped.
pub trait Gettable<T: Clone> {
    fn get(&self) -> T;
}

/// A trait for setting the value of a shared.
/// So we can use the same function to set the static value and the dynamic value.
pub trait Settable<T> {
    fn set(&self, value: impl Into<T>);
}

/// A trait for observing changes in a shared.
pub trait Observable {
    fn add_observer(&mut self, id: usize, observer: Box<dyn FnMut() + Send>) -> Removal;
}

pub struct Removal {
    removal: Box<dyn FnOnce() + Send>,
}

impl Removal {
    pub fn new(removal: impl FnOnce() + Send + 'static) -> Self {
        Self {
            removal: Box::new(removal),
        }
    }
    pub fn drop(self) {}
    pub fn unwrap(self) -> Box<dyn FnOnce() + Send> {
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

pub struct SharedGuard<'a, T> {
    outer: MutexGuard<'a, Arc<Mutex<T>>>,
    inner: ArcMutexGuard<RawMutex, T>
}

impl<'a, T> SharedGuard<'a, T> {
    pub fn new(
        outer: MutexGuard<'a, Arc<Mutex<T>>>,
        inner: ArcMutexGuard<RawMutex, T>,
    ) -> Self {
        Self { outer, inner }
    }
}

impl<'a, T> Deref for SharedGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.inner.deref()
    }
}

impl<'a, T> DerefMut for SharedGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner.deref_mut()
    }
}

impl<'a, T> AsRef<T> for SharedGuard<'a, T> {
    fn as_ref(&self) -> &T {
        self.inner.deref()
    }
}

pub struct Shared<T> {
    id: usize,
    /// The value of the shared.
    is_from_shared: Arc<Mutex<bool>>,
    value: Arc<Mutex<Arc<Mutex<T>>>>,
    value_generator: Arc<Mutex<Option<Box<dyn Fn() -> T + Send>>>>,
    filter: Arc<Mutex<Vec<Box<dyn Fn(T) -> T + Send>>>>,
    /// A list of objects that observed by this shared.
    /// The first element of the tuple is the observable object, and the second element is the id of the observer.
    /// The id is used to remove the observer when the observable object is dropped.
    observed_objects:
        Arc<Mutex<Vec<(Option<Box<dyn Observable + Send>>, Box<dyn FnOnce() + Send>)>>>,
    /// A list of simple observers. The key is the id of the observer.
    simple_observers: Arc<Mutex<Vec<(usize, Box<dyn FnMut() + Send>)>>>,
    /// A list of specific observers. The key is the id of the observer. The value is the observer function.
    /// The observer function takes a mutable reference to the shared value.
    specific_observers: Arc<Mutex<Vec<(usize, Box<dyn FnMut(&mut T) + Send>)>>>,
    animation: Arc<Mutex<Option<SharedAnimation<T>>>>,
}

impl<T: Send + 'static> Shared<T> {
    fn inner_new(value: T, value_generator: Option<Box<dyn Fn() -> T + Send>>) -> Self {
        let value = Arc::new(Mutex::new(Arc::new(Mutex::new(value))));
        let value_generator = Arc::new(Mutex::new(value_generator));
        Self {
            id: next_id(),
            is_from_shared: Arc::new(Mutex::new(false)),
            value,
            value_generator,
            filter: Arc::new(Mutex::new(Vec::with_capacity(0))),
            observed_objects: Arc::new(Mutex::new(Vec::with_capacity(0))),
            simple_observers: Arc::new(Mutex::new(Vec::with_capacity(0))),
            specific_observers: Arc::new(Mutex::new(Vec::with_capacity(0))),
            animation: Arc::new(Mutex::new(None)),
        }
    }

    pub fn from_static(value: T) -> Self {
        Self::inner_new(value, None)
    }

    pub fn from_dynamic(
        o: Box<[Box<dyn Observable + Send + 'static>]>,
        value_generator: impl Fn() -> T + Send + 'static,
    ) -> Self {
        let value_generator: Box<dyn Fn() -> T + Send> = Box::new(value_generator);
        let value = value_generator();
        let value_generator = Some(value_generator);
        let shared = Self::inner_new(value, value_generator);
        for observable in o {
            shared.observe(observable);
        }
        shared
    }

    pub fn from_async(value: impl Future<Output = T> + Send + 'static, default_value: T) -> Self {
        let shared = Self::from_static(default_value);
        let shared_clone = shared.clone();
        tokio::spawn(async move {
            let value = value.await;
            shared_clone.set_static(value);
        });
        shared
    }

    /// Modifications made through this function will not send notifications to observers.
    /// If you want to notify observers after modifying the value, use the [`write`](Shared::write) method.
    pub fn lock(&self) -> SharedGuard<T> {
        let outer = self.value.lock();
        let inner = outer.lock_arc();
        SharedGuard::new(outer, inner)
    }

    pub fn try_lock(&self) -> Option<SharedGuard<T>> {
        if self.value.is_locked() {
            return None;
        }
        let outer = self.value.try_lock()?;
        let inner = outer.try_lock_arc()?;
        Some(SharedGuard::new(outer, inner))
    }

    pub fn is_locked(&self) -> bool {
        self.value.is_locked()
    }

    pub fn read<R>(&self, mut operation: impl FnMut(&T) -> R) -> R {
        let value = self.lock();
        operation(value.deref())
    }

    /// Modifications made through this function will send notifications to observers.
    /// If you don't want to notify observers after modifying the value, use the [`value`](Shared::lock) method.
    pub fn write<R>(&self, mut operation: impl FnMut(&mut T) -> R) -> R {
        let r = {
            let mut value = self.lock();
            operation(value.deref_mut())
        };
        self.notify();
        r
    }

    pub fn id(&self) -> usize {
        self.id
    }
    
    fn filter(&self, value: T) -> T {
        let filter = self.filter.lock();
        let mut value = value;
        for f in filter.iter() {
            value = f(value);
        }
        value
    }

    pub fn set_static(&self, value: T) {
        {
            self.clear_observed_objects();
            let filtered_value = self.filter(value);
            if *self.is_from_shared.lock() {
                let value = Arc::new(Mutex::new(filtered_value));
                *self.value.lock() = value;
                *self.is_from_shared.lock() = false;
            } else {
                *self.lock() = self.filter(filtered_value);
            }
            *self.value_generator.lock() = None;
        }
        self.notify();
    }

    pub fn try_set_static(&self, value: T) {
        if self.value.is_locked() {
            return;
        }
        self.set_static(value);
    }


    pub fn add_filter<F>(&self, filter: F)
    where
        F: Fn(T) -> T + Send + 'static,
    {
        self.filter.lock().push(Box::new(filter));
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
    // pub fn set_dynamic(&self, value_generator: impl Fn() -> T + Send + 'static) {
    //     self.clear_observed_objects();
    //     let value_generator = Box::new(value_generator);
    //     let value = (&value_generator)();
    //     *self.value.lock() = value;
    //     *self.value_generator.lock() = Some(value_generator);
    //     self.notify();
    // }
    pub fn set_dynamic(
        &self,
        o: Box<[Box<dyn Observable + Send + 'static>]>,
        value_generator: impl Fn() -> T + Send + 'static,
    ) {
        self.clear_observed_objects();
        let value_generator: Box<dyn Fn() -> T + Send> = Box::new(value_generator);
        *self.value_generator.lock() = Some(value_generator);
        self.notify();

        for observable in o {
            self.observe(observable);
        }
    }

    pub fn try_set_dynamic(
        &self,
        o: Box<[Box<dyn Observable + Send + 'static>]>,
        value_generator: impl Fn() -> T + Send + 'static,
    ) {
        if self.value.is_locked() {
            return;
        }
        self.set_dynamic(o, value_generator);
    }
    
    pub fn set_shared(&self, shared: impl Into<Shared<T>>) {
        *self.is_from_shared.lock() = true;
        let shared = shared.into();
        self.clear_observed_objects();
        *self.value.lock() = shared.value.lock().clone();
        *self.value_generator.lock() = None;
        self.observe(shared);
        self.notify();
    }

    fn can_generate(&self) -> bool {
        self.value_generator.lock().is_some()
    }

    pub fn notify(&self) {
        if self.can_generate() {
            let value_generator = self.value_generator.lock();
            let generated_value = value_generator.as_ref().unwrap()();
            let filtered_value = self.filter(generated_value);
            if *self.is_from_shared.lock() {
                let value = Arc::new(Mutex::new(filtered_value));
                *self.value.lock() = value;
                *self.is_from_shared.lock() = false;
            } else {
                *self.lock() = filtered_value;
            }
        }

        for (_, observer) in self.simple_observers.lock().iter_mut() {
            observer();
        }

        let mut value = self.lock();
        for (_, observer) in self.specific_observers.lock().iter_mut() {
            observer(&mut *value);
        }
    }

    fn clear_observed_objects(&self) {
        for (_, removal) in self.observed_objects.lock().drain(..) {
            removal();
        }
    }

    pub fn add_specific_observer(&self, id: usize, observer: impl FnMut(&mut T) + Send + 'static) {
        self.specific_observers
            .lock()
            .push((id, Box::new(observer)));
    }

    pub fn observe<O: Into<Box<dyn Observable + Send + 'static>>>(&self, observable: O) {
        let mut observable: Box<dyn Observable + Send> = observable.into();
        let self_weak = self.weak();
        let removal = observable
            .add_observer(
                self.id(),
                Box::new(move || {
                    if let Some(property) = self_weak.upgrade() {
                        property.notify();
                    }
                }),
            )
            .unwrap();
        self.observed_objects
            .lock()
            .push((Some(observable), removal));
    }

    pub fn to_observable(&self) -> Box<dyn Observable + Send> {
        let self_: Box<dyn Observable + Send> = Box::new(self.clone());
        self_
    }

    pub fn remove_observer(&self, id: usize) {
        self.simple_observers.lock().retain(|(i, _)| *i != id);
        self.specific_observers.lock().retain(|(i, _)| *i != id);
    }

    pub fn get_animation(&self) -> Option<SharedAnimation<T>> {
        self.animation.lock().as_ref().cloned()
    }

    pub fn weak(&self) -> WeakShared<T> {
        WeakShared {
            id: self.id,
            is_from_shared: Arc::downgrade(&self.is_from_shared),
            value: Arc::downgrade(&self.value),
            value_generator: Arc::downgrade(&self.value_generator),
            filter: Arc::downgrade(&self.filter),
            observed_objects: Arc::downgrade(&self.observed_objects),
            simple_observers: Arc::downgrade(&self.simple_observers),
            specific_observers: Arc::downgrade(&self.specific_observers),
            animation: Arc::downgrade(&self.animation),
        }
    }

    pub fn redraw_when_changed(mut self, event_loop_proxy: &EventLoopProxy, id: usize) -> Self {
        let event_loop_proxy = event_loop_proxy.clone();
        self.add_observer(
            id,
            Box::new(move || {
                event_loop_proxy.request_redraw();
            }),
        );
        self
    }

    pub fn layout_when_changed(mut self, event_loop_proxy: &EventLoopProxy, id: usize) -> Self {
        let event_loop_proxy = event_loop_proxy.clone();
        self.add_observer(
            id,
            Box::new(move || {
                event_loop_proxy.request_layout();
            }),
        );
        self
    }
}

/*impl<T: Send + Observable + 'static> Shared<T> {
    pub fn from_observable(
        observable: T
    ) -> Self {
        let self_ = Self::new(observable);
        {
            let self_weak = self_.weak();
            let mut observable = self_.lock();
            let removal = observable
                .add_observer(
                    self_.id(),
                    Box::new(move || {
                        if let Some(property) = self_weak.upgrade() {
                            property.notify();
                        }
                    }),
                );
            self_.observed_objects
                .lock()
                .push((None, removal.unwrap()));
        }
        self_
    }
}
*/
impl<T> Clone for Shared<T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            is_from_shared: self.is_from_shared.clone(),
            value: self.value.clone(),
            value_generator: self.value_generator.clone(),
            filter: self.filter.clone(),
            observed_objects: self.observed_objects.clone(),
            simple_observers: self.simple_observers.clone(),
            specific_observers: self.specific_observers.clone(),
            animation: self.animation.clone(),
        }
    }
}

impl<T: Send> Observable for Shared<T> {
    fn add_observer(&mut self, id: usize, observer: Box<dyn FnMut() + Send>) -> Removal {
        self.simple_observers.lock().push((id, observer));
        let simple_observers = self.simple_observers.clone();
        Removal {
            removal: Box::new(move || {
                simple_observers.lock().retain(|(i, _)| *i != id);
            }),
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

impl<T: Send + 'static> From<T> for Shared<T> {
    fn from(value: T) -> Self {
        Self::from_static(value)
    }
}

impl<T: Send + 'static> Settable<T> for Shared<T> {
    fn set(&self, value: impl Into<T>) {
        self.set_static(value.into());
    }
}

// impl<T: Send + 'static> Settable<Box<dyn Fn() -> T + Send>> for Shared<T> {
//     fn set(&self, value: Box<dyn Fn() -> T + Send>) {
//         self.set_dynamic(value);
//     }
// }

impl<T: Clone + Send + 'static> Gettable<T> for Shared<T> {
    fn get(&self) -> T {
        self.lock().clone()
    }
}

impl<T: Clone + Send + 'static> Shared<T> {
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

impl<T: Send + 'static + PartialEq> PartialEq for Shared<T> {
    fn eq(&self, other: &Self) -> bool {
        self.read(|value| other.read(|other_value| value == other_value))
    }
}

impl<T: Send + 'static + Display> Display for Shared<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.read(|value| value.fmt(f))
    }
}

impl<T: Send + 'static + Default> Default for Shared<T> {
    fn default() -> Self {
        Self::from_static(Default::default())
    }
}

impl<T: Send + 'static> Into<Box<dyn Observable + Send + 'static>> for Shared<T> {
    fn into(self) -> Box<dyn Observable + Send + 'static> {
        let self_: Box<dyn Observable + Send + 'static> = Box::new(self);
        self_
    }
}

impl<T: Send + 'static> Into<Box<dyn Observable + Send + 'static>> for &Shared<T> {
    fn into(self) -> Box<dyn Observable + Send + 'static> {
        let self_: Box<dyn Observable + Send + 'static> = Box::new(self.clone());
        self_
    }
}

#[derive(Clone)]
pub struct WeakShared<T> {
    id: usize,
    is_from_shared: Weak<Mutex<bool>>,
    value: Weak<Mutex<Arc<Mutex<T>>>>,
    value_generator: Weak<Mutex<Option<Box<dyn Fn() -> T + Send>>>>,
    filter: Weak<Mutex<Vec<Box<dyn Fn(T) -> T + Send>>>>,
    observed_objects:
        Weak<Mutex<Vec<(Option<Box<dyn Observable + Send>>, Box<dyn FnOnce() + Send>)>>>,
    simple_observers: Weak<Mutex<Vec<(usize, Box<dyn FnMut() + Send>)>>>,
    specific_observers: Weak<Mutex<Vec<(usize, Box<dyn FnMut(&mut T) + Send>)>>>,
    animation: Weak<Mutex<Option<SharedAnimation<T>>>>,
}

impl<T: Send + 'static> WeakShared<T> {
    pub fn upgrade(&self) -> Option<Shared<T>> {
        let is_from_shared = self.is_from_shared.upgrade()?;
        let value = self.value.upgrade()?;
        let value_generator = self.value_generator.upgrade()?;
        let filter = self.filter.upgrade()?;
        let observed_objects = self.observed_objects.upgrade()?;
        let simple_observers = self.simple_observers.upgrade()?;
        let specific_observers = self.specific_observers.upgrade()?;
        let animation = self.animation.upgrade()?;
        Some(Shared {
            id: self.id,
            is_from_shared,
            value,
            value_generator,
            filter,
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
    enable_repeat: bool,
    shared: WeakShared<T>,
    from: T,
    to: T,
    value_generator: Box<dyn Fn(&T, &T, f32) -> T + Send>,
    duration: Duration,
    start_time: Instant,
    interpolator: Box<dyn Interpolator + Send>,
    on_start: Option<Box<dyn FnMut() + Send>>,
    on_finish: Option<Box<dyn FnMut() + Send>>,
}

impl<T: Send + 'static> InnerSharedAnimation<T> {
    pub fn new(
        f32: Shared<T>,
        from: T,
        to: T,
        value_generator: impl Fn(&T, &T, f32) -> T + Send + 'static,
    ) -> Self {
        Self {
            id: next_id(),
            is_finished: false,
            enable_repeat: false,
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

    pub fn enable_repeat(&mut self) {
        self.enable_repeat = true;
    }

    pub fn duration(&mut self, duration: Duration) {
        self.duration = duration;
    }

    pub fn interpolator(&mut self, interpolator: impl Interpolator + Send + 'static) {
        self.interpolator = Box::new(interpolator);
    }

    pub fn on_start(&mut self, on_start: impl FnMut() + Send + 'static) {
        self.on_start = Some(Box::new(on_start));
    }

    /// Set the function to be called when the animation is finished or stopped.
    pub fn on_finish(&mut self, on_finish: impl FnMut() + Send + 'static) {
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

    pub fn is_finished(&mut self) -> bool {
        if self.enable_repeat {
            if self.is_finished {
                true
            } else if self.start_time.elapsed() >= self.duration {
                self.start_time = Instant::now();
                false
            } else {
                false
            }
        } else {
            self.is_finished || self.start_time.elapsed() >= self.duration
        }
    }

    pub fn update(&mut self) {
        if self.is_finished() {
            let new_value = (self.value_generator)(&self.from, &self.to, 1.0);
            if let Some(shared) = self.shared.upgrade() {
                shared.set(new_value);
            }
            if let Some(on_finish) = self.on_finish.as_mut() {
                on_finish();
            }
            return;
        }
        let time_elapsed = self.start_time.elapsed().as_millis() as f32;
        let progress = (time_elapsed / self.duration.as_millis() as f32).clamp(0.0, 1.0);
        let interpolated = self.interpolator.interpolate(progress);
        let new_value = (self.value_generator)(&self.from, &self.to, interpolated);
        if let Some(shared) = self.shared.upgrade() {
            shared.set(new_value);
        }
    }
}

pub struct SharedAnimation<T> {
    inner: Arc<Mutex<InnerSharedAnimation<T>>>,
}

impl<T: Send + 'static> SharedAnimation<T> {
    pub fn new(
        f32: Shared<T>,
        from: T,
        to: T,
        value_generator: impl Fn(&T, &T, f32) -> T + Send + 'static,
    ) -> Self {
        let inner = InnerSharedAnimation::new(f32, from, to, value_generator);
        Self {
            inner: Arc::new(Mutex::new(inner)),
        }
    }

    pub fn enable_repeat(self) -> Self {
        self.inner.lock().enable_repeat();
        self
    }

    pub fn duration(self, duration: Duration) -> Self {
        self.inner.lock().duration = duration;
        self
    }

    pub fn interpolator(self, interpolator: impl Interpolator + Send + 'static) -> Self {
        self.inner.lock().interpolator(interpolator);
        self
    }

    pub fn on_start(self, on_start: impl FnMut() + Send + 'static) -> Self {
        self.inner.lock().on_start(on_start);
        self
    }

    pub fn on_finish(self, on_finish: impl FnMut() + Send + 'static) -> Self {
        self.inner.lock().on_finish(on_finish);
        self
    }

    pub fn start(self, event_loop_proxy: &EventLoopProxy) -> Self {
        {
            let mut inner = self.inner.lock();
            inner.start_time = Instant::now();
            event_loop_proxy.start_shared_animation(Box::new(self.clone()));
            let cloned = self.clone();
            if let Some(shared) = inner.shared.upgrade() {
                shared.animation.lock().replace(cloned);
            }
            if let Some(mut on_start) = inner.on_start.take() {
                on_start();
            }
        }
        self
    }

    pub fn start_delayed(self, event_loop_proxy: &EventLoopProxy, delay: Duration) -> Self {
        let event_loop_proxy = event_loop_proxy.clone();
        let self_clone = self.clone();
        tokio::spawn(
            async move {
                tokio::time::sleep(delay).await;
                self_clone.start(&event_loop_proxy);
            }
        );
        self
    }

    pub fn cancel(&mut self) {
        self.inner.lock().on_finish.take();
        self.stop()
    }

    pub fn stop(&mut self) {
        self.inner.lock().stop();
    }

    pub fn id(&self) -> usize {
        self.inner.lock().id
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

impl<T: Send + 'static> SharedAnimationTrait for SharedAnimation<T> {
    fn is_finished(&self) -> bool {
        self.inner.lock().is_finished()
    }

    fn update(&self) {
        self.inner.lock().update();
    }
}

#[macro_export]
macro_rules! observables {
    ($($observable:expr),* $(,)?) => {
        [
            $(
                $observable.to_observable()
            )*
        ].into()
    }
}

#[macro_export]
macro_rules! bind {
    ($a:ident, $b:ident, $a2b:expr, $b2a:expr) => {
        {
            let a_id = $a.id();
            let b_id = $b.id();
            $a.add_specific_observer(b_id, {
                let b = $b.clone();
                move |value| {
                    if let Some(value) = $a2b(value) {
                        b.try_set_static(value);
                    }
                }
            });
            $b.add_specific_observer(a_id, {
                let a = $a.clone();
                move |value| {
                    if let Some(value) = $b2a(value) {
                        a.try_set_static(value);
                    }
                }
            });
        }
    };
}