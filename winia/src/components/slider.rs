//! Slider 组件 — 对标 material3 `Slider`（新版 M3 token v2_3_5）
//!
//! M3 实现要点（对齐项）：
//! - Track：16dp 高、**两段独立胶囊**（`ActiveTrackHeight/InactiveTrackHeight`）——
//!   active（Primary）从起点到 thumb 左侧、inactive（SecondaryContainer）从
//!   thumb 右侧到终点，**与 thumb 保持 6dp 间隙**（`ThumbTrackGapSize`），
//!   外端全圆 8dp、内端 2dp 小圆角（`TrackInsideCornerSize`）；
//! - Thumb：4×44 胶囊（`HandleWidth×HandleHeight`）、Primary；
//!   交互中（press/drag/focus）宽度减半为 2dp（Compose ThumbContent 语义）；
//! - Steps 离散刻度：steps+2 个 4dp 圆点（`StopIndicatorSize`）——
//!   active 区用 InactiveTrackColor、inactive 区用 ActiveTrackColor（对比交叉）；
//! - 交互：点击跳转（TapOnTap）+ 拖动（DragOnStart/Move/End）+ 键盘步进
//!   （方向键 1 步、PageUp/Down 大步、Home/End 端点）；
//! - 触摸目标 48dp 高（`minimumInteractiveComponentSize` 语义）。
//!
//! 架构（exp/modifier-node 首个真实迁移）：轨道绘制经 `SliderTrackNode`
//! （DrawNode）挂载——具名类型（调试树可见）、`node_key` 精确 Skip、
//! 绘制参数结构体化可单测。原 `Modifier::draw` 匿名闭包已替换。

use crate::runtime::composer::{ComposeCtx, GroupStatus};
use crate::composable;
use crate::layout::BoxLayout;
use crate::modifier::{Modifier};
use crate::input::{KbEvent, KbEventType};
use crate::graphics::{Color};
use crate::interaction::MutableInteractionSource;
use crate::theme::{ThemeColors, WiniaTheme};
use std::sync::Arc;

/// 轨道高度（`SliderTokens.ActiveTrackHeight/InactiveTrackHeight = 16dp`）
pub const SLIDER_TRACK_HEIGHT: f32 = 16.0;
/// 拇指宽度（`SliderTokens.HandleWidth = 4dp`）
pub const SLIDER_THUMB_WIDTH: f32 = 4.0;
/// 拇指高度（`SliderTokens.HandleHeight = 44dp`）
pub const SLIDER_THUMB_HEIGHT: f32 = 44.0;
/// 交互中拇指宽度（Compose ThumbContent：interactions 非空时宽度减半）
pub const SLIDER_ACTIVE_THUMB_WIDTH: f32 = 2.0;
/// 刻度点直径（`SliderTokens.StopIndicatorSize = 4dp`）
pub const SLIDER_TICK_SIZE: f32 = 4.0;
/// 拇指与轨道间隙（`SliderTokens.ActiveHandleLeadingSpace = 6dp`——
/// M3 轨道为两段独立胶囊，与 thumb 保持间隙）
pub const SLIDER_THUMB_GAP: f32 = 6.0;
/// 轨道内侧圆角（`SliderTokens.TrackInsideCornerSize = 2dp`——靠近拇指端）
pub const SLIDER_TRACK_INSIDE_CORNER: f32 = 2.0;
/// 触摸目标高度（`minimumInteractiveComponentSize` 语义 = 48dp）
pub const SLIDER_TOUCH_HEIGHT: f32 = 48.0;
/// 无 steps 时的键盘步进（1% 值域——Compose `actualSteps = 100`）
pub const SLIDER_KEYBOARD_DEFAULT_STEPS: i32 = 100;

/// 滑块颜色集（对标 material3 `SliderColors` 10 字段——thumb/track/tick ×
/// enabled/disabled × active/inactive）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SliderColors {
    pub thumb_color: Color,
    pub active_track_color: Color,
    pub active_tick_color: Color,
    pub inactive_track_color: Color,
    pub inactive_tick_color: Color,
    pub disabled_thumb_color: Color,
    pub disabled_active_track_color: Color,
    pub disabled_active_tick_color: Color,
    pub disabled_inactive_track_color: Color,
    pub disabled_inactive_tick_color: Color,
}

