//! Chips — 输入/筛选/操作标签组件（对齐 Jetpack Compose material3 Chip 系列）
//!
//! 统一入口 `Chip` + 变体构造（`Chip::filter()` 等——对标 material3 的
//! AssistChip/ElevatedAssistChip/FilterChip/ElevatedFilterChip/InputChip/
//! SuggestionChip/ElevatedSuggestionChip 家族）：
//! - `Chip::assist(label, on_click)`：智能/自动化操作（可选 leading/trailing icon）
//! - `Chip::filter(selected, label, on_click)`：筛选（selected 切换）
//! - `Chip::input(selected, label, on_click)`：输入信息（leading/avatar/trailing）
//! - `Chip::suggestion(label, on_click)`：建议性操作（可选 icon）
//! - `Chip::elevated_assist/elevated_filter/elevated_suggestion`：填充容器 + 抬升
//!
//! Numbers come from the M3 tokens, dp not level indices (`ElevationTokens` is Level0 0, Level1 1,
//! Level2 3, Level3 6, Level4 8): container 32 dp, `CornerSmall` (8 dp), icon 18 dp, avatar 24 dp and
//! `CornerFull` (a circle), horizontal container padding 8 dp (the input chip's asymmetric 4/8 rule),
//! element gap 8 dp. Colours: assist labels are `OnSurface` while a suggestion's is `OnSurfaceVariant`,
//! every flat chip's outline is `OutlineVariant` (disabled `OnSurface` @ 0.12), a disabled selected
//! container is `OnSurface` @ 0.12, and an input chip keeps a `Primary` leading icon when selected (a
//! FilterChip's turns `OnSecondaryContainer`).

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

/// ChipDefaults — the token numbers, in dp, and one colour factory per variant.
///
/// M3 keeps the four flat variants' numbers equal (32 dp tall, `CornerSmall`, 18 dp icons) but gives
/// each its own colour set: an assist chip's label is `OnSurface` where a suggestion chip's is
/// `OnSurfaceVariant`, an input chip keeps a `Primary` leading icon when selected where a filter
/// chip's turns `OnSecondaryContainer`, and the elevated variants fill with `SurfaceContainerLow`.
pub struct ChipDefaults;

impl ChipDefaults {
    pub const HEIGHT: f32 = 32.0; // ContainerHeight
    pub const ICON_SIZE: f32 = 18.0; // IconSize / LeadingIconSize
    pub const CORNER_RADIUS: f32 = 8.0; // ContainerShape = CornerSmall
    /// `HorizontalElementsPadding` — the container's horizontal padding for the assist, filter and
    /// suggestion chips, and the gap between elements in every variant. (winia used 16 dp when a chip had
    /// no icon; that was the older spec.)
    pub const PADDING_HORIZONTAL: f32 = 8.0;
    /// The input chip's asymmetric rule: `start = if (hasAvatar || !hasLeadingIcon) 4 else 8`,
    /// `end = if (hasTrailingIcon) 8 else 4` (`Chip.kt:2944-2952`).
    pub const INPUT_PADDING_COMPACT: f32 = 4.0;
    pub const ELEMENT_GAP: f32 = 8.0; // between leading icon / label / trailing icon
    pub const BORDER_WIDTH: f32 = 1.0; // FlatOutlineWidth / FlatUnselectedOutlineWidth
    pub const AVATAR_SIZE: f32 = 24.0; // InputChipTokens.AvatarSize
    /// `DisabledLabelTextOpacity` (all four variants) and `DisabledAvatarOpacity` (input chip).
    pub const DISABLED_OPACITY: f32 = 0.38;
    /// `FlatDisabledOutlineOpacity` / `FlatDisabledSelectedContainerOpacity` / `ElevatedDisabledContainerOpacity`.
    pub const DISABLED_CONTAINER_OPACITY: f32 = 0.12;

    fn with_alpha(c: crate::graphics::Color, alpha: f32) -> crate::graphics::Color {
        crate::graphics::Color::from_argb((c.a as f32 * alpha) as u8, c.r, c.g, c.b)
    }

    /// `AssistChipDefaults.assistChipColors()`: transparent container, `OnSurface` label,
    /// `Primary` leading icon, `OnSurfaceVariant` trailing icon.
    pub fn assist_chip_colors(theme: &crate::theme::ThemeColors) -> ChipColors {
        ChipColors {
            container: crate::graphics::Color::TRANSPARENT,
            label: theme.on_surface,
            leading_icon: theme.primary,
            trailing_icon: theme.on_surface_variant,
            disabled_container: crate::graphics::Color::TRANSPARENT,
            disabled_label: Self::with_alpha(theme.on_surface, Self::DISABLED_OPACITY),
        }
    }

    /// `SuggestionChipDefaults.suggestionChipColors()`: like the assist chip, but the label is
    /// `OnSurfaceVariant` (`SuggestionChipTokens.LabelTextColor`).
    pub fn suggestion_chip_colors(theme: &crate::theme::ThemeColors) -> ChipColors {
        ChipColors {
            label: theme.on_surface_variant,
            ..Self::assist_chip_colors(theme)
        }
    }

