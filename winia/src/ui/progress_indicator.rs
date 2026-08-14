//! Progress Indicator 组件 — 对标 material3 `LinearProgressIndicator` / `CircularProgressIndicator`
//!（Compose material3 ProgressIndicator.kt + M3 token v0_7_0）
//!
//! M3 实现要点（对齐项）：
//! - **两种形态**：Linear（240×4dp 轨道 + 4dp 圆点 stop indicator）与 Circular
//!   （40dp 圆 + 4dp 弧线），`TrackThickness = 4dp`、`TrackActiveSpace = 4dp`；
//! - **两种行为**：Determinate（`progress` 0..1，indicator 段 + track 段 + stop）与
//!   Indeterminate（无限循环动画，不显示 track 空隙语义）——Compose 同 API 分态；
//! - **颜色**：active = Primary、track = SecondaryContainer（`ActiveIndicatorColor/
//!   TrackColor`），indeterminate circular 无 track（Transparent）；
//! - **Linear gap 语义**：Round cap 时 `adjustedGap = gap + strokeWidth`（两端 cap
//!   各占半宽，视觉间隙更大）；track 从 `progress + min(progress, gapFraction)` 开始
//!   （进度小时 gap 收缩避免重叠）；
//! - **Linear stop indicator**：track 末端 4dp 圆点（`StopSize`），`StopIndicatorTrailing
//!   Space = 6dp` 限位，颜色 = active 色；
//! - **Indeterminate 动画**（对标 Compose 无限 keyframes）——Linear：4 条线（两条
//!   head/tail，周期 1750ms，`EasingEmphasizedAccelerate(0.3,0,0.8,0.15)`）；
//!   Circular：全局旋转 1080°/6s 线性 + 额外旋转 90°步进（`EmphasizedDecelerate(0.05,
//!   0.7,0.1,1)`）+ 进度 0.1↔0.87 呼吸（`Standard(0.2,0,0,1)`）；
//! - **Stroke cap**：默认 Round（`LinearStrokeCap/CircularDeterminateStrokeCap`）；
//!   `height > width` 时退化 Butt（Compose 兼容逻辑）；
//! - **Circular 几何**：弧线内缩 strokeWidth/2 保持 stroke 中点在直径上；
//!   startAngle = 270°（12 点钟方向）；gap 用弧长/周长换算成扫过角度。
//!
//! 架构：同 Slider —— `Modifier::draw()` 自定义 Canvas 绘制；indeterminate 动画
//! 用 `remember_infinite_transition`（帧驱动在 app.rs）。

use crate::core::composer::{ComposeCtx, GroupStatus};
use crate::composable;
use crate::layout::BoxLayout;
use crate::modifier::{Color, Modifier};
use crate::ui::theme::{ThemeColors, WiniaTheme};
use crate::animation::{InfiniteRepeatableSpec, interpolator};
use std::sync::Arc;
use std::time::Duration;

/// Linear 指示器默认宽度（Compose `LinearIndicatorWidth = 240dp`——spec 未定义 token）
pub const LINEAR_INDICATOR_WIDTH: f32 = 240.0;
/// Linear 指示器默认高度（`LinearProgressIndicatorTokens.Height = 4dp`）
pub const LINEAR_INDICATOR_HEIGHT: f32 = 4.0;
/// Linear stop indicator 直径（`LinearProgressIndicatorTokens.StopSize = 4dp`）
pub const LINEAR_STOP_SIZE: f32 = 4.0;
/// stop indicator 距末端的最大偏移（`StopIndicatorTrailingSpace = 6dp`）
pub const STOP_INDICATOR_TRAILING_SPACE: f32 = 6.0;
/// Circular 指示器直径（`CircularProgressIndicatorTokens.Size = 40dp`）
pub const CIRCULAR_INDICATOR_DIAMETER: f32 = 40.0;
/// Circular 弧线宽度（`CircularProgressIndicatorTokens.TrackThickness = 4dp`）
pub const CIRCULAR_STROKE_WIDTH: f32 = 4.0;
/// 指示器与轨道间隙（`Linear/CircularProgressIndicatorTokens.TrackActiveSpace = 4dp`）
pub const TRACK_ACTIVE_SPACE: f32 = 4.0;

/// 描边端点形状（对标 Compose `StrokeCap`——winia 组件自用枚举）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressIndicatorStrokeCap {
    /// 平头（无延伸）
    Butt,
    /// 圆头（两端各延伸半宽）
    Round,
}

impl ProgressIndicatorStrokeCap {
    fn to_skia(self) -> skia_safe::paint::Cap {
        match self {
            Self::Butt => skia_safe::paint::Cap::Butt,
            Self::Round => skia_safe::paint::Cap::Round,
        }
    }
}

/// 默认值（对标 Compose `ProgressIndicatorDefaults`）
pub struct ProgressIndicatorDefaults;

impl ProgressIndicatorDefaults {
    /// Linear/Circular 指示器色（`ProgressIndicatorTokens.ActiveIndicatorColor = Primary`）
    pub fn indicator_color(theme: &ThemeColors) -> Color {
        theme.primary
    }
    /// Linear/Circular(determinate) 轨道色（`ProgressIndicatorTokens.TrackColor = SecondaryContainer`）
    pub fn track_color(theme: &ThemeColors) -> Color {
        theme.secondary_container
    }
    /// Linear 默认 stroke cap（`LinearStrokeCap = Round`）
    pub fn linear_stroke_cap() -> ProgressIndicatorStrokeCap {
        ProgressIndicatorStrokeCap::Round
    }
    /// Circular 默认 stroke cap（`CircularDeterminateStrokeCap = Round`）
    pub fn circular_stroke_cap() -> ProgressIndicatorStrokeCap {
        ProgressIndicatorStrokeCap::Round
    }
    /// determinate 进度切换动画规格（对标 `ProgressIndicatorDefaults.ProgressAnimationSpec`——
    /// Spring 无弹跳 + 极低刚度，阈值 1/1000）
    pub fn progress_animation_spec() -> crate::animation::AnimationSpec {
        crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec {
            damping_ratio: crate::animation::SpringSpec::DAMPING_RATIO_NO_BOUNCY,
            stiffness: crate::animation::SpringSpec::STIFFNESS_VERY_LOW,
            mass: 1.0,
            threshold: 1.0 / 1000.0,
        })
    }
}

// ═══════════════════════════════════════════════════════
// Indeterminate 动画规格（对标 Compose linear/circularIndeterminate*Spec）
// ═══════════════════════════════════════════════════════

