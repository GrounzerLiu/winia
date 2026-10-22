//! The graphics-layer parameters a `Modifier::graphics_layer` carries — transforms, shadow and
//! background specs.
//!
//! Data only: which of these reach the renderer, and how, is the renderer's business.

use super::{Color, ColorFilter, Shape};

// Skia's native shadow utility consumes the alpha directly. These values match
// the low-opacity ambient/spot defaults used by Skia's shadow examples.
pub(crate) const DEFAULT_AMBIENT_SHADOW_COLOR: Color = Color { r: 0, g: 0, b: 0, a: 0x20 };

pub(crate) const DEFAULT_SPOT_SHADOW_COLOR: Color = Color { r: 0, g: 0, b: 0, a: 0x50 };

/// 图形层变换参数
///
/// ⚠ 只影响**绘制**（外观），不参与布局与命中测试（对标 Compose
/// graphicsLayer：命中区域始终是布局 bounds）。命中测试唯一考虑的
/// 位移是 scroll（布局层）；此处变换（translation/scale/rotate/
/// rotationX/Y/camera）不会改变可点击区域或按压点本地坐标。
#[derive(Debug, Clone, PartialEq)]
pub struct GraphicsLayerParams {
    pub scale_x: f32,
    pub scale_y: f32,
    pub alpha: f32,
    pub translation_x: f32,
    pub translation_y: f32,
    pub rotation_z: f32,
    /// 变换原点（pivot 分数——0..1，相对节点宽高）——对标 Compose
    /// `transformOrigin`（默认 Center——scale/rotate 绕中心）
    pub transform_origin: TransformOrigin,
    /// 裁剪到节点 bounds（对标 Compose graphicsLayer `clip`；
    /// `Modifier.alpha` 便捷版默认 clip=true）
    pub clip: bool,
    /// 绕 X 轴 3D 旋转（度——带 cameraDistance 透视）
    pub rotation_x: f32,
    /// 绕 Y 轴 3D 旋转（度——带 cameraDistance 透视）
    pub rotation_y: f32,
    /// 3D 相机距离（逻辑 px——越大透视越平；Compose 默认 8.dp）
    pub camera_distance: f32,
    /// 图层阴影高度（>0 时由 Skia ShadowUtils 绘制 ambient+spot 阴影，对标 Compose
    /// graphicsLayer.shadowElevation）。
    ///
    /// The renderer COMPRESSES this value before it becomes Skia's Z: `z = (K+1)·e/(K+e)` with K = 16
    /// (`render::paint::compressed_elevation`), so the visual steps get smaller as elevation grows.
    /// The colour alpha is not compressed. See `docs/draw-layer-extras.md`.
    pub shadow_elevation: f32,
    /// 图层阴影形状（None = 矩形）
    pub shadow_shape: Option<Shape>,
    /// 环境光阴影颜色（默认约 10% 黑，对标 Compose ambientShadowColor）。
    pub ambient_shadow_color: Color,
    /// 投射光阴影颜色（默认约 25% 黑，对标 Compose spotShadowColor）。
    pub spot_shadow_color: Color,
    /// 颜色滤镜（对标 Compose graphicsLayer `colorFilter`——渲染期 saveLayer
    /// paint 挂 color filter，层内所有内容被染色；Text/Icon 用 `Tint` 做动态颜色动画）
    pub color_filter: Option<ColorFilter>,
    /// How the layer is composited — Compose's `compositingStrategy`. winia supports `Auto` (the
    /// layer appears only when alpha/filter/effect need one) and `Offscreen` (always composite
    /// through an offscreen layer, which is what blend modes and group opacity need). `ModulateAlpha`
    /// is accepted for API parity and behaves like `Auto`: winia has no per-draw alpha modulation.
    pub compositing: CompositingStrategy,
    /// A render effect applied to the whole layer — Compose's `renderEffect`. Only blur is ported.
    pub render_effect: Option<RenderEffect>,
}