    /// `AssistChipDefaults.elevatedAssistChipColors()`: `SurfaceContainerLow` container, and the disabled
    /// container is `OnSurface` @ 0.12 rather than transparent.
    pub fn elevated_assist_chip_colors(theme: &crate::theme::ThemeColors) -> ChipColors {
        ChipColors {
            container: theme.surface_container_low,
            disabled_container: Self::with_alpha(theme.on_surface, Self::DISABLED_CONTAINER_OPACITY),
            ..Self::assist_chip_colors(theme)
        }
    }

    /// `SuggestionChipDefaults.elevatedSuggestionChipColors()`.
    pub fn elevated_suggestion_chip_colors(theme: &crate::theme::ThemeColors) -> ChipColors {
        ChipColors {
            container: theme.surface_container_low,
            disabled_container: Self::with_alpha(theme.on_surface, Self::DISABLED_CONTAINER_OPACITY),
            ..Self::suggestion_chip_colors(theme)
        }
    }

    /// `FilterChipDefaults.filterChipColors()`: unselected transparent with an `OnSurfaceVariant` label,
    /// selected `SecondaryContainer` with `OnSecondaryContainer`, and both icons `OnSecondaryContainer`
    /// once selected.
    pub fn filter_chip_colors(theme: &crate::theme::ThemeColors) -> SelectableChipColors {
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
            disabled_label: Self::with_alpha(theme.on_surface, Self::DISABLED_OPACITY),
            disabled_selected_container: Self::with_alpha(
                theme.on_surface,
                Self::DISABLED_CONTAINER_OPACITY,
            ),
        }
    }

    /// `ElevatedFilterChipDefaults.elevatedFilterChipColors()`: the unselected container is
    /// `SurfaceContainerLow` instead of transparent.
    pub fn elevated_filter_chip_colors(theme: &crate::theme::ThemeColors) -> SelectableChipColors {
        SelectableChipColors {
            container: theme.surface_container_low,
            disabled_container: Self::with_alpha(theme.on_surface, Self::DISABLED_CONTAINER_OPACITY),
            ..Self::filter_chip_colors(theme)
        }
    }

    /// `InputChipDefaults.inputChipColors()`: the filter chip's colours except that a SELECTED leading
    /// icon stays `Primary` (`InputChipTokens.SelectedLeadingIconColor`).
    pub fn input_chip_colors(theme: &crate::theme::ThemeColors) -> SelectableChipColors {
        SelectableChipColors {
            selected_leading_icon: theme.primary,
            ..Self::filter_chip_colors(theme)
        }
    }

    pub fn shape() -> Shape {
        Shape::RoundedRect { corner_radius: Self::CORNER_RADIUS }
    }
}

// ═══════════════════════════════════════════════════════════
// ChipElevation / ChipBorder
// ═══════════════════════════════════════════════════════════

/// A chip's shadow elevation per interaction state, in dp — material3's `ChipElevation` /
/// `SelectableChipElevation` (both carry the same six values; a selectable chip's *selected* cases are
/// simply what the factory puts in them).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChipElevation {
    pub default: f32,
    pub pressed: f32,
    pub focused: f32,
    pub hovered: f32,
    pub dragged: f32,
    pub disabled: f32,
}

impl ChipElevation {
    pub fn new(default: f32, pressed: f32, focused: f32, hovered: f32, dragged: f32, disabled: f32) -> Self {
        Self { default, pressed, focused, hovered, dragged, disabled }
    }

    /// `assistChipElevation()` / `suggestionChipElevation()` / `inputChipElevation()`: `FlatContainerElevation`
    /// (Level0 = 0) in every state, and only a DRAG lifts it (`DraggedContainerElevation` = Level4 = 8 dp).
    pub fn flat() -> Self {
        Self::new(0.0, 0.0, 0.0, 0.0, 8.0, 0.0)
    }

    /// `filterChipElevation()`: like `flat()`, but a hovered chip rises 1 dp
    /// (`FlatSelectedHoverContainerElevation` = Level1).
    pub fn hover_lifting() -> Self {
        Self::new(0.0, 0.0, 0.0, 1.0, 8.0, 0.0)
    }

    /// The `Elevated*` variants: `ElevatedContainerElevation` Level1 (1 dp) at rest, pressed and focused,
    /// Level2 (3 dp) hovered, Level0 disabled, 8 dp while dragged.
    pub fn elevated() -> Self {
        Self::new(1.0, 1.0, 1.0, 3.0, 8.0, 0.0)
    }

    /// The elevation for a state snapshot, latest interaction first — the same order the Button and Card
    /// use, since `animateElevation` resolves it from the last interaction.
    pub fn for_state(&self, state: &crate::interaction::ComponentState) -> f32 {
        if !state.enabled {
            self.disabled
        } else if state.dragged {
            self.dragged
        } else if state.pressed {
            self.pressed
        } else if state.hovered {
            self.hovered
        } else if state.focused {
            self.focused
        } else {
            self.default
        }
    }

    /// Every state is 0: no shadow to animate and no layer to pay for.
    pub fn is_flat(&self) -> bool {
        self.default <= 0.0
            && self.pressed <= 0.0
            && self.focused <= 0.0
            && self.hovered <= 0.0
            && self.dragged <= 0.0
            && self.disabled <= 0.0
    }
}