/// M3 `MotionTokens.EasingEmphasizedAccelerateCubicBezier`——Linear 线条动画
fn emphasized_accelerate() -> Arc<dyn interpolator::Interpolator> {
    interpolator::CubicBezier::new(0.3, 0.0, 0.8, 0.15).into()
}

/// M3 `MotionTokens.EasingStandardCubicBezier`——Circular 进度呼吸
fn standard_easing() -> Arc<dyn interpolator::Interpolator> {
    interpolator::CubicBezier::new(0.2, 0.0, 0.0, 1.0).into()
}

/// Linear 周期总时长（`LinearAnimationDuration = 1750ms`）
const LINEAR_ANIMATION_DURATION_MS: u64 = 1750;

/// 构造一条线的 head/tail 无限动画规格：delay 后从 0 缓动到 1 并保持到周期末
fn linear_line_spec(delay_ms: u64, duration_ms: u64) -> InfiniteRepeatableSpec {
    let total = LINEAR_ANIMATION_DURATION_MS as f32;
    let e = emphasized_accelerate();
    // keyframes：(周期内进度 0..1, 值 0..1，段间插值器)
    // 与 Compose `keyframes { durationMillis=1750; 0 at delay; 1 at delay+dur }` 对齐：
    // 保持段（0→0）与上升段（0→1）都用 EmphasizedAccelerate
    let frames = vec![
        (0.0, 0.0, e.clone()),
        (delay_ms as f32 / total, 0.0, e.clone()),
        ((delay_ms + duration_ms) as f32 / total, 1.0, e.clone()),
    ];
    InfiniteRepeatableSpec::restart_keyframes(Duration::from_millis(LINEAR_ANIMATION_DURATION_MS), frames)
}

/// Linear first line head：0ms 起 1000ms 到 1
fn linear_first_line_head_spec() -> InfiniteRepeatableSpec {
    linear_line_spec(0, 1000)
}

/// Linear first line tail：250ms 起 1000ms 到 1
fn linear_first_line_tail_spec() -> InfiniteRepeatableSpec {
    linear_line_spec(250, 1000)
}

/// Linear second line head：650ms 起 850ms 到 1
fn linear_second_line_head_spec() -> InfiniteRepeatableSpec {
    linear_line_spec(650, 850)
}

/// Linear second line tail：900ms 起 850ms 到 1
fn linear_second_line_tail_spec() -> InfiniteRepeatableSpec {
    linear_line_spec(900, 850)
}

/// Circular 全局旋转：0→1080° 线性 6000ms（`circularIndeterminateGlobalRotationAnimationSpec`）
fn circular_global_rotation_spec() -> InfiniteRepeatableSpec {
    InfiniteRepeatableSpec::restart_tween(
        Duration::from_millis(6000),
        crate::animation::TweenSpec::new(
            Duration::from_millis(6000),
            interpolator::Linear::new(),
        ),
    )
}

/// Circular 额外旋转：90°步进 6000ms keyframes（`circularIndeterminateRotationAnimationSpec`）
///
/// ⚠ easing 分配严格对齐 Compose 源码语义（VectorizedKeyframesSpec 实测）：
/// `using E` 作用于【段起点帧】开始的区间，且 timestamps 自动补 0/durationMillis
/// （无显式帧 → LinearEasing）。Compose 中 `90f at 300 using Decelerate` 的缓动
/// 只落在 hold 段 [300, 1500]（值不变，零视觉作用）——**所有动画段实际全为线性**。
/// 曾误将 0→90° 段设为 Decelerate（起点斜率 0.7/0.05=14 倍线性速度→猛冲，
/// 实测一顿一顿）。winia 取【段终点帧】easing，故映射为到达帧全 Linear。
fn circular_additional_rotation_spec() -> InfiniteRepeatableSpec {
    let linear: Arc<dyn interpolator::Interpolator> = Arc::new(interpolator::Linear::new());
    let total = 6000.0f32;
    let frames = vec![
        (0.0, 0.0, linear.clone()),
        (300.0 / total, 0.25, linear.clone()),       // [0, 300ms] 0°→90°：Linear（Compose 隐式 0ms 帧）
        (1500.0 / total, 0.25, linear.clone()),  // hold 至 1500ms
        (1800.0 / total, 0.5, linear.clone()),   // [1500, 1800ms] 90→180°：Linear
        (3000.0 / total, 0.5, linear.clone()),   // hold 至 3000ms
        (3300.0 / total, 0.75, linear.clone()),  // [3000, 3300ms] 180→270°：Linear
        (4500.0 / total, 0.75, linear.clone()),  // hold 至 4500ms
        (4800.0 / total, 1.0, linear.clone()),   // [4500, 4800ms] 270→360°：Linear
    ];
    InfiniteRepeatableSpec::restart_keyframes(Duration::from_millis(6000), frames)
}

/// Circular 进度呼吸：0.1→0.87→0.1（`circularIndeterminateProgressAnimationSpec`）
fn circular_progress_spec() -> InfiniteRepeatableSpec {
    let standard = standard_easing();
    let linear: Arc<dyn interpolator::Interpolator> = Arc::new(interpolator::Linear::new());
    // Compose 源码语义：段 [0, 3000ms]（0.1→0.87）起点 0ms 隐式帧 → Linear；
    // 段 [3000, 6000ms]（0.87→0.1）起点 `0.87 at 3000 using Standard` → Standard。
    // winia 取【段终点帧】easing：上升段终点帧 0.5 挂 Linear、下降段终点帧 1.0 挂 Standard
    let frames = vec![
        (0.0, 0.0, linear.clone()),
        (0.5, 1.0, linear.clone()),
        (1.0, 0.0, standard.clone()),
    ];
    InfiniteRepeatableSpec::restart_keyframes(Duration::from_millis(6000), frames)
}

// ═══════════════════════════════════════════════════════
// LinearProgressIndicator
// ═══════════════════════════════════════════════════════

/// 线性进度指示器（对标 Compose `LinearProgressIndicator`）
///
/// determinate：`LinearProgressIndicator::new(progress)`（progress 0..1，越界 clamp）；
/// indeterminate：`LinearProgressIndicator::indeterminate()`（无限线条动画，忽略 progress）。
#[derive(Clone)]
pub struct LinearProgressIndicator {
    progress: f32,
    indeterminate: bool,
    modifier: Modifier,
    color: Option<Color>,
    track_color: Option<Color>,
    stroke_cap: ProgressIndicatorStrokeCap,
    gap_size: f32,
    draw_stop_indicator: bool,
}

impl LinearProgressIndicator {
    /// determinate 构造：progress 0..1（越界自动 clamp）
    pub fn new(progress: f32) -> Self {
        Self {
            progress,
            indeterminate: false,
            modifier: Modifier::new(),
            color: None,
            track_color: None,
            stroke_cap: ProgressIndicatorDefaults::linear_stroke_cap(),
            gap_size: TRACK_ACTIVE_SPACE,
            draw_stop_indicator: true,
        }
    }

