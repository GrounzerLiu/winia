//! Scrollbar 滚动条 — 对标 Compose Multiplatform 桌面 `VerticalScrollbar` /
//! `HorizontalScrollbar` + M3 `Modifier.nonInteractiveScrollbar`
//!（`Scrollbar.kt`，源码见浏览器取证；几何公式同 gist `drawScrollbar`）。
//!
//! 形态：独立组件（非 modifier），放在滚动容器旁（Row 内并排 / Stack 右侧覆盖）。
//! 数据源：`ScrollState{offset/fling_limit}` + 自身约束（viewport）。
//! 三要素：offset（读）、viewport（自身约束高/宽）、content（= fling_limit + viewport）。
//!
//! - thumb 几何（M3 同）：`thumb = clamp(track * viewport/content, min, track*0.9)`，
//!   `thumb_offset = (scroll/max) * (track - thumb)`；content <= viewport 时不画。
//! - 拖 thumb：`on_drag` → `offset.set(drag_pos / (track - thumb) * max)`。
//!   拖拽开始抢占（取消列表侧惯性 fling + 置滚动中，松手清——否则惯性期
//!   update_animations 写回 offset 导致拖不动；松手后 hover 仍在则保持显示）。
//! - 显示：`always_show` / 滚动中 / hover / 拖拽中 / 滚动脉冲 + fade
//!   动画（M3 默认 400ms delay + 250ms tween——`push_animatable(fade_state)`
//!   驱动，绘制期 `peek` 叠加 alpha，`fade` 不进 key 零重组）。
//!   wheel/程序化滚动不置 `is_scroll_in_progress`（分发层只 cancel+置 false），
//!   故滚动检测直接看 offset 变化（脉冲）：每次 offset 变都重启 effect
//!   （abort 睡眠）→持续滚动保持显示；停下 400ms 后 fade out。
//!   ⚠ 不能用 bool 门闩（置 true 后不清零→常显不藏，实测 bug）。
//!
//! v1 范围：无 RTL 镜像（垂直条恒右侧，调用方放）、
//! 无 LazyList 适配（LazyListState 另有 first_visible 锚点模型——后续加）。

use crate::composable;
use crate::core::composer::{ComposeCtx, GroupStatus};
use crate::core::state::State;
use crate::layout::BoxLayout;
use crate::modifier::{Color, Modifier, Shape};
use crate::ui::interaction::MutableInteractionSource;
use crate::ui::theme::WiniaTheme;

/// 默认厚度（M3 `Thickness = 4dp`，桌面 CMP 常用 8dp 含 padding——winia 取 6dp）
pub const SCROLLBAR_THICKNESS: f32 = 6.0;
/// 默认 thumb 最小长度（M3 `ThumbMinLength = 24dp`）
pub const SCROLLBAR_THUMB_MIN_LENGTH: f32 = 24.0;
/// thumb 最大长度占 track 比例（M3 `ThumbMaxLengthFraction = 0.9`）
pub const SCROLLBAR_THUMB_MAX_FRACTION: f32 = 0.9;
/// 默认 thumb 色（M3 outline 70% —— `NonInteractiveScrollbarDefaults.thumbColor`）
pub fn scrollbar_thumb_color(theme: &crate::ui::theme::ThemeColors) -> Color {
    let o = theme.outline;
    Color::from_argb(((o.a as f32) * 0.7) as u8, o.r, o.g, o.b)
}

/// 纯几何：给定 viewport/content/scroll，算 (thumb_offset, thumb_len, visible)。
/// 与绘制/组件解耦，可单测。track = viewport - 2*inset。
pub(crate) fn scrollbar_geometry(
    viewport: f32,
    content: f32,
    scroll: f32,
    thumb_min: f32,
    thumb_max_fraction: f32,
    inset: f32,
) -> Option<(f32, f32)> {
    if viewport <= 0.0 || content <= viewport {
        return None;
    }
    let track = (viewport - 2.0 * inset).max(0.0);
    if track <= 0.0 || track < thumb_min {
        return None;
    }
    let max_thumb = track * thumb_max_fraction;
    let thumb = (track * viewport / content).clamp(thumb_min, max_thumb);
    let max_offset = content - viewport;
    let thumb_offset = if max_offset > 0.0 {
        (scroll.clamp(0.0, max_offset) / max_offset) * (track - thumb)
    } else {
        0.0
    };
    Some((thumb_offset, thumb))
}

