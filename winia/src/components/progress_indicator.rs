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

use crate::runtime::composer::{ComposeCtx, GroupStatus};
use crate::composable;
use crate::layout::BoxLayout;
use crate::modifier::{Modifier};
use crate::graphics::{Color};
use crate::theme::{ThemeColors, WiniaTheme};
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
    pub(crate) fn to_skia(self) -> skia_safe::paint::Cap {
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
pub(crate) fn linear_first_line_head_spec() -> InfiniteRepeatableSpec {
    linear_line_spec(0, 1000)
}

/// Linear first line tail：250ms 起 1000ms 到 1
pub(crate) fn linear_first_line_tail_spec() -> InfiniteRepeatableSpec {
    linear_line_spec(250, 1000)
}

/// Linear second line head：650ms 起 850ms 到 1
pub(crate) fn linear_second_line_head_spec() -> InfiniteRepeatableSpec {
    linear_line_spec(650, 850)
}

/// Linear second line tail：900ms 起 850ms 到 1
pub(crate) fn linear_second_line_tail_spec() -> InfiniteRepeatableSpec {
    linear_line_spec(900, 850)
}

/// Circular 全局旋转：0→1080° 线性 6000ms（`circularIndeterminateGlobalRotationAnimationSpec`）
pub(crate) fn circular_global_rotation_spec() -> InfiniteRepeatableSpec {
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
pub(crate) fn circular_additional_rotation_spec() -> InfiniteRepeatableSpec {
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
pub(crate) fn circular_progress_spec() -> InfiniteRepeatableSpec {
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
        ctx.changed(&self.draw_stop_indicator);
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
                .draw_node(LinearIndeterminateNode {
                    color, track_color, cap, gap,
                    fh: fh.clone(), ft: ft.clone(), sh: sh.clone(), st: st.clone(),
                })
        } else {
            let progress = self.progress;
            let draw_stop = self.draw_stop_indicator;
            Modifier::new()
                .size(LINEAR_INDICATOR_WIDTH, LINEAR_INDICATOR_HEIGHT)
                .draw_node(LinearDeterminateNode {
                    color, track_color, cap, gap, progress, draw_stop,
                })
        };

        // Accessibility: a progress bar that reports only its role tells a screen reader nothing —
        // the value is what the widget IS. Determinate bars report it (Compose's
        // `ProgressBarRangeInfo`); an indeterminate one has no value to report, which is not the same
        // as zero, so it stays silent about progress and announces itself as a progress bar only.
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

/// Linear indeterminate 绘制节点（exp/progress-node 迁移探针：原 `.draw` 匿名闭包）。
///
/// 照抄 `SliderTrackNode` 形状（`slider.rs`）：具名 struct + `DrawNode`，`draw` 内
/// 复用原绘制函数，`node_key` 纳入全部静态视觉参数。4 个无限动画值
/// （fh/ft/sh/st）是逐帧 peek 的瞬态值，MUST NOT 进 key（进则每帧 Enter；
/// 重绘由动画引擎每帧 `request_redraw` 驱动，与 Skip 无关——本探针验证此约定）。
#[derive(Debug)]
pub(crate) struct LinearIndeterminateNode {
    pub(crate) color: Color,
    pub(crate) track_color: Color,
    pub(crate) cap: ProgressIndicatorStrokeCap,
    pub(crate) gap: f32,
    /// 无限动画 4 条线进度（渲染期 peek，不进 key）
    pub(crate) fh: crate::runtime::state::Visual<f32>,
    pub(crate) ft: crate::runtime::state::Visual<f32>,
    pub(crate) sh: crate::runtime::state::Visual<f32>,
    pub(crate) st: crate::runtime::state::Visual<f32>,
}

impl crate::modifier::DrawNode for LinearIndeterminateNode {
    fn draw(&self, canvas: &skia_safe::Canvas, rect: skia_safe::Rect) {
        draw_linear_indeterminate(
            canvas, rect,
            self.color, self.track_color, self.cap, self.gap,
            self.fh.peek(), self.ft.peek(), self.sh.peek(), self.st.peek(),
        );
    }
    fn node_key(&self) -> String {
        format!(
            "linear-indeterminate:{:?}:{:?}:{}:{}",
            self.color,
            self.track_color,
            self.cap as u8,
            self.gap.to_bits(),
        )
    }
}

/// Linear determinate 绘制节点（静态 progress，无动画值——progress 进 key）。
#[derive(Debug)]
pub(crate) struct LinearDeterminateNode {
    pub(crate) color: Color,
    pub(crate) track_color: Color,
    pub(crate) cap: ProgressIndicatorStrokeCap,
    pub(crate) gap: f32,
    pub(crate) progress: f32,
    pub(crate) draw_stop: bool,
}

impl crate::modifier::DrawNode for LinearDeterminateNode {
    fn draw(&self, canvas: &skia_safe::Canvas, rect: skia_safe::Rect) {
        draw_linear_determinate(
            canvas, rect,
            self.color, self.track_color, self.cap, self.gap,
            self.progress, self.draw_stop,
        );
    }
    fn node_key(&self) -> String {
        format!(
            "linear-determinate:{:?}:{:?}:{}:{}:{}:{}",
            self.color,
            self.track_color,
            self.cap as u8,
            self.gap.to_bits(),
            self.progress.to_bits(),
            self.draw_stop,
        )
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
                .draw_node(CircularIndeterminateNode {
                    color, cap, stroke_width,
                    global: global.clone(),
                    additional: additional.clone(),
                    progress_anim: progress_anim.clone(),
                })
        } else {
            let progress = self.progress;
            Modifier::new()
                .size(CIRCULAR_INDICATOR_DIAMETER, CIRCULAR_INDICATOR_DIAMETER)
                .draw_node(CircularDeterminateNode {
                    color, track_color, cap, gap, stroke_width, progress,
                })
        };

        // Same rule as the linear indicator: the value is the point of the widget.
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

/// Circular determinate 绘制节点（静态 progress，进 key）。
#[derive(Debug)]
pub(crate) struct CircularDeterminateNode {
    pub(crate) color: Color,
    pub(crate) track_color: Color,
    pub(crate) cap: ProgressIndicatorStrokeCap,
    pub(crate) gap: f32,
    pub(crate) stroke_width: f32,
    pub(crate) progress: f32,
}

impl crate::modifier::DrawNode for CircularDeterminateNode {
    fn draw(&self, canvas: &skia_safe::Canvas, rect: skia_safe::Rect) {
        draw_circular_determinate(
            canvas, rect,
            self.color, self.track_color, self.cap, self.gap,
            self.stroke_width, self.progress,
        );
    }
    fn node_key(&self) -> String {
        format!(
            "circular-determinate:{:?}:{:?}:{}:{}:{}:{}",
            self.color,
            self.track_color,
            self.cap as u8,
            self.gap.to_bits(),
            self.stroke_width.to_bits(),
            self.progress.to_bits(),
        )
    }
}

/// Circular indeterminate 绘制节点（3 个无限动画值 peek，不进 key；
/// `_gap_size` 在原函数即无用（无 track），故不存）。
#[derive(Debug)]
pub(crate) struct CircularIndeterminateNode {
    pub(crate) color: Color,
    pub(crate) cap: ProgressIndicatorStrokeCap,
    pub(crate) stroke_width: f32,
    pub(crate) global: crate::runtime::state::Visual<f32>,
    pub(crate) additional: crate::runtime::state::Visual<f32>,
    pub(crate) progress_anim: crate::runtime::state::Visual<f32>,
}

impl crate::modifier::DrawNode for CircularIndeterminateNode {
    fn draw(&self, canvas: &skia_safe::Canvas, rect: skia_safe::Rect) {
        draw_circular_indeterminate(
            canvas, rect,
            self.color, self.cap, 0.0,
            self.stroke_width,
            self.global.peek(), self.additional.peek(), self.progress_anim.peek(),
        );
    }
    fn node_key(&self) -> String {
        format!(
            "circular-indeterminate:{:?}:{}:{}",
            self.color,
            self.cap as u8,
            self.stroke_width.to_bits(),
        )
    }
}


#[cfg(test)]
mod tests;
