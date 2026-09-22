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

/// The stored theme mode: `0` = follow the system (the initial state), `1` = forced light, `2` = forced
/// dark. Written by [`set_system_dark_mode`] — an application that wants to pin its theme, or a test.
/// The platform's own `WindowEvent::ThemeChanged` does NOT pin anything: it OBSERVES the system, which is
/// why it is recorded in a separate slot below.
static SYSTEM_THEME_MODE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
/// The last value the platform reported for the system theme: `-1` = never reported, `0` = light,
/// `1` = dark. Consulted only while the mode follows the system; [`is_system_dark_theme`] falls back to
/// detection, which is also the only source on platforms whose event never fires (X11, Wayland).
static SYSTEM_THEME_DARK: std::sync::atomic::AtomicI8 = std::sync::atomic::AtomicI8::new(-1);
/// The mode as a REACTIVE value, and the one the composition actually reads: a tracked read of it
/// makes every scope that composed with `WiniaTheme::auto` a dependent, so a mode change invalidates
/// exactly those scopes through the ordinary state-dependency machinery. The atomic above stays for the
/// callers that ask outside a composition (`is_system_dark_theme` from the app loop).
static SYSTEM_THEME_STATE: std::sync::LazyLock<crate::core::state::Reactive<u8>> =
    std::sync::LazyLock::new(|| crate::core::state::Reactive::new(system_theme_mode()));
/// Set when the resolved theme may have changed and the tree has not been told yet — the app loop drains
/// it to run [`crate::app::apply_system_theme`], which refreshes the window's own theme snapshot and
/// re-composes the tree.
static SYSTEM_THEME_DIRTY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The pinned mode (`0` = follow the system, `1` = light, `2` = dark).
fn system_theme_mode() -> u8 {
    SYSTEM_THEME_MODE.load(std::sync::atomic::Ordering::Relaxed)
}

/// Pin the theme, or hand it back to the system with `None`.
///
/// The change lands on the next frame: this only stores the value and raises the dirty flag the app
/// loop watches (running a full recompose from arbitrary code would be a re-entrancy hazard, and the
/// loop is where the composers live). `WiniaTheme::auto` reads the stored value, so the next
/// composition picks it up.
pub fn set_system_dark_mode(mode: Option<bool>) {
    use std::sync::atomic::Ordering;
    let value = match mode {
        None => 0,
        Some(false) => 1,
        Some(true) => 2,
    };
    if SYSTEM_THEME_MODE.swap(value, Ordering::Relaxed) != value {
        // Wake every composition that read the mode (the tracked read in `WiniaTheme::auto`), and flag
        // the app loop so it can refresh the window's own theme snapshot.
        SYSTEM_THEME_STATE.set(value);
        SYSTEM_THEME_DIRTY.store(true, Ordering::Relaxed);
    }
}

/// Record what the platform reported for the system theme — `WindowEvent::ThemeChanged`.
///
/// An observation, not a pin: the platform value is what "follow the system" resolves to, and pinning it
/// would freeze the app on the first report (a later system change would then be ignored, because an
/// explicit pin outranks detection by design). While an application has pinned a mode this only records
/// the value — nothing the user sees can change.
pub(crate) fn note_platform_theme(dark: bool) {
    use std::sync::atomic::Ordering;
    let value = if dark { 1i8 } else { 0i8 };
    if SYSTEM_THEME_DARK.swap(value, Ordering::Relaxed) != value && system_theme_mode() == 0 {
        SYSTEM_THEME_DIRTY.store(true, Ordering::Relaxed);
    }
}

/// Whether a theme change is waiting to be applied (the app loop's check).
pub(crate) fn take_system_theme_dirty() -> bool {
    SYSTEM_THEME_DIRTY.swap(false, std::sync::atomic::Ordering::Relaxed)
}

