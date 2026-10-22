//! Switch 组件 — 按项目自定义设计实现（用户定义的“两层 ripple + Handle 容器”模型）
//!
//! 结构：
//! - 轨道 52×32、CornerFull、2dp 边框（内缩绘制，不超出组件范围）；
//! - Handle 是 28×28 的**容器**（自身带 unbounded ripple），位置：
//!   关闭 (2,2)、开启 (22,2)，位置 Spring 动画；
//! - 容器中心是**有颜色的圆形**：关闭 16×16、开启 24×24、
//!   按下/拖拽 28×28，尺寸与颜色均动画过渡；
//! - 背景色、边框色、中心圆颜色切换时 180ms 过渡；
//! - 按下：中心圆放大到 28×28（有动画），按住保持，松手按状态决定
//!   回 16×16 还是保持 28×28；
//! - 拖拽：容器跟随手指（2..22），释放按中点 12 判定切换。

use crate::runtime::composer::{ComposeCtx, GroupStatus};
use crate::composable;
use crate::layout::BoxLayout;
use crate::modifier::{Modifier};
use crate::graphics::{Color, Shape};
use crate::input::interaction::MutableInteractionSource;
use crate::theme::{ThemeColors, WiniaTheme};
use std::sync::Arc;

/// 轨道尺寸（`SwitchTokens.TrackWidth/Height`）
pub const SWITCH_TRACK_WIDTH: f32 = 52.0;
pub const SWITCH_TRACK_HEIGHT: f32 = 32.0;
/// 轨道边框（`SwitchTokens.TrackOutlineWidth`）
pub const SWITCH_TRACK_OUTLINE_WIDTH: f32 = 2.0;
/// Handle 容器尺寸（固定 28×28——ripple 挂在容器上）
pub const SWITCH_THUMB_CONTAINER_SIZE: f32 = 28.0;
/// 中心圆尺寸：开启 24×24
pub const SWITCH_THUMB_CHECKED_SIZE: f32 = 24.0;
/// 未选中拇指（`UnselectedHandleWidth = 16`）
pub const SWITCH_THUMB_UNCHECKED_SIZE: f32 = 16.0;
/// 拇指内容图标尺寸（`SwitchDefaults.IconSize`）
pub const SWITCH_ICON_SIZE: f32 = 16.0;
/// 容器偏移：关闭 x=2、开启 x=22（28px 容器在 52px 轨道内的 2px 边距）
pub const SWITCH_DRAG_MIN_OFFSET: f32 = SWITCH_TRACK_OUTLINE_WIDTH;
pub const SWITCH_DRAG_MAX_OFFSET: f32 =
    SWITCH_TRACK_WIDTH - SWITCH_TRACK_OUTLINE_WIDTH - SWITCH_THUMB_CONTAINER_SIZE;
pub const SWITCH_DRAG_THRESHOLD: f32 = (SWITCH_DRAG_MIN_OFFSET + SWITCH_DRAG_MAX_OFFSET) / 2.0;

/// The thumb's two spatial animations — its slide (the container offset) and its size — run on M3's
/// `FastSpatial` token: `material3/Switch.kt:162` hands the thumb element
/// `MotionSchemeKeyTokens.FastSpatial.value()`, which is damping 0.9 / stiffness 1400
/// (`tokens/StandardMotionTokens.kt:24-25`). winia used `SpringSpec::default()` (no-bouncy, 200) for
/// both — softer than M3, and without the slight overshoot the token has.
fn switch_thumb_spec() -> crate::animation::SpringSpec {
    crate::animation::SpringSpec::spatial_fast()
}

/// Switch 颜色集（对标 material3 `SwitchColors`）——enabled/disabled ×
/// checked/unchecked 的 thumb/track/border/icon 四组色。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SwitchColors {
    pub checked_thumb: Color,
    pub checked_track: Color,
    pub checked_border: Color,
    pub checked_icon: Color,
    pub unchecked_thumb: Color,
    pub unchecked_track: Color,
    pub unchecked_border: Color,
    pub unchecked_icon: Color,
    pub disabled_checked_thumb: Color,
    pub disabled_checked_track: Color,
    pub disabled_checked_border: Color,
    pub disabled_checked_icon: Color,
    pub disabled_unchecked_thumb: Color,
    pub disabled_unchecked_track: Color,
    pub disabled_unchecked_border: Color,
    pub disabled_unchecked_icon: Color,
}

