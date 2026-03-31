use crate::ui::widget::icon::icon_style::IconStyleExt;
use crate::depend;
use letclone::clone;
use proc_macro::ItemProps;
use crate::drawable::Drawable;
use crate::icon::{IconDrawable, MaterialSymbol};
use crate::shared::{SharedDerived, SharedDerivedColor, SharedDerivedF32, SharedDerivedSize, SharedDerivedU32};
use crate::shared_derived;
use crate::ui::{Alignment, Color, HorizontalAlignment, Item, Orientation, Size, VerticalAlignment};
use crate::ui::item::{ItemEvent, ItemKind, ItemProps, MeasureMode, PhysicalX};
//            symbol,
//             size,
//             color,
//             fill,
//             weight,
//             grade,
//             optical_size,
#[derive(ItemProps)]
pub struct IconProps {
    item_props: ItemProps,
    #[constructor]
    symbol: SharedDerived<MaterialSymbol>,
    fill: SharedDerivedF32,
    weight: SharedDerivedF32,
    grade: SharedDerivedF32,
    optical_size: SharedDerivedF32,
    color: SharedDerivedColor,
    align_content: SharedDerived<Alignment>
}

macro_rules! get_style {
    ($get_style_method:ident, $theme:ident, $item_state:ident) => {
        SharedDerived::from_fn(
            depend!($theme, $item_state),
            {
                clone!($theme, $item_state);
                move || {
                    let theme = $theme.lock();
                    theme.get_icon_style(icon_style::ICON_STYLE, $item_state.get()).unwrap()
                        .$get_style_method(&theme).expect(stringify!($get_style_method)).clone()
                }
            }
        )
    }
}

impl IconProps {
    pub fn new(item_props: ItemProps, symbol: impl Into<SharedDerived<MaterialSymbol>>) -> Self {
        let w = item_props.window_context.clone();
        let theme = w.theme().clone();
        let item_state = item_props.item_state.clone();
        let fill = get_style!(get_fill, theme, item_state);
        let weight = get_style!(get_weight, theme, item_state);
        let grade = get_style!(get_grade, theme, item_state);
        let optical_size = get_style!(get_optical_size, theme, item_state);
        let color = get_style!(get_color, theme, item_state);
        let size = get_style!(get_size, theme, item_state);
        let size = shared_derived!(size => Size::Fixed(size.get()));

        Self {
            item_props,
            symbol: symbol.into(),
            fill,
            weight,
            grade,
            optical_size,
            color,
            align_content: Alignment::center().into(),
        }
            .size(&size, &size)
    }
}

pub fn icon(props: IconProps) -> Item {
    Item::new(
        ItemKind::Widget,
        item_event(&props),
        props,
        vec![]
    )
}

fn item_event(props: &IconProps) -> ItemEvent {
    ItemEvent::new()
        .set_measure({
            move |item, width_mode, height_mode| {
                let padding_vertical = item.get_padding(Orientation::Vertical);
                let padding_horizontal = item.get_padding(Orientation::Horizontal);
                let (w, h) = match (width_mode, height_mode) {
                    (MeasureMode::Specified(w), MeasureMode::Specified(h)) => (w, h),
                    (MeasureMode::Specified(w), MeasureMode::Unspecified(_)) => {
                        (w, w + padding_vertical)
                    }
                    (MeasureMode::Unspecified(_), MeasureMode::Specified(h)) => {
                        (h + padding_horizontal, h)
                    }
                    (MeasureMode::Unspecified(_), MeasureMode::Unspecified(_)) => {
                        let w = 24.0 + padding_horizontal;
                        let h = 24.0 + padding_vertical;
                        (w, h)
                    }
                };
                let w = item.clamp_width(w);
                let h = item.clamp_height(h);
                item.measure_frame.width = w;
                item.measure_frame.height = h;
            }
        })
        .set_layout({
            clone!(
                props.fill,
                props.weight,
                props.grade,
                props.optical_size,
                props.color,
                props.align_content
            );
            move |item, width, height| {
                let padding_start = item.padding.start.get();
                let padding_end = item.padding.end.get();
                let padding_top = item.padding.top.get();
                let padding_bottom = item.padding.bottom.get();
                let padding_horizontal = padding_start + padding_end;
                let padding_vertical = padding_end + padding_top;

                let remaining_width = width - padding_horizontal;
                let remaining_height = height - padding_vertical;

                let icon_size = remaining_width.min(remaining_height);

                let align_content = align_content.get();
                let x = match align_content.horizontal() {
                    HorizontalAlignment::Start => padding_start,
                    HorizontalAlignment::Center => (width - icon_size) / 2.0,
                    HorizontalAlignment::End => width - icon_size - padding_end
                };

                let y = match align_content.vertical() {
                    VerticalAlignment::Top => padding_top,
                    VerticalAlignment::Center => (height - icon_size) / 2.0,
                    VerticalAlignment::Bottom => height - icon_size - padding_bottom
                };
                let layout_direction = item.layout_direction.get();
                let target_frame = &mut item.target_frame;
                target_frame.set_float_param("icon_size", icon_size);
                target_frame.set_float_param("icon_x", x.physical_x(layout_direction, width, icon_size));
                target_frame.set_float_param("icon_y", y);
                target_frame.set_float_param("fill", fill.get());
                target_frame.set_float_param("weight", weight.get());
                target_frame.set_float_param("grade", grade.get());
                target_frame.set_float_param("optical_size", optical_size.get());
                target_frame.set_color_param("color", color.get());
            }
        })
        .set_draw({
            clone!(props.symbol);
            move |item, canvas| {
                let frame = item.current_frame();
                let icon_size = frame.get_float_param("icon_size").unwrap();
                let icon_x = frame.get_float_param("icon_x").unwrap();
                let icon_y = frame.get_float_param("icon_y").unwrap();
                let fill = frame.get_float_param("fill").unwrap();
                let weight = frame.get_float_param("weight").unwrap();
                let grade = frame.get_float_param("grade").unwrap();
                let optical_size = frame.get_float_param("optical_size").unwrap();
                let color = frame.get_color_param("color").unwrap();
                let mut icon_drawable = IconDrawable::new(symbol.get());
                icon_drawable.set_color(color);
                icon_drawable.set_size(icon_size);
                icon_drawable.set_width(icon_size);
                icon_drawable.set_height(icon_size);
                icon_drawable.set_fill(fill);
                icon_drawable.set_weight(weight);
                icon_drawable.set_grade(grade);
                icon_drawable.set_optical_size(optical_size);
                icon_drawable.draw(canvas, frame.x() + icon_x, frame.y() + icon_y);
            }
        })
}

pub mod icon_style {
    use proc_macro::style;
    use crate::Theme;
    use crate::theme::{color, StateStyles, ThemeValue};
    use crate::ui::Color;

    pub const ICON_STYLE: &str = "icon_style";

    #[style]
    pub struct IconStyle {
        color: Color,
        size: f32,
        fill: f32,
        weight: f32,
        grade: f32,
        optical_size: f32,
    }

    pub fn apply_icon_style(theme: &mut Theme) {
        let style = StateStyles::enabled(IconStyle {
            color: ThemeValue::from(color::ON_SURFACE),
            size: ThemeValue::Direct(24.0),
            fill: ThemeValue::Direct(0.0),
            weight: ThemeValue::Direct(400.0),
            grade: ThemeValue::Direct(0.0),
            optical_size: ThemeValue::Direct(24.0),
        });
        theme.set_style(ICON_STYLE, Box::new(style));
    }


}