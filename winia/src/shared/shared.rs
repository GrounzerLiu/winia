use crate::core::{generate_id, RefClone};
use crate::ui::app::AppContext;
use std::fmt::Display;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex, MutexGuard, Weak};

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
        }
    }

    pub fn from_static(value: T) -> Self {
        Self::inner_new(value, None)
    }

    pub fn from_dynamic(value_generator: impl Fn() -> T + 'static) -> Self {
        let value_generator: Box<dyn Fn() -> T> = Box::new(value_generator);
        let value = value_generator();
        let value_generator = Some(value_generator);
        Self::inner_new(value, value_generator)
    }

    pub fn value(&self) -> MutexGuard<T> {
        self.value.lock().unwrap()
    }

    pub fn read<R>(&self, mut operation: impl FnMut(&T) -> R) -> R {
        let value = self.value.lock().unwrap();
        operation(value.deref())
    }

    pub fn write<R>(&self, mut operation: impl FnMut(&mut T) -> R) -> R {
        let mut value = self.value.lock().unwrap();
        let r = operation(value.deref_mut());
        self.notify_with_value(value.deref_mut());
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
        let mut value = self.value.lock().unwrap();
        self.notify_with_value(value.deref_mut());
    }

    pub fn notify_with_value(&self, value: &mut T) {
        if self.can_generate() {
            let value_generator = self.value_generator.lock().unwrap();
            *value = (&value_generator.as_ref().unwrap())();
        }

        for (_, observer) in self.simple_observers.lock().unwrap().iter_mut() {
            observer();
        }

        for (_, observer) in self.specific_observers.lock().unwrap().iter_mut() {
            observer(value);
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

    pub fn observe<O: Observable + RefClone + 'static>(&mut self, observable: impl AsRef<O>) {
        let mut observable = Box::new(observable.as_ref().ref_clone());
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

    pub fn weak(&self) -> WeakShared<T> {
        WeakShared {
            id: self.id,
            value: Arc::downgrade(&self.value),
            value_generator: Arc::downgrade(&self.value_generator),
            observed_objects: Arc::downgrade(&self.observed_objects),
            simple_observers: Arc::downgrade(&self.simple_observers),
            specific_observers: Arc::downgrade(&self.specific_observers),
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

impl<T: 'static> From<Box<dyn Fn() -> T>> for Shared<T> {
    fn from(value_generator: Box<dyn Fn() -> T>) -> Self {
        Self::from_dynamic(value_generator)
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

impl<T: 'static> RefClone for Shared<T> {
    fn ref_clone(&self) -> Self {
        Self {
            id: self.id,
            value: self.value.clone(),
            value_generator: self.value_generator.clone(),
            observed_objects: self.observed_objects.clone(),
            simple_observers: self.simple_observers.clone(),
            specific_observers: self.specific_observers.clone(),
        }
    }
}

impl<T: 'static> From<&Shared<T>> for Shared<T> {
    fn from(value: &Shared<T>) -> Self {
        value.ref_clone()
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

pub struct WeakShared<T> {
    id: usize,
    value: Weak<Mutex<T>>,
    value_generator: Weak<Mutex<Option<Box<dyn Fn() -> T>>>>,
    observed_objects: Weak<Mutex<Vec<(Option<Box<dyn Observable>>, Box<dyn FnOnce()>)>>>,
    simple_observers: Weak<Mutex<Vec<(usize, Box<dyn FnMut()>)>>>,
    specific_observers: Weak<Mutex<Vec<(usize, Box<dyn FnMut(&mut T)>)>>>,
}

impl<T: 'static> WeakShared<T> {
    pub fn upgrade(&self) -> Option<Shared<T>> {
        let value = self.value.upgrade()?;
        let value_generator = self.value_generator.upgrade()?;
        let observed_objects = self.observed_objects.upgrade()?;
        let simple_observers = self.simple_observers.upgrade()?;
        let specific_observers = self.specific_observers.upgrade()?;
        Some(Shared {
            id: self.id,
            value,
            value_generator,
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

impl<T> RefClone for WeakShared<T> {
    fn ref_clone(&self) -> Self {
        Self {
            id: self.id,
            value: self.value.clone(),
            value_generator: self.value_generator.clone(),
            observed_objects: self.observed_objects.clone(),
            simple_observers: self.simple_observers.clone(),
            specific_observers: self.specific_observers.clone(),
        }
    }
}


pub trait SharedAnimation{
    fn start(self, app_context: &AppContext);
    fn is_finished(&self) -> bool;
    fn update(&mut self);
}
