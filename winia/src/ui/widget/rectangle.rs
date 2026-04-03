use crate::shared::{Shared, SharedDerived, SharedDerivedColor};
use crate::ui::item::{Children, ItemKind, ItemProps, LayoutDirection};
use crate::ui::{Color, Item, Orientation, SetColor};
use letclone::clone;
use proc_macro::ItemProps;
use skia_bindings::{SkPaint_Cap, SkPaint_Join};
use skia_safe::paint::Style;
use skia_safe::{RRect, Rect, Vector};
use crate::event::ItemEvent;

#[derive(Clone, Debug)]
pub struct Radius {
    pub top_start: SharedDerived<f32>,
    pub top_end: SharedDerived<f32>,
    pub bottom_start: SharedDerived<f32>,
    pub bottom_end: SharedDerived<f32>,
}

impl Radius {
    pub fn top_left(&self, layout_direction: &LayoutDirection) -> f32 {
        match layout_direction {
            LayoutDirection::LTR => self.top_start.get(),
            LayoutDirection::RTL => self.top_end.get(),
        }
    }
    pub fn top_right(&self, layout_direction: &LayoutDirection) -> f32 {
        match layout_direction {
            LayoutDirection::LTR => self.top_end.get(),
            LayoutDirection::RTL => self.top_start.get(),
        }
    }
    pub fn bottom_left(&self, layout_direction: &LayoutDirection) -> f32 {
        match layout_direction {
            LayoutDirection::LTR => self.bottom_start.get(),
            LayoutDirection::RTL => self.bottom_end.get(),
        }
    }
    pub fn bottom_right(&self, layout_direction: &LayoutDirection) -> f32 {
        match layout_direction {
            LayoutDirection::LTR => self.bottom_end.get(),
            LayoutDirection::RTL => self.bottom_start.get(),
        }
    }
}
impl Default for Radius {
    fn default() -> Self {
        Self {
            top_start: 0.0.into(),
            top_end: 0.0.into(),
            bottom_start: 0.0.into(),
            bottom_end: 0.0.into(),
        }
    }
}
impl Radius {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn all(mut self, radius: impl Into<SharedDerived<f32>>) -> Self {
        let radius = radius.into();
        self.top_start = radius.clone();
        self.top_end = radius.clone();
        self.bottom_start = radius.clone();
        self.bottom_end = radius;
        self
    }
    pub fn top_start(mut self, radius: impl Into<SharedDerived<f32>>) -> Self {
        self.top_start = radius.into();
        self
    }
    pub fn top_end(mut self, radius: impl Into<SharedDerived<f32>>) -> Self {
        self.top_end = radius.into();
        self
    }
    pub fn bottom_start(mut self, radius: impl Into<SharedDerived<f32>>) -> Self {
        self.bottom_start = radius.into();
        self
    }
    pub fn bottom_end(mut self, radius: impl Into<SharedDerived<f32>>) -> Self {
        self.bottom_end = radius.into();
        self
    }

    pub fn fully_rounded() -> Self {
        Self::new().all(Shared::new_derived(f32::MAX))
    }
}

#[derive(Clone, Copy, Debug)]
pub enum BorderPosition {
    Inside,
    Center,
    Outside
}

#[derive(Clone, Copy, Debug)]
pub enum StrokeCap {
    Butt,
    Round,
    Square,
}