/// Compose's `CompositingStrategy` — see [`GraphicsLayerParams::compositing`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompositingStrategy {
    #[default]
    Auto,
    /// Always draw through an offscreen layer.
    Offscreen,
    /// Apply alpha per draw instead of compositing a layer; winia treats it like `Auto`.
    ModulateAlpha,
}

/// Compose's `RenderEffect`, reduced to what winia's Skia path can apply to a layer today.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RenderEffect {
    /// Gaussian blur, sigma in logical pixels.
    Blur { radius: f32 },
}

impl RenderEffect {
    pub fn blur(radius: f32) -> Self {
        Self::Blur { radius }
    }
}

impl Default for GraphicsLayerParams {
    fn default() -> Self {
        Self {
            scale_x: 1.0, scale_y: 1.0, alpha: 1.0,
            translation_x: 0.0, translation_y: 0.0, rotation_z: 0.0,
            transform_origin: TransformOrigin::CENTER,
            clip: false,
            rotation_x: 0.0, rotation_y: 0.0,
            camera_distance: 8.0,
            shadow_elevation: 0.0,
            shadow_shape: None,
            ambient_shadow_color: DEFAULT_AMBIENT_SHADOW_COLOR,
            spot_shadow_color: DEFAULT_SPOT_SHADOW_COLOR,
            color_filter: None,
            compositing: CompositingStrategy::Auto,
            render_effect: None,
        }
    }
}

/// 变换原点（对标 Compose `TransformOrigin`）——pivot 分数坐标，
/// 相对节点宽高（0.0 = 左/上，0.5 = 中心，1.0 = 右/下）
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TransformOrigin(pub f32, pub f32);

/// 阴影参数（对标 Compose `graphics.shadow.Shadow`——dropShadow 可配置集）。
/// 绘制对齐 DropShadowPainter：扩边画布 → 形状路径（模糊）画进离屏 mask →
/// 颜色 SrcIn 着色 → 按 offset 平移到画布。
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ShadowParams {
    /// 模糊半径（逻辑 px，对标 radius）
    pub radius: f32,
    /// 扩展半径（阴影比形状大多少——超出部分另画 stroke，对标 spread）
    pub spread: f32,
    /// 阴影偏移（对标 offset）
    pub offset_x: f32,
    pub offset_y: f32,
    /// 阴影颜色（对标 color，默认黑）
    pub color: Color,
    /// 独立透明度 0-1（对标 alpha）
    pub alpha: f32,
}

impl ShadowParams {
    /// 便捷构造（radius/offset/color/alpha；spread=0）
    pub fn new(radius: f32, offset_x: f32, offset_y: f32, color: Color, alpha: f32) -> Self {
        Self { radius, spread: 0.0, offset_x, offset_y, color, alpha }
    }
}

impl Default for ShadowParams {
    fn default() -> Self {
        Self {
            radius: 0.0,
            spread: 0.0,
            offset_x: 0.0,
            offset_y: 0.0,
            color: Color::from_argb(255, 0, 0, 0),
            alpha: 1.0,
        }
    }
}

impl TransformOrigin {
    /// 中心（Compose 默认）
    pub const CENTER: Self = Self(0.5, 0.5);
    /// 左上角
    pub const TOP_LEFT: Self = Self(0.0, 0.0);
    /// 右下角
    pub const BOTTOM_RIGHT: Self = Self(1.0, 1.0);
}

impl Default for TransformOrigin {
    fn default() -> Self {
        Self::CENTER
    }
}

/// The background colour: a literal, or a value evaluated per paint.
///
/// An alias of [`Dyn`](super::Dyn) so the "static or per-frame" rules exist once; the static form
/// stores the colour inline (a constant background allocates nothing).
pub type BackgroundColor = super::Dyn<Color>;

/// The border colour — the same carrier as the background's.
///
/// The `Border` element stores it directly: a literal is inline (a changed value is a changed
/// parameter, so the node re-enters), a state or closure is a parameter read at paint time.
pub type BorderColor = super::Dyn<Color>;

/// The graphics-layer parameters: a literal, or a value evaluated per paint.
pub type GraphicsLayerSpec = super::Dyn<GraphicsLayerParams>;
