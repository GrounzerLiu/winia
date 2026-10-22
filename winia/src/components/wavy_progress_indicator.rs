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

// ═══════════════════════════════════════════════════════
// 默认值（对标 WavyProgressIndicatorDefaults）
// ═══════════════════════════════════════════════════════

/// Linear 容器宽度（Compose `LinearContainerWidth = 240dp`）
pub const WAVY_LINEAR_WIDTH: f32 = 240.0;
/// Linear 容器高度（`WaveHeight = 10dp`）
pub const WAVY_LINEAR_HEIGHT: f32 = 10.0;
/// Circular 容器尺寸（`WaveSize = 48dp`）
pub const WAVY_CIRCULAR_SIZE: f32 = 48.0;
/// active/track 描边宽度（`ActiveThickness/TrackThickness = 4dp`）
pub const WAVY_STROKE_WIDTH: f32 = 4.0;
pub const WAVY_TRACK_STROKE_WIDTH: f32 = 4.0;
/// indicator 与 track 间隙（`TrackActiveSpace = 4dp`）
pub const WAVY_GAP_SIZE: f32 = 4.0;
/// Linear stop indicator 尺寸（`StopSize = 4dp`）
pub const WAVY_LINEAR_STOP_SIZE: f32 = 4.0;
/// Linear determinate 波长（`ActiveWaveWavelength = 40dp`）
pub const WAVY_LINEAR_DETERMINATE_WAVELENGTH: f32 = 40.0;
/// Linear indeterminate 波长（`IndeterminateActiveWaveWavelength = 20dp`）
pub const WAVY_LINEAR_INDETERMINATE_WAVELENGTH: f32 = 20.0;
/// Circular 波长（`ActiveWaveWavelength = 15dp`）
pub const WAVY_CIRCULAR_WAVELENGTH: f32 = 15.0;
/// 振幅过渡/进度动画时长（`MotionTokens.DurationLong2 = 600ms`）
pub const WAVY_ANIMATION_DURATION_MS: u64 = 600;
/// 最短 wave offset 动画时长（Compose `MinAnimationDuration = 50ms`）
const MIN_WAVE_ANIMATION_MS: u64 = 50;
/// Circular 最少顶点数（Compose `MinCircularVertexCount = 5`）
const MIN_CIRCULAR_VERTEX_COUNT: usize = 5;

/// Wavy 进度指示器默认值。
pub struct WavyProgressIndicatorDefaults;

impl WavyProgressIndicatorDefaults {
    /// active 指示器色（`ProgressIndicatorTokens.ActiveIndicatorColor = Primary`）
    pub fn indicator_color(theme: &ThemeColors) -> Color {
        theme.primary
    }
    /// track 色（`ProgressIndicatorTokens.TrackColor = SecondaryContainer`）
    pub fn track_color(theme: &ThemeColors) -> Color {
        theme.secondary_container
    }
    /// Linear active stroke 宽度
    pub fn linear_stroke_width() -> f32 {
        WAVY_STROKE_WIDTH
    }
    /// Circular active stroke 宽度
    pub fn circular_stroke_width() -> f32 {
        WAVY_STROKE_WIDTH
    }
    /// Linear track stroke 宽度
    pub fn linear_track_stroke_width() -> f32 {
        WAVY_TRACK_STROKE_WIDTH
    }
    /// Circular track stroke 宽度
    pub fn circular_track_stroke_width() -> f32 {
        WAVY_TRACK_STROKE_WIDTH
    }
    /// Linear determinate 波长
    pub fn linear_determinate_wavelength() -> f32 {
        WAVY_LINEAR_DETERMINATE_WAVELENGTH
    }
    /// Linear indeterminate 波长
    pub fn linear_indeterminate_wavelength() -> f32 {
        WAVY_LINEAR_INDETERMINATE_WAVELENGTH
    }
    /// Circular 波长
    pub fn circular_wavelength() -> f32 {
        WAVY_CIRCULAR_WAVELENGTH
    }
    /// determinate 进度动画规格（wavy 是 tween 线性，与 flat 的 Spring 不同）
    pub fn progress_animation_spec() -> AnimationSpec {
        AnimationSpec::Tween(TweenSpec::new(
            Duration::from_millis(WAVY_ANIMATION_DURATION_MS),
            interpolator::Linear::new(),
        ))
    }
    /// 默认振幅函数（Compose `indicatorAmplitude`）
    pub fn indicator_amplitude(progress: f32) -> f32 {
        if progress <= 0.1 || progress >= 0.95 {
            0.0
        } else {
            1.0
        }
    }
}

/// 振幅配置：determinate 用函数，indeterminate 用固定值。
#[derive(Clone)]
enum WavyAmplitude {
    /// 默认 Compose `indicatorAmplitude` 函数
    Indicator,
    /// 自定义函数（determinate）
    Custom(Arc<dyn Fn(f32) -> f32 + Send + Sync>),
    /// 固定值（indeterminate）
    Fixed(f32),
}

impl WavyAmplitude {
    fn resolve(&self, progress: f32) -> f32 {
        let value = match self {
            Self::Indicator => WavyProgressIndicatorDefaults::indicator_amplitude(progress),
            Self::Custom(f) => f(progress),
            Self::Fixed(v) => *v,
        };
        sanitize_amplitude(value)
    }

    fn fixed(&self) -> f32 {
        let value = match self {
            Self::Fixed(v) => *v,
            _ => 1.0,
        };
        sanitize_amplitude(value)
    }

    /// 供 `ctx.changed` 使用的可比较 token：固定值按数值、自定义函数按 Arc 指针。
    fn change_token(&self) -> (u8, u64, usize) {
        match self {
            Self::Indicator => (0, 0, 0),
            Self::Custom(f) => (1, 0, Arc::as_ptr(f) as *const () as usize),
            Self::Fixed(v) => (2, v.to_bits() as u64, 0),
        }
    }
}