/// 纯几何：thumb 拖到 track 位置 pos（相对 track 起点），反推 scroll offset。
pub(crate) fn scrollbar_offset_for_thumb_pos(
    pos: f32,
    track: f32,
    thumb: f32,
    max_offset: f32,
) -> f32 {
    let travel = (track - thumb).max(1.0);
    (pos / travel * max_offset).clamp(0.0, max_offset.max(0.0))
}

// ── ScrollbarNode ──

/// 滚动条绘制节点（thumb 圆角条 + 可选 track）。
///
/// fade 语义（M3 默认 400ms delay + 250ms tween）：`fade: State` 存进节点，
/// `draw` 内 `peek` 求值叠加 alpha（零重组——动画引擎每帧 `request_redraw`
/// 驱动重绘，与 Slider `focus_alpha` 同惯例）。**不能**在 build 期快照为 f32
/// （peek 不注册依赖 → fade 变化永不重组 → 节点残留旧值 → 常显不显、
/// 松手即消失，实测 bug）。
#[derive(Debug)]
pub(crate) struct ScrollbarNode {
    pub(crate) vertical: bool,
    pub(crate) thumb_color: Color,
    pub(crate) track_color: Color,
    pub(crate) thickness: f32,
    pub(crate) inset: f32,
    /// 渲染期几何（build 期由 constraints + State 算好，draw 只画）。
    pub(crate) thumb_offset: f32,
    pub(crate) thumb_len: f32,
    /// 几何有效（content > viewport 且 track 放得下）。fade 另由 `fade` 状态门控。
    pub(crate) has_thumb: bool,
    /// fade alpha 状态（draw 期 peek，不进 key）。
    pub(crate) fade: State<f32>,
}

impl crate::modifier::DrawNode for ScrollbarNode {
    fn draw(&self, canvas: &skia_safe::Canvas, rect: skia_safe::Rect) {
        // fade 状态 draw 期 peek（零重组——动画引擎每帧 request_redraw 驱动；
        // build 期快照 f32 则 peek 不注册依赖，fade 变化永不重组，实测 bug）。
        let fade = self.fade.peek();
        if !self.has_thumb || fade <= 0.01 {
            return;
        }
        // fade alpha 叠加（M3 fade 语义）
        let mul = fade.clamp(0.0, 1.0);
        let thumb_a = ((self.thumb_color.a as f32) * mul) as u8;
        let track_a = ((self.track_color.a as f32) * mul) as u8;
        let mut paint = skia_safe::Paint::default();
        paint.set_anti_alias(true);
        // track（全长，透明则跳过）
        if track_a != 0 {
            paint.set_color(skia_safe::Color::from_argb(
                track_a,
                self.track_color.r,
                self.track_color.g,
                self.track_color.b,
            ));
            if self.vertical {
                canvas.draw_rect(
                    skia_safe::Rect::new(
                        rect.left,
                        rect.top + self.inset,
                        rect.left + self.thickness,
                        rect.bottom - self.inset,
                    ),
                    &paint,
                );
            } else {
                canvas.draw_rect(
                    skia_safe::Rect::new(
                        rect.left + self.inset,
                        rect.top,
                        rect.right - self.inset,
                        rect.top + self.thickness,
                    ),
                    &paint,
                );
            }
        }
        // thumb（圆角，半径 = thickness/2）
        paint.set_color(skia_safe::Color::from_argb(
            thumb_a,
            self.thumb_color.r,
            self.thumb_color.g,
            self.thumb_color.b,
        ));
        let r = self.thickness / 2.0;
        if self.vertical {
            let thumb_rect = skia_safe::Rect::new(
                rect.left,
                rect.top + self.inset + self.thumb_offset,
                rect.left + self.thickness,
                rect.top + self.inset + self.thumb_offset + self.thumb_len,
            );
            canvas.draw_round_rect(thumb_rect, r, r, &paint);
        } else {
            let thumb_rect = skia_safe::Rect::new(
                rect.left + self.inset + self.thumb_offset,
                rect.top,
                rect.left + self.inset + self.thumb_offset + self.thumb_len,
                rect.top + self.thickness,
            );
            canvas.draw_round_rect(thumb_rect, r, r, &paint);
        }
    }
    fn node_key(&self) -> String {
        // fade State 不进 key（逐帧动画值，draw 期 peek；进则每帧 Enter。
        // 回写通道类比 Slider focus_alpha——绘制由引擎 request_redraw 驱动）。
        format!(
            "scrollbar:{}:{:?}:{:?}:{}:{}:{}:{}:{}",
            self.vertical,
            self.thumb_color,
            self.track_color,
            self.thickness.to_bits(),
            self.inset.to_bits(),
            self.thumb_offset.to_bits(),
            self.thumb_len.to_bits(),
            self.has_thumb,
        )
    }
}

