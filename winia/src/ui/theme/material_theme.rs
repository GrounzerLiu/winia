use material_colors::color::Argb;
use material_colors::theme::ThemeBuilder;
use crate::ui::component::{button, text_style};
use crate::ui::theme::{colors, dimensions, styles, Style};
use skia_safe::Color;
use crate::ui::{Theme, Value};

fn argb_to_u32(a: u8, r: u8, g: u8, b: u8) -> u32 {
    ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

trait Convert<T> {
    fn convert(&self) -> T;
}

impl Convert<Color> for Argb {
    fn convert(&self) -> Color {
        Color::from_argb(self.alpha, self.red, self.green, self.blue)
    }
}

impl Convert<Argb> for Color {
    fn convert(&self) -> Argb {
        Argb {
            alpha: self.a(),
            red: self.r(),
            green: self.g(),
            blue: self.b(),
        }
    }
}

pub fn material_theme(color: Color, is_dark: bool) -> Theme {
    let theme = ThemeBuilder::with_source(color.convert()).build();
    let scheme = if is_dark {
        &theme.schemes.dark
    } else {
        &theme.schemes.light
    };
    
    let primary = scheme.primary.convert();
    let on_primary = scheme.on_primary.convert();
    let primary_container = scheme.primary_container.convert();
    let on_primary_container = scheme.on_primary_container.convert();
    let inverse_primary = scheme.inverse_primary.convert();
    let primary_fixed = scheme.primary_fixed.convert();
    let primary_fixed_dim = scheme.primary_fixed_dim.convert();
    let on_primary_fixed = scheme.on_primary_fixed.convert();
    let on_primary_fixed_variant = scheme.on_primary_fixed_variant.convert();
    let secondary = scheme.secondary.convert();
    let on_secondary = scheme.on_secondary.convert();
    let secondary_container = scheme.secondary_container.convert();
    let on_secondary_container = scheme.on_secondary_container.convert();
    let secondary_fixed = scheme.secondary_fixed.convert();
    let secondary_fixed_dim = scheme.secondary_fixed_dim.convert();
    let on_secondary_fixed = scheme.on_secondary_fixed.convert();
    let on_secondary_fixed_variant = scheme.on_secondary_fixed_variant.convert();
    let tertiary = scheme.tertiary.convert();
    let on_tertiary = scheme.on_tertiary.convert();
    let tertiary_container = scheme.tertiary_container.convert();
    let on_tertiary_container = scheme.on_tertiary_container.convert();
    let tertiary_fixed = scheme.tertiary_fixed.convert();
    let tertiary_fixed_dim = scheme.tertiary_fixed_dim.convert();
    let on_tertiary_fixed = scheme.on_tertiary_fixed.convert();
    let on_tertiary_fixed_variant = scheme.on_tertiary_fixed_variant.convert();
    let error = scheme.error.convert();
    let on_error = scheme.on_error.convert();
    let error_container = scheme.error_container.convert();
    let on_error_container = scheme.on_error_container.convert();
    let surface_dim = scheme.surface_dim.convert();
    let surface = scheme.surface.convert();
    let surface_tint = scheme.surface_tint.convert();
    let surface_bright = scheme.surface_bright.convert();
    let surface_container_lowest = scheme.surface_container_lowest.convert();
    let surface_container_low = scheme.surface_container_low.convert();
    let surface_container = scheme.surface_container.convert();
    let surface_container_high = scheme.surface_container_high.convert();
    let surface_container_highest = scheme.surface_container_highest.convert();
    let on_surface = scheme.on_surface.convert();
    let on_surface_variant = scheme.on_surface_variant.convert();
    let outline = scheme.outline.convert();
    let outline_variant = scheme.outline_variant.convert();
    let inverse_surface = scheme.inverse_surface.convert();
    let inverse_on_surface = scheme.inverse_on_surface.convert();
    let surface_variant = scheme.surface_variant.convert();
    let background = scheme.background.convert();
    let on_background = scheme.on_background.convert();
    let shadow = scheme.shadow.convert();
    let scrim = scheme.scrim.convert();
    
    let mut theme = Theme::new();
    theme.set_color(colors::PRIMARY, primary)
        .set_color(colors::ON_PRIMARY, on_primary)
        .set_color(colors::PRIMARY_CONTAINER, primary_container)
        .set_color(colors::ON_PRIMARY_CONTAINER, on_primary_container)
        .set_color(colors::INVERSE_PRIMARY, inverse_primary)
        .set_color(colors::PRIMARY_FIXED, primary_fixed)
        .set_color(colors::PRIMARY_FIXED_DIM, primary_fixed_dim)
        .set_color(colors::ON_PRIMARY_FIXED, on_primary_fixed)
        .set_color(colors::ON_PRIMARY_FIXED_VARIANT, on_primary_fixed_variant)
        .set_color(colors::SECONDARY, secondary)
        .set_color(colors::ON_SECONDARY, on_secondary)
        .set_color(colors::SECONDARY_CONTAINER, secondary_container)
        .set_color(colors::ON_SECONDARY_CONTAINER, on_secondary_container)
        .set_color(colors::SECONDARY_FIXED, secondary_fixed)
        .set_color(colors::SECONDARY_FIXED_DIM, secondary_fixed_dim)
        .set_color(colors::ON_SECONDARY_FIXED, on_secondary_fixed)
        .set_color(colors::ON_SECONDARY_FIXED_VARIANT, on_secondary_fixed_variant)
        .set_color(colors::TERTIARY, tertiary)
        .set_color(colors::ON_TERTIARY, on_tertiary)
        .set_color(colors::TERTIARY_CONTAINER, tertiary_container)
        .set_color(colors::ON_TERTIARY_CONTAINER, on_tertiary_container)
        .set_color(colors::TERTIARY_FIXED, tertiary_fixed)
        .set_color(colors::TERTIARY_FIXED_DIM, tertiary_fixed_dim)
        .set_color(colors::ON_TERTIARY_FIXED, on_tertiary_fixed)
        .set_color(colors::ON_TERTIARY_FIXED_VARIANT, on_tertiary_fixed_variant)
        .set_color(colors::ERROR, error)
        .set_color(colors::ON_ERROR, on_error)
        .set_color(colors::ERROR_CONTAINER, error_container)
        .set_color(colors::ON_ERROR_CONTAINER, on_error_container)
        .set_color(colors::SURFACE_DIM, surface_dim)
        .set_color(colors::SURFACE, surface)
        .set_color(colors::SURFACE_TINT, surface_tint)
        .set_color(colors::SURFACE_BRIGHT, surface_bright)
        .set_color(colors::SURFACE_CONTAINER_LOWEST, surface_container_lowest)
        .set_color(colors::SURFACE_CONTAINER_LOW, surface_container_low)
        .set_color(colors::SURFACE_CONTAINER, surface_container)
        .set_color(colors::SURFACE_CONTAINER_HIGH, surface_container_high)
        .set_color(colors::SURFACE_CONTAINER_HIGHEST, surface_container_highest)
        .set_color(colors::ON_SURFACE, on_surface)
        .set_color(colors::ON_SURFACE_VARIANT, on_surface_variant)
        .set_color(colors::OUTLINE, outline)
        .set_color(colors::OUTLINE_VARIANT, outline_variant)
        .set_color(colors::INVERSE_SURFACE, inverse_surface)
        .set_color(colors::INVERSE_ON_SURFACE, inverse_on_surface)
        .set_color(colors::SURFACE_VARIANT, surface_variant)
        .set_color(colors::BACKGROUND, background)
        .set_color(colors::ON_BACKGROUND, on_background)
        .set_color(colors::SHADOW, shadow)
        .set_color(colors::SCRIM, scrim)
    
        .set_color(colors::WINDOW_BACKGROUND_COLOR, colors::BACKGROUND);

    theme.set_dimension(dimensions::elevation::LEVEL_0, 0.0)
        .set_dimension(dimensions::elevation::LEVEL_1, 1.0)
        .set_dimension(dimensions::elevation::LEVEL_2, 3.0)
        .set_dimension(dimensions::elevation::LEVEL_3, 6.0)
        .set_dimension(dimensions::elevation::LEVEL_4, 8.0)
        .set_dimension(dimensions::elevation::LEVEL_5, 12.0);

    let elevated_button = button::style::Button{
        enable: button::style::Enable{
            container: button::style::Container{
                height: Value::Value(40.0),
                elevation: Value::Value(1.0),
                shadow_color: Value::Ref(colors::SHADOW.to_owned()),
                color: Value::Ref(colors::SURFACE_CONTAINER_LOW.to_owned()),
                opacity: Value::Value(1.0),
            },
            label: button::style::Label {
                size: Value::Value(14.0),
                color: Value::Ref(colors::ON_SURFACE.to_owned()),
                opacity: Value::Value(1.0),
            },
            icon: button::style::Icon {
                size: Value::Value(18.0),
                color: Value::Ref(colors::PRIMARY.to_owned()),
                opacity: Value::Value(1.0),
            },
        },
        disable: button::style::Disable{
            container: button::style::Container{
                height: Value::Value(40.0),
                elevation: Value::Value(0.0),
                shadow_color: Value::Ref(colors::SHADOW.to_owned()),
                color: Value::Ref(colors::SURFACE_CONTAINER_LOW.to_owned()),
                opacity: Value::Value(0.12),
            },
            label: button::style::Label {
                size: Value::Value(14.0),
                color: Value::Ref(colors::ON_SURFACE.to_owned()),
                opacity: Value::Value(0.38),
            },
            icon: button::style::Icon {
                size: Value::Value(18.0),
                color: Value::Ref(colors::PRIMARY.to_owned()),
                opacity: Value::Value(0.38),
            },
        },
        hover: button::style::Hover{
            container: button::style::Container{
                height: Value::Value(40.0),
                elevation: Value::Value(3.0),
                shadow_color: Value::Ref(colors::SHADOW.to_owned()),
                color: Value::Ref(colors::SURFACE_CONTAINER_LOW.to_owned()),
                opacity: Value::Value(1.0),
            },
            label: button::style::Label {
                size: Value::Value(14.0),
                color: Value::Ref(colors::ON_SURFACE.to_owned()),
                opacity: Value::Value(1.0),
            },
            state_layer: button::style::StateLayer {
                color: Value::Ref(colors::PRIMARY.to_owned()),
                opacity: Value::Value(0.08),
            },
            icon: button::style::Icon {
                size: Value::Value(18.0),
                color: Value::Ref(colors::PRIMARY.to_owned()),
                opacity: Value::Value(1.0),
            },
        },
        focus: button::style::Focus {
            container: button::style::Container{
                height: Value::Value(40.0),
                elevation: Value::Value(3.0),
                shadow_color: Value::Ref(colors::SHADOW.to_owned()),
                color: Value::Ref(colors::SURFACE_CONTAINER_LOW.to_owned()),
                opacity: Value::Value(1.0),
            },
            label: button::style::Label {
                size: Value::Value(14.0),
                color: Value::Ref(colors::ON_SURFACE.to_owned()),
                opacity: Value::Value(1.0),
            },
            state_layer: button::style::StateLayer {
                color: Value::Ref(colors::PRIMARY.to_owned()),
                opacity: Value::Value(0.1),
            },
            icon: button::style::Icon {
                size: Value::Value(18.0),
                color: Value::Ref(colors::PRIMARY.to_owned()),
                opacity: Value::Value(1.0),
            },
            focus_indicator: button::style::FocusIndicator {
                color: Value::Ref(colors::SECONDARY.to_owned()),
                thickness: Value::Value(3.0),
                offset: Value::Value(2.0),
            },
        },
        press: button::style::Press {
            container: button::style::Container{
                height: Value::Value(40.0),
                elevation: Value::Value(1.0),
                shadow_color: Value::Ref(colors::SHADOW.to_owned()),
                color: Value::Ref(colors::SURFACE_CONTAINER_LOW.to_owned()),
                opacity: Value::Value(0.12),
            },
            label: button::style::Label {
                size: Value::Value(14.0),
                color: Value::Ref(colors::ON_SURFACE.to_owned()),
                opacity: Value::Value(0.38),
            },
            state_layer: button::style::StateLayer {
                color: Value::Ref(colors::PRIMARY.to_owned()),
                opacity: Value::Value(0.1),
            },
            icon: button::style::Icon {
                size: Value::Value(18.0),
                color: Value::Ref(colors::PRIMARY.to_owned()),
                opacity: Value::Value(0.38),
            },
        },
    };

    elevated_button.apply(&mut theme, "elevated_button");
    
    theme
}