fn sanitize_amplitude(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn increasing_amplitude_spec() -> AnimationSpec {
    AnimationSpec::Tween(TweenSpec::new(
        Duration::from_millis(WAVY_ANIMATION_DURATION_MS),
        interpolator::CubicBezier::new(0.2, 0.0, 0.0, 1.0), // Standard
    ))
}

fn decreasing_amplitude_spec() -> AnimationSpec {
    AnimationSpec::Tween(TweenSpec::new(
        Duration::from_millis(WAVY_ANIMATION_DURATION_MS),
        interpolator::CubicBezier::new(0.3, 0.0, 0.8, 0.15), // EmphasizedAccelerate
    ))
}

fn wave_animation_spec(wavelength: f32, wave_speed: f32) -> InfiniteRepeatableSpec {
    let duration_ms = if wave_speed > 0.0 && wavelength > 0.0 {
        ((wavelength / wave_speed) * 1000.0)
            .round()
            .max(MIN_WAVE_ANIMATION_MS as f32) as u64
    } else {
        MIN_WAVE_ANIMATION_MS
    };
    InfiniteRepeatableSpec::restart_tween(
        Duration::from_millis(duration_ms),
        TweenSpec::new(
            Duration::from_millis(duration_ms),
            interpolator::Linear::new(),
        ),
    )
}

fn skia_color(c: Color) -> skia_safe::Color {
    skia_safe::Color::from_argb(c.a, c.r, c.g, c.b)
}

fn paint_stroke(width: f32, cap: ProgressIndicatorStrokeCap, color: Color) -> skia_safe::Paint {
    let mut paint = skia_safe::Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(skia_safe::PaintStyle::Stroke);
    paint.set_stroke_width(width);
    paint.set_stroke_cap(cap.to_skia());
    paint.set_color(skia_color(color));
    paint
}

// ═══════════════════════════════════════════════════════
// LinearWavyProgressIndicator
// ═══════════════════════════════════════════════════════

/// 线性 wavy 进度指示器（对标 Compose `LinearWavyProgressIndicator`）。
///
/// determinate：`LinearWavyProgressIndicator::new(progress)`；
/// indeterminate：`LinearWavyProgressIndicator::indeterminate()`。
#[derive(Clone)]
pub struct LinearWavyProgressIndicator {
    progress: f32,
    indeterminate: bool,
    modifier: Modifier,
    color: Option<Color>,
    track_color: Option<Color>,
    stroke_width: f32,
    track_stroke_width: f32,
    gap_size: f32,
    stop_size: f32,
    amplitude: WavyAmplitude,
    wavelength: f32,
    wave_speed: f32,
}

impl LinearWavyProgressIndicator {
    /// determinate 构造（progress 0..1，越界 clamp）
    pub fn new(progress: f32) -> Self {
        Self {
            progress,
            indeterminate: false,
            modifier: Modifier::new(),
            color: None,
            track_color: None,
            stroke_width: WavyProgressIndicatorDefaults::linear_stroke_width(),
            track_stroke_width: WavyProgressIndicatorDefaults::linear_track_stroke_width(),
            gap_size: WAVY_GAP_SIZE,
            stop_size: WAVY_LINEAR_STOP_SIZE,
            amplitude: WavyAmplitude::Indicator,
            wavelength: WavyProgressIndicatorDefaults::linear_determinate_wavelength(),
            wave_speed: WavyProgressIndicatorDefaults::linear_determinate_wavelength(),
        }
    }

    /// indeterminate 构造：无限 wave + 4 条 head/tail 动画
    pub fn indeterminate() -> Self {
        Self::new(0.0).indeterminate_mode()
    }

    fn indeterminate_mode(mut self) -> Self {
        self.indeterminate = true;
        self.amplitude = WavyAmplitude::Fixed(1.0);
        self.wavelength = WavyProgressIndicatorDefaults::linear_indeterminate_wavelength();
        self.wave_speed = self.wavelength;
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    /// 指示器颜色（默认 Primary）
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    /// 轨道颜色（默认 SecondaryContainer）
    pub fn track_color(mut self, color: Color) -> Self {
        self.track_color = Some(color);
        self
    }

    /// active 描边宽度（dp，默认 4）
    pub fn stroke_width(mut self, width: f32) -> Self {
        self.stroke_width = width.max(0.0);
        self
    }

    /// track 描边宽度（dp，默认 4）
    pub fn track_stroke_width(mut self, width: f32) -> Self {
        self.track_stroke_width = width.max(0.0);
        self
    }

    /// 指示器与轨道间隙（dp，默认 4）
    pub fn gap_size(mut self, gap: f32) -> Self {
        self.gap_size = gap.max(0.0);
        self
    }

    /// stop indicator 尺寸（dp，默认 4）
    pub fn stop_size(mut self, size: f32) -> Self {
        self.stop_size = size.max(0.0);
        self
    }

    /// 自定义振幅函数（determinate；对标 Compose `amplitude` lambda）
    pub fn amplitude_fn(mut self, f: impl Fn(f32) -> f32 + Send + Sync + 'static) -> Self {
        self.amplitude = WavyAmplitude::Custom(Arc::new(f));
        self
    }

    /// 固定振幅（indeterminate；对标 Compose `amplitude: Float`）
    pub fn amplitude(mut self, value: f32) -> Self {
        self.amplitude = WavyAmplitude::Fixed(value);
        self
    }

    /// 波长（dp，默认 determinate 40 / indeterminate 20）
    pub fn wavelength(mut self, value: f32) -> Self {
        self.wavelength = value.max(0.0);
        self
    }

    /// 波速（dp/s，默认 = wavelength，即每秒移动一个波长）
    pub fn wave_speed(mut self, value: f32) -> Self {
        self.wave_speed = value.max(0.0);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        ctx.changed(&self.progress);
        ctx.changed(&self.indeterminate);
        ctx.changed(&self.color);
        ctx.changed(&self.track_color);
        ctx.changed(&self.stroke_width);
        ctx.changed(&self.track_stroke_width);
        ctx.changed(&self.gap_size);
        ctx.changed(&self.stop_size);
        ctx.changed(&self.wavelength);
        ctx.changed(&self.wave_speed);
        let amplitude_token = self.amplitude.change_token();
        ctx.changed(&amplitude_token);

        let key = ctx.next_key();
        let theme = WiniaTheme::colors();
        let color = self
            .color
            .unwrap_or_else(|| WavyProgressIndicatorDefaults::indicator_color(&theme));
        let track_color = self
            .track_color
            .unwrap_or_else(|| WavyProgressIndicatorDefaults::track_color(&theme));
        let stroke_width = self.stroke_width;
        let track_stroke_width = self.track_stroke_width;
        let gap_size = self.gap_size;
        let stop_size = self.stop_size;
        let wavelength = self.wavelength;
        let wave_speed = self.wave_speed;
        let cap = ProgressIndicatorStrokeCap::Round;
        let enable_motion = wave_speed > 0.0 && wavelength > 0.0;

        let shapes_cache = Arc::new(Mutex::new(LinearShapesCache::default()));

        let m = if self.indeterminate {
            let amplitude = self.amplitude.fixed();
            let mut inf = ctx.remember_infinite_transition();
            let wave_offset = inf.animate_float_preserving(
                ctx,
                0.0,
                1.0,
                wave_animation_spec(wavelength, wave_speed),
            );
            if !enable_motion {
                crate::animation::remove_animation_by_state(wave_offset.state_id());
            }
            let fh = inf.animate_float(
                ctx,
                0.0,
                1.0,
                progress_indicator::linear_first_line_head_spec(),
            );
            let ft = inf.animate_float(
                ctx,
                0.0,
                1.0,
                progress_indicator::linear_first_line_tail_spec(),
            );
            let sh = inf.animate_float(
                ctx,
                0.0,
                1.0,
                progress_indicator::linear_second_line_head_spec(),
            );
            let st = inf.animate_float(
                ctx,
                0.0,
                1.0,
                progress_indicator::linear_second_line_tail_spec(),
            );
            let cache = shapes_cache.clone();
            Modifier::new()
                .size(WAVY_LINEAR_WIDTH, WAVY_LINEAR_HEIGHT)
                .clip(Shape::Rectangle)
                .draw_node(LinearWavyIndeterminateNode {
                    color, track_color, stroke_width, track_stroke_width,
                    cap, gap_size, amplitude, wavelength, enable_motion,
                    fh: fh.clone(), ft: ft.clone(), sh: sh.clone(), st: st.clone(),
                    wave_offset: wave_offset.clone(),
                    cache,
                })
        } else {
            let progress = self.progress;
            let amplitude_fn = self.amplitude.clone();
            let amplitude_state: State<f32> = ctx.remember(|| amplitude_fn.resolve(progress));
            let target = amplitude_fn.resolve(progress);
            let current = amplitude_state.peek();
            let spec = if current < target {
                increasing_amplitude_spec()
            } else {
                decreasing_amplitude_spec()
            };
            let mut inf = ctx.remember_infinite_transition();
            let wave_offset = inf.animate_float_preserving(
                ctx,
                0.0,
                1.0,
                wave_animation_spec(wavelength, wave_speed),
            );
            // Compose 仅在真正画波时运行 wave offset 动画：motion 关闭或振幅为 0 时
            // 立即移除刚注册的无限动画（保留 State，绘制时读到 0）。
            let wave_active = enable_motion && (target > 0.0 || current > 0.0);
            if !wave_active {
                crate::animation::remove_animation_by_state(wave_offset.state_id());
            }

            let wave_for_draw = wave_offset.clone();
            if target == 0.0 && current != 0.0 {
                // 振幅从 >0 过渡到 0：动画结束后移除 wave offset 动画，避免持续空转。
                // 注：done 回调仅作兜底；振幅到达 0 后的重组分支也会显式移除。
                let wave_for_stop = wave_offset.clone();
                crate::animation::push_animatable_with_done(
                    amplitude_state.clone(),
                    target,
                    spec,
                    move || {
                        crate::animation::remove_animation_by_state(wave_for_stop.state_id());
                    },
                );
            } else {
                crate::animation::push_animatable(amplitude_state.clone(), target, spec);
            }

            let cache = shapes_cache.clone();
            Modifier::new()
                .size(WAVY_LINEAR_WIDTH, WAVY_LINEAR_HEIGHT)
                .clip(Shape::Rectangle)
                .draw_node(LinearWavyDeterminateNode {
                    color, track_color, stroke_width, track_stroke_width,
                    cap, gap_size, stop_size, progress, wavelength, enable_motion,
                    amplitude_token,
                    amplitude_state: amplitude_state.clone(),
                    wave_offset: wave_for_draw.clone(),
                    cache,
                })
        };


        // Same rule as the plain indicators: the value is what a progress bar IS to a screen reader,
        // and an indeterminate one has none to report.
        let m = m.semantics(match self.indeterminate {
            true => crate::semantics::SemanticsConfig::new()
                .role(crate::semantics::SemanticsRole::ProgressBar),
            false => crate::semantics::SemanticsConfig::new()
                .role(crate::semantics::SemanticsRole::ProgressBar)
                .state(crate::semantics::SemanticsState::new().progress(self.progress, 0.0, 1.0)),
        });

        let m = m.then(self.modifier);
        match ctx.start_restartable_group(key, m, BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {}
        }
        ctx.end_restartable_group();
    }
}

/// Wavy 绘制节点（exp/wavy-node 迁移：原 `.draw` 匿名闭包 ×4 + loading ×1）。
///
/// 照抄 `SliderTrackNode` 形状：具名 struct + `DrawNode`，`draw` 内复用原
/// 绘制函数。静态视觉参数全进 `node_key`；无限动画值（fh/ft/sh/st/wave_offset/
/// global/additional/progress_anim）与振幅过渡值（amplitude_state）渲染期
/// `peek` 直读，不进 key（重绘由动画引擎每帧 `request_redraw` 驱动）。
/// 路径缓存（`Arc<Mutex<ShapesCache>>`）为性能设施，不影响绘制语义，不进 key；
/// cache 含 `skia_safe::Path` 不宜 Debug，故手写 `impl Debug`（只打静态参数）。
/// `amplitude_token`（WavyAmplitude 身份，fn 指针/固定值）进 key——fn 换了即 Enter。
pub(crate) struct LinearWavyIndeterminateNode {
    pub(crate) color: Color,
    pub(crate) track_color: Color,
    pub(crate) stroke_width: f32,
    pub(crate) track_stroke_width: f32,
    pub(crate) cap: ProgressIndicatorStrokeCap,
    pub(crate) gap_size: f32,
    pub(crate) amplitude: f32,
    pub(crate) wavelength: f32,
    pub(crate) enable_motion: bool,
    pub(crate) fh: crate::runtime::state::Visual<f32>,
    pub(crate) ft: crate::runtime::state::Visual<f32>,
    pub(crate) sh: crate::runtime::state::Visual<f32>,
    pub(crate) st: crate::runtime::state::Visual<f32>,
    pub(crate) wave_offset: crate::runtime::state::Visual<f32>,
    pub(crate) cache: Arc<Mutex<LinearShapesCache>>,
}

impl std::fmt::Debug for LinearWavyIndeterminateNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinearWavyIndeterminateNode")
            .field("color", &self.color)
            .field("track_color", &self.track_color)
            .field("stroke_width", &self.stroke_width)
            .field("track_stroke_width", &self.track_stroke_width)
            .field("cap", &self.cap)
            .field("gap_size", &self.gap_size)
            .field("amplitude", &self.amplitude)
            .field("wavelength", &self.wavelength)
            .field("enable_motion", &self.enable_motion)
            .finish_non_exhaustive()
    }
}

