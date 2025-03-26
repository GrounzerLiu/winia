use crate::shared::{Children, Gettable, Shared, SharedBool};
use crate::ui::app::AppContext;
use crate::ui::item::{HorizontalAlignment, LogicalX, MeasureMode, Orientation, VerticalAlignment};
use crate::ui::layout::GetAlignSelf;
use crate::ui::Item;
use proc_macro::item;

#[derive(Clone)]
struct StackProperty {
    constrain_children: SharedBool,
}

#[item(children:Children)]
pub struct Stack {
    item: Item,
    property: Shared<StackProperty>,
}

impl Stack {
    pub fn new(app_context: AppContext, children: Children) -> Self {
        let property = Shared::new(StackProperty {
            constrain_children: true.into(),
        });

        let item = Item::new(app_context, children);
        item.data()
            .set_measure(|item, width_mode, height_mode| {
                item.measure_children(width_mode, height_mode);
                let mut child_max_width = 0.0_f32;
                let mut child_max_height = 0.0_f32;
                for child in item.get_children().items().iter() {
                    let child_measure_parameter = child.data().clone_measure_parameter();
                    let child_margin_horizontal = child.data().get_margin(Orientation::Horizontal);
                    let child_margin_vertical = child.data().get_margin(Orientation::Vertical);
                    child_max_width = child_max_width
                        .max(child_measure_parameter.width + child_margin_horizontal);
                    child_max_height = child_max_height
                        .max(child_measure_parameter.height + child_margin_vertical);
                }
                let width = match width_mode {
                    MeasureMode::Specified(width) => width,
                    MeasureMode::Unspecified(width) => width.min(child_max_width),
                };
                let height = match height_mode {
                    MeasureMode::Specified(height) => height,
                    MeasureMode::Unspecified(height) => height.min(child_max_height),
                };
                let measure_parameter = item.get_measure_parameter();
                measure_parameter.width = width;
                measure_parameter.height = height;
            })
            .set_layout({
                let property = property.clone();
                move |item, width, height| {
                    let x = LogicalX::new(item.get_layout_direction().get(), 0.0, width);
                    let y = 0.0;

                    let padding_start = item.get_padding_start().get().to_dp(item.get_app_context());
                    let padding_end = item.get_padding_end().get().to_dp(item.get_app_context());
                    let padding_top = item.get_padding_top().get().to_dp(item.get_app_context());
                    let padding_bottom = item.get_padding_bottom().get().to_dp(item.get_app_context());

                    let align_content = item.get_align_content().get();
                    let constrain_children = property.value().constrain_children.get();

                    item.for_each_child_mut(|child| {
                        let mut child_data = child.data();
                        let child_margin_start = child_data.get_margin_start().get().to_dp(child.data().get_app_context());
                        let child_margin_end = child_data.get_margin_end().get().to_dp(child.data().get_app_context());
                        let child_margin_top = child_data.get_margin_top().get().to_dp(child.data().get_app_context());
                        let child_margin_bottom = child_data.get_margin_bottom().get().to_dp(child.data().get_app_context());
                        let mut child_width = child_data.get_measure_parameter().width;
                        let mut child_height = child_data.get_measure_parameter().height;
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

                        if constrain_children {
                            let max_width = (width
                                - padding_start
                                - child_margin_start
                                - padding_end
                                - child_margin_end)
                                .max(0.0);
                            let max_height = (height
                                - padding_top
                                - child_margin_top
                                - padding_bottom
                                - child_margin_bottom)
                                .max(0.0);

                            child_width = child_width.min(max_width);
                            child_height = child_height.min(max_height);
                        }

                        child.data().dispatch_layout(
                            child_x.physical_value(child_width),
                            child_y,
                            child_width,
                            child_height,
                        );
                    })
                }
            });
        Self { item, property }
    }
}
