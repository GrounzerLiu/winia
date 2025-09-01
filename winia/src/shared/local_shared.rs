use crate::core::next_id;
use crate::shared::{Gettable, Settable};
use crate::ui::app::EventLoopProxy;
use parking_lot::{Mutex, MutexGuard};
use std::fmt::Display;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Weak};

pub struct LocalRemoval {
    removal: Box<dyn FnOnce()>,
}

impl LocalRemoval {
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

pub trait LocalObservable {
    fn add_observer(&mut self, id: usize, observer: Box<dyn FnMut()>) -> LocalRemoval;
}

pub struct LocalShared<T> {
    id: usize,
    /// The value of the shared.
    value: Arc<Mutex<T>>,
    value_generator: Arc<Mutex<Option<Box<dyn Fn() -> T>>>>,
    filter: Arc<Mutex<Option<Box<dyn Fn(T) -> Option<T>>>>>,
    /// A list of objects that observed by this shared.
    /// The first element of the tuple is the observable object, and the second element is the id of the observer.
    /// The id is used to remove the observer when the observable object is dropped.
    observed_objects:
        Arc<Mutex<Vec<(Option<Box<dyn LocalObservable>>, Box<dyn FnOnce()>)>>>,
    /// A list of simple observers. The key is the id of the observer.
    simple_observers: Arc<Mutex<Vec<(usize, Box<dyn FnMut()>)>>>,
    /// A list of specific observers. The key is the id of the observer. The value is the observer function.
    /// The observer function takes a mutable reference to the shared value.
    specific_observers: Arc<Mutex<Vec<(usize, Box<dyn FnMut(&mut T)>)>>>,
}

impl<T: 'static> LocalShared<T> {
    fn inner_new(value: T, value_generator: Option<Box<dyn Fn() -> T>>) -> Self {
        let value = Arc::new(Mutex::new(value));
        let value_generator = Arc::new(Mutex::new(value_generator));
        Self {
            id: next_id(),
            value,
            value_generator,
            filter: Arc::new(Mutex::new(None)),
            observed_objects: Arc::new(Mutex::new(Vec::with_capacity(0))),
            simple_observers: Arc::new(Mutex::new(Vec::with_capacity(0))),
            specific_observers: Arc::new(Mutex::new(Vec::with_capacity(0))),
        }
    }

    pub fn from_static(value: T) -> Self {
        Self::inner_new(value, None)
    }

