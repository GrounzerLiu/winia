use clonelet::clone;
use crate::ui::item::{ItemProps, Padding};
use crate::ui::{rectangle, ripple, stack, Alignment, Color, Item, LabelProps, LabelPropsTrait, RectanglePropsTrait, RipplePropsTrait, StackProps, StackPropsTrait};
use crate::{define_props, depend};
use crate::shared::SharedDerivedColor;
use crate::theme::color;

define_props!(
    ButtonPropsTrait;
    button_props;
    ButtonProps {}
    {}
);

impl ButtonProps {
    pub fn new(mut item_props: ItemProps) -> Self {
        item_props.height = 40.0.into();
        item_props.padding = Padding::horizontal(24.0);
        Self { item_props }
    }
}

pub fn button(props: ButtonProps, text: impl FnOnce(LabelProps) -> Item) -> Item {
    let w = props.item_props.window_context.clone();
    let theme = w.theme().clone();
    let label_color = SharedDerivedColor::from_fn(
        depend!(theme),
        {
            clone!(theme);
            move || {
                let c = theme.lock().get_color(color::ON_PRIMARY).map_or(Color::WHITE, |c| *c);
                println!("Button label color updated: {:?}", c);
                c
            }
        }
    );
    let container_color = SharedDerivedColor::from_fn(
        depend!(theme),
        {
            clone!(theme);
            move || {
                let c = theme.lock().get_color(color::PRIMARY).map_or(Color::BLACK, |c| *c);
                println!("Button container color updated: {:?}", c);
                c
            }
        }
    );
    let text = text(
        w.label_props()
            .color(label_color)
    );
    let item = stack(
        StackProps::new(props.item_props)
            .height(40)
            .background(
                rectangle(
                    w.rectangle_props()
                        .color(container_color)
                )
            )
            .foreground(
                ripple(w.ripple_props())
            )
            .alignment(Alignment::center()),
        vec![
            text,
        ],
    );
    item
}

pub mod style {
    use crate::theme::shape::Corner;
    use crate::theme::{color, elevation, shape, StateStyles, ThemeValue};
    use crate::ui::Color;
    use crate::Theme;
    use proc_macro::style;

    #[style]
    pub struct ButtonStyle {
        container_color: Color,
        container_opacity: f32,
        container_color_unselected: Color,
        container_color_selected: Color,
        container_height: f32,
        container_state_layer_color: Color,
        container_state_layer_opacity: f32,
        container_state_layer_color_unselected: Color,
        container_state_layer_color_selected: Color,

        shadow_color: Color,
        elevation: f32,

        label_color: Color,
        label_opacity: f32,
        label_color_unselected: Color,
        label_color_selected: Color,
        label_size: f32,

        icon_color: Color,
        icon_opacity: f32,
        icon_color_unselected: Color,
        icon_color_selected: Color,
        icon_size: f32,

        leading_space: f32,
        trailing_space: f32,
        between_icon_label_space: f32,

        shape_round: Corner,
        shape_square: Corner,

        selected_container_shape_round: Corner,
        selected_container_shape_square: Corner,

        focus_ring_indicator_color: Color,
        focus_ring_indicator_width: f32,
        focus_ring_outline_offset: f32,
    }

    fn elevated_button_style() -> StateStyles<ButtonStyle> {
        StateStyles::enabled(ButtonStyle {
            container_color: color::SURFACE_CONTAINER_LOW.into(),
            container_color_unselected: color::SURFACE_CONTAINER_LOW.into(),
            container_color_selected: color::PRIMARY.into(),
            container_opacity: 1.0.into(),
            container_height: 40.0.into(),
            container_state_layer_color: color::PRIMARY.into(),
            container_state_layer_opacity: 0.08.into(),
            container_state_layer_color_unselected: color::PRIMARY.into(),
            container_state_layer_color_selected: color::ON_PRIMARY.into(),
            shadow_color: color::SHADOW.into(),
            elevation: elevation::LEVEL_1.into(),
            label_color: color::PRIMARY.into(),
            label_opacity: 1.0.into(),
            label_color_unselected: color::PRIMARY.into(),
            label_color_selected: color::ON_PRIMARY.into(),
            label_size: 14.0.into(),
            icon_color: color::PRIMARY.into(),
            icon_opacity: 1.0.into(),
            icon_color_unselected: color::PRIMARY.into(),
            icon_color_selected: color::ON_PRIMARY.into(),
            icon_size: 20.0.into(),
            leading_space: 24.0.into(),
            trailing_space: 24.0.into(),
            between_icon_label_space: 8.0.into(),
            shape_round: shape::corner::FULL.into(),
            shape_square: shape::corner::MEDIUM.into(),
            selected_container_shape_round: shape::corner::MEDIUM.into(),
            selected_container_shape_square: shape::corner::FULL.into(),
            focus_ring_indicator_color: color::SECONDARY.into(),
            focus_ring_indicator_width: 3.0.into(),
            focus_ring_outline_offset: 2.0.into(),
        })
        .disabled(|style| {
            style
                .set_container_color(color::ON_SURFACE)
                .set_container_opacity(0.1)
                .set_elevation(elevation::LEVEL_0)
                .set_label_color(color::ON_SURFACE)
                .set_label_opacity(0.38)
                .set_icon_color(color::ON_SURFACE)
                .set_icon_opacity(0.38);
        })
        .hovered(|style| {
            style.set_elevation(elevation::LEVEL_2);
        })
        .focused(|style| {
            style.set_container_state_layer_opacity(0.1);
        })
        .pressed(|style| {
            style.set_container_state_layer_opacity(0.1);
        })
    }

    pub fn apply_elevated_button_style(theme: &mut Theme) {
        let style = elevated_button_style();
        theme.set_style("elevated_button", Box::new(style));
    }
}
