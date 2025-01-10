use crate::dynamic_color::{material_dynamic_colors, Variant};
use crate::hct::Hct;
use crate::palettes::TonalPalette;
use crate::utils::{sanitize_degrees_double, Argb};

pub struct DynamicScheme {
    pub source_color_hct: Hct,
    pub variant: Variant,
    pub is_dark: bool,
    pub contrast_level: f64,

    pub primary_palette: TonalPalette,
    pub secondary_palette: TonalPalette,
    pub tertiary_palette: TonalPalette,
    pub neutral_palette: TonalPalette,
    pub neutral_variant_palette: TonalPalette,
    pub error_palette: TonalPalette,
}

impl DynamicScheme{
    pub fn new(
        source_color_argb: Argb,
        variant: Variant,
        contrast_level: f64,
        is_dark: bool,
        primary_palette: TonalPalette,
        secondary_palette: TonalPalette,
        tertiary_palette: TonalPalette,
        neutral_palette: TonalPalette,
        neutral_variant_palette: TonalPalette,
    ) -> Self {
        Self {
            source_color_hct: Hct::from_argb(source_color_argb),
            variant,
            is_dark,
            contrast_level,
            primary_palette,
            secondary_palette,
            tertiary_palette,
            neutral_palette,
            neutral_variant_palette,
            error_palette: TonalPalette::from_hue_and_chroma(25.0, 84.0),
        }
    }

    pub fn source_color_hct(&self) -> Hct {
        self.source_color_hct
    }

    pub fn variant(&self) -> Variant {
        self.variant
    }

    pub fn is_dark(&self) -> bool {
        self.is_dark
    }

    pub fn contrast_level(&self) -> f64 {
        self.contrast_level
    }

    pub fn primary_palette(&self) -> TonalPalette {
        self.primary_palette
    }

    pub fn secondary_palette(&self) -> TonalPalette {
        self.secondary_palette
    }

    pub fn tertiary_palette(&self) -> TonalPalette {
        self.tertiary_palette
    }

    pub fn neutral_palette(&self) -> TonalPalette {
        self.neutral_palette
    }

    pub fn neutral_variant_palette(&self) -> TonalPalette {
        self.neutral_variant_palette
    }

    pub fn error_palette(&self) -> TonalPalette {
        self.error_palette
    }

    pub fn get_rotated_hue(
        source_color: Hct,
        hues: &[f64],
        rotations: &[f64],
    ) -> f64 {
        let source_hue = source_color.hue();
        if rotations.len() == 1 {
            return sanitize_degrees_double(source_color.hue() + rotations[0]);
        }
        let size = hues.len();
        for i in 0..=(size - 2) {
            let this_hue = hues[i];
            let next_hue = hues[i + 1];
            if this_hue < source_hue && source_hue < next_hue {
                return sanitize_degrees_double(source_hue + rotations[i]);
            }
        }
        source_hue
    }

    pub fn source_color_argb(&self) -> Argb {
        self.source_color_hct.to_argb()
    }

    pub fn get_primary_palette_key_color(&self) -> Argb {
        material_dynamic_colors::primary_palette_key_color().get_argb(self)
    }

    pub fn get_secondary_palette_key_color(&self) -> Argb {
        material_dynamic_colors::secondary_palette_key_color().get_argb(self)
    }

    pub fn get_tertiary_palette_key_color(&self) -> Argb {
        material_dynamic_colors::tertiary_palette_key_color().get_argb(self)
    }

    pub fn get_neutral_palette_key_color(&self) -> Argb {
        material_dynamic_colors::neutral_palette_key_color().get_argb(self)
    }

    pub fn get_neutral_variant_palette_key_color(&self) -> Argb {
        material_dynamic_colors::neutral_variant_palette_key_color().get_argb(self)
    }

    pub fn get_background(&self) -> Argb {
        material_dynamic_colors::background().get_argb(self)
    }

    pub fn get_on_background(&self) -> Argb {
        material_dynamic_colors::on_background().get_argb(self)
    }

    pub fn get_surface(&self) -> Argb {
        material_dynamic_colors::surface().get_argb(self)
    }

    pub fn get_surface_dim(&self) -> Argb {
        material_dynamic_colors::surface_dim().get_argb(self)
    }

    pub fn get_surface_bright(&self) -> Argb {
        material_dynamic_colors::surface_bright().get_argb(self)
    }

    pub fn get_surface_container_lowest(&self) -> Argb {
        material_dynamic_colors::surface_container_lowest().get_argb(self)
    }

    pub fn get_surface_container_low(&self) -> Argb {
        material_dynamic_colors::surface_container_low().get_argb(self)
    }

    pub fn get_surface_container(&self) -> Argb {
        material_dynamic_colors::surface_container().get_argb(self)
    }

    pub fn get_surface_container_high(&self) -> Argb {
        material_dynamic_colors::surface_container_high().get_argb(self)
    }

    pub fn get_surface_container_highest(&self) -> Argb {
        material_dynamic_colors::surface_container_highest().get_argb(self)
    }

    pub fn get_on_surface(&self) -> Argb {
        material_dynamic_colors::on_surface().get_argb(self)
    }