impl SliderColors {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        thumb_color: Color,
        active_track_color: Color,
        active_tick_color: Color,
        inactive_track_color: Color,
        inactive_tick_color: Color,
        disabled_thumb_color: Color,
        disabled_active_track_color: Color,
        disabled_active_tick_color: Color,
        disabled_inactive_track_color: Color,
        disabled_inactive_tick_color: Color,
    ) -> Self {
        Self {
            thumb_color,
            active_track_color,
            active_tick_color,
            inactive_track_color,
            inactive_tick_color,
            disabled_thumb_color,
            disabled_active_track_color,
            disabled_active_tick_color,
            disabled_inactive_track_color,
            disabled_inactive_tick_color,
        }
    }

    /// 从主题推导默认色（对标 Compose `defaultSliderColors`：tick 色交叉——
    /// active 区 tick 用 InactiveTrackColor、inactive 区用 ActiveTrackColor，
    /// 保证 tick 在浅/深轨道上都可见）
    pub fn from_theme(theme: &ThemeColors) -> Self {
        let alpha = |c: Color, a: f32| Color::from_argb((c.a as f32 * a) as u8, c.r, c.g, c.b);
        Self::new(
            theme.primary,
            theme.primary,
            theme.secondary_container,
            theme.secondary_container,
            theme.primary,
            alpha(theme.on_surface, 0.38),
            alpha(theme.on_surface, 0.38),
            alpha(theme.on_surface, 0.12),
            alpha(theme.on_surface, 0.12),
            alpha(theme.on_surface, 0.38),
        )
    }

    /// 拇指色（enabled × disabled）
    pub fn thumb_color(&self, enabled: bool) -> Color {
        if enabled { self.thumb_color } else { self.disabled_thumb_color }
    }

    /// 轨道色（active = 值之前的已走过部分）
    pub fn track_color(&self, enabled: bool, active: bool) -> Color {
        if !enabled {
            if active { self.disabled_active_track_color } else { self.disabled_inactive_track_color }
        } else if active {
            self.active_track_color
        } else {
            self.inactive_track_color
        }
    }

    /// 刻度色（active 区 / inactive 区）
    pub fn tick_color(&self, enabled: bool, active: bool) -> Color {
        if !enabled {
            if active { self.disabled_active_tick_color } else { self.disabled_inactive_tick_color }
        } else if active {
            self.active_tick_color
        } else {
            self.inactive_tick_color
        }
    }
}

/// Slider 默认值（对标 material3 `SliderDefaults`）
pub struct SliderDefaults;

impl SliderDefaults {
    pub fn slider_colors(theme: &ThemeColors) -> SliderColors {
        SliderColors::from_theme(theme)
    }
}

// ── 值换算纯函数（对齐 Compose SliderState）──

/// 刻度分数（含两端）：steps=4 → [0, .2, .4, .6, .8, 1]（steps+2 点）
pub fn tick_fractions(steps: i32) -> Vec<f32> {
    if steps <= 0 {
        Vec::new()
    } else {
        let n = (steps + 1) as f32;
        (0..=steps + 1).map(|i| i as f32 / n).collect()
    }
}

/// 吸附到最近刻度（Compose `snapValueToTick`——无刻度时原值返回）
pub fn snap_value(value: f32, steps: i32, min: f32, max: f32) -> f32 {
    if steps <= 0 || max <= min {
        value.clamp(min, max)
    } else {
        let n = (steps + 1) as f32;
        let i = ((value - min) / (max - min) * n).round() as i32;
        (min + (max - min) * i as f32 / n).clamp(min, max)
    }
}

/// 位置 → 值（x 为节点本地坐标；steps 时吸附）。
/// thumb 活动范围 = [corner, width - corner]（corner = 轨道圆头半径 8dp）——
/// 端点时 thumb 中心正好停在 stop indicator 上（stop 在轨道端头圆心，不贴边）
pub fn value_at_x(local_x: f32, width: f32, min: f32, max: f32, steps: i32) -> f32 {
    let corner = SLIDER_TRACK_HEIGHT / 2.0;
    let track_w = width - 2.0 * corner;
    let f = if track_w > 0.0 {
        ((local_x - corner) / track_w).clamp(0.0, 1.0)
    } else {
        0.0
    };
    snap_value(min + f * (max - min), steps, min, max)
}

/// 值 → 轨道位置分数（0..1，clamp）
pub fn fraction_from_value(value: f32, min: f32, max: f32) -> f32 {
    if max <= min { 0.0 } else { ((value - min) / (max - min)).clamp(0.0, 1.0) }
}

