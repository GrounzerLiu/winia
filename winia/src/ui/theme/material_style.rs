use std::collections::HashMap;
use skia_safe::Color;
use material_color_utilities::dynamic_color::material_dynamic_colors;
use material_color_utilities::hct::Hct;
use crate::ui::component::text_style;
use crate::ui::theme::{colors, styles, Style, WINDOW_BACKGROUND_COLOR};

fn argb_to_u32(a: u8, r: u8, g: u8, b: u8) -> u32 {
    (a as u32) << 24 | (r as u32) << 16 | (g as u32) << 8 | (b as u32)
}
pub fn material_style(color: Color, is_dark: bool) -> Style {
    let a = color.a();
    let r = color.r();
    let g = color.g();
    let b = color.b();
    let argb = argb_to_u32(a, r, g, b);
    let scheme =
        material_color_utilities::scheme::scheme_tonal_spot(Hct::from_argb(argb), is_dark);
    let primary_color:Color = material_dynamic_colors::primary().get_argb(&scheme).into();
    let on_primary_color:Color = material_dynamic_colors::on_primary().get_argb(&scheme).into();
    let primary_container_color:Color =
        material_dynamic_colors::primary_container().get_argb(&scheme).into();
    let on_primary_container_color:Color =
        material_dynamic_colors::on_primary_container().get_argb(&scheme).into();
    let secondary_color:Color = material_dynamic_colors::secondary().get_argb(&scheme).into();
    let on_secondary_color:Color = material_dynamic_colors::on_secondary().get_argb(&scheme).into();
    let secondary_container_color:Color =
        material_dynamic_colors::secondary_container().get_argb(&scheme).into();
    let on_secondary_container_color:Color =
        material_dynamic_colors::on_secondary_container().get_argb(&scheme).into();
    let tertiary_color:Color = material_dynamic_colors::tertiary().get_argb(&scheme).into();
    let on_tertiary_color:Color = material_dynamic_colors::on_tertiary().get_argb(&scheme).into();
    let tertiary_container_color:Color =
        material_dynamic_colors::tertiary_container().get_argb(&scheme).into();
    let on_tertiary_container_color:Color =
        material_dynamic_colors::on_tertiary_container().get_argb(&scheme).into();
    let error_color:Color = material_dynamic_colors::error().get_argb(&scheme).into();
    let on_error_color:Color = material_dynamic_colors::on_error().get_argb(&scheme).into();
    let error_container_color:Color = material_dynamic_colors::error_container().get_argb(&scheme).into();
    let on_error_container_color:Color =
        material_dynamic_colors::on_error_container().get_argb(&scheme).into();
    let background_color:Color = material_dynamic_colors::background().get_argb(&scheme).into();
    let on_background_color:Color = material_dynamic_colors::on_background().get_argb(&scheme).into();
    let surface_color:Color = material_dynamic_colors::surface().get_argb(&scheme).into();
    let on_surface_color:Color = material_dynamic_colors::on_surface().get_argb(&scheme).into();
    let surface_variant_color:Color = material_dynamic_colors::surface_variant().get_argb(&scheme).into();
    let on_surface_variant_color:Color =
        material_dynamic_colors::on_surface_variant().get_argb(&scheme).into();
    let outline_color:Color = material_dynamic_colors::outline().get_argb(&scheme).into();
    let outline_variant_color:Color = material_dynamic_colors::outline_variant().get_argb(&scheme).into();
    let shadow_color:Color = material_dynamic_colors::shadow().get_argb(&scheme).into();
    let scrim_color:Color = material_dynamic_colors::scrim().get_argb(&scheme).into();
    let inverse_surface_color:Color = material_dynamic_colors::inverse_surface().get_argb(&scheme).into();
    let inverse_on_surface_color:Color =
        material_dynamic_colors::inverse_on_surface().get_argb(&scheme).into();
    let inverse_primary_color:Color = material_dynamic_colors::inverse_primary().get_argb(&scheme).into();
    Style::new()
        .set_bool("is_dark", is_dark)
        .set_color(colors::PRIMARY, primary_color)
        .set_color(colors::ON_PRIMARY, on_primary_color)
        .set_color(colors::PRIMARY_CONTAINER, primary_container_color)
        .set_color(
            colors::ON_PRIMARY_CONTAINER,
            on_primary_container_color,
        )
        .set_color(colors::SECONDARY, secondary_color)
        .set_color(colors::ON_SECONDARY, on_secondary_color)
        .set_color(
            colors::SECONDARY_CONTAINER,
            secondary_container_color,
        )
        .set_color(
            colors::ON_SECONDARY_CONTAINER,
            on_secondary_container_color,
        )
        .set_color(colors::TERTIARY, tertiary_color)
        .set_color(colors::ON_TERTIARY, on_tertiary_color)
        .set_color(colors::TERTIARY_CONTAINER, tertiary_container_color)
        .set_color(
            colors::ON_TERTIARY_CONTAINER,
            on_tertiary_container_color,
        )
        .set_color(colors::ERROR, error_color)
        .set_color(colors::ON_ERROR, on_error_color)
        .set_color(colors::ERROR_CONTAINER, error_container_color)
        .set_color(colors::ON_ERROR_CONTAINER, on_error_container_color)
        .set_color(colors::BACKGROUND, background_color)
        .set_color(colors::ON_BACKGROUND, on_background_color)
        .set_color(colors::SURFACE, surface_color)
        .set_color(colors::ON_SURFACE, on_surface_color)
        .set_color(colors::SURFACE_VARIANT, surface_variant_color)
        .set_color(colors::ON_SURFACE_VARIANT, on_surface_variant_color)
        .set_color(colors::OUTLINE, outline_color)
        .set_color(colors::OUTLINE_VARIANT, outline_variant_color)
        .set_color(colors::SHADOW, shadow_color)
        .set_color(colors::SCRIM, scrim_color)
        .set_color(colors::INVERSE_SURFACE, inverse_surface_color)
        .set_color(colors::INVERSE_ON_SURFACE, inverse_on_surface_color)
        .set_color(colors::INVERSE_PRIMARY, inverse_primary_color)
        .set_color(WINDOW_BACKGROUND_COLOR, background_color)
        .set_style(
            styles::TEXT,
            Style::new()
                .set_color(text_style::COLOR, colors::ON_SURFACE_VARIANT)
                .set_dimension(text_style::FONT_SIZE, 16.0),
        )
}