use crate::shared::{Children, Gettable, Settable, Shared, SharedItem, SharedUnSend};
use crate::ui::app::WindowContext;
use crate::ui::item::{HorizontalAlignment, LogicalX, MeasureMode, MouseScrollDelta, Orientation, Scroller, Size, VerticalAlignment};
use crate::ui::Item;
use proc_macro::item;
use crate::impl_property_layout;

#[derive(Clone)]
struct ScrollAreaProperty {
    scroll_range: Shared<(f32, f32)>,
    scroll_extent: Shared<(f32, f32)>,
    scroll_position: Shared<(f32, f32)>,
    horizontal_scrollable: Shared<bool>,
    vertical_scrollable: Shared<bool>,
}

#[item(children: impl Into<Children>)]
pub struct ScrollArea {
    item: Item,
    property: Shared<ScrollAreaProperty>,
}

impl_property_layout!(ScrollArea, horizontal_scrollable, Shared<bool>);
impl_property_layout!(ScrollArea, vertical_scrollable, Shared<bool>);

impl ScrollArea {
    pub fn new(window_context: &WindowContext, children: impl Into<Children>) -> Self {
        let property = Shared::from_static(ScrollAreaProperty {
            scroll_range: (0.0, 0.0).into(),
            scroll_extent: (0.0, 0.0).into(),
            scroll_position: (0.0, 0.0).into(),
            horizontal_scrollable: false.into(),
            vertical_scrollable: true.into(),
        });

        let item = Item::new(window_context, children.into());
        let scroller = Shared::from(Scroller::new(
            window_context.event_loop_proxy(),
            (true, true)
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
                    scroller.set_scroll_enabled(
                        (horizontal_scrollable, vertical_scrollable),
                    );

                    item.measure_children(width_mode, height_mode);
                    let (child_max_width, child_max_height) = item.get_children().lock().first().map_or((0.0, 0.0), |child| {
                        let margin_horizontal = child.data().get_margin(Orientation::Horizontal);
                        let margin_vertical = child.data().get_margin(Orientation::Vertical);
                        let mut child_data = child.data();
                        let measure_parameter = child_data.get_measure_parameter();
                        (
                            measure_parameter.width + margin_horizontal,
                            measure_parameter.height + margin_vertical
                        )
                    });
                    let padding_horizontal = item.get_padding(Orientation::Horizontal);
                    let padding_vertical = item.get_padding(Orientation::Vertical);
                    let width = match width_mode {
                        MeasureMode::Specified(width) => width,
                        MeasureMode::Unspecified(width) => width.min(child_max_width + padding_horizontal),
                    };
                    let height = match height_mode {
                        MeasureMode::Specified(height) => height,
                        MeasureMode::Unspecified(height) => height.min(child_max_height + padding_vertical),
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
                    if let Some(child) = item .get_children().lock().first() {
                        let property = property.lock();
                        {
                            let mut scroll_position = property.scroll_position.lock();

                            let mut scroller = scroller.lock();
                            while let Some(delta) = scroller.x_deltas().pop_front() {
                                scroll_position.0 += delta;
                            }
                            
                            while let Some(delta) = scroller.y_deltas().pop_front() {
                                scroll_position.1 += delta;
                            }
                            
                            let scroll_range = property.scroll_range.get();
                            let scroll_extent = property.scroll_extent.get();
                            
                            if scroll_position.0 < 0.0 {
                                scroll_position.0 = 0.0;
                            } else if scroll_position.0 > scroll_range.0 - scroll_extent.0 {
                                scroll_position.0 = scroll_range.0 - scroll_extent.0;
                            }
                            if scroll_position.1 < 0.0 {
                                scroll_position.1 = 0.0;
                            } else if scroll_position.1 > scroll_range.1 - scroll_extent.1 {
                                scroll_position.1 = scroll_range.1 - scroll_extent.1;
                            }
                        }
                        
                        let scroll_position = property.scroll_position.get();
                        
                        let x = LogicalX::new(item.get_layout_direction().get(), 0.0, width)
                            - scroll_position.0;
                        let y = -scroll_position.1;

                        let padding_start = item.get_padding_start().get();
                        let padding_end = item.get_padding_end().get();
                        let padding_top = item.get_padding_top().get();
                        let padding_bottom = item.get_padding_bottom().get();

                        let alignment = item.get_align_content().get();

                        let mut child_data = child.data();
                        let child_margin_start = child_data.get_margin_start().get();
                        let child_margin_end = child_data.get_margin_end().get();
                        let child_margin_top = child_data.get_margin_top().get();
                        let child_margin_bottom = child_data.get_margin_bottom().get();
                        let child_width = child_data.get_measure_parameter().width;
                        let child_height = child_data.get_measure_parameter().height;
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

                        // scroller.set_scroll_range((child_width, child_height));
                        // scroller.set_scroll_extent((width.min(child_width), height.min(child_height)));
                        
                        property.scroll_range.set((child_width, child_height));
                        property.scroll_extent.set((width.min(child_width), height.min(child_height)));
                    }
                }
            })
            .set_mouse_wheel({
                let property = property.clone();
                let scroller = scroller.clone();
                move |item, mut mouse_wheel| {
                    // let property = property.lock();
                    // let scroll_range = property.scroll_range.get();
                    // let scroll_extent = property.scroll_extent.get();
                    // let scroll_position = property.scroll_position.get();
                    // let is_x_scrollable = scroll_range.0 + scroll_extent.0 < scroll_range.0;
                    // let is_y_scrollable = scroll_range.1 + scroll_extent.1 < scroll_range.1;
                    // let scroller = scroller.lock();
                    // match &mouse_wheel.delta {
                    //     MouseScrollDelta::LineDelta(x, y) => {
                    //         let x = if is_x_scrollable { *x } else { 0.0 };
                    //         let y = if is_y_scrollable { *y } else { 0.0 };
                    //         
                    //     }
                    //     MouseScrollDelta::LogicalDelta(x, y) => {}
                    // }
                    let mut scroller = scroller.lock();
                    scroller.update_by_mouse_wheel(mouse_wheel);
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
                let child_display_parameter =
                    if let Some(child) = item.get_children().lock().first() {
                        Some(child.data().get_display_parameter())
                    } else {
                        None
                    };

                if let Some(child_display_parameter) = child_display_parameter {
                    let mut scroller = scroller.lock();
                    let content_size = (
                        child_display_parameter.width,
                        child_display_parameter.height,
                    );
                    let canvas = surface.canvas();
                    let property = property.lock();
                    scroller.draw(item.get_window_context(), &display_parameter, canvas,
                        property.scroll_range.get(),
                        property.scroll_extent.get(),
                        property.scroll_position.get(),
                    );
                }
            }
        });
        Self { item, property }
    }
}
