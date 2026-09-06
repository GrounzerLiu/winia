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
    // 非有限输入防腐（offset 是 pub State，用户可 set(NaN)；NaN 进 clamp/
    // skia rect 均未定义行为）——直接不画。
    if !viewport.is_finite() || !content.is_finite() || !scroll.is_finite() {
        return None;
    }
    if viewport <= 0.0 || content <= viewport {
        return None;
    }
    let track = (viewport - 2.0 * inset).max(0.0);
    if track <= 0.0 || track < thumb_min {
        return None;
    }
    let max_thumb = track * thumb_max_fraction;
    // P0-1：max_thumb < thumb_min 时 clamp(min>max) 直接 panic（小 track/
    // 大 thumb_min 时必现）——track 放不下最小 thumb 则不画（与上式语义一致）。
    if max_thumb < thumb_min {
        return None;
    }
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
    if !pos.is_finite() || !track.is_finite() || !thumb.is_finite() || !max_offset.is_finite() {
        return 0.0;
    }
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

// ── 共享 build 逻辑（P2-7：垂直/水平两套 build 逐行重复，修 bug 漏改——
// 横向 viewport peek→get 漏改首屏不显示即实测案例。故抽公共函数：
// remember 顺序固定（调用序即槽位序）：hover_src → last_scroll → last_pulse →
// fade_state → effect（单槽位）→ viewport_state → grab_offset——三组件
// （Vertical/Horizontal/Lazy）同调同一函数，同序无槽位漂移。
// `LazyScrollbar::build` 在调共享函数前多一个 `scrolling_state` remember，
// 但各组件 scope 不同（`#[composable]` 源码哈希隔离），槽位互不干扰。
// 轴差异由 ScrollbarAxis 参数化（4 处）：尺寸元素、
// on_size_changed 轴、drag 本地坐标轴、ScrollbarNode.vertical。
/// 滚动条轴（垂直/水平 build 差异的全部参数化）。
#[derive(Clone, Copy)]
enum ScrollbarAxis {
    Vertical,
    Horizontal,
}

/// 共享配置（两组件外观参数子集——changed 注册用；Copy 进闭包，无借用逃逸）。
#[derive(Clone, Copy)]
struct ScrollbarConfig {
    thickness: f32,
    thumb_color: Option<Color>,
    track_color: Color,
    thumb_min_length: f32,
    always_show: bool,
    inset: f32,
}

