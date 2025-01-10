use crate::dynamic_color::{DynamicScheme, Variant};
use crate::hct::Hct;
use crate::palettes::TonalPalette;

const HUES: [f64; 9] = [0.0, 21.0, 51.0, 121.0, 151.0, 191.0, 271.0, 321.0, 360.0];

const SECONDARY_ROTATIONS: [f64; 9] = [45.0, 95.0, 45.0, 20.0, 45.0, 90.0, 45.0, 45.0, 45.0];

#[allow(dead_code)]
const TERTIARY_ROTATIONS: [f64; 9] = [120.0, 120.0, 20.0, 45.0, 20.0, 15.0, 20.0, 120.0, 120.0];

pub fn scheme_expressive_with_contrast(
    set_source_color_hct: Hct,
    set_is_dark: bool,
    set_contrast_level: f64,
) -> DynamicScheme {
    DynamicScheme::new(
        /*source_color_argb:*/ set_source_color_hct,
        /*variant:*/ Variant::Expressive,
        /*contrast_level:*/ set_contrast_level,
        /*is_dark:*/ set_is_dark,
        /*primary_palette:*/
        TonalPalette::from_hue_and_chroma(set_source_color_hct.get_hue() + 240.0, 40.0),
        /*secondary_palette:*/
        TonalPalette::from_hue_and_chroma(
            DynamicScheme::get_rotated_hue(set_source_color_hct, &HUES, &SECONDARY_ROTATIONS),
            24.0,
        ),
        /*tertiary_palette:*/
        TonalPalette::from_hue_and_chroma(
            DynamicScheme::get_rotated_hue(set_source_color_hct, &HUES, &SECONDARY_ROTATIONS),
            32.0,
        ),
        /*neutral_palette:*/
        TonalPalette::from_hue_and_chroma(set_source_color_hct.get_hue() + 15.0, 8.0),
        /*neutral_variant_palette:*/
        TonalPalette::from_hue_and_chroma(set_source_color_hct.get_hue() + 15.0, 12.0),
    )
}

pub fn scheme_expressive(set_source_color_hct: Hct, set_is_dark: bool) -> DynamicScheme {
    scheme_expressive_with_contrast(set_source_color_hct, set_is_dark, 0.0)
}