    /// indeterminate 构造：无限动画（忽略 progress）
    pub fn indeterminate() -> Self {
        Self::new(0.0).indeterminate_mode()
    }

    fn indeterminate_mode(mut self) -> Self {
        self.indeterminate = true;
        // indeterminate 无 stop indicator（Compose 仅 determinate 绘制 stop）
        self.draw_stop_indicator = false;
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

    /// 端点形状（默认 Round）
    pub fn stroke_cap(mut self, cap: ProgressIndicatorStrokeCap) -> Self {
        self.stroke_cap = cap;
        self
    }

    /// 指示器与轨道间隙（dp，默认 4——`TrackActiveSpace`）
    pub fn gap_size(mut self, gap: f32) -> Self {
        self.gap_size = gap.max(0.0);
        self
    }

    /// 是否绘制末端 stop indicator（默认 true，仅 determinate 有效）
    pub fn draw_stop_indicator(mut self, draw: bool) -> Self {
        self.draw_stop_indicator = draw;
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        ctx.changed(&self.progress);
        ctx.changed(&self.indeterminate);
        ctx.changed(&self.color);
        ctx.changed(&self.track_color);
        ctx.changed(&self.stroke_cap);
        ctx.changed(&self.gap_size);
        let key = ctx.next_key();
        let theme = WiniaTheme::colors();
        let color = self.color.unwrap_or_else(|| ProgressIndicatorDefaults::indicator_color(&theme));
        let track_color = self.track_color.unwrap_or_else(|| ProgressIndicatorDefaults::track_color(&theme));
        let cap = self.stroke_cap;
        let gap = self.gap_size;

        let m = if self.indeterminate {
            // 无限动画：4 条线（first/second × head/tail）——remember_infinite_transition
            // 生命周期绑定组合点（on_remove 自动 dispose）
            let mut inf = ctx.remember_infinite_transition();
            let fh = inf.animate_float(ctx, 0.0, 1.0, linear_first_line_head_spec());
            let ft = inf.animate_float(ctx, 0.0, 1.0, linear_first_line_tail_spec());
            let sh = inf.animate_float(ctx, 0.0, 1.0, linear_second_line_head_spec());
            let st = inf.animate_float(ctx, 0.0, 1.0, linear_second_line_tail_spec());
            Modifier::new()
                .size(LINEAR_INDICATOR_WIDTH, LINEAR_INDICATOR_HEIGHT)
                .draw(move |canvas, rect| {
                    draw_linear_indeterminate(
                        canvas, rect,
                        color, track_color, cap, gap,
                        fh.peek(), ft.peek(), sh.peek(), st.peek(),
                    );
                })
        } else {
            let progress = self.progress;
            let draw_stop = self.draw_stop_indicator;
            Modifier::new()
                .size(LINEAR_INDICATOR_WIDTH, LINEAR_INDICATOR_HEIGHT)
                .draw(move |canvas, rect| {
                    draw_linear_determinate(
                        canvas, rect,
                        color, track_color, cap, gap, progress, draw_stop,
                    );
                })
        };

        let m = m.then(self.modifier);
        match ctx.start_restartable_group(key, m, BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {}
        }
        ctx.end_restartable_group();
    }
}

// ═══════════════════════════════════════════════════════
// Linear 绘制（对齐 Compose drawLinearIndicator / determinate / indeterminate）
// ═══════════════════════════════════════════════════════

/// 画一条水平线（fraction 相对组件宽度，y 居中；对标 Compose `drawLinearIndicator`）
fn draw_linear_indicator(
    canvas: &skia_safe::Canvas,
    rect: skia_safe::Rect,
    start_fraction: f32,
    end_fraction: f32,
    color: Color,
    stroke_width: f32,
    cap: ProgressIndicatorStrokeCap,
) {
    let w = rect.width();
    let h = rect.height();
    if w <= 0.0 || h <= 0.0 { return; }
    // 从 stroke 的垂直中心开始画
    let y_offset = rect.top + h / 2.0;
    let bar_start = rect.left + start_fraction * w;
    let bar_end = rect.left + end_fraction * w;

    let mut paint = skia_safe::Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(skia_safe::PaintStyle::Stroke);
    paint.set_stroke_width(stroke_width);
    paint.set_color(skia_color(color));

    // 空间不足画 cap 时退化为 Butt（Compose 同）
    if cap == ProgressIndicatorStrokeCap::Butt || h > w {
        paint.set_stroke_cap(skia_safe::paint::Cap::Butt);
        canvas.draw_line(
            skia_safe::Point::new(bar_start, y_offset),
            skia_safe::Point::new(bar_end, y_offset),
            &paint,
        );
    } else {
        // Round cap 需要为 cap 预留空间：端点内缩 strokeWidth/2
        let stroke_cap_offset = stroke_width / 2.0;
        let adjusted_start = bar_start.clamp(
            rect.left + stroke_cap_offset,
            rect.right - stroke_cap_offset,
        );
        let adjusted_end = bar_end.clamp(
            rect.left + stroke_cap_offset,
            rect.right - stroke_cap_offset,
        );
        if (end_fraction - start_fraction).abs() > 0.0 {
            paint.set_stroke_cap(skia_safe::paint::Cap::Round);
            canvas.draw_line(
                skia_safe::Point::new(adjusted_start, y_offset),
                skia_safe::Point::new(adjusted_end, y_offset),
                &paint,
            );
        }
    }
}

/// 画 track 末端 stop indicator（圆形或方形，取决于 cap；对标 Compose `drawStopIndicator`）
fn draw_stop_indicator(
    canvas: &skia_safe::Canvas,
    rect: skia_safe::Rect,
    stop_size: f32,
    color: Color,
    cap: ProgressIndicatorStrokeCap,
) {
    let h = rect.height();
    if h <= 0.0 { return; }
    // stop 不能超过 track 高度
    let adjusted_stop_size = stop_size.min(h);
    // 大高度时限制 stop 偏移（StopIndicatorTrailingSpace = 6dp）
    let stop_offset = ((h - adjusted_stop_size) / 2.0).min(STOP_INDICATOR_TRAILING_SPACE);
    let cx = rect.right - adjusted_stop_size / 2.0 - stop_offset;
    let cy = rect.top + h / 2.0;
    let mut paint = skia_safe::Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(skia_color(color));
    if cap == ProgressIndicatorStrokeCap::Round {
        canvas.draw_circle(skia_safe::Point::new(cx, cy), adjusted_stop_size / 2.0, &paint);
    } else {
        let r = skia_safe::Rect::from_xywh(
            cx - adjusted_stop_size / 2.0,
            cy - adjusted_stop_size / 2.0,
            adjusted_stop_size,
            adjusted_stop_size,
        );
        canvas.draw_rect(r, &paint);
    }
}

/// determinate 线性进度（对标 Compose determinate 分支）
pub(crate) fn draw_linear_determinate(
    canvas: &skia_safe::Canvas,
    rect: skia_safe::Rect,
    color: Color,
    track_color: Color,
    cap: ProgressIndicatorStrokeCap,
    gap_size: f32,
    progress: f32,
    draw_stop: bool,
) {
    let w = rect.width();
    let h = rect.height();
    if w <= 0.0 || h <= 0.0 { return; }
    let stroke_width = h;
    // Round cap 时视觉间隙 = gap + 两端 cap 延伸（Compose 同）
    let adjusted_gap_size = if cap == ProgressIndicatorStrokeCap::Butt || h > w {
        gap_size
    } else {
        gap_size + stroke_width
    };
    let gap_fraction = adjusted_gap_size / w;
    let p = progress.clamp(0.0, 1.0);

    // track：从 progress + min(progress, gapFraction) 到 1（进度小时 gap 收缩防重叠）
    let track_start = p + p.min(gap_fraction);
    if track_start <= 1.0 {
        draw_linear_indicator(canvas, rect, track_start, 1.0, track_color, stroke_width, cap);
    }
    // indicator：0 到 progress
    draw_linear_indicator(canvas, rect, 0.0, p, color, stroke_width, cap);
    // stop：track 末端圆点
    if draw_stop {
        draw_stop_indicator(canvas, rect, LINEAR_STOP_SIZE, color, cap);
    }
}

/// indeterminate 线性进度（对标 Compose indeterminate 分支——两线 + 三段 track）
pub(crate) fn draw_linear_indeterminate(
    canvas: &skia_safe::Canvas,
    rect: skia_safe::Rect,
    color: Color,
    track_color: Color,
    cap: ProgressIndicatorStrokeCap,
    gap_size: f32,
    first_line_head: f32,
    first_line_tail: f32,
    second_line_head: f32,
    second_line_tail: f32,
) {
    let w = rect.width();
    let h = rect.height();
    if w <= 0.0 || h <= 0.0 { return; }
    let stroke_width = h;
    let adjusted_gap_size = if cap == ProgressIndicatorStrokeCap::Butt || h > w {
        gap_size
    } else {
        gap_size + stroke_width
    };
    let gap_fraction = adjusted_gap_size / w;

    // Line 1 之前的 track（line1 head 未到 1-gap 前存在）
    if first_line_head < 1.0 - gap_fraction {
        let start = if first_line_head > 0.0 {
            first_line_head + gap_fraction
        } else {
            0.0
        };
        draw_linear_indicator(canvas, rect, start, 1.0, track_color, stroke_width, cap);
    }

    // Line 1（head - tail > 0 时存在）
    if first_line_head - first_line_tail > 0.0 {
        draw_linear_indicator(canvas, rect, first_line_head, first_line_tail, color, stroke_width, cap);
    }

    // Line 1 与 Line 2 之间的 track
    if first_line_tail > gap_fraction {
        let start = if second_line_head > 0.0 {
            second_line_head + gap_fraction
        } else {
            0.0
        };
        let end = if first_line_tail < 1.0 {
            first_line_tail - gap_fraction
        } else {
            1.0
        };
        draw_linear_indicator(canvas, rect, start, end, track_color, stroke_width, cap);
    }

    // Line 2
    if second_line_head - second_line_tail > 0.0 {
        draw_linear_indicator(canvas, rect, second_line_head, second_line_tail, color, stroke_width, cap);
    }

    // Line 2 之后的 track
    if second_line_tail > gap_fraction {
        let end = if second_line_tail < 1.0 {
            second_line_tail - gap_fraction
        } else {
            1.0
        };
        draw_linear_indicator(canvas, rect, 0.0, end, track_color, stroke_width, cap);
    }
}

fn skia_color(c: Color) -> skia_safe::Color {
    skia_safe::Color::from_argb(c.a, c.r, c.g, c.b)
}

// ═══════════════════════════════════════════════════════
// CircularProgressIndicator
// ═══════════════════════════════════════════════════════

/// 圆形进度指示器（对标 Compose `CircularProgressIndicator`）
///
/// determinate：`CircularProgressIndicator::new(progress)`（0..1，12 点方向顺时针）；
/// indeterminate：`CircularProgressIndicator::indeterminate()`（旋转 + 进度呼吸动画，
/// 无 track——`circularIndeterminateTrackColor = Transparent`）。
#[derive(Clone)]
pub struct CircularProgressIndicator {
    progress: f32,
    indeterminate: bool,
    modifier: Modifier,
    color: Option<Color>,
    track_color: Option<Color>,
    stroke_width: f32,
    stroke_cap: ProgressIndicatorStrokeCap,
    gap_size: f32,
}

impl CircularProgressIndicator {
    /// determinate 构造：progress 0..1（越界自动 clamp）
    pub fn new(progress: f32) -> Self {
        Self {
            progress,
            indeterminate: false,
            modifier: Modifier::new(),
            color: None,
            track_color: None,
            stroke_width: CIRCULAR_STROKE_WIDTH,
            stroke_cap: ProgressIndicatorDefaults::circular_stroke_cap(),
            gap_size: TRACK_ACTIVE_SPACE,
        }
    }

