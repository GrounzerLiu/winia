use crate::ui::component::{button, divider};
use crate::ui::theme::Value::{Direct, Reference};
use crate::ui::theme::{color, elevation, styles, typescale, Access, Shape};
use crate::ui::theme::shape;
use crate::ui::Theme;
use material_colors::color::Argb;
use material_colors::theme::ThemeBuilder;
use skia_safe::Color;
use crate::ui::component::divider::style::add_divider_style;
use crate::ui::component::style::add_button_styles;
// use crate::ui::component::divider::style::add_divider_style;

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
    use std::time::Instant;
    let instant = Instant::now();
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
    theme
        .set_color(color::PRIMARY, primary)
        .set_color(color::ON_PRIMARY, on_primary)
        .set_color(color::PRIMARY_CONTAINER, primary_container)
        .set_color(color::ON_PRIMARY_CONTAINER, on_primary_container)
        .set_color(color::INVERSE_PRIMARY, inverse_primary)
        .set_color(color::PRIMARY_FIXED, primary_fixed)
        .set_color(color::PRIMARY_FIXED_DIM, primary_fixed_dim)
        .set_color(color::ON_PRIMARY_FIXED, on_primary_fixed)
        .set_color(color::ON_PRIMARY_FIXED_VARIANT, on_primary_fixed_variant)
        .set_color(color::SECONDARY, secondary)
        .set_color(color::ON_SECONDARY, on_secondary)
        .set_color(color::SECONDARY_CONTAINER, secondary_container)
        .set_color(color::ON_SECONDARY_CONTAINER, on_secondary_container)
        .set_color(color::SECONDARY_FIXED, secondary_fixed)
        .set_color(color::SECONDARY_FIXED_DIM, secondary_fixed_dim)
        .set_color(color::ON_SECONDARY_FIXED, on_secondary_fixed)
        .set_color(
            color::ON_SECONDARY_FIXED_VARIANT,
            on_secondary_fixed_variant,
        )
        .set_color(color::TERTIARY, tertiary)
        .set_color(color::ON_TERTIARY, on_tertiary)
        .set_color(color::TERTIARY_CONTAINER, tertiary_container)
        .set_color(color::ON_TERTIARY_CONTAINER, on_tertiary_container)
        .set_color(color::TERTIARY_FIXED, tertiary_fixed)
        .set_color(color::TERTIARY_FIXED_DIM, tertiary_fixed_dim)
        .set_color(color::ON_TERTIARY_FIXED, on_tertiary_fixed)
        .set_color(color::ON_TERTIARY_FIXED_VARIANT, on_tertiary_fixed_variant)
        .set_color(color::ERROR, error)
        .set_color(color::ON_ERROR, on_error)
        .set_color(color::ERROR_CONTAINER, error_container)
        .set_color(color::ON_ERROR_CONTAINER, on_error_container)
        .set_color(color::SURFACE_DIM, surface_dim)
        .set_color(color::SURFACE, surface)
        .set_color(color::SURFACE_TINT, surface_tint)
        .set_color(color::SURFACE_BRIGHT, surface_bright)
        .set_color(color::SURFACE_CONTAINER_LOWEST, surface_container_lowest)
        .set_color(color::SURFACE_CONTAINER_LOW, surface_container_low)
        .set_color(color::SURFACE_CONTAINER, surface_container)
        .set_color(color::SURFACE_CONTAINER_HIGH, surface_container_high)
        .set_color(color::SURFACE_CONTAINER_HIGHEST, surface_container_highest)
        .set_color(color::ON_SURFACE, on_surface)
        .set_color(color::ON_SURFACE_VARIANT, on_surface_variant)
        .set_color(color::OUTLINE, outline)
        .set_color(color::OUTLINE_VARIANT, outline_variant)
        .set_color(color::INVERSE_SURFACE, inverse_surface)
        .set_color(color::INVERSE_ON_SURFACE, inverse_on_surface)
        .set_color(color::SURFACE_VARIANT, surface_variant)
        .set_color(color::BACKGROUND, background)
        .set_color(color::ON_BACKGROUND, on_background)
        .set_color(color::SHADOW, shadow)
        .set_color(color::SCRIM, scrim)
        .set_color(color::WINDOW_BACKGROUND_COLOR, color::BACKGROUND);

    add_elevation(&mut theme);
    add_shape(&mut theme);
    add_typescale(&mut theme);



    add_button_styles(&mut theme);
    add_divider_style(&mut theme);

    theme
}

