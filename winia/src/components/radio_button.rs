//! RadioButton 组件 — 对标 material3 `RadioButton`（M3 1.4.0 / Compose androidx-main）
//!
//! M3 实现要点（对齐项）：
//! - 视觉 20×20（`RadioButtonTokens.IconSize`）：外圈 2dp 圆环描边 +
//!   选中时内实心圆点（半径 = DotSize/2 - 描边/2 = 5dp，`RadioButtonDotSize = 12dp`）；
//! - 触摸目标/状态层 40×40（`StateLayerSize`），波纹 unbounded（半径 20）；
//! - 选中动画：内点半径 0→5dp（Compose `animateDpAsState(FastSpatial)`）——
//!   本框架用 graphics_layer scale 0→1 近似（半径比例动画视觉等价）；
//!   颜色动画 `animateColorAsState(DefaultEffects)`——Tween 300ms；
//! - 颜色只分 enabled×selected（`RadioButtonColors` 四字段，无 hover/focus/press
//!   变体——交互反馈由 ripple/state layer 承担）；
//! - `onClick = null` → 不可交互（不挂 clickable），与 Compose 语义一致。

use crate::runtime::composer::{ComposeCtx, GroupStatus};
use crate::composable;
use crate::layout::BoxLayout;
use crate::modifier::{Modifier};
use crate::graphics::{Color, GraphicsLayerParams, Shape};
use crate::interaction::MutableInteractionSource;
use crate::theme::{ThemeColors, WiniaTheme};
use std::sync::Arc;

/// 视觉图标尺寸（`RadioButtonTokens.IconSize = 20dp`）
pub const RADIO_BUTTON_SIZE: f32 = 20.0;
/// The node a CLICKABLE radio occupies: `Modifier.minimumInteractiveComponentSize()` = 48 dp, with the 20 dp
/// icon centred inside (`material3/RadioButton.kt:100-115`; applied whenever `onClick != null`, disabled or not).
pub const RADIO_TOUCH_TARGET: f32 = 48.0;
/// `RadioButtonTokens.StateLayerSize = 40dp` — the DIAMETER of the hover/focus halo, which is what Compose hands
/// to `ripple` as its radius. It is smaller than the node, so it is named rather than derived from the node's
/// diagonal.
pub const RADIO_STATE_LAYER_SIZE: f32 = 40.0;
/// The node a radio WITHOUT a click handler occupies: the 20 dp icon plus Compose's 2 dp padding on each side
/// (`RadioButtonPadding`, `material3/RadioButton.kt:264`), and no minimum interactive size.
pub const RADIO_IDLE_SIZE: f32 = RADIO_BUTTON_SIZE + 4.0;
/// 圆环描边宽度（Compose `RadioStrokeWidth = 2dp`——对齐 M3 stroke 规格）
pub const RADIO_STROKE_WIDTH: f32 = 2.0;
/// 选中内点直径（Compose `RadioButtonDotSize = 12dp`；绘制半径 =
/// DotSize/2 - 描边/2 = 5dp——外圈内缘 8dp 与内点 5dp 间 3dp 间隙）
pub const RADIO_DOT_SIZE: f32 = 12.0;

/// 单选按钮颜色集（对标 material3 `RadioButtonColors`）——selected/unselected ×
/// enabled/disabled 四组色。M3 `RadioButtonColors` 无 hover/focus/press 变体。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RadioButtonColors {
    pub selected_color: Color,
    pub unselected_color: Color,
    pub disabled_selected_color: Color,
    pub disabled_unselected_color: Color,
}

impl RadioButtonColors {
    pub fn new(
        selected_color: Color,
        unselected_color: Color,
        disabled_selected_color: Color,
        disabled_unselected_color: Color,
    ) -> Self {
        Self {
            selected_color,
            unselected_color,
            disabled_selected_color,
            disabled_unselected_color,
        }
    }

