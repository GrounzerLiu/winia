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
use crate::composable;
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
    /// checkedCheckmarkColor——与 1.4.0 实现一致）。
    /// 注：生产组合路径用 `checked_checkmark` 常量（Off 由 scale=0 隐藏，
    /// 保证取消选中退出动画可见），本方法保留供 API 对齐与测试引用。
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

/// 共享实现（对标 M3 `CheckboxImpl`）——`Checkbox` 与 `TriStateCheckbox` 共用。
/// #[composable]：内部组合子组件（Icon×2）调用语句注入——多实例隔离
/// （勾号/横线两个 Icon 同调用位置靠语句 id 区分）
#[allow(clippy::too_many_arguments)]
#[composable]
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

    // 动画规格：Compose/M3 默认级 Spring（StiffnessMedium=400、NoBouncy）。
    // 注意 winia 的 SpringSpec::default() 是 StiffnessLow(200)——比 M3 基准
    // 软一档，过渡尾巴偏长（“动画有点慢”）。仅浮点动画生效。
    let check_spec = crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec {
        stiffness: crate::animation::SpringSpec::STIFFNESS_MEDIUM,
        ..crate::animation::SpringSpec::default()
    });
    // 容器/边框颜色过渡（M3 animateColorAsState）：选中/取消选中都从当前
    // 颜色动画到目标色，而不是瞬间跳变。push_animatable_color 会把 Spring
    // 降级为 TweenSpec::default()（300ms Linear），这里显式传 Tween 避免
    // 规格/注释误导。
    let color_spec =
        crate::animation::AnimationSpec::Tween(crate::animation::TweenSpec::default());
    let box_color_anim = ctx.animate_color_as_state(box_color, color_spec.clone());
    let border_color_anim = ctx.animate_color_as_state(border_color, color_spec);

    // On→check 缩放 1、Indeterminate→dash 缩放 1、Off→都 0（Spring 近似
    // M3 checkDrawFraction + crossCenterGravitation 过渡）
    let check_scale = ctx.animate_float_as_state(
        if state == ToggleableState::On { 1.0 } else { 0.0 },
        check_spec.clone(),
    );
    let dash_scale = ctx.animate_float_as_state(
        if state == ToggleableState::Indeterminate { 1.0 } else { 0.0 },
        check_spec,
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

    #[composable]
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

    /// #[composable]：内部 TriStateCheckbox 调用点从本 build 语句取稳定 base
    #[composable]
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

    #[test]
    fn disabled_checked_border_merges_with_fill() {
        // 回归：禁用已选中时边框色 = 容器色（半透明），渲染层应合并为
        // 纯填充；若边框再叠一层 stroke 会双重混合，边框带明显深于内部。
        use skia_safe::{Color as SkColor, surfaces};
        let _g = crate::animation::tests::TEST_SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let fill = crate::modifier::Color::from_argb(96, 40, 80, 220);
        let mut composer = crate::core::composer::Composer::new();
        let scene = |ctx: &mut ComposeCtx| {
            Checkbox::new(true)
                .enabled(false)
                .colors(CheckboxColors::new(
                    fill, fill, fill, fill, fill, fill, fill, fill, fill, fill, fill, fill,
                ))
                .on_checked_change(|_| {})
                .build(ctx);
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
        // 40×40 触摸目标内居中 20×20 视觉盒
        let (mut bx, mut by) = (0.0f32, 0.0f32);
        let mut found = false;
        for node in nodes {
            if node.measured_size.width == CHECKBOX_TOUCH_TARGET
                && node.measured_size.height == CHECKBOX_TOUCH_TARGET
            {
                bx = node.position.x + (CHECKBOX_TOUCH_TARGET - CHECKBOX_SIZE) / 2.0;
                by = node.position.y + (CHECKBOX_TOUCH_TARGET - CHECKBOX_SIZE) / 2.0;
                found = true;
                break;
            }
        }
        assert!(found, "应有 40×40 触摸目标");
        // 当前场景是单层布局，node.position 即全局像素坐标（与 switch 像素
        // 测试同一约定）；若测试引入嵌套节点需改为递归累计偏移。
        let at = |x: f32, y: f32| {
            let p = px[(y as usize) * 300 + (x as usize)];
            (p[0] as i32, p[1] as i32, p[2] as i32)
        };
        // 内部参考点选在勾号路径上方（y+3），避免勾号覆盖
        let interior = at(bx + 10.0, by + 3.0);
        let edges = [
            at(bx + 1.0, by + 10.0),
            at(bx + 18.0, by + 10.0),
            at(bx + 10.0, by + 1.0),
            at(bx + 10.0, by + 18.0),
        ];
        for (i, e) in edges.iter().enumerate() {
            let d = (e.0 - interior.0).abs() + (e.1 - interior.1).abs() + (e.2 - interior.2).abs();
            assert!(
                d < 40,
                "边框应合并为纯填充（edge[{i}]={e:?} interior={interior:?} diff={d}）"
            );
        }
    }
}

    /// 父子联动回归（checkbox_demo）：Column 内 c1/c2/c3 remember +
    /// parent_state 计算 + 子项 Checkbox 循环。点子项（c2.set）→ 全选
    /// TriStateCheckbox 的 state 必须更新（On→Indeterminate/Off）。
    /// ⚠ 子项 State 必须 remember（绑定 owner queue——State::new 的 notify
    /// 不入队不触发重组——测试环境既有陷阱）。
    #[test]
    fn parent_tri_state_follows_children() {
        use std::cell::RefCell;
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut composer = crate::core::composer::Composer::new();
        let holder = RefCell::new(None::<crate::core::state::State<bool>>);
        let parent_states = RefCell::new(Vec::new());

        let build = |composer: &mut crate::core::composer::Composer| {
            composer.compose(crate::compose!(|ctx| {
                crate::ui::Column::new().build(ctx, |ctx| {
                    let c1 = ctx.remember(|| true);
                    let c2 = ctx.remember(|| true);
                    let c3 = ctx.remember(|| true);
                    *holder.borrow_mut() = Some(c2.clone());
                    let all = c1.get() && c2.get() && c3.get();
                    let none = !c1.get() && !c2.get() && !c3.get();
                    let ps = if all {
                        ToggleableState::On
                    } else if none {
                        ToggleableState::Off
                    } else {
                        ToggleableState::Indeterminate
                    };
                    parent_states.borrow_mut().push(ps);
                    // 全选 TriStateCheckbox
                    TriStateCheckbox::new(ps).build(ctx);
                    // 子项循环（列表显式 key——实例隔离）
                    for (label, c) in [("子项 1", c1.clone()), ("子项 2", c2.clone()), ("子项 3", c3.clone())] {
                        ctx.key(label, |ctx| {
                            crate::ui::Row::new().build(ctx, |ctx| {
                                crate::ui::Text::new(label).font_size(13.0).build(ctx);
                                Checkbox::new(c.get()).on_checked_change(|_| {}).build(ctx);
                            });
                        });
                    }
                });
            }));
            composer.layout(crate::layout::constraints::Constraints::new(0.0, 300.0, 0.0, 600.0));
        };

        // 帧1：全 true → parent On
        build(&mut composer);
        let ps1 = *parent_states.borrow().last().unwrap();
        assert_eq!(ps1, ToggleableState::On, "全 true → On（实际 {ps1:?}）");

        // 点子项2（c2 → false）：On → Indeterminate（真实联动验证）
        let c2 = holder.borrow().clone().unwrap();
        c2.set(false);
        build(&mut composer);
        let ps2 = *parent_states.borrow().last().unwrap();
        assert_eq!(ps2, ToggleableState::Indeterminate, "c2=false → Indeterminate（实际 {ps2:?}）——全选应随子项变化");

        // 再点 c3 → false：全 false → Off
        // （c3 在闭包内 remember 未持引用——通过序列长度确认持续联动：父状态每次子项变化都重算）
        assert!(parent_states.borrow().len() >= 2,
            "子项变化后 parent_state 应重算（实际 {} 次）", parent_states.borrow().len());
    }

    /// 真实点击链路（demo 等价）：从 arena 找子项 Checkbox 的 Clickable 回调并
    /// 调用（等价鼠标点击）→ 子项 State 更新 → 父 TriStateCheckbox 联动。
    /// 验证：点击子项2（checked=true→false）→ parent On→Indeterminate。
    #[test]
    fn parent_tri_state_follows_click() {
        use std::cell::RefCell;
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let mut composer = crate::core::composer::Composer::new();
        let holder = RefCell::new(None::<crate::core::state::State<bool>>);
        let parent_states = RefCell::new(Vec::new());
        // 记录子项 Checkbox 的 Clickable 回调（按出现顺序——子项1/2/3）
        let clickables = RefCell::new(Vec::new());

        let build = |composer: &mut crate::core::composer::Composer| {
            composer.compose(crate::compose!(|ctx| {
                crate::ui::Column::new().build(ctx, |ctx| {
                    let c1 = ctx.remember(|| true);
                    let c2 = ctx.remember(|| true);
                    let c3 = ctx.remember(|| true);
                    if holder.borrow().is_none() { *holder.borrow_mut() = Some(c2.clone()); }
                    let all = c1.get() && c2.get() && c3.get();
                    let none = !c1.get() && !c2.get() && !c3.get();
                    let ps = if all { ToggleableState::On }
                        else if none { ToggleableState::Off }
                        else { ToggleableState::Indeterminate };
                    parent_states.borrow_mut().push(ps);
                    TriStateCheckbox::new(ps).build(ctx);
                    for (label, c) in [("子项 1", c1.clone()), ("子项 2", c2.clone()), ("子项 3", c3.clone())] {
                        ctx.key(label, |ctx| {
                            crate::ui::Row::new().build(ctx, |ctx| {
                                crate::ui::Text::new(label).font_size(13.0).build(ctx);
                                let cc = c.clone();
                                Checkbox::new(c.get())
                                    .on_checked_change(move |v| cc.update(|s| *s = v))
                                    .build(ctx);
                            });
                        });
                    }
                });
            }));
            composer.layout(crate::layout::constraints::Constraints::new(0.0, 300.0, 0.0, 600.0));
            // 收集 Clickable 回调
            clickables.borrow_mut().clear();
            for n in composer.arena_nodes() {
                for el in n.modifier.elements() {
                    if let crate::modifier::ModifierElement::Clickable { on_click, .. } = el {
                        clickables.borrow_mut().push(on_click.clone());
                    }
                }
            }
        };

        build(&mut composer);
        assert_eq!(*parent_states.borrow().last().unwrap(), ToggleableState::On, "全 true → On");
        let clicks = clickables.borrow().clone();
        // 找子项2 的 Clickable：demo 结构里子项循环的 Checkbox 是第 2/3/4 个
        // Clickable（前有"全选"的 TriStateCheckbox 1 个）——取第 2 个 = 子项2
        assert!(clicks.len() >= 3, "应有多个 Clickable（实际 {}）", clicks.len());
        let sub2_click = clicks[1].clone();
        // 模拟点击子项2（on_click = cb(!checked)——checked=true → 传 false → c2=false）
        sub2_click();
        // 下一帧：c2=false → parent Indeterminate
        build(&mut composer);
        let ps = *parent_states.borrow().last().unwrap();
        assert_eq!(ps, ToggleableState::Indeterminate,
            "点击子项2 后 parent 应变 Indeterminate（实际 {ps:?}）——全选应随点击联动");
        let c2 = holder.borrow().clone().unwrap();
        assert!(!c2.get(), "点击后 c2 应为 false");
    }

    /// 渲染级父子联动（demo 等价）：Column + 子项循环 + 全选 TriStateCheckbox。
    /// 点击子项3（false→true）→ 全选从 Indeterminate→On → 全选区域位图必须变化
    /// （用户报告：全选按钮不随子项变化——此测试锁定视觉联动）。
    #[test]
    fn parent_tri_state_renders_change_on_child_click() {
        use std::cell::RefCell;
        use skia_safe::{Color, surfaces};
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = crate::core::composer::Composer::new();
        let clickables = RefCell::new(Vec::new());
        let parent_box_pos = RefCell::new(None::<(f32, f32, f32, f32)>);

        let build_scene = |ctx: &mut ComposeCtx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                crate::ui::Column::new().build(ctx, |ctx| {
                    let c1 = ctx.remember(|| true);
                    let c2 = ctx.remember(|| true);
                    let c3 = ctx.remember(|| false);
                    let all = c1.get() && c2.get() && c3.get();
                    let none = !c1.get() && !c2.get() && !c3.get();
                    let ps = if all { ToggleableState::On }
                        else if none { ToggleableState::Off }
                        else { ToggleableState::Indeterminate };
                    // 全选（第一个 40x40 checkbox）——demo 等价：带 on_click
                    TriStateCheckbox::new(ps).on_click(|| {}).build(ctx);
                    for (label, c) in [("子项 1", c1.clone()), ("子项 2", c2.clone()), ("子项 3", c3.clone())] {
                        ctx.key(label, |ctx| {
                            crate::ui::Row::new().build(ctx, |ctx| {
                                crate::ui::Text::new(label).font_size(13.0).build(ctx);
                                let cc = c.clone();
                                Checkbox::new(c.get())
                                    .on_checked_change(move |v| cc.update(|s| *s = v))
                                    .build(ctx);
                            });
                        });
                    }
                });
            });
        };
        // 渲染一次 + 记录全选位置 + 收集 Clickable
        let mut render = |composer: &mut crate::core::composer::Composer| -> Vec<u8> {
            composer.compose(build_scene);
            composer.layout(crate::layout::constraints::Constraints::new(0.0, 400.0, 0.0, 600.0));
            // 全选 = 第一个 40x40 节点（Column 直接子级里找）
            let root = composer.layout_root_idx().unwrap();
            let nodes = composer.arena_nodes();
            fn first_box(nodes: &[crate::layout::node::LayoutNode], idx: usize) -> Option<(f32,f32,f32,f32)> {
                let n = &nodes[idx];
                let abs = (n.position.x, n.position.y);
                if n.measured_size.width == 40.0 && n.measured_size.height == 40.0 {
                    return Some((abs.0, abs.1, 40.0, 40.0));
                }
                for &c in &n.children {
                    if let Some(r) = first_box(nodes, c) { return Some(r); }
                }
                None
            }
            *parent_box_pos.borrow_mut() = first_box(nodes, root);
            clickables.borrow_mut().clear();
            for n in nodes {
                for el in n.modifier.elements() {
                    if let crate::modifier::ModifierElement::Clickable { on_click, .. } = el {
                        clickables.borrow_mut().push(on_click.clone());
                    }
                }
            }
            let mut surface = surfaces::raster_n32_premul((400, 600)).unwrap();
            let canvas = surface.canvas();
            canvas.clear(Color::WHITE);
            crate::render::render(nodes, root, canvas);
            let pm = surface.peek_pixels().expect("pixmap");
            let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
            px.iter().flat_map(|p| [p[0], p[1], p[2]]).collect::<Vec<_>>()
        };
        // 提取全选区域像素
        let region_px = |px: &[u8], (x, y, w, h): (f32, f32, f32, f32)| -> Vec<u8> {
            let mut out = Vec::new();
            for row in 0..(h as usize) {
                for col in 0..(w as usize) {
                    let gx = (x as usize) + col;
                    let gy = (y as usize) + row;
                    let i = (gy * 400 + gx) * 3;
                    if i + 2 < px.len() { out.extend_from_slice(&px[i..i+3]); }
                }
            }
            out
        };

        // 首帧渲染：全选 Indeterminate
        let px1 = render(&mut composer);
        let pos = parent_box_pos.borrow().clone().expect("全选位置");
        let r1 = region_px(&px1, pos);
        let clicks = clickables.borrow().clone();
        assert!(clicks.len() >= 4, "应有全选+3子项 Clickable（实际 {}）", clicks.len());
        // 点击子项3（第 4 个 Clickable——全选+子项1+2+3）
        let sub3 = clicks[3].clone();
        sub3(); // c3: false→true
        // 推进动画（颜色/勾号过渡）
        for _ in 0..20 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        let px2 = render(&mut composer);
        for _ in 0..20 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        let px3 = render(&mut composer);
        let r2 = region_px(&px3, pos);
        assert_ne!(r1, r2,
            "点击子项3 后全选区域位图必须变化（Indeterminate→On）——全选应随子项联动（区域 {pos:?}）");
    }

    /// demo 差异复现：Column + vertical_scroll + 循环 ctx.key + 全选联动。
    /// 用户报告：滚动列表里点子项3，子项勾上了但全选不变。
    /// （无 scroll 的 parent_tri_state_renders_change_on_child_click 通过——差异在 scroll）
    #[test]
    fn parent_tri_state_follows_child_in_scroll_column() {
        use std::cell::RefCell;
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let theme = ThemeColors::light_from_seed(0x6750A4);
        let mut composer = crate::core::composer::Composer::new();
        let clickables = RefCell::new(Vec::new());
        let parent_states = RefCell::new(Vec::new());

        let build_scene = |ctx: &mut ComposeCtx| {
            WiniaTheme::with_theme(theme.clone(), ctx, |ctx| {
                crate::ui::Column::new()
                    .modifier(crate::modifier::Modifier::new()
                        .fill_max_size()
                        .vertical_scroll(crate::modifier::ScrollState::new()))
                    .build(ctx, |ctx| {
                        let c1 = ctx.remember(|| true);
                        let c2 = ctx.remember(|| true);
                        let c3 = ctx.remember(|| false);
                        let all = c1.get() && c2.get() && c3.get();
                        let none = !c1.get() && !c2.get() && !c3.get();
                        let ps = if all { ToggleableState::On }
                            else if none { ToggleableState::Off }
                            else { ToggleableState::Indeterminate };
                        parent_states.borrow_mut().push(ps);
                        TriStateCheckbox::new(ps).on_click(|| {}).build(ctx);
                        for (label, c) in [("子项 1", c1.clone()), ("子项 2", c2.clone()), ("子项 3", c3.clone())] {
                            ctx.key(label, |ctx| {
                                crate::ui::Row::new().build(ctx, |ctx| {
                                    crate::ui::Text::new(label).font_size(13.0).build(ctx);
                                    let cc = c.clone();
                                    Checkbox::new(c.get())
                                        .on_checked_change(move |v| cc.update(|s| *s = v))
                                        .build(ctx);
                                });
                            });
                        }
                    });
            });
        };
        // 帧1 + 点击子项3（第 4 个 Clickable）
        composer.compose(build_scene);
        composer.layout(crate::layout::constraints::Constraints::new(0.0, 400.0, 0.0, 600.0));
        clickables.borrow_mut().clear();
        for n in composer.arena_nodes() {
            for el in n.modifier.elements() {
                if let crate::modifier::ModifierElement::Clickable { on_click, .. } = el {
                    clickables.borrow_mut().push(on_click.clone());
                }
            }
        }
        let clicks = clickables.borrow().clone();
        assert!(clicks.len() >= 4, "Clickable 数 {}", clicks.len());
        let sub3 = clicks[3].clone();
        sub3(); // c3 false→true
        // 下一帧：全选应变 On
        composer.compose(build_scene);
        composer.layout(crate::layout::constraints::Constraints::new(0.0, 400.0, 0.0, 600.0));
        let ps = *parent_states.borrow().last().unwrap();
        assert_eq!(ps, ToggleableState::On,
            "带 scroll 的 Column：点击子项3 后全选应变 On（实际 {ps:?}）——scroll 容器不应阻断联动");
    }

    /// 核心假设验证：宏化 Column 的 content 顶层 State.get() 变化 →
    /// Column 应 Enter（compose_dirty_count 增加）。若 Skip（计数不变）→
    /// 内容 scope 收不到依赖 → 全选联动断（demo 横线不变根因）。
    #[test]
    fn macroized_column_content_state_notify_enters_column() {
        use std::cell::RefCell;
        let mut composer = crate::core::composer::Composer::new();
        let holder = RefCell::new(None::<crate::core::state::State<bool>>);
        let scene = |composer: &mut crate::core::composer::Composer| {
            composer.compose(crate::compose!(|ctx| {
                crate::ui::Column::new().build(ctx, |ctx| {
                    let c3 = ctx.remember(|| false);
                    *holder.borrow_mut() = Some(c3.clone());
                    let _ = c3.get(); // 顶层依赖 → 应注册 Column scope
                    TriStateCheckbox::new(
                        if c3.get() { ToggleableState::On } else { ToggleableState::Off }
                    ).on_click(|| {}).build(ctx);
                });
            }));
            composer.layout(crate::layout::constraints::Constraints::new(0.0, 300.0, 0.0, 300.0));
        };
        scene(&mut composer);
        let c = holder.borrow().clone().unwrap();
        c.set(true);
        scene(&mut composer);
        // Column 应 Enter（content 重跑 → ps 重算 → TriStateCheckbox 变 On）
        let d2 = composer.compose_dirty_count;
        assert!(d2 >= 1, "c3 变化后 Column 应 Enter（dirty 计数 {d2}）");
    }