/// Slider 组件 Builder（对标 material3 `Slider(value, onValueChange, ...)`）
/// ——受控组件：value 由调用方持有，on_value_change 更新。
pub struct Slider {
    value: f32,
    on_value_change: Option<Arc<dyn Fn(f32) + Send + Sync>>,
    on_value_change_finished: Option<Arc<dyn Fn() + Send + Sync>>,
    value_range: (f32, f32),
    steps: i32,
    enabled: bool,
    colors: Option<SliderColors>,
    interaction_source: Option<MutableInteractionSource>,
    modifier: Modifier,
}

impl Slider {
    pub fn new(value: f32) -> Self {
        Self {
            value,
            on_value_change: None,
            on_value_change_finished: None,
            value_range: (0.0, 1.0),
            steps: 0,
            enabled: true,
            colors: None,
            interaction_source: None,
            modifier: Modifier::new(),
        }
    }

    /// 值变化回调（拖动/点击/键盘每步调用）
    pub fn on_value_change(mut self, f: impl Fn(f32) + Send + Sync + 'static) -> Self {
        self.on_value_change = Some(Arc::new(f));
        self
    }

    /// 值变化结束回调（拖动结束/点击完成/键盘每步的 KeyUp）
    pub fn on_value_change_finished(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_value_change_finished = Some(Arc::new(f));
        self
    }

    /// 值域（对标 Compose `valueRange`——winia 无 Range 类型用元组）
    pub fn value_range(mut self, min: f32, max: f32) -> Self {
        self.value_range = (min, max);
        self
    }