    pub fn from_dynamic(
        o: Box<[Box<dyn LocalObservable + 'static>]>,
        value_generator: impl Fn() -> T + 'static,
    ) -> Self {
        let value_generator: Box<dyn Fn() -> T> = Box::new(value_generator);
        let value = value_generator();
        let value_generator = Some(value_generator);
        let shared = Self::inner_new(value, value_generator);
        for observable in o {
            shared.observe(observable);
        }
        shared
    }

    /// Modifications made through this function will not send notifications to observers.
    /// If you want to notify observers after modifying the value, use the [`write`](LocalShared::write) method.
    pub fn lock(&self) -> MutexGuard<T> {
        self.value.lock()
    }

    pub fn try_lock(&self) -> Option<MutexGuard<T>> {
        self.value.try_lock()
    }

    pub fn is_locked(&self) -> bool {
        self.value.is_locked()
    }

    pub fn read<R>(&self, mut operation: impl FnMut(&T) -> R) -> R {
        let value = self.value.lock();
        operation(value.deref())
    }

    /// Modifications made through this function will send notifications to observers.
    /// If you don't want to notify observers after modifying the value, use the [`value`](LocalShared::lock) method.
    pub fn write<R>(&self, mut operation: impl FnMut(&mut T) -> R) -> R {
        let r = {
            let mut value = self.value.lock();
            operation(value.deref_mut())
        };
        self.notify();
        r
    }

    pub fn id(&self) -> usize {
        self.id
    }

    pub fn set_static(&self, value: T) {
        {
            let filter = self.filter.lock();
            let value = if let Some(filter) = filter.deref() {
                if let Some(value) = filter(value) {
                    value
                } else {
                    return;
                }
            } else {
                value
            };
            self.clear_observed_objects();
            *self.value.lock() = value;
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

    pub fn set_filter(&self, filter: impl Fn(T) -> Option<T> + 'static) {
        *self.filter.lock() = Some(Box::new(filter));
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
    pub fn set_dynamic(
        &self,
        o: Box<[Box<dyn LocalObservable + 'static>]>,
        value_generator: impl Fn() -> T + 'static,
    ) {
        self.clear_observed_objects();
        let value_generator: Box<dyn Fn() -> T> = Box::new(value_generator);
        *self.value_generator.lock() = Some(value_generator);
        self.notify();

        for observable in o {
            self.observe(observable);
        }
    }

    pub fn try_set_dynamic(
        &self,
        o: Box<[Box<dyn LocalObservable + 'static>]>,
        value_generator: impl Fn() -> T + 'static,
    ) {
        if self.value.is_locked() {
            return;
        }
        self.set_dynamic(o, value_generator);
    }

    fn can_generate(&self) -> bool {
        self.value_generator.lock().is_some()
    }

    pub fn notify(&self) {
        if self.can_generate() {
            let value_generator = self.value_generator.lock();
            let mut value = self.value.lock();
            let generated_value = value_generator.as_ref().unwrap()();
            let filter = self.filter.lock();
            let new_value = if let Some(filter) = filter.deref() {
                if let Some(value) = filter(generated_value) {
                    value
                } else {
                    return;
                }
            } else {
                generated_value
            };
            *value = new_value;
        }

        for (_, observer) in self.simple_observers.lock().iter_mut() {
            observer();
        }

        let mut value = self.value.lock();
        for (_, observer) in self.specific_observers.lock().iter_mut() {
            observer(&mut *value);
        }
    }

    fn clear_observed_objects(&self) {
        for (_, removal) in self.observed_objects.lock().drain(..) {
            removal();
        }
    }

    pub fn add_specific_observer(&self, id: usize, observer: impl FnMut(&mut T) + 'static) {
        self.specific_observers
            .lock()
            .push((id, Box::new(observer)));
    }

    fn observe<O: Into<Box<dyn LocalObservable + 'static>>>(&self, observable: O) {
        let mut observable: Box<dyn LocalObservable> = observable.into();
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

    pub fn to_observable(&self) -> Box<dyn LocalObservable> {
        let self_: Box<dyn LocalObservable> = Box::new(self.clone());
        self_
    }

    pub fn remove_observer(&self, id: usize) {
        self.simple_observers.lock().retain(|(i, _)| *i != id);
        self.specific_observers.lock().retain(|(i, _)| *i != id);
    }

    pub fn weak(&self) -> WeakLocalShared<T> {
        WeakLocalShared {
            id: self.id,
            value: Arc::downgrade(&self.value),
            value_generator: Arc::downgrade(&self.value_generator),
            filter: Arc::downgrade(&self.filter),
            observed_objects: Arc::downgrade(&self.observed_objects),
            simple_observers: Arc::downgrade(&self.simple_observers),
            specific_observers: Arc::downgrade(&self.specific_observers),
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

impl<T> Clone for LocalShared<T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            value: self.value.clone(),
            value_generator: self.value_generator.clone(),
            filter: self.filter.clone(),
            observed_objects: self.observed_objects.clone(),
            simple_observers: self.simple_observers.clone(),
            specific_observers: self.specific_observers.clone(),
        }
    }
}

impl<T> LocalObservable for LocalShared<T> {
    fn add_observer(&mut self, id: usize, observer: Box<dyn FnMut()>) -> LocalRemoval {
        self.simple_observers.lock().push((id, observer));
        let simple_observers = self.simple_observers.clone();
        LocalRemoval::new(Box::new(move || {
                simple_observers.lock().retain(|(i, _)| *i != id);
            }
        ))
    }
}

impl<T: 'static> From<T> for LocalShared<T> {
    fn from(value: T) -> Self {
        Self::from_static(value)
    }
}

impl<T: 'static> Settable<T> for LocalShared<T> {
    fn set(&self, value: impl Into<T>) {
        self.set_static(value.into());
    }
}

impl<T: Clone> Gettable<T> for LocalShared<T> {
    fn get(&self) -> T {
        self.value.lock().clone()
    }
}

impl<T: Clone> LocalShared<T> {
    fn into(self) -> T {
        self.get()
    }
}

impl<T: 'static> AsRef<LocalShared<T>> for LocalShared<T> {
    fn as_ref(&self) -> &LocalShared<T> {
        self
    }
}

impl<T: 'static> From<&LocalShared<T>> for LocalShared<T> {
    fn from(value: &LocalShared<T>) -> Self {
        value.clone()
    }
}

impl<T: 'static + PartialEq> PartialEq for LocalShared<T> {
    fn eq(&self, other: &Self) -> bool {
        self.read(|value| other.read(|other_value| value == other_value))
    }
}

impl<T: 'static + Display> Display for LocalShared<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.read(|value| value.fmt(f))
    }
}

impl<T: 'static + Default> Default for LocalShared<T> {
    fn default() -> Self {
        Self::from_static(Default::default())
    }
}

impl<T: 'static> Into<Box<dyn LocalObservable + 'static>> for LocalShared<T> {
    fn into(self) -> Box<dyn LocalObservable + 'static> {
        let self_: Box<dyn LocalObservable + 'static> = Box::new(self);
        self_
    }
}

impl<T: 'static> Into<Box<dyn LocalObservable + 'static>> for &LocalShared<T> {
    fn into(self) -> Box<dyn LocalObservable + 'static> {
        let self_: Box<dyn LocalObservable + 'static> = Box::new(self.clone());
        self_
    }
}

#[derive(Clone)]
pub struct WeakLocalShared<T> {
    id: usize,
    value: Weak<Mutex<T>>,
    value_generator: Weak<Mutex<Option<Box<dyn Fn() -> T>>>>,
    filter: Weak<Mutex<Option<Box<dyn Fn(T) -> Option<T>>>>>,
    observed_objects:
        Weak<Mutex<Vec<(Option<Box<dyn LocalObservable>>, Box<dyn FnOnce()>)>>>,
    simple_observers: Weak<Mutex<Vec<(usize, Box<dyn FnMut()>)>>>,
    specific_observers: Weak<Mutex<Vec<(usize, Box<dyn FnMut(&mut T)>)>>>,
}

impl<T: 'static> WeakLocalShared<T> {
    pub fn upgrade(&self) -> Option<LocalShared<T>> {
        let value = self.value.upgrade()?;
        let value_generator = self.value_generator.upgrade()?;
        let filter = self.filter.upgrade()?;
        let observed_objects = self.observed_objects.upgrade()?;
        let simple_observers = self.simple_observers.upgrade()?;
        let specific_observers = self.specific_observers.upgrade()?;
        Some(LocalShared {
            id: self.id,
            value,
            value_generator,
            filter,
            observed_objects,
            simple_observers,
            specific_observers,
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