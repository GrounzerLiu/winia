/*use std::sync::Arc;
use parking_lot::Mutex;
use crate::core::generate_id;
use crate::shared::Observable;

pub enum ListOperation {
    Insert(usize),
    Remove(usize),
    Update(usize),
}

pub struct SharedList<T> {
    list_operations: Arc<Mutex<Vec<ListOperation>>>,
    id: usize,
    value: Arc<Mutex<Vec<T>>>,
    value_generator: Arc<Mutex<Option<Box<dyn Fn() -> Vec<T> + Send>>>>,
    observed_objects:
        Arc<Mutex<Vec<(Option<Box<dyn Observable + Send>>, Box<dyn FnOnce() + Send>)>>>,
    simple_observers: Arc<Mutex<Vec<(usize, Box<dyn FnMut() + Send>)>>>,
}

impl<T: Send + 'static> SharedList<T> {
    fn inner_new(value: Vec<T>, value_generator: Option<Box<dyn Fn() -> Vec<T> + Send>>) -> Self {
        let value = Arc::new(Mutex::new(value));
        let value_generator = Arc::new(Mutex::new(value_generator));
        Self {
            list_operations: Arc::new(Mutex::new(Vec::with_capacity(0))),
            id: generate_id(),
            value,
            value_generator,
            observed_objects: Arc::new(Mutex::new(Vec::with_capacity(0))),
            simple_observers: Arc::new(Mutex::new(Vec::with_capacity(0))),
        }
    }

    pub fn new() -> Self {
        Self::inner_new(Vec::new(), None)
    }

    pub fn from_static(value: Vec<T>) -> Self {
        Self::inner_new(value, None)
    }

    pub fn from_dynamic(
        value_generator: impl Fn() -> Vec<T> + Send + 'static,
    ) -> Self {
        let value_generator: Box<dyn Fn() -> Vec<T> + Send> = Box::new(value_generator);
        let value = value_generator();
        let value_generator = Some(value_generator);
        let mut shared = Self::inner_new(value, value_generator);
        shared
    }

    pub fn observe<O: Observable + Send + Clone + 'static>(&mut self, observable: O) {
        let mut observable = Box::new(observable);
        let self_weak = self.weak();
        let removal  = observable
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
        self.observed_objects.lock().retain(|(observable, _)| {
            if let Some(observable) = observable {
                observable.remove_observer(id);
            }
            true
        });
    }
}
*/