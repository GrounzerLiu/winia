use crate::{depend, With};
use crate::shared::{SharedBool, SharedDerived, SharedDerivedBool, SharedDerivedColor, SharedDerivedF32, SharedDerivedSize};
use crate::theme::shape::Corner;
use crate::ui::item::{Children, ItemProps, Padding};
use crate::ui::widget::badge::badge_style::BadgeStyleExt;
use crate::ui::{rectangle, stack, Alignment, Item, LabelProps, LabelPropsTrait, RectanglePropsTrait, Size, StackPropsTrait};
use letclone::clone;
use proc_macro::ItemProps;
use std::ops::Deref;

#[derive(ItemProps)]
pub struct BadgeProps {
    pub item_props: ItemProps,
    color: SharedDerivedColor,
    shape: SharedDerived<Corner>,
    badge_size: SharedDerivedF32,
    large_color: SharedDerivedColor,
    large_shape: SharedDerived<Corner>,
    large_size: SharedDerivedF32,

    large_label_text_color: SharedDerivedColor,
    large_label_text_font: SharedDerived<String>,
    large_label_text_line_height: SharedDerivedF32,
    large_label_text_size: SharedDerivedF32,
    large_label_text_tracking: SharedDerivedF32,
    large_label_text_weight: SharedDerivedF32,
}
macro_rules! get_style {
    ($get_style_method:ident, $theme:ident, $item_state:ident) => {
        SharedDerived::from_fn(
            depend!($theme, $item_state),
            {
                clone!($theme, $item_state);
                move || {
                    let theme = $theme.lock();
                    theme.get_badge_style(badge_style::BADGE, $item_state.get()).unwrap()
                        .$get_style_method(&theme).expect(stringify!($get_style_method)).clone()
                }
            }
        )
    }
}
impl BadgeProps {
    pub fn new(item_props: ItemProps) -> Self {
        let theme = item_props.window_context.theme().clone();
        let item_state = item_props.item_state.clone();
        let color = get_style!(get_color, theme, item_state);
        let shape = get_style!(get_shape, theme, item_state);
        let badge_size = get_style!(get_size, theme, item_state);
        let large_color = get_style!(get_large_color, theme, item_state);
        let large_shape = get_style!(get_large_shape, theme, item_state);
        let large_size = get_style!(get_large_size, theme, item_state);
        let large_label_text_color = get_style!(get_large_label_text_color, theme, item_state);
        let large_label_text_font = get_style!(get_large_label_text_font, theme, item_state);
        let large_label_text_line_height = get_style!(get_large_label_text_line_height, theme, item_state);
        let large_label_text_size = get_style!(get_large_label_text_size, theme, item_state);
        let large_label_text_tracking = get_style!(get_large_label_text_tracking, theme, item_state);
        let large_label_text_weight = get_style!(get_large_label_text_weight, theme, item_state);

        Self {
            item_props,
            color,
            shape,
            badge_size,
            large_color,
            large_shape,
            large_size,
            large_label_text_color,
            large_label_text_font,
            large_label_text_line_height,
            large_label_text_size,
            large_label_text_tracking,
            large_label_text_weight: SharedDerived::from_fn(
                depend!(large_label_text_weight),
                {
                    clone!(large_label_text_weight);
                    move || {
                        large_label_text_weight.lock().font_weight
                    }
                },
            ),
        }
    }
}

pub fn badge(props: BadgeProps, text: impl Fn(LabelProps) -> Option<Item>) -> Item {
    let w = props.window_context.clone();
    let text = text(
        w.label_props("")
         .color(&props.large_label_text_color)
         .font_size(&props.large_label_text_size)
    );
    let mut children = Children::new();
    if let Some(text) = text {
        children.add_item(text);
    }
    if children.len() == 0 {
        let size: SharedDerivedSize = SharedDerived::from_fn(
            depend!(props.badge_size),
            {
                clone!(props.badge_size);
                move || {
                    Size::Fixed(badge_size.get())
                }
            },
        );
        rectangle(
            w.rectangle_props(&props.color)
             .size(&size, &size)
             .radius(props.shape.into())
             .with_mut(|props| {
                 props.set_custom_prop::<bool>("is_large", false);
             })
        )
    } else {
        let size: SharedDerivedSize = SharedDerived::from_fn(
            depend!(props.large_size),
            {
                clone!(props.large_size);
                move || {
                    Size::Fixed(large_size.get())
                }
            },
        );
        stack(
            w.stack_props()
             .alignment(Alignment::center())
             .size(Size::Auto, &size)
             .min_width(&props.large_size)
             .padding(
                 Padding::horizontal(4.0)
             )
             .background(
                 rectangle(
                     w.rectangle_props(&props.large_color)
                      .radius(props.large_shape.into())
                 )
             )
             .with_mut(|props| {
                 props.set_custom_prop::<bool>("is_large", true);
             }),
            children,
        )
    }
}

macro_rules! color_from_theme {
    ($theme:ident, $style:ty, $key:path, $block:expr_2021) => {
        SharedDerived::from_fn(depend!($theme), {
            clone!($theme);
            move || {
                let theme_lock = $theme.lock();
                let theme_ref = theme_lock.deref();
                let style: &$style = theme_ref.get_style($key).unwrap()
                style.get_color(theme_ref)
            }
        })
    };
}

pub mod badge_style {
    use crate::theme::shape::{corner, Corner};
    use crate::theme::typescale::TypeScale;
    use crate::theme::{color, StateStyles};
    use crate::theme::{typescale, ThemeValue};
    use crate::ui::Color;
    use crate::Theme;
    use proc_macro::style;

    pub static BADGE: &str = "badge";

    #[style]
    pub struct BadgeStyle {
        color: Color,
        shape: Corner,
        size: f32,
        large_color: Color,
        large_shape: Corner,
        large_size: f32,

        large_label_text_color: Color,
        large_label_text_font: String,
        large_label_text_line_height: f32,
        large_label_text_size: f32,
        large_label_text_tracking: f32,
        large_label_text_weight: TypeScale,
    }

    pub fn apply_badge_style(theme: &mut Theme) {
        let style = StateStyles::enabled(BadgeStyle {
            color: ThemeValue::from(color::ERROR),
            shape: ThemeValue::from(corner::FULL),
            size: ThemeValue::from(6.0),
            large_color: ThemeValue::from(color::ERROR),
            large_shape: ThemeValue::from(corner::FULL),
            large_size: ThemeValue::from(16.0),

            large_label_text_color: ThemeValue::from(color::ON_ERROR),
            large_label_text_font: ThemeValue::Direct("Roboto".to_string()),
            large_label_text_line_height: ThemeValue::from(16.0),
            large_label_text_size: ThemeValue::from(11.0),
            large_label_text_tracking: ThemeValue::from(0.5),
            large_label_text_weight: ThemeValue::Ref(typescale::LABEL_SMALL.to_string()),
        });
        theme.set_style(BADGE, Box::new(style));
    }
}
