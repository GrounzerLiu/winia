use crate::impl_property_layout;
use crate::shared::{Gettable, Observable, Settable, Shared, SharedColor, SharedDrawable, SharedText};
use crate::skia_safe::Vector;
use crate::ui::app::WindowContext;
use crate::ui::component::style::{ButtonStyle, State};
use crate::ui::component::{ImageExt, RectangleExt, RippleExt, TextExt};
use crate::ui::item::{Alignment, ItemData, ItemState, LayoutDirection, Size};
use crate::ui::layout::{RowExt, StackExt};
use crate::ui::theme::Access;
use crate::ui::{Item, Theme};
use proc_macro::item;
use skia_safe::{Color, Path, RRect, Rect};
use std::fmt::Display;
use std::ops::{Deref, DerefMut};

#[derive(Clone)]
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
    outer_icon: Option<SharedDrawable>,
    text: SharedText,
    icon_size: Shared<Size>,
    button_type: Shared<ButtonType>,
    outer_button_type: Option<Shared<ButtonType>>,
}

#[item(text: impl Into<SharedText>)]
pub struct Button {
    item: Item,
    property: Shared<ButtonProperty>,
}

// impl_property_layout!(Button, button_type, Shared<ButtonType>);
// impl_property_layout!(Button, icon, SharedDrawable);



impl Button {
    pub fn icon(self, icon: impl Into<SharedDrawable>) -> Self {
        {
            let mut property = self.property.lock();
            if let Some(icon) = property.outer_icon.take() {
                icon.remove_observer(self.item.data().get_id());
            }
            let property_icon = property.icon.clone();
            let icon = icon.into();
            icon.add_specific_observer(
                self.item.data().get_id(),
                move |icon| {
                    property_icon.set_static(icon.clone_drawable())
                },
            );
            property.icon.notify();
            property.outer_icon = Some(icon);
        }
        self
    }

    pub fn button_type(self, button_type: impl Into<Shared<ButtonType>>) -> Self {
        {
            let id = self.item.data().get_id();
            let event_loop_proxy = self.item.data().get_window_context().event_loop_proxy().clone();
            let mut property = self.property.lock();
            property.button_type.remove_observer(id);
            property.button_type = button_type.into();
            property.button_type.add_specific_observer(
                id,
                move |_| {
                    event_loop_proxy.request_layout();
                },
            );
        }
        self.property.notify();
        self
    }
}