    /// 离散步数（>0 时离散，允许值 = 两端之间的 steps 个等距值；0 = 连续）
    pub fn steps(mut self, steps: i32) -> Self {
        self.steps = steps.max(0);
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn colors(mut self, colors: SliderColors) -> Self {
        self.colors = Some(colors);
        self
    }

    /// 注入交互源（hoist——press/hover/focus/drag 状态发射到此源）
    pub fn interaction_source(mut self, source: MutableInteractionSource) -> Self {
        self.interaction_source = Some(source);
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        ctx.changed(&self.value);
        ctx.changed(&self.enabled);
        ctx.changed(&self.steps);
        ctx.changed(&self.value_range);
        ctx.changed(&self.colors);
        let key = ctx.next_key();
        let theme = WiniaTheme::colors();
        let colors = self.colors.unwrap_or_else(|| SliderDefaults::slider_colors(&theme));
        let interaction = self.interaction_source
            .unwrap_or_else(|| ctx.remember(|| MutableInteractionSource::new()).get());
        let (min, max) = self.value_range;
        let (min, max) = if max > min { (min, max) } else { (min, min + 1.0) };
        let steps = self.steps;
        // A discrete slider only ever SHOWS an on-tick value: Compose snaps in `SliderState`'s
        // setter, so a caller-supplied value lands on the nearest tick as well, not just a dragged
        // one. Without `steps` this is the plain clamp.
        let value = snap_value(self.value.clamp(min, max), steps, min, max);
        let enabled = self.enabled;

        // 交互状态：拇指宽度减半（press/drag/focus——对齐 ThumbContent）
        let st = interaction.state(enabled);
        let thumb_active = st.pressed || st.focused || st.dragged;

        // Track width: written back at render time by the draw closure
        // (Backchannel — no recompose); gesture callbacks (tap/drag) read it
        // for px<->value conversion (no onSizeChanged — the draw closure is
        // the only composition-side channel that sees node size).
        let track_width = ctx.remember_backchannel(|| 0.0f32);
        let tw_tap = track_width.clone();
        let tw_drag = track_width.clone();
        let tw_press = track_width.clone();

        // 值换算闭包（拖动/点击共用）
        let set_value = self.on_value_change.clone();
        let finished = self.on_value_change_finished.clone();

        let mut m = Modifier::new()
            .fill_max_width()
            .min_height(SLIDER_TOUCH_HEIGHT)
            // 轨道/刻度/拇指——SliderTrackNode 具名绘制（exp/modifier-node 迁移：
            // 原 .draw 匿名闭包。宽度回写（track_width）与焦点读取移入 node 内，
            // build 侧只组参数——绘制参数结构体化，node_key 精确 Skip）。
            // no_focus_ring：焦点环自绘（包围 thumb 胶囊，而非整个组件）
            .no_focus_ring()
            .draw_node(SliderTrackNode {
                track_width: track_width.clone(),
                interaction: interaction.clone(),
                colors,
                enabled,
                value,
                min,
                max,
                steps,
                thumb_active,
            });

        if enabled {
            // 按下：立即跳转到按下位置（用户规范——不等 tap/drag）+ 发射 press
            // （thumb 宽度减半——对齐 Compose ThumbContent 收集 PressInteraction；
            // slider 未挂 ripple，ripple 层仅内部数据无害）
            let src = interaction.clone();
            let v_press = set_value.clone();
            m = m.on_press(move |pos| {
                src.emit_press_at(pos);
                if let Some(cb) = &v_press { cb(value_at_x(pos.0, tw_press.get(), min, max, steps)); }
            });
            // 点击跳转（TapOnTap——点击位置即新值，对齐 Compose onTap）+ 释放
            let v = set_value.clone();
            let f = finished.clone();
            let tw = tw_tap;
            let src2 = interaction.clone();
            m = m.on_tap(move |pos| {
                src2.emit_release();
                if let Some(cb) = &v { cb(value_at_x(pos.0, tw.get(), min, max, steps)); }
                if let Some(cb) = &f { cb(); }
            });
            // 拖动（对齐 Compose draggable：start 跳转 + move 绝对位置跟随）
            // drag 发射 dragged 状态（thumb 宽减半）——手势回调不自动 emit
            let v2 = set_value.clone();
            let f2 = finished.clone();
            let tw2 = tw_drag.clone();
            let src3 = interaction.clone();
            m = m.on_drag_start(move |pos| {
                src3.emit_drag_start();
                if let Some(cb) = &v2 { cb(value_at_x(pos.0, tw2.get(), min, max, steps)); }
            });
            let v3 = set_value.clone();
            m = m.on_drag(move |pos, _delta| {
                if let Some(cb) = &v3 { cb(value_at_x(pos.0, tw_drag.get(), min, max, steps)); }
            });
            let src4 = interaction.clone();
            m = m.on_drag_end(move || {
                src4.emit_drag_end();
                src4.emit_release();
                if let Some(cb) = &f2 { cb(); }
            });
            let src5 = interaction.clone();
            m = m.on_drag_cancel(move || {
                src5.emit_drag_end();
                src5.emit_release();
            });
        }

        if enabled {
            let v4 = set_value.clone();
            let f4 = finished.clone();
            m = m
                .focusable_with_source(&interaction)
                .on_key_event(move |ke| handle_key(ke, value, min, max, steps, &v4, &f4));
        }
        m = m.then(self.modifier);

        match ctx.start_restartable_group(
            key,
            m,
            BoxLayout::new().alignment(crate::layout::Alignment::Center),
        ) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {}
        }
        ctx.set_current_node_focus_color(theme.primary);
        ctx.end_restartable_group();
    }
}

/// 键盘步进（对齐 Compose `slideOnKeyEvents`——KeyDown 处理、KeyUp 回调 finished）
pub(crate) fn handle_key(
    ke: &KbEvent,
    value: f32,
    min: f32,
    max: f32,
    steps: i32,
    on_value_change: &Option<Arc<dyn Fn(f32) + Send + Sync>>,
    on_value_change_finished: &Option<Arc<dyn Fn() + Send + Sync>>,
) -> bool {
    use winit::keyboard::{Key, NamedKey};
    let range_len = max - min;
    let actual_steps = if steps > 0 { steps + 1 } else { SLIDER_KEYBOARD_DEFAULT_STEPS };
    let delta = range_len / actual_steps as f32;
    let page = ((actual_steps / 10).clamp(1, 10)) as f32;
    let call = |v: f32| {
        if let Some(cb) = on_value_change {
            cb(v.clamp(min, max));
        }
    };
    match ke.event_type {
        KbEventType::KeyDown => {
            match &ke.key {
                Key::Named(NamedKey::Home) => { call(min); true }
                Key::Named(NamedKey::End) => { call(max); true }
                Key::Named(NamedKey::ArrowRight) => { call(value + delta); true }
                Key::Named(NamedKey::ArrowLeft) => { call(value - delta); true }
                Key::Named(NamedKey::PageUp) => { call(value + page * delta); true }
                Key::Named(NamedKey::PageDown) => { call(value - page * delta); true }
                _ => false,
            }
        }
        KbEventType::KeyUp => {
            let consumed = matches!(&ke.key,
                Key::Named(NamedKey::Home | NamedKey::End | NamedKey::ArrowRight | NamedKey::ArrowLeft
                    | NamedKey::PageUp | NamedKey::PageDown));
            if consumed {
                if let Some(cb) = on_value_change_finished { cb(); }
            }
            consumed
        }
        _ => false,
    }
}

