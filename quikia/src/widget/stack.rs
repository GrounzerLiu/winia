use crate::app::SharedApp;
use crate::property::{Gettable};
use crate::ui::{Children, Gravity, Item, ItemEvent, LogicalX, measure_child, MeasureMode};

#[macro_export]
macro_rules! stack {
    ($($child:expr)+) => {
        {
            let children = $crate::children!($($child),*);
            let app = children.lock().get(0).unwrap().get_app().clone();
            $crate::widget::Stack::new(app, children)
        }
    }
}

pub struct Stack {
    item: Item,
}

impl Stack {
    pub fn new(app: SharedApp, children:Children) -> Self {
        let mut item = Item::new(
            app,
            ItemEvent::default()
                .set_measure_event(
                    |item, width_measure_mode, height_measure_mode|{
                        let mut layout_params = item.get_display_parameter().clone();

                        let mut measured_width = 0.0_f32;
                        let mut measured_height = 0.0_f32;

                        item.get_children().lock().iter_mut().for_each(|child| {
                            let (child_width_measure_mode, child_height_measure_mode) = measure_child(child,&layout_params, width_measure_mode, height_measure_mode);
                            child.measure(child_width_measure_mode, child_height_measure_mode);
                            let mut child_layout_params = child.get_display_parameter().clone();
                            child_layout_params.init_from_item(child);
                            child.set_layout_params(&child_layout_params);
                            measured_height = measured_height.max(child_layout_params.height + child_layout_params.margin_top + child_layout_params.margin_bottom);
                            measured_width = measured_width.max(child_layout_params.width + child_layout_params.margin_start + child_layout_params.margin_end);
                        });

                        match width_measure_mode {
                            MeasureMode::Specified(width) => {
                                layout_params.width = width;
                            }
                            MeasureMode::Unspecified(width) => {
                                layout_params.width = width.min(measured_width);
                                
                            }
                        }

                        match height_measure_mode {
                            MeasureMode::Specified(height) => {
                                layout_params.height = height;
                            }
                            MeasureMode::Unspecified(height) => {
                                layout_params.height = height.min(measured_height);
                            }
                        }
                        
                        if let Some(background) = item.get_background().lock().as_mut() {
                            background.measure(MeasureMode::Specified(layout_params.width), MeasureMode::Specified(layout_params.height));
                        }
                        
                        if let Some(foreground) = item.get_foreground().lock().as_mut() {
                            foreground.measure(MeasureMode::Specified(layout_params.width), MeasureMode::Specified(layout_params.height));
                        }

                        item.set_layout_params(&layout_params);
                    }
                )
                .set_layout_event(
                    |item, relative_x, relative_y|{
                        let mut layout_params = item.get_display_parameter().clone();
                        layout_params.relative_x = relative_x;
                        layout_params.relative_y = relative_y;
                        item.set_layout_params(&layout_params);
                        
                        let horizontal_gravity = item.get_horizontal_gravity().get();
                        let vertical_gravity = item.get_vertical_gravity().get();

                        item.get_children().lock().iter_mut().for_each(|child| {
                            let child_layout_params = child.get_display_parameter();

                            let x = match horizontal_gravity {
                                Gravity::Start => {
                                    LogicalX::new(
                                        item.get_layout_direction().get(),
                                        child_layout_params.margin_start+layout_params.padding_start,
                                        layout_params.width)
                                }
                                Gravity::Center => {
                                    LogicalX::new(
                                        item.get_layout_direction().get(),
                                        (layout_params.width - child_layout_params.width)/2.0,
                                        layout_params.width)
                                }
                                Gravity::End => {
                                    LogicalX::new(
                                        item.get_layout_direction().get(),
                                        layout_params.width - child_layout_params.margin_end - child_layout_params.width - layout_params.padding_end,
                                        layout_params.width)
                                }
                            };
                            
                            let y = match vertical_gravity {
                                Gravity::Start => {
                                    child_layout_params.margin_top+layout_params.padding_top
                                }
                                Gravity::Center => {
                                    (layout_params.height - child_layout_params.height)/2.0
                                }
                                Gravity::End => {
                                    layout_params.height - child_layout_params.margin_bottom - child_layout_params.height - layout_params.padding_bottom
                                }
                            };
                            
                            child.layout(x.physical_value(child_layout_params.width),y)
                        });
                    }
                )
        );
        item.set_children(children);
        Stack{
            item,
        }
    }
    
    pub fn item(self) -> Item {
        self.item
    }
    
}

pub trait StackExt {
    fn stack(&self, children:Children) -> Stack;
}

impl StackExt for SharedApp {
    fn stack(&self, children:Children) -> Stack {
        Stack::new(self.clone(), children)
    }
}