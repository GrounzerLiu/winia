use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex};

use lazy_static::lazy_static;


pub use bool_property::*;
pub use color_property::*;
pub use float_property::*;
pub use gravity_property::*;
pub use item_property::*;
pub use size_property::*;
pub use text_property::*;

mod float_property;
mod size_property;
mod color_property;
mod bool_property;
mod item_property;
mod alignment_property;
mod text_property;
mod gravity_property;

lazy_static!(
    pub(crate) static ref OBSERVABLE_ID: Mutex<usize> = Mutex::new(0);
);

pub(crate) fn get_observable_id() -> usize {
    let mut observable_id = OBSERVABLE_ID.lock().unwrap();
    *observable_id += 1;
    *observable_id
}

pub struct Observer {
    listener: Box<dyn FnMut()>,
    owner_id: usize,
}

impl Observer {
    pub fn new(listener: impl FnMut() + 'static, owner_id: usize) -> Self {
        Self {
            listener: Box::new(listener),
            owner_id,
        }
    }

    pub fn new_without_id(listener: impl FnMut() + 'static) -> Self {
        let listener = Box::new(listener);
        Self {
            listener,
            owner_id: 0,
        }
    }

    pub fn owner_id(&self) -> usize {
        self.owner_id
    }

    pub fn notify(&mut self) {
        (self.listener)();
    }
}

pub trait Observable {
    fn add_observer(&self, listener: Observer);
    fn remove_observer(&self, owner_id: usize);
    fn clear_observers(&self);
    fn notify(&self);
}

pub struct Property<T> {
    id: usize,
    value: T,
    value_generator: Option<Box<dyn Fn() -> T>>,
    observers: Arc<Mutex<Vec<Observer>>>,
    observed_properties: Vec<Box<dyn Observable>>,
}

impl<T> Property<T> {
    pub fn from_value(value: T) -> Self {
        Self {
            id: get_observable_id(),
            value,
            value_generator: None,
            observers: Arc::new(Mutex::new(Vec::new())),
            observed_properties: Vec::new(),
        }
    }

    pub fn from_generator(value_generator: Box<dyn Fn() -> T>) -> Self {
        Self {
            id: get_observable_id(),
            value: value_generator(),
            value_generator: Some(value_generator),
            observers: Arc::new(Mutex::new(Vec::new())),
            observed_properties: Vec::new(),
        }
    }

    pub fn get_id(&self) -> usize {
        self.id
    }

    pub fn add_observer(&mut self, observer: Observer) {
        self.observers.lock().unwrap().push(observer);
    }

    pub fn remove_observer(&mut self, owner_id: usize) {
        self.observers.lock().unwrap().retain(|observer| {
            observer.owner_id() != owner_id
        });
    }

    pub fn clear_observers(&mut self) {
        self.observers.lock().unwrap().clear();
    }

    pub fn notify_observers(&mut self) {
        self.observers.lock().unwrap().iter_mut().for_each(|observer| {
            observer.notify()
        });
    }


    pub fn set_generator(&mut self, value_generator: Box<dyn Fn() -> T>) {
        self.observed_properties.iter().for_each(|observable| {
            observable.remove_observer(self.id);
        });
        self.observed_properties.clear();
        self.value_generator = Some(value_generator);
        self.value = self.value_generator.as_ref().unwrap()();
        self.notify_observers();
    }

    pub fn set_value<U: Into<T>>(&mut self, value: U) {
        self.observed_properties.iter().for_each(|observable| {
            observable.remove_observer(self.id);
        });
        self.observed_properties.clear();

        let value = value.into();
        self.value_generator = None;
        self.value = value;
        self.notify_observers();
    }
}


impl<T> AsRef<T> for Property<T> {
    fn as_ref(&self) -> &T {
        &self.value
    }
}

impl<T> AsMut<T> for Property<T> {
    fn as_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

impl<T> Deref for Property<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T> DerefMut for Property<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

pub struct SharedProperty<T> {
    value: Arc<Mutex<Property<T>>>,
}

impl<T: 'static> SharedProperty<T> {

    pub fn from_value(value: T) -> Self {
        Self {
            value: Arc::new(Mutex::new(Property::from_value(value))),
        }
    }

