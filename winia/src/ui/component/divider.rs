use std::ops::DerefMut;
use crate::core::generate_id;
use crate::shared::Observable;
use crate::shared::{Gettable, Removal, Settable, SharedSize};
use crate::shared::{Shared, SharedColor, SharedF32};
use crate::ui::app::WindowContext;
use crate::ui::component::RectangleExt;
use crate::ui::item::Size;
use crate::ui::Item;
use crate::{impl_property_layout, impl_property_redraw};
use parking_lot::Mutex;
use proc_macro::{item, observable};
use skia_safe::Color;
use std::sync::Arc;
use crate::ui::component::divider::style::DividerStyle;
use crate::ui::theme::Access;

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
                    let mut theme = theme.lock();
                    let theme_mut = theme.deref_mut();
                    let mut style = DividerStyle::new(theme_mut, style::DIVIDER);
                    style.color().get().unwrap_or(Color::TRANSPARENT)
                }
            }
        );
        let thickness = Shared::from_dynamic(
            [theme.as_ref().into()].into(),
            {
                let theme = theme.clone();
                move||{
                    let mut theme = theme.lock();
                    let theme_mut = theme.deref_mut();
                    let mut style = DividerStyle::new(theme_mut, style::DIVIDER);
                    style.thickness().get().map_or(Size::Auto, |thickness| {
                        if thickness > 0.0 {
                            Size::Fixed(thickness)
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
    use crate::ui::theme::{color, Access, StyleProperty};
    use crate::ui::Theme;
    use proc_macro::style;
    use skia_safe::Color;

    pub static DIVIDER: &str = "divider";

    #[style]
    pub struct DividerStyle {
        thickness: f32,
        color: Color,
    }

    pub fn add_divider_style(theme: &mut Theme) {
        let mut style = DividerStyle::new(theme, DIVIDER);
        style.color().set(color::OUTLINE_VARIANT);
        style.thickness().set(1.0);
    }
}
