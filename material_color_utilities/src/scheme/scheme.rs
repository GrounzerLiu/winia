use crate::palettes::CorePalette;
use crate::utils::Argb;

#[derive(Debug, Clone,Default)]
pub struct Scheme{
    pub primary: Argb,
    pub on_primary: Argb,
    pub primary_container: Argb,
    pub on_primary_container: Argb,
    pub secondary: Argb,
    pub on_secondary: Argb,
    pub secondary_container: Argb,
    pub on_secondary_container: Argb,
    pub tertiary: Argb,
    pub on_tertiary: Argb,
    pub tertiary_container: Argb,
    pub on_tertiary_container: Argb,
    pub error: Argb,
    pub on_error: Argb,
    pub error_container: Argb,
    pub on_error_container: Argb,
    pub background: Argb,
    pub on_background: Argb,
    pub surface: Argb,
    pub on_surface: Argb,
    pub surface_variant: Argb,
    pub on_surface_variant: Argb,
    pub outline: Argb,
    pub outline_variant: Argb,
    pub shadow: Argb,
    pub scrim: Argb,
    pub inverse_surface: Argb,
    pub inverse_on_surface: Argb,
    pub inverse_primary: Argb,
}

/**
 * Returns the light material color scheme based on the given core palette.
 */
pub fn material_light_color_scheme_from_palette(palette:CorePalette) -> Scheme {
    let mut scheme = Scheme::default();

    scheme.primary = palette.primary().get(40.0);
    scheme.on_primary = palette.primary().get(100.0);
    scheme.primary_container = palette.primary().get(90.0);
    scheme.on_primary_container = palette.primary().get(10.0);
    scheme.secondary = palette.secondary().get(40.0);
    scheme.on_secondary = palette.secondary().get(100.0);
    scheme.secondary_container = palette.secondary().get(90.0);
    scheme.on_secondary_container = palette.secondary().get(10.0);
    scheme.tertiary = palette.tertiary().get(40.0);
    scheme.on_tertiary = palette.tertiary().get(100.0);
    scheme.tertiary_container = palette.tertiary().get(90.0);
    scheme.on_tertiary_container = palette.tertiary().get(10.0);
    scheme.error = palette.error().get(40.0);
    scheme.on_error = palette.error().get(100.0);
    scheme.error_container = palette.error().get(90.0);
    scheme.on_error_container = palette.error().get(10.0);
    scheme.background = palette.neutral().get(99.0);
    scheme.on_background = palette.neutral().get(10.0);
    scheme.surface = palette.neutral().get(99.0);
    scheme.on_surface = palette.neutral().get(10.0);
    scheme.surface_variant = palette.neutral_variant().get(90.0);
    scheme.on_surface_variant = palette.neutral_variant().get(30.0);
    scheme.outline = palette.neutral_variant().get(50.0);
    scheme.outline_variant = palette.neutral_variant().get(80.0);
    scheme.shadow = palette.neutral().get(0.0);
    scheme.scrim = palette.neutral().get(0.0);
    scheme.inverse_surface = palette.neutral().get(20.0);
    scheme.inverse_on_surface = palette.neutral().get(95.0);
    scheme.inverse_primary = palette.primary().get(80.0);

    scheme
}

/**
 * Returns the dark material color scheme based on the given core palette.
 */
pub fn material_dark_color_scheme_from_palette(palette:CorePalette) -> Scheme {
    let mut scheme = Scheme::default();

    scheme.primary = palette.primary().get(80.0);
    scheme.on_primary = palette.primary().get(20.0);
    scheme.primary_container = palette.primary().get(30.0);
    scheme.on_primary_container = palette.primary().get(90.0);
    scheme.secondary = palette.secondary().get(80.0);
    scheme.on_secondary = palette.secondary().get(20.0);
    scheme.secondary_container = palette.secondary().get(30.0);
    scheme.on_secondary_container = palette.secondary().get(90.0);
    scheme.tertiary = palette.tertiary().get(80.0);
    scheme.on_tertiary = palette.tertiary().get(20.0);
    scheme.tertiary_container = palette.tertiary().get(30.0);
    scheme.on_tertiary_container = palette.tertiary().get(90.0);
    scheme.error = palette.error().get(80.0);
    scheme.on_error = palette.error().get(20.0);
    scheme.error_container = palette.error().get(30.0);
    scheme.on_error_container = palette.error().get(80.0);
    scheme.background = palette.neutral().get(10.0);
    scheme.on_background = palette.neutral().get(90.0);
    scheme.surface = palette.neutral().get(10.0);
    scheme.on_surface = palette.neutral().get(90.0);
    scheme.surface_variant = palette.neutral_variant().get(30.0);
    scheme.on_surface_variant = palette.neutral_variant().get(80.0);
    scheme.outline = palette.neutral_variant().get(60.0);
    scheme.outline_variant = palette.neutral_variant().get(30.0);
    scheme.shadow = palette.neutral().get(0.0);
    scheme.scrim = palette.neutral().get(0.0);
    scheme.inverse_surface = palette.neutral().get(90.0);
    scheme.inverse_on_surface = palette.neutral().get(20.0);
    scheme.inverse_primary = palette.primary().get(40.0);

    scheme
}

/**
 * Returns the light material color scheme based on the given color,
 * in ARGB format.
 */
pub fn material_light_color_scheme(color:Argb) -> Scheme{
    material_light_color_scheme_from_palette(CorePalette::from_argb(color))
}

/**
 * Returns the dark material color scheme based on the given color,
 * in ARGB format.
 */
pub fn material_dark_color_scheme(color:Argb) -> Scheme{
    material_dark_color_scheme_from_palette(CorePalette::from_argb(color))
}

/**
 * Returns the light material content color scheme based on the given color,
 * in ARGB format.
 */
pub fn material_light_content_color_scheme(color:Argb) -> Scheme{
    material_light_color_scheme_from_palette(CorePalette::content_from_argb(color))
}

/**
 * Returns the dark material content color scheme based on the given color,
 * in ARGB format.
 */
pub fn material_dark_content_color_scheme(color:Argb) -> Scheme{
    material_dark_color_scheme_from_palette(CorePalette::content_from_argb(color))
}