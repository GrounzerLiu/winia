//! Chips — 输入/筛选/操作标签组件（对齐 Jetpack Compose material3 Chip 系列）
//!
//! 统一入口 `Chip` + 变体构造（`Chip::filter()` 等——对标 material3 的
//! AssistChip/FilterChip/InputChip/SuggestionChip 家族）：
//! - `Chip::assist(label, on_click)`：智能/自动化操作（可选 leading/trailing icon）
//! - `Chip::filter(selected, label, on_click)`：筛选（selected 切换）
//! - `Chip::input(selected, label, on_click)`：输入信息（leading/avatar/trailing）
//! - `Chip::suggestion(label, on_click)`：建议性操作（可选 icon）
//!
//! 视觉（M3 specs）：容器高 32dp、8dp 圆角、icon 18dp；左右 padding 无 icon 16 /
//! 有 icon 8；元素间距 8。Assist/Suggestion：transparent + 1dp outline 边框 +
//! onSurfaceVariant 文字 + primary icon；Filter/Input：selected = secondaryContainer
//! + onSecondaryContainer + 0 边框，unselected = transparent + outline 边框。

use crate::runtime::composer::{ComposeCtx, GroupStatus};
use crate::modifier::{Modifier};
use crate::graphics::{Shape};
use crate::components::text::ProvideTextStyle;

// ═══════════════════════════════════════════════════════════
// ChipColors — 非选择型配色（Assist/Suggestion）
// ═══════════════════════════════════════════════════════════

/// Chip 配色（Assist/Suggestion——非选择型；对标 material3 ChipColors）。
#[derive(Debug, Clone, Copy)]
pub struct ChipColors {
    pub container: crate::graphics::Color,
    pub label: crate::graphics::Color,
    pub leading_icon: crate::graphics::Color,
    pub trailing_icon: crate::graphics::Color,
    pub disabled_container: crate::graphics::Color,
    pub disabled_label: crate::graphics::Color,
}

impl ChipColors {
    pub fn new(
        container: crate::graphics::Color,
        label: crate::graphics::Color,
        leading_icon: crate::graphics::Color,
        trailing_icon: crate::graphics::Color,
        disabled_container: crate::graphics::Color,
        disabled_label: crate::graphics::Color,
    ) -> Self {
        Self { container, label, leading_icon, trailing_icon, disabled_container, disabled_label }
    }
}

// ═══════════════════════════════════════════════════════════
// SelectableChipColors — 选择型配色（Filter/Input）
// ═══════════════════════════════════════════════════════════

/// 选择型 chip 配色（Filter/Input——selected/unselected 双态；
/// 对标 material3 SelectableChipColors）。
#[derive(Debug, Clone, Copy)]
pub struct SelectableChipColors {
    pub container: crate::graphics::Color,
    pub label: crate::graphics::Color,
    pub leading_icon: crate::graphics::Color,
    pub trailing_icon: crate::graphics::Color,
    pub selected_container: crate::graphics::Color,
    pub selected_label: crate::graphics::Color,
    pub selected_leading_icon: crate::graphics::Color,
    pub selected_trailing_icon: crate::graphics::Color,
    pub disabled_container: crate::graphics::Color,
    pub disabled_label: crate::graphics::Color,
    pub disabled_selected_container: crate::graphics::Color,
}

impl SelectableChipColors {
    pub fn new(
        container: crate::graphics::Color,
        label: crate::graphics::Color,
        leading_icon: crate::graphics::Color,
        trailing_icon: crate::graphics::Color,
        selected_container: crate::graphics::Color,
        selected_label: crate::graphics::Color,
        selected_leading_icon: crate::graphics::Color,
        selected_trailing_icon: crate::graphics::Color,
        disabled_container: crate::graphics::Color,
        disabled_label: crate::graphics::Color,
        disabled_selected_container: crate::graphics::Color,
    ) -> Self {
        Self {
            container, label, leading_icon, trailing_icon,
            selected_container, selected_label, selected_leading_icon, selected_trailing_icon,
            disabled_container, disabled_label, disabled_selected_container,
        }
    }
}

