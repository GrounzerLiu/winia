use crate::dislike::fix_if_disliked;
use crate::dynamic_color::{DynamicScheme, Variant};
use crate::hct::Hct;
use crate::palettes::TonalPalette;
use crate::TemperatureCache;

pub fn scheme_fidelity_with_contrast(
    set_source_color_hct: Hct,
    set_is_dark: bool,
    set_contrast_level: f64,
) -> DynamicScheme {
    DynamicScheme::new(
        /*source_color_argb:*/ set_source_color_hct.to_argb(),
        /*variant:*/ Variant::Fidelity,
        /*contrast_level:*/ set_contrast_level,
        /*is_dark:*/ set_is_dark,
        /*primary_palette:*/
        TonalPalette::from_hue_and_chroma(
            set_source_color_hct.get_hue(),
            set_source_color_hct.get_chroma(),
        ),
        /*secondary_palette:*/
        TonalPalette::from_hue_and_chroma(
            set_source_color_hct.get_hue(),
            (set_source_color_hct.get_chroma() - 32.0).max(set_source_color_hct.get_chroma() * 0.5),
        ),
        /*tertiary_palette:*/
        TonalPalette::from_hct(fix_if_disliked(
            TemperatureCache::new(set_source_color_hct).get_complement(),
        )),
        /*neutral_palette:*/
        TonalPalette::from_hue_and_chroma(
            set_source_color_hct.get_hue(),
            set_source_color_hct.get_chroma() / 8.0,
        ),
        /*neutral_variant_palette:*/
        TonalPalette::from_hue_and_chroma(
            set_source_color_hct.get_hue(),
            set_source_color_hct.get_chroma() / 8.0 + 4.0,
        ),
    )
}

pub fn scheme_fidelity(set_source_color_hct: Hct, set_is_dark: bool) -> DynamicScheme {
    scheme_fidelity_with_contrast(set_source_color_hct, set_is_dark, 0.0)
}
