//! 主题系统 — Material Design 3 完整色彩方案 + dark-light 自动检测。
//!
//! ```ignore
//! WiniaTheme::auto(ctx, |ctx| {
//!     let c = WiniaTheme::colors();
//!     Text::new("Hello").color(c.primary).build(ctx);
//! });
//! ```

use crate::core::composition_local::CompositionLocal;
use crate::core::composer::ComposeCtx;
use crate::layout::LayoutDirection;
use crate::modifier::Color;
use crate::ui::text::{FontWeight, TextStyle};
use crate::unit::{Sp, TextUnit};
use material_colors::color::Argb;
use material_colors::theme::ThemeBuilder;
use std::sync::LazyLock;

// ═══════════════════════════════════════════════════════════
// 完整 Material 3 色彩方案（49 色槽，对齐 material-colors Scheme）
// ═══════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct ThemeColors {
    // ── Primary ──
    pub primary: Color,
    pub on_primary: Color,
    pub primary_container: Color,
    pub on_primary_container: Color,
    pub inverse_primary: Color,
    pub primary_fixed: Color,
    pub primary_fixed_dim: Color,
    pub on_primary_fixed: Color,
    pub on_primary_fixed_variant: Color,

    // ── Secondary ──
    pub secondary: Color,
    pub on_secondary: Color,
    pub secondary_container: Color,
    pub on_secondary_container: Color,
    pub secondary_fixed: Color,
    pub secondary_fixed_dim: Color,
    pub on_secondary_fixed: Color,
    pub on_secondary_fixed_variant: Color,

    // ── Tertiary ──
    pub tertiary: Color,
    pub on_tertiary: Color,
    pub tertiary_container: Color,
    pub on_tertiary_container: Color,
    pub tertiary_fixed: Color,
    pub tertiary_fixed_dim: Color,
    pub on_tertiary_fixed: Color,
    pub on_tertiary_fixed_variant: Color,

    // ── Error ──
    pub error: Color,
    pub on_error: Color,
    pub error_container: Color,
    pub on_error_container: Color,

    // ── Surface（7 层 elevation）──
    pub surface_dim: Color,
    pub surface: Color,
    pub surface_tint: Color,
    pub surface_bright: Color,
    pub surface_container_lowest: Color,
    pub surface_container_low: Color,
    pub surface_container: Color,
    pub surface_container_high: Color,
    pub surface_container_highest: Color,
    pub surface_variant: Color,

    // ── On Surface ──
    pub on_surface: Color,
    pub on_surface_variant: Color,

    // ── Outline ──
    pub outline: Color,
    pub outline_variant: Color,

    // ── Inverse ──
    pub inverse_surface: Color,
    pub inverse_on_surface: Color,

    // ── Background／Shadow／Scrim ──
    pub background: Color,
    pub on_background: Color,
    pub shadow: Color,
    pub scrim: Color,
}

