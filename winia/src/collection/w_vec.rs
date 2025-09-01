use std::collections::LinkedList;
use crate::collection::Operation;
use crate::shared::Shared;

pub struct WVec<T> {
    vec: Vec<T>,
    operations: Shared<LinkedList<Operation>>,
}

impl<T> WVec<T> {
    pub fn new() -> Self {
        Self {
            vec: Vec::new(),
            operations: LinkedList::new().into()
        }
    }

    pub fn push(&mut self, item: T) {
        self.operations.lock().push_back(Operation::Add(self.vec.len()));
        self.vec.push(item);
    }

    pub fn insert(&mut self, index: usize, item: T) {
        self.vec.insert(index, item);
        self.operations.lock().push_back(Operation::Add(index));
    }

    pub fn remove(&mut self, index: usize) -> T {
        self.operations.lock().push_back(Operation::Remove(index));
        self.vec.remove(index)
    }
    
    pub fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(&T) -> bool,
    {
        self.operations.lock().push_back(Operation::Other);
        self.vec.retain(|item| f(item))
    }
    
    pub fn pop(&mut self) -> Option<T> {
        let item = self.vec.pop();
        if item.is_some() {
            self.operations.lock().push_back(Operation::Remove(self.vec.len()));
        }
        item
    }
    
    pub fn clear(&mut self) {
        self.vec.clear();
        self.operations.lock().push_back(Operation::Clear);
    }

    pub fn get(&self, index: usize) -> Option<&T> {
        self.vec.get(index)
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.operations.lock().push_back(Operation::Update(index));
        self.vec.get_mut(index)
    }

    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.vec.iter()
    }

    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, T> {
        self.operations.lock().push_back(Operation::Other);
        self.vec.iter_mut()
    }

    pub fn is_empty(&self) -> bool {
        self.vec.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.vec.capacity()
    }

    pub fn len(&self) -> usize {
        self.vec.len()
    }
    
    pub fn first(&self) -> Option<&T> {
        self.vec.first()
    }
    
    pub fn last(&self) -> Option<&T> {
        self.vec.last()
    }
    
    pub fn first_mut(&mut self) -> Option<&mut T> {
        self.operations.lock().push_back(Operation::Update(0));
        self.vec.first_mut()
    }
    
    pub fn last_mut(&mut self) -> Option<&mut T> {
        self.operations.lock().push_back(Operation::Update(self.vec.len() - 1));
        self.vec.last_mut()
    }
    
    pub fn operations(&self) -> Shared<LinkedList<Operation>> {
        self.operations.clone()
    }
}

impl<T> Default for WVec<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> From<Vec<T>> for WVec<T> {
    fn from(vec: Vec<T>) -> Self {
        Self {
            vec,
            operations: LinkedList::new().into(),
        }
    }
}

impl<T: Clone> Clone for WVec<T> {
    fn clone(&self) -> Self {
        Self {
            vec: self.vec.clone(),
            operations: self.operations.clone(),
        }
    }
}