/// A chip's outline per state — material3's `BorderStroke` built by `assistChipBorder(enabled)` /
/// `filterChipBorder(enabled, selected)`. `resolve` returns `None` when nothing is drawn, which is how
/// a selected chip's zero-width border is expressed (winia must not attach a zero-width border: it still
/// paints).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChipBorder {
    pub color: crate::graphics::Color,
    pub selected_color: crate::graphics::Color,
    pub disabled_color: crate::graphics::Color,
    pub disabled_selected_color: crate::graphics::Color,
    pub width: f32,
    pub selected_width: f32,
}

impl ChipBorder {
    pub fn new(
        color: crate::graphics::Color,
        selected_color: crate::graphics::Color,
        disabled_color: crate::graphics::Color,
        disabled_selected_color: crate::graphics::Color,
        width: f32,
        selected_width: f32,
    ) -> Self {
        Self { color, selected_color, disabled_color, disabled_selected_color, width, selected_width }
    }

    /// `assistChipBorder(enabled)` / `suggestionChipBorder(enabled)`: 1 dp `OutlineVariant`, and
    /// `OnSurface` @ 0.12 when disabled.
    pub fn flat(theme: &crate::theme::ThemeColors) -> Self {
        Self::new(
            theme.outline_variant,
            crate::graphics::Color::TRANSPARENT,
            ChipDefaults::with_alpha(theme.on_surface, ChipDefaults::DISABLED_CONTAINER_OPACITY),
            crate::graphics::Color::TRANSPARENT,
            ChipDefaults::BORDER_WIDTH,
            0.0,
        )
    }

    /// `filterChipBorder(enabled, selected)` / `inputChipBorder(enabled, selected)`: the same outline when
    /// enabled and unselected, a ZERO-width transparent border when selected, and `OnSurface` @ 0.12 when
    /// disabled and unselected.
    pub fn selectable(theme: &crate::theme::ThemeColors) -> Self {
        Self::flat(theme)
    }

    /// No outline at all (the elevated variants' default, and what `BorderStroke? = null` means).
    pub fn none() -> Self {
        Self::new(
            crate::graphics::Color::TRANSPARENT,
            crate::graphics::Color::TRANSPARENT,
            crate::graphics::Color::TRANSPARENT,
            crate::graphics::Color::TRANSPARENT,
            0.0,
            0.0,
        )
    }

    pub fn resolve(&self, enabled: bool, selected: bool) -> Option<(f32, crate::graphics::Color)> {
        let (width, color) = if enabled {
            if selected { (self.selected_width, self.selected_color) } else { (self.width, self.color) }
        } else if selected {
            (self.selected_width, self.disabled_selected_color)
        } else {
            (self.width, self.disabled_color)
        };
        if width > 0.0 {
            Some((width, color))
        } else {
            None
        }
    }
}

// ═══════════════════════════════════════════════════════════
// ChipVariant — 变体
// ═══════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChipVariant {
    /// 辅助操作（AssistChip）
    Assist,
    /// Filled + lifting assist chip (ElevatedAssistChip)
    ElevatedAssist,
    /// 筛选（FilterChip）
    Filter,
    /// Filled + lifting filter chip (ElevatedFilterChip)
    ElevatedFilter,
    /// 输入信息（InputChip）
    Input,
    /// 建议（SuggestionChip）
    Suggestion,
    /// Filled + lifting suggestion chip (ElevatedSuggestionChip)
    ElevatedSuggestion,
}

impl ChipVariant {
    /// Filter and input chips carry a `selected` state; the others have `Role.Button` semantics and no
    /// selection (`Chip.kt:1963` uses `Role.Button`, `:2010` `Role.Checkbox` for the selectable pair).
    pub fn is_selectable(self) -> bool {
        matches!(self, ChipVariant::Filter | ChipVariant::ElevatedFilter | ChipVariant::Input)
    }

    /// The `Elevated*` variants fill with `SurfaceContainerLow` instead of a transparent container and
    /// carry no outline by default.
    pub fn is_elevated(self) -> bool {
        matches!(
            self,
            ChipVariant::ElevatedAssist | ChipVariant::ElevatedFilter | ChipVariant::ElevatedSuggestion
        )
    }
}

// ═══════════════════════════════════════════════════════════
// Chip — 统一入口（变体由构造方法决定）
// ═══════════════════════════════════════════════════════════

/// The Chip component (material3's AssistChip / ElevatedAssistChip / FilterChip / ElevatedFilterChip /
/// InputChip / SuggestionChip / ElevatedSuggestionChip).
///
/// Build one with `Chip::assist/elevated_assist/filter/elevated_filter/input/suggestion/elevated_suggestion`;
/// the visuals and defaults follow the variant:
/// - Assist / Suggestion: not selectable (`ChipColors`) — `colors(ChipColors)`
/// - Filter / Input: selectable (`SelectableChipColors`) — `selectable_colors(...)`
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
    /// Shadow elevation (`None` = the variant's token default); `ChipElevation? = null` in Compose.
    elevation: Option<ChipElevation>,
    /// Outline (`None` = the variant's token default; `Some(ChipBorder::none())` = no outline).
    border: Option<ChipBorder>,
    /// Caller-supplied interaction source (`interactionSource: MutableInteractionSource?`).
    interaction_source: Option<crate::interaction::MutableInteractionSource>,
}

