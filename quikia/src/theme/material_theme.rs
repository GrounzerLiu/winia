use material_color_utilities::dynamic_color::material_dynamic_colors;
use material_color_utilities::hct::Hct;
use material_color_utilities::scheme::DynamicScheme;
use skia_safe::Color;
use crate::app::{Theme, ThemeColor};

fn argb_to_u32(a:u8, r:u8, g:u8, b:u8) -> u32{
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
    let a=color.a();
    let r=color.r();
    let g=color.g();
    let b=color.b();
    let argb=argb_to_u32(a, r, g, b);
    let scheme_ops = material_color_utilities::scheme::scheme_tonal_spot(Hct::from_argb(argb), is_dark);
    let scheme = DynamicScheme::new(scheme_ops);
    Theme::new(is_dark)
        .set_color(ThemeColor::Primary, material_dynamic_colors::primary().get_argb(&scheme).into())
        .set_color(ThemeColor::OnPrimary, material_dynamic_colors::on_primary().get_argb(&scheme).into())
        .set_color(ThemeColor::PrimaryContainer, material_dynamic_colors::primary_container().get_argb(&scheme).into())
        .set_color(ThemeColor::OnPrimaryContainer, material_dynamic_colors::on_primary_container().get_argb(&scheme).into())
        .set_color(ThemeColor::Secondary, material_dynamic_colors::secondary().get_argb(&scheme).into())
        .set_color(ThemeColor::OnSecondary, material_dynamic_colors::on_secondary().get_argb(&scheme).into())
        .set_color(ThemeColor::SecondaryContainer, material_dynamic_colors::secondary_container().get_argb(&scheme).into())
        .set_color(ThemeColor::OnSecondaryContainer, material_dynamic_colors::on_secondary_container().get_argb(&scheme).into())
        .set_color(ThemeColor::Tertiary, material_dynamic_colors::tertiary().get_argb(&scheme).into())
        .set_color(ThemeColor::OnTertiary, material_dynamic_colors::on_tertiary().get_argb(&scheme).into())
        .set_color(ThemeColor::TertiaryContainer, material_dynamic_colors::tertiary_container().get_argb(&scheme).into())
        .set_color(ThemeColor::OnTertiaryContainer, material_dynamic_colors::on_tertiary_container().get_argb(&scheme).into())
        .set_color(ThemeColor::Error, material_dynamic_colors::error().get_argb(&scheme).into())
        .set_color(ThemeColor::OnError, material_dynamic_colors::on_error().get_argb(&scheme).into())
        .set_color(ThemeColor::ErrorContainer, material_dynamic_colors::error_container().get_argb(&scheme).into())
        .set_color(ThemeColor::OnErrorContainer, material_dynamic_colors::on_error_container().get_argb(&scheme).into())
        .set_color(ThemeColor::Background, material_dynamic_colors::background().get_argb(&scheme).into())
        .set_color(ThemeColor::OnBackground, material_dynamic_colors::on_background().get_argb(&scheme).into())
        .set_color(ThemeColor::Surface, material_dynamic_colors::surface().get_argb(&scheme).into())
        .set_color(ThemeColor::OnSurface, material_dynamic_colors::on_surface().get_argb(&scheme).into())
        .set_color(ThemeColor::SurfaceVariant, material_dynamic_colors::surface_variant().get_argb(&scheme).into())
        .set_color(ThemeColor::OnSurfaceVariant, material_dynamic_colors::on_surface_variant().get_argb(&scheme).into())
        .set_color(ThemeColor::Outline, material_dynamic_colors::outline().get_argb(&scheme).into())
        .set_color(ThemeColor::OutlineVariant, material_dynamic_colors::outline_variant().get_argb(&scheme).into())
        .set_color(ThemeColor::Shadow, material_dynamic_colors::shadow().get_argb(&scheme).into())
        .set_color(ThemeColor::Scrim, material_dynamic_colors::scrim().get_argb(&scheme).into())
        .set_color(ThemeColor::InverseSurface, material_dynamic_colors::inverse_surface().get_argb(&scheme).into())
        .set_color(ThemeColor::InverseOnSurface, material_dynamic_colors::inverse_on_surface().get_argb(&scheme).into())
        .set_color(ThemeColor::InversePrimary, material_dynamic_colors::inverse_primary().get_argb(&scheme).into())
}