// ── VerticalScrollbar ──

/// 垂直滚动条（对标 CMP `VerticalScrollbar`）。
///
/// 用法：与滚动容器并排等高（Row 内），`ScrollState` 共享。
/// ```ignore
/// Row::new().build(ctx, |ctx| {
///     Column::new().modifier(Modifier::new().fill_max_height().weight(1.0)
///         .vertical_scroll(scroll.clone())).build(ctx, content);
///     VerticalScrollbar::new(scroll.clone()).build(ctx);
/// });
/// ```
pub struct VerticalScrollbar {
    scroll: crate::modifier::ScrollState,
    modifier: Modifier,
    thickness: f32,
    thumb_color: Option<Color>,
    track_color: Color,
    thumb_min_length: f32,
    always_show: bool,
}

impl VerticalScrollbar {
    pub fn new(scroll: crate::modifier::ScrollState) -> Self {
        Self {
            scroll,
            modifier: Modifier::new(),
            thickness: SCROLLBAR_THICKNESS,
            thumb_color: None,
            track_color: Color::TRANSPARENT,
            thumb_min_length: SCROLLBAR_THUMB_MIN_LENGTH,
            always_show: false,
        }
    }

    pub fn modifier(mut self, m: Modifier) -> Self { self.modifier = self.modifier.then(m); self }
    pub fn thickness(mut self, t: f32) -> Self { self.thickness = t; self }
    pub fn thumb_color(mut self, c: Color) -> Self { self.thumb_color = Some(c); self }
    pub fn track_color(mut self, c: Color) -> Self { self.track_color = c; self }
    pub fn thumb_min_length(mut self, l: f32) -> Self { self.thumb_min_length = l; self }
    /// 常显（默认仅滚动中显示，对标 `alwaysShowScrollBar`）
    pub fn always_show(mut self, v: bool) -> Self { self.always_show = v; self }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        ctx.changed(&self.thickness);
        ctx.changed(&self.thumb_color);
        ctx.changed(&self.track_color);
        ctx.changed(&self.thumb_min_length);
        ctx.changed(&self.always_show);
        let key = ctx.next_key();
        let theme = WiniaTheme::colors();
        let thumb_color = self.thumb_color.unwrap_or_else(|| scrollbar_thumb_color(&theme));
        let thickness = self.thickness;
        let inset = 2.0f32;

