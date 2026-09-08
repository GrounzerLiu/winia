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

use crate::core::composer::{ComposeCtx, GroupStatus};
use crate::composable;
use crate::layout::BoxLayout;
use crate::modifier::{Color, KbEvent, KbEventType, Modifier};
use crate::ui::interaction::MutableInteractionSource;
use crate::ui::theme::{ThemeColors, WiniaTheme};
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
        let value = self.value.clamp(min, max);
        let steps = self.steps;
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
    pub(crate) track_width: crate::core::state::Backchannel<f32>,
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

/// 绘制滑块——M3 轨道为**两段独立胶囊**（对齐 Compose `drawTrack`）：
/// - active track：`[0, value_pos - end_gap]`（Primary，左端全圆 8dp/右端 2dp 小圆角）
/// - inactive track：`[value_pos + end_gap, w]`（SecondaryContainer，左端 2dp/右端全圆 8dp）
/// - `end_gap = thumb宽/2 + 6dp`（`ThumbTrackGapSize`）——thumb 与轨道保持 6dp 间隙
/// - 有 steps 时 value_pos 与 tick 位置按 `corner + (w - 2×corner) × f` 内缩（Compose 同）
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
    let w = rect.width();
    let h = rect.height();
    if w <= 0.0 || h <= 0.0 { return; }
    let cy = rect.top + h / 2.0;
    let fraction = fraction_from_value(value, min, max);

    let inactive = colors.track_color(enabled, false);
    let active = colors.track_color(enabled, true);
    let inactive_tick = colors.tick_color(enabled, false);
    let active_tick = colors.tick_color(enabled, true);
    let thumb_c = colors.thumb_color(enabled);

    // 几何（M3：轨道视觉占满组件宽度 [0, w]；stop 在轨道端头圆心
    //（corner = 8dp，不贴边）；thumb 端点中心 = stop 中心——滑动范围
    // [corner, w - corner] 不占满宽度。连续/离散统一按 corner 内缩）
    let corner = SLIDER_TRACK_HEIGHT / 2.0;
    let inside = SLIDER_TRACK_INSIDE_CORNER;
    let end_gap = SLIDER_THUMB_WIDTH / 2.0 + SLIDER_THUMB_GAP;
    let track_left = rect.left;
    let track_right = rect.right;
    let track_w = w;
    // thumb 中心 = corner + (w - 2×corner) × fraction（端点正好停在 stop 上）
    let value_pos = track_left + corner + (track_w - 2.0 * corner) * fraction;
    let has_ticks = steps > 0;

    // ── active track：[track_left, value_pos - end_gap]（左全圆、右小圆角）──
    let active_end = value_pos - end_gap;
    if active_end > track_left + corner {
        draw_track_path(canvas, track_left, active_end, cy, SLIDER_TRACK_HEIGHT, corner, inside, &active);
    }
    // ── inactive track：[value_pos + end_gap, track_right]（左小圆角、右全圆）──
    let inactive_start = value_pos + end_gap;
    if inactive_start < track_right - corner {
        draw_track_path(canvas, inactive_start, track_right, cy, SLIDER_TRACK_HEIGHT, inside, corner, &inactive);
    }

    // ── 刻度（steps 时：steps+2 个 4dp 圆点；位置沿轨道内缩 corner；
    //    active track 范围内用 activeTickColor，范围外 inactiveTickColor）──
    //    与 thumb 重合的 tick 不画（用户规范——避免 thumb 盖住 stop 的视觉冲突）
    if has_ticks {
        let radius = SLIDER_TICK_SIZE / 2.0;
        let mut tp = skia_safe::Paint::default();
        tp.set_anti_alias(true);
        for f in tick_fractions(steps).iter() {
            let x = track_left + corner + (track_w - 2.0 * corner) * f;
            // 重合判定：tick 与 thumb（宽 4）中心距 < (4+4)/2 = 4
            if (x - value_pos).abs() < (SLIDER_THUMB_WIDTH + SLIDER_TICK_SIZE) / 2.0 { continue; }
            let color = if x <= active_end { active_tick } else { inactive_tick };
            tp.set_color(skia_color(color));
            canvas.draw_circle(skia_safe::Point::new(x, cy), radius, &tp);
        }
    }

    // ── stop indicator（轨道外端头中心 4dp 圆点）──
    //   开始端（active 段左端）：离散时与同区 tick 同色（与其它 stop 一致——
    //   用户规范）；连续时与 active track 同色。
    //   结束端（inactive 段右端）：与 active track 同色（离散时恰好 = inactive
    //   tick 色，与 tick 一致）。
    let stop_start_c = if has_ticks {
        colors.tick_color(enabled, true)
    } else {
        colors.track_color(enabled, true)
    };
    let stop_end_c = colors.track_color(enabled, true);
    // 与 thumb 重合的端头 stop 不画（value 在端点时 thumb 停在 stop 上）
    let overlap_thumb = |x: f32| (x - value_pos).abs() < (SLIDER_THUMB_WIDTH + SLIDER_TICK_SIZE) / 2.0;
    let mut sp = skia_safe::Paint::default();
    sp.set_anti_alias(true);
    if active_end > track_left + corner && !overlap_thumb(track_left + corner) {
        sp.set_color(skia_color(stop_start_c));
        canvas.draw_circle(skia_safe::Point::new(track_left + corner, cy), SLIDER_TICK_SIZE / 2.0, &sp);
    }
    if inactive_start < track_right - corner && !overlap_thumb(track_right - corner) {
        sp.set_color(skia_color(stop_end_c));
        canvas.draw_circle(skia_safe::Point::new(track_right - corner, cy), SLIDER_TICK_SIZE / 2.0, &sp);
    }

    // ── 拇指（4×44 胶囊；交互中宽减半）──
    // ⚠ value_pos 已是绝对坐标（含 track_left）——不可再加 rect.left（否则
    // 非根节点（rect.left≠0）时 thumb 相对轨道右偏——2026-08 debug server 实测）
    let thumb_w = if thumb_active { SLIDER_ACTIVE_THUMB_WIDTH } else { SLIDER_THUMB_WIDTH };
    let thumb_h = SLIDER_THUMB_HEIGHT;
    let tx = value_pos;
    let rrect = skia_safe::RRect::new_rect_xy(
        skia_safe::Rect::from_xywh(tx - thumb_w / 2.0, cy - thumb_h / 2.0, thumb_w, thumb_h),
        thumb_w / 2.0,
        thumb_w / 2.0,
    );
    let mut tp = skia_safe::Paint::default();
    tp.set_anti_alias(true);
    tp.set_color(skia_color(thumb_c));
    canvas.draw_rrect(rrect, &tp);

    // ── 焦点环：包围 thumb 胶囊（用户规范——而非整个组件 rect）──
    // 环带中心线 = track 端头位置（距 thumb 中心 end_gap=8）→ 环带间距 = 16 =
    // 两个轨道端头的距离（active 终点 ↔ inactive 起点 = 2×end_gap）
    if focused || focus_alpha > 0.001 {
        // ⚠ 基准用静止 thumb 尺寸（4×44）——不能用交互变窄的 thumb_w（否则
        // 聚焦时环带中心线偏移（±7 而非 ±8），与轨道端头错位（2026-08 实测）
        let ring_rect = skia_safe::Rect::from_xywh(
            value_pos - SLIDER_THUMB_WIDTH / 2.0,
            cy - SLIDER_THUMB_HEIGHT / 2.0,
            SLIDER_THUMB_WIDTH,
            SLIDER_THUMB_HEIGHT,
        );
        // draw_focus 环带中心线距 rect 边缘 = gap + 环宽/2(1.5)。
        // 要中心线距 thumb 中心 8（track 端头）→ gap = 8 - thumb半宽(2) - 1.5 = 4.5
        let ring_gap = end_gap - SLIDER_THUMB_WIDTH / 2.0 - 1.5;
        crate::render::draw_focus(canvas, ring_rect, &crate::modifier::Shape::Pill, thumb_c, focus_alpha, ring_gap);
    }
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
mod tests {
    use super::*;

