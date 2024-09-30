use std::sync::{Arc, Mutex};
use crate::core::{generate_id, RefClone};
use crate::property::{Children, Gettable, GravityProperty, Observable};
use crate::ui::app::AppContext;
use crate::ui::Item;
use crate::ui::item::{Gravity, ItemEvent, LogicalX, MeasureMode, Orientation, Size};

struct StackProperty{
    horizontal_gravity: GravityProperty,
    vertical_gravity: GravityProperty,
}

pub struct Stack {
    item: Item,
    property: Arc<Mutex<StackProperty>>
}

impl Stack {
    pub fn new(app_context: AppContext, children: Children) -> Self {
        let property = Arc::new(Mutex::new(StackProperty {
            horizontal_gravity: Gravity::Start.into(),
            vertical_gravity: Gravity::Start.into(),
        }));
        let item_event = ItemEvent::default()
            .measure(|item, orientation, mode| {
                item.measure_children(orientation, mode);
                let child_max_size = {
                    let mut max_size = 0.0_f32;
                    for child in item.get_children().items().iter_mut() {
                        let child_size = child.get_measure_parameter().size(orientation);
                        let margin = child.get_margin(orientation);
                        max_size = max_size.max(child_size + margin);
                    }
                    max_size
                };
                let size = match mode {
                    MeasureMode::Specified(size) => size.clamp(item.get_min_size(orientation), item.get_max_size(orientation)),
                    MeasureMode::Unspecified(size) => {
                        let max_size = size.min(item.get_max_size(orientation));
                        let stack_size = child_max_size + item.get_padding(orientation);
                        stack_size.clamp(item.get_min_size(orientation), max_size)
                    }
                };
                item.get_measure_parameter().set_size(orientation, size);
            })
            .layout({
                let property = property.clone();
                move|item, relative_x, relative_y, width, height| {
                    
                    
                    let x = LogicalX::new(item.get_layout_direction().get(), 0.0, width);
                    let y = 0.0;

                    let horizontal_padding = item.get_padding(Orientation::Horizontal);
                    let vertical_padding = item.get_padding(Orientation::Vertical);
                    item.layout_layers(width - horizontal_padding, height - vertical_padding);
                    let padding_start = item.get_padding_start().get();
                    let padding_end = item.get_padding_end().get();
                    let padding_top = item.get_padding_top().get();
                    let padding_bottom = item.get_padding_bottom().get();
                    item.for_each_child_mut(|child| {
                        let width_property = child.get_width();
                        let height_property = child.get_height();
                        match width_property.get() {
                            Size::Stretch => {
                                child.measure(Orientation::Horizontal, MeasureMode::Specified(width - horizontal_padding));
                            }
                            Size::Relative(factor) => {
                                let factor = factor.clamp(0.0, 1.0);
                                let child_width = (width - horizontal_padding) * factor;
                                child.measure(Orientation::Horizontal, MeasureMode::Specified(child_width));
                            }
                            _ => {}
                        }
                        match height_property.get() {
                            Size::Stretch => {
                                child.measure(Orientation::Vertical, MeasureMode::Unspecified(height - vertical_padding));
                            }
                            Size::Relative(factor) => {
                                let child_height = (height - vertical_padding) * factor;
                                child.measure(Orientation::Vertical, MeasureMode::Specified(child_height));
                            }
                            _ => {}
                        }
                        let child_margin_start = child.get_margin_start().get();
                        let child_margin_end = child.get_margin_end().get();
                        let child_margin_top = child.get_margin_top().get();
                        let child_margin_bottom = child.get_margin_bottom().get();
                        let child_width = child.get_measure_parameter().width();
                        let child_height = child.get_measure_parameter().height();

                        let (horizontal_gravity, vertical_gravity) = {
                            let mut property = property.lock().unwrap();
                            (property.horizontal_gravity.get(), property.vertical_gravity.get())
                        };
                        let child_x = match horizontal_gravity {
                            Gravity::Start => {
                                x + child_margin_start + padding_start
                            }
                            Gravity::Center => {
                                x + (width - child_width) / 2.0
                            }
                            Gravity::End => {
                                x + width - child_width - child_margin_end - padding_end
                            }
                        };
                        
                        let child_y = match vertical_gravity {
                            Gravity::Start => {
                                y + child_margin_top + padding_top
                            }
                            Gravity::Center => {
                                y + (height - child_height) / 2.0
                            }
                            Gravity::End => {
                                y + height - child_height - child_margin_bottom - padding_bottom
                            }
                        };

                        child.dispatch_layout(child_x.physical_value(child_width), child_y, child_width, child_height);
                    })
                }
            });
        let item = Item::new(app_context, children, item_event);
        Self {
            item,
            property,
        }
    }
    
    // pub fn gravity(mut self, gravity: impl Into<GravityProperty>) -> Self {
    //     let mut property = self.property.lock().unwrap();
    //     property.gravity = gravity.into();
    //     let app_context = self.item.get_app_context();
    //     property.gravity.add_observer(
    //         generate_id(),
    //         Box::new(move || {
    //             app_context.request_redraw();
    //         }),
    //     );
    //     drop(property);
    //     self
    // }
    
    pub fn horizontal_gravity(mut self, gravity: impl Into<GravityProperty>) -> Self {
        let mut property = self.property.lock().unwrap();
        property.horizontal_gravity = gravity.into();
        let app_context = self.item.get_app_context();
        property.horizontal_gravity.add_observer(
            generate_id(),
            Box::new(move || {
                app_context.request_redraw();
            }),
        );
        drop(property);
        self
    }
    
    pub fn vertical_gravity(mut self, gravity: impl Into<GravityProperty>) -> Self {
        let mut property = self.property.lock().unwrap();
        property.vertical_gravity = gravity.into();
        let app_context = self.item.get_app_context();
        property.vertical_gravity.add_observer(
            generate_id(),
            Box::new(move || {
                app_context.request_redraw();
            }),
        );
        drop(property);
        self
    }
    
    pub fn item(self) -> Item {
        self.item
    }
}

impl Into<Item> for Stack {
    fn into(self) -> Item {
        self.item
    }
}

pub trait StackExt {
    fn stack(&self, children: Children) -> Stack;
}


impl StackExt for AppContext {
    
    fn stack(&self, children: Children) -> Stack {
        Stack::new(self.ref_clone(), children)
    }
}