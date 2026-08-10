//! Checkbox / TriStateCheckbox 组件 — 对标 material3 `Checkbox` / `TriStateCheckbox`
//!
//! M3 1.4.0 实现要点（对齐项）：
//! - 视觉 20×20、圆角 2、描边 2dp；checked/indeterminate 时边框色=容器色（合并）；
//! - 触摸目标/状态层 40×40（`CheckboxTokens.StateLayerSize`），波纹 unbounded；
//! - 颜色只分 enabled×state（M3 `CheckboxColors` 无 hover/focus/press 变体，
//!   交互反馈由 ripple 承担）；
//! - On=check 路径、Indeterminate=横线（M3 drawCheck 中段压平），用两个图标
//!   各自缩放动画近似 `checkDrawFraction` + `crossCenterGravitation` 过渡；
//! - `Checkbox` 内部委托 `TriStateCheckbox(state = ToggleableState(checked))`。

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
/// Indeterminate 横线（M3 drawCheck 中段：0.2w..0.8w、y=0.5h，2dp 高）
const DASH_PATH: &str = "M4.8 11h14.4v2H4.8z";

/// 三态（对标 foundation `ToggleableState`）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToggleableState {
    Off,
    On,
    Indeterminate,
}

impl ToggleableState {
    /// `ToggleableState(checked: Boolean)` 等价物
    pub fn from_bool(checked: bool) -> Self {
        if checked { Self::On } else { Self::Off }
    }

    /// 视觉“着色选中”态：On 与 Indeterminate 都算（颜色解析用）。
    /// 注意与 M3 `ToggleableState.isSelected`（仅 On）语义不同——外部如需
    /// “真选中”判断请用 `self == ToggleableState::On`。
    pub fn is_checked(self) -> bool {
        matches!(self, Self::On | Self::Indeterminate)
    }

    pub fn is_indeterminate(self) -> bool {
        self == Self::Indeterminate
    }
}