impl Button {
    pub fn new(window_context: &WindowContext, text: impl Into<SharedText>) -> Self {
        let property = Shared::from(ButtonProperty{
            icon: SharedDrawable::empty(),
            outer_icon: None,
            text: text.into(),
            icon_size: Shared::from(Size::Fixed(0.0)),
            button_type: Shared::from(ButtonType::Filled),
            outer_button_type: None,
        });

        let icon_size = property.lock().icon_size.clone();

        let theme = window_context.theme();

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

        let property_ = property.lock();
        // let image = property_.image.clone();
        let text = property_.text.clone();
        drop(property_);

        let item = window_context.stack(
                window_context.row(
                    window_context.stack(
                        window_context
                            .image(property.lock().icon.clone())
                            .color(&icon_color)
                            .item()
                            .size(Size::Fixed(18.0), Size::Fixed(18.0))
                            .align_content(Alignment::Center)
                            .opacity(&icon_opacity)
                    ).item()
                        .clip(true)
                        .margin_end(8.0)
                        .size(&icon_size, Size::Fixed(18.0)) +
                    window_context
                        .text(text)
                        .editable(false)
                        .color(&label_color)
                        .font_size(&label_size)
                        .item()
                        .opacity(&label_opacity),
                ),
            )
            .item()
            .background(
                window_context
                    .rectangle(&container_color)
                    .radius_bottom_start(&container_radius_bottom_end)
                    .radius_bottom_end(&container_radius_bottom_start)
                    .radius_top_start(&container_radius_top_start)
                    .radius_top_end(&container_radius_top_end)
                    .outline_color(&outline_color)
                    .outline_width(&outline_width)
                    .item()
                    .elevation(&container_elevation)
                    .opacity(&container_opacity)
                    .foreground(
                        window_context.ripple()
                            .color(&layer_state_color)
                            .item().clip(true)
                            .clip_shape({
                                let container_radius_top_start = container_radius_top_start.clone();
                                let container_radius_top_end = container_radius_top_end.clone();
                                let container_radius_bottom_start = container_radius_bottom_start.clone();
                                let container_radius_bottom_end = container_radius_bottom_end.clone();
                                let shape: Box<dyn Fn(&mut ItemData) -> Path +Send > = Box::new(move |item: &mut ItemData|{
                                    let display_parameter = item.get_display_parameter();

                                    let radius_top_start = container_radius_top_start.get();
                                    let radius_top_end = container_radius_top_end.get();
                                    let radius_bottom_start = container_radius_bottom_start.get();
                                    let radius_bottom_end = container_radius_bottom_end.get();

                                    let layout_direction = item.get_layout_direction().get();
                                    let rect = Rect::from_xywh(
                                        display_parameter.x(),
                                        display_parameter.y(),
                                        display_parameter.width,
                                        display_parameter.height,
                                    );
                                    let rrect = if layout_direction == LayoutDirection::LTR {
                                        RRect::new_rect_radii(
                                            &rect,
                                            &[
                                                Vector::new(radius_top_start, radius_top_start),
                                                Vector::new(radius_top_end, radius_top_end),
                                                Vector::new(radius_bottom_end, radius_bottom_end),
                                                Vector::new(radius_bottom_start, radius_bottom_start),
                                            ],
                                        )
                                    } else {
                                        RRect::new_rect_radii(
                                            &rect,
                                            &[
                                                Vector::new(radius_top_end, radius_top_end),
                                                Vector::new(radius_top_start, radius_top_start),
                                                Vector::new(radius_bottom_start, radius_bottom_start),
                                                Vector::new(radius_bottom_end, radius_bottom_end),
                                            ],
                                        )
                                    };
                                    Path::rrect(rrect, None)
                                });
                                Shared::from_static(shape)
                            })
                    )
            )
            .align_content(Alignment::CenterStart)
            .width(Size::Auto)
            .height(&container_height)
            .padding_start(16.0)
            .padding_end(24.0);

        fn set_property_from_theme<T: Send + 'static>(shared: &Shared<T>, theme: &Shared<Theme>, state: &Shared<ItemState>, property: &Shared<ButtonProperty>, f: impl Fn(State) -> T + Send + 'static) {
            let theme = theme.clone();
            let state = state.clone();
            let property = property.clone();
            // let f = Box::new(f);
            shared.set_dynamic(
                [theme.as_ref().into(), state.as_ref().into(), property.as_ref().into()].into(),
                move || {
                    let mut theme = theme.lock();
                    let theme_mut = theme.deref_mut();
                    let button_type = property.lock().button_type.get();
                    let state = state.get();
                    let mut style = ButtonStyle::new(theme_mut, button_type.to_string() + "_button");
                    // println!("button_type: {}, time: {:?}", button_type, std::time::SystemTime::now());
                    let state = match state {
                        ItemState::Enabled => style.enable(),
                        ItemState::Disabled => style.disable(),
                        ItemState::Focused => style.focus(),
                        ItemState::Hovered => style.hover(),
                        ItemState::Pressed => style.press(),
                    };
                    f(state)
                },
            );
        }

        let state = item.data().get_state().clone();
        let mut property = property.clone();

        set_property_from_theme(
            &container_radius_top_start,
            &theme,
            &state,
            &property,
            |mut state| state.container().shape().top_start().get().unwrap_or(0.0),
        );

        set_property_from_theme(
            &container_radius_top_end,
            &theme,
            &state,
            &property,
            |mut state| state.container().shape().top_end().get().unwrap_or(0.0),
        );

        set_property_from_theme(
            &container_radius_bottom_start,
            &theme,
            &state,
            &property,
            |mut state| state.container().shape().bottom_start().get().unwrap_or(0.0),
        );

        set_property_from_theme(
            &container_radius_bottom_end,
            &theme,
            &state,
            &property,
            |mut state| state.container().shape().bottom_end().get().unwrap_or(0.0),
        );