    /// 从主题推导默认色（对标 `RadioButtonDefaults.colors()`：
    /// `RadioButtonTokens` v0_117）
    pub fn from_theme(theme: &ThemeColors) -> Self {
        let alpha = |c: Color, a: f32| Color::from_argb((c.a as f32 * a) as u8, c.r, c.g, c.b);
        Self::new(
            theme.primary,
            theme.on_surface_variant,
            // Disabled：OnSurface @ 0.38（Disabled*IconOpacity = 0.38f）
            alpha(theme.on_surface, 0.38),
            alpha(theme.on_surface, 0.38),
        )
    }

    /// Override some of the four colours and keep the rest — Compose's `RadioButtonColors.copy(...)`, whose
    /// KDoc says `Color.Unspecified` "means 'use the value from the source'" (`material3/RadioButton.kt:200-203`),
    /// and the implementation `RadioButtonDefaults.colors(...)` is built on (`:149-161`, every parameter
    /// defaulting to `Color.Unspecified`).
    ///
    /// Compose spells "not given" as `Color.Unspecified`, a colour carrying an unspecified colour SPACE; winia's
    /// `Color` is an 8-bit sRGB value with no such state (`TRANSPARENT` is a real colour, not a placeholder), so
    /// a gap is `None` here. Start from the token set and override what you need:
    ///
    /// ```ignore
    /// RadioButtonDefaults::radio_button_colors(&theme).copy(Some(my_colour), None, None, None)
    /// ```
    pub fn copy(
        &self,
        selected_color: Option<Color>,
        unselected_color: Option<Color>,
        disabled_selected_color: Option<Color>,
        disabled_unselected_color: Option<Color>,
    ) -> Self {
        Self {
            selected_color: selected_color.unwrap_or(self.selected_color),
            unselected_color: unselected_color.unwrap_or(self.unselected_color),
            disabled_selected_color: disabled_selected_color.unwrap_or(self.disabled_selected_color),
            disabled_unselected_color: disabled_unselected_color.unwrap_or(self.disabled_unselected_color),
        }
    }

    /// 按 enabled×selected 解析当前色（对标 M3 `radioColor(enabled, selected)`）
    pub fn radio_color(&self, enabled: bool, selected: bool) -> Color {
        if !enabled {
            if selected {
                self.disabled_selected_color
            } else {
                self.disabled_unselected_color
            }
        } else if selected {
            self.selected_color
        } else {
            self.unselected_color
        }
    }
}

/// RadioButton 默认值（对标 material3 `RadioButtonDefaults`）
pub struct RadioButtonDefaults;

impl RadioButtonDefaults {
    pub fn radio_button_colors(theme: &ThemeColors) -> RadioButtonColors {
        RadioButtonColors::from_theme(theme)
    }

    /// 状态层/视觉形状（RadioButtonTokens 为 Circle——与 checkbox 的
    /// CheckboxDefaults::shape() 对应）
    pub fn shape() -> Shape {
        Shape::Circle
    }
}

/// The radio's colour spec: M3 animates it with `animateColorAsState(target,
/// MotionSchemeKeyTokens.DefaultEffects.value())` (`material3/RadioButton.kt:237`) — a critically damped
/// spring (1.0 / 1600), not a 300 ms linear tween, which is what this component used while the engine
/// could not put a colour on a spring at all. Named so the token is assertable.
fn radio_colour_spec() -> crate::animation::SpringSpec {
    crate::animation::SpringSpec::effects_default()
}

/// The dot's appearance spec: M3 animates the dot radius with the `FastSpatial` token
/// (`material3/RadioButton.kt:86`: `animateDpAsState(targetValue = … RadioButtonDotSize / 2 else 0.dp,
/// animationSpec = MotionSchemeKeyTokens.FastSpatial.value())`) — damping 0.9, stiffness 1400
/// (`tokens/StandardMotionTokens.kt:24-25`). winia used a no-bouncy spring at stiffness 400 and a
/// comment quoting "spring(600/30)", neither of which is this token.
fn radio_dot_spec() -> crate::animation::SpringSpec {
    crate::animation::SpringSpec::spatial_fast()
}

