use std::collections::LinkedList;
use std::rc::{Rc, Weak};
use std::sync::{Mutex, MutexGuard};

pub struct MulticastEvent<T:?Sized>{
    pub events: Rc<Mutex<LinkedList<Box<T>>>>
}

impl<T:?Sized> MulticastEvent<T> {
    pub fn new() -> Self {
        Self {
            events: Rc::new(Mutex::new(LinkedList::new()))
        }
    }

    pub fn add_event(&self, event: Box<T>) {
        self.events.lock().unwrap().push_back(event);
    }

    pub fn clear(&self) {
        self.events.lock().unwrap().clear();
    }
    
    pub fn set_event(&self, event: Box<T>) {
        let mut events = self.events.lock().unwrap();
        events.clear();
        events.push_back(event);
    }
    
    pub fn get_events(&self) -> MutexGuard<LinkedList<Box<T>>> {
        self.events.lock().unwrap()
    }
    
    pub fn weak(&self) -> Weak<Mutex<LinkedList<Box<T>>>> {
        Rc::downgrade(&self.events)
    }
}

impl<T:?Sized> Clone for MulticastEvent<T> {
    fn clone(&self) -> Self {
        Self {
            events: self.events.clone()
        }
    }
}