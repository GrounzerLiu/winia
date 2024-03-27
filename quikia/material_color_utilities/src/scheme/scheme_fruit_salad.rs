use crate::hct::Hct;
use crate::palettes::TonalPalette;
use crate::scheme::{DynamicSchemeOptions, Variant};
use crate::utils::sanitize_degrees_double;

pub fn scheme_fruit_salad(source_color_hct: Hct, is_dark: bool) -> DynamicSchemeOptions {
    DynamicSchemeOptions {
        source_color_hct,
        variant: Variant::FruitSalad,
        contrast_level: 0.0,
        is_dark,
        primary_palette: TonalPalette::from_hue_and_chroma(
            sanitize_degrees_double(source_color_hct.hue() - 50.0),
            48.0,
        ),
        secondary_palette: TonalPalette::from_hue_and_chroma(
            sanitize_degrees_double(source_color_hct.hue() + 50.0),
            36.0
        ),
        tertiary_palette: TonalPalette::from_hue_and_chroma(
            source_color_hct.hue(),
            36.0
        ),
        neutral_palette: TonalPalette::from_hue_and_chroma(
            source_color_hct.hue(),
            10.0
        ),
        neutral_variant_palette: TonalPalette::from_hue_and_chroma(
            source_color_hct.hue(),
            16.0
        )
    }
}