use crate::animation::AnimationExt;
use crate::shared::{SharedDerived, SharedDerivedBool, SharedDerivedColor, SharedDerivedF32, SharedDerivedSize, SharedF32, SharedSource};
use crate::theme::{color, StateStyles};
use crate::theme::shape::Corner;
use crate::ui::item::{ItemProps, ItemState, Padding};
use crate::ui::{rectangle, ripple, stack, Alignment, Color, Item, LabelProps, LabelPropsTrait, RectanglePropsTrait, RipplePropsTrait, StackProps, StackPropsTrait};
use crate::{bind_properties, define_props, depend};
use clonelet::clone;
use proc_macro::ItemProps;
use crate::ui::button::button_style::ButtonStyle;
use crate::ui::widget::button::button_style::ButtonStyleExt;

#[derive(Clone, Debug)]
pub enum ButtonType {
    Elevated,
    Filled,
    Tonal,
    Outlined,
    Text,
}

impl Into<String> for ButtonType {
    fn into(self) -> String {
        match self {
            ButtonType::Elevated => "elevated_button".to_string(),
            ButtonType::Filled => "filled_button".to_string(),
            ButtonType::Tonal => "tonal_button".to_string(),
            ButtonType::Outlined => "outlined_button".to_string(),
            ButtonType::Text => "text_button".to_string(),
        }
    }
}

/*define_props!(
    ButtonPropsTrait;
    button_props;
    ButtonProps {
        button_type: SharedDerived<ButtonType>,

        toggleable: SharedDerivedBool,

        container_color: SharedDerivedColor,
        container_selected_color: SharedDerivedColor,
        container_unselected_color: SharedDerivedColor,
        container_opacity: SharedDerivedF32,
        container_height: SharedDerivedF32,

        shadow_color: SharedDerivedColor,
        elevation: SharedDerivedF32,

        label_color: SharedDerivedColor,
        label_selected_color: SharedDerivedColor,
        label_unselected_color: SharedDerivedColor,
        label_opacity: SharedDerivedF32,
        label_size: SharedDerivedF32,

        icon_color: SharedDerivedColor,
        icon_selected_color: SharedDerivedColor,
        icon_unselected_color: SharedDerivedColor,
        icon_opacity: SharedDerivedF32,
        icon_size: SharedDerivedF32,

        shape_round: SharedDerived<Corner>,
        shape_square: SharedDerived<Corner>,
        shape_pressed_morph: SharedDerived<Corner>,
        selected_container_shape_round: SharedDerived<Corner>,
        selected_container_shape_square: SharedDerived<Corner>,

        leading_space: SharedDerivedF32,
        between_icon_label_space: SharedDerivedF32,
        trailing_space: SharedDerivedF32,

        focus_ring_indicator_color: SharedDerivedColor,
        focus_ring_indicator_thickness: SharedDerivedF32,
        focus_ring_outline_offset: SharedDerivedF32,

        container_state_layer_color: SharedDerivedColor,
        container_state_layer_selected_color: SharedDerivedColor,
        container_state_layer_unselected_color: SharedDerivedColor,
        container_state_layer_opacity: SharedDerivedF32,
    }
);*/

#[derive(ItemProps)]
pub struct ButtonProps {
    item_props: ItemProps,
    
    button_type: SharedDerived<ButtonType>,

    toggleable: SharedDerivedBool,

    container_color: SharedDerivedColor,
    container_selected_color: SharedDerivedColor,
    container_unselected_color: SharedDerivedColor,
    container_opacity: SharedDerivedF32,
    container_height: SharedDerivedF32,

    shadow_color: SharedDerivedColor,
    elevation: SharedDerivedF32,

    label_color: SharedDerivedColor,
    label_selected_color: SharedDerivedColor,
    label_unselected_color: SharedDerivedColor,
    label_opacity: SharedDerivedF32,
    label_size: SharedDerivedF32,

    icon_color: SharedDerivedColor,
    icon_selected_color: SharedDerivedColor,
    icon_unselected_color: SharedDerivedColor,
    icon_opacity: SharedDerivedF32,
    icon_size: SharedDerivedF32,

    shape_round: SharedDerived<Corner>,
    shape_square: SharedDerived<Corner>,
    shape_pressed_morph: SharedDerived<Corner>,
    selected_container_shape_round: SharedDerived<Corner>,
    selected_container_shape_square: SharedDerived<Corner>,

    leading_space: SharedDerivedF32,
    between_icon_label_space: SharedDerivedF32,
    trailing_space: SharedDerivedF32,

    focus_ring_indicator_color: SharedDerivedColor,
    focus_ring_indicator_thickness: SharedDerivedF32,
    focus_ring_outline_offset: SharedDerivedF32,

    container_state_layer_color: SharedDerivedColor,
    container_state_layer_selected_color: SharedDerivedColor,
    container_state_layer_unselected_color: SharedDerivedColor,
    container_state_layer_opacity: SharedDerivedF32,
}

