/*use crate::shared::{SharedDerived, SharedDerivedWVec, SharedSource, SharedUsize};
use crate::ui::item::{ItemEvent, ItemKind, ItemProps, MeasureMode, PhysicalX, SetCustomProp};
use crate::ui::{Alignment, HorizontalAlignment, Item, LazyListState, Orientation};
use crate::define_props;
use clonelet::clone;
use proc_macro::ItemProps;
use crate::collection::{CollectionOperation, Operable};

/*define_props! {
    LazyListPropsTrait;
    lazy_column_props;
    LazyListProps {
        list_state: SharedSource<LazyListState>,
        list_items: Box<dyn Operable>,
        orientation: SharedDerived<Orientation>,
    }
}*/

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListDirection {
    TopToBottom,
    BottomToTop,
    StartToEnd,
    EndToStart,
}

#[derive(ItemProps)]
pub struct LazyListProps<T: Send + Sync + 'static> {
    pub item_props: ItemProps,
    pub direction: SharedDerived<ListDirection>,
    pub list_state: SharedDerived<LazyListState>,
    #[not_shared]
    #[constructor]
    pub list_items: SharedDerivedWVec<T>,
    #[not_shared]
    #[constructor(type = impl Fn(usize, &T) -> Item + Send + Sync + 'static)]
    pub item_generator: SharedSource<Box<dyn Fn(usize, &T) -> Item + Send + Sync + 'static>>,
    #[not_shared]
    pub on_list_state_change: SharedSource<Option<Box<dyn Fn(LazyListState) + Send + Sync + 'static>>>,
}

impl<T: Send + Sync + 'static> LazyListProps<T> {
    pub fn new(
        item_props: ItemProps, list_items: impl Into<SharedDerivedWVec<T>>,
        item_generator: impl Fn(usize, &T) -> Item + Send + Sync + 'static,
    ) -> Self {
        Self {
            item_props,
            direction: ListDirection::TopToBottom.into(),
            list_state: SharedDerived::new_derived(LazyListState::new()),
            list_items: list_items.into(),
            item_generator: SharedSource::new(Box::new(item_generator)),
        }
    }

    pub fn on_list_state_change(
        mut self,
        callback: impl Fn(LazyListState) + Send + Sync + 'static,
    ) -> Self {
        self.on_list_state_change = SharedSource::new(Some(Box::new(callback)));
        self
    }
}

pub fn lazy_list<T: Send + Sync + 'static>(
    props: LazyListProps<T>,
) -> Item {
    Item::new(
        ItemKind::Container,
        item_event(&props),
        props,
        vec![],
    )
}

fn item_event<T: Send + Sync + 'static>(props: &LazyListProps<T>) -> ItemEvent {
    ItemEvent::new()
        .set_measure({
            clone!(props.list_state, props.list_items, props.item_generator);
            move |item, width_mode, height_mode| {
                let padding_v = item.get_padding(Orientation::Vertical);
                let padding_h = item.get_padding(Orientation::Horizontal);
                let mut max_width = 0_f32;
                let mut max_height = 0_f32;
                let mut children = item.children().lock();
                let (children_max_width, children_total_height) = measure_children(
                    &mut children,
                    &list_state,
                    &list_items,

                    
                )
            }
        })
        .set_layout({
            clone!(props.padding, props.layout_direction);
            move |item, width, height| {
                let padding_start = padding.start.get();
                let padding_end = padding.end.get();
                let padding_top = padding.top.get();
                let padding_bottom = padding.bottom.get();
                let layout_direction = layout_direction.get();

                let children = item.children();
                {
                    let children = children.lock();
                    for child in children.iter() {
/*                        let mut child_data = child.data();
                        let alignment = child_data
                            .props()
                            .get_custom::<SharedDerived<Alignment>>("align_self")
                            .cloned()
                            .unwrap_or_else(|| alignment.clone())
                            .get();

                        let measure_frame = &child_data.measure_frame;
                        let child_width = measure_frame.width;
                        let child_height = measure_frame.height;

                        let x = match alignment.horizontal() {
                            HorizontalAlignment::Start => padding_start,
                            HorizontalAlignment::Center => (width - child_width) / 2.0,
                            HorizontalAlignment::End => width - child_width - padding_end,
                        };
                        let y = match alignment.vertical() {
                            crate::ui::VerticalAlignment::Top => padding_top,
                            crate::ui::VerticalAlignment::Center => (height - child_height) / 2.0,
                            crate::ui::VerticalAlignment::Bottom => {
                                height - child_height - padding_bottom
                            }
                        };
                        child_data.dispatch_layout(
                            x.physical_x(layout_direction, width, child_width),
                            y,
                            child_width,
                            child_height,
                        );*/
                    }
                }
            }
        })
}

