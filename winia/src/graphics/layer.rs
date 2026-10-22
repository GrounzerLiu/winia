//! The graphics-layer parameters a `Modifier::graphics_layer` carries — transforms, shadow and
//! background specs.
//!
//! Data only: which of these reach the renderer, and how, is the renderer's business.

use super::{Color, ColorFilter, Shape};

// Skia's native shadow utility consumes the alpha directly. These values match
// the low-opacity ambient/spot defaults used by Skia's shadow examples.
pub(crate) const DEFAULT_AMBIENT_SHADOW_COLOR: Color = Color { r: 0, g: 0, b: 0, a: 0x20 };

pub(crate) const DEFAULT_SPOT_SHADOW_COLOR: Color = Color { r: 0, g: 0, b: 0, a: 0x50 };

/// Graphics-layer transform parameters.
///
/// ⚠ These affect **drawing only** (appearance) and take no part in layout or hit testing
/// (Compose's `graphicsLayer`: the hit area is always the layout bounds). The only displacement
/// hit testing accounts for is scroll (a layout concern); the transforms here
/// (translation/scale/rotate/rotationX/Y/camera) never change the clickable area or the local
/// coordinates of a press point.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphicsLayerParams {
    pub scale_x: f32,
    pub scale_y: f32,
    pub alpha: f32,
    pub translation_x: f32,
    pub translation_y: f32,
    pub rotation_z: f32,
    /// Transform origin (a pivot fraction, 0..1, relative to the node's width and height) —
    /// Compose's `transformOrigin` (default Center, so scale/rotate go around the centre).
    pub transform_origin: TransformOrigin,
    /// Clip to the node's bounds (Compose's `graphicsLayer` `clip`; the convenience form,
    /// `Modifier.alpha`, defaults to clip=true).
    pub clip: bool,
    /// 3D rotation about the X axis (degrees — perspective applies via `camera_distance`).
    pub rotation_x: f32,
    /// 3D rotation about the Y axis (degrees — perspective applies via `camera_distance`).
    pub rotation_y: f32,
    /// 3D camera distance (logical px — the larger it is, the flatter the perspective; Compose
    /// defaults to 8.dp).
    pub camera_distance: f32,
    /// Layer shadow elevation (>0 makes Skia's ShadowUtils draw an ambient plus a spot shadow —
    /// Compose's `graphicsLayer.shadowElevation`).
    ///
    /// The renderer COMPRESSES this value before it becomes Skia's Z: `z = (K+1)·e/(K+e)` with K = 16
    /// (`render::paint::compressed_elevation`), so the visual steps get smaller as elevation grows.
    /// The colour alpha is not compressed. See `docs/draw-layer-extras.md`.
    pub shadow_elevation: f32,
    /// The layer's shadow shape (None = a rectangle).
    pub shadow_shape: Option<Shape>,
    /// Ambient shadow colour (about 10% black by default; Compose's `ambientShadowColor`).
    pub ambient_shadow_color: Color,
    /// Spot shadow colour (about 25% black by default; Compose's `spotShadowColor`).
    pub spot_shadow_color: Color,
    /// Colour filter (Compose's `graphicsLayer` `colorFilter` — the saveLayer paint carries a
    /// colour filter at render time, tinting everything inside the layer; Text/Icon use `Tint` for
    /// animated colour changes).
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

/// The transform origin (Compose's `TransformOrigin`) — pivot fractions relative to the node's
/// width and height (0.0 = left/top, 0.5 = centre, 1.0 = right/bottom).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TransformOrigin(pub f32, pub f32);

/// Shadow parameters (Compose's `graphics.shadow.Shadow` — the configurable drop-shadow set).
/// Drawing follows DropShadowPainter: grow the canvas → draw the shape path (blurred) into an
/// offscreen mask → tint it with SrcIn → translate it onto the canvas by the offset.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ShadowParams {
    /// Blur radius (logical px, Compose's `radius`).
    pub radius: f32,
    /// Spread radius (how much larger the shadow is than the shape — the excess is stroked
    /// separately; Compose's `spread`).
    pub spread: f32,
    /// Shadow offset (Compose's `offset`).
    pub offset_x: f32,
    pub offset_y: f32,
    /// Shadow colour (Compose's `color`, black by default).
    pub color: Color,
    /// Independent opacity, 0-1 (Compose's `alpha`).
    pub alpha: f32,
}

impl ShadowParams {
    /// Convenience constructor (radius/offset/color/alpha; spread = 0).
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
    /// Centre (Compose's default).
    pub const CENTER: Self = Self(0.5, 0.5);
    /// Top-left corner.
    pub const TOP_LEFT: Self = Self(0.0, 0.0);
    /// Bottom-right corner.
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
