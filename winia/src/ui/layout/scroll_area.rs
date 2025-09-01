use crate::impl_property_layout;
use crate::shared::{Children, Gettable, Settable, Shared};
use crate::ui::app::WindowContext;
use crate::ui::item::{
    HorizontalAlignment, LogicalX, MeasureMode, MouseScrollDelta, Orientation, Scroller,
    VerticalAlignment,
};
use crate::ui::Item;
use proc_macro::item;

#[derive(Clone)]
struct ScrollAreaProperty {
    scroll_content_size: Shared<(f32, f32)>,
    scroll_viewport_size: Shared<(f32, f32)>,
    // scroll_offset: Shared<(f32, f32)>,
    scroll_position: Shared<(f32, f32)>,
    horizontal_scrollable: Shared<bool>,
    vertical_scrollable: Shared<bool>,
}

#[item(children: impl Into<Children>)]
pub struct ScrollArea {
    pub(crate) item: Item,
    property: Shared<ScrollAreaProperty>,
}

impl_property_layout!(ScrollArea, horizontal_scrollable, Shared<bool>);
impl_property_layout!(ScrollArea, vertical_scrollable, Shared<bool>);
impl_property_layout!(ScrollArea, scroll_position, Shared<(f32, f32)>);

impl ScrollArea {
    pub fn new(window_context: &WindowContext, children: impl Into<Children>) -> Self {
        let property = Shared::from_static(ScrollAreaProperty {
            scroll_content_size: (0.0, 0.0).into(),
            scroll_viewport_size: (0.0, 0.0).into(),
            // scroll_offset: (0.0, 0.0).into(),
            scroll_position: (0.0, 0.0).into(),
            horizontal_scrollable: false.into(),
            vertical_scrollable: true.into(),
        });

        let item = Item::new(window_context, children.into()).clip(true);
        // bind_position_to_offset(&item, &property);
        let scroller = Shared::from(Scroller::new(
            window_context.event_loop_proxy(),
            (true, true),
        ));
        item.data()
            .set_measure({
                let property = property.clone();
                let scroller = scroller.clone();
                move |item, width_mode, height_mode| {
                    if item.get_children().lock().len() > 1 {
                        panic!("ScrollArea can only have one child");
                    }
                    let property = property.lock();
                    let horizontal_scrollable = property.horizontal_scrollable.get();
                    let vertical_scrollable = property.vertical_scrollable.get();
                    let mut scroller = scroller.lock();
                    scroller.set_scroll_enabled((horizontal_scrollable, vertical_scrollable));

                    item.measure_children(width_mode, height_mode);
                    let (child_max_width, child_max_height) = item
                        .get_children()
                        .lock()
                        .first()
                        .map_or((0.0, 0.0), |child| {
                            let margin_horizontal =
                                child.data().get_margin(Orientation::Horizontal);
                            let margin_vertical = child.data().get_margin(Orientation::Vertical);
                            let mut child_data = child.data();
                            let measure_parameter = child_data.get_measure_parameter();
                            (
                                measure_parameter.width + margin_horizontal,
                                measure_parameter.height + margin_vertical,
                            )
                        });
                    let padding_horizontal = item.get_padding(Orientation::Horizontal);
                    let padding_vertical = item.get_padding(Orientation::Vertical);
                    let width = match width_mode {
                        MeasureMode::Specified(width) => width,
                        MeasureMode::Unspecified(width) => {
                            width.min(child_max_width + padding_horizontal)
                        }
                    };
                    let height = match height_mode {
                        MeasureMode::Specified(height) => height,
                        MeasureMode::Unspecified(height) => {
                            height.min(child_max_height + padding_vertical)
                        }
                    };
                    let measure_parameter = item.get_measure_parameter();
                    measure_parameter.width = width;
                    measure_parameter.height = height;
                }
            })
            .set_layout({
                let property = property.clone();
                let scroller = scroller.clone();
                move |item, width, height| {
                    if item.get_children().lock().len() > 1 {
                        panic!("ScrollArea can only have one child");
                    }
                    if let Some(child) = item.get_children().lock().first() {
                        let property = property.lock();
                        let mut child_data = child.data();
                        let child_width = child_data.get_measure_parameter().width;
                        let child_height = child_data.get_measure_parameter().height;
                        property
                            .scroll_content_size
                            .set((child_width, child_height));
                        property
                            .scroll_viewport_size
                            .set((width.min(child_width), height.min(child_height)));
                        {
                            let mut scroller = scroller.lock();
                            let mut scroll_position = property.scroll_position.get();
                            let scroll_content_size = property.scroll_content_size.get();
                            if scroll_content_size.0 != 0.0 {
                                scroll_position.0 -= *scroller.x_deltas() / scroll_content_size.0;
                                *scroller.x_deltas() = 0.0;
                            }
                            if scroll_content_size.1 != 0.0 {
                                scroll_position.1 -= *scroller.y_deltas() / scroll_content_size.1;
                                *scroller.y_deltas() = 0.0;
                            }

                            scroll_position.0 = scroll_position.0.clamp(0.0, 1.0);
                            scroll_position.1 = scroll_position.1.clamp(0.0, 1.0);
                            property.scroll_position.set(scroll_position);
                        }

                        let scroll_position = property.scroll_position.get();
                        let scroll_content_size = property.scroll_content_size.get();
                        let scroll_viewport_size = property.scroll_viewport_size.get();
                        let x = LogicalX::new(item.get_layout_direction().get(), 0.0, width)
                            - scroll_position.0
                                * (scroll_content_size.0 - scroll_viewport_size.0)
                                    .clamp(0.0, f32::MAX);
                        let y = -scroll_position.1
                            * (scroll_content_size.1 - scroll_viewport_size.1).clamp(0.0, f32::MAX);
                        // println!("x: {:?}, y: {}, width: {}, height: {}", x, y, width, height);

                        let padding_start = item.get_padding_start().get();
                        let padding_end = item.get_padding_end().get();
                        let padding_top = item.get_padding_top().get();
                        let padding_bottom = item.get_padding_bottom().get();

                        let alignment = item.get_align_content().get();

                        let child_margin_start = child_data.get_margin_start().get();
                        let child_margin_end = child_data.get_margin_end().get();
                        let child_margin_top = child_data.get_margin_top().get();
                        let child_margin_bottom = child_data.get_margin_bottom().get();
                        drop(child_data);

                        let child_x = match alignment.to_horizontal_alignment() {
                            HorizontalAlignment::Start => x + child_margin_start + padding_start,
                            HorizontalAlignment::Center => x + (width - child_width) / 2.0,
                            HorizontalAlignment::End => {
                                x + width - child_width - child_margin_end - padding_end
                            }
                        };

                        let child_y = match alignment.to_vertical_alignment() {
                            VerticalAlignment::Top => y + child_margin_top + padding_top,
                            VerticalAlignment::Center => y + (height - child_height) / 2.0,
                            VerticalAlignment::Bottom => {
                                y + height - child_height - child_margin_bottom - padding_bottom
                            }
                        };

                        child.data().dispatch_layout(
                            child_x.physical_value(child_width),
                            child_y,
                            child_width,
                            child_height,
                        );
                    }
                }
            })
            .set_mouse_wheel_y({
                let property = property.clone();
                let scroller = scroller.clone();
                move |item, mouse_wheel| {
                    let scroll_position = property.lock().scroll_position.get();
                    let mut scroller = scroller.lock();
                    match mouse_wheel.delta {
                        MouseScrollDelta::LineDelta(y) => {
                            if y < 0.0 && scroll_position.1 >= 1.0 {
                                return false;
                            }
                            if y > 0.0 && scroll_position.1 <= 0.0 {
                                return false;
                            }
                        }
                        MouseScrollDelta::LogicalDelta(y) => {
                            if y < 0.0 && scroll_position.1 >= 1.0 {
                                return false;
                            }
                            if y > 0.0 && scroll_position.1 <= 0.0 {
                                return false;
                            }
                        }
                    }
                    scroller.update_by_mouse_wheel_y(mouse_wheel);
                    item.get_window_context().request_layout();
                    true
                }
            })
            .set_mouse_wheel_x({
                let property = property.clone();
                let scroller = scroller.clone();
                move |item, mouse_wheel| {
                    let scroll_position = property.lock().scroll_position.get();
                    let mut scroller = scroller.lock();
                    match mouse_wheel.delta {
                        MouseScrollDelta::LineDelta(x) => {
                            if x < 0.0 && scroll_position.0 >= 1.0 {
                                return false;
                            }
                            if x > 0.0 && scroll_position.0 <= 0.0 {
                                return false;
                            }
                        }
                        MouseScrollDelta::LogicalDelta(x) => {
                            if x < 0.0 && scroll_position.0 >= 1.0 {
                                return false;
                            }
                            if x > 0.0 && scroll_position.0 <= 0.0 {
                                return false;
                            }
                        }
                    }
                    scroller.update_by_mouse_wheel_x(mouse_wheel);
                    item.get_window_context().request_layout();
                    true
                }
            });

        let dispatch_draw = item.data().get_dispatch_draw();
        item.data().set_dispatch_draw({
            let scroller = scroller.clone();
            let property = property.clone();
            move |item, surface, x, y| {
                // let window_context = item.get_window_context();
                dispatch_draw.lock()(item, surface, x, y);
                let display_parameter = item.get_display_parameter();
                let child_display_parameter = item
                    .get_children()
                    .lock()
                    .first()
                    .map(|child| child.data().get_display_parameter());

                if let Some(child_display_parameter) = child_display_parameter {
                    let mut scroller = scroller.lock();
                    let content_size = (
                        child_display_parameter.width,
                        child_display_parameter.height,
                    );
                    let canvas = surface.canvas();
                    let property = property.lock();
                    // scroller.draw(
                    //     item.get_window_context(),
                    //     &display_parameter,
                    //     canvas,
                    //     property.scroll_content_size.get(),
                    //     property.scroll_viewport_size.get(),
                    //     property.scroll_offset.get(),
                    // );
                }
            }
        });
        Self { item, property }
    }

