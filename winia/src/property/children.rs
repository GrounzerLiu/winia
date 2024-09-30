use std::collections::HashMap;
use std::ops::Add;
use std::sync::{Arc, LockResult, Mutex, MutexGuard, Weak};
use crate::core::generate_id;
use crate::{LockUnwrap, OptionalInvoke};
use crate::property::{Observable, Removal};
use crate::ui::Item;
use crate::ui::item::DisplayParameter;

pub enum Action<'a> {
    Add(&'a mut Item),
    Remove(&'a mut Item),
}

pub struct Children {
    items: Arc<Mutex<Vec<Item>>>,
    simple_observers: Arc<Mutex<Vec<(usize, Box<dyn FnMut()>)>>>,
    specific_observers: Arc<Mutex<Vec<(usize, Box<dyn FnMut(&Action)>)>>>,
}

impl Children {
    pub fn new() -> Self {
        Self {
            items: Arc::new(Mutex::new(Vec::new())),
            simple_observers: Arc::new(Mutex::new(Vec::new())),
            specific_observers: Arc::new(Mutex::new(Vec::new())),
        }
    }
    

    pub fn notify(&mut self, action: Action) {
        for (_, observer) in self.simple_observers.lock().unwrap().iter_mut() {
            observer();
        }
        for (_, observer) in self.specific_observers.lock().unwrap().iter_mut() {
            observer(&action);
        }
    }

    // pub fn add_child(&mut self, mut item: Item) {
    //     let action = Action::Add(&mut item);
    //     self.notify(action);
    //     self.items.lock().unwrap().push(item);
    // }

    // pub fn retain<F>(&mut self, mut f: F)
    // where
    //     F: FnMut(&Item) -> bool,
    // {
    //     let mut items = self.items.lock().unwrap();
    //     items.retain(|item| f(item));
    // }

    pub fn remove_by_index(&mut self, index: usize) {
        let mut item = self.items.lock().unwrap().remove(index);
        let action = Action::Remove(&mut item);
        self.notify(action);
        item.on_detach.iter_mut().for_each(|f| f());
    }

    pub fn remove_by_id(&mut self, id: usize) {
        let items = self.items.lock().unwrap();
        let index = items.iter().position(|item| item.get_id() == id);
        drop(items);
        if let Some(index) = index {
            self.remove_by_index(index);
        }
    }

    pub fn len(&self) -> usize {
        self.items.lock().unwrap().len()
    }

    pub fn clear(&mut self) {
        while let Some(mut item) = self.items.lock().unwrap().pop() {
            item.on_detach.iter_mut().for_each(|f| f());
        }
    }

    pub fn items(&self) -> MutexGuard<'_, Vec<Item>> {
        self.items.lock().unwrap()
    }

    pub fn add_specific_observer(&mut self, observer: Box<dyn FnMut(&Action)>) -> Box<dyn FnOnce()> {
        let id = generate_id();
        self.specific_observers.lock().unwrap().push((id, observer));
        let specific_observers = self.specific_observers.clone();
        Box::new(move || {
            specific_observers.lock().unwrap().retain(|(observer_id, _)| *observer_id != id);
        })
    }

    fn weak(&self) -> ChildrenWeak {
        ChildrenWeak {
            items: Arc::downgrade(&self.items),
            simple_observers: Arc::downgrade(&self.simple_observers),
            specific_observers: Arc::downgrade(&self.specific_observers),
        }
    }

    pub fn manager(&self) -> ChildrenManager {
        ChildrenManager {
            children_weak: self.weak()
        }
    }
}

impl Observable for Children {
    fn add_observer(&mut self, id: usize, observer: Box<dyn FnMut()>) -> Removal {
        self.simple_observers.lock().unwrap().push((id, observer));
        let simple_observers = self.simple_observers.clone();
        Removal::new(move || {
            simple_observers.lock().unwrap().retain(|(observer_id, _)| *observer_id != id);
        })
    }
}

impl Add<Item> for Children {
    type Output = Self;

