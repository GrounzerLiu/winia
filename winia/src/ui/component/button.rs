use crate::core::next_id;
use crate::shared::{Gettable, Settable, Shared, SharedDrawable, SharedText};
use crate::ui::animation::AnimationExt;
use crate::ui::app::WindowContext;
use crate::ui::component::style::ButtonStyle;
use crate::ui::component::{ImageExt, RectangleExt, RippleExt, ScaleMode, TextExt};
use crate::ui::item::{Alignment, ItemState, Size};
use crate::ui::layout::{AlignItems, RowExt, StackExt};
use crate::ui::Item;
use crate::exclude_target;
use clonelet::clone;
use proc_macro::item;
use skia_safe::Color;
use std::fmt::Display;
use std::time::Duration;
use strum_macros::EnumString;

#[derive(Clone, EnumString, Debug)]
pub enum ButtonType {
    Elevated,
    Filled,
    FilledTonal,
    Outlined,
    Text,
}

impl Display for ButtonType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ButtonType::Elevated => write!(f, "elevated"),
            ButtonType::Filled => write!(f, "filled"),
            ButtonType::FilledTonal => write!(f, "filled_tonal"),
            ButtonType::Outlined => write!(f, "outlined"),
            ButtonType::Text => write!(f, "text"),
        }
    }
}

struct ButtonProperty {
    icon: SharedDrawable,
    label: SharedText,
    selectable: Shared<bool>,
    selected: Shared<bool>,
    style: Shared<ButtonStyle>,
}

#[item(text: impl Into<SharedText>)]
pub struct Button {
    item: Item,
    property: Shared<ButtonProperty>,
}

// impl_property_layout!(Button, button_type, Shared<ButtonType>);

impl Button {
    pub fn icon(self, icon: impl Into<SharedDrawable>) -> Self {
        let property = self.property.lock();
        property.icon.set_shared(icon);
        drop(property);
        self
    }

    pub fn label(self, label: impl Into<SharedText>) -> Self {
        let property = self.property.lock();
        property.label.set_shared(label);
        drop(property);
        self
    }
    
    pub fn selectable(self, selectable: impl Into<Shared<bool>>) -> Self {
        let property = self.property.lock();
        property.selectable.set_shared(selectable);
        drop(property);
        self
    }
    
    pub fn selected(self, selected: impl Into<Shared<bool>>) -> Self {
        let property = self.property.lock();
        property.selected.set_shared(selected);
        drop(property);
        self
    }
    
    pub fn style(self, style: impl Into<Shared<ButtonStyle>>) -> Self {
        let property = self.property.lock();
        property.style.set_shared(style);
        drop(property);
        self
    }
}

