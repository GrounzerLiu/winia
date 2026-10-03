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