        // 三要素：offset（读，注册依赖）+ fling_limit（读，注册依赖）；
        // viewport 由自身约束来（fill_max_height 下 = 父高，要求与滚动容器等高）。
        // ⚠ 必须在 build 期 get（注册组合依赖），绘制期 peek 不注册。
        let scroll = self.scroll.offset.get();
        let limit = self.scroll.fling_limit.get();
        let scrolling = self.scroll.is_scroll_in_progress.get();
        // hover/拖拽中也显示（否则隐藏态无从下手拖）——interaction 源 remember 持有
        let hover_src: MutableInteractionSource =
            ctx.remember(|| MutableInteractionSource::new()).get();
        let hovered = hover_src.is_hovered();
        let dragged = hover_src.is_dragged();
        // fade：目标 alpha（常显/滚动/hover/拖拽/滚动脉冲 → 1，否则 0），
        // 经 tween 250ms 驱动（M3 ThumbFadeDurationMillis；delay 由 LaunchedEffect
        // 400ms 实现）。绘制期 peek 求值（零重组）——fade_state 不进 node_key。
        //
        // ⚠ wheel/程序化滚动时 `scrolling` 恒 false（分发层 cancel+置 false，
        // 拖拽路径才置 true）——故滚动检测不用它，而用"offset 变化"脉冲：
        // `last_scroll` 记住上帧 offset，本帧不同即正在滚（set_silent 写回，
        // 不触发重组——本帧 build 照常继续）。
        // ⚠ 不能用 bool 门闩（滚过即 true 不清零→常显不藏，实测 bug）。
        // ⚠ key 必须含 `scroll`：bool 条件会合并连续滚动——tween 250ms 播到 1
        // 后 key 不变，而滚动仍在继续，随后的 400ms 睡眠会把条 fade 掉；
        // scroll 每帧都变→每帧重启 effect→abort 睡眠→保持显示。
        let last_scroll: State<f32> = ctx.remember(|| scroll);
        let scroll_active = last_scroll.peek() != scroll;
        if scroll_active {
            last_scroll.set_silent(scroll);
        }
        let fade_target = if self.always_show || scrolling || hovered || dragged
            || scroll_active
        {
            1.0f32
        } else {
            0.0f32
        };
        // 离开交互（hover 出 + 停滚）400ms 后 fade out（M3 ThumbFadeDelayMillis）：
        // 单 LaunchedEffect 包办"显示→等→藏"：key 含全部输入——任何变化都
        // abort 睡眠重启（持续滚动每帧重启→保持显示；停滚后无重组无重启，
        // 最后一次滚动那帧的任务睡满 400ms→藏，正好是停滚后 400ms 隐藏）。
        // 睡醒后只看静态保持条件（常显/滚动中/hover/拖拽）：
        // - 成立则保持（hover 静置不能藏——藏了无重组再显示；后续变化重启再定）。
        // - 否则播到 0 隐藏。
        // ⚠ 静态条件快照进任务即最新：睡眠不被 abort 即证明 key 输入全未变
        // （滚动脉冲除外——它若存活必带新 scroll abort 本任务，故睡醒时必衰减）。
        // ⚠ 不能用"滚过"门闩（不清零→常显不藏）——脉冲 + 快照才是完整语义。
        let fade_state: State<f32> = ctx.remember(|| fade_target);
        let keep_visible = self.always_show || scrolling || hovered || dragged;
        crate::effect::LaunchedEffect::new((fade_target, scrolling, hovered, dragged, scroll)).build(
            ctx,
            {
                let fade_state = fade_state.clone();
                move |scope| {
                    let fade_state = fade_state.clone();
                    async move {
                        if fade_target > 0.5 {
                            // 显示：tween 到 1（从当前值播，无闪烁）
                            crate::animation::push_animatable(
                                fade_state.clone(),
                                1.0,
                                crate::animation::AnimationSpec::Tween(
                                    crate::animation::TweenSpec::new(
                                        std::time::Duration::from_millis(250),
                                        crate::animation::interpolator::Linear::new(),
                                    ),
                                ),
                            );
                        }
                        // 停留 delay：期间滚动/hover/拖拽变化→key 变化→abort 睡眠→保持。
                        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
                        if !keep_visible {
                            crate::animation::push_animatable(
                                fade_state,
                                0.0,
                                crate::animation::AnimationSpec::Tween(
                                    crate::animation::TweenSpec::new(
                                        std::time::Duration::from_millis(250),
                                        crate::animation::interpolator::Linear::new(),
                                    ),
                                ),
                            );
                        }
                        drop(scope);
                    }
                }
            },
        );
        // viewport 未知（首帧约束未定）→ 用 remembered 上帧值兜底，首帧不画
        let viewport_state: State<f32> = ctx.remember(|| 0.0f32);
        // 注意：viewport 来自 measure 期，此处先读上帧值；measure 后写回由 policy？
        // v1 简化：scrollbar 自身 fill_max_height，约束由父定——build 期拿不到约束。
        // 改从 layout 拿：用 on_size_changed 回调写 viewport_state（对标 NavTransition
        // 位移基准捕获）。首帧 0 → 不画，次帧回写后重组显示。
        let viewport = viewport_state.peek();
        let content = limit + viewport;

        let (thumb_offset, thumb_len, has_thumb) = match scrollbar_geometry(
            viewport,
            content,
            scroll,
            self.thumb_min_length,
            SCROLLBAR_THUMB_MAX_FRACTION,
            inset,
        ) {
            Some((off, len)) => (off, len, true),
            _ => (0.0, 0.0, false),
        };

