use crate::shared::{Gettable, SharedSize};
use crate::shared::{Shared, SharedColor};
use crate::ui::app::WindowContext;
use crate::ui::component::divider::style::DividerStyle;
use crate::ui::component::RectangleExt;
use crate::ui::item::{ItemState, Size};
use crate::ui::Item;
use proc_macro::item;
use skia_safe::Color;
use std::ops::{Deref, DerefMut};

struct DividerProperty {
    thickness: SharedSize,
    color: SharedColor,
}

#[item]
pub struct Divider {
    item: Item,
    property: Shared<DividerProperty>,
}

impl Divider {
    pub fn new(app_context: &WindowContext) -> Self {
        let theme = app_context.theme();
        let color = Shared::from_dynamic(
            [theme.as_ref().into()].into(),
            {
                let theme = theme.clone();
                move||{
                    let theme_lock = theme.lock();
                    let theme = theme_lock.deref();
                    let style: &DividerStyle = theme.get_style(style::DIVIDER).unwrap();
                    style.get_color(theme, ItemState::Enabled).cloned().unwrap()
                }
            }
        );
        let thickness = Shared::from_dynamic(
            [theme.as_ref().into()].into(),
            {
                let theme = theme.clone();
                move||{
                    let theme_lock = theme.lock();
                    let theme_ref = theme_lock.deref();
                    let style: &DividerStyle = theme_ref.get_style(style::DIVIDER).unwrap();
                    style.get_thickness(theme_ref, ItemState::Enabled).map_or(Size::Auto, |thickness| {
                        if *thickness > 0.0 {
                            Size::Fixed(*thickness)
                        } else {
                            Size::Auto
                        }
                    })
                    
                }
            }
        );
        let property = Shared::from(DividerProperty {
            thickness: thickness.clone(),
            color: color.clone(),
        });
        let divider = app_context
            .rectangle(&color)
            .item()
            .size(&thickness, &thickness);
        Self {
            item: divider,
            property,
        }
    }
}

pub mod style {
    use crate::ui::Theme;
    use proc_macro::style;
    use skia_safe::Color;
    use crate::ui::item::ItemState;
    use crate::ui::theme::{color, State, ThemeValue};

    pub static DIVIDER: &str = "divider";
    
    #[style]
    pub struct DividerStyle {
        thickness: f32,
        color: Color,
    }
    
    pub fn divider_style(theme: &mut Theme) {
        let style = DividerStyle {
            thickness: State::new(1.0),
            color: State::new(color::OUTLINE_VARIANT)
        };
        theme.set_style(DIVIDER, Box::new(style));
    }
}
