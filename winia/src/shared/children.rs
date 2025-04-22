/*use crate::core::generate_id;
use crate::shared::{Observable, Removal};
use crate::ui::Item;
use crate::OptionalInvoke;
use parking_lot::lock_api::MutexGuard;
use parking_lot::{Mutex, RawMutex};
use rayon::prelude::IntoParallelRefMutIterator;
use rayon::slice::IterMut;
use std::ops::{Add, Deref, DerefMut};
use std::sync::{Arc, Weak};

pub enum Action<'a> {
    Add(&'a mut Item),
    Remove(&'a mut Item),
}

pub struct Items {
    items: Vec<Item>,
}

impl Items {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    pub fn iter_visible_item(
        &mut self,
        window_size: (f32, f32),
    ) -> impl Iterator<Item = &mut Item> {
        self.items.iter_mut().filter(move |item| {
            let display_parameter = item.data().get_display_parameter();
            let x = display_parameter.x();
            let y = display_parameter.y();
            let width = display_parameter.width;
            let height = display_parameter.height;

            !(
                x + width < 0.0
                    || y + height < 0.0
                    || x > window_size.0
                    || y > window_size.1
                    || width <= 0.0
                    || height <= 0.0
            )
        })
    }

    pub fn par_iter_mut(&mut self) -> IterMut<Item> {
        self.items.par_iter_mut()
    }
}

impl Deref for Items {
    type Target = Vec<Item>;

    fn deref(&self) -> &Self::Target {
        &self.items
    }
}
impl DerefMut for Items {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.items
    }
}

pub struct Children {
    items: Arc<Mutex<Items>>,
    simple_observers: Arc<Mutex<Vec<(usize, Box<dyn FnMut() + Send>)>>>,
    specific_observers: Arc<Mutex<Vec<(usize, Box<dyn FnMut(&Action) + Send>)>>>,
}

impl Children {
    pub fn new() -> Self {
        Self {
            items: Arc::new(Mutex::new(Items::new())),
            simple_observers: Arc::new(Mutex::new(Vec::new())),
            specific_observers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn notify(&mut self, action: Action) {
        for (_, observer) in self.simple_observers.lock().iter_mut() {
            observer();
        }
        for (_, observer) in self.specific_observers.lock().iter_mut() {
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
        let mut item = self.items.lock().remove(index);
        let action = Action::Remove(&mut item);
        self.notify(action);
        item.data().get_on_detach().iter_mut().for_each(|f| f());
    }

    pub fn remove_by_id(&mut self, id: usize) {
        let items = self.items.lock();
        let index = items.iter().position(|item| item.data().get_id() == id);
        drop(items);
        if let Some(index) = index {
            self.remove_by_index(index);
        }
    }

    pub fn len(&self) -> usize {
        self.items.lock().len()
    }

    pub fn clear(&mut self) {
        while let Some(item) = self.items.lock().pop() {
            item.data().get_on_detach().iter_mut().for_each(|f| f());
        }
    }

    pub fn items(&self) -> MutexGuard<'_, RawMutex, Items> {
        self.items.lock()
    }

    pub fn add_specific_observer(
        &mut self,
        observer: Box<dyn FnMut(&Action) + Send>,
    ) -> Box<dyn FnOnce()> {
        let id = generate_id();
        self.specific_observers.lock().push((id, observer));
        let specific_observers = self.specific_observers.clone();
        Box::new(move || {
            specific_observers
                .lock()
                .retain(|(observer_id, _)| *observer_id != id);
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
            children_weak: self.weak(),
        }
    }
}

impl Observable for Children {
    fn add_observer(&mut self, id: usize, observer: Box<dyn FnMut() + Send>) -> Removal {
        self.simple_observers.lock().push((id, observer));
        let simple_observers = self.simple_observers.clone();
        Removal::new(move || {
            simple_observers
                .lock()
                .retain(|(observer_id, _)| *observer_id != id);
        })
    }
}

impl Add<Item> for Children {
    type Output = Self;

    fn add(mut self, mut rhs: Item) -> Self::Output {
        let action = Action::Add(&mut rhs);
        self.notify(action);
        {
            let mut items = self.items.lock();
            items.push(rhs);
            items
                .last_mut()
                .unwrap()
                .data()
                .get_on_attach()
                .iter_mut()
                .for_each(|f| f());
        }
        self
    }
}

struct ChildrenWeak {
    items: Weak<Mutex<Items>>,
    simple_observers: Weak<Mutex<Vec<(usize, Box<dyn FnMut() + Send>)>>>,
    specific_observers: Weak<Mutex<Vec<(usize, Box<dyn FnMut(&Action) + Send>)>>>,
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
        self.children_weak.upgrade().if_some(|children| {
            let _ = children.add(item);
        });
    }

    pub fn remove_by_index(&mut self, index: usize) {
        self.children_weak
            .upgrade()
            .if_some(|mut children| children.remove_by_index(index));
    }

    pub fn remove_by_id(&mut self, id: usize) {
        self.children_weak
            .upgrade()
            .if_some(|mut children| children.remove_by_id(id));
    }

    // pub fn retain<F>(&mut self, f: F)
    // where
    //     F: FnMut(&Item) -> bool,
    // {
    //     self.children_weak.upgrade().if_some(|mut children| children.retain(f));
    // }

    pub fn len(&self) -> usize {
        self.children_weak
            .upgrade()
            .map(|children| children.len())
            .unwrap_or(0)
    }

    pub fn clear(&mut self) {
        self.children_weak
            .upgrade()
            .if_some(|mut children| children.clear());
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
*/
use std::ops::Add;
use crate::shared::{Shared, SharedUnSend};
use crate::ui::Item;

pub type Children = SharedUnSend<Vec<Item>>;

impl Children {
    pub fn new() -> Self {
        SharedUnSend::from_static(Vec::new())
    }
    pub fn add(&mut self, item: Item) {
        self.lock().push(item);
        self.notify();
    }

    pub fn push(&mut self, item: Item) {
        self.lock().push(item);
        self.notify();
    }

    pub fn remove(&mut self, index: usize) {
        self.lock().remove(index);
        self.notify();
    }

    pub fn remove_by_id(&mut self, id: usize) {
        {
            let mut items = self.lock();
            items.retain(|item| item.data().get_id() != id);
        }
        self.notify();
    }
}

impl Add<Item> for Children {
    type Output = Self;

    fn add(self, rhs: Item) -> Self::Output {
        self.lock().push(rhs);
        self
    }
}

impl From<Item> for Children {
    fn from(item: Item) -> Self {
        let mut children = Self::new();
        children.push(item);
        children
    }
}