        set_property_from_theme(
            &container_height,
            &theme,
            &state,
            &property,
            |mut state| state.container().height().get().map_or(Size::Auto, |height| {
                if height > 0.0 {
                    Size::Fixed(height)
                } else {
                    Size::Auto
                }
            }),
        );

        set_property_from_theme(
            &container_elevation,
            &theme,
            &state,
            &property,
            |mut state| state.container().elevation().get().unwrap_or(0.0),
        );

        set_property_from_theme(
            &container_shadow_color,
            &theme,
            &state,
            &property,
            |mut state| state.container().shadow_color().get().unwrap_or(Color::TRANSPARENT),
        );

        set_property_from_theme(
            &container_color,
            &theme,
            &state,
            &property,
            |mut state| state.container().color().get().unwrap_or(Color::TRANSPARENT),
        );

        set_property_from_theme(
            &container_opacity,
            &theme,
            &state,
            &property,
            |mut state| state.container().opacity().get().unwrap_or(1.0),
        );

        set_property_from_theme(
            &outline_width,
            &theme,
            &state,
            &property,
            |mut state| state.outline().width().get().unwrap_or(0.0),
        );

        set_property_from_theme(
            &outline_color,
            &theme,
            &state,
            &property,
            |mut state| state.outline().color().get().unwrap_or(Color::TRANSPARENT),
        );

        // set_property_from_theme(
        //     &label_size,
        //     &theme,
        //     &state,
        //     &property,
        //     |mut state| state.label().size().get().unwrap_or(16.0),
        // );

        set_property_from_theme(
            &label_color,
            &theme,
            &state,
            &property,
            |mut state| state.label().color().get().unwrap_or(Color::BLACK),
        );

        set_property_from_theme(
            &label_opacity,
            &theme,
            &state,
            &property,
            |mut state| state.label().opacity().get().unwrap_or(1.0),
        );

        set_property_from_theme(
            &icon_color,
            &theme,
            &state,
            &property,
            |mut state| state.icon().color().get(),
        );
        
        set_property_from_theme(
            &icon_opacity,
            &theme,
            &state,
            &property,
            |mut state| state.icon().opacity().get().unwrap_or(1.0),
        );

        set_property_from_theme(
            &layer_state_color,
            &theme,
            &state,
            &property,
            |mut state| state.state_layer().color().get().unwrap_or(Color::TRANSPARENT),
        );

        {
            let theme = theme.clone();
            let property = property.clone();
            // let f = Box::new(f);
            layer_state_color.set_dynamic(
                [theme.as_ref().into(), state.as_ref().into()].into(),
                move || {
                    let mut theme = theme.lock();
                    let theme_mut = theme.deref_mut();
                    let button_type = property.lock().button_type.get();
                    let mut style = ButtonStyle::new(theme_mut, button_type.to_string() + "_button");
                    // println!("button_type: {}, time: {:?}", button_type, std::time::SystemTime::now());
                    style.hover().state_layer().color().get().unwrap_or(Color::TRANSPARENT)
                },
            );
        }


        {
            let icon_size = icon_size.clone();
            let event_loop_proxy = window_context.event_loop_proxy().clone();
            let property = property.lock();
            property.icon.add_specific_observer(
                item.data().get_id(),
                move |image| {
                    event_loop_proxy.request_layout();
                    if image.is_empty() {
                        icon_size.set(Size::Fixed(0.0));
                    } else {
                        icon_size.set(Size::Fixed(18.0));
                    }
                },
            );
        }

        Self { item, property }
    }
}

pub mod style {
    use crate::ui::theme::{color, elevation, shape, typescale, Access, Shape};
    use crate::ui::theme::StyleProperty;
    use crate::ui::Theme;
    use proc_macro::style;
    use skia_safe::Color;

    #[style]
    pub struct Container {
        shape: Shape,
        height: f32,
        elevation: f32,
        shadow_color: Color,
        color: Color,
        opacity: f32,
    }

    #[style]
    pub struct Outline {
        width: f32,
        color: Color,
    }

    #[style]
    pub struct Label {
        line_height: f32,
        size: f32,
        color: Color,
        weight: f32,
        tracking: f32,
        opacity: f32,
    }