impl SwitchColors {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        checked_thumb: Color,
        checked_track: Color,
        checked_border: Color,
        checked_icon: Color,
        unchecked_thumb: Color,
        unchecked_track: Color,
        unchecked_border: Color,
        unchecked_icon: Color,
        disabled_checked_thumb: Color,
        disabled_checked_track: Color,
        disabled_checked_border: Color,
        disabled_checked_icon: Color,
        disabled_unchecked_thumb: Color,
        disabled_unchecked_track: Color,
        disabled_unchecked_border: Color,
        disabled_unchecked_icon: Color,
    ) -> Self {
        Self {
            checked_thumb,
            checked_track,
            checked_border,
            checked_icon,
            unchecked_thumb,
            unchecked_track,
            unchecked_border,
            unchecked_icon,
            disabled_checked_thumb,
            disabled_checked_track,
            disabled_checked_border,
            disabled_checked_icon,
            disabled_unchecked_thumb,
            disabled_unchecked_track,
            disabled_unchecked_border,
            disabled_unchecked_icon,
        }
    }

    /// 从主题推导默认色（对标 `SwitchDefaults.colors()` / `SwitchTokens` 1.4.0；
    /// disabled 为 token alpha compositeOver(surface)）
    pub fn from_theme(theme: &ThemeColors) -> Self {
        let transparent = Color::from_argb(0, 0, 0, 0);
        let over = |c: Color, a: f32| theme.surface.overlay(c, a);
        Self::new(
            theme.on_primary,                  // checked_thumb
            theme.primary,                     // checked_track
            transparent,                       // checked_border
            theme.on_primary_container,        // checked_icon
            theme.outline,                     // unchecked_thumb
            theme.surface_container_highest,   // unchecked_track
            theme.outline,                     // unchecked_border
            theme.surface_container_highest,   // unchecked_icon
            theme.surface,                     // disabled_checked_thumb（opacity 1.0）
            over(theme.on_surface, 0.12),      // disabled_checked_track
            transparent,                       // disabled_checked_border
            over(theme.on_surface, 0.38),      // disabled_checked_icon
            over(theme.on_surface, 0.38),      // disabled_unchecked_thumb
            over(theme.surface_container_highest, 0.12), // disabled_unchecked_track
            over(theme.on_surface, 0.12),      // disabled_unchecked_border
            over(theme.surface_container_highest, 0.38), // disabled_unchecked_icon
        )
    }

    pub fn thumb_color(&self, enabled: bool, checked: bool) -> Color {
        if !enabled {
            if checked { self.disabled_checked_thumb } else { self.disabled_unchecked_thumb }
        } else if checked {
            self.checked_thumb
        } else {
            self.unchecked_thumb
        }
    }

    pub fn track_color(&self, enabled: bool, checked: bool) -> Color {
        if !enabled {
            if checked { self.disabled_checked_track } else { self.disabled_unchecked_track }
        } else if checked {
            self.checked_track
        } else {
            self.unchecked_track
        }
    }

    pub fn border_color(&self, enabled: bool, checked: bool) -> Color {
        if !enabled {
            if checked { self.disabled_checked_border } else { self.disabled_unchecked_border }
        } else if checked {
            self.checked_border
        } else {
            self.unchecked_border
        }
    }

    pub fn icon_color(&self, enabled: bool, checked: bool) -> Color {
        if !enabled {
            if checked { self.disabled_checked_icon } else { self.disabled_unchecked_icon }
        } else if checked {
            self.checked_icon
        } else {
            self.unchecked_icon
        }
    }
}

/// Switch 默认值（对标 material3 `SwitchDefaults`）
pub struct SwitchDefaults;

impl SwitchDefaults {
    pub fn switch_colors(theme: &ThemeColors) -> SwitchColors {
        SwitchColors::from_theme(theme)
    }

    pub fn shape() -> Shape {
        Shape::pill()
    }

    pub fn track_outline_width() -> f32 {
        SWITCH_TRACK_OUTLINE_WIDTH
    }

    pub fn icon_size() -> f32 {
        SWITCH_ICON_SIZE
    }
}

/// Switch 组件 Builder（对标 material3 `Switch(checked, onCheckedChange,
/// modifier, thumbContent, enabled, colors, interactionSource)`——
/// thumbContent 对应 build 的 content 闭包，空闭包 = 无拇指内容）
pub struct Switch {
    checked: bool,
    on_checked_change: Option<Arc<dyn Fn(bool) + Send + Sync>>,
    enabled: bool,
    colors: Option<SwitchColors>,
    interaction_source: Option<MutableInteractionSource>,
    modifier: Modifier,
}

