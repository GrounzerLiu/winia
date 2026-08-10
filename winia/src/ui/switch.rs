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

use crate::core::composer::{ComposeCtx, GroupStatus};
use crate::layout::BoxLayout;
use crate::modifier::{Color, Modifier, Shape};
use crate::ui::interaction::MutableInteractionSource;
use crate::ui::theme::{ThemeColors, WiniaTheme};
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

        // 背景/边框/中心圆颜色过渡（180ms EaseOutCubic）
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
            crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec::default()),
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
            crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec::default()),
        );

        // 轨道：toggleable + 拖拽；波纹在 Handle 容器上
        let track_shape = SwitchDefaults::shape();
        let mut modifier = Modifier::new()
            .size(SWITCH_TRACK_WIDTH, SWITCH_TRACK_HEIGHT)
            .background(move || track_color_anim.peek(), track_shape)
            .border_dynamic(
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
                    // 从释放位置开始动画归位（静默写入，避免先弹回旧目标）
                    anim_offset3.set_silent(release);
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
                    anim_offset4.set_silent(d_offset4.get());
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
mod tests {
    use super::*;

    #[test]
    fn color_state_resolution() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let colors = SwitchDefaults::switch_colors(&theme);
        // checked：OnPrimary 拇指 / Primary 轨道 / 透明边框
        assert_eq!(colors.thumb_color(true, true), theme.on_primary);
        assert_eq!(colors.track_color(true, true), theme.primary);
        assert_eq!(colors.border_color(true, true).a, 0);
        assert_eq!(colors.icon_color(true, true), theme.on_primary_container);
        // unchecked：Outline 拇指 / SurfaceContainerHighest 轨道 / Outline 边框
        assert_eq!(colors.thumb_color(true, false), theme.outline);
        assert_eq!(colors.track_color(true, false), theme.surface_container_highest);
        assert_eq!(colors.border_color(true, false), theme.outline);
        assert_eq!(colors.icon_color(true, false), theme.surface_container_highest);
        // disabled 优先：token alpha compositeOver(surface)
        let over = |c: Color, a: f32| theme.surface.overlay(c, a);
        assert_eq!(colors.track_color(false, true), over(theme.on_surface, 0.12));
        assert_eq!(colors.thumb_color(false, true), theme.surface);
        assert_eq!(colors.icon_color(false, true), over(theme.on_surface, 0.38));
        assert_eq!(colors.thumb_color(false, false), over(theme.on_surface, 0.38));
        assert_eq!(
            colors.track_color(false, false),
            over(theme.surface_container_highest, 0.12)
        );
        assert_eq!(colors.border_color(false, false), over(theme.on_surface, 0.12));
    }

    #[test]
    fn callback_receives_inverted_checked() {
        use crate::modifier::ModifierElement;
        use std::sync::atomic::{AtomicBool, Ordering};
        for (checked, expect) in [(false, true), (true, false)] {
            let received = Arc::new(AtomicBool::new(false));
            let cb = received.clone();
            let mut composer = crate::core::composer::Composer::new();
            composer.compose(|ctx| {
                Switch::new(checked)
                    .on_checked_change(move |v| cb.store(v, Ordering::Relaxed))
                    .build(ctx, |_| {});
            });
            composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
            let mut clicked = false;
            for node in composer.arena_nodes() {
                for el in node.modifier.elements() {
                    if let ModifierElement::Clickable { on_click, .. } = el {
                        on_click();
                        clicked = true;
                    }
                }
            }
            assert!(clicked, "存在 Clickable");
            assert_eq!(received.load(Ordering::Relaxed), expect, "回调收到 !checked");
        }
    }

    #[test]
    fn disabled_switch_has_no_interaction_elements() {
        use crate::modifier::ModifierElement;
        let mut composer = crate::core::composer::Composer::new();
        composer.compose(|ctx| {
            Switch::new(false)
                .enabled(false)
                .on_checked_change(|_| {})
                .build(ctx, |_| {});
        });
        composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        for node in composer.arena_nodes() {
            for el in node.modifier.elements() {
                assert!(
                    !matches!(el, ModifierElement::Ripple { .. })
                        && !matches!(el, ModifierElement::Focusable { .. })
                        && !matches!(el, ModifierElement::Clickable { .. }),
                    "禁用 Switch 不应有交互元素: {el:?}"
                );
            }
        }
    }

    #[test]
    fn ripple_on_handle_container_unbounded() {
        // 波纹在 Handle 容器（28×28）上、unbounded；与 clickable（轨道）不同节点
        use crate::modifier::ModifierElement;
        let mut composer = crate::core::composer::Composer::new();
        composer.compose(|ctx| {
            Switch::new(false)
                .on_checked_change(|_| {})
                .build(ctx, |_| {});
        });
        composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        let mut clickable_idx = None;
        let mut ripple_idx = None;
        for (i, node) in composer.arena_nodes().iter().enumerate() {
            for el in node.modifier.elements() {
                if let ModifierElement::Ripple { bounded, .. } = el {
                    assert!(!bounded, "Switch 波纹 unbounded");
                    ripple_idx = Some(i);
                }
                if matches!(el, ModifierElement::Clickable { .. }) {
                    clickable_idx = Some(i);
                }
            }
        }
        assert!(ripple_idx.is_some(), "Handle 容器应带波纹");
        assert!(clickable_idx.is_some(), "轨道应可点击");
        assert_ne!(
            ripple_idx, clickable_idx,
            "波纹应在 Handle 容器（与 clickable 不同节点）"
        );
        assert_eq!(SWITCH_THUMB_CONTAINER_SIZE, 28.0, "容器固定 28");
        assert_eq!(SWITCH_THUMB_UNCHECKED_SIZE, 16.0);
        assert_eq!(SWITCH_THUMB_CHECKED_SIZE, 24.0, "开启圆 24");
    }

    #[test]
    fn drag_gesture_attached_when_interactable() {
        use crate::modifier::ModifierElement;
        let mut composer = crate::core::composer::Composer::new();
        composer.compose(|ctx| {
            Switch::new(false)
                .on_checked_change(|_| {})
                .build(ctx, |_| {});
        });
        composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        let mut start = false;
        let mut mv = false;
        let mut end = false;
        let mut cancel = false;
        for node in composer.arena_nodes() {
            for el in node.modifier.elements() {
                match el {
                    ModifierElement::DragOnStart { .. } => start = true,
                    ModifierElement::DragOnMove { .. } => mv = true,
                    ModifierElement::DragOnEnd { .. } => end = true,
                    ModifierElement::DragOnCancel { .. } => cancel = true,
                    _ => {}
                }
            }
        }
        assert!(start && mv && end && cancel, "可交互 Switch 应挂拖拽手势");
        assert_eq!(SWITCH_DRAG_MIN_OFFSET, 2.0);
        assert_eq!(SWITCH_DRAG_MAX_OFFSET, 22.0);
        assert_eq!(SWITCH_DRAG_THRESHOLD, 12.0);
    }

    #[test]
    fn drag_keeps_circle_at_pressed_size() {
        // 关闭状态下按下 → 圆 28；拖动期间 drag_active 覆盖 → 仍 28
        use crate::modifier::ModifierElement;
        use std::sync::Arc;
        let mut composer = crate::core::composer::Composer::new();
        composer.compose(|ctx| {
            Switch::new(false)
                .on_checked_change(|_| {})
                .build(ctx, |_| {});
        });
        composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        let circle_size = |c: &crate::core::composer::Composer| -> f32 {
            c.arena_nodes()
                .iter()
                .find_map(|n| {
                    let circle_bg = n.modifier.elements().iter().any(|el| {
                        matches!(el, ModifierElement::Background { shape: Shape::Circle, .. })
                    });
                    circle_bg.then_some(n.measured_size.width)
                })
                .unwrap_or(-1.0)
        };
        assert_eq!(circle_size(&composer), 16.0, "初始关闭圆 16");
        let mut start: Option<Arc<dyn Fn((f32, f32)) + Send + Sync>> = None;
        let mut mv: Option<Arc<dyn Fn((f32, f32), (f32, f32)) + Send + Sync>> = None;
        for node in composer.arena_nodes() {
            for el in node.modifier.elements() {
                match el {
                    ModifierElement::DragOnStart { cb } => start = Some(cb.clone()),
                    ModifierElement::DragOnMove { cb } => mv = Some(cb.clone()),
                    _ => {}
                }
            }
        }
        let start = start.expect("drag start 回调");
        let mv = mv.expect("drag move 回调");
        start((30.0, 16.0));
        mv((45.0, 16.0), (15.0, 0.0));
        // 模拟一帧：compose 消费 pending（标记布局失效）→ layout 重测
        composer.compose(|ctx| {
            Switch::new(false)
                .on_checked_change(|_| {})
                .build(ctx, |_| {});
        });
        composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        assert_eq!(
            circle_size(&composer),
            28.0,
            "拖动期间中心圆应保持 28（drag_active 覆盖）"
        );
    }

    #[test]
    fn rapid_toggle_text_matches_final_state() {
        // 快速连续更新（多次 set 后才 compose）：文本内容必须等于最终状态
        use crate::modifier::ModifierElement;
        let mut composer = crate::core::composer::Composer::new();
        let checked = crate::core::state::State::new(true);
        let scene = |ctx: &mut ComposeCtx| {
            let c2 = checked.clone();
            Switch::new(checked.get())
                .on_checked_change(move |v| c2.update(|s| *s = v))
                .build(ctx, |_| {});
            crate::ui::Text::new(if checked.get() { "已开启" } else { "已关闭" }).build(ctx);
        };
        composer.compose(scene);
        // 快速三次翻转（中间不 compose）：true → false → true → false
        checked.update(|s| *s = false);
        checked.update(|s| *s = true);
        checked.update(|s| *s = false);
        composer.compose(scene);
        composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        // 找 TextContent：应只有“已关闭”
        let mut texts = Vec::new();
        for node in composer.arena_nodes() {
            for el in node.modifier.elements() {
                if let ModifierElement::TextContent { content, .. } = el {
                    texts.push(content.clone());
                }
            }
        }
        assert!(!texts.is_empty());
        assert!(
            texts.iter().all(|t| t == "已关闭"),
            "快速切换后文字应为最终状态（已关闭），实际 {texts:?}"
        );
    }

    #[test]
    fn skip_text_content_change_force_remeasure() {
        // 回归：依赖注册在父容器时，Text 槽 Clean/Skip 但内容已变——
        // 必须强制重测（否则 cached_paragraph 旧内容 → 渲染画旧文本，
        // 即 demo“当前 已开启/已关闭”不同步问题）
        use crate::modifier::ModifierElement;
        let mut composer = crate::core::composer::Composer::new();
        // 用 ctx.remember 创建（带 owner 队列）——外部 State 的 set 不通知
        // composer（notify pushed=false），测不到真实重组路径
        let holder = std::cell::RefCell::new(None::<crate::core::state::State<bool>>);
        let scene = |ctx: &mut ComposeCtx| {
            let c = ctx.remember(|| true);
            holder.replace(Some(c.clone()));
            crate::ui::Column::new().build(ctx, |ctx| {
                crate::ui::Text::new(if c.get() { "aaaaaaaaaa" } else { "bb" }).build(ctx);
            });
        };
        let text_width = |composer: &crate::core::composer::Composer| -> f32 {
            composer
                .arena_nodes()
                .iter()
                .find_map(|n| {
                    let has_text = n.modifier.elements().iter().any(|el| {
                        matches!(el, ModifierElement::TextContent { .. })
                    });
                    has_text.then_some(n.measured_size.width)
                })
                .unwrap_or(-1.0)
        };
        composer.compose(scene);
        composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        let w1 = text_width(&composer);
        let c = holder.borrow().clone().expect("remember 状态");
        c.update(|s| *s = false);
        composer.compose(scene);
        composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        let w2 = text_width(&composer);
        assert!(w1 > 20.0, "长文本宽度应较大，实际 {w1}");
        assert!(
            (w2 - w1).abs() > 5.0,
            "内容从 10 个 a 变为 bb 后必须重测（w1={w1} w2={w2}）——Skip 折叠测量 bug"
        );
    }

    #[test]
    fn rendered_text_pixels_change_with_state() {
        // 进程内复刻 demo 的“共享 State + Column 内 Switch/Text”：
        // 渲染“已关闭”与“已开启”两次，文字区域像素必须显著不同。
        // 若此测试失败 = cached_paragraph 未重建（渲染仍画旧文本）。
        use crate::modifier::ModifierElement;
        use skia_safe::{Color, surfaces};
        use std::time::Duration;
        let _g = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = crate::core::composer::Composer::new();
        let holder = std::cell::RefCell::new(None::<crate::core::state::State<bool>>);
        let build_scene = |ctx: &mut ComposeCtx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                let c = ctx.remember(|| false);
                holder.replace(Some(c.clone()));
                crate::ui::Column::new().build(ctx, |ctx| {
                    let c2 = c.clone();
                    Switch::new(c.get())
                        .on_checked_change(move |v| c2.update(|s| *s = v))
                        .build(ctx, |_| {});
                    crate::ui::Text::new(if c.get() { "已开启" } else { "已关闭" })
                        .build(ctx);
                });
            });
        };
        let mut render_text_region = |composer: &mut crate::core::composer::Composer| -> (Vec<u8>, f32, f32, f32, f32) {
            composer.compose(build_scene);
            // 模拟真实 recompose_layout_render 的循环：动画注册会产生 pending state，
            // 同帧会再 compose 一次（第二次物化可能覆盖第一次设置的 dirty）
            composer.compose(build_scene);
            composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 100.0));
            let mut surface = surfaces::raster_n32_premul((300, 100)).unwrap();
            let canvas = surface.canvas();
            canvas.clear(Color::WHITE);
            let root = composer.layout_root_idx().expect("root");
            let nodes = composer.arena_nodes();
            crate::render::render(nodes, root, canvas);
            let pm = surface.peek_pixels().expect("pixmap");
            let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
            let mut text_pos = None;
            let mut text_size = (0.0f32, 0.0f32);
            for node in nodes {
                if node.modifier.elements().iter().any(|el| {
                    matches!(el, ModifierElement::TextContent { .. })
                }) {
                    text_pos = Some((node.position.x, node.position.y));
                    text_size = (node.measured_size.width, node.measured_size.height);
                    break;
                }
            }
            let (tx, ty) = text_pos.expect("text node");
            let mut region = Vec::new();
            let x0 = tx.max(0.0) as usize;
            let y0 = ty.max(0.0) as usize;
            let x1 = ((tx + text_size.0) as usize).min(300);
            let y1 = ((ty + text_size.1) as usize).min(100);
            for y in y0..y1 {
                for x in x0..x1 {
                    let p = px[y * 300 + x];
                    region.extend_from_slice(&[p[0], p[1], p[2]]);
                }
            }
            (region, tx, ty, text_size.0, text_size.1)
        };
        composer.compose(build_scene);
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 100.0));
        let c = holder.borrow().clone().expect("state");
        let (region_false, tx, ty, tw, th) = render_text_region(&mut composer);
        let dark_false = region_false.iter().filter(|&&v| v < 200).count();
        c.update(|s| *s = true);
        let (region_true, tx2, ty2, tw2, th2) = render_text_region(&mut composer);
        let dark_true = region_true.iter().filter(|&&v| v < 200).count();
        // 动画推完（确保后续无状态再覆盖文字）
        for _ in 0..400 {
            if !crate::animation::update_animations() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let (region_settled, _, _, _, _) = render_text_region(&mut composer);
        let dark_true_settled = region_settled.iter().filter(|&&v| v < 200).count();
        let diff_count = region_false
            .iter()
            .zip(region_true.iter())
            .filter(|(a, b)| (**a as i32 - **b as i32).abs() > 30)
            .count();
        let diff_settled = region_false
            .iter()
            .zip(region_settled.iter())
            .filter(|(a, b)| (**a as i32 - **b as i32).abs() > 30)
            .count();
        let _ = (tx, ty, tw, th, tx2, ty2, tw2, th2);
        assert!(
            dark_false > 5 && dark_true > 5,
            "两种状态都应有文字暗像素（false={dark_false} true={dark_true}）"
        );
        assert!(
            diff_count > 20 || diff_settled > 20,
            "文字区域像素应随状态变化（false={dark_false} true={dark_true} settled={dark_true_settled} diff={diff_count}/{diff_settled}）"
        );
    }

    #[test]
    fn rapid_toggle_settles_at_final_state() {
        // 端到端（真实帧循环）：快速点击 → 每帧 compose+动画步进+layout
        // → 动画静止后容器位置/圆尺寸必须等于最终状态
        use crate::modifier::ModifierElement;
        use std::time::Duration;
        let _g = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let mut composer = crate::core::composer::Composer::new();
        let c = crate::core::state::State::new(true);
        let scene = |ctx: &mut ComposeCtx| {
            let c2 = c.clone();
            Switch::new(c.get())
                .on_checked_change(move |v| c2.update(|s| *s = v))
                .build(ctx, |_| {});
        };
        let frame = |composer: &mut crate::core::composer::Composer| {
            composer.compose(scene);
            composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
            for _ in 0..20 {
                crate::animation::update_animations();
                std::thread::sleep(Duration::from_millis(5));
            }
        };
        frame(&mut composer);
        // 快速点击 3 次（真实 on_click 回调 → c 翻转；每帧推进动画）
        for i in 0..3 {
            let mut clicked = false;
            for node in composer.arena_nodes() {
                for el in node.modifier.elements() {
                    if let ModifierElement::Clickable { on_click, .. } = el {
                        on_click();
                        clicked = true;
                    }
                }
            }
            assert!(clicked, "点击回调存在");
            frame(&mut composer);
        }
        // 推进动画直到静止（最多 300 帧）
        for _ in 0..300 {
            if !crate::animation::update_animations() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        composer.compose(scene);
        composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        // 最终状态 false：容器 x=2、圆 16
        assert!(!c.get(), "3 次点击后状态应为 false");
        let mut container_x = f32::NAN;
        let mut circle_size = f32::NAN;
        for node in composer.arena_nodes() {
            let has_ripple = node
                .modifier
                .elements()
                .iter()
                .any(|el| matches!(el, ModifierElement::Ripple { .. }));
            if has_ripple {
                container_x = node.position.x;
            }
            let has_circle_bg = node
                .modifier
                .elements()
                .iter()
                .any(|el| matches!(el, ModifierElement::Background { shape: Shape::Circle, .. }));
            if has_circle_bg {
                circle_size = node.measured_size.width;
            }
        }
        assert!(
            (container_x - 2.0).abs() < 0.5,
            "动画静止后容器应在关闭位置 x=2，实际 {container_x}"
        );
        assert!(
            (circle_size - 16.0).abs() < 0.5,
            "动画静止后圆应为 16，实际 {circle_size}"
        );
    }

    #[test]
    fn rapid_triple_click_text_and_visual_sync() {
        // 快速三连击（真实 notify + 每击 1-2 帧 + 中间一击带抖动触发拖拽）：
        // 动画静止后 文字 / 容器位置 / 圆尺寸 必须全部等于最终状态
        use crate::modifier::ModifierElement;
        use std::sync::Arc;
        use std::time::Duration;
        let _g = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let mut composer = crate::core::composer::Composer::new();
        let holder = std::cell::RefCell::new(None::<crate::core::state::State<bool>>);
        let scene = |ctx: &mut ComposeCtx| {
            let c = ctx.remember(|| false);
            holder.replace(Some(c.clone()));
            crate::ui::Column::new().build(ctx, |ctx| {
                let c2 = c.clone();
                Switch::new(c.get())
                    .on_checked_change(move |v| c2.update(|s| *s = v))
                    .build(ctx, |_| {});
                crate::ui::Text::new(if c.get() { "已开启" } else { "已关闭" }).build(ctx);
            });
        };
        let frame = |composer: &mut crate::core::composer::Composer, steps: usize| {
            composer.compose(scene);
            composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
            for _ in 0..steps {
                crate::animation::update_animations();
                std::thread::sleep(Duration::from_millis(5));
            }
        };
        let click = |composer: &mut crate::core::composer::Composer| {
            // 与运行时 up 顺序一致：先 detect_click（on_click），后 gesture_up（drag_end）
            let mut on_click: Option<Arc<dyn Fn() + Send + Sync>> = None;
            let mut drag_start: Option<Arc<dyn Fn((f32, f32)) + Send + Sync>> = None;
            let mut drag_move: Option<Arc<dyn Fn((f32, f32), (f32, f32)) + Send + Sync>> = None;
            let mut drag_end: Option<Arc<dyn Fn() + Send + Sync>> = None;
            for node in composer.arena_nodes() {
                for el in node.modifier.elements() {
                    match el {
                        ModifierElement::Clickable { on_click: cb, .. } => on_click = Some(cb.clone()),
                        ModifierElement::DragOnStart { cb } => drag_start = Some(cb.clone()),
                        ModifierElement::DragOnMove { cb } => drag_move = Some(cb.clone()),
                        ModifierElement::DragOnEnd { cb } => drag_end = Some(cb.clone()),
                        _ => {}
                    }
                }
            }
            let on_click = on_click.expect("click");
            let drag_start = drag_start.expect("drag start");
            let drag_move = drag_move.expect("drag move");
            let drag_end = drag_end.expect("drag end");
            // 模拟按下后带 10px 抖动再抬起（触发 drag 但仍在 CLICK_SLOP 内）
            drag_start((30.0, 16.0));
            drag_move((40.0, 16.0), (10.0, 0.0));
            on_click();
            drag_end();
        };
        frame(&mut composer, 2);
        // 三连击，每击之间只跑 1 帧
        for _ in 0..3 {
            click(&mut composer);
            frame(&mut composer, 1);
        }
        // 动画推完
        for _ in 0..400 {
            if !crate::animation::update_animations() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        composer.compose(scene);
        composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        let c = holder.borrow().clone().unwrap();
        let expect_checked = c.get();
        // 文字
        let mut text = String::new();
        // 容器/圆
        let mut container_x = f32::NAN;
        let mut circle_size = f32::NAN;
        for node in composer.arena_nodes() {
            for el in node.modifier.elements() {
                match el {
                    ModifierElement::TextContent { content, .. } => text = content.clone(),
                    ModifierElement::Ripple { .. } => container_x = node.position.x,
                    ModifierElement::Background { shape: Shape::Circle, .. } => {
                        circle_size = node.measured_size.width;
                    }
                    _ => {}
                }
            }
        }
        let expect_text = if expect_checked { "已开启" } else { "已关闭" };
        let expect_x = if expect_checked { 22.0 } else { 2.0 };
        let expect_circle = if expect_checked { 24.0 } else { 16.0 };
        assert_eq!(text, expect_text, "文字应等于最终状态");
        assert!(
            (container_x - expect_x).abs() < 0.5,
            "容器应停在 {expect_x}，实际 {container_x}"
        );
        assert!(
            (circle_size - expect_circle).abs() < 0.5,
            "圆应为 {expect_circle}，实际 {circle_size}"
        );
    }

    #[test]
    fn rapid_triple_click_rendered_pixels_match_state() {
        // 渲染级验证：快速三连击 + 动画静止后，位图上的轨道色/圆色/文字
        // 必须与最终状态一致（覆盖动画重定向冲突的视觉残留）
        use crate::modifier::ModifierElement;
        use std::sync::Arc;
        use std::time::Duration;
        let _g = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = crate::core::composer::Composer::new();
        let holder = std::cell::RefCell::new(None::<crate::core::state::State<bool>>);
        let src_holder = std::cell::RefCell::new(None::<crate::ui::interaction::MutableInteractionSource>);
        let scene = |ctx: &mut ComposeCtx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                let c = ctx.remember(|| false);
                holder.replace(Some(c.clone()));
                crate::ui::Column::new().build(ctx, |ctx| {
                    let c2 = c.clone();
                    let src = ctx.remember(|| MutableInteractionSource::new()).get();
                    src_holder.replace(Some(src.clone()));
                    Switch::new(c.get())
                        .interaction_source(src)
                        .on_checked_change(move |v| c2.update(|s| *s = v))
                        .build(ctx, |_| {});
                    crate::ui::Text::new(if c.get() { "已开启" } else { "已关闭" }).build(ctx);
                });
            });
        };
        let frame = |composer: &mut crate::core::composer::Composer| {
            // 与真实帧循环一致：先推进动画，再 compose，再 layout
            crate::animation::update_animations();
            composer.compose(scene);
            composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 200.0));
        };
        let click = |composer: &mut crate::core::composer::Composer| {
            let src = src_holder.borrow().clone().unwrap();
            src.emit_press();
            let mut on_click: Option<Arc<dyn Fn() + Send + Sync>> = None;
            let mut drag_start: Option<Arc<dyn Fn((f32, f32)) + Send + Sync>> = None;
            let mut drag_move: Option<Arc<dyn Fn((f32, f32), (f32, f32)) + Send + Sync>> = None;
            let mut drag_end: Option<Arc<dyn Fn() + Send + Sync>> = None;
            for node in composer.arena_nodes() {
                for el in node.modifier.elements() {
                    match el {
                        ModifierElement::Clickable { on_click: cb, .. } => on_click = Some(cb.clone()),
                        ModifierElement::DragOnStart { cb } => drag_start = Some(cb.clone()),
                        ModifierElement::DragOnMove { cb } => drag_move = Some(cb.clone()),
                        ModifierElement::DragOnEnd { cb } => drag_end = Some(cb.clone()),
                        _ => {}
                    }
                }
            }
            drag_start.expect("drag start")((30.0, 16.0));
            drag_move.expect("drag move")((40.0, 16.0), (10.0, 0.0));
            on_click.expect("click")();
            drag_end.expect("drag end")();
            src.emit_release();
        };
        frame(&mut composer);
        for _ in 0..3 {
            click(&mut composer);
            frame(&mut composer);
            std::thread::sleep(Duration::from_millis(10));
        }
        for _ in 0..400 {
            if !crate::animation::update_animations() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        composer.compose(scene);
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 200.0));
        let c = holder.borrow().clone().unwrap();
        let checked = c.get();
        // 渲染
        use skia_safe::{Color, surfaces};
        let mut surface = surfaces::raster_n32_premul((300, 200)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(Color::WHITE);
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        crate::render::render(nodes, root, canvas);
        let pm = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        let at = |x: usize, y: usize| -> [u8; 4] { px[y * 300 + x] };
        // 节点绝对位置（沿父链累加）
        let abs_pos = |target_id: u64| -> (f32, f32) {
            fn walk(nodes: &[crate::layout::node::LayoutNode], idx: usize, target: u64, ax: f32, ay: f32) -> Option<(f32, f32)> {
                let nx = ax + nodes[idx].position.x;
                let ny = ay + nodes[idx].position.y;
                if nodes[idx].id == target {
                    return Some((nx, ny));
                }
                for &ch in &nodes[idx].children {
                    if let Some(r) = walk(nodes, ch, target, nx, ny) {
                        return Some(r);
                    }
                }
                None
            }
            walk(nodes, root, target_id, 0.0, 0.0).unwrap()
        };
        let mut circle_id = 0u64;
        let mut track_id = 0u64;
        let mut text_has = false;
        for node in nodes {
            for el in node.modifier.elements() {
                if matches!(el, ModifierElement::Background { shape: Shape::Circle, .. }) {
                    circle_id = node.id;
                }
                if matches!(el, ModifierElement::BorderDynamic { .. }) {
                    track_id = node.id;
                }
                if matches!(el, ModifierElement::TextContent { .. }) {
                    text_has = true;
                }
            }
        }
        assert!(text_has, "存在文本节点");
        let (cx, cy) = abs_pos(circle_id);
        let circle = nodes.iter().find(|n| n.id == circle_id).unwrap();
        let (ccx, ccy) = (cx + circle.measured_size.width / 2.0, cy + circle.measured_size.height / 2.0);
        let (tx, ty) = abs_pos(track_id);
        // 轨道采样点：左侧 x=5（避开两种状态的圆）
        let track_px = at((tx + 5.0) as usize, (ty + 16.0) as usize);
        let circle_px = at(ccx as usize, ccy as usize);
        let expect_track = if checked { theme.primary } else { theme.surface_container_highest };
        let expect_circle = if checked { theme.on_primary } else { theme.outline };
        let near = |a: &[u8; 4], c: &crate::modifier::Color| -> bool {
            // raster_n32_premul = BGRA 字节序
            (a[2] as i32 - c.r as i32).abs() <= 6
                && (a[1] as i32 - c.g as i32).abs() <= 6
                && (a[0] as i32 - c.b as i32).abs() <= 6
        };
        assert!(
            near(&track_px, &expect_track),
            "轨道色应={:?} 实际={:?}（checked={checked}）",
            (expect_track.r, expect_track.g, expect_track.b),
            (&track_px[0], &track_px[1], &track_px[2]),
        );
        assert!(
            near(&circle_px, &expect_circle),
            "圆色应={:?} 实际={:?}（checked={checked}）",
            (expect_circle.r, expect_circle.g, expect_circle.b),
            (&circle_px[0], &circle_px[1], &circle_px[2]),
        );
        // 文字区域（开关下方）应有非背景像素
        let mut text_dark = 0;
        for y in 40..70usize {
            for x in 0..120usize {
                let p = at(x, y);
                if p[0] < 220 || p[1] < 220 || p[2] < 220 {
                    text_dark += 1;
                }
            }
        }
        assert!(text_dark > 10, "文字应渲染（暗像素 {text_dark}）");
    }

    #[test]
    fn full_demo_rapid_triple_click_all_sync() {
        // 复刻 switch_demo：4 个共享状态的交互开关（默认/图标/自定义色/hoist）
        // + 状态文字；对每个开关快速三连击后，渲染并校验所有开关与文字一致
        use crate::modifier::ModifierElement;
        use std::sync::Arc;
        use std::time::Duration;
        let _g = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = crate::core::composer::Composer::new();
        let holder = std::cell::RefCell::new(None::<crate::core::state::State<bool>>);
        let src_holder = std::cell::RefCell::new(None::<crate::ui::interaction::MutableInteractionSource>);
        let mut custom_colors = SwitchColors::from_theme(&theme);
        custom_colors.checked_track = crate::modifier::Color::from_argb(255, 46, 125, 50);
        custom_colors.checked_thumb = crate::modifier::Color::WHITE;
        custom_colors.unchecked_track = crate::modifier::Color::from_argb(255, 224, 224, 224);
        custom_colors.unchecked_thumb = crate::modifier::Color::from_argb(255, 100, 100, 100);
        custom_colors.unchecked_border = custom_colors.unchecked_thumb;
        let scene_custom = custom_colors.clone();
        let scene = |ctx: &mut ComposeCtx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                let c = ctx.remember(|| false);
                holder.replace(Some(c.clone()));
                crate::ui::Column::new().build(ctx, |ctx| {
                    // 1) 默认开关（点击切换）
                    let c1 = c.clone();
                    Switch::new(c.get())
                        .on_checked_change(move |v| c1.update(|s| *s = v))
                        .build(ctx, |_| {});
                    crate::ui::Text::new(if c.get() { "已开启" } else { "已关闭" }).build(ctx);
                    // 2) 带图标
                    let c2 = c.clone();
                    Switch::new(c.get())
                        .on_checked_change(move |v| c2.update(|s| *s = v))
                            .build(ctx, |ctx| {
                                crate::ui::icon::Icon::svg_path("M12 2L22 12 12 22 2 12Z")
                                    .size(SWITCH_ICON_SIZE)
                                    .build(ctx);
                            });
                    // 3) 自定义色
                    let c3 = c.clone();
                    Switch::new(c.get())
                        .colors(scene_custom.clone())
                        .on_checked_change(move |v| c3.update(|s| *s = v))
                        .build(ctx, |_| {});
                    // 4) hoist
                    let src = ctx.remember(|| MutableInteractionSource::new()).get();
                    src_holder.replace(Some(src.clone()));
                    let c4 = c.clone();
                    Switch::new(c.get())
                        .interaction_source(src)
                        .on_checked_change(move |v| c4.update(|s| *s = v))
                        .build(ctx, |_| {});
                    crate::ui::Text::new(if c.get() { "已开启" } else { "已关闭" }).build(ctx);
                });
            });
        };
        let frame = |composer: &mut crate::core::composer::Composer| {
            crate::animation::update_animations();
            composer.compose(scene);
            composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 300.0));
        };
        let click = |composer: &mut crate::core::composer::Composer, switch_no: usize| {
            let src = src_holder.borrow().clone().unwrap();
            src.emit_press();
            let mut on_click: Option<Arc<dyn Fn() + Send + Sync>> = None;
            let mut drag_start: Option<Arc<dyn Fn((f32, f32)) + Send + Sync>> = None;
            let mut drag_move: Option<Arc<dyn Fn((f32, f32), (f32, f32)) + Send + Sync>> = None;
            let mut drag_end: Option<Arc<dyn Fn() + Send + Sync>> = None;
            let mut seen = 0;
            for node in composer.arena_nodes() {
                for el in node.modifier.elements() {
                    if matches!(el, ModifierElement::Clickable { .. }) {
                        seen += 1;
                        if seen != switch_no {
                            continue;
                        }
                    }
                    match el {
                        ModifierElement::Clickable { on_click: cb, .. } if seen == switch_no => {
                            on_click = Some(cb.clone());
                        }
                        ModifierElement::DragOnStart { cb } => drag_start = Some(cb.clone()),
                        ModifierElement::DragOnMove { cb } => drag_move = Some(cb.clone()),
                        ModifierElement::DragOnEnd { cb } => drag_end = Some(cb.clone()),
                        _ => {}
                    }
                }
            }
            drag_start.expect("drag start")((30.0, 16.0));
            drag_move.expect("drag move")((40.0, 16.0), (10.0, 0.0));
            on_click.expect("click")();
            drag_end.expect("drag end")();
            src.emit_release();
        };
        frame(&mut composer);
        // 对每个交互开关三连击（1..=4）
        for sw in 1..=4 {
            for _ in 0..3 {
                click(&mut composer, sw);
                frame(&mut composer);
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        for _ in 0..500 {
            if !crate::animation::update_animations() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        composer.compose(scene);
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 300.0));
        let c = holder.borrow().clone().unwrap();
        let checked = c.get();
        // 渲染并校验：4 个交互开关（各自独立的轨道/圆节点）颜色一致
        use skia_safe::{Color, surfaces};
        let mut surface = surfaces::raster_n32_premul((400, 300)).unwrap();
        let canvas = surface.canvas();
        canvas.clear(Color::WHITE);
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        crate::render::render(nodes, root, canvas);
        let pm = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        let at = |x: usize, y: usize| -> [u8; 4] { px[y * 400 + x] };
        let abs_pos = |target_id: u64| -> (f32, f32) {
            fn walk(nodes: &[crate::layout::node::LayoutNode], idx: usize, target: u64, ax: f32, ay: f32) -> Option<(f32, f32)> {
                let nx = ax + nodes[idx].position.x;
                let ny = ay + nodes[idx].position.y;
                if nodes[idx].id == target {
                    return Some((nx, ny));
                }
                for &ch in &nodes[idx].children {
                    if let Some(r) = walk(nodes, ch, target, nx, ny) {
                        return Some(r);
                    }
                }
                None
            }
            walk(nodes, root, target_id, 0.0, 0.0).unwrap()
        };
        let mut circles = Vec::new();
        let mut tracks = Vec::new();
        for node in nodes {
            let mut has_circle_bg = false;
            let mut has_border = false;
            for el in node.modifier.elements() {
                if matches!(el, ModifierElement::Background { shape: Shape::Circle, .. }) {
                    has_circle_bg = true;
                }
                if matches!(el, ModifierElement::BorderDynamic { .. }) {
                    has_border = true;
                }
            }
            if has_circle_bg {
                circles.push(node.id);
            }
            if has_border {
                tracks.push(node.id);
            }
        }
        assert_eq!(circles.len(), 4, "应校验 4 个交互开关的圆");
        assert_eq!(tracks.len(), 4, "应校验 4 个交互开关的轨道");
        for (idx, (circle_id, track_id)) in circles.iter().zip(tracks.iter()).enumerate() {
            let circle_node = nodes.iter().find(|n| n.id == *circle_id).unwrap();
            {
                let (nx, ny) = abs_pos(*circle_id);
                // 圆心可能被 thumbContent 图标覆盖（未选中时图标 tint 接近轨道色），
                // 采样点取圆心沿对角线外移 30%：仍在圆内，但避开 16dp 图标菱形。
                let cx = (nx + circle_node.measured_size.width * 0.8) as usize;
                let cy = (ny + circle_node.measured_size.height * 0.8) as usize;
                let p = at(cx, cy);
                let expect = if idx == 2 {
                    if checked {
                        custom_colors.checked_thumb
                    } else {
                        custom_colors.unchecked_thumb
                    }
                } else if checked {
                    theme.on_primary
                } else {
                    theme.outline
                };
                let near = (p[2] as i32 - expect.r as i32).abs() <= 8
                    && (p[1] as i32 - expect.g as i32).abs() <= 8
                    && (p[0] as i32 - expect.b as i32).abs() <= 8;
                assert!(near, "圆 {}（第 {} 个）颜色应={:?} 实际=({},{},{})（checked={checked}）",
                    circle_id, idx + 1, (expect.r, expect.g, expect.b), p[2], p[1], p[0]);
            }
            {
                // 轨道左侧 x=5 采样
                let (nx, ny) = abs_pos(*track_id);
                let p = at((nx + 5.0) as usize, (ny + 16.0) as usize);
                let expect = if idx == 2 {
                    if checked {
                        custom_colors.checked_track
                    } else {
                        custom_colors.unchecked_track
                    }
                } else if checked {
                    theme.primary
                } else {
                    theme.surface_container_highest
                };
                let near = (p[2] as i32 - expect.r as i32).abs() <= 8
                    && (p[1] as i32 - expect.g as i32).abs() <= 8
                    && (p[0] as i32 - expect.b as i32).abs() <= 8;
                assert!(near, "轨道 {}（第 {} 个）颜色应={:?} 实际=({},{},{})（checked={checked}）",
                    track_id, idx + 1, (expect.r, expect.g, expect.b), p[2], p[1], p[0]);
            }
        }
    }

    #[test]
    fn thumb_content_icon_tint_resolves() {
        // 集成：thumbContent 内 Icon（tint Auto）解析为 icon_color
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = crate::core::composer::Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                Switch::new(true)
                    .on_checked_change(|_| {})
                    .build(ctx, |ctx| {
                        crate::ui::icon::Icon::svg_path("M12 2L22 12 12 22 2 12Z")
                            .size(SWITCH_ICON_SIZE)
                            .build(ctx);
                    });
            });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        let mut tint = None;
        for node in composer.arena_nodes() {
            for el in node.modifier.elements() {
                if let crate::modifier::ModifierElement::DrawIcon { spec } = el {
                    tint = spec.tint;
                }
            }
        }
        assert_eq!(tint, Some(theme.on_primary_container), "checked 图标色下传");
    }
}
