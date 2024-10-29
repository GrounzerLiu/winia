use std::ops::DerefMut;
use std::sync::{Arc, Mutex, MutexGuard, Weak};

use crate::core::{generate_id, RefClone};

/// A trait for getting the value of a property.
/// This function will return a specific value instead of `PropertyValue` which needs to be unwrapped.
pub trait Gettable<T: Clone> {
    fn get(&self) -> T;
}

/// A trait for setting the value of a property.
/// So we can use the same function to set the static value and the dynamic value.
pub trait Settable<T: ?Sized> {
    fn set(&mut self, value: T);
}

/// A trait for observing changes in a property.
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

pub struct Property<T> {
    id: usize,
    /// The value of the property.
    value: Arc<Mutex<T>>,
    value_generator: Arc<Mutex<Option<Box<dyn Fn() -> T>>>>,
    /// A list of objects that observed by this property.
    /// The first element of the tuple is the observable object, and the second element is the id of the observer.
    /// The id is used to remove the observer when the observable object is dropped.
    observed_objects: Arc<Mutex<Vec<(Option<Box<dyn Observable>>, Box<dyn FnOnce()>)>>>,
    /// A list of simple observers. The key is the id of the observer.
    simple_observers: Arc<Mutex<Vec<(usize, Box<dyn FnMut()>)>>>,
    /// A list of specific observers. The key is the id of the observer. The value is the observer function.
    /// The observer function takes a mutable reference to the property value.
    specific_observers: Arc<Mutex<Vec<(usize, Box<dyn FnMut(&mut T)>)>>>,
}

impl<T: 'static> Property<T> {
    fn new(value: T, value_generator: Option<Box<dyn Fn() -> T>>) -> Self {
        let value = Arc::new(Mutex::new(value));
        let value_generator = Arc::new(Mutex::new(value_generator));
        Self {
            id: generate_id(),
            value,
            value_generator,
            observed_objects: Arc::new(Mutex::new(Vec::new())),
            simple_observers: Arc::new(Mutex::new(Vec::new())),
            specific_observers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn from_static(value: T) -> Self {
        Self::new(value, None)
    }

    pub fn from_dynamic(value_generator: impl Fn() -> T + 'static) -> Self {
        let value_generator: Box<dyn Fn() -> T> = Box::new(value_generator);
        let value = value_generator();
        let value_generator = Some(value_generator);
        Self::new(value, value_generator)
    }

    pub fn value(&self) -> MutexGuard<T> {
        self.value.lock().unwrap()
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

    pub fn notify(&mut self) {
        let mut value = self.value.lock().unwrap();
        if self.can_generate() {
            let value_generator = self.value_generator.lock().unwrap();
            *value = (&value_generator.as_ref().unwrap())();
        }

        for (_, observer) in self.simple_observers.lock().unwrap().iter_mut() {
            observer();
        }

        for (_, observer) in self.specific_observers.lock().unwrap().iter_mut() {
            observer(value.deref_mut());
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

    pub fn weak(&self) -> PropertyWeak<T> {
        PropertyWeak {
            id: self.id,
            value: Arc::downgrade(&self.value),
            value_generator: Arc::downgrade(&self.value_generator),
            observed_objects: Arc::downgrade(&self.observed_objects),
            simple_observers: Arc::downgrade(&self.simple_observers),
            specific_observers: Arc::downgrade(&self.specific_observers),
        }
    }
}

impl<T: Observable + 'static> Property<T> {
    pub fn from_observable(mut observable: T) -> Self {
        let id = generate_id();
        let simple_observers: Arc<Mutex<Vec<(usize, Box<dyn FnMut()>)>>> = Arc::new(Mutex::new(Vec::new()));
        let simple_observers_clone = simple_observers.clone();

        let mut observed_objects = Vec::new();

        let removal = observable.add_observer(id, Box::new(move || {
            for (_, observer) in simple_observers_clone.lock().unwrap().iter_mut() {
                observer();
            }
        })).unwrap();

        observed_objects.push((None, removal));

        let value = Arc::new(Mutex::new(observable));
        Self {
            id,
            value,
            value_generator: Arc::new(Mutex::new(None)),
            observed_objects: Arc::new(Mutex::new(observed_objects)),
            simple_observers,
            specific_observers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn set_observable(&mut self, mut observable: T) {
        self.clear_observed_objects();
        let simple_observers = self.simple_observers.clone();
        let simple_observers_clone = simple_observers.clone();

        let mut observed_objects = Vec::new();

        let removal = observable.add_observer(self.id(), Box::new(move || {
            for (_, observer) in simple_observers_clone.lock().unwrap().iter_mut() {
                observer();
            }
        })).unwrap();

        observed_objects.push((None, removal));

        *self.value.lock().unwrap() = observable;
        *self.observed_objects.lock().unwrap() = observed_objects;

        self.notify();
    }
}

impl<T> Observable for Property<T> {
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


impl<T: 'static> From<T> for Property<T> {
    fn from(value: T) -> Self {
        Self::from_static(value)
    }
}

impl<T: 'static> From<Box<dyn Fn() -> T>> for Property<T> {
    fn from(value_generator: Box<dyn Fn() -> T>) -> Self {
        Self::from_dynamic(value_generator)
    }
}

impl<T: 'static> Settable<T> for Property<T> {
    fn set(&mut self, value: T) {
        self.set_static(value);
    }
}

impl<T: 'static> Settable<Box<dyn Fn() -> T>> for Property<T> {
    fn set(&mut self, value: Box<dyn Fn() -> T>) {
        self.set_dynamic(value);
    }
}

impl<T: Clone> Gettable<T> for Property<T> {
    fn get(&self) -> T {
        self.value.lock().unwrap().clone()
    }
}

impl<T: Clone> Property<T> {
    fn into(self) -> T {
        self.get()
    }
}

impl<T: 'static> AsRef<Property<T>> for Property<T> {
    fn as_ref(&self) -> &Property<T> {
        self
    }
}

impl<T: 'static> RefClone for Property<T> {
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

impl<T: 'static> From<&Property<T>> for Property<T> {
    fn from(value: &Property<T>) -> Self {
        value.ref_clone()
    }
}

pub struct PropertyWeak<T> {
    id: usize,
    value: Weak<Mutex<T>>,
    value_generator: Weak<Mutex<Option<Box<dyn Fn() -> T>>>>,
    observed_objects: Weak<Mutex<Vec<(Option<Box<dyn Observable>>, Box<dyn FnOnce()>)>>>,
    simple_observers: Weak<Mutex<Vec<(usize, Box<dyn FnMut()>)>>>,
    specific_observers: Weak<Mutex<Vec<(usize, Box<dyn FnMut(&mut T)>)>>>,
}

impl<T> PropertyWeak<T> {
    pub fn upgrade(&self) -> Option<Property<T>> {
        let value = self.value.upgrade()?;
        let value_generator = self.value_generator.upgrade()?;
        let observed_objects = self.observed_objects.upgrade()?;
        let simple_observers = self.simple_observers.upgrade()?;
        let specific_observers = self.specific_observers.upgrade()?;
        Some(Property {
            id: self.id,
            value,
            value_generator,
            observed_objects,
            simple_observers,
            specific_observers,
        })
    }
}

impl<T> RefClone for PropertyWeak<T> {
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