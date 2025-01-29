/*pub enum ThemeColor{
    Primary,
    OnPrimary,
    PrimaryContainer,
    OnPrimaryContainer,
    Secondary,
    OnSecondary,
    SecondaryContainer,
    OnSecondaryContainer,
    Tertiary,
    OnTertiary,
    TertiaryContainer,
    OnTertiaryContainer,
    Error,
    OnError,
    ErrorContainer,
    OnErrorContainer,
    Background,
    OnBackground,
    Surface,
    OnSurface,
    SurfaceVariant,
    OnSurfaceVariant,
    Outline,
    OutlineVariant,
    Shadow,
    Scrim,
    InverseSurface,
    InverseOnSurface,
    InversePrimary,
}*/
use skia_safe::Color;

pub static PRIMARY: &str = "primary";
pub static ON_PRIMARY: &str = "on_primary";
pub static PRIMARY_CONTAINER: &str = "primary_container";
pub static ON_PRIMARY_CONTAINER: &str = "on_primary_container";
pub static SECONDARY: &str = "secondary";
pub static ON_SECONDARY: &str = "on_secondary";
pub static SECONDARY_CONTAINER: &str = "secondary_container";
pub static ON_SECONDARY_CONTAINER: &str = "on_secondary_container";
pub static TERTIARY: &str = "tertiary";
pub static ON_TERTIARY: &str = "on_tertiary";
pub static TERTIARY_CONTAINER: &str = "tertiary_container";
pub static ON_TERTIARY_CONTAINER: &str = "on_tertiary_container";
pub static ERROR: &str = "error";
pub static ON_ERROR: &str = "on_error";
pub static ERROR_CONTAINER: &str = "error_container";
pub static ON_ERROR_CONTAINER: &str = "on_error_container";
pub static BACKGROUND: &str = "background";
pub static ON_BACKGROUND: &str = "on_background";
pub static SURFACE: &str = "surface";
pub static ON_SURFACE: &str = "on_surface";
pub static SURFACE_VARIANT: &str = "surface_variant";
pub static ON_SURFACE_VARIANT: &str = "on_surface_variant";
pub static OUTLINE: &str = "outline";
pub static OUTLINE_VARIANT: &str = "outline_variant";
pub static SHADOW: &str = "shadow";
pub static SCRIM: &str = "scrim";
pub static INVERSE_SURFACE: &str = "inverse_surface";
pub static INVERSE_ON_SURFACE: &str = "inverse_on_surface";
pub static INVERSE_PRIMARY: &str = "inverse_primary";

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
