use crate::shared::{Children, Gettable, Shared, SharedBool};
use crate::ui::app::WindowContext;
use crate::ui::item::{HorizontalAlignment, LogicalX, MeasureMode, Orientation, VerticalAlignment};
use crate::ui::layout::GetAlignSelf;
use crate::ui::Item;
use proc_macro::item;

#[item(children:impl Into<Children>)]
pub struct Stack {
    item: Item
}

impl Stack {
    pub fn new(app_context: &WindowContext, children: impl Into<Children>) -> Self {

        let item = Item::new(app_context, children.into());
        item.data()
            .set_measure(|item, width_mode, height_mode| {
                item.measure_children(width_mode, height_mode);
                let mut child_max_width = 0.0_f32;
                let mut child_max_height = 0.0_f32;
                let padding_horizontal = item.get_padding(Orientation::Horizontal);
                let padding_vertical = item.get_padding(Orientation::Vertical);
                let max_width = item.clamp_width(width_mode.value());
                let max_height = item.clamp_height(height_mode.value());
                for child in item.get_children_mut().lock().iter_mut() {
                    child.data().dispatch_measure(
                        max_width - padding_horizontal,
                        max_height - padding_vertical,
                    );
                    let mut child_data = child.data();
                    let child_measure_parameter = child_data.get_measure_parameter();
                    child_max_width = child_max_width
                        .max(child_measure_parameter.outer_width());
                    child_max_height = child_max_height
                        .max(child_measure_parameter.outer_height());
                }

                let width = item.clamp_width(match width_mode {
                    MeasureMode::Specified(width) => width,
                    MeasureMode::Unspecified(width) => {
                        width.min(child_max_width + padding_horizontal)
                    }
                });
                let height = item.clamp_height(match height_mode {
                    MeasureMode::Specified(height) => height,
                    MeasureMode::Unspecified(height) => {
                        height.min(child_max_height + padding_vertical)
                    }
                });

                let measure_parameter = item.get_measure_parameter();
                measure_parameter.width = width;
                measure_parameter.height = height;
            })
            .set_layout({
                move |item, width, height| {
                    let x = LogicalX::new(item.get_layout_direction().get(), 0.0, width);
                    let y = 0.0;

                    let padding_start = item.get_padding_start().get();
                    let padding_end = item.get_padding_end().get();
                    let padding_top = item.get_padding_top().get();
                    let padding_bottom = item.get_padding_bottom().get();

                    let align_content = item.get_align_content().get();

                    item.for_each_child_mut(|child| {   
                        let mut child_data = child.data();
                        let child_margin_start = child_data.get_margin_start().get();
                        let child_margin_end = child_data.get_margin_end().get();
                        let child_margin_top = child_data.get_margin_top().get();
                        let child_margin_bottom = child_data.get_margin_bottom().get();
                        let child_width = child_data.get_measure_parameter().width;
                        let child_height = child_data.get_measure_parameter().height;
                        let alignment = child_data
                            .get_align_self()
                            .map_or(align_content, |align_self| align_self.get());
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
                    })
                }
            });
        Self { item }
    }
}
