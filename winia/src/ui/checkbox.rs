//! Checkbox 组件 — 对标 material3 `Checkbox`（Boolean 状态）
//!
//! M3 1.4.0 实现要点（对齐项）：
//! - 视觉 20×20、圆角 2、勾选框；描边 2dp，checked 时边框色=容器色（合并）；
//! - 触摸目标/状态层 40×40（`CheckboxTokens.StateLayerSize`），波纹 unbounded；
//! - 颜色只分 enabled×checked（M3 `CheckboxColors` 无 hover/focus/press 变体，
//!   交互反馈由 ripple 承担）；
//! - 勾号用填充 check 路径 + 缩放动画近似 M3 `checkDrawFraction` 过渡。

use crate::core::composer::{ComposeCtx, GroupStatus};
use crate::layout::BoxLayout;
use crate::modifier::{Color, GraphicsLayerParams, Modifier, Shape};
use crate::ui::interaction::MutableInteractionSource;
use crate::ui::theme::{ThemeColors, WiniaTheme};
use std::sync::Arc;

/// 视觉勾选框尺寸（M3 实现 `CheckboxSize = 20dp`）
pub const CHECKBOX_SIZE: f32 = 20.0;
/// 状态层/触摸目标尺寸（`CheckboxTokens.StateLayerSize = 40dp`）
pub const CHECKBOX_TOUCH_TARGET: f32 = 40.0;
/// 描边宽度（`CheckboxDefaults.StrokeWidth = 2dp`）
pub const CHECKBOX_STROKE_WIDTH: f32 = 2.0;
/// 容器圆角（`CheckboxTokens.ContainerShape = RoundedCornerShape(2dp)`）
pub const CHECKBOX_CORNER_RADIUS: f32 = 2.0;

/// Material Icons “check”（24dp viewBox 填充路径）——勾号
const CHECK_MARK_PATH: &str = "M9.55 18.2 3.55 12.2 5 10.75 9.55 15.3 19 5.85 20.45 7.3z";

/// 勾选框颜色集（对标 material3 `CheckboxColors`）——box/border/checkmark
/// 三组色 × enabled/disabled × checked/unchecked。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CheckboxColors {
    pub checked_checkmark: Color,
    pub unchecked_checkmark: Color,
    pub checked_box: Color,
    pub unchecked_box: Color,
    pub disabled_checked_box: Color,
    pub disabled_unchecked_box: Color,
    pub checked_border: Color,
    pub unchecked_border: Color,
    pub disabled_border: Color,
    pub disabled_unchecked_border: Color,
}

impl CheckboxColors {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        checked_checkmark: Color,
        unchecked_checkmark: Color,
        checked_box: Color,
        unchecked_box: Color,
        disabled_checked_box: Color,
        disabled_unchecked_box: Color,
        checked_border: Color,
        unchecked_border: Color,
        disabled_border: Color,
        disabled_unchecked_border: Color,
    ) -> Self {
        Self {
            checked_checkmark,
            unchecked_checkmark,
            checked_box,
            unchecked_box,
            disabled_checked_box,
            disabled_unchecked_box,
            checked_border,
            unchecked_border,
            disabled_border,
            disabled_unchecked_border,
        }
    }

    /// 从主题推导默认色（对标 `CheckboxDefaults.colors()`：
    /// `CheckboxTokens` 1.4.0）
    pub fn from_theme(theme: &ThemeColors) -> Self {
        let transparent = Color::from_argb(0, 0, 0, 0);
        let alpha = |c: Color, a: f32| Color::from_argb((c.a as f32 * a) as u8, c.r, c.g, c.b);
        // Selected：容器/边框 Primary、勾号 OnPrimary
        // Unselected：容器透明、边框 OnSurfaceVariant、勾号透明
        // Disabled Selected：OnSurface @ 0.38 + 勾号 OnPrimary
        // （M3 CheckboxColors 无 disabled checkmark 字段，禁用勾号仍取
        // checkedCheckmarkColor——与 1.4.0 实现一致）
        // Disabled Unselected：容器透明、边框 OnSurface @ 0.38
        Self::new(
            theme.on_primary,
            transparent,
            theme.primary,
            transparent,
            alpha(theme.on_surface, 0.38),
            transparent,
            theme.primary,
            theme.on_surface_variant,
            alpha(theme.on_surface, 0.38),
            alpha(theme.on_surface, 0.38),
        )
    }

    pub fn box_color(&self, enabled: bool, checked: bool) -> Color {
        if !enabled {
            if checked { self.disabled_checked_box } else { self.disabled_unchecked_box }
        } else if checked {
            self.checked_box
        } else {
            self.unchecked_box
        }
    }

    pub fn border_color(&self, enabled: bool, checked: bool) -> Color {
        if !enabled {
            if checked { self.disabled_border } else { self.disabled_unchecked_border }
        } else if checked {
            self.checked_border
        } else {
            self.unchecked_border
        }
    }

    pub fn checkmark_color(&self, enabled: bool, checked: bool) -> Color {
        if !enabled {
            if checked { self.checked_checkmark } else { self.unchecked_checkmark }
        } else if checked {
            self.checked_checkmark
        } else {
            self.unchecked_checkmark
        }
    }
}