/// 共享 build：读三要素 → fade 脉冲/effect → viewport → 组装 Modifier → 返回。
/// changed 注册仍在各 build 头部（参数归属各自 Self）。
/// `scroll_reverse`: 横向 reverse（RTL）时传入 Some（`is_horizontal_scroll_reversed`
/// 的值）——offset 语义被 render 镜像（offset 0 = 内容末端），scrollbar 几何/
/// 拖拽用同一镜像坐标 `visual = max - scroll`；垂直传 None。
/// `None` 与 `Some(false)` 等价（都直用）；`Option` 造型是为了直接透传
/// `is_horizontal_scroll_reversed()` 的返回值。
fn scrollbar_build_shared(
    ctx: &mut ComposeCtx,
    scroll_state_src: &crate::modifier::ScrollState,
    cfg: ScrollbarConfig,
    axis: ScrollbarAxis,
    scroll_reverse: Option<bool>,
) -> Modifier {
    let vertical = matches!(axis, ScrollbarAxis::Vertical);
    let theme = WiniaTheme::colors();
    let thumb_color = cfg
        .thumb_color
        .unwrap_or_else(|| scrollbar_thumb_color(&theme));
    // 三要素：offset（读，注册依赖）+ fling_limit（读，注册依赖）；
    // viewport 由自身约束来（fill_max_height 下 = 父高，要求与滚动容器等高）。
    // ⚠ 必须在 build 期 get（注册组合依赖），绘制期 peek 不注册。
    let scroll_raw = scroll_state_src.offset.get();
    let limit = scroll_state_src.fling_limit.get();
    let scrolling = scroll_state_src.is_scroll_in_progress.get();
    // P2-3：reverse 时 offset 语义镜像——scrollbar 用 visual 坐标
    // （visual 0 = 内容末端 = 条在起点；visual = max - scroll）。
    // 垂直无 reverse 概念，传 None 即直用。
    let scroll = match scroll_reverse {
        Some(true) => (limit - scroll_raw).clamp(0.0, limit.max(0.0)),
        _ => scroll_raw,
    };
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
    // 拖拽路径才置 true）——故滚动检测不用它，而用"offset/脉冲变化"：
    // `last_scroll` 记住上帧 offset，本帧不同即正在滚（set_silent 写回，
    // 不触发重组——本帧 build 照常继续）；`scroll_pulse` 是分发层在"命中
    // 但消费为 0"（顶/底继续滚，offset 无变化）时自增的计数（P1-3）——
    // 两者任一变化即点亮 fade，给"到底了"的反馈（M3/CMP 同行为）。
    // ⚠ 不能用 bool 门闩（滚过即 true 不清零→常显不藏，实测 bug）。
    // ⚠ key 必须含 `scroll`：bool 条件会合并连续滚动——tween 250ms 播到 1
    // 后 key 不变，而滚动仍在继续，随后的 400ms 睡眠会把条 fade 掉；
    // scroll 每帧都变→每帧重启 effect→abort 睡眠→保持显示。
    // （pulse 只在边界触发一次，不进 key——单次脉冲若进 key 会与 scroll
    // 同帧双重启，无谓 churn；pulse 的 get 注册依赖已足够驱动点亮帧。）
    let last_scroll: State<f32> = ctx.remember(|| scroll);
    let scroll_active = last_scroll.peek() != scroll;
    if scroll_active {
        last_scroll.set_silent(scroll);
    }
    let last_pulse: State<u64> = ctx.remember(|| scroll_state_src.scroll_pulse.get());
    let pulse_active = last_pulse.peek() != scroll_state_src.scroll_pulse.get();
    if pulse_active {
        last_pulse.set_silent(scroll_state_src.scroll_pulse.get());
    }
    let fade_target = if cfg.always_show || scrolling || hovered || dragged
        || scroll_active || pulse_active
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
    let keep_visible = cfg.always_show || scrolling || hovered || dragged;
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
    // ⚠ P0-2：必须 `get`（注册组合依赖）——`set` 的 notify 才能驱动重组；
    // `peek` 无订阅者→回写后无重组→首屏/resize 后几何 stale（手动 double-build
    // 的测试掩盖了此问题；横向曾漏改 peek→首屏不显示，实测 bug）。`set` 有
    // PartialEq 去重 + on_size_changed 元素级去重，尺寸稳定即停，无循环。
    let viewport = viewport_state.get();
    let content = limit + viewport;

    let (thumb_offset, thumb_len, has_thumb) = match scrollbar_geometry(
        viewport,
        content,
        scroll,
        cfg.thumb_min_length,
        SCROLLBAR_THUMB_MAX_FRACTION,
        cfg.inset,
    ) {
        Some((off, len)) => (off, len, true),
        _ => (0.0, 0.0, false),
    };

    // 拖 thumb → 反推 offset（需 track/thumb/max——build 期值，闭包捕获）
    let track = (viewport - 2.0 * cfg.inset).max(0.0);
    let max_offset = (content - viewport).max(0.0);
    let drag_thumb_len = thumb_len;
    // P1-1（CMP 语义）：抓取偏移保持——按下时记 grab = 按下点(track 内) -
    // 当时 thumb 顶部，拖动时 target = 光标 - grab，thumb 跟手不跳。
    // 旧中心对齐（target = 光标 - thumb/2）大 thumb 下首帧跳变，实测修。
    // grab 是交互期临时值（remember 持有，不进 node_key、不注册依赖——
    // set_silent 写，build 内 peek 读；拖动中 offset.set 驱动重组已足够）。
    let grab_offset: State<f32> = ctx.remember(|| 0.0f32);
    let drag_inset = cfg.inset;
    let drag_cb = {
        let scroll_state_src = scroll_state_src.clone();
        let grab_offset = grab_offset.clone();
        // reverse 下 visual→raw 需镜像写回（visual = max - raw）。
        let reverse_write = matches!(scroll_reverse, Some(true));
        let write_max = max_offset;
        move |pos: (f32, f32)| {
            // pos 为 thumb 条本地坐标（含 inset 偏移）；减 inset 得 track 内位置
            let pos_in_track = if vertical { pos.1 } else { pos.0 } - drag_inset;
            let target = pos_in_track - grab_offset.peek();
            let off_visual = scrollbar_offset_for_thumb_pos(target, track, drag_thumb_len, max_offset);
            let off = if reverse_write {
                (write_max - off_visual).clamp(0.0, write_max.max(0.0))
            } else {
                off_visual
            };
            scroll_state_src.offset.set(off);
        }
    };
    let (hover_src_clone, scroll_state_clone) =
        (hover_src.clone(), scroll_state_src.clone());
    let (hover_src_clone2, scroll_state_clone2) =
        (hover_src.clone(), scroll_state_src.clone());
    // P1-2：drag cancel 与 end 完全相同的清理（窗口失焦/系统打断不走 end）——
    // 否则 is_scroll_in_progress/dragged 残留置位→fade 常显不藏。
    let m = if vertical {
        Modifier::new()
            .width(cfg.thickness)
            .fill_max_height()
    } else {
        Modifier::new()
            .height(cfg.thickness)
            .fill_max_width()
    }
    .hoverable(&hover_src)
    .on_size_changed(move |w, h| {
        if vertical {
            let _ = w;
            viewport_state.set(h);
        } else {
            viewport_state.set(w);
        }
    })
    .draw_node(ScrollbarNode {
        vertical,
        thumb_color,
        track_color: cfg.track_color,
        thickness: cfg.thickness,
        inset: cfg.inset,
        thumb_offset,
        thumb_len,
        has_thumb,
        fade: fade_state.clone(),
    })
    .on_drag_start({
        let hover_src = hover_src_clone.clone();
        let scroll_state = scroll_state_clone.clone();
        let grab_offset = grab_offset.clone();
        move |pos: (f32, f32)| {
            hover_src.emit_drag_start();
            // 抢占：取消列表侧惯性 fling（否则 update_animations 下帧
            // 把 offset 写回，thumb 拖不动）；标记滚动中（松手清）。
            scroll_state.cancel_fling();
            scroll_state.is_scroll_in_progress.set(true);
            // 抓取偏移 = 按下点(track 内) - 当时 thumb 顶部（build 期值捕获）。
            // clamp 到 [0, thumb]：按下点若在 thumb 外（track 空白处按下），
            // grab 钳到 thumb 边缘，避免首帧大跳（等价于"点哪 thumb 边缘跟到哪"）。
            let pos_in_track = if vertical { pos.1 } else { pos.0 } - drag_inset;
            grab_offset.set_silent((pos_in_track - thumb_offset).clamp(0.0, drag_thumb_len.max(0.0)));
        }
    })
    .on_drag_end(move || {
        hover_src_clone2.emit_drag_end();
        scroll_state_clone2.is_scroll_in_progress.set(false);
    })
    .on_drag_cancel({
        let hover_src = hover_src.clone();
        let scroll_state_src = scroll_state_src.clone();
        move || {
            hover_src.emit_drag_end();
            scroll_state_src.is_scroll_in_progress.set(false);
        }
    })
    .on_drag(move |pos, _delta| drag_cb(pos));
    m
}