impl Into<SkPaint_Cap> for StrokeCap {
    fn into(self) -> SkPaint_Cap {
        match self {
            StrokeCap::Butt => SkPaint_Cap::Butt,
            StrokeCap::Round => SkPaint_Cap::Round,
            StrokeCap::Square => SkPaint_Cap::Square,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum StrokeJoin {
    Miter,
    Round,
    Bevel,
}

impl Into<SkPaint_Join> for StrokeJoin {
    fn into(self) -> SkPaint_Join {
        match self {
            StrokeJoin::Miter => SkPaint_Join::Miter,
            StrokeJoin::Round => SkPaint_Join::Round,
            StrokeJoin::Bevel => SkPaint_Join::Bevel,
        }
    }
}

#[derive(ItemProps)]
pub struct RectangleProps {
    pub item_props: ItemProps,
    pub border_width: SharedDerived<f32>,
    pub border_position: SharedDerived<BorderPosition>,
    pub border_stroke_cap: SharedDerived<StrokeCap>,
    pub border_stroke_join: SharedDerived<StrokeJoin>,
    #[constructor]
    pub color: SharedDerived<Color>,
    pub is_filled: SharedDerived<bool>,
    #[not_shared]
    pub radius: Radius,
}

impl RectangleProps {
    pub fn radius(mut self, radius: Radius) -> Self {
        self.radius = radius;
        self
    }
}

impl RectangleProps {
    pub fn new(mut item_props: ItemProps, color: impl Into<SharedDerivedColor>) -> Self {
        item_props.clipped = true.into();
        Self {
            item_props,
            border_width: 0.0.into(),
            border_position: BorderPosition::Center.into(),
            border_stroke_cap: StrokeCap::Butt.into(),
            border_stroke_join: StrokeJoin::Miter.into(),
            color: color.into(),
            is_filled: true.into(),
            radius: Radius::default(),
        }.name("Rectangle")
    }
}

pub fn rectangle(props: RectangleProps) -> Item {
    Item::new(
        ItemKind::Widget,
        item_event(&props),
        props,
        Children::new(),
    )
}

fn item_event(props: &RectangleProps) -> ItemEvent {
    ItemEvent::new()
        .set_layout({
            clone!(
                props.color,
                props.border_width,
                props.radius
            );
            move |item, width, height| {
                let color = color.get();
                let border_width = border_width.get();
                let layout_direction = item.props().layout_direction.get();
                let radius_top_left = radius.top_left(&layout_direction);
                let radius_top_right = radius.top_right(&layout_direction);
                let radius_bottom_left = radius.bottom_left(&layout_direction);
                let radius_bottom_right = radius.bottom_right(&layout_direction);
                fn clamp_radius(
                    radius: f32,
                    width: f32,
                    height: f32,
                ) -> f32 {
                    let max_radius = (width.min(height)) / 2.0;
                    radius.min(max_radius)
                }

                let padding_horizontal = item.get_padding(Orientation::Horizontal);
                let padding_vertical = item.get_padding(Orientation::Vertical);
                let rect_width = width - padding_horizontal;
                let rect_height = height - padding_vertical;
                let radius_top_left = clamp_radius(radius_top_left, rect_width, rect_height);
                let radius_top_right = clamp_radius(radius_top_right, rect_width, rect_height);
                let radius_bottom_left = clamp_radius(radius_bottom_left, rect_width, rect_height);
                let radius_bottom_right = clamp_radius(radius_bottom_right, rect_width, rect_height);

                let target_frame = &mut item.target_frame;
                target_frame.set_color_param("color", color);
                target_frame.set_float_param("border_width", border_width);
                target_frame.set_float_param("radius_top_left", radius_top_left);
                target_frame.set_float_param("radius_top_right", radius_top_right);
                target_frame.set_float_param("radius_bottom_left", radius_bottom_left);
                target_frame.set_float_param("radius_bottom_right", radius_bottom_right);
            }
        })
        .set_draw({
            let mut paint = skia_safe::Paint::default();
            paint.set_anti_alias(true);
            clone!(
                props.is_filled,
                props.border_position
            );
            move |item, canvas| {
                let current_frame = item.current_frame();
                let is_filled = is_filled.get();
                let color = current_frame
                    .get_color_param("color")
                    .unwrap_or(Color::TRANSPARENT);
                let border_width = current_frame.get_float_param("border_width").unwrap_or(0.0);
                let radius_top_left = current_frame
                    .get_float_param("radius_top_left")
                    .unwrap_or(0.0);
                let radius_top_right = current_frame
                    .get_float_param("radius_top_right")
                    .unwrap_or(0.0);
                let radius_bottom_left = current_frame
                    .get_float_param("radius_bottom_left")
                    .unwrap_or(0.0);
                let radius_bottom_right = current_frame
                    .get_float_param("radius_bottom_right")
                    .unwrap_or(0.0);
                let padding_horizontal = item.get_padding(Orientation::Horizontal);
                let padding_vertical = item.get_padding(Orientation::Vertical);

                let rect = if is_filled {
                    Rect::from_xywh(
                        current_frame.x(),
                        current_frame.y(),
                        current_frame.width() - padding_horizontal,
                        current_frame.height() - padding_vertical,
                    )
                } else {
                    match border_position.get() {
                        BorderPosition::Inside => Rect::from_xywh(
                            current_frame.x() + border_width / 2.0,
                            current_frame.y() + border_width / 2.0,
                            current_frame.width() - padding_horizontal - border_width,
                            current_frame.height() - padding_vertical - border_width,
                        ),
                        BorderPosition::Center => Rect::from_xywh(
                            current_frame.x(),
                            current_frame.y(),
                            current_frame.width() - padding_horizontal,
                            current_frame.height() - padding_vertical,
                        ),
                        BorderPosition::Outside => Rect::from_xywh(
                            current_frame.x() - border_width / 2.0,
                            current_frame.y() - border_width / 2.0,
                            current_frame.width() - padding_horizontal + border_width,
                            current_frame.height() - padding_vertical + border_width,
                        ),
                    }
                };

                let rrect = RRect::new_rect_radii(
                    rect,
                    &[
                        Vector::new(radius_top_left, radius_top_left),
                        Vector::new(radius_top_right, radius_top_right),
                        Vector::new(radius_bottom_right, radius_bottom_right),
                        Vector::new(radius_bottom_left, radius_bottom_left),
                    ],
                );

                if color == Color::TRANSPARENT
                    || (!is_filled && border_width <= 0.0)
                {
                    return;
                }
                paint.set_any_color(color);
                if is_filled {
                    paint.set_style(Style::Fill);
                } else {
                    paint.set_style(Style::Stroke);
                    paint.set_stroke_width(border_width);
                }
                canvas.draw_rrect(rrect, &paint);
            }
        })
}