/// 轨道绘制节点（exp/modifier-node 首个真实迁移）：`Modifier::draw` 匿名闭包的
/// 具名等价物。绘制几何见 [`draw_slider`]。`node_key` 纳入静态视觉参数
/// （值/颜色/开关/源身份），回写通道与瞬态动画值排除（见 `node_key` 注释）。
/// `track_width` 回写（像素↔值换算通道）保留在 node 内（Backchannel，不触发重组）。
///
/// 可见性（P1-4）：`pub(crate)`——第三方照抄形状自定节点类型，不复用本节点
/// （value 未 clamp、`min>max` 未归一——归一在 `build` 侧，不在 node 内）。
#[derive(Debug)]
pub(crate) struct SliderTrackNode {
    /// 轨道宽度回写（tap/drag 像素↔值换算读此值）。
    pub(crate) track_width: crate::runtime::state::Backchannel<f32>,
    /// 绘制用交互源（渲染期读焦点/波纹状态——peek，不注册依赖）。
    pub(crate) interaction: MutableInteractionSource,
    pub(crate) colors: SliderColors,
    pub(crate) enabled: bool,
    pub(crate) value: f32,
    pub(crate) min: f32,
    pub(crate) max: f32,
    pub(crate) steps: i32,
    pub(crate) thumb_active: bool,
}

impl crate::modifier::DrawNode for SliderTrackNode {
    fn draw(&self, canvas: &skia_safe::Canvas, rect: skia_safe::Rect) {
        self.track_width.set(rect.width());
        // 渲染期 peek（P1-4）：get 在渲染期注册不上依赖（已出依赖帧），用 peek
        // 语义诚实；focused 经 thumb_active 间接进 key（build 期），focus_alpha
        // 为瞬态动画值故意不进 key（见 node_key 注释）。
        let focused = self.interaction.is_focused_value();
        let focus_alpha = self.interaction.focus_indicator_alpha_value();
        draw_slider(
            canvas, rect, &self.colors, self.enabled, self.value,
            self.min, self.max, self.steps, self.thumb_active, focused, focus_alpha,
        );
    }
    fn node_key(&self) -> String {
        // 静态视觉参数进 key（颜色/开关/值域/步数/thumb 状态/源身份）；
        // track_width 回写通道不进；瞬态动画值 focus_alpha 不进（逐帧 peek 直读，
        // 进 key 则动画每帧 Enter——与 Background 色闭包/GraphicsLayer 同惯例）；
        // focused 不直接进（经 thumb_active = pressed||focused||dragged 间接覆盖，
        // build 期 interaction.state() 已注册依赖）。
        // -0.0/0.0 key 不同但同画（保守多 Enter，无害）；NaN 下 changed 本就恒
        // dirty（P2-3）。value 由 build 侧 clamp，min<=max 归一亦在 build 侧。
        format!(
            "slidertrack:{:?}:{}:{}:{}:{}:{}:{}:{}",
            self.colors,
            self.enabled,
            self.value.to_bits(),
            self.min.to_bits(),
            self.max.to_bits(),
            self.steps,
            self.thumb_active,
            self.interaction.source_id(),
        )
    }
}

/// One thumb of a slider track: where it sits, plus the two transient facts about it.
///
/// `value` is in the slider's own range (the caller clamps and normalises it — the track does not
/// re-do that), `active` halves the thumb's width while its own gesture runs (Compose's
/// `ThumbContent`), and `focused` asks for the focus ring around THIS thumb — the ring wraps the
/// thumb capsule, never the whole component rect.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TrackThumb {
    pub value: f32,
    pub active: bool,
    pub focused: bool,
}