// ── Lazy 滚动条适配 ──
//
// LazyColumn/LazyRow 用 LazyListState（锚点模型：offset 像素 + first_visible
// 派生），而 Vertical/HorizontalScrollbar 吃 ScrollState（像素模型）。
// LazyListState.offset 与 ScrollState.offset 是同一像素语义（lazy 滚动输入
// 走 vertical_scroll 通道，offset 挂同一驱动），故适配 = 把 LazyListState 的
// 三个通道转成 ScrollState 句柄：offset（读依赖）+ fling_limit（读依赖）+
// is_scroll_in_progress（lazy 侧无此字段——用 remember 持有 false 常量？
// 不：lazy build 内有 is_scrolling remember（lazy_column.rs:643），但外部只
// 拿到 LazyListState，拿不到它。故此处只转 offset/fling_limit/pulse，
// scrolling 恒 false——wheel/程序化滚动本来就不置它（分发层只 cancel+置
// false），fade 靠 offset 脉冲 + pulse 点亮，拖 thumb 则走 scrollbar 自己的
// on_drag_start 置位（scrollbar 自带 ScrollState 拼装，故拖拽显示正常）。
// pulse 同样 clone（lazy 已接通，见 lazy_column.rs）。
//
// 用法：
// ```ignore
// let list_state = ctx.remember(|| LazyListState::new()).get();
// LazyColumn::new().state(list_state.clone()).build(ctx, content);
// LazyScrollbar::new(list_state).build(ctx);
// ```
/// Lazy 列表滚动条（垂直，对标 CMP `VerticalScrollbar` + LazyListState 适配）。
/// `scroll_reverse`: 反向懒列表（`reverse_layout(true)`）——render 侧 offset
/// 语义镜像（offset 0 = 内容末端），scrollbar 几何/拖拽用同一镜像坐标
/// `visual = max - scroll`。`None` 与 `Some(false)` 等价（都直用）。
pub struct LazyScrollbar {
    state: crate::ui::lazy_column::LazyListState,
    modifier: Modifier,
    thickness: f32,
    thumb_color: Option<Color>,
    track_color: Color,
    thumb_min_length: f32,
    always_show: bool,
    scroll_reverse: Option<bool>,
}

