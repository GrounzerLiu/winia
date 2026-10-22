//! Wavy Progress Indicator 组件 — 对标 M3 Expressive `LinearWavyProgressIndicator` /
//! `CircularWavyProgressIndicator`（Compose material3 WavyProgressIndicator.kt +
//! internal Linear/CircularWavyProgressModifiers.kt）
//!
//! M3 实现要点（对齐项）：
//! - **两种形态**：Linear（240×10dp，wave 高度 10dp）与 Circular（48dp 圆）；
//! - **颜色**：active = Primary、track = SecondaryContainer（同 flat）；
//! - **描边**：`ActiveThickness/TrackThickness = 4dp`，`StrokeCap = Round`；
//! - **Linear gap/stop**：`TrackActiveSpace = 4dp`、`StopSize = 4dp`；
//! - **振幅**：默认 `progress<=0.1 || progress>=0.95 → 0，否则 1`；
//! - **动画**：waveOffset 无限循环（Linear duration=(wavelength/waveSpeed)*1000；
//!   Circular 再乘顶点数）；振幅过渡 Increasing=Standard、Decreasing=EmphasizedAccelerate；
//!   determinate 进度动画默认 `tween(DurationLong2, LinearEasing)`；
//! - **Linear 波路径**：满幅二次贝塞尔波（半波长交替上下，控制点高 height-stroke.width，
//!   多画两波长供滚动），`PathMeasure` 按 progress 截段；
//! - **Circular 形状**：`RoundedPolygon.circle(numVertices)` 与
//!   `star(numVerticesPerRadius, innerRadius=0.75, rounding=(0.35,0.4),
//!   innerRounding=(0.5))` 归一化后 Morph；amplitude∈(0..1) 用 `Morph.to_path`；
//!   路径重复两圈 + waveOffset 偏移取段 + 绕中心旋转补偿。

use crate::animation::interpolator;
use crate::animation::{AnimationSpec, InfiniteRepeatableSpec, TweenSpec};
use crate::composable;
use crate::runtime::composer::{ComposeCtx, GroupStatus};
use crate::runtime::state::State;
use crate::layout::BoxLayout;
use crate::modifier::{Modifier};
use crate::graphics::{Color, Shape};
use crate::components::progress_indicator::{self, ProgressIndicatorStrokeCap};
use crate::theme::{ThemeColors, WiniaTheme};
use material_shapes::{CornerRounding, Cubic, Morph, MorphToPath, PolygonToPath, RoundedPolygon};
use std::f32::consts::PI;
use std::sync::{Arc, Mutex};
use std::time::Duration;


mod circular;
mod linear;
mod shared;

pub(crate) use linear::*;
pub(crate) use shared::*;
#[cfg(test)]
pub(crate) use circular::*;

pub use circular::CircularWavyProgressIndicator;
pub use linear::LinearWavyProgressIndicator;
pub use shared::{
    WavyProgressIndicatorDefaults, WAVY_ANIMATION_DURATION_MS, WAVY_CIRCULAR_SIZE,
    WAVY_CIRCULAR_WAVELENGTH, WAVY_GAP_SIZE, WAVY_LINEAR_DETERMINATE_WAVELENGTH,
    WAVY_LINEAR_HEIGHT, WAVY_LINEAR_INDETERMINATE_WAVELENGTH, WAVY_LINEAR_STOP_SIZE,
    WAVY_LINEAR_WIDTH, WAVY_STROKE_WIDTH, WAVY_TRACK_STROKE_WIDTH,
};

#[cfg(test)]
mod tests;
