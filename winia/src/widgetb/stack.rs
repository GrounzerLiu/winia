use crate::app::SharedApp;
use crate::property::{Gettable};
use crate::uib::{Children, ChildrenManager, Gravity, Item, ItemEvent, LogicalX, measure_child, MeasureMode};

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
                        let mut display_parameter = item.get_display_parameter();

                        let mut measured_width = 0.0_f32;
                        let mut measured_height = 0.0_f32;

                        item.get_children().lock().iter_mut().for_each(|child| {
                            let (child_width_measure_mode, child_height_measure_mode) = measure_child(child, &display_parameter, width_measure_mode, height_measure_mode);
                            child.measure(child_width_measure_mode, child_height_measure_mode);
                            child.init_display_parameter();
                            let mut child_display_parameter = child.get_display_parameter();
                            measured_height = measured_height.max(child_display_parameter.height + child_display_parameter.margin_top + child_display_parameter.margin_bottom);
                            measured_width = measured_width.max(child_display_parameter.width + child_display_parameter.margin_start + child_display_parameter.margin_end);
                        });
                        
                        match width_measure_mode {
                            MeasureMode::Specified(width) => {
                                item.get_display_parameter_mut().width = width;
                            }
                            MeasureMode::Unspecified(width) => {
                                item.get_display_parameter_mut().width = width.min(measured_width);
                                
                            }
                        }

                        match height_measure_mode {
                            MeasureMode::Specified(height) => {
                                item.get_display_parameter_mut().height = height;
                            }
                            MeasureMode::Unspecified(height) => {
                                item.get_display_parameter_mut().height = height.min(measured_height);
                            }
                        }
                        
                        if let Some(background) = item.get_background().lock().unwrap().as_mut() {
                            background.measure(MeasureMode::Specified(display_parameter.width), MeasureMode::Specified(display_parameter.height));
                        }
                        
                        if let Some(foreground) = item.get_foreground().lock().unwrap().as_mut() {
                            foreground.measure(MeasureMode::Specified(display_parameter.width), MeasureMode::Specified(display_parameter.height));
                        }

                        // item.set_display_parameter(&layout_params);
                    }
                )
                .set_layout_event(
                    |item, relative_x, relative_y|{
                        // println!("Stack layout");
                        {
                            let mut display_parameter = item.get_display_parameter_mut();
                            display_parameter.relative_x = relative_x;
                            display_parameter.relative_y = relative_y;
                        }
                        
                        let display_parameter = item.get_display_parameter();
                        
                        let horizontal_gravity = item.get_horizontal_gravity().get();
                        let vertical_gravity = item.get_vertical_gravity().get();

                        item.get_children().lock().iter_mut().for_each(|child| {
                            let child_layout_params = child.get_display_parameter();

                            let x = match horizontal_gravity {
                                Gravity::Start => {
                                    LogicalX::new(
                                        item.get_layout_direction().get(),
                                        child_layout_params.margin_start+ display_parameter.padding_start,
                                        display_parameter.width)
                                }
                                Gravity::Center => {
                                    LogicalX::new(
                                        item.get_layout_direction().get(),
                                        (display_parameter.width - child_layout_params.width)/2.0,
                                        display_parameter.width)
                                }
                                Gravity::End => {
                                    LogicalX::new(
                                        item.get_layout_direction().get(),
                                        display_parameter.width - child_layout_params.margin_end - child_layout_params.width - display_parameter.padding_end,
                                        display_parameter.width)
                                }
                            };
                            
                            let y = match vertical_gravity {
                                Gravity::Start => {
                                    child_layout_params.margin_top+ display_parameter.padding_top
                                }
                                Gravity::Center => {
                                    (display_parameter.height - child_layout_params.height)/2.0
                                }
                                Gravity::End => {
                                    display_parameter.height - child_layout_params.margin_bottom - child_layout_params.height - display_parameter.padding_bottom
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
    fn stack(&self, children: Children) -> Stack;
}

impl StackExt for SharedApp {
    fn stack(&self, children: Children) -> Stack {
        Stack::new(self.clone(), children)
    }
}