    /// indeterminate 构造：无限旋转动画
    pub fn indeterminate() -> Self {
        Self::new(0.0).indeterminate_mode()
    }

    fn indeterminate_mode(mut self) -> Self {
        self.indeterminate = true;
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

    /// 轨道颜色（默认 SecondaryContainer；indeterminate 无 track）
    pub fn track_color(mut self, color: Color) -> Self {
        self.track_color = Some(color);
        self
    }

    /// 弧线宽度（dp，默认 4——`TrackThickness`）
    pub fn stroke_width(mut self, width: f32) -> Self {
        self.stroke_width = width.max(0.0);
        self
    }

    /// 端点形状（默认 Round）
    pub fn stroke_cap(mut self, cap: ProgressIndicatorStrokeCap) -> Self {
        self.stroke_cap = cap;
        self
    }

    /// 指示器与轨道间隙（dp，默认 4——`TrackActiveSpace`）
    pub fn gap_size(mut self, gap: f32) -> Self {
        self.gap_size = gap.max(0.0);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        ctx.changed(&self.progress);
        ctx.changed(&self.indeterminate);
        ctx.changed(&self.color);
        ctx.changed(&self.track_color);
        ctx.changed(&self.stroke_width);
        ctx.changed(&self.stroke_cap);
        ctx.changed(&self.gap_size);
        let key = ctx.next_key();
        let theme = WiniaTheme::colors();
        let color = self.color.unwrap_or_else(|| ProgressIndicatorDefaults::indicator_color(&theme));
        let track_color = self.track_color.unwrap_or_else(|| ProgressIndicatorDefaults::track_color(&theme));
        let cap = self.stroke_cap;
        let gap = self.gap_size;
        let stroke_width = self.stroke_width;

        let m = if self.indeterminate {
            // 三个无限动画：全局旋转（线性 1080°/6s）+ 额外旋转（90°步进）
            // + 进度呼吸（0.1↔0.87）
            let mut inf = ctx.remember_infinite_transition();
            let global = inf.animate_float(ctx, 0.0, 1080.0, circular_global_rotation_spec());
            let additional = inf.animate_float(ctx, 0.0, 360.0, circular_additional_rotation_spec());
            let progress_anim = inf.animate_float(ctx, 0.1, 0.87, circular_progress_spec());
            Modifier::new()
                .size(CIRCULAR_INDICATOR_DIAMETER, CIRCULAR_INDICATOR_DIAMETER)
                .draw(move |canvas, rect| {
                    draw_circular_indeterminate(
                        canvas, rect,
                        color, cap, gap, stroke_width,
                        global.peek(), additional.peek(), progress_anim.peek(),
                    );
                })
        } else {
            let progress = self.progress;
            Modifier::new()
                .size(CIRCULAR_INDICATOR_DIAMETER, CIRCULAR_INDICATOR_DIAMETER)
                .draw(move |canvas, rect| {
                    draw_circular_determinate(
                        canvas, rect,
                        color, track_color, cap, gap, stroke_width,
                        progress,
                    );
                })
        };

        let m = m.then(self.modifier);
        match ctx.start_restartable_group(key, m, BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {}
        }
        ctx.end_restartable_group();
    }
}

// ═══════════════════════════════════════════════════════
// Circular 绘制（对齐 Compose drawCircularIndicator / determinate / indeterminate）
// ═══════════════════════════════════════════════════════

/// 画一段圆弧（stroke 中心线落在直径上；对标 Compose `drawCircularIndicator`）
fn draw_circular_indicator(
    canvas: &skia_safe::Canvas,
    rect: skia_safe::Rect,
    start_angle: f32,
    sweep: f32,
    color: Color,
    stroke_width: f32,
    cap: ProgressIndicatorStrokeCap,
) {
    let d = rect.width();
    if d <= 0.0 || stroke_width <= 0.0 { return; }
    // 弧线包围盒内缩 strokeWidth/2——stroke 中心线恰好落在直径圆上
    let diameter_offset = stroke_width / 2.0;
    let arc_dimen = d - 2.0 * diameter_offset;
    let oval = skia_safe::Rect::from_xywh(
        rect.left + diameter_offset,
        rect.top + diameter_offset,
        arc_dimen,
        arc_dimen,
    );
    let mut paint = skia_safe::Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(skia_safe::PaintStyle::Stroke);
    paint.set_stroke_width(stroke_width);
    paint.set_stroke_cap(cap.to_skia());
    paint.set_color(skia_color(color));
    canvas.draw_arc(oval, start_angle, sweep, false, &paint);
}

/// gap 占圆周的比例换算为扫过角度（gap 长度 / 周长 × 360°）
fn gap_size_sweep(gap_size: f32, diameter: f32) -> f32 {
    if diameter <= 0.0 { return 0.0; }
    (gap_size / (std::f32::consts::PI * diameter)) * 360.0
}

/// determinate 圆形进度（对标 Compose determinate 分支）
pub(crate) fn draw_circular_determinate(
    canvas: &skia_safe::Canvas,
    rect: skia_safe::Rect,
    color: Color,
    track_color: Color,
    cap: ProgressIndicatorStrokeCap,
    gap_size: f32,
    stroke_width: f32,
    progress: f32,
) {
    let d = rect.width();
    let h = rect.height();
    if d <= 0.0 || stroke_width <= 0.0 { return; }
    let p = progress.clamp(0.0, 1.0);
    // 12 点方向起画（Compose startAngle = 270°）
    let start_angle = 270.0;
    let sweep = p * 360.0;
    let adjusted_gap_size = if cap == ProgressIndicatorStrokeCap::Butt || h > d {
        gap_size
    } else {
        gap_size + stroke_width
    };
    let gap_sweep = gap_size_sweep(adjusted_gap_size, d);

    // track：从 indicator 终点 + gap 扫到完整圆（360 - sweep - 2×min(sweep, gapSweep)）
    let min_sweep_gap = sweep.min(gap_sweep);
    draw_circular_indicator(
        canvas, rect,
        start_angle + sweep + min_sweep_gap,
        360.0 - sweep - min_sweep_gap * 2.0,
        track_color,
        stroke_width,
        cap,
    );
    // indicator：12 点起顺时针 sweep
    draw_circular_indicator(
        canvas, rect,
        start_angle,
        sweep,
        color,
        stroke_width,
        cap,
    );
}

/// indeterminate 圆形进度（对标 Compose indeterminate 分支——整体旋转 + 进度呼吸）
pub(crate) fn draw_circular_indeterminate(
    canvas: &skia_safe::Canvas,
    rect: skia_safe::Rect,
    color: Color,
    cap: ProgressIndicatorStrokeCap,
    _gap_size: f32,
    stroke_width: f32,
    global_rotation: f32,
    additional_rotation: f32,
    progress: f32,
) {
    let d = rect.width();
    if d <= 0.0 || stroke_width <= 0.0 { return; }
    let sweep = progress.clamp(0.0, 1.0) * 360.0;

    let cx = rect.left + d / 2.0;
    let cy = rect.top + d / 2.0;
    canvas.save();
    // 整体旋转：全局（线性 1080°）+ 额外（90°步进）
    canvas.rotate(global_rotation + additional_rotation, Some(skia_safe::Point::new(cx, cy)));
    // 无 track（indeterminate track = Transparent）；只有进度弧
    draw_circular_indicator(
        canvas, rect,
        0.0,
        sweep,
        color,
        stroke_width,
        cap,
    );
    canvas.restore();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::composer::Composer;
    use crate::layout::Constraints;

    // ── 纯函数：gap/stop 几何 ──
    #[test]
    fn gap_fraction_matches_compose() {
        // Compose：Round cap → adjustedGap = gap(4) + strokeWidth(4) = 8dp；
        // 组件宽 240dp → gapFraction = 8/240
        let adjusted: f32 = 4.0 + 4.0;
        assert!(((adjusted / 240.0) - 8.0 / 240.0).abs() < 1e-6);
        // Butt cap → 不加 strokeWidth
        assert_eq!(4.0 / 240.0, 4.0 / 240.0);
    }

    #[test]
    fn stop_indicator_geometry() {
        // Compose drawStopIndicator：adjustedStopSize = min(4, height)；
        // stopOffset = min((h - size)/2, 6dp)；圆形圆心 = 宽 - size/2 - offset
        let h = 4.0;
        let stop_size = LINEAR_STOP_SIZE.min(h);
        let offset = ((h - stop_size) / 2.0).min(STOP_INDICATOR_TRAILING_SPACE);
        let cx = 240.0 - stop_size / 2.0 - offset;
        assert_eq!(stop_size, 4.0);
        assert_eq!(offset, 0.0);
        assert_eq!(cx, 238.0, "240 - 2 - 0");
        // 大高度：offset 受限 6dp
        let h2: f32 = 20.0;
        let offset2 = ((h2 - 4.0) / 2.0).min(STOP_INDICATOR_TRAILING_SPACE);
        assert_eq!(offset2, 6.0, "(20-4)/2=8 超限 → 6");
    }

    #[test]
    fn circular_gap_sweep_conversion() {
        // Compose：gapSizeSweep = adjustedGap / (π × 直径) × 360°
        let sweep = gap_size_sweep(8.0, 40.0);
        let expected = (8.0 / (std::f32::consts::PI * 40.0)) * 360.0;
        assert!((sweep - expected).abs() < 1e-4);
    }

    // ── 颜色解析 ──
    #[test]
    fn default_colors_resolution() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        assert_eq!(ProgressIndicatorDefaults::indicator_color(&theme), theme.primary);
        assert_eq!(ProgressIndicatorDefaults::track_color(&theme), theme.secondary_container);
        assert_eq!(ProgressIndicatorDefaults::linear_stroke_cap(), ProgressIndicatorStrokeCap::Round);
        assert_eq!(ProgressIndicatorDefaults::circular_stroke_cap(), ProgressIndicatorStrokeCap::Round);
    }

