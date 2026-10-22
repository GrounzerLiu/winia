use super::*;

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
pub(crate) const MIN_WAVE_ANIMATION_MS: u64 = 50;
/// Circular 最少顶点数（Compose `MinCircularVertexCount = 5`）
pub(crate) const MIN_CIRCULAR_VERTEX_COUNT: usize = 5;

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
pub(crate) enum WavyAmplitude {
    /// 默认 Compose `indicatorAmplitude` 函数
    Indicator,
    /// 自定义函数（determinate）
    Custom(Arc<dyn Fn(f32) -> f32 + Send + Sync>),
    /// 固定值（indeterminate）
    Fixed(f32),
}

impl WavyAmplitude {
    pub(crate) fn resolve(&self, progress: f32) -> f32 {
        let value = match self {
            Self::Indicator => WavyProgressIndicatorDefaults::indicator_amplitude(progress),
            Self::Custom(f) => f(progress),
            Self::Fixed(v) => *v,
        };
        sanitize_amplitude(value)
    }

    pub(crate) fn fixed(&self) -> f32 {
        let value = match self {
            Self::Fixed(v) => *v,
            _ => 1.0,
        };
        sanitize_amplitude(value)
    }

    /// 供 `ctx.changed` 使用的可比较 token：固定值按数值、自定义函数按 Arc 指针。
    pub(crate) fn change_token(&self) -> (u8, u64, usize) {
        match self {
            Self::Indicator => (0, 0, 0),
            Self::Custom(f) => (1, 0, Arc::as_ptr(f) as *const () as usize),
            Self::Fixed(v) => (2, v.to_bits() as u64, 0),
        }
    }
}

pub(crate) fn sanitize_amplitude(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

pub(crate) fn increasing_amplitude_spec() -> AnimationSpec {
    AnimationSpec::Tween(TweenSpec::new(
        Duration::from_millis(WAVY_ANIMATION_DURATION_MS),
        interpolator::CubicBezier::new(0.2, 0.0, 0.0, 1.0), // Standard
    ))
}

pub(crate) fn decreasing_amplitude_spec() -> AnimationSpec {
    AnimationSpec::Tween(TweenSpec::new(
        Duration::from_millis(WAVY_ANIMATION_DURATION_MS),
        interpolator::CubicBezier::new(0.3, 0.0, 0.8, 0.15), // EmphasizedAccelerate
    ))
}

pub(crate) fn wave_animation_spec(wavelength: f32, wave_speed: f32) -> InfiniteRepeatableSpec {
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

pub(crate) fn skia_color(c: Color) -> skia_safe::Color {
    skia_safe::Color::from_argb(c.a, c.r, c.g, c.b)
}

pub(crate) fn paint_stroke(width: f32, cap: ProgressIndicatorStrokeCap, color: Color) -> skia_safe::Paint {
    let mut paint = skia_safe::Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(skia_safe::PaintStyle::Stroke);
    paint.set_stroke_width(width);
    paint.set_stroke_cap(cap.to_skia());
    paint.set_color(skia_color(color));
    paint
}

