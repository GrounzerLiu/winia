


/*use proc_macro::item;
use crate::ui::app::AppContext;
use crate::shared::{Children, SharedText};
use crate::ui::Item;

pub enum ButtonType {

}

#[derive(Clone)]
struct ButtonProperty {

}

#[item]
pub struct Button {
    item: Item,
    property: ButtonProperty,
}

impl Button {
    pub fn new(app_context: AppContext, text: impl Into<SharedText>) -> Self {
        let property = ButtonProperty {

        };
        let item = Item::new(app_context, Children::new());
        Self {
            item,
            property,
        }
    }
}

*/


pub mod style {
    use skia_safe::Color;
    use proc_macro::{style, style_unit};
    use crate::ui::{Theme, Value};
    use crate::ui::theme::Style;

    #[style_unit]
    pub struct Container {
        pub height: Value<f32>,
        pub elevation: Value<f32>,
        pub shadow_color: Value<Color>,
        pub color: Value<Color>,
        pub opacity: Value<f32>,
    }

    #[style_unit]
    pub struct Label {
        pub size: Value<f32>,
        pub color: Value<Color>,
        pub opacity: Value<f32>,
    }

    #[style_unit]
    pub struct Icon {
        pub size: Value<f32>,
        pub color: Value<Color>,
        pub opacity: Value<f32>,
    }

    #[style]
    pub struct Enable{
        pub container: Container,
        pub label: Label,
        pub icon: Icon,
    }

    #[style]
    pub struct Disable{
        pub container: Container,
        pub label: Label,
        pub icon: Icon,
    }

    #[style_unit]
    pub struct StateLayer {
        pub color: Value<Color>,
        pub opacity: Value<f32>,
    }

    #[style]
    pub struct Hover {
        pub container: Container,
        pub label: Label,
        pub state_layer: StateLayer,
        pub icon: Icon,
    }
    
    #[style_unit]
    pub struct FocusIndicator {
        pub color: Value<Color>,
        pub thickness: Value<f32>,
        pub offset: Value<f32>,
    }

    #[style]
    pub struct Focus {
        pub container: Container,
        pub label: Label,
        pub state_layer: StateLayer,
        pub icon: Icon,
        pub focus_indicator: FocusIndicator,
    }
    
    #[style]
    pub struct Press {
        pub container: Container,
        pub label: Label,
        pub state_layer: StateLayer,
        pub icon: Icon,
    }
    
    #[style]
    pub struct Button {
        pub enable: Enable,
        pub disable: Disable,
        pub hover: Hover,
        pub focus: Focus,
        pub press: Press,
    }
}