    // pub fn scroll_position(self, scroll_position: impl Into<Shared<(f32, f32)>>) -> Self {
    //     let id = self.item.data().get_id();
    //     self.property.lock().scroll_position.remove_observer(id);
    //     self.property.lock().scroll_offset.remove_observer(id);
    //     self.property.lock().scroll_position = scroll_position.into();
    //     // bind_position_to_offset(&self.item, &self.property);
    //     self
    // }
}

//
// fn bind_position_to_offset(item: &Item, property: &Shared<ScrollAreaProperty>)         {
//     let id = item.data().get_id();
//     let event_loop_proxy = item.data().get_window_context().event_loop_proxy().clone();
//     let property = property.lock();
//     let scroll_offset = property.scroll_offset.clone();
//     let scroll_viewport_size = property.scroll_viewport_size.clone();
//     let scroll_content_size = property.scroll_content_size.clone();
//     let scroll_position = property.scroll_position.clone();
//     let mut last_scroll_offset = scroll_offset.get();
//     scroll_offset.add_specific_observer(
//         id,
//         {
//             clone!(scroll_position, scroll_viewport_size, scroll_content_size, event_loop_proxy);
//             move |(x_offset, y_offset)| {
//                 if last_scroll_offset == (*x_offset, *y_offset) {
//                     return;
//                 }
//                 last_scroll_offset = (*x_offset, *y_offset);
//                 let scroll_viewport_size = scroll_viewport_size.get();
//                 let scroll_content_size = scroll_content_size.get();
//                 let new_scroll_position_x = {
//                     let new_scroll_position_x = *x_offset / (scroll_content_size.0 - scroll_viewport_size.0);
//                     new_scroll_position_x.clamp(0.0, 1.0)
//                 };
//                 let new_scroll_position_y = {
//                     let new_scroll_position_y = *y_offset / (scroll_content_size.1 - scroll_viewport_size.1);
//                     new_scroll_position_y.clamp(0.0, 1.0)
//                 };
//                 scroll_position.try_set_static((new_scroll_position_x, new_scroll_position_y));
//                 event_loop_proxy.request_layout();
//             }
//         }
//     );
//     let mut last_scroll_position = scroll_position.get();
//     scroll_position.add_specific_observer(
//         id,
//         {
//             clone!(scroll_offset, scroll_viewport_size, scroll_content_size, event_loop_proxy);
//             move |(x_position, y_position)| {
//                 if last_scroll_position == (*x_position, *y_position) {
//                     return;
//                 }
//                 last_scroll_position = (*x_position, *y_position);
//                 let scroll_viewport_size = scroll_viewport_size.get();
//                 let scroll_content_size = scroll_content_size.get();
//                 let x_position = x_position.clamp(0.0, 1.0);
//                 let y_position = y_position.clamp(0.0, 1.0);
//                 let new_scroll_offset_x = x_position * (scroll_content_size .0 - scroll_viewport_size.0);
//                 let new_scroll_offset_y = y_position * (scroll_content_size.1 - scroll_viewport_size.1);
//                 scroll_offset.try_set_static((new_scroll_offset_x, new_scroll_offset_y));
//                 event_loop_proxy.request_layout();
//             }
//         }
//     )
// }
