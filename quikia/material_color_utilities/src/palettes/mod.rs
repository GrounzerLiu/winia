mod tones;
mod core;

pub use tones::*;
pub use core::*;

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
mod core_test {
    use crate::assert_near;
    use crate::hct::{argb_from_hcl, Cam};
    use crate::palettes::CorePalette;
    use crate::utils::diff_degrees;

    #[test]
    fn test_hue_rotates_red() {
        let color = 0xffff0000;
        let core_palette = CorePalette::from_argb(color);
        let delta_hue = diff_degrees(Cam::from_argb(core_palette.tertiary().get(50.0)).hue,
                                     Cam::from_argb(core_palette.primary().get(50.0)).hue);
        assert_near!(delta_hue, 60.0, 2.0);
    }

    #[test]
    fn test_hue_rotates_green() {
        let color = 0xff00ff00;
        let core_palette = CorePalette::from_argb(color);
        let delta_hue = diff_degrees(Cam::from_argb(core_palette.tertiary().get(50.0)).hue,
                                     Cam::from_argb(core_palette.primary().get(50.0)).hue);
        assert_near!(delta_hue, 60.0, 2.0);
    }

    #[test]
    fn test_hue_rotates_blue() {
        let color = 0xff0000ff;
        let core_palette = CorePalette::from_argb(color);
        let delta_hue = diff_degrees(Cam::from_argb(core_palette.tertiary().get(50.0)).hue,
                                     Cam::from_argb(core_palette.primary().get(50.0)).hue);
        assert_near!(delta_hue, 60.0, 1.0);
    }

    #[test]
    fn test_hue_wraps_when_rotating() {
        let cam = Cam::from_argb(argb_from_hcl(350.0, 48.0, 50.0));
        let core_palette = CorePalette::from_hue_and_chroma(cam.hue, cam.chroma);
        let a1_hue = Cam::from_argb(core_palette.primary().get(50.0)).hue;
        let a3_hue = Cam::from_argb(core_palette.tertiary().get(50.0)).hue;
        assert_near!(diff_degrees(a1_hue, a3_hue), 60.0, 1.0);
        assert_near!(a3_hue, 50.0, 1.0);
    }
}