impl crate::modifier::DrawNode for LinearWavyIndeterminateNode {
    fn draw(&self, canvas: &skia_safe::Canvas, rect: skia_safe::Rect) {
        draw_linear_wavy_indeterminate(
            canvas, rect,
            self.color, self.track_color,
            self.stroke_width, self.track_stroke_width,
            self.cap, self.gap_size,
            self.amplitude, self.wavelength, self.enable_motion,
            self.fh.peek(), self.ft.peek(), self.sh.peek(), self.st.peek(),
            self.wave_offset.peek(),
            &self.cache,
        );
    }
    fn node_key(&self) -> String {
        format!(
            "wavy-linear-indet:{:?}:{:?}:{}:{}:{}:{}:{}:{}:{}",
            self.color,
            self.track_color,
            self.stroke_width.to_bits(),
            self.track_stroke_width.to_bits(),
            self.cap as u8,
            self.gap_size.to_bits(),
            self.amplitude.to_bits(),
            self.wavelength.to_bits(),
            self.enable_motion,
        )
    }
}

/// Linear determinate wavy 节点（振幅过渡值 peek，不进 key；fn 身份 token 进 key）。
pub(crate) struct LinearWavyDeterminateNode {
    pub(crate) color: Color,
    pub(crate) track_color: Color,
    pub(crate) stroke_width: f32,
    pub(crate) track_stroke_width: f32,
    pub(crate) cap: ProgressIndicatorStrokeCap,
    pub(crate) gap_size: f32,
    pub(crate) stop_size: f32,
    pub(crate) progress: f32,
    pub(crate) wavelength: f32,
    pub(crate) enable_motion: bool,
    pub(crate) amplitude_token: (u8, u64, usize),
    pub(crate) amplitude_state: State<f32>,
    pub(crate) wave_offset: crate::runtime::state::Visual<f32>,
    pub(crate) cache: Arc<Mutex<LinearShapesCache>>,
}

impl std::fmt::Debug for LinearWavyDeterminateNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinearWavyDeterminateNode")
            .field("color", &self.color)
            .field("track_color", &self.track_color)
            .field("stroke_width", &self.stroke_width)
            .field("track_stroke_width", &self.track_stroke_width)
            .field("cap", &self.cap)
            .field("gap_size", &self.gap_size)
            .field("stop_size", &self.stop_size)
            .field("progress", &self.progress)
            .field("wavelength", &self.wavelength)
            .field("enable_motion", &self.enable_motion)
            .field("amplitude_token", &self.amplitude_token)
            .finish_non_exhaustive()
    }
}

impl crate::modifier::DrawNode for LinearWavyDeterminateNode {
    fn draw(&self, canvas: &skia_safe::Canvas, rect: skia_safe::Rect) {
        draw_linear_wavy_determinate(
            canvas, rect,
            self.color, self.track_color,
            self.stroke_width, self.track_stroke_width,
            self.cap, self.gap_size, self.stop_size,
            self.progress,
            self.amplitude_state.peek(),
            self.wavelength,
            self.wave_offset.peek(),
            true,
            self.enable_motion,
            &self.cache,
        );
    }
    fn node_key(&self) -> String {
        format!(
            "wavy-linear-det:{:?}:{:?}:{}:{}:{}:{}:{}:{}:{}:{}:{:?}",
            self.color,
            self.track_color,
            self.stroke_width.to_bits(),
            self.track_stroke_width.to_bits(),
            self.cap as u8,
            self.gap_size.to_bits(),
            self.stop_size.to_bits(),
            self.progress.to_bits(),
            self.wavelength.to_bits(),
            self.enable_motion,
            self.amplitude_token,
        )
    }
}