    fn add(mut self, mut rhs: Item) -> Self::Output {
        let action = Action::Add(&mut rhs);
        self.notify(action);
        self.items.lock_unwrap_mut(|items| {
            items.push(rhs);
            items.last_mut().unwrap().on_attach.iter_mut().for_each(|f| f());
        });
        self
    }
}

struct ChildrenWeak {
    items: Weak<Mutex<Vec<Item>>>,
    simple_observers: Weak<Mutex<Vec<(usize, Box<dyn FnMut()>)>>>,
    specific_observers: Weak<Mutex<Vec<(usize, Box<dyn FnMut(&Action)>)>>>,
}

impl ChildrenWeak {
    fn upgrade(&self) -> Option<Children> {
        let items = self.items.upgrade()?;
        let simple_observers = self.simple_observers.upgrade()?;
        let specific_observers = self.specific_observers.upgrade()?;
        Some(Children {
            items,
            simple_observers,
            specific_observers,
        })
    }
}

pub struct ChildrenManager {
    children_weak: ChildrenWeak,
}

impl ChildrenManager {
    pub fn is_valid(&self) -> bool {
        self.children_weak.upgrade().is_some()
    }

    pub fn add(&mut self, item: Item) {
        self.children_weak.upgrade().if_some(|children| { children.add(item); });
    }

    pub fn remove_by_index(&mut self, index: usize) {
        self.children_weak.upgrade().if_some(|mut children| children.remove_by_index(index));
    }

    pub fn remove_by_id(&mut self, id: usize) {
        self.children_weak.upgrade().if_some(|mut children| children.remove_by_id(id));
    }

    // pub fn retain<F>(&mut self, f: F)
    // where
    //     F: FnMut(&Item) -> bool,
    // {
    //     self.children_weak.upgrade().if_some(|mut children| children.retain(f));
    // }

    pub fn len(&self) -> usize {
        self.children_weak.upgrade().map(|children| children.len()).unwrap_or(0)
    }

    pub fn clear(&mut self) {
        self.children_weak.upgrade().if_some(|mut children| children.clear());
    }
}

#[macro_export]
macro_rules! children {
    () => (
        {
            $crate::uib::Children::new()
        }
    );
    ($($x:expr),+ $(,)?) => (
        {
            let mut children = $crate::uib::Children::new();
            $(
                children.add($x);
            )+
            children
        }
    );
}


// pub struct ItemCollection{
//     items: Vec<Item>,
//     observers: Rc<Mutex<Vec<Observer>>>
// }
//
// impl Observable for ItemCollection{
//     fn add_observer(&self, listener: Observer){
//         self.observers.lock().unwrap().push(listener);
//     }
//     fn remove_observer(&self, owner_id: usize){
//         let mut observers = self.observers.lock().unwrap();
//         observers.retain(|observer| observer.owner_id != owner_id);
//     }
//     fn clear_observers(&self){
//         self.observers.lock().unwrap().clear();
//     }
//     fn notify(&self){
//         let mut observers = self.observers.lock().unwrap();
//         for observer in observers.iter_mut(){
//             observer.notify();
//         }
//     }
// }

// impl ItemCollection{
//     pub fn new() -> Self{
//         Self{
//             items: Vec::new(),
//             observers: Rc::new(Mutex::new(Vec::new()))
//         }
//     }
//
//     pub fn add(&mut self, item: Item){
//         self.items.push(item);
//         self.notify();
//     }
//
//     pub fn remove(&mut self, index: usize){
//         self.items.remove(index);
//         self.notify();
//     }
//
//     pub fn get(&self, index: usize) -> Option<&Item>{
//         self.items.get(index)
//     }
//
//     pub fn len(&self) -> usize{
//         self.items.len()
//     }
//
//     pub fn clear(&mut self){
//         self.items.clear();
//         self.notify();
//     }
//
//     pub fn iter(&self) -> Iter<Item>{
//         self.items.iter()
//     }
//
//     pub fn iter_mut(&mut self) -> std::slice::IterMut<Item>{
//         self.items.iter_mut()
//     }
// }
//
// pub type ItemCollectionProperty = SharedProperty<ItemCollection>;
//
// impl ItemCollectionProperty{
//     pub fn new() -> Self{
//         Self::from_value(ItemCollection::new())
//     }
// }*/