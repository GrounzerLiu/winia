use crate::dynamic_color::{DynamicScheme, Variant};
use crate::hct::Hct;
use crate::palettes::TonalPalette;
use crate::utils::sanitize_degrees_double;

pub fn scheme_rainbow_with_contrast(
    set_source_color_hct: Hct,
    set_is_dark: bool,
    set_contrast_level: f64,
) -> DynamicScheme {
    DynamicScheme::new(
        /*source_color_argb:*/ set_source_color_hct,
        /*variant:*/ Variant::Rainbow,
        /*contrast_level:*/ set_contrast_level,
        /*is_dark:*/ set_is_dark,
        /*primary_palette:*/
        TonalPalette::from_hue_and_chroma(set_source_color_hct.get_hue(), 48.0),
        /*secondary_palette:*/
        TonalPalette::from_hue_and_chroma(set_source_color_hct.get_hue(), 16.0),
        /*tertiary_palette:*/
        TonalPalette::from_hue_and_chroma(
            sanitize_degrees_double(set_source_color_hct.get_hue() + 60.0),
            24.0,
        ),
        /*neutral_palette:*/
        TonalPalette::from_hue_and_chroma(set_source_color_hct.get_hue(), 0.0),
        /*neutral_variant_palette:*/
        TonalPalette::from_hue_and_chroma(set_source_color_hct.get_hue(), 0.0),
    )
}

pub fn scheme_rainbow(set_source_color_hct: Hct, set_is_dark: bool) -> DynamicScheme {
    scheme_rainbow_with_contrast(set_source_color_hct, set_is_dark, 0.0)
}
