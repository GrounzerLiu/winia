use crate::app::SharedApp;
use crate::property::Gettable;
use crate::ui::{Gravity, Item, ItemEvent, LogicalX, measure_child, MeasureMode};

#[macro_export]
macro_rules! stack {
    ($($child:expr)*) => {
        $crate::layout::Stack::new(vec![$($child),*])
    }
}

pub struct Stack {
    item: Item,
}

impl Stack {
    pub fn new(app: SharedApp, children: Vec<Item>) -> Self {
        let mut item = Item::new(
            app,
            ItemEvent::default()
                .set_on_measure(
                    |item, width_measure_mode, height_measure_mode| {
                        let mut layout_params = item.get_layout_params_mut().clone();

                        let mut width = 0.0;
                        let mut height = 0.0_f32;

                        let mut remaining_width = match width_measure_mode {
                            MeasureMode::Specified(width) => width,
                            MeasureMode::Unspecified(width) => width,
                        };

                        item.get_children_mut().iter_mut().for_each(|child| {
                            let width_measure_mode = match width_measure_mode {
                                MeasureMode::Specified(_) => MeasureMode::Specified(remaining_width),
                                MeasureMode::Unspecified(_) => MeasureMode::Unspecified(remaining_width),
                            };

                            let child_occupied_width;
                            let (child_width_measure_mode, child_height_measure_mode) = measure_child(child,&layout_params, width_measure_mode, height_measure_mode);

                            let mut child_layout_params = child.get_layout_params().clone();
                            child_layout_params.padding_start = child.get_margin_start().get();
                            child_layout_params.padding_top = child.get_margin_top().get();
                            child_layout_params.padding_end = child.get_margin_end().get();
                            child_layout_params.padding_bottom = child.get_margin_bottom().get();
                            child_layout_params.margin_start = child.get_margin_start().get();
                            child_layout_params.margin_top = child.get_margin_top().get();
                            child_layout_params.margin_end = child.get_margin_end().get();
                            child_layout_params.margin_bottom = child.get_margin_bottom().get();
                            child.set_layout_params(&child_layout_params);

                            child.measure(child_width_measure_mode, child_height_measure_mode);
                            child_occupied_width = child_layout_params.width + child_layout_params.margin_start + child_layout_params.margin_end;
                            height = height.max(child_layout_params.height + child_layout_params.margin_top + child_layout_params.margin_bottom);


                            width += child_occupied_width;

                            if remaining_width - child_occupied_width < 0.0 {
                                remaining_width = 0.0;
                            } else {
                                remaining_width -= child_occupied_width;
                            }
                        });

                        match width_measure_mode {
                            MeasureMode::Specified(measured_width) => {
                                layout_params.width = measured_width;
                            }
                            MeasureMode::Unspecified(measured_width) => {
                                layout_params.width = measured_width.min(width);
                            }
                        }

                        match height_measure_mode {
                            MeasureMode::Specified(measured_height) => {
                                layout_params.height = measured_height;
                            }
                            MeasureMode::Unspecified(measured_height) => {
                                layout_params.height = measured_height.min(height);
                            }
                        }

                        item.set_layout_params(&layout_params);
                    }
                )
                .set_on_layout(
                    |item, x, y| {
                        let vertical_gravity = item.get_vertical_gravity().get();
                        let horizontal_gravity = item.get_horizontal_gravity().get();
                        let mut layout_params = item.get_layout_params().clone();
                        layout_params.x = x;
                        layout_params.y = y;
                        item.set_layout_params(&layout_params);

                        if let Some(background) = item.get_background().lock().as_mut() {
                            background.layout(x, y);
                        }

                        if let Some(foreground) = item.get_foreground().lock().as_mut() {
                            foreground.layout(x, y);
                        }

                        let x = LogicalX::new(item.get_layout_direction().get(), x, x, layout_params.width);
                        item.get_children_mut().iter_mut().for_each(|child| {
                            let child_layout_params = child.get_layout_params();
                            let mut x = x + child_layout_params.margin_start;
                            let mut y = y + child_layout_params.margin_top;
                            let remaining_width = layout_params.width - layout_params.padding_start - layout_params.padding_end - child_layout_params.width - child_layout_params.margin_start - child_layout_params.margin_end;
                            let remaining_height = layout_params.height - layout_params.padding_top - layout_params.padding_bottom - child_layout_params.height - child_layout_params.margin_top - child_layout_params.margin_bottom;
                            let x= match vertical_gravity {
                                Gravity::Start => {
                                    x
                                }
                                Gravity::Center => {
                                    x + remaining_width / 2.0
                                }
                                Gravity::End => {
                                    x + remaining_width - child_layout_params.width
                                }
                            };

                            let y = match horizontal_gravity {
                                Gravity::Start => {
                                    y
                                }
                                Gravity::Center => {
                                    y + remaining_height / 2.0
                                }
                                Gravity::End => {
                                    y + remaining_height - child_layout_params.height
                                }
                            };

                            child.layout(x.physical_value(child_layout_params.width), y);
                        });
                    }
                )
        );
        item.set_children(children);

        Stack {
            item,
        }
    }

    pub fn unwrap(self) -> Item {
        self.item
    }
}