/// Linear determinate wavy 绘制。
#[allow(clippy::too_many_arguments)]
fn draw_linear_wavy_determinate(
    canvas: &skia_safe::Canvas,
    rect: skia_safe::Rect,
    color: Color,
    track_color: Color,
    stroke_width: f32,
    track_stroke_width: f32,
    cap: ProgressIndicatorStrokeCap,
    gap_size: f32,
    stop_size: f32,
    progress: f32,
    amplitude: f32,
    wavelength: f32,
    wave_offset: f32,
    draw_stop: bool,
    enable_motion: bool,
    cache: &Mutex<LinearShapesCache>,
) {
    let fractions = [0.0f32, progress.clamp(0.0, 1.0)];
    let (track_path, progress_paths) = {
        let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
        build_linear_wavy_paths(
            &mut cache,
            rect,
            wavelength,
            &fractions,
            amplitude,
            wave_offset,
            gap_size,
            stroke_width,
            track_stroke_width,
            cap,
            enable_motion,
        )
    };

    canvas.save();
    canvas.translate((rect.left, rect.top));
    draw_linear_wavy_paths(
        canvas,
        rect,
        &track_path,
        &progress_paths,
        color,
        track_color,
        stroke_width,
        track_stroke_width,
        cap,
    );

    if draw_stop {
        draw_linear_stop_indicator(
            canvas,
            rect,
            fractions[1],
            stop_size,
            cap,
            stroke_width,
            track_stroke_width,
            color,
        );
    }
    canvas.restore();
}

/// Linear indeterminate wavy 绘制（4 条 head/tail 段）。
#[allow(clippy::too_many_arguments)]
fn draw_linear_wavy_indeterminate(
    canvas: &skia_safe::Canvas,
    rect: skia_safe::Rect,
    color: Color,
    track_color: Color,
    stroke_width: f32,
    track_stroke_width: f32,
    cap: ProgressIndicatorStrokeCap,
    gap_size: f32,
    amplitude: f32,
    wavelength: f32,
    enable_motion: bool,
    first_head: f32,
    first_tail: f32,
    second_head: f32,
    second_tail: f32,
    wave_offset: f32,
    cache: &Mutex<LinearShapesCache>,
) {
    let fractions = [first_tail, first_head, second_tail, second_head];
    let (track_path, progress_paths) = {
        let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
        build_linear_wavy_paths(
            &mut cache,
            rect,
            wavelength,
            &fractions,
            amplitude,
            wave_offset,
            gap_size,
            stroke_width,
            track_stroke_width,
            cap,
            enable_motion,
        )
    };

    canvas.save();
    canvas.translate((rect.left, rect.top));
    draw_linear_wavy_paths(
        canvas,
        rect,
        &track_path,
        &progress_paths,
        color,
        track_color,
        stroke_width,
        track_stroke_width,
        cap,
    );
    canvas.restore();
}

/// Linear 波路径缓存：按 size/wavelength/stroke/cap/振幅是否为零 重建满幅波路径。
/// 避免每帧重新构造二次贝塞尔波和 PathMeasure（对标 Compose `LinearProgressDrawingCache`）。
#[derive(Default)]
pub(crate) struct LinearShapesCache {
    key: Option<(f32, f32, f32, f32, f32, ProgressIndicatorStrokeCap, bool)>,
    full_path: skia_safe::Path,
    full_path_length: f32,
    full_bounds_width: f32,
}

impl LinearShapesCache {
    fn update(
        &mut self,
        width: f32,
        height: f32,
        wavelength: f32,
        stroke_width: f32,
        track_stroke_width: f32,
        cap: ProgressIndicatorStrokeCap,
        amplitude_zero: bool,
    ) {
        let key = (
            width,
            height,
            wavelength,
            stroke_width,
            track_stroke_width,
            cap,
            amplitude_zero,
        );
        if self.key == Some(key) {
            return;
        }

        let mut full_builder = skia_safe::PathBuilder::new();
        full_builder.move_to((0.0, 0.0));
        if amplitude_zero || wavelength <= 0.0 {
            full_builder.line_to((width, 0.0));
        } else {
            let half_wavelength = wavelength / 2.0;
            let mut anchor_x = half_wavelength;
            let mut control_x = half_wavelength / 2.0;
            let mut control_y = height - stroke_width;
            let width_with_extra = width + wavelength * 2.0;
            while anchor_x <= width_with_extra {
                full_builder.quad_to((control_x, control_y), (anchor_x, 0.0));
                anchor_x += half_wavelength;
                control_x += half_wavelength;
                control_y *= -1.0;
            }
        }
        full_builder.offset((0.0, height / 2.0));
        let full_path = full_builder.detach();

        let mut measure = skia_safe::PathMeasure::new(&full_path, false, None);
        let full_path_length = measure.length();
        let full_bounds_width = full_path.bounds().width().max(0.00000001);

        self.key = Some(key);
        self.full_path = full_path;
        self.full_path_length = full_path_length;
        self.full_bounds_width = full_bounds_width;
    }
}

/// 构造 Linear wavy 的 track 路径与各 progress 段路径。
fn build_linear_wavy_paths(
    cache: &mut LinearShapesCache,
    rect: skia_safe::Rect,
    wavelength: f32,
    progress_fractions: &[f32],
    amplitude: f32,
    wave_offset: f32,
    gap_size: f32,
    stroke_width: f32,
    track_stroke_width: f32,
    cap: ProgressIndicatorStrokeCap,
    enable_motion: bool,
) -> (skia_safe::Path, Vec<skia_safe::Path>) {
    let width = rect.width();
    let height = rect.height();
    if width <= 0.0 || height <= 0.0 {
        return (skia_safe::Path::new(), Vec::new());
    }

    let current_stroke_cap_width = if cap == ProgressIndicatorStrokeCap::Butt || height > width {
        0.0
    } else {
        stroke_width.max(track_stroke_width) / 2.0
    };

    // 满幅波路径（按 size/wavelength/stroke/振幅是否为零 缓存，避免每帧重建）
    cache.update(
        width,
        height,
        wavelength,
        stroke_width,
        track_stroke_width,
        cap,
        amplitude == 0.0,
    );
    let mut measure = skia_safe::PathMeasure::new(&cache.full_path, false, None);
    let full_path_length = cache.full_path_length;
    let progress_path_scale = full_path_length / cache.full_bounds_width;

    let half_height = height / 2.0;
    let mut track_builder = skia_safe::PathBuilder::new();
    let mut next_end_track_offset = width - current_stroke_cap_width;
    track_builder.move_to((next_end_track_offset, half_height));

    let mut adjusted_track_gap = gap_size;
    let mut active_indicator_visible = false;
    let mut progress_paths = Vec::with_capacity(progress_fractions.len() / 2);

    for (i, pair) in progress_fractions.chunks_exact(2).enumerate() {
        let start_fraction = pair[0];
        let end_fraction = pair[1];
        let bar_tail = start_fraction * width;
        let bar_head = end_fraction * width;

        if i == 0 {
            adjusted_track_gap = if bar_head < current_stroke_cap_width {
                0.0
            } else {
                (bar_head - current_stroke_cap_width).min(gap_size)
            };
            active_indicator_visible = bar_head >= current_stroke_cap_width;
        }

        let adjusted_bar_head =
            bar_head.clamp(current_stroke_cap_width, width - current_stroke_cap_width);
        let adjusted_bar_tail =
            bar_tail.clamp(current_stroke_cap_width, width - current_stroke_cap_width);

        if (end_fraction - start_fraction).abs() > 0.0 {
            let wave_shift = if amplitude != 0.0 && enable_motion {
                wave_offset.rem_euclid(1.0) * wavelength
            } else {
                0.0
            };
            let mut seg_builder = skia_safe::PathBuilder::new();
            measure.get_segment(
                (adjusted_bar_tail + wave_shift) * progress_path_scale,
                (adjusted_bar_head + wave_shift) * progress_path_scale,
                &mut seg_builder,
                true,
            );
            let mut seg_path = seg_builder.detach();
            let dx = if wave_shift > 0.0 { -wave_shift } else { 0.0 };
            let matrix = skia_safe::Matrix::scale_translate(
                (1.0, amplitude),
                (dx, (1.0 - amplitude) * half_height),
            );
            seg_path = seg_path.make_transform(&matrix);
            progress_paths.push(seg_path);
        }

        let adaptive_track_spacing = if active_indicator_visible {
            adjusted_track_gap + current_stroke_cap_width * 2.0
        } else {
            adjusted_track_gap
        };
        if next_end_track_offset > adjusted_bar_head + adaptive_track_spacing {
            track_builder.line_to((
                (adjusted_bar_head + adaptive_track_spacing).max(current_stroke_cap_width),
                half_height,
            ));
        }
        if bar_head > bar_tail {
            next_end_track_offset =
                (adjusted_bar_tail - adaptive_track_spacing).max(current_stroke_cap_width);
            track_builder.move_to((next_end_track_offset, half_height));
        }
    }

    if next_end_track_offset > current_stroke_cap_width {
        track_builder.line_to((current_stroke_cap_width, half_height));
    }
    let track_path = track_builder.detach();

    (track_path, progress_paths)
}