impl ThemeColors {
    fn from_scheme(s: &material_colors::scheme::Scheme) -> Self {
        Self {
            primary:              argb(s.primary),
            on_primary:           argb(s.on_primary),
            primary_container:    argb(s.primary_container),
            on_primary_container: argb(s.on_primary_container),
            inverse_primary:      argb(s.inverse_primary),
            primary_fixed:        argb(s.primary_fixed),
            primary_fixed_dim:    argb(s.primary_fixed_dim),
            on_primary_fixed:     argb(s.on_primary_fixed),
            on_primary_fixed_variant: argb(s.on_primary_fixed_variant),

            secondary:              argb(s.secondary),
            on_secondary:           argb(s.on_secondary),
            secondary_container:    argb(s.secondary_container),
            on_secondary_container: argb(s.on_secondary_container),
            secondary_fixed:        argb(s.secondary_fixed),
            secondary_fixed_dim:    argb(s.secondary_fixed_dim),
            on_secondary_fixed:     argb(s.on_secondary_fixed),
            on_secondary_fixed_variant: argb(s.on_secondary_fixed_variant),

            tertiary:              argb(s.tertiary),
            on_tertiary:           argb(s.on_tertiary),
            tertiary_container:    argb(s.tertiary_container),
            on_tertiary_container: argb(s.on_tertiary_container),
            tertiary_fixed:        argb(s.tertiary_fixed),
            tertiary_fixed_dim:    argb(s.tertiary_fixed_dim),
            on_tertiary_fixed:     argb(s.on_tertiary_fixed),
            on_tertiary_fixed_variant: argb(s.on_tertiary_fixed_variant),

            error:              argb(s.error),
            on_error:           argb(s.on_error),
            error_container:    argb(s.error_container),
            on_error_container: argb(s.on_error_container),

            surface_dim:               argb(s.surface_dim),
            surface:                   argb(s.surface),
            surface_tint:              argb(s.surface_tint),
            surface_bright:            argb(s.surface_bright),
            surface_container_lowest:  argb(s.surface_container_lowest),
            surface_container_low:     argb(s.surface_container_low),
            surface_container:         argb(s.surface_container),
            surface_container_high:    argb(s.surface_container_high),
            surface_container_highest: argb(s.surface_container_highest),
            surface_variant:           argb(s.surface_variant),

            on_surface:         argb(s.on_surface),
            on_surface_variant: argb(s.on_surface_variant),

            outline:         argb(s.outline),
            outline_variant: argb(s.outline_variant),

            inverse_surface:    argb(s.inverse_surface),
            inverse_on_surface: argb(s.inverse_on_surface),

            background:    argb(s.background),
            on_background: argb(s.on_background),
            shadow:        argb(s.shadow),
            scrim:         argb(s.scrim),
        }
    }

    pub fn light_from_seed(seed: u32) -> Self {
        let theme = ThemeBuilder::with_source(Argb::from_u32(seed)).build();
        Self::from_scheme(&theme.schemes.light)
    }

    pub fn dark_from_seed(seed: u32) -> Self {
        let theme = ThemeBuilder::with_source(Argb::from_u32(seed)).build();
        Self::from_scheme(&theme.schemes.dark)
    }

    pub fn default_light() -> Self { Self::light_from_seed(0xff6750A4) }
    pub fn default_dark() -> Self  { Self::dark_from_seed(0xff6750A4) }
}

fn argb(a: material_colors::color::Argb) -> Color {
    Color::from_argb(a.alpha, a.red, a.green, a.blue)
}

// ═══════════════════════════════════════════════════════════
// 系统暗色模式检测（dark-light crate）
// ═══════════════════════════════════════════════════════════

/// 检测操作系统是否处于暗色模式。
/// 跨平台：Windows 读注册表、macOS 读 NSUserDefaults、Linux 读 freedesktop。
pub fn is_system_dark_theme() -> bool {
    match dark_light::detect().unwrap_or(dark_light::Mode::Unspecified) {
        dark_light::Mode::Dark => true,
        _ => false,
    }
}

// ═══════════════════════════════════════════════════════════
// Material Typography
// ═══════════════════════════════════════════════════════════

/// Material 3 type scale. Styles intentionally omit color and layout semantics;
/// components provide those through their own content-color and slot rules.
#[derive(Debug, Clone, PartialEq)]
pub struct Typography {
    pub display_large: TextStyle,
    pub display_medium: TextStyle,
    pub display_small: TextStyle,
    pub headline_large: TextStyle,
    pub headline_medium: TextStyle,
    pub headline_small: TextStyle,
    pub title_large: TextStyle,
    pub title_medium: TextStyle,
    pub title_small: TextStyle,
    pub body_large: TextStyle,
    pub body_medium: TextStyle,
    pub body_small: TextStyle,
    pub label_large: TextStyle,
    pub label_medium: TextStyle,
    pub label_small: TextStyle,
}

impl Typography {
    fn style(size: f32, line_height: f32, letter_spacing: f32, weight: FontWeight) -> TextStyle {
        TextStyle::new()
            .font_size(TextUnit::Sp(Sp(size)))
            .line_height(line_height)
            .letter_spacing(letter_spacing)
            .font_weight(weight)
    }