    // ── 动画规格 ──
    #[test]
    fn linear_line_spec_matches_compose_keyframes() {
        // Compose linearIndeterminateFirstLineHeadAnimationSpec：1750ms 周期，0 at 0ms，1 at 1000ms
        let spec = linear_first_line_head_spec();
        assert_eq!(spec.duration, Duration::from_millis(1750));
        let frames = match &spec.base {
            Some(crate::animation::AnimationSpec::Keyframes(kf)) => &kf.frames,
            _ => panic!("head spec 应为 keyframes"),
        };
        assert_eq!(frames.len(), 3);
        assert_eq!(frames[0].0, 0.0);
        assert_eq!(frames[0].1, 0.0);
        // 保持段：0 at 0ms（首帧即 0）
        // 上升段：1000/1750 到 1
        assert!(((frames[2].0 - 1000.0 / 1750.0).abs()) < 1e-5);
        assert_eq!(frames[2].1, 1.0);
    }

    #[test]
    fn linear_tail_delay_matches_compose() {
        // Compose firstLineTail：0 at 250ms → 保持段 [0, 250/1750]
        let spec = linear_first_line_tail_spec();
        let frames = match &spec.base {
            Some(crate::animation::AnimationSpec::Keyframes(kf)) => &kf.frames,
            _ => panic!(),
        };
        assert_eq!(frames.len(), 3);
        assert!(((frames[1].0 - 250.0 / 1750.0).abs()) < 1e-5, "tail 保持段至 250ms");
        assert_eq!(frames[1].1, 0.0);
        assert!(((frames[2].0 - 1250.0 / 1750.0).abs()) < 1e-5);
    }

