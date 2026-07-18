use crate::palettes::TonalPalette;

#[derive(Clone, Copy, Default)]
pub struct CorePalette {
    primary: TonalPalette,
    secondary: TonalPalette,
    tertiary: TonalPalette,
    neutral: TonalPalette,
    neutral_variant: TonalPalette,
}