    pub fn from_generator(value_generator: Box<dyn Fn() -> T>) -> Self {
        Self {
            value: Arc::new(Mutex::new(Property::from_generator(value_generator))),
        }
    }

    pub fn lock(&self) -> std::sync::MutexGuard<'_, Property<T>> {
        self.value.lock().unwrap()
    }

    pub fn observe<O: 'static + Observable + Clone>(&self, observable: &O) {
        let self_clone = self.clone();
        let mut value = self.value.lock().unwrap();
        value.observed_properties.push(Box::new(observable.clone()));
        let id = value.id;
        drop(value);
        observable.add_observer(Observer::new(move || {
            let mut value = self_clone.lock();
            if let Some(value_generator) = &value.value_generator {
                value.value = value_generator();
            }
            drop(value);
            self_clone.notify();
        }, id))
    }

    pub fn set_generator(&self, value_generator: Box<dyn Fn() -> T>) {
        self.value.lock().unwrap().set_generator(value_generator);
    }

    pub fn set_value<U: Into<T>>(&self, value: U) {
        self.value.lock().unwrap().set_value(value);
    }
}

impl<T: 'static + Observable> SharedProperty<T> {
    pub fn from_observable(observable: T) -> Self {
        let observers: Arc<Mutex<Vec<Observer>>> = Arc::new(Mutex::new(Vec::new()));
        let id = get_observable_id();
        let observers_clone = Arc::clone(&observers);
        observable.add_observer(Observer::new(move || {
            observers_clone.lock().unwrap().iter_mut().for_each(|observer| {
                observer.notify()
            });
        }, id));
        Self {
            value: Arc::new(Mutex::new(Property {
                id,
                value: observable,
                value_generator: None,
                observers,
                observed_properties: Vec::new(),
            })),
        }
    }
}

impl<T> Observable for SharedProperty<T> {
    fn add_observer(&self, listener: Observer) {
        self.value.lock().unwrap().add_observer(listener);
    }

    fn remove_observer(&self, owner_id: usize) {
        self.value.lock().unwrap().remove_observer(owner_id);
    }

    fn clear_observers(&self) {
        self.value.lock().unwrap().clear_observers();
    }

    fn notify(&self) {
        self.value.lock().unwrap().notify_observers();
    }
}

impl<T> Clone for SharedProperty<T> {
    fn clone(&self) -> Self {
        Self {
            value: Arc::clone(&self.value),
        }
    }
}

impl<T> Drop for SharedProperty<T> {
    fn drop(&mut self) {
        self.value.lock().unwrap().observed_properties.iter().for_each(|observable| {
            observable.remove_observer(self.value.lock().unwrap().id);
        });
    }
}

impl<T: Default + 'static> Default for SharedProperty<T> {
    fn default() -> Self {
        Self::from_value(T::default())
    }
}

impl<T: 'static> From<T> for SharedProperty<T> {
    fn from(value: T) -> Self {
        Self::from_value(value)
    }
}

impl<T: Clone + 'static> From<&T> for SharedProperty<T> {
    fn from(value: &T) -> Self {
        Self::from_value(value.clone())
    }
}

impl<T: 'static> From<Box<dyn Fn() -> T>> for SharedProperty<T> {
    fn from(value_generator: Box<dyn Fn() -> T>) -> Self {
        Self::from_generator(value_generator)
    }
}

pub trait Gettable<T: Clone> {
    fn get(&self) -> T;
}

impl<T: Clone> Gettable<T> for SharedProperty<T> {
    fn get(&self) -> T {
        self.value.lock().unwrap().value.clone()
    }
}


