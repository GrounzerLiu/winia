//! Drawing payloads the toolkit and the renderer share: what an icon IS (its source, its tint and
//! mirror flags, the variable-font axes) and how an image fills its box.
//!
//! These are not components — they are the data a draw call needs. They lived in
//! `components/icon.rs` and `components/image.rs`, which meant `modifier.rs` (the chain's
//! `DrawIcon`/`DrawImage` elements) and `render.rs` (the drawing itself) had to name a component to
//! describe what they were drawing.

#[cfg(any(
    feature = "material-symbols-outlined",
    feature = "material-symbols-rounded",
    feature = "material-symbols-sharp"
))]
use parking_lot::Mutex;
use std::sync::Arc;
pub(crate) use layer::{DEFAULT_AMBIENT_SHADOW_COLOR, DEFAULT_SPOT_SHADOW_COLOR};
pub use layer::{CompositingStrategy, RenderEffect};

pub mod brush;
pub mod color;
pub mod dyn_value;
pub mod layer;
pub mod shape;

pub use brush::{Brush, BrushSource, BrushTile, GradientBrush};
pub use dyn_value::Dyn;
pub use color::{skia_color, BlendMode, Color, ColorFilter, FilterQuality};
pub use layer::{
    BackgroundColor, BorderColor, GraphicsLayerParams, GraphicsLayerSpec, ShadowParams,
    TransformOrigin,
};
pub use shape::Shape;

use skia_safe::{svg, Rect};
use std::time::{Duration, Instant};
use std::sync::LazyLock;
use std::cell::RefCell;
use std::collections::HashMap;

/// Path fill rule (written into the fill-rule attribute of the wrapping SVG — the path itself is never parsed here).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PathFillType {
    NonZero,
    EvenOdd,
}


mod icon;
mod image;

pub(crate) use icon::*;
pub(crate) use image::*;

pub use icon::{parse_view_box, AxisValue, IconSource, IconSpec, SymbolAxes};
pub use image::{ContentScale, ImageAlignment};

#[cfg(test)]
mod tests;
