use crate::collection::WVec;
use crate::shared::{LocalShared, Shared};

pub type SharedVec<T> = Shared<WVec<T>>;

impl<T: Send + 'static> SharedVec<T> {
    pub fn push(&self, value: T) {
        self.lock().push(value);
    }
    
    pub fn insert(&self, index: usize, value: T) {
        self.lock().insert(index, value);
    }
    
    pub fn remove(&self, index: usize) -> T {
        self.lock().remove(index)
    }
    
    pub fn pop(&self) -> Option<T> {
        self.lock().pop()
    }
    
    pub fn clear(&self) {
        self.lock().clear();
    }
    
    pub fn len(&self) -> usize {
        self.lock().len()
    }
    
    pub fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }
}

pub type LocalSharedVec<T> = LocalShared<WVec<T>>;

impl<T: 'static> LocalSharedVec<T> {
    pub fn push(&self, value: T) {
        self.lock().push(value);
    }

    pub fn insert(&self, index: usize, value: T) {
        self.lock().insert(index, value);
    }

    pub fn remove(&self, index: usize) -> T {
        self.lock().remove(index)
    }

    pub fn pop(&self) -> Option<T> {
        self.lock().pop()
    }

    pub fn clear(&self) {
        self.lock().clear();
    }

    pub fn len(&self) -> usize {
        self.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }
}