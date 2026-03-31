use std::ops::{Add, Deref, DerefMut};
use std::sync::Arc;
use std::time::Instant;
use getset::Getters;
use parking_lot::Mutex;
use crate::animation::LayoutAnimation;
use crate::app::EventLoopProxy;
use crate::shared::{SharedReadGuard, SharedSource};
use crate::ui::Item;
use crate::ui::item::{Frame, ItemUpdater, MeasureMode};

// pub type ChildrenOperations = SharedSource<Vec<Box<dyn FnOnce(&mut Vec<Item>) + Send + Sync>>>;
#[derive(Clone)]
pub struct ChildrenOperations {
    operations: SharedSource<Vec<Box<dyn FnOnce(&mut Vec<Item>) + Send + Sync>>>,
    event_loop_proxy: Option<EventLoopProxy>
}

impl From<Vec<Box<dyn FnOnce(&mut Vec<Item>) + Send + Sync>>> for ChildrenOperations {
    fn from(value: Vec<Box<dyn FnOnce(&mut Vec<Item>) + Send + Sync>>) -> Self {
        Self {
            operations: SharedSource::new(value),
            event_loop_proxy: None,
        }
    }
}

impl ChildrenOperations {
    pub fn add_operation<F>(&self, operation: F)
    where
        F: FnOnce(&mut Vec<Item>) + Send + Sync + 'static,
    {
        self.operations.lock().push(Box::new(operation));
        if let Some(proxy) = &self.event_loop_proxy {
            proxy.request_update_layout();
        }
    }
}

#[derive(Clone, Getters)]
pub struct Children {
    parent_updater: Option<Arc<Mutex<ItemUpdater>>>,
    inner: SharedSource<Vec<Item>>,
    #[get = "pub"]
    pending_operations: ChildrenOperations,
}

impl Default for Children {
    fn default() -> Self {
        Self::new()
    }
}

impl Children {
    pub fn new() -> Self {
        Self {
            parent_updater: None,
            inner: SharedSource::new(vec![]),
            pending_operations: vec![].into(),
        }
    }

    pub(crate) fn set_parent_updater(&mut self, parent_updater: &Arc<Mutex<ItemUpdater>>) {
        self.parent_updater = Some(parent_updater.clone());
        for item in self.inner.lock().iter_mut() {
            item.data().item_updater.lock().parent = Some(Arc::downgrade(parent_updater));
        }
    }
    
    pub(crate) fn set_event_loop_proxy(&mut self, event_loop_proxy: &EventLoopProxy) {
        self.pending_operations.event_loop_proxy = Some(event_loop_proxy.clone());
    }
    
    pub fn write<F>(&self, f: F)
    where
        F: FnOnce(&mut Vec<Item>),
    {
        self.apply_pending_operations();
        self.inner.write(f);
    }
    
