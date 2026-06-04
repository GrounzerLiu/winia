use crate::collection::CollectionOperation;
use std::ops::{Index, IndexMut};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct WVec<T> {
    items: Vec<T>,
    operations: Vec<CollectionOperation>,
}

impl<T> From<Vec<T>> for WVec<T> {
    fn from(v: Vec<T>) -> Self {
        WVec {
            items: v,
            operations: vec![CollectionOperation::UpdateAll],
        }
    }
}

impl<T> WVec<T> {
    pub fn new() -> Self {
        WVec {
            items: Vec::new(),
            operations: vec![],
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        WVec {
            items: Vec::with_capacity(capacity),
            operations: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn take_operations(&mut self) -> Vec<CollectionOperation> {
        let ops = self.operations.clone();
        self.operations.clear();
        ops
    }

    pub fn push(&mut self, item: T) {
        self.items.push(item);
        self.operations.push(CollectionOperation::Add(self.items.len() - 1));
    }
    
    pub fn insert(&mut self, index: usize, item: T) {
        self.items.insert(index, item);
        self.operations.push(CollectionOperation::Add(index));
    }
    
    pub fn pop(&mut self) -> Option<T> {
        let item = self.items.pop();
        if let Some(_) = item {
            self.operations.push(CollectionOperation::Remove(self.items.len()));
        }
        item
    }

    pub fn remove(&mut self, index: usize) -> T {
        let item = self.items.remove(index);
        self.operations.push(CollectionOperation::Remove(index));
        item
    }

    pub fn update(&mut self, index: usize, item: T) {
        self.items[index] = item;
        self.operations.push(CollectionOperation::Update(index));
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.operations.push(CollectionOperation::Clear);
    }

    pub fn get(&self, index: usize) -> Option<&T> {
        self.items.get(index)
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.operations.push(CollectionOperation::Update(index));
        self.items.get_mut(index)
    }

    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.items.iter()
    }

    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, T> {
        self.operations.push(CollectionOperation::UpdateAll);
        self.items.iter_mut()
    }

}

impl<T> Index<usize> for WVec<T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        &self.items[index]
    }
}

impl<T> IndexMut<usize> for WVec<T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        self.operations.push(CollectionOperation::Update(index));
        &mut self.items[index]
    }
}

impl<T: Clone> Clone for WVec<T> {
    fn clone(&self) -> Self {
        WVec {
            items: self.items.clone(),
            operations: self.operations.clone(),
        }
    }
}