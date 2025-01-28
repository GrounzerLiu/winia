use crate::ui::theme::colors;
use crate::ui::Item;
use material_color_utilities::dynamic_color::{material_dynamic_colors, DynamicScheme};
use material_color_utilities::hct::Hct;
use skia_safe::Color;
use std::collections::HashMap;

pub static WINDOW_BACKGROUND_COLOR: &str = "window_background_color";

fn argb_to_u32(a: u8, r: u8, g: u8, b: u8) -> u32 {
    (a as u32) << 24 | (r as u32) << 16 | (g as u32) << 8 | (b as u32)
}

pub struct Style {
    is_dark: bool,
    colors: HashMap<String, Color>,
    dimensions: HashMap<String, f32>,
    bools: HashMap<String, bool>,
    items: HashMap<String, Box<dyn Fn() -> Item>>,
    styles: HashMap<String, Style>,
}

impl Style {
    pub fn new(color: Color, is_dark: bool) -> Self {
        let a = color.a();
        let r = color.r();
        let g = color.g();
        let b = color.b();
        let argb = argb_to_u32(a, r, g, b);
        let scheme =
            material_color_utilities::scheme::scheme_tonal_spot(Hct::from_argb(argb), is_dark);
        let primary_color = material_dynamic_colors::primary().get_argb(&scheme);
        let on_primary_color = material_dynamic_colors::on_primary().get_argb(&scheme);
        let primary_container_color =
            material_dynamic_colors::primary_container().get_argb(&scheme);
        let on_primary_container_color =
            material_dynamic_colors::on_primary_container().get_argb(&scheme);
        let secondary_color = material_dynamic_colors::secondary().get_argb(&scheme);
        let on_secondary_color = material_dynamic_colors::on_secondary().get_argb(&scheme);
        let secondary_container_color =
            material_dynamic_colors::secondary_container().get_argb(&scheme);
        let on_secondary_container_color =
            material_dynamic_colors::on_secondary_container().get_argb(&scheme);
        let tertiary_color = material_dynamic_colors::tertiary().get_argb(&scheme);
        let on_tertiary_color = material_dynamic_colors::on_tertiary().get_argb(&scheme);
        let tertiary_container_color =
            material_dynamic_colors::tertiary_container().get_argb(&scheme);
        let on_tertiary_container_color =
            material_dynamic_colors::on_tertiary_container().get_argb(&scheme);
        let error_color = material_dynamic_colors::error().get_argb(&scheme);
        let on_error_color = material_dynamic_colors::on_error().get_argb(&scheme);
        let error_container_color = material_dynamic_colors::error_container().get_argb(&scheme);
        let on_error_container_color =
            material_dynamic_colors::on_error_container().get_argb(&scheme);
        let background_color = material_dynamic_colors::background().get_argb(&scheme);
        let on_background_color = material_dynamic_colors::on_background().get_argb(&scheme);
        let surface_color = material_dynamic_colors::surface().get_argb(&scheme);
        let on_surface_color = material_dynamic_colors::on_surface().get_argb(&scheme);
        let surface_variant_color = material_dynamic_colors::surface_variant().get_argb(&scheme);
        let on_surface_variant_color =
            material_dynamic_colors::on_surface_variant().get_argb(&scheme);
        let outline_color = material_dynamic_colors::outline().get_argb(&scheme);
        let outline_variant_color = material_dynamic_colors::outline_variant().get_argb(&scheme);
        let shadow_color = material_dynamic_colors::shadow().get_argb(&scheme);
        let scrim_color = material_dynamic_colors::scrim().get_argb(&scheme);
        let inverse_surface_color = material_dynamic_colors::inverse_surface().get_argb(&scheme);
        let inverse_on_surface_color =
            material_dynamic_colors::inverse_on_surface().get_argb(&scheme);
        let inverse_primary_color = material_dynamic_colors::inverse_primary().get_argb(&scheme);
        Self {
            is_dark,
            colors: HashMap::new(),
            dimensions: HashMap::new(),
            bools: HashMap::new(),
            items: HashMap::new(),
            styles: HashMap::new(),
        }
        .set_color(colors::PRIMARY, primary_color.into())
        .set_color(colors::ON_PRIMARY, on_primary_color.into())
        .set_color(colors::PRIMARY_CONTAINER, primary_container_color.into())
        .set_color(
            colors::ON_PRIMARY_CONTAINER,
            on_primary_container_color.into(),
        )
        .set_color(colors::SECONDARY, secondary_color.into())
        .set_color(colors::ON_SECONDARY, on_secondary_color.into())
        .set_color(
            colors::SECONDARY_CONTAINER,
            secondary_container_color.into(),
        )
        .set_color(
            colors::ON_SECONDARY_CONTAINER,
            on_secondary_container_color.into(),
        )
        .set_color(colors::TERTIARY, tertiary_color.into())
        .set_color(colors::ON_TERTIARY, on_tertiary_color.into())
        .set_color(colors::TERTIARY_CONTAINER, tertiary_container_color.into())
        .set_color(
            colors::ON_TERTIARY_CONTAINER,
            on_tertiary_container_color.into(),
        )
        .set_color(colors::ERROR, error_color.into())
        .set_color(colors::ON_ERROR, on_error_color.into())
        .set_color(colors::ERROR_CONTAINER, error_container_color.into())
        .set_color(colors::ON_ERROR_CONTAINER, on_error_container_color.into())
        .set_color(colors::BACKGROUND, background_color.into())
        .set_color(colors::ON_BACKGROUND, on_background_color.into())
        .set_color(colors::SURFACE, surface_color.into())
        .set_color(colors::ON_SURFACE, on_surface_color.into())
        .set_color(colors::SURFACE_VARIANT, surface_variant_color.into())
        .set_color(colors::ON_SURFACE_VARIANT, on_surface_variant_color.into())
        .set_color(colors::OUTLINE, outline_color.into())
        .set_color(colors::OUTLINE_VARIANT, outline_variant_color.into())
        .set_color(colors::SHADOW, shadow_color.into())
        .set_color(colors::SCRIM, scrim_color.into())
        .set_color(colors::INVERSE_SURFACE, inverse_surface_color.into())
        .set_color(colors::INVERSE_ON_SURFACE, inverse_on_surface_color.into())
        .set_color(colors::INVERSE_PRIMARY, inverse_primary_color.into())
        .set_color(WINDOW_BACKGROUND_COLOR, background_color.into())
    }

