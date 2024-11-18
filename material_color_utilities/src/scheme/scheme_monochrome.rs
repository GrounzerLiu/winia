use crate::hct::Hct;
use crate::palettes::TonalPalette;
use crate::scheme::{DynamicSchemeOptions, Variant};

pub fn scheme_monochrome(source_color_hct: Hct, is_dark: bool) -> DynamicSchemeOptions {
    DynamicSchemeOptions {
        source_color_hct,
        variant: Variant::Monochrome,
        contrast_level: 0.0,
        is_dark,
        primary_palette: TonalPalette::from_hue_and_chroma(
            source_color_hct.hue(),
            0.0,
        ),
        secondary_palette: TonalPalette::from_hue_and_chroma(
            source_color_hct.hue(),
            0.0
        ),
        tertiary_palette: TonalPalette::from_hue_and_chroma(
            source_color_hct.hue(),
            0.0
        ),
        neutral_palette: TonalPalette::from_hue_and_chroma(
            source_color_hct.hue(),
            0.0
        ),
        neutral_variant_palette: TonalPalette::from_hue_and_chroma(
            source_color_hct.hue(),
            0.0
        )
    }
}