impl LazyScrollbar {
    pub fn new(state: crate::ui::lazy_column::LazyListState) -> Self {
        Self {
            state,
            modifier: Modifier::new(),
            thickness: SCROLLBAR_THICKNESS,
            thumb_color: None,
            track_color: Color::TRANSPARENT,
            thumb_min_length: SCROLLBAR_THUMB_MIN_LENGTH,
            always_show: false,
            scroll_reverse: None,
        }
    }

    pub fn modifier(mut self, m: Modifier) -> Self { self.modifier = self.modifier.then(m); self }
    pub fn thickness(mut self, t: f32) -> Self { self.thickness = t; self }
    pub fn thumb_color(mut self, c: Color) -> Self { self.thumb_color = Some(c); self }
    pub fn track_color(mut self, c: Color) -> Self { self.track_color = c; self }
    pub fn thumb_min_length(mut self, l: f32) -> Self { self.thumb_min_length = l; self }
    pub fn always_show(mut self, v: bool) -> Self { self.always_show = v; self }
    /// 反向懒列表（`reverse_layout(true)`——与列表的 reverse 同值）。
    pub fn scroll_reverse(mut self, v: Option<bool>) -> Self { self.scroll_reverse = v; self }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        ctx.changed(&self.thickness);
        ctx.changed(&self.thumb_color);
        ctx.changed(&self.track_color);
        ctx.changed(&self.thumb_min_length);
        ctx.changed(&self.always_show);
        ctx.changed(&self.scroll_reverse);
        ctx.changed(&self.state.offset.state_id());
        let key = ctx.next_key();
        let cfg = ScrollbarConfig {
            thickness: self.thickness,
            thumb_color: self.thumb_color,
            track_color: self.track_color,
            thumb_min_length: self.thumb_min_length,
            always_show: self.always_show,
            inset: 2.0,
        };
        // 拼装 ScrollState：offset/fling_limit/pulse clone（同 State 跨帧稳定）；
        // is_scroll_in_progress 用 remember 持有 false（lazy 无此通道；fade 靠
        // 脉冲，拖拽时 scrollbar 自己的 on_drag_start 置位本拼装 state——注意
        // 这不会回写到 LazyListState，但 fade 条件读的是拼装后的 scrolling，
        // 同一帧内有效；松手清同样落拼装 state，无残留）。
        let scrolling_state: State<bool> = ctx.remember(|| false);
        let assembled = crate::modifier::ScrollState {
            offset: self.state.offset.clone(),
            is_scroll_in_progress: scrolling_state.clone(),
            fling_limit: self.state.fling_limit.clone(),
            scroll_pulse: self.state.scroll_pulse.clone(),
        };
        let m = scrollbar_build_shared(ctx, &assembled, cfg, ScrollbarAxis::Vertical, self.scroll_reverse)
            .then(self.modifier);
        match ctx.start_restartable_group(key, m, BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {}
        }
        ctx.end_restartable_group();
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
        // P2-4：ScrollState 句柄身份（换整个 state 必须重组——否则旧 offset
        // 订阅不失效、新 state 无订阅，条绑死旧状态）。
        ctx.changed(&self.scroll.offset.state_id());
        let key = ctx.next_key();
        let cfg = ScrollbarConfig {
            thickness: self.thickness,
            thumb_color: self.thumb_color,
            track_color: self.track_color,
            thumb_min_length: self.thumb_min_length,
            always_show: self.always_show,
            inset: 2.0,
        };
        let m = scrollbar_build_shared(ctx, &self.scroll, cfg, ScrollbarAxis::Vertical, None)
            .then(self.modifier);
        match ctx.start_restartable_group(key, m, BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {}
        }
        ctx.end_restartable_group();
    }
}