// ═══════════════════════════════════════════════════════════
// ChipDefaults — 默认值（对齐 M3 specs token）
// ═══════════════════════════════════════════════════════════

pub struct ChipDefaults;

impl ChipDefaults {
    pub const HEIGHT: f32 = 32.0; // 容器高 32dp
    pub const ICON_SIZE: f32 = 18.0; // icon 18dp
    pub const CORNER_RADIUS: f32 = 8.0; // 形状 8dp 圆角
    pub const PADDING_NO_ICON: f32 = 16.0; // 无 icon 左右 padding 16dp
    pub const PADDING_WITH_ICON: f32 = 8.0; // 有 icon 左右 padding 8dp
    /// The specs page's input chip diagram: "left padding for avatar 4dp, right padding for avatar 8dp"
    /// (the avatar sits closer to the leading edge than an icon does).
    pub const INPUT_AVATAR_LEFT_PADDING: f32 = 4.0;
    pub const ELEMENT_GAP: f32 = 8.0; // 元素间距 8dp
    pub const BORDER_WIDTH: f32 = 1.0; // 边框 1dp
    pub const AVATAR_SIZE: f32 = 24.0; // InputChip avatar 24dp
    /// The specs page: "avatar shape 12dp corner radius" — a rounded square, NOT a circle.
    pub const AVATAR_CORNER_RADIUS: f32 = 12.0;

    /// `AssistChipDefaults.assistChipColors()` — the specs page's assist chip color roles: transparent
    /// container, label **On surface**, leading icon Primary, trailing icon On surface variant.
    pub fn assist_chip_colors(theme: &crate::theme::ThemeColors) -> ChipColors {
        ChipColors {
            container: crate::graphics::Color::TRANSPARENT,
            label: theme.on_surface,
            leading_icon: theme.primary,
            trailing_icon: theme.on_surface_variant,
            disabled_container: crate::graphics::Color::TRANSPARENT,
            disabled_label: crate::graphics::Color::from_argb(97, theme.on_surface.r, theme.on_surface.g, theme.on_surface.b),
        }
    }

    /// `SuggestionChipDefaults.suggestionChipColors()` — the same as the assist chip except that the label is
    /// **On surface variant** (the specs page lists that role for the suggestion chip).
    pub fn suggestion_chip_colors(theme: &crate::theme::ThemeColors) -> ChipColors {
        ChipColors {
            label: theme.on_surface_variant,
            ..Self::assist_chip_colors(theme)
        }
    }

    /// 选择型（Filter/Input）默认配色：unselected **transparent + outline variant 边框**
    /// + onSurfaceVariant；selected secondaryContainer（填充）+ onSecondaryContainer
    /// + 0 边框（有填充即无边框）
    pub fn selectable_chip_colors(theme: &crate::theme::ThemeColors) -> SelectableChipColors {
        SelectableChipColors {
            container: crate::graphics::Color::TRANSPARENT,
            label: theme.on_surface_variant,
            leading_icon: theme.primary,
            trailing_icon: theme.on_surface_variant,
            selected_container: theme.secondary_container,
            selected_label: theme.on_secondary_container,
            selected_leading_icon: theme.on_secondary_container,
            selected_trailing_icon: theme.on_secondary_container,
            disabled_container: crate::graphics::Color::TRANSPARENT,
            disabled_label: crate::graphics::Color::from_argb(97, theme.on_surface.r, theme.on_surface.g, theme.on_surface.b),
            disabled_selected_container: crate::graphics::Color::from_argb(97, theme.on_surface.r, theme.on_surface.g, theme.on_surface.b),
        }
    }

    /// `InputChipDefaults.inputChipColors()` — the filter chip's colours with one difference: the specs page
    /// gives an input chip's leading icon the **Primary** role, and `InputChipTokens.SelectedLeadingIconColor`
    /// says the same, so it stays Primary when the chip is selected (a filter chip's turns On secondary
    /// container).
    pub fn input_chip_colors(theme: &crate::theme::ThemeColors) -> SelectableChipColors {
        SelectableChipColors {
            selected_leading_icon: theme.primary,
            ..Self::selectable_chip_colors(theme)
        }
    }