    #[style]
    pub struct Icon {
        size: f32,
        color: Color,
        opacity: f32,
    }

    #[style]
    pub struct StateLayer {
        color: Color,
        opacity: f32,
    }

    #[style]
    pub struct FocusIndicator {
        color: Color,
        thickness: f32,
        offset: f32,
    }

    #[style]
    pub struct State {
        container: Container,
        outline: Outline,
        label: Label,
        state_layer: StateLayer,
        icon: Icon,
        focus_indicator: FocusIndicator,
    }

    #[style]
    pub struct ButtonStyle {
        enable: State,
        disable: State,
        hover: State,
        focus: State,
        press: State,
    }

    pub fn add_button_styles(theme: &mut Theme) {
        add_elevated_button_style(theme, "elevated_button");
        add_filled_button_style(theme, "filled_button");
        add_filled_tonal_button_style(theme, "filled_tonal_button");
        add_outlined_button_style(theme, "outlined_button");
        add_text_button_style(theme, "text_button");
    }

    pub fn add_elevated_button_style(theme: &mut Theme, prefix: &str) {
        let mut style = ButtonStyle::new(theme, prefix);
        fn add_enable_state(state: &mut State) {
            {
                let mut container = state.container();
                {
                    let mut shape = container.shape();
                    let (top_start, top_end, bottom_start, bottom_end) = {
                        let shape = shape::corner::FULL.to_string();
                        (
                            shape.clone() + "_top_start",
                            shape.clone() + "_top_end",
                            shape.clone() + "_bottom_start",
                            shape.clone() + "_bottom_end",
                        )
                    };
                    shape.top_start().set(top_start);
                    shape.top_end().set(top_end);
                    shape.bottom_start().set(bottom_start);
                    shape.bottom_end().set(bottom_end);
                }
                container.height().set(40.0);
                container.elevation().set(elevation::LEVEL_1);
                container.shadow_color().set(color::SHADOW);
                container.color().set(color::SURFACE_CONTAINER_LOW);
            }
            {
                let mut outline = state.outline();
                outline.width().set(0.0);
                outline.color().set(color::OUTLINE);
            }
            {
                let mut label = state.label();
                label.line_height().set(typescale::label_large::LINE_HEIGHT);
                label.size().set(typescale::label_large::SIZE);
                label.color().set(color::PRIMARY);
                label.weight().set(typescale::label_large::WEIGHT);
                label.tracking().set(typescale::label_large::TRACKING);
                label.opacity().set(1.0);
            }
            {
                let mut icon = state.icon();
                icon.size().set(18.0);
                icon.color().set(color::PRIMARY);
                icon.opacity().set(1.0);
            }
        }

        {
            let mut enable = style.enable();
            add_enable_state(&mut enable);
        }

        {
            let mut disable = style.disable();
            add_enable_state(&mut disable);
            {
                let mut container = disable.container();
                container.color().set(color::ON_SURFACE);
                container.opacity().set(0.12);
                container.elevation().set(elevation::LEVEL_0);
            }
            {
                let mut label = disable.label();
                label.color().set(color::ON_SURFACE);
                label.opacity().set(0.38);
            }
            {
                let mut icon = disable.icon();
                icon.color().set(color::ON_SURFACE);
                icon.opacity().set(0.38);
            }
        }

        {
            let mut hover = style.hover();
            add_enable_state(&mut hover);
            {
                let mut container = hover.container();
                container.elevation().set(elevation::LEVEL_2);
            }
            {
                let mut state_layer = hover.state_layer();
                state_layer.color().set(color::PRIMARY);
                state_layer.opacity().set(0.08);
            }
        }

        {
            let mut focus = style.focus();
            add_enable_state(&mut focus);
            {
                let mut focus_indicator = focus.focus_indicator();
                focus_indicator.color().set(color::SECONDARY);
                focus_indicator.thickness().set(3.0);
                focus_indicator.offset().set(2.0);
            }
            {
                let mut state_layer = focus.state_layer();
                state_layer.color().set(color::PRIMARY);
                state_layer.opacity().set(0.1);
            }
        }
        {
            let mut press = style.press();
            add_enable_state(&mut press);
            {
                let mut state_layer = press.state_layer();
                state_layer.color().set(color::PRIMARY);
                state_layer.opacity().set(0.1);
            }
        }
    }

