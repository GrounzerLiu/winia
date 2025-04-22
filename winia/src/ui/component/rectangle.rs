use crate::{impl_property_layout, impl_property_redraw};
use crate::shared::{Children, Gettable, Observable, Shared, SharedColor, SharedF32};
use crate::ui::app::WindowContext;
use crate::ui::Item;
use proc_macro::item;
use skia_safe::{Color, Path, RRect, Rect, Shader, Vector};
use skia_safe::paint::Style;
use crate::ui::item::{ItemData, LayoutDirection, LogicalX};

#[derive(Clone)]
struct RectangleProperty {
    color: SharedColor,
    shader: Shared<Option<Shader>>,
    radius_top_start: SharedF32,
    radius_top_end: SharedF32,
    radius_bottom_start: SharedF32,
    radius_bottom_end: SharedF32,
    outline_width: SharedF32,
    outline_color: SharedColor,
    outline_offset: SharedF32,
}

#[item(color: impl Into<SharedColor>)]
pub struct Rectangle {
    item: Item,
    property: Shared<RectangleProperty>,
}

impl_property_layout!(Rectangle, color, SharedColor);
impl_property_layout!(Rectangle, shader, Shared<Option<Shader>>);
impl_property_layout!(Rectangle, radius_top_start, SharedF32);
impl_property_layout!(Rectangle, radius_top_end, SharedF32);
impl_property_layout!(Rectangle, radius_bottom_start, SharedF32);
impl_property_layout!(Rectangle, radius_bottom_end, SharedF32);
impl_property_layout!(Rectangle, outline_width, SharedF32);
impl_property_layout!(Rectangle, outline_color, SharedColor);
impl_property_layout!(Rectangle, outline_offset, SharedF32);