        // 拖 thumb → 反推 offset（需 track/thumb/max——build 期值，闭包捕获）
        let track = (viewport - 2.0 * inset).max(0.0);
        let max_offset = (content - viewport).max(0.0);
        let scroll_state = self.scroll.clone();
        let drag_thumb_len = thumb_len;
        let m = Modifier::new()
            .width(thickness)
            .fill_max_height()
            .hoverable(&hover_src)
            .on_size_changed(move |w, h| {
                let _ = w;
                viewport_state.set(h);
            })
            .draw_node(ScrollbarNode {
                vertical: true,
                thumb_color,
                track_color: self.track_color,
                thickness,
                inset,
                thumb_offset,
                thumb_len,
                has_thumb,
                fade: fade_state.clone(),
            })
            .on_drag_start({
                let hover_src = hover_src.clone();
                let scroll_state = scroll_state.clone();
                move |_pos: (f32, f32)| {
                    hover_src.emit_drag_start();
                    // 抢占：取消列表侧惯性 fling（否则 update_animations 下帧
                    // 把 offset 写回，thumb 拖不动）；标记滚动中（松手清）。
                    scroll_state.cancel_fling();
                    scroll_state.is_scroll_in_progress.set(true);
                }
            })
            .on_drag_end({
                let hover_src = hover_src.clone();
                let scroll_state = scroll_state.clone();
                move || {
                    hover_src.emit_drag_end();
                    scroll_state.is_scroll_in_progress.set(false);
                }
            })
            .on_drag(move |pos, _delta| {
                // pos 为 thumb 条本地坐标（含 inset 偏移）；减 inset 得 track 内位置
                let pos_in_track = pos.1 - inset;
                // thumb 顶部对齐拖点：目标 thumb_offset = pos - thumb/2？
                // CMP 语义：拖 thumb 本体跟随——用增量换算更稳：
                // scroll += delta.scroll_units。简化：绝对位置映射（thumb 顶部 = 拖点 - thumb/2）
                let target = pos_in_track - drag_thumb_len / 2.0;
                let off = scrollbar_offset_for_thumb_pos(target, track, drag_thumb_len, max_offset);
                scroll_state.offset.set(off);
            });

        let m = m.then(self.modifier);
        match ctx.start_restartable_group(key, m, BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {}
        }
        ctx.end_restartable_group();
    }
}

// ── HorizontalScrollbar ──

/// 水平滚动条（对标 CMP `HorizontalScrollbar`）。语义与垂直对称。
pub struct HorizontalScrollbar {
    scroll: crate::modifier::ScrollState,
    modifier: Modifier,
    thickness: f32,
    thumb_color: Option<Color>,
    track_color: Color,
    thumb_min_length: f32,
    always_show: bool,
}

impl HorizontalScrollbar {
    pub fn new(scroll: crate::modifier::ScrollState) -> Self {
        Self {
            scroll,
            modifier: Modifier::new(),
            thickness: SCROLLBAR_THICKNESS,
            thumb_color: None,
            track_color: Color::TRANSPARENT,
            thumb_min_length: SCROLLBAR_THUMB_MIN_LENGTH,
            always_show: false,
        }
    }

    pub fn modifier(mut self, m: Modifier) -> Self { self.modifier = self.modifier.then(m); self }
    pub fn thickness(mut self, t: f32) -> Self { self.thickness = t; self }
    pub fn thumb_color(mut self, c: Color) -> Self { self.thumb_color = Some(c); self }
    pub fn track_color(mut self, c: Color) -> Self { self.track_color = c; self }
    pub fn thumb_min_length(mut self, l: f32) -> Self { self.thumb_min_length = l; self }
    pub fn always_show(mut self, v: bool) -> Self { self.always_show = v; self }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        ctx.changed(&self.thickness);
        ctx.changed(&self.thumb_color);
        ctx.changed(&self.track_color);
        ctx.changed(&self.thumb_min_length);
        ctx.changed(&self.always_show);
        let key = ctx.next_key();
        let theme = WiniaTheme::colors();
        let thumb_color = self.thumb_color.unwrap_or_else(|| scrollbar_thumb_color(&theme));
        let thickness = self.thickness;
        let inset = 2.0f32;