impl Switch {
    pub fn new(checked: bool) -> Self {
        Self {
            checked,
            on_checked_change: None,
            enabled: true,
            colors: None,
            interaction_source: None,
            modifier: Modifier::new(),
        }
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    pub fn on_checked_change(mut self, f: impl Fn(bool) + Send + Sync + 'static) -> Self {
        self.on_checked_change = Some(Arc::new(f));
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn colors(mut self, colors: SwitchColors) -> Self {
        self.colors = Some(colors);
        self
    }

    /// 注入交互源（hoist——press/hover/focus 状态发射到此源）
    pub fn interaction_source(mut self, source: MutableInteractionSource) -> Self {
        self.interaction_source = Some(source);
        self
    }

    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = self.modifier.then(modifier);
        self
    }

    #[composable]
    pub fn build(self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        ctx.changed(&self.checked);
        ctx.changed(&self.enabled);
        ctx.changed(&self.colors);
        let key = ctx.next_key();
        let theme = WiniaTheme::colors();
        let colors = self
            .colors
            .unwrap_or_else(|| SwitchDefaults::switch_colors(&theme));
        let interaction = self
            .interaction_source
            .unwrap_or_else(|| ctx.remember(|| MutableInteractionSource::new()).get());
        let state = interaction.state(self.enabled);
        let checked = self.checked;
        let track_color = colors.track_color(self.enabled, checked);
        let border_color = colors.border_color(self.enabled, checked);
        let thumb_color = colors.thumb_color(self.enabled, checked);
        let icon_color = colors.icon_color(self.enabled, checked);

        // A winia addition, recorded rather than presented as a port: M3's `Switch` applies its
        // track/border/thumb colours directly (`material3/Switch.kt:145-152`, the file contains no
        // `animateColorAsState`), so this 180 ms EaseOutCubic fade is ours. The engine can animate a
        // colour on a spring now, so M3-style motion would be `SpringSpec::effects_default()` here.
        let color_spec = crate::animation::AnimationSpec::Tween(crate::animation::TweenSpec::new(
            std::time::Duration::from_millis(180),
            crate::animation::interpolator::EaseOutCubic::new(),
        ));
        let track_color_anim = ctx.animate_color_as_state(track_color, color_spec.clone());
        let border_color_anim = ctx.animate_color_as_state(border_color, color_spec.clone());
        let thumb_color_anim = ctx.animate_color_as_state(thumb_color, color_spec);

        // Handle 容器位置：关闭 (2,2)、开启 (22,2)
        let container_offset = ctx.animate_float_as_state(
            if checked {
                SWITCH_DRAG_MAX_OFFSET
            } else {
                SWITCH_DRAG_MIN_OFFSET
            },
            crate::animation::AnimationSpec::Spring(switch_thumb_spec()),
        );
        // 拖拽状态：drag 期间 container_offset 被覆盖；释放后从当前位置动画回目标
        let drag_active = ctx.remember(|| false);
        let drag_offset = ctx.remember(|| 0.0f32);
        let drag_base = ctx.remember(|| 0.0f32);
        let drag_start_x = ctx.remember(|| 0.0f32);

        // 组合期读取 drag_active（注册组合依赖）——拖拽开始/结束触发重组，
        // 重新计算圆尺寸目标：drag 期间保持 28，不依赖 measure 期覆盖
        let dragging = drag_active.get();

        // 中心圆尺寸：按下/拖拽 28×28、开启 24×24、关闭 16×16
        let circle_size = ctx.animate_float_as_state(
            if state.pressed || dragging {
                SWITCH_THUMB_CONTAINER_SIZE
            } else if checked {
                SWITCH_THUMB_CHECKED_SIZE
            } else {
                SWITCH_THUMB_UNCHECKED_SIZE
            },
            crate::animation::AnimationSpec::Spring(switch_thumb_spec()),
        );

        // 轨道：toggleable + 拖拽；波纹在 Handle 容器上
        let track_shape = SwitchDefaults::shape();
        let mut modifier = Modifier::new()
            .size(SWITCH_TRACK_WIDTH, SWITCH_TRACK_HEIGHT)
            .background(move || track_color_anim.peek(), track_shape)
            .border(
                SWITCH_TRACK_OUTLINE_WIDTH,
                move || border_color_anim.peek(),
                track_shape,
            );
        if self.enabled {
            if let Some(on_checked_change) = &self.on_checked_change {
                let cb = on_checked_change.clone();
                // 拖拽回调：位置是轨道本地坐标，增量不用于偏移（防双倍位移）
                let d_active = drag_active.clone();
                let d_offset = drag_offset.clone();
                let d_base = drag_base.clone();
                let d_start_x = drag_start_x.clone();
                let anim_offset = container_offset.clone();
                let on_drag_start = move |pos: (f32, f32)| {
                    let base = anim_offset.peek();
                    d_base.set(base);
                    d_offset.set(base);
                    d_start_x.set(pos.0);
                    d_active.set(true);
                };
                let d_active2 = drag_active.clone();
                let d_offset2 = drag_offset.clone();
                let d_base2 = drag_base.clone();
                let d_start_x2 = drag_start_x.clone();
                let on_drag = move |pos: (f32, f32), _delta: (f32, f32)| {
                    if !d_active2.get() {
                        return;
                    }
                    let offset = (d_base2.get() + (pos.0 - d_start_x2.get()))
                        .clamp(SWITCH_DRAG_MIN_OFFSET, SWITCH_DRAG_MAX_OFFSET);
                    d_offset2.set(offset);
                };
                let d_active3 = drag_active.clone();
                let d_offset3 = drag_offset.clone();
                let anim_offset3 = container_offset.clone();
                let checked_end = checked;
                let cb_end = on_checked_change.clone();
                let on_drag_end = move || {
                    if !d_active3.get() {
                        return;
                    }
                    let release = d_offset3.get();
                    // 从释放位置开始动画归位（Animating 写：重组但不唤醒，
                    // 避免先弹回旧目标；帧由 request_redraw 驱动）
                    anim_offset3.as_raw().set_animating(release);
                    d_active3.set(false);
                    let target = release > SWITCH_DRAG_THRESHOLD;
                    if target != checked_end {
                        cb_end(target);
                    }
                };
                let d_active4 = drag_active.clone();
                let d_offset4 = drag_offset.clone();
                let anim_offset4 = container_offset.clone();
                let on_drag_cancel = move || {
                    if !d_active4.get() {
                        return;
                    }
                    anim_offset4.as_raw().set_animating(d_offset4.get());
                    d_active4.set(false);
                };
                modifier = modifier
                    .clickable_with_source(&interaction, move || cb(!checked))
                    .on_drag_start(on_drag_start)
                    .on_drag(on_drag)
                    .on_drag_end(on_drag_end)
                    .on_drag_cancel(on_drag_cancel);
            }
        }
        modifier = modifier.semantics(
            crate::semantics::SemanticsConfig::new()
                .role(crate::semantics::SemanticsRole::Switch)
                .merge_descendants(true)
                .state(crate::semantics::SemanticsState::new()
                    .checked_bool(self.checked)
                    .enabled(self.enabled)),
        );
        modifier = modifier.then(self.modifier);

        match ctx.start_restartable_group(
            key,
            modifier,
            // 容器由 Start + 动态 offset 定位：x 动画、y 固定 2
            BoxLayout::new().alignment(crate::layout::Alignment::Start),
        ) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                // Handle 容器：28×28，带 unbounded ripple；位置 (2,2)/(22,2)
                let da_x = drag_active.clone();
                let do_x = drag_offset.clone();
                let o1 = container_offset.clone();
                let offset_eff = move || {
                    if da_x.get() {
                        do_x.get()
                    } else {
                        o1.get()
                    }
                };
                let container = Modifier::new()
                    .size(SWITCH_THUMB_CONTAINER_SIZE, SWITCH_THUMB_CONTAINER_SIZE)
                    .offset(offset_eff, SWITCH_DRAG_MIN_OFFSET);
                let container = if self.enabled {
                    container.ripple(&interaction, theme.on_surface, false)
                } else {
                    container
                };
                let ckey = ctx.next_key();
                match ctx.start_restartable_group(
                    ckey,
                    container,
                    BoxLayout::new().alignment(crate::layout::Alignment::Center),
                ) {
                    GroupStatus::Skip => {}
                    GroupStatus::Enter => {
                        // 中心圆：16/28 动态尺寸 + 颜色过渡。
                        // 双保险：组合期目标已含 drag_active；measure 期
                        // 再覆盖一次（drag 中强制 28，防目标更新前先缩回）
                        let da_size = drag_active.clone();
                        let cs1 = circle_size.clone();
                        let cs2 = circle_size.clone();
                        let size_eff = move || {
                            if da_size.get() {
                                SWITCH_THUMB_CONTAINER_SIZE
                            } else {
                                cs1.get()
                            }
                        };
                        let circle = Modifier::new()
                            .size(size_eff, move || cs2.get())
                            .background(move || thumb_color_anim.peek(), Shape::Circle);
                        let rkey = ctx.next_key();
                        match ctx.start_restartable_group(
                            rkey,
                            circle,
                            BoxLayout::new().alignment(crate::layout::Alignment::Center),
                        ) {
                            GroupStatus::Skip => {}
                            GroupStatus::Enter => {
                                // thumbContent：图标色下传（Icon tint Auto 跟随）
                                WiniaTheme::with_content_color(icon_color, ctx, |ctx| {
                                    content(ctx);
                                });
                            }
                        }
                        ctx.end_restartable_group();
                    }
                }
                ctx.end_restartable_group();
            }
        }
        // M3 Switch 焦点指示色 = Secondary（FocusIndicatorColor token）
        ctx.set_current_node_focus_color(theme.secondary);
        ctx.end_restartable_group();
    }

    pub fn get_checked(&self) -> bool {
        self.checked
    }
    pub fn get_enabled(&self) -> bool {
        self.enabled
    }
    pub fn get_colors(&self) -> Option<SwitchColors> {
        self.colors
    }
}


#[cfg(test)]
mod tests;