fn draw_linear_wavy_paths(
    canvas: &skia_safe::Canvas,
    _rect: skia_safe::Rect,
    track_path: &skia_safe::Path,
    progress_paths: &[skia_safe::Path],
    color: Color,
    track_color: Color,
    stroke_width: f32,
    track_stroke_width: f32,
    cap: ProgressIndicatorStrokeCap,
) {
    if track_color.a != 0 {
        let track_paint = paint_stroke(track_stroke_width, cap, track_color);
        canvas.draw_path(track_path, &track_paint);
    }
    if color.a != 0 {
        let progress_paint = paint_stroke(stroke_width, cap, color);
        for path in progress_paths {
            canvas.draw_path(path, &progress_paint);
        }
    }
}

/// Linear stop indicator（对标 Compose `drawStopIndicator`）。
fn draw_linear_stop_indicator(
    canvas: &skia_safe::Canvas,
    rect: skia_safe::Rect,
    progress_end: f32,
    max_stop_size: f32,
    cap: ProgressIndicatorStrokeCap,
    stroke_width: f32,
    track_stroke_width: f32,
    color: Color,
) {
    let width = rect.width();
    let height = rect.height();
    if width <= 0.0 || height <= 0.0 {
        return;
    }
    let mut stop_size = track_stroke_width.min(max_stop_size);
    let indicator_x_offset = if stop_size == track_stroke_width {
        0.0
    } else {
        track_stroke_width / 4.0
    };
    let mut indicator_x = width - stop_size - indicator_x_offset;
    let cap_width = stroke_cap_width(cap, stroke_width, track_stroke_width, width, height);
    let progress_x = width * progress_end + cap_width;
    if indicator_x <= progress_x {
        stop_size = (stop_size - (progress_x - indicator_x)).max(0.0);
        indicator_x = progress_x;
    }
    if stop_size > 0.0 {
        let mut paint = skia_safe::Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(skia_color(color));
        let cy = height / 2.0;
        if cap == ProgressIndicatorStrokeCap::Round {
            canvas.draw_circle(
                skia_safe::Point::new(indicator_x + stop_size / 2.0, cy),
                stop_size / 2.0,
                &paint,
            );
        } else {
            let r =
                skia_safe::Rect::from_xywh(indicator_x, cy - stop_size / 2.0, stop_size, stop_size);
            canvas.draw_rect(r, &paint);
        }
    }
}

fn stroke_cap_width(
    cap: ProgressIndicatorStrokeCap,
    stroke_width: f32,
    track_stroke_width: f32,
    width: f32,
    height: f32,
) -> f32 {
    if cap == ProgressIndicatorStrokeCap::Butt || height > width {
        0.0
    } else {
        stroke_width.max(track_stroke_width) / 2.0
    }
}

// ═══════════════════════════════════════════════════════
// CircularWavyProgressIndicator
// ═══════════════════════════════════════════════════════

/// 圆形 wavy 进度指示器（对标 Compose `CircularWavyProgressIndicator`）。
///
/// determinate：`CircularWavyProgressIndicator::new(progress)`；
/// indeterminate：`CircularWavyProgressIndicator::indeterminate()`。
#[derive(Clone)]
pub struct CircularWavyProgressIndicator {
    progress: f32,
    indeterminate: bool,
    modifier: Modifier,
    color: Option<Color>,
    track_color: Option<Color>,
    stroke_width: f32,
    track_stroke_width: f32,
    gap_size: f32,
    amplitude: WavyAmplitude,
    wavelength: f32,
    wave_speed: f32,
}

impl CircularWavyProgressIndicator {
    /// determinate 构造（progress 0..1）
    pub fn new(progress: f32) -> Self {
        Self {
            progress,
            indeterminate: false,
            modifier: Modifier::new(),
            color: None,
            track_color: None,
            stroke_width: WavyProgressIndicatorDefaults::circular_stroke_width(),
            track_stroke_width: WavyProgressIndicatorDefaults::circular_track_stroke_width(),
            gap_size: WAVY_GAP_SIZE,
            amplitude: WavyAmplitude::Indicator,
            wavelength: WavyProgressIndicatorDefaults::circular_wavelength(),
            wave_speed: WavyProgressIndicatorDefaults::circular_wavelength(),
        }
    }

    /// indeterminate 构造：无限旋转 + 进度呼吸 + wave 滚动
    pub fn indeterminate() -> Self {
        Self::new(0.0).indeterminate_mode()
    }