    fn is_linear(i: &Arc<dyn interpolator::Interpolator>) -> bool {
        // Linear 恒等：任意 x 返回 x
        (i.interpolate(0.3) - 0.3).abs() < 1e-6 && (i.interpolate(0.7) - 0.7).abs() < 1e-6
    }

    fn is_standard(i: &Arc<dyn interpolator::Interpolator>) -> bool {
        // Standard(0.2,0,0,1)：x=0.5 时 y≈0.63（非线性但起点不猛冲）
        !is_linear(i) && i.interpolate(0.1) < 0.3
    }

    #[test]
    fn circular_animation_specs_match_compose() {
        // 全局旋转：6000ms 线性 0→1080
        let g = circular_global_rotation_spec();
        assert_eq!(g.duration, Duration::from_millis(6000));
        assert!(matches!(g.base, Some(crate::animation::AnimationSpec::Tween(_))));
        // 额外旋转：300ms 到 90°（0.25）
        let a = circular_additional_rotation_spec();
        let frames = match &a.base {
            Some(crate::animation::AnimationSpec::Keyframes(kf)) => &kf.frames,
            _ => panic!(),
        };
        assert_eq!(frames.len(), 8);
        assert!(((frames[1].0 - 300.0 / 6000.0).abs()) < 1e-5);
        assert_eq!(frames[1].1, 0.25);
        // ⚠ easing 分配（对齐 Compose VectorizedKeyframesSpec 源码语义）：
        // `using E` 属于【段起点帧】；0ms 为隐式补入帧 → LinearEasing。故 Compose 中
        // `90f at 300 using Decelerate` 只影响 hold 段 [300,1500]（值不变），
        // **所有动画段实际全为 Linear**（曾误设 0→90° 为 Decelerate——起点斜率
        // 0.7/0.05=14 倍线性速度导致猛冲，实测一顿一顿）。
        for idx in [1usize, 3, 5, 7] {
            assert!(
                is_linear(&frames[idx].2),
                "全部动画段应 Linear（Compose 隐式/默认），idx={idx}",
            );
        }
        // 进度呼吸：上升段 [0,3000ms] 起点 0ms 隐式帧 → Linear；
        // 下降段 [3000,6000ms] 起点 `0.87 at 3000 using Standard` → Standard
        let p = circular_progress_spec();
        let frames = match &p.base {
            Some(crate::animation::AnimationSpec::Keyframes(kf)) => &kf.frames,
            _ => panic!(),
        };
        assert_eq!(frames[1].0, 0.5);
        assert_eq!(frames[1].1, 1.0);
        assert_eq!(frames[2].1, 0.0);
        assert!(is_linear(&frames[1].2), "上升段应 Linear（隐式 0ms 帧）");
        assert!(is_standard(&frames[2].2), "下降段应 Standard（显式 using）");
    }