impl Button {
    pub fn new(w: &WindowContext, label: impl Into<SharedText>) -> Self {
        let property = Shared::from(ButtonProperty {
            icon: SharedDrawable::empty(),
            label: label.into(),
            selectable: Shared::from_static(false),
            selected: Shared::from_static(false),
            style: Shared::from(style::elevated_button_style()),
        });

        let theme = w.theme();
        let container_radius_top_start = Shared::from_static(0.0);
        let container_radius_top_end = Shared::from(0.0);
        let container_radius_bottom_start = Shared::from(0.0);
        let container_radius_bottom_end = Shared::from(0.0);
        let container_height = Shared::from(40.0);
        let container_elevation = Shared::from(0.0);
        let container_shadow_color = Shared::from(Color::TRANSPARENT);
        let container_color = Shared::from(Color::TRANSPARENT);
        let container_opacity = Shared::from(1.0);
        let outline_width = Shared::from(0.0);
        let outline_color = Shared::from(Color::TRANSPARENT);
        let label_size = Shared::from(16.0);
        let label_color = Shared::from(Color::BLACK);
        let label_opacity = Shared::from(1.0);

        let icon_color = Shared::from(Some(Color::BLACK));
        let icon_opacity = Shared::from(1.0);

        let layer_state_color = Shared::from(Color::TRANSPARENT);

        let focus_indicator_color = Shared::from(Color::TRANSPARENT);

        let property_ = property.lock();
        let icon = property_.icon.clone();
        let label = property_.label.clone();
        let selectable = property_.selectable.clone();
        let selected = property_.selected.clone();
        let style = property_.style.clone();
        drop(property_);

        let item = w
            .stack(
                w.row(
                    w.image(&icon)
                        .color(&icon_color)
                        .oversize_scale_mode(ScaleMode::Contain)
                        .undersize_scale_mode(ScaleMode::Contain)
                        .item()
                        .visible(Shared::from_dynamic(
                            [icon.to_observable()].into(),
                            move || {
                                !icon.lock().is_empty()
                            }
                        ))
                        .size(Size::Fixed(18.0), Size::Fixed(18.0))
                        .align_content(Alignment::Center)
                        .opacity(&icon_opacity)
                        + w.text(label)
                            .editable(false)
                            .color(&label_color)
                            .font_size(&label_size)
                            .item()
                            .opacity(&label_opacity),
                )
                .align_items(AlignItems::Center)
                .item(),
            )
            .item()
            .background(
                w.rectangle(&container_color)
                    .radius_bottom_start(&container_radius_bottom_end)
                    .radius_bottom_end(&container_radius_bottom_start)
                    .radius_top_start(&container_radius_top_start)
                    .radius_top_end(&container_radius_top_end)
                    .outline_color(&outline_color)
                    .outline_width(&outline_width)
                    .item()
                    .clip(true)
                    .elevation(&container_elevation)
                    .opacity(&container_opacity)
                    .foreground(
                        w.ripple()
                            .color(&layer_state_color)
                            .item()
                    ),
            )
            .foreground(
                w.rectangle(Color::TRANSPARENT)
                    .outline_width(3)
                    .outline_offset(5)
                    .outline_color(&focus_indicator_color)
                    .radius(f32::MAX)
                    .item(),
            )
            .align_content(Alignment::CenterStart)
            .width(Size::Auto)
            .height(&container_height)
            .padding_start(16.0)
            .padding_end(24.0);

        {
            let state = item.data().get_state().clone();
            
            let event_loop_proxy = w.event_loop_proxy().clone();
            let theme = theme.clone();
            state.observe(selected.clone());
            state.add_specific_observer(
                next_id(),
                move |state| {
                    event_loop_proxy.animate(exclude_target!())
                        .transformation({
                            clone!(
                                style,
                                theme,
                                state,
                                selectable,
                                selected,
                                container_radius_top_start,
                                container_radius_top_end,
                                container_radius_bottom_start,
                                container_radius_bottom_end,
                                container_height,
                                container_color,
                                container_elevation,
                                label_color
                            );
                            move || {
                                let theme = theme.lock();
                                let style = style.lock();
                                if selectable.get() {
                                    if selected.get() {
                                        let corner = style.get_shape_square(&theme, state).unwrap();
                                        container_radius_top_start.set(corner.top_start);
                                        container_radius_top_end.set(corner.top_end);
                                        container_radius_bottom_start.set(corner.bottom_start);
                                        container_radius_bottom_end.set(corner.bottom_end);
                                        container_elevation.set(
                                            *style.get_elevation(&theme, state).unwrap(),
                                        );
                                        
                                        container_color.set(
                                            *style.get_container_color_selected(&theme, state).unwrap(),
                                        );
                                        container_height.set(
                                            *style.get_container_height(&theme, state).unwrap(),
                                        );
                                        label_color.set(
                                            *style.get_label_color_selected(&theme, state).unwrap(),
                                        );
                                    } else {
                                        let corner = if state == ItemState::Pressed {
                                            style.get_shape_square(&theme, state).unwrap()
                                        } else {
                                            style.get_shape_round(&theme, state).unwrap()
                                        };
                                        container_radius_top_start.set(corner.top_start);
                                        container_radius_top_end.set(corner.top_end);
                                        container_radius_bottom_start.set(corner.bottom_start);
                                        container_radius_bottom_end.set(corner.bottom_end);
                                        container_elevation.set(
                                            *style.get_elevation(&theme, state).unwrap(),
                                        );
                                        
                                        container_color.set(
                                            *style.get_container_color_unselected(&theme, state).unwrap(),
                                        );
                                        container_height.set(
                                            *style.get_container_height(&theme, state).unwrap(),
                                        );
                                        label_color.set(
                                            *style.get_label_color_unselected(&theme, state).unwrap(),
                                        );
                                    }
                                } else { 
                                    let corner = if state == ItemState::Pressed {
                                        style.get_shape_square(&theme, state).unwrap()
                                    } else {
                                        style.get_shape_round(&theme, state).unwrap()
                                    };
                                    container_radius_top_start.set(corner.top_start);
                                    container_radius_top_end.set(corner.top_end);
                                    container_radius_bottom_start.set(corner.bottom_start);
                                    container_radius_bottom_end.set(corner.bottom_end);
                                    container_elevation.set(
                                        *style.get_elevation(&theme, state).unwrap(),
                                    );
                                    
                                    container_color.set(
                                        *style.get_container_color(&theme, state).unwrap(),
                                    );
                                    container_height.set(
                                        *style.get_container_height(&theme, state).unwrap(),
                                    );
                                    label_color.set(
                                        *style.get_label_color(&theme, state).unwrap(),
                                    );
                                }
                            }
                        })
                        .duration(Duration::from_millis(500))
                        .start();
                }
            );
            state.notify()
            // focus_indicator_color.set_dynamic(
            //     [state.to_observable(), theme.to_observable()].into(),
            //     move || {
            //         if state.get() == ItemState::Focused {
            //             theme.lock().get_color(color::SECONDARY).cloned().unwrap()
            //         } else {
            //             Color::TRANSPARENT
            //         }
            //     },
            // );
        }

        item.data().set_focus_next(|item| {
            if !item.get_enabled().get() {
                return true;
            }
            let focused = item.get_focused();
            if !focused.get() {
                focused.set(true);
                false
            } else {
                true
            }
        });

        Self { item, property }
    }
}