    fn indeterminate_mode(mut self) -> Self {
        self.indeterminate = true;
        self.amplitude = WavyAmplitude::Fixed(1.0);
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    /// 指示器颜色（默认 Primary）
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    /// 轨道颜色（默认 SecondaryContainer；indeterminate 也会绘制 track，与 Compose 一致）
    pub fn track_color(mut self, color: Color) -> Self {
        self.track_color = Some(color);
        self
    }

    /// active 描边宽度（dp，默认 4）
    pub fn stroke_width(mut self, width: f32) -> Self {
        self.stroke_width = width.max(0.0);
        self
    }

    /// track 描边宽度（dp，默认 4）
    pub fn track_stroke_width(mut self, width: f32) -> Self {
        self.track_stroke_width = width.max(0.0);
        self
    }

    /// 指示器与轨道间隙（dp，默认 4）
    pub fn gap_size(mut self, gap: f32) -> Self {
        self.gap_size = gap.max(0.0);
        self
    }

    /// 自定义振幅函数（determinate）
    pub fn amplitude_fn(mut self, f: impl Fn(f32) -> f32 + Send + Sync + 'static) -> Self {
        self.amplitude = WavyAmplitude::Custom(Arc::new(f));
        self
    }

    /// 固定振幅（indeterminate）
    pub fn amplitude(mut self, value: f32) -> Self {
        self.amplitude = WavyAmplitude::Fixed(value);
        self
    }

    /// 波长（dp，默认 15）
    pub fn wavelength(mut self, value: f32) -> Self {
        self.wavelength = value.max(0.0);
        self
    }

    /// 波速（dp/s，默认 = wavelength）
    pub fn wave_speed(mut self, value: f32) -> Self {
        self.wave_speed = value.max(0.0);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        ctx.changed(&self.progress);
        ctx.changed(&self.indeterminate);
        ctx.changed(&self.color);
        ctx.changed(&self.track_color);
        ctx.changed(&self.stroke_width);
        ctx.changed(&self.track_stroke_width);
        ctx.changed(&self.gap_size);
        ctx.changed(&self.wavelength);
        ctx.changed(&self.wave_speed);
        let amplitude_token = self.amplitude.change_token();
        ctx.changed(&amplitude_token);

        let key = ctx.next_key();
        let theme = WiniaTheme::colors();
        let color = self
            .color
            .unwrap_or_else(|| WavyProgressIndicatorDefaults::indicator_color(&theme));
        let track_color = self
            .track_color
            .unwrap_or_else(|| WavyProgressIndicatorDefaults::track_color(&theme));
        let stroke_width = self.stroke_width;
        let track_stroke_width = self.track_stroke_width;
        let gap_size = self.gap_size;
        let wavelength = self.wavelength;
        let wave_speed = self.wave_speed;
        let cap = ProgressIndicatorStrokeCap::Round;
        let enable_motion = wave_speed > 0.0 && wavelength > 0.0;

        let shapes_cache = Arc::new(Mutex::new(CircularShapesCache::default()));

        let m = if self.indeterminate {
            let amplitude = self.amplitude.fixed();
            let duration_ms =
                circular_wave_duration_ms(wavelength, wave_speed, WAVY_CIRCULAR_SIZE, stroke_width);
            let mut inf = ctx.remember_infinite_transition();
            let wave_offset = inf.animate_float_preserving(
                ctx,
                0.0,
                1.0,
                wave_animation_spec_duration(duration_ms),
            );
            if !enable_motion {
                crate::animation::remove_animation_by_state(wave_offset.state_id());
            }
            let global = inf.animate_float(
                ctx,
                0.0,
                1080.0,
                progress_indicator::circular_global_rotation_spec(),
            );
            let additional = inf.animate_float(
                ctx,
                0.0,
                360.0,
                progress_indicator::circular_additional_rotation_spec(),
            );
            let progress_anim =
                inf.animate_float(ctx, 0.1, 0.87, progress_indicator::circular_progress_spec());
            Modifier::new()
                .size(WAVY_CIRCULAR_SIZE, WAVY_CIRCULAR_SIZE)
                .draw_node(CircularWavyIndeterminateNode {
                    color, track_color, stroke_width, track_stroke_width,
                    cap, gap_size, amplitude, wavelength, enable_motion,
                    wave_offset: wave_offset.clone(),
                    global: global.clone(),
                    additional: additional.clone(),
                    progress_anim: progress_anim.clone(),
                    cache: shapes_cache.clone(),
                })
        } else {
            let progress = self.progress;
            let amplitude_fn = self.amplitude.clone();
            let amplitude_state: State<f32> = ctx.remember(|| amplitude_fn.resolve(progress));
            let target = amplitude_fn.resolve(progress);
            let current = amplitude_state.peek();
            let spec = if current < target {
                increasing_amplitude_spec()
            } else {
                decreasing_amplitude_spec()
            };
            let duration_ms =
                circular_wave_duration_ms(wavelength, wave_speed, WAVY_CIRCULAR_SIZE, stroke_width);
            let mut inf = ctx.remember_infinite_transition();
            let wave_offset = inf.animate_float_preserving(
                ctx,
                0.0,
                1.0,
                wave_animation_spec_duration(duration_ms),
            );
            // Compose 仅在真正画波时运行 wave offset 动画（同 Linear）。
            let wave_active = enable_motion && (target > 0.0 || current > 0.0);
            if !wave_active {
                crate::animation::remove_animation_by_state(wave_offset.state_id());
            }

            let wave_for_draw = wave_offset.clone();
            if target == 0.0 && current != 0.0 {
                // 同 Linear：done 回调仅作兜底，重组分支也会移除 wave 动画。
                let wave_for_stop = wave_offset.clone();
                crate::animation::push_animatable_with_done(
                    amplitude_state.clone(),
                    target,
                    spec,
                    move || {
                        crate::animation::remove_animation_by_state(wave_for_stop.state_id());
                    },
                );
            } else {
                crate::animation::push_animatable(amplitude_state.clone(), target, spec);
            }

            Modifier::new()
                .size(WAVY_CIRCULAR_SIZE, WAVY_CIRCULAR_SIZE)
                .draw_node(CircularWavyDeterminateNode {
                    color, track_color, stroke_width, track_stroke_width,
                    cap, gap_size, progress, wavelength, enable_motion,
                    amplitude_token,
                    amplitude_state: amplitude_state.clone(),
                    wave_offset: wave_for_draw.clone(),
                    cache: shapes_cache.clone(),
                })
        };


        // Same rule as the plain indicators: the value is what a progress bar IS to a screen reader,
        // and an indeterminate one has none to report.
        let m = m.semantics(match self.indeterminate {
            true => crate::semantics::SemanticsConfig::new()
                .role(crate::semantics::SemanticsRole::ProgressBar),
            false => crate::semantics::SemanticsConfig::new()
                .role(crate::semantics::SemanticsRole::ProgressBar)
                .state(crate::semantics::SemanticsState::new().progress(self.progress, 0.0, 1.0)),
        });

        let m = m.then(self.modifier);
        match ctx.start_restartable_group(key, m, BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {}
        }
        ctx.end_restartable_group();
    }
}

/// Circular wave offset 动画时长（Compose：`(wavelength/waveSpeed)*1000*vertexCount`）。
fn circular_wave_duration_ms(
    wavelength: f32,
    wave_speed: f32,
    size: f32,
    stroke_width: f32,
) -> u64 {
    if wave_speed <= 0.0 || wavelength <= 0.0 {
        return MIN_WAVE_ANIMATION_MS;
    }
    let r = size / 2.0 - stroke_width / 2.0;
    let num_vertices =
        ((2.0 * PI * r / wavelength).round() as usize).max(MIN_CIRCULAR_VERTEX_COUNT);
    (((wavelength / wave_speed) * 1000.0) * num_vertices as f32)
        .round()
        .max(MIN_WAVE_ANIMATION_MS as f32) as u64
}

fn wave_animation_spec_duration(duration_ms: u64) -> InfiniteRepeatableSpec {
    InfiniteRepeatableSpec::restart_tween(
        Duration::from_millis(duration_ms),
        TweenSpec::new(
            Duration::from_millis(duration_ms),
            interpolator::Linear::new(),
        ),
    )
}

/// Circular determinate wavy 绘制。
#[allow(clippy::too_many_arguments)]
fn draw_circular_wavy_determinate(
    canvas: &skia_safe::Canvas,
    rect: skia_safe::Rect,
    color: Color,
    track_color: Color,
    stroke_width: f32,
    track_stroke_width: f32,
    cap: ProgressIndicatorStrokeCap,
    gap_size: f32,
    progress: f32,
    amplitude: f32,
    wavelength: f32,
    enable_motion: bool,
    wave_offset: f32,
    cache: &Mutex<CircularShapesCache>,
) {
    let progress = progress.clamp(0.0, 1.0);
    let mut shapes = cache.lock().unwrap_or_else(|e| e.into_inner());
    let (track_path, progress_path) = build_circular_wavy_paths(
        &mut shapes,
        rect,
        wavelength,
        stroke_width,
        track_stroke_width,
        cap,
        gap_size,
        0.0,
        progress,
        amplitude,
        wave_offset,
        enable_motion,
    );
    canvas.save();
    canvas.translate((rect.left, rect.top));
    draw_circular_wavy_paths(
        canvas,
        rect,
        &track_path,
        &progress_path,
        color,
        track_color,
        stroke_width,
        track_stroke_width,
        cap,
    );
    canvas.restore();
}

/// Circular indeterminate wavy 绘制。
#[allow(clippy::too_many_arguments)]
/// Circular indeterminate/determinate wavy 节点。
pub(crate) struct CircularWavyIndeterminateNode {
    pub(crate) color: Color,
    pub(crate) track_color: Color,
    pub(crate) stroke_width: f32,
    pub(crate) track_stroke_width: f32,
    pub(crate) cap: ProgressIndicatorStrokeCap,
    pub(crate) gap_size: f32,
    pub(crate) amplitude: f32,
    pub(crate) wavelength: f32,
    pub(crate) enable_motion: bool,
    pub(crate) wave_offset: crate::runtime::state::Visual<f32>,
    pub(crate) global: crate::runtime::state::Visual<f32>,
    pub(crate) additional: crate::runtime::state::Visual<f32>,
    pub(crate) progress_anim: crate::runtime::state::Visual<f32>,
    pub(crate) cache: Arc<Mutex<CircularShapesCache>>,
}

impl std::fmt::Debug for CircularWavyIndeterminateNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CircularWavyIndeterminateNode")
            .field("color", &self.color)
            .field("track_color", &self.track_color)
            .field("stroke_width", &self.stroke_width)
            .field("track_stroke_width", &self.track_stroke_width)
            .field("cap", &self.cap)
            .field("gap_size", &self.gap_size)
            .field("amplitude", &self.amplitude)
            .field("wavelength", &self.wavelength)
            .field("enable_motion", &self.enable_motion)
            .finish_non_exhaustive()
    }
}