/// Checkbox 默认值（对标 material3 `CheckboxDefaults`）
pub struct CheckboxDefaults;

impl CheckboxDefaults {
    pub fn checkbox_colors(theme: &ThemeColors) -> CheckboxColors {
        CheckboxColors::from_theme(theme)
    }

    pub fn shape() -> Shape {
        Shape::rounded(CHECKBOX_CORNER_RADIUS)
    }

    pub fn stroke_width() -> f32 {
        CHECKBOX_STROKE_WIDTH
    }
}

/// Checkbox 组件 Builder（对标 material3 `Checkbox(checked, onCheckedChange,
/// enabled, colors, interactionSource)`）
pub struct Checkbox {
    checked: bool,
    on_checked_change: Option<Arc<dyn Fn(bool) + Send + Sync>>,
    enabled: bool,
    colors: Option<CheckboxColors>,
    interaction_source: Option<MutableInteractionSource>,
    modifier: Modifier,
}

impl Checkbox {
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

    pub fn colors(mut self, colors: CheckboxColors) -> Self {
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

    pub fn build(self, ctx: &mut ComposeCtx) {
        ctx.changed(&self.checked);
        ctx.changed(&self.enabled);
        ctx.changed(&self.colors);
        let key = ctx.next_key();
        let theme = WiniaTheme::colors();
        let colors = self
            .colors
            .unwrap_or_else(|| CheckboxDefaults::checkbox_colors(&theme));
        let interaction = self
            .interaction_source
            .unwrap_or_else(|| ctx.remember(|| MutableInteractionSource::new()).get());
        let checked = self.checked;
        let box_color = colors.box_color(self.enabled, checked);
        let border_color = colors.border_color(self.enabled, checked);
        let check_color = colors.checkmark_color(self.enabled, checked);

        // 勾号动画：checked 缩放入场、unchecked 缩出（对标 M3 checkDrawFraction
        // 过渡——Spring 近似 motion scheme）
        let check_scale = ctx.animate_float_as_state(
            if checked { 1.0 } else { 0.0 },
            crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec::default()),
        );

        let shape = CheckboxDefaults::shape();
        let mut modifier = Modifier::new()
            .size(CHECKBOX_TOUCH_TARGET, CHECKBOX_TOUCH_TARGET)
            // 状态层形状 = Circle（焦点环跟随圆形；波纹 unbounded 不受 clip 影响）
            .clip(Shape::Circle);
        if self.enabled {
            if let Some(on_checked_change) = &self.on_checked_change {
                let cb = on_checked_change.clone();
                modifier = modifier
                    .clickable_with_source(&interaction, move || cb(!checked))
                    // M3：ripple(bounded = false, radius = StateLayerSize / 2)；
                    // 本框架 ripple 无 radius 参数，半径由节点尺寸隐式决定
                    .ripple(&interaction, theme.on_surface, false);
            }
        }
        modifier = modifier.then(self.modifier);

        match ctx.start_restartable_group(
            key,
            modifier,
            BoxLayout::new().alignment(crate::layout::Alignment::Center),
        ) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                // 视觉 20×20 勾选框：checked 时边框色=容器色（合并为纯填充），
                // unchecked 时透明底 + 2dp 边框——对标 M3 drawBox 分支
                let visual = Modifier::new()
                    .size(CHECKBOX_SIZE, CHECKBOX_SIZE)
                    .background(box_color, shape)
                    .border(CHECKBOX_STROKE_WIDTH, border_color, shape);
                let vkey = ctx.next_key();
                match ctx.start_restartable_group(
                    vkey,
                    visual,
                    BoxLayout::new().alignment(crate::layout::Alignment::Center),
                ) {
                    GroupStatus::Skip => {}
                    GroupStatus::Enter => {
                        // 勾号：graphicsLayer 缩放动画（unchecked scale=0 不渲染）
                        let scale = check_scale.clone();
                        crate::ui::icon::Icon::svg_path(CHECK_MARK_PATH)
                            .size(CHECKBOX_SIZE)
                            .tint(check_color)
                            .modifier(Modifier::new().graphics_layer(move || GraphicsLayerParams {
                                scale_x: scale.get(),
                                scale_y: scale.get(),
                                ..Default::default()
                            }))
                            .build(ctx);
                    }
                }
                ctx.end_restartable_group();
            }
        }
        ctx.set_current_node_focus_color(theme.primary);
        ctx.end_restartable_group();
    }

    pub fn get_checked(&self) -> bool {
        self.checked
    }
    pub fn get_enabled(&self) -> bool {
        self.enabled
    }
    pub fn get_colors(&self) -> Option<CheckboxColors> {
        self.colors
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_state_resolution() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let colors = CheckboxDefaults::checkbox_colors(&theme);
        // checked：primary 容器/边框 + onPrimary 勾号
        assert_eq!(colors.box_color(true, true), theme.primary);
        assert_eq!(colors.border_color(true, true), theme.primary);
        assert_eq!(colors.checkmark_color(true, true), theme.on_primary);
        // unchecked：透明容器 + OnSurfaceVariant 边框 + 透明勾号
        assert_eq!(colors.box_color(true, false).a, 0);
        assert_eq!(colors.border_color(true, false), theme.on_surface_variant);
        assert_eq!(colors.checkmark_color(true, false).a, 0);
        // disabled 优先于 checked
        assert_eq!(colors.box_color(false, true), colors.disabled_checked_box);
        assert_eq!(colors.border_color(false, false), colors.disabled_unchecked_border);
        // disabled 色值：OnSurface @ 38%（SelectedDisabledContainerOpacity）
        assert_eq!(colors.disabled_checked_box.a, (theme.on_surface.a as f32 * 0.38) as u8);
        assert_eq!(colors.disabled_unchecked_border.a, (theme.on_surface.a as f32 * 0.38) as u8);
        // M3 CheckboxColors 无 disabled checkmark 字段：禁用勾号仍取
        // checkedCheckmarkColor = OnPrimary（SelectedDisabledIconColor token
        // 定义了但实现未使用——与 1.4.0 源码一致）
        assert_eq!(colors.checkmark_color(false, true), theme.on_primary);
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
                Checkbox::new(checked)
                    .on_checked_change(move |v| cb.store(v, Ordering::Relaxed))
                    .build(ctx);
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
    fn disabled_checkbox_has_no_interaction_elements() {
        use crate::modifier::ModifierElement;
        let mut composer = crate::core::composer::Composer::new();
        composer.compose(|ctx| {
            Checkbox::new(false)
                .enabled(false)
                .on_checked_change(|_| {})
                .build(ctx);
        });
        composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        for node in composer.arena_nodes() {
            for el in node.modifier.elements() {
                assert!(
                    !matches!(el, ModifierElement::Ripple { .. })
                        && !matches!(el, ModifierElement::Focusable { .. })
                        && !matches!(el, ModifierElement::Clickable { .. }),
                    "禁用 Checkbox 不应有交互元素: {el:?}"
                );
            }
        }
    }

    #[test]
    fn checked_checkmark_icon_tint_resolves_to_on_primary() {
        // 集成：checked 时勾号图标 tint = on_primary
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = crate::core::composer::Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                Checkbox::new(true).on_checked_change(|_| {}).build(ctx);
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
        assert_eq!(tint, Some(theme.on_primary), "勾号 tint = OnPrimary");
    }

    #[test]
    fn unchecked_has_transparent_checkmark_and_no_fill() {
        // 结构断言：unchecked 时勾号透明、容器无填充色（M3 语义）
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let colors = CheckboxColors::from_theme(&theme);
        assert_eq!(colors.unchecked_box.a, 0);
        assert_eq!(colors.unchecked_checkmark.a, 0);
        assert_eq!(colors.checked_box, theme.primary);
    }
}
