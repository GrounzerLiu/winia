//! The graphics-layer parameters a `Modifier::graphics_layer` carries — transforms, shadow and
//! background specs.
//!
//! Data only: which of these reach the renderer, and how, is the renderer's business.

use super::{Color, ColorFilter, Shape};
use std::sync::Arc;

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
    /// 图层阴影高度（逻辑 px——>0 时由 Skia ShadowUtils 绘制 ambient+spot 阴影，
    /// 对标 Compose graphicsLayer.shadowElevation）
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

/// 背景色规格：静态 `Color` 或动态闭包（渲染时每帧求值）。
/// 通过 `impl Into<BackgroundColor>` 统一 `background()` 入口——传 `Color` 或闭包均可。
pub struct BackgroundColor(pub(crate) Arc<dyn Fn() -> Color + Send + Sync>);

impl From<Color> for BackgroundColor {
    fn from(color: Color) -> Self {
        Self(Arc::new(move || color))
    }
}

impl From<crate::runtime::state::DerivedValue<Color>> for BackgroundColor {
    fn from(d: crate::runtime::state::DerivedValue<Color>) -> Self {
        Self(Arc::new(move || d.get()))
    }
}

/// 图形层规格：静态 `GraphicsLayerParams` 或动态闭包（渲染时每帧求值）。
/// 通过 `impl Into<GraphicsLayerSpec>` 统一 `graphics_layer()` 入口。
pub struct GraphicsLayerSpec(pub(crate) Arc<dyn Fn() -> GraphicsLayerParams + Send + Sync>);

impl From<GraphicsLayerParams> for GraphicsLayerSpec {
    fn from(params: GraphicsLayerParams) -> Self {
        Self(Arc::new(move || params.clone()))
    }
}