/// 共享实现（对标 M3 `RadioButtonImpl`）。
/// #[composable]：内部圆环/内点叶子节点语句注入——多实例隔离
#[allow(clippy::too_many_arguments)]
#[composable]
fn radio_button_impl(
    ctx: &mut ComposeCtx,
    selected: bool,
    on_click: Option<Arc<dyn Fn() + Send + Sync>>,
    enabled: bool,
    colors: Option<RadioButtonColors>,
    interaction_source: Option<MutableInteractionSource>,
    modifier: Modifier,
) {
    ctx.changed(&selected);
    ctx.changed(&enabled);
    ctx.changed(&colors);
    let key = ctx.next_key();
    let theme = WiniaTheme::colors();
    let colors = colors.unwrap_or_else(|| RadioButtonDefaults::radio_button_colors(&theme));
    let interaction = interaction_source
        .unwrap_or_else(|| ctx.remember(|| MutableInteractionSource::new()).get());
    let color = colors.radio_color(enabled, selected);

    // The radio's colour, per M3 — see `radio_colour_spec`. A DISABLED radio snaps instead:
    // `RadioButton.kt:146-154` returns the disabled colour through `rememberUpdatedState` and says why —
    // "If not enabled 'snap' to the disabled state, as there should be no animations between enabled /
    // disabled". Animating it unconditionally made the switch to disabled read as a slow fade.
    //
    // Note the DOT is different: upstream's `animateDpAsState` for it has no enabled branch, so it keeps
    // its spring in both states, and so does this.
    let color_spec = if enabled {
        crate::animation::AnimationSpec::Spring(radio_colour_spec())
    } else {
        crate::animation::AnimationSpec::Snap(crate::animation::SnapSpec::default())
    };
    let color_anim = ctx.animate_color_as_state(color, color_spec);

    // The dot's appearance, which M3 animates with the `FastSpatial` token — see `radio_dot_spec`.
    let dot_scale = ctx.animate_float_as_state(
        if selected { 1.0 } else { 0.0 },
        crate::animation::AnimationSpec::Spring(radio_dot_spec()),
    );

    let mut m = Modifier::new()
        // Size: Compose applies `Modifier.minimumInteractiveComponentSize()` in front of the selectable
        // modifier ONLY when the radio takes clicks (`material3/RadioButton.kt:100-115`), so an interactive
        // radio is a 48 dp node with its 20 dp icon centred, and one without a click handler is just the icon
        // plus Compose's 2 dp padding. winia used `StateLayerSize` (40 dp) for both — the same 8 dp-short
        // target the checkbox had.
        .size(
            if on_click.is_some() { RADIO_TOUCH_TARGET } else { RADIO_IDLE_SIZE },
            if on_click.is_some() { RADIO_TOUCH_TARGET } else { RADIO_IDLE_SIZE },
        )
        // The ring's SHAPE, carried without clipping: the renderer infers it from the nearest
        // Background/Border/Clip and strokes the ring OUTSIDE the node's box, so a `clip` here cut the ring away
        // (the reported "the focus ring is gone") — the same trap the Button's Text style hit. A zero-width
        // transparent border states the shape and clips nothing.
        .border(
            0.0,
            crate::graphics::Color::from_argb(0, 0, 0, 0),
            RadioButtonDefaults::shape(),
        );
    if enabled {
        if let Some(on_click) = on_click {
            m = m
                .clickable_with_source(&interaction, move || on_click())
                // `RadioButton.kt:97`: `ripple(bounded = false, radius = RadioButtonTokens.StateLayerSize / 2)`
                // — the halo is `StateLayerSize` (40 dp), SMALLER than the 48 dp node, so it has to be named
                // rather than derived from the node. The spreading layer still grows to the node's diagonal.
                .ripple_with_state_layer_size(
                    &interaction,
                    theme.on_surface,
                    false,
                    RADIO_STATE_LAYER_SIZE,
                );
        }
    }
    // Accessibility: a radio is SELECTED, not checked (Compose's `selectable` → `Selected`).
    m = m.semantics(
        crate::semantics::SemanticsConfig::new()
            .role(crate::semantics::SemanticsRole::RadioButton)
            .merge_descendants(true)
            .state(crate::semantics::SemanticsState::new().selected(selected).enabled(enabled)),
    );
    m = m.then(modifier);

    match ctx.start_restartable_group(
        key,
        m,
        BoxLayout::new().alignment(crate::layout::Alignment::Center),
    ) {
        GroupStatus::Skip => {}
        GroupStatus::Enter => {
            // Visual 20×20: the ring (a 2dp Circle border - the outer circle's centre-line
            // (20-2)/2 = 9dp，与 Compose drawCircle(radius = IconSize/2 -
            // strokeWidth/2, Stroke) 一致）+ 内点（background 实心圆）。
            let ring_color = color_anim.clone();
            let visual = Modifier::new()
                .size(RADIO_BUTTON_SIZE, RADIO_BUTTON_SIZE)
                .border(
                    RADIO_STROKE_WIDTH,
                    move || ring_color.peek(),
                    Shape::Circle,
                );
            let vkey = ctx.next_key();
            match ctx.start_restartable_group(
                vkey,
                visual,
                BoxLayout::new().alignment(crate::layout::Alignment::Center),
            ) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    // 内点：10×10 实心圆（半径 5 = DotSize/2 - 描边/2），
                    // scale 0→1 动画（Compose 半径 0→5dp 动画的视觉等价——
                    // 同心圆从 0 放大到最终半径）。未选中静止态 scale=0 隐藏。
                    let k = ctx.next_key();
                    let scale = dot_scale.clone();
                    let c = color_anim.clone();
                    ctx.start_leaf(
                        k,
                        Modifier::new()
                            .size(RADIO_DOT_SIZE - RADIO_STROKE_WIDTH, RADIO_DOT_SIZE - RADIO_STROKE_WIDTH)
                            .background(move || c.peek(), Shape::Circle)
                            .graphics_layer(move || GraphicsLayerParams {
                                scale_x: scale.peek(),
                                scale_y: scale.peek(),
                                ..Default::default()
                            }),
                    );
                    ctx.end_node();
                }
            }
            ctx.end_restartable_group();
        }
    }
    ctx.set_current_node_focus_color(theme.primary);
    ctx.end_restartable_group();
}