impl Rectangle {
    pub fn new(app_context: &WindowContext, color: impl Into<SharedColor>) -> Self {
        let item = Item::new(app_context, Children::new());
        let id = item.data().get_id();
        let event_loop_proxy = app_context.event_loop_proxy();
        let property = Shared::from(RectangleProperty {
            color: color.into().layout_when_changed(&event_loop_proxy, id),
            shader: Shared::from(None).layout_when_changed(&event_loop_proxy, id),
            radius_top_start: SharedF32::from(0.0).layout_when_changed(&event_loop_proxy, id),
            radius_top_end: SharedF32::from(0.0).layout_when_changed(&event_loop_proxy, id),
            radius_bottom_start: SharedF32::from(0.0).layout_when_changed(&event_loop_proxy, id),
            radius_bottom_end: SharedF32::from(0.0).layout_when_changed(&event_loop_proxy, id),
            outline_width: SharedF32::from(0.0).layout_when_changed(&event_loop_proxy, id),
            outline_color: SharedColor::from(Color::TRANSPARENT)
                .layout_when_changed(&event_loop_proxy, id),
            outline_offset: SharedF32::from(0.0).layout_when_changed(&event_loop_proxy, id),
        });

        item.data()
            .set_layout({
                let property = property.clone();
                move |item, width, height| {
                    let property = property.lock();
                    let color = property.color.get();
                    let outline_width = property.outline_width.get();
                    let outline_color = property.outline_color.get();
                    let outline_offset = property.outline_offset.get();
                    
                    let padding_start = item.get_padding_start().get();
                    let padding_top = item.get_padding_top().get();
                    let padding_end = item.get_padding_end().get();
                    let padding_bottom = item.get_padding_bottom().get();
                    
                    let r_width = width - padding_start - padding_end;
                    let r_height = height - padding_top - padding_bottom;
                    
                    let min_radius = r_width.min(r_height) / 2.0;
                    let radius_top_start = property.radius_top_start.get().clamp(0.0, min_radius);
                    let radius_top_end = property.radius_top_end.get().clamp(0.0, min_radius);
                    let radius_bottom_end = property.radius_bottom_start.get().clamp(0.0, min_radius);
                    let radius_bottom_start = property.radius_bottom_end.get().clamp(0.0, min_radius);
                    
                    let layout_direction = item.get_layout_direction().get();
                    let x = LogicalX::new(layout_direction, padding_start, width);
                    
                    let target_parameter = item.get_target_parameter();
                    target_parameter.set_color_param("color", color);
                    target_parameter.set_float_param("width", r_width);
                    target_parameter.set_float_param("height", r_height);
                    target_parameter.set_float_param("x", x.physical_value(r_width));
                    target_parameter.set_float_param("y", padding_top);
                    target_parameter.set_float_param("radius_top_start", radius_top_start);
                    target_parameter.set_float_param("radius_top_end", radius_top_end);
                    target_parameter.set_float_param("radius_bottom_end", radius_bottom_end);
                    target_parameter.set_float_param("radius_bottom_start", radius_bottom_start);
                    target_parameter.set_float_param("outline_width", outline_width);
                    target_parameter.set_color_param("outline_color", outline_color);
                    target_parameter.set_float_param("outline_offset", outline_offset);
                    
                }
            })
            .set_draw({
                let property = property.clone();
                move |item, canvas| {
                    let property = property.lock();
                    let display_parameter = item.get_display_parameter().clone();

                    let color = display_parameter
                        .get_color_param("color")
                        .unwrap_or(Color::TRANSPARENT);
                    let width = display_parameter.get_float_param("width").unwrap_or(0.0);
                    let height = display_parameter.get_float_param("height").unwrap_or(0.0);
                    let max_radius = width.min(height) / 2.0;
                    let x = display_parameter.get_float_param("x").unwrap_or(0.0);
                    let y = display_parameter.get_float_param("y").unwrap_or(0.0);
                    let radius_top_start = display_parameter
                        .get_float_param("radius_top_start")
                        .unwrap_or(0.0);
                    let radius_top_end = display_parameter
                        .get_float_param("radius_top_end")
                        .unwrap_or(0.0);
                    let radius_bottom_end = display_parameter
                        .get_float_param("radius_bottom_end")
                        .unwrap_or(0.0);
                    let radius_bottom_start = display_parameter
                        .get_float_param("radius_bottom_start")
                        .unwrap_or(0.0);
                    let outline_width = display_parameter
                        .get_float_param("outline_width")
                        .unwrap_or(0.0);
                    let outline_color = display_parameter
                        .get_color_param("outline_color")
                        .unwrap_or(Color::TRANSPARENT);
                    let outline_offset = display_parameter
                        .get_float_param("outline_offset")
                        .unwrap_or(0.0);
                    let rect = Rect::from_xywh(
                        display_parameter.x() + x,
                        display_parameter.y() + y,
                        width,
                        height,
                    );
                    let layout_direction = item.get_layout_direction().get();
                    let rrect = if layout_direction == LayoutDirection::LTR {
                        RRect::new_rect_radii(
                            &rect,
                            &[
                                Vector::new(radius_top_start, radius_top_start),
                                Vector::new(radius_top_end, radius_top_end),
                                Vector::new(radius_bottom_end, radius_bottom_end),
                                Vector::new(radius_bottom_start, radius_bottom_start),
                            ],
                        )
                    } else {
                        RRect::new_rect_radii(
                            &rect,
                            &[
                                Vector::new(radius_top_end, radius_top_end),
                                Vector::new(radius_top_start, radius_top_start),
                                Vector::new(radius_bottom_start, radius_bottom_start),
                                Vector::new(radius_bottom_end, radius_bottom_end),
                            ],
                        )
                    };
                    let shader = property.shader.get();
                    let mut paint = skia_safe::Paint::default();
                    paint.set_anti_alias(true);
                    paint.set_color(color);
                    if let Some(shader) = shader {
                        paint.set_shader(shader);
                    }
                    canvas.draw_rrect(rrect, &paint);
                    
                    if outline_width > 0.0 {
                        paint.set_color(outline_color);
                        paint.set_style(Style::Stroke);
                        paint.set_stroke_width(outline_width);
                        canvas.draw_rrect(
                            {
                                let offset = outline_width / 2.0;
                                let rect = Rect::from_xywh(
                                    display_parameter.x() + x + offset - outline_offset,
                                    display_parameter.y() + y + offset - outline_offset,
                                    width - outline_width + outline_offset * 2.0,
                                    height - outline_width + outline_offset * 2.0,
                                );
                                let offset = offset * 2.0;
                                if layout_direction == LayoutDirection::LTR {
                                    RRect::new_rect_radii(
                                        &rect,
                                        &[
                                            Vector::new(radius_top_start + offset, radius_top_start + offset),
                                            Vector::new(radius_top_end + offset, radius_top_end + offset),
                                            Vector::new(radius_bottom_end + offset, radius_bottom_end + offset),
                                            Vector::new(radius_bottom_start + offset, radius_bottom_start + offset),
                                        ],
                                    )
                                } else {
                                    RRect::new_rect_radii(
                                        &rect,
                                        &[
                                            Vector::new(radius_top_end + offset, radius_top_end + offset),
                                            Vector::new(radius_top_start + offset, radius_top_start + offset),
                                            Vector::new(radius_bottom_start + offset, radius_bottom_start + offset),
                                            Vector::new(radius_bottom_end + offset, radius_bottom_end + offset),
                                        ],
                                    )
                                }
                            }, 
                            &paint
                        );
                    }
                }
            });
        Self { item, property }
    }

    pub fn radius(self, radius: impl Into<SharedF32>) -> Self {
        let radius = radius.into();
        self.radius_top_start(radius.clone())
            .radius_top_end(radius.clone())
            .radius_bottom_end(radius.clone())
            .radius_bottom_start(radius)
    }
}