/*pub struct GeneratorProperty<T: Clone> {
    id: usize,
    value: Box<dyn Fn() -> T>,
    observers: Arc<Mutex<Vec<Observer>>>,
    observed_properties: Vec<Box<dyn Observable>>,
}

impl<T: Clone + 'static> GeneratorProperty<T> {
    pub fn new(value_generator: Box<dyn Fn() -> T>) -> Self {
        Self {
            id: get_observable_id(),
            value: value_generator,
            observers: Arc::new(Mutex::new(Vec::new())),
            observed_properties: Vec::new(),
        }
    }

    pub fn get_id(&self) -> usize {
        self.id
    }

    pub fn observe<O: 'static + Observable + Clone>(&mut self, observable: &O) {
        self.observed_properties.push(Box::new(observable.clone()));
        let observers = Arc::clone(&self.observers);
        observable.add_observer(Observer::new(Box::new(move || {
            observers.lock().unwrap().iter_mut().for_each(|observer| {
                observer.notify()
            });
        }), self.id));
    }

    pub fn set<U: Into<T>>(&mut self, value: U) {
        let value = value.into();
        self.observed_properties.iter_mut().for_each(|observable| {
            observable.remove_observer(self.id);
        });
        self.observed_properties.clear();
        let value = value.clone();
        self.value = Box::new(move || value.clone());
        self.notify_observers();
    }

    pub fn add_observer(&mut self, observer: Observer) {
        self.observers.lock().unwrap().push(observer);
    }

    pub fn remove_observer(&mut self, owner_id: usize) {
        self.observers.lock().unwrap().retain(|observer| {
            observer.owner_id() != owner_id
        });
    }

    pub fn notify_observers(&mut self) {
        self.observers.lock().unwrap().iter_mut().for_each(|observer| {
            observer.notify()
        });
    }
}

impl<T: Clone> Drop for GeneratorProperty<T> {
    fn drop(&mut self) {
        self.observed_properties.iter().for_each(|observable| {
            observable.remove_observer(self.id);
        });
    }
}

pub struct SharedGeneratorProperty<T: Clone> {
    value: Arc<Mutex<GeneratorProperty<T>>>,
}

impl<T: Clone + 'static> SharedGeneratorProperty<T> {
    pub fn from_generator(value_generator: Box<dyn Fn() -> T>) -> Self {
        Self {
            value: Arc::new(Mutex::new(GeneratorProperty::new(value_generator))),
        }
    }

    pub fn from_value(value: T) -> Self {
        let value = value.clone();
        Self {
            value: Arc::new(Mutex::new(GeneratorProperty::new(Box::new(move || value.clone())))),
        }
    }

    pub fn lock(&self) -> std::sync::MutexGuard<'_, GeneratorProperty<T>> {
        self.value.lock().unwrap()
    }

    pub fn value(&self) -> Arc<Mutex<GeneratorProperty<T>>> {
        Arc::clone(&self.value)
    }

    pub fn observe<O: 'static + Observable + Clone>(&self, observable: &O) {
        self.value.lock().unwrap().observe(observable);
    }

    pub fn set<U: Into<T>>(&self, value: U) {
        self.value.lock().unwrap().set(value);
    }

    pub fn get(&self) -> T {
        (self.value.lock().unwrap().value)()
    }

    fn get_id(&self) -> usize {
        self.value.lock().unwrap().get_id()
    }
}

impl<T: Clone + 'static> Observable for SharedGeneratorProperty<T> {
    fn add_observer(&self, observer: Observer) {
        self.value.lock().unwrap().add_observer(observer);
    }

    fn remove_observer(&self, owner_id: usize) {
        self.value.lock().unwrap().remove_observer(owner_id);
    }

    fn notify(&self) {
        self.value.lock().unwrap().observers.lock().unwrap().iter_mut().for_each(|observer| {
            observer.notify()
        });
    }
}

impl<T: Clone> Clone for SharedGeneratorProperty<T> {
    fn clone(&self) -> Self {
        Self {
            value: Arc::clone(&self.value),
        }
    }
}

impl<T: Clone + 'static> From<T> for SharedGeneratorProperty<T> {
    fn from(value: T) -> Self {
        Self::from_value(value)
    }
}

impl<T: Clone + 'static> From<&T> for SharedGeneratorProperty<T> {
    fn from(value: &T) -> Self {
        let value = value.clone();
        Self::from_generator(Box::new(move || value.clone()))
    }
}

impl<T: Clone + 'static> From<Box<dyn Fn() -> T>> for SharedGeneratorProperty<T> {
    fn from(value_generator: Box<dyn Fn() -> T>) -> Self {
        Self::from_generator(value_generator)
    }
}


pub struct WrapperProperty<T> {
    id: usize,
    value: T,
    observers: Arc<Mutex<Vec<Observer>>>,
    observed_properties: Vec<Box<dyn Observable>>,
}

impl<T> WrapperProperty<T> {
    pub fn new(value: T) -> Self {
        Self {
            id: get_observable_id(),
            value,
            observers: Arc::new(Mutex::new(Vec::new())),
            observed_properties: Vec::new(),
        }
    }

    pub fn get_id(&self) -> usize {
        self.id
    }

    pub fn observe<O: 'static + Observable + Clone>(&mut self, observable: &O) {
        self.observed_properties.push(Box::new(observable.clone()));
        let observers = Arc::clone(&self.observers);
        observable.add_observer(Observer::new(Box::new(move || {
            observers.lock().unwrap().iter_mut().for_each(|observer| {
                observer.notify()
            });
        }), self.id));
    }

    pub fn set<U: Into<T>>(&mut self, value: U) {
        let value = value.into();
        self.observed_properties.iter().for_each(|observable| {
            observable.remove_observer(self.id);
        });
        self.observed_properties.clear();
        self.value = value;
        self.notify_observers();
    }

    pub fn add_observer(&mut self, observer: Observer) {
        self.observers.lock().unwrap().push(observer);
    }

    pub fn remove_observer(&mut self, owner_id: usize) {
        self.observers.lock().unwrap().retain(|observer| {
            observer.owner_id() != owner_id
        });
    }

    pub fn notify_observers(&mut self) {
        self.observers.lock().unwrap().iter_mut().for_each(|observer| {
            observer.notify()
        });
    }
}

impl<T> Drop for WrapperProperty<T> {
    fn drop(&mut self) {
        self.observed_properties.iter().for_each(|observable| {
            observable.remove_observer(self.id);
        });
    }
}

impl<T> AsRef<T> for WrapperProperty<T> {
    fn as_ref(&self) -> &T {
        &self.value
    }
}

impl<T> AsMut<T> for WrapperProperty<T> {
    fn as_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

pub struct SharedWrapperProperty<T> {
    value: Arc<Mutex<WrapperProperty<T>>>,
}

impl<T> SharedWrapperProperty<T> {
    pub fn new(value: T) -> Self {
        Self {
            value: Arc::new(Mutex::new(WrapperProperty::new(value))),
        }
    }

    pub fn lock(&self) -> std::sync::MutexGuard<'_, WrapperProperty<T>> {
        self.value.lock().unwrap()
    }

    pub fn value(&self) -> Arc<Mutex<WrapperProperty<T>>> {
        Arc::clone(&self.value)
    }

    pub fn observe<O: 'static + Observable + Clone>(&self, observable: &O) {
        self.value.lock().unwrap().observe(observable);
    }

    pub fn set<U: Into<T>>(&self, value: U) {
        self.value.lock().unwrap().set(value);
    }
}

impl<T: Observable + 'static> SharedWrapperProperty<T> {
    pub fn from_observable(value: T) -> Self {
        let s = Self {
            value: Arc::new(Mutex::new(WrapperProperty::new(value))),
        };
        s.observe(&s);
        s
    }
    fn get_id(&self) -> usize {
        self.value.lock().unwrap().get_id()
    }
}

impl<T: 'static> Observable for SharedWrapperProperty<T> {
    fn add_observer(&self, observer: Observer) {
        self.value.lock().unwrap().add_observer(observer);
    }

    fn remove_observer(&self, owner_id: usize) {
        self.value.lock().unwrap().remove_observer(owner_id);
    }

    fn notify(&self) {
        self.value.lock().unwrap().observers.lock().unwrap().iter_mut().for_each(|mut observer| {
            observer.notify()
        });
    }
}

impl<T: 'static> Clone for SharedWrapperProperty<T> {
    fn clone(&self) -> Self {
        Self {
            value: Arc::clone(&self.value),
        }
    }
}

impl<T: Clone + 'static> From<&T> for SharedWrapperProperty<T> {
    fn from(value: &T) -> Self {
        let value = value.clone();
        Self::from(value)
    }
}

impl<T: 'static> From<T> for SharedWrapperProperty<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}*/