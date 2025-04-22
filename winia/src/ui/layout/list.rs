use std::ops::AddAssign;
use crate::shared::{Children, Gettable, Observable, Settable, Shared, SharedUnSend};
use crate::ui::app::WindowContext;
use crate::ui::item::{MeasureMode, Orientation, Scroller};
use crate::ui::Item;

struct ListProperty<T> {
    offset: Shared<f32>,
    start_index: Shared<usize>,
    visible_items_count: Shared<usize>,
    scroll_direction: Shared<Orientation>,
    items: Shared<Vec<T>>,
    item_builder: Shared<Box<dyn Fn(&WindowContext, Shared<Vec<T>>, usize) -> Item + Send>>,
    items_updated: Shared<bool>,
}

pub struct List<T> {
    item: Item,
    property: Shared<ListProperty<T>>,
}

impl<T: Send + 'static> List<T> {
    pub fn new(
        window_context: &WindowContext,
        items: impl Into<Shared<Vec<T>>>,
        item_builder: impl Fn(&WindowContext, Shared<Vec<T>>, usize) -> Item + Send + 'static,
    ) -> Self {
        let item_builder: Box<dyn Fn(&WindowContext, Shared<Vec<T>>, usize) -> Item + Send> =
            Box::new(item_builder);
        let property = Shared::from(ListProperty {
            offset: 0.0.into(),
            start_index: 0.into(),
            visible_items_count: 0.into(),
            scroll_direction: Orientation::Vertical.into(),
            items: items.into(),
            item_builder: Shared::from(item_builder),
            items_updated: false.into(),
        });




        let children = Children::new();
        let scroller = Shared::from(Scroller::new(
            window_context.event_loop_proxy(),
            (true, true),
        ));
        let item = Item::new(window_context, children.clone());

        let items_updated = property.lock().items_updated.clone();
        let event_loop_proxy = item.data().get_window_context().event_loop_proxy().clone();
        property.lock().items.add_observer(
            item.data().get_id(),
            Box::new(move ||{
                let mut items_updated = items_updated.lock();
                *items_updated = true;
                event_loop_proxy.request_layout();
            })
        );

        item.data()
            .set_measure({
                let property = property.clone();
                let children = children.clone();
                move |item, width_mode, height_mode| {
                    let property = property.lock();
                    // children.lock().clear();
                    if property.items_updated.get() {
                        children.lock().clear();
                        property.items_updated.set(false);
                    }
                    let items_count = property.items.lock().len();
                    let item_builder = property.item_builder.lock();
                    let (width, height) = match (width_mode, height_mode) {
                        (MeasureMode::Specified(width), MeasureMode::Specified(height)) => {
                            (width, height)
                        }
                        (MeasureMode::Specified(width), MeasureMode::Unspecified(height)) => {
                            let offset = property.offset.get();
                            let start_index = property.start_index.get();
                            let mut total_height = offset;
                            let mut visible_items_count = 0_usize;
                            for i in start_index..items_count {
                                let list_item = item_builder(item.get_window_context(), property.items.clone(), i);
                                item.measure_child(&list_item, width_mode, height_mode);
                                let item_height = list_item.data().get_measure_parameter().height;
                                if total_height + item_height > height {
                                    break;
                                }
                                total_height += item_height;
                                visible_items_count += 1;
                                children.lock().push(list_item);
                            }
                            property.visible_items_count.set(visible_items_count);

                            (width, total_height)
                        }
                        (MeasureMode::Unspecified(width), MeasureMode::Specified(height)) => {
                            (width, height)
                        }
                        (MeasureMode::Unspecified(width), MeasureMode::Unspecified(height)) => {
                            (width, height)
                        }
                    };
                    // children.lock().clear();

                    let measure_parameter = item.get_measure_parameter();
                    measure_parameter.width = width;
                    measure_parameter.height = height;
                }
            })
            .set_layout({
                let property = property.clone();
                let scroller = scroller.clone();
                let children = children.clone();
                move |item, width, height| {
                    let property = property.lock();
                    let mut offset = property.offset.lock();
                    let mut scroller = scroller.lock();

                    // while let Some(delta) = scroller.y_deltas().pop_front() {
                    //     *offset -= delta;
                    // }
                    *offset += *scroller.y_deltas();
                    *scroller.y_deltas() = 0.0;

                    if property.items.lock().is_empty() {
                        *offset = 0.0;
                        *property.start_index.lock() = 0;
                        *property.visible_items_count.lock() = 0;
                        return;
                    }


                    let item_builder = property.item_builder.lock();
                    let mut start_index = property.start_index.lock();
                    let mut visible_items_count = property.visible_items_count.lock();
                    {
                        let children = children.lock();
                        for child in children.iter() {
                            item.measure_child_by_specified(child, width, height);
                        }
                    }
                    'outer:loop {
                        let mut children = children.lock();

                        if *offset < 0.0 {
                            if children.is_empty() {
                                let list_item = item_builder(item.get_window_context(), property.items.clone(), *start_index);
                                children.push(list_item);
                            }

                            {// Handle possible blank space at the bottom after scrolling
                                let mut total_height = 0.0;
                                // Calculate the current height of all children
                                for child in children.iter() {
                                    let item_height = child.data().get_measure_parameter().height;
                                    total_height += item_height;
                                }
                                // After scrolling, new items may need to be added at the bottom
                                loop {
                                    if *offset + total_height < height {
                                        // Indicates that all current children are fully displayed and there might be blank space
                                        if *start_index + children.len() == property.items.lock().len() {
                                            // Indicates that there are no more items
                                            // Recalculate offset so that the bottom of the last item aligns with the bottom of the List
                                            // *offset = height - total_height;
                                            if *start_index == 0 {
                                                *offset = 0.0;
                                            } else {
                                                *offset = height - total_height;
                                            }
                                            continue 'outer;
                                        } else {
                                            // Indicates that there are more items
                                            let new_item_index = *start_index + children.len();
                                            let list_item = item_builder(item.get_window_context(), property.items.clone(), new_item_index);
                                            item.measure_child_by_specified(&list_item, width, height);
                                            let item_height = list_item.data().get_measure_parameter().height;
                                            children.push(list_item);
                                            total_height += item_height;
                                        }
                                    } else {
                                        // Indicates that all current children are fully displayed and there's no blank space
                                        break;
                                    }
                                }
                            }


                            let item_height = {
                                let list_item = children.get(0).unwrap();
                                list_item.data().get_measure_parameter().height
                            };
                            if *offset + item_height > 0.0 {
                                break
                            } else { 
                                *offset += item_height;
                                *start_index += 1;
                                children.remove(0);
                            }
                        } else {
                            if *start_index == 0 {
                                *offset = 0.0;
                                break;
                            }
                            let list_item = item_builder(item.get_window_context(), property.items.clone(), *start_index -1);
                            item.measure_child_by_specified(&list_item, width, height);
                            let item_height = list_item.data().get_measure_parameter().height;
                            children.insert(0, list_item);
                            *offset -= item_height;
                            *start_index -= 1;
                            if *offset < 0.0 {
                                break;
                            }
                        }
                    }

                    let mut total_height = *offset;
                    let mut new_visible_items_count = 0_usize;
                    let mut y = *offset;
                    let item_count = property.items.lock().len();
                    for i in *start_index..item_count {
                        let mut children = children.lock();
                        let index_in_children = i - *start_index;
                        if index_in_children >= children.len() {
                            let list_item = item_builder(item.get_window_context(), property.items.clone(), i);
                            children.push(list_item);
                        }
                        let list_item = children.get(index_in_children).unwrap();
                        item.measure_child_by_specified(&list_item, width, height);
                        let item_height = list_item.data().get_measure_parameter().height;
                        let item_width = list_item.data().get_measure_parameter().width;
                        list_item.data().dispatch_layout(0.0, y, item_width, item_height);
                        y += item_height;
                        total_height += item_height;
                        new_visible_items_count += 1;
                        if total_height > height {
                            break;
                        }
                    }

                    let mut children = children.lock();
                    // Remove the items that are not between start_index and start_index + visible_items_count
                    while children.len() > new_visible_items_count {
                        children.pop();
                    }
                    *visible_items_count = new_visible_items_count;
                }
            })
            .set_mouse_wheel({
                let scroller = scroller.clone();
                move |item, mouse_wheel| {
                    let mut scroller = scroller.lock();
                    scroller.update_by_mouse_wheel(mouse_wheel);
                    item.get_window_context().request_layout();
                    true
                }
            })
            .set_draw({
                let property = property.clone();
                let scroller = scroller.clone();
                move |item, canvas| {
                    let property = property.lock();
                    let start_index = property.start_index.get();
                    let visible_items_count = property.visible_items_count.get();
                    let item_count = property.items.lock().len();
                    let mut scroller = scroller.lock();
                    let display_parameter = item.get_display_parameter();

                    scroller.draw(
                        &item.get_window_context(),
                        &display_parameter,
                        canvas,
                        (0.0, item_count as f32),
                        (0.0, visible_items_count as f32),
                        (0.0, start_index as f32),
                    );
                }
            });

        Self { item, property }
    }

    pub fn item(self) -> Item {
        self.item
    }
}

pub trait ListExt<T> {
    fn list(
        self,
        items: impl Into<Shared<Vec<T>>>,
        item_builder: impl Fn(&WindowContext, Shared<Vec<T>>, usize) -> Item + Send + 'static,
    ) -> List<T>;
}

impl<T: Send + 'static> ListExt<T> for &WindowContext {
    fn list(self, items: impl Into<Shared<Vec<T>>>, item_builder: impl Fn(&WindowContext, Shared<Vec<T>>, usize) -> Item + Send + 'static) -> List<T> {
        List::new(self, items, item_builder)
    }
}
