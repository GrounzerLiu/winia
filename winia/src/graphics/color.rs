//! Colour, blending and filters — Compose's `ui.graphics.Color`, `BlendMode`, `ColorFilter` and
//! `FilterQuality`.
//!
//! They lived in `modifier.rs`, which made the crate's most-used type (`Color` appears in over
//! 1,700 places) a member of the modifier chain rather than of the drawing layer it describes.


/// 颜色（占位，后续由 skia Color 或 material theme 替代）
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const TRANSPARENT: Color = Color { r: 0, g: 0, b: 0, a: 0 };
    pub const BLACK: Color = Color { r: 0, g: 0, b: 0, a: 255 };
    pub const WHITE: Color = Color { r: 255, g: 255, b: 255, a: 255 };
    pub const RED: Color = Color { r: 255, g: 0, b: 0, a: 255 };
    pub const GREEN: Color = Color { r: 0, g: 255, b: 0, a: 255 };
    pub const BLUE: Color = Color { r: 0, g: 0, b: 255, a: 255 };

    pub fn from_argb(a: u8, r: u8, g: u8, b: u8) -> Self {
        Color { r, g, b, a }
    }

    /// The colour as four animatable components in Oklab — `[alpha, L, a, b]`, the representation
    /// Compose's `ColorVectorConverter` animates (`animation/ColorVectorConverter.kt:31-49`): its
    /// `convertToVector` converts to Oklab and hands the animation `AnimationVector4D(alpha, L, a, b)`.
    ///
    /// Oklab, not the sRGB bytes: interpolating the ENCODED channels darkens or shifts hue through the
    /// middle of a transition (`black -> white` passes through `#7F7F7F` in sRGB bytes, but `#636363` at
    /// a 50 % displacement — the same lightness to the eye). The matrices are the standard ones
    /// Compose's `ColorSpaces.Oklab` uses: sRGB -> linear light -> LMS -> cube root -> Lab.
    pub fn to_oklab(self) -> [f32; 4] {
        let (r, g, b) = (srgb_to_linear(self.r), srgb_to_linear(self.g), srgb_to_linear(self.b));
        let l = 0.412_221_47 * r + 0.536_332_54 * g + 0.051_445_995 * b;
        let m = 0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b;
        let s = 0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b;
        let (l, m, s) = (l.cbrt(), m.cbrt(), s.cbrt());
        [
            self.a as f32 / 255.0,
            0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
            1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
            0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
        ]
    }

    /// The inverse of [`Color::to_oklab`], with Compose's clamps for the round trip
    /// (`ColorVectorConverter.kt:40-44`: L in 0..1, a and b in -0.5..0.5, alpha in 0..1). Compose then
    /// converts back into the colour space the animation started in; winia's `Color` is always sRGB, so
    /// that last step has nothing to do here — recorded as a deviation in `animation.rs`.
    pub fn from_oklab(components: [f32; 4]) -> Self {
        let [alpha, l, a, b] = components;
        let l = l.clamp(0.0, 1.0);
        let a = a.clamp(-0.5, 0.5);
        let b = b.clamp(-0.5, 0.5);
        let l_ = l + 0.396_337_78 * a + 0.215_803_76 * b;
        let m_ = l - 0.105_561_346 * a - 0.063_854_17 * b;
        let s_ = l - 0.089_484_18 * a - 1.291_485_5 * b;
        let (l3, m3, s3) = (l_ * l_ * l_, m_ * m_ * m_, s_ * s_ * s_);
        let r = 4.076_741_7 * l3 - 3.307_711_6 * m3 + 0.230_969_94 * s3;
        let g = -1.268_438 * l3 + 2.609_757_4 * m3 - 0.341_319_4 * s3;
        let blue = -0.004_196_086_3 * l3 - 0.703_418_6 * m3 + 1.707_614_7 * s3;
        Self::from_argb(
            (alpha.clamp(0.0, 1.0) * 255.0).round() as u8,
            to_srgb_byte(r),
            to_srgb_byte(g),
            to_srgb_byte(blue),
        )
    }
}

/// sRGB byte -> linear light, the transfer function the Oklab matrices expect.
fn srgb_to_linear(byte: u8) -> f32 {
    let c = byte as f32 / 255.0;
    if c <= 0.040_45 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

/// Linear light -> sRGB byte, clamped the way a display expects.
fn to_srgb_byte(c: f32) -> u8 {
    let c = c.clamp(0.0, 1.0);
    let v = if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
    (v * 255.0).round() as u8
}

impl Color {
    /// 状态层叠加（对标 Material3 state layer）：
    /// 把 `overlay` 以 `alpha` 透明度叠到当前颜色上——hover 8% / press/focus 12% /
    /// drag 16% 的近似实现（Material3 的容器状态层）。
    pub fn overlay(&self, overlay: Color, alpha: f32) -> Color {
        let a = alpha.clamp(0.0, 1.0);
        let lerp = |b: u8, o: u8| (b as f32 * (1.0 - a) + o as f32 * a).round() as u8;
        Color::from_argb(
            self.a,
            lerp(self.r, overlay.r),
            lerp(self.g, overlay.g),
            lerp(self.b, overlay.b),
        )
    }
}

/// 混合模式（对标 Compose `BlendMode`，与 skia 同源 29 值——
/// 渲染期映射 `skia_safe::BlendMode`）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendMode {
    Clear, Src, Dst, SrcOver, DstOver, SrcIn, DstIn, SrcOut, DstOut,
    SrcATop, DstATop, Xor, Plus, Modulate, Screen, Overlay, Darken, Lighten,
    ColorDodge, ColorBurn, HardLight, SoftLight, Difference, Exclusion, Multiply,
    Hue, Saturation, Color, Luminosity,
}

