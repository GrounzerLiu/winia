use parking_lot::lock_api::MutexGuard;
use parking_lot::{Mutex, RawMutex};
use std::collections::HashMap;
use std::marker::PhantomData;
use std::ops::Deref;
use std::sync::{Arc, Weak};
use crate::core::next_id;

pub trait Readable: Send + Sync + Sized + 'static {}
pub trait Writable: Readable {}
pub struct Derived;
impl Readable for Derived {}
pub struct Source;
impl Readable for Source {}
impl Writable for Source {}

pub struct Shared<T, Access: Readable> {
    _access_marker: PhantomData<Access>,
    id: u32,
    value: Arc<Mutex<T>>,
    generator: Arc<Mutex<Option<Box<dyn Fn() -> T + Send>>>>,
    observers: Arc<Mutex<HashMap<u32, Box<dyn Fn() + Send>>>>,
    dependencies: Arc<Mutex<Vec<Weak<Mutex<HashMap<u32, Box<dyn Fn() + Send>>>>>>>,
}

pub type SharedSource<T> = Shared<T, Source>;
pub type SharedDerived<T> = Shared<T, Derived>;

impl<T, A: Readable> Clone for Shared<T, A> {
    fn clone(&self) -> Self {
        Shared {
            _access_marker: Default::default(),
            id: self.id,
            value: self.value.clone(),
            generator: self.generator.clone(),
            observers: self.observers.clone(),
            dependencies: self.dependencies.clone(),
        }
    }
}

impl<T> Shared<T, Source>
where
    T: Send + Sync + 'static,
{
    pub fn new(value: T) -> Self {
        Shared::<T, Source> {
            _access_marker: Default::default(),
            id: next_id(),
            value: Arc::new(Mutex::new(value)),
            generator: Arc::new(Mutex::new(None)),
            observers: Arc::new(Mutex::new(HashMap::new())),
            dependencies: Arc::new(Mutex::new(Vec::new())),
        }
    }
}
impl<T, A> Shared<T, A>
where
    T: Send + Sync + 'static,
    A: Writable + Readable,
{
    pub fn set(&self, new_value: T) {
        let mut value = self.value.lock();
        *value = new_value;
        drop(value);
        self.notify();
    }

    pub fn write<R>(&self, func: impl FnOnce(&mut T) -> R) -> R {
        let mut value = self.value.lock();
        let r = func(&mut value);
        drop(value);
        self.notify();
        r
    }

    pub fn notify(&self) {
        let observers = self.observers.lock();
        for observer in observers.values() {
            observer();
        }
    }
}

#[macro_export]
macro_rules! depend {
    ($($dep:expr),* $(,)?) => {
        {
            use $crate::shared::Observable;
            vec![
                $(
                    {
                        let o: Box<dyn Observable> = Box::new($dep.clone());
                        o
                    }
                ),*
            ]
        }
    }
}

impl<T> Shared<T, Derived>
where
    T: Send + Sync + 'static,
{
    pub fn from_fn<F>(dependencies: Vec<Box<dyn Observable>>, generator: F) -> Self
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        let shared = Shared::<T, Derived> {
            _access_marker: Default::default(),
            id: next_id(),
            value: Arc::new(Mutex::new(generator())),
            generator: Arc::new(Mutex::new(Some(Box::new(generator)))),
            observers: Arc::new(Mutex::new(HashMap::new())),
            dependencies: Arc::new(Mutex::new(Vec::new())),
        };
        for dependency in dependencies {
            shared.depends_on(dependency.deref());
        }
        shared
    }
}