        let scroll = self.scroll.offset.get();
        let limit = self.scroll.fling_limit.get();
        let scrolling = self.scroll.is_scroll_in_progress.get();
        let hover_src: MutableInteractionSource =
            ctx.remember(|| MutableInteractionSource::new()).get();
        let hovered = hover_src.is_hovered();
        let dragged = hover_src.is_dragged();
        // fade：目标 alpha（常显/滚动/hover/拖拽/滚动脉冲 → 1，否则 0），
        // 经 tween 250ms 驱动（M3 ThumbFadeDurationMillis；delay 由 LaunchedEffect
        // 400ms 实现）。绘制期 peek 求值（零重组）——fade_state 不进 node_key。
        //
        // ⚠ wheel/程序化滚动时 `scrolling` 恒 false（分发层 cancel+置 false，
        // 拖拽路径才置 true）——故滚动检测不用它，而用"offset 变化"脉冲：
        // `last_scroll` 记住上帧 offset，本帧不同即正在滚（set_silent 写回，
        // 不触发重组——本帧 build 照常继续）。
        // ⚠ 不能用 bool 门闩（滚过即 true 不清零→常显不藏，实测 bug）。
        // ⚠ key 必须含 `scroll`：bool 条件会合并连续滚动——tween 250ms 播到 1
        // 后 key 不变，而滚动仍在继续，随后的 400ms 睡眠会把条 fade 掉；
        // scroll 每帧都变→每帧重启 effect→abort 睡眠→保持显示。
        let last_scroll: State<f32> = ctx.remember(|| scroll);
        let scroll_active = last_scroll.peek() != scroll;
        if scroll_active {
            last_scroll.set_silent(scroll);
        }
        let fade_target = if self.always_show || scrolling || hovered || dragged
            || scroll_active
        {
            1.0f32
        } else {
            0.0f32
        };
        // 离开交互（hover 出 + 停滚）400ms 后 fade out（M3 ThumbFadeDelayMillis）：
        // 单 LaunchedEffect 包办"显示→等→藏"：key 含全部输入——任何变化都
        // abort 睡眠重启（持续滚动每帧重启→保持显示；停滚后无重组无重启，
        // 最后一次滚动那帧的任务睡满 400ms→藏，正好是停滚后 400ms 隐藏）。
        // 睡醒后只看静态保持条件（常显/滚动中/hover/拖拽）：
        // - 成立则保持（hover 静置不能藏——藏了无重组再显示；后续变化重启再定）。
        // - 否则播到 0 隐藏。
        // ⚠ 静态条件快照进任务即最新：睡眠不被 abort 即证明 key 输入全未变
        // （滚动脉冲除外——它若存活必带新 scroll abort 本任务，故睡醒时必衰减）。
        // ⚠ 不能用"滚过"门闩（不清零→常显不藏）——脉冲 + 快照才是完整语义。
        let fade_state: State<f32> = ctx.remember(|| fade_target);
        let keep_visible = self.always_show || scrolling || hovered || dragged;
        crate::effect::LaunchedEffect::new((fade_target, scrolling, hovered, dragged, scroll)).build(
            ctx,
            {
                let fade_state = fade_state.clone();
                move |scope| {
                    let fade_state = fade_state.clone();
                    async move {
                        if fade_target > 0.5 {
                            // 显示：tween 到 1（从当前值播，无闪烁）
                            crate::animation::push_animatable(
                                fade_state.clone(),
                                1.0,
                                crate::animation::AnimationSpec::Tween(
                                    crate::animation::TweenSpec::new(
                                        std::time::Duration::from_millis(250),
                                        crate::animation::interpolator::Linear::new(),
                                    ),
                                ),
                            );
                        }
                        // 停留 delay：期间滚动/hover/拖拽变化→key 变化→abort 睡眠→保持。
                        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
                        if !keep_visible {
                            crate::animation::push_animatable(
                                fade_state,
                                0.0,
                                crate::animation::AnimationSpec::Tween(
                                    crate::animation::TweenSpec::new(
                                        std::time::Duration::from_millis(250),
                                        crate::animation::interpolator::Linear::new(),
                                    ),
                                ),
                            );
                        }
                        drop(scope);
                    }
                }
            },
        );
        let viewport_state: State<f32> = ctx.remember(|| 0.0f32);
        let viewport = viewport_state.peek();
        let content = limit + viewport;