    pub fn set_color(mut self, id: impl Into<String>, color: Color) -> Self {
        self.colors.insert(id.into(), color);
        self
    }

    pub fn set_dimension(mut self, id: impl Into<String>, dimension: f32) -> Self {
        self.dimensions.insert(id.into(), dimension);
        self
    }

    pub fn set_bool(mut self, id: impl Into<String>, boolean: bool) -> Self {
        self.bools.insert(id.into(), boolean);
        self
    }

    pub fn set_item(mut self, id: impl Into<String>, item: Box<dyn Fn() -> Item>) -> Self {
        self.items.insert(id.into(), item);
        self
    }

    pub fn set_style(mut self, id: impl Into<String>, style: Style) -> Self {
        self.styles.insert(id.into(), style);
        self
    }

    pub fn get_color(&self, id: impl Into<String>) -> Option<Color> {
        self.colors.get(&id.into()).cloned()
    }

    pub fn get_dimension(&self, id: impl Into<String>) -> Option<f32> {
        self.dimensions.get(&id.into()).cloned()
    }

    pub fn get_bool(&self, id: impl Into<String>) -> Option<bool> {
        self.bools.get(&id.into()).cloned()
    }

    pub fn get_item(&self, id: impl Into<String>) -> Option<Item> {
        self.items.get(&id.into()).map(|f| f())
    }

    pub fn get_style(&self, id: impl Into<String>) -> Option<&Style> {
        self.styles.get(&id.into())
    }
}