macro_rules! get_style {
    ($get_style_method:ident, $theme:ident, $item_state:ident, $button_type:ident) => {
        SharedDerived::from_fn(
            depend!($theme, $item_state, $button_type),
            {
                clone!($theme, $item_state, $button_type);
                move || {
                    let theme = $theme.lock();
                    *theme.get_button_style($button_type.get(), $item_state.get()).unwrap()
                        .$get_style_method(&theme).unwrap()
                }
            }
        )
    }
}

impl ButtonProps {
    pub fn new(mut item_props: ItemProps) -> Self {
        let w = item_props.window_context.clone();
        let theme = w.theme().clone();
        let item_state = item_props.item_state.clone();

        let button_type = SharedDerived::<ButtonType>::new_derived(ButtonType::Filled);

        let container_color = get_style!(get_container_color, theme, item_state, button_type);
        let container_selected_color = get_style!(get_container_selected_color, theme, item_state, button_type);
        let container_unselected_color = get_style!(get_container_unselected_color, theme, item_state, button_type);
        let container_opacity = get_style!(get_container_opacity, theme, item_state, button_type);
        let container_height = get_style!(get_container_height, theme, item_state, button_type);

        let shadow_color = get_style!(get_shadow_color, theme, item_state, button_type);
        let elevation = get_style!(get_elevation, theme, item_state, button_type);

        let label_color = get_style!(get_label_color, theme, item_state, button_type);
        let label_selected_color = get_style!(get_label_selected_color, theme, item_state, button_type);
        let label_unselected_color = get_style!(get_label_unselected_color, theme, item_state, button_type);
        let label_opacity = get_style!(get_label_opacity, theme, item_state, button_type);
        let label_size = get_style!(get_label_size, theme, item_state, button_type);

        let icon_color = get_style!(get_icon_color, theme, item_state, button_type);
        let icon_selected_color = get_style!(get_icon_selected_color, theme, item_state, button_type);
        let icon_unselected_color = get_style!(get_icon_unselected_color, theme, item_state, button_type);
        let icon_opacity = get_style!(get_icon_opacity, theme, item_state, button_type);
        let icon_size = get_style!(get_icon_size, theme, item_state, button_type);

        let shape_round = get_style!(get_shape_round, theme, item_state, button_type);
        let shape_square = get_style!(get_shape_square, theme, item_state, button_type);
        let shape_pressed_morph = get_style!(get_shape_pressed_morph, theme, item_state, button_type);
        let selected_container_shape_round = get_style!(get_selected_container_shape_round, theme, item_state, button_type);
        let selected_container_shape_square = get_style!(get_selected_container_shape_square, theme, item_state, button_type);

        let leading_space = get_style!(get_leading_space, theme, item_state, button_type);
        let between_icon_label_space = get_style!(get_between_icon_label_space, theme, item_state, button_type);
        let trailing_space = get_style!(get_trailing_space, theme, item_state, button_type);

        let focus_ring_indicator_color = get_style!(get_focus_ring_indicator_color, theme, item_state, button_type);
        let focus_ring_indicator_thickness = get_style!(get_focus_ring_indicator_thickness, theme, item_state, button_type);
        let focus_ring_outline_offset = get_style!(get_focus_ring_outline_offset, theme, item_state, button_type);

        let container_state_layer_color = get_style!(get_container_state_layer_color, theme, item_state, button_type);
        let container_state_layer_selected_color = get_style!(get_container_state_layer_selected_color, theme, item_state, button_type);
        let container_state_layer_unselected_color = get_style!(get_container_state_layer_unselected_color, theme, item_state, button_type);
        let container_state_layer_opacity = get_style!(get_container_state_layer_opacity, theme, item_state, button_type);

        item_props.height = SharedDerivedSize::from_fn(depend!(container_height), {
            clone!(container_height);
            move || container_height.get().into()
        });
        item_props.padding = Padding::horizontal(24.0);
        Self {
            item_props,
            button_type,
            toggleable: false.into(),
            container_color,
            container_selected_color,
            container_unselected_color,
            container_opacity,
            container_height,
            shadow_color,
            elevation,
            label_color,
            label_selected_color,
            label_unselected_color,
            label_opacity,
            label_size,
            icon_color,
            icon_selected_color,
            icon_unselected_color,
            icon_opacity,
            icon_size,
            shape_round,
            shape_square,
            shape_pressed_morph,
            selected_container_shape_round,
            selected_container_shape_square,
            leading_space,
            between_icon_label_space,
            trailing_space,
            focus_ring_indicator_color,
            focus_ring_indicator_thickness,
            focus_ring_outline_offset,
            container_state_layer_color,
            container_state_layer_selected_color,
            container_state_layer_unselected_color,
            container_state_layer_opacity,
        }
    }
}

pub fn button(props: ButtonProps, text: impl FnOnce(LabelProps, SharedSource<ItemState>) -> Item) -> Item {
    let w = props.item_props.window_context.clone();
    let text = text(
        w.label_props("label")
            .color(&props.label_color),
        props.item_state.clone(),
    );
    stack(
        StackProps::new(props.item_props)
            .height(40)
            .background(
                rectangle(
                    w.rectangle_props(&props.container_color)
                )
            )
            .foreground(
                ripple(w.ripple_props().color(&props.container_state_layer_color))
            )
            .alignment(Alignment::center()),
        vec![
            text,
        ],
    )
}

