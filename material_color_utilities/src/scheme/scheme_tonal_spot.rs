use crate::hct::Hct;
use crate::palettes::TonalPalette;
use crate::scheme::{DynamicSchemeOptions, Variant};
use crate::utils::sanitize_degrees_double;

pub fn scheme_tonal_spot(source_color_hct: Hct, is_dark: bool) -> DynamicSchemeOptions {
    DynamicSchemeOptions {
        source_color_hct,
        variant: Variant::TonalSpot,
        contrast_level: 0.0,
        is_dark,
        primary_palette: TonalPalette::from_hue_and_chroma(
            source_color_hct.hue(),
            36.0,
        ),
        secondary_palette: TonalPalette::from_hue_and_chroma(
            source_color_hct.hue(),
            16.0
        ),
        tertiary_palette: TonalPalette::from_hue_and_chroma(
            sanitize_degrees_double(source_color_hct.hue() + 60.0),
            24.0
        ),
        neutral_palette: TonalPalette::from_hue_and_chroma(
            source_color_hct.hue(),
            6.0
        ),
        neutral_variant_palette: TonalPalette::from_hue_and_chroma(
            source_color_hct.hue(),
            8.0
        )
    }
}