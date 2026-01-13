use crate::shared::SharedDerived;
use crate::theme::StateStyles;
use crate::ui::item::ItemProps;
use crate::ui::widget::divider::style::DividerStyle;
use crate::ui::{Color, Size};
use crate::{define_props, depend};
use clonelet::clone;
use std::ops::Deref;
use proc_macro::ItemProps;
/*define_props!(
    DividerPropsTrait;
    divider_props;
    DividerProps {
        thickness: SharedDerived<Size>,
        color: SharedDerived<Color>,
    }
);*/
#[derive(ItemProps)]
pub struct DividerProps {
    pub item_props: ItemProps,
    pub thickness: SharedDerived<Size>,
    pub color: SharedDerived<Color>,
}

impl DividerProps {
    pub fn new(item_props: ItemProps) -> Self {
        let theme = item_props.window_context.theme().clone();
        let item_state = item_props.item_state.clone();
        let thickness = SharedDerived::from_fn(depend!(theme, item_state), {
            clone!(theme, item_state);
            move || {
                let theme_lock = theme.lock();
                let theme_ref = theme_lock.deref();
                let style: &StateStyles<DividerStyle> =
                    theme_ref.get_style(style::DIVIDER).unwrap();
                style
                    .get(item_state.get())
                    .get_thickness(theme_ref)
                    .cloned()
                    .map(Size::from)
                    .unwrap()
            }
        });
        let color = SharedDerived::from_fn(depend!(theme, item_state), {
            clone!(theme, item_state);
            move || {
                let theme_lock = theme.lock();
                let theme_ref = theme_lock.deref();
                let style: &StateStyles<DividerStyle> =
                    theme_ref.get_style(style::DIVIDER).unwrap();
                style
                    .get(item_state.get())
                    .get_color(theme_ref)
                    .cloned()
                    .unwrap()
            }
        });
        Self {
            item_props,
            thickness,
            color,
        }
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

pub mod style {
    use std::any::Any;
use crate::ui::item::ItemState;
use crate::theme::ThemeValue;
    use crate::theme::{color, StateStyles};
    use crate::ui::Color;
    use crate::Theme;
    use proc_macro::style;

    pub static DIVIDER: &str = "divider";

    // #[style]
    // pub struct DividerStyle {
    //     thickness: f32,
    //     color: Color,
    // }
    //
    // pub fn divider_style(theme: &mut Theme) {
    //     let style = DividerStyle {
    //         thickness: State::new(1.0),
    //         color: State::new(color::OUTLINE_VARIANT),
    //     };
    //     theme.set_style(DIVIDER, Box::new(style));
    // }

    #[style]
    pub struct DividerStyle {
        thickness: f32,
        color: Color,
    }

    pub fn divider_style(theme: &mut Theme) {
        let style = StateStyles::enabled(DividerStyle {
            thickness: ThemeValue::Direct(1.0),
            color: ThemeValue::Ref(color::OUTLINE_VARIANT.to_string()),
        }).disabled(|style| {
            style.set_color("").set_thickness("");
        });
        theme.set_style(DIVIDER, Box::new(style));
    }
}