/// 颜色滤镜（对标 Compose `ColorFilter`——Image/Icon 渲染期挂到 paint）
#[derive(Debug, Clone, PartialEq)]
pub enum ColorFilter {
    /// 染色（对标 `ColorFilter.tint`——默认 SrcIn 保留形状 alpha）
    Tint { color: Color, blend_mode: BlendMode },
    /// 颜色矩阵（20 值行主序——对标 `ColorFilter.colorMatrix`）
    Matrix([f32; 20]),
    /// 光照效果（像素 × multiply + add——对标 `ColorFilter.lighting`）
    Lighting { multiply: Color, add: Color },
}

/// 采样质量（对标 Compose `FilterQuality`）——缩放位图时的过滤策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterQuality {
    /// 最近邻（无过滤——像素风/精确采样）
    None,
    /// 双线性（默认——缩小放大平滑）
    Low,
    /// 双线性 + 最近 mipmap（缩小更平滑）
    Medium,
    /// 三线性（双线性 + 线性 mipmap——最高质量）
    High,
}

impl Default for FilterQuality {
    fn default() -> Self { Self::Low }
}

/// This crate's `Color` as skia's — the one place the two meet, so that a module holding a colour
/// (text layout, say) does not have to reach up into the renderer to convert it.
pub fn skia_color(c: Color) -> skia_safe::Color {
    skia_safe::Color::from_argb(c.a, c.r, c.g, c.b)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pinned Oklab coordinates are the ones Compose's `ColorSpaces.Oklab` produces for sRGB —
    /// red is `(0.6280, 0.2249, 0.1258)` there, and white/black are the axis ends.
    #[test]
    fn oklab_matches_composes_coordinates() {
        let white = Color::WHITE.to_oklab();
        assert!((white[1] - 1.0).abs() < 1e-4 && white[2].abs() < 1e-4 && white[3].abs() < 1e-4);
        let black = Color::BLACK.to_oklab();
        assert!(black[1].abs() < 1e-6 && black[2].abs() < 1e-6 && black[3].abs() < 1e-6);
        let red = Color::RED.to_oklab();
        assert!(
            (red[1] - 0.6280).abs() < 1e-3 && (red[2] - 0.2249).abs() < 1e-3 && (red[3] - 0.1258).abs() < 1e-3,
            "sRGB red in Oklab: {red:?}"
        );
        assert_eq!(Color::from_argb(128, 1, 2, 3).to_oklab()[0], 128.0 / 255.0, "alpha is its own component");
    }

    #[test]
    fn oklab_round_trips_every_channel() {
        for color in [
            Color::from_argb(255, 103, 80, 164),
            Color::from_argb(80, 0, 0, 0),
            Color::from_argb(255, 255, 255, 255),
            Color::from_argb(255, 0, 128, 255),
            Color::from_argb(17, 200, 30, 90),
        ] {
            let back = Color::from_oklab(color.to_oklab());
            assert_eq!(back, color, "round trip of {color:?} (through {:?})", color.to_oklab());
        }
    }

    /// What the Oklab midpoint buys: `black -> white` at 50 % displacement is `#636363`, not the
    /// `#7F7F7F` that interpolating the encoded channels gives.
    #[test]
    fn the_midpoint_is_the_perceptual_one() {
        let mid = Color::from_oklab([1.0, 0.5, 0.0, 0.0]);
        assert_eq!((mid.r, mid.g, mid.b), (99, 99, 99), "Oklab L = 0.5 is #636363 in sRGB");
        let encoded_mid = Color::from_argb(255, 128, 128, 128);
        assert_ne!(mid, encoded_mid, "and that is NOT the byte-wise midpoint");
    }

    /// The five-step grey ramp the colour demo draws, pinned here because its captions print these
    /// values: the engine's Oklab steps against a byte-wise blend of the same progress. A change to
    /// either interpolation shows up as a failing test instead of a wrong label in the demo.
    #[test]
    fn the_grey_ramp_is_perceptual_not_byte_wise() {
        let engine: Vec<(u8, u8, u8)> = (0..5)
            .map(|i| {
                let c = Color::from_oklab([1.0, i as f32 / 4.0, 0.0, 0.0]);
                (c.r, c.g, c.b)
            })
            .collect();
        assert_eq!(
            engine,
            vec![
                (0x00, 0x00, 0x00),
                (0x22, 0x22, 0x22),
                (0x63, 0x63, 0x63),
                (0xAE, 0xAE, 0xAE),
                (0xFF, 0xFF, 0xFF),
            ],
            "the demo's engine strip"
        );
        let bytes: Vec<u8> = (0..5).map(|i| (255.0 * (i as f32 / 4.0)).round() as u8).collect();
        assert_eq!(bytes, vec![0x00, 0x40, 0x80, 0xBF, 0xFF], "the demo's byte strip");
        assert!(
            engine[1].0 < bytes[1] && engine[3].0 < bytes[3],
            "the perceptual ramp stays darker than the byte blend of the same progress"
        );
    }

    #[test]
    fn from_oklab_clamps_like_compose_does() {
        assert_eq!(Color::from_oklab([2.0, 2.0, 0.0, 0.0]), Color::WHITE);
        assert_eq!(Color::from_oklab([-1.0, -1.0, 0.0, 0.0]), Color::TRANSPARENT);
        let wild = Color::from_oklab([0.5, 0.5, 9.0, -9.0]);
        assert_eq!(wild.a, 128, "alpha clamps, it does not wrap");
    }
}