/// Detect whether the system is in dark mode — or answer with the mode an application pinned through
/// [`set_system_dark_mode`].
///
/// Order: a pinned mode wins; then the value the platform reported; then a fresh detection
/// (cross-platform: the registry on Windows, NSUserDefaults on macOS, freedesktop on Linux). Note that
/// the platform's live theme event exists on Windows and macOS only; elsewhere a pinned mode is the only
/// way to follow a change without the app asking again.
pub fn is_system_dark_theme() -> bool {
    use std::sync::atomic::Ordering;
    match SYSTEM_THEME_MODE.load(Ordering::Relaxed) {
        1 => false,
        2 => true,
        _ => match SYSTEM_THEME_DARK.load(Ordering::Relaxed) {
            1 => true,
            0 => false,
            _ => matches!(dark_light::detect().unwrap_or(dark_light::Mode::Unspecified), dark_light::Mode::Dark),
        },
    }
}

/// The palette the system theme resolves to right now (the default M3 palettes; a pinned mode outranks
/// the system).
fn system_theme_colors() -> ThemeColors {
    if is_system_dark_theme() { ThemeColors::default_dark() } else { ThemeColors::default_light() }
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

/// How a window resolves its theme — the INTENT, captured where the window is created, as opposed to the
/// palette in force at that moment.
///
/// A window's content closure runs on every frame, so a palette captured as a value pins that window to
/// the colors it started with: measured on the demo, an `auto` window kept its dark colors while the
/// surface behind it flipped to light, because the per-frame wrapper re-provided the palette sampled when
/// the window was created. Recording the intent instead lets the wrapper resolve afresh each frame
/// (`Auto`), or re-provide a palette the application chose (`Fixed`).
#[derive(Clone, Debug)]
pub enum ThemeSpec {
    /// Follow the system — or the mode an application pinned through [`set_system_dark_mode`].
    Auto,
    /// Always this palette.
    Fixed(ThemeColors),
}

impl ThemeSpec {
    /// The palette this spec resolves to right now.
    pub fn colors(&self) -> ThemeColors {
        match self {
            ThemeSpec::Auto => system_theme_colors(),
            ThemeSpec::Fixed(colors) => colors.clone(),
        }
    }

    /// Provide this spec's palette for `content`, resolving it now — the per-frame wrapper a window's
    /// content composes under.
    pub(crate) fn provide(&self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        match self {
            ThemeSpec::Auto => WiniaTheme::auto(ctx, content),
            ThemeSpec::Fixed(colors) => WiniaTheme::with_theme(colors.clone(), ctx, content),
        }
    }
}

thread_local! {
    /// The spec of the theme the running composition sits inside. A `Window` node samples it so the
    /// window's own content can keep resolving the same way on later frames.
    static CURRENT_THEME_SPEC: std::cell::RefCell<Option<ThemeSpec>> = std::cell::RefCell::new(None);
}

/// The theme spec of the enclosing composition, for a caller that has to carry it into a composition of
/// its own (a window). Falls back to the palette in force, so content created outside any theme node
/// behaves as it always did.
pub(crate) fn current_theme_spec() -> ThemeSpec {
    CURRENT_THEME_SPEC.with(|s| s.borrow().clone()).unwrap_or_else(|| ThemeSpec::Fixed(WiniaTheme::colors()))
}

/// Restores the enclosing spec when a theme provide ends (also on the panic path).
struct SpecGuard(Option<ThemeSpec>);

impl Drop for SpecGuard {
    fn drop(&mut self) {
        CURRENT_THEME_SPEC.with(|s| *s.borrow_mut() = self.0.take());
    }
}

pub struct WiniaTheme;

impl WiniaTheme {
    /// 自动检测系统暗色模式并在子树中提供对应主题。
    pub fn auto(ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        // A TRACKED read: this scope becomes a dependent of the mode, so `set_system_dark_mode` (or the
        // platform's ThemeChanged) invalidates exactly the compositions that resolved a theme.
        let _ = SYSTEM_THEME_STATE.get();
        Self::provide_spec(ThemeSpec::Auto, Typography::default(), LayoutDirection::Ltr, ctx, content);
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
        Self::provide_spec(ThemeSpec::Fixed(colors), typography, direction, ctx, content);
    }

    /// The one place a theme reaches the locals: resolve the spec, provide the four locals, and record
    /// the spec for whoever has to carry this theme into a composition of its own (a window's content,
    /// which runs outside this closure on later frames).
    fn provide_spec(
        spec: ThemeSpec,
        typography: Typography,
        direction: LayoutDirection,
        ctx: &mut ComposeCtx,
        content: impl FnOnce(&mut ComposeCtx),
    ) {
        let colors = spec.colors();
        let on_surface = colors.on_surface;
        let _restore = SpecGuard(CURRENT_THEME_SPEC.with(|s| s.borrow_mut().replace(spec)));
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

    /// The theme mode is process-wide, so the tests that move it must not overlap.
    static THEME_TEST_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// A `surface`-filled 40×40 leaf under a param-less wrapper group — the smallest shape that shows both
    /// whether a theme reached the tree and whether an unmarked wrapper kept the colors it composed with.
    fn surface_scene(ctx: &mut crate::core::composer::ComposeCtx) {
        use crate::core::composer::GroupStatus;
        use crate::layout::BoxLayout;
        use crate::modifier::{Modifier, Shape};
        let key = ctx.next_key();
        match ctx.start_restartable_group(key, Modifier::new(), BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                let leaf = ctx.next_key();
                ctx.start_leaf(
                    leaf,
                    Modifier::new()
                        .size(40.0, 40.0)
                        .background(WiniaTheme::colors().surface, Shape::Rectangle),
                );
                ctx.end_node();
            }
        }
        ctx.end_restartable_group();
    }

    /// B channel of the centre pixel of `surface_scene` drawn into a 40×40 raster (BGRA).
    fn centre_pixel(composer: &mut crate::core::composer::Composer) -> i32 {
        use crate::layout::constraints::Constraints;
        composer.layout(Constraints::new(0.0, 40.0, 0.0, 40.0));
        let mut surface = skia_safe::surfaces::raster_n32_premul((40, 40)).unwrap();
        surface.canvas().clear(skia_safe::Color::WHITE);
        let root = composer.layout_root_idx().expect("root");
        crate::render::render(composer.arena_nodes(), root, surface.canvas());
        let pm = surface.peek_pixels().expect("pixmap");
        let px: &[[u8; 4]] = pm.pixels::<[u8; 4]>().expect("pixels");
        px[20 * 40 + 20][0] as i32
    }

    /// A theme change only reaches the tree when the composer is told, and it has to be told about the
    /// whole SUBTREE: `WiniaTheme::auto` resolved its colors when its group last composed, and a
    /// param-less wrapper between the theme and the content would otherwise be Skipped and keep them
    /// (the SearchBar filtering bug had that shape). This drives the same three steps
    /// `app::apply_system_theme` does, and the middle one shows the dirtying is load-bearing.
    #[test]
    fn a_theme_change_needs_the_subtree_dirty() {
        let _serial = THEME_TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        use crate::core::composer::Composer;

        let scene = |ctx: &mut crate::core::composer::ComposeCtx| {
            WiniaTheme::auto(ctx, surface_scene);
        };

        set_system_dark_mode(Some(false));
        let mut composer = Composer::new();
        composer.compose(scene);
        let light = centre_pixel(&mut composer);

        // The value changed, but nothing told the tree: the colors stay as composed.
        set_system_dark_mode(Some(true));
        composer.recompose(scene);
        assert_eq!(centre_pixel(&mut composer), light, "without the dirtying an idle subtree keeps its colors");

        // The step `apply_system_theme` performs.
        composer.mark_content_dirty();
        composer.recompose(scene);
        assert_ne!(centre_pixel(&mut composer), light, "the recomposition must pick the new theme up");

        // Hand the mode back: it is process-global.
        set_system_dark_mode(None);
    }

    /// The shape of a WINDOW: the application resolves its theme once, around the node that opens the
    /// window, and whatever the window samples there is carried into the content closure the framework
    /// re-runs every frame.
    ///
    /// A palette captured as a VALUE pins the window to its startup colors — measured on the demo, an
    /// `auto` window stayed dark while the surface behind the tree flipped to light, because the per-frame
    /// wrapper re-provided the palette sampled at creation. The SPEC keeps resolving, so the tree follows
    /// once it is told to run again.
    #[test]
    fn a_window_content_re_resolves_the_theme_it_sampled() {
        let _serial = THEME_TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        use crate::core::composer::Composer;

        set_system_dark_mode(Some(false));
        // What `Window::build` samples, while the application's own theme node is in scope.
        let sampled = std::cell::RefCell::new(None);
        let mut init = Composer::new();
        init.compose(|ctx| {
            WiniaTheme::auto(ctx, |_| {
                *sampled.borrow_mut() = Some(current_theme_spec());
            })
        });
        let spec = sampled.into_inner().expect("the window samples a theme spec");
        assert!(matches!(spec, ThemeSpec::Auto), "a window inside `auto` must keep following the system");

        // Each frame after that: the window's own composer runs the content under the sampled spec.
        let mut composer = Composer::new();
        let mut frame = |composer: &mut Composer| {
            composer.compose(|ctx| spec.provide(ctx, surface_scene));
        };
        frame(&mut composer);
        let light = centre_pixel(&mut composer);

        // The application switches its own theme (the demo's Auto / Light / Dark row).
        set_system_dark_mode(Some(true));
        composer.mark_content_dirty();
        frame(&mut composer);
        assert_ne!(centre_pixel(&mut composer), light, "the window's tree must follow the theme it follows");

        set_system_dark_mode(None);
    }

    /// `Auto` resolves on every call; `Fixed` answers with the palette it was built from, whatever the mode
    /// says — an application that pinned a theme must not be dragged by the system.
    #[test]
    fn theme_spec_resolves_auto_per_call_and_fixed_never() {
        let _serial = THEME_TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let fixed = ThemeSpec::Fixed(ThemeColors::default_light());
        // Outside any theme node a caller gets the palette in force, never a follow-the-system spec.
        assert!(matches!(current_theme_spec(), ThemeSpec::Fixed(_)));

        set_system_dark_mode(Some(true));
        assert_eq!(ThemeSpec::Auto.colors().background, ThemeColors::default_dark().background);
        assert_eq!(fixed.colors().background, ThemeColors::default_light().background);

        set_system_dark_mode(Some(false));
        assert_eq!(ThemeSpec::Auto.colors().background, ThemeColors::default_light().background);
        assert_eq!(fixed.colors().background, ThemeColors::default_light().background);
        set_system_dark_mode(None);
    }

    /// A theme provide records its spec for the duration of its content only — a window that samples it
    /// deeper in the tree gets the INNERMOST one, and the enclosing composition's spec comes back when the
    /// provide ends.
    #[test]
    fn a_theme_provide_records_its_spec_for_its_content_only() {
        let mut composer = crate::core::composer::Composer::new();
        let inside = std::cell::RefCell::new(None);
        let after = std::cell::RefCell::new(None);
        composer.compose(|ctx| {
            WiniaTheme::auto(ctx, |ctx| {
                WiniaTheme::light(ctx, |_| {
                    *inside.borrow_mut() = Some(current_theme_spec());
                });
                *after.borrow_mut() = Some(current_theme_spec());
            });
        });
        assert!(matches!(inside.into_inner(), Some(ThemeSpec::Fixed(_))));
        assert!(matches!(after.into_inner(), Some(ThemeSpec::Auto)));
    }

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