/// Draw the parts of a slider track that are NOT thumbs: the segments, the ticks and the stop
/// indicators, for the span between `start_value` and `end_value`.
///
/// Split out of [`draw_track`] for the range slider, whose two thumbs are their own nodes (each
/// focusable — see `crate::components::range_slider`) so it needs exactly this part. M3 models the track as
/// **independent capsules** (Compose `drawTrack`):
/// - left segment `[track_left, start_pos - gap]` — the ACTIVE track when `single_sided` (a single
///   slider: everything left of its thumb is active), the inactive one for a range;
/// - active segment `[start_pos + gap, end_pos - gap]` — from `track_left` when a single slider's
///   span starts there;
/// - right segment `[end_pos + gap, track_right]` — inactive.
/// - A segment takes the full-round corner (`= track height / 2`) of a track end it reaches and the
///   2 dp inside corner of one it faces, and a segment shorter than the round end it owns is not
///   drawn at all. That one rule reproduces the single-slider look and keeps a thumb's gap clean.
/// - `gap = thumb half width + 6 dp` (`ThumbTrackGapSize`).
/// - Thumb centre `= corner + (w - 2 × corner) × fraction`, so a thumb at an end of the range stops
///   exactly on its stop indicator — continuous and discrete alike.
/// - With `steps`, the `steps + 2` ticks sit on that same inset axis; a tick is active inside the
///   active segment. A tick or stop a thumb would cover is not drawn, and an outer stop is skipped
///   when its segment is too short to have left it visible.
///
/// `thumb_values` are the values whose thumbs the caller draws (this function draws none of them):
/// they are what "a thumb would cover" is measured against.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_track_body(
    canvas: &skia_safe::Canvas,
    rect: skia_safe::Rect,
    colors: &SliderColors,
    enabled: bool,
    start_value: f32,
    end_value: f32,
    min: f32,
    max: f32,
    steps: i32,
    single_sided: bool,
    thumb_values: &[f32],
) {
    let w = rect.width();
    let h = rect.height();
    if w <= 0.0 || h <= 0.0 || thumb_values.is_empty() { return; }
    let cy = rect.top + h / 2.0;

    let inactive = colors.track_color(enabled, false);
    let active = colors.track_color(enabled, true);
    let inactive_tick = colors.tick_color(enabled, false);
    let active_tick = colors.tick_color(enabled, true);

    let corner = SLIDER_TRACK_HEIGHT / 2.0;
    let inside = SLIDER_TRACK_INSIDE_CORNER;
    let end_gap = SLIDER_THUMB_WIDTH / 2.0 + SLIDER_THUMB_GAP;
    let track_left = rect.left;
    let track_right = rect.right;
    let track_w = w;
    // Thumb centre = corner + (w - 2 × corner) × fraction (an end value lands on its stop).
    let pos_of = |v: f32| {
        track_left + corner + (track_w - 2.0 * corner) * fraction_from_value(v, min, max)
    };
    let has_ticks = steps > 0;

    let start_pos = pos_of(start_value);
    let end_pos = pos_of(end_value);

    // ── Left segment: [track_left, start_pos - gap] ──
    // Drawn (with its stop indicator) only when it is longer than the round end it owns, and the
    // round end is ALWAYS required: Compose's `Track` uses its default `enableCornerShrinking = false`
    // for both sliders (Slider.kt's `Track` overloads; the shrinking behaviour belongs to the
    // non-default overload), so `!enableCornerShrinking || tickFractions.isNotEmpty()` is true either
    // way and `gap + corner` is the threshold with and without ticks. An earlier version of this
    // function dropped the corner when a track had no ticks, which drew sub-corner slivers near the
    // ends that Compose does not.
    let threshold = corner;
    let left_seg_end = start_pos - end_gap;
    if left_seg_end > track_left + threshold {
        let color = if single_sided { &active } else { &inactive };
        draw_track_path(canvas, track_left, left_seg_end, cy, SLIDER_TRACK_HEIGHT, corner, inside, color);
    }

    // ── Active segment ──
    // A single slider's active track starts at the track's left end (Compose's `activeTrackStart =
    // 0f`) and takes its full-round corner; everything else — the right end of a single slider and
    // BOTH ends of a range — stops one thumb gap short of its thumb, so the thumb reads as the end
    // of the fill and the last `gap` pixels stay clear. That is the whole rule; a range that reaches
    // an end is not special-cased, which is what makes a range look like a single slider at its ends.
    let active_left = if single_sided { track_left } else { start_pos + end_gap };
    let active_right = end_pos - end_gap;
    let left_r = if active_left <= track_left { corner } else { inside };
    let right_r = if active_right >= track_right { corner } else { inside };
    if active_right - active_left > left_r.max(right_r) {
        draw_track_path(canvas, active_left, active_right, cy, SLIDER_TRACK_HEIGHT, left_r, right_r, &active);
    }

    // ── Right segment: [end_pos + gap, track_right] ──
    let right_seg_start = end_pos + end_gap;
    if right_seg_start < track_right - threshold {
        draw_track_path(canvas, right_seg_start, track_right, cy, SLIDER_TRACK_HEIGHT, inside, corner, &inactive);
    }

    // ── Ticks (steps + 2 dots) on the inset axis, active inside the active segment ──
    // A tick under a thumb is not drawn (the thumb would sit on top of the stop dot).
    let overlap_thumb = |x: f32| thumb_values.iter().any(|v| {
        (x - pos_of(*v)).abs() < (SLIDER_THUMB_WIDTH + SLIDER_TICK_SIZE) / 2.0
    });
    if has_ticks {
        let radius = SLIDER_TICK_SIZE / 2.0;
        let mut tp = skia_safe::Paint::default();
        tp.set_anti_alias(true);
        for f in tick_fractions(steps).iter() {
            let x = track_left + corner + (track_w - 2.0 * corner) * f;
            if overlap_thumb(x) { continue; }
            let color = if x >= active_left && x <= active_right { active_tick } else { inactive_tick };
            tp.set_color(skia_color(color));
            canvas.draw_circle(skia_safe::Point::new(x, cy), radius, &tp);
        }
    }

    // ── Stop indicators (4 dp dot at each track end's centre) ──
    // Start end: the active-tick color when the track has ticks (so it matches its neighbours),
    // the active track color otherwise. End end: the active track color — with ticks that IS the
    // inactive tick color, so it still matches the ticks around it.
    let stop_start_c = if has_ticks {
        colors.tick_color(enabled, true)
    } else {
        colors.track_color(enabled, true)
    };
    let stop_end_c = colors.track_color(enabled, true);
    let mut sp = skia_safe::Paint::default();
    sp.set_anti_alias(true);
    if left_seg_end > track_left + threshold && !overlap_thumb(track_left + corner) {
        sp.set_color(skia_color(stop_start_c));
        canvas.draw_circle(skia_safe::Point::new(track_left + corner, cy), SLIDER_TICK_SIZE / 2.0, &sp);
    }
    if right_seg_start < track_right - threshold && !overlap_thumb(track_right - corner) {
        sp.set_color(skia_color(stop_end_c));
        canvas.draw_circle(skia_safe::Point::new(track_right - corner, cy), SLIDER_TICK_SIZE / 2.0, &sp);
    }
}