pub struct SharedReadGuard<'a, T> {
    guard: MutexGuard<'a, RawMutex, T>,
}
impl <'a, T> Deref for SharedReadGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &*self.guard
    }
}
impl<T, A> Shared<T, A>
where
    T: Send + Sync + 'static,
    A: Readable,
{
    pub fn read(&self) -> SharedReadGuard<'_, T> {
        SharedReadGuard {
            guard: self.value.lock(),
        }
    }

    pub fn subscribe(&self, observer_id: u32, callback: Box<dyn Fn() + Send>) {
        self.observers.lock().insert(observer_id, callback);
    }

    pub fn depends_on(&self, other: &(impl Observable + ?Sized)) {
        other.observers().lock().insert(self.id, {
            let observers_weak = Arc::downgrade(&self.observers);
            let value_generator_weak = Arc::downgrade(&self.generator);
            let value_weak = Arc::downgrade(&self.value);
            Box::new(move || {
                if let (Some(observers), Some(value_generator), Some(value)) = (
                    observers_weak.upgrade(),
                    value_generator_weak.upgrade(),
                    value_weak.upgrade(),
                ) {
                    if let Some(generator) = &*value_generator.lock() {
                        let new_value = generator();
                        *value.lock() = new_value;
                    }
                    let observers = observers.lock();
                    for observer in observers.values() {
                        observer();
                    }
                }
            })
        });
        self.dependencies
            .lock()
            .push(Arc::downgrade(other.observers()));
    }
}
impl<T, A> Shared<T, A>
where
    T: Send + Sync + Clone + 'static,
    A: Readable,
{
    pub fn get(&self) -> T {
        self.value.lock().clone()
    }
}
impl<T, A: Readable> Drop for Shared<T, A> {
    fn drop(&mut self) {
        if Arc::strong_count(&self.value) == 1 {
            let mut observed = self.dependencies.lock();
            observed.iter_mut().for_each(|weak_observers| {
                if let Some(observers) = weak_observers.upgrade() {
                    observers.lock().remove(&self.id);
                }
            });
        }
    }
}

impl<T> Into<Shared<T, Derived>> for Shared<T, Source>
where
    T: Send + Sync + 'static,
{
    fn into(self) -> Shared<T, Derived> {
        Shared::<T, Derived> {
            _access_marker: Default::default(),
            id: self.id,
            value: self.value.clone(),
            generator: self.generator.clone(),
            observers: self.observers.clone(),
            dependencies: self.dependencies.clone(),
        }
    }
}
/*impl<T> Into<Shared<T, Derived>> for &Shared<T, Source>
where
    T: Send + Sync + 'static,
{
    fn into(self) -> Shared<T, Derived> {
        Shared::<T, Derived> {
            _access_marker: Default::default(),
            id: self.id,
            value: self.value.clone(),
            generator: self.generator.clone(),
            observers: self.observers.clone(),
            dependencies: self.dependencies.clone(),
        }
    }
}*/


pub trait Observable {
    fn observers(&self) -> &Arc<Mutex<HashMap<u32, Box<dyn Fn() + Send>>>>;
}

impl<T, A: Readable> Observable for Shared<T, A>
where
    T: Send + Sync + 'static,
{
    fn observers(&self) -> &Arc<Mutex<HashMap<u32, Box<dyn Fn() + Send>>>> {
        &self.observers
    }
}

impl<T: Send + Sync + 'static> Into<Box<dyn Observable>> for &Shared<T, Source> {
    fn into(self) -> Box<dyn Observable> {
        Box::new(self.clone())
    }
}


impl<T: Send + Sync + 'static> From<T> for Shared<T, Source> {
    fn from(value: T) -> Self {
        Shared::new(value)
    }
}

impl<T: Send + Sync + Clone + 'static, A: Readable> From<&Shared<T, A>> for Shared<T, Derived> {
    fn from(value: &Shared<T, A>) -> Self {
        let value = value.clone();
        SharedDerived::from_fn(depend!(value), move || value.get())
    }
}


impl<T: Send + Sync + 'static> From<T> for Shared<T, Derived> {
    fn from(value: T) -> Self {
        Shared {
            _access_marker: Default::default(),
            id: next_id(),
            value: Arc::new(Mutex::new(value)),
            generator: Arc::new(Mutex::new(None)),
            observers: Arc::new(Mutex::new(HashMap::new())),
            dependencies: Arc::new(Mutex::new(Vec::new())),
        }
    }
}


impl<T, A: Readable> AsRef<Shared<T, A>> for Shared<T, A> {
    fn as_ref(&self) -> &Shared<T, A> {
        self
    }
}