    pub fn add_filled_button_style(theme: &mut Theme, prefix: &str) {
        let mut style = ButtonStyle::new(theme, prefix);
        fn add_enable_state(state: &mut State) {
            {
                let mut container = state.container();
                {
                    let mut shape = container.shape();
                    let (top_start, top_end, bottom_start, bottom_end) = {
                        let shape = shape::corner::FULL.to_string();
                        (
                            shape.clone() + "_top_start",
                            shape.clone() + "_top_end",
                            shape.clone() + "_bottom_start",
                            shape.clone() + "_bottom_end",
                        )
                    };
                    shape.top_start().set(top_start);
                    shape.top_end().set(top_end);
                    shape.bottom_start().set(bottom_start);
                    shape.bottom_end().set(bottom_end);
                }
                container.height().set(40.0);
                container.elevation().set(elevation::LEVEL_0);
                container.shadow_color().set(color::SHADOW);
                container.color().set(color::PRIMARY);
            }
            {
                let mut outline = state.outline();
                outline.width().set(0.0);
                outline.color().set(color::OUTLINE);
            }
            {
                let mut label = state.label();
                label.line_height().set(typescale::label_large::LINE_HEIGHT);
                label.size().set(typescale::label_large::SIZE);
                label.color().set(color::ON_PRIMARY);
                label.weight().set(typescale::label_large::WEIGHT);
                label.tracking().set(typescale::label_large::TRACKING);
                label.opacity().set(1.0);
            }
            {
                let mut icon = state.icon();
                icon.size().set(18.0);
                icon.color().set(color::ON_PRIMARY);
                icon.opacity().set(1.0);
            }
        }

        {
            let mut enable = style.enable();
            add_enable_state(&mut enable);
        }

        {
            let mut disable = style.disable();
            add_enable_state(&mut disable);
            {
                let mut container = disable.container();
                container.color().set(color::ON_SURFACE);
                container.opacity().set(0.12);
            }
            {
                let mut label = disable.label();
                label.color().set(color::ON_SURFACE);
                label.opacity().set(0.38);
            }
            {
                let mut icon = disable.icon();
                icon.color().set(color::ON_SURFACE);
                icon.opacity().set(0.38);
            }
        }

        {
            let mut hover = style.hover();
            add_enable_state(&mut hover);
            {
                let mut state_layer = hover.state_layer();
                state_layer.color().set(color::ON_PRIMARY);
                state_layer.opacity().set(0.08);
            }
        }

        {
            let mut focus = style.focus();
            add_enable_state(&mut focus);
            {
                let mut focus_indicator = focus.focus_indicator();
                focus_indicator.color().set(color::SECONDARY);
                focus_indicator.thickness().set(3.0);
                focus_indicator.offset().set(2.0);
            }
            {
                let mut state_layer = focus.state_layer();
                state_layer.color().set(color::PRIMARY);
                state_layer.opacity().set(0.1);
            }
        }
        {
            let mut press = style.press();
            add_enable_state(&mut press);
            {
                let mut state_layer = press.state_layer();
                state_layer.color().set(color::ON_PRIMARY);
                state_layer.opacity().set(0.1);
            }
        }
    }

