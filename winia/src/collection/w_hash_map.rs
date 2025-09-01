use std::collections::{HashMap, LinkedList};
use std::collections::hash_map::{Iter, IterMut};
use crate::collection::Operation;
use crate::shared::Shared;

pub struct WHashMap<K, V> {
    map: HashMap<K, V>,
    operations: Shared<LinkedList<Operation>>,
}

impl<K: Eq + std::hash::Hash, V> WHashMap<K, V> {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            operations: LinkedList::new().into(),
        }
    }

    pub fn insert(&mut self, key: K, value: V) {
        self.operations.lock().push_back(Operation::Add(self.map.len()));
        self.map.insert(key, value);
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        if let Some(value) = self.map.remove(key) {
            self.operations.lock().push_back(Operation::Remove(self.map.len()));
            Some(value)
        } else {
            None
        }
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        self.map.get(key)
    }

    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.operations.lock().push_back(Operation::Update(self.map.len()));
        self.map.get_mut(key)
    }

    pub fn clear(&mut self) {
        self.map.clear();
        self.operations.lock().push_back(Operation::Clear);
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }
    
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
    
    pub fn operations(&self) -> Shared<LinkedList<Operation>> {
        self.operations.clone()
    }
    
    pub fn iter(&self) -> Iter<'_, K, V> {
        self.map.iter()
    }
    
    pub fn iter_mut(&mut self) -> IterMut<'_, K, V> {
        self.operations.lock().push_back(Operation::Other);
        self.map.iter_mut()
    }   
}