use crate::core::generate_id;
use crate::shared::{Gettable, Observable, Removal, Settable, SharedAnimation};
use parking_lot::{Mutex, MutexGuard};
use std::fmt::Display;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Weak};

#[macro_export]
macro_rules! shared_un_send {
    (|$($observable:ident),*| $value_generator:block) => {
        {
            let observable = [$($observable.clone()),*];
            SharedUnSend::from_dynamic(&observable, move || $value_generator)
        }
    }
}

/// A trait for observing changes in a shared.
pub trait UnSendObservable {
    fn add_observer(&mut self, id: usize, observer: Box<dyn FnMut()>) -> Removal;
}

pub struct SharedUnSend<T> {
    id: usize,
    /// The value of the shared.
    value: Arc<Mutex<T>>,
    value_generator: Arc<Mutex<Option<Box<dyn Fn() -> T>>>>,
    /// A list of objects that observed by this shared.
    /// The first element of the tuple is the observable object, and the second element is the id of the observer.
    /// The id is used to remove the observer when the observable object is dropped.
    observed_objects:
        Arc<Mutex<Vec<(Option<Box<dyn UnSendObservable>>, Box<dyn FnOnce() + Send>)>>>,
    /// A list of simple observers. The key is the id of the observer.
    simple_observers: Arc<Mutex<Vec<(usize, Box<dyn FnMut() + Send>)>>>,
    /// A list of specific observers. The key is the id of the observer. The value is the observer function.
    /// The observer function takes a mutable reference to the shared value.
    specific_observers: Arc<Mutex<Vec<(usize, Box<dyn FnMut(&mut T) + Send>)>>>,
    animation: Arc<Mutex<Option<SharedAnimation<T>>>>,
}

impl<T: 'static> SharedUnSend<T> {
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

    pub fn from_dynamic<O: UnSendObservable + Clone + 'static>(
        o: &[O],
        value_generator: impl Fn() -> T + 'static,
    ) -> Self {
        let value_generator: Box<dyn Fn() -> T> = Box::new(value_generator);
        let value = value_generator();
        let value_generator = Some(value_generator);
        let mut shared = Self::inner_new(value, value_generator);
        for observable in o {
            shared.observe(observable.clone());
        }
        shared
    }

    /// Modifications made through this function will not send notifications to observers.
    /// If you want to notify observers after modifying the value, use the [`write`](SharedUnSend::write) method.
    pub fn lock(&self) -> MutexGuard<T> {
        self.value.lock()
    }

    pub fn read<R>(&self, mut operation: impl FnMut(&T) -> R) -> R {
        let value = self.value.lock();
        operation(value.deref())
    }

    /// Modifications made through this function will send notifications to observers.
    /// If you don't want to notify observers after modifying the value, use the [`value`](SharedUnSend::lock) method.
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
        self.clear_observed_objects();
        *self.value.lock() = value;
        *self.value_generator.lock() = None;
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
    pub fn set_dynamic(&self, value_generator: impl Fn() -> T + 'static) {
        self.clear_observed_objects();
        let value_generator = Box::new(value_generator);
        let value = (&value_generator)();
        *self.value.lock() = value;
        *self.value_generator.lock() = Some(value_generator);
        self.notify();
    }

    fn can_generate(&self) -> bool {
        self.value_generator.lock().is_some()
    }

    pub fn notify(&self) {
        if self.can_generate() {
            let value_generator = self.value_generator.lock();
            let mut value = self.value.lock();
            *value = (&value_generator.as_ref().unwrap())();
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

    pub fn add_specific_observer(
        &mut self,
        id: usize,
        observer: impl FnMut(&mut T) + Send + 'static,
    ) {
        self.specific_observers
            .lock()
            .push((id, Box::new(observer)));
    }

    fn observe<O: UnSendObservable + Clone + 'static>(&mut self, observable: O) {
        let mut observable = Box::new(observable);
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

    pub fn remove_observer(&mut self, id: usize) {
        self.simple_observers.lock().retain(|(i, _)| *i != id);
        self.specific_observers.lock().retain(|(i, _)| *i != id);
    }

    pub fn get_animation(&self) -> Option<SharedAnimation<T>> {
        self.animation.lock().as_ref().cloned()
    }

    pub fn weak(&self) -> WeakSharedUnSend<T> {
        WeakSharedUnSend {
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

impl<T> Clone for SharedUnSend<T> {
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

impl<T> Observable for SharedUnSend<T> {
    fn add_observer(&mut self, id: usize, observer: Box<dyn FnMut() + Send>) -> Removal {
        self.simple_observers.lock().push((id, observer));
        let simple_observers = self.simple_observers.clone();
        Removal::new(move || {
            simple_observers.lock().retain(|(i, _)| *i != id);
        })
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

impl<T: 'static> From<T> for SharedUnSend<T> {
    fn from(value: T) -> Self {
        Self::from_static(value)
    }
}

impl<T: 'static> Settable<T> for SharedUnSend<T> {
    fn set(&self, value: impl Into<T>) {
        self.set_static(value.into());
    }
}

// impl<T: 'static> Settable<Box<dyn Fn() -> T + Send>> for SharedUnSend<T> {
//     fn set(&self, value: Box<dyn Fn() -> T + Send>) {
//         self.set_dynamic(value);
//     }
// }

impl<T: Clone> Gettable<T> for SharedUnSend<T> {
    fn get(&self) -> T {
        self.value.lock().clone()
    }
}

impl<T: Clone> SharedUnSend<T> {
    fn into(self) -> T {
        self.get()
    }
}

impl<T: 'static> AsRef<SharedUnSend<T>> for SharedUnSend<T> {
    fn as_ref(&self) -> &SharedUnSend<T> {
        self
    }
}

impl<T: 'static> From<&SharedUnSend<T>> for SharedUnSend<T> {
    fn from(value: &SharedUnSend<T>) -> Self {
        value.clone()
    }
}

impl<T: 'static + PartialEq> PartialEq for SharedUnSend<T> {
    fn eq(&self, other: &Self) -> bool {
        self.read(|value| other.read(|other_value| value == other_value))
    }
}

impl<T: 'static + Display> Display for SharedUnSend<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.read(|value| value.fmt(f))
    }
}

impl<T: 'static + Default> Default for SharedUnSend<T> {
    fn default() -> Self {
        Self::from_static(Default::default())
    }
}

#[derive(Clone)]
pub struct WeakSharedUnSend<T> {
    id: usize,
    value: Weak<Mutex<T>>,
    value_generator: Weak<Mutex<Option<Box<dyn Fn() -> T>>>>,
    observed_objects:
        Weak<Mutex<Vec<(Option<Box<dyn UnSendObservable>>, Box<dyn FnOnce() + Send>)>>>,
    simple_observers: Weak<Mutex<Vec<(usize, Box<dyn FnMut() + Send>)>>>,
    specific_observers: Weak<Mutex<Vec<(usize, Box<dyn FnMut(&mut T) + Send>)>>>,
    animation: Weak<Mutex<Option<SharedAnimation<T>>>>,
}

impl<T: 'static> WeakSharedUnSend<T> {
    pub fn upgrade(&self) -> Option<SharedUnSend<T>> {
        let value = self.value.upgrade()?;
        let value_generator = self.value_generator.upgrade()?;
        let observed_objects = self.observed_objects.upgrade()?;
        let simple_observers = self.simple_observers.upgrade()?;
        let specific_observers = self.specific_observers.upgrade()?;
        let animation = self.animation.upgrade()?;
        Some(SharedUnSend {
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
