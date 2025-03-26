use crate::impl_property_redraw;
use crate::shared::{Children, Gettable, Observable, Shared, SharedColor, SharedF32, SharedUnit};
use crate::ui::app::AppContext;
use crate::ui::Item;
use proc_macro::item;
use skia_safe::{Color, RRect, Rect, Vector};
use skia_safe::paint::Style;
use crate::ui::unit::Dp;

#[derive(Clone)]
struct RectangleProperty {
    color: SharedColor,
    radius_top_left: SharedUnit,
    radius_top_right: SharedUnit,
    radius_bottom_right: SharedUnit,
    radius_bottom_left: SharedUnit,
    outline_width: SharedUnit,
    outline_color: SharedColor,
}

#[item(color: impl Into<SharedColor>)]
pub struct Rectangle {
    item: Item,
    property: Shared<RectangleProperty>,
}

impl_property_redraw!(Rectangle, color, SharedColor);
impl_property_redraw!(Rectangle, radius_top_left, SharedUnit);
impl_property_redraw!(Rectangle, radius_top_right, SharedUnit);
impl_property_redraw!(Rectangle, radius_bottom_right, SharedUnit);
impl_property_redraw!(Rectangle, radius_bottom_left, SharedUnit);
impl_property_redraw!(Rectangle, outline_width, SharedUnit);
impl_property_redraw!(Rectangle, outline_color, SharedColor);

impl Rectangle {
    pub fn new(app_context: AppContext, color: impl Into<SharedColor>) -> Self {
        let item = Item::new(app_context.clone(), Children::new());
        let id = item.data().get_id();
        let event_loop_proxy = app_context.event_loop_proxy();
        let property = Shared::new(RectangleProperty {
            color: color.into().layout_when_changed(&event_loop_proxy, id),
            radius_top_left: SharedUnit::new(0.dp()).layout_when_changed(&event_loop_proxy, id),
            radius_top_right: SharedUnit::new(0.dp()).layout_when_changed(&event_loop_proxy, id),
            radius_bottom_right: SharedUnit::new(0.dp()).layout_when_changed(&event_loop_proxy, id),
            radius_bottom_left: SharedUnit::new(0.dp()).layout_when_changed(&event_loop_proxy, id),
            outline_width: SharedUnit::new(0.dp()).layout_when_changed(&event_loop_proxy, id),
            outline_color: SharedColor::new(Color::TRANSPARENT)
                .layout_when_changed(&event_loop_proxy, id),
        });

        item.data()
            .set_layout({
                let property = property.clone();
                move |item, _, _| {
                    let property = property.value();
                    let color = property.color.get();
                    let radius_top_left = property.radius_top_left.get().to_dp(item.get_app_context());
                    let radius_top_right = property.radius_top_right.get().to_dp(item.get_app_context());
                    let radius_bottom_right = property.radius_bottom_right.get().to_dp(item.get_app_context());
                    let radius_bottom_left = property.radius_bottom_left.get().to_dp(item.get_app_context());
                    let outline_width = property.outline_width.get().to_dp(item.get_app_context());
                    let outline_color = property.outline_color.get();
                    let target_parameter = item.get_target_parameter();
                    target_parameter.set_color_param("color", color);
                    target_parameter.set_float_param("radius_top_left", radius_top_left);
                    target_parameter.set_float_param("radius_top_right", radius_top_right);
                    target_parameter.set_float_param("radius_bottom_right", radius_bottom_right);
                    target_parameter.set_float_param("radius_bottom_left", radius_bottom_left);
                    target_parameter.set_float_param("outline_width", outline_width);
                    target_parameter.set_color_param("outline_color", outline_color);
                }
            })
            .set_draw({
                let property = property.clone();
                move |item, canvas| {
                    let property = property.value();
                    let display_parameter = item.get_display_parameter().clone();
                    let padding_left = item.get_padding_left();
                    let padding_top = item.get_padding_top().get().to_dp(app_context.clone());
                    let padding_right = item.get_padding_right();
                    let padding_bottom = item.get_padding_bottom().get().to_dp(app_context.clone());

                    let color = display_parameter
                        .get_color_param("color")
                        .unwrap_or(Color::TRANSPARENT);
                    let radius_top_left = display_parameter
                        .get_float_param("radius_top_left")
                        .unwrap_or(0.0);
                    let radius_top_right = display_parameter
                        .get_float_param("radius_top_right")
                        .unwrap_or(0.0);
                    let radius_bottom_right = display_parameter
                        .get_float_param("radius_bottom_right")
                        .unwrap_or(0.0);
                    let radius_bottom_left = display_parameter
                        .get_float_param("radius_bottom_left")
                        .unwrap_or(0.0);
                    let outline_width = display_parameter
                        .get_float_param("outline_width")
                        .unwrap_or(0.0);
                    let outline_color = display_parameter
                        .get_color_param("outline_color")
                        .unwrap_or(Color::TRANSPARENT);
                    let rect = Rect::from_xywh(
                        display_parameter.x() + padding_left,
                        display_parameter.y() + padding_top,
                        display_parameter.width - padding_left - padding_right,
                        display_parameter.height - padding_top - padding_bottom,
                    );
                    let rrect = RRect::new_rect_radii(
                        &rect,
                        &[
                            Vector::new(radius_top_left, radius_top_left),
                            Vector::new(radius_top_right, radius_top_right),
                            Vector::new(radius_bottom_right, radius_bottom_right),
                            Vector::new(radius_bottom_left, radius_bottom_left),
                        ],
                    );
                    let mut paint = skia_safe::Paint::default();
                    paint.set_anti_alias(true);
                    paint.set_color(color);
                    canvas.draw_rrect(rrect, &paint);
                    
                    if outline_width > 0.0 {
                        paint.set_color(outline_color);
                        paint.set_style(Style::Stroke);
                        paint.set_stroke_width(outline_width);
                        canvas.draw_rrect(rrect, &paint);
                    }
                }
            });
        Self { item, property }
    }

    pub fn radius(self, radius: impl Into<SharedUnit>) -> Self {
        let radius = radius.into();
        self.radius_top_left(radius.clone())
            .radius_top_right(radius.clone())
            .radius_bottom_right(radius.clone())
            .radius_bottom_left(radius)
    }
}