// ── HorizontalScrollbar ──

/// 水平滚动条（对标 CMP `HorizontalScrollbar`）。语义与垂直对称。
/// `scroll_reverse`: 横向 reverse（RTL，`horizontal_scroll_reverse(true)`）——
/// render 侧 offset 语义镜像（offset 0 = 内容末端），scrollbar 几何/拖拽用
/// 同一镜像坐标（`visual = max - scroll`），条位置与内容同向。
/// 不传（None）= 正向。垂直条无 reverse 概念。
pub struct HorizontalScrollbar {
    scroll: crate::modifier::ScrollState,
    modifier: Modifier,
    thickness: f32,
    thumb_color: Option<Color>,
    track_color: Color,
    thumb_min_length: f32,
    always_show: bool,
    scroll_reverse: Option<bool>,
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
            scroll_reverse: None,
        }
    }

    pub fn modifier(mut self, m: Modifier) -> Self { self.modifier = self.modifier.then(m); self }
    pub fn thickness(mut self, t: f32) -> Self { self.thickness = t; self }
    pub fn thumb_color(mut self, c: Color) -> Self { self.thumb_color = Some(c); self }
    pub fn track_color(mut self, c: Color) -> Self { self.track_color = c; self }
    pub fn thumb_min_length(mut self, l: f32) -> Self { self.thumb_min_length = l; self }
    pub fn always_show(mut self, v: bool) -> Self { self.always_show = v; self }
    /// 横向反向滚动（RTL——与滚动容器的 `horizontal_scroll_reverse` 同值；
    /// `TabRow` 等 RTL 容器传 `Some(is_rtl)`）。
    pub fn scroll_reverse(mut self, v: Option<bool>) -> Self { self.scroll_reverse = v; self }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx) {
        ctx.changed(&self.thickness);
        ctx.changed(&self.thumb_color);
        ctx.changed(&self.track_color);
        ctx.changed(&self.thumb_min_length);
        ctx.changed(&self.always_show);
        ctx.changed(&self.scroll_reverse);
        // P2-4：ScrollState 句柄身份（换整个 state 必须重组——否则旧 offset
        // 订阅不失效、新 state 无订阅，条绑死旧状态）。
        ctx.changed(&self.scroll.offset.state_id());
        let key = ctx.next_key();
        let cfg = ScrollbarConfig {
            thickness: self.thickness,
            thumb_color: self.thumb_color,
            track_color: self.track_color,
            thumb_min_length: self.thumb_min_length,
            always_show: self.always_show,
            inset: 2.0,
        };
        let m = scrollbar_build_shared(ctx, &self.scroll, cfg, ScrollbarAxis::Horizontal, self.scroll_reverse)
            .then(self.modifier);
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
    fn geometry_small_track_no_panic() {
        // P0-1 回归：track=26 < max(24/0.9)=26.67 时 max_thumb(23.4) < thumb_min(24)，
        // 旧 clamp(min>max) 直接 panic——现应返回 None（track 放不下最小 thumb 则不画）。
        assert!(scrollbar_geometry(30.0, 1000.0, 0.0, 24.0, 0.9, 2.0).is_none());
        // thumb_min > track 同样不画（已有 track < thumb_min 分支锁定）。
        assert!(scrollbar_geometry(30.0, 1000.0, 0.0, 30.0, 0.9, 2.0).is_none());
    }

    #[test]
    fn geometry_rejects_nonfinite() {
        // P2-2：NaN/Inf 输入不 panic、不产出 NaN 几何。
        assert!(scrollbar_geometry(f32::NAN, 1000.0, 0.0, 24.0, 0.9, 2.0).is_none());
        assert!(scrollbar_geometry(600.0, f32::INFINITY, 0.0, 24.0, 0.9, 2.0).is_none());
        assert!(scrollbar_geometry(600.0, 2000.0, f32::NAN, 24.0, 0.9, 2.0).is_none());
        assert_eq!(scrollbar_offset_for_thumb_pos(f32::NAN, 596.0, 178.8, 1400.0), 0.0);
        assert_eq!(scrollbar_offset_for_thumb_pos(100.0, f32::NAN, 178.8, 1400.0), 0.0);
    }

    #[test]
    fn offset_for_thumb_pos_degenerate_travel() {
        // travel<=0（track<=thumb）时不除零、不 NaN（.max(1.0) 保护锁定）。
        let v = scrollbar_offset_for_thumb_pos(10.0, 100.0, 100.0, 500.0);
        assert!(v.is_finite(), "v={v}");
        let v2 = scrollbar_offset_for_thumb_pos(10.0, 50.0, 100.0, 500.0);
        assert!(v2.is_finite(), "v2={v2}");
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
    fn drag_preserves_grab_offset() {
        // P1-1 回归：grab 保持（非中心对齐）。thumb_offset=100、thumb=178.8，
        // 按下点 pos=150（thumb 内，grab=50）后微移到 160：
        // grab 语义 target=160-50=110；旧中心对齐 target=160-89.4=70.6。
        // 断言走 grab 路径（差值应 <1px），锁定"跟手不跳"。
        let track = 596.0;
        let thumb = 178.8;
        let max = 1400.0;
        let thumb_offset = 100.0f32;
        let inset = 2.0f32;
        // 模拟 on_drag_start 的 grab 计算（含 clamp）：grab=(150-2-100)=48
        let grab = ((150.0 - inset - thumb_offset).clamp(0.0, thumb));
        assert!((grab - 48.0).abs() < 0.01, "grab={grab}");
        // 模拟 on_drag：pos=160 → target=160-2-48=110
        let target = (160.0 - inset) - grab;
        let off_grab = scrollbar_offset_for_thumb_pos(target, track, thumb, max);
        let off_center =
            scrollbar_offset_for_thumb_pos((160.0 - inset) - thumb / 2.0, track, thumb, max);
        assert!((off_grab - off_center).abs() > 50.0, "两语义应显著不同（grab={off_grab} center={off_center}）");
        // grab 路径增量 = (160-150)/travel*max ≈ 10/417.2*1400 ≈ 33.5
        let travel = track - thumb;
        assert!((off_grab - scrollbar_offset_for_thumb_pos(150.0 - inset - grab, track, thumb, max) - 10.0 / travel * max).abs() < 1.0);
        // 按下点在 thumb 外（track 空白处 pos=400）：grab 钳到 thumb（178.8），
        //  thumb 边缘跟到光标，不大跳到中心。
        let grab_clamped = (400.0 - inset - thumb_offset).clamp(0.0, thumb);
        assert!((grab_clamped - thumb).abs() < 0.01, "thumb 外按下应钳到边缘，grab={grab_clamped}");
    }

    #[test]
    fn reverse_visual_round_trip() {
        // P2-3 回归：reverse 下 visual = max - raw，来回镜像精确还原。
        // max=1400：raw=0 → visual=1400（条在起点，内容末端）；raw=1400 → visual=0。
        let max = 1400.0f32;
        let to_visual = |raw: f32| (max - raw).clamp(0.0, max.max(0.0));
        assert_eq!(to_visual(0.0), 1400.0);
        assert_eq!(to_visual(1400.0), 0.0);
        assert_eq!(to_visual(300.0), 1100.0);
        // 写回镜像：visual→raw 同公式（对合），拖拽往返不漂移。
        let back = |visual: f32| (max - visual).clamp(0.0, max.max(0.0));
        assert_eq!(back(to_visual(300.0)), 300.0);
        // 超界钳：raw 越界时 visual 钳到 [0, max]。
        assert_eq!(to_visual(-50.0), 1400.0);
        assert_eq!(to_visual(9999.0), 0.0);
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
        // fade 值不同但 key 相同（fade 不进 key——逐帧动画值，进则每帧 Enter；
        // 绘制由引擎 request_redraw 驱动，draw 期 peek 读最新值）。
        let mut a = mk(0.0, 100.0);
        a.fade = State::new(0.0);
        let b = mk(0.0, 100.0);
        assert_eq!(a.node_key(), b.node_key(), "fade 不应进 key");
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

    /// Lazy 适配联动：LazyColumn + LazyScrollbar 共享 LazyListState；
    /// fling_limit 回写后 thumb 出现，offset.set 后 thumb key 跟随。
    #[test]
    fn lazy_scrollbar_follows_lazy_list_state() {
        use crate::core::composer::Composer;
        use crate::layout::Constraints;
        use crate::ui::lazy_column::LazyListState;
        use crate::ui::theme::{ThemeColors, WiniaTheme};
        let rt = tokio::runtime::Runtime::new().unwrap();
        let _guard = rt.enter();
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let list_state = LazyListState::new();
        // 模拟测量回写：内容 2000 - 视口 400 = 极限 1600
        list_state.fling_limit.set(1600.0);
        let mut composer = Composer::new();
        let build = |composer: &mut Composer| {
            composer.compose(|ctx| {
                WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    crate::ui::layout_components::Row::new().build(ctx, |ctx| {
                        crate::ui::lazy_column::LazyColumn::new()
                            .state(list_state.clone())
                            .modifier(Modifier::new().fill_max_height().layout_weight(1.0))
                            .items_plain(30, |ctx, _i| {
                                let k = ctx.next_key();
                                ctx.start_leaf(k, Modifier::new().size(100.0, 60.0));
                                ctx.end_node();
                            })
                            .build(ctx);
                        LazyScrollbar::new(list_state.clone())
                            .always_show(true)
                            .build(ctx);
                    });
                });
            });
            composer.layout(Constraints::new(0.0, 200.0, 0.0, 400.0));
        };
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
        build(&mut composer); // 次帧：viewport 回写生效
        let root = composer.layout_root_idx().expect("root");
        let key1 = scrollbar_key(composer.arena_nodes(), root)
            .expect("viewport 回写后应有 lazy scrollbar 节点");
        list_state.offset.set(500.0);
        build(&mut composer);
        let key2 = scrollbar_key(composer.arena_nodes(), root).expect("滚动后仍在");
        assert_ne!(key1, key2, "offset 变化后 thumb key 应跟随（key1={key1} key2={key2}）");
        assert!(key2.ends_with(":true"), "常显模式 key 末尾 has_thumb=true，key2={key2}");
    }

    /// T8（P1-1 锁定）：反向懒列表 + LazyScrollbar(reverse=Some(true))，
    /// offset=0 时 thumb 应在起点（与正向 offset=max 同 key）；不传 reverse
    /// 则反向（与正向 offset=0 同 key）——锁定"条跟内容走"。
    #[test]
    fn lazy_reverse_mirror() {
        use crate::core::composer::Composer;
        use crate::layout::Constraints;
        use crate::ui::lazy_column::LazyListState;
        use crate::ui::theme::{ThemeColors, WiniaTheme};
        let rt = tokio::runtime::Runtime::new().unwrap();
        let _guard = rt.enter();
        let theme = ThemeColors::light_from_seed(0x6750A4);
        fn build_lazy(
            composer: &mut Composer,
            theme: &ThemeColors,
            list_state: &LazyListState,
            reverse_bar: bool,
        ) {
            composer.compose(|ctx| {
                WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    crate::ui::layout_components::Row::new().build(ctx, |ctx| {
                        crate::ui::lazy_column::LazyColumn::new()
                            .state(list_state.clone())
                            .reverse_layout(true)
                            .modifier(Modifier::new().fill_max_height().layout_weight(1.0))
                            .items_plain(30, |ctx, _i| {
                                let k = ctx.next_key();
                                ctx.start_leaf(k, Modifier::new().size(100.0, 60.0));
                                ctx.end_node();
                            })
                            .build(ctx);
                        let bar = LazyScrollbar::new(list_state.clone()).always_show(true);
                        let bar = if reverse_bar {
                            bar.scroll_reverse(Some(true))
                        } else {
                            bar
                        };
                        bar.build(ctx);
                    });
                });
            });
            composer.layout(Constraints::new(0.0, 200.0, 0.0, 400.0));
        }
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
        // 正向 build helper（与 build_lazy 同结构，不带 reverse）
        fn build_fwd(
            composer: &mut Composer,
            theme: &ThemeColors,
            list_state: &LazyListState,
        ) {
            composer.compose(|ctx| {
                WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                    crate::ui::layout_components::Row::new().build(ctx, |ctx| {
                        crate::ui::lazy_column::LazyColumn::new()
                            .state(list_state.clone())
                            .modifier(Modifier::new().fill_max_height().layout_weight(1.0))
                            .items_plain(30, |ctx, _i| {
                                let k = ctx.next_key();
                                ctx.start_leaf(k, Modifier::new().size(100.0, 60.0));
                                ctx.end_node();
                            })
                            .build(ctx);
                        LazyScrollbar::new(list_state.clone())
                            .always_show(true)
                            .build(ctx);
                    });
                });
            });
            composer.layout(Constraints::new(0.0, 200.0, 0.0, 400.0));
        }
        // reverse + 镜像：offset=0 → visual=max → thumb 在末端（offset 最大同位）
        let list_state = LazyListState::new();
        let mut composer = Composer::new();
        build_lazy(&mut composer, &theme, &list_state, true);
        build_lazy(&mut composer, &theme, &list_state, true);
        // 读回测量期真实回写的 limit（set(1600) 会被 measure 覆盖为真实值）
        let root = composer.layout_root_idx().expect("root");
        let real_limit = list_state.fling_limit.get();
        let key_rev0 = scrollbar_key(composer.arena_nodes(), root).expect("reverse 条应存在");
        // 正向 offset=limit → thumb 在末端：与 reverse offset=0 同 key
        // （visual 相同 → 几何相同；fade 状态差异不影响 key——fade 不进 key）。
        let list_state2 = LazyListState::new();
        let mut composer2 = Composer::new();
        // 先跑两帧让测量回写 limit，再设 offset=limit（与 reverse 侧同 max）
        build_fwd(&mut composer2, &theme, &list_state2);
        build_fwd(&mut composer2, &theme, &list_state2);
        list_state2.offset.set(list_state2.fling_limit.get());
        build_fwd(&mut composer2, &theme, &list_state2);
        let root2 = composer2.layout_root_idx().expect("root");
        let key_fwd_max = scrollbar_key(composer2.arena_nodes(), root2).expect("正向条应存在");
        assert_eq!(key_rev0, key_fwd_max, "reverse offset=0 应与正向 offset=max 同几何");
        // 不传 reverse：offset=0 → thumb 在顶（与正向 offset=0 同位，反向错误）。
        let list_state3 = LazyListState::new();
        let mut composer3 = Composer::new();
        build_lazy(&mut composer3, &theme, &list_state3, false);
        build_lazy(&mut composer3, &theme, &list_state3, false);
        let root3 = composer3.layout_root_idx().expect("root");
        let key_nobar = scrollbar_key(composer3.arena_nodes(), root3).expect("条应存在");
        assert_ne!(key_nobar, key_rev0, "不传 reverse 应与镜像位不同（反向错误复现）");
    }
}