pub mod button_style {
    use std::any::Any;
    use crate::ui::widget::button::ItemState;
    use crate::theme::shape::Corner;
    use crate::theme::{color, elevation, shape, StateStyles, ThemeValue};
    use crate::ui::{ Color};
    use crate::{Let, Theme, With};
    use proc_macro::style;
    use crate::ui::button::ButtonType;

    #[style]
    pub struct ButtonStyle {
        container_color: Color,
        container_opacity: f32,
        container_unselected_color: Color,
        container_selected_color: Color,
        container_height: f32,
        container_state_layer_color: Color,
        container_state_layer_opacity: f32,
        container_state_layer_unselected_color: Color,
        container_state_layer_selected_color: Color,

        shadow_color: Color,
        elevation: f32,

        label_color: Color,
        label_opacity: f32,
        label_unselected_color: Color,
        label_selected_color: Color,
        label_size: f32,

        icon_color: Color,
        icon_opacity: f32,
        icon_unselected_color: Color,
        icon_selected_color: Color,
        icon_size: f32,

        leading_space: f32,
        trailing_space: f32,
        between_icon_label_space: f32,

        shape_round: Corner,
        shape_square: Corner,
        shape_pressed_morph: Corner,

        selected_container_shape_round: Corner,
        selected_container_shape_square: Corner,

        focus_ring_indicator_color: Color,
        focus_ring_indicator_thickness: f32,
        focus_ring_outline_offset: f32,
    }

    impl Default for ButtonStyle {
        fn default() -> Self {
            ButtonStyle {
                container_color: color::SURFACE_CONTAINER_LOW.into(),
                container_unselected_color: color::SURFACE_CONTAINER_LOW.into(),
                container_selected_color: color::PRIMARY.into(),
                container_opacity: 1.0.into(),
                container_height: 40.0.into(),
                container_state_layer_color: color::ON_PRIMARY.into(),
                container_state_layer_opacity: 0.08.into(),
                container_state_layer_unselected_color: color::PRIMARY.into(),
                container_state_layer_selected_color: color::ON_PRIMARY.into(),
                shadow_color: color::SHADOW.into(),
                elevation: elevation::LEVEL_1.into(),
                label_color: color::PRIMARY.into(),
                label_opacity: 1.0.into(),
                label_unselected_color: color::PRIMARY.into(),
                label_selected_color: color::ON_PRIMARY.into(),
                label_size: 14.0.into(),
                icon_color: color::PRIMARY.into(),
                icon_opacity: 1.0.into(),
                icon_unselected_color: color::PRIMARY.into(),
                icon_selected_color: color::ON_PRIMARY.into(),
                icon_size: 20.0.into(),
                leading_space: 24.0.into(),
                trailing_space: 24.0.into(),
                between_icon_label_space: 8.0.into(),
                shape_round: shape::corner::FULL.into(),
                shape_square: shape::corner::MEDIUM.into(),
                shape_pressed_morph: shape::corner::SMALL.into(),
                selected_container_shape_round: shape::corner::MEDIUM.into(),
                selected_container_shape_square: shape::corner::FULL.into(),
                focus_ring_indicator_color: color::SECONDARY.into(),
                focus_ring_indicator_thickness: 3.0.into(),
                focus_ring_outline_offset: 2.0.into(),
            }
        }
    }

    fn elevated_button_style() -> StateStyles<ButtonStyle> {
        StateStyles::enabled(ButtonStyle::default())
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

    fn filled_button_style() -> StateStyles<ButtonStyle> {
        StateStyles::enabled(
            ButtonStyle::default().with_mut(|style| {
                style
                    .set_container_color(color::PRIMARY)
                    .set_container_unselected_color(color::SURFACE_CONTAINER)
                    .set_container_selected_color(color::PRIMARY)
                    .set_elevation(elevation::LEVEL_0)
                    .set_label_color(color::ON_PRIMARY)
                    .set_label_unselected_color(color::ON_SURFACE_VARIANT)
                    .set_label_selected_color(color::ON_PRIMARY)
                    .set_icon_color(color::ON_PRIMARY)
                    .set_icon_unselected_color(color::ON_SURFACE_VARIANT)
                    .set_icon_selected_color(color::ON_PRIMARY);
            })
        ).disabled(|style| {
            style
                .set_container_color(color::ON_SURFACE)
                .set_container_opacity(0.1)
                .set_elevation(elevation::LEVEL_0)
                .set_label_color(color::ON_SURFACE)
                .set_label_opacity(0.38)
                .set_icon_color(color::ON_SURFACE)
                .set_icon_opacity(0.38);
        }).hovered(|style| {
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
        theme.set_button_style(ButtonType::Elevated, style);
    }

    pub fn apply_filled_button_style(theme: &mut Theme) {
        let style = filled_button_style();
        theme.set_button_style(ButtonType::Filled, style);
    }
}