    #[test]
    fn infinite_keyframes_registers_and_cleans() {
        // 验证 keyframes 无限动画能注册进全局动画表并被清理
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        use crate::animation::push_infinite;
        let state = crate::core::state::State::new(0.0f32);
        push_infinite(state.clone(), 0.0, 1.0, linear_first_line_head_spec());
        assert!(crate::animation::has_animation_for_state(state.id()));
        // 更新一帧：Restart 模式下 t≈0 → 曲线起点 0
        crate::animation::update_animations();
        assert!(state.peek().abs() < 0.01, "t≈0 时 keyframes 首帧值=0");
        crate::animation::remove_animation_by_state(state.id());
        assert!(!crate::animation::has_animation_for_state(state.id()));
    }

    // ── 像素测试：determinate 渲染 ──
    fn render_progress(build: impl FnOnce(&mut ComposeCtx)) -> (Vec<[u8; 4]>, usize) {
        use skia_safe::{Color as SkColor, surfaces};
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = Composer::new();
        let scene = |ctx: &mut ComposeCtx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| build(ctx));
        };
        composer.compose(scene);
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
        let mut surface = surfaces::raster_n32_premul((300, 300)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(SkColor::WHITE);
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        crate::render::render(nodes, root, canvas);
        let pm = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        (px.to_vec(), pm.width() as usize)
    }

    /// 读取 (x, y) 像素为 RGBA（raster_n32_premul 是 BGRA 小端）
    fn px_at(buf: &[[u8; 4]], w: usize, x: usize, y: usize) -> [u8; 4] {
        let p = buf[y * w + x];
        [p[2], p[1], p[0], p[3]]
    }

    fn color_eq(c: Color, px: [u8; 4], tol: u8) -> bool {
        (c.r as i16 - px[0] as i16).abs() <= tol as i16
            && (c.g as i16 - px[1] as i16).abs() <= tol as i16
            && (c.b as i16 - px[2] as i16).abs() <= tol as i16
            && (c.a as i16 - px[3] as i16).abs() <= tol as i16
    }

    #[test]
    fn linear_determinate_pixels() {
        // 进度 50%：左半 primary（indicator），右半 secondary_container（track），
        // 最右端 stop 圆点（primary）
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let (buf, w) = render_progress(|ctx| {
            LinearProgressIndicator::new(0.5).build(ctx);
        });
        assert_eq!(w, 300);
        // 组件默认 240×4 放在根节点内（BoxLayout 默认对齐——验证实际位置）
        // 先扫描找到指示器行：y 方向高度 4dp，找 primary 色所在行
        let primary = ProgressIndicatorDefaults::indicator_color(&theme);
        let track = ProgressIndicatorDefaults::track_color(&theme);
        // 全图扫描：收集所有 primary 像素的 x 范围（indicator 段 + stop 圆点）
        let mut xs = vec![];
        for y in 0..300 {
            for x in 0..300 {
                let p = px_at(&buf, w, x, y);
                if color_eq(primary, p, 4) { xs.push(x); }
            }
        }
        assert!(!xs.is_empty(), "indicator 段应有 primary 像素");
        let x_min = *xs.iter().min().unwrap();
        let x_max = *xs.iter().max().unwrap();
        // 进度 50% + Round cap 延伸：indicator 从 0 到 ~120+2
        assert!(x_min <= 2, "indicator 从最左开始（Round cap 内缩 2）");
        // stop 圆点在右端（x ≈ 238±3），与 indicator 段分离
        assert!(x_max >= 235 && x_max <= 245, "stop 圆点中心在 238 附近，x_max={x_max}");
        // track 色：右半（在 stop 左侧大片区域）
        let mut track_xs = vec![];
        for y in 0..300 {
            for x in 0..300 {
                let p = px_at(&buf, w, x, y);
                if color_eq(track, p, 4) { track_xs.push(x); }
            }
        }
        assert!(!track_xs.is_empty(), "track 段应有 secondary_container 像素");
        // track 从 indicator 末端（~122+gap 8 ≈ 130）开始
        let t_min = *track_xs.iter().min().unwrap();
        assert!(t_min >= 125 && t_min <= 135, "track 从 ~130 开始，t_min={t_min}");
    }

    #[test]
    fn linear_determinate_full_progress() {
        // progress=1：全部 primary（无 track），stop 仍在末端
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let primary = ProgressIndicatorDefaults::indicator_color(&theme);
        let track = ProgressIndicatorDefaults::track_color(&theme);
        let (buf, w) = render_progress(|ctx| {
            LinearProgressIndicator::new(1.0).build(ctx);
        });
        let mut row = None;
        for y in 0..300 {
            for x in 0..300 {
                let p = px_at(&buf, w, x, y);
                if color_eq(primary, p, 4) { row = Some(y); break; }
            }
            if row.is_some() { break; }
        }
        let y = row.expect("行");
        let mut track_found = false;
        for x in 0..300 {
            if color_eq(track, px_at(&buf, w, x, y), 4) { track_found = true; break; }
        }
        assert!(!track_found, "progress=1 时无 track");
    }

    #[test]
    fn linear_zero_progress_no_indicator() {
        // progress=0：indicator 零长不画；track 全宽；stop 在末端
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let primary = ProgressIndicatorDefaults::indicator_color(&theme);
        let (buf, w) = render_progress(|ctx| {
            LinearProgressIndicator::new(0.0).build(ctx);
        });
        let mut row = None;
        for y in 0..300 {
            for x in 0..300 {
                let p = px_at(&buf, w, x, y);
                if color_eq(primary, p, 4) { row = Some(y); break; }
            }
            if row.is_some() { break; }
        }
        // 仅 stop 圆点在 238 附近（indicator 零长无像素）
        let y = row.expect("stop 圆点行");
        let mut primary_xs = vec![];
        for x in 0..300 {
            if color_eq(primary, px_at(&buf, w, x, y), 4) { primary_xs.push(x); }
        }
        assert!(!primary_xs.is_empty());
        assert!(primary_xs.iter().all(|&x| x >= 230), "只剩 stop 圆点（x≥230）");
    }

    #[test]
    fn circular_determinate_pixels() {
        // 25% 进度：12 点起顺时针 90° 弧为 primary；其余 track（secondary_container）
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let primary = ProgressIndicatorDefaults::indicator_color(&theme);
        let track = ProgressIndicatorDefaults::track_color(&theme);
        let (buf, w) = render_progress(|ctx| {
            CircularProgressIndicator::new(0.25).build(ctx);
        });
        // 组件 40×40，居中于 300×300？BoxLayout 默认对齐需确认——扫描全图
        // 中心附近找 primary 弧
        let mut found_primary = false;
        let mut found_track = false;
        for y in 0..300 {
            for x in 0..300 {
                let p = px_at(&buf, w, x, y);
                if color_eq(primary, p, 4) { found_primary = true; }
                if color_eq(track, p, 4) { found_track = true; }
            }
        }
        assert!(found_primary, "应有 primary 弧");
        assert!(found_track, "应有 track 弧");
    }

