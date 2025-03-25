/*
pub struct Scheme {

    pub primary: Argb,
    pub on_primary: Argb,
    pub primary_container: Argb,
    pub on_primary_container: Argb,
    pub inverse_primary: Argb,
    pub primary_fixed: Argb,
    pub primary_fixed_dim: Argb,
    pub on_primary_fixed: Argb,
    pub on_primary_fixed_variant: Argb,
    pub secondary: Argb,
    pub on_secondary: Argb,
    pub secondary_container: Argb,
    pub on_secondary_container: Argb,
    pub secondary_fixed: Argb,
    pub secondary_fixed_dim: Argb,
    pub on_secondary_fixed: Argb,
    pub on_secondary_fixed_variant: Argb,
    pub tertiary: Argb,
    pub on_tertiary: Argb,
    pub tertiary_container: Argb,
    pub on_tertiary_container: Argb,
    pub tertiary_fixed: Argb,
    pub tertiary_fixed_dim: Argb,
    pub on_tertiary_fixed: Argb,
    pub on_tertiary_fixed_variant: Argb,
    pub error: Argb,
    pub on_error: Argb,
    pub error_container: Argb,
    pub on_error_container: Argb,
    pub surface_dim: Argb,
    pub surface: Argb,
    pub surface_tint: Argb,
    pub surface_bright: Argb,
    pub surface_container_lowest: Argb,
    pub surface_container_low: Argb,
    pub surface_container: Argb,
    pub surface_container_high: Argb,
    pub surface_container_highest: Argb,
    pub on_surface: Argb,
    pub on_surface_variant: Argb,
    pub outline: Argb,
    pub outline_variant: Argb,
    pub inverse_surface: Argb,
    pub inverse_on_surface: Argb,
    pub surface_variant: Argb,
    pub background: Argb,
    pub on_background: Argb,
    pub shadow: Argb,
    pub scrim: Argb,
}
*/
use skia_safe::Color;

pub static PRIMARY: &str = "primary";
pub static ON_PRIMARY: &str = "on_primary";
pub static PRIMARY_CONTAINER: &str = "primary_container";
pub static ON_PRIMARY_CONTAINER: &str = "on_primary_container";
pub static INVERSE_PRIMARY: &str = "inverse_primary";
pub static PRIMARY_FIXED: &str = "primary_fixed";
pub static PRIMARY_FIXED_DIM: &str = "primary_fixed_dim";
pub static ON_PRIMARY_FIXED: &str = "on_primary_fixed";
pub static ON_PRIMARY_FIXED_VARIANT: &str = "on_primary_fixed_variant";
pub static SECONDARY: &str = "secondary";
pub static ON_SECONDARY: &str = "on_secondary";
pub static SECONDARY_CONTAINER: &str = "secondary_container";
pub static ON_SECONDARY_CONTAINER: &str = "on_secondary_container";
pub static SECONDARY_FIXED: &str = "secondary_fixed";
pub static SECONDARY_FIXED_DIM: &str = "secondary_fixed_dim";
pub static ON_SECONDARY_FIXED: &str = "on_secondary_fixed";
pub static ON_SECONDARY_FIXED_VARIANT: &str = "on_secondary_fixed_variant";
pub static TERTIARY: &str = "tertiary";
pub static ON_TERTIARY: &str = "on_tertiary";
pub static TERTIARY_CONTAINER: &str = "tertiary_container";
pub static ON_TERTIARY_CONTAINER: &str = "on_tertiary_container";
pub static TERTIARY_FIXED: &str = "tertiary_fixed";
pub static TERTIARY_FIXED_DIM: &str = "tertiary_fixed_dim";
pub static ON_TERTIARY_FIXED: &str = "on_tertiary_fixed";
pub static ON_TERTIARY_FIXED_VARIANT: &str = "on_tertiary_fixed_variant";
pub static ERROR: &str = "error";
pub static ON_ERROR: &str = "on_error";
pub static ERROR_CONTAINER: &str = "error_container";
pub static ON_ERROR_CONTAINER: &str = "on_error_container";
pub static SURFACE_DIM: &str = "surface_dim";
pub static SURFACE: &str = "surface";
pub static SURFACE_TINT: &str = "surface_tint";
pub static SURFACE_BRIGHT: &str = "surface_bright";
pub static SURFACE_CONTAINER_LOWEST: &str = "surface_container_lowest";
pub static SURFACE_CONTAINER_LOW: &str = "surface_container_low";
pub static SURFACE_CONTAINER: &str = "surface_container";
pub static SURFACE_CONTAINER_HIGH: &str = "surface_container_high";
pub static SURFACE_CONTAINER_HIGHEST: &str = "surface_container_highest";
pub static ON_SURFACE: &str = "on_surface";
pub static ON_SURFACE_VARIANT: &str = "on_surface_variant";
pub static OUTLINE: &str = "outline";
pub static OUTLINE_VARIANT: &str = "outline_variant";
pub static INVERSE_SURFACE: &str = "inverse_surface";
pub static INVERSE_ON_SURFACE: &str = "inverse_on_surface";
pub static SURFACE_VARIANT: &str = "surface_variant";
pub static BACKGROUND: &str = "background";
pub static ON_BACKGROUND: &str = "on_background";
pub static SHADOW: &str = "shadow";
pub static SCRIM: &str = "scrim";




pub static WINDOW_BACKGROUND_COLOR: &str = "window_background_color";

/// Parses a color from a string.
/// Formats supported:
/// - #RRGGBB
/// - #AARRGGBB
/// - 0xRRGGBB
/// - 0xAARRGGBB
pub fn parse_color(color: &str) -> Option<Color> {
    let color = color.trim();
    if color.starts_with("#") || color.starts_with("0x") {
        let color = if color.starts_with("#") {
            &color[1..]
        } else {
            &color[2..]
        };
        let u32_color = if let Ok(v) = u32::from_str_radix(color, 16) {
            v
        } else {
            return None;
        };
        let len = color.len();
        let color = Color::from(u32_color);
        return if len == 6 {
            Some(color.with_a(0xFF))
        } else if len == 8 {
            Some(color)
        } else {
            None
        };
    }
    None
}
