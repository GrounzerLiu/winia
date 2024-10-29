use material_color_utilities::dynamic_color::material_dynamic_colors;
use material_color_utilities::hct::Hct;
use material_color_utilities::scheme::DynamicScheme;
use skia_safe::Color;
use crate::app::{Style, Theme, ThemeColor};

fn argb_to_u32(a: u8, r: u8, g: u8, b: u8) -> u32 {
    (a as u32) << 24 | (r as u32) << 16 | (g as u32) << 8 | (b as u32)
}

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
pub fn material_theme(color: Color, is_dark: bool) -> Theme {
    let a = color.a();
    let r = color.r();
    let g = color.g();
    let b = color.b();
    let argb = argb_to_u32(a, r, g, b);
    let scheme_ops = material_color_utilities::scheme::scheme_tonal_spot(Hct::from_argb(argb), is_dark);
    let scheme = DynamicScheme::new(scheme_ops);
    let primary_color = material_dynamic_colors::primary().get_argb(&scheme);
    let on_primary_color = material_dynamic_colors::on_primary().get_argb(&scheme);
    let primary_container_color = material_dynamic_colors::primary_container().get_argb(&scheme);
    let on_primary_container_color = material_dynamic_colors::on_primary_container().get_argb(&scheme);
    let secondary_color = material_dynamic_colors::secondary().get_argb(&scheme);
    let on_secondary_color = material_dynamic_colors::on_secondary().get_argb(&scheme);
    let secondary_container_color = material_dynamic_colors::secondary_container().get_argb(&scheme);
    let on_secondary_container_color = material_dynamic_colors::on_secondary_container().get_argb(&scheme);
    let tertiary_color = material_dynamic_colors::tertiary().get_argb(&scheme);
    let on_tertiary_color = material_dynamic_colors::on_tertiary().get_argb(&scheme);
    let tertiary_container_color = material_dynamic_colors::tertiary_container().get_argb(&scheme);
    let on_tertiary_container_color = material_dynamic_colors::on_tertiary_container().get_argb(&scheme);
    let error_color = material_dynamic_colors::error().get_argb(&scheme);
    let on_error_color = material_dynamic_colors::on_error().get_argb(&scheme);
    let error_container_color = material_dynamic_colors::error_container().get_argb(&scheme);
    let on_error_container_color = material_dynamic_colors::on_error_container().get_argb(&scheme);
    let background_color = material_dynamic_colors::background().get_argb(&scheme);
    let on_background_color = material_dynamic_colors::on_background().get_argb(&scheme);
    let surface_color = material_dynamic_colors::surface().get_argb(&scheme);
    let on_surface_color = material_dynamic_colors::on_surface().get_argb(&scheme);
    let surface_variant_color = material_dynamic_colors::surface_variant().get_argb(&scheme);
    let on_surface_variant_color = material_dynamic_colors::on_surface_variant().get_argb(&scheme);
    let outline_color = material_dynamic_colors::outline().get_argb(&scheme);
    let outline_variant_color = material_dynamic_colors::outline_variant().get_argb(&scheme);
    let shadow_color = material_dynamic_colors::shadow().get_argb(&scheme);
    let scrim_color = material_dynamic_colors::scrim().get_argb(&scheme);
    let inverse_surface_color = material_dynamic_colors::inverse_surface().get_argb(&scheme);
    let inverse_on_surface_color = material_dynamic_colors::inverse_on_surface().get_argb(&scheme);
    let inverse_primary_color = material_dynamic_colors::inverse_primary().get_argb(&scheme);
    Theme::new(is_dark)
        .set_color(ThemeColor::Primary, primary_color.into())
        .set_color(ThemeColor::OnPrimary, on_primary_color.into())
        .set_color(ThemeColor::PrimaryContainer, primary_container_color.into())
        .set_color(ThemeColor::OnPrimaryContainer, on_primary_container_color.into())
        .set_color(ThemeColor::Secondary, secondary_color.into())
        .set_color(ThemeColor::OnSecondary, on_secondary_color.into())
        .set_color(ThemeColor::SecondaryContainer, secondary_container_color.into())
        .set_color(ThemeColor::OnSecondaryContainer, on_secondary_container_color.into())
        .set_color(ThemeColor::Tertiary, tertiary_color.into())
        .set_color(ThemeColor::OnTertiary, on_tertiary_color.into())
        .set_color(ThemeColor::TertiaryContainer, tertiary_container_color.into())
        .set_color(ThemeColor::OnTertiaryContainer, on_tertiary_container_color.into())
        .set_color(ThemeColor::Error, error_color.into())
        .set_color(ThemeColor::OnError, on_error_color.into())
        .set_color(ThemeColor::ErrorContainer, error_container_color.into())
        .set_color(ThemeColor::OnErrorContainer, on_error_container_color.into())
        .set_color(ThemeColor::Background, background_color.into())
        .set_color(ThemeColor::OnBackground, on_background_color.into())
        .set_color(ThemeColor::Surface, surface_color.into())
        .set_color(ThemeColor::OnSurface, on_surface_color.into())
        .set_color(ThemeColor::SurfaceVariant, surface_variant_color.into())
        .set_color(ThemeColor::OnSurfaceVariant, on_surface_variant_color.into())
        .set_color(ThemeColor::Outline, outline_color.into())
        .set_color(ThemeColor::OutlineVariant, outline_variant_color.into())
        .set_color(ThemeColor::Shadow, shadow_color.into())
        .set_color(ThemeColor::Scrim, scrim_color.into())
        .set_color(ThemeColor::InverseSurface, inverse_surface_color.into())
        .set_color(ThemeColor::InverseOnSurface, inverse_on_surface_color.into())
        .set_color(ThemeColor::InversePrimary, inverse_primary_color.into())
        
        .set_color(ThemeColor::WindowBackground, background_color.into())
}