pub mod style {
    use crate::ui::item::ItemState;
    use crate::ui::theme::shape::Corner;
    use crate::ui::theme::ThemeValue;
    use crate::ui::theme::{color, elevation, shape, State};
    use crate::ui::Theme;
    use proc_macro::style;
    use skia_safe::Color;

    #[style]
    pub struct ButtonStyle {
        container_color: Color,
        container_opacity: f32,
        container_color_unselected: Color,
        container_color_selected: Color,
        container_height: f32,

        container_state_layer_color: Color,
        container_state_layer_color_unselected: Color,
        container_state_layer_color_selected: Color,
        container_state_layer_opacity: f32,

        shadow_color: Color,
        elevation: f32,

        label_color: Color,
        label_color_unselected: Color,
        label_color_selected: Color,
        label_opacity: f32,
        label_size: f32,

        icon_color: Color,
        icon_opacity: f32,
        icon_color_unselected: Color,
        icon_color_selected: Color,

        shape_round: Corner,
        shape_square: Corner,
        shape_pressed_morph: Corner,
        selected_container_shape_round: Corner,
        selected_container_shape_square: Corner,

        leading_space: f32,
        between_icon_label_space: f32,
        trailing_space: f32,

        focus_ring_indicator_color: Color,
        focus_ring_indicator_thickness: f32,
        focus_ring_indicator_offset: f32,
    }

    pub static ELEVATED_BUTTON: &str = "elevated_button";
    pub fn elevated_button_style() -> ButtonStyle {
        ButtonStyle {
            container_color: State::new(color::SURFACE_CONTAINER_LOW).disabled(color::ON_SURFACE),
            container_opacity: State::new(1.0).disabled(0.1),
            container_color_unselected: State::new(color::SURFACE_CONTAINER_LOW),
            container_color_selected: State::new(color::PRIMARY),
            container_height: State::new(40.0),

            container_state_layer_color: State::new(color::PRIMARY),
            container_state_layer_color_unselected: State::new(color::PRIMARY),
            container_state_layer_color_selected: State::new(color::ON_PRIMARY),
            container_state_layer_opacity: State::new(0.08).hovered(0.08).focused(0.1).pressed(0.1),

            shadow_color: State::new(color::SHADOW),
            elevation: State::new(elevation::LEVEL_1)
                .disabled(elevation::LEVEL_0)
                .hovered(elevation::LEVEL_2),

            label_color: State::new(color::PRIMARY).disabled(color::ON_SURFACE),
            label_color_unselected: State::new(color::PRIMARY),
            label_color_selected: State::new(color::ON_PRIMARY),
            label_opacity: State::new(1.0).disabled(0.38),
            label_size: State::new(14.0),

            icon_color: State::new(color::PRIMARY).disabled(color::ON_SURFACE),
            icon_opacity: State::new(1.0).disabled(0.38),
            icon_color_unselected: State::new(color::PRIMARY),
            icon_color_selected: State::new(color::ON_PRIMARY),

            shape_round: State::new(shape::corner::FULL),
            shape_square: State::new(shape::corner::MEDIUM),
            shape_pressed_morph: State::new(shape::corner::SMALL),
            selected_container_shape_round: State::new(shape::corner::MEDIUM),
            selected_container_shape_square: State::new(shape::corner::FULL),

            leading_space: State::new(24.0),
            between_icon_label_space: State::new(8.0),
            trailing_space: State::new(24.0),

            focus_ring_indicator_color: State::new(color::SECONDARY),
            focus_ring_indicator_thickness: State::new(3.0),
            focus_ring_indicator_offset: State::new(2.0),
        }
    }

    pub static FILLED_BUTTON: &str = "filled_button";
    pub fn filled_button_style() -> ButtonStyle {
        let mut button_style = elevated_button_style();
        button_style.container_color = State::new(color::PRIMARY).disabled(color::ON_SURFACE);
        button_style.container_opacity = State::new(1.0).disabled(0.1);
        button_style.container_color_unselected = State::new(color::SURFACE_CONTAINER);
        button_style.container_color_selected = State::new(color::PRIMARY);
        button_style.shadow_color = State::new(color::SHADOW);
        button_style.elevation = State::new(elevation::LEVEL_0);
        button_style.label_color = State::new(color::ON_PRIMARY).disabled(color::ON_SURFACE);
        button_style.label_color_unselected = State::new(color::ON_SURFACE_VARIANT);
        button_style.label_color_selected = State::new(color::ON_PRIMARY);
        button_style.icon_color = State::new(color::ON_PRIMARY).disabled(color::ON_SURFACE);
        button_style.icon_color_unselected = State::new(color::ON_SURFACE_VARIANT);
        button_style.icon_color_selected = State::new(color::ON_PRIMARY);
        button_style
    }
}