/// Draw one thumb capsule centred on `(cx, cy)`, plus its focus ring.
///
/// Shared by the single slider (which draws its one thumb from [`draw_track`]) and the range slider
/// (whose thumbs are their own nodes and call this with their node's centre).
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_thumb(
    canvas: &skia_safe::Canvas,
    cx: f32,
    cy: f32,
    colors: &SliderColors,
    enabled: bool,
    active: bool,
    focused: bool,
    focus_alpha: f32,
) {
    let thumb_c = colors.thumb_color(enabled);
    // ⚠ `cx` is absolute — the callers add their own origin, and adding one twice is exactly the
    // bug that drew a thumb right of its track (2026-08 debug-server measurement).
    let thumb_w = if active { SLIDER_ACTIVE_THUMB_WIDTH } else { SLIDER_THUMB_WIDTH };
    let thumb_h = SLIDER_THUMB_HEIGHT;
    let rrect = skia_safe::RRect::new_rect_xy(
        skia_safe::Rect::from_xywh(cx - thumb_w / 2.0, cy - thumb_h / 2.0, thumb_w, thumb_h),
        thumb_w / 2.0,
        thumb_w / 2.0,
    );
    let mut tp = skia_safe::Paint::default();
    tp.set_anti_alias(true);
    tp.set_color(skia_color(thumb_c));
    canvas.draw_rrect(rrect, &tp);

    // ── Focus ring around the thumb capsule (not the component rect) ──
    // The band's centre line sits at the track end's position (end_gap = 8 from the thumb centre) →
    // the band is 16 apart, the distance between the two track ends.
    if focused || focus_alpha > 0.001 {
        // ⚠ Measure from the resting thumb (4 × 44), not the narrowed `thumb_w`: with the active
        // width the band's centre line lands ±7 instead of ±8 and misses the track ends.
        let ring_rect = skia_safe::Rect::from_xywh(
            cx - SLIDER_THUMB_WIDTH / 2.0,
            cy - SLIDER_THUMB_HEIGHT / 2.0,
            SLIDER_THUMB_WIDTH,
            SLIDER_THUMB_HEIGHT,
        );
        // draw_focus puts the band's centre line `gap + ring width / 2 (1.5)` outside the rect.
        // For a centre line 8 from the thumb centre: gap = 8 - thumb half width (2) - 1.5 = 4.5.
        let end_gap = SLIDER_THUMB_WIDTH / 2.0 + SLIDER_THUMB_GAP;
        let ring_gap = end_gap - SLIDER_THUMB_WIDTH / 2.0 - 1.5;
        crate::render::draw_focus(canvas, ring_rect, &crate::graphics::Shape::Pill, None, thumb_c, focus_alpha, ring_gap);
    }
}