/// 勾选框颜色集（对标 material3 `CheckboxColors`）——box/border/checkmark
/// 三组色 × enabled/disabled × Off/On/Indeterminate。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CheckboxColors {
    pub checked_checkmark: Color,
    pub unchecked_checkmark: Color,
    pub checked_box: Color,
    pub unchecked_box: Color,
    pub disabled_checked_box: Color,
    pub disabled_unchecked_box: Color,
    pub disabled_indeterminate_box: Color,
    pub checked_border: Color,
    pub unchecked_border: Color,
    pub disabled_border: Color,
    pub disabled_unchecked_border: Color,
    pub disabled_indeterminate_border: Color,
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
        disabled_indeterminate_box: Color,
        checked_border: Color,
        unchecked_border: Color,
        disabled_border: Color,
        disabled_unchecked_border: Color,
        disabled_indeterminate_border: Color,
    ) -> Self {
        Self {
            checked_checkmark,
            unchecked_checkmark,
            checked_box,
            unchecked_box,
            disabled_checked_box,
            disabled_unchecked_box,
            disabled_indeterminate_box,
            checked_border,
            unchecked_border,
            disabled_border,
            disabled_unchecked_border,
            disabled_indeterminate_border,
        }
    }

    /// 从主题推导默认色（对标 `CheckboxDefaults.colors()`：
    /// `CheckboxTokens` 1.4.0）
    pub fn from_theme(theme: &ThemeColors) -> Self {
        let transparent = Color::from_argb(0, 0, 0, 0);
        let alpha = |c: Color, a: f32| Color::from_argb((c.a as f32 * a) as u8, c.r, c.g, c.b);
        // Selected（On/Indeterminate）：容器/边框 Primary、勾号 OnPrimary
        // Unselected：容器透明、边框 OnSurfaceVariant、勾号透明
        // Disabled：容器与边框 OnSurface @ 0.38（含 indeterminate 专用字段）
        Self::new(
            theme.on_primary,
            transparent,
            theme.primary,
            transparent,
            alpha(theme.on_surface, 0.38),
            transparent,
            alpha(theme.on_surface, 0.38),
            theme.primary,
            theme.on_surface_variant,
            alpha(theme.on_surface, 0.38),
            alpha(theme.on_surface, 0.38),
            alpha(theme.on_surface, 0.38),
        )
    }

    pub fn box_color(&self, enabled: bool, state: ToggleableState) -> Color {
        if !enabled {
            match state {
                ToggleableState::On => self.disabled_checked_box,
                ToggleableState::Off => self.disabled_unchecked_box,
                ToggleableState::Indeterminate => self.disabled_indeterminate_box,
            }
        } else if state.is_checked() {
            self.checked_box
        } else {
            self.unchecked_box
        }
    }

    pub fn border_color(&self, enabled: bool, state: ToggleableState) -> Color {
        if !enabled {
            match state {
                ToggleableState::On => self.disabled_border,
                ToggleableState::Off => self.disabled_unchecked_border,
                ToggleableState::Indeterminate => self.disabled_indeterminate_border,
            }
        } else if state.is_checked() {
            self.checked_border
        } else {
            self.unchecked_border
        }
    }

    /// M3 `checkmarkColor(state)`：只分 On/Indeterminate 与 Off，忽略 enabled
    /// （`CheckboxColors` 无 disabled checkmark 字段，禁用勾号仍取
    /// checkedCheckmarkColor——与 1.4.0 实现一致）
    pub fn checkmark_color(&self, state: ToggleableState) -> Color {
        if state.is_checked() {
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

/// 共享实现（对标 M3 `CheckboxImpl`）——`Checkbox` 与 `TriStateCheckbox` 共用
#[allow(clippy::too_many_arguments)]
fn checkbox_impl(
    ctx: &mut ComposeCtx,
    state: ToggleableState,
    on_click: Option<Arc<dyn Fn() + Send + Sync>>,
    enabled: bool,
    colors: Option<CheckboxColors>,
    interaction_source: Option<MutableInteractionSource>,
    modifier: Modifier,
) {
    ctx.changed(&state);
    ctx.changed(&enabled);
    ctx.changed(&colors);
    let key = ctx.next_key();
    let theme = WiniaTheme::colors();
    let colors = colors.unwrap_or_else(|| CheckboxDefaults::checkbox_colors(&theme));
    let interaction = interaction_source
        .unwrap_or_else(|| ctx.remember(|| MutableInteractionSource::new()).get());
    let box_color = colors.box_color(enabled, state);
    let border_color = colors.border_color(enabled, state);
    // 勾号保持选中色常量：Off 静止态由 scale=0 隐藏；取消选中过渡期（1→0）
    // 仍可见，否则 tint 瞬时切透明会把退出动画吞掉（M3 checkmarkColor 本身
    // 也是随 checkDrawFraction 过渡的）。
    let check_color = colors.checked_checkmark;

    // 容器/边框颜色过渡（M3 animateColorAsState + CheckAnimationSpec）：
    // 选中/取消选中都从当前颜色动画到目标色，而不是瞬间跳变。
    let color_spec =
        crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec::default());
    let box_color_anim = ctx.animate_color_as_state(box_color, color_spec.clone());
    let border_color_anim = ctx.animate_color_as_state(border_color, color_spec);

    // On→check 缩放 1、Indeterminate→dash 缩放 1、Off→都 0（Spring 近似
    // M3 checkDrawFraction + crossCenterGravitation 过渡）
    let check_scale = ctx.animate_float_as_state(
        if state == ToggleableState::On { 1.0 } else { 0.0 },
        crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec::default()),
    );
    let dash_scale = ctx.animate_float_as_state(
        if state == ToggleableState::Indeterminate { 1.0 } else { 0.0 },
        crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec::default()),
    );

    let shape = CheckboxDefaults::shape();
    let mut m = Modifier::new()
        .size(CHECKBOX_TOUCH_TARGET, CHECKBOX_TOUCH_TARGET)
        // 状态层形状 = Circle（焦点环跟随圆形；波纹 unbounded 不受 clip 影响）
        .clip(Shape::Circle);
    if enabled {
        if let Some(on_click) = on_click {
            m = m
                .clickable_with_source(&interaction, move || on_click())
                // M3：ripple(bounded = false, radius = StateLayerSize / 2)；
                // 本框架 ripple 无 radius 参数，半径由节点尺寸隐式决定
                .ripple(&interaction, theme.on_surface, false);
        }
    }
    m = m.then(modifier);

    match ctx.start_restartable_group(
        key,
        m,
        BoxLayout::new().alignment(crate::layout::Alignment::Center),
    ) {
        GroupStatus::Skip => {}
        GroupStatus::Enter => {
            // 视觉 20×20 勾选框：checked/indeterminate 时边框色=容器色（合并为
            // 纯填充），Off 时透明底 + 2dp 边框——对标 M3 drawBox 分支
            let visual = Modifier::new()
                .size(CHECKBOX_SIZE, CHECKBOX_SIZE)
                .background(move || box_color_anim.peek(), shape)
                .border_dynamic(
                    CHECKBOX_STROKE_WIDTH,
                    move || border_color_anim.peek(),
                    shape,
                );
            let vkey = ctx.next_key();
            match ctx.start_restartable_group(
                vkey,
                visual,
                BoxLayout::new().alignment(crate::layout::Alignment::Center),
            ) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    // 勾号（On）
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
                    // 横线（Indeterminate）
                    let scale = dash_scale.clone();
                    crate::ui::icon::Icon::svg_path(DASH_PATH)
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

/// TriStateCheckbox 组件 Builder（对标 material3 `TriStateCheckbox(state,
/// onClick, enabled, colors, interactionSource)`）
pub struct TriStateCheckbox {
    state: ToggleableState,
    on_click: Option<Arc<dyn Fn() + Send + Sync>>,
    enabled: bool,
    colors: Option<CheckboxColors>,
    interaction_source: Option<MutableInteractionSource>,
    modifier: Modifier,
}

impl TriStateCheckbox {
    pub fn new(state: ToggleableState) -> Self {
        Self {
            state,
            on_click: None,
            enabled: true,
            colors: None,
            interaction_source: None,
            modifier: Modifier::new(),
        }
    }

    pub fn state(mut self, state: ToggleableState) -> Self {
        self.state = state;
        self
    }

    pub fn on_click(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_click = Some(Arc::new(f));
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

    fn on_click_opt(mut self, f: Option<Arc<dyn Fn() + Send + Sync>>) -> Self {
        self.on_click = f;
        self
    }

    fn colors_opt(mut self, colors: Option<CheckboxColors>) -> Self {
        self.colors = colors;
        self
    }

    fn interaction_source_opt(mut self, source: Option<MutableInteractionSource>) -> Self {
        self.interaction_source = source;
        self
    }

    pub fn build(self, ctx: &mut ComposeCtx) {
        checkbox_impl(
            ctx,
            self.state,
            self.on_click,
            self.enabled,
            self.colors,
            self.interaction_source,
            self.modifier,
        );
    }

    pub fn get_state(&self) -> ToggleableState {
        self.state
    }
    pub fn get_enabled(&self) -> bool {
        self.enabled
    }
    pub fn get_colors(&self) -> Option<CheckboxColors> {
        self.colors
    }
}

/// Checkbox 组件 Builder（对标 material3 `Checkbox(checked, onCheckedChange,
/// enabled, colors, interactionSource)`——内部委托 `TriStateCheckbox`）
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
        // M3：Checkbox → TriStateCheckbox(state = ToggleableState(checked),
        // onClick = { onCheckedChange(!checked) })
        let checked = self.checked;
        let on_click = self.on_checked_change.map(|cb| -> Arc<dyn Fn() + Send + Sync> {
            Arc::new(move || cb(!checked))
        });
        TriStateCheckbox::new(ToggleableState::from_bool(checked))
            .on_click_opt(on_click)
            .enabled(self.enabled)
            .colors_opt(self.colors)
            .interaction_source_opt(self.interaction_source)
            .modifier(self.modifier)
            .build(ctx);
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
    fn toggleable_state_conversion() {
        assert_eq!(ToggleableState::from_bool(true), ToggleableState::On);
        assert_eq!(ToggleableState::from_bool(false), ToggleableState::Off);
        assert!(ToggleableState::On.is_checked());
        assert!(ToggleableState::Indeterminate.is_checked());
        assert!(!ToggleableState::Off.is_checked());
        assert!(ToggleableState::Indeterminate.is_indeterminate());
    }

    #[test]
    fn color_state_resolution() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let colors = CheckboxDefaults::checkbox_colors(&theme);
        // On：primary 容器/边框 + onPrimary 勾号
        assert_eq!(colors.box_color(true, ToggleableState::On), theme.primary);
        assert_eq!(colors.border_color(true, ToggleableState::On), theme.primary);
        assert_eq!(colors.checkmark_color(ToggleableState::On), theme.on_primary);
        // Indeterminate 与 On 共用 checked 色组
        assert_eq!(colors.box_color(true, ToggleableState::Indeterminate), theme.primary);
        assert_eq!(colors.border_color(true, ToggleableState::Indeterminate), theme.primary);
        assert_eq!(colors.checkmark_color(ToggleableState::Indeterminate), theme.on_primary);
        // Off：透明容器 + OnSurfaceVariant 边框 + 透明勾号
        assert_eq!(colors.box_color(true, ToggleableState::Off).a, 0);
        assert_eq!(colors.border_color(true, ToggleableState::Off), theme.on_surface_variant);
        assert_eq!(colors.checkmark_color(ToggleableState::Off).a, 0);
        // disabled 优先于状态
        assert_eq!(colors.box_color(false, ToggleableState::On), colors.disabled_checked_box);
        assert_eq!(
            colors.border_color(false, ToggleableState::Indeterminate),
            colors.disabled_indeterminate_border
        );
        assert_eq!(
            colors.box_color(false, ToggleableState::Indeterminate),
            colors.disabled_indeterminate_box
        );
        // disabled 色值：OnSurface @ 38%
        assert_eq!(colors.disabled_checked_box.a, (theme.on_surface.a as f32 * 0.38) as u8);
        assert_eq!(colors.disabled_unchecked_border.a, (theme.on_surface.a as f32 * 0.38) as u8);
        assert_eq!(
            colors.disabled_indeterminate_box.a,
            (theme.on_surface.a as f32 * 0.38) as u8
        );
        // M3 CheckboxColors 无 disabled checkmark 字段：禁用勾号仍取
        // checkedCheckmarkColor = OnPrimary（与 1.4.0 源码一致）
        assert_eq!(colors.checkmark_color(ToggleableState::On), theme.on_primary);
    }

    #[test]
    fn checkbox_callback_receives_inverted_checked() {
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
    fn tri_state_on_click_fires() {
        use crate::modifier::ModifierElement;
        use std::sync::atomic::{AtomicUsize, Ordering};
        let clicks = Arc::new(AtomicUsize::new(0));
        let c = clicks.clone();
        let mut composer = crate::core::composer::Composer::new();
        composer.compose(|ctx| {
            TriStateCheckbox::new(ToggleableState::Indeterminate)
                .on_click(move || {
                    c.fetch_add(1, Ordering::Relaxed);
                })
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
        assert_eq!(clicks.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn disabled_checkbox_has_no_interaction_elements() {
        use crate::modifier::ModifierElement;
        for state in [ToggleableState::Off, ToggleableState::On, ToggleableState::Indeterminate] {
            let mut composer = crate::core::composer::Composer::new();
            composer.compose(|ctx| {
                TriStateCheckbox::new(state)
                    .enabled(false)
                    .on_click(|| {})
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
    }

    #[test]
    fn checked_checkmark_icon_tint_resolves_to_on_primary() {
        // 集成：On 时勾号/横线图标 tint = on_primary
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = crate::core::composer::Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                Checkbox::new(true).on_checked_change(|_| {}).build(ctx);
            });
        });
        composer.layout(crate::layout::Constraints::new(0.0, 200.0, 0.0, 200.0));
        let mut count = 0;
        for node in composer.arena_nodes() {
            for el in node.modifier.elements() {
                if let crate::modifier::ModifierElement::DrawIcon { spec } = el {
                    assert_eq!(spec.tint, Some(theme.on_primary), "勾号 tint = OnPrimary");
                    count += 1;
                }
            }
        }
        assert_eq!(count, 2, "On 状态同时挂 check 与 dash 两个图标（动画节点）");
    }

    #[test]
    fn unchecked_has_transparent_checkmark_and_no_fill() {
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let colors = CheckboxColors::from_theme(&theme);
        assert_eq!(colors.unchecked_box.a, 0);
        assert_eq!(colors.checkmark_color(ToggleableState::Off).a, 0);
        assert_eq!(colors.checked_box, theme.primary);
    }

    #[test]
    fn uncheck_keeps_primary_pixels_during_transition() {
        // 回归：取消选中时容器/勾号必须从 Primary 渐变消失（状态切换后的
        // 第一个渲染帧仍可见），而不是瞬间切透明——否则勾号缩放退出动画
        // 被吞掉，表现为“取消选中没有动画”。
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
                let c = ctx.remember(|| true);
                holder.replace(Some(c.clone()));
                Checkbox::new(c.get()).on_checked_change(|_| {}).build(ctx);
            });
        };
        let mut render = |composer: &mut crate::core::composer::Composer| -> Vec<u8> {
            composer.compose(build_scene);
            composer.compose(build_scene);
            composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
            let mut surface = surfaces::raster_n32_premul((300, 300)).unwrap();
            let canvas = surface.canvas();
            canvas.clear(Color::WHITE);
            let root = composer.layout_root_idx().expect("root");
            let nodes = composer.arena_nodes();
            crate::render::render(nodes, root, canvas);
            let pm = surface.peek_pixels().expect("pixmap");
            let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
            px.iter()
                .flat_map(|p| [p[0], p[1], p[2]])
                .collect::<Vec<_>>()
        };
        let count_primary = |px: &[u8]| {
            px.chunks_exact(3)
                // skia N32 premul 内存序为 BGRA：flat_map 取前 3 字节 = B,G,R
                .filter(|rgb| rgb[2] > 80 && rgb[0] as i32 > rgb[2] as i32 + 20)
                .count()
        };
        composer.compose(build_scene);
        composer.layout(crate::layout::Constraints::new(0.0, 300.0, 0.0, 300.0));
        let c = holder.borrow().clone().expect("state");
        let checked_px = render(&mut composer);
        assert!(
            count_primary(&checked_px) > 50,
            "选中态应有 Primary 容器像素"
        );
        // 取消选中：状态立即切 Off，但颜色/勾号动画尚未推进——过渡帧必须
        // 仍保留 Primary 像素（修复前容器/勾号瞬切透明 → 这里为 0）
        c.update(|s| *s = false);
        let transition_px = render(&mut composer);
        let transition_primary = count_primary(&transition_px);
        assert!(
            transition_primary > 50,
            "取消选中过渡帧应保留 Primary 像素（实际 {transition_primary}）——颜色瞬切透明吞掉退出动画"
        );
        // 动画推完 → 回到未选中静止态：不再有 Primary 像素
        for _ in 0..400 {
            if !crate::animation::update_animations() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let settled_px = render(&mut composer);
        assert!(
            count_primary(&settled_px) < 5,
            "未选中静止态不应再有 Primary 像素"
        );
    }
}