    pub fn shape() -> Shape {
        Shape::RoundedRect { corner_radius: Self::CORNER_RADIUS }
    }
}

// ═══════════════════════════════════════════════════════════
// ChipVariant — 变体
// ═══════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChipVariant {
    /// 辅助操作（AssistChip）
    Assist,
    /// 筛选（FilterChip）
    Filter,
    /// 输入信息（InputChip）
    Input,
    /// 建议（SuggestionChip）
    Suggestion,
}

// ═══════════════════════════════════════════════════════════
// Chip — 统一入口（变体由构造方法决定）
// ═══════════════════════════════════════════════════════════

/// Chip 组件（对标 material3 AssistChip/FilterChip/InputChip/SuggestionChip）。
///
/// 用 `Chip::assist/filter/input/suggestion` 构造；视觉与默认值按变体：
/// - Assist/Suggestion：非选择型（ChipColors）——`colors(ChipColors)`
/// - Filter/Input：选择型（SelectableChipColors）——`selectable_colors(...)`
pub struct Chip {
    variant: ChipVariant,
    selected: bool,
    label: Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>,
    on_click: std::sync::Arc<dyn Fn() + Send + Sync>,
    modifier: Modifier,
    enabled: bool,
    leading_icon: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    trailing_icon: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    avatar: Option<Box<dyn FnOnce(&mut ComposeCtx) + Send + Sync>>,
    shape: Shape,
    /// 非选择型配色（assist/suggestion 用）
    flat_colors: Option<ChipColors>,
    /// 选择型配色（filter/input 用）
    selectable_colors: Option<SelectableChipColors>,
}

impl Chip {
    /// 辅助操作 chip（对标 AssistChip）
    pub fn assist(label: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static, on_click: impl Fn() + Send + Sync + 'static) -> Self {
        Self::new(ChipVariant::Assist, false, label, on_click)
    }

    /// 筛选 chip（对标 FilterChip）——selected 切换
    pub fn filter(selected: bool, label: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static, on_click: impl Fn() + Send + Sync + 'static) -> Self {
        Self::new(ChipVariant::Filter, selected, label, on_click)
    }

    /// 输入 chip（对标 InputChip）——selected 切换 + avatar/trailing 关闭按钮
    pub fn input(selected: bool, label: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static, on_click: impl Fn() + Send + Sync + 'static) -> Self {
        Self::new(ChipVariant::Input, selected, label, on_click)
    }

    /// 建议 chip（对标 SuggestionChip）
    pub fn suggestion(label: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static, on_click: impl Fn() + Send + Sync + 'static) -> Self {
        Self::new(ChipVariant::Suggestion, false, label, on_click)
    }