impl crate::modifier::DrawNode for CircularWavyIndeterminateNode {
    fn draw(&self, canvas: &skia_safe::Canvas, rect: skia_safe::Rect) {
        draw_circular_wavy_indeterminate(
            canvas, rect,
            self.color, self.track_color,
            self.stroke_width, self.track_stroke_width,
            self.cap, self.gap_size,
            self.amplitude, self.wavelength, self.enable_motion,
            self.wave_offset.peek(),
            self.global.peek(), self.additional.peek(), self.progress_anim.peek(),
            &self.cache,
        );
    }
    fn node_key(&self) -> String {
        format!(
            "wavy-circular-indet:{:?}:{:?}:{}:{}:{}:{}:{}:{}:{}",
            self.color,
            self.track_color,
            self.stroke_width.to_bits(),
            self.track_stroke_width.to_bits(),
            self.cap as u8,
            self.gap_size.to_bits(),
            self.amplitude.to_bits(),
            self.wavelength.to_bits(),
            self.enable_motion,
        )
    }
}

pub(crate) struct CircularWavyDeterminateNode {
    pub(crate) color: Color,
    pub(crate) track_color: Color,
    pub(crate) stroke_width: f32,
    pub(crate) track_stroke_width: f32,
    pub(crate) cap: ProgressIndicatorStrokeCap,
    pub(crate) gap_size: f32,
    pub(crate) progress: f32,
    pub(crate) wavelength: f32,
    pub(crate) enable_motion: bool,
    pub(crate) amplitude_token: (u8, u64, usize),
    pub(crate) amplitude_state: State<f32>,
    pub(crate) wave_offset: crate::runtime::state::Visual<f32>,
    pub(crate) cache: Arc<Mutex<CircularShapesCache>>,
}

impl std::fmt::Debug for CircularWavyDeterminateNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CircularWavyDeterminateNode")
            .field("color", &self.color)
            .field("track_color", &self.track_color)
            .field("stroke_width", &self.stroke_width)
            .field("track_stroke_width", &self.track_stroke_width)
            .field("cap", &self.cap)
            .field("gap_size", &self.gap_size)
            .field("progress", &self.progress)
            .field("wavelength", &self.wavelength)
            .field("enable_motion", &self.enable_motion)
            .field("amplitude_token", &self.amplitude_token)
            .finish_non_exhaustive()
    }
}

impl crate::modifier::DrawNode for CircularWavyDeterminateNode {
    fn draw(&self, canvas: &skia_safe::Canvas, rect: skia_safe::Rect) {
        draw_circular_wavy_determinate(
            canvas, rect,
            self.color, self.track_color,
            self.stroke_width, self.track_stroke_width,
            self.cap, self.gap_size,
            self.progress,
            self.amplitude_state.peek(),
            self.wavelength, self.enable_motion,
            self.wave_offset.peek(),
            &self.cache,
        );
    }
    fn node_key(&self) -> String {
        format!(
            "wavy-circular-det:{:?}:{:?}:{}:{}:{}:{}:{}:{}:{}:{:?}",
            self.color,
            self.track_color,
            self.stroke_width.to_bits(),
            self.track_stroke_width.to_bits(),
            self.cap as u8,
            self.gap_size.to_bits(),
            self.progress.to_bits(),
            self.wavelength.to_bits(),
            self.enable_motion,
            self.amplitude_token,
        )
    }
}

fn draw_circular_wavy_indeterminate(
    canvas: &skia_safe::Canvas,
    rect: skia_safe::Rect,
    color: Color,
    track_color: Color,
    stroke_width: f32,
    track_stroke_width: f32,
    cap: ProgressIndicatorStrokeCap,
    gap_size: f32,
    amplitude: f32,
    wavelength: f32,
    enable_motion: bool,
    wave_offset: f32,
    global_rotation: f32,
    additional_rotation: f32,
    progress: f32,
    cache: &Mutex<CircularShapesCache>,
) {
    let mut shapes = cache.lock().unwrap_or_else(|e| e.into_inner());
    let (track_path, progress_path) = build_circular_wavy_paths(
        &mut shapes,
        rect,
        wavelength,
        stroke_width,
        track_stroke_width,
        cap,
        gap_size,
        0.0,
        progress,
        amplitude,
        wave_offset,
        enable_motion,
    );
    let cx = rect.width() / 2.0;
    let cy = rect.height() / 2.0;
    canvas.save();
    canvas.translate((rect.left, rect.top));
    // 保持与 Compose 参考一致：indeterminate 在 start_angle=270°（12 点）基础上
    // 再叠加 +90°，因此初始基准方向为右侧（3 点）；determinate 不叠加，起点在 12 点。
    canvas.rotate(
        global_rotation + additional_rotation + 90.0,
        Some(skia_safe::Point::new(cx, cy)),
    );
    draw_circular_wavy_paths(
        canvas,
        rect,
        &track_path,
        &progress_path,
        color,
        track_color,
        stroke_width,
        track_stroke_width,
        cap,
    );
    canvas.restore();
}