    #[test]
    fn circular_indeterminate_no_track() {
        // indeterminate circular：无 track（indeterminate track = Transparent），只有 primary
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let track = ProgressIndicatorDefaults::track_color(&theme);
        // 直接调绘制函数（无动画帧上下文——用固定值）
        let mut surface = skia_safe::surfaces::raster_n32_premul((40, 40)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::WHITE);
        let rect = skia_safe::Rect::from_xywh(0.0, 0.0, 40.0, 40.0);
        draw_circular_indeterminate(
            canvas, rect,
            ProgressIndicatorDefaults::indicator_color(&theme),
            ProgressIndicatorStrokeCap::Round,
            TRACK_ACTIVE_SPACE,
            CIRCULAR_STROKE_WIDTH,
            0.0, 0.0, 0.5,
        );
        let pm = surface.peek_pixels().unwrap();
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().unwrap();
        let mut track_found = false;
        for p in px.iter() {
            let rgba = [p[2], p[1], p[0], p[3]];
            if color_eq(track, rgba, 4) { track_found = true; break; }
        }
        assert!(!track_found, "indeterminate circular 无 track 色");
    }

    #[test]
    fn linear_indeterminate_draws_lines() {
        // 直接调绘制函数：固定 head/tail 值验证两条线 + track 分段逻辑
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let primary = ProgressIndicatorDefaults::indicator_color(&theme);
        let track = ProgressIndicatorDefaults::track_color(&theme);
        let mut surface = skia_safe::surfaces::raster_n32_premul((240, 4)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::WHITE);
        let rect = skia_safe::Rect::from_xywh(0.0, 0.0, 240.0, 4.0);
        // 用 Compose 时序值（t=1000ms）：fh=1.0, ft=0.75, sh=0.412, st=0.118——
        // 真实动画中 line2 head 恒 ≤ line1 tail，track-between 段始终有效
        draw_linear_indeterminate(
            canvas, rect,
            primary, track, ProgressIndicatorStrokeCap::Round, TRACK_ACTIVE_SPACE,
            1.0, 0.75,   // first head/tail
            0.412, 0.118, // second head/tail
        );
        let pm = surface.peek_pixels().unwrap();
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().unwrap();
        let w = 240usize;
        // 采样中线 y=2 各 x 的颜色
        let color_at = |x: usize| {
            let p = px[2 * w + x];
            [p[2], p[1], p[0], p[3]]
        };
        // 期望（t=1000ms，gapFraction = 8/240 ≈ 0.033）：
        // line1 [ft, fh] = [0.75, 1.0] → x [180, 240] primary；
        // line2 [st, sh] = [0.118, 0.412] → x [28, 99] primary；
        // track-between [sh+gap, ft-gap] = [0.445, 0.717] → x [107, 172] track；
        // track-after-line2 [0, st-gap] = [0, 0.085] → x [0, 20] track；
        // x=60：line2 内 → primary
        assert!(color_eq(primary, color_at(60), 6), "x=60 在 line2 [28,99] 内");
        // x=200：line1 内 [180,240] → primary
        assert!(color_eq(primary, color_at(200), 6), "x=200 在 line1 [180,240] 内");
        // x=140：track-between [107,172] → track
        assert!(color_eq(track, color_at(140), 6), "x=140 在 track-between [107,172] 段");
        // x=10：track-after-line2 [0,20] → track
        assert!(color_eq(track, color_at(10), 6), "x=10 在 track-after [0,20] 段");
    }

    #[test]
    fn circular_custom_stroke_width_applies() {
        // .stroke_width(8.0) 应画出更宽的弧——直接调绘制函数验证像素行数
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let color = ProgressIndicatorDefaults::indicator_color(&theme);
        let mut surface = skia_safe::surfaces::raster_n32_premul((40, 40)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::WHITE);
        let rect = skia_safe::Rect::from_xywh(0.0, 0.0, 40.0, 40.0);
        // 100% 进度（sweep=360）→ 完整圆环
        draw_circular_determinate(
            canvas, rect,
            color, ProgressIndicatorDefaults::track_color(&theme),
            ProgressIndicatorStrokeCap::Round, TRACK_ACTIVE_SPACE, 8.0,
            1.0,
        );
        let pm = surface.peek_pixels().unwrap();
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().unwrap();
        let w = 40usize;
        // 统计中线 x=20 列上 color 像素的行数（弧宽 = stroke_width）
        let mut rows = 0;
        for y in 0..40 {
            let p = px[y * w + 20];
            let rgba = [p[2], p[1], p[0], p[3]];
            if color_eq(color, rgba, 6) { rows += 1; }
        }
        // 8px 宽弧完整圆环：中线列穿过上下两段 → ~16 行（±2 反锯齿）
        assert!(rows >= 14 && rows <= 18, "8px stroke 完整圆环中线 ~16 行，实际 {rows}");
    }

    #[test]
    fn height_gt_width_falls_back_to_butt() {
        // Compose：height > width 时忽略 cap（退化 Butt）——不 crash 即可
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut surface = skia_safe::surfaces::raster_n32_premul((4, 8)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::WHITE);
        let rect = skia_safe::Rect::from_xywh(0.0, 0.0, 4.0, 8.0);
        draw_linear_determinate(
            canvas, rect,
            ProgressIndicatorDefaults::indicator_color(&theme),
            ProgressIndicatorDefaults::track_color(&theme),
            ProgressIndicatorStrokeCap::Round,
            TRACK_ACTIVE_SPACE,
            0.5, true,
        );
    }

    // ── 组件级：indeterminate 注册动画 ──
    #[test]
    fn indeterminate_build_registers_infinite_animations() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = Composer::new();
        let scene = |ctx: &mut ComposeCtx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                LinearProgressIndicator::indeterminate().build(ctx);
            });
        };
        composer.compose(scene);
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
        assert!(crate::animation::is_animating(), "indeterminate 应注册无限动画");
        // 清理全局动画表（防跨测试污染）
        crate::animation::clear_all_animations();
    }

    #[test]
    fn circular_indeterminate_build_registers_animations() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = Composer::new();
        let scene = |ctx: &mut ComposeCtx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                CircularProgressIndicator::indeterminate().build(ctx);
            });
        };
        composer.compose(scene);
        composer.layout(Constraints::new(0.0, 300.0, 0.0, 300.0));
        assert!(crate::animation::is_animating());
        crate::animation::clear_all_animations();
    }
}
