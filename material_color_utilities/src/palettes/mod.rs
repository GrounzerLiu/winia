mod core;
mod tones;

pub use core::*;
pub use tones::*;

#[cfg(test)]
mod tones_test {
    use crate::palettes::TonalPalette;

    #[test]
    fn test_blue() {
        let color = 0xff0000ff;
        let tonal_palette = TonalPalette::from_argb(color);
        assert_eq!(tonal_palette.get(100.0), 0xffffffff);
        assert_eq!(tonal_palette.get(95.0), 0xfff1efff);
        assert_eq!(tonal_palette.get(90.0), 0xffe0e0ff);
        assert_eq!(tonal_palette.get(80.0), 0xffbec2ff);
        assert_eq!(tonal_palette.get(70.0), 0xff9da3ff);
        assert_eq!(tonal_palette.get(60.0), 0xff7c84ff);
        assert_eq!(tonal_palette.get(50.0), 0xff5a64ff);
        assert_eq!(tonal_palette.get(40.0), 0xff343dff);
        assert_eq!(tonal_palette.get(30.0), 0xff0000ef);
        assert_eq!(tonal_palette.get(20.0), 0xff0001ac);
        assert_eq!(tonal_palette.get(10.0), 0xff00006e);
        assert_eq!(tonal_palette.get(0.0), 0xff000000);
    }
}

#[cfg(test)]
mod key_color_test {
    use crate::assert_near;
    use crate::palettes::TonalPalette;

    #[test]
    fn exact_chroma_available() {
        // Requested chroma is exactly achievable at a certain tone.
        let palette = TonalPalette::from_hue_and_chroma(50.0, 60.0);
        let result = palette.key_color();

        assert_near!(result.get_hue(), 50.0, 10.0);
        assert_near!(result.get_chroma(), 60.0, 0.5);
        // Tone might vary, but should be within the range from 0 to 100.
        assert!(result.get_tone() > 0.0);
        assert!(result.get_tone() < 100.0);
    }

    #[test]
    fn unusually_high_chroma() {
        // Requested chroma is above what is achievable. For Hue 149, chroma peak
        // is 89.6 at Tone 87.9. The result key color's chroma should be close to the
        // chroma peak.
        let palette = TonalPalette::from_hue_and_chroma(149.0, 200.0);
        let result = palette.key_color();

        assert_near!(result.get_hue(), 149.0, 10.0);
        assert!(result.get_chroma() > 89.0);
        // Tone might vary, but should be within the range from 0 to 100.
        assert!(result.get_tone() > 0.0);
        assert!(result.get_tone() < 100.0);
    }

    #[test]
    fn test_key_color() {
        // By definition, the key color should be the first tone, starting from Tone
        // 50, matching the given hue and chroma. When requesting a very low chroma,
        // the result should be close to Tone 50, since most tones can produce a low
        // chroma.
        let palette = TonalPalette::from_hue_and_chroma(50.0, 3.0);
        let result = palette.key_color();

        // Higher error tolerance for hue when the requested chroma is unusually low.
        assert_near!(result.get_hue(), 50.0, 10.0);
        assert_near!(result.get_chroma(), 3.0, 0.5);
        assert_near!(result.get_tone(), 50.0, 0.5);
    }
}