/// Draw a slider track: [`draw_track_body`] for the segments, then one [`draw_thumb`] per thumb.
///
/// The single slider's entry point; the range slider draws the same body from its own track node and
/// hosts its thumbs as nodes of their own.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_track(
    canvas: &skia_safe::Canvas,
    rect: skia_safe::Rect,
    colors: &SliderColors,
    enabled: bool,
    thumbs: &[TrackThumb],
    min: f32,
    max: f32,
    steps: i32,
    single_sided: bool,
    focus_alpha: f32,
) {
    if thumbs.is_empty() { return; }
    let h = rect.height();
    let cy = rect.top + h / 2.0;
    let corner = SLIDER_TRACK_HEIGHT / 2.0;
    let track_w = rect.width();
    let pos_of = |v: f32| {
        rect.left + corner + (track_w - 2.0 * corner) * fraction_from_value(v, min, max)
    };
    // The span the body draws between: the thumbs are passed sorted, so the first and last are its
    // ends (for the single slider they are the same thumb).
    let values: Vec<f32> = thumbs.iter().map(|t| t.value).collect();
    draw_track_body(
        canvas,
        rect,
        colors,
        enabled,
        thumbs[0].value,
        *values.last().unwrap(),
        min,
        max,
        steps,
        single_sided,
        &values,
    );
    for t in thumbs {
        draw_thumb(canvas, pos_of(t.value), cy, colors, enabled, t.active, t.focused, focus_alpha);
    }
}

/// Single-thumb slider = [`draw_track`] with one thumb, kept as a named entry point for `Slider`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_slider(
    canvas: &skia_safe::Canvas,
    rect: skia_safe::Rect,
    colors: &SliderColors,
    enabled: bool,
    value: f32,
    min: f32,
    max: f32,
    steps: i32,
    thumb_active: bool,
    focused: bool,
    focus_alpha: f32,
) {
    draw_track(
        canvas,
        rect,
        colors,
        enabled,
        &[TrackThumb { value, active: thumb_active, focused }],
        min,
        max,
        steps,
        true,
        focus_alpha,
    );
}

/// 绘制轨道段——两端不同圆角的水平圆角矩形（对齐 Compose `drawTrackPath`）。
/// 用 `RRect::new_rect_radii` 指定每角半径（skia 支持 per-corner 圆角，
/// 无需手写 path）：radii = [左上, 右上, 右下, 左下]。
fn draw_track_path(
    canvas: &skia_safe::Canvas,
    x0: f32,
    x1: f32,
    cy: f32,
    height: f32,
    start_r: f32,
    end_r: f32,
    color: &Color,
) {
    if x1 <= x0 { return; }
    let half = height / 2.0;
    let top = cy - half;
    let rrect = skia_safe::RRect::new_rect_radii(
        skia_safe::Rect::from_xywh(x0, top, x1 - x0, height),
        &[
            skia_safe::Vector::new(start_r, start_r),
            skia_safe::Vector::new(end_r, end_r),
            skia_safe::Vector::new(end_r, end_r),
            skia_safe::Vector::new(start_r, start_r),
        ],
    );
    let mut paint = skia_safe::Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(skia_color(*color));
    canvas.draw_rrect(&rrect, &paint);
}

fn skia_color(c: Color) -> skia_safe::Color {
    skia_safe::Color::from_argb(c.a, c.r, c.g, c.b)
}



#[cfg(test)]
mod tests;
