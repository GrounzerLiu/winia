/*use std::rc::Rc;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::shared::{Observable, Observer, Observers};
use crate::uib::Item;

#[derive(Clone)]
pub struct Children{
    pub(crate) items: Rc<Mutex<Vec<Item>>>,
    pub(crate) observers: Observers<Children>
}

impl Observable for Children{
    fn removal_closure(&self, id: usize) -> Box<dyn FnMut() + 'static> {
        let observers = Arc::clone(&self.observers);
        Box::new(move || {
            observers.lock().unwrap().retain(|(observer_id, _)| *observer_id != id);
        })
    }

    fn add_observer(&self, observer: impl FnMut() + 'static, id: usize) {
        self.observers.lock().unwrap().push((id, Observer::Common(Box::new(observer))));
    }

    fn remove_observer(&self, id: usize) {
        self.observers.lock().unwrap().retain(|(observer_id, _)| *observer_id != id);
    }

    fn clear_observers(&self) {
        self.observers.lock().unwrap().clear();
    }
}

impl Children{
    pub fn new() -> Self{
        Self{
            items: Rc::new(Mutex::new(Vec::new())),
            observers: Arc::new(Mutex::new(Vec::new()))
        }
    }

    pub fn notify(&mut self) {
        let observers_clone = self.observers.clone();
        let mut observers = observers_clone.lock().unwrap();
        for (_, observer) in observers.iter_mut() {
            match observer {
                Observer::Common(common_observer) => common_observer(),
                Observer::Specific(specific_observer) => specific_observer(self),
            }
        }
    }

    pub fn add_child(&mut self, item: Item){
        self.items.lock().unwrap().push(item);
        self.notify();
    }

    pub fn remove_child(&mut self, index: usize){
        self.items.lock().unwrap().remove(index);
        self.notify();
    }

    pub fn len(&self) -> usize{
        self.items.lock().unwrap().len()
    }

    pub fn clear(&mut self){
        self.items.lock().unwrap().clear();
        self.notify();
    }
    
    pub fn items(&self) -> &Rc<Mutex<Vec<Item>>>{
        &self.items
    }
    
    pub fn items_mut(&mut self) -> &mut Rc<Mutex<Vec<Item>>>{
        &mut self.items
    }
    
    pub fn lock(&self) -> MutexGuard<Vec<Item>>{
        self.items.lock().unwrap()
    }

    pub fn manager(&self) -> ChildrenManager{
        ChildrenManager{
            children: Rc::clone(&self.items),
            observers: Arc::clone(&self.observers)
        }
    }
}

#[derive(Clone)]
pub struct ChildrenManager{
    children: Rc<Mutex<Vec<Item>>>,
    observers: Observers<Children>
}

impl ChildrenManager{
    pub fn add(&mut self, item: Item){
        self.children.lock().unwrap().push(item);
    }

    pub fn remove(&mut self, index: usize){
        self.children.lock().unwrap().remove(index);
    }
    
    pub fn retain<F>(&mut self, f: F) where F: FnMut(&Item) -> bool{
        self.children.lock().unwrap().retain(f);
    }

    pub fn len(&self) -> usize{
        self.children.lock().unwrap().len()
    }

    pub fn clear(&mut self){
        self.children.lock().unwrap().clear();
    }
}

impl Observable for ChildrenManager{
    fn removal_closure(&self, id: usize) -> Box<dyn FnMut() + 'static> {
        let observers = Arc::clone(&self.observers);
        Box::new(move || {
            observers.lock().unwrap().retain(|(observer_id, _)| *observer_id != id);
        })
    }

    fn add_observer(&self, observer: impl FnMut() + 'static, id: usize) {
        self.observers.lock().unwrap().push((id, Observer::Common(Box::new(observer))));
    }

    fn remove_observer(&self, id: usize) {
        self.observers.lock().unwrap().retain(|(observer_id, _)| *observer_id != id);
    }

    fn clear_observers(&self) {
        self.observers.lock().unwrap().clear();
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