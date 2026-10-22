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
    /// `FlatDisabledOutlineOpacity` / `FlatDisabledUnselectedOutlineOpacity` /
    /// `DisabledUnselectedOutlineOpacity` — every flat chip's disabled outline is `OnSurface` at 12 %.
    pub const DISABLED_OUTLINE_OPACITY: f32 = 0.12;
    /// `FlatDisabledSelectedContainerOpacity` (filter) / `DisabledSelectedContainerOpacity` (input) — a
    /// disabled SELECTED chip's container is `OnSurface` at 12 %, the same value as its outline.
    pub const DISABLED_SELECTED_CONTAINER_OPACITY: f32 = 0.12;

    /// The disabled outline's colour: `OnSurface` at `DISABLED_OUTLINE_OPACITY`.
    pub fn disabled_outline(theme: &crate::theme::ThemeColors) -> crate::graphics::Color {
        let alpha = (255.0 * Self::DISABLED_OUTLINE_OPACITY).round() as u8;
        crate::graphics::Color::from_argb(alpha, theme.on_surface.r, theme.on_surface.g, theme.on_surface.b)
    }
    /// How long a chip fades between its container colours. A winia addition: M3's `Chip.kt` applies the colour
    /// directly. The shadow's gate reads this same animation (see `Chip::build`), so a chip whose container is
    /// transparent — or still on its way to being filled — shows no shadow.
    pub const CONTAINER_FADE_MILLIS: u64 = 180;
    /// The container alpha at which a shadow may be drawn at all. Below it the chip's background reads as
    /// transparent, and a shadow behind a transparent fill looks like a smudge rather than a lift.
    const SHADOW_OPAQUE_ALPHA: u8 = 250;

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
            // A disabled SELECTED container is `OnSurface` at 12 % — `FlatDisabledSelectedContainerColor` /
            // `FlatDisabledSelectedContainerOpacity` (filter) and `DisabledSelectedContainerColor` /
            // `DisabledSelectedContainerOpacity` (input). winia painted 38 % here.
            disabled_selected_container: crate::graphics::Color::from_argb(
                (255.0 * Self::DISABLED_SELECTED_CONTAINER_OPACITY).round() as u8,
                theme.on_surface.r,
                theme.on_surface.g,
                theme.on_surface.b,
            ),
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

    /// The elevated variants' container (`SurfaceContainerLow`, the specs page's "surface container low
    /// (optional)" role) and the disabled colours they come with.
    pub fn elevated_assist_chip_colors(theme: &crate::theme::ThemeColors) -> ChipColors {
        ChipColors {
            container: theme.surface_container_low,
            disabled_container: crate::graphics::Color::from_argb(31, theme.on_surface.r, theme.on_surface.g, theme.on_surface.b),
            ..Self::assist_chip_colors(theme)
        }
    }

    pub fn elevated_suggestion_chip_colors(theme: &crate::theme::ThemeColors) -> ChipColors {
        ChipColors {
            label: theme.on_surface_variant,
            ..Self::elevated_assist_chip_colors(theme)
        }
    }

    pub fn elevated_selectable_chip_colors(theme: &crate::theme::ThemeColors) -> SelectableChipColors {
        SelectableChipColors {
            container: theme.surface_container_low,
            disabled_container: crate::graphics::Color::from_argb(31, theme.on_surface.r, theme.on_surface.g, theme.on_surface.b),
            ..Self::selectable_chip_colors(theme)
        }
    }

    pub fn elevated_input_chip_colors(theme: &crate::theme::ThemeColors) -> SelectableChipColors {
        SelectableChipColors {
            selected_leading_icon: theme.primary,
            ..Self::elevated_selectable_chip_colors(theme)
        }
    }
}

// ═══════════════════════════════════════════════════════════
// ChipElevation — 阴影高度（六个交互状态）
// ═══════════════════════════════════════════════════════════

