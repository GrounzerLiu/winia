//! Switch 组件 — 对标 material3 `Switch`
//!
//! M3 1.4.0 实现要点（对齐项）：
//! - 轨道 52×32、CornerFull、2dp 边框；拇指圆形，checked=24 / unchecked=16 /
//!   pressed=28（`SwitchTokens`）；
//! - 拇指水平偏移：unchecked=4、checked=24（pressed 时内收 2），尺寸/偏移
//!   均 Spring 动画（M3 `ThumbNode` FastSpatial；pressed 为 Snap——统一用
//!   Spring 近似）；
//! - 点击/波纹：toggleable 在轨道（indication=null），unbounded ripple 在
//!   拇指上（radius = StateLayerSize/2 = 20）；
//! - 颜色按 enabled×checked 解析（M3 `SwitchColors`）；disabled 色为 token
//!   alpha compositeOver(surface)；
//! - 焦点环颜色 = Secondary（`SwitchTokens.FocusIndicatorColor`，与其它组件
//!   用 Primary 不同）。

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
/// 拇指尺寸（`SwitchTokens.SelectedHandleWidth = 24`）
pub const SWITCH_THUMB_CHECKED_SIZE: f32 = 24.0;
/// 未选中拇指（`UnselectedHandleWidth = 16`）
pub const SWITCH_THUMB_UNCHECKED_SIZE: f32 = 16.0;
/// 按压拇指（`PressedHandleWidth = 28`）
pub const SWITCH_THUMB_PRESSED_SIZE: f32 = 28.0;
/// 拇指起始偏移 = (TrackHeight - ThumbDiameter)/2 = 4
pub const SWITCH_THUMB_PADDING: f32 = 4.0;
/// 拇指最大偏移 = (TrackWidth - ThumbDiameter) - ThumbPadding = 24
pub const SWITCH_THUMB_MAX_OFFSET: f32 = 24.0;
/// 拇指内容图标尺寸（`SwitchDefaults.IconSize`）
pub const SWITCH_ICON_SIZE: f32 = 16.0;

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

        // 拇指尺寸/偏移动画（M3 ThumbNode：pressed=28 且偏移内收 2）
        let thumb_size = ctx.animate_float_as_state(
            if state.pressed {
                SWITCH_THUMB_PRESSED_SIZE
            } else if checked {
                SWITCH_THUMB_CHECKED_SIZE
            } else {
                SWITCH_THUMB_UNCHECKED_SIZE
            },
            crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec::default()),
        );
        let thumb_offset = ctx.animate_float_as_state(
            if state.pressed && checked {
                SWITCH_THUMB_MAX_OFFSET - SWITCH_TRACK_OUTLINE_WIDTH
            } else if state.pressed {
                SWITCH_TRACK_OUTLINE_WIDTH
            } else if checked {
                SWITCH_THUMB_MAX_OFFSET
            } else {
                SWITCH_THUMB_PADDING
            },
            crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec::default()),
        );

        // 轨道（toggleable 在此，indication=null——波纹在拇指上）
        let track_shape = SwitchDefaults::shape();
        let mut modifier = Modifier::new()
            .size(SWITCH_TRACK_WIDTH, SWITCH_TRACK_HEIGHT)
            .background(track_color, track_shape)
            .border(
                SWITCH_TRACK_OUTLINE_WIDTH,
                border_color,
                track_shape,
            );
        if self.enabled {
            if let Some(on_checked_change) = &self.on_checked_change {
                let cb = on_checked_change.clone();
                modifier = modifier
                    .clickable_with_source(&interaction, move || cb(!checked));
            }
        }
        modifier = modifier.then(self.modifier);

        match ctx.start_restartable_group(
            key,
            modifier,
            // Alignment 横纵同值：用 Start + 拇指动态 offset 实现
            // “左对齐 + 垂直居中”（y = (TrackHeight - thumbSize)/2）
            BoxLayout::new().alignment(crate::layout::Alignment::Start),
        ) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                // 拇指：动态尺寸 + 水平偏移 + Circle 背景；unbounded ripple
                let s1 = thumb_size.clone();
                let s2 = thumb_size.clone();
                let s3 = thumb_size.clone();
                let o1 = thumb_offset.clone();
                let mut thumb = Modifier::new()
                    .size(move || s1.get(), move || s2.get())
                    .offset(
                        move || o1.get(),
                        move || (SWITCH_TRACK_HEIGHT - s3.get()) / 2.0,
                    )
                    .background(thumb_color, Shape::Circle);
                if self.enabled {
                    // M3：ripple(bounded = false, radius = StateLayerSize / 2)
                    thumb = thumb.ripple(&interaction, theme.on_surface, false);
                }
                let tkey = ctx.next_key();
                match ctx.start_restartable_group(
                    tkey,
                    thumb,
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