        let (thumb_offset, thumb_len, has_thumb) = match scrollbar_geometry(
            viewport,
            content,
            scroll,
            self.thumb_min_length,
            SCROLLBAR_THUMB_MAX_FRACTION,
            inset,
        ) {
            Some((off, len)) => (off, len, true),
            _ => (0.0, 0.0, false),
        };

        let track = (viewport - 2.0 * inset).max(0.0);
        let max_offset = (content - viewport).max(0.0);
        let scroll_state = self.scroll.clone();
        let drag_thumb_len = thumb_len;
        let m = Modifier::new()
            .height(thickness)
            .fill_max_width()
            .hoverable(&hover_src)
            .on_size_changed(move |w, _h| {
                viewport_state.set(w);
            })
            .draw_node(ScrollbarNode {
                vertical: false,
                thumb_color,
                track_color: self.track_color,
                thickness,
                inset,
                thumb_offset,
                thumb_len,
                has_thumb,
                fade: fade_state.clone(),
            })
            .on_drag_start({
                let hover_src = hover_src.clone();
                let scroll_state = scroll_state.clone();
                move |_pos: (f32, f32)| {
                    hover_src.emit_drag_start();
                    scroll_state.cancel_fling();
                    scroll_state.is_scroll_in_progress.set(true);
                }
            })
            .on_drag_end({
                let hover_src = hover_src.clone();
                let scroll_state = scroll_state.clone();
                move || {
                    hover_src.emit_drag_end();
                    scroll_state.is_scroll_in_progress.set(false);
                }
            })
            .on_drag(move |pos, _delta| {
                let pos_in_track = pos.0 - inset;
                let target = pos_in_track - drag_thumb_len / 2.0;
                let off = scrollbar_offset_for_thumb_pos(target, track, drag_thumb_len, max_offset);
                scroll_state.offset.set(off);
            });

        let m = m.then(self.modifier);
        match ctx.start_restartable_group(key, m, BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {}
        }
        ctx.end_restartable_group();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geometry_hidden_when_content_fits() {
        // content <= viewport → None（M3：内容放得下不画）
        assert!(scrollbar_geometry(600.0, 600.0, 0.0, 24.0, 0.9, 2.0).is_none());
        assert!(scrollbar_geometry(600.0, 400.0, 0.0, 24.0, 0.9, 2.0).is_none());
        assert!(scrollbar_geometry(0.0, 1000.0, 0.0, 24.0, 0.9, 2.0).is_none());
    }

    #[test]
    fn geometry_thumb_proportional() {
        // viewport 600 / content 2000 → thumb = track * 0.3
        // track = 600-4 = 596；thumb = 596*0.3 = 178.8（< max 596*0.9）
        let (off, len) = scrollbar_geometry(600.0, 2000.0, 0.0, 24.0, 0.9, 2.0).unwrap();
        assert!((len - 178.8).abs() < 0.01, "thumb={len}");
        assert_eq!(off, 0.0, "scroll=0 时 thumb 在顶");
        // scroll 到底：offset = (1400/1400) * (596-178.8) = 417.2
        let (off2, _) = scrollbar_geometry(600.0, 2000.0, 1400.0, 24.0, 0.9, 2.0).unwrap();
        assert!((off2 - 417.2).abs() < 0.01, "off2={off2}");
        // scroll 超界 clamp
        let (off3, _) = scrollbar_geometry(600.0, 2000.0, 9999.0, 24.0, 0.9, 2.0).unwrap();
        assert!((off3 - 417.2).abs() < 0.01, "超界 clamp 到底");
    }

    #[test]
    fn geometry_thumb_clamped_min_max() {
        // 内容巨大 → thumb 按 min 钳
        let (_, len) = scrollbar_geometry(600.0, 100000.0, 0.0, 24.0, 0.9, 2.0).unwrap();
        assert_eq!(len, 24.0, "thumb 下限 min");
        // 内容略超 → thumb 按 max 钳（track*0.9）
        let (_, len2) = scrollbar_geometry(600.0, 610.0, 0.0, 24.0, 0.9, 2.0).unwrap();
        assert!((len2 - 596.0 * 0.9).abs() < 0.01, "thumb 上限 track*0.9，len2={len2}");
    }