/*fn add_button_styles(theme: &mut Theme) {
    // Elevated Button
    let mut button = button::style::Button {
        enable: button::style::Enable {
            container: button::style::Container {
                shape: Shape::from_theme(&theme, shape::corner::FULL),
                height: Direct(40.0),
                elevation: Direct(1.0),
                shadow_color: Reference(color::SHADOW.to_owned()),
                color: Reference(color::SURFACE_CONTAINER_LOW.to_owned()),
                opacity: Direct(1.0),
            },
            outline: button::style::Outline {
                color: Reference(color::OUTLINE.to_owned()),
                width: Direct(0.0),
            },
            label: button::style::Label {
                size: Direct(14.0),
                color: Reference(color::ON_SURFACE.to_owned()),
                opacity: Direct(1.0),
            },
            icon: button::style::Icon {
                size: Direct(18.0),
                color: Reference(color::PRIMARY.to_owned()),
                opacity: Direct(1.0),
            },
        },
        disable: button::style::Disable {
            container: button::style::Container {
                shape: Shape::from_theme(&theme, shape::corner::FULL),
                height: Direct(40.0),
                elevation: Direct(0.0),
                shadow_color: Reference(color::SHADOW.to_owned()),
                color: Reference(color::ON_SURFACE.to_owned()),
                opacity: Direct(0.12),
            },
            outline: button::style::Outline {
                color: Reference(color::OUTLINE.to_owned()),
                width: Direct(0.0),
            },
            label: button::style::Label {
                size: Direct(14.0),
                color: Reference(color::ON_SURFACE.to_owned()),
                opacity: Direct(0.38),
            },
            icon: button::style::Icon {
                size: Direct(18.0),
                color: Reference(color::PRIMARY.to_owned()),
                opacity: Direct(0.38),
            },
        },
        hover: button::style::Hover {
            container: button::style::Container {
                shape: Shape::from_theme(&theme, shape::corner::FULL),
                height: Direct(40.0),
                elevation: Direct(3.0),
                shadow_color: Reference(color::SHADOW.to_owned()),
                color: Reference(color::SURFACE_CONTAINER_LOW.to_owned()),
                opacity: Direct(1.0),
            },
            outline: button::style::Outline {
                color: Reference(color::OUTLINE.to_owned()),
                width: Direct(0.0),
            },
            label: button::style::Label {
                size: Direct(14.0),
                color: Reference(color::ON_SURFACE.to_owned()),
                opacity: Direct(1.0),
            },
            state_layer: button::style::StateLayer {
                color: Reference(color::PRIMARY.to_owned()),
                opacity: Direct(0.08),
            },
            icon: button::style::Icon {
                size: Direct(18.0),
                color: Reference(color::PRIMARY.to_owned()),
                opacity: Direct(1.0),
            },
        },
        focus: button::style::Focus {
            container: button::style::Container {
                shape: Shape::from_theme(&theme, shape::corner::FULL),
                height: Direct(40.0),
                elevation: Direct(3.0),
                shadow_color: Reference(color::SHADOW.to_owned()),
                color: Reference(color::SURFACE_CONTAINER_LOW.to_owned()),
                opacity: Direct(1.0),
            },
            outline: button::style::Outline {
                color: Reference(color::OUTLINE.to_owned()),
                width: Direct(0.0),
            },
            label: button::style::Label {
                size: Direct(14.0),
                color: Reference(color::ON_SURFACE.to_owned()),
                opacity: Direct(1.0),
            },
            state_layer: button::style::StateLayer {
                color: Reference(color::PRIMARY.to_owned()),
                opacity: Direct(0.1),
            },
            icon: button::style::Icon {
                size: Direct(18.0),
                color: Reference(color::PRIMARY.to_owned()),
                opacity: Direct(1.0),
            },
            focus_indicator: button::style::FocusIndicator {
                color: Reference(color::SECONDARY.to_owned()),
                thickness: Direct(3.0),
                offset: Direct(2.0),
            },
        },
        press: button::style::Press {
            container: button::style::Container {
                shape: Shape::from_theme(&theme, shape::corner::FULL),
                height: Direct(40.0),
                elevation: Direct(1.0),
                shadow_color: Reference(color::SHADOW.to_owned()),
                color: Reference(color::SURFACE_CONTAINER_LOW.to_owned()),
                opacity: Direct(0.12),
            },
            outline: button::style::Outline {
                color: Reference(color::OUTLINE.to_owned()),
                width: Direct(0.0),
            },
            label: button::style::Label {
                size: Direct(14.0),
                color: Reference(color::ON_SURFACE.to_owned()),
                opacity: Direct(0.38),
            },
            state_layer: button::style::StateLayer {
                color: Reference(color::PRIMARY.to_owned()),
                opacity: Direct(0.1),
            },
            icon: button::style::Icon {
                size: Direct(18.0),
                color: Reference(color::PRIMARY.to_owned()),
                opacity: Direct(0.38),
            },
        },
    };

    button.apply(theme, styles::ELEVATED_BUTTON);

    // Filled Button
    button.enable.container
        .set_elevation(elevation::LEVEL_0)
        .set_color(color::PRIMARY);
    button.enable.label.set_color(color::ON_PRIMARY);
    button.enable.icon.set_color(color::ON_PRIMARY);

    button.disable.container
        .set_elevation(elevation::LEVEL_0)
        .set_color(color::PRIMARY);
    button.disable.label.set_color(color::ON_PRIMARY);
    button.disable.icon.set_color(color::ON_PRIMARY);

    button.hover.container
        .set_elevation(elevation::LEVEL_0)
        .set_color(color::PRIMARY);
    button.hover.label.set_color(color::ON_PRIMARY);
    button.hover.icon.set_color(color::ON_PRIMARY);
    button.hover.state_layer.set_color(color::ON_PRIMARY);

    button.focus.container
        .set_elevation(elevation::LEVEL_1)
        .set_color(color::PRIMARY);
    button.focus.label.set_color(color::ON_PRIMARY);
    button.focus.icon.set_color(color::ON_PRIMARY);
    button.focus.state_layer.set_color(color::ON_PRIMARY);

    button.press.container
        .set_elevation(elevation::LEVEL_0)
        .set_color(color::PRIMARY);
    button.press.label.set_color(color::ON_PRIMARY);
    button.press.icon.set_color(color::ON_PRIMARY);
    button.press.state_layer.set_color(color::ON_PRIMARY);

    button.apply(theme, styles::FILLED_BUTTON);

    // Filled Tonal Button
    button.enable.container.set_color(color::SECONDARY_CONTAINER);
    button.enable.label.set_color(color::ON_SECONDARY_CONTAINER);
    button.enable.icon.set_color(color::ON_SECONDARY_CONTAINER);


}*/