    pub fn read(&self) -> SharedReadGuard<'_, Vec<Item>> {
        self.apply_pending_operations();
        self.inner.read()
    }
    
    pub fn lock(&self) -> parking_lot::MutexGuard<'_, Vec<Item>> {
        self.apply_pending_operations();
        self.inner.lock()
    }

    pub fn subscribe(&self, observer_id: u32, callback: impl FnMut() + Send + 'static) {
        self.inner.subscribe(observer_id, callback);
    }
    
    pub fn apply_pending_operations(&self) {
        let operations = self.pending_operations.operations.lock().drain(..).collect::<Vec<_>>();
        if operations.is_empty() {
            return;
        }
        self.inner.write(|children|{
            for operation in operations {
                operation(children);
            }
        });
    }

    pub fn add_item(&mut self, item: Item) {
        self.apply_pending_operations();
        if let Some(parent_updater) = &self.parent_updater {
            item.data().item_updater.lock().parent = Some(Arc::downgrade(parent_updater));
        }
        self.write(|children|{
            if children.iter().any(|i| i.id() == item.id()) {
                return;
            }
            children.push(item);
        })
    }

    pub fn insert_item(&mut self, index: usize, item: Item) {
        self.apply_pending_operations();
        if let Some(parent_updater) = &self.parent_updater {
            item.data().item_updater.lock().parent = Some(Arc::downgrade(parent_updater));
        }
        self.write(|children|{
            if children.iter().any(|i| i.id() == item.id()) {
                return;
            }
            children.insert(index, item);
        });
    }

    pub fn remove_item(&mut self, id: u32) {
        self.apply_pending_operations();
        self.write(|children|{
            if let Some(children) = children.extract_if(.., |i| i.id() == id).next() {
                children.data().item_updater.lock().parent = None;
                children.data().is_mounted = false;
                children.data().on_unmounted();
            }
        });
    }

    pub fn remove_item_with_animation(
        &mut self,
        id: u32,
        animation: LayoutAnimation,
        exit_frame: impl Fn(&Frame) -> Frame + 'static
    ) {
        self.apply_pending_operations();
        let Some(item) = self.read().iter().find(|i| i.id() == id).cloned() else {
            return;
        };

        {
            let mut item_data = item.data();
            item_data.exit_frame = Some(Box::new(exit_frame));
            item_data.is_exited = false;
        }

        let transition_visible = {
            let item_data = item.data();
            item_data.transition_visible.clone()
        };
        let pending_operations = self.pending_operations.clone();
        animation.on_finished(move || {
            transition_visible.set(false);
            pending_operations.add_operation(move |children| {
                children.extract_if(.., |i| i.id() == id).for_each(|item| {
                    item.data().item_updater.lock().parent = None;
                    item.data().is_mounted = false;
                    item.data().on_unmounted();
                });
            });
        }).start();
    }

    pub fn add_item_with_animation(
        &mut self,
        item: Item,
        animation: LayoutAnimation,
        entry_frame: impl Fn(&Frame) -> Frame + 'static
    ) {
        let transition_visible = {
            let item_data = item.data();
            item_data.transition_visible.clone()
        };
        {
            let mut item_data = item.data();
            item_data.transition_visible.set(false);
            item_data.is_entered = false;
            item_data.entry_frame = Some(Box::new(entry_frame));
        }
        self.add_item(item);
        animation
            .transformation(move || {
                transition_visible.set(true);
            })
            .start()
    }

    pub fn insert_item_with_animation(
        &mut self,
        index: usize,
        item: Item,
        animation: LayoutAnimation,
        entry_frame: impl Fn(&Frame) -> Frame + 'static
    ) {
        let transition_visible = {
            let item_data = item.data();
            item_data.transition_visible.clone()
        };
        {
            let mut item_data = item.data();
            item_data.transition_visible.set(false);
            item_data.is_entered = false;
            item_data.entry_frame = Some(Box::new(entry_frame));
        }
        self.insert_item(index, item);
        animation
            .transformation(move || {
                transition_visible.set(true);
            })
            .start()
    }

    pub fn remove_by_id(&mut self, id: u32) {
        self.apply_pending_operations();
        self.write(|children|{
            // children.retain(|i| i.id() != id);
            children.extract_if(.., |i| i.id() == id).for_each(|item| {
                item.data().item_updater.lock().parent = None;
                item.data().is_mounted = false;
                item.data().on_unmounted();
            })
        })
    }

    pub fn clear(&mut self) {
        self.apply_pending_operations();
        self.write(|children|{
            for item in children.drain(..) {
                item.data().item_updater.lock().parent = None;
                item.data().is_mounted = false;
                item.data().on_unmounted();
            }
        })
    }
    
    pub fn len(&self) -> usize {
        self.apply_pending_operations();
        self.inner.lock().len()
    }
}

impl Add<Item> for Children {
    type Output = Children;

    fn add(mut self, rhs: Item) -> Self::Output {
        self.add_item(rhs);
        self
    }
}

impl From<&Children> for Children {
    fn from(children: &Children) -> Self {
        children.clone()
    }
}

/*impl Deref for Children {
    type Target = SharedSource<Vec<Item>>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for Children {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}*/

impl From<SharedSource<Vec<Item>>> for Children {
    fn from(value: SharedSource<Vec<Item>>) -> Self {
        Self {
            parent_updater: None,
            inner: value,
            pending_operations: vec![].into(),
        }
    }
}

impl From<Vec<Item>> for Children {
    fn from(value: Vec<Item>) -> Self {
        Self {
            parent_updater: None,
            inner: SharedSource::new(value),
            pending_operations: vec![].into(),
        }
    }
}

impl From<Item> for Children {
    fn from(value: Item) -> Self {
        let mut children = Children::new();
        children.add_item(value);
        children
    }
}