    pub fn add_filled_tonal_button_style(theme: &mut Theme, prefix: &str) {
        let mut style = ButtonStyle::new(theme, prefix);
        fn add_enable_state(state: &mut State) {
            {
                let mut container = state.container();
                {
                    let mut shape = container.shape();
                    let (top_start, top_end, bottom_start, bottom_end) = {
                        let shape = shape::corner::FULL.to_string();
                        (
                            shape.clone() + "_top_start",
                            shape.clone() + "_top_end",
                            shape.clone() + "_bottom_start",
                            shape.clone() + "_bottom_end",
                        )
                    };
                    shape.top_start().set(top_start);
                    shape.top_end().set(top_end);
                    shape.bottom_start().set(bottom_start);
                    shape.bottom_end().set(bottom_end);
                }
                container.height().set(40.0);
                container.elevation().set(elevation::LEVEL_0);
                container.shadow_color().set(color::SHADOW);
                container.color().set(color::SECONDARY_CONTAINER);
            }
            {
                let mut outline = state.outline();
                outline.width().set(0.0);
                outline.color().set(color::OUTLINE);
            }
            {
                let mut label = state.label();
                label.line_height().set(typescale::label_large::LINE_HEIGHT);
                label.size().set(typescale::label_large::SIZE);
                label.color().set(color::ON_SECONDARY_CONTAINER);
                label.weight().set(typescale::label_large::WEIGHT);
                label.tracking().set(typescale::label_large::TRACKING);
                label.opacity().set(1.0);
            }
            {
                let mut icon = state.icon();
                icon.size().set(18.0);
                icon.color().set(color::ON_SECONDARY_CONTAINER);
                icon.opacity().set(1.0);
            }
        }

        {
            let mut enable = style.enable();
            add_enable_state(&mut enable);
        }

        {
            let mut disable = style.disable();
            add_enable_state(&mut disable);
            {
                let mut container = disable.container();
                container.color().set(color::ON_SURFACE);
                container.opacity().set(0.12);
            }
            {
                let mut label = disable.label();
                label.color().set(color::ON_SURFACE);
                label.opacity().set(0.38);
            }
            {
                let mut icon = disable.icon();
                icon.color().set(color::ON_SURFACE);
                icon.opacity().set(0.38);
            }
        }

        {
            let mut hover = style.hover();
            add_enable_state(&mut hover);
            {
                let mut state_layer = hover.state_layer();
                state_layer.color().set(color::ON_SECONDARY_CONTAINER);
                state_layer.opacity().set(0.08);
            }
        }

        {
            let mut focus = style.focus();
            add_enable_state(&mut focus);
            {
                let mut focus_indicator = focus.focus_indicator();
                focus_indicator.color().set(color::SECONDARY);
                focus_indicator.thickness().set(3.0);
                focus_indicator.offset().set(2.0);
            }
            {
                let mut state_layer = focus.state_layer();
                state_layer.color().set(color::ON_SECONDARY_CONTAINER);
                state_layer.opacity().set(0.1);
            }
        }
        {
            let mut press = style.press();
            add_enable_state(&mut press);
            {
                let mut state_layer = press.state_layer();
                state_layer.color().set(color::ON_SECONDARY_CONTAINER);
                state_layer.opacity().set(0.1);
            }
        }
    }

    pub fn add_outlined_button_style(theme: &mut Theme, prefix: &str) {
        let mut style = ButtonStyle::new(theme, prefix);
        fn add_enable_state(state: &mut State) {
            {
                let mut container = state.container();
                {
                    let mut shape = container.shape();
                    let (top_start, top_end, bottom_start, bottom_end) = {
                        let shape = shape::corner::FULL.to_string();
                        (
                            shape.clone() + "_top_start",
                            shape.clone() + "_top_end",
                            shape.clone() + "_bottom_start",
                            shape.clone() + "_bottom_end",
                        )
                    };
                    shape.top_start().set(top_start);
                    shape.top_end().set(top_end);
                    shape.bottom_start().set(bottom_start);
                    shape.bottom_end().set(bottom_end);
                }
                container.height().set(40.0);
            }
            {
                let mut outline = state.outline();
                outline.width().set(1.0);
                outline.color().set(color::OUTLINE);
            }
            {
                let mut label = state.label();
                label.line_height().set(typescale::label_large::LINE_HEIGHT);
                label.size().set(typescale::label_large::SIZE);
                label.color().set(color::PRIMARY);
                label.weight().set(typescale::label_large::WEIGHT);
                label.tracking().set(typescale::label_large::TRACKING);
                label.opacity().set(1.0);
            }
            {
                let mut icon = state.icon();
                icon.size().set(18.0);
                icon.color().set(color::PRIMARY);
                icon.opacity().set(1.0);
            }
        }

        {
            let mut enable = style.enable();
            add_enable_state(&mut enable);
        }

        {
            let mut disable = style.disable();
            add_enable_state(&mut disable);
            {
                let mut label = disable.label();
                label.color().set(color::ON_SURFACE);
                label.opacity().set(0.38);
            }
            {
                let mut icon = disable.icon();
                icon.color().set(color::ON_SURFACE);
                icon.opacity().set(0.38);
            }
            {
                let mut outline = disable.outline();
                outline.color().set(color::ON_SURFACE);
                outline.width().set(0.12);
            }
        }

        {
            let mut hover = style.hover();
            add_enable_state(&mut hover);
            {
                let mut state_layer = hover.state_layer();
                state_layer.color().set(color::PRIMARY);
                state_layer.opacity().set(0.08);
            }
        }

        {
            let mut focus = style.focus();
            add_enable_state(&mut focus);
            {
                let mut focus_indicator = focus.focus_indicator();
                focus_indicator.color().set(color::SECONDARY);
                focus_indicator.thickness().set(3.0);
                focus_indicator.offset().set(2.0);
            }
            {
                let mut state_layer = focus.state_layer();
                state_layer.color().set(color::PRIMARY);
                state_layer.opacity().set(0.1);
            }
        }
        {
            let mut press = style.press();
            add_enable_state(&mut press);
            {
                let mut state_layer = press.state_layer();
                state_layer.color().set(color::PRIMARY);
                state_layer.opacity().set(0.1);
            }
        }
    }