fn measure_children<T: Send + Sync + 'static>(
    children: &mut Vec<Item>,
    list_state: &SharedDerived<LazyListState>,
    list_items: &mut SharedDerivedWVec<T>,
    item_generator: &SharedSource<Box<dyn Fn(usize, &T) -> Item + Send + Sync + 'static>>,
    on_list_state_change: &SharedSource<Option<Box<dyn Fn(LazyListState) + Send + Sync + 'static>>>,
    max_width: f32,
    max_height: f32,
) -> (f32, f32)
{
    let list_items_locked = list_items.lock();
    let count = list_items_locked.len();
    let mut new_list_state = list_state.get();
    let mut children_max_width = 0_f32;
    let mut children_total_height = 0_f32;
    // Measure children
    for child in children.iter_mut() {
        measure_item(child, max_width, max_height);
        let child_data = child.data();
        children_max_width = children_max_width.max(child_data.measure_frame.width);
        children_total_height += child_data.measure_frame.height;
    }

    // Remove the invisible items in front
/*    loop {
        let first_item = children.pop();
        if let Some(first_item) = first_item {
            let first_item_height = first_item.data().measure_frame.height;
            if -new_list_state.offset >= first_item_height { // Still invisible
                new_list_state.offset += first_item_height;
            } else {
                children.insert(0, first_item);
                break;
            }
        } else {
            break;
        }
    }*/
    loop {
        let first_item = children.first();
        if let Some(first_item) = first_item {
            let first_item_height = first_item.data().measure_frame.height;
            if -new_list_state.offset >= first_item_height { // Still invisible
                children.remove(0);
                new_list_state.offset += first_item_height;
                new_list_state.visible_range.start += 1;
            } else {
                break;
            }
        } else {
            break;
        }
    }

    let item = item.lock();
    let mut operations = list_items.take_operations();
    let first_update_all = operations.iter().enumerate().find(|(_, op)| matches!(op, CollectionOperation::UpdateAll));
    if let Some((index, _)) = first_update_all {
        // If there is an UpdateAll operation, we can skip all previous operations
        operations.drain(0..index);
    }

    for operation in operations {
        match operation {
            CollectionOperation::Add(index) => {
                if index >= new_list_state.visible_range.start && index < new_list_state.visible_range.end {
                    let new_item = item(index);
                    measure_item(&new_item, max_width, max_height);
                    let item_height = new_item.data().measure_frame.height;
                    let item_width = new_item.data().measure_frame.width;
                    children.insert(index - new_list_state.visible_range.start, new_item);
                    children_total_height += item_height;
                    children_max_width = children_max_width.max(item_width);
                    new_list_state.visible_range.end += 1;
                }
            }
            CollectionOperation::Remove(index) => {
                if index >= new_list_state.visible_range.start && index < new_list_state.visible_range.end {
                    let removed_item = children.remove(index - new_list_state.visible_range.start);
                    let item_height = removed_item.data().measure_frame.height;
                    children_total_height -= item_height;
                    new_list_state.visible_range.end -= 1;
                }
            }
            CollectionOperation::Update(index) => {
                if index >= new_list_state.visible_range.start && index < new_list_state.visible_range.end {
                    let new_item = item(index);
                    measure_item(&new_item, max_width, max_height);
                    let item_height = new_item.data().measure_frame.height;
                    let item_width = new_item.data().measure_frame.width;
                    let old_item = &mut children[index - new_list_state.visible_range.start];
                    let old_item_height = old_item.data().measure_frame.height;
                    children_total_height = children_total_height - old_item_height + item_height;
                    children_max_width = children_max_width.max(item_width);
                    *old_item = new_item;
                }
            }
            CollectionOperation::UpdateAll => {
                children.clear();
                children_total_height = 0.0;
                children_max_width = 0.0;
                new_list_state.visible_range.end = new_list_state.visible_range.start;
            }
            CollectionOperation::Clear => {
                children.clear();
                children_total_height = 0.0;
                children_max_width = 0.0;
                new_list_state.offset = 0.0;
                new_list_state.visible_range.start = 0;
                new_list_state.visible_range.end = 0;
            }
        }
    }
    
    // Add or remove children to fill the space
    loop {
        if children_total_height + new_list_state.offset < max_height {
            if new_list_state.visible_range.end >= count {
                break;
            }
            let new_item = item(new_list_state.visible_range.end);
            measure_item(&new_item, max_width, max_height);
            let item_height = new_item.data().measure_frame.height;
            let item_width = new_item.data().measure_frame.width;
            children.push(new_item);
            children_total_height += item_height;
            children_max_width = children_max_width.max(item_width);
            new_list_state.visible_range.end += 1;
        } else {
            break;
        }
    }
    loop {
        let last = children.last();
        if let Some(last) = last {
            let last_height = last.data().measure_frame.height;
            if children_total_height + new_list_state.offset - last_height >= max_height {
                children.pop();
                children_total_height -= last_height;
                new_list_state.visible_range.end -= 1;
            } else {
                break;
            }
        }
    }

/*    list_state.set(new_list_state);*/
    if let Some(callback) = &*on_list_state_change.lock() {
        callback(new_list_state);
    }

    (children_max_width, children_total_height)
}
fn measure_item(item: &Item, max_width: f32, max_height: f32) {
    let mut item_data = item.data();
    let item_width = item_data.props().width.get();
    let item_height = item_data.props().height.get();
    item_data.dispatch_measure(
        item_width.create_measure_mode(max_width),
        item_height.create_measure_mode(max_height)
    );
}*/