impl Chip {
    /// 辅助操作 chip（对标 AssistChip）
    pub fn assist(label: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static, on_click: impl Fn() + Send + Sync + 'static) -> Self {
        Self::new(ChipVariant::Assist, false, label, on_click)
    }

    /// Filled + lifting assist chip (ElevatedAssistChip)
    pub fn elevated_assist(label: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static, on_click: impl Fn() + Send + Sync + 'static) -> Self {
        Self::new(ChipVariant::ElevatedAssist, false, label, on_click)
    }

    /// 筛选 chip（对标 FilterChip）——selected 切换
    pub fn filter(selected: bool, label: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static, on_click: impl Fn() + Send + Sync + 'static) -> Self {
        Self::new(ChipVariant::Filter, selected, label, on_click)
    }

    /// Filled + lifting filter chip (ElevatedFilterChip)
    pub fn elevated_filter(selected: bool, label: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static, on_click: impl Fn() + Send + Sync + 'static) -> Self {
        Self::new(ChipVariant::ElevatedFilter, selected, label, on_click)
    }

    /// 输入 chip（对标 InputChip）——selected 切换 + avatar/trailing 关闭按钮
    pub fn input(selected: bool, label: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static, on_click: impl Fn() + Send + Sync + 'static) -> Self {
        Self::new(ChipVariant::Input, selected, label, on_click)
    }

    /// 建议 chip（对标 SuggestionChip）
    pub fn suggestion(label: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static, on_click: impl Fn() + Send + Sync + 'static) -> Self {
        Self::new(ChipVariant::Suggestion, false, label, on_click)
    }