fn add_elevation(theme: &mut Theme) {
    theme.set_dimension(elevation::LEVEL_0, 0.0);
    theme.set_dimension(elevation::LEVEL_1, 1.0);
    theme.set_dimension(elevation::LEVEL_2, 3.0);
    theme.set_dimension(elevation::LEVEL_3, 6.0);
    theme.set_dimension(elevation::LEVEL_4, 8.0);
    theme.set_dimension(elevation::LEVEL_5, 12.0);
}

fn add_shape(theme: &mut Theme) {
    // Shape::new(0.0, 0.0, 0.0, 0.0)
    //     .apply(theme, shape::corner::NONE);
    // Shape::new(4.0, 4.0, 4.0, 4.0)
    //     .apply(theme, shape::corner::EXTRA_SMALL);
    // Shape::new(4.0, 4.0, 0.0, 0.0)
    //     .apply(theme, shape::corner::extra_small::TOP);
    // Shape::new(8.0, 8.0, 8.0, 8.0)
    //     .apply(theme, shape::corner::SMALL);
    // Shape::new(12.0, 12.0, 12.0, 12.0)
    //     .apply(theme, shape::corner::MEDIUM);
    // Shape::new(16.0, 16.0, 16.0, 16.0)
    //     .apply(theme, shape::corner::LARGE);
    // Shape::new(16.0, 16.0, 0.0, 0.0)
    //     .apply(theme, shape::corner::large::TOP);
    // Shape::new(16.0, 0.0, 0.0, 16.0)
    //     .apply(theme, shape::corner::large::START);
    // Shape::new(0.0, 16.0, 16.0, 0.0)
    //     .apply(theme, shape::corner::large::END);
    // Shape::new(28.0, 28.0, 28.0, 28.0)
    //     .apply(theme, shape::corner::EXTRA_LARGE);
    // Shape::new(28.0, 28.0, 0.0, 0.0)
    //     .apply(theme, shape::corner::extra_large::TOP);
    // Shape::new(f32::MAX, f32::MAX, f32::MAX, f32::MAX)
    //     .apply(theme, shape::corner::FULL);

    fn add_a_shape(theme: &mut Theme, corner: &str, top_start: f32, top_end: f32, bottom_start: f32, bottom_end: f32) {
        let mut shape = Shape::new(theme, corner);
        shape.top_start().set(top_start);
        shape.top_end().set(top_end);
        shape.bottom_start().set(bottom_start);
        shape.bottom_end().set(bottom_end);
    }
    
    add_a_shape(theme, shape::corner::NONE, 0.0, 0.0, 0.0, 0.0);
    add_a_shape(theme, shape::corner::EXTRA_SMALL, 4.0, 4.0, 4.0, 4.0);
    add_a_shape(theme, shape::corner::extra_small::TOP, 4.0, 4.0, 0.0, 0.0);
    add_a_shape(theme, shape::corner::SMALL, 8.0, 8.0, 8.0, 8.0);
    add_a_shape(theme, shape::corner::MEDIUM, 12.0, 12.0, 12.0, 12.0);
    add_a_shape(theme, shape::corner::LARGE, 16.0, 16.0, 16.0, 16.0);
    add_a_shape(theme, shape::corner::large::TOP, 16.0, 16.0, 0.0, 0.0);
    add_a_shape(theme, shape::corner::large::START, 16.0, 0.0, 0.0, 16.0);
    add_a_shape(theme, shape::corner::large::END, 0.0, 16.0, 16.0, 0.0);
    add_a_shape(theme, shape::corner::EXTRA_LARGE, 28.0, 28.0, 28.0, 28.0);
    add_a_shape(theme, shape::corner::extra_large::TOP, 28.0, 28.0, 0.0, 0.0);
    add_a_shape(theme, shape::corner::FULL, f32::MAX, f32::MAX, f32::MAX, f32::MAX);
}

