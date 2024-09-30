use crate::item::{Item, ItemEvent, measure_child, MeasureMode};
use crate::property::Gettable;

#[macro_export]
macro_rules! flow {
    ($($child:expr)*) => {
        $crate::layout::Flow::new(vec![$($child),*])
    }
}

pub struct Flow {
    item: Item,
}

impl Flow {
    pub fn new(children: Vec<Item>) -> Self {
        let mut item = Item::new(
            ItemEvent::default()
                .set_on_measure(
                    |item, width_measure_mode, height_measure_mode| {
                        let mut layout_params = item.get_layout_params_mut().clone();

                        let mut width = 0.0;
                        let mut height = 0.0_f32;

                        let mut remaining_width = match width_measure_mode {
                            MeasureMode::Exactly(width) => width,
                            MeasureMode::AtMost(width) => width,
                        };

                        item.get_children_mut().iter_mut().for_each(|child| {
                            let width_measure_mode = match width_measure_mode {
                                MeasureMode::Exactly(_) => MeasureMode::Exactly(remaining_width),
                                MeasureMode::AtMost(_) => MeasureMode::AtMost(remaining_width),
                            };

                            let mut child_occupied_width = 0.0;
                            let (child_width_measure_mode, child_height_measure_mode) = measure_child(child,layout_params, width_measure_mode, height_measure_mode);

                            let mut child_layout_params = child.get_layout_params().clone();
                            child_layout_params.padding_start = child.get_margin_start().get();
                            child_layout_params.padding_top = child.get_margin_top().get();
                            child_layout_params.padding_end = child.get_margin_end().get();
                            child_layout_params.padding_bottom = child.get_margin_bottom().get();
                            child_layout_params.margin_start = child.get_margin_start().get();
                            child_layout_params.margin_top = child.get_margin_top().get();
                            child_layout_params.margin_end = child.get_margin_end().get();
                            child_layout_params.margin_bottom = child.get_margin_bottom().get();
                            child.set_layout_params(child_layout_params);

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
                            MeasureMode::Exactly(measured_width) => {
                                layout_params.width = measured_width;
                            }
                            MeasureMode::AtMost(measured_width) => {
                                layout_params.width = measured_width.min(width);
                            }
                        }

                        match height_measure_mode {
                            MeasureMode::Exactly(measured_height) => {
                                layout_params.height = measured_height;
                            }
                            MeasureMode::AtMost(measured_height) => {
                                layout_params.height = measured_height.min(height);
                            }
                        }

                        item.set_layout_params(layout_params);
                    }
                )
                .set_on_layout(
                    |item, x, y| {
                        let layout_params = item.get_layout_params_mut();
                        layout_params.x = x;
                        layout_params.y = y;
                        item.get_children_mut().iter_mut().for_each(|child| {
                            child.layout(x, y);
                        });
                    }
                )
        );
        item.set_children(children);

        Flow {
            item,
        }
    }

    pub fn unwrap(self) -> Item {
        self.item
    }
}