    /// Filled + lifting suggestion chip (ElevatedSuggestionChip)
    pub fn elevated_suggestion(label: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static, on_click: impl Fn() + Send + Sync + 'static) -> Self {
        Self::new(ChipVariant::ElevatedSuggestion, false, label, on_click)
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
            elevation: None,
            border: None,
            interaction_source: None,
        }
    }

    pub fn modifier(mut self, m: Modifier) -> Self { self.modifier = m; self }
    pub fn enabled(mut self, e: bool) -> Self { self.enabled = e; self }
    pub fn shape(mut self, s: Shape) -> Self { self.shape = s; self }
    /// 非选择型配色（Assist/Suggestion）
    pub fn colors(mut self, c: ChipColors) -> Self { self.flat_colors = Some(c); self }
    /// 选择型配色（Filter/Input）
    pub fn selectable_colors(mut self, c: SelectableChipColors) -> Self { self.selectable_colors = Some(c); self }
    /// Shadow elevation (`ChipElevation?` — `Some(ChipElevation::flat())` removes the shadow).
    pub fn elevation(mut self, e: ChipElevation) -> Self { self.elevation = Some(e); self }
    /// Outline (`BorderStroke?` — `ChipBorder::none()` for no outline).
    pub fn border(mut self, b: ChipBorder) -> Self { self.border = Some(b); self }
    /// A caller-supplied interaction source (`interactionSource: MutableInteractionSource?`)
    pub fn interaction_source(mut self, source: crate::interaction::MutableInteractionSource) -> Self {
        self.interaction_source = Some(source);
        self
    }
    pub fn leading_icon(mut self, f: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self { self.leading_icon = Some(Box::new(f)); self }
    pub fn trailing_icon(mut self, f: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self { self.trailing_icon = Some(Box::new(f)); self }
    /// Avatar (input chips only — 24 dp, and `CornerFull`, i.e. a circle)
    pub fn avatar(mut self, f: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self { self.avatar = Some(Box::new(f)); self }
    /// 建议图标（SuggestionChip 的 icon——等效 leading）
    pub fn icon(mut self, f: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self { self.leading_icon = Some(Box::new(f)); self }

    pub fn build(self, ctx: &mut ComposeCtx) {
        let theme = crate::theme::WiniaTheme::colors();
        let sel = self.selected;
        let variant = self.variant;
        // 配色解析（按变体取对应默认）
        let (container, label_color, icon_color, default_border) = if variant.is_selectable() {
            let c = self.selectable_colors.unwrap_or_else(|| match variant {
                ChipVariant::ElevatedFilter => ChipDefaults::elevated_filter_chip_colors(&theme),
                ChipVariant::Input => ChipDefaults::input_chip_colors(&theme),
                _ => ChipDefaults::filter_chip_colors(&theme),
            });
            let (container, label, icon) = if !self.enabled {
                let container = if sel { c.disabled_selected_container } else { c.disabled_container };
                (container, c.disabled_label, c.disabled_label)
            } else if sel {
                (c.selected_container, c.selected_label, c.selected_leading_icon)
            } else {
                (c.container, c.label, c.leading_icon)
            };
            (container, label, icon, ChipBorder::selectable(&theme))
        } else {
            let c = self.flat_colors.unwrap_or_else(|| match variant {
                ChipVariant::ElevatedAssist => ChipDefaults::elevated_assist_chip_colors(&theme),
                ChipVariant::ElevatedSuggestion => {
                    ChipDefaults::elevated_suggestion_chip_colors(&theme)
                }
                ChipVariant::Suggestion => ChipDefaults::suggestion_chip_colors(&theme),
                _ => ChipDefaults::assist_chip_colors(&theme),
            });
            let (container, label, icon) = if self.enabled {
                (c.container, c.label, c.leading_icon)
            } else {
                (c.disabled_container, c.disabled_label, c.disabled_label)
            };
            (container, label, icon, ChipBorder::flat(&theme))
        };
        // The elevated variants carry no outline unless the caller asks for one (`border = null` upstream).
        let border = self.border.unwrap_or(if variant.is_elevated() {
            ChipBorder::none()
        } else {
            default_border
        });
        let border = border.resolve(self.enabled, sel);
        // Padding: 8 dp horizontally for assist/filter/suggestion and the gap between elements; the input
        // chip's own asymmetric rule.
        let has_leading = self.leading_icon.is_some() || self.avatar.is_some();
        let (pad_start, pad_end) = if variant == ChipVariant::Input {
            let start = if self.avatar.is_some() || !has_leading {
                ChipDefaults::INPUT_PADDING_COMPACT
            } else {
                ChipDefaults::PADDING_HORIZONTAL
            };
            let end = if self.trailing_icon.is_some() {
                ChipDefaults::PADDING_HORIZONTAL
            } else {
                ChipDefaults::INPUT_PADDING_COMPACT
            };
            (start, end)
        } else {
            (ChipDefaults::PADDING_HORIZONTAL, ChipDefaults::PADDING_HORIZONTAL)
        };
        let on_click = if self.enabled { Some(self.on_click.clone()) } else { None };
        let interaction = self
            .interaction_source
            .unwrap_or_else(|| ctx.remember(|| crate::interaction::MutableInteractionSource::new()).get());
        let state = interaction.state(self.enabled);
        // Elevation (`ChipElevation?`): the variant's token default, animated with the shared
        // interaction-pair rule the Button and Card use (`animateElevation`). The interaction kind the last
        // target came from lives in a backchannel so reading it registers no dependency.
        let last_kind: crate::runtime::state::Backchannel<
            Option<crate::animation::ElevationInteraction>,
        > = ctx.remember_backchannel(|| None);
        let elevation = self.elevation.unwrap_or_else(|| match variant {
            ChipVariant::ElevatedAssist | ChipVariant::ElevatedFilter
            | ChipVariant::ElevatedSuggestion => ChipElevation::elevated(),
            ChipVariant::Filter | ChipVariant::ElevatedFilter => ChipElevation::hover_lifting(),
            _ => ChipElevation::flat(),
        });
        let elevation_anim = if elevation.is_flat() {
            None
        } else {
            let target = elevation.for_state(&state);
            let to = state.elevation_interaction();
            let from = if state.enabled { last_kind.peek() } else { None };
            let spec = crate::animation::elevation_spec_for_pair(from, to)
                .unwrap_or(crate::animation::AnimationSpec::Snap(crate::animation::SnapSpec::default()));
            last_kind.set(to);
            Some(ctx.animate_float_as_state(target, spec))
        };
        let leading = self.leading_icon;
        let trailing = self.trailing_icon;
        let avatar = self.avatar;
        let label = self.label;
        let shape = self.shape;

        build_chip(
            ctx,
            self.modifier.padding_start(pad_start).padding_end(pad_end),
            self.enabled,
            variant,
            sel,
            container,
            border,
            shape,
            label_color,
            on_click,
            Some(interaction),
            elevation_anim,
            move |ctx| {
                // ⚠ Row 必须显式交叉轴居中（默认 Alignment::Start 顶部对齐）——
                // 有图标（18dp）时文字会顶对齐而非垂直居中
                crate::layout::components::Row::new()
                    .spacing(ChipDefaults::ELEMENT_GAP)
                    .alignment(crate::layout::Alignment::Center)
                    .build(ctx, |ctx| {
                    if let Some(av) = avatar {
                        // `InputChipTokens.AvatarShape` is `CornerFull` — a circle — drawn at 24 dp, and a
                        // disabled chip fades it (`DisabledAvatarOpacity` 0.38).
                        let avatar_modifier = if self.enabled {
                            Modifier::new()
                        } else {
                            Modifier::new().alpha(ChipDefaults::DISABLED_OPACITY)
                        };
                        crate::layout::components::Stack::new()
                            .modifier(avatar_modifier
                                .size(ChipDefaults::AVATAR_SIZE, ChipDefaults::AVATAR_SIZE)
                                .clip(Shape::Circle))
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
            theme.secondary,
        );
    }
}

// ═══════════════════════════════════════════════════════════
// 内部通用 chip 容器（4 变体共用）
// ═══════════════════════════════════════════════════════════

/// 通用 chip 容器：32dp 高 + shape 背景 + 边框 + clickable/ripple +
/// 水平内容排列（垂直居中）。
#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
fn build_chip(
    ctx: &mut ComposeCtx,
    modifier: Modifier,
    enabled: bool,
    variant: ChipVariant,
    selected: bool,
    container: crate::graphics::Color,
    border: Option<(f32, crate::graphics::Color)>,
    shape: Shape,
    content_color: crate::graphics::Color,
    on_click: Option<std::sync::Arc<dyn Fn() + Send + Sync>>,
    interaction: Option<crate::interaction::MutableInteractionSource>,
    elevation_anim: Option<crate::runtime::state::State<f32>>,
    content: impl FnOnce(&mut ComposeCtx),
    focus_color: crate::graphics::Color,
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
    // A zero-width border still paints, so only a resolved (width > 0) border is attached.
    if let Some((border_width, border_color)) = border {
        m = m.border(border_width, border_color, shape);
    }
    // Elevation rides the graphics layer so the shadow follows the animated value without recomposing.
    if let Some(anim) = elevation_anim {
        m = m.graphics_layer(move || crate::graphics::GraphicsLayerParams {
            shadow_elevation: anim.get(),
            shadow_shape: Some(shape),
            ..Default::default()
        });
    }
    if enabled {
        if let Some(cb) = on_click {
            let interaction = interaction
                .unwrap_or_else(|| ctx.remember(|| crate::interaction::MutableInteractionSource::new()).get());
            let cb2 = cb.clone();
            m = m
                .clickable_with_source(&interaction, move || cb2())
                .ripple_with_shape(&interaction, content_color, true, shape);
        }
    }
    // Accessibility: `Chip.kt:1963` declares `Role.Button` for an assist/suggestion chip and `:2010`
    // `Role.Checkbox` plus the selected state for a filter/input one.
    let semantics = if variant.is_selectable() {
        crate::semantics::SemanticsConfig::new()
            .role(crate::semantics::SemanticsRole::Checkbox)
            .merge_descendants(true)
            .state(
                crate::semantics::SemanticsState::new()
                    .selected(selected)
                    .enabled(enabled),
            )
    } else {
        crate::semantics::SemanticsConfig::new()
            .role(crate::semantics::SemanticsRole::Button)
            .merge_descendants(true)
    };
    m = m.semantics(semantics);
    m = m.then(modifier);
    // A clickable chip reserves the 48 dp touch target: Compose builds chips on `Surface(onClick = ...)`,
    // whose Box applies `Modifier.minimumInteractiveComponentSize()` (`material3/Surface.kt:219`), with the
    // 32 dp visual centred inside. winia used the 32 dp visual as the node itself.
    //
    // DEVIATION, recorded: the clickable and the ripple stay on the 32 dp visual, so the 8 dp ring the target
    // adds is SPACING rather than hit area (`fire_click_along_path` walks the hit path for a node carrying
    // `on_click`, and the target node carries none). Putting the clickable on the 48 dp node instead would keep
    // its ripple clipped to a 48 dp shape, i.e. a wash 8 dp above and below the chip on every press — the
    // silent visual regression the user has already rejected once. Widen the hit area only together with a way
    // to keep the ripple on the visual.
    let target = Modifier::new().minimum_interactive_component_size();
    match ctx.start_restartable_group(key, target, crate::layout::BoxLayout::new().alignment(crate::layout::Alignment::Center)) {
        GroupStatus::Skip => {}
        GroupStatus::Enter => {
            let inner = ctx.next_key();
            match ctx.start_restartable_group(inner, m, crate::layout::BoxLayout::new().alignment(crate::layout::Alignment::Center)) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    crate::theme::WiniaTheme::with_content_color(content_color, ctx, |ctx| {
                        let mut text_style = crate::theme::WiniaTheme::typography().label_large;
                        text_style.color = Some(content_color);
                        ProvideTextStyle(text_style, ctx, content);
                    });
                }
            }
            // Focus ring colour: every chip's token is `FocusIndicatorColor = Secondary` (all four token
            // files). Without this the framework's default ring is drawn — the grey halo that made a focused
            // chip look wrong.
            ctx.set_current_node_focus_color(focus_color);
            ctx.end_restartable_group();
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

    /// Compose one chip and hand back the composer plus the two nodes it builds: the outer touch target and
    /// the chip's own container.
    fn chip_nodes(chip: Chip) -> (Composer, usize, usize) {
        let mut composer = Composer::new();
        composer.compose(|ctx| chip.build(ctx));
        composer.layout(crate::layout::Constraints::new(0.0, 400.0, 0.0, 400.0));
        let root = composer.layout_root_idx().expect("root");
        let nodes = composer.arena_nodes();
        let inner = nodes[root]
            .children
            .first()
            .copied()
            .expect("the chip's container is inside the touch target");
        (composer, root, inner)
    }

    fn painted_background(composer: &Composer, idx: usize) -> crate::graphics::Color {
        composer.arena_nodes()[idx]
            .modifier
            .elements()
            .iter()
            .find_map(|el| match el {
                crate::modifier::ModifierElement::Background { color_fn, .. } => Some(color_fn()),
                _ => None,
            })
            .expect("the chip paints a container")
    }

    fn border_of(composer: &Composer, idx: usize) -> Option<(f32, crate::graphics::Color)> {
        composer.arena_nodes()[idx]
            .modifier
            .elements()
            .iter()
            .find_map(|el| match el {
                crate::modifier::ModifierElement::Border { width, color, .. } => Some((*width, *color)),
                _ => None,
            })
    }

    fn role_of(composer: &Composer, idx: usize) -> Option<crate::semantics::SemanticsRole> {
        composer.arena_nodes()[idx]
            .modifier
            .semantics_config()
            .role_value()
    }

    /// Labels differ per variant: `AssistChipTokens.LabelTextColor` is `OnSurface` while a suggestion
    /// chip's is `OnSurfaceVariant` — winia used the suggestion value for both.
    #[test]
    fn assist_and_suggestion_labels_differ() {
        let theme = crate::theme::WiniaTheme::colors();
        let assist = ChipDefaults::assist_chip_colors(&theme);
        let suggestion = ChipDefaults::suggestion_chip_colors(&theme);
        assert_eq!(assist.label, theme.on_surface);
        assert_eq!(suggestion.label, theme.on_surface_variant);
        assert_eq!(assist.leading_icon, theme.primary);
        assert_eq!(assist.trailing_icon, theme.on_surface_variant);
    }

    /// `FlatOutlineColor` is `OutlineVariant` (winia used `Outline`), and a disabled chip's outline is
    /// `OnSurface` @ 0.12. A selected chip has a ZERO-width border, which is why `resolve` returns `None`
    /// rather than a zero-width stroke.
    #[test]
    fn the_outline_follows_the_state_it_is_in() {
        let theme = crate::theme::WiniaTheme::colors();
        let flat = ChipBorder::flat(&theme);
        assert_eq!(flat.resolve(true, false), Some((1.0, theme.outline_variant)));
        let disabled = flat.resolve(false, false).expect("a disabled chip keeps its outline");
        assert_eq!(disabled.0, 1.0);
        assert_eq!(disabled.1.a, (theme.on_surface.a as f32 * 0.12) as u8);

        let selectable = ChipBorder::selectable(&theme);
        assert_eq!(selectable.resolve(true, true), None, "a selected chip draws no outline");
        assert_eq!(selectable.resolve(true, false), flat.resolve(true, false));
        assert_eq!(ChipBorder::none().resolve(true, false), None);
    }

    /// The elevation sets, in dp: a flat chip is 0 everywhere except while dragged (`Level4` = 8); a filter
    /// chip lifts 1 dp on hover (`FlatSelectedHoverContainerElevation` = `Level1`); the elevated variants are
    /// `Level1` at rest and `Level2` (3 dp) hovered.
    #[test]
    fn the_elevation_sets_are_the_tokens_in_dp() {
        assert_eq!(ChipElevation::flat().for_state(&crate::interaction::ComponentState::idle()), 0.0);
        assert_eq!(
            ChipElevation::flat().for_state(&crate::interaction::ComponentState {
                dragged: true,
                pressed: true,
                ..crate::interaction::ComponentState::idle()
            }),
            8.0,
            "a dragged chip rises to Level4, and the drag outranks the press it holds"
        );
        assert_eq!(
            ChipElevation::hover_lifting().for_state(&crate::interaction::ComponentState {
                hovered: true,
                ..crate::interaction::ComponentState::idle()
            }),
            1.0
        );
        assert_eq!(
            ChipElevation::elevated().for_state(&crate::interaction::ComponentState {
                hovered: true,
                ..crate::interaction::ComponentState::idle()
            }),
            3.0,
            "Level2 is 3 dp, not 2"
        );
        assert_eq!(
            ChipElevation::flat().for_state(&crate::interaction::ComponentState::disabled()),
            0.0
        );
        assert!(
            ChipElevation::new(0.0, 0.0, 0.0, 0.0, 0.0, 0.0).is_flat(),
            "only an all-zero set is shadow-free"
        );
        assert!(
            !ChipElevation::flat().is_flat(),
            "a flat chip still lifts to Level4 while dragged, so it keeps the layer"
        );
        assert!(!ChipElevation::elevated().is_flat());
    }

    /// A clickable chip reserves Compose's 48 dp touch target (`Surface(onClick)` applies
    /// `minimumInteractiveComponentSize()`), with the 32 dp container inside it. The input chip's padding is
    /// the asymmetric `start 4/8, end 8/4` rule, everything else is a flat 8 dp.
    #[test]
    fn a_clickable_chip_is_a_48_dp_target_and_the_padding_is_the_token_rule() {
        let (_composer, root, inner) = chip_nodes(Chip::assist(
            |ctx| crate::components::Text::new("Label").build(ctx),
            || {},
        ));
        let nodes = _composer.arena_nodes();
        assert_eq!(nodes[root].measured_size.height, 48.0, "the touch target is 48 dp tall");
        assert_eq!(nodes[inner].measured_size.height, ChipDefaults::HEIGHT, "the chip itself is 32 dp");

        // Padding: `padding_start`/`padding_end` push separate `PaddingSides` elements whose other sides are
        // `Auto`, so take the last FIXED value per side.
        let fixed = |v: &SizeValue| -> Option<f32> {
            match v {
                SizeValue::Static(Dimension::Fixed(dp)) => Some(*dp),
                _ => None,
            }
        };
        let padding = |idx: usize| -> (Option<f32>, Option<f32>) {
            let mut start = None;
            let mut end = None;
            for el in nodes[idx].modifier.elements() {
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
        };
        assert_eq!(
            padding(inner),
            (Some(8.0), Some(8.0)),
            "an assist chip pads 8 dp on both sides"
        );

        let (_c2, _r2, inner2) = chip_nodes(Chip::input(
            false,
            |ctx| crate::components::Text::new("Label").build(ctx),
            || {},
        ));
        let nodes2 = _c2.arena_nodes();
        let mut start = None;
        let mut end = None;
        for el in nodes2[inner2].modifier.elements() {
            if let crate::modifier::ModifierElement::PaddingSides { start: s, end: e, .. } = el {
                if let Some(v) = fixed(s) {
                    start = Some(v);
                }
                if let Some(v) = fixed(e) {
                    end = Some(v);
                }
            }
        }
        assert_eq!(
            (start, end),
            (Some(4.0), Some(4.0)),
            "an input chip with no icon pads 4 dp, the compact rule"
        );
    }

    /// Roles: an assist chip is a `Button`, a filter chip is a `Checkbox` that reports its selected state.
    #[test]
    fn the_roles_follow_the_variant() {
        let (_c, _root, inner) = chip_nodes(Chip::assist(
            |ctx| crate::components::Text::new("Label").build(ctx),
            || {},
        ));
        assert_eq!(role_of(&_c, inner), Some(crate::semantics::SemanticsRole::Button));

        let (_c2, _r2, inner2) = chip_nodes(Chip::filter(
            true,
            |ctx| crate::components::Text::new("Label").build(ctx),
            || {},
        ));
        assert_eq!(role_of(&_c2, inner2), Some(crate::semantics::SemanticsRole::Checkbox));
        assert_eq!(
            _c2.arena_nodes()[inner2]
                .modifier
                .semantics_config()
                .state_value()
                .selected_value(),
            Some(true),
            "and reports the selected state a screen reader needs"
        );

        let (_c3, _r3, inner3) = chip_nodes(Chip::suggestion(
            |ctx| crate::components::Text::new("Label").build(ctx),
            || {},
        ));
        assert_eq!(role_of(&_c3, inner3), Some(crate::semantics::SemanticsRole::Button));
    }

    /// The elevated variants fill with `SurfaceContainerLow` and carry no outline; the flat ones stay
    /// transparent with an `OutlineVariant` outline.
    #[test]
    fn elevated_chips_fill_and_drop_the_outline() {
        let theme = crate::theme::WiniaTheme::colors();
        let (_flat, _rf, flat_inner) = chip_nodes(Chip::assist(
            |ctx| crate::components::Text::new("Label").build(ctx),
            || {},
        ));
        assert_eq!(painted_background(&_flat, flat_inner), crate::graphics::Color::TRANSPARENT);
        assert!(border_of(&_flat, flat_inner).is_some(), "a flat chip is outlined");

        let (_elev, _re, elev_inner) = chip_nodes(Chip::elevated_assist(
            |ctx| crate::components::Text::new("Label").build(ctx),
            || {},
        ));
        assert_eq!(painted_background(&_elev, elev_inner), theme.surface_container_low);
        assert_eq!(border_of(&_elev, elev_inner), None, "an elevated chip has no outline by default");
    }

    /// An input chip keeps a `Primary` leading icon when selected; a filter chip's turns
    /// `OnSecondaryContainer` (`InputChipTokens.SelectedLeadingIconColor`).
    #[test]
    fn the_selected_leading_icon_differs_between_filter_and_input() {
        let theme = crate::theme::WiniaTheme::colors();
        assert_eq!(
            ChipDefaults::filter_chip_colors(&theme).selected_leading_icon,
            theme.on_secondary_container
        );
        assert_eq!(
            ChipDefaults::input_chip_colors(&theme).selected_leading_icon,
            theme.primary
        );
        assert_eq!(
            ChipDefaults::filter_chip_colors(&theme).disabled_selected_container.a,
            (theme.on_surface.a as f32 * 0.12) as u8,
            "a disabled selected container is OnSurface @ 0.12, not 0.38"
        );
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

    /// Every chip's focus indicator is `FocusIndicatorColor = Secondary` (all four token files). winia never
    /// declared one, so a focused chip got the framework's default ring — the grey halo in the report about a
    /// selected chip looking wrong.
    #[test]
    fn a_chip_declares_its_secondary_focus_ring() {
        let theme = crate::theme::WiniaTheme::colors();
        let (_c, _root, inner) = chip_nodes(Chip::filter(
            true,
            |ctx| crate::components::Text::new("Lunch").build(ctx),
            || {},
        ));
        assert_eq!(
            _c.arena_nodes()[inner].focus_color.get(),
            theme.secondary,
            "the chip's ring is `FocusIndicatorColor`"
        );
    }
}