    /// Standard Material 3 `TypeScaleTokens` values.
    pub fn material3_default() -> Self {
        Self {
            display_large: Self::style(57.0, 64.0, -0.2, FontWeight::NORMAL),
            display_medium: Self::style(45.0, 52.0, 0.0, FontWeight::NORMAL),
            display_small: Self::style(36.0, 44.0, 0.0, FontWeight::NORMAL),
            headline_large: Self::style(32.0, 40.0, 0.0, FontWeight::NORMAL),
            headline_medium: Self::style(28.0, 36.0, 0.0, FontWeight::NORMAL),
            headline_small: Self::style(24.0, 32.0, 0.0, FontWeight::NORMAL),
            title_large: Self::style(22.0, 28.0, 0.0, FontWeight::NORMAL),
            title_medium: Self::style(16.0, 24.0, 0.2, FontWeight::MEDIUM),
            title_small: Self::style(14.0, 20.0, 0.1, FontWeight::MEDIUM),
            body_large: Self::style(16.0, 24.0, 0.5, FontWeight::NORMAL),
            body_medium: Self::style(14.0, 20.0, 0.2, FontWeight::NORMAL),
            body_small: Self::style(12.0, 16.0, 0.4, FontWeight::NORMAL),
            label_large: Self::style(14.0, 20.0, 0.1, FontWeight::MEDIUM),
            label_medium: Self::style(12.0, 16.0, 0.5, FontWeight::MEDIUM),
            label_small: Self::style(11.0, 16.0, 0.5, FontWeight::MEDIUM),
        }
    }
}

impl Default for Typography {
    fn default() -> Self {
        Self::material3_default()
    }
}

// ═══════════════════════════════════════════════════════════
// CompositionLocal
// ═══════════════════════════════════════════════════════════

static LOCAL_COLORS: LazyLock<CompositionLocal<ThemeColors>> = LazyLock::new(|| {
    CompositionLocal::new(|| ThemeColors::default_light())
});

static LOCAL_DIRECTION: LazyLock<CompositionLocal<LayoutDirection>> = LazyLock::new(|| {
    CompositionLocal::new(|| LayoutDirection::Ltr)
});

static LOCAL_TYPOGRAPHY: LazyLock<CompositionLocal<Typography>> = LazyLock::new(|| {
    CompositionLocal::new(Typography::default)
});

/// 内容色（对标 Compose `LocalContentColor`）——Icon 等内容组件默认取
/// 当前子树内容色；`with_theme*` 子树内默认 on_surface，容器组件
/// （如 IconButton）可覆盖。未包裹主题时回退纯黑（与 Compose 默认一致）。
static LOCAL_CONTENT_COLOR: LazyLock<CompositionLocal<Color>> =
    LazyLock::new(|| CompositionLocal::new(|| Color::BLACK));

// ═══════════════════════════════════════════════════════════
// 主题入口
// ═══════════════════════════════════════════════════════════

pub struct WiniaTheme;

impl WiniaTheme {
    /// 自动检测系统暗色模式并在子树中提供对应主题。
    pub fn auto(ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        let colors = if is_system_dark_theme() {
            ThemeColors::default_dark()
        } else {
            ThemeColors::default_light()
        };
        Self::with_theme(colors, ctx, content);
    }

    /// 在子树中提供亮色主题。
    pub fn light(ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        Self::with_theme(ThemeColors::default_light(), ctx, content);
    }

    /// 在子树中提供暗色主题。
    pub fn dark(ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        Self::with_theme(ThemeColors::default_dark(), ctx, content);
    }

    /// 在子树中提供自定义颜色方案 + LTR 方向。
    pub fn with_theme(colors: ThemeColors, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        Self::with_theme_typography_and_direction(colors, Typography::default(), LayoutDirection::Ltr, ctx, content);
    }

    /// 在子树中提供自定义颜色方案、Typography 和 LTR 方向。
    pub fn with_theme_and_typography(
        colors: ThemeColors,
        typography: Typography,
        ctx: &mut ComposeCtx,
        content: impl FnOnce(&mut ComposeCtx),
    ) {
        Self::with_theme_typography_and_direction(colors, typography, LayoutDirection::Ltr, ctx, content);
    }

