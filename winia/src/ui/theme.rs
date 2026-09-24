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

#[derive(Debug, Clone, PartialEq, Copy)]
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
/// The mode as a REACTIVE value, and the one the composition actually reads: a tracked read of it is what
/// makes a window's composer wake at all when the mode changes (each window composes in its own composer,
/// and a composer with nothing pending is never asked to redraw). The atomic above stays for the callers
/// that ask outside a composition (`is_system_dark_theme` from the app loop).
static SYSTEM_THEME_STATE: std::sync::LazyLock<crate::core::state::Reactive<u8>> =
    std::sync::LazyLock::new(|| crate::core::state::Reactive::new(system_theme_mode()));
/// How many times the SYSTEM theme may have changed. A window records the epoch it last resolved at and
/// re-resolves when it moves — per WINDOW state, deliberately: a single process-wide "pending" flag is
/// consumed by whichever window renders first, which left every other window on its old palette.
static SYSTEM_THEME_EPOCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
/// Set when every window should be asked for a frame because the mode moved.
///
/// Windows that follow the system are woken by their own dependency on the mode, but not every window has
/// one: a window whose content an application composed itself (through `app::open_window_with_title`) has
/// nothing to wake it, and a mode change can come from a background thread that cannot reach the windows.
/// The app loop drains this — it is the one place that knows them.
static THEME_REDRAW_ALL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether every window should be asked for a frame (the app loop's check).
pub(crate) fn take_theme_redraw_all() -> bool {
    THEME_REDRAW_ALL.swap(false, std::sync::atomic::Ordering::Relaxed)
}

/// The pinned mode (`0` = follow the system, `1` = light, `2` = dark).
fn system_theme_mode() -> u8 {
    SYSTEM_THEME_MODE.load(std::sync::atomic::Ordering::Relaxed)
}

/// Bumped whenever the system theme may have changed ([`set_system_dark_mode`], [`note_platform_theme`]).
/// Callers compare it against the epoch they last resolved at: equal means "nothing to look at".
pub(crate) fn system_theme_epoch() -> u64 {
    SYSTEM_THEME_EPOCH.load(std::sync::atomic::Ordering::Relaxed)
}

/// Pin the theme, or hand it back to the system with `None`.
///
/// The change lands on the next frame: this only stores the value and bumps [`system_theme_epoch`]
/// (running a full recompose from arbitrary code would be a re-entrancy hazard, and the loop is where the
/// composers live). Each window resolves the stored mode when its next frame refreshes the theme.
pub fn set_system_dark_mode(mode: Option<bool>) {
    use std::sync::atomic::Ordering;
    let value = match mode {
        None => 0,
        Some(false) => 1,
        Some(true) => 2,
    };
    if SYSTEM_THEME_MODE.swap(value, Ordering::Relaxed) != value {
        // Wake every window that follows the system (the tracked read in the content wrapper) and tell
        // them there is something to re-resolve; ask the loop for a frame for the ones that cannot be woken
        // that way.
        SYSTEM_THEME_STATE.set(value);
        SYSTEM_THEME_EPOCH.fetch_add(1, Ordering::Relaxed);
        THEME_REDRAW_ALL.store(true, Ordering::Relaxed);
        crate::core::state::wake_loop();
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
        SYSTEM_THEME_EPOCH.fetch_add(1, Ordering::Relaxed);
    }
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
#[derive(Clone, Debug, PartialEq)]
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
}

/// A window's theme: the intent to resolve (`ThemeSpec`), the typography and direction in force where the
/// window was declared, and the palette the intent last resolved to — SHARED between the `Window` node that
/// manages the window and the window itself.
///
/// The two sides live in different composers — the node runs in the tree that DECLARES the window and
/// re-samples all of it every frame (an application may switch which theme node wraps it), while the
/// content wrapper runs in the window's own composer and must see the same, current values. A palette
/// captured as a value pinned the window to its startup colors; an intent sampled once and then frozen
/// pinned it to its startup NODE (a sub-window whose declaring tree switched from `light` to `dark` stayed
/// light forever), and the same held for typography and direction, which the wrapper re-provided as the
/// DEFAULTS every frame — so a custom type scale around a `Window` node survived exactly one frame. Hence a
/// shared cell, written by whichever side knows more.
#[derive(Clone)]
pub struct WindowTheme(std::sync::Arc<std::sync::Mutex<ThemeCell>>);

/// What a window has already drawn with. Compared with the cell's current values to decide whether the tree
/// has to run again: a component resolved colors, type and direction when it built, so a change in ANY of
/// them is only visible after a recomposition.
#[derive(Clone, PartialEq)]
pub(crate) struct AppliedTheme {
    pub colors: ThemeColors,
    pub typography: Typography,
    pub direction: LayoutDirection,
}

struct ThemeCell {
    spec: ThemeSpec,
    typography: Typography,
    direction: LayoutDirection,
    /// The palette `spec` resolved to at `resolved_epoch`.
    colors: ThemeColors,
    /// The system-theme epoch `colors` was resolved at.
    resolved_epoch: u64,
    /// The declaring tree published something different and nobody has resolved it yet.
    spec_dirty: bool,
}

impl WindowTheme {
    /// A theme for `spec` with the default type scale and direction — a window's first frame has no earlier
    /// resolution to reuse, and `Window::build` publishes what it sampled before that frame runs.
    pub fn new(spec: ThemeSpec) -> Self {
        let colors = spec.colors();
        Self(std::sync::Arc::new(std::sync::Mutex::new(ThemeCell {
            spec,
            typography: Typography::default(),
            direction: LayoutDirection::Ltr,
            colors,
            resolved_epoch: system_theme_epoch(),
            spec_dirty: false,
        })))
    }

    /// The cell, never panicking on a poisoned lock: a panic inside a content closure would otherwise
    /// disable the window for the rest of the process (the app loop gives up rendering after 30 consecutive
    /// panics), and the cell holds no invariant a panic could leave half-written — every field is written
    /// under this same lock.
    fn cell(&self) -> std::sync::MutexGuard<'_, ThemeCell> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The intent in force.
    pub fn spec(&self) -> ThemeSpec {
        self.cell().spec.clone()
    }

    /// What this window would draw with right now. The honest starting point for a window whose first frame
    /// has NOT run yet (and whose theme was never compared): defaulting the typography and direction here
    /// would (a) re-run that first frame for nothing and (b) miss a later publish that changed a custom type
    /// scale back to the default, leaving the window on the old one until something else re-ran it.
    pub(crate) fn applied(&self) -> AppliedTheme {
        let cell = self.cell();
        AppliedTheme { colors: cell.colors, typography: cell.typography.clone(), direction: cell.direction }
    }

    /// The palette to compose with — the last resolved one, deliberately not a fresh resolve: a frame
    /// that composes and still skips every group should not rebuild a Material color scheme.
    pub(crate) fn colors(&self) -> ThemeColors {
        self.cell().colors
    }

    /// Publish what the declaring tree sampled this frame. Returns whether anything moved — the window's
    /// own frame has to be scheduled in that case, because the change happened in ANOTHER composer (which
    /// leaves the window's composer with nothing pending).
    pub(crate) fn publish(&self, spec: ThemeSpec, typography: Typography, direction: LayoutDirection) -> bool {
        let mut cell = self.cell();
        if cell.spec == spec && cell.typography == typography && cell.direction == direction {
            return false;
        }
        cell.spec = spec;
        cell.typography = typography;
        cell.direction = direction;
        cell.spec_dirty = true;
        true
    }

    /// Bring the window's theme up to date — the system epoch moved, or the declaring tree published
    /// something else — and compare it with what the window has already drawn. Returns whether what was
    /// drawn is now WRONG; a `Fixed` window under a system change resolves the same palette and keeps the
    /// same type, so it answers `false` and costs nothing.
    pub(crate) fn refresh(&self, drawn: &mut AppliedTheme) -> bool {
        let epoch = system_theme_epoch();
        let mut cell = self.cell();
        if !cell.spec_dirty && cell.resolved_epoch == epoch {
            return false;
        }
        cell.spec_dirty = false;
        cell.resolved_epoch = epoch;
        let colors = cell.spec.colors();
        cell.colors = colors;
        if colors == drawn.colors && cell.typography == drawn.typography && cell.direction == drawn.direction {
            return false;
        }
        drawn.colors = colors;
        drawn.typography = cell.typography.clone();
        drawn.direction = cell.direction;
        true
    }

    /// Provide this window's theme for `content` — the wrapper its content composes under every frame.
    pub(crate) fn provide(&self, ctx: &mut ComposeCtx, content: impl FnOnce(&mut ComposeCtx)) {
        let (spec, colors, typography, direction) = {
            let cell = self.cell();
            (cell.spec.clone(), cell.colors, cell.typography.clone(), cell.direction)
        };
        WiniaTheme::provide_resolved(spec, colors, typography, direction, ctx, content);
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
        Self::provide_resolved(spec, colors, typography, direction, ctx, content);
    }

    /// Provide a palette that has ALREADY been resolved, keeping the intent it came from — the per-frame
    /// wrapper a window's content composes under.
    ///
    /// Why not resolve here: a window recomposes for every interaction, and a Material scheme is not free
    /// to build, so an idle frame must not rebuild one just to provide the same colors again. The intent
    /// still travels (a `Window` node further down samples it), so a sub-window of a window that follows
    /// the system follows the system too.
    pub(crate) fn provide_resolved(
        spec: ThemeSpec,
        colors: ThemeColors,
        typography: Typography,
        direction: LayoutDirection,
        ctx: &mut ComposeCtx,
        content: impl FnOnce(&mut ComposeCtx),
    ) {
        // A TRACKED read while the theme follows the system: it is what makes THIS composer a dependent of
        // the mode, and therefore what wakes the window at all when the mode changes — a composer with
        // nothing pending is never asked to redraw, and a frame never runs for it.
        if matches!(spec, ThemeSpec::Auto) {
            let _ = SYSTEM_THEME_STATE.get();
        }
        let on_surface = colors.on_surface;
        // A `CompositionLocal` does not invalidate its readers, so the provider has to: when the resolved
        // theme differs from the previous frame's, everything inside composed with the old value —
        // padding, alignment, colors — and has to run again. Without this, a theme or direction switch
        // only reached components that happened to read a state directly (measured: a param-less group
        // between the provider and the reader stayed on the previous palette after the system mode moved).
        //
        // The previous values live in a backchannel: it has the same slot stability as `remember` but
        // never notifies, and this frame is already composing — waking the composer for a value it is
        // reading right now would only schedule an empty pass.
        // The tuple is cheap to clone and every field is `PartialEq`; the backchannel keeps a COPY, so
        // the values below still move into the locals.
        // Keyed by the POSITION rather than by a remembered call site: this function also runs from a
        // window's per-frame wrapper, where there is no `#[composable]` call site to key off, and two
        // providers inside one composer must not share the memory of what they last provided.
        const NAMESPACE: u64 = 0x7769_6E69_6174_6865; // "winia the(me)"
        let prev = ctx.remember_backchannel_at_key(ctx.position_key(NAMESPACE), || {
            crate::core::state::Backchannel::new((colors, typography.clone(), direction))
        });
        let resolved = (colors, typography.clone(), direction);
        if prev.get() != resolved {
            prev.set(resolved);
            ctx.mark_subtree_dirty();
        }
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

/// The theme mode is process-wide, so the tests that move it must not overlap — inside this module and
/// outside it (the app's window-level test drives the same mode).
#[cfg(test)]
pub(crate) static THEME_TEST_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
pub(crate) fn theme_mode_test_lock() -> &'static std::sync::Mutex<()> {
    &THEME_TEST_SERIAL
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// A theme change carries itself into an idle subtree.
    ///
    /// The provider compares what it is about to provide with the previous frame's and dirties its own
    /// subtree when they differ, so a param-less wrapper between the theme and the content re-runs and
    /// picks the new palette up. Before that, the same shape kept the colors it composed with until
    /// something outside told the composer (`mark_content_dirty`, which `PerWindow::refresh_theme`
    /// performs) — the SearchBar filtering bug had exactly that shape, and this test used to assert the
    /// stale behavior as the contract.
    #[test]
    fn a_theme_change_reaches_an_idle_subtree_by_itself() {
        let _serial = THEME_TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        use crate::core::composer::Composer;

        let scene = |ctx: &mut crate::core::composer::ComposeCtx| {
            WiniaTheme::auto(ctx, surface_scene);
        };

        set_system_dark_mode(Some(false));
        let mut composer = Composer::new();
        composer.compose(scene);
        let light = centre_pixel(&mut composer);

        // The system mode moves. Nothing external tells the tree about it, and the subtree is idle: the
        // provider itself has to carry the new palette in.
        set_system_dark_mode(Some(true));
        composer.recompose(scene);
        assert_ne!(
            centre_pixel(&mut composer),
            light,
            "the provider must reach an idle subtree on its own"
        );

        // The window-level step is still what SCHEDULES the frame (`needs_recomposition`); it must stay
        // harmless now that the provider propagates too.
        composer.mark_content_dirty();
        composer.recompose(scene);
        assert_ne!(centre_pixel(&mut composer), light, "and the external dirtying still works");

        // Hand the mode back: it is process-global.
        set_system_dark_mode(None);
    }

    /// A reader that changed nothing about ITSELF still has to pick up a new local value.
    ///
    /// This is the gap `CompositionLocal::provides` documents: the local is a stack, and a reader only
    /// re-reads when its own group is re-entered. A param-less wrapper declares nothing, so once the
    /// provider resolves a NEW direction/theme it would otherwise keep the old one — the components
    /// inside compiled their padding, alignment and colors from it. The provider closes that by
    /// dirtying its own subtree when the resolved values differ from the previous frame's.
    #[test]
    fn a_local_change_reaches_a_reader_that_declared_nothing() {
        let _serial = THEME_TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        use crate::core::composer::{Composer, ComposeCtx, GroupStatus};
        use crate::layout::BoxLayout;
        use crate::layout::LayoutDirection;
        use crate::modifier::Modifier;
        use crate::ui::text::Text;

        // Records what the reader actually built with, on every run — and every SKIP, so a test can
        // tell "the reader re-read the local" from "the reader never skipped in the first place".
        fn reader(
            ctx: &mut ComposeCtx,
            seen: &std::rc::Rc<std::cell::RefCell<Vec<LayoutDirection>>>,
            skips: &std::rc::Rc<std::cell::Cell<usize>>,
        ) {
            let key = ctx.next_key();
            // A param-less wrapper: it declares nothing, so nothing about IT changes between frames.
            match ctx.start_restartable_group(key, Modifier::new(), BoxLayout::new()) {
                GroupStatus::Skip => { skips.set(skips.get() + 1); }
                GroupStatus::Enter => {
                    seen.borrow_mut().push(WiniaTheme::direction());
                    let leaf = ctx.next_key();
                    let dir = if WiniaTheme::direction() == LayoutDirection::Rtl { "rtl" } else { "ltr" };
                    Text::new(dir).build(ctx);
                    let _ = leaf;
                }
            }
            ctx.end_restartable_group();
        }

        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let seen_in = seen.clone();
        let skips = std::rc::Rc::new(std::cell::Cell::new(0usize));
        let skips_in = skips.clone();
        let direction = crate::State::new(LayoutDirection::Ltr);
        let direction_in = direction.clone();

        let scene: Box<dyn Fn(&mut ComposeCtx)> = Box::new(move |ctx: &mut ComposeCtx| {
            let dir = direction_in.get();
            WiniaTheme::with_theme_typography_and_direction(
                ThemeColors::default_light(),
                Typography::default(),
                dir,
                ctx,
                |ctx| reader(ctx, &seen_in, &skips_in),
            );
        });

        // A frame is compose → layout: the LAYOUT pass is what fills `prev_nodes`, and a group can only
        // Skip when it has a cached subtree to restore (composer.rs:2054). A test that composes twice
        // without laying out never sees a Skip at all, so its "the reader kept the old value" would be
        // vacuous — that mistake is what the CONTROL below exists to catch.
        let mut frame = |composer: &mut Composer, scene: &dyn Fn(&mut ComposeCtx)| {
            composer.compose(|ctx| scene(ctx));
            composer.layout(crate::layout::constraints::Constraints::new(0.0, 200.0, 0.0, 200.0));
        };
        let mut composer = Composer::new();
        frame(&mut composer, &scene);
        assert_eq!(seen.borrow().as_slice(), [LayoutDirection::Ltr], "first frame reads the provided value");

        // CONTROL: does this group skip AT ALL when nothing changed? If it never skips, a test built on
        // it cannot see the local-invalidation bug and proves nothing.
        frame(&mut composer, &scene);
        assert_eq!(seen.borrow().len(), 1, "CONTROL: an unchanged frame must Skip the reader");
        assert!(skips.get() >= 1, "CONTROL: the group skipped");

        // Swap the DIRECTION only. The reader itself declares nothing, so its group is Skippable.
        direction.set(LayoutDirection::Rtl);
        frame(&mut composer, &scene);
        assert_eq!(
            seen.borrow().as_slice(),
            [LayoutDirection::Ltr, LayoutDirection::Rtl],
            "a reader inside the provider must re-read after the provided value changed"
        );
    }


    /// The shape of a WINDOW, end to end at the composer level: the declaring tree samples the intent and
    /// PUBLISHES it into the window's cell, the window's own composer composes under the cell's palette,
    /// and a change reaches the tree by two independent routes.
    ///
    /// Both routes are load-bearing. A palette captured as a value pinned the window to its startup colors
    /// (measured on the demo: an `auto` window stayed dark while the surface behind it flipped to light),
    /// and an intent sampled once pinned it to its startup NODE (a sub-window whose declaring tree switched
    /// from light to dark kept its palette forever).
    #[test]
    fn a_window_content_follows_its_cell() {
        let _serial = THEME_TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        use crate::core::composer::Composer;

        set_system_dark_mode(Some(false));
        // What `Window::build` samples, while the application's own theme node is in scope.
        let sampled = std::cell::RefCell::new(None);
        let mut init = Composer::new();
        init.compose(|ctx| {
            WiniaTheme::auto(ctx, |_| {
                *sampled.borrow_mut() = Some(WindowTheme::new(current_theme_spec()));
            })
        });
        let cell = sampled.into_inner().expect("the window samples a theme");
        assert_eq!(cell.spec(), ThemeSpec::Auto, "a window inside `auto` must keep following the system");
        let mut drawn = cell.applied();

        // Each frame after that: the window's own composer runs the content under the cell's palette.
        let mut composer = Composer::new();
        let mut frame = |composer: &mut Composer| {
            composer.compose(|ctx| cell.provide(ctx, surface_scene));
        };
        frame(&mut composer);
        let light = centre_pixel(&mut composer);

        // (1) The system mode moves, and the loop refreshes this window's palette.
        set_system_dark_mode(Some(true));
        assert!(cell.refresh(&mut drawn), "the system theme moved");
        composer.mark_content_dirty();
        frame(&mut composer);
        assert_ne!(centre_pixel(&mut composer), light, "the window's tree must follow the system");
        let dark = centre_pixel(&mut composer);

        // (2) The DECLARING tree publishes another intent (it switched its own theme node). The window
        // composes in a different composer, so without the cell it would never hear about it.
        let plain = ThemeSpec::Fixed(ThemeColors::default_light());
        assert!(cell.publish(plain, Typography::default(), LayoutDirection::Ltr), "the intent moved");
        assert!(cell.refresh(&mut drawn), "the published palette is not what is drawn");
        composer.mark_content_dirty();
        frame(&mut composer);
        let after = centre_pixel(&mut composer);
        assert_ne!(after, dark, "a published intent has to reach the window too");
        assert_eq!(after, light, "it is the light palette the tree published");

        set_system_dark_mode(None);
    }

    /// A window carries the TYPOGRAPHY and DIRECTION in force where it was declared, not just the palette.
    ///
    /// The per-frame wrapper used to provide `Typography::default()` and LTR unconditionally, so an
    /// application that set a type scale (or an RTL direction) around its `Window` node kept it for exactly
    /// one frame — and nothing would have reported it: the window looks fine, just default.
    #[test]
    fn a_window_content_follows_published_typography_and_direction() {
        let _serial = THEME_TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        use crate::core::composer::Composer;

        let custom = Typography { body_large: TextStyle::new().font_size(23.0), ..Typography::default() };
        // What `Window::build` samples, from inside the theme nodes that wrap it.
        let sampled = std::cell::RefCell::new(None);
        let mut init = Composer::new();
        init.compose(|ctx| {
            WiniaTheme::with_theme_and_direction(ThemeColors::default_light(), LayoutDirection::Rtl, ctx, |ctx| {
                WiniaTheme::with_typography(custom.clone(), ctx, |ctx| {
                    *sampled.borrow_mut() = Some(WindowTheme::new(current_theme_spec()));
                    let cell = sampled.borrow();
                    cell.as_ref()
                        .unwrap()
                        .publish(current_theme_spec(), WiniaTheme::typography(), WiniaTheme::direction());
                })
            })
        });
        let cell = sampled.into_inner().expect("the window samples a theme");
        let mut drawn = cell.applied();

        // Each frame: the window's own composer composes under the cell's theme. What a component would read
        // while building has to be the published type scale and direction.
        let seen = std::cell::RefCell::new(Vec::new());
        let mut composer = Composer::new();
        let mut frame = |composer: &mut Composer| {
            composer.compose(|ctx| {
                cell.provide(ctx, |_| {
                    seen.borrow_mut().push((WiniaTheme::typography(), WiniaTheme::direction()));
                })
            });
        };
        frame(&mut composer);
        let (typography, direction) = seen.borrow()[0].clone();
        assert_eq!(typography, custom, "the declared type scale has to reach the window's content");
        assert_eq!(direction, LayoutDirection::Rtl, "and so does the direction");

        // Published changes are what the frame step exists for: they have to read as "what was drawn is now
        // wrong", even though the palette did not move.
        let other = Typography { body_large: TextStyle::new().font_size(31.0), ..Typography::default() };
        assert!(cell.publish(current_theme_spec(), other.clone(), LayoutDirection::Ltr), "the type scale moved");
        assert!(cell.refresh(&mut drawn), "a type-scale change has to re-run the tree");
        frame(&mut composer);
        let (typography, direction) = seen.borrow()[1].clone();
        assert_eq!(typography, other);
        assert_eq!(direction, LayoutDirection::Ltr);
        assert!(!cell.refresh(&mut drawn), "and then there is nothing left to do");

        // A DIRECTION-only change is the other half of that comparison: same palette, same type scale, and
        // the tree still has to run again (a Row mirrors, text aligns the other way). Without it, dropping
        // the direction from the comparison would pass every other test in this file.
        assert!(cell.publish(current_theme_spec(), other.clone(), LayoutDirection::Rtl), "the direction moved");
        assert!(cell.refresh(&mut drawn), "a direction change has to re-run the tree");
        frame(&mut composer);
        assert_eq!(seen.borrow()[2].1, LayoutDirection::Rtl, "and the content sees it");
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
                // `dark`, not `light`: outside every theme node the answer is already `Fixed(default_light)`,
                // so a lighter inner theme could not tell "the inner provide was recorded" from "no
                // thread-local was ever written at all".
                WiniaTheme::dark(ctx, |_| {
                    *inside.borrow_mut() = Some(current_theme_spec());
                });
                *after.borrow_mut() = Some(current_theme_spec());
            });
        });
        let inside = inside.into_inner().expect("the inner provide recorded its spec");
        assert_eq!(inside, ThemeSpec::Fixed(ThemeColors::default_dark()), "the INNERMOST theme wins");
        assert_eq!(after.into_inner(), Some(ThemeSpec::Auto), "and the enclosing one comes back");
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
