use std::collections::linked_list::{Iter, IterMut};
use std::collections::LinkedList;
use crate::collection::Operation;
use crate::shared::Shared;

pub struct WLinkedList<T> {
    list: LinkedList<T>,
    operations: Shared<LinkedList<Operation>>,
}

impl<T> WLinkedList<T> {
    pub fn new() -> Self {
        Self {
            list: LinkedList::new(),
            operations: LinkedList::new().into(),
        }
    }

    pub fn push_back(&mut self, item: T) {
        self.operations.lock().push_back(Operation::Add(self.list.len()));
        self.list.push_back(item);
    }

    pub fn push_front(&mut self, item: T) {
        self.operations.lock().push_back(Operation::Add(0));
        self.list.push_front(item);
    }

    pub fn pop_back(&mut self) -> Option<T> {
        let item = self.list.pop_back();
        if item.is_some() {
            self.operations.lock().push_back(Operation::Remove(self.list.len()));
        }
        item
    }

    pub fn pop_front(&mut self) -> Option<T> {
        let item = self.list.pop_front();
        if item.is_some() {
            self.operations.lock().push_back(Operation::Remove(0));
        }
        item
    }

    pub fn clear(&mut self) {
        self.list.clear();
        self.operations.lock().push_back(Operation::Clear);
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn operations(&self) -> Shared<LinkedList<Operation>> {
        self.operations.clone()
    }
    
    pub fn iter(&self) -> Iter<'_, T> {
        self.list.iter()
    }
    
    pub fn iter_mut(&mut self) -> IterMut<'_, T> {
        self.operations.lock().push_back(Operation::Other);
        self.list.iter_mut()
    }
}