use crate::collection::WVec;
use crate::exclude_target;
use crate::shared::shared_vec::LocalSharedVec;
use crate::shared::{Gettable, LocalShared, Settable};
use crate::ui::animation::{AnimationExt, LocalAnimationExt};
use crate::ui::Item;
use clonelet::clone;
use std::ops::Add;
use std::slice::Iter;
use std::time::Duration;

pub type Children = LocalSharedVec<Item>;

impl Children {
    pub fn new() -> Self {
        LocalShared::from_static(WVec::new())
    }
    pub fn add_item(&self, item: Item) {
        self.lock().push(item);
        self.notify();
    }

    pub fn insert_with_animation(&self, index: usize, item: Item) {
        let visible = item.data().get_visible().clone();
        if visible.get() {
            visible.set(false);
        }

        {
            let mut animated = false;
            let layout = item.data().get_layout();
            item.data().set_layout(move |item, w, h| {
                if !animated {
                    item.get_window_context()
                        .animate(exclude_target!())
                        .transformation({
                            clone!(visible);
                            move || {
                                if !visible.get() {
                                    visible.set(true);
                                }
                            }
                        })
                        .duration(Duration::from_millis(500))
                        .start();
                    animated = true;
                }
                let mut layout = layout.lock();
                layout(item, w, h)
            });
        }
        self.lock().insert(index, item);
        self.notify();
    }

    pub fn remove_with_animation(&self, id: usize) {
        let self_clone = self.clone();
        let self_lock = self.lock();
        if let Some(item) = self_lock.iter().find(|item| item.data().get_id() == id) {
            let visible = item.data().get_visible().clone();

            item.data()
                .get_window_context()
                .local_animate(exclude_target!())
                .transformation({
                    clone!(visible);
                    move || {
                        visible.set(false);
                    }
                })
                .duration(Duration::from_millis(500))
                .on_finished({
                    move || {
                        self_clone.remove_by_id(id);
                    }
                })
                .start()
        }
    }

    pub fn remove_by_id(&self, id: usize) {
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
        let children = Self::new();
        children.push(item);
        children
    }
}

#[macro_export]
macro_rules! children {
    ($($item:expr),* $(,)?) => {
        {
            let mut children = Children::new();
            $(
                children.lock().push($item);
            )*
            children
        }
    };
}

pub trait ForEachOption<T> {
    fn for_each_option<F>(self, f: F) -> Children
    where
        F: FnMut(&T) -> Item;
}

impl<T> ForEachOption<T> for Iter<'_, T> {
    fn for_each_option<F>(self, mut f: F) -> Children
    where
        F: FnMut(&T) -> Item
    {
        let children = Children::new();
        for item in self {
            children.lock().push(f(item));
        }
        children
    }
}