fn add_typescale(theme: &mut Theme) {
    theme.set_dimension(typescale::weight::REGULAR, 400.0);
    theme.set_dimension(typescale::weight::MEDIUM, 500.0);
    theme.set_dimension(typescale::weight::BOLD, 700.0);
    
    theme.set_string(typescale::PLAIN, "".to_string());
    theme.set_string(typescale::BRAND, "".to_string());
    
    theme.set_string(typescale::display_large::FONT, typescale::BRAND);
    theme.set_dimension(typescale::display_large::WEIGHT, typescale::weight::REGULAR);
    theme.set_dimension(typescale::display_large::SIZE, 57.0);
    theme.set_dimension(typescale::display_large::TRACKING, -0.25);
    theme.set_dimension(typescale::display_large::LINE_HEIGHT, 64.0);

    theme.set_string(typescale::display_medium::FONT, typescale::BRAND);
    theme.set_dimension(typescale::display_medium::WEIGHT, typescale::weight::REGULAR);
    theme.set_dimension(typescale::display_medium::SIZE, 45.0);
    theme.set_dimension(typescale::display_medium::TRACKING, 0.0);
    theme.set_dimension(typescale::display_medium::LINE_HEIGHT, 52.0);

    theme.set_string(typescale::display_small::FONT, typescale::BRAND);
    theme.set_dimension(typescale::display_small::WEIGHT, typescale::weight::REGULAR);
    theme.set_dimension(typescale::display_small::SIZE, 36.0);
    theme.set_dimension(typescale::display_small::TRACKING, 0.0);
    theme.set_dimension(typescale::display_small::LINE_HEIGHT, 44.0);

    theme.set_string(typescale::headline_large::FONT, typescale::BRAND);
    theme.set_dimension(typescale::headline_large::WEIGHT, typescale::weight::REGULAR);
    theme.set_dimension(typescale::headline_large::SIZE, 32.0);
    theme.set_dimension(typescale::headline_large::TRACKING, 0.0);
    theme.set_dimension(typescale::headline_large::LINE_HEIGHT, 40.0);

    theme.set_string(typescale::headline_medium::FONT, typescale::BRAND);
    theme.set_dimension(typescale::headline_medium::WEIGHT, typescale::weight::REGULAR);
    theme.set_dimension(typescale::headline_medium::SIZE, 28.0);
    theme.set_dimension(typescale::headline_medium::TRACKING, 0.0);
    theme.set_dimension(typescale::headline_medium::LINE_HEIGHT, 36.0);

    theme.set_string(typescale::headline_small::FONT, typescale::BRAND);
    theme.set_dimension(typescale::headline_small::WEIGHT, typescale::weight::REGULAR);
    theme.set_dimension(typescale::headline_small::SIZE, 24.0);
    theme.set_dimension(typescale::headline_small::TRACKING, 0.0);
    theme.set_dimension(typescale::headline_small::LINE_HEIGHT, 32.0);

    theme.set_string(typescale::title_large::FONT, typescale::BRAND);
    theme.set_dimension(typescale::title_large::WEIGHT, typescale::weight::REGULAR);
    theme.set_dimension(typescale::title_large::SIZE, 22.0);
    theme.set_dimension(typescale::title_large::TRACKING, 0.0);
    theme.set_dimension(typescale::title_large::LINE_HEIGHT, 28.0);

    theme.set_string(typescale::title_medium::FONT, typescale::PLAIN);
    theme.set_dimension(typescale::title_medium::WEIGHT, typescale::weight::MEDIUM);
    theme.set_dimension(typescale::title_medium::SIZE, 16.0);
    theme.set_dimension(typescale::title_medium::TRACKING, 0.15);
    theme.set_dimension(typescale::title_medium::LINE_HEIGHT, 24.0);

    theme.set_string(typescale::title_small::FONT, typescale::PLAIN);
    theme.set_dimension(typescale::title_small::WEIGHT, typescale::weight::MEDIUM);
    theme.set_dimension(typescale::title_small::SIZE, 14.0);
    theme.set_dimension(typescale::title_small::TRACKING, 0.1);
    theme.set_dimension(typescale::title_small::LINE_HEIGHT, 20.0);

    theme.set_string(typescale::body_large::FONT, typescale::PLAIN);
    theme.set_dimension(typescale::body_large::WEIGHT, typescale::weight::REGULAR);
    theme.set_dimension(typescale::body_large::SIZE, 16.0);
    theme.set_dimension(typescale::body_large::TRACKING, 0.5);
    theme.set_dimension(typescale::body_large::LINE_HEIGHT, 24.0);

    theme.set_string(typescale::body_medium::FONT, typescale::PLAIN);
    theme.set_dimension(typescale::body_medium::WEIGHT, typescale::weight::REGULAR);
    theme.set_dimension(typescale::body_medium::SIZE, 14.0);
    theme.set_dimension(typescale::body_medium::TRACKING, 0.25);
    theme.set_dimension(typescale::body_medium::LINE_HEIGHT, 20.0);

    theme.set_string(typescale::body_small::FONT, typescale::PLAIN);
    theme.set_dimension(typescale::body_small::WEIGHT, typescale::weight::REGULAR);
    theme.set_dimension(typescale::body_small::SIZE, 12.0);
    theme.set_dimension(typescale::body_small::TRACKING, 0.4);
    theme.set_dimension(typescale::body_small::LINE_HEIGHT, 16.0);

    theme.set_string(typescale::label_large::FONT, typescale::PLAIN);
    theme.set_dimension(typescale::label_large::WEIGHT, typescale::weight::MEDIUM);
    theme.set_dimension(typescale::label_large::WEIGHT_PROMINENT, typescale::weight::BOLD);
    theme.set_dimension(typescale::label_large::SIZE, 14.0);
    theme.set_dimension(typescale::label_large::TRACKING, 0.1);
    theme.set_dimension(typescale::label_large::LINE_HEIGHT, 20.0);

    theme.set_string(typescale::label_medium::FONT, typescale::PLAIN);
    theme.set_dimension(typescale::label_medium::WEIGHT, typescale::weight::MEDIUM);
    theme.set_dimension(typescale::label_medium::WEIGHT_PROMINENT, typescale::weight::BOLD);
    theme.set_dimension(typescale::label_medium::SIZE, 12.0);
    theme.set_dimension(typescale::label_medium::TRACKING, 0.5);
    theme.set_dimension(typescale::label_medium::LINE_HEIGHT, 16.0);

    theme.set_string(typescale::label_small::FONT, typescale::PLAIN);
    theme.set_dimension(typescale::label_small::WEIGHT, typescale::weight::MEDIUM);
    theme.set_dimension(typescale::label_small::SIZE, 11.0);
    theme.set_dimension(typescale::label_small::TRACKING, 0.5);
    theme.set_dimension(typescale::label_small::LINE_HEIGHT, 16.0);
}