/// RadioButton 组件 Builder（对标 material3 `RadioButton(selected, onClick,
/// modifier, enabled, colors, interactionSource)`）
pub struct RadioButton {
    selected: bool,
    on_click: Option<Arc<dyn Fn() + Send + Sync>>,
    enabled: bool,
    colors: Option<RadioButtonColors>,
    interaction_source: Option<MutableInteractionSource>,
    modifier: Modifier,
}

impl RadioButton {
    pub fn new(selected: bool) -> Self {
        Self {
            selected,
            on_click: None,
            enabled: true,
            colors: None,
            interaction_source: None,
            modifier: Modifier::new(),
        }
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// 点击回调。None → 不可交互（不挂 clickable，对标 Compose onClick = null）。
    pub fn on_click(mut self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_click = Some(Arc::new(f));
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn colors(mut self, colors: RadioButtonColors) -> Self {
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
    pub fn build(self, ctx: &mut ComposeCtx) {
        radio_button_impl(
            ctx,
            self.selected,
            self.on_click,
            self.enabled,
            self.colors,
            self.interaction_source,
            self.modifier,
        );
    }

    pub fn get_selected(&self) -> bool {
        self.selected
    }
    pub fn get_enabled(&self) -> bool {
        self.enabled
    }
    pub fn get_colors(&self) -> Option<RadioButtonColors> {
        self.colors
    }
}


#[cfg(test)]
mod tests;
