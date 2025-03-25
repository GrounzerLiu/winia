use crate::shared::{Children, Gettable, Shared, SharedUnSend};
use crate::ui::app::AppContext;
use crate::ui::item::{
    HorizontalAlignment, LogicalX, MeasureMode, Orientation, Scroller, Size, VerticalAlignment,
};
use crate::ui::Item;
use proc_macro::item;

#[derive(Clone)]
struct ScrollAreaProperty {}

#[item(children:Children)]
pub struct ScrollArea {
    item: Item,
    property: Shared<ScrollAreaProperty>,
}

impl ScrollArea {
    pub fn new(app_context: AppContext, children: Children) -> Self {
        let property = Shared::new(ScrollAreaProperty {});

        let item = Item::new(app_context.clone(), children);
        let scroller = SharedUnSend::from(Scroller::new(
            app_context,
            (true, true),
            (0.0, 0.0),
            (0.0, 0.0),
            (0.0, 0.0),
        ));
        item.data()
            .set_measure(|item, width_mode, height_mode| {
                if item.get_children().len() > 1 {
                    panic!("ScrollArea can only have one child");
                }
                let mut child_max_width = 0.0_f32;
                let mut child_max_height = 0.0_f32;
                let children = item.get_children();
                if let Some(child) = children.items().first() {
                    let max_width = match width_mode {
                        MeasureMode::Specified(width) => width,
                        MeasureMode::Unspecified(width) => width,
                    };
                    let max_height = match height_mode {
                        MeasureMode::Specified(height) => height,
                        MeasureMode::Unspecified(height) => height,
                    };

                    fn create_mode(size: Size, max_size: f32) -> MeasureMode {
                        match size {
                            Size::Compact => MeasureMode::Unspecified(max_size),
                            Size::Expanded => MeasureMode::Unspecified(max_size),
                            Size::Fixed(size) => MeasureMode::Specified(size),
                            Size::Relative(ratio) => MeasureMode::Specified(max_size * ratio),
                        }
                    }

                    let child_width = child.data().get_width().get();
                    let child_height = child.data().get_height().get();
                    let max_width = child.data().clamp_width(max_width);
                    let max_height = child.data().clamp_height(max_height);
                    child.data().measure(
                        create_mode(child_width, max_width),
                        create_mode(child_height, max_height),
                    );

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
                let _property = property.clone();
                let scroller = scroller.clone();
                move |item, width, height| {
                    if item.get_children().len() > 1 {
                        panic!("ScrollArea can only have one child");
                    }
                    if let Some(child) = item.get_children().items().first() {
                        let mut scroller = scroller.value();
                        let x = LogicalX::new(item.get_layout_direction().get(), 0.0, width)
                            - scroller.scroll_position().0;
                        let y = -scroller.scroll_position().1;

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

                        scroller.set_scroll_range((child_width, child_height));
                        scroller.set_scroll_extent((width, height));
                    }
                }
            })
            .set_mouse_wheel({
                let scroller = scroller.clone();
                move |item, mouse_wheel| {
                    let mut scroller = scroller.value();
                    scroller.update_by_mouse_wheel(mouse_wheel);
                    item.get_app_context().request_layout();
                    true
                }
            });

        let dispatch_draw = item.data().get_dispatch_draw();
        item.data().set_dispatch_draw({
            let scroller = scroller.clone();
            move |item, surface, x, y| {
                dispatch_draw.lock()(item, surface, x, y);
                let display_parameter = item.get_display_parameter().clone();
                let child_display_parameter =
                    if let Some(child) = item.get_children().items().first() {
                        Some(child.data().get_display_parameter())
                    } else {
                        None
                    };

                if let Some(child_display_parameter) = child_display_parameter {
                    let mut scroller = scroller.value();
                    let content_size = (
                        child_display_parameter.width,
                        child_display_parameter.height,
                    );
                    let canvas = surface.canvas();
                    scroller.draw(&display_parameter, content_size, canvas);
                }
            }
        });
        Self { item, property }
    }
}
