use crate::ui::theme::shape;
use crate::ui::theme::shape::Corner;
use crate::ui::theme::typescale::TypeScale;
use crate::ui::theme::{color, elevation, typescale};
use crate::ui::Theme;
use material_colors::color::Argb;
use material_colors::theme::ThemeBuilder;
use skia_safe::Color;
use crate::ui::component::divider::style::divider_style;
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
    add_shape_corner(&mut theme);
    add_typescale(&mut theme);

    divider_style(&mut theme);
    
    theme
}

fn add_elevation(theme: &mut Theme) {
    theme.set_dimension(elevation::LEVEL_0, 0.0);
    theme.set_dimension(elevation::LEVEL_1, 1.0);
    theme.set_dimension(elevation::LEVEL_2, 3.0);
    theme.set_dimension(elevation::LEVEL_3, 6.0);
    theme.set_dimension(elevation::LEVEL_4, 8.0);
    theme.set_dimension(elevation::LEVEL_5, 12.0);
}

fn add_shape_corner(theme: &mut Theme) {
    fn add_a_shape(
        theme: &mut Theme,
        corner: &str,
        top_start: f32,
        top_end: f32,
        bottom_start: f32,
        bottom_end: f32,
    ) {
        theme.set_style(
            corner,
            Box::new(Corner {
                top_start,
                top_end,
                bottom_start,
                bottom_end,
            }),
        );
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
    add_a_shape(
        theme,
        shape::corner::FULL,
        f32::MAX,
        f32::MAX,
        f32::MAX,
        f32::MAX,
    );
}

fn add_typescale(theme: &mut Theme) {
    fn add_a_type_scale(theme: &mut Theme, name: &str, type_scale: TypeScale) {
        theme.set_style(name, Box::new(type_scale));
    }

    add_a_type_scale(
        theme,
        typescale::DISPLAY_LARGE,
        TypeScale::new("Roboto", 400.0, 57.0, -0.25, 64.0),
    );
    add_a_type_scale(
        theme,
        typescale::DISPLAY_MEDIUM,
        TypeScale::new("Roboto", 400.0, 45.0, 0.0, 52.0),
    );
    add_a_type_scale(
        theme,
        typescale::DISPLAY_SMALL,
        TypeScale::new("Roboto", 400.0, 36.0, 0.0, 44.0),
    );
    add_a_type_scale(
        theme,
        typescale::HEADLINE_LARGE,
        TypeScale::new("Roboto", 400.0, 32.0, 0.0, 40.0),
    );
    add_a_type_scale(
        theme,
        typescale::HEADLINE_MEDIUM,
        TypeScale::new("Roboto", 400.0, 28.0, 0.0, 36.0),
    );
    add_a_type_scale(
        theme,
        typescale::HEADLINE_SMALL,
        TypeScale::new("Roboto", 400.0, 24.0, 0.0, 32.0),
    );
    add_a_type_scale(
        theme,
        typescale::TITLE_LARGE,
        TypeScale::new("Roboto", 400.0, 22.0, 0.0, 28.0),
    );
    add_a_type_scale(
        theme,
        typescale::TITLE_MEDIUM,
        TypeScale::new("Roboto", 500.0, 16.0, 0.15, 24.0),
    );
    add_a_type_scale(
        theme,
        typescale::TITLE_SMALL,
        TypeScale::new("Roboto", 500.0, 14.0, 0.1, 20.0),
    );
    add_a_type_scale(
        theme,
        typescale::BODY_LARGE,
        TypeScale::new("Roboto", 400.0, 16.0, 0.5, 24.0),
    );
    add_a_type_scale(
        theme,
        typescale::BODY_MEDIUM,
        TypeScale::new("Roboto", 400.0, 14.0, 0.25, 20.0),
    );
    add_a_type_scale(
        theme,
        typescale::BODY_SMALL,
        TypeScale::new("Roboto", 400.0, 12.0, 0.4, 16.0),
    );
    add_a_type_scale(
        theme,
        typescale::LABEL_LARGE,
        TypeScale::new("Roboto", 500.0, 14.0, 0.1, 20.0),
    );
    add_a_type_scale(
        theme,
        typescale::LABEL_MEDIUM,
        TypeScale::new("Roboto", 500.0, 12.0, 0.5, 16.0),
    );
    add_a_type_scale(
        theme,
        typescale::LABEL_SMALL,
        TypeScale::new("Roboto", 500.0, 11.0, 0.5, 16.0),
    );
}
