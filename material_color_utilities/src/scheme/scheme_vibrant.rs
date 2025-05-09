use crate::dynamic_color::{DynamicScheme, Variant};
use crate::hct::Hct;
use crate::palettes::TonalPalette;

const HUES: [f64; 9] = [0.0, 41.0, 61.0, 101.0, 131.0, 181.0, 251.0, 301.0, 360.0];

const SECONDARY_ROTATIONS: [f64; 9] = [18.0, 15.0, 10.0, 12.0, 15.0, 18.0, 15.0, 12.0, 12.0];

const TERTIARY_ROTATIONS: [f64; 9] = [35.0, 30.0, 20.0, 25.0, 30.0, 35.0, 30.0, 25.0, 25.0];

pub fn scheme_vibrant_with_contrast(
    set_source_color_hct: Hct,
    set_is_dark: bool,
    set_contrast_level: f64,
) -> DynamicScheme {
    DynamicScheme::new(
        /*source_color_argb:*/ set_source_color_hct,
        /*variant:*/ Variant::Vibrant,
        /*contrast_level:*/ set_contrast_level,
        /*is_dark:*/ set_is_dark,
        /*primary_palette:*/
        TonalPalette::from_hue_and_chroma(set_source_color_hct.get_hue(), 200.0),
        /*secondary_palette:*/
        TonalPalette::from_hue_and_chroma(
            DynamicScheme::get_rotated_hue(set_source_color_hct, &HUES, &SECONDARY_ROTATIONS),
            24.0,
        ),
        /*tertiary_palette:*/
        TonalPalette::from_hue_and_chroma(
            DynamicScheme::get_rotated_hue(set_source_color_hct, &HUES, &TERTIARY_ROTATIONS),
            32.0,
        ),
        /*neutral_palette:*/
        TonalPalette::from_hue_and_chroma(set_source_color_hct.get_hue(), 10.0),
        /*neutral_variant_palette:*/
        TonalPalette::from_hue_and_chroma(set_source_color_hct.get_hue(), 12.0),
        None,
    )
}

pub fn scheme_vibrant(set_source_color_hct: Hct, set_is_dark: bool) -> DynamicScheme {
    scheme_vibrant_with_contrast(set_source_color_hct, set_is_dark, 0.0)
}
