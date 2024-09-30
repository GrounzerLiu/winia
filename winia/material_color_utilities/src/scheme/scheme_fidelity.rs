use crate::hct::Hct;
use crate::dislike::fix_if_disliked;
use crate::palettes::TonalPalette;
use crate::scheme::{DynamicSchemeOptions, Variant};
use crate::TemperatureCache;

pub fn scheme_fidelity(source_color_hct: Hct, is_dark: bool) -> DynamicSchemeOptions {
    DynamicSchemeOptions {
        source_color_hct,
        variant: Variant::Fidelity,
        contrast_level: 0.0,
        is_dark,
        primary_palette: TonalPalette::from_hue_and_chroma(
            source_color_hct.hue(), source_color_hct.chroma(),
        ),
        secondary_palette: TonalPalette::from_hue_and_chroma(
            source_color_hct.hue(),
            (source_color_hct.chroma() - 32.0)
                .max(source_color_hct.chroma() * 0.5)
        ),
        tertiary_palette: TonalPalette::from_hct(
            fix_if_disliked(
                TemperatureCache::new(source_color_hct)
                    .get_complement()
            )),
        neutral_palette: TonalPalette::from_hue_and_chroma(
            source_color_hct.hue(),
            source_color_hct.chroma() / 8.0
        ),
        neutral_variant_palette: TonalPalette::from_hue_and_chroma(
            source_color_hct.hue(),
            source_color_hct.chroma() / 8.0 + 4.0)
    }
}