    fn new(variant: ChipVariant, selected: bool, label: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static, on_click: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            variant,
            selected,
            label: Box::new(label),
            on_click: std::sync::Arc::new(on_click),
            modifier: Modifier::new(),
            enabled: true,
            leading_icon: None,
            trailing_icon: None,
            avatar: None,
            shape: ChipDefaults::shape(),
            flat_colors: None,
            selectable_colors: None,
        }
    }

    pub fn modifier(mut self, m: Modifier) -> Self { self.modifier = m; self }
    pub fn enabled(mut self, e: bool) -> Self { self.enabled = e; self }
    pub fn shape(mut self, s: Shape) -> Self { self.shape = s; self }
    /// 非选择型配色（Assist/Suggestion）
    pub fn colors(mut self, c: ChipColors) -> Self { self.flat_colors = Some(c); self }
    /// 选择型配色（Filter/Input）
    pub fn selectable_colors(mut self, c: SelectableChipColors) -> Self { self.selectable_colors = Some(c); self }
    pub fn leading_icon(mut self, f: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self { self.leading_icon = Some(Box::new(f)); self }
    pub fn trailing_icon(mut self, f: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self { self.trailing_icon = Some(Box::new(f)); self }
    /// 头像（InputChip——24dp 圆角）
    pub fn avatar(mut self, f: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self { self.avatar = Some(Box::new(f)); self }
    /// 建议图标（SuggestionChip 的 icon——等效 leading）
    pub fn icon(mut self, f: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self { self.leading_icon = Some(Box::new(f)); self }

    pub fn build(self, ctx: &mut ComposeCtx) {
        let theme = crate::theme::WiniaTheme::colors();
        let sel = self.selected;
        // 配色解析（按变体取对应默认）——颜色角色来自 M3 specs 页面
        // (m3.material.io/components/chips/specs) 各变体的 "color roles" 列表：
        // Assist 标签是 On surface、Suggestion 是 On surface variant；Assist/Suggestion 的
        // outline 角色是 `Outline`，Filter/Input 是 `Outline variant`。
        let (container, label_color, icon_color, border_w, border_color) = match self.variant {
            ChipVariant::Assist | ChipVariant::Suggestion => {
                let c = self.flat_colors.unwrap_or_else(|| match self.variant {
                    ChipVariant::Suggestion => ChipDefaults::suggestion_chip_colors(&theme),
                    _ => ChipDefaults::assist_chip_colors(&theme),
                });
                let (container, label, icon) = if self.enabled {
                    (c.container, c.label, c.leading_icon)
                } else {
                    (c.disabled_container, c.disabled_label, c.disabled_label)
                };
                (container, label, icon, ChipDefaults::BORDER_WIDTH, theme.outline)
            }
            ChipVariant::Filter | ChipVariant::Input => {
                let c = self.selectable_colors.unwrap_or_else(|| match self.variant {
                    ChipVariant::Input => ChipDefaults::input_chip_colors(&theme),
                    _ => ChipDefaults::selectable_chip_colors(&theme),
                });
                let (container, label, icon) = if !self.enabled {
                    (c.disabled_container, c.disabled_label, c.disabled_label)
                } else if sel {
                    (c.selected_container, c.selected_label, c.selected_leading_icon)
                } else {
                    (c.container, c.label, c.leading_icon)
                };
                // 选中：0 边框；未选中：1dp outline variant
                let w = if sel { 0.0 } else { ChipDefaults::BORDER_WIDTH };
                (container, label, icon, w, theme.outline_variant)
            }
        };
        // 左右 padding 按 specs：无 icon 16dp、有 icon 8dp；InputChip 带头像时左 4dp / 右 8dp。
        let pad = match self.variant {
            ChipVariant::Input => {
                let left = if self.avatar.is_some() {
                    ChipDefaults::INPUT_AVATAR_LEFT_PADDING
                } else if self.leading_icon.is_some() {
                    ChipDefaults::PADDING_WITH_ICON
                } else {
                    ChipDefaults::PADDING_NO_ICON
                };
                let right = if self.trailing_icon.is_some() {
                    ChipDefaults::PADDING_WITH_ICON
                } else {
                    ChipDefaults::PADDING_NO_ICON
                };
                (left, right)
            }
            _ => {
                let has_icon = self.leading_icon.is_some() || self.trailing_icon.is_some();
                let p = if has_icon { ChipDefaults::PADDING_WITH_ICON } else { ChipDefaults::PADDING_NO_ICON };
                (p, p)
            }
        };
        let on_click = if self.enabled { Some(self.on_click.clone()) } else { None };
        let leading = self.leading_icon;
        let trailing = self.trailing_icon;
        let avatar = self.avatar;
        let label = self.label;

        build_chip(
            ctx,
            self.modifier.padding_start(pad.0).padding_end(pad.1),
            self.enabled,
            container,
            border_color,
            border_w,
            self.shape,
            label_color,
            on_click,
            move |ctx| {
                // ⚠ Row 必须显式交叉轴居中（默认 Alignment::Start 顶部对齐）——
                // 有图标（18dp）时文字会顶对齐而非垂直居中
                crate::layout::components::Row::new()
                    .spacing(ChipDefaults::ELEMENT_GAP)
                    .alignment(crate::layout::Alignment::Center)
                    .build(ctx, |ctx| {
                    if let Some(av) = avatar {
                        // avatar 24dp、12dp 圆角（specs："avatar shape 12dp corner radius"——圆角方块，不是圆）
                        crate::layout::components::Stack::new()
                            .modifier(Modifier::new()
                                .size(ChipDefaults::AVATAR_SIZE, ChipDefaults::AVATAR_SIZE)
                                .clip(Shape::RoundedRect { corner_radius: ChipDefaults::AVATAR_CORNER_RADIUS }))
                            .build(ctx, |c| av(c));
                    }
                    if let Some(ic) = leading {
                        crate::theme::WiniaTheme::with_content_color(icon_color, ctx, |c| ic(c));
                    }
                    crate::theme::WiniaTheme::with_content_color(label_color, ctx, |c| label(c));
                    if let Some(ic) = trailing {
                        crate::theme::WiniaTheme::with_content_color(icon_color, ctx, |c| ic(c));
                    }
                });
            },
        );
    }
}

// ═══════════════════════════════════════════════════════════
// 内部通用 chip 容器（4 变体共用）
// ═══════════════════════════════════════════════════════════

/// 通用 chip 容器：32dp 高 + shape 背景 + 边框 + clickable/ripple +
/// 水平内容排列（垂直居中）。
#[allow(clippy::too_many_arguments)]
fn build_chip(
    ctx: &mut ComposeCtx,
    modifier: Modifier,
    enabled: bool,
    container: crate::graphics::Color,
    border_color: crate::graphics::Color,
    border_width: f32,
    shape: Shape,
    content_color: crate::graphics::Color,
    on_click: Option<std::sync::Arc<dyn Fn() + Send + Sync>>,
    content: impl FnOnce(&mut ComposeCtx),
) {
    let key = ctx.next_key();
    // A winia addition: `animate_color_as_state` remembers the animated colour and registers the
    // animation, so a chip fades between its container colours (selected / unselected / disabled).
    // M3's chips apply their container colour directly (`material3/Chip.kt` has no
    // `animateColorAsState`), so this 180 ms EaseOutCubic fade is ours, not a port — kept because it
    // reads better than a hard cut, and recorded rather than presented as alignment. A spring would be
    // `SpringSpec::effects_default()` here now that the engine animates colours per Oklab component.
    let bg_anim = ctx.animate_color_as_state(
        container,
        crate::animation::AnimationSpec::Tween(
            crate::animation::TweenSpec::new(
                std::time::Duration::from_millis(180),
                crate::animation::interpolator::EaseOutCubic::new(),
            )
        ),
    );
    // ⚠ background 接受动态闭包（`impl Fn() -> Color`）——渲染期每帧求值
    // color_fn()：动画 tick 推进 State + request_redraw → 渲染读最新插值。
    // 若传 bg_anim.get() 的**值**则冻结在 build 时（动画不显示）。
    let bg = bg_anim.clone();
    let mut m = Modifier::new()
        .min_height(ChipDefaults::HEIGHT)
        .background(move || bg.get(), shape);
    // ⚠ 有填充色（selected/secondaryContainer 等）时边框必须消失（M3 语义）——
    // border_width=0 时**不挂** border modifier（挂 0 宽线仍会绘制、可见）
    if border_width > 0.0 {
        m = m.border(border_width, border_color, shape);
    }
    if enabled {
        if let Some(cb) = on_click {
            let interaction = ctx.remember(|| crate::interaction::MutableInteractionSource::new()).get();
            let cb2 = cb.clone();
            m = m
                .clickable_with_source(&interaction, move || cb2())
                .ripple_with_shape(&interaction, content_color, true, shape);
        }
    }
    m = m.then(modifier);
    match ctx.start_restartable_group(key, m, crate::layout::BoxLayout::new().alignment(crate::layout::Alignment::Center)) {
        GroupStatus::Skip => {}
        GroupStatus::Enter => {
            crate::theme::WiniaTheme::with_content_color(content_color, ctx, |ctx| {
                let mut text_style = crate::theme::WiniaTheme::typography().label_large;
                text_style.color = Some(content_color);
                ProvideTextStyle(text_style, ctx, content);
            });
        }
    }
    ctx.end_restartable_group();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::composer::Composer;
    use crate::theme::Typography;
    use crate::text::{FontWeight, TextStyle};
    use crate::unit::{Sp, TextUnit};
    use crate::layout::sizing::{Dimension, SizeValue};
    use crate::components::{Icon, Text};
    use crate::layout::components::Stack;

    /// Compose one chip and hand back the composer plus its container node (the chip's own box).
    fn chip_container(chip: Chip) -> (Composer, usize) {
        let mut composer = Composer::new();
        composer.compose(|ctx| chip.build(ctx));
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        // A single chip is the root itself unless a wrapper was inserted; find the node carrying the outline
        // or the background.
        let inner = if nodes[root]
            .modifier
            .elements()
            .iter()
            .any(|el| matches!(el, crate::modifier::ModifierElement::Background { .. }))
        {
            root
        } else {
            nodes[root].children.first().copied().expect("the chip's box")
        };
        (composer, inner)
    }

    fn outline_of(composer: &Composer, idx: usize) -> Option<(f32, crate::graphics::Color)> {
        composer.arena_nodes()[idx]
            .modifier
            .elements()
            .iter()
            .find_map(|el| match el {
                crate::modifier::ModifierElement::Border { width, color, .. } => Some((*width, *color)),
                _ => None,
            })
    }

    fn fixed(v: &SizeValue) -> Option<f32> {
        match v {
            SizeValue::Static(Dimension::Fixed(dp)) => Some(*dp),
            _ => None,
        }
    }

    /// The horizontal padding the chip asks for, (start, end).
    fn padding_of(composer: &Composer, idx: usize) -> (Option<f32>, Option<f32>) {
        let mut start = None;
        let mut end = None;
        for el in composer.arena_nodes()[idx].modifier.elements() {
            if let crate::modifier::ModifierElement::PaddingSides { start: s, end: e, .. } = el {
                if let Some(v) = fixed(s) {
                    start = Some(v);
                }
                if let Some(v) = fixed(e) {
                    end = Some(v);
                }
            }
        }
        (start, end)
    }

    fn label(text: &'static str) -> impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static {
        move |ctx: &mut ComposeCtx| Text::new(text).build(ctx)
    }

    /// The specs page lists the assist chip's label role as **On surface** and the suggestion chip's as
    /// **On surface variant**; winia used the suggestion value for both.
    #[test]
    fn assist_and_suggestion_labels_are_different_roles() {
        let theme = crate::theme::WiniaTheme::colors();
        assert_eq!(ChipDefaults::assist_chip_colors(&theme).label, theme.on_surface);
        assert_eq!(
            ChipDefaults::suggestion_chip_colors(&theme).label,
            theme.on_surface_variant
        );
    }

    /// The specs page: assist and suggestion chips outline with `Outline`, filter and input chips with
    /// `Outline variant`; a selected chip has no outline at all.
    #[test]
    fn the_outline_role_follows_the_variant() {
        let theme = crate::theme::WiniaTheme::colors();
        let (assist, a_idx) = chip_container(Chip::assist(label("A"), || {}));
        assert_eq!(outline_of(&assist, a_idx), Some((1.0, theme.outline)));
        let (suggestion, s_idx) = chip_container(Chip::suggestion(label("S"), || {}));
        assert_eq!(outline_of(&suggestion, s_idx), Some((1.0, theme.outline)));
        let (filter, f_idx) = chip_container(Chip::filter(false, label("F"), || {}));
        assert_eq!(outline_of(&filter, f_idx), Some((1.0, theme.outline_variant)));
        let (selected, s2_idx) = chip_container(Chip::filter(true, label("F"), || {}));
        assert_eq!(outline_of(&selected, s2_idx), None, "a selected chip draws no outline");
        let (input, i_idx) = chip_container(Chip::input(false, label("I"), || {}));
        assert_eq!(outline_of(&input, i_idx), Some((1.0, theme.outline_variant)));
    }

    /// The specs page gives an input chip's leading icon the Primary role, so it stays Primary when the chip
    /// is selected; a filter chip's turns On secondary container.
    #[test]
    fn an_input_chips_selected_leading_icon_stays_primary() {
        let theme = crate::theme::WiniaTheme::colors();
        assert_eq!(
            ChipDefaults::selectable_chip_colors(&theme).selected_leading_icon,
            theme.on_secondary_container
        );
        assert_eq!(
            ChipDefaults::input_chip_colors(&theme).selected_leading_icon,
            theme.primary
        );
    }

    /// The specs page's padding rules: 16 dp with nothing at the edge, 8 dp beside an icon, and for an input
    /// chip "left padding for avatar 4 dp, right padding for avatar 8 dp".
    #[test]
    fn the_padding_is_the_specs_rule() {
        let (plain, p_idx) = chip_container(Chip::assist(label("A"), || {}));
        assert_eq!(padding_of(&plain, p_idx), (Some(16.0), Some(16.0)));
        let (icon, i_idx) = chip_container(Chip::assist(label("A"), || {}).leading_icon(|ctx| {
            Icon::new(crate::graphics::IconSource::svg_path("M12 3v10h8v2H10V3h2z")).build(ctx);
        }));
        assert_eq!(padding_of(&icon, i_idx), (Some(8.0), Some(8.0)));

        let (bare_input, b_idx) = chip_container(Chip::input(false, label("I"), || {}));
        assert_eq!(padding_of(&bare_input, b_idx), (Some(16.0), Some(16.0)));
        let (with_avatar, av_idx) = chip_container(
            Chip::input(false, label("I"), || {}).avatar(|ctx| {
                Stack::new().build(ctx, |_| {});
            }),
        );
        assert_eq!(
            padding_of(&with_avatar, av_idx),
            (Some(4.0), Some(16.0)),
            "an avatar sits 4 dp from the leading edge (the specs page's left padding for avatar); the trailing \
             edge has nothing in it, so it keeps the default 16 dp"
        );
        let (with_close, c_idx) = chip_container(Chip::input(false, label("I"), || {}).trailing_icon(|ctx| {
            Icon::new(crate::graphics::IconSource::svg_path("M19 6.41 12 13.4 5 6.41 3.59 8 12 16.4 20.41 8z")).build(ctx);
        }));
        assert_eq!(padding_of(&with_close, c_idx), (Some(16.0), Some(8.0)));
    }

    fn find_text_style(nodes: &[crate::layout::node::LayoutNode], idx: usize) -> Option<(f32, FontWeight, f32, Option<f32>)> {
        for el in nodes[idx].modifier.elements() {
            if let crate::modifier::ModifierElement::TextContent { font_size, font_weight, letter_spacing, line_height, .. } = el {
                return Some((*font_size, *font_weight, *letter_spacing, *line_height));
            }
        }
        for &child in &nodes[idx].children {
            if let Some(style) = find_text_style(nodes, child) { return Some(style); }
        }
        None
    }

    #[test]
    fn label_uses_typography_label_large() {
        let custom = Typography {
            label_large: TextStyle::new()
                .font_size(TextUnit::Sp(Sp(19.0)))
                .line_height(27.0)
                .letter_spacing(1.3)
                .font_weight(FontWeight::BOLD),
            ..Typography::default()
        };
        let mut composer = Composer::new();
        composer.compose(|ctx| {
            crate::theme::WiniaTheme::with_typography(custom, ctx, |ctx| {
                Chip::assist(|ctx| crate::components::Text::new("Label").build(ctx), || {}).build(ctx);
            });
        });
        let root = composer.layout_root_idx().unwrap();
        let style = find_text_style(composer.arena_nodes(), root).unwrap();
        assert_eq!(style.0, 19.0);
        assert_eq!(style.1, FontWeight::BOLD);
        assert_eq!(style.2, 1.3);
        assert_eq!(style.3, Some(27.0));
    }
}
