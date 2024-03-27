use crate::app::SharedApp;
use crate::impl_item_property;
use crate::item::{Gravity, Item, ItemEvent, LayoutDirection, measure_child, MeasureMode};
use crate::property::{Gettable, SharedProperty};

#[macro_export]
macro_rules! axis_layout {
    ($($child:expr)*) => {
        $crate::layout::AxisLayout::new(vec![$($child),*])
    }
}


#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Axis {
    Horizontal,
    Vertical,
}

pub type AxisProperty = SharedProperty<Axis>;

pub struct AxisLayout {
    item: Item,
    axis: AxisProperty,
}

impl AxisLayout {
    pub fn new(children: Vec<Item>) -> Self {
        let axis: AxisProperty = Axis::Horizontal.into();
        let mut item = Item::new(
            ItemEvent::default()
                .set_on_measure(
                    {
                        let axis = axis.clone();
                        move |item, width_measure_mode, height_measure_mode| {
                            let mut display_parameter = item.get_display_parameter_mut().clone();
                            display_parameter.init_from_item(item);
                            let max_width = display_parameter.max_width;
                            let max_height = display_parameter.max_height;
                            let min_width = display_parameter.min_width;
                            let min_height = display_parameter.min_height;
                            match axis.get() {
                                Axis::Horizontal => {
                                    let mut width = 0.0;
                                    let mut height = 0.0_f32;

                                    let mut remaining_width = match width_measure_mode {
                                        MeasureMode::Exactly(width) => width,
                                        MeasureMode::AtMost(width) => width,
                                    };

                                    let measure =|child:&mut Item| {
                                        let width_measure_mode = match width_measure_mode {
                                            MeasureMode::Exactly(_) => MeasureMode::Exactly(remaining_width),
                                            MeasureMode::AtMost(_) => MeasureMode::AtMost(remaining_width),
                                        };

                                        let mut child_occupied_width = 0.0;
                                        let (child_width_measure_mode, child_height_measure_mode) = measure_child(child,display_parameter, width_measure_mode, height_measure_mode);
                                        child.measure(child_width_measure_mode, child_height_measure_mode);
                                        let child_display_parameter = child.get_display_parameter();

                                        child_occupied_width = child_display_parameter.width + child_display_parameter.margin_start + child_display_parameter.margin_end;
                                        height = height.max(child_display_parameter.height + child_display_parameter.margin_top + child_display_parameter.margin_bottom);

                                        width += child_occupied_width;

                                        if remaining_width - child_occupied_width < 0.0 {
                                            remaining_width = 0.0;
                                        } else {
                                            remaining_width -= child_occupied_width;
                                        }
                                    };

                                    match item.get_layout_direction().get() {
                                        LayoutDirection::LeftToRight => {
                                            item.get_children_mut().iter_mut().for_each(measure);
                                        }
                                        LayoutDirection::RightToLeft => {
                                            item.get_children_mut().iter_mut().rev().for_each(measure);
                                        }
                                    };
                                    match width_measure_mode {
                                        MeasureMode::Exactly(measured_width) => {
                                            display_parameter.width = measured_width.clamp(min_width, max_width);
                                        }
                                        MeasureMode::AtMost(measured_width) => {
                                            display_parameter.width = measured_width.min(width).clamp(min_width, max_width);
                                        }
                                    }

                                    match height_measure_mode {
                                        MeasureMode::Exactly(measured_height) => {
                                            display_parameter.height = measured_height.clamp(min_height, max_height);
                                        }
                                        MeasureMode::AtMost(measured_height) => {
                                            display_parameter.height = measured_height.min(height).clamp(min_height, max_height);
                                        }
                                    }
                                }
                                Axis::Vertical => {
                                    let mut width = 0.0_f32;
                                    let mut height = 0.0;

                                    let mut remaining_height = match height_measure_mode {
                                        MeasureMode::Exactly(height) => height,
                                        MeasureMode::AtMost(height) => height,
                                    };

                                    item.get_children_mut().iter_mut().for_each(|child| {
                                        let height_measure_mode = match height_measure_mode {
                                            MeasureMode::Exactly(_) => MeasureMode::Exactly(remaining_height),
                                            MeasureMode::AtMost(_) => MeasureMode::AtMost(remaining_height),
                                        };

                                        let mut child_occupied_height = 0.0;
                                        let (child_width_measure_mode, child_height_measure_mode) = measure_child(child,display_parameter, width_measure_mode, height_measure_mode);
                                        child.measure(child_width_measure_mode, child_height_measure_mode);
                                        let child_display_parameter = child.get_display_parameter();

                                        child_occupied_height = child_display_parameter.height + child_display_parameter.margin_top + child_display_parameter.margin_bottom;
                                        width = width.max(child_display_parameter.width + child_display_parameter.margin_start + child_display_parameter.margin_end);

                                        height += child_occupied_height;

                                        if remaining_height - child_occupied_height < 0.0 {
                                            remaining_height = 0.0;
                                        } else {
                                            remaining_height -= child_occupied_height;
                                        }
                                    });

                                    match width_measure_mode {
                                        MeasureMode::Exactly(measured_width) => {
                                            display_parameter.width = measured_width.clamp(min_width, max_width);
                                        }
                                        MeasureMode::AtMost(measured_width) => {
                                            display_parameter.width = measured_width.min(width).clamp(min_width, max_width);
                                        }
                                    }

                                    match height_measure_mode {
                                        MeasureMode::Exactly(measured_height) => {
                                            display_parameter.height = measured_height.clamp(min_height, max_height);
                                        }
                                        MeasureMode::AtMost(measured_height) => {
                                            display_parameter.height = measured_height.min(height).clamp(min_height, max_height);
                                        }
                                    }
                                }
                            }

                            item.set_display_parameter(display_parameter);
                        }
                    }
                )
                .set_on_layout(
                    {
                        let axis = axis.clone();
                        move |item, x, y| {
                            let mut display_parameter = item.get_display_parameter().clone();
                            display_parameter.x = x;
                            display_parameter.y = y;
                            item.set_display_parameter(display_parameter);
                            let direction = item.get_layout_direction().get();
                            let vertical_gravity = item.get_vertical_gravity().get();
                            match axis.get() {
                                Axis::Horizontal => {
                                    let children_occupied_width = item.get_children().iter().fold(0.0, |acc, child| {
                                        let child_display_parameter = child.get_display_parameter();
                                        acc + child_display_parameter.width + child_display_parameter.margin_start + child_display_parameter.margin_end
                                    });
                                    match direction {
                                        LayoutDirection::LeftToRight => {
                                            let mut x = match item.get_horizontal_gravity().get() {
                                                Gravity::Start => {
                                                    display_parameter.x
                                                }
                                                Gravity::Center => {
                                                    display_parameter.x + (display_parameter.width - children_occupied_width) / 2.0
                                                }
                                                Gravity::End => {
                                                    display_parameter.x + display_parameter.width - children_occupied_width
                                                }
                                            };
                                            item.get_children_mut().iter_mut().for_each(|child| {
                                                let child_display_parameter = child.get_display_parameter().clone();
                                                let child_x = x + child_display_parameter.margin_start;
                                                let child_y = match vertical_gravity {
                                                    Gravity::Start => {
                                                        display_parameter.y + child_display_parameter.margin_top
                                                    }
                                                    Gravity::Center => {
                                                        display_parameter.y + (display_parameter.height - child_display_parameter.height) / 2.0
                                                    }
                                                    Gravity::End => {
                                                        display_parameter.y + display_parameter.height - child_display_parameter.height - child_display_parameter.margin_bottom
                                                    }
                                                };
                                                child.layout(child_x, child_y);
                                                x += child_display_parameter.width + child_display_parameter.margin_start + child_display_parameter.margin_end;
                                            });
                                        }
                                        LayoutDirection::RightToLeft => {
                                            let mut x = match item.get_horizontal_gravity().get() {
                                                Gravity::Start => {
                                                    display_parameter.x + display_parameter.x + display_parameter.width
                                                }
                                                Gravity::Center => {
                                                    display_parameter.x + display_parameter.width - (display_parameter.width - children_occupied_width) / 2.0
                                                }
                                                Gravity::End => {
                                                    display_parameter.x + display_parameter.width
                                                }
                                            };
                                            item.get_children_mut().iter_mut().for_each(|child| {
                                                let child_display_parameter = child.get_display_parameter().clone();
                                                let child_x = x - child_display_parameter.margin_start - child_display_parameter.width;
                                                let child_y = match vertical_gravity {
                                                    Gravity::Start => {
                                                        display_parameter.y + child_display_parameter.margin_top
                                                    }
                                                    Gravity::Center => {
                                                        display_parameter.y + (display_parameter.height - child_display_parameter.height) / 2.0
                                                    }
                                                    Gravity::End => {
                                                        display_parameter.y + display_parameter.height - child_display_parameter.height - child_display_parameter.margin_bottom
                                                    }
                                                };
                                                child.layout(child_x, child_y);
                                                x -= child_display_parameter.width + child_display_parameter.margin_start + child_display_parameter.margin_end;
                                            });
                                        }
                                    }
                                }
                                Axis::Vertical => {
                                    let mut y = display_parameter.y;
                                    item.get_children_mut().iter_mut().for_each(|child| {
                                        let child_display_parameter = child.get_display_parameter().clone();
                                        let child_x = match direction {
                                            LayoutDirection::LeftToRight => {
                                                display_parameter.x + child_display_parameter.margin_start
                                            }
                                            LayoutDirection::RightToLeft => {
                                                display_parameter.x + display_parameter.width - child_display_parameter.margin_start - child_display_parameter.width
                                            }
                                        };
                                        let child_y = y + child_display_parameter.margin_top;
                                        child.layout(child_x, child_y);
                                        y += child_display_parameter.height + child_display_parameter.margin_top + child_display_parameter.margin_bottom;
                                    });
                                }
                            }
                        }
                    }
                )
        );
        item.set_children(children);
        Self {
            item,
            axis: Axis::Horizontal.into(),
        }
    }

    pub fn unwrap(self) -> Item {
        self.item
    }

    pub fn get_app(&self) -> SharedApp {
        self.item.get_app()
    }
}