    /// 在子树中提供自定义颜色方案和布局方向。
    pub fn with_theme_and_direction(colors: ThemeColors, direction: LayoutDirection, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        Self::with_theme_typography_and_direction(colors, Typography::default(), direction, ctx, content);
    }

    /// 在子树中提供完整的颜色、Typography 与布局方向主题。
    pub fn with_theme_typography_and_direction(
        colors: ThemeColors,
        typography: Typography,
        direction: LayoutDirection,
        ctx: &mut ComposeCtx,
        content: impl FnOnce(&mut ComposeCtx),
    ) {
        let on_surface = colors.on_surface;
        LOCAL_DIRECTION.provides(direction, || {
            LOCAL_COLORS.provides(colors, || {
                LOCAL_TYPOGRAPHY.provides(typography, || {
                    LOCAL_CONTENT_COLOR.provides(on_surface, || {
                        content(ctx);
                    });
                });
            });
        });
    }

    /// 在子树中覆盖 Material Typography token，不改变颜色或布局方向。
    pub fn with_typography(typography: Typography, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        LOCAL_TYPOGRAPHY.provides(typography, || content(ctx));
    }

    /// 在子树中覆盖内容色（对标 Compose `CompositionLocalProvider(LocalContentColor)`）
    pub fn with_content_color(color: Color, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        LOCAL_CONTENT_COLOR.provides(color, || content(ctx));
    }

    /// 读取当前子树主题色。
    pub fn colors() -> ThemeColors {
        LOCAL_COLORS.current()
    }

    /// 读取当前布局方向（Ltr 或 Rtl）。
    pub fn direction() -> LayoutDirection {
        LOCAL_DIRECTION.current()
    }

    /// 读取当前 Material Typography token。
    pub fn typography() -> Typography {
        LOCAL_TYPOGRAPHY.current()
    }

    /// 读取当前子树内容色（默认主题 on_surface）
    pub fn content_color() -> Color {
        LOCAL_CONTENT_COLOR.current()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::text::Text;

    #[test]
    fn material3_typography_matches_list_item_tokens() {
        let typography = Typography::default();
        assert_eq!(typography.body_large.font_size, Some(TextUnit::Sp(Sp(16.0))));
        assert_eq!(typography.body_large.line_height, Some(TextUnit::Sp(crate::unit::Sp(24.0))));
        assert_eq!(typography.body_large.letter_spacing, Some(0.5));
        assert_eq!(typography.body_medium.font_size, Some(TextUnit::Sp(Sp(14.0))));
        assert_eq!(typography.label_small.font_size, Some(TextUnit::Sp(Sp(11.0))));
        assert_eq!(typography.label_small.font_weight, Some(FontWeight::MEDIUM));
    }

    #[test]
    fn typography_local_nests_and_restores() {
        let custom = Typography {
            body_large: TextStyle::new().font_size(20.0),
            ..Typography::default()
        };
        let mut composer = crate::core::composer::Composer::new();
        composer.compose(|ctx| {
            let default = WiniaTheme::typography();
            WiniaTheme::with_typography(custom.clone(), ctx, |ctx| {
                assert_eq!(WiniaTheme::typography(), custom);
            });
            assert_eq!(WiniaTheme::typography(), default);
        });
    }

    #[test]
    fn legacy_theme_keeps_bare_text_default_size() {
        let mut composer = crate::core::composer::Composer::new();
        composer.compose(|ctx| {
            WiniaTheme::light(ctx, |ctx| {
                Text::new("unchanged").build(ctx);
            });
        });
        let root = composer.layout_root_idx().unwrap();
        let nodes = composer.arena_nodes();
        let size = nodes[root].modifier.elements().iter().find_map(|el| {
            if let crate::modifier::ModifierElement::TextContent { font_size, .. } = el {
                Some(*font_size)
            } else {
                None
            }
        }).unwrap();
        assert_eq!(size, 14.0, "旧主题入口不得将裸 Text 自动改为 BodyLarge");
    }
}
