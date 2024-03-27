use crate::hct::Hct;
use crate::palettes::TonalPalette;
use crate::scheme::{DynamicSchemeOptions, get_rotated_hue, Variant};

const HUES: [f64; 9] = [0.0, 21.0, 51.0, 121.0, 151.0, 191.0, 271.0, 321.0, 360.0];

const SECONDARY_ROTATIONS: [f64; 9] = [45.0, 95.0, 45.0, 20.0, 45.0, 90.0, 45.0, 45.0, 45.0];

#[allow(dead_code)]
const TERTIARY_ROTATIONS: [f64; 9] = [120.0, 120.0, 20.0, 45.0, 20.0, 15.0, 20.0, 120.0, 120.0];

pub fn scheme_expressive(source_color_hct: Hct, is_dark: bool) -> DynamicSchemeOptions {
    DynamicSchemeOptions {
        source_color_hct,
        variant: Variant::Expressive,
        contrast_level: 0.0,
        is_dark,
        primary_palette: TonalPalette::from_hue_and_chroma(
            source_color_hct.hue() + 240.0, 40.0
        ),
        secondary_palette: TonalPalette::from_hue_and_chroma(
            get_rotated_hue(&source_color_hct, &HUES, &SECONDARY_ROTATIONS),
            24.0
        ),
        tertiary_palette: TonalPalette::from_hue_and_chroma(
            get_rotated_hue(&source_color_hct, &HUES, &SECONDARY_ROTATIONS),
            32.0
        ),
        neutral_palette: TonalPalette::from_hue_and_chroma(source_color_hct.hue() + 15.0, 8.0),
        neutral_variant_palette: TonalPalette::from_hue_and_chroma(source_color_hct.hue() + 15.0, 12.0),
    }
}