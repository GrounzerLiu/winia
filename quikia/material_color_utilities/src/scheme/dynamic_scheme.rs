use crate::hct::Hct;
use crate::palettes::TonalPalette;
use crate::scheme::Variant;
use crate::utils::sanitize_degrees_double;

#[derive(Clone)]
pub struct DynamicSchemeOptions {
    pub source_color_hct: Hct,
    pub variant: Variant,
    pub contrast_level: f64,
    pub is_dark: bool,
    pub primary_palette: TonalPalette,
    pub secondary_palette: TonalPalette,
    pub tertiary_palette: TonalPalette,
    pub neutral_palette: TonalPalette,
    pub neutral_variant_palette: TonalPalette,
}

#[derive(Clone)]
pub struct DynamicScheme {
    dynamic_scheme_options: DynamicSchemeOptions,
    error_palette: TonalPalette,
}

impl DynamicScheme {
    pub fn new(dynamic_scheme_options: DynamicSchemeOptions) -> Self {
        Self {
            dynamic_scheme_options,
            error_palette: TonalPalette::from_hue_and_chroma(25.0, 84.0),
        }
    }

    pub fn source_color_hct(&self) -> Hct {
        self.dynamic_scheme_options.source_color_hct
    }

    pub fn variant(&self) -> Variant {
        self.dynamic_scheme_options.variant
    }

    pub fn contrast_level(&self) -> f64 {
        self.dynamic_scheme_options.contrast_level
    }

    pub fn is_dark(&self) -> bool {
        self.dynamic_scheme_options.is_dark
    }

    pub fn primary_palette(&self) -> TonalPalette {
        self.dynamic_scheme_options.primary_palette
    }

    pub fn secondary_palette(&self) -> TonalPalette {
        self.dynamic_scheme_options.secondary_palette
    }

    pub fn tertiary_palette(&self) -> TonalPalette {
        self.dynamic_scheme_options.tertiary_palette
    }

    pub fn neutral_palette(&self) -> TonalPalette {
        self.dynamic_scheme_options.neutral_palette
    }

    pub fn neutral_variant_palette(&self) -> TonalPalette {
        self.dynamic_scheme_options.neutral_variant_palette
    }

    pub fn error_palette(&self) -> TonalPalette {
        self.error_palette
    }

    pub fn source_color_argb(&self) -> u32 {
        self.source_color_hct().to_argb()
    }
}

pub fn get_rotated_hue(source_color: &Hct, hues: &[f64], rotations: &[f64]) -> f64 {
    let source_hue = source_color.hue();

    if rotations.len() == 1 {
        return sanitize_degrees_double(source_color.hue() + rotations[0]);
    }
    let size = hues.len();
    for i in 0..=(size - 2) {
        let
            this_hue = hues[i];
        let
            next_hue = hues[i + 1];
        if this_hue < source_hue && source_hue < next_hue {
            return sanitize_degrees_double(source_hue + rotations[i]);
        }
    }
    source_hue
}