    pub fn add_text_button_style(theme: &mut Theme, prefix: &str) {
        let mut style = ButtonStyle::new(theme, prefix);
        fn add_enable_state(state: &mut State) {
            {
                let mut container = state.container();
                {
                    let mut shape = container.shape();
                    let (top_start, top_end, bottom_start, bottom_end) = {
                        let shape = shape::corner::FULL.to_string();
                        (
                            shape.clone() + "_top_start",
                            shape.clone() + "_top_end",
                            shape.clone() + "_bottom_start",
                            shape.clone() + "_bottom_end",
                        )
                    };
                    shape.top_start().set(top_start);
                    shape.top_end().set(top_end);
                    shape.bottom_start().set(bottom_start);
                    shape.bottom_end().set(bottom_end);
                }
                container.height().set(40.0);
            }
            {
                let mut label = state.label();
                label.line_height().set(typescale::label_large::LINE_HEIGHT);
                label.size().set(typescale::label_large::SIZE);
                label.color().set(color::PRIMARY);
                label.weight().set(typescale::label_large::WEIGHT);
                label.tracking().set(typescale::label_large::TRACKING);
                label.opacity().set(1.0);
            }
            {
                let mut icon = state.icon();
                icon.size().set(18.0);
                icon.color().set(color::PRIMARY);
                icon.opacity().set(1.0);
            }
        }

        {
            let mut enable = style.enable();
            add_enable_state(&mut enable);
        }

        {
            let mut disable = style.disable();
            add_enable_state(&mut disable);
            {
                let mut label = disable.label();
                label.color().set(color::ON_SURFACE);
                label.opacity().set(0.38);
            }
            {
                let mut icon = disable.icon();
                icon.color().set(color::ON_SURFACE);
                icon.opacity().set(0.38);
            }
        }

        {
            let mut hover = style.hover();
            add_enable_state(&mut hover);
            {
                let mut state_layer = hover.state_layer();
                state_layer.color().set(color::PRIMARY);
                state_layer.opacity().set(0.08);
            }
        }

        {
            let mut focus = style.focus();
            add_enable_state(&mut focus);
            {
                let mut focus_indicator = focus.focus_indicator();
                focus_indicator.color().set(color::SECONDARY);
                focus_indicator.thickness().set(3.0);
                focus_indicator.offset().set(2.0);
            }
            {
                let mut state_layer = focus.state_layer();
                state_layer.color().set(color::PRIMARY);
                state_layer.opacity().set(0.1);
            }
        }
        {
            let mut press = style.press();
            add_enable_state(&mut press);
            {
                let mut state_layer = press.state_layer();
                state_layer.color().set(color::PRIMARY);
                state_layer.opacity().set(0.1);
            }
        }
    }
}