/// A chip's shadow elevation per interaction state, in dp — material3's `ChipElevation` /
/// `SelectableChipElevation`, taken from the token LEVELS (`ElevationTokens`: Level0 0, Level1 1, Level2 3,
/// Level3 6, Level4 8).
///
/// winia draws a shadow ONLY while the container is opaque (see `Chip::build`): a chip whose background is
/// transparent shows nothing, whatever its token value says, because a shadow behind a transparent fill reads as
/// a smudge rather than a lift.
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

    /// `assistChipElevation()` / `suggestionChipElevation()` / `inputChipElevation()`: flat (Level0) in every
    /// state, lifted to Level4 (8 dp) only while DRAGGED.
    pub fn flat() -> Self {
        Self::new(0.0, 0.0, 0.0, 0.0, 8.0, 0.0)
    }

    /// `filterChipElevation()`: like `flat()`, plus `FlatSelectedHoverContainerElevation` = Level1 (1 dp) on
    /// hover. It only shows while the chip is filled — an unselected filter chip's container is transparent.
    pub fn hover_lifting() -> Self {
        Self::new(0.0, 0.0, 0.0, 1.0, 8.0, 0.0)
    }

    /// The `Elevated*` variants: `ElevatedContainerElevation` Level1 (1 dp) at rest, pressed and focused,
    /// Level2 (3 dp) hovered, Level0 disabled, Level4 (8 dp) dragged.
    pub fn elevated() -> Self {
        Self::new(1.0, 1.0, 1.0, 3.0, 8.0, 0.0)
    }

    /// The elevation for a state snapshot, latest interaction first — the order the Button and Card use, since
    /// `animateElevation` resolves it from the last interaction.
    pub fn for_state(&self, state: &crate::input::interaction::ComponentState) -> f32 {
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

    /// Every state is zero.
    pub fn is_flat(&self) -> bool {
        self.default <= 0.0
            && self.pressed <= 0.0
            && self.focused <= 0.0
            && self.hovered <= 0.0
            && self.dragged <= 0.0
            && self.disabled <= 0.0
    }

    /// What a chip actually paints: its state's elevation, or NOTHING while the container reads as transparent.
    ///
    /// A shadow behind a transparent fill looks like a smudge rather than a lift, so the alpha decides:
    /// `container_alpha` is the container colour the renderer is showing right now, which during the selection
    /// fade is the interpolated one — the transparent-to-filled direction therefore shows no shadow until the
    /// fill has arrived, and the filled-to-transparent direction drops it as soon as the fill starts leaving.
    pub fn shadow_for(state: &crate::input::interaction::ComponentState, container_alpha: u8, elevation: &ChipElevation) -> f32 {
        if container_alpha < ChipDefaults::SHADOW_OPAQUE_ALPHA {
            0.0
        } else {
            elevation.for_state(state)
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
    /// 填充 + 抬升的辅助 chip（ElevatedAssistChip）
    ElevatedAssist,
    /// 筛选（FilterChip）
    Filter,
    /// 填充 + 抬升的筛选 chip（ElevatedFilterChip）
    ElevatedFilter,
    /// 输入信息（InputChip）
    Input,
    /// 建议（SuggestionChip）
    Suggestion,
    /// 填充 + 抬升的建议 chip（ElevatedSuggestionChip）
    ElevatedSuggestion,
}

impl ChipVariant {
    /// Filter and input chips carry a `selected` state.
    pub fn is_selectable(self) -> bool {
        matches!(self, ChipVariant::Filter | ChipVariant::ElevatedFilter | ChipVariant::Input)
    }

    /// The `Elevated*` variants: `SurfaceContainerLow` container and no outline by default.
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
    /// Shadow elevation (`None` = the variant's token default; `ChipElevation?` upstream).
    elevation: Option<ChipElevation>,
    /// A caller-supplied interaction source (`interactionSource: MutableInteractionSource?` upstream). When it
    /// is absent the component remembers one of its own.
    interaction_source: Option<crate::input::interaction::MutableInteractionSource>,
}

impl Chip {
    /// 辅助操作 chip（对标 AssistChip）
    pub fn assist(label: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static, on_click: impl Fn() + Send + Sync + 'static) -> Self {
        Self::new(ChipVariant::Assist, false, label, on_click)
    }

    /// 填充 + 抬升的辅助 chip（对标 ElevatedAssistChip）
    pub fn elevated_assist(label: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static, on_click: impl Fn() + Send + Sync + 'static) -> Self {
        Self::new(ChipVariant::ElevatedAssist, false, label, on_click)
    }

    /// 筛选 chip（对标 FilterChip）——selected 切换
    pub fn filter(selected: bool, label: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static, on_click: impl Fn() + Send + Sync + 'static) -> Self {
        Self::new(ChipVariant::Filter, selected, label, on_click)
    }

    /// 填充 + 抬升的筛选 chip（对标 ElevatedFilterChip）
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

    /// 填充 + 抬升的建议 chip（对标 ElevatedSuggestionChip）
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
    /// Shadow elevation (`ChipElevation?`; `ChipElevation::flat()` removes it in the states that can show one).
    pub fn elevation(mut self, e: ChipElevation) -> Self { self.elevation = Some(e); self }
    /// A caller-supplied interaction source (`interactionSource: MutableInteractionSource?`), so a test or a
    /// parent component can put a state in place before the first composition — the same parameter every other
    /// clickable in winia exposes.
    pub fn interaction_source(
        mut self,
        source: crate::input::interaction::MutableInteractionSource,
    ) -> Self {
        self.interaction_source = Some(source);
        self
    }
    pub fn leading_icon(mut self, f: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self { self.leading_icon = Some(Box::new(f)); self }
    pub fn trailing_icon(mut self, f: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self { self.trailing_icon = Some(Box::new(f)); self }
    /// 头像（InputChip——24dp、12dp 圆角）
    pub fn avatar(mut self, f: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self { self.avatar = Some(Box::new(f)); self }
    /// 建议图标（SuggestionChip 的 icon——等效 leading）
    pub fn icon(mut self, f: impl FnOnce(&mut ComposeCtx) + Send + Sync + 'static) -> Self { self.leading_icon = Some(Box::new(f)); self }

    pub fn build(self, ctx: &mut ComposeCtx) {
        let theme = crate::theme::WiniaTheme::colors();
        let sel = self.selected;
        // Colour roles come from the M3 specs page (m3.material.io/components/chips/specs), one list per
        // variant: an assist chip's label is `On surface` and a suggestion chip's is `On surface variant`;
        // assist and suggestion outline with `Outline` while filter and input use `Outline variant`.
        //
        // A DISABLED chip's outline is `OnSurface` at 12 % — `FlatDisabledOutlineColor` /
        // `FlatDisabledOutlineOpacity` (assist, suggestion), `FlatDisabledUnselectedOutlineColor` / `…Opacity`
        // (filter) and `DisabledUnselectedOutlineColor` / `…Opacity` (input) — not the enabled role's colour.
        let disabled_outline = ChipDefaults::disabled_outline(&theme);
        let (container, label_color, icon_color, border_w, border_color) = match self.variant {
            ChipVariant::Assist
            | ChipVariant::ElevatedAssist
            | ChipVariant::Suggestion
            | ChipVariant::ElevatedSuggestion => {
                let c = self.flat_colors.unwrap_or_else(|| match self.variant {
                    ChipVariant::ElevatedAssist => ChipDefaults::elevated_assist_chip_colors(&theme),
                    ChipVariant::ElevatedSuggestion => ChipDefaults::elevated_suggestion_chip_colors(&theme),
                    ChipVariant::Suggestion => ChipDefaults::suggestion_chip_colors(&theme),
                    _ => ChipDefaults::assist_chip_colors(&theme),
                });
                let (container, label, icon) = if self.enabled {
                    (c.container, c.label, c.leading_icon)
                } else {
                    (c.disabled_container, c.disabled_label, c.disabled_label)
                };
                // A flat assist or suggestion chip outlines with `Outline`; the elevated variants carry no
                // outline by default. Disabled swaps in the 12 % `OnSurface` role above.
                let w = if self.variant.is_elevated() { 0.0 } else { ChipDefaults::BORDER_WIDTH };
                let colour = if self.enabled { theme.outline } else { disabled_outline };
                (container, label, icon, w, colour)
            }
            ChipVariant::Filter | ChipVariant::ElevatedFilter | ChipVariant::Input => {
                let c = self.selectable_colors.unwrap_or_else(|| match self.variant {
                    ChipVariant::ElevatedFilter => ChipDefaults::elevated_selectable_chip_colors(&theme),
                    ChipVariant::Input => ChipDefaults::input_chip_colors(&theme),
                    _ => ChipDefaults::selectable_chip_colors(&theme),
                });
                let (container, label, icon) = if !self.enabled {
                    // A disabled chip keeps its selected/unselected distinction: selected fills with the 12 %
                    // `OnSurface` role, unselected stays transparent and carries the outline instead. winia
                    // used the transparent one for both, so a disabled selected chip lost its fill entirely.
                    let container = if sel { c.disabled_selected_container } else { c.disabled_container };
                    (container, c.disabled_label, c.disabled_label)
                } else if sel {
                    (c.selected_container, c.selected_label, c.selected_leading_icon)
                } else {
                    (c.container, c.label, c.leading_icon)
                };
                // A selected chip draws no outline; unselected draws 1 dp of `Outline variant` (or the disabled
                // role above), and the elevated variants carry none by default.
                let w = if sel || self.variant.is_elevated() {
                    0.0
                } else {
                    ChipDefaults::BORDER_WIDTH
                };
                let colour = if self.enabled { theme.outline_variant } else { disabled_outline };
                (container, label, icon, w, colour)
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

        // 阴影：`ChipElevation` 按交互状态取值（token 层级换算成 dp），用与 Button/Card 相同的
        // `elevation_spec_for_pair` 配对规则升降；**但只在容器不透明时才画**——透明背景后面的阴影看起来
        // 是一块脏印，而不是抬升（用户明确要求）。容器色是渐变的，所以"是否透明"问的是动画色本身，
        // 这同时给出了过渡期的行为：变透明时阴影立即消失，变不透明时等颜色到位才出现。
        let elevation = self.elevation.unwrap_or_else(|| match self.variant {
            ChipVariant::ElevatedAssist | ChipVariant::ElevatedFilter
            | ChipVariant::ElevatedSuggestion => ChipElevation::elevated(),
            ChipVariant::Filter | ChipVariant::ElevatedFilter => ChipElevation::hover_lifting(),
            _ => ChipElevation::flat(),
        });
        let container_anim = ctx.animate_color_as_state(
            container,
            crate::animation::AnimationSpec::Tween(crate::animation::TweenSpec::new(
                std::time::Duration::from_millis(ChipDefaults::CONTAINER_FADE_MILLIS),
                crate::animation::interpolator::EaseOutCubic::new(),
            )),
        );
        let interaction = self
            .interaction_source
            .clone()
            .unwrap_or_else(|| ctx.remember(|| crate::input::interaction::MutableInteractionSource::new()).get());
        let state = interaction.state(self.enabled);
        let last_kind: crate::runtime::state::Backchannel<
            Option<crate::animation::ElevationInteraction>,
        > = ctx.remember_backchannel(|| None);
        let last_shadow: crate::runtime::state::Backchannel<f32> = ctx.remember_backchannel(|| 0.0);
        let elevation_anim = if elevation.is_flat() {
            None
        } else {
            // Reading the animated colour is what makes this build run on every colour frame, so the gate
            // releases on the frame the fill lands. `ChipElevation::shadow_for` holds the rule.
            let shown_colour = container_anim.get();
            let wanted = ChipElevation::shadow_for(&state, shown_colour.a, &elevation);
            let shown = last_shadow.peek();
            let to = state.elevation_interaction();
            let from = if state.enabled { last_kind.peek() } else { None };
            // The tracker records the CURRENT interaction on every change, falls included. `from` only matters
            // for a rise that carries no interaction kind (a lift caused by the container becoming opaque),
            // where `elevation_spec_for_pair` would otherwise pick the hover-outgoing curve for the stale kind.
            last_kind.set(to);
            let spec = if wanted <= shown {
                crate::animation::AnimationSpec::Snap(crate::animation::SnapSpec::default())
            } else {
                crate::animation::elevation_spec_for_pair(from, to)
                    .unwrap_or(crate::animation::AnimationSpec::Snap(crate::animation::SnapSpec::default()))
            };
            last_shadow.set(wanted);
            Some(ctx.animate_float_as_state(wanted, spec))
        };
        build_chip(
            ctx,
            self.modifier.padding_start(pad.0).padding_end(pad.1),
            self.enabled,
            container_anim,
            interaction,
            elevation_anim,
            border_color,
            border_w,
            self.shape,
            label_color,
            on_click,
            // material3's roles: `Chip.kt:1963` gives the non-selectable chip `Role.Button`, and the
            // selectable one `Role.Checkbox` (`:2010`, whose `Surface(selected = …)` publishes the selected
            // state beside the role).
            {
                let mut config = crate::semantics::SemanticsConfig::new()
                    .role(if self.variant.is_selectable() {
                        crate::semantics::SemanticsRole::Checkbox
                    } else {
                        crate::semantics::SemanticsRole::Button
                    })
                    .merge_descendants(true)
                    .state(crate::semantics::SemanticsState::new().enabled(self.enabled));
                if self.variant.is_selectable() {
                    config = config.state(crate::semantics::SemanticsState::new().selected(sel));
                }
                config
            },
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
    container_anim: crate::runtime::state::State<crate::graphics::Color>,
    interaction: crate::input::interaction::MutableInteractionSource,
    elevation_anim: Option<crate::runtime::state::State<f32>>,
    border_color: crate::graphics::Color,
    border_width: f32,
    shape: Shape,
    content_color: crate::graphics::Color,
    on_click: Option<std::sync::Arc<dyn Fn() + Send + Sync>>,
    semantics: crate::semantics::SemanticsConfig,
    content: impl FnOnce(&mut ComposeCtx),
) {
    let key = ctx.next_key();
    // The background accepts a dynamic colour (`impl Into<BackgroundColor>`) evaluated per paint:
    // an animation tick advances the State and requests a redraw, so the renderer reads the latest
    // interpolated value. Passing `bg_anim.get()` — the VALUE — would freeze it at build time.
    let bg = container_anim.clone();
    let mut m = Modifier::new()
        .min_height(ChipDefaults::HEIGHT)
        .background(move || bg.peek(), shape);
    // ⚠ 有填充色（selected/secondaryContainer 等）时边框必须消失（M3 语义）——
    // border_width=0 时**不挂** border modifier（挂 0 宽线仍会绘制、可见）
    if border_width > 0.0 {
        m = m.border(border_width, border_color, shape);
    }
    // 阴影走 graphics_layer 动态闭包：渲染期每帧读动画值，形状跟随 chip shape。
    if let Some(anim) = elevation_anim {
        m = m.graphics_layer(move || crate::graphics::GraphicsLayerParams {
            shadow_elevation: anim.peek(),
            shadow_shape: Some(shape),
            ..Default::default()
        });
    }
    if enabled {
        if let Some(cb) = on_click {
            let cb2 = cb.clone();
            m = m
                .clickable_with_source(&interaction, move || cb2())
                .ripple_with_shape(&interaction, content_color, true, shape);
        }
    }
    // Accessibility: one element per chip (its label and icons are inside it), with the role the variant
    // carries and the state a reader needs — `selected` for a selectable chip, `enabled` for all of them.
    m = m.semantics(semantics);
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
mod tests;