    #[test]
    fn offset_for_thumb_pos_round_trip() {
        // thumb 拖到 track 中点 → scroll 中点
        let track = 596.0;
        let thumb = 178.8;
        let max = 1400.0;
        let mid = scrollbar_offset_for_thumb_pos((track - thumb) / 2.0, track, thumb, max);
        assert!((mid - 700.0).abs() < 1.0, "mid={mid}");
        assert_eq!(scrollbar_offset_for_thumb_pos(-10.0, track, thumb, max), 0.0);
        assert_eq!(scrollbar_offset_for_thumb_pos(9999.0, track, thumb, max), max);
    }

    #[test]
    fn scrollbar_node_key_covers_params() {
        use crate::modifier::DrawNode;
        let mk = |off: f32, len: f32| ScrollbarNode {
            vertical: true,
            thumb_color: Color::from_argb(255, 1, 2, 3),
            track_color: Color::TRANSPARENT,
            thickness: 6.0,
            inset: 2.0,
            thumb_offset: off,
            thumb_len: len,
            has_thumb: true,
            fade: State::new(1.0),
        };
        assert_eq!(mk(0.0, 100.0).node_key(), mk(0.0, 100.0).node_key());
        assert_ne!(mk(0.0, 100.0).node_key(), mk(10.0, 100.0).node_key(), "offset 应进 key");
        assert_ne!(mk(0.0, 100.0).node_key(), mk(0.0, 120.0).node_key(), "len 应进 key");
    }

    /// 组件联动：Row 内滚动列 + scrollbar；offset.set 后 thumb key 跟随。
    /// viewport 靠 on_size_changed 回写（次帧），故跑两帧 compose+layout。
    /// LaunchedEffect（fade）需 tokio 上下文——测试内建 Runtime（对标 loading）。
    #[test]
    fn vertical_scrollbar_follows_scroll_state() {
        use crate::core::composer::Composer;
        use crate::layout::Constraints;
        use crate::ui::theme::{ThemeColors, WiniaTheme};
        use crate::modifier::ScrollState;
        let rt = tokio::runtime::Runtime::new().unwrap();
        let _guard = rt.enter();
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let scroll = ScrollState::new();
        // 模拟布局回写：内容 1000 - 视口 400 = 极限 600
        scroll.fling_limit.set(600.0);
        let mut composer = Composer::new();
        let build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    crate::ui::layout_components::Row::new().build(ctx, |ctx| {
                        crate::ui::layout_components::Column::new()
                            .modifier(
                                Modifier::new()
                                    .fill_max_height()
                                    .vertical_scroll(scroll.clone()),
                            )
                            .build(ctx, |ctx| {
                                let k = ctx.next_key();
                                ctx.start_leaf(k, Modifier::new().size(100.0, 1000.0));
                                ctx.end_node();
                            });
                        VerticalScrollbar::new(scroll.clone())
                            .always_show(true)
                            .build(ctx);
                    });
                });
            });
            composer.layout(Constraints::new(0.0, 200.0, 0.0, 400.0));
        };
        // 找 scrollbar 叶的 node_key（含 "scrollbar:" 前缀）
        fn scrollbar_key(nodes: &[crate::layout::LayoutNode], idx: usize) -> Option<String> {
            let m = &nodes[idx].modifier;
            for n in m.modifier_nodes() {
                let key = crate::modifier::node_key_of(n);
                if key.starts_with("draw:scrollbar:") {
                    return Some(key);
                }
            }
            for &c in &nodes[idx].children {
                if let Some(k) = scrollbar_key(nodes, c) { return Some(k); }
            }
            None
        }
        build(&mut composer);
        build(&mut composer); // 次帧：viewport 回写（400）生效
        let root = composer.layout_root_idx().expect("root");
        let key1 = scrollbar_key(composer.arena_nodes(), root)
            .expect("viewport 回写后应有 scrollbar 节点");
        // offset 0 → thumb 在顶；set(300) 后 key 变化（thumb 跟随）
        scroll.offset.set(300.0);
        build(&mut composer);
        let key2 = scrollbar_key(composer.arena_nodes(), root).expect("滚动后仍在");
        assert_ne!(key1, key2, "offset 变化后 thumb key 应跟随（key1={key1} key2={key2}）");
        assert!(key2.ends_with(":true"), "常显模式 key 末尾 has_thumb=true，key2={key2}");
    }
}