/// 构造 Circular wavy 的 track 与 progress 路径。
#[allow(clippy::too_many_arguments)]
fn build_circular_wavy_paths(
    shapes: &mut CircularShapesCache,
    rect: skia_safe::Rect,
    wavelength: f32,
    stroke_width: f32,
    track_stroke_width: f32,
    cap: ProgressIndicatorStrokeCap,
    gap_size: f32,
    start_progress: f32,
    end_progress: f32,
    amplitude: f32,
    wave_offset: f32,
    enable_motion: bool,
) -> (skia_safe::Path, skia_safe::Path) {
    let width = rect.width();
    let height = rect.height();
    if width <= 0.0 || height <= 0.0 || wavelength <= 0.0 {
        return (skia_safe::Path::new(), skia_safe::Path::new());
    }

    let cap_width = stroke_cap_width(cap, stroke_width, track_stroke_width, width, height);

    // 形状缓存（RoundedPolygon）
    shapes.update(width, height, wavelength, stroke_width);

    let mut full_progress_path = shapes.get_progress_path(amplitude, enable_motion);
    process_circular_path(&mut full_progress_path, width, height, stroke_width);

    let mut progress_measure = skia_safe::PathMeasure::new(&full_progress_path, true, None);
    let progress_path_length = if enable_motion {
        progress_measure.length() / 2.0
    } else {
        progress_measure.length()
    };

    let mut full_track_path = shapes.get_track_path();
    process_circular_path(&mut full_track_path, width, height, stroke_width);
    let mut track_measure = skia_safe::PathMeasure::new(&full_track_path, true, None);
    let track_path_length = track_measure.length();

    let p_start = start_progress * progress_path_length;
    let p_stop = end_progress * progress_path_length;

    let track_gap_size = p_stop.min(gap_size);
    let horizontal_insets = p_stop.min(cap_width);
    let track_spacing = horizontal_insets * 2.0 + track_gap_size;

    let mut progress_builder = skia_safe::PathBuilder::new();
    if enable_motion {
        let coerced_wave_offset = wave_offset.rem_euclid(1.0);
        let shift = coerced_wave_offset * progress_path_length;
        progress_measure.get_segment(p_start + shift, p_stop + shift, &mut progress_builder, true);
        let mut progress_path = progress_builder.detach();
        let offset_angle = (coerced_wave_offset * 360.0) % 360.0;
        if offset_angle != 0.0 {
            let bounds = *full_progress_path.bounds();
            let center = bounds.center();
            progress_path = progress_path.make_offset((-center.x, -center.y));
            let matrix = skia_safe::Matrix::rotate_deg(-offset_angle);
            progress_path = progress_path.make_transform(&matrix);
            progress_path = progress_path.make_offset((center.x, center.y));
        }
        progress_builder = skia_safe::PathBuilder::new();
        progress_builder.add_path(&progress_path, None);
    } else {
        progress_measure.get_segment(p_start, p_stop, &mut progress_builder, true);
    }
    let progress_path = progress_builder.detach();

    let mut track_builder = skia_safe::PathBuilder::new();
    if track_path_length > 0.0 {
        let t_start = end_progress * track_path_length + track_spacing;
        let t_stop = track_path_length - track_spacing;
        track_measure.get_segment(t_start, t_stop, &mut track_builder, true);
    }
    let track_path = track_builder.detach();

    (track_path, progress_path)
}

fn process_circular_path(path: &mut skia_safe::Path, width: f32, height: f32, stroke_width: f32) {
    let scale_matrix = skia_safe::Matrix::scale((width - stroke_width, height - stroke_width));
    *path = path.make_transform(&scale_matrix);
    let bounds = *path.bounds();
    let dx = width / 2.0 - bounds.center_x();
    let dy = height / 2.0 - bounds.center_y();
    *path = path.make_offset((dx, dy));
    // `to_path(start_angle=270)` 已由 material-shapes 修正为真正旋转到 270°
    // （12 点方向），这里不再做额外旋转，避免 Morph 中间帧因动态旋转导致相位跳变。
}

fn draw_circular_wavy_paths(
    canvas: &skia_safe::Canvas,
    _rect: skia_safe::Rect,
    track_path: &skia_safe::Path,
    progress_path: &skia_safe::Path,
    color: Color,
    track_color: Color,
    stroke_width: f32,
    track_stroke_width: f32,
    cap: ProgressIndicatorStrokeCap,
) {
    if track_color.a != 0 {
        let track_paint = paint_stroke(track_stroke_width, cap, track_color);
        canvas.draw_path(track_path, &track_paint);
    }
    if color.a != 0 {
        let progress_paint = paint_stroke(stroke_width, cap, color);
        canvas.draw_path(progress_path, &progress_paint);
    }
}

/// 缓存 Circular 使用的 RoundedPolygon（按 size/wavelength 重建）。
///
/// 额外缓存 `Morph::morph_match`：`Morph::new` 的 feature-mapping 是昂贵步骤，
/// 这里只做一次，之后每帧用 `Morph::from_morph_match` 廉价插值。
#[derive(Default)]
pub(crate) struct CircularShapesCache {
    size: (f32, f32),
    wavelength: f32,
    stroke_width: f32,
    vertex_count: usize,
    track_polygon: Option<RoundedPolygon>,
    active_polygon: Option<RoundedPolygon>,
    morph_match: Option<Vec<(Cubic, Cubic)>>,
}

impl CircularShapesCache {
    fn update(&mut self, width: f32, height: f32, wavelength: f32, stroke_width: f32) {
        if self.size == (width, height)
            && self.wavelength == wavelength
            && self.stroke_width == stroke_width
            && self.track_polygon.is_some()
            && self.active_polygon.is_some()
            && self.morph_match.is_some()
        {
            return;
        }
        let r = width.min(height) / 2.0 - stroke_width / 2.0;
        let num_vertices =
            ((2.0 * PI * r / wavelength).round() as usize).max(MIN_CIRCULAR_VERTEX_COUNT);

        let track = RoundedPolygon::circle(num_vertices, None, None, None).normalized();
        let active = RoundedPolygon::star(
            num_vertices,
            None,
            Some(0.75),
            Some(CornerRounding::new(0.35, Some(0.4))),
            Some(CornerRounding::new(0.5, None)),
            None,
            None,
            None,
        )
        .normalized();
        // 只做一次昂贵的 feature-mapping，缓存 morph_match 供每帧插值复用。
        let morph_match = Morph::new(&track, &active).morph_match().clone();

        self.size = (width, height);
        self.wavelength = wavelength;
        self.stroke_width = stroke_width;
        self.vertex_count = num_vertices;
        self.track_polygon = Some(track);
        self.active_polygon = Some(active);
        self.morph_match = Some(morph_match);
    }

    fn get_track_path(&self) -> skia_safe::Path {
        match &self.track_polygon {
            Some(p) => p.to_path(Some(270), Some(false), None),
            None => skia_safe::Path::new(),
        }
    }

    fn get_progress_path(&self, amplitude: f32, repeat_path: bool) -> skia_safe::Path {
        match (&self.track_polygon, &self.active_polygon, &self.morph_match) {
            (Some(track), Some(active), Some(morph_match)) => {
                if amplitude == 0.0 {
                    track.to_path(Some(270), Some(repeat_path), None)
                } else {
                    // 统一走 Morph（含 amplitude==1.0）：Compose 在创建 Morph 后也是
                    // 全部用 Morph.toPath，避免纯 star path 与 Morph 极限相位不一致
                    // 导致振幅接近 1 时波峰/波谷角度跳变。
                    let morph = Morph::from_morph_match(track, active, morph_match.clone());
                    morph.to_path(
                        amplitude,
                        Some(270),
                        Some(repeat_path),
                        None,
                        Some(0.5),
                        Some(0.5),
                    )
                }
            }
            _ => skia_safe::Path::new(),
        }
    }
}


#[cfg(test)]
mod tests;
