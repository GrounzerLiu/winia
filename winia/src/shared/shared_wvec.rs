use std::time::Instant;
use crate::animation::LayoutAnimation;
use crate::collection::{CollectionOperation, Operable, OperableClone, WVec};
use crate::depend;
use crate::shared::{SharedDerived, SharedSource};

pub type SharedWVec<T> = SharedSource<WVec<T>>;
pub type SharedDerivedWVec<T> = SharedDerived<WVec<T>>;

impl<T: 'static> From<Vec<T>> for SharedWVec<T> {
    fn from(value: Vec<T>) -> Self {
        SharedSource::new(WVec::from(value))
    }
}

impl<T: 'static> From<Vec<T>> for SharedDerivedWVec<T> {
    fn from(value: Vec<T>) -> Self {
        SharedDerived::new_derived(WVec::from(value))
    }
}

impl<T: 'static> SharedWVec<T> {
    pub fn push(&self, item: T) {
        self.write(move |wvec| {
            wvec.push(item);
        });
    }
    pub fn insert(&self, index: usize, item: T) {
        self.write(move |wvec| {
            wvec.insert(index, item);
        });
    }


    pub fn pop(&self) -> Option<T> {
        self.write(|wvec| {
            wvec.pop()
        })
    }
    pub fn remove(&self, index: usize) -> T {
        self.write(|wvec| {
            wvec.remove(index)
        })
    }
    pub fn clear(&self) {
        self.write(|wvec| {
            wvec.clear();
        });
    }
    pub fn len(&self) -> usize {
        self.read().len()
    }

    pub fn is_empty(&self) -> bool {
        self.read().is_empty()
    }

    pub fn take_operations(&self) -> Vec<CollectionOperation> {
        self.write(|wvec| {
            wvec.take_operations()
        })
    }
}

impl<T: Send + Sync + 'static> SharedDerivedWVec<T> {
    pub fn shared_len(&self) -> SharedDerived<usize> {
        SharedDerived::from_fn(
            depend!(self),
            {
                let shared_self = self.clone();
                move || {
                    shared_self.read().len()
                }
            }
        )
    }
}

impl<T: 'static + Send + Sync> OperableClone for SharedDerivedWVec<T> {
    fn clone_box(&self) -> Box<dyn Operable> {
        Box::new(self.clone())
    }
}

impl<T: Send + Sync + 'static> Operable for SharedDerivedWVec<T> {
    fn take_operations(&mut self) -> Vec<CollectionOperation> {
        self.lock().take_operations()
    }
}

impl<T: 'static + Send + Sync> Into<Box<dyn Operable>> for SharedDerivedWVec<T> {
    fn into(self) -> Box<dyn Operable> {
        Box::new(self.clone())
    }
}