    // ── 纯函数：刻度/吸附/换算 ──
    #[test]
    fn tick_fractions_matches_compose() {
        // steps=4 → 6 点（含两端）：[0, .2, .4, .6, .8, 1]（Compose steps+2）
        let tf = tick_fractions(4);
        assert_eq!(tf.len(), 6);
        for (i, f) in tf.iter().enumerate() {
            assert!(((f - i as f32 / 5.0).abs()) < 1e-6, "tick[{i}]={f}");
        }
        assert!(tick_fractions(0).is_empty(), "连续滑块无刻度");
    }

    #[test]
    fn snap_value_to_nearest_tick() {
        // 0..10 4 步 → 允许 2/4/6/8（Compose 语义）
        assert_eq!(snap_value(0.0, 4, 0.0, 10.0), 0.0);
        assert_eq!(snap_value(10.0, 4, 0.0, 10.0), 10.0);
        assert_eq!(snap_value(2.0, 4, 0.0, 10.0), 2.0);
        assert_eq!(snap_value(1.2, 4, 0.0, 10.0), 2.0, "1.2 → 最近刻度 2");
        assert_eq!(snap_value(2.9, 4, 0.0, 10.0), 2.0, "2.9 → 最近刻度 2");
        assert_eq!(snap_value(3.1, 4, 0.0, 10.0), 4.0, "3.1 → 最近刻度 4");
        // 连续：clamp 不吸附
        assert_eq!(snap_value(3.3, 0, 0.0, 10.0), 3.3);
        assert_eq!(snap_value(11.0, 0, 0.0, 10.0), 10.0, "越界 clamp");
    }

    #[test]
    fn value_at_x_and_fraction() {
        assert_eq!(value_at_x(0.0, 100.0, 0.0, 10.0, 0), 0.0);
        assert_eq!(value_at_x(100.0, 100.0, 0.0, 10.0, 0), 10.0);
        assert_eq!(value_at_x(50.0, 100.0, 0.0, 10.0, 0), 5.0);
        assert_eq!(value_at_x(-5.0, 100.0, 0.0, 10.0, 0), 0.0, "负坐标 clamp");
        assert_eq!(fraction_from_value(5.0, 0.0, 10.0), 0.5);
        assert_eq!(fraction_from_value(20.0, 0.0, 10.0), 1.0, "越界 clamp");
        assert_eq!(fraction_from_value(-1.0, 0.0, 10.0), 0.0);
    }

    // ── 颜色解析 ──
    #[test]
    fn slider_colors_resolution() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let c = SliderDefaults::slider_colors(&theme);
        // enabled：thumb/active = primary、inactive = secondary_container
        assert_eq!(c.thumb_color(true), theme.primary);
        assert_eq!(c.track_color(true, true), theme.primary);
        assert_eq!(c.track_color(true, false), theme.secondary_container);
        // tick 交叉（Compose defaultSliderColors）：active 区 = inactive track 色
        assert_eq!(c.tick_color(true, true), theme.secondary_container);
        assert_eq!(c.tick_color(true, false), theme.primary);
        // disabled：38%/38%/12%
        assert_eq!(c.thumb_color(false).a, (theme.on_surface.a as f32 * 0.38) as u8);
        assert_eq!(c.track_color(false, true).a, (theme.on_surface.a as f32 * 0.38) as u8);
        assert_eq!(c.track_color(false, false).a, (theme.on_surface.a as f32 * 0.12) as u8);
    }

    // ── 键盘步进（对齐 slideOnKeyEvents）──
    #[test]
    fn keyboard_steps_and_pages() {
        use std::sync::atomic::{AtomicU32, Ordering};
        use winit::keyboard::{Key, NamedKey};
        let mk = |k: Key, t: KbEventType| KbEvent {
            key: k,
            event_type: t,
            is_alt_pressed: false, is_ctrl_pressed: false, is_shift_pressed: false, is_meta_pressed: false,
            repeat: false,
        };
        let last = Arc::new(AtomicU32::new(0));
        let last_cb = last.clone();
        let set: Arc<dyn Fn(f32) + Send + Sync> = Arc::new(move |v: f32| { last_cb.store((v * 100.0) as u32, Ordering::Relaxed); });
        let none: Option<Arc<dyn Fn() + Send + Sync>> = None;
        // 连续：方向键 = 1% 值域
        assert!(handle_key(&mk(Key::Named(NamedKey::ArrowRight), KbEventType::KeyDown), 5.0, 0.0, 10.0, 0, &Some(set.clone()), &none));
        assert_eq!(last.load(Ordering::Relaxed), 510, "5 + 1%*10 = 5.1");
        assert!(handle_key(&mk(Key::Named(NamedKey::ArrowLeft), KbEventType::KeyDown), 5.0, 0.0, 10.0, 0, &Some(set.clone()), &none));
        assert_eq!(last.load(Ordering::Relaxed), 490, "5 - 0.1 = 4.9");
        // 离散 4 步：步长 = 10/5 = 2
        assert!(handle_key(&mk(Key::Named(NamedKey::ArrowRight), KbEventType::KeyDown), 4.0, 0.0, 10.0, 4, &Some(set.clone()), &none));
        assert_eq!(last.load(Ordering::Relaxed), 600, "4 + 2 = 6");
        // Home/End 端点
        assert!(handle_key(&mk(Key::Named(NamedKey::Home), KbEventType::KeyDown), 5.0, 0.0, 10.0, 0, &Some(set.clone()), &none));
        assert_eq!(last.load(Ordering::Relaxed), 0);
        assert!(handle_key(&mk(Key::Named(NamedKey::End), KbEventType::KeyDown), 5.0, 0.0, 10.0, 0, &Some(set.clone()), &none));
        assert_eq!(last.load(Ordering::Relaxed), 1000);
        // 无关键不消费
        assert!(!handle_key(&mk(Key::Named(NamedKey::Escape), KbEventType::KeyDown), 5.0, 0.0, 10.0, 0, &Some(set.clone()), &none));
    }

    // ── 像素测试：track/thumb 渲染 ──
    fn render_slider_px(build: impl FnOnce(&mut ComposeCtx)) -> (Vec<[u8; 4]>, usize) {
        use skia_safe::{Color as SkColor, surfaces};
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = crate::core::composer::Composer::new();
        let scene = |ctx: &mut ComposeCtx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| build(ctx));
        };
        composer.compose(scene);
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
        let mut surface = surfaces::raster_n32_premul((300, 300)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(SkColor::WHITE);
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        crate::render::render(nodes, root, canvas);
        let pm = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        (px.to_vec(), 300)
    }

    fn at(px: &[[u8; 4]], w: usize, x: f32, y: f32) -> (i32, i32, i32) {
        let p = px[(y as usize) * w + (x as usize)];
        (p[2] as i32, p[1] as i32, p[0] as i32) // BGRA → RGB
    }

    fn close(a: (i32, i32, i32), b: (i32, i32, i32)) -> bool {
        (a.0 - b.0).abs() <= 8 && (a.1 - b.1).abs() <= 8 && (a.2 - b.2).abs() <= 8
    }

    #[test]
    fn slider_renders_track_and_thumb() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let (px, w) = render_slider_px(|ctx| {
            Slider::new(0.5)
                .value_range(0.0, 1.0)
                .on_value_change(|_| {})
                .build(ctx);
        });
        let prim = (theme.primary.r as i32, theme.primary.g as i32, theme.primary.b as i32);
        let sec = (theme.secondary_container.r as i32, theme.secondary_container.g as i32, theme.secondary_container.b as i32);
        // 节点 48 高 → 轨道中心 y=24；值 0.5 → active 0..150
        assert!(close(at(&px, w, 75.0, 24.0), prim), "active 轨道应为 primary");
        assert!(close(at(&px, w, 225.0, 24.0), sec), "inactive 轨道应为 secondary_container");
        // thumb 中心 x=150（宽 4 高 44）
        assert!(close(at(&px, w, 150.0, 24.0), prim), "thumb 应为 primary");
        assert!(close(at(&px, w, 150.0, 10.0), prim), "thumb 上段应 primary");
        assert!(close(at(&px, w, 250.0, 24.0), sec), "inactive 轨道右侧");
        // M3 轨道两段独立：active 到 thumb 左侧 gap=6（end_gap=8，thumb 半宽 2）
        // → active 终点 142、inactive 起点 158——间隙区（145, 155）应为白
        assert!(close(at(&px, w, 145.0, 24.0), (255, 255, 255)), "active 与 thumb 间隙应为白（实际 {:?}）", at(&px, w, 145.0, 24.0));
        assert!(close(at(&px, w, 155.0, 24.0), (255, 255, 255)), "inactive 与 thumb 间隙应为白（实际 {:?}）", at(&px, w, 155.0, 24.0));
        // stop indicator：inactive 段右端头中心 = track_right - corner = 300-8 = 292——
        // primary 点画在 secondary 轨道端头上（可见）；stop 不贴边（距边 corner）
        assert!(close(at(&px, w, 292.0, 24.0), prim), "inactive 端头 stop indicator 应为 primary（实际 {:?}）", at(&px, w, 292.0, 24.0));
        // 连续时 active 段左端 stop（8,24）与 active track 同色（primary）——
        // 用户规范（连续时开始的 stop 与 active track 同色）
        assert!(close(at(&px, w, 8.0, 24.0), prim), "连续时 active 端头 stop 应与 active track 同色（primary，实际 {:?}）", at(&px, w, 8.0, 24.0));
    }

    #[test]
    fn slider_zero_value_full_inactive() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let (px, w) = render_slider_px(|ctx| {
            Slider::new(0.0).value_range(0.0, 1.0).on_value_change(|_| {}).build(ctx);
        });
        let sec = (theme.secondary_container.r as i32, theme.secondary_container.g as i32, theme.secondary_container.b as i32);
        let prim = (theme.primary.r as i32, theme.primary.g as i32, theme.primary.b as i32);
        assert!(close(at(&px, w, 75.0, 24.0), sec), "value=0 全 inactive");
        assert!(close(at(&px, w, 150.0, 24.0), sec), "轨道中部 inactive（无 active）");
        // 端点几何：thumb 中心 = corner = 8（轨道端头圆心 = stop 位置）——
        // handle 端点正好停在 stop 上（stop 不贴边，thumb 左缘 6）
        assert!(close(at(&px, w, 8.0, 24.0), prim), "value=0 时 thumb 中心应在 x=8（stop 上，实际 {:?}）", at(&px, w, 8.0, 24.0));
    }

    #[test]
    fn slider_steps_renders_ticks() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let (px, w) = render_slider_px(|ctx| {
            Slider::new(0.5)
                .value_range(0.0, 1.0)
                .steps(4)
                .on_value_change(|_| {})
                .build(ctx);
        });
        let prim = (theme.primary.r as i32, theme.primary.g as i32, theme.primary.b as i32);
        let sec = (theme.secondary_container.r as i32, theme.secondary_container.g as i32, theme.secondary_container.b as i32);
        // tick 位置沿轨道内缩 corner：x = 8 + (300-16)*f = 8+284f
        // f=0.2 → 64.8、f=0.8 → 235.2；active_end = 150-8 = 142
        //（≤142 用 active 色 secondary_container，>142 用 inactive 色 primary）
        assert!(close(at(&px, w, 64.8, 24.0), sec), "active 区 tick = secondary_container（在 primary 轨道上）");
        assert!(close(at(&px, w, 235.2, 24.0), prim), "inactive 区 tick = primary（在 secondary 轨道上）");
        // 离散时 active 段左端 stop（8,24，与 f=0 的 tick 同位）应与同区 tick 同色
        //（secondary_container）——用户规范（离散与其它 stop 一致）
        assert!(close(at(&px, w, 8.0, 24.0), sec), "离散时 active 端头 stop 应与同区 tick 同色（secondary_container，实际 {:?}）", at(&px, w, 8.0, 24.0));
    }

    // ── 交互测试：按下立即跳转（用户规范——不等 tap/drag）──
    #[test]
    fn press_callback_jumps_immediately() {
        use std::sync::atomic::{AtomicI32, Ordering};
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let got = Arc::new(AtomicI32::new(-1));
        let mut composer = crate::core::composer::Composer::new();
        let scene = |ctx: &mut ComposeCtx| {
            let g = got.clone();
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                Slider::new(0.0)
                    .value_range(0.0, 10.0)
                    .on_value_change(move |v| g.store((v * 100.0) as i32, Ordering::Relaxed))
                    .build(ctx);
            });
        };
        composer.compose(scene);
        composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 300.0));
        // 渲染一次让 draw 闭包写入轨道宽度（200）
        let mut surface = skia_safe::surfaces::raster_n32_premul((200, 300)).unwrap();
        let canvas = surface.canvas();
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        crate::render::render(nodes, root, canvas);
        // 找 TapOnPress 回调并模拟按下 x=100（值应 = 10 * (100-8)/(200-16) = 5）
        let mut press: Option<Arc<dyn Fn((f32, f32)) + Send + Sync>> = None;
        for n in composer.arena_nodes() {
            for el in n.modifier.elements() {
                if let crate::modifier::ModifierElement::TapOnPress { cb } = el {
                    press = Some(cb.clone());
                }
            }
        }
        let press = press.expect("应有 TapOnPress");
        press((100.0, 24.0));
        assert_eq!(got.load(Ordering::Relaxed), 500, "按下 x=100/200 → value=5（立即跳转）");
        // 按下 x=150 → (150-8)/(200-16) = 0.7717 → 7.717
        press((150.0, 24.0));
        assert_eq!(got.load(Ordering::Relaxed), 771, "按下 x=150 → value=7.717");
    }

    // ── 防回归：非根节点（rect.left ≠ 0）时 thumb 必须相对轨道居中 ──
    // 2026-08 debug server 实测：thumb 绘制曾双重加 rect.left（value_pos 已是
    // 绝对坐标）——根节点测试（left=0）无法暴露，真实窗口（left=24）偏右 24px
    #[test]
    fn thumb_centered_when_embedded_offsets() {
        use skia_safe::{Color as SkColor, surfaces};
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = crate::core::composer::Composer::new();
        let scene = |ctx: &mut ComposeCtx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                // 外层容器：size(300, 300) 把 slider 推到 (50, 100)
                let k = ctx.next_key();
                ctx.start_container(k, Modifier::new().size(300.0, 300.0), crate::layout::BoxLayout::new());
                let sk = ctx.next_key();
                match ctx.start_restartable_group(
                    sk,
                    Modifier::new().offset(50.0, 100.0),
                    crate::layout::BoxLayout::new().alignment(crate::layout::Alignment::Start),
                ) {
                    crate::core::composer::GroupStatus::Skip => {}
                    crate::core::composer::GroupStatus::Enter => {
                        Slider::new(0.5)
                            .value_range(0.0, 1.0)
                            .on_value_change(|_| {})
                            .build(ctx);
                    }
                }
                ctx.end_restartable_group();
                ctx.end_node();
            });
        };
        composer.compose(scene);
        composer.compose(scene);
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
        let mut surface = surfaces::raster_n32_premul((300, 300)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(SkColor::WHITE);
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        crate::render::render(nodes, root, canvas);
        let pm = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        let at = |x: f32, y: f32| {
            let p = px[(y as usize) * 300 + (x as usize)];
            (p[2] as i32, p[1] as i32, p[0] as i32)
        };
        let prim = (theme.primary.r as i32, theme.primary.g as i32, theme.primary.b as i32);
        // slider fill_max_width → 宽 300，在 (50,100)；轨道中心 y=100+24=124
        // value=0.5 → thumb 中心 = 50+2+296*0.5 = 200（fix 前右偏 50 → 250）
        let thumb = at(200.0, 124.0);
        assert!(close(thumb, prim), "thumb 应居中于轨道（x=200，实际 {thumb:?}）");
        // thumb 两侧间隙（end_gap=8：thumb [198,202]，间隙 [190,197]/[203,210]）
        let gap_l = at(193.0, 124.0);
        let gap_r = at(207.0, 124.0);
        assert!(gap_l.0 > 245 && gap_l.1 > 245 && gap_l.2 > 245, "thumb 左侧应为间隙（实际 {gap_l:?}）");
        assert!(gap_r.0 > 245 && gap_r.1 > 245 && gap_r.2 > 245, "thumb 右侧应为间隙（实际 {gap_r:?}）");
    }

    // ── 防回归：拖动时 thumb 宽度减半（4→2）──
    // 手势回调（on_drag_start）必须发射 interaction.dragged——否则 thumb_active
    // 恒 false（2026-08 实测：拖动时 thumb 不变窄）
    #[test]
    fn drag_interaction_narrows_thumb() {
        use skia_safe::{Color as SkColor, surfaces};
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = crate::core::composer::Composer::new();
        let src = MutableInteractionSource::new();
        let scene = |ctx: &mut ComposeCtx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                Slider::new(0.5)
                    .value_range(0.0, 1.0)
                    .interaction_source(src.clone())
                    .on_value_change(|_| {})
                    .build(ctx);
            });
        };
        let render = |composer: &mut crate::core::composer::Composer| -> usize {
            composer.compose(scene);
            composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
            let mut surface = surfaces::raster_n32_premul((300, 300)).unwrap();
            let canvas = surface.canvas();
            canvas.clear(SkColor::WHITE);
            let root = composer.layout_root_idx().expect("root");
            let nodes = composer.arena_nodes();
            crate::render::render(nodes, root, canvas);
            let pm = surface.peek_pixels().expect("pixmap");
            let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
            // 中心行 y=24（slider 根节点）；thumb 在 148-152（value=0.5）——
            // 扫描 x=144..156 避开 active 轨道终点 142 与 inactive 起点 158
            let prim = (theme.primary.r as i32, theme.primary.g as i32, theme.primary.b as i32);
            let mut w = 0usize;
            for x in 144..156usize {
                let p = px[24 * 300 + x];
                let c = (p[2] as i32, p[1] as i32, p[0] as i32);
                if (c.0 - prim.0).abs() <= 8 && (c.1 - prim.1).abs() <= 8 && (c.2 - prim.2).abs() <= 8 { w += 1; }
            }
            w
        };
        // 静止：thumb 宽 4
        let w0 = render(&mut composer);
        assert_eq!(w0, 4, "静止时 thumb 宽应为 4（实际 {w0}）");
        // 拖动开始：dragged=true → 重组 → thumb 宽 2
        src.emit_drag_start();
        let w1 = render(&mut composer);
        assert_eq!(w1, 2, "拖动时 thumb 宽应减半为 2（实际 {w1}）——on_drag_start 必须 emit_drag_start");
        // 拖动结束：恢复 4
        src.emit_drag_end();
        let w2 = render(&mut composer);
        assert_eq!(w2, 4, "拖动结束 thumb 宽应恢复 4（实际 {w2}）");
    }

    // ── 防回归：与 thumb 重合的 stop 不显示 ──
    // steps=4、value=0.4：thumb 中心 121.6 恰在 f=0.4 的 tick 上——该 tick 不画
    //（121.6 处为轨道色 primary）；f=0.2 的 tick（64.8）不重合仍显示
    #[test]
    fn tick_under_thumb_is_hidden() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let (px, w) = render_slider_px(|ctx| {
            Slider::new(0.4)
                .value_range(0.0, 1.0)
                .steps(4)
                .on_value_change(|_| {})
                .build(ctx);
        });
        let prim = (theme.primary.r as i32, theme.primary.g as i32, theme.primary.b as i32);
        let sec = (theme.secondary_container.r as i32, theme.secondary_container.g as i32, theme.secondary_container.b as i32);
        // thumb 中心 = 8+284×0.4 = 121.6——重合 tick 不画：121.6 处是轨道色（primary）
        let under = at(&px, w, 121.6, 24.0);
        assert!(close(under, prim), "thumb 下的 tick 不应绘制（应为轨道 primary，实际 {under:?}）");
        // 不重合的 tick（f=0.2 → 64.8）仍显示（secondary_container）
        let other = at(&px, w, 64.8, 24.0);
        assert!(close(other, sec), "不重合 tick 应正常显示（secondary_container，实际 {other:?}）");
    }

    // ── 防回归：焦点环包围 thumb 胶囊（而非整个组件 rect）──
    #[test]
    fn focus_ring_wraps_thumb_not_component() {
        use skia_safe::{Color as SkColor, surfaces};
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = crate::core::composer::Composer::new();
        let src = MutableInteractionSource::new();
        let scene = |ctx: &mut ComposeCtx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                Slider::new(0.5)
                    .value_range(0.0, 1.0)
                    .interaction_source(src.clone())
                    .on_value_change(|_| {})
                    .build(ctx);
            });
        };
        let render = |composer: &mut crate::core::composer::Composer| -> Vec<[u8; 4]> {
            composer.compose(scene);
            composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
            let mut surface = surfaces::raster_n32_premul((300, 300)).unwrap();
            let canvas = surface.canvas();
            canvas.clear(SkColor::WHITE);
            let root = composer.layout_root_idx().expect("root");
            let nodes = composer.arena_nodes();
            crate::render::render(nodes, root, canvas);
            let pm = surface.peek_pixels().expect("pixmap");
            pm.pixels::<[u8; 4]>().expect("pixels").to_vec()
        };
        let at = |px: &[[u8; 4]], x: f32, y: f32| {
            let p = px[(y as usize) * 300 + (x as usize)];
            (p[2] as i32, p[1] as i32, p[0] as i32)
        };
        let prim = (theme.primary.r as i32, theme.primary.g as i32, theme.primary.b as i32);
        // 无焦点：(145,24) 是 thumb 左侧间隙（白）
        let px0 = render(&mut composer);
        let gap0 = at(&px0, 145.0, 24.0);
        assert!(gap0.0 > 245 && gap0.1 > 245 && gap0.2 > 245, "未聚焦时 thumb 左侧应为间隙（实际 {gap0:?}）");
        // 聚焦 + 推进焦点环淡入动画
        src.emit_focus();
        for _ in 0..60 {
            if !crate::animation::update_animations() { break; }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let px1 = render(&mut composer);
        // 焦点环环带中心线 = track 端头（value_pos ± end_gap = 142/158）——
        // 环带间距与两个轨道端头距离一致（16dp）
        // inactive 侧环带（中心线 158 = track 端头）画在 secondary 轨道上——primary 可见
        let ring_r = at(&px1, 158.0, 24.0);
        assert!(close(ring_r, prim), "焦点环环带应对齐 inactive 轨道端头（x=158，实际 {ring_r:?}）");
        // thumb 紧邻处（145）无环——环带已外移到轨道端头
        let near = at(&px1, 145.0, 24.0);
        assert!(near.0 > 245 && near.1 > 245 && near.2 > 245, "thumb 紧邻处不应有环带（环带在轨道端头，实际 {near:?}）");
        // 整组件焦点环会在组件顶部 y=0 全宽分布（gap2+宽3 外圈）；thumb 环只在
        // thumb 附近（x 144.5..155.5）——验证 y=0 行远离 thumb 处无环色
        let far_top = at(&px1, 50.0, 0.0);
        assert!(far_top.0 > 245 && far_top.1 > 245 && far_top.2 > 245, "y=0 远离 thumb 处不应有焦点环（整组件环会全宽分布，实际 {far_top:?}）");
        // 环顶/底水平段在组件外（sr 顶 -4，环带 -5.5..-2.5）——y=0 无环色
        let top = at(&px1, 150.0, 0.0);
        assert!(top.0 > 245 && top.1 > 245 && top.2 > 245, "y=0 不应有环（环顶段在组件外，实际 {top:?}）");
    }

    // ── exp/modifier-node 首个真实迁移验证 ──
    #[test]
    fn slider_track_node_key_covers_all_visual_params() {
        use crate::core::state::State;
        use crate::modifier::DrawNode;
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let colors = SliderDefaults::slider_colors(&theme);
        let mut colors2 = colors;
        colors2.thumb_color = Color::from_argb(255, 1, 2, 3);
        // 同一 interaction 源（换源单独测——每次 new 源 id 不同）
        let shared_src = MutableInteractionSource::new();
        let shared_tw = crate::core::state::Backchannel::new(300.0);
        #[allow(clippy::too_many_arguments)]
        let mk = |value: f32, enabled: bool, thumb_active: bool, colors: SliderColors,
                  min: f32, max: f32, steps: i32| {
            SliderTrackNode {
                track_width: shared_tw.clone(),
                interaction: shared_src.clone(),
                colors,
                enabled,
                value,
                min,
                max,
                steps,
                thumb_active,
            }
        };
        let base = mk(0.5, true, false, colors, 0.0, 1.0, 0);
        // 同参 → 相等（Skip）
        assert_eq!(base.node_key(), mk(0.5, true, false, colors, 0.0, 1.0, 0).node_key());
        // 值/开关/状态任一变化 → 不等（Enter）
        assert_ne!(base.node_key(), mk(0.6, true, false, colors, 0.0, 1.0, 0).node_key(), "value 应进 key");
        assert_ne!(base.node_key(), mk(0.5, false, false, colors, 0.0, 1.0, 0).node_key(), "enabled 应进 key");
        assert_ne!(base.node_key(), mk(0.5, true, true, colors, 0.0, 1.0, 0).node_key(), "thumb_active 应进 key");
        // P0-2 补：colors/min/max/steps 任一变化 → 不等
        assert_ne!(base.node_key(), mk(0.5, true, false, colors2, 0.0, 1.0, 0).node_key(), "colors 应进 key");
        assert_ne!(base.node_key(), mk(0.5, true, false, colors, 0.5, 1.0, 0).node_key(), "min 应进 key");
        assert_ne!(base.node_key(), mk(0.5, true, false, colors, 0.0, 2.0, 0).node_key(), "max 应进 key");
        assert_ne!(base.node_key(), mk(0.5, true, false, colors, 0.0, 1.0, 4).node_key(), "steps 应进 key");
        // P0-2 补：track_width 回写通道不进 key（值变化 → key 相等，走依赖通道）
        shared_tw.set(999.0);
        assert_eq!(
            base.node_key(), mk(0.5, true, false, colors, 0.0, 1.0, 0).node_key(),
            "track_width 回写值变化不应进 key"
        );
        // 换源 → 不等（重建绑定）
        let other = SliderTrackNode {
            track_width: crate::core::state::Backchannel::new(300.0),
            interaction: MutableInteractionSource::new(), // 新源 id 不同
            colors,
            enabled: true,
            value: 0.5,
            min: 0.0,
            max: 1.0,
            steps: 0,
            thumb_active: false,
        };
        assert_ne!(base.node_key(), other.node_key(), "换 interaction 源应进 key");
    }

    #[test]
    fn slider_track_node_renders_identical_to_enum_draw() {
        // P0-1 真双路对照：同参一路 draw_node(SliderTrackNode)，一路旧
        // `Modifier::draw` 匿名闭包（v2 语义逐行复刻），同 300×48 surface
        // 逐字节 assert_eq。两路均用全新 unfocused 源（focused=false，
        // focus_alpha=0），rect 一致——差异即迁移保真失败。
        use skia_safe::{Color as SkColor, surfaces};
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let colors = SliderDefaults::slider_colors(&theme);
        let render_with = |modifier: Modifier| {
            let mut composer = crate::core::composer::Composer::new();
            let scene = |ctx: &mut ComposeCtx| {
                WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    let key = ctx.next_key();
                    ctx.start_leaf(key, modifier.clone());
                    ctx.end_node();
                });
            };
            composer.compose(scene);
            composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 48.0));
            let mut surface = surfaces::raster_n32_premul((300, 48)).unwrap();
            let canvas = surface.canvas();
            canvas.clear(SkColor::WHITE);
            let root = composer.layout_root_idx().expect("root");
            let nodes = composer.arena_nodes();
            crate::render::render(nodes, root, canvas);
            let pm = surface.peek_pixels().expect("pixmap");
            pm.pixels::<[u8; 4]>().expect("pixels").to_vec()
        };
        let tw_node = crate::core::state::Backchannel::new(0.0f32);
        let tw_enum = crate::core::state::Backchannel::new(0.0f32);
        let src_node = MutableInteractionSource::new();
        let src_enum = MutableInteractionSource::new();
        let node_mod = Modifier::new().size(300.0, 48.0).draw_node(SliderTrackNode {
            track_width: tw_node.clone(),
            interaction: src_node.clone(),
            colors,
            enabled: true,
            value: 0.5,
            min: 0.0,
            max: 1.0,
            steps: 4,
            thumb_active: false,
        });
        // 旧闭包逐行复刻（v2 slider.rs build 侧 .draw 体）：回写宽度 +
        // 读焦点/环透明度 + draw_slider 同参。注意 node 绘制顺序已移至枚举链
        // 之后（P1-1）——本节点无 Background/Icon 同胞，顺序差无像素影响。
        let enum_mod = Modifier::new().size(300.0, 48.0).draw(move |canvas, rect| {
            tw_enum.set(rect.width());
            let focused = src_enum.is_focused_value();
            let focus_alpha = src_enum.focus_indicator_alpha_value();
            draw_slider(canvas, rect, &colors, true, 0.5, 0.0, 1.0, 4, false, focused, focus_alpha);
        });
        let px_node = render_with(node_mod);
        let px_enum = render_with(enum_mod);
        assert_eq!(px_node.len(), 300 * 48);
        assert_eq!(px_enum.len(), 300 * 48);
        assert_eq!(
            px_node, px_enum,
            "node 路与旧 draw 闭包路必须逐字节一致（迁移保真）"
        );
        // 诊断性断言（diff 失败时快速定位）：thumb 中心应为 primary
        // （value=0.5 → x = corner + (300-2*corner)*0.5 = 150，y=24）。
        // 常量：SLIDER_TRACK_HEIGHT=16 → corner=8。
        let p = px_node[24 * 300 + 150];
        let (r, g, b) = (p[2] as i32, p[1] as i32, p[0] as i32);
        let prim = theme.primary;
        assert!(
            (r - prim.r as i32).abs() <= 8
                && (g - prim.g as i32).abs() <= 8
                && (b - prim.b as i32).abs() <= 8,
            "node 绘制 thumb 应为 primary，实际 ({r},{g},{b})"
        );
    }

    #[cfg(feature = "debug-server")]
    #[test]
    fn slider_track_node_visible_in_debug_tree() {
        // 调试树应含 node(slidertrack:...) 条目（具名可观测——匿名闭包无此能力）。
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = crate::core::composer::Composer::new();
        let scene = |ctx: &mut ComposeCtx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                Slider::new(0.5).on_value_change(|_| {}).build(ctx);
            });
        };
        composer.compose(scene);
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        let json = crate::debug::build_tree_json(nodes, root);
        assert!(
            json.contains("node(draw:slidertrack:"),
            "调试树应含 SliderTrackNode 条目，实际: {}",
            &json[..json.len().min(500)]
        );
    }
}