    pub fn get_surface_variant(&self) -> Argb {
        material_dynamic_colors::surface_variant().get_argb(self)
    }

    pub fn get_on_surface_variant(&self) -> Argb {
        material_dynamic_colors::on_surface_variant().get_argb(self)
    }

    pub fn get_inverse_surface(&self) -> Argb {
        material_dynamic_colors::inverse_surface().get_argb(self)
    }

    pub fn get_inverse_on_surface(&self) -> Argb {
        material_dynamic_colors::inverse_on_surface().get_argb(self)
    }

    pub fn get_outline(&self) -> Argb {
        material_dynamic_colors::outline().get_argb(self)
    }

    pub fn get_outline_variant(&self) -> Argb {
        material_dynamic_colors::outline_variant().get_argb(self)
    }

    pub fn get_shadow(&self) -> Argb {
        material_dynamic_colors::shadow().get_argb(self)
    }

    pub fn get_scrim(&self) -> Argb {
        material_dynamic_colors::scrim().get_argb(self)
    }

    pub fn get_surface_tint(&self) -> Argb {
        material_dynamic_colors::surface_tint().get_argb(self)
    }

    pub fn get_primary(&self) -> Argb {
        material_dynamic_colors::primary().get_argb(self)
    }

    pub fn get_on_primary(&self) -> Argb {
        material_dynamic_colors::on_primary().get_argb(self)
    }

    pub fn get_primary_container(&self) -> Argb {
        material_dynamic_colors::primary_container().get_argb(self)
    }

    pub fn get_on_primary_container(&self) -> Argb {
        material_dynamic_colors::on_primary_container().get_argb(self)
    }

    pub fn get_inverse_primary(&self) -> Argb {
        material_dynamic_colors::inverse_primary().get_argb(self)
    }

    pub fn get_secondary(&self) -> Argb {
        material_dynamic_colors::secondary().get_argb(self)
    }

    pub fn get_on_secondary(&self) -> Argb {
        material_dynamic_colors::on_secondary().get_argb(self)
    }

    pub fn get_secondary_container(&self) -> Argb {
        material_dynamic_colors::secondary_container().get_argb(self)
    }

    pub fn get_on_secondary_container(&self) -> Argb {
        material_dynamic_colors::on_secondary_container().get_argb(self)
    }

    pub fn get_tertiary(&self) -> Argb {
        material_dynamic_colors::tertiary().get_argb(self)
    }

    pub fn get_on_tertiary(&self) -> Argb {
        material_dynamic_colors::on_tertiary().get_argb(self)
    }

    pub fn get_tertiary_container(&self) -> Argb {
        material_dynamic_colors::tertiary_container().get_argb(self)
    }

    pub fn get_on_tertiary_container(&self) -> Argb {
        material_dynamic_colors::on_tertiary_container().get_argb(self)
    }

    pub fn get_error(&self) -> Argb {
        material_dynamic_colors::error().get_argb(self)
    }

    pub fn get_on_error(&self) -> Argb {
        material_dynamic_colors::on_error().get_argb(self)
    }

    pub fn get_error_container(&self) -> Argb {
        material_dynamic_colors::error_container().get_argb(self)
    }

    pub fn get_on_error_container(&self) -> Argb {
        material_dynamic_colors::on_error_container().get_argb(self)
    }

    pub fn get_primary_fixed(&self) -> Argb {
        material_dynamic_colors::primary_fixed().get_argb(self)
    }

    pub fn get_primary_fixed_dim(&self) -> Argb {
        material_dynamic_colors::primary_fixed_dim().get_argb(self)
    }

    pub fn get_on_primary_fixed(&self) -> Argb {
        material_dynamic_colors::on_primary_fixed().get_argb(self)
    }

    pub fn get_on_primary_fixed_variant(&self) -> Argb {
        material_dynamic_colors::on_primary_fixed_variant().get_argb(self)
    }

    pub fn get_secondary_fixed(&self) -> Argb {
        material_dynamic_colors::secondary_fixed().get_argb(self)
    }

    pub fn get_secondary_fixed_dim(&self) -> Argb {
        material_dynamic_colors::secondary_fixed_dim().get_argb(self)
    }

    pub fn get_on_secondary_fixed(&self) -> Argb {
        material_dynamic_colors::on_secondary_fixed().get_argb(self)
    }

    pub fn get_on_secondary_fixed_variant(&self) -> Argb {
        material_dynamic_colors::on_secondary_fixed_variant().get_argb(self)
    }

    pub fn get_tertiary_fixed(&self) -> Argb {
        material_dynamic_colors::tertiary_fixed().get_argb(self)
    }

    pub fn get_tertiary_fixed_dim(&self) -> Argb {
        material_dynamic_colors::tertiary_fixed_dim().get_argb(self)
    }

    pub fn get_on_tertiary_fixed(&self) -> Argb {
        material_dynamic_colors::on_tertiary_fixed().get_argb(self)
    }

    pub fn get_on_tertiary_fixed_variant(&self) -> Argb {
        material_dynamic_colors::on_tertiary_fixed